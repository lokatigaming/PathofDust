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

[CmdletBinding()]
param(
    # The port that identifies THIS deployment. 4005 is adventure_web
    # (game/src/main.rs, ADVENTURE_WEB_PORT default) - the port the
    # cloudflared tunnel fronts and the one the bot's
    # ADVENTURE_API_BASE_URL points at, i.e. the port whose absence
    # actually means this world is down. A second deployment passes its
    # own (e.g. -Port 4015).
    [int] $Port = 4005,

    [string] $TaskName = 'GameProcess',

    # The deployment root the listening process is expected to live
    # under. Only consulted when the image path is readable at all.
    [string] $ExpectedPathRoot = $PSScriptRoot,

    [string] $LogPath,

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
    Write-Host "would do         : $action"
    Write-Host 'would terminate  : nothing, ever - this script has no process-termination code'
    return
}

# ---------------------------------------------------------------------
# Act
# ---------------------------------------------------------------------

if ($state -eq 'healthy') { return }
if ($state -eq 'listening-unverifiable' -and -not $RequireOwnPath) { return }

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
