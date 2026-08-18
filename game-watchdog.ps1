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

$logPath = Join-Path $PSScriptRoot "game-watchdog.log"
$proc = Get-Process -Name "game" -ErrorAction SilentlyContinue

if (-not $proc) {
    $timestamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
    $taskInfo = Get-ScheduledTaskInfo -TaskName "GameProcess"
    $line = "$timestamp - game process not found (LastTaskResult=$($taskInfo.LastTaskResult)) - restarting"
    Add-Content -Path $logPath -Value $line -Encoding utf8
    Start-ScheduledTask -TaskName "GameProcess"
}
