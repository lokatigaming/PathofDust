# Watchdog for the GameProcess scheduled task - the game.exe counterpart
# to watchdog.ps1 (see that file's own doc for why this pattern exists
# at all: Windows Task Scheduler's native RestartOnFailure proved
# unreliable across real STATUS_STACK_OVERFLOW crashes). Checks every run
# whether the game process is actually alive, and if not, restarts the
# task and logs the recovery.
#
# Prepared ahead of the LIVE BAKE cutover (REFACTOR_PLAN.md, Stage 5) -
# registered as its own scheduled task ("GameProcess-Watchdog") on the
# same short repeating trigger as TwitchBotRS-Watchdog, BEFORE the
# GameProcess task itself starts (per the owner's explicit ordering: no
# window where game.exe runs unprotected).
#
# ---------------------------------------------------------------------
# DETECTION REWRITE (2026-08-23, fix/ops-backup-and-watchdog)
# ---------------------------------------------------------------------
# This script used to detect liveness with `Get-Process -Name "game"`.
# That is wrong the moment a second deployment exists: two game.exe
# processes share one image name, so the check returns non-empty whenever
# EITHER is alive and the watchdog silently stops detecting the death of
# EITHER one. Standing up a second world would have un-protected the live
# one with no visible signal.
#
# Liveness is now "is anything LISTENING on MY port", which is
# per-deployment by construction, needs no image-name matching, and uses
# the same port-to-PID resolution CLAUDE.md's PRODUCTION SAFETY rule
# already requires before stopping anything.
#
# THIS IS A DETECTION CHANGE ONLY. The restart action, the log line's
# exact text and timestamp format, and the fact that a healthy run logs
# nothing at all are all preserved. (There was no backoff in the previous
# version to preserve - the only pacing is the task's own PT2M repetition
# interval, which lives in the task definition, not here.)
#
# NEVER TERMINATES ANYTHING. This script starts a scheduled task and
# writes a log line. It contains no Stop-Process, no taskkill, no
# Stop-ScheduledTask - by image name or by any other means. The image
# name it reads is written into the log for a human reading an incident
# later and never drives a decision.
#
# WHAT THE PATH CHECK CAN AND CANNOT DO HERE. Confirming the listener's
# image path is under this deployment means reading another process's
# executable path, and that is NOT available at the run level these tasks
# currently use. Measured 2026-08-23: GameProcess and GameProcess-Watchdog
# both run as `Administrator` with `RunLevel = Limited`, and from a
# non-elevated context `(Get-Process -Id N).Path`, `.MainModule.FileName`
# and `Win32_Process.ExecutablePath` all come back EMPTY - for game.exe
# and twitch-bot-rs.exe alike. So:
#
#   * the RESTART decision hinges only on "is anything listening on my
#     port" - which needs no elevation, and is the part that matters,
#     since a restart cannot help while the port is occupied anyway;
#   * the path check runs when it can and is three-state: confirmed /
#     foreign / unverifiable;
#   * -RequireOwnPath promotes "unverifiable" to a logged warning, for an
#     operator who has raised the task to RunLevel = Highest. It is OFF
#     by default, so production behaviour at today's Limited run level is
#     unchanged.
#
# Registering or altering scheduled tasks is out of this script's scope -
# see docs/ops_backup_and_watchdog.md for the recommended task changes.
#
# ---------------------------------------------------------------------
# MAINTENANCE GATE (2026-08-23, fix/watchdog-maintenance-gate)
# ---------------------------------------------------------------------
# REFACTOR_PLAN.md section 13 step 4 tells a deploy session to disable
# this watchdog before swapping the binary. It cannot: Disable-
# ScheduledTask needs an elevated token and no deploy session has one, so
# the step silently did not happen and every binary swap so far has run
# with the watchdog live. It has not fired mid-swap only because swaps
# have been fast relative to the task's ~2 minute repetition interval.
# That is luck, not protection.
#
# So the suppression a deploy actually needs is a FILE, which a non-
# elevated session can create and delete: while a valid flag exists at
# $MaintenanceFlagPath, this script logs that it is suppressed and exits
# without restarting anything.
#
# THE FLAG EXPIRES, and that is the point. A flag someone forgot to
# remove would otherwise disable protection forever - a strictly worse
# failure than the one this fixes, and a silent one. Past
# $MaintenanceMaxAgeMinutes (default 30) the flag is IGNORED, the fact
# that it was ignored is logged loudly, and normal protection resumes.
# Anything unreadable, undated, or dated in the future is treated the
# same way: a flag that cannot be positively verified as current does not
# suppress anything. Every ambiguous case fails toward protecting the
# world, never toward staying quiet.
#
# The gate is checked only on runs that would otherwise ACT. A healthy
# run still logs nothing at all, flag or no flag, so a deploy window does
# not fill the log with suppression lines.
#
# ---------------------------------------------------------------------
# $ExpectedPathRoot RESOLVES IN THE BODY (2026-08-23, same branch)
# ---------------------------------------------------------------------
# It used to default to `$PSScriptRoot` in the PARAM BLOCK, where that
# variable is not yet populated under the `-File` invocation the
# scheduled task actually uses - so the root arrived EMPTY, Test-UnderRoot
# returned $null for every candidate, and every listener resolved
# `unverifiable` no matter where it lived. Harmless at today's
# RunLevel = Limited (paths are unreadable anyway, so that is the same
# verdict either way) but it would have silently defeated -RequireOwnPath
# the moment the task was raised to RunLevel = Highest to obtain exactly
# that check - false confidence in place of protection. $LogPath was
# always resolved in the body and always worked; both now do.

[CmdletBinding()]
param(
    # The port that identifies THIS deployment. 4005 is adventure_web
    # (game/src/main.rs, ADVENTURE_WEB_PORT default) - the port the
    # cloudflared tunnel fronts, i.e. the port whose absence actually
    # means this world is down. (It was also the port the bot's
    # ADVENTURE_API_BASE_URL pointed at; that seam was deleted from both
    # binaries on 2026-09-02 and the bot no longer speaks to the game at
    # all.) A second deployment passes its own (e.g. -Port 4015).
    [int] $Port = 4005,

    [string] $TaskName = 'GameProcess',

    # The deployment root the listening process is expected to live
    # under. Only consulted when the image path is readable at all.
    # Resolved in the BODY, not here - see the header. A param-block
    # default of $PSScriptRoot arrives empty under -File.
    [string] $ExpectedPathRoot,

    [string] $LogPath,

    # The maintenance flag a deploy session drops to suppress this
    # watchdog without elevation (see the header, and
    # maintenance-flag.ps1 which writes it). Resolved in the BODY for the
    # same reason as the two above.
    [string] $MaintenanceFlagPath,

    # How long a maintenance flag stays valid. Past this it is ignored
    # and the fact is logged loudly - a forgotten flag must not disable
    # protection indefinitely. 0 disables the gate entirely (every flag
    # is ignored), which is the safe direction to fail.
    [int] $MaintenanceMaxAgeMinutes = 30,

    # A single in-run re-check before believing the world is down. The
    # old name-based check saw a process the instant it was created; a
    # port-based check does not see it until it has bound, and
    # AdventureManager::new loads a 3.3 MB roster and runs any pending
    # migrations before the servers start. Without this, the new
    # detection would be strictly more trigger-happy than the old one -
    # a regression in restart safety rather than an improvement. Set 0
    # to disable.
    [int] $RecheckDelaySeconds = 5,

    # The same startup window from the other side: do not restart a task
    # that only just started. Set 0 to disable.
    [int] $StartupGraceSeconds = 90,

    # Report the resolved state and what would happen. Starts nothing,
    # writes nothing to the log.
    [switch] $DryRun,

    # Treat "listening, but I could not read its image path" as a logged
    # warning instead of silent health. Only useful once the task runs
    # with RunLevel = Highest.
    [switch] $RequireOwnPath
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $LogPath = Join-Path $PSScriptRoot 'game-watchdog.log'
}

# Both of these default off $PSScriptRoot for the same reason $LogPath
# does, and in the same place - the body, where $PSScriptRoot is actually
# populated under -File. See the header.
if ([string]::IsNullOrWhiteSpace($ExpectedPathRoot)) {
    $ExpectedPathRoot = $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($MaintenanceFlagPath)) {
    # NAMED FOR ITS WATCHDOG, not for "the watchdog". The bot watchdog has
    # its own gate and its own separate flag beside this one; a shared file
    # would mean a game-only deploy (the common case - section 13 deploys
    # the bot only when the diff says so) silently suppressed the BOT's
    # watchdog too, through a window where section 13 explicitly leaves the
    # bot running untouched. Two files, two independent lifetimes.
    $MaintenanceFlagPath = Join-Path $PSScriptRoot 'game-watchdog-maintenance.flag'
}

function Get-ListenerPids {
    # Every distinct PID holding a LISTEN socket on $ThePort.
    # Get-NetTCPConnection returns one row per address family, so a
    # process listening on both IPv4 and IPv6 collapses to one PID here.
    #
    # The netstat fallback is real, not decoration: the NetTCPIP module
    # is absent from Server Core and can be missing from a trimmed
    # image, and a watchdog that throws is a watchdog that has stopped
    # watching.
    param([int] $ThePort)

    if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
        try {
            $rows = @(Get-NetTCPConnection -State Listen -LocalPort $ThePort -ErrorAction SilentlyContinue)
            return @($rows | ForEach-Object { $_.OwningProcess } | Sort-Object -Unique)
        } catch { }
    }

    $found = New-Object Collections.ArrayList
    foreach ($line in (netstat -ano -p TCP)) {
        $t = $line.Trim()
        if (-not $t.StartsWith('TCP')) { continue }
        $cols = @($t -split '\s+')
        if ($cols.Count -lt 5) { continue }
        if ($cols[3] -ne 'LISTENING') { continue }
        $local = $cols[1]
        $idx = $local.LastIndexOf(':')
        if ($idx -lt 0) { continue }
        if ($local.Substring($idx + 1) -ne "$ThePort") { continue }
        $null = $found.Add([int] $cols[4])
    }
    return @($found | Sort-Object -Unique)
}

function Get-ProcessFacts {
    # Best-effort diagnostics. `Name` is usually readable without
    # elevation; `Path` usually is not (see the header's measurement).
    # Neither drives the restart decision, and the name is never used to
    # match or terminate anything - it exists so a human reading an
    # incident log can see what held the port.
    param([int] $ProcessId)

    $facts = @{ Name = ''; Path = '' }
    try {
        $p = Get-Process -Id $ProcessId -ErrorAction Stop
        $facts.Name = $p.Name
        try { if ($p.Path) { $facts.Path = $p.Path } } catch { }
    } catch { }
    if ([string]::IsNullOrWhiteSpace($facts.Path)) {
        try {
            $c = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction Stop
            if ($null -ne $c) {
                if ([string]::IsNullOrWhiteSpace($facts.Name) -and $c.Name) { $facts.Name = $c.Name }
                if ($c.ExecutablePath) { $facts.Path = $c.ExecutablePath }
            }
        } catch { }
    }
    return $facts
}

function Get-MaintenanceGate {
    # Three-state, returned as a hashtable:
    #   State  = 'none'    - no flag file; behave exactly as before.
    #          = 'active'  - a valid, current flag; suppress the restart.
    #          = 'expired' - a flag that exists but cannot be positively
    #                        verified as current. Ignored, loudly.
    #   Detail = human-readable, goes straight into the log line.
    #
    # EVERY ambiguous case resolves to 'expired', never 'active':
    # unreadable file, unparseable contents, missing/garbage timestamp, a
    # timestamp in the future, or a non-positive max age. Suppression has
    # to be positively proven current; the absence of proof protects the
    # world rather than silencing the watchdog.
    param([string] $Path, [int] $MaxAgeMinutes)

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) {
        return @{ State = 'none'; Detail = '' }
    }

    if ($MaxAgeMinutes -le 0) {
        return @{ State = 'expired'; Detail = "flag present but the maintenance gate is disabled (-MaintenanceMaxAgeMinutes $MaxAgeMinutes)" }
    }

    $raw = $null
    try { $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop } catch { }
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return @{ State = 'expired'; Detail = 'flag file is empty or unreadable' }
    }

    $parsed = $null
    try { $parsed = $raw | ConvertFrom-Json -ErrorAction Stop } catch { }
    if ($null -eq $parsed) {
        return @{ State = 'expired'; Detail = 'flag file is not valid JSON' }
    }

    $reason = 'no reason recorded'
    if ($parsed.PSObject.Properties.Name -contains 'reason' -and -not [string]::IsNullOrWhiteSpace($parsed.reason)) {
        $reason = $parsed.reason
    }
    $by = ''
    if ($parsed.PSObject.Properties.Name -contains 'by' -and -not [string]::IsNullOrWhiteSpace($parsed.by)) {
        $by = " by=$($parsed.by)"
    }

    if ($parsed.PSObject.Properties.Name -notcontains 'created') {
        return @{ State = 'expired'; Detail = "flag file carries no 'created' timestamp (reason='$reason')" }
    }

    # Written by maintenance-flag.ps1 as a round-trip ISO 8601 string WITH
    # its real UTC offset - deliberately NOT the bare-"Z"-on-local-time
    # shape this script's own log lines use. That shape is preserved below
    # for compatibility with anything already grepping the log; it is not
    # a format to build new comparisons on.
    $created = $null
    try { $created = [datetimeoffset]::Parse([string]$parsed.created, [Globalization.CultureInfo]::InvariantCulture) } catch { }
    if ($null -eq $created) {
        return @{ State = 'expired'; Detail = "flag file's 'created' value is not a parseable timestamp (reason='$reason')" }
    }

    $ageMinutes = ([datetimeoffset]::Now - $created).TotalMinutes

    # Clock skew or a hand-edited future date. Not current, so not honored.
    if ($ageMinutes -lt -2) {
        return @{ State = 'expired'; Detail = ("flag is dated {0:N1} minutes in the FUTURE (reason='$reason'$by)" -f [Math]::Abs($ageMinutes)) }
    }
    if ($ageMinutes -gt $MaxAgeMinutes) {
        return @{ State = 'expired'; Detail = ("flag is {0:N1} minutes old, past the {1}-minute limit (reason='$reason'$by)" -f $ageMinutes, $MaxAgeMinutes) }
    }

    return @{ State = 'active'; Detail = ("reason='$reason'$by age={0:N1}m limit={1}m" -f [Math]::Max($ageMinutes, 0), $MaxAgeMinutes) }
}

function Test-UnderRoot {
    # $true / $false / $null, where $null means "could not tell".
    param([string] $CandidatePath, [string] $Root)
    if ([string]::IsNullOrWhiteSpace($CandidatePath)) { return $null }
    if ([string]::IsNullOrWhiteSpace($Root)) { return $null }
    try {
        $full = [IO.Path]::GetFullPath($CandidatePath)
        $r = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    } catch { return $null }
    return $full.StartsWith($r + '\', 'OrdinalIgnoreCase')
}

# ---------------------------------------------------------------------
# Resolve state
# ---------------------------------------------------------------------

$listenerPids = Get-ListenerPids -ThePort $Port

if ($listenerPids.Count -eq 0 -and $RecheckDelaySeconds -gt 0 -and -not $DryRun) {
    Start-Sleep -Seconds $RecheckDelaySeconds
    $listenerPids = Get-ListenerPids -ThePort $Port
}

$state = 'down'
$detail = ''
if ($listenerPids.Count -gt 0) {
    $state = 'listening-unverifiable'
    $descriptions = New-Object Collections.ArrayList
    $anyConfirmed = $false
    $anyForeign = $false
    foreach ($listenerPid in $listenerPids) {
        $f = Get-ProcessFacts -ProcessId $listenerPid
        $under = Test-UnderRoot -CandidatePath $f.Path -Root $ExpectedPathRoot
        $verdict = 'unverifiable'
        if ($under -eq $true) { $verdict = 'confirmed'; $anyConfirmed = $true }
        elseif ($under -eq $false) { $verdict = 'foreign'; $anyForeign = $true }
        $shownPath = $f.Path
        if ([string]::IsNullOrWhiteSpace($shownPath)) { $shownPath = '<unreadable at this run level>' }
        $shownName = $f.Name
        if ([string]::IsNullOrWhiteSpace($shownName)) { $shownName = '?' }
        $null = $descriptions.Add("pid=$listenerPid name=$shownName path=$shownPath verdict=$verdict")
    }
    $detail = ($descriptions -join '; ')
    if ($anyConfirmed) { $state = 'healthy' }
    elseif ($anyForeign) { $state = 'foreign' }
}

# ---------------------------------------------------------------------
# Dry run
# ---------------------------------------------------------------------

if ($DryRun) {
    $shownPids = '(none)'
    if ($listenerPids.Count -gt 0) { $shownPids = ($listenerPids -join ', ') }
    $shownDetail = '(none)'
    if ($detail) { $shownDetail = $detail }

    Write-Host "port             : $Port"
    Write-Host "task             : $TaskName"
    Write-Host "expected root    : $ExpectedPathRoot"
    Write-Host "maintenance flag : $MaintenanceFlagPath"
    $gateDry = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
    $gateShown = $gateDry.State
    if ($gateDry.Detail) { $gateShown = "$($gateDry.State) - $($gateDry.Detail)" }
    Write-Host "maintenance gate : $gateShown"
    Write-Host "log              : $LogPath"
    Write-Host "listening PIDs   : $shownPids"
    Write-Host "listener detail  : $shownDetail"
    Write-Host "resolved state   : $state"
    try {
        $ti = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction Stop
        Write-Host "task LastRunTime : $($ti.LastRunTime)"
        Write-Host "task LastResult  : $($ti.LastTaskResult)"
    } catch {
        Write-Host "task lookup      : FAILED - $($_.Exception.Message)"
    }

    $action = 'nothing (healthy)'
    if ($state -eq 'down') {
        $action = "WOULD LOG + Start-ScheduledTask -TaskName $TaskName"
    } elseif ($state -eq 'foreign') {
        $action = 'WOULD LOG a foreign-listener warning and NOT restart'
    } elseif ($state -eq 'listening-unverifiable') {
        if ($RequireOwnPath) { $action = 'WOULD LOG an unverifiable-listener warning and NOT restart' }
        else { $action = 'nothing (listening; path unverifiable at this run level)' }
    }
    # The gate only intercepts runs that would otherwise act - a healthy
    # run is unaffected by it, so the dry run must say so the same way.
    if ($action -ne 'nothing (healthy)' -and $action -notlike 'nothing (listening*') {
        if ($gateDry.State -eq 'active') {
            $action = "SUPPRESSED by the maintenance flag - WOULD LOG the suppression and do nothing else"
        } elseif ($gateDry.State -eq 'expired') {
            $action = "$action, AND would first log loudly that it ignored a stale maintenance flag"
        }
    }
    Write-Host "would do         : $action"
    Write-Host 'would terminate  : nothing, ever - this script has no process-termination code'
    return
}

# ---------------------------------------------------------------------
# Act
# ---------------------------------------------------------------------

if ($state -eq 'healthy') { return }
if ($state -eq 'listening-unverifiable' -and -not $RequireOwnPath) { return }

# ---------------------------------------------------------------------
# Maintenance gate - checked ONLY here, past the two silent-return paths
# above, so a healthy run still logs nothing whether or not a flag
# exists. See the header for why this is a file rather than
# Disable-ScheduledTask.
# ---------------------------------------------------------------------
$gate = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
$gateStamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"

if ($gate.State -eq 'active') {
    $line = "$gateStamp - SUPPRESSED by maintenance flag ($($gate.Detail)) - state was '$state', taking no action on $TaskName"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
    return
}

if ($gate.State -eq 'expired') {
    # Loud on purpose. Protection has just been restored underneath
    # someone who may still believe it is suppressed, and the flag is
    # still sitting on disk telling them otherwise.
    $line = "$gateStamp - IGNORING a maintenance flag at ${MaintenanceFlagPath}: $($gate.Detail) - protection is ACTIVE and the checks below WILL run; delete the flag"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
}

# Every remaining branch logs, so resolve the task info once - the
# original script queried it inline for exactly this purpose.
$lastResult = 'unknown'
$lastRun = $null
try {
    $taskInfo = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction Stop
    $lastResult = $taskInfo.LastTaskResult
    $lastRun = $taskInfo.LastRunTime
} catch { }

# PRESERVED EXACTLY from the pre-2026-08-23 script: local time stamped
# with a bare "Z". It is wrong - this is not UTC - and a UTC-vs-local
# misreading of a sibling log has already cost one session real time
# (docs/anomaly_ledger.md, the #44 self-correction). Preserved rather
# than fixed because this change was scoped to detection only, and
# because anything already grepping game-watchdog.log expects this
# shape. The recommended fix is filed in docs/ops_backup_and_watchdog.md.
$timestamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"

if ($state -eq 'foreign') {
    # A new state - it could not exist while detection was image-name
    # based. Something holds this deployment's port and it is not this
    # deployment. Restarting would only produce a bind failure, and
    # touching the other process is categorically forbidden, so this
    # logs loudly and stops.
    $line = "$timestamp - port $Port is held by a process OUTSIDE $ExpectedPathRoot ($detail) - NOT restarting $TaskName, NOT touching the other process"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
    exit 1
}

if ($state -eq 'listening-unverifiable') {
    $line = "$timestamp - port $Port is listening but its owner could not be verified against $ExpectedPathRoot ($detail) - NOT restarting $TaskName (raise the task to RunLevel=Highest to make image paths readable)"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
    exit 1
}

# $state -eq 'down'
if ($StartupGraceSeconds -gt 0 -and $null -ne $lastRun) {
    $age = ((Get-Date) - $lastRun).TotalSeconds
    if ($age -ge 0 -and $age -lt $StartupGraceSeconds) {
        $line = "$timestamp - nothing listening on port $Port, but $TaskName started $([int]$age)s ago (< ${StartupGraceSeconds}s startup grace) - not restarting yet"
        Add-Content -Path $LogPath -Value $line -Encoding utf8
        return
    }
}

# PRESERVED EXACTLY: same wording, same LastTaskResult field, same
# Add-Content/-Encoding utf8, same Start-ScheduledTask. Only the reason
# this branch is reached has changed.
$line = "$timestamp - game process not found (LastTaskResult=$lastResult) - restarting"
Add-Content -Path $LogPath -Value $line -Encoding utf8
Start-ScheduledTask -TaskName $TaskName
