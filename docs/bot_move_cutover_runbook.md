# Cutover runbook — moving the bot into `bot/`

**Written 2026-09-08 on `feature/bot-into-subdirectory`. REVISED 2026-09-11. STILL NOT RUN.**
This stops the live bot mid-stream if run at the wrong time, so **the owner picks the
window.** Nothing in this file is executed by the session that wrote it.

> **What the 2026-09-11 revision changed, and why there is one.** The runbook was written
> as the companion to a code change, on the assumption that the deploy would follow
> promptly. It did not: when the cutover was finally packaged for the owner, the
> deployment checkout was at `1465e45` — 207 commits behind — `C:\PathofDust\bot` did not
> exist, and the bot binary was dated Sep 2. **So the cutover is also the bot's first
> binary deploy in nine days, carrying items 7a, 7b, 7c and 7d.** Every amendment below
> is marked inline with its date and keeps the superseded text visible:
>
> | | |
> |---|---|
> | PRECONDITIONS | "assumes `bot\` exists" was false — now **STEP 0b**, with a go/no-go test |
> | STEP 0 | backup command now names `-SourceDir` and `-BackupRoot` explicitly |
> | STEP 3 | **16 files, not 13** — item 7d adds three |
> | STEP 5 | a check that `BotDataBackup` was re-pointed, plus what proves the new binary is running |
> | AFTER | "register `BotDataBackup`" → **re-point it**, and it moves into the window |
> | AFTER | "the log sink is still unpruned" **retired** — items 7 and 7b both ship here |
> | AFTER | delete the root copy of `backup-bot-data.ps1`, after one full stream |
>
> The owner-facing read of the same window is
> `C:\dust-work\reports\2026-09-11-bot-cutover-package.md`; this file is the checklist.

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
| `watchdog.ps1` | **STAYS AT THE REPOSITORY ROOT. Do not move it, and do not set `$ExpectedPathRoot`.** It defaults to `$PSScriptRoot` and is compared against the listening process's **image path** — still `target\release\twitch-bot-rs.exe`, a sibling of the script, because cargo puts every workspace member's binary in one shared `target\`. Move the script into `bot\` and `$ExpectedPathRoot` becomes `…\bot`, the live bot's own exe stops testing as "under my root", and **the watchdog reads a perfectly healthy bot as foreign** — it would stop restarting the thing it exists to restart, silently. The script's own comment (`watchdog.ps1:139-154`) is the authority for this and for the working-directory decision opposite. |
| `maintenance-flag.ps1 -Target Bot` | Resolves the authoritative root from the **`TwitchBotRS-Watchdog`** task's `-File` argument (`maintenance-flag.ps1:146,168`), which points at `watchdog.ps1` at the repository root. That path does not move, so the flag still lands where the running watchdog looks. |
| the bot's three ports | 4001 alerts / 4002 song / 4003 chat overlay, unchanged. **OBS browser sources need no edit.** |
| the game | Untouched. Its crate, its assets, its service, its data root and its watchdog are all where they were. |

---

## PRECONDITIONS

> **AMENDED 2026-09-11.** The original text read *"This runbook assumes `bot\` exists in
> the deployment."* **It did not, and on the day this was checked it still did not** —
> `C:\PathofDust\bot` was absent, the deployment checkout was at `1465e45` (207 commits
> behind master), and `target\release\twitch-bot-rs.exe` was dated Sep 2. So this is not
> only a directory move: **it is also the bot's first binary deploy in nine days**,
> carrying items 7a, 7b, 7c and 7d at once. The precondition is now a step with a
> go/no-go test (STEP 2) rather than an assumption.

- [ ] The branch is merged and deployed by session c (it holds merge and deploy
      authority), and **STEP 2 below has confirmed both halves actually landed on the
      box** — `bot\` with its assets, and a binary whose hash is not the one that was
      running before. Do not take "the release went out" for either.
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
.\backup-bot-data.ps1 -SourceDir "C:\PathofDust" -BackupRoot "C:\pod-backups\bot" -IncludeEnv -Verbose
```

Note it runs from wherever the bot's state currently is — **pre-cutover that is the
deployment root, so `backup-bot-data.ps1` is invoked from there, not from `bot\`.** After
the cutover it lives in `bot\` and needs neither argument.

> **AMENDED 2026-09-11.** Both arguments are now given explicitly. `-SourceDir` because
> the root copy of the script and the data happen to sit together today and the default
> would be right by coincidence rather than by intent; `-BackupRoot` because the default
> resolves to `C:\PathofDust\bot-backups` — **inside the checkout** — and the registered
> `BotDataBackup` task already uses `C:\pod-backups\bot`, outside it. A pre-cutover
> backup that lands somewhere else than every other snapshot is a backup nobody finds.

- [ ] verdict reads `clean`
- [ ] `filesCopied` matches what the run reports absent — every file NOT listed as absent
      must have been copied
- [ ] the snapshot directory exists under `C:\pod-backups\bot\`

**Do not proceed on a `degraded` verdict.** Degraded means a source did not parse, and
the run deliberately exits 1 and prunes nothing. Find out why first; a cutover is the
worst moment to discover a file was already corrupt.

---

## STEP 0b — BRING THE DEPLOYMENT TO THE HEAD, AND PROVE IT ARRIVED

*(Added 2026-09-11. Numbered 0b rather than renumbering everything below it: the
sequence is backup → deploy → suppress → stop → copy → re-point → verify, and the
existing step numbers are referenced from elsewhere.)*

c merges and deploys per REFACTOR_PLAN §13. **Nothing below this line may run until
both halves are confirmed on the box.**

### The go/no-go test — derive the head, do not trust a remembered number

```powershell
cd C:\dust-work\d
git fetch origin
git rev-parse origin/master                                    # the head to deploy
git log --oneline <deployed-commit>..origin/master -- bot/ Cargo.lock
```

That second command is REFACTOR_PLAN §13's own conditional-bot-redeploy test: if any
changed path falls under `bot/**` or root `Cargo.lock`, **the bot deploys this release.**
`<deployed-commit>` is whatever `git -C C:\PathofDust rev-parse HEAD` reports — it was
`1465e45` when this was written, and quoting that number rather than re-deriving it is
how this section goes stale.

- [ ] the command prints at least one commit. **If it prints nothing, STOP** — the
      premise is wrong, not the bot.

### CLEAR THE UNTRACKED SPRITES BEFORE THE PULL — `git pull` refuses otherwise

*(Added 2026-09-12, after c hit exactly this on the box.)*

The deployment at `1465e45` held untracked sprite files at paths master tracks, so
`git pull` aborted rather than overwrite them. They were byte-identical to master's
copies, but the pull cannot know that and will not guess. **Step 2's pull will hit the
same wall**, and the temptation in a live window — with the bot stopped and the clock
running — is to `git clean` them away.

> **Count the files, not the `git status` lines.** This was first reported as *five*
> untracked sprites, because plain `git status` collapses an untracked directory into a
> single entry. Running the detection below on the box on 2026-09-12 found **55**: the
> five `sprites/custom/*.gif`, plus fifty `sprites/basicenemy/*.png` hiding behind one
> `?? public_adventure_overlay/sprites/basicenemy/` line. That is why the command uses
> `--untracked-files=all`, and why the number is not written into this step — **derive
> it on the day.**

**Do not delete. Move.** `game/src/adventure/stores.rs:500` is explicit about why:

> *"restoring this tree from a checkout LOSES sprites, which has already happened once"*

— and names the overlay tree as *"irreplaceable operator data: the custom sprite
drop-ins, of which 5 of 14 exist only on the box"*. A sprite that exists only here and
is deleted during a pull is gone; a sprite that is moved is recoverable in one command.

```powershell
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$hold  = "C:\dust-work\sprites-pre-pull-$stamp"
New-Item -ItemType Directory -Force $hold | Out-Null

cd C:\PathofDust
# Every untracked file under the overlay tree that collides with a path master tracks.
$untracked = git status --porcelain --untracked-files=all public_adventure_overlay |
             Where-Object { $_.StartsWith('?? ') } |
             ForEach-Object { $_.Substring(3).Trim('"') }

$differs = @()
foreach ($p in $untracked) {
    # Only files master TRACKS can block a pull; anything else is left alone.
    git cat-file -e "origin/master:$p" 2>$null
    if ($LASTEXITCODE -ne 0) { continue }

    # Compare git's OWN object ids, not file hashes. These are sprites -
    # binary - and piping `git cat-file blob` through PowerShell would
    # mangle it into text before anything could hash it. `hash-object`
    # hashes the working file under the same rules git would store it by,
    # so this is exactly the comparison that decides whether the pull
    # would destroy anything.
    $theirs = (git rev-parse "origin/master:$p").Trim()
    $ours   = (git hash-object -- $p).Trim()
    if ($ours -ne $theirs) { $differs += $p; continue }

    # Mirror the RELATIVE PATH under the hold directory, never just the
    # leaf. These files span more than one subdirectory (custom/ and
    # basicenemy/), and flattening them would lose which tree each came
    # from - making the post-pull check below meaningless and any manual
    # restore a guess.
    $dest = Join-Path $hold $p.Replace('/', '\')
    New-Item -ItemType Directory -Force (Split-Path $dest -Parent) | Out-Null
    Move-Item -LiteralPath $p -Destination $dest
    "moved  $p"
}
"$($untracked.Count) untracked, $($differs.Count) differing"
if ($differs) { $differs | ForEach-Object { "DIFFERS  $_" } }
```

- [ ] **If anything printed `DIFFERS`, STOP and report.** A colliding file whose
      contents are not master's is a sprite that exists only on the box under a name
      master also uses. Moving it is still right, but deciding which copy wins is the
      owner's, and it is not a decision to make against a stopped bot.
- [ ] every other collision now lives in `C:\dust-work\sprites-pre-pull-<stamp>\`, and
      `git status` shows the overlay tree clean of them

Now the pull runs. **Afterwards, prove the tracked copies are what you moved aside:**

```powershell
cd C:\PathofDust
Get-ChildItem $hold -File -Recurse | ForEach-Object {
    # The relative path is what was preserved on the way in, so it is
    # what identifies the file on the way back out.
    $rel  = $_.FullName.Substring($hold.Length + 1)
    $live = Join-Path 'C:\PathofDust' $rel
    $verdict =
        if (-not (Test-Path -LiteralPath $live)) { 'MISSING' }
        elseif ((Get-FileHash -LiteralPath $live -Algorithm SHA256).Hash -eq
                (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash) { 'match' }
        else { 'MISMATCH' }
    "{0,-60} {1}" -f $rel, $verdict
}
```

- [ ] every line reads `match` — `MISSING` means the pull did not restore that path and the hold directory is now the only copy; `MISMATCH` means master's version differs from what was there
- [ ] **keep the hold directory until the cutover is signed off**, for the same reason
      the old state files are kept: it is the only copy of anything that turns out not
      to have come back

### Then confirm it actually landed

```powershell
Test-Path C:\PathofDust\bot\public_song_overlay\overlay.html
(Get-Item C:\PathofDust\target\release\twitch-bot-rs.exe).LastWriteTime
(Get-FileHash C:\PathofDust\target\release\twitch-bot-rs.exe -Algorithm SHA256).Hash
```

- [ ] `bot\` exists in the deployment **with its assets in it** — the overlay paths are
      CWD-relative (`bot/src/main.rs:437,450`), so a missing asset tree is a black
      overlay after the switch, not a build error before it
- [ ] the binary's hash **differs from the one that was running**, and its timestamp is
      from this deploy

**If the hash is unchanged, STOP.** Going further would move the working directory
without shipping the bot changes — the worst of both. Note that a hash difference alone
is not proof of a *behaviour* change (§13: Rust release builds are not byte-reproducible,
"the diff is authoritative, not the hash"); an *unchanged* hash, though, is proof the
binary did not move.

> **History worth knowing here.** Items 7a and 7b were each journaled as *"deploy refused
> as byte-identical"*, and the bot binary consequently did not move for either, though
> both changed `bot/**`. Whether that was the rule being misapplied or a case the wording
> does not cover belongs to the deploy session; this step exists so the question is
> answered by a hash on the day rather than assumed either way.

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
  'playrandom-state.json','playrandom-log.json','playrandom-history.json',
  'playrandom-blocklist.json'
)
foreach ($f in $files) { if (Test-Path $f) { Copy-Item $f -Destination "bot\$f" } }
```

**All 16, not the 11 the backup keeps.** The backup deliberately excludes
`search-cache.json` and `daily-greeted.json` as regenerable — but "regenerable" is an
argument about what is worth *retaining*, not about what is worth *carrying across a
move*. Leaving them behind costs a day's YouTube quota and a round of duplicate
greetings for no gain.

**Sixteen, not the thirteen this list carried until 2026-09-11.** Item 7d adds
`playrandom-log.json` (the random play log), `playrandom-history.json` (the 100-play
no-repeat window) and `playrandom-blocklist.json` (random videos the overlay could not
play, with the reason). **None of the three exists before the new binary's first run**,
so the `Test-Path` guard skips them on the cutover this was written for. They are listed
anyway, because a rollback taken *after* they exist — or any later move — would
otherwise leave them behind silently: losing the history makes `!playrandom` start
repeating songs it just played, and losing the blocklist means paying again, one dead
slot each, for every unplayable video already discovered.

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

7. [ ] **`BotDataBackup` is re-pointed and has produced one clean snapshot from the new
       directory.** The commands and their rollback are under AFTER → *`BotDataBackup` —
       RE-POINT it*, and despite living in that section this belongs **here, inside the
       window**: left pointing at the old directory the task runs green while backing up
       a directory the bot no longer writes to.

### Also verify what this deploy ships

*(Added 2026-09-11.)* Because this is the bot's first binary deploy in nine days, it
carries items **7a, 7b, 7c and 7d** as well as the move. Checks 1–7 above would all pass
on the OLD binary running from the NEW directory, so these are what distinguish "the
cutover took" from "the new bot is running".

- [ ] **One `!sr`** — queues and plays. Exercises YouTube resolve end to end, including
      7d's new pre-check on the same `videos.list` call.
- [ ] **One `!voteskip` on a random song** (requires `!playrandom` on, with a random song
      playing). It must skip on that **single** vote and reply *"Skipped — nobody
      requested that one, so one vote is enough"*. **This is the cheapest proof the new
      binary is the one running**: the old one requires the configured threshold and
      replies differently.
- [ ] **The entrance-loop case, against the live overlay port.** With an entrance theme
      set to a short clip (xDaido's `EdvgC3C4Zgc`, 11.0s, is the original report), trigger
      it: the clip plays **once**, the interrupted song resumes at its position, and
      `bot.log` shows **no** `never got an insertEnded report` warning. If an insert ever
      does wedge, `!modskip` must now break it even past `duration + 30` — that is 7a, and
      it is the half that was silently disarmed before. Session d built the harness for
      this case and can run it on request, after these steps and **before** the
      maintenance flag is cleared.

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
- [ ] **Then delete the root copy of `backup-bot-data.ps1`.** `C:\PathofDust\backup-bot-data.ps1`
      is a hand-copied duplicate of a version-controlled file, placed there on 2026-09-11
      so the data had an hourly backup before the cutover existed. It drifts: it was
      refreshed by hand twice in its first day. After the cutover the real one arrives
      with the deploy at `C:\PathofDust\bot\backup-bot-data.ps1`. Delete the root copy
      **only once the re-pointed task has produced one clean snapshot** (below), never
      before — until then it is the tool the rollback depends on.
- [ ] Then delete the copied-from state files at the root.

### `BotDataBackup` — RE-POINT it, inside the window

> **AMENDED 2026-09-11.** This entry used to read *"Register `bot\backup-bot-data.ps1` as
> a scheduled task."* **It is already registered** — created 2026-09-11 against the
> pre-cutover layout, with `-SourceDir "C:\PathofDust"` and the script at the deployment
> root, because `bot\` did not exist yet and the data was unprotected in the meantime.

**This is not an after-step. Do it inside the window, before clearing the maintenance
flag.** Left pointing at the old directory the task keeps running **green while backing
up a directory the bot has stopped writing to** — a backup that reports success while
protecting nothing is the worst shape a backup failure takes, because nobody goes looking.

```powershell
Set-ScheduledTask -TaskName BotDataBackup -Action (New-ScheduledTaskAction -Execute 'powershell.exe' -Argument @'
-NoProfile -ExecutionPolicy Bypass -File "C:\PathofDust\bot\backup-bot-data.ps1" -BackupRoot "C:\pod-backups\bot"
'@)
Start-ScheduledTask -TaskName BotDataBackup
```

`-SourceDir` **drops out**: the script sits beside its data again, which is the property
it was designed around (see its own header — "if the two are ever separated, `-SourceDir`
is the one argument to set").

- [ ] `Get-ScheduledTaskInfo -TaskName BotDataBackup` reports `LastTaskResult` 0
- [ ] the newest snapshot's `_backup-manifest.json` shows `"sourceDir": "C:\\PathofDust\\bot"`

**Rollback, one command:**

```powershell
Set-ScheduledTask -TaskName BotDataBackup -Action (New-ScheduledTaskAction -Execute 'powershell.exe' -Argument @'
-NoProfile -ExecutionPolicy Bypass -File "C:\PathofDust\backup-bot-data.ps1" -SourceDir "C:\PathofDust" -BackupRoot "C:\pod-backups\bot"
'@)
```

### The log sink is no longer unpruned

> **RETIRED 2026-09-11.** This section used to end: *"The log sink is still unpruned…
> the game moved and the bot did not, so it never arrived. Not fixed here; carried
> forward deliberately."* **That is no longer true, and both halves of the fix ship in
> this same deploy.**
>
> - **Item 7** (log retention) bounds the **number** of files — merged as
>   `a1f7389` "Port release 26's log retention fix to the bot".
> - **Item 7b** (reconnect storm) bounds their **size**. Retention counts files, so one
>   runaway minute defeats it: on 2026-09-04 `bot.log` reached 58 MB with 232,184 of its
>   lines inside the single minute 07:38. 7b caps the two `twitch_irc` targets that wrote
>   them and stops the spin that produced them.
>
> The original text is kept above rather than deleted, per CLAUDE.md's append-only rule:
> a future reader chasing the old claim must land on what was actually written, and on
> the date it stopped being true.
