# Scheduled backups and per-deployment watchdog detection

Two operational defects found during second-deployment scoping
(2026-08-23). Both were live problems on the single-instance setup, not
future ones. Branch `fix/ops-backup-and-watchdog`.

This document is the operator-facing half: what the two scripts cover,
the retention scheme and why it is shaped that way, the scheduled-task
definitions needed, and how to verify each against the live
single-instance setup **before** any second deployment exists.

Neither script registers, alters, or deletes a scheduled task. Neither
terminates a process by any means — see "Process safety" below.

---

## 1. `backup-game-data.ps1`

### The defect

Nothing backed up the game's persisted state on a schedule. The only two
mechanisms that existed were incidental:

1. Migration-time `.pre-*-backup` copies, written by whichever one-time
   migration happened to run.
2. Deploy-time `backup-pre-<name>/` directories, written by hand as
   REFACTOR_PLAN.md §13 step 4.

Both are tied to a **deploy**. A world not being deployed to — exactly
what a frozen legacy world is — receives no backups at all. The August
2026 UTF-8 BOM incident wiped all 60 characters, and the only reason
they came back is that a deploy had happened to make a copy.

### What it covers

The file list is **derived from the code**, not guessed. Every entry
carries its source in a comment. Cross-check on 2026-08-23: the
code-derived marker list is 23 files (11 from `manager.rs`/`main.rs`/
`fight_storage.rs`, 8 from `ITEM_MIGRATIONS`, 4 from
`CHARACTER_MIGRATIONS`) and the glob `adventure-*-marker.json` found
exactly 23 on disk — the two agree with no residue.

| Group | Contents | Why |
|---|---|---|
| Core state | `adventure-characters.json`, `-world`, `-reforge-cooldown`, `-rampage-state`, `-sessions`, `-sprite-count`, `-live-tunables.toml`, `-passive-overrides.toml`, `-item-balance.toml`, `patch-notes.json`, `bot-published-constants.json`, `adventure-last-fights.json` | Player progress, operator tuning, logins |
| Markers (23) | every `adventure-*-marker.json` | A missing marker re-runs its giveaway or migration against **live** data on next start. Restoring `characters.json` without these re-applies every migration to already-migrated items |
| Sequence counters (4) | `adventure-fights-*-seq.json` | A restored archive whose counter moved on overwrites existing files |
| Small dirs | `adventure-fights-summary/` (200 files, 2.6 MB), `public_adventure_overlay/sprites/custom/` (14 files, 4.3 MB) | Summary is the tier that serves player-facing history (§13 already pins it per-deploy); custom sprites are player uploads that exist nowhere else |

Note that `adventure-sessions.json`, `patch-notes.json`,
`bot-published-constants.json` and `adventure-wings-giveaway-marker.json`
are **not** `data_path`-wrapped in the game (see
`game/src/adventure/paths.rs`, which says so deliberately) — they resolve
against the process CWD. The backup treats them the same as the rest
because `-SourceDir` *is* the deployment's working directory.

**Measured snapshot size on the live world, 2026-08-23: 10.18 MB / 252
files.**

### What it deliberately excludes

`adventure-fights-coarse/` (333 MB), `-detail/` (1,060 MB) and
`-bundle/` (1,072 MB) — 2,465 MB measured live. At the default retention
that would be roughly **130 GB of snapshots against 165 GB free**.
`-IncludeFightArchives` opts in.

`adventure-fights-pinned/` is in the opt-in group too, because a pinned
file is a full-size coarse/detail copy. It is also the one directory the
game **never** prunes (`fight_storage.rs:261`), so if it is non-empty and
being skipped the script says so on every run rather than letting it look
covered. It was empty on 2026-08-23.

### Retention scheme

**Hourly for 24 hours, then the earliest snapshot of each calendar day
for 30 days.**

Both numbers are parameters (`-HourlyRetentionHours`,
`-DailyRetentionDays`), because a frozen legacy world may well want a
longer daily tail than an actively-developed one.

Steady state at the measured 10.18 MB: 24 hourly + 30 daily = 54
snapshots ≈ **550 MB per deployment**.

Three decisions worth the reasoning:

**Why hourly for 24h.** The BOM incident is the design case: a silent
corruption of live save data, noticed within a session. Hourly means at
most one hour of player progress is lost when it is caught same-day.
Finer than hourly buys little (the game's own writes are the unit of
loss); coarser leaves a whole stream segment at risk.

**Why the earliest snapshot of each day, not the latest.** This is the
part that actually matters and it is counter-intuitive. The daily tier
only ever gets consulted *because damage went unnoticed for days*. The
earliest snapshot of a day is the one with the most of that day still
ahead of it — the most pre-damage state available for that date. Keeping
the latest would hand back 23:xx, which for a corruption that happened at
14:00 is a backup **of the corruption**. Within a day the script prefers
the earliest *verified* snapshot, falling back to the earliest of any
kind rather than dropping the day entirely.

**Why 30 days.** Long enough to survive a slow-burn corruption that only
manifests when a specific player next logs in, and to span a full patch
cycle so a "this broke somewhere in the last release" question has
material to diff against. At ~10 MB a snapshot the marginal cost of the
daily tier is ~300 MB, which is not worth optimising.

### Safety against a live game process

- **Copies only.** Never moves, never renames, never writes anything into
  `-SourceDir`. There is no code path that modifies the source.
- **Cannot lock the game out.** Every source is opened with
  `FileShare.ReadWrite | FileShare.Delete`, so the game's writes and its
  own pruning proceed exactly as if the script were not running. (A plain
  `Copy-Item` asks only for `FILE_SHARE_READ`.)
- **Verifies before pruning.** The real hazard is that the game persists
  with `std::fs::write`, which truncates and then writes. A copy taken
  inside that window is a truncated file that is perfectly valid on disk
  and useless as a backup. Every copy is therefore parsed; a zero-length
  result is a hard failure; a failed verify retries the copy
  (`-CopyRetries`, default 3) rather than accepting it.
- **A degraded run never prunes.** If any file fails verification after
  its retries, the snapshot is marked `degraded`, older snapshots are left
  completely untouched, and the script exits 1. A degraded run is exactly
  when an incident may be under way, which is the worst possible moment to
  be deleting older copies.
- **Pruning refuses to leave zero verified snapshots**, as a backstop.

### Verification limits — stated plainly

- **JSON** is a real parse (`ConvertFrom-Json`). Measured at 391 ms for
  the 3.3 MB `adventure-characters.json`.
- **TOML is an integrity check, not a parse.** Windows PowerShell 5.1 has
  no TOML parser and the script takes no dependencies. It checks
  non-empty, no NUL bytes, strict UTF-8 decode, and bracket balance.
  Bracket balance is the one structural invariant worth having because it
  targets the truncation hazard directly — the game writes these files
  with multi-line arrays, so a torn copy almost always ends inside an
  unclosed array.
- **Sprites and archived fight files are not content-verified.** Sprites
  are binary; fight files run to 620 MB and parsing them would dominate
  the run. Copy success is the check, and the manifest records that
  rather than implying a parse happened.

Two earlier, stricter TOML checks were **wrong on the live data and the
dry run caught both**. First: requiring at least one uncommented
assignment failed `adventure-item-balance.toml`, which ships entirely
commented out — that is its legitimate default state. Second: requiring
every line to be a comment, a `[table]` header, or a `key = value`
assignment failed the two files that use multi-line arrays. Do not
tighten this again without running `-DryRun` against all three live
TOMLs.

### Manifest drift

The marker list was derived from the code on 2026-08-23. A future release
adding a marker would silently fall out of the backup set, so every run
compares the list against the `adventure-*-marker.json` glob and reports
anything unknown. **Unknown markers are backed up anyway** — the drift
report exists to get the list updated, not to skip a file. This is the
same durable lesson CLAUDE.md records for form POSTs: derive the set from
reality rather than hand-maintaining it and hoping.

### Scheduled task — definition only, NOT registered

The script does not create this. Run it by hand when you want it.

```powershell
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument @'
-NoProfile -ExecutionPolicy Bypass -File "C:\PathofDust\backup-game-data.ps1" -SourceDir "C:\PathofDust"
'@
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).Date `
    -RepetitionInterval (New-TimeSpan -Hours 1)
$principal = New-ScheduledTaskPrincipal -UserId 'Administrator' -LogonType S4U -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 30) -StartWhenAvailable
Register-ScheduledTask -TaskName 'GameDataBackup' -Action $action -Trigger $trigger `
    -Principal $principal -Settings $settings
```

- Principal mirrors the existing `GameProcess` / `TwitchBotRS` tasks
  (`Administrator`, `S4U`, `RunLevel Limited`). The backup needs no
  elevation — it only reads files that user already owns.
- `-MultipleInstances IgnoreNew` matters: a slow run must never overlap
  the next hour's.
- Omitting `-RepetitionDuration` gives indefinite repetition on current
  Windows; if your build defaults it to one day, pass
  `-RepetitionDuration ([TimeSpan]::MaxValue)`.
- Exit code 1 means a degraded run — it will show as `LastTaskResult=1`.
- Default `-BackupRoot` is `<parent of SourceDir>\pod-backups\<leaf>`,
  i.e. `C:\pod-backups\PathofDust`. The script refuses a backup root
  inside the source directory.

**Second deployment:** same command with `-SourceDir "C:\PathofDust2"`
and `-TaskName 'GameDataBackup-World2'`. Nothing in the script is
specific to `C:\PathofDust`.

---

## 2. `game-watchdog.ps1`

### The defect

`Get-Process -Name "game"` was the entire liveness test. With two
`game.exe` instances that returns non-empty whenever **either** is alive,
so the watchdog silently stops detecting the death of **either** one.
Standing up a second world would have un-protected the live one with no
visible signal.

Demonstrated on 2026-08-23 with the single live instance running:
`Get-Process -Name 'game'` returns 1 process, and would do so no matter
which deployment were asking.

### The change

Liveness is now **"is anything LISTENING on my port"** — per-deployment
by construction, no image-name matching, and the same port→PID resolution
CLAUDE.md's PRODUCTION SAFETY rule already requires before stopping
anything.

This is a **detection change only**. The restart action, the log line's
exact text and timestamp format, and the fact that a healthy run logs
nothing are all preserved.

**Premise correction:** the order said to preserve "restart logic,
logging, backoff". There was no backoff in the previous version — the
whole script was 10 lines and the only pacing is the task's own `PT2M`
repetition interval, which lives in the task definition, not the script.
Nothing was removed; there was nothing there.

### States

| State | Meaning | Action |
|---|---|---|
| `healthy` | Listener present, image path confirmed under `-ExpectedPathRoot` | nothing, logs nothing (as before) |
| `listening-unverifiable` | Listener present, image path unreadable | nothing by default; `-RequireOwnPath` makes it a logged warning |
| `foreign` | Listener present, image path is **outside** this deployment | log loudly, **do not restart**, **do not touch the other process** |
| `down` | Nothing listening | log + `Start-ScheduledTask` (unchanged) |

`foreign` is a new state that could not exist under name-based detection.
Restarting would only produce a bind failure, and touching the other
process is categorically forbidden.

### The run-level finding

Confirming the listener's image path requires reading another process's
executable path, and **that is not available at the run level these tasks
currently use.** Measured 2026-08-23: `GameProcess` and
`GameProcess-Watchdog` both run as `Administrator` with
`RunLevel = Limited`, and from a non-elevated context
`(Get-Process -Id N).Path`, `.MainModule.FileName` and
`Win32_Process.ExecutablePath` all return **empty** — for `game.exe` and
`twitch-bot-rs.exe` alike.

So the design deliberately does not depend on it: the **restart decision
hinges only on the listener**, which needs no elevation and is the part
that matters, since a restart cannot help while the port is occupied
anyway. The path check is best-effort enrichment.

**Recommendation (operator's call, not applied here):** raise
`GameProcess-Watchdog` to `RunLevel = Highest` and add `-RequireOwnPath`.
That turns the two-deployment port-collision case from "silently assumed
healthy" into a logged warning. Worth doing before a second deployment
exists; not required for the single-instance setup.

> **That recommendation would not have worked as written until 2026-08-23**
> (branch `fix/watchdog-maintenance-gate`). `$ExpectedPathRoot` defaulted
> to `$PSScriptRoot` **in the param block**, where that variable is not
> populated under the `-File` invocation the scheduled task uses — so the
> root arrived empty, `Test-UnderRoot` returned `$null` for every
> candidate, and every listener resolved `unverifiable` no matter where it
> lived. Raising the run level would have produced *false confidence*: the
> path would finally be readable and the comparison would still never
> confirm anything. `$LogPath` was always resolved in the body and always
> worked; both now do. Verified on a listener with a readable path:
> correct root → `verdict=confirmed`, deliberately wrong root →
> `verdict=foreign`; the pre-fix script gave `unverifiable` for both.

### The maintenance gate (2026-08-23)

**The defect.** REFACTOR_PLAN.md §13 step 4 told a deploy session to
disable `GameProcess-Watchdog` before swapping the binary and re-enable
it after. **A deploy session cannot do either** — `Disable-ScheduledTask`
requires an elevated token and returns `Access denied` without one. The
step was therefore silently skipped on every deploy up to and including
the 2026-08-23 pacing release, and every binary swap ran with the
watchdog live. Nothing has broken only because swaps finish well inside
the task's ~2 minute repetition interval. That is luck: a slower swap — a
large backup, a retried copy, a stalled disk — races it, and the watchdog
restarts the game off a half-written binary.

**The fix.** A file, which a non-elevated session *can* create.
`maintenance-flag.ps1` writes `watchdog-maintenance.flag` next to the
scripts (gitignored — per-deployment runtime state, exactly like the port
it is scoped by); `game-watchdog.ps1` honours it.

```powershell
C:\PathofDust\maintenance-flag.ps1 -Set -Reason "pacing deploy 0110be6"
C:\PathofDust\maintenance-flag.ps1 -Status
C:\PathofDust\maintenance-flag.ps1 -Clear   # safe when no flag exists
```

**Always the deployment's own copy, by absolute path.** Both scripts
default the flag path off their *own* directory, and those agree only
because both sit in the deployment root. Run the helper from a worktree —
where a deploy session naturally has a shell open — and the flag lands
somewhere the live watchdog never reads, while `-Status` cheerfully
reports `SUPPRESSED`. The swap then runs unprotected under an operator
who believes it is gated: the same false-confidence shape as the
`$ExpectedPathRoot` bug above, relocated.

`-Set` therefore resolves the authoritative root from the scheduled
task's own action (`-File "<path>"`, readable unelevated) and **refuses**
to write anywhere else, naming both directories and the command to run
instead. `-Force` overrides it for a deliberate second-deployment case;
when the task cannot be read at all it warns and proceeds rather than
becoming unusable. `-Status` always ends with a `scope :` line saying
whether the flag it just described is the one that task actually reads —
so a wrong-directory flag cannot look like a working one even under
`-Force`.

**It is a lease, not a switch.** `game-watchdog.ps1` ignores a flag older
than `-MaintenanceMaxAgeMinutes` (default 30) and logs loudly that it
did. A forgotten flag disabling protection forever is a worse failure
than the one being fixed, and a quieter one. Everything ambiguous fails
the same way — toward protecting the world, never toward staying silent:

| Flag state | Watchdog behaviour |
|---|---|
| absent | unchanged; full protection |
| valid and within the age limit | logs the suppression, restarts nothing |
| older than the limit | logs `IGNORING a maintenance flag …`, **then acts normally** |
| unreadable / not JSON | same as expired |
| no `created` timestamp | same as expired |
| dated in the future (>2 min skew) | same as expired |
| `-MaintenanceMaxAgeMinutes 0` | gate disabled; every flag ignored |

The gate is consulted **only on runs that would otherwise act**, so a
healthy run still logs nothing at all whether or not a flag exists — a
deploy window does not fill the log with suppression lines.

The flag records an ISO 8601 timestamp *with its real UTC offset*,
deliberately not the bare-`Z`-on-local-time shape `game-watchdog.log`
uses. That shape is a known-wrong format preserved in the log only for
compatibility with existing greps (see §5 below), and it must not spread
into a value something computes an age from.

**Still not fixed: the bot half.** `watchdog.ps1` /
`TwitchBotRS-Watchdog` has the identical elevation defect and no
maintenance gate — §13's `disable TwitchBotRS-Watchdog` instruction is
equally unperformable from a deploy session. Out of scope for the branch
that fixed the game side; a real follow-up.

### Two guards the port-based check needs

The old name-based check saw a process the instant it was created. A
port-based check does not see it until it has **bound**, and
`AdventureManager::new` loads a 3.3 MB roster and runs any pending
migrations before the servers start. Without guards the new detection
would be strictly more trigger-happy than the old one — a regression in
restart safety rather than an improvement.

- `-RecheckDelaySeconds` (default 5) — one in-run re-check before
  believing the world is down.
- `-StartupGraceSeconds` (default 90) — do not restart a task that only
  just started; logs a distinct line instead.

Both can be set to 0 to disable.

### The existing task needs no change

Defaults are `-Port 4005 -TaskName GameProcess -ExpectedPathRoot
$PSScriptRoot`, which is exactly the live setup. The registered
`GameProcess-Watchdog` action passes no arguments and continues to work
unmodified.

**Second deployment** would register its own:

```
powershell.exe -NoProfile -ExecutionPolicy Bypass ^
  -File "C:\PathofDust2\game-watchdog.ps1" -Port 4015 -TaskName "GameProcess-World2"
```

`-ExpectedPathRoot` defaults to the script's own directory, so a copy
living in the second deployment already checks against the right root.

---

## 3. Process safety

Neither script terminates a process by image name — or by any other
means. Between them they contain no `Stop-Process`, no `taskkill`, no
`Stop-ScheduledTask`, no `Remove-Item` aimed at anything but the
watchdog's own aged snapshot directories.

`backup-game-data.ps1` contains no process code at all.

`game-watchdog.ps1` reads a process's image *name* only to write it into
the log for a human reading an incident later. It never drives a
decision. The restart path calls `Start-ScheduledTask` and nothing else.

---

## 4. Verifying against the live single-instance setup

Everything below was run on 2026-08-23 against the live world (game.exe
PID 5748 on ports 4004/4005) with no production side effects: no
scheduled task created, no file written under `C:\PathofDust`, no
`C:\pod-backups` directory created.

### Backup

```powershell
# 1. Dry run. Verifies the LIVE sources in place, so it doubles as a
#    "is my current data parseable right now" check. Writes nothing.
.\backup-game-data.ps1 -SourceDir C:\PathofDust -DryRun

# 2. Live run into a throwaway root, off the production path.
.\backup-game-data.ps1 -SourceDir C:\PathofDust -BackupRoot D:\scratch\bk `
    -LogPath D:\scratch\bk.log

# 3. Confirm the snapshot is honest.
$snap = Get-ChildItem D:\scratch\bk -Directory | Select-Object -Last 1
(Get-Content (Join-Path $snap.FullName '_backup-manifest.json') -Raw) | ConvertFrom-Json |
    Format-List verdict, filesCopied, filesFailed, bytes, manifestDrift

# 4. Prove a copy is byte-identical to its source.
foreach ($f in 'adventure-world.json','adventure-live-tunables.toml') {
    (Get-FileHash (Join-Path C:\PathofDust $f)).Hash -eq
    (Get-FileHash (Join-Path $snap.FullName $f)).Hash
}
```

Expected on the live world: dry run reports 40 files + 2 directories,
~10.18 MB, no `SOURCE FAILS VERIFY`, no `MANIFEST DRIFT`. Live run
reports `verdict=clean`, `filesCopied=252`, `filesFailed=0`. Hashes match
for anything the game did not rewrite mid-run (`adventure-characters.json`
and `adventure-world.json` legitimately change between fights — that is
expected, not a fault).

**Degraded path**, against a synthetic source so production is never
involved: put a zero-length `adventure-characters.json` in a scratch
directory and point `-SourceDir` at it. Expect
`VERIFY FAILED ... zero-length`, `PRUNE SKIPPED`, exit code 1, and the
existing snapshot count **unchanged**.

**Retention**, against fabricated snapshot directories: create
`pod-backup-<stamp>` directories at chosen ages with a
`{"verdict":"clean"}` manifest, then run `-DryRun` and read the plan.
Verified 2026-08-23 across 11 fabricated snapshots — hourly keeps,
earliest-of-day daily keeps, the verified-over-earliest preference within
a day, the all-unverified fallback, and >30d expiry all behaved as
specified, and the live prune then matched the dry-run plan exactly
(11 → 5 survivors + 1 new).

### Watchdog

All five states were exercised live. Dry run writes nothing and starts
nothing.

```powershell
# Real production case: live game on 4005, path unreadable at Limited run level.
.\game-watchdog.ps1 -DryRun
#   -> listening PIDs : 5748
#      resolved state : listening-unverifiable
#      would do       : nothing

# Same, with the stricter switch.
.\game-watchdog.ps1 -DryRun -RequireOwnPath
#   -> would do       : WOULD LOG an unverifiable-listener warning and NOT restart

# Down.
.\game-watchdog.ps1 -DryRun -Port 4099
#   -> resolved state : down
#      would do       : WOULD LOG + Start-ScheduledTask -TaskName GameProcess
```

To exercise `healthy` and `foreign` you need a listener whose image path
*is* readable. Bind one inside your own PowerShell process — no child
process, nothing to clean up:

```powershell
$l = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 4098); $l.Start()
.\game-watchdog.ps1 -DryRun -Port 4098 -ExpectedPathRoot 'C:\WINDOWS\System32\WindowsPowerShell'
#   -> verdict=confirmed, resolved state : healthy
.\game-watchdog.ps1 -DryRun -Port 4098 -ExpectedPathRoot 'C:\PathofDust'
#   -> verdict=foreign,   would do : WOULD LOG a foreign-listener warning and NOT restart
$l.Stop()
```

**netstat fallback.** The `Get-NetTCPConnection` path is primary; netstat
is a real fallback because the NetTCPIP module is absent from Server Core
and can be missing from a trimmed image, and a watchdog that throws has
stopped watching. Both resolved port 4005 to PID 5748 identically on
2026-08-23.

**The two-deployment property**, verifiable today without a second
deployment: with the live game on 4005 and a decoy listener on 4098,
asking about 4005, 4098 and 4099 yields three independent verdicts in the
same moment. The old script's single answer — `Get-Process -Name 'game'`
— is the same regardless of which deployment asks. That difference *is*
the fix.

---

## 5. Known wart, deliberately not fixed

`game-watchdog.ps1` stamps its log lines with
`Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"` — **local time with a bare
`Z`**. It is wrong; this is not UTC. A UTC-vs-local misreading of a
sibling log has already cost one session real time
(`docs/anomaly_ledger.md`, the `#44` self-correction).

It is preserved byte-for-byte because this change was scoped to detection
only and because anything already grepping `game-watchdog.log` expects
this shape. `backup-game-data.ps1`, being new, uses
`yyyy-MM-ddTHH:mm:sszzz` with a real offset.

**Recommended follow-up:** move `game-watchdog.ps1` and `watchdog.ps1` to
the offset format together, in one change, so both logs shift at the same
moment rather than leaving a mixed-format window. Not done here.
