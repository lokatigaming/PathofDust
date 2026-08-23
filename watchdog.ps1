# Watchdog for the TwitchBotRS scheduled task - checks every run whether
# the bot is actually alive, and if not, restarts the task and logs the
# recovery. Exists because Windows Task Scheduler's own RestartOnFailure
# (RestartCount/RestartInterval, already configured on the TwitchBotRS
# task itself) proved unreliable in practice - it did not fire across 4
# real STATUS_STACK_OVERFLOW crashes on 2026-08-16, each of which needed
# a manual Start-ScheduledTask to recover. This script is the
# independent, verifiable substitute: registered as its own scheduled
# task ("TwitchBotRS-Watchdog") on a short repeating trigger, so it
# doesn't depend on the same native mechanism that already failed once.
#
# ---------------------------------------------------------------------
# DETECTION REWRITE (2026-08-24, fix/watchdog-maintenance-gate)
# ---------------------------------------------------------------------
# This script used to detect liveness with `Get-Process -Name
# "twitch-bot-rs"`. That is wrong the moment a second deployment exists:
# two twitch-bot-rs.exe processes share one image name, so the check
# returns non-empty whenever EITHER is alive and the watchdog silently
# stops detecting the death of EITHER one. Standing up a second bot would
# have un-protected the live one with no visible signal. Exactly the flaw
# game-watchdog.ps1 was rewritten away from on 2026-08-23; this is the
# same rewrite, applied to the bot.
#
# Liveness is now "is anything LISTENING on MY port", which is
# per-deployment by construction, needs no image-name matching, and uses
# the same port-to-PID resolution CLAUDE.md's PRODUCTION SAFETY rule
# already requires before stopping anything.
#
# WHY PORT 4001 AND NOT 4002/4003. The bot binds three ports (src/config.rs
# :258/:267/:275): 4001 alerts, 4002 song requests, 4003 chat overlay.
# (4004/4005 are still DECLARED in the bot's config but are bound by the
# GAME process since the 2026-08-22 bot/game decoupling - keying on either
# would watch the wrong process entirely.) Of the three real ones:
#
#   * 4002 is CONDITIONAL - `start_song_overlay_server` only runs when
#     `config.youtube_api_keys` is non-empty (main.rs:555). Clear the
#     YouTube keys and 4002 never binds, so a watchdog keyed to it would
#     read a perfectly healthy bot as dead and restart it forever.
#     Disqualified outright.
#   * 4003 binds LAST, after `emotes::fetch_all` (main.rs:579) - a network
#     round-trip to Twitch. A slow emote fetch would read as death.
#   * 4001 is unconditional (main.rs:550) and the EARLIEST of the three,
#     and `start_alert_server` awaits `TcpListener::bind` before returning
#     (alerts.rs:60), so "listening on 4001" is a true, synchronous signal
#     that the bot got that far.
#
# A crash releases all three ports, so any of them detects a real death
# equally; the difference is entirely in FALSE positives, and 4001 has the
# fewest. It is the port whose absence actually means this bot is down.
#
# THIS IS A DETECTION CHANGE ONLY. The restart action, the log line's
# exact text and timestamp format, and the fact that a healthy run logs
# nothing at all are all preserved. The maintenance gate added earlier on
# this branch is unchanged.
#
# STARTUP GRACE IS SIZED TO THE BOT, NOT THE GAME. game-watchdog.ps1 uses
# 90s because `AdventureManager::new` loads a 3.3 MB roster and runs any
# pending migrations before its servers start. The bot's pre-bind path is
# much cheaper: create the log dir, `Config::load`, `AuthClient::new`
# (local tokens.json), then ONE Helix call (`get_user_id_by_login`,
# main.rs:543) before 4001 binds. Neither auth.rs nor helix.rs contains a
# retry loop, a sleep or an explicit timeout, so the normal case is
# sub-second and the worst case is one slow HTTPS round-trip. 45s is a
# very large margin over that while recovering a genuinely dead bot in
# half the game's window - which matters, since 4 unrecovered crashes are
# why this file exists.
#
# ---------------------------------------------------------------------
# MAINTENANCE GATE (2026-08-24, same branch)
# ---------------------------------------------------------------------
# REFACTOR_PLAN.md section 13 step 4's conditional bot branch tells a
# deploy session to disable TwitchBotRS-Watchdog before swapping
# twitch-bot-rs.exe. It cannot: Disable-ScheduledTask needs an elevated
# token and no deploy session has one, so the step silently did not
# happen and every bot swap ran with this watchdog live.
#
# So suppression is a FILE, which a non-elevated session can create:
# while a valid flag exists at $MaintenanceFlagPath, this script logs
# that it is suppressed and exits without restarting anything.
# maintenance-flag.ps1 -Target Bot writes and removes it.
#
# THE FLAG EXPIRES. Past $MaintenanceMaxAgeMinutes (default 30) it is
# IGNORED, the fact is logged loudly, and normal protection resumes.
# Anything unreadable, undated, or dated in the future is treated the
# same way. A flag that cannot be positively verified as current
# suppresses nothing - every ambiguous case fails toward protecting the
# bot, never toward staying quiet.
#
# SEPARATE FLAG FROM THE GAME'S, deliberately. Section 13 deploys the
# game unconditionally and the bot only when the diff says so, so a
# shared flag would suppress THIS watchdog through every game-only deploy
# - a window in which section 13 explicitly leaves the bot running
# untouched. See maintenance-flag.ps1's header for the full reasoning.
#
# The gate is consulted ONLY on runs that would otherwise act, so a
# healthy run still logs nothing at all, flag or no flag.
#
# WHAT THE PATH CHECK CAN AND CANNOT DO HERE. Same measurement as the
# game side (2026-08-23): TwitchBotRS and TwitchBotRS-Watchdog both run as
# `Administrator` with `RunLevel = Limited`, and from a non-elevated
# context `(Get-Process -Id N).Path`, `.MainModule.FileName` and
# `Win32_Process.ExecutablePath` all come back EMPTY. So the RESTART
# decision hinges only on "is anything listening on my port"; the path
# check is three-state (confirmed / foreign / unverifiable) and
# -RequireOwnPath promotes "unverifiable" to a logged warning for an
# operator who has raised the task to RunLevel = Highest. OFF by default,
# so behaviour at today's run level is unchanged.
#
# DELIBERATELY DUPLICATED, not shared with game-watchdog.ps1.
# Get-MaintenanceGate, Get-ListenerPids, Get-ProcessFacts and
# Test-UnderRoot are copies of that script's functions. A shared
# dot-sourced module would drift-proof them, but it would also give each
# watchdog a second file it must find in order to work, and a watchdog
# that throws is a watchdog that has stopped watching. KEEP IN SYNC.
#
# NEVER TERMINATES ANYTHING. This script starts a scheduled task and
# writes a log line. No Stop-Process, no taskkill, no Stop-ScheduledTask,
# by image name or otherwise. The image name it reads is written into the
# log for a human reading an incident later and never drives a decision.
#
# Registering or altering scheduled tasks is out of this script's scope -
# see docs/ops_backup_and_watchdog.md.

[CmdletBinding()]
param(
    # The port that identifies THIS deployment's bot. 4001 is the alert
    # server (src/config.rs:258) - see the header for why not 4002/4003.
    # A second deployment passes its own.
    [int] $Port = 4001,

    # The task this watchdog restarts. NOT its own task name
    # (TwitchBotRS-Watchdog); this is the bot itself.
    [string] $TaskName = 'TwitchBotRS',

    # The deployment root the listening process is expected to live
    # under. Only consulted when the image path is readable at all.
    # Resolved in the BODY - a param-block default of $PSScriptRoot
    # arrives EMPTY under the `-File` invocation the scheduled task uses.
    [string] $ExpectedPathRoot,

    [string] $LogPath,

    # The maintenance flag a deploy session drops to suppress this
    # watchdog without elevation. Resolved in the BODY for the same
    # reason as the two above.
    [string] $MaintenanceFlagPath,

    # How long a maintenance flag stays valid. Past this it is ignored
    # and the fact logged loudly. 0 disables the gate entirely (every
    # flag ignored), which is the safe direction to fail.
    [int] $MaintenanceMaxAgeMinutes = 30,

    # A single in-run re-check before believing the bot is down. A
    # port-based check does not see the process until it has bound;
    # without this the new detection would be strictly more
    # trigger-happy than the old name-based one. Set 0 to disable.
    [int] $RecheckDelaySeconds = 5,

    # The same startup window from the other side: do not restart a task
    # that only just started. 45s, sized to the bot's own boot path - see
    # the header. Set 0 to disable.
    [int] $StartupGraceSeconds = 45,

    # Report the resolved state and what would happen. Starts nothing,
    # writes nothing to the log.
    [switch] $DryRun,

    # Treat "listening, but I could not read its image path" as a logged
    # warning instead of silent health. Only useful once the task runs
    # with RunLevel = Highest.
    [switch] $RequireOwnPath
)

$ErrorActionPreference = 'Stop'

# Every default resolves HERE, in the body, where $PSScriptRoot is
# actually populated under -File. The original script already did this
# correctly for its log path - it had no param block at all, so it never
# had the defect game-watchdog.ps1 did - and that is preserved.
if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $LogPath = Join-Path $PSScriptRoot "watchdog.log"
}
if ([string]::IsNullOrWhiteSpace($ExpectedPathRoot)) {
    $ExpectedPathRoot = $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($MaintenanceFlagPath)) {
    $MaintenanceFlagPath = Join-Path $PSScriptRoot 'bot-watchdog-maintenance.flag'
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
    # bot rather than silencing the watchdog.
    #
    # COPY of game-watchdog.ps1's function of the same name - see this
    # file's header for why it is duplicated rather than shared. Keep in
    # sync.
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

    $created = $null
    try { $created = [datetimeoffset]::Parse([string]$parsed.created, [Globalization.CultureInfo]::InvariantCulture) } catch { }
    if ($null -eq $created) {
        return @{ State = 'expired'; Detail = "flag file's 'created' value is not a parseable timestamp (reason='$reason')" }
    }

    $ageMinutes = ([datetimeoffset]::Now - $created).TotalMinutes

    if ($ageMinutes -lt -2) {
        return @{ State = 'expired'; Detail = ("flag is dated {0:N1} minutes in the FUTURE (reason='$reason'$by)" -f [Math]::Abs($ageMinutes)) }
    }
    if ($ageMinutes -gt $MaxAgeMinutes) {
        return @{ State = 'expired'; Detail = ("flag is {0:N1} minutes old, past the {1}-minute limit (reason='$reason'$by)" -f $ageMinutes, $MaxAgeMinutes) }
    }

    return @{ State = 'active'; Detail = ("reason='$reason'$by age={0:N1}m limit={1}m" -f [Math]::Max($ageMinutes, 0), $MaxAgeMinutes) }
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
    $gateDry = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
    $gateShown = $gateDry.State
    if ($gateDry.Detail) { $gateShown = "$($gateDry.State) - $($gateDry.Detail)" }
    $shownPids = '(none)'
    if ($listenerPids.Count -gt 0) { $shownPids = ($listenerPids -join ', ') }
    $shownDetail = '(none)'
    if ($detail) { $shownDetail = $detail }

    Write-Host "port             : $Port"
    Write-Host "task             : $TaskName"
    Write-Host "expected root    : $ExpectedPathRoot"
    Write-Host "log              : $LogPath"
    Write-Host "maintenance flag : $MaintenanceFlagPath"
    Write-Host "maintenance gate : $gateShown"
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
            $action = 'SUPPRESSED by the maintenance flag - WOULD LOG the suppression and do nothing else'
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
# exists.
# ---------------------------------------------------------------------
$gate = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
$gateStamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"

if ($gate.State -eq 'active') {
    $line = "$gateStamp - SUPPRESSED by maintenance flag ($($gate.Detail)) - state was '$state', taking no action on $TaskName"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
    return
}

if ($gate.State -eq 'expired') {
    # Loud on purpose: protection has just been restored underneath
    # someone who may still believe it is suppressed, and the flag is
    # still on disk telling them otherwise.
    $line = "$gateStamp - IGNORING a maintenance flag at ${MaintenanceFlagPath}: $($gate.Detail) - protection is ACTIVE and the checks below WILL run; delete the flag"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
}

# Every remaining branch logs, so resolve the task info once. Kept as
# $taskInfo, and referenced below exactly as the pre-rewrite script did.
$taskInfo = $null
try { $taskInfo = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction Stop } catch { }

# PRESERVED EXACTLY from the pre-rewrite script: local time stamped with
# a bare "Z". It is wrong - this is not UTC - and is preserved rather
# than fixed because this change was scoped to detection only, and
# because anything already grepping watchdog.log expects this shape. The
# recommended fix is filed in docs/ops_backup_and_watchdog.md.
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
if ($StartupGraceSeconds -gt 0 -and $null -ne $taskInfo -and $null -ne $taskInfo.LastRunTime) {
    $age = ((Get-Date) - $taskInfo.LastRunTime).TotalSeconds
    if ($age -ge 0 -and $age -lt $StartupGraceSeconds) {
        $line = "$timestamp - nothing listening on port $Port, but $TaskName started $([int]$age)s ago (< ${StartupGraceSeconds}s startup grace) - not restarting yet"
        Add-Content -Path $LogPath -Value $line -Encoding utf8
        return
    }
}

# PRESERVED EXACTLY: same wording, same LastTaskResult field, same
# Add-Content/-Encoding utf8, same Start-ScheduledTask. Only the reason
# this branch is reached has changed.
$line = "$timestamp - bot process not found (LastTaskResult=$($taskInfo.LastTaskResult)) - restarting"
Add-Content -Path $LogPath -Value $line -Encoding utf8
Start-ScheduledTask -TaskName $TaskName
