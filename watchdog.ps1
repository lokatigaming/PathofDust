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

$logPath = Join-Path $PSScriptRoot "watchdog.log"
$proc = Get-Process -Name "twitch-bot-rs" -ErrorAction SilentlyContinue

if (-not $proc) {
    $timestamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
    $taskInfo = Get-ScheduledTaskInfo -TaskName "TwitchBotRS"
    $line = "$timestamp - bot process not found (LastTaskResult=$($taskInfo.LastTaskResult)) - restarting"
    Add-Content -Path $logPath -Value $line -Encoding utf8
    Start-ScheduledTask -TaskName "TwitchBotRS"
}
