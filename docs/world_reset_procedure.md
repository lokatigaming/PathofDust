# World Reset Procedure — Path of Dust

**Status:** Executable procedure. Written 2026-09-02 for the World 1 → World 2
reset. Rehearsed on a shadow copy before first production use (see §9).

**There is no reset code path in this codebase.** No route, no CLI flag, no
function. `grep -rniE "world_reset|reset_world|new_season"` over `game/src/`
returns nothing but test names and `/passives/reset` (a passive-tree preview
discard, unrelated). A reset is a human stopping the service and deleting
files in a specific order. This document is that order.

Three of its steps silently corrupt data if done wrong. Each is marked
**TRAP** at the point where it bites.

---

## 1. What a reset is, and is not

A reset ends a season. It wipes progression and identity, keeps authored
content, and returns every tuning dial to its compiled default so the new
season starts from the shipped balance rather than from the previous season's
end state.

It is **not** a deploy. No binary changes. No code is deleted. Twitch is
turned off by environment variable, not by removal — the ~46 code-removal
targets in `docs/external_integration_removal_scope.md` ship later as
ordinary cleanup.

---

## 2. Preconditions

Every one of these must hold before step 1 of §5.

| # | Precondition | How to check |
|---|---|---|
| P1 | You are on the Linux production box, as root | `hostname; id -u` |
| P2 | A verified backup exists, taken today, after the last fight you care about | §4 |
| P3 | Disk free is at least 3× the size of `/var/lib/pathofdust` | `df -h /var; du -sh /var/lib/pathofdust` |
| P4 | You know the operator login you will bootstrap, and it is NOT `lokati_gaming` | §7 |
| P5 | `/etc/pathofdust/production.env` has been copied somewhere safe | §8.1 — without it, rollback restores the world but not the seam |
| P6 | The owner has announced the reset to players | Owner's job, not the operator's |

---

## 3. The complete state inventory

Everything in `/var/lib/pathofdust` (the systemd `WorkingDirectory`), with
what the code does when the file is absent, and what the reset does to it.

**Load behaviour matters and is not uniform.** Two different loaders are in
use and they behave oppositely on a damaged file:

- **`load_json_fail_loud`** (`game/src/state.rs:48-77`) — absent returns
  `None` (caller defaults); **present-but-unparseable PANICS and refuses to
  start**, rather than overwriting good data with an empty default. Used for
  characters, world, reforge cooldowns, rampage state.
- **`load_json`** — absent *and* unparseable both return `None` silently.
  Used for sessions, accounts, every sequence counter, every marker, patch
  notes, sprite count, published constants.
- **TOML files** (`tunables.rs:621`, `passive_overrides.rs:147`) — absent or
  malformed both fall back to compiled defaults, logged as a warning, never a
  boot failure.

The practical consequence: **a half-deleted or truncated
`adventure-characters.json` stops the game dead; a half-deleted
`adventure-accounts.json` silently locks every player out with no error.**
The first failure mode is loud and safe. The second is silent, and is the
reason §5 deletes files rather than editing them.

### 3.1 WIPE — progression and identity

| File | Loader | Absent gives | Why it goes |
|---|---|---|---|
| `adventure-characters.json` | fail-loud | empty roster | The season's progression. 4.6 MB, 67 characters at World 1's end |
| `adventure-world.json` | fail-loud | `stage: 0`, `boss_power_mult: 1.0`, `hp_pacing_mult: 1.0`, empty outcome/DPS windows (`manager.rs:371-375`) | World 1 ended at stage 7359 with `hp_pacing_mult` saturated at 428.2. Meaningless at stage 1 |
| `adventure-accounts.json` | silent | no accounts | Frees every username. Every player re-registers with a password |
| `adventure-sessions.json` | silent | no sessions | `current_session` never re-validates and the TTL is 30 days; leaving these alive leaves stale logins pointing at characters that no longer exist |
| `adventure-reforge-cooldown.json` | fail-loud | no cooldowns | Per-player hourly reforge claims. World 1 state |
| `adventure-rampage-state.json` | fail-loud | `0` | `rampage_remaining`. Its only producer was the chat seam, which is being turned off |
| `adventure-sprite-count.json` | silent | `0` | Triggers the "new sprites available" one-shot on next start. Harmless either way; wiped for cleanliness |

### 3.2 WIPE TOGETHER — fight archives and their counters

**See TRAP 1 in §5 step 5.** These nine entries are one unit.

| Entry | Value at World 1's end |
|---|---|
| `adventure-fights-coarse/` + `adventure-fights-coarse-seq.json` | 5 files, counter 19443 |
| `adventure-fights-detail/` + `adventure-fights-detail-seq.json` | 3 files, counter 19341 |
| `adventure-fights-summary/` + `adventure-fights-summary-seq.json` | 200 files, counter 18937 |
| `adventure-fights-bundle/` + `adventure-fights-bundle-seq.json` | 3 files, counter 14476 |
| `adventure-fights-pinned/` | does not currently exist; created on demand |

### 3.3 RESET TO COMPILED DEFAULTS — the tuning dials

| File | Absent gives | Why it goes |
|---|---|---|
| `adventure-live-tunables.toml` | `LiveTunables::default()` (`tunables.rs:621-633`) | World 1's end-state pacing is meaningless at stage 1. Balance is tuned live through `/admin/tunables` during World 2's early stages |
| `adventure-passive-overrides.toml` | `PassiveOverrides::default()` (`passive_overrides.rs:147-158`) | Every row `/admin/passives` ever saved is pinned, including rows saved unchanged at their compiled defaults. Carried forward, each one silently overrides World 2's defaults and a rebalance appears to do nothing on exactly those nodes |

Both loaders treat *absent* and *malformed* identically — compiled defaults, a
`tracing::warn!`, never a boot failure. Deleting them is safe.

### 3.4 KEEP — and why each one

| Entry | Why it stays |
|---|---|
| **23 `*-marker.json` files** | **TRAP 2, see §5 step 8.** Every one-time grant and data migration is marker-gated. Delete them and the grants re-fire against whoever exists at that moment |
| `adventure-item-balance.toml` | Authored content, not accumulated state. Unchanged since 2026-08-16 |
| `patch-notes.json` | Player-visible release history. Spans worlds |
| `public_adventure_overlay/` | Sprite art, including `sprites/custom/` (14 files; only 9 are in git). Never restorable from a checkout |
| `templates/`, `wiki/` | Authored content. The wiki session owns `wiki/` |
| `bot-published-constants.json` | Becomes permanently stale once the seam is off; the wiki renders `"varies"` without it, which is the documented fallback. Harmless — leave it |
| `logs/` | Journald is the log of record, but `logs/game.log` is written by the binary regardless |

### 3.5 DELETE — inert World 1 residue

Neither is written by any code path in either crate today. Both are
pre-migration leftovers. Deleting them is optional and changes no behaviour;
they are listed so nobody has to wonder later.

| File | Status |
|---|---|
| `adventure-last-fight.json` (164 KB, singular) | No writer. The live constant is `adventure-last-fights.json` (plural, `manager.rs:1544`), read only by the marker-gated storage migration, and that plural file does not exist on the box |
| `announcements.json` (567 B) | No writer in `game/**` or `src/**` |

### 3.6 NOT IN THE STATE DIRECTORY, but part of the reset

| Thing | Action |
|---|---|
| `/etc/pathofdust/production.env` | Remove `ADVENTURE_API_SECRET`, `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET` |
| `/etc/systemd/system/pathofdust.service` (the `Environment=TWITCH_*` lines) | Remove them. **See TRAP 3** |

---

## 4. Step 0 — Back up, and verify the backup

Nothing else in this document is reversible without this step.

First, preserve the environment file. The reset removes three keys from it and
they are unrecoverable from this box afterwards:

```
cp -a /etc/pathofdust/production.env /root/production.env.pre-reset
chmod 0600 /root/production.env.pre-reset
```

Then run the proven backup:

```
systemctl start pathofdust-backup.service
journalctl -u pathofdust-backup.service -n 40 --no-pager
```

The run must end without `FAILURE:` and without `ABORTED`.
`backup-game-data.sh` verifies every JSON and TOML member parses before it
prunes anything, and refuses to prune at all if the run is degraded.

Confirm the archive and record its hash:

```
ls -t /var/backups/pathofdust/pod-backup-*.tar.gz | head -1
sha256sum "$(ls -t /var/backups/pathofdust/pod-backup-*.tar.gz | head -1)"
```

**The default backup EXCLUDES the large fight tiers** —
`adventure-fights-coarse`, `-detail`, `-bundle` and `-pinned` are in
`ARCHIVE_DIRS`, excluded by size. Only `adventure-fights-summary` is captured.
**Rollback therefore restores fight *summaries* but not coarse/detail/bundle
replays.** If those matter for the world being retired, pin them separately
first:

```
mkdir -p /var/backups/pathofdust/pre-reset-fight-tiers
cp -a /var/lib/pathofdust/adventure-fights-coarse \
      /var/lib/pathofdust/adventure-fights-detail \
      /var/lib/pathofdust/adventure-fights-bundle \
      /var/backups/pathofdust/pre-reset-fight-tiers/
```

**Take a second copy off-box.** `PodPullLinuxBackups` on the Windows box is
the only off-box copy of anything. If the Windows box is powered off, say so
explicitly in the reset report and keep a second copy on this box on a
different path instead:

```
cp -a "$(ls -t /var/backups/pathofdust/pod-backup-*.tar.gz | head -1)" /root/
```

Finally, take the whole-directory snapshot that rollback actually uses. This
is faster and more faithful than restoring from the tarball, and unlike the
tarball it includes every fight tier:

```
cp -a /var/lib/pathofdust /var/lib/pod-prereset-$(date +%Y%m%d-%H%M%S)
du -sh /var/lib/pod-prereset-*
```

---

## 5. The reset

Numbered. Do not reorder. Steps 1–2 are reversible with no downtime; the
service does not stop until step 3.

### Step 1 — Turn the `/api/*` seam off

Remove the `ADVENTURE_API_SECRET` line from `/etc/pathofdust/production.env`:

```
sed -i '/^ADVENTURE_API_SECRET=/d' /etc/pathofdust/production.env
grep -c ADVENTURE_API_SECRET /etc/pathofdust/production.env   # must print 0
```

`adventure_web/api.rs:61-62` returns `None` without it and the whole `/api/*`
router is never mounted. No code change. Fully reversible by putting the line
back.

This is the player-facing moment: the 10 chat commands, the 3 channel-point
redemptions and chat activity XP stop working. Per owner ruling, nothing is
rebuilt to replace them.

The bot on the Windows box keeps its own copy of the secret and will begin
failing every call — an SSE reconnect roughly every 7 s, and all 10 commands
replying *"The adventure is restarting — try again in a moment!"*. That is
noise, not damage. Silencing it means removing the key on Windows, which is
out of scope here.

### Step 2 — Turn `/login` off

**TRAP 3 — the Twitch credentials are set in TWO places, and removing them
from only one leaves `/login` mounted.**

`/etc/pathofdust/production.env` holds the real values. The unit file itself
holds staging placeholders (`TWITCH_CLIENT_ID=staging-no-twitch`). The
`EnvironmentFile` supplied by drop-in `10-production.conf` overrides the
unit's `Environment=` lines — so deleting only the `production.env` entries
makes the **placeholders** take effect, `state.twitch` is still `Some`, and
`/login` and `/auth/callback` stay mounted and visibly linked, pointing at a
Twitch app that does not exist. Both must go:

```
sed -i '/^TWITCH_CLIENT_ID=/d;/^TWITCH_CLIENT_SECRET=/d' /etc/pathofdust/production.env
sed -i '/^Environment=TWITCH_CLIENT_ID=/d;/^Environment=TWITCH_CLIENT_SECRET=/d' \
    /etc/systemd/system/pathofdust.service
systemctl daemon-reload
```

Verify neither name survives anywhere:

```
grep -rn TWITCH /etc/pathofdust/production.env /etc/systemd/system/pathofdust.service*
```

While in the unit file, the comment above those lines claims *"TWITCH_CLIENT_ID/
SECRET are REQUIRED: main.rs aborts startup without them."* That has been false
since 2026-08-31 (`game/src/main.rs:122-123` — both are `env_var()`, i.e.
`Option`). Correct or delete the comment so the next operator is not misled.

With both absent, `start_adventure_web_server` logs `TWITCH_CLIENT_ID/
TWITCH_CLIENT_SECRET are unset - the dashboard's Twitch login is off` and
mounts neither route.

### Step 3 — Stop the service

```
systemctl stop pathofdust
systemctl is-active pathofdust    # must print "inactive"
ss -lntp | grep -E ':(4004|4005)' || echo "ports clear"
```

`Restart=always` does not fire on an explicit `systemctl stop` — a stop is not
an exit systemd acts on. There is no watchdog to suppress; systemd replaced
`game-watchdog.ps1` and that script does not run on this box.

**Downtime starts here.**

### Step 4 — Wipe progression and identity

```
cd /var/lib/pathofdust
rm -f adventure-characters.json \
      adventure-world.json \
      adventure-accounts.json \
      adventure-sessions.json \
      adventure-reforge-cooldown.json \
      adventure-rampage-state.json \
      adventure-sprite-count.json
```

**Delete, do not truncate, and do not edit to `{}`.** An absent
`adventure-characters.json` gives an empty roster cleanly; a
present-but-unparseable one panics the process at startup and it will not come
up. An absent `adventure-accounts.json` is correct; a corrupt one silently
loads as empty with no error at all, which looks identical until someone tries
to log in as the operator.

### Step 5 — Wipe the fight archives and their counters, together

**TRAP 1 — the directories and the counters are one unit.**

`next_seq` (`fight_storage.rs:86-93`) reads its counter with `load_json`, so a
missing counter file reads as `0` and the next fight is numbered `1`.

- **Delete the counters but keep the directories** and **every new fight is
  silently deleted the instant it is written.** `write_and_prune`
  (`fight_storage.rs:115-138`) writes the file, lists the directory sorted
  ascending by filename, and when over capacity prunes `files[..len-capacity]`
  — the *lowest-numbered* files. A restarted counter makes every World 2 file
  the lowest-numbered thing in the directory, so it is the first thing pruned:
  written, then removed, in the same call. Nothing errors. `/fights` keeps
  serving World 1 fights forever because `read_recent` also orders by
  filename. This persists until the counter climbs back past the retained
  World 1 numbers — 18,937 fights for the summary tier.

  *(An earlier draft of this document said the new file would overwrite a
  retained World 1 fight of the same number. That is wrong: the retained
  numbers are all high, so there is no collision. The real failure is
  self-pruning, and it is worse — the world appears to run while producing no
  durable fight history at all. Verified by rehearsal, 2026-09-02.)*

- **Delete the directories but keep the counters** and World 2 starts at fight
  19444 forever, with no fight 1.

Both, or neither:

```
rm -rf adventure-fights-coarse adventure-fights-detail \
       adventure-fights-summary adventure-fights-bundle \
       adventure-fights-pinned
rm -f  adventure-fights-coarse-seq.json adventure-fights-detail-seq.json \
       adventure-fights-summary-seq.json adventure-fights-bundle-seq.json
```

Verify nothing survived on either side. **Do not use a bare
`adventure-fights-*` glob** — it also matches
`adventure-fights-storage-migration-marker.json`, which is one of the 23
markers that must be kept, so the check reports failure on a correct reset.
Name the five directories and the counters explicitly:

```
remaining=$(ls -d adventure-fights-coarse adventure-fights-detail \
                  adventure-fights-summary adventure-fights-bundle \
                  adventure-fights-pinned adventure-fights-*-seq.json \
                  2>/dev/null | wc -l)
[ "$remaining" -eq 0 ] && echo "fight state clear" \
                       || { ls -d adventure-fights-coarse adventure-fights-detail \
                                  adventure-fights-summary adventure-fights-bundle \
                                  adventure-fights-pinned adventure-fights-*-seq.json 2>/dev/null
                            echo "STOP: $remaining fight entries remain"; }
```

*(The bare glob was in the first draft and produced a false alarm on the
rehearsal's own correct run. Found 2026-09-02.)*

The directories are recreated on the first fight by `create_dir_all`
(`fight_storage.rs:117`, `:194`).

### Step 6 — Reset the tuning dials

```
rm -f adventure-live-tunables.toml adventure-passive-overrides.toml
```

Do **not** delete `adventure-item-balance.toml` — authored content, not
accumulated state.

### Step 7 — Delete the inert residue (optional)

```
rm -f adventure-last-fight.json announcements.json
```

### Step 8 — Confirm the markers survived

**TRAP 2 — deleting the markers re-fires every one-time grant.**

All 23 `*-marker.json` files gate one-time item migrations
(`migrations.rs:212-219`, `:401-404`), launch grants (pity, wings-launch,
craft-token, Kibukah compensation) and the wings giveaway in
`game/src/main.rs:180-186`. On an empty roster most are no-ops, but the wings
giveaway is `tokio::spawn`ed at startup and the grants would fire against
whoever happens to exist when they run.

```
ls adventure-*marker*.json | wc -l    # must print 23
```

If that prints anything other than 23, restore the missing ones from the
pre-reset snapshot before starting the service.

### Step 9 — Confirm what is left

```
ls -A1 /var/lib/pathofdust
```

Expect exactly: the 23 markers, `adventure-item-balance.toml`,
`patch-notes.json`, `bot-published-constants.json`, `logs/`,
`public_adventure_overlay/`, `templates/`, `wiki/`. Nothing else.

### Step 10 — Start

```
systemctl start pathofdust
systemctl is-active pathofdust
journalctl -u pathofdust -n 40 --no-pager
```

**Downtime ends here.** Expect in the log:

- `loaded 0 characters from adventure-characters.json`
- `TWITCH_CLIENT_ID/TWITCH_CLIENT_SECRET are unset - the dashboard's Twitch login is off`
- no `/api/*` mount, and no panic

---

## 6. Verification

Run every one. Each must pass before the reset is called done.

| # | Check | Command | Expected |
|---|---|---|---|
| V1 | Site is up | `curl -s -o /dev/null -w '%{http_code}\n' --max-time 10 http://localhost:4005/` | `200` |
| V2 | Roster empty | `journalctl -u pathofdust \| grep 'loaded .* characters'` | `loaded 0 characters` |
| V3 | World at defaults | `cat adventure-world.json` after the first save | low `stage`, both multipliers `1.0` |
| V4 | Seam un-mounted | `curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:4005/api/commands/join` | **`404`** (`401` would mean still mounted) |
| V5 | Twitch login gone | `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:4005/login` | `404` |
| V6 | Registration works | register a fresh name at `/account/register` | `302` + `adv_session` cookie |
| V7 | A World 1 name is free | register a name that belonged to a World 1 character | `302`, not `400 "already taken"` |
| V8 | Fight numbering | after the first fight, `ls adventure-fights-summary` | `fight-0000000001.json` |
| V9 | Fight is sane | `journalctl -u pathofdust \| grep -i fight` | a fight resolves; duration in seconds, not minutes |
| V10 | Operator works | §7, then `GET /admin/tunables` with that session | `200`, not `404` |

Use `curl` from the box, not a PowerShell capture from Windows — a PowerShell
capture of a large page blocks for minutes and reads as a hung server
(journal, 2026-09-02).

---

## 7. Operator re-bootstrap

**The operator account does not survive an accounts wipe.** `OPERATOR_LOGIN`
is `lokati`, and `adventure-accounts.json` was just deleted, so the account
the three admin gates point at no longer exists. `username_rejection`
(`accounts.rs:186-193`) refuses to register any name matching
`ADMIN_TUNABLES_LOGIN`, `FIGHTS_PAGE_LOGIN` or `BUNDLE_OPERATOR_LOGIN` — which
is the operator's own login. Without the bootstrap variable the operator
cannot create their own account: the reservation that protects the name also
blocks the only person entitled to it.

`OPERATOR_BOOTSTRAP` carries the **login value**, not a boolean, and must
equal `OPERATOR_LOGIN` exactly (`accounts.rs:157-160`). A variable left set
after `OPERATOR_LOGIN` moves permits nothing at all.

`lokati_gaming` is in `RESERVED_USERNAMES` **permanently** and
`OPERATOR_BOOTSTRAP` cannot release it. Point `OPERATOR_LOGIN` at a different
name.

### The dance — set, register, unset, restart

**1. Set it, matching `OPERATOR_LOGIN` exactly.**

```
mkdir -p /etc/systemd/system/pathofdust.service.d
printf '[Service]\nEnvironment=OPERATOR_BOOTSTRAP=lokati\n' \
    > /etc/systemd/system/pathofdust.service.d/20-bootstrap.conf
systemctl daemon-reload && systemctl restart pathofdust
```

**2. Confirm the process actually has it.**

```
pid=$(systemctl show -p MainPID --value pathofdust)
tr '\0' '\n' < /proc/$pid/environ | grep OPERATOR
```

Both `OPERATOR_LOGIN` and `OPERATOR_BOOTSTRAP` must appear, with the same
value. Nothing is logged until a registration attempt uses it.

**3. Register the operator account** at `/account/register` with that exact
username and a password of at least 8 characters. The log line
`OPERATOR_BOOTSTRAP is set to "lokati" - the operator reservation on that
login is being bypassed for this registration` confirms it took.

Do this immediately. While the variable is set, that one name is registrable
by anyone who reaches the form first. Do not leave the window open
unattended.

**4. Verify the operator gates BEFORE removing the variable.**

```
curl -s -o /dev/null -w '%{http_code}\n' -b "adv_session=<token>" \
     http://localhost:4005/admin/tunables
```

Must be `200`. A `404` is the gate refusing you — fix it now, while the
bootstrap window is still open, not after.

**5. Unset and restart.**

```
rm /etc/systemd/system/pathofdust.service.d/20-bootstrap.conf
systemctl daemon-reload && systemctl restart pathofdust
```

**6. Confirm the window is closed.** Attempt to register the operator name a
second time. Must be refused, `400`, with **`That username is reserved.`** —
not `That username is already taken.` `username_rejection` checks the
operator-gate arm before it checks the accounts map, so with
`OPERATOR_BOOTSTRAP` gone the name is refused as *reserved* regardless of the
account now existing. Either message means the window is shut; only the
reserved one means the variable is really gone.

---

## 8. Rollback — putting World 1 back

Rollback is a directory swap, not a restore. It takes about as long as a `mv`,
and the pre-reset snapshot from §4 is the source.

### 8.1 Procedure

```
systemctl stop pathofdust
mv /var/lib/pathofdust /var/lib/pod-postreset-$(date +%Y%m%d-%H%M%S)
cp -a /var/lib/pod-prereset-<TIMESTAMP> /var/lib/pathofdust
chown -R pathofdust:pathofdust /var/lib/pathofdust
chmod 0750 /var/lib/pathofdust
```

Put the environment back — the reset removed three keys and two unit lines:

```
cp -a /root/production.env.pre-reset /etc/pathofdust/production.env
```

and restore the two `Environment=TWITCH_*` lines to the unit file, then:

```
systemctl daemon-reload
systemctl start pathofdust
```

**`/root/production.env.pre-reset` is taken at §4 and is the only copy of the
secrets on this box.** Without it, rollback restores the world but not the
seam, and `ADVENTURE_API_SECRET` must be re-agreed with the bot on Windows
from scratch.

Verify: `loaded 67 characters`, stage back near 7359,
`POST /api/commands/join` returns `401` (mounted), `/login` returns `302`.

### 8.2 What rollback loses

| Lost | Why |
|---|---|
| **Every World 2 character, account and session** created since the reset | They lived in the directory that was moved aside. Recoverable only from `/var/lib/pod-postreset-*`, which is kept, not deleted |
| **All World 1 coarse, detail and bundle replays**, if the snapshot in §4 was skipped and only the tarball exists | The tarball excludes them by size. Only `adventure-fights-summary` is covered. The `cp -a` snapshot does include them |
| **Sessions minted between the snapshot and the reset** | The snapshot is a point in time |
| **Any live tunable or passive override edited after the snapshot** | Same |
| **Nothing else.** Sprites, wiki, templates, patch notes and markers are all carried in the snapshot | — |

### 8.3 The rollback window

The pre-reset snapshot and the post-reset directory are both kept on the box.
**Do not delete either until the owner declares World 2 settled.** They are the
only thing standing between a bad reset and an unrecoverable one.

---

## 9. Rehearsal

This procedure must be rehearsed on a shadow copy before its first production
use, and again after any edit to §5.

A shadow is a full copy of `/var/lib/pathofdust` in a separate directory,
running a **separate copy of the binary** on separate ports, with
`GAME_DATA_DIR` set to the shadow directory and the process's working
directory set there too.

Four things that are easy to get wrong:

- **`CUSTOM_SPRITE_DIR` and the `templates/`, `wiki/`, `public_adventure_overlay/`
  directories are CWD-relative, not `GAME_DATA_DIR`-relative**
  (`character.rs:792`). Start the shadow with its working directory set to the
  shadow root or it serves no sprites. These are reads only, so a mistake here
  costs fidelity, not safety.
- **The listener binds `0.0.0.0`** and the nftables rule on this box only drops
  4004/4005, with an `accept` input policy. **Add a drop for the shadow ports
  before starting it**, or the shadow is internet-facing.
- **Run a copied binary**, never `/opt/pathofdust/bin/game`, so `/proc/<pid>/exe`
  distinguishes the shadow from production and it can be stopped by PID with
  its identity confirmed first.
- **Never stop a game process by image name.** Production runs the same `game`
  image (CLAUDE.md, PRODUCTION SAFETY). Resolve the PID, confirm its `exe`
  path, then stop that PID.

### 9.1 Rehearsal record

**2026-09-02 — first rehearsal. Passed, after three corrections to this
document.**

Shadow: `/var/lib/pod-shadow`, a 6.3 GB `cp -a` of live state under
`nice -n 19 ionice -c3`, running a copied binary (byte-identical,
`sha256 2d7d8114…`, but at its own path so `/proc/<pid>/exe` distinguishes it)
on ports 4015/4014 with an nftables drop added for both. Production was never
stopped and never touched; it stayed on PID 69253 throughout and its stage
advanced normally (7359 → 7363) across the whole rehearsal.

Before-state, to prove the shadow was faithful: `loaded 67 characters from
/var/lib/pod-shadow/…` (the shadow path, proving `GAME_DATA_DIR` isolation),
`/` 200, `/login` 303, `POST /api/commands/join` 401.

| Check | Result |
|---|---|
| Empty roster | `loaded 0 characters` |
| World at defaults | `stage 1`, `boss_power_mult 1.0`, `hp_pacing_mult 1.0` (World 1: stage 7361, hp mult 472.13) |
| Seam un-mounted | `POST /api/commands/join` **404**, `/api/announcements/stream` **404** (were 401) |
| Twitch login gone | `/login` **404** (was 303), plus the explicit "Twitch login is off" log line |
| No panic | 0 panic lines |
| Registration with no Twitch | new name `shadowtester` → 302 + `adv_session`; re-login 302; wrong password 401 |
| A World 1 name is free | `artelorian` (a live World 1 character) → **302**, no "already taken" |
| Operator name still protected | `lokati` and `lokati_gaming` both **400 "That username is reserved"** without the bootstrap variable |
| Bootstrap dance | set → both vars visible in `/proc/<pid>/environ` → register 302 with the bypass warning logged → `/admin/tunables` **200** (anonymous 404), `/admin/passives` 200 → unset → re-registration **400** → operator session survived the restart, still 200 |
| Fight numbering | all four tiers wrote `fight-0000000001.json`, all four counters at 1 |
| Fight is sane | boss fight, 3 players, won, **4,164 ms real** / 6,000 ms display. Artifacts 4 K / 8 K / 40 K / 44 K against World 1's ~950 MB. Whole world 59 MB after the reset, from 6.3 GB |
| `/fights.json` | 401 anonymous, 200 authenticated, serving fight 1 |

**TRAP 1 demonstrated, not just asserted.** The coarse tier was put into the
wrong state deliberately — five retained files numbered 19439–19443, counter
deleted — and a real fight triggered. The new `fight-0000000001.json` was
written and pruned inside the same `write_and_prune` call: the directory came
back byte-for-byte unchanged, the counter advanced to 1, and **zero** errors
were logged. The summary tier, reset correctly alongside its counter in the
same run, wrote fight 2 normally. That is the control.

**Three defects this rehearsal found in this document:**

1. **§5 step 5's trap description was wrong.** It said a restarted counter
   overwrites a retained World 1 fight. It does not — retained numbers are all
   high, so there is no collision. The real failure is self-pruning, which is
   worse and equally silent. Corrected before the rehearsal, then proven by it.
2. **§5 step 5's verification command gave a false alarm.** The bare
   `adventure-fights-*` glob also matches
   `adventure-fights-storage-migration-marker.json`, one of the 23 markers that
   must be kept, so a correct reset printed `STOP: fight state remains`.
   Replaced with an explicit list.
3. **§7 step 6 expected the wrong refusal message.** Re-registering the
   operator name is refused as `That username is reserved`, not `already
   taken` — `username_rejection` checks the operator-gate arm before the
   accounts map.

Not defects, recorded so the next run does not re-investigate them:
`/fights.json` returns `401` and an empty array without a session (it is
session-gated); `bc` is not installed on this box.
