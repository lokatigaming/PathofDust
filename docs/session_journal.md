# Session journal

## 2026-08-25 — OX — Stage 2 drift migration (tunable_audit.md §3 Groups B+C)

Branch `feature/passive-tunables-stage1` (off origin/master line), own
worktree `C:\PathofDust-stage1`, own target dir `target-stage1`.

**Done:** 17 nodes switched from `passive_node_rank` to declared-value
reads — deathdefiant, timewarp, demonicspeed, unwavering,
unyieldingfaith, huntersfocus, healingflames, blazing, finaloffering,
unrelenting (value folds/ladders → Special/SpecialPerRank) plus the seven
count nodes golemmaster, risingphoenix, virulence, cursedblood,
livingbond, naturesembrace, verdantburst(charges) → `passive_node_count`,
added to INTEGER_COUNT_NODES. golemmaster's three call sites
(combat spawn / manager slot check / web picker) all read the same count.
Deleted the two bespoke lookup fns (`healing_flames_regen_pct`,
`blazing_attack_speed_pct`). Lists: PENDING 47→31, PARTIALLY 7→3,
INTEGER_COUNT 21→28.

**Left behind:** sacrifice (Bloodpact damage mult), bloomingfield
(bounce count), reaperscall (chain max-extra) — second value of their
own still rank-fed, but each node's single magnitude table is occupied by
its wired primary; migrating needs a structural change. Kept listed.
No action needed: mercifultouch (verified wired), ravage/endlessthirst/
naturesblessing (only structural unlock-gates read rank; removed from
the partial list).

**Verification:** defaults reproduce old behavior exactly — pinned by
two new tests; full workspace suite green (679 passed, was 678);
golden fixtures untouched; clippy clean on touched code.

Commit: see `git log -1` (drift batch).

## 2026-08-27 — STAGE3-PASSIVE-TUNABLES (feature session)

Branch `feature/passive-tunables-stage3`, worktree `C:\PathofDust-stage3`,
own `--target-dir target-stage3`. Off origin/master 234a487.

**Work list as found:** PENDING_MIGRATION_NODES 31 entries,
PARTIALLY_TUNABLE_NODES 3 entries (sacrifice, bloomingfield, reaperscall
— exactly the three the order excluded, so the whole partial list was out
of scope and is untouched).

**Done:** 25 of the 31 pending nodes migrated from `passive_node_rank`
to declared-value reads through `magnitude_at_rank` →
`passive_override_for`, plus the matching call-site edits in combat.rs.
PENDING 31 → 6, INTEGER_COUNT 28 → 40. Admin inputs needed no
adventure_web.rs change: the page already renders r1/r2/r3 for any node
not listed as pending, so removal from the list IS the input (and the
form's field set is unchanged, so no 422 in either direction).

**Left behind (BLOCKED + reason):** 6, in two kinds, both now documented
on the list itself — clarity/lastlaugh/neverending/sanctifiedtouch are
structure-only (every rank read is an unlock gate; they own no rank-fed
number), and reckless/deathwish need a second per-node value slot (dealt
AND taken ladders), the same blocker as the excluded three. Schema note
for all five second-slot nodes is in docs/passive_tunables_spec.md
"Stage 3 record".

**FOUND (out of scope, not acted on):** eight nodes' declared per-rank
values disagreed with what combat.rs actually used, always by a rank
(payback, secondwind, crush, vitalstrike, gloriousdeath, undying,
doubletap, lastrites) — inert declarations, so no live number moved, but
anything rendering a node's magnitude was showing a wrong number. The
migration declares the game's real values; WIKI_IMPACT.md lists the
display deltas. `lastrites` additionally advertises a 33/66/100% chance
that has never been implemented (the shared save check is a charge
count); description left untouched, flagged for the owner.

**Verification:** `cargo test --release --workspace --quiet
--target-dir target-stage3` → 716 passed, 0 failed. Clippy: no new
warnings on touched code (the only hits in the touched files are the
pre-existing doc-list-indentation ones in PARTIALLY_TUNABLE_NODES' doc).
Golden fixtures untouched and unregenerated.

Commits: 86fb5b0 (migration), ad3f026 (lists + tests), plus this docs
commit.


## DEPLOY-PASSIVE-TUNABLES-STAGE3 (2026-08-27) — shipped, with one live incident caught and reverted

Merge `7ddfd8d` (`feature/passive-tunables-stage3` → master), gitignore
commit `23e2b32`, pushed. Binary swapped per §13 4a: live
`5361D4AD…` → `5F3B595A…`. Rollback at
`backup-pre-passive-tunables-stage3/` (old game.exe + 200-file pinned
fight-summary snapshot) and `target/release/game.exe.pre-passive-tunables-stage3`.
Bot diff-clean, not redeployed. Maintenance flag set before the stop and
cleared after the health check; downtime a few seconds.

Verification: `cargo test --release --workspace --quiet --target-dir
target-deploy-stage3` → 716 passed, 0 failed (exit 0). Clippy exit 0, no
new warnings on touched lines (blame confirms the doc-indentation hits
pre-date this branch). Golden corpus REGENERATED at merge per house rule:
14 of 17 fixtures rewrote, but the only changed keys across all of them
were `hitId`/`eventId` (20008 + 17118 occurrences, zero combat values) —
process-global counters that `approx_eq` skips by design, low here only
because a filtered single-test run restarts them. Committed fixtures
restored; no semantic diff.

**INCIDENT — three stale overrides went live at the swap.** Stage 3
switched 25 nodes from reading rank to reading their declared magnitude,
which means the override store now feeds them. Three keys had sat inert
in `adventure-passive-overrides.toml` (all three were on the OLD pending
list, so the page never offered them — generic seed values, not owner
tuning) and activated the moment the binary swapped:

| node | pre-swap | went live as | players affected |
| --- | --- | --- | --- |
| chakraoflife | 1000/2000/3000 ms | 330/660/1000 ms (~3x nerf) | 4 monks |
| unyieldingspirit | 0.35/0.45/0.55 | 0.33/0.66/1.0 (r3 always-on) | 8 monks |
| shattering | 1/2/3 targets | 2/4/6 targets (2x) | 2 elementalists |

The suite stayed green because every migration test pins DEFAULTS, and
the live store is not at defaults. Caught post-deploy by the
`current ≠ default` columns on `/admin/passives`. Remediated by reverting
all three to declared defaults (which reproduce the old call-site values
bit-exact), confirmed back at pre-swap values. Live for roughly twenty
minutes across ~20 boss fights.

**Durable rule this earns:** MOVED (2026-08-28) to
`docs/passive_tunables_spec.md`, "Required pre-migration step
(2026-08-28, BINDING)" — the authoritative copy, kept there because that
is the file a migration session is told to read. Ledger `#49`.

Store audit (ordered follow-up): the 33 remaining keys were checked
against the 25 migrated — intersection empty, so nothing else was
activated by this deploy. All 33 are genuinely consumed via
`magnitude_at_rank`, either by literal-key read or through the generic
`FlatStat`/`OverflowConversion` accumulation paths; no inert overrides
remain.

**FOUND (reported, not acted on):** `/admin/passives` rows render NO unit
word — not "fraction", "percent", "seconds" or "count". `relentlessassault`
shows "0 / 0 / 2" with nothing saying SECONDS; `payback` shows
"0 / 0.3 / 0.45" with nothing saying it is a 0-1 fraction of max HP. Save
validation is only "known key + finite", with no range clamp, so typing
45 for "45%" into payback persists 45.0 and reads as an always-true
threshold. Stored units DO match what combat consumes on the three
spot-checked (payback fraction, doubletap count, relentlessassault
seconds→ms).

Step 8 of the deploy order (confirm a stored override actually reaches
combat) — **VERIFIED**, but proven unintentionally by the incident above
rather than by the ordered deliberate-value test. The three stale
overrides activating at the swap produced observable combat changes for
14 players across three nodes (chakraoflife 4 monks, unyieldingspirit
8 monks, shattering 2 elementalists); that a value sitting in
`adventure-passive-overrides.toml` changed live fight behaviour the
moment the binary began reading it is conclusive evidence the override
path reaches the engine.

The ordered test itself was not run, and per owner ruling (2026-08-27)
must NOT be re-run. For the record of why it was not runnable: all 200
summaries covering 3.5 hours are `kind=boss` with all 46 players in every
fight, so the "wait for the boss to resolve" precondition had no window,
and the narrowest-blast-radius candidate (payback, a single player) has a
saturated observable — that character already crits on 100% of hits.

## 2026-08-27 — PASSIVE-OVERRIDE-UNITS (fix/passive-override-units)

Classified all 463 editable `/admin/passives` nodes by unit from their
CONSUMING code and added per-field range validation on the save path.
463 nodes: 385 fraction (42 of those bounded 0..1 by a probability roll
or an HP-fraction comparison, 3 clamped 0..0.9), 50 count, 24 seconds,
2 milliseconds (symbiosis, unrelenting — declared in ms, read with no
scaling), 2 multiplier (flamegolem, surgicalstrike). 0 percent — nothing
in the sources divides a passive magnitude by 100. 0 unit unconfirmed.

FOUND: nothing currently in `C:/PathofDust/adventure-passive-overrides.toml`
would be REJECTED by the new validation. Six stored keys hold values
above 1 on an unbounded fraction (cutthroat, finalcut, volley, growing,
echo, chainshot — all `1.0/2.0/3.0`-shaped ladders that are legitimately
over 100%), so re-saving one of those rows now warns and asks for an
explicit confirm rather than refusing it. vampiricfrenzy's rank-3 `0.9`
sits exactly on the clamp ceiling its consumer applies, and is accepted.

## 2026-08-28 — STAGE2-OPERATOR-LEVERS (feature/operator-levers)

Added one web operator control, `POST /admin/ops/next-encounter`, with
the boss select. Two of the three ordered capabilities were cut on owner
rulings after the fit report: Permanent Rampage already existed as a
`/admin/tunables` checkbox, and Force Boss is a strictly worse
next_encounter as an operator control (deferred to content work as the
player-facing dust-priced version).

FOUND: the existing admin POST routes (`/admin/tunables/save`,
`/admin/passives/save`, `/admin/passives/revert`) answer a non-admin
submission with a bare redirect and no status code — indistinguishable
from success. Open ledger finding, not fixed here; the new ops route
deliberately does not copy the pattern.

FOUND: `LiveTunables::permanent_rampage`'s doc comment (tunables.rs:213-215)
says `rampage_remaining` is "in-memory-only/cleared by a restart". That
stopped being true on 2026-08-17 when `persist_rampage_remaining` /
`RAMPAGE_STATE_PATH` landed; manager.rs:1749 states the corrected
behavior. Stale doc only, no behavior involved.

FOUND (for the removal stage): once the bot's `!rampage` command and the
player 3-vote are deleted, nothing can set `rampage_remaining`. Deletion
candidates at that point: `rampage_remaining`, `rampage_notify`,
`rampage_votes`, `RAMPAGE_VOTE_THRESHOLD`, `RAMPAGE_ENCOUNTER_COUNT`,
`RampageVoteOutcome`, `start_rampage`, `register_rampage_vote`,
`persist_rampage_remaining`, `RAMPAGE_STATE_PATH` /
`adventure-rampage-state.json` (and its `backup-game-data.ps1` entry),
the countdown branch of `spawn_rampage_loop`, and
`announce_rampage_complete`. `permanent_rampage` becomes the only
rampage state, and `rampage_active()` collapses to reading it.

## 2026-08-29 — BOT-STANDALONE (feature/bot-standalone)

Made the bot's game integration optional so the seam can be turned off
without killing the bot. `ADVENTURE_API_SECRET` was hard-required
(`config.rs`, the only game key that was), and unsetting it is exactly
how the game un-mounts `/api/*` — so before this, the audit's Stage 1
would have taken the OBS overlays and song requests down with the game
integration. Now `Option<String>`, same contract as `streamelements_jwt`.

Three files, no deletions, no game-crate change. `Services.adventure` is
`Option<Arc<AdventureApiClient>>`, so every one of the 15 call sites had
to handle absence at compile time. The ten command arms return `None`
from `handle_builtin` — documented there as "not a built-in at all" —
which falls through to the static-command lookup and then to
`Reply::None`, i.e. the command is genuinely unregistered and the bot is
silent. Owner's call: these commands are being retired permanently, so
silence beats an error string.

FOUND (reported, guarded): the three adventure channel-point rewards were
created UNCONDITIONALLY — "the adventure game is always on". The
removal-scope audit's framing missed this; it treats the redemptions as
three routes, not as three purchasable objects with a creation path.
Creation is now gated. Note this only stops NEW creation: the three
rewards already live in the channel persist on Twitch's side and must be
disabled by hand at cutover — Reforge Gear
`bfe77bde-b911-42de-9cf3-911ca6ac097e`, Repair All Gear
`778acf7b-1182-4128-a68e-f4e134ae1064`, Force Boss Fight
`c652ea13-1166-4c2a-beb5-2fa81da1b7f7`.

FOUND (for the ledger, deliberately NOT fixed here): the bot's log sink
is `tracing_appender::rolling::daily` with no retention policy and no
pruning anywhere — `main.rs`'s own comment records logs/ having reached
several GB once already, fixed by a one-time manual cleanup. This is a
standing disk risk independent of this change; it resolves at the Linux
move where journald owns rotation. It is also why the announcements
relay task is not spawned at all when the integration is off rather than
left to fail politely: against an un-mounted `/api/*` the loop would warn
once per 5s, roughly 17,300 lines a day, into that unpruned sink.

FOUND: `hand_written_public_entries()` and `BUILTIN_NAMES` still list the
adventure commands when the integration is off — left alone by ruling
(delete nothing; the list becoming briefly inaccurate is a documentation
problem with a documentation fix, and reversibility is worth more than
freeing nine reserved names).

Honest test gap: the startup smoke proves `Config::load()` no longer
gates on the secret — with and without it the binary now fails at the
same later point (`No tokens.json found`) in an isolated temp CWD. It
does NOT prove a full live start, which needs real Twitch tokens and
would mean a second bot joining production chat. That belongs to the
deploy session.

## 2026-08-29 — LINUX-READINESS (branch `feature/linux-readiness`)

Four ordered fixes from `docs/platform_portability_audit.md`: the missing
`#[cfg(unix)]` directory fsync after `write_atomic`'s rename; the
Windows-only rename-retry loop made conditional; `is_valid_custom_sprite`'s
case asymmetry; and five of the six Group-B game files routed through
`GAME_DATA_DIR` (the sixth, the custom-sprite directory, was left
CWD-relative by the owner's explicit ruling). Full detail in the commit
message rather than repeated here.

FOUND — `public_adventure_overlay/sprites/custom/` holds mutable user data
inside a checked-in source directory: 14 files in the live deployment
against 9 in the repository, the difference being player uploads that exist
only on the production box. Harmless while the deployment root IS the
checkout; on a Linux /opt-vs-/var-lib split the naive layout puts it on the
code side, where every deploy destroys the uploads and the service user
cannot write to it. NOT recorded here beyond this line and NOT written into
`docs/platform_portability_audit.md` — the owner assigned that record to the
session standing in the production checkout, which is corroborating the file
counts directly; this session must not write the same fact twice.

FOUND — `Sitch89_2.gif` is present in the live drop-in sprite directory but
is unselectable by anyone: `custom_sprite_name_matches` accepts a prefix
followed by digits only, and `"sitch89_2"` leaves `"_2"`. Not touched.

FOUND — a test that spawns the game binary and panics before reaping it
leaks the child, which then holds the harness's inherited handles and hangs
the whole `cargo test` invocation long after the tests report. Fixed in
`game_data_dir_paths.rs` with a `Drop` guard; `killed_process_smoke.rs` has
the same shape and the same exposure.

COORDINATION — one line inside the wiki module was changed:
`adventure_web/wiki.rs`'s `PUBLISHED_CONSTANTS_PATH` read became
`published_constants_path()`. Not a content or route change; it had to move
with the writer or the wiki would have rendered "varies" forever once
`GAME_DATA_DIR` is set. Flagged for the wiki session.

FOUND (DEPLOY-POOL-CAP-TUNABLE) — three clippy warnings land on the new
branch code, all inside `mod tests` in `pacing.rs` (`manual
RangeInclusive::contains` at :1107; `this assertion has a constant value` at
:1133 and :1137). Test-only, clippy exits 0, shipped code is clean. Left
unfixed during the deploy window; recorded in anomaly ledger #68.

## ADMIN-GATES-AND-BOOTSTRAP (2026-08-31, branch fix/admin-gates-and-bootstrap)

Four ordered fixes plus ledger #51, all in the admin/identity/startup area.
Fit report approved with five rulings; built as five commits.

SELF-CORRECTION — the fit report said that once the handler rejects,
`sanitize_pool_cap` "becomes unreachable over HTTP and survives only for
the TOML load path". That was too narrow: `pacing.rs:420`
(`capped_hp_mult_for_pool`) calls it on every generation read of the cap,
and `pacing::tests` at :1120-1127 already covers it directly. The ordered
"give it a direct unit test so it stays covered" item was therefore
already satisfied; no duplicate test was added. Stated in the report.

FOUND — `do_save_passive_override`'s BAD-KEY arm still answers with the
`?saved=1` redirect (a hand-crafted POST naming a node outside the class
being edited). Same "reported a success it did not perform" shape as
ledger #51, but a different arm and not in this order. One line, not
touched.

FOUND — the "no such character" cards at `adventure_web.rs:2254`/`:2277`
also return HTTP 200 with a body reading "Not Found". Same fake-404 shape
as the admin gates, on the public character pages. Out of this order's
scope; not touched.

FOUND — nine dynamic-pacing fields on `TunablesForm` carried
`#[serde(default)]` resolving to 0.0, below their own accepted floors. A
body omitting them had 0.0 silently clamped up, quietly overwriting live
pacing config. Fixed in the same commit as the validation pass (they would
otherwise have started 400ing), resolving to the shipped constants the way
`default_enemy_hp_pool_hard_cap` already did.

NOTE — `/admin/passives` returns 200 on a rejected save. The owner ruled
that is itself wrong and that `/admin/tunables` must use 400; aligning the
passives page is left for a separate order and was NOT done here.

NOTE — `docs/linux_staging.md`, cited in the order, is not on master. It
lives on `chore/linux-staging` (`6cc5456`, `70f601b`). Non-blocking.

## 2026-08-31 — LINUX-BACKUPS

FOUND — `tunables.rs:638` builds its error with
`std::io::Error::new(std::io::ErrorKind::Other, err)`, which clippy flags
as `clippy::io_other_error`; `passive_overrides.rs:192` already uses the
`std::io::Error::other` form. Pre-existing, one word, adjacent to this
session's edit but not part of it. Not touched.

FOUND — `cloudflared service install` writes THREE units into
`/etc/systemd/system` (daemon, update service, update timer), none owned
by dpkg. `--no-autoupdate` on the daemon does not cover the timer, which
was `disabled` but `active` and would have upgraded cloudflared and
restarted the tunnel unattended. Masked under the owner's ruling; see
`docs/linux_ingress.md`.

FOUND — a `cargo test --release --workspace --quiet` run backgrounded
through the harness reported exit 0 with only 6 of 34 `test result:` lines
captured (14 tests, not 758). The foreground re-run is the number of
record. Do not trust a backgrounded suite's captured output as a count.

FOUND — production's `adventure-item-balance.toml` names a retired affix.
Every start logs `adventure-item-balance.toml: 'lingeringEffect' is a
retired affix with no live base value to override, ignoring`. Harmless
(it is ignored) but it is live data that no longer matches the code.

FOUND — `/characters` emits BOTH a `.png` and a `.gif` URL for every
custom sprite and lets the browser fall back, so each custom sprite
produces exactly one guaranteed 404 per page render. By design, not a
migration fault; noted because it looks alarming in an access log.

FOUND — the operator account is unregisterable on a box holding
production characters. `do_register` (`accounts.rs:273`) refuses any
username a live character owns, `lokati` is both `OPERATOR_LOGIN` and one
of the 67 characters, and `OPERATOR_BOOTSTRAP` does not pierce that
check. A rebuild-from-empty therefore has no UI path to an operator
account. Needs an owner decision before cutover — see divergence #10 in
`docs/linux_deploy.md`.

FOUND — off-box scp DOWNLOAD throughput collapsed from 3.35 MB/s to
~10-70 KB/s for roughly ten minutes mid-session, then recovered to
5.14 MB/s. Uploads were unaffected throughout. The nightly
`PodPullLinuxBackups` and any restore-from-off-box run in the affected
direction. Not diagnosed.

## 2026-09-01 — CUTOVER-RUNBOOK

Wrote `docs/cutover_runbook.md` (executable procedure; nothing cut over,
no DNS touched, no tunnel config edited, nothing on Windows stopped).
Production was read and measured only.

FOUND — `/api/status` is not a route. Every check of the form "/api/status
returned 404, so /api/* is not mounted" is invalid: an unmatched path in
the nested router falls through to the outer fallback without reaching the
shared-secret middleware, so it returns 404 whether the seam is mounted or
not. `docs/linux_ingress.md` corrected here. `docs/linux_deploy.md` on
`chore/linux-deploy-proc` carries the same defect in three places (lines
295, 385, 430 as of `d913da9`) and must be corrected when it merges. Valid
probe: unauthenticated `POST /api/commands/join` — 401 = mounted, 404 =
not. Live Windows production returns 401.

FOUND — `adventure-fights-pinned/` does not exist on production. The
directory is created lazily by `pin_most_recent_fight`, so `!pinfight` has
never landed. Nothing to transfer at cutover; kept as a pre-flight
measurement in case a mod pins something first.

FOUND — the churning fight tiers are larger than the 2026-08-23 figure of
record: coarse 1,188 MB / detail 3,735 MB / bundle 3,775 MB = 8,698 MB,
which is 5 m 45 s at the measured 25.23 MB/s, not the ~4.6 min previously
extrapolated. Not carried, per ruling.

FOUND — `ADVENTURE_API_BASE_URL` is absent from `C:\PathofDust\.env`, so
the bot runs on its default `http://127.0.0.1:4005`. All 16 bot↔game links
break the moment the game leaves Windows. Not a fix for this session;
it is a numbered step in the runbook.

FOUND — the Linux unit's placeholder `TWITCH_CLIENT_ID`/`_SECRET` and its
loopback `ADVENTURE_WEB_PUBLIC_URL` would break Twitch login and its OAuth
redirect for any new login after cutover. Also a runbook step, with a
per-value verification.

## 2026-09-02 — CUTOVER-EXECUTE (attempt 1: aborted at the state gate)

Cutover attempt 1 was **aborted at §8.5 before the flip**. DNS never moved,
production was restored to Windows, and the session ended with Windows live
and fully protected. Per the standing ruling, an abort is a successful
outcome. Downtime **4 m 11 s** (04:13:43–04:17:54 UTC).

**OPERATOR ERROR — gate 2, on the owner's record.** The binding abort gate
was written as "world stage must be 7379", a value read from live production
during pre-flight, over an hour before the stop. At the stop the stage was
7369. The gate failed and the session aborted rather than override it.

The data was never at fault: `adventure-world.json`,
`adventure-characters.json`, `adventure-accounts.json` and
`adventure-sessions.json` were all byte-identical between the frozen Windows
source and the payload as it landed on Linux. The world had simply moved in
the intervening hour, and moved *backwards* — that file carries
`boss_losses_since_win` and `recent_boss_outcomes`, and a boss loss regresses
the stage. The gate was measuring elapsed time, not the migration.

Logged as an operator error, not a session error: the owner pinned a binding
check to a snapshot of a live, mutating value. The session's abort was the
correct response — a gate the operator overrides on the spot is not a gate.
The corrected invariant (equality against a reference read at §8.2a, plus
SHA-256 equality on the four state files) is now in the runbook, with the
reasoning, so the shape of the mistake does not repeat.

FOUND — **§8.4 would have served a broken site after the flip.** `templates/`
(2 files), `wiki/` (14) and `public_adventure_overlay/` (40 MB, including the
14 custom sprites) all live *inside* `/var/lib/pathofdust`, so §8.4's `mv`
takes them with it — and the runbook's instruction to "restore them from the
deployment" is impossible, because `/opt/pathofdust` holds only `bin/`. There
is no deployment copy of any of the three. Following the old text literally
yields a state directory with no templates: the game starts, the journal
reports a clean load, and every page render fails. Caught before the window
opened, worked around by carrying the three directories from the moved-aside
tree, and now written into §8.4 as a numbered step with its own verification.

FOUND — **§6's premise is false.** It said Step 0 needs an elevated prompt and
is an owner action, because a deploy session's non-elevated token gets
`Access denied` from `Disable-ScheduledTask`. The session ran both
`Disable-ScheduledTask` and `Enable-ScheduledTask` against all three tasks
with no elevation and no error. Step 0 does not require the owner.

FOUND — **`Start-ScheduledTask` fails outright on a disabled task**
(`The task is disabled`, `HRESULT 0x80041326`). §10.1 listed `Enable` before
`Start` but never said why the order binds, and the failure is quiet under
pressure: the error goes to the error stream while the port poll runs to its
timeout, so it reads as "slow to start" rather than "never started". Cost
about a minute of the 4 m 11 s outage during the abort.

FOUND — **`/api/status` remains absent from the route table**, as recorded on
2026-09-01. The valid probe (`POST /api/commands/join`, no secret header)
returned 401 on Windows production and 401 on Linux once the drop-in landed.

FOUND — one fight resolved on Linux (`fight-0000018855`) between the load and
the stop, moving the staged world 7369 -> 7367. DNS never pointed there, so
no player saw it and Windows at 7369 remained authoritative. Ruled by the
owner as a fork to discard, not to reconcile; attempt 2's fresh copy wipes it.

## 2026-09-02 — CUTOVER-EXECUTE (attempt 2: production is on Linux)

**Production moved to Debian at 05:16:51 UTC.** All six binding gates passed,
the flip was clean, and no rollback was needed.

Downtime **~1 m 13 s** (Windows origin released 05:15:40 → record flipped
05:16:51). Ten consecutive post-flip probes returned 200 with **no 502 at
all** — the Linux origin was already up and verified before the record moved,
so the usual "one or two 502s" in §4 did not materialise.

Gates, as measured: 67 characters loaded · loaded stage **7380** equal to the
§8.2a reference read from the frozen Windows state · all four state-file
SHA-256s identical between frozen source and the payload read back out of the
shipped tarball · operator `/admin/tunables` **200 at 103,052 B** via a carried
`adv_session` cookie, anonymous **404 at 71,722 B** · `Sitch89.gif` 200 at
687,999 B with the lowercase variant 404 · templates/wiki/overlay carried and
served (2 / 14 / 4 entries, sprites 14).

The §8.2a reference mattered: the stage at the stop was **7380**, not the 7379
seen at attempt 1's pre-flight nor the 7369 that aborted it. A fixed number
would have failed a third time. The equality check passed first try.

First fight on Linux: **fight-0000018875**, a boss fight, won, **30,400 ms** —
normal pacing, not the ~2 s that §11 flags as a controller fault.

Bot repointed by adding `ADVENTURE_API_BASE_URL` to `.env` and restarting under
its own maintenance flag; `published_constants` posted to the game on
**attempt=1**, and the `/api/announcements/stream` reconnect loop that had been
failing against loopback every 7 s stopped at the restart. Sessions survived:
a pre-cutover player session authenticated (94,014 B vs 72,025 B anonymous).

FOUND — **`/patch-notes` is 193,519 B and PowerShell hangs capturing it.**
`curl.exe -s <url>` into a PowerShell variable blocked past a 120 s timeout on
that page while `curl --max-time 10` from Git Bash returned it in **1.23 s**.
The server is fine; the buffering is PowerShell's. Do not diagnose a live
site as hung on the strength of a PowerShell capture — re-probe from Git Bash
with `--max-time` before believing it.

Windows end state per §12.1: game **stopped**, `GameProcess`,
`GameProcess-Watchdog` and `GameDataBackup` all disabled; `TwitchBotRS` and its
watchdog running and untouched; `PodPullLinuxBackups` still enabled — it is now
the only off-box copy of anything. `staging.lokati.net` retired at both layers:
ingress rule removed here, DNS record deleted by the owner (NXDOMAIN).

Rollback assets retained: `/var/lib/pod-precutover-20260902-071616`,
`/root/pod-cutover-state.tar.gz`, `/root/patch-notes-precutover.json`,
`C:\dust-work\.env.pre-cutover-backup`, and the frozen `C:\PathofDust` itself.

## 2026-09-02 — CUTOVER-EXECUTE (the post-cutover outage, and its fix)

Production went to Linux at 05:16:51 UTC and was **71% unresponsive**
within the hour. Root-caused and fixed the same session. No rollback: the
owner cancelled rollback authorisation mid-triage and ruled that we stay on
Linux and fix it there. Windows stayed stopped and frozen throughout.

**Two framings were proposed and both were wrong, in sequence. What was
actually true is the third.**

1. *"It's the tunnel / QUIC."* Reasonable — cloudflared defaults to QUIC over
   UDP, and a UDP-buffer or MTU problem produces exactly this signature.
   **Killed by measurement:** loopback `http://localhost:4005/` stalled just
   as badly as the tunnel (178 samples, 26 non-200, max 8.0 s, mean 1.21 s).
   A tunnel fault cannot make loopback slow. For the record the transport IS
   QUIC and there were never any UDP-buffer or MTU warnings.
2. *"It's the disk — ~1.9 GB per fight."* Half right and the wrong half.
   The volume is real (detail 933 MB + bundle 943 MB per fight) but it is
   **CPU serialisation on the async runtime, not disk I/O.** iowait measured
   **0–1%** throughout, 282 GB free, no OOM, no swap. "It's the big files"
   and "it's the disk" are different claims and only the first is true.
3. *"It's the serialisation, so wrap `save_last_fight`."* Also wrong, and
   this one was the owner's stated leading candidate — re-scoped BEFORE it
   was acted on. Bracketing the tier writes by their mtimes (coarse 08:50:05
   -> detail 08:50:10 -> bundle 08:50:17 -> summary 08:50:18) puts the entire
   write phase at **13 s**, against a **158 s** stall. **The cost is
   `simulate_battle`, ~145 s — 92% of it.** Wrapping only the writes would
   have recovered under 10% and left a ~145 s freeze, looking like a failed
   fix.

**Actual root cause.** `simulate_battle` (`manager.rs:5001`, `:5618`) is a
synchronous, unbounded-duration computation called directly from the async
encounter loop. It never yields, so it freezes the whole Tokio runtime.
Not merely slow handlers: **`accept()` itself stopped** — `LISTEN Recv-Q`
backed up to 12, and probes showed `connect` completing in 0.00018 s with
`starttransfer` never arriving. A **static sprite** stalled in the same
instants as a dynamic page, which is what ruled out per-route lock
contention on game state. Measured **158 s unresponsive of every 221 s
cycle**.

**Why it only appeared after the cutover.** The defect was always there and
was firing on Windows too, just briefly enough to be invisible. This box is
a generic emulated `QEMU Virtual CPU version 2.5+` (no host passthrough, so
no modern instruction sets) at ~3992 BogoMIPS; Windows ran an i5-12400F.
Steal time 0, so it is the vCPU model, not a noisy neighbour. Same code,
same data, near-identical output sizes, similar cadence — the machine
simply got much slower at the same work, and a rounding error became 71%
downtime. This also retires the §11 "known unknown: one unexplained
120-second stall after a migration load, never reproduced". That was this.

**Fix.** `tokio::task::spawn_blocking` around all four call sites —
`simulate_battle` at `:5001` and `:5618`, `save_last_fight` at `:5487` and
`:5852`. Verified before writing any code, at the owner's instruction:
no lock guard is alive across any of the four boundaries (each `world` /
`characters` guard sits in an explicit own-scope block that closes first);
`fighting`, `tunables` and `result` are MOVED in and back out rather than
cloned, so the ~1 GB event log crosses as a move; and `combat.rs`,
`fight_storage.rs` and `replay_bundle.rs` contain **zero** occurrences of
`tokio::`, `.await` or `async fn`, so nothing downstream assumes a runtime
worker. The RNG is constructed inside each closure — `ThreadRng` is not
`Send`.

**MissedTickBehavior::Skip**, chosen deliberately rather than defaulted.
The hazard is NEWLY reachable: before this change the runtime froze during
a fight so the timer was starved and could not accumulate; moving the work
off the runtime lets the clock run ahead of the loop. Default `Burst` would
fire every missed tick back-to-back and resolve several fights in seconds —
indistinguishable to a player from a bug or an exploit. `Delay` was
rejected: it reschedules a full interval after each fight completes,
silently stretching cadence to fight_duration + interval and halving the
fight rate. `Skip` realigns to the wall-clock grid: one fight per interval,
never a flurry, and a single skipped beat as the worst case.

FOUND — `manager.rs` has a pre-existing `unused_mut` on `let mut broken:
Vec<BrokenItem>` (never mutated afterwards). Confirmed pre-existing on HEAD,
not introduced here. Not touched.

FOUND — §13B's build step fails on first use: `systemd-run` does not inherit
the login environment, so `cargo` is not on `PATH` and the build dies
instantly with `cargo: command not found`, `exit 127`. `cargo` is at
`/root/.cargo/bin/cargo`. `HOME=/root` is needed too or cargo cannot resolve
`CARGO_HOME`. Corrected in REFACTOR_PLAN.md §13B.1 with the working form.

FOUND — the bot logs recurring `PayPal relay poll failed` against
`young-hall-6c35.parnold-id.workers.dev/pending-tips`. Unrelated to the
cutover or this fix (external Cloudflare Worker endpoint), pre-dates both.
Not investigated.

## 2026-09-02 — Twitch removed from the game repo (session TWITCH-REMOVAL-GAME)

Branch `chore/twitch-removal-game`. The `/api/*` bot seam, the Twitch OAuth
login and the overlay's Twitch chat embed are DELETED from source, not
merely unmounted by absent environment. `accounts.rs::mint_session` is now
the only session minter in the process.

**Overlay.** The chat panel and the canvas-shrink IIFE lived in the same
`if session.is_some()` block; the IIFE existed only to undo the 340px the
panel reserved. Deleting the block restores `overlay.html`'s own `resize()`
as the sole sizer. Measured in headless Chrome against a disposable
instance with a seeded session: all three canvases (`stage-back`,
`stage-mid`, `stage-top`) are 1280x720 at a 1280x720 viewport and 1000x600
after a resize to 1000x600 — full viewport, no reserved strip. Under the
old code they would have been 940 and 660 wide.

FOUND — the removal scope (docs/external_integration_removal_scope.md D30)
claims `subscribe_announcements`' "sole consumer is api.rs:403-411". Wrong:
`adventure_overlay_server.rs:260` calls it too, teeing every announcement
onto the live `/ws` socket. Deleting it broke the build. Retained, with the
correction recorded on the method.

FOUND — the scope's D56 says the three `channel-points-*-reward.json` files
have lines in the backup scripts. They do not; only
`bot-published-constants.json` did, and it is now removed from both. Those
three files are written by the bot crate and were never in either script.

FOUND — `AppState.public_url` and `main.rs`'s `env_var_or` helper became
dead with the OAuth `redirect_uri`. Both removed. `ADVENTURE_WEB_PUBLIC_URL`
is now read by nothing; it is inert wherever it is still set. Whether to
pull it from the unit file is a deploy-time call.

FOUND — three constants (`ACTIVITY_XP_COOLDOWN`, `ACTIVITY_XP_AMOUNT`,
`RAMPAGE_VOTE_THRESHOLD`) are retained WITHOUT callers because
`wiki.rs:282/308/309` reads them and this session may not edit that file
(owner ruling 1). The wiki therefore documents two mechanics the game no
longer has. WIKI_IMPACT.md carries the removal request.

FOUND — the golden-corpus/full-suite run was aborted once by my own error:
a disposable `/overlay` instance started from `target/release/game.exe`
file-locked the binary while cargo tried to relink it. Do not start an
instance from the shared target dir while a suite is running.

**The removal-scope audit has now been wrong three times, all in the same
direction: calling something dead that has a live consumer.** Owner's ruling,
2026-09-02 — treat `docs/external_integration_removal_scope.md` as a LEAD, not
a list. Verify every entry against the code before acting on it.

| # | Audit claim | Reality |
|---|---|---|
| 1 | D30: `subscribe_announcements`' "sole consumer is api.rs:403-411" | `adventure_overlay_server.rs:260` calls it too, teeing announcements onto `/ws`. Deleting it broke the build |
| 2 | D24/D28: the activity-XP and rampage-vote constants fall with their functions | `wiki.rs:282/308/309` reads all three. Deleting them breaks a file this session may not edit |
| 3 | D56: the three `channel-points-*-reward.json` files have lines in the backup scripts | Neither script ever listed them |

(Ledger #54 recorded the same failure shape on the Patreon slice — five missed
targets — so this is a fourth instance of the pattern, not a first.)

**Why the overlay measurement means anything.** The numbers are only evidence
because of the counterfactual: the deleted IIFE set every canvas to
`window.innerWidth - CHAT_WIDTH_PX` with `CHAT_WIDTH_PX = 340`. Under the old
code the same two viewports would have produced canvases **940** and **660**
wide. Measuring 1280 and 1000 is what proves the shrink override is gone rather
than merely inert.

### Deploy — World 2, Twitch removal (2026-09-02)

Merge `f3328b7` to master; binary `4e5b8ca4d2742138988257bd433ed9245a98f244097930c5b0001efa12412106`
(previous `2d7d8114d5a458612d097545f040d2fc0b1ee9b0e71aa2cc90b8d51dd1120c20`).
Built on the box from `git archive f3328b7` under `systemd-run` with explicit
PATH/HOME: build 2m36s exit 0, suite **755 passed / 0 failed / 0 ignored**
across 31 binaries, exit 0 — the corrected §13B baseline, hit exactly.
`deploy-linux.sh` downtime **0.24s**, `NRestarts=0` before and after both
restarts. Rollback slot
`/var/backups/pathofdust/deploy-pre-twitch-removal/game.pre-twitch-removal`,
backup archive `pod-backup-20260902-143149.tar.gz`.

**Binary verified before any env change** (owner's ordering, so a failure
would implicate one change not two): all five `/api/*` probes 404, `/login`
and `/auth/callback` 404, `/` `/account/login` `/patch-notes` `/wiki`
`/overlay` all 200. Overlay measured in headless Chrome through an SSH
tunnel against live production: all three canvases 1280x720 at a 1280x720
viewport, 1000x600 after resize, zero iframes, settings tray intact.

**Env cleanup, second restart.** Deleted `/etc/pathofdust/production.env`
(one line: `ADVENTURE_WEB_PUBLIC_URL`), `10-production.conf`,
`20-bootstrap.conf`, and unit line 43. Resolved environment is now exactly
`OPERATOR_LOGIN=lokati ADVENTURE_WEB_PORT=4005
ADVENTURE_OVERLAY_SERVER_PORT=4004` — the stale-variable grep returns 0.
Operator gate verified BY EFFECT after the restart: `/admin/tunables` 200
with the operator session, 404 anonymous, `/admin/passives` 200.

FOUND — `ADVENTURE_WEB_PUBLIC_URL` was set in TWO places, the env file AND
unit line 43. The original deploy step removed only the file, which would
have left behind the exact variable the cleanup existed to remove. Caught
by the owner's order to enumerate both files before deleting rather than
after. Enumerate-before-delete earned its keep here.

FOUND — the first production overlay measurement read 300x150 (the HTML
canvas default), i.e. `resize()` had not run. Not a defect: `readyState`
was still `loading` 12s in, because the overlay's assets come through the
SSH tunnel slowly. At 60s the same page read 1280x720. A measurement taken
before `readyState` leaves `loading` measures nothing.
---

## 2026-09-02 — WIN-BASED XP (branch `feature/win-based-xp`)

**The premise was half wrong, and the working half mattered.** The order
was "players are not receiving XP; XP was tied to chat activity and chat
activity XP is dead." Chat XP is indeed dead — `/api/*` is not mounted at
all when `adventure_api_secret` is unset (`api.rs:61` returns `None`), so
`POST /api/activity_xp` 404s and `grant_activity_xp` is unreachable; the
Twitch-removal session is deleting the function outright.

But `add_xp` has exactly two call sites, and the second one — the boss-win
grant in `run_encounter_inner` — was working the whole time. Verified
against LIVE production by reading the public overlay socket
(`wss://adventure.lokati.net/ws`): stage 2, six characters, every one of
them level 1 with `xp: 6` and `wins: 1`. Six is exactly `(5 + stage 1) ×
catchup 1.0`. The mechanism was intact; the RATE was the defect. Chat XP
at 4 XP / 180 s was worth up to 1,920 XP/day to a chatter against fight
XP's 672/day, so losing it removed roughly three quarters of all income
and left a counter that moves once per ten minutes.

**Cadence, from the constants.** `ENCOUNTER_INTERVAL` 600 s → 144 boss
encounters/day; `BASIC_ENCOUNTER_INTERVAL` 180 s → 480 filler fights,
which pay no XP and no W/L by design. At the live `target_win_loss_ratio`
of 2.0 that is 96 wins/day. Two facts make 2:1 the right anchor rather
than an assumption: Controller B actively drives the party to it, and the
stage walk is +1 per win / −2 per loss, so exactly 2:1 is NEUTRAL — which
is also why the old `5 + stage` was a growth term that mostly did not
grow.

**The curve.** `xp_per_win = win_xp_flat + win_xp_level_pct ×
xp_to_next_level(level)`. The flat term is fixed in XP so its value in
levels decays against the quadratic level cost — that is the day-one
burst. The level-scaled term is worth a constant number of levels forever
— that is the floor. `levels/day → wins/day × win_xp_level_pct`, so 96 ×
1/48 = exactly 2/day, and `win_xp_flat = 12` puts day one at exactly 10
levels. Modelled at 2:1: 10, 5, 4, 3, 3, 3, 3, 3, 2, 3, 2, 3, 2, 3 —
level 50 at day 14, settling on 2/day. At 1:1 the asymptote is 1.5/day,
at 4:1 it is 2.4/day.

**Linearity in win rate is automatic**, not a term anyone added: XP is
paid per win, so daily XP is strictly proportional to the fraction of
encounters won. RULING (owner, accepted as-is): the band that gives is
inherently **0× to 1.5× of the 2:1 baseline**, because a win fraction
cannot exceed 1.0 and 2:1 is 0.667. A 4:1 ratio is only 1.2× the income
of 2:1. Recorded here explicitly so nobody later reads "linear in win
rate" as "unbounded" and re-derives the feature around a ratio score.

**Rampage.** A rampage makes `spawn_rampage_loop` the sole encounter
driver at a 60 s floor with every encounter a boss fight — 10× the rate
the curve is calibrated on. Guard is `win_xp_cooldown_secs`, a
per-character 450 s floor between XP-paying wins, mirroring
`ACTIVITY_XP_COOLDOWN`. At the scheduled 600 s cadence it never binds; a
rampage is throttled to 1.33× normal instead of 10×, and Force Boss Fight
(`FORCE_BOSS_MAX_PER_CYCLE`, up to 3× within a cycle) and `!nextencounter`
fall under the same mechanism. Chosen over a hard "no XP during rampage"
gate on owner ruling: a gate is exact but turns the most popular content
in the game into an XP drought.

**`permanent_rampage` was CONFIRMED OFF by the operator reading
/admin/tunables**, not inferred. Recording how it was established, because
the inference and the confirmation are not the same evidence: this session
had no shell on the Debian box and argued from the announcement-feed batch
pattern (batches of 2–3 fights flushed on the 5-minute timer, which is the
600 s + 180 s cadence; a rampage would produce 5-fight batches minimum).
The inference was right, but the operator's read of the admin page is what
closes it. Note that `C:\PathofDust\adventure-live-tunables.toml` — the
pre-cutover Windows config — still reads `permanent_rampage = true`, so
anyone reaching for a local file to answer this question will get the
wrong answer.

**Order of operations on the grant**, written out because "multiplier" is
overloaded on `LiveTunables`:
1. `win_xp_flat + win_xp_level_pct × xp_to_next_level(level)` — level read
   before `add_xp` can move it, so a win is always priced at the level
   that earned it.
2. `× catchup_multiplier` (1.0–3.0, per character, from PRE-fight group
   levels; switchable via `win_xp_catchup_enabled`).
3. `× win_xp_mult` — the uniform growth-rate dial (owner request), applied
   last. It scales both shape terms equally, so it cannot change the decay
   rate or the level the curve settles onto; it only moves the whole curve.
4. `.round()`, floored at 0, then `add_xp`.
`loot_mult` and `sand_mult` are NOT in this chain and never were — they
scale dust/items and sand. Nothing multiplies XP except steps 2 and 3.
Bounds on the multiplier are 0.0–100.0: 0 is the deliberate end-of-season
progression freeze (an operator needs a kill switch, and hiding it behind
an 0.01 floor would be worse than naming it — the zero this project has
been bitten by twice is an OMITTED field defaulting to 0.0, which
`default_win_xp_mult` prevents, not an operator typing one), and 100
leaves four orders of magnitude above the practical 0.01 floor while still
rejecting a fat-finger like 1e6.

**No backfill** (owner ruling). Existing World 2 characters keep their 6
XP and clear level 2 in about three wins at the new 13-XP-per-win rate. No
levels are handed out that nobody earned.

**Golden corpus untouched, and it did not need to be.** Corpus scenarios
take `level` as a fixed INPUT to `simulate_battle` and capture combat
output; the XP grant runs in `run_encounter_inner` after the sim returns
and never enters a fixture. No regeneration.

KNOWN INTERACTION TO WATCH (owner: accepted deliberately, not a defect) —
"2 levels/day forever" means level is unbounded, and worlds reset
seasonally so the real bound is season length, not the curve. The
compounding term is the archetype bonus multiplier `1 + 0.10 × level`
(`character.rs:129`), which is 15.8× at the level 148 the model reaches by
day 60 at 2:1. Defensive stats cap at `defensive_stat_hard_cap` long
before that; increased-damage and crit do not. The pacing controllers are
expected to adapt. Worth a look if a season ever runs long.

PATCH NOTE for the deploy session, ship verbatim:
  "XP comes from winning fights now. Every boss fight your party wins
   levels you up, and the more of them you win, the faster you level.
   Filler fights and losses don't give XP. Chat XP is gone."

FOUND — `manager.rs` still carries the pre-existing `unused_mut` on `let
mut broken: Vec<BrokenItem>` noted in the cutover entry above. Still
pre-existing, still not touched.

### 2026-09-02, follow-up — the win-XP arithmetic tests

Filling the gap the first pass reported: the grant had no test of its own,
only the constants assertion inside the HTTP test. On a live-world
progression change that was the wrong place to stop.

**One structural change to make it testable, and it removes a real bug
class rather than only exposing it.** The grant was five lines inline in
`run_encounter_inner`, which is reachable only through a whole simulated
encounter. It is now `Character::win_xp_for_win(level, catchup, &t)` —
pure, so the arithmetic is assertable directly — plus
`Character::award_win_xp(&mut self, catchup, &t)`, which reads the level
and calls `add_xp`. That second method exists for exactly one reason: the
price must come from the level that EARNED the win, and `add_xp` moves
`self.level` in the same breath. Spelling those two steps out at the call
site is how that becomes an off-by-one where a threshold-crossing win is
billed at the new, more expensive level — silently, and worth more the
bigger the grant. Inside one method the read provably precedes the
mutation.

**11 new tests**, 7 in `character::win_xp_tests` and 4 in
`manager::win_xp_cooldown_tests`:
- shipped-default grants at levels 1/10/25/50 (13, 18, 33, 80), each
  checked against `xp_to_next_level` so a level-curve change fails here
  too; plus the headline claim that three level-1 wins clear level 2.
- the off-by-one, forced visible by turning `win_xp_mult` to 100 so one
  win crosses ten levels — at the shipped 1/48 the adjacent levels round
  to the same grant (L1 and L2 are both 13), so the shipped dials cannot
  see this bug and the test has to leave them.
- order of operations, with values picked so all three readings differ:
  correct 32, rounding the sum early 33, multipliers on the level term
  alone 11.
- `win_xp_mult = 0` grants a clean zero, and NaN/negative floor to 0
  rather than wrapping into a huge `u64` or hanging `add_xp`'s
  subtract-and-loop.
- catch-up at 1.0/2.0/3.0, plus a guard asserting the real
  `catchup_multiplier` stays inside the 1.0–3.0 band the grant is
  documented against.
- the cooldown: two wins inside the window pay once, a win after it
  elapses pays again, the window is per-character (a newcomer is not
  locked out by someone else's recent win), 0 means no throttle, and 450 s
  sits strictly between `RAMPAGE_MIN_INTERVAL` and `ENCOUNTER_INTERVAL`
  with at least 120 s of slack.

**Mutation-checked, because a test that cannot fail proves nothing.**
Three deliberate defects were introduced and reverted: pricing after
`add_xp` (caught by the off-by-one test), rounding the sum before
multiplying (caught by 3 tests), and applying the multipliers to the
level-scaled term alone (caught by 3 tests). All three were caught.

Deliberately NOT asserted: that catch-up and `win_xp_mult` can be
swapped. Multiplication commutes, so their relative order cannot produce
a different number — the test says so explicitly instead, so nobody later
"fixes" a non-problem. What is order-sensitive is where the multipliers
sit relative to the SUM and where the single `round()` sits, and that is
what is covered.

The timing test uses a 10 ms cooldown against a 250 ms wait — a 25x
margin, deliberately wide so it does not join the known
flaky-under-parallel set.

### 2026-09-02 — WIN-BASED XP deploy record (release `win-based-xp`)

Merged `feature/win-based-xp` into master with `--no-ff` (merge `02da915`,
8 files, no conflicts — the rebase onto `3835699` had already resolved
them). Deployed to the Debian box by REFACTOR_PLAN §13B.

| | |
|---|---|
| master before / after | `3835699` → `02da915` |
| binary before / after | `4e5b8ca4…` → `154f13e6…` |
| source archive | `cb2fd853…`, `git archive` of the merge commit |
| suite (local / box) | 767 passed, 0 failed, 32 suites — identical both sides |
| build on box | 2 m 37 s, exit 0 |
| downtime | **0.21 s** |
| NRestarts | 0 before, 0 after |

Baseline arithmetic, checked rather than assumed: the box's own
`test-deploy.log` for master alone read **755 / 31 suites**, matching
§13B.1's recorded baseline. 755 + 12 (11 unit + 1 integration) = 767, and
the extra suite is `admin_tunables_win_xp_http.rs`. My earlier local
count of 770 was the same branch on the pre-removal master (758 + 12).

**A misconfigured remote wasted a cycle.** `origin` pointed at the local
`C:\PathofDust` clone rather than GitHub, so two "pushes" reported earlier
in the session went nowhere and the local mirror's stale `master` made the
Twitch-removal merge look unpushed. It was not: `3835699` was already
GitHub's master. The rebase happened to be correct anyway because it was
done against the commit hash, not against `origin/master`. Corrected by
the owner; the stale `feature/win-based-xp` ref left behind at `cac87f4`
in `C:\PathofDust` was deleted (ref only — that repo's working tree was
byte-identical before and after, 13 pre-existing dirty entries unchanged).

VERIFIED BY EFFECT, not by inference. §13B.5's seven checks all passed
(note check 3's expected N is **9**, not the 67 the table still records —
that is a World-1 figure and predates the World 2 reset). Then the five
dials rendered on live `/admin/tunables` with their shipped defaults and
full bounds, and four out-of-range POSTs (`win_xp_mult=101`,
`win_xp_flat=-1`, `win_xp_level_pct=1.5`, `win_xp_cooldown_secs=3601`)
each returned 400, named the field, said NOT SAVED, and left the live
tunables file byte-identical. Those POSTs were built by scraping the
rendered form's CURRENT values and perturbing one field, so an unexpected
accept could not have moved anything else.

**The real proof — one live boss win, 15:35:55, all 9 characters
participating, pre-fight levels [1,1,1,2,2,2,2,2,2]:**

| pre-fight level | XP granted | predicted |
|---|---|---|
| 1 (gorshie, jachiny, zolaries) | **38** | 38 |
| 2 (six others) | **26** | 26 |

Exact on both. Stage advanced 1 → 2 on that win.

FINDING — **the design table was computed at catch-up 1.0, and the live
roster is not at 1.0.** `catchup_multiplier` returns 1.0 only when every
level in the fight is equal. The moment the roster is mixed, everyone at
or below the median gets at least 2×, because the `l <= median` branch
floors the bonus at 100%. Today that is 2× for the six level-2 characters
and 3× for the three level-1s — which is exactly why the observed grants
were 26 and 38 rather than the 13 the no-catch-up model predicts.

This is correct behaviour, not a defect: catch-up on XP predates this
work, and the owner ruled `win_xp_catchup_enabled` ships ON. But it means
the approved curve understates real progression whenever the roster is
uneven, which for a 9-player world with staggered joins is most of the
time. Modelled at 2:1:

| catch-up | day 1 | day 7 | day 14 |
|---|---|---|---|
| 1.0× (the approved table) | L11 (+10) | +3/day | L50 |
| 2.0× (typical mixed roster) | L16 (+15) | +5/day | L81 |
| 3.0× (the trailing player) | L20 (+19) | +7/day | L110 |

So the "10 levels on day one, settling to 2/day" shape holds in form but
runs roughly 1.5–2× hot in practice. Nothing needs changing today —
levelling being too fast for a week is the recoverable direction, and the
group converges toward uniform levels which pulls catch-up back to 1.0.
If the owner wants the table honoured literally, the dial is
`win_xp_mult` at ~0.5, NOT re-tuning `win_xp_flat`/`win_xp_level_pct`,
because the multiplier is the one that scales without touching the shape.
Flagging rather than acting: it is a calibration judgement, not a bug.

FOUND — the `win_xp_*` keys are **absent from
`/var/lib/pathofdust/adventure-live-tunables.toml`** until the first
successful save of the tunables form. The running process holds the
shipped defaults via `#[serde(default)]` on `LiveTunables`, and the admin
page renders them correctly, so behaviour is right — but a future session
grepping that file for `win_xp_flat` will find nothing and may conclude
the feature did not deploy. It did; the file simply predates the fields.

Evidence kept on the box: `/root/watch_win.log` and
`/root/win_evidence.json` (the before/after character snapshot for the
verified win), `/root/patch-notes.pre-win-xp.json` and
`/root/tunables.pre-reject-test.toml` (rollback copies), and the rollback
slot at `/var/backups/pathofdust/deploy-pre-win-based-xp/`.

Patch notes: one new entry at the top of `patch-notes.json`, "XP comes
from winning fights now", six items, installed before the swap and
confirmed live. The rampage throttle is called a nerf in plain words, per
the honest-patch-notes rule.

### 2026-09-02 — RULING on the hot XP rate: leave it, and the dial if it persists

Owner ruling, recorded so the reasoning survives the session that produced
it. The deploy record above measured live grants running 1.5–2× the
approved curve table, because that table was computed at
`catchup_multiplier` = 1.0 and a mixed roster never is.

**No change made. Do not touch `win_xp_mult` today.** World 2 is a day old
with nine players clustered at levels 1–2, which is exactly the condition
that maximises catch-up: the spread between min and median is at its
widest relative to the level costs. It compresses on its own as the roster
converges, so today's 1.5–2× is the extreme of the range, not the steady
state. Too fast is also the recoverable direction, and fast early
progression is what was asked for. **Revisit in 24–48 hours against real
cumulative-level data, not against a projection.**

**If it is STILL running hot once levels converge, the dial is
`win_xp_mult` — not `win_xp_flat`, and not `win_xp_level_pct`.** This is
the part worth keeping, because the intuitive move is the wrong one.

The two shape terms do different jobs and changing either bends the curve:

* `win_xp_flat` is fixed in XP, so its worth *in levels* decays against
  the quadratic level cost. It sets the day-one burst and almost nothing
  else. Cutting it flattens the early game specifically and leaves the
  late rate untouched.
* `win_xp_level_pct` is a fraction of the level's own cost, so it is worth
  a constant number of levels forever. It sets the floor the rate settles
  onto. Cutting it lowers the asymptote and barely moves day one.

Reach for either and you change the SHAPE the owner approved — which
level range gets slower — while trying to change only the overall speed.
`win_xp_mult` multiplies both terms equally, so it scales the whole curve
and provably cannot alter the decay rate or the level it settles onto.
That is the entire reason it exists as a multiplier rather than a third
additive term, and `character::win_xp_tests::
the_multiplier_scales_without_changing_the_shape_of_the_curve` asserts
exactly that property at levels 1/10/25/50/100.

So: to halve progression, `win_xp_mult` 1.0 → 0.5. One field, no reshaping,
and the approved curve comes back intact at a different scale.

Turning `win_xp_catchup_enabled` off is NOT the equivalent lever and
should not be reached for as one. It would remove the trailing-player
bonus entirely rather than scaling the curve, which is a different
mechanic with a different purpose (it predates this work and was ruled to
ship ON), and it would slow the newest players most — the opposite of
what catch-up is for.

### 2026-09-02 — CRAFT CONFIRMATION FIX deploy record (template-only)

Merged `9d11733` from `feature/player-facing-batch` into master by HASH,
not by branch name, per the order. Verified first that its sole parent is
`3835699` and that nothing else rides along.

**The order's premise about the branch was already stale, in the harmless
direction:** `9d11733` IS the branch tip — there are no commits after it.
Merging the hash and merging the tip were the same operation today.
Merging by hash anyway was still correct: it pins the content against
someone pushing to that branch mid-deploy.

| | |
|---|---|
| master before / after | `1b6595b` → `30beba2` (merge) → `642504d` (docs) |
| merge conflicts | one, `WIKI_IMPACT.md`, keep-both chronological |
| suite (local / box) | **768 passed, 0 failed, 33 suites**, identical both sides |
| arithmetic | 767 (post-XP master) + 1 = 768. The commit's own "756" was 755 + 1 against the pre-XP master |
| build on box | 2 m 26 s, exit 0 |

**THE BINARY DID NOT CHANGE, AND THAT IS CORRECT.** Candidate hash came
out bit-identical to live: `154f13e6…` both sides. `git diff` over
`game/src`, `src`, `Cargo.toml` and `Cargo.lock` between the deployed
commit and this one is EMPTY — the fix is entirely `templates/base.html`
plus two test files and docs. The commit says so itself: "No Rust broke."

This matters procedurally, because **`deploy-linux.sh` aborts when the
two hashes match** ("nothing to deploy, you most likely built the wrong
tree"). That guard is right for the case it was written for and wrong for
this one. §13A step 4 already names the category — it makes the binary
swap conditional on the deploy "changing the game binary's behavior (not a
source/docs-only or **template-hot-reload-only** change)" — but §13B has
no written procedure for that category, so a session reaching for the
script gets an abort and no guidance.

What was actually done, which is the swap step minus the binary:
`cp -r --preserve=mode,timestamps` of `templates/`, `wiki/` and
`public_adventure_overlay/` into `/var/lib/pathofdust/`, then
`chown -R pathofdust:pathofdust`. Took **0.10 s**, and there is **no
downtime at all** — no `systemctl stop`, no restart, `NRestarts` still 0.
Templates hot-reload: `adventure_web/render.rs` holds one process-wide
minijinja `AutoReloader` watching `TEMPLATE_DIR`, and `acquire_env()`
re-checks mtime on every render, so the next page render picks up the new
file. That is the property `live_reload_tests::
editing_a_template_takes_effect_without_a_rebuild` exists to guarantee.

Backups taken before touching anything: `pathofdust-backup.service` ran
green and its archive `pod-backup-20260902-160501.tar.gz` verifies against
its own `.sha256` and contains `adventure-characters.json`,
`adventure-world.json` and `adventure-accounts.json`. A template rollback
slot was added at
`/var/backups/pathofdust/deploy-pre-craft-confirm/` (the whole previous
`templates/`, the previous `patch-notes.json`, and the pre-change
`base.html` sha `f1f23222…`).

VERIFIED BY EFFECT. On the live `/inventory` page fetched as a logged-in
player (the craft card lives there — `/craft` is POST-only and answers
405), **five of the six buttons render `data-confirm`**: Krangle,
Annulment Orb, Chancing, Scour, Hideout Warrior. On the box,
`templates/base.html` has exactly one `document.addEventListener('submit'
…)` and, once `//` comments are stripped, **zero** occurrences of the old
`querySelector('.craft-actions')?.closest('form')` binding — the one raw
match is line 960, the comment that quotes the old binding verbatim so it
is never reintroduced. Same on the HTML the server actually served. No
listener is bound to anything but `document` for submit.

**Divinity could NOT be verified on a live page, and this is stated rather
than glossed.** Its row is gated on holding a Unique Shard token
(`adventure_web.rs:6231`, `craft_token_count(CraftAction::UniqueShard) >
0`) and **no character in World 2 currently holds one** — checked all
nine. So the button cannot render anywhere on live today. Its coverage
rests on `craft_confirm_ui_http.rs`, which renders it on a synthetic
character and asserts the attribute, plus the structural argument that the
delegated listener keys off the SUBMITTER's own `data-confirm`, so it
cannot miss a button it never had to find. That is genuinely weaker
evidence than the other five have, and it is the one action that has never
confirmed once in its life. Re-check the moment any player earns a Unique
Shard.

Health after: active, NRestarts 0, binary unchanged as intended,
`/characters` 200/76,291 B, `/passives` 200/90,170 B, `/inventory`
200/100,412 B, anon `/admin/tunables` 404, anon `POST
/api/commands/join` 404, zero panics, zero template errors, stage 2, two
fights resolved since the refresh. Check 3 in its new equality form: 9
logged at boot, 9 entries in the file, equal.

FOUND — §13B has no procedure for a template-only or asset-only deploy,
even though §13A step 4 names the category. Anyone who follows §13B
literally for such a change gets an abort from `deploy-linux.sh` and no
documented next step. Not fixed here; it is a procedure change and belongs
to whoever owns §13B, not to a deploy session improvising mid-release.
