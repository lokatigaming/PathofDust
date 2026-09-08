# Cutover runbook — moving the bot into `bot/`

**Written 2026-09-08 on `feature/bot-into-subdirectory`. NOT RUN.**
This stops the live bot mid-stream if run at the wrong time, so **the owner picks the
window.** Nothing in this file is executed by the session that wrote it.

Companion to the branch's code change. The branch moves the bot's *sources* and *assets*
in the repository; this runbook moves its *runtime state* and re-points the one live
object that knows where the bot runs: the `TwitchBotRS` scheduled task's
**WorkingDirectory**.

---

## WHY A CUTOVER IS NEEDED AT ALL

The bot has no path indirection — no `data_path`, no `GAME_DATA_DIR` equivalent. Every
one of its 16 runtime paths is a bare CWD-relative literal in `bot/src/**`. That is what
made the code change free, and it is exactly what makes the runtime change a real
operation: **the working directory *is* the data directory.** Change it without moving
the files and the bot starts with an empty roster of commands, no themes, no queue, and
no `tokens.json` — it will not even authenticate.

### What does NOT change, verified rather than assumed

| thing | why it is unaffected |
|---|---|
| `target\release\twitch-bot-rs.exe` | Cargo puts every workspace member's binary in one shared `target\`. Moving the crate moved no binary. **The task's `Execute` path does not change.** |
| `watchdog.ps1` | Stays at the repository root, unmodified except for a comment. Its `$ExpectedPathRoot` defaults to `$PSScriptRoot` and is compared against the listening process's **image path** — which is still `target\release\twitch-bot-rs.exe`, a sibling of the script. Moving the watchdog into `bot/` would have broken it. |
| `maintenance-flag.ps1 -Target Bot` | Resolves the authoritative root from the **`TwitchBotRS-Watchdog`** task's `-File` argument (`maintenance-flag.ps1:146,168`), which points at `watchdog.ps1` at the repository root. That path does not move, so the flag still lands where the running watchdog looks. |
| the bot's three ports | 4001 alerts / 4002 song / 4003 chat overlay, unchanged. **OBS browser sources need no edit.** |
| the game | Untouched. Its crate, its assets, its service, its data root and its watchdog are all where they were. |

---

## PRECONDITIONS

- [ ] The branch is merged and deployed by session c (it holds merge and deploy
      authority). This runbook assumes `bot\` exists in the deployment with the bot's
      sources and assets in it.
- [ ] The owner has chosen the window. **Off-air, or an accepted interruption** — every
      overlay goes dark for the duration and the song queue stops.
- [ ] A shell in the deployment root, not in a worktree copy. `maintenance-flag.ps1`
      refuses to write from a copy without `-Force` for exactly this reason, but the file
      moves below have no such guard.

---

## STEP 0 — TAKE A BACKUP FIRST

This tool did not exist before today, which is the whole reason this work happened. Use
it before touching anything.

```powershell
cd C:\PathofDust
.\backup-bot-data.ps1 -IncludeEnv -Verbose
```

Note it runs from wherever the bot's state currently is — **pre-cutover that is the
deployment root, so `backup-bot-data.ps1` is invoked from there, not from `bot\`.** After
the cutover it lives in `bot\` and needs no argument at all.

- [ ] verdict reads `clean`
- [ ] `filesCopied` matches what the run reports absent — every file NOT listed as absent
      must have been copied
- [ ] the snapshot directory exists under `<root>-backups\`

**Do not proceed on a `degraded` verdict.** Degraded means a source did not parse, and
the run deliberately exits 1 and prunes nothing. Find out why first; a cutover is the
worst moment to discover a file was already corrupt.

---

## STEP 1 — SUPPRESS THE BOT WATCHDOG. THIS IS THE STEP THAT LOSES A TOKEN.

`watchdog.ps1` exists to restart the bot when its port goes quiet. Left armed through a
cutover it does exactly that — **restarting the OLD bot, from the OLD working directory,
while you are copying its state.** Two processes then hold `tokens.json`, and whichever
writes last wins the refresh.

Use the existing mechanism rather than disabling the task; the flag needs no elevation
and `-Clear` is safe to run unconditionally.

```powershell
.\maintenance-flag.ps1 -Target Bot -Set -Reason "bot directory cutover"
.\maintenance-flag.ps1 -Target Bot -Status
```

- [ ] `-Status` reports SUPPRESSED
- [ ] the reported watchdog root is the deployment root, not a worktree

---

## STEP 2 — STOP THE BOT. BY PID, NEVER BY IMAGE NAME.

**`taskkill /IM twitch-bot-rs.exe` is banned and a `/FI` filter is not a safeguard**
(CLAUDE.md PRODUCTION SAFETY). A filter that matches nothing today matches everything the
day it stops applying, and the bot shares its image name with any second deployment.

Stop the scheduled task first — that is the supported path and it needs no PID at all:

```powershell
Stop-ScheduledTask -TaskName TwitchBotRS
```

Then confirm the port is actually free, resolving port → PID → image path:

```powershell
$c = Get-NetTCPConnection -State Listen -LocalPort 4001 -ErrorAction SilentlyContinue
if ($c) {
  $p = Get-Process -Id $c.OwningProcess
  "$($p.Id)  $($p.Path)"     # confirm this path before doing anything to it
}
```

- [ ] nothing is listening on 4001
- [ ] if something still is, and only if its resolved path is the bot you intend to stop,
      `Stop-Process -Id <that PID>` — **`-Id`, never `-Name`**

---

## STEP 3 — COPY. DO NOT MOVE.

Copy is the rollback. A move leaves nothing to go back to, and the whole reason this
sequence is safe is that the old directory stays complete and consistent until the new
one has proven itself.

The bot is stopped, so the copy cannot land inside a write. (Its `write_atomic` means a
copy could not have been torn anyway — see `bot/src/state.rs` — but a stopped process
also cannot append a refresh after the copy has read the file, which is the real reason
for the ordering.)

```powershell
cd C:\PathofDust
$files = @(
  'tokens.json','commands.json','entrance-themes.json','personal-playlists.json',
  'song-queue.json','search-cache.json','daily-greeted.json','bugreports.json',
  'tips-history.json','paypal-tips-history.json',
  'channel-points-interrupt-reward.json','channel-points-theme-reward.json',
  'playrandom-state.json'
)
foreach ($f in $files) { if (Test-Path $f) { Copy-Item $f -Destination "bot\$f" } }
```

**All 13, not the 11 the backup keeps.** The backup deliberately excludes
`search-cache.json` and `daily-greeted.json` as regenerable — but "regenerable" is an
argument about what is worth *retaining*, not about what is worth *carrying across a
move*. Leaving them behind costs a day's YouTube quota and a round of duplicate
greetings for no gain.

### `.env` — split, do not move

The bot reads 29 keys and the game reads 5, and **no key is read by both**. So each side
gets its own file, and the game's existing `.env` stays exactly where it is.

```powershell
Copy-Item .env -Destination bot\.env
```

- [ ] `bot\.env` exists
- [ ] the game's `.env` is untouched at the root

Copying the whole file rather than filtering it is deliberate: the surplus keys are inert
on both sides (each process reads only its own names), and a hand-filtered file is a
chance to drop a key the bot needs. `bot\.env.example` and the root `.env.example`
document the two halves for anyone building a fresh install; **trimming a live `.env` buys
nothing and risks a silent feature-off.**

---

## STEP 4 — RE-POINT THE TASK AND START

The `TwitchBotRS` task's **WorkingDirectory** is the one live object that has to change.
Its `Execute` path does not — the binary is still `target\release\twitch-bot-rs.exe`.

```powershell
$t = Get-ScheduledTask -TaskName TwitchBotRS
$t.Actions[0] | Format-List Execute, Arguments, WorkingDirectory   # RECORD THIS FIRST
```

- [ ] **the original three values are written down before anything is changed** — they are
      the rollback

```powershell
$a = New-ScheduledTaskAction -Execute $t.Actions[0].Execute `
       -Argument $t.Actions[0].Arguments `
       -WorkingDirectory 'C:\PathofDust\bot'
Set-ScheduledTask -TaskName TwitchBotRS -Action $a
Start-ScheduledTask -TaskName TwitchBotRS
```

---

## STEP 5 — VERIFY, IN THIS ORDER

Ordered cheapest-first, and by what each failure would mean.

1. [ ] **It authenticated.** The bot joined chat and responds to a known static command.
       Failure here means `tokens.json` did not come across, or came across into the wrong
       directory.
2. [ ] **All three ports answer** — 4001, 4002, 4003. 4002 only if YouTube keys are set;
       an absent 4002 with keys unset is correct, not a failure.
3. [ ] **OBS overlays render.** The browser sources were never re-pointed, so a black
       overlay means the asset directories did not move with the crate.
4. [ ] **The song queue survived.** `!queue` shows what it showed before the stop.
5. [ ] **Hand-curated data survived** — a `!command` that only exists in `commands.json`,
       and one user's entrance theme.
6. [ ] **The state files are being written in the NEW directory.** Check that
       `bot\tokens.json` has a newer timestamp than the copy at the root after the first
       token refresh. **This is the check that proves the cutover actually took** — the bot
       will run happily on the old directory if the WorkingDirectory change did not apply,
       and every check above would still pass.

Then, and only then:

```powershell
.\maintenance-flag.ps1 -Target Bot -Clear
.\maintenance-flag.ps1 -Target Bot -Status
```

- [ ] `-Status` reports not suppressed. **The watchdog is unprotected until this runs.**

---

## ROLLBACK

Available at every step, because nothing was moved and nothing was deleted.

1. `maintenance-flag.ps1 -Target Bot -Set -Reason "cutover rollback"` (if already cleared)
2. `Stop-ScheduledTask -TaskName TwitchBotRS`
3. `Set-ScheduledTask` with the **recorded original WorkingDirectory** from step 4
4. `Start-ScheduledTask -TaskName TwitchBotRS`
5. Verify against the same step-5 list
6. `maintenance-flag.ps1 -Target Bot -Clear`

The old directory is still a complete, consistent bot installation, so rollback is
re-pointing the task and starting it. **Nothing needs to be copied back** — any state the
new bot wrote lives in `bot\` and is simply abandoned, which for one interrupted session
means at most a few queued songs.

---

## AFTER — DO NOT CLEAN UP THE SAME DAY

- [ ] **Leave the old files at the deployment root untouched for one full stream.** They
      are the rollback, and a token refresh is the event that proves the new location is
      genuinely live. Delete them only after that.
- [ ] Register `bot\backup-bot-data.ps1` as a scheduled task, on the pattern
      `docs/ops_backup_and_watchdog.md` documents for the game's. Until that exists the
      bot's state is backed up only when someone remembers, which is how it came to have
      no backup at all.
- [ ] **The log sink is still unpruned.** `tracing_appender::rolling::daily` has no
      retention policy and `logs/` has reached several GB once already. The mitigation was
      expected to arrive "at the Linux move where journald owns rotation" — the game moved
      and the bot did not, so it never arrived. A `bot\logs\` directory is now somewhere a
      rotation policy can finally point. Not fixed here; carried forward deliberately.
