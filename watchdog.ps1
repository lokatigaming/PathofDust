# Watchdog for the TwitchBotRS scheduled task - checks every run whether
# the bot process is actually alive, and if not, restarts the task and
# logs the recovery. Exists because Windows Task Scheduler's own
# RestartOnFailure (RestartCount/RestartInterval, already configured on
# the TwitchBotRS task itself) proved unreliable in practice - it did not
# fire across 4 real STATUS_STACK_OVERFLOW crashes on 2026-08-16, each of
# which needed a manual Start-ScheduledTask to recover. This script is the
# independent, verifiable substitute: registered as its own scheduled
# task ("TwitchBotRS-Watchdog") on a short repeating trigger, so it
# doesn't depend on the same native mechanism that already failed once.
#
# ---------------------------------------------------------------------
# MAINTENANCE GATE (2026-08-24, fix/watchdog-maintenance-gate)
# ---------------------------------------------------------------------
# REFACTOR_PLAN.md section 13 step 4's conditional bot branch tells a
# deploy session to disable TwitchBotRS-Watchdog before swapping
# twitch-bot-rs.exe. It cannot: Disable-ScheduledTask needs an elevated
# token and no deploy session has one, so the step silently did not
# happen and every bot swap ran with this watchdog live. Identical defect
# to the one found on the game side during the 2026-08-23 pacing deploy.
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
# bot, never toward staying quiet. A forgotten flag disabling protection
# forever is a worse and quieter failure than the one being fixed.
#
# SEPARATE FLAG FROM THE GAME'S, deliberately. Section 13 deploys the
# game unconditionally and the bot only when the diff says so, so a
# shared flag would suppress THIS watchdog through every game-only deploy
# - a window in which section 13 explicitly leaves the bot running
# untouched. See maintenance-flag.ps1's header for the full reasoning.
#
# The gate is consulted ONLY on runs that would otherwise restart, so a
# healthy run still logs nothing at all, flag or no flag.
#
# DELIBERATELY DUPLICATED, not shared with game-watchdog.ps1.
# Get-MaintenanceGate below is a copy of that script's function. A shared
# dot-sourced module would drift-proof them, but it would also give each
# watchdog a second file it must find in order to work, and a watchdog
# that throws is a watchdog that has stopped watching (the same reasoning
# behind game-watchdog.ps1's netstat fallback). Falling back to "no gate"
# when the module is missing would be worse still: the deploy would
# believe it was suppressed while this script restarted the bot mid-swap.
# KEEP THE TWO COPIES IN SYNC.
#
# WHAT THIS CHANGE DOES *NOT* TOUCH. Detection is still
# Get-Process -Name, the restart is still Start-ScheduledTask, and the
# "bot process not found (LastTaskResult=...) - restarting" line and its
# timestamp format are byte-identical to before. Only the gate is new.
#
# NOTE, not fixed here and out of this change's scope: detection is by
# IMAGE NAME, which cannot distinguish two deployments' bots from each
# other - the same flaw game-watchdog.ps1 was rewritten away from on
# 2026-08-23 (it now detects by listening port). This script only READS
# the name; it never terminates anything by name or by any other means,
# so it is not a violation of CLAUDE.md's PRODUCTION SAFETY rule. But
# before a second deployment exists this needs the same port-based
# rewrite, or the second bot's watchdog will read the first bot as proof
# that its own is alive. Filed in docs/ops_backup_and_watchdog.md.
#
# NEVER TERMINATES ANYTHING. This script starts a scheduled task and
# writes a log line. No Stop-Process, no taskkill, no Stop-ScheduledTask,
# by image name or otherwise.

[CmdletBinding()]
param(
    [string] $LogPath,

    # The maintenance flag a deploy session drops to suppress this
    # watchdog without elevation. Resolved in the BODY - a param-block
    # default of $PSScriptRoot arrives EMPTY under the `-File` invocation
    # the scheduled task uses, which is exactly the bug that made
    # game-watchdog.ps1's -RequireOwnPath silently useless.
    [string] $MaintenanceFlagPath,

    # How long a maintenance flag stays valid. Past this it is ignored
    # and the fact logged loudly. 0 disables the gate entirely (every
    # flag ignored), which is the safe direction to fail.
    [int] $MaintenanceMaxAgeMinutes = 30,

    # Report what this run would do. Starts nothing, writes nothing.
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

# Both defaults resolve HERE, in the body, where $PSScriptRoot is
# actually populated under -File. The original script already did this
# correctly for its log path - it had no param block at all, so it never
# had the defect game-watchdog.ps1 did - and that is preserved; the new
# flag path simply follows the same shape.
if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $LogPath = Join-Path $PSScriptRoot "watchdog.log"
}
if ([string]::IsNullOrWhiteSpace($MaintenanceFlagPath)) {
    $MaintenanceFlagPath = Join-Path $PSScriptRoot 'bot-watchdog-maintenance.flag'
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

# PRESERVED EXACTLY: image-name detection, unchanged (see the header's
# note on why it is not being rewritten here).
$proc = Get-Process -Name "twitch-bot-rs" -ErrorAction SilentlyContinue

if ($DryRun) {
    $gateDry = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
    $gateShown = $gateDry.State
    if ($gateDry.Detail) { $gateShown = "$($gateDry.State) - $($gateDry.Detail)" }
    $procShown = 'NOT FOUND'
    if ($proc) { $procShown = "alive (pid $(($proc | ForEach-Object { $_.Id }) -join ', '))" }
    Write-Host "task             : TwitchBotRS"
    Write-Host "log              : $LogPath"
    Write-Host "maintenance flag : $MaintenanceFlagPath"
    Write-Host "maintenance gate : $gateShown"
    Write-Host "bot process      : $procShown"
    $action = 'nothing (healthy)'
    if (-not $proc) {
        if ($gateDry.State -eq 'active') {
            $action = 'SUPPRESSED by the maintenance flag - WOULD LOG the suppression and do nothing else'
        } elseif ($gateDry.State -eq 'expired') {
            $action = 'WOULD LOG that it ignored a stale maintenance flag, THEN log + Start-ScheduledTask -TaskName TwitchBotRS'
        } else {
            $action = 'WOULD LOG + Start-ScheduledTask -TaskName TwitchBotRS'
        }
    }
    Write-Host "would do         : $action"
    Write-Host 'would terminate  : nothing, ever - this script has no process-termination code'
    return
}

if (-not $proc) {
    # -----------------------------------------------------------------
    # Maintenance gate - checked ONLY here, inside the would-restart
    # branch, so a healthy run still logs nothing whether or not a flag
    # exists.
    # -----------------------------------------------------------------
    $gate = Get-MaintenanceGate -Path $MaintenanceFlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
    $gateStamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"

    if ($gate.State -eq 'active') {
        $line = "$gateStamp - SUPPRESSED by maintenance flag ($($gate.Detail)) - bot process not found, taking no action on TwitchBotRS"
        Add-Content -Path $LogPath -Value $line -Encoding utf8
        return
    }

    if ($gate.State -eq 'expired') {
        # Loud on purpose: protection has just been restored underneath
        # someone who may still believe it is suppressed, and the flag is
        # still on disk telling them otherwise.
        $line = "$gateStamp - IGNORING a maintenance flag at ${MaintenanceFlagPath}: $($gate.Detail) - protection is ACTIVE and the restart below WILL run; delete the flag"
        Add-Content -Path $LogPath -Value $line -Encoding utf8
    }

    # PRESERVED EXACTLY from the pre-gate script: same timestamp format
    # (local time stamped with a bare "Z" - wrong, and preserved because
    # anything grepping watchdog.log expects this shape), same
    # LastTaskResult field, same wording, same Add-Content/-Encoding, same
    # Start-ScheduledTask.
    $timestamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
    $taskInfo = Get-ScheduledTaskInfo -TaskName "TwitchBotRS"
    $line = "$timestamp - bot process not found (LastTaskResult=$($taskInfo.LastTaskResult)) - restarting"
    Add-Content -Path $LogPath -Value $line -Encoding utf8
    Start-ScheduledTask -TaskName "TwitchBotRS"
}
