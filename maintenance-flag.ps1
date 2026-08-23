# Maintenance flag for game-watchdog.ps1 - the non-elevated replacement
# for REFACTOR_PLAN.md section 13 step 4's "disable the watchdog".
#
# WHY THIS EXISTS (2026-08-23, fix/watchdog-maintenance-gate)
# ----------------------------------------------------------
# Section 13 step 4 tells a deploy session to disable GameProcess-Watchdog
# before swapping the binary. It cannot: Disable-ScheduledTask returns
# Access denied without an elevated token, and no deploy session has one.
# The step has therefore been silently skipped on every deploy to date,
# and each binary swap ran with the watchdog live. Nothing has broken yet
# only because swaps finish well inside the task's ~2 minute repetition
# interval - luck, not protection. A slower swap (a large backup, a
# retried copy, a stalled disk) races it, and the watchdog would restart
# the game from a half-written binary.
#
# A FILE is the suppression a non-elevated session can actually create.
# This script writes and removes it; game-watchdog.ps1 honors it. Neither
# script registers, alters, enables or disables a scheduled task, and
# neither terminates a process by any means.
#
# THE FLAG EXPIRES. game-watchdog.ps1 ignores one older than its
# -MaintenanceMaxAgeMinutes (default 30) and says loudly in its log that
# it did. A forgotten flag disabling protection forever is a worse and
# quieter failure than the one being fixed, so the flag is a lease, not a
# switch. -Set is deliberately cheap to repeat: if a deploy genuinely
# needs longer than the window, re-run it rather than raising the limit.
#
# USAGE
# -----
#   .\maintenance-flag.ps1 -Set -Reason "pacing deploy 0110be6"
#   .\maintenance-flag.ps1 -Status
#   .\maintenance-flag.ps1 -Clear
#
# -Clear is safe to run unconditionally, including when no flag exists,
# so a deploy's cleanup path never needs a conditional around it.
#
# THE FLAG MUST LAND WHERE THE RUNNING WATCHDOG LOOKS. Both scripts
# default their flag path off their OWN $PSScriptRoot, which agree only
# because both sit in the deployment root. Run this helper from a COPY -
# a worktree checkout, which is exactly where a deploy session naturally
# has a shell open - and the flag is written somewhere production's
# watchdog never reads, while -Status cheerfully reports "SUPPRESSED".
# The swap then runs unprotected under an operator who believes it is
# gated: the same false-confidence failure as the $ExpectedPathRoot bug
# this branch also fixed, just relocated.
#
# So -Set resolves the AUTHORITATIVE root from the scheduled task itself
# (its action's `-File` argument, readable without elevation) and REFUSES
# to write a flag anywhere else unless -Force. When the task cannot be
# read at all - another machine, a not-yet-registered second deployment -
# it warns and proceeds, because a tool that cannot be used is not a
# safety feature.

[CmdletBinding(DefaultParameterSetName = 'Status')]
param(
    # Create (or refresh) the flag.
    [Parameter(ParameterSetName = 'Set')]
    [switch] $Set,

    # Why the watchdog is being suppressed. Recorded in the flag and
    # echoed into the watchdog's log, so an operator reading the log
    # later can tell a deploy from a mistake. Required with -Set: an
    # unexplained flag is indistinguishable from a forgotten one.
    [Parameter(ParameterSetName = 'Set', Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $Reason,

    # Remove the flag. Not an error when there is nothing to remove.
    [Parameter(ParameterSetName = 'Clear')]
    [switch] $Clear,

    # Report what the flag currently says and whether the watchdog would
    # honor it. Default when no other action is given.
    [Parameter(ParameterSetName = 'Status')]
    [switch] $Status,

    # Same default as game-watchdog.ps1's own, and resolved in the BODY
    # for the same reason: a param-block default of $PSScriptRoot arrives
    # EMPTY under the `-File` invocation scheduled tasks use. Pass this
    # explicitly when driving a second deployment's flag.
    [string] $FlagPath,

    # Only used to report whether the flag still looks current. The
    # watchdog owns the real decision; this must match the value it runs
    # with to be meaningful.
    [int] $MaintenanceMaxAgeMinutes = 30,

    # The scheduled task whose deployment root is authoritative for where
    # the flag belongs. Reading its action tells us which game-watchdog.ps1
    # is actually running, and therefore which directory it will look in.
    [string] $WatchdogTaskName = 'GameProcess-Watchdog',

    # Write the flag even when it would not land in the running
    # watchdog's own directory. For a second deployment whose task is not
    # registered yet, or a deliberate out-of-band test.
    [switch] $Force
)

$ErrorActionPreference = 'Stop'

# ValidateNotNullOrEmpty above is the most Windows PowerShell 5.1 offers
# (ValidateNotNullOrWhiteSpace is 7+), so the whitespace-only case is
# rejected here instead. An unexplained flag reads exactly like a
# forgotten one, which is the failure this whole mechanism exists to
# keep visible.
if ($Set -and [string]::IsNullOrWhiteSpace($Reason)) {
    throw "-Reason cannot be blank: the flag records why the watchdog is suppressed, and an unexplained flag is indistinguishable from a forgotten one."
}

if ([string]::IsNullOrWhiteSpace($FlagPath)) {
    $FlagPath = Join-Path $PSScriptRoot 'watchdog-maintenance.flag'
}

function Get-WatchdogRoot {
    # The directory of the game-watchdog.ps1 the scheduled task actually
    # runs, parsed out of its action's `-File "<path>"` argument. Returns
    # $null when the task is absent or its action cannot be read - both
    # legitimate (another machine, an unregistered second deployment), so
    # callers warn rather than fail on $null.
    param([string] $TaskName)

    $action = $null
    try { $action = (Get-ScheduledTask -TaskName $TaskName -ErrorAction Stop).Actions | Select-Object -First 1 } catch { return $null }
    if ($null -eq $action -or [string]::IsNullOrWhiteSpace($action.Arguments)) { return $null }

    # -File may be quoted or bare; take the first .ps1 either way.
    $m = [regex]::Match($action.Arguments, '-File\s+"([^"]+\.ps1)"')
    if (-not $m.Success) { $m = [regex]::Match($action.Arguments, '-File\s+(\S+\.ps1)') }
    if (-not $m.Success) { return $null }

    try { return [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($m.Groups[1].Value)) } catch { return $null }
}

function Test-SameDirectory {
    param([string] $A, [string] $B)
    if ([string]::IsNullOrWhiteSpace($A) -or [string]::IsNullOrWhiteSpace($B)) { return $false }
    try {
        return ([IO.Path]::GetFullPath($A).TrimEnd('\')) -ieq ([IO.Path]::GetFullPath($B).TrimEnd('\'))
    } catch { return $false }
}

function Show-FlagStatus {
    param([string] $Path, [int] $MaxAgeMinutes)

    if (-not (Test-Path -LiteralPath $Path)) {
        Write-Host "flag     : (none) - $Path"
        Write-Host "watchdog : NOT suppressed"
        return
    }

    $raw = $null
    try { $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop } catch { }
    Write-Host "flag     : $Path"

    $parsed = $null
    if (-not [string]::IsNullOrWhiteSpace($raw)) {
        try { $parsed = $raw | ConvertFrom-Json -ErrorAction Stop } catch { }
    }
    if ($null -eq $parsed) {
        Write-Host 'contents : UNREADABLE / not valid JSON'
        Write-Host 'watchdog : NOT suppressed - it ignores a flag it cannot verify, and logs that it did'
        return
    }

    $created = $null
    if ($parsed.PSObject.Properties.Name -contains 'created') {
        try { $created = [datetimeoffset]::Parse([string]$parsed.created, [Globalization.CultureInfo]::InvariantCulture) } catch { }
    }
    Write-Host "reason   : $($parsed.reason)"
    Write-Host "by       : $($parsed.by)"
    Write-Host "created  : $($parsed.created)"

    if ($null -eq $created) {
        Write-Host 'watchdog : NOT suppressed - no parseable timestamp, so the flag cannot be verified as current'
        return
    }
    $age = ([datetimeoffset]::Now - $created).TotalMinutes
    Write-Host ("age      : {0:N1} minutes (limit {1})" -f $age, $MaxAgeMinutes)
    if ($age -lt -2) {
        Write-Host 'watchdog : NOT suppressed - flag is dated in the future'
    } elseif ($age -gt $MaxAgeMinutes) {
        Write-Host 'watchdog : NOT suppressed - flag has EXPIRED; protection is active and the watchdog is logging that it ignored this flag. Delete it.'
    } else {
        Write-Host ("watchdog : SUPPRESSED for another {0:N1} minutes" -f ($MaxAgeMinutes - $age))
    }
}

if ($Set) {
    # Refuse to write a flag the running watchdog will never read - see
    # the header. This is the whole reason -Set is not just Set-Content.
    $flagDir = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($FlagPath))
    $watchdogRoot = Get-WatchdogRoot -TaskName $WatchdogTaskName
    if ($null -eq $watchdogRoot) {
        Write-Warning "Could not read '$WatchdogTaskName' to confirm where the running watchdog looks for its flag. Writing to $flagDir on trust - verify that is the deployment being swapped."
    } elseif (-not (Test-SameDirectory $flagDir $watchdogRoot)) {
        if (-not $Force) {
            throw @"
REFUSING to write a flag the running watchdog will never read.

  flag would go to : $flagDir
  '$WatchdogTaskName' runs the watchdog from : $watchdogRoot

Those must be the same directory. You are almost certainly running this
helper from a worktree or other copy rather than from the deployment you
are about to swap. Writing here would report "SUPPRESSED" while leaving
the real watchdog fully live - the exact false confidence this guard
exists to prevent.

Run the deployment's own copy instead:
  $watchdogRoot\maintenance-flag.ps1 -Set -Reason "..."

Or pass -Force if you genuinely mean to flag a different deployment.
"@
        }
        Write-Warning "-Force: writing to $flagDir, which is NOT where '$WatchdogTaskName' looks ($watchdogRoot). That watchdog is NOT suppressed."
    }

    $payload = [ordered]@{
        # Round-trip ISO 8601 WITH the real UTC offset. Deliberately not
        # the bare-"Z"-on-local-time shape game-watchdog.log uses: that
        # shape is a known-wrong format preserved there only for
        # compatibility with existing log greps (see its own comment and
        # docs/anomaly_ledger.md #44), and it must not spread into a value
        # something actually computes an age from.
        created = (Get-Date).ToString('o')
        reason  = $Reason
        by      = "$env:USERNAME@$env:COMPUTERNAME"
        pid     = $PID
    }
    $json = $payload | ConvertTo-Json
    Set-Content -LiteralPath $FlagPath -Value $json -Encoding utf8
    Write-Host "maintenance flag SET"
    Show-FlagStatus -Path $FlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes
    if ($null -ne $watchdogRoot -and (Test-SameDirectory $flagDir $watchdogRoot)) {
        Write-Host "verified : this IS the directory '$WatchdogTaskName' reads"
    }
    Write-Host ''
    Write-Host "REMEMBER: .\maintenance-flag.ps1 -Clear  (it expires on its own in $MaintenanceMaxAgeMinutes minutes either way)"
    return
}

if ($Clear) {
    if (Test-Path -LiteralPath $FlagPath) {
        Remove-Item -LiteralPath $FlagPath -Force
        Write-Host "maintenance flag CLEARED - $FlagPath"
    } else {
        Write-Host "no maintenance flag to clear - $FlagPath"
    }
    Write-Host 'watchdog : protection ACTIVE'
    return
}

Show-FlagStatus -Path $FlagPath -MaxAgeMinutes $MaintenanceMaxAgeMinutes

# Whether this flag path is even the one the running watchdog consults.
# A "SUPPRESSED" line above means nothing if the answer is no.
$statusFlagDir = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($FlagPath))
$statusRoot = Get-WatchdogRoot -TaskName $WatchdogTaskName
if ($null -eq $statusRoot) {
    Write-Host "scope    : could not read '$WatchdogTaskName' - cannot confirm this is the flag it reads"
} elseif (Test-SameDirectory $statusFlagDir $statusRoot) {
    Write-Host "scope    : this IS the flag '$WatchdogTaskName' reads"
} else {
    Write-Host "scope    : *** WRONG DIRECTORY *** '$WatchdogTaskName' reads $statusRoot - anything above about suppression does NOT apply to it"
}
