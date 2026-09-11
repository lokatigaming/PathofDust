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

---

## 2026-09-02 — CRAFTING-COST-CURVE (feature/crafting-cost-curve)

Base crafting costs cut 10x and the per-tier surcharge changed from
`3 x tier` to `3 x tier^1.1`, both as LiveTunables
(`craft_base_cost_mult` 0.1, bounds 0–10; `craft_tier_exponent` 1.1,
bounds 1.0–1.5) on /admin/tunables. Rounding is ceil PER TERM, then sum —
a nonzero base fee can never round away to nothing, and a tier-1 craft
still costs 3 dust even at multiplier 0. Precedent copied throughout:
`pacing::ENEMY_HP_POOL_HARD_CAP` / `admin_tunables_pool_cap_http.rs`.

SELF-CORRECTION — the fit report argued the 10.0 ceiling "restores the
pre-cut prices exactly". It does not: the multiplier scales the UNCHANGED
base constants, so 1.0 is the restore value and 10.0 is ten times the old
prices. Caught by `the_bounds_restore_the_old_curve_exactly` failing.
Bounds unchanged (the owner ruled 0–10); the justification in every doc
comment and the admin hint was corrected.

FOUND — `templates/base.html` carried `var TIER_CRAFT_DUST_COST = 3`, a
second copy of the cost formula the crafting panel previews with. Left
alone it would have quoted the old price while the server charged the new
one. Now parameterised via `data-tier-mult`/`data-tier-exp` on each
button, with `admin_tunables_craft_cost_http.rs` asserting the quoted
price equals the dust actually deducted by a real POST /craft.

FOUND — `AdventureManager::new` runs two one-time craft-token backfills
gated on marker files. Any test that seeds a token-less character in a
fresh data dir gets the tokens handed straight back unless it pre-writes
`adventure-craft-token-backfill{,-v2}-marker.json`.

FOUND — panel Reforge (30 x tier dust) was left out of the cut per an
owner ruling and is now roughly 5x a Scour at tier 10, widening with
tier. On the board as a follow-up, not shipped silently.

PATCH NOTE DRAFT (for the deploy session to paste into
C:/PathofDust/patch-notes.json — not written by this session, which does
not deploy):

  "Crafting Costs" —
  "Crafting is much cheaper. Every craft's base price is a tenth of what
   it was: Transmute and Scour now cost 25 dust instead of 250, Augment
   50, Regal 75, Chancing 80, Annulment 100, Exalt 125, Krangle 250.
   Veiling a craft costs 50 instead of 500."
  "The per-tier part of the price now rises a little faster at higher
   tiers: it was 3 dust per tier, and it is now 3 x tier^1.1. At tier 10
   that is 38 dust instead of 30, at tier 50 it is 222 instead of 150,
   at tier 100 it is 476 instead of 300."
  "Net effect: crafting is cheaper for everyone below roughly tier 120,
   and the deeper you go the more the per-tier part eats into the
   saving. On the most expensive actions the cut still wins well past
   tier 700."
  NERF DISCLOSURE, must stay in whatever wording ships: past the
  crossover tier this is a PRICE RISE, not a cut. Do not describe the
  release as purely cheaper if the live world is past those tiers — see
  the crossover table in this session's report.

FOUND (second instance in one day) — `C:\PathofDust\adventure-world.json`
is the FROZEN pre-cutover Windows install and is NOT production. It is
mtime-fresh (written 2026-09-02 13:15) and reads stage 7380 and
permanent_rampage=true against live World 2's stage 1-2 and
permanent_rampage=false. It has now misled two sessions on the same day;
it is recorded as historical-only in docs/world2_build_plan.md. Rule:
read the live world ONLY from /var/lib/pathofdust on the Debian box. A
local file being recently modified is not evidence that it is live.

---

## 2026-09-02 — CRAFTING-COST-CURVE deploy record (§13B, binary swap)

Shipped: base crafting costs cut 10x and the per-tier surcharge changed
from `3 x tier` to `3 x tier^1.1`, both as LiveTunables.

| | |
|---|---|
| merge commit | `371e941` (`--no-ff` of feature/crafting-cost-curve), verified on origin with `git ls-remote` |
| old binary | `154f13e69f6a2805d645e9eff9cb678e1fa80ff98b5f48208a48033508f942ed` |
| new binary | `ab458dc67c167fb2eb7d79e9d380edf5d0b95108bea059658a0075eaf9e61a86` |
| rollback slot | `/var/backups/pathofdust/deploy-pre-craft-cost-curve/game.pre-craft-cost-curve` |
| downtime | 0.22 s |
| patch notes | new section "Crafting is about ten times cheaper" inserted at the top of the existing September 2, 2026 block (25 blocks, 2 sections in the top one); pre-edit copy at `/root/patch-notes.pre-craft-cost.json` |

Suite on the box: **777 passed / 0 failed**, `cargo test --release
--workspace --quiet`, exit 0. Baseline arithmetic: 755 was the
post-Twitch-removal baseline; this branch adds 9 (8 in
`craft::cost_curve_tests`, 1 in `admin_tunables_craft_cost_http.rs`) and
the local pre-merge run of the branch alone read 764 = 755 + 9, so
master at `a2d75fa` stood at 768 and 768 + 9 = 777.

The first box run failed with `live_reload_tests::
editing_a_template_takes_effect_without_a_rebuild` — the known
flaky-under-parallel test named in CLAUDE.md. Confirmed passing in
isolation (`--test-threads=1`, 1 passed), then the full suite re-run
clean at 777. A `-p game --lib` failure also aborts the run before the
integration binaries execute, so a first-failure count is never the
whole suite's count.

Seven health checks: (1) active; (2) NRestarts 0, unchanged; (3) "loaded
10 characters" against 10 in the characters file — two live numbers, per
the §13B.8 correction, never a literal; (4) live hash equals the
candidate; (5) authenticated `/characters` 77,452 B and `/passives`
90,967 B, both 200; (6) anonymous `/admin/tunables` 404, 73,730 B; (7)
anonymous `POST /api/commands/join` 404. Two fights resolved after the
swap (fight-119 at 16:52:34, fight-120 at 16:55:34, restart 16:49:33).

Verified by effect on production, not only in test:

- live world **stage 4** read from `/var/lib/pathofdust` on the box (it
  was 1 when this session started; the world is advancing). Tier is
  `1 + stage/5` = **tier 1**, hundreds of stages below the tier-122
  crossover, so nothing costs more than it did.
- both dials render with shipped defaults and bounds: `craft_base_cost_mult`
  value 0.1, min 0 max 10, required; `craft_tier_exponent` value 1.1,
  min 1 max 1.5, required. 74 fields in the save form, including all
  five win-XP fields — worktree b's stage gates have NOT landed on
  master yet, so they are not among them.
- four out-of-bounds POSTs (mult -1 and 250, exponent 0.5 and 4), each
  built by scraping the rendered form and changing one field: all **HTTP
  400**, all naming the field, all leaving
  `adventure-live-tunables.toml` byte-identical (sha 95cd4bb506a01c74
  before and after each).
- **preview vs charge, on production.** The live Exalt button rendered
  `data-base="125" data-tier-mult="3" data-tier-exp="1.1"
  data-veil-extra="50"` (1250 and 500 both cut by 10). On a tier-1
  item the panel's own arithmetic quotes 125 + ceil(3 x 1^1.1) = **128
  dust**; the real `POST /craft` moved the owner's dust 282 -> 154,
  **charged exactly 128**. That is the assertion the base.html drift
  would have broken, proven live.
- the shard sentinel is still a sentinel: `POST /craft` with `action=
  unique shard` and no token is refused with "it can't be bought with
  dust" and takes **0 dust**.

The production craft used the owner's own character and additive actions
only — a free Regal token (2 -> 3 modifiers, veil candidate 0) to reach
Exalt's 3-modifier precondition, then the paid Exalt. Nothing was
scoured, krangled or locked.

FOUND — an item's tier is re-synced to the CHARACTER's level on some
paths (`Character::sync_tier_to(level)`), not only set from the world
stage at drop time: the test item read tier 1 before the craft and tier
4 after, while the charge correctly priced the pre-craft tier and
matched the preview. Consequence for reading the cost table: a player's
craft prices climb with their level, not only with the world stage.
Pre-existing behaviour, untouched by this release.

FOLLOW-UPS on the board, not for this release: panel Reforge is still a
flat 30 x tier dust (~5x a Scour at tier 10, widening with tier), and
Recombine's veiled price is still the unscaled 500 + 500 per combined
modifier. Both are now out of step with everything around them.
## Stage-gated drops + craft tokens retired (feature/stage-gated-drops, 2026-09-02)

Rebased onto `3835699` (the Twitch-removal merge) on 2026-09-02. The only
textual conflicts were the two append-only docs, resolved keep-both; the
real work was `start_adventure_web_server` losing four arguments, which
this branch's two new HTTP call sites had to follow. NOT yet rebased onto
`feature/win-based-xp` - that branch was still unmerged at the time of
writing, so its five `LiveTunables` fields have not yet collided with this
branch's four. That rebase is still owed.

Four drops gated on world stage, all four thresholds live-tunable; the
Divine Dust recipe locked behind a one-way stage latch; free craft-token
drops removed entirely.

**Why new tunable fields rather than repurposing `late_content_stage`.**
Perfect items already had a gate on that field at 100, and the order wanted
150. Changing its compiled default would have been INERT on the live
server: `adventure-live-tunables.toml` is a full-struct serialisation and
already carries `late_content_stage = 100` (verified at
`C:\PathofDust\adventure-live-tunables.toml:47`), so a saved value beats a
changed default every time. Four brand-new fields are absent from that file
and therefore take their shipped defaults on the first boot, which is what
"active immediately" actually requires. `late_content_stage` was then
removed outright rather than left as a dial that does nothing.

**Current stage for drops, high-water mark for the recipe.** Per the owner's
ruling the four drop gates read the live `WorldState::stage`, so a boss-loss
regression really does pause them. The recipe latch reads a new
`highest_stage` field instead, `#[serde(default)]` with a `max(stage)`
backfill at load — without that backfill an already-past-300 server would
have loaded `highest_stage: 0` and re-locked a recipe its players had
earned. The backfill is not marker-guarded because, unlike the one-time
character grants, it is idempotent.

**The disenchant route is deliberately porous** (owner ruling: fight grants
only). Below stage 100 a player still earns sand by disenchanting gear:
`roll_disenchant_sand` hits with probability `quality_percent/100` for 1-3
sand. `power_roll` is uniform over `POWER_ROLL_RANGE` (0.85..1.2), so mean
quality is 50% and the route yields ~1 sand per item disenchanted against
4.5 per boss win and 2 per filler win. Real, but roughly a quarter-rate and
only for a player actively breaking gear down. Auto-disenchant is off by
default, so it is opt-in on top of that.

**Why the boundary tests run at lowered thresholds.** Boss difficulty is
driven by the same `stage` the gates read, and every gate only fires on a
WIN. At stage 300 a test character cannot reliably win — `BOSS_DEFENSE_CAP`
evasion/block/DR plus the 90s fight cap — and a lost fight would have
satisfied every "below the gate" assertion for entirely the wrong reason: a
false pass. `stage_gate_tests` therefore pins the four gates to 8/11/14/17
(deliberately distinct, so a copy-paste bug pointing two gates at one field
fails) and tests `T-1`/`T`/`T+1` there, while the shipped 100/150/300/300
are pinned directly by `the_shipped_gate_defaults_are_the_ordered_numbers`
and end-to-end by `tests/admin_tunables_stage_gates_http.rs`.

FOUND — a fight's post-fight revival bookkeeping is spawned, so under a
loaded test runner it can land after a test has cleared `downed_until` and
leave the character ineligible for the very next tick (`NobodyJoined`).
Reproduced only under the full parallel suite, never in isolation. Worked
around in this module's own helpers by retrying the encounter; not
investigated further and not touched in production code.

FOUND — `BOSS_CRAFT_PITY_GAIN`/`BASIC_CRAFT_PITY_GAIN` are now read by
nothing but `adventure_web/wiki.rs:342-343`, which renders them into the
wiki's pity table. The game no longer has a craft-token pity payout at all,
so that table documents a mechanic that no longer exists. Flagged in
WIKI_IMPACT.md; not fixed here (wiki module is another session's).

These two are now in EXACTLY the position the Twitch removal left
`ACTIVITY_XP_COOLDOWN`, `ACTIVITY_XP_AMOUNT` and `RAMPAGE_VOTE_THRESHOLD`
in (see the "RETAINED WITHOUT A CALLER" doc comments and this journal's
Twitch-removal entry): alive only because `wiki.rs` reads them, each
rendering a real number for a mechanic that no longer runs. **All five
should die together, in the same change that removes the wiki sections
they feed.** Deleting any of them before the wiki stops rendering it just
breaks a file neither session may edit alone.

### Patch-notes entry, drafted and NOT yet applied

Deploy step 1 (REFACTOR_PLAN §13) is the deploy session's, and this session
stops before deploy. Paste this as the newest entry at the TOP of
`patch-notes.json`:

```json
{
  "date": "September 2, 2026",
  "sections": [
    {
      "heading": "Drops Now Start At A World Stage",
      "items": [
        "Polishing sand now starts dropping from fights at world stage 100. Below that, winning a fight grants none — but disenchanting gear still gives sand at any stage, so early players are not cut off entirely.",
        "Perfect items now start dropping at world stage 150. This is a nerf: they used to start at 100.",
        "Divine Dust now starts dropping from fights at world stage 300. As with sand, disenchanting a Sacred item can still grant it at any stage.",
        "Sacred items still start at world stage 300 — unchanged, just no longer hardcoded.",
        "All four follow the CURRENT stage. If the group loses bosses and the world slips back below a threshold, those drops pause until you climb back above it."
      ]
    },
    {
      "heading": "The Divine Dust Recipe Has To Be Unlocked",
      "items": [
        "The Craft Divine Dust recipe on /craft is locked until the group reaches world stage 300. Until then the row shows what it needs instead of a Craft button.",
        "Once the group has reached stage 300 the recipe stays unlocked permanently. A bad boss streak that pushes the world back down cannot take it away."
      ]
    },
    {
      "heading": "Free Crafting Tokens No Longer Drop",
      "items": [
        "Crafting tokens no longer drop from fights, and the token pity counter is gone with them. This is a nerf.",
        "The only crafting tokens in the game are now the starter set every new character receives — one each of Transmute, Scour, Augment, Regal, Exalt, Krangle, Annulment and Chancing. That grant is unchanged, and tokens you already hold are untouched.",
        "Unique Shards are NOT affected. They are a separate currency with their own drop, they still drop at the same rate, and Divinity and the Unique Affix picker are unchanged."
      ]
    }
  ]
}
```

---

## 2026-09-02 — BOT-DECOUPLING (branch `chore/bot-decoupling`)

The bot no longer speaks to the game. Scope was the root crate only
(`src/**`); `game/**` was not touched.

**Deleted.** `src/adventure_client.rs` (`AdventureApiClient` and all its
methods) and `src/published_constants.rs`, both modules whole. From
`main.rs`: `handle_reforge_redemption`, `handle_repair_redemption`,
`handle_force_boss_redemption` and their three `*_redemption_action`
decision fns plus the `RedemptionAction` struct, `adventure_integration`,
`adventure_rewards_enabled`, the three adventure reward creations, the
three dispatch arms, the SSE announcements relay loop, the
fire-and-forget `activity_xp` spawn, and the whole `mod tests` (all ten
of its tests were adventure-only). From `commands.rs`: ten match arms
covering sixteen trigger words, the nine public help rows, the adventure
entries in `BUILTIN_NAMES`, `adventure_reply`, `ADVENTURE_DOWN_REPLY`,
`handle_event_command`, the `Services.adventure` field, and its `mod
tests` (three tests, all `adventure_reply`). From `channel_points.rs`:
three of the five `ensure_*_reward` fns with their titles and prompts.
From `eventsub.rs`: three subscription blocks, three parameters through
three function signatures, three log branches. From `config.rs`:
`adventure_api_base_url`, `adventure_api_secret` and the three
`channel_points_*_reward_cost` fields for reforge/repair/force-boss,
with their env reads.

**Three of the order's premises were wrong, and all three were checked
against the code before anything was deleted.**

1. `reconcile_missed_redemptions` was to be deleted. It reconciles FIVE
   rewards and two of them survive - "Set Entrance Theme Song" and
   "Interrupt the Music", both pure Twitch/OBS work backed by
   `EntranceThemeManager`/`SongRequestManager`, neither touching the
   adventure client on any path. Deleting it would have silently ended
   backlog reconciliation for two live rewards, so a redemption made
   while the bot was down would sit UNFULFILLED forever. REDUCED from
   five reward ids to two instead.
2. `src/bug_reports.rs` was believed dead because the game "has its own
   port" and because `!bugreport` was believed to be one of the
   adventure arms. Neither holds. `grep -rn "BugReport" game/src
   game/tests` returns nothing on master - that port is Piece 3 on an
   unmerged branch. And `!bugreport`/`!bugreports` call
   `services.bug_reports`, a bot-local file-backed manager writing
   `bugreports.json`; no adventure client is involved. KEPT whole.
3. "Eleven adventure command arms" is ten. Ten arms guard on
   `services.adventure`; they cover sixteen trigger words, which is
   probably where eleven came from.

**The removal-scope audit was wrong for the sixth time today**, again in
the direction of misstating what the code actually holds - this time an
overcount of arms on top of the two false-death claims above. It is not
a usable target list. Every deletion here was verified against the code
first, and the closing grep below is the proof rather than a claim.

**Survivor grep** (`adventure`, `redemption`, `channel_point`,
`api_secret`, `activity_xp` over `src/`): `api_secret` and `activity_xp`
return ZERO. `adventure` returns twelve, all legitimate - nine are the
three vestigial config fields (`adventure_overlay_server_port`,
`adventure_web_port`, `adventure_web_public_url`) plus their docs, and
three are this change's own explanatory header in `lib.rs`. `redemption`
and `channel_point` survive only in the two rewards that stay, the Helix
API that serves them, and the `channel:manage:redemptions` OAuth scope in
`bin/auth.rs`. No survivor is a miss.

**A variable read that no longer exists anywhere:**
`ADVENTURE_WEB_PUBLIC_URL`. `config.rs` still reads it into
`adventure_web_public_url`, which nothing in the bot consumes, and
`game/src/main.rs:11` records that as of 2026-09-02 the game reads it
nowhere either. `ADVENTURE_OVERLAY_SERVER_PORT` and `ADVENTURE_WEB_PORT`
are the same shape bot-side - read, never used - but both are still live
game-side. These three are the prior audit's L24 "vestigial config"; they
were NOT in the order's deletion list and were deliberately left rather
than folded in, since they predate the seam and removing them is a
separate call.

FOUND - when the player-facing batch merges, the game gains its own web
bug report while the bot keeps its chat one, giving the project two
independent bug inboxes writing two files. Worse than one. Flagged for a
ruling at that merge; deliberately not resolved here.

FOUND - `README.md:11` still tells readers viewers play by typing
`!join`. That went stale when Twitch was removed from the game, not here.
Not touched: it is the game's README and other sessions are in flight.

**Tests.** `CARGO_TARGET_DIR=C:/dust-work/target-botdecouple cargo test
--release --workspace --quiet` - baseline on `a2d75fa` was 768 passed / 0
failed / 0 ignored across 33 suites; after, 755 / 0 / 0 across 33.
768 - 13 = 755, and the 13 are exactly the two deleted test modules (ten
in `main.rs`, three in `commands.rs`). No suite disappeared. Clippy on
the same target dir is unchanged on touched code: the bot binary's only
two warnings (`too_many_arguments` on `handle_interrupt_redemption`,
`trim_split_whitespace` at the chat-command split) both sit on lines
byte-identical to `origin/master`, neither of which this branch edited.

**The bot's environment after this** needs exactly `TWITCH_CLIENT_ID`,
`TWITCH_CLIENT_SECRET` and `TWITCH_CHANNEL` as hard requirements, plus
whichever optional keys are wanted. `ADVENTURE_API_SECRET`,
`ADVENTURE_API_BASE_URL`, `CHANNEL_POINTS_REFORGE_REWARD_COST`,
`CHANNEL_POINTS_REPAIR_REWARD_COST` and
`CHANNEL_POINTS_FORCE_BOSS_REWARD_COST` are now read by nothing and
should come out of `C:\PathofDust\.env`. `.env.example` never carried any
of the five, so it needed no edit.

**Docs.** Dated, append-only records were left alone rather than
rewritten (this journal, the anomaly ledger, WIKI_IMPACT.md, the
removal-scope doc). Four documents that described the seam as live got a
dated SUPERSEDED IN PART banner instead of an inline rewrite -
`docs/bot_decoupling_audit.md`, `docs/platform_portability_audit.md`,
`docs/cutover_runbook.md` and REFACTOR_PLAN.md section 4 - because each
is a long dated report whose body is still an accurate record of its own
moment. `docs/world2_build_plan.md` had its two-step correction marked
DONE and SUPERSEDED in place. `game-watchdog.ps1`'s port comment no
longer claims 4005 is what the bot points at. The `docs/linux_*.md` files
already said the secret was absent and needed nothing.

NOT DEPLOYED, by instruction. The deploy analysis is in the session
report.

**Three owner rulings applied after the first commit (`c70222e`), before
the merge.**

1. **The three vestigial adventure config fields are gone after all** -
   `adventure_overlay_server_port`, `adventure_web_port` and
   `adventure_web_public_url`, with their docs and their env reads. The
   first commit left them deliberately, on "add nothing that was not
   asked for" grounds. Overruled, and correctly: a config field with zero
   consumers is the same defect class as a comment that says a credential
   is required when it is not - it tells the next reader something false.
   That two of the three env vars are still live on the GAME side is
   irrelevant to the bot, which is a different binary on a different box.
   `grep -rni adventure src/` now returns two hits, both inside this
   change's own explanatory header in `lib.rs`.
2. **`README.md:11`** no longer tells readers viewers play by typing
   `!join`. It now says they join from the web dashboard, and records
   when the chat command went and why.
3. **REFACTOR_PLAN.md 13A's bot start-ordering clause is amended**, with
   the date and the reason inline. It required the operator to "start
   `TwitchBotRS` only after `GameProcess` is confirmed healthy". There
   has been no `GameProcess` task on the Windows box since production
   moved to Debian, and after this change the bot never contacts the game
   at all, so the ordering had nothing left to order. It now reads: the
   bot's start is unordered with respect to the game; verify port 4001
   only.

**Patch notes: deliberately none, on the owner's explicit instruction**
("No patch note - this changes nothing a player sees"). This is a
knowing deviation from 13A step 1, which asks internal-only releases for
a one-line `Internal:` entry so the record stays unbroken. Recording it
here so the gap in `patch-notes.json` is explained rather than looking
like an omission. The one arguably player-visible effect is that
lokati.net/commands.html stops listing nine adventure commands that have
answered nothing since cutover.

---

## 2026-09-02 — Stage-gated drops deploy record (binary swap, 0.16 s downtime)

Merge `25989ea` into master, deployed to the Debian box as release
`stage-gated-drops`. Four drops gated on world stage as LiveTunables, the
Divine Dust recipe locked behind a one-way latch, free craft-token drops
retired.

| | |
|---|---|
| merge commit | `25989ea` |
| binary before | `ab458dc67c167fb2eb7d79e9d380edf5d0b95108bea059658a0075eaf9e61a86` |
| binary after | `58972241ba06422fa06e870c047cf2a495ac3a650600b588203cb254b3c19a80` |
| downtime | 0.16 s |
| suite on the box | 788 passed / 0 failed |
| rollback slot | `/var/backups/pathofdust/deploy-pre-stage-gated-drops/game.pre-stage-gated-drops` |
| backup archive | `pod-backup-20260902-172120.tar.gz` |

**Master moved seven times during this session.** `3835699` (Twitch
removal) → `02da915` (win-based XP) → `642504d` → `f04ea49` → `a2d75fa` →
`371e941` (crafting cost curve) → `e1a369c` (orphaned docs). Three pushes
were rejected by `--force-with-lease`/non-fast-forward and redone. Every
check used `git ls-remote`, never a local ref. **The lesson worth keeping:
on a day like this, the gap between "I confirmed master" and "I pushed" is
itself long enough for master to move — so confirm immediately before the
push, and treat a rejection as the system working, not as an obstacle.**

**`master` is checked out in the stale `C:/PathofDust` worktree** (still at
`eab55f9`), so `git checkout master` fails in any dust-work worktree and
local `refs/heads/master` is permanently stale. Merges are therefore done on
a **detached HEAD off `origin/master`**, pushed with `git push origin
HEAD:master`. That touches neither the Windows deployment root nor the local
branch ref. Any session that needs to merge should do the same rather than
trying to fix the worktree.

**One real cross-merge failure, caught by the suite.**
`admin_tunables_craft_cost_http.rs` (from `feature/crafting-cost-curve`,
which merged first) asserts the Divine Dust recipe FORM is on the crafting
panel, and a locked recipe deliberately renders no submittable form. Its
scratch world sits at stage 0, so it 100% failed against this merge. Fixed
in the merge commit by seeding its scratch world unlocked — the same
one-line seed `divine_dust_craft_http.rs` and `divine_dust_ui_http.rs`
already carry. **Its own assertion was not weakened.** General shape worth
naming: *any* test that renders `/inventory` or `/craft` and expects the
Divine Dust row to be interactive now needs an unlocked world, and the
cheapest way to get one is
`{"stage":300,"last_boss_kind":null}` written to the scratch world file —
which also exercises the `highest_stage` backfill for free.

### Verified by effect on production, not by code trace

Live world read from the BOX (`/var/lib/pathofdust/adventure-world.json`),
never from the frozen `C:\PathofDust` artifact.

- **`highest_stage` backfill worked in production.** The field was ABSENT
  from the live world file at swap time (stage 4). After the first fight the
  file carries `highest_stage: 5`. Had the backfill not run, it would have
  been written as `1` and the Divine Dust recipe would have been re-lockable
  by regression on a server that had legitimately climbed.
- **All nine new tunables render** on the live `/admin/tunables`: the four
  stage gates at 100/150/300/300 and the five `win_xp_*` fields, each with
  `min`, `max` and `required`. `late_content_stage` is gone from the page.
- **Three out-of-bounds POSTs** (`100001`, `4294967295`, `999999`) were each
  refused `400 NOT SAVED`, naming the field and the range, with
  `adventure-live-tunables.toml` byte-identical (sha256 unchanged) before
  and after all three.
- **The saved `adventure-live-tunables.toml` still contains
  `late_content_stage = 100`**, now an unknown key that serde ignores. This
  is the live vindication of shipping four NEW fields instead of
  re-defaulting that one: had the Perfect gate stayed on
  `late_content_stage`, production would have kept gating Perfect at 100
  and the ordered move to 150 would have silently done nothing.

### The negative proof — one real boss win at stage 4

All four gates and the token removal sit far above the live stage, so a win
must award dust and XP and **nothing else**. Triggered one encounter via
`/admin/ops/next-encounter`; it was a win (stage 4 → 5), all 12 characters
participated.

| Awarded | Total across the roster | Expected | |
|---|---|---|---|
| polishing sand | 0 | 0 | PASS |
| divine dust | 0 | 0 | PASS |
| perfect items | 0 | 0 | PASS |
| sacred items | 0 | 0 | PASS |
| craft tokens | 0 | 0 | PASS |
| `craft_pity` movement | none | none | PASS |
| dust | +114 | >0 | PASS |
| XP | +208 | >0 | PASS |

The dust and XP rows are the ones that make this a real test rather than a
tautology: they prove the fight actually paid out, so the five zeros are
gates holding rather than a fight that did nothing. Two characters show
negative XP deltas — `jachiny` 4→5 and `kuokiz` 2→3 — which is a level-up
consuming the bar, not lost XP.

**Starter tokens still granted.** No character had been created under the
new binary (last registration was six minutes pre-swap), so this was proven
on a disposable instance of the DEPLOYED binary — same
`/opt/pathofdust/bin/game`, `GAME_DATA_DIR` pointed at a scratch dir, ports
4104/4105, production data untouched. A freshly registered character
received all eight starter tokens, one each, with `craft_pity` at 0. The
instance was stopped by PID after confirming its PID differed from
`systemctl show pathofdust -p MainPID` and its cwd was not the production
data dir — never by image name, per the house rule.

**Locked recipe, live at stage 4:** `/inventory` renders
`Craft Divine Dust — unlocks at stage 300` with **no** `value="divine dust
craft"` form on the page, and a hand-crafted POST to `/craft` is refused
server-side, redirecting to `craft_failed=The Divine Dust recipe unlocks
when the group reaches stage 300…` with no currency spent.

### Health (§13B.5, all seven)

1 `active` · 2 `NRestarts 0` · 3 journal `loaded 12 characters` = file `12`
· 4 live hash equals the candidate · 5 `/characters` 200 / 78,105 B,
`/passives` 200 / 91,032 B · 6 anonymous `/admin/tunables` **404**,
73,730 B · 7 `POST /api/commands/join` **404**. Zero panics and zero
error-level journal lines since the swap.

FOUND — `/admin/ops/next-encounter` requires
`Content-Type: application/x-www-form-urlencoded` even with an empty body; a
bare `curl -X POST` gets `415`. Harmless from a browser, a trap from the
command line. Not changed.
## 2026-09-02 — BOT-DECOUPLING deploy record (merge `4abecde`)

First bot-only deploy since production moved to Linux, and the first
exercise of 13A's bot path with no game on the box.

**Merged twice, because master moved twice underneath.** Base was
`a2d75fa`; by the time the branch was verified master was `e1a369c`
(crafting cost curve + orphaned-docs recovery), and by the time THAT
merge was verified it was `5c116c3` (stage-gated drops). Neither touched
`src/**`, so neither collided: `git diff --name-only origin/master..HEAD`
on the final state lists seventeen files, all of them the bot crate or
docs, and zero under `game/**`. Both merges conflicted only in
`WIKI_IMPACT.md` and `docs/session_journal.md`, both resolved keep-both.
The resolution was checked rather than eyeballed: `git diff` against each
parent showed additions only, zero removed lines on either side.

**Test arithmetic.** Measured on `e1a369c`: 777 passed / 0 failed / 0
ignored, 34 suites. Measured on the final merged tree `4abecde`: **775
passed / 0 failed / 0 ignored, 35 suites**
(`CARGO_TARGET_DIR=C:/dust-work/target-botdecouple cargo test --release
--workspace --quiet`, exit 0). Master's own baseline at merge time
(`5c116c3`) is therefore 775 + 13 = **788** — derived, not measured, and
the derivation is closed rather than assumed: the merged tree is exactly
`5c116c3` plus a diff containing no `game/**` file, so no game test can
have moved, and the 13 are pinned by the suite listing (the bot lib's
`running 3 tests` and the bot bin's `running 10 tests` both became
`running 0 tests`; the count of empty suites went 5 -> 7). Master itself
gained 11 between `e1a369c` and `5c116c3` (game lib 730 -> 740, one new
test binary), which reconciles 777 + 11 = 788. Master was not re-measured
at `5c116c3` because a third baseline run would have raced a fourth push.

Clippy exit 0, zero errors. The bot binary's only two warnings
(`too_many_arguments` on `handle_interrupt_redemption`,
`trim_split_whitespace` in the chat-command split) sit on lines
byte-identical to the pre-branch master and were untouched here.

**Binary swap.** Old `FA9BB513…` (17,614,336 B, 2026-08-29), new
`DCC1DAFA…` (17,283,584 B) — 330,752 bytes smaller, which is the deleted
code. Backed up to `C:\PathofDust\backup-pre-bot-decoupling\` (both
`twitch-bot-rs.exe` and `twitch-bot-rs.exe.pre-bot-decoupling`, backup
hash confirmed equal to the old live hash before the copy). Cargo did not
relink the bot between the two merge commits; that this is correct rather
than a stale artifact was proved with `git diff f52899a..4abecde -- src/
Cargo.toml Cargo.lock`, which is empty.

**Watchdog handled as a lease, not a switch.** `-Target Bot -Set`
confirmed with `-Status` printing `scope : this IS the flag
'TwitchBotRS-Watchdog' reads` BEFORE anything was stopped, and cleared
after the health check, confirmed absent. The bot was stopped via
`Stop-ScheduledTask -TaskName TwitchBotRS`, never by image name; the
running PID (19844) was resolved first and its `ExecutablePath` asserted
to be under `C:\PathofDust` before the stop, and confirmed gone after.
`TwitchBotRS-Watchdog` has run twice since (23:30, 23:40) and restarted
nothing; `watchdog.log`'s last entry is still 2026-08-19.

**13A's cross-binary start ordering was amended in this release, not
worked around.** It required starting the bot only after `GameProcess`
was healthy. `GameProcess` and `GameProcess-Watchdog` are both *Disabled*
on this box and there is no game here to check, so the step could not be
performed as written. Rewritten with the date and reason in place; old
wording preserved inside the amendment.

**THE SYMPTOM IS GONE, MEASURED BOTH SIDES.** Before: 3,585 of the day's
4,759 bot log lines — 75% of everything the bot wrote — were
`Failed to open the adventure announcements stream … 404`, running at 108
lines per 10 minutes. After, over an 11.5-minute window on the new
binary: **0 adventure lines, 0 WARN, 0 ERROR**, 30 lines total and every
one of them real work.

**Verified live, by real viewers, not by contrivance.** In that same
window the surviving bot exercised itself: an entrance theme fired for
Kalashuddin; `!song` answered; `!playlist <user>` queued five songs each
for three different people; `!vs` ran a full vote-skip to completion
(1/3, 2/3, "Vote to skip passed!"); a malformed `!vs#` was handled
without incident; both hourly PoE pricing sheet syncs ran. All three OBS
ports serve 200 (4001 alert box 18,924 B, 4002 overlay 16,385 B + dock
10,729 B, 4003 chat overlay 4,359 B). Chat connected, 388 emotes loaded,
OBS WebSocket identified, StreamElements and PayPal watchers started.
EventSub now reports exactly two redemption subscriptions — "entrance-
theme redemptions and Interrupt the Music redemptions" — where it used to
report five, which is the cleanest single line of evidence that the three
adventure subscriptions are gone and the two survivors still work.

`commands-data.json` regenerated at startup: 168 entries, **zero**
occurrences of `join`, `character`, `rampage`, `pinfight` or `giftdust`,
and `bugreport` still present. lokati.net/commands.html no longer
advertises nine commands that answered nothing.

**NOT verifiable without the owner, stated rather than glossed:** nobody
redeemed a channel-point reward in the window, so the theme and interrupt
handlers are proven subscribed but not proven end-to-end; no follow, sub,
raid or tip arrived, so the alert box is proven to serve its page but not
to fire an alert; no `!votevolume` was used, so the OBS fader path is
unexercised. Each is unchanged code on a path this release did not touch,
which is an argument, not a measurement.

The three orphaned `channel-points-{reforge,repair,force-boss}-reward.json`
were deleted from `C:\PathofDust` after the swap; the theme and interrupt
files remain.

**STILL OUTSTANDING, OWNER ONLY.** The three rewards remain live in the
Twitch dashboard. A viewer can still spend points on Reforge Gear, Repair
All Gear or Force Boss Fight and now gets nothing at all — no handler, so
no fulfil and no refund, points simply consumed. No code in this repo can
retire them. This is the one real player-facing harm in the current
state and it can only be fixed by hand in the Twitch dashboard.

**Patch notes: none, on the owner's explicit instruction.** A knowing
deviation from 13A step 1's "internal-only releases get a one-line
`Internal:` entry". Recorded so the gap is explained rather than
mistaken for an omission. The commands.html change above is the one
effect that arguably crosses into player-visible.

`.env` on the box still carries `ADVENTURE_API_SECRET`,
`ADVENTURE_API_BASE_URL` and `ADVENTURE_WEB_PUBLIC_URL`, all three now
read by nothing in either binary. They are inert, not harmful, and were
left for the owner rather than edited during a deploy window.
FOUND, same shape, not acted on: `PATREON_CLIENT_ID`,
`PATREON_CLIENT_SECRET` and `PATREON_POLL_INTERVAL_MS` are also still
there although Patreon was removed on 2026-08-28.

---

## 2026-09-03 — OPS-HARDENING (branch `chore/ops-hardening`)

Four pieces, all outside `game/src` by design so the four game sessions
in flight were not disturbed. Branch cut from `2cf9a59`, confirmed with
`ls-remote`.

### Piece 1 — the pre-reset check (`docs/world_reset_procedure.md`)

New step: **enumerate everything ratified but unbuilt, and rule on each
one, before the world opens.** Written in the document's existing shape —
what it is, what actually happened, the step, the check, why a check and
not an instruction — and with the incident attached, because that is what
makes a step get followed.

The incident is stated plainly: World 2 opened on the pre-ratification
item scaling because `docs/affix_curve_spec.md` — 3,008 owner-ratified
lines covering the affix tier curve, four new gear slots and the
crit-multiplier halving — sat on a branch whose final commit message read
"branch CLOSED" and was never merged. Master carried a four-word stub for
ten days; a session searched master, found nothing, and recorded that the
work did not exist. The world is over-tuned by exactly the amount the
curve and the halving were ratified to remove, and it surfaced only
because the owner remembered a stray line about base items.

The step is executable rather than aspirational. Three sweeps, each with
its command: every branch not an ancestor of `origin/master` (via
`git merge-base --is-ancestor`, the sweep that would have caught this —
and the step says in terms that a branch's own "CLOSED" message is not
evidence its contents reached master); every document on master that
ratifies something, checked **against code, not against another
document**; and `world2_build_plan.md` §5's deferred list plus §7's open
rulings. The output is a table committed to this journal **before** the
reset runs, one row per item, with a decision of exactly BUILD, DEFER or
DROP against every row and no blanks. DEFER and DROP are owner rulings; a
session may recommend and may not decide. A DEFER must say what the world
will be like without the item in player terms. An empty table is itself a
finding that has to be defended.

The branch sweep was dry-run while writing it and returns 8 branches
today, so the command in the document is one that has actually been
executed rather than one that looks right.

### Piece 2 — the stale `C:\PathofDust` checkout: REPORTED, NOT TOUCHED

Held for the owner's ruling per the order. Findings are in the session
report. Two corrections to the order's premises, both verified:
`C:\dust-work\c` and `C:\PathofDust` are **independent clones, not
worktrees of one repository** (`git worktree list` in each shows only
itself; both have a real `.git` directory), so the stale checkout cannot
be what forces a session to work detached — this session did `git
checkout -B` on a branch off master with no trouble. And the dirty
entries were **14, not 13** at the start of the session, every one of
them untracked (`??`), with zero modified tracked files.

### Piece 3 — six dead keys removed from `C:\PathofDust\.env`

`ADVENTURE_API_SECRET`, `ADVENTURE_API_BASE_URL`,
`ADVENTURE_WEB_PUBLIC_URL`, `PATREON_CLIENT_ID`, `PATREON_CLIENT_SECRET`,
`PATREON_POLL_INTERVAL_MS`. Proven unread first rather than assumed: the
complete set of keys each binary reads was extracted from its own
`env_var*` call sites, and none of the six appears in either. The only
textual hit on any of them anywhere in the tree is a comment in
`game/src/main.rs:11` stating that `ADVENTURE_WEB_PUBLIC_URL` is read by
nothing.

Backed up first to `C:\dust-work\PathofDust-env.bak-2026-09-02-ops-hardening`,
hash-verified identical, and deliberately **outside every git checkout**
so a secrets file is never sitting untracked inside a repo. 29 keys
before, 23 after; no value was printed at any point.

Verified by effect, not by reading the file back: the bot was restarted
under its maintenance flag and came up clean on the trimmed environment —
all three ports serving (4001 18,924 B, 4002 16,385 B, 4003 4,359 B),
chat connected, 388 emotes, OBS WebSocket identified, EventSub subscribed
with its two surviving redemptions, both hourly pricing syncs run. The
watchdog has run since (00:06:19) and did **not** restart it; PID 37024
is unchanged and `watchdog.log`'s last entry is still 2026-08-19.

**A process error worth recording, because it could have taken the bot
down.** After `Stop-ScheduledTask` I waited 4 seconds, saw the old PID
still alive, and called `Start-ScheduledTask` anyway instead of stopping
to wait. The task's `MultipleInstances` policy is `IgnoreNew`, so the
start would have been silently **ignored** had the old process really
still been running — leaving no bot at all, with its watchdog suppressed
by my own flag. The old process happened to exit in the gap and one
healthy instance came up. The correct shape is to loop until the PID is
gone and abort if it never is; the 12-second wait used in the
bot-decoupling deploy was adequate and 4 was not.

`OPERATOR_LOGIN` was left in place. It is inert on this box, but unlike
the six it is read by real code (`game/src/main.rs`) on the Debian box,
so "nothing reads it" is not true of it in general.

### Piece 4 — orphaned state files

**Answer to the headline question: no reward JSON files remain beyond the
two that are still live.** `channel-points-theme-reward.json` and
`channel-points-interrupt-reward.json` are both read every startup by
`ensure_reward`. There is no `patreon-*.json` on the box at all.

Two genuine orphans found and deleted, copies kept at
`C:\dust-work\PathofDust-orphans-2026-09-02\`:

- `announcements.json` (567 B, last written 2026-08-06) — zero code
  references in either crate; `announcements.rs` fetches from
  `ANNOUNCEMENTS_URL` over HTTP and takes no path at all. A leftover from
  the Node bot. `docs/platform_portability_audit.md` §13 reached the same
  conclusion independently on 2026-08-27 and did not act on it.
- `verify_status.txt` (27 B, 2026-08-24) — contains `build_exit=0` and
  `test_exit=0`. A past session's scratch file. Zero references anywhere.

**Deliberately NOT deleted, and this is the important half.** Every
`adventure-*.json` / `adventure-*.toml`, `patch-notes.json` and
`bot-published-constants.json` is an orphan by the letter of "its writer
no longer runs here" — `game.exe` runs on Debian now. They are also the
**frozen pre-cutover World 1 snapshot**, they are still read by live game
code on the other box, and `bot-published-constants.json` is in
`backup-game-data.ps1`'s manifest. "Nothing writes it here" is not
"nothing reads it", and it is nowhere near "safe to delete". They stay.

Also added `/backup-pre-bot-decoupling` to `.gitignore` — that directory
was created by yesterday's deploy and left untracked, which is one of the
dirty entries Piece 2 is about. Deleting `verify_status.txt` and ignoring
that directory takes the checkout from 14 untracked entries to 12 once it
is next updated.

FOUND — `personal_playlists` sync to Apps Script is intermittently
failing, and it predates everything in this session: HTTP 404 twice at
05:04 on 2026-09-02, and a connection error at 16:00:39 after the
restart. `PLAYLIST_SYNC_SECRET` was not touched here. The bot's local
playlist data and `!playlist <username>` keep working; only the public
site falls behind. Not investigated.

FOUND — the owner added a static `!join` command in chat at 23:49 on
2026-09-02 pointing players at `https://adventure.lokati.net/` to
register. That is the decoupling working as designed: the builtin arm is
gone, the name falls through to the static-command table, and the owner
can point it wherever they like without a deploy.

### Piece 2 — EXECUTED, and two additions the owner ordered on top

**`C:\PathofDust` updated**, `eab55f9` → `2cf9a59`, 33 commits, clean
fast-forward with no conflicts. Verified afterwards that nothing live
moved: bot PID **37024 unchanged**, one instance, all three ports serving
byte counts identical to before (4001 18,924 B, 4002 16,385 B, 4003
4,359 B), the deployed binary still `DCC1DAFA…`, `.env` still 23 keys.
No restart, as established — every runtime path is gitignored, so the
merge could not reach the binary, the environment or any state file.

`.clinerules` re-copied from `CLAUDE.md` and confirmed byte-identical
(13,915 B, sha `9740FE35…`). It had genuinely diverged: the update
brought in the two house rules added on 2026-09-02 (append-only defined,
and branch closure / no silent orphans), so Ox sessions were a revision
behind until this copy.

Untracked entries: 14 at session start → 13 now.

### ADDITION 1 — the account store was one `git add -A` from a public remote

**This is the most consequential thing this session found, and it reached
the owner as a row in a table.** It was written up as one line of a
dirty-entries inventory — `adventure-accounts.json | pre-cutover account
store` — sitting among sprite art and rollback backups, ranked by nothing
and flagged as nothing. The owner picked it out of that table. A finding
about credential exposure does not belong in a column; if something is
the most serious thing on a page it has to be the loudest thing on the
page, and this was the quietest.

**What it is.** `adventure-accounts.json` holds usernames and argon2
password hashes for every account registered in World 1. It was
**untracked but not ignored** in a git repository whose remote is a
public GitHub repo. Nothing had committed it — `git ls-files` confirms it
has never been tracked, so it is not in history — but nothing prevented
it either. A single `git add -A` by any session or any person puts
credential material on a public remote permanently.

**Why it happened, which matters more than the fix.** `.gitignore` lists
runtime state files **one at a time**. `adventure-accounts.json` shipped
with operator identity (Stage 3a, 2026-08-28); its sibling
`adventure-sessions.json` was added to the ignore list in the same change
and **the accounts file was simply missed**. Nothing fails when an entry
is forgotten: the file works, the bot works, the game works, and the only
symptom is a `??` in `git status` that reads exactly like the sprite art
two lines below it. It sat that way for six days.

**Fixed** in the same commit as the `/backup-pre-bot-decoupling` entry,
placed next to `adventure-sessions.json` so the two are found together,
with the trap written into the file: this list is per-file, so a new data
file needs its line in the same commit that introduces it.

**The sweep, so the answer is a measurement rather than a reassurance.**
All 65 untracked files in the deployment root were enumerated and every
sensitive candidate was checked for both tracked and ignored status:

| File | Tracked | Ignored before | Action |
|---|---|---|---|
| `adventure-accounts.json` | never | **NO** | now ignored |
| `Quick Notes.md` | never | **NO** | now ignored (owner's personal notes) |
| `.continue/` | never | **NO** | now ignored (same class as `.clinerules`) |
| `adventure-sessions.json` (session tokens) | never | yes | — |
| `tokens.json` (Twitch OAuth) | never | yes | — |
| `.env` | never | yes | — |
| the other 10 bot state files | never | yes | — |
| `adventure-characters.json`, `patch-notes.json` | never | yes | — |
| `*.json.<pid>.<n>.tmp` atomic-write temps | never | yes (`*.tmp`, line 231) | — |

**Nothing sensitive is or has ever been tracked**, so there is nothing to
purge from history. The remaining 62 untracked files are sprite art (55),
rollback binaries (5) and scratch notes.

**The protection is not live yet, and saying otherwise would be false.**
`C:\PathofDust` now sits at `origin/master`, which does not carry this
branch. Until `chore/ops-hardening` merges, `adventure-accounts.json` is
still untracked-and-committable in the deployment root. Merging is what
closes the window, not this commit.

### ADDITION 2 — REFACTOR_PLAN §13A's stop step, rewritten from a near-miss

The old text was three words — "confirm it exited" — and this session
read them as "sleep a few seconds and glance": 4 seconds, saw the old PID
still alive, called `Start-ScheduledTask` anyway. It worked only because
the process exited in the gap.

**The mechanism it would have hit, written down because the near-miss is
cheaper than the incident.** `TwitchBotRS`'s `MultipleInstances` setting
is `IgnoreNew`. A start against a live instance is not an error — it is
silently ignored, with a success-looking exit. So the deploy would have
stopped the bot, started nothing, believed it succeeded, and moved on;
**and the one thing that recovers a dead bot, `TwitchBotRS-Watchdog`, was
suppressed by the maintenance flag that same step had just set.** Silent
at every layer: silent stop, silent non-start, silent watchdog.

§13A now carries a polling loop with a 60-second deadline that **aborts**
rather than starting, plus the instruction to clear the Bot flag before
walking away from an abort — an aborted deploy that leaves the flag set
has disarmed the watchdog over a bot that may be wedged. The flag is a
lease and does expire, but a deploy must not rely on the expiry to undo
its own half-finished state.

### Board

`personal_playlists`' intermittent Apps Script sync failure recorded in
`world2_build_plan.md` §5 as instructed, not investigated. Local playlist
data and `!playlist <username>` are unaffected; only the public site
falls behind.

---

## 2026-09-03 — Affix tier curve deployed, and an incident 64 seconds later

Merge `c36d582` into master, deployed as `affix-tier-curve`, then
redeployed as `affix-curve-restore` after a concurrent session reverted
it. Both swaps clean; the curve is live and the retroactive rescale is
applied.

| | |
|---|---|
| merge | `c36d582` |
| binary (curve) | `ab49d67953e47f40aa731da9ab1e8282046c4763ca4bbc8f9018a23de4000498` |
| binary before | `58972241…c19a80` |
| downtime | 0.17 s (deploy) + 0.15 s (restore) |
| suite on the box | 786 passed / 0 failed |
| pre-migration backup | `pod-backup-20260902-191938.tar.gz`, **sha256 verified**, contains `adventure-characters.json`, roster 14 = 14 live |

### THE ROLLBACK, in the owner's words

**Binary rollback, restore `adventure-characters.json` from the
pre-migration backup, delete the marker file. It loses fights resolved
since, and the mitigation is that the window is minutes, not that the
loss is avoidable.**

The owner's original assumption — that the ratio rescale is exactly
invertible, so rollback would be arithmetic rather than a restore — was
tested and withdrawn. The arithmetic *is* invertible on a frozen
snapshot (worst relative error 3.3e-16 across 96,000 cases; the ×0.5
crit halving is bit-exact, being a power of two). It is **not**
invertible on a running game: post-migration drops already roll on the
curve, the three growth sites now grow by the curve ratio, and
`sync_tier_to` changes `tier` itself — so the inverse factor would be
computed against the wrong `T` even for pre-existing items.

### The migration, verified on production

| | |
|---|---|
| items compared (same id, same tier) | 225 |
| affix values compared | 346 |
| value mismatches vs `f(T)/T` | **0** |
| jitter not preserved to 5 dp | **0** |

Polish preserved exactly, including the cases the spec's own
jitter-reconstruction shape would have destroyed: `merkosh`'s
coldDamage at jitter **1.44000 → 1.44000** (Perfect ×1.20 *and* polished
×1.20), `lokati`'s divineDamage at 1.20000 → 1.20000.

The owner's tier-7 Worn Robe, live from production:

| affix | before | after |
|---|---|---|
| cold | 14.92% | **5.64%** |
| divine | 18.90% | **7.14%** |
| lightning | 18.10% | **6.84%** |
| max hp | 22.45% | **8.49%** |

### INCIDENT — a concurrent session reverted the curve for five minutes

**19:19:51** my swap lands, migration runs, marker written.
**19:20:43** — 64 seconds later — another session runs
`deploy-linux.sh` for `player-facing-batch` and installs binary
`c180491e`, built from `/root/deploy-src`, a tree extracted at **18:45**
that predates my merge and contains **zero** references to
`affix_tier_curve`. Production spent ~5 minutes with **migrated (curved)
stored values and a linear binary** — the worst of both: new drops
rolling at full linear magnitude against gear that had just been cut,
tier growth re-inflating, and every migrated affix reading Q0% because
`affix_quality_percent` divides by a linear base.
**19:24:37** I redeployed master's build. Marker guard held — the
migration did NOT re-run (Worn Robe still 5.64%, not the 2.13% a second
application would give).

**Contamination during the window, complete:** 5 affixes across 4 items
rolled at linear magnitudes (~1.73× high, all T3), plus one item grown
under the linear ratio (~2.00× high). Left in place deliberately — hand-
editing live player data outside a marker-guarded migration is a worse
risk than the inflation, and the amounts are trivial:

| character | item | affix | live value | on-curve equivalent |
|---|---|---|---|---|
| lokati | Iron Plate T3 | critMultiplier | 0.15549 | 0.08977 |
| lokati | Iron Plate T3 | damageReduction | 0.06430 | 0.03713 |
| sitch89 | Steel Circlet T3 | critMultiplier | 0.12793 | 0.07386 |
| sitch89 | Sturdy Blade T3 | divineDamage | 0.05941 | 0.03430 |
| sitch89 | Iron Gloves T3 | chaosDamage | 0.07212 | 0.04164 |
| galquin | Worn Helm T1→T4 | (all) | ×4.0 applied | ×2.0 correct |

**Two process failures, both worth fixing:**

1. **`/root/deploy-src` is a single fixed path shared by every deploying
   session.** A concurrent session's `rm -rf /root/deploy-src` destroyed
   my first build mid-flight (the test run failed with missing `.rlib`s
   and no binary, on a box with 280 GB free). I rebuilt in
   `/root/deploy-src-affix-curve`. **§13B should name a per-release
   source directory, not a shared one.**
2. **A binary built from an unmerged branch was deployed over master's.**
   `origin/master` was `c36d582` throughout and still is; the tree that
   was deployed is on no ref. The house rule that only the deploy session
   merges and deploys exists precisely for this, and the failure mode it
   prevents is exactly what happened: a stale branch silently reverting a
   balance change that had already rewritten player data.

### Backup allow-list — the owner's assumption was wrong, and it mattered

The order assumed `backup-game-data.sh` globs `adventure-*-marker.json`.
**It does not** — `MARKER_FILES` is a hand-maintained array of 23 literal
filenames. `adventure-affix-tier-curve-marker.json` was added to it (repo
and box; `deploy-linux.sh` does **not** refresh `bin/`, so the box copy
needed a separate `scp`). Verified after the fact: the marker is present
in `pod-backup-20260902-192057.tar.gz`.

This mattered more than a normal marker would, because
`migrate_affix_tier_curve` is deliberately **not idempotent**. A restore
that brought back `adventure-characters.json` without the marker would
have applied the cut a second time. There is a drift check in the script
that reports unknown markers on disk, but it warns — it does not back the
file up.

### FOUND, for the board — not this release

- **The Stone Fist rank-4 cap/efficiency defect is still present**
  (`character.rs:2933`): `magnitude_at_rank` clamps a Specialization node
  to `effective_rank = min(rank, 3)` while the cap uses the RAW rank, so
  rank 4 buys +0.10 of cap and zero efficiency while the node text still
  reads "up to +30% at 3/3". It affects every `spec()`-tier
  `OverflowConversion` node — `unbreakable`, `elusive`, `shiftingform`,
  `aegisward`. Spec D44 ratifies it as a defect; the fix changes live
  player power and belongs to the passive rebalance.
- **Every line number in `docs/affix_curve_spec.md` is from the
  2026-08-23 tree and most no longer resolve.** `affix_base_value` is at
  385 not 385-387's neighbours; `compute_power` is at 1010, the spec says
  967. Treat its citations as leads, not addresses. Three of its factual
  claims have also drifted: `OVERFLOW_CONVERSION_CAP_PER_RANK` is now the
  `overflow_conversion_cap_per_rank` LiveTunable with a per-node override
  table, and the clamp is `clamp_overflow_conversion`, not an inline
  `.min()`.
- **`reforge_item`'s `tier_ratio` was a third growth site the spec never
  named** (§4.1 lists only `sync_tier_to` and `roll_recombine`). Found by
  sweeping for the shape rather than by following the document. Anyone
  implementing from this spec should sweep rather than trust its lists.

### §5.1 — pacing baselines are now calibrated against a curve that is gone

`BASELINE_STAGE_ANCHORS`/`BASELINE_HP_ANCHORS`/`BASELINE_ATK_ANCHORS`
were hand-authored against the linear power term, and their own doc
reasons explicitly from "party power has historically outrun the LINEAR
stage curve". That premise is now false. They were deliberately NOT
changed in this release (owner: "note it, do not build it"). They are
LiveTunables, so this is tunable on `/admin/tunables` without a deploy —
but the shipped pacing floor is wrong in a direction nobody has measured,
and it should be retuned once the curve has been live long enough to
read. The golden corpus is the early warning: 7 of 17 scenarios flipped
from win to loss, every one at stage 200+, which is what a party running
on a slower power curve against enemies tuned for the old one looks like.
Production is at stage 5 and nowhere near it — but the world climbs.

## 2026-09-02 — PLAYER-FACING-BATCH (feature/player-facing-batch)

Four commits on a feature branch, in the owner's ordered sequence. Not
merged, not deployed. Piece 1 is independently deployable and was reported
separately for that reason.

| # | Commit | What |
|---|---|---|
| 1 | `9d11733` | Six craft confirmations, dead in production since 2026-08-19, made to fire again |
| 2 | `c5075e5` | Unique Shard joins them |
| 4 | `8320487` | 50 basic-enemy sprites, rolled server-side |
| 3 | `495e41f` | Bug reports: `/bugs` + `/admin/bugs` |

Suite `cargo test --release --workspace --quiet`: 761 passed, 0 failed
(755 baseline + 1 confirm-wiring + 4 sprite + 1 bug-report test file).
Clippy clean on touched code. `node tools/bundle-contract.test.mjs` 19/19.

**Piece 1's fix is structural.** The confirm handler now delegates on
`document` instead of resolving one form by first match. Verified the new
test fails against the pre-fix `base.html` and passes after — it is the
assertion whose absence let a dead confirmation ship for two weeks.

**DOM-order sweep (ordered).** No second live instance. `overlay.html` has
no `querySelector` or `.closest` at all; its only listeners are on
`window`. In `base.html` nothing else binds a listener through a
class-based first-match: the cost-preview block resolves by unique input
name or unique data-attribute, and the `times` picker is scoped to
`.polish-reforge-actions`, which the Divine Dust row deliberately does not
use. All latent rather than live, and all cosmetic label preview — a
stolen binding there shows a stale price, it does not skip a safety gate.

FOUND — in a basic encounter, every enemy after the first has been
rendering as the death sprite. The client-side sprite pick produced a
ONE-entry array, and `spriteNameForEnemySlot` falls back to `death` for any
slot past the end of the array. Fixed as a side effect of Piece 4.

FOUND — the comment at that call site still described the pre-Lich-adds
fallback ("reused for every index via bossImgAt's own fallback"), which is
why the code read as correct. The behaviour changed under it and the
comment did not.

FOUND — the Annulment button's `action` value is `annulment orb`, not
`annulment`. Caught on the new confirm test's first run.

### PATCH NOTE DRAFT — for the deploy session

Not written to `C:/PathofDust/patch-notes.json` by this session: that file
is runtime data on a box this session must not touch, and patch notes ship
with the deploy. Text as ordered, nerf-honest:

> **Confirmation prompts were broken, and we're sorry.**
> Since 19 August, the "are you sure?" prompt has not been appearing on
> Krangle, Scour, Annulment Orb, Chancing or Hideout Warrior. Divinity has
> never shown one at all — it shipped after the break. If you lost an item
> to a click you did not mean to make, that was a bug on our side, not you
> misreading the interface. All six ask again now, and Unique Shard has
> been added to them: it is the one crafting cost you cannot re-earn with
> dust.
>
> **Most of the enemies in a basic fight were drawn as corpses. That was
> a bug.** In any filler fight with more than one enemy, every enemy after
> the first has been rendering with the death sprite. It looked
> intentional — a horde of the dead — and there was no reason for you to
> read it as anything but the art. It was not: the game only ever sent one
> enemy picture per fight, and every slot after the first fell through to
> the corpse. Fixed. You will see the actual monsters now, and there will
> be more of them on screen than you are used to.
>
> **Basic fights have 50 new enemies.** Filler fights used to reuse three
> boss sprites. They now draw from fifty of their own.
>
> **Enemies stay the same across replays.** Your browser used to pick the
> monsters itself, at the moment of drawing, so the same fight showed
> different enemies on every replay and different enemies to every person
> watching at once. The server picks them now: one fight, one set of
> monsters, the same for everyone, every time.
>
> **Report a Bug** is in the top menu. Logged-in players can send a report
> straight to the owner — it replaces the old `!bugreport` chat command
> that went away with Twitch. One a minute.

## 2026-09-03 — PLAYER-FACING-BATCH: I reverted the affix curve for four minutes

My account of the concurrent-deploy incident the affix-curve session
recorded in `ea5ef88`. Their record is the canonical one; this is what I
did wrong, so the next session does not repeat it.

**What happened.** I deployed `ce6ba5c` (my merge of
`feature/player-facing-batch` onto master `1465e45`) at 19:20:55 box
time. The affix-curve release had gone live at 19:19:51 — 64 seconds
earlier — deployed from its branch BEFORE it merged to master. My binary
was built from a master that did not contain it, so the swap removed the
affix tier curve, the crit-multiplier halving and the retroactive
rescale from production. The affix-curve session re-deployed at 19:24:51
and restored it. Roughly four minutes of wrong item stats, over one
basic fight (`fight-0000000182`).

**The check I had and ignored.** I read the live binary hash twice: at
the start it was `58972241`, and immediately before the swap it was
`ab49d679`. I NOTICED the change, said so out loud, and then reasoned:
"another session deployed while I worked; my merge sits on top of that
master, so my candidate is strictly newer." That inference is invalid
and is the whole error. A live hash I cannot account for does not mean
production is behind me — it means I do not know what is running. The
affix-curve binary was ahead of my master, not behind it.

**Rule this should have been.** Before a swap, the live binary must be
attributable to a commit that is an ancestor of the one being deployed.
Hash inequality proves only that something changed. If the live hash
cannot be tied to a known ancestor, STOP: either identify it or wait.
A cheap version of this check: `git log origin/master` for a deploy
record naming that hash, or ask, before swapping.

**Second lesson, unrelated to the collision.** `deploy-linux.sh`'s asset
refresh works, but on a box where two releases are interleaved the
assets and the binary can end up from DIFFERENT trees: my 50
`basicenemy/*.png` are still on the box (new files, nothing deletes
them) while `overlay.html` and `base.html` came back from the
affix-curve tree. Assets are not part of the rollback slot, so a
rollback restores the binary and leaves the other release's assets in
place. Worth knowing before the next interleaved night.

**Verified by effect, while my release was briefly live** — the release
itself is sound, which is not in question here:

| | |
|---|---|
| `fight-0000000182` (mine live) | 17 enemies, 17 sprites, all `basicenemy/`, zero death sprites |
| `fight-0000000181` (before) | 16 enemies, 0 sprites — 16 slots drawn as corpses |
| `fight-0000000183` (after revert) | 8 enemies, 0 sprites — corpses again |
| health checks 1-7 | all passed; downtime 0.12 s; NRestarts 0 |
| log during my window | clean apart from a pre-existing retired-affix WARN |

FOUND — the pre/post fight records are the clearest evidence yet for the
death-sprite bug: every basic fight before this release stored ZERO
sprites for 8, 16 and 21 enemies respectively. Every slot past the first
fell through to `death`.

**State at hand-off.** Production runs the affix-curve binary
`ab49d679`, correct and untouched by me. My patch-notes entry was
reverted from `/var/lib/pathofdust/patch-notes.json` (restored from
`/root/patch-notes.pre-player-facing-batch.json`, 27 entries, the
affix-curve note back on top) because it advertised a release that is no
longer live; the entry is kept at `/root/patch-entry.json` for re-use.
`feature/player-facing-batch` is rebased onto master `ea5ef88` and
pushed. NOT merged to master, NOT deployed, awaiting a fresh go.

---

## 2026-09-03 — DEPLOY-CONTROL: SSH hardening, the backup misread, and the catch-up defect

Session with sole authority to merge and deploy today. Three feature
branches queued; this entry covers the work that came before and
alongside the first of them.

### SSH was not key-only, and setup's record that it was had never been checked

`sshd -T` reported **`permitrootlogin yes`** and **`passwordauthentication
yes`** on the internet-facing box, against **1,392 failed auth attempts
from 39 IPs** in the twelve hours since the previous deploy. The host was
recorded at setup as key-only root SSH. That was believed and never
verified — the whole exposure is the gap between a written claim and
`sshd -T`.

Root has exactly one authorised key (`SHA256:REikbw…`, `pathofdust-deploy`);
`podbackup` has exactly one (`SHA256:F5lF68…`, `restrict,command=`). Both
were fingerprint-matched against the Windows box's `id_ed25519` and
`pod_pull` **before** anything changed, because the only way this strands
us is if the key we are about to depend on is not the key that is
installed.

| | before | after |
|---|---|---|
| `passwordauthentication` | yes | **no** |
| `permitrootlogin` | yes | **prohibit-password** |

Changed in `/etc/ssh/sshd_config` lines 124–125 (no drop-ins exist);
backup at `/etc/ssh/sshd_config.bak-preharden-20260903-052721`.

**The safety construction, which matters more than the change.** Three
independent layers, because a bad sshd config on a remote box is
unrecoverable from the same channel that broke it:

1. A second authenticated root session held open across the reload.
2. A **dead-man auto-revert** armed BEFORE the reload —
   `systemd-run --on-active=600 --unit=sshd-revert` restoring the backup
   and reloading. Disarmed only after verification; it never fired.
3. `ssh.service`'s own `ExecReload` runs `sshd -t` with
   `ignore_errors=no`, so an invalid config fails the reload instead of
   applying it. Free, and worth knowing it is there.

`reload` (SIGHUP), not `restart` — existing sessions are not dropped.
`NRestarts` stayed 0.

Verified after, from the Windows box, on **new** connections:

| test | result |
|---|---|
| root, key | **OK** — `sshd -T` reads `permitrootlogin without-password`, `passwordauthentication no` |
| `podbackup` forced command (`list`) | **OK** |
| root, password only (`PubkeyAuthentication=no`) | **refused: `Permission denied (publickey)`** — no password method offered at all |

`KbdInteractiveAuthentication no` was already set, which is what makes
this complete: with it `yes`, PAM can still offer a password prompt and
`PasswordAuthentication no` is a half-measure. Worth checking both any
time this comes up again.

### The off-box backup: the reported symptom was a misread, and the defects were real independently

**Both halves matter, so both are recorded.**

**The symptom was a misread.** The report was that this morning's
catch-up pull died partway through its 13th archive, leaving 12 local
and no `pull end` line, with `pod-backup-20260902-192437.tar.gz` a
3,473,408-byte partial. Measured at the time of investigation: the run
**completed at 10:55:50** — `pull end - fetched 14, pruned 0, held 21,
newest=pod-backup-20260903-032011.tar.gz age=7.6h` — and that archive is
**4,448,257 bytes, byte-identical to the remote**, with a valid sidecar.
The observation was made during a genuine **7m05s stall** (10:48:37 →
10:55:42) while that file was in flight. A transient was read as a
failure. Nothing was lost and nothing was unbacked-up.

**The defects were real anyway, and would have bitten on the next killed
run.** They are properties of the code, not of that run:

1. `pull-linux-backups.ps1`'s three delete-the-partial paths only run
   *while the script is alive*. A run killed mid-transfer (shutdown,
   sleep, task terminated) leaves the partial, and the fetch gate was
   `if (Test-Path $local) { continue }` — **existence alone** — so that
   partial was never re-fetched, never verified, never removed.
2. Retention and the 36-hour staleness alarm both read a plain
   name-sorted `Get-ChildItem`. A partial carrying a recent timestamp
   sorts newest and holds the age under the limit **indefinitely**. The
   one alarm built to say "your off-box backups have stopped" is
   silenced by a broken backup that looks fresh. It could also prune a
   good archive to keep a broken one.

Fixed: `Test-ArchiveVerified` (sidecar present + 64-hex match against
`Get-FileHash`), `Get-VerifiedArchives`, an **unconditional sweep before
the fetch**, and retention / `held N` / staleness all reading the
verified set only.

**Proven on a genuine killed-run partial, not a hand-made one** — a real
transfer was killed mid-flight, leaving 4,030,464 of 5,408,653 bytes, a
zero-byte `.stderr`, and no sidecar. Same fixture, both scripts:

| | old | new |
|---|---|---|
| result | `held 7, newest=…031911 age=32h` → **exit 0, alarm silent** | `swept` both, `held 6, newest=…160235 age=43.3h` → **FAILED, exit 1** |
| partial survives | yes | no |

That fixture is the right shape for this class of bug and should be
reused: **construct the failure, run the old code, watch it pass.** A fix
for a quiet failure that is never shown failing is a guess.

The hash is matched by regex rather than `-split` on the first field:
`Set-Content -Encoding utf8` on PS 5.1 writes a **UTF-8 BOM**, so the
first whitespace-delimited field carries three invisible bytes and a
naive comparison never matches. This cost one wrong "20 of 21 archives
MISMATCH" reading during verification before it was spotted.

After the fix: 20 of 20 remote archives local, **21/21 verified against
their sidecars** (the 21st is the deliberately renamed
`…115104-WORLD1-FINAL-STATE` keepsake), newest `pod-backup-20260903-032011`.
Scheduled task re-run `LastTaskResult=0`.

### FOUND — the transfers intermittently stall for minutes

4m09s and 7m05s stalls observed on single archive transfers today. This
is the mechanism that made a healthy pull look dead. Not investigated.
One candidate, untested: Defender real-time protection is on with archive
scanning and **no exclusion for `C:\pod-backups-linux`**, so every 4.5 MB
`.tar.gz` is unpacked and scanned as it lands.

### FOUND — catch-up XP degenerates into a flat global multiplier on a bunched roster

**This is the real defect. The `win_xp_mult` dial is a counterweight, not
a fix**, and it is recorded here so the next person does not read the
dial as the answer.

`manager.rs:catchup_multiplier` keys the bonus off the group **MEDIAN**.
When the roster is bunched — the common steady state, and exactly the
state a working catch-up mechanic produces — **the median equals the
maximum**, so every character at the top level lands in the `l <= median`
branch and takes the full **+100%**. Catch-up pays out most generously
precisely when it should be doing nothing.

Roster at 2026-09-03 02:52Z (t+17.0 h after the World 2 reset):

| level | chars | multiplier | grant per win |
|---|---|---|---|
| 11 | 14 | **x2.00** | 37 xp |
| 9 | 1 | x2.22 | 38 xp |
| 8 | 1 | x2.33 | 38 xp |
| 2 | 1 | x3.00 | 39 xp |

The symptom, stated plainly: **a level-11 leader earns within 17% of what
a level-2 newcomer earns.** The mechanic is not a trailing-player bonus
in this state; it is a flat 2x global XP multiplier that happens to be
1.17x steeper at the bottom.

Measured consequence: **14.2 levels/day** across the whole run and
**13.25 levels/day** in the final 1.53 h window — no meaningful decay,
against an approved shape of 10 levels on day one settling toward 2/day.
The pack spent day one's entire allowance in about 14 hours. At x1.00 the
same roster would sit near 7 levels/day, so this accounts for essentially
the whole overshoot.

Owner is setting `win_xp_mult` 1.0 → 0.5 on the live page. Not touched by
this session. The curve shape is untouched by that dial (see the
2026-09-02 ruling), so the degenerate branch survives it and will
resurface the moment the roster bunches again at any scale.

### The HP pacing controller was saturated at its ceiling all night

First live measurement of the §5.1 warning that the pacing baselines were
calibrated against a power curve the affix tier curve removed.

| | at deploy 19:24 | at 04:52 | bound |
|---|---|---|---|
| `hp_pacing_mult` | 3.0518 | **6.000** | `hp_multiplier_ceiling` 6.0 — **pinned** |
| `boss_power_mult` | 0.5121 | 2.2268 | `dmg_multiplier_ceiling` 4.0 — headroom |
| median win DPS | 409 | 2738 | |

The damage controller was dead on target: boss W/L **1.94:1** against
`target_win_loss_ratio` 2.0. The HP controller was not: the target
duration band is **30–45 s** and the median fight was **10.6 s**. At 25%
per fight it climbed 3.05 → 6.0 in about **four winning fights** — roughly
ten minutes after the swap — and sat against the stop for the remaining
nine hours. To reach the 37.5 s midpoint it needed about **21x** and was
allowed **6x**.

The direction is worth stating because it is the opposite of what was
expected from a 3–10x affix nerf: **the party was not weaker, it was far
stronger.** Level gain swamped the nerf entirely.

Owner has raised `hp_multiplier_ceiling` to **50** on the live page.
Releases landing after that will see the controller move again — it was
unpinned at 05:20 and had already walked 6.0 → 7.5 by 05:30. **That is
expected, not a regression**, and anyone reading fight durations across
this boundary should not attribute the change to their own release.

### FOUND — `adventure-bugreports.json` was ignored by nothing and backed up by nothing

Caught at deploy, in the branch being deployed, **not** in the commit that
introduced the file. The per-file trap documented in `.gitignore` fired
again — and in its nastiest form.

`.gitignore` line 151 already carried `bugreports.json`, a leftover from
the Twitch bot era sitting among `commands.json` and `entrance-themes.json`.
It **looks** like this file's entry. It is not: a gitignore pattern with no
slash matches the whole basename, so `bugreports.json` never matched
`adventure-bugreports.json`. `git check-ignore -v adventure-bugreports.json`
returns nothing.

The file holds player-submitted free text plus the reporter's account
name. Without the fix it would have sat untracked-but-committable in the
deployment root of a repo whose remote is public — the same shape as the
account-store near-miss of 2026-09-02, one `git add -A` from permanent.

`backup-game-data.sh` was the second half: `CORE_FILES` is a
hand-maintained literal list and the file was not in it, and the drift
check that warns about unknown files on disk covers **markers only**. Every
bug a player filed would have been outside the backup set from the first
one.

Both fixed in the deploy commit. **The durable lesson, which is not the
one already written down:** a near-miss ignore entry is worse than no
entry, because it reads as coverage to anyone who greps for "bugreports".
When adding a data file, grep the **exact** filename and confirm with
`git check-ignore -v <name>` — the pattern's presence is not the test, the
match is.

### §13B now names a per-release source directory

Ordered as part of this release rather than left for the next session to
rediscover. `/root/deploy-src` was a shared mutable global between
sessions that cannot see each other; a concurrent `rm -rf` destroyed the
affix curve release's build mid-flight. Now `/root/deploy-src-$REL`, with
the archive (`src-deploy-$REL.tar.gz`), the logs (`build-$REL.log`,
`test-$REL.log`) and the transient build unit (`pod-build-$REL`) all
per-release.

Two things found while writing it:

- `--setenv=REL=$REL` is **load-bearing** on the `systemd-run` form. The
  `bash -c` string is single-quoted so `$?` survives to the inner shell,
  which means `$REL` reaches it unexpanded too — and a transient unit
  inherits nothing, so without that line it expands to empty and the
  build logs to `/root/build-.log`. You then read a stale or absent log
  and draw a conclusion about the wrong build.
- §13B had **no step that checks the unpacked tree is the one you think
  it is**, which is the root cause of the concurrent-deploy incident: a
  tree extracted before the curve merge was built and shipped over
  master's. Added — grep the source for a symbol the release introduces
  and stop if it is absent.

### 2026-09-03 — PLAYER-FACING-BATCH deploy record (release `player-facing-batch`)

First of three queued releases. Merged by HASH `352821d`, not by branch
name.

| | |
|---|---|
| merge | `f5c2035` (master `ea5ef88` → `f5c2035` → `f852584` ops/docs) |
| binary before | `ab49d679…000498` (affix tier curve) |
| binary after | `5bf620d4704a5ad103fe8c87ab9b653a1b63337500a2b2fd9c1610661fa4cb33` |
| rollback slot | `/var/backups/pathofdust/deploy-pre-player-facing-batch/game.pre-player-facing-batch` |
| downtime | **0.31 s** |
| build | 2 m 27 s, exit 0 |
| suite on the box | **791 passed / 0 failed / 0 ignored, 37 suites** — `cargo test --release --workspace --quiet` in `/root/deploy-src-player-facing-batch` |
| source dir | `/root/deploy-src-player-facing-batch` — **first release under the new per-release §13B path** |

Baseline arithmetic: master was 786 after the affix curve; this branch
adds `basic_enemy_sprites.rs` (new), `bug_reports_http.rs` (new) and
extends `craft_confirm_ui_http.rs`. 791 is the branch's own stated count
and it reproduced exactly on the box.

### The tree-identity check, which is the point of this release's procedure change

This is the branch whose stale build reverted the affix curve yesterday.
Before building, the unpacked tree was checked for the thing that was
missing last time:

| check | result |
|---|---|
| `c36d582` (curve merge) is an ancestor of `352821d` | **yes** |
| `affix_tier_curve` references in `$SRC/game/src` | **24** across `affix.rs` and `migrations.rs` (the bad tree had **zero**) |
| release's own symbols present | `BASIC_ENEMY_SPRITES` ×5, `bug_reports.rs` present, 50 sprites |
| archive sha256, dev machine vs box | `bbe74983…f02fae` both sides |

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0`, unchanged |
| 3 | loaded characters vs file | **18 = 18** |
| 4 | live sha256 | `5bf620d4…` = candidate |
| 5 | authenticated `/characters`, `/passives` | 200 / 80,074 B, 200 / 91,109 B |
| 6 | anonymous `/admin/tunables` | **404**, 73,730 B |
| 7 | anonymous `POST /api/commands/join` | **404** |

Zero panics or ERROR lines since the swap. Fights kept resolving across
it (`fight-0000000445` at 05:57).

### Verified by effect on production, not by code trace

**Confirmations.** On the live `/inventory` page, rendered with the
owner's own session, five of the six now carry `data-confirm="1"`:
**Krangle, Scour, Annulment Orb, Chancing, Hideout Warrior**. The
remaining two — **Unique Shard and Divinity** — could NOT be
click-through verified, because both buttons only render when the
character holds a Unique Shard and the owner holds none
(`craft_tokens` shows `chancing 0, regal 0, exalt 0`, no unique-shard
entry). Their `data-confirm="1"` was confirmed in the deployed source at
`adventure_web.rs:6484` and `:6632`. **Stated as five verified live and
two verified by trace, rather than six**, because the distinction is
exactly the one the house rule is about.

Worth recording for whoever re-checks: the confirm attribute is emitted
on BOTH branches of `action_btn` — the free-token branch and the
dust-paid branch. Since token drops were retired by stage-gated-drops,
almost every real craft goes through the dust branch, so a fix that only
covered the token branch would have been nearly inert. It covers both.

**Sprites.** Fight bundle `fight-0000000445`, resolved at 05:57:32 —
after the 05:52 swap — carries
`members.core.bossSprites = ["basicenemy/07-skeleton-archer",
"basicenemy/17-plague-doctor", "basicenemy/26-acid-hound",
"basicenemy/45-magma-golem", "basicenemy/49-red-demon", …]`: one
server-picked sprite per enemy, drawn from the new pool of 50. Before
this release the overlay rolled `Math.random()` client-side over three
boss sprites at render time. 50 sprite files landed in
`/var/lib/pathofdust/public_adventure_overlay/sprites/basicenemy/`.

**New routes.** `/bugs` 200 authenticated; `/admin/bugs` 200 for the
owner and **403** anonymous.

### Patch notes

Five sections inserted at the top of the existing "September 3, 2026"
block (1 → 6 sections; the affix curve's entry stays below them).
Pre-edit copy at `/root/patch-notes.pre-player-facing-batch.json`.
`/patch-notes` renders at 211,790 B. Nerf-honest per the standing rule —
the confirmation section leads by saying the prompts were broken since
19 August and apologising, rather than announcing a feature.

### `backup-game-data.sh` had to be copied separately

`deploy-linux.sh` does **not** refresh `bin/`, so the release's updated
`backup-game-data.sh` (the `adventure-bugreports.json` line) was
installed by hand to `/opt/pathofdust/bin/`, before the swap, so the
first backup taken after `/bugs` went live already covers it. Previous
copy at `/root/backup-game-data.sh.pre-player-facing-batch`. This is the
second release in a row to hit this; §13B names it for markers but the
general shape is "anything under `bin/` needs its own `scp`".

### FOUND — `deploy-linux.sh`'s rollback slot is keyed on release name alone

`BACKUP=/var/backups/pathofdust/deploy-pre-$NAME`, so **re-using a
release name overwrites the previous slot's rollback binary**. That
happened today: yesterday's incident left a
`deploy-pre-player-facing-batch/` directory, and this deploy wrote over
its `game.pre-player-facing-batch`. **Nothing was lost, by luck** — the
binary live before today's swap was the same `ab49d679` that slot already
held — and the pinned corpus merged rather than nesting (`cp -a` into an
existing directory), so the slot now holds the union, 381 summaries. A
genuine re-release of a name would have destroyed the only copy of the
earlier rollback binary. Not fixed; a date suffix on `$NAME`, or refusing
to write into an existing slot, would close it.



### 2026-09-03 — SMALL-ISOLATED-DEFECTS deploy record (release `small-isolated-defects`)

Second of four queued releases. Rebased from `e9aef0a` onto master by
this session; the authoring session is closed.

| | |
|---|---|
| merge | `f960656` |
| binary before | `5bf620d4…4cb33` |
| binary after | `7897d6a25f00d4776b63328858cc1a7bee91585334227ba0817409de0970c943` |
| downtime | **0.16 s** |
| build | 2 m 16 s, exit 0 |
| suite on the box | **799 passed / 0 failed / 0 ignored, 37 suites** — `cargo test --release --workspace --quiet` in `/root/deploy-src-small-isolated-defects` |
| rollback slot | `/var/backups/pathofdust/deploy-pre-small-isolated-defects/game.pre-small-isolated-defects` |

**Suite arithmetic, checked both ways.** The branch reported 794 / 35
suites against its own base of 786. Master stood at 791 / 37 after
`player-facing-batch`. 786 + 8 = 794 and 791 + 8 = 799, and 35 + the two
test files `player-facing-batch` added = 37. Both readings agree the
branch contributes exactly 8 tests, which is what makes 799 a
confirmation rather than a number.

### Ordering deviation, approved: build BEFORE push

Merged locally, built and tested on the box, and only then pushed. The
standing §13B order is merge → push → build. This branch needed a real
conflict resolution inside `AppState` — `player-facing-batch`'s `bugs`
field and this branch's `login_failures` both land in the same struct and
the same initialiser — and a merge that has not compiled has no business
on origin. Owner ratified the new order for the rest of the queue:
**merge, build and test on the box, push, then swap.** The push still
precedes the binary swap.

Two conflicts, both additive, both resolved keep-both: the `AppState`
field pair above, and `WIKI_IMPACT.md` (chronological, per the
append-only rule).

### The loopback bind was verified against production BEFORE it was merged

The change binds 4005 and 4004 to `127.0.0.1` instead of `0.0.0.0`. On a
box whose only ingress is a Cloudflare Tunnel, that is either harmless or
a total outage, and the difference is not visible in the diff.

What was checked first, on live production:

| | |
|---|---|
| ingress | `adventure.lokati.net` → `http://localhost:4005`, cloudflared on the same host |
| `localhost` resolution order | **`::1` first**, then `127.0.0.1` (`getent ahosts`) |
| answering on `[::1]:4005` before the change | **nothing** — the old `0.0.0.0` bind is IPv4-only |
| firewall | nftables `table inet pod` already drops 4004/4005 off-loopback |

So production was *already* running the exact path the change leaves
behind: cloudflared resolves `localhost`, tries `::1`, gets refused,
falls back to IPv4. Binding `127.0.0.1` cannot break what `0.0.0.0` was
not providing. Confirmed after the swap, before touching cloudflared at
all: both sockets on `127.0.0.1`, public tunnel still **HTTP 200**.

Port 4004 is not published through the tunnel and never was.

### Verified by effect on production

| claim | evidence |
|---|---|
| login throttle | **live probe against a nonexistent username**: attempts 1–10 ~0.8 ms, attempt 11 **1.00 s**, 12 **2.00 s**, 13 **4.00 s** — exact doubling, matching `LOGIN_FREE_FAILURES = 10` and the 1 s base under a 30 s cap |
| loopback bind | `ss -tlnp` shows `127.0.0.1:4004` and `127.0.0.1:4005`; site 200 through the tunnel |
| Last Rites text | the false wording "33% chance per rank" returns **0 occurrences** anywhere in the rendered passives page |

**NOT verified live, stated rather than glossed:**

- **Last Rites' charge ladder.** There is **no Slayer on the roster** —
  18 characters, zero slayers, and no secondary archetypes at all — so
  the node cannot be reached, let alone triggered, on production today.
  Verified instead from the deployed tree: `passive_tree.rs:2020` ships
  `SpecialPerRank { values: &[1.0, 1.0, 2.0] }` with the corrected text,
  and `adventure-passive-overrides.toml` on the box carries no
  `lastrites` entry, so the shipped values are the live ones. **The buff
  changes nobody's power today**; it becomes real the first time someone
  rolls a Slayer.
- **argon2 moving off the async runtime.** The throttle probe used a
  username that does not exist, and that path returns in ~0.8 ms — it
  never reaches a password hash. Proving the runtime change would need a
  real account and a load generator, which is not worth doing against
  production. It is covered by the branch's own tests.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **18 = 18** |
| 4 | live sha256 | `7897d6a2…` = candidate |
| 5 | `/characters`, `/passives` (auth) | 200 / 80,075 B, 200 / 91,109 B |
| 6 | anon `/admin/tunables` | **404**, 73,730 B |
| 7 | anon `POST /api/commands/join` | **404** |

Zero panics or ERROR lines since the swap.

### Patch notes

Three sections at the top of the "September 3, 2026" block (6 → 9).
Pre-edit copy `/root/patch-notes.pre-small-isolated-defects.json`. The
Last Rites entry leads by saying the node advertised a chance it never
rolled and that ranks 2 and 3 granted nothing, rather than announcing a
buff.

---

### 2026-09-03 — CLOUDFLARED INGRESS: `localhost` → `127.0.0.1` (separate from the release above)

Ordered alongside the `small-isolated-defects` deploy because the config
was already open, and recorded separately because it is **not part of
that release** — it is a production ingress change with its own risk and
its own evidence, and it predates every branch in today's queue.

**The premise was that every request paid a failed connection attempt.
Measured, that is not what happens — but the fix is still right.**

| test (30 parallel requests through the public tunnel) | failed TCP connects |
|---|---|
| 20 sequential requests, warm pool, before the change | **0** |
| 30 parallel requests, before the change | **22** |
| 30 parallel requests, after the change | **0** |

cloudflared holds a **keep-alive pool** to the origin — 16 established
connections to `127.0.0.1:4005` at the time of measurement — so a request
that reuses a pooled connection dials nothing and pays nothing. The
wasted `::1` attempt is paid **once per NEW origin connection**, not once
per request. On a warm pool that is zero; under a burst, or connection
churn, it is one per connection opened, which is where the 22 came from.

So: not "real latency on every page load", but a real and completely
free-to-remove cost under exactly the conditions where the site is
busiest. Changed to an explicit `http://127.0.0.1:4005`, which also
removes a resolver dependency from the hot path.

Procedure: both `/etc/cloudflared/config.yml` and
`/root/.cloudflared/config.yml` edited and backed up
(`.bak-preloopback-20260903-*`); validated with
`cloudflared --config … tunnel ingress validate` → **OK**, and
`… tunnel ingress rule https://adventure.lokati.net/` → matched rule #0
to the new service, with an unknown host still falling to the `404`
catch-all. **`--config` must precede `tunnel`**; the other order silently
prints help and exits 0, which looks like a passing validation and is
not.

`cloudflared.service` has **no `ExecReload`**, so this needs a restart
and a brief tunnel drop. It was folded into the same window as the binary
swap rather than taken as a second outage. Site verified **HTTP 200 in
0.048 s** afterwards.

## 2026-09-02 — TIER SOURCE AUDIT + the bump unification (branch `fix/craft-tier-bump`)

**The economy does not brake itself. This is the substantive finding of
the audit and it should be read before anyone tunes a crafting dial.**

- **Income and cost scale at essentially the same rate in the
  stage-driven regime.** Every dust source is keyed to WORLD STAGE: boss
  win `mean(1..3) x stage x loot_mult`, filler win `0.5..1.0 x stage x
  loot_mult`, disenchant `mean(1..6) x the DROPPED item's tier x
  max(1, 5 x affixes)` where a dropped item's tier is `1 + stage/5`.
  Income is therefore linear in tier; cost is `tier^1.1`; **the net brake
  is only `tier^0.1`**. In boss-wins per Transmute that reads 0.67 at
  tier 11, 0.56 at tier 21, 0.50 at tier 51, 0.53 at tier 201 —
  **crafting gets CHEAPER in boss-wins as tier rises**, not harder. There
  is no brake in this regime.

- **`craft_tier_exponent` acts on that regime and barely moves it.** Boss
  wins per Transmute from tier 11 to tier 201 (an 18x tier increase):
  1.0 -> 0.58/0.31, 1.1 -> 0.67/0.53, 1.25 -> 0.86/1.15, 1.5 ->
  1.35/4.29. **1.5 is where it becomes a force.** At 1.25 it is a nudge.

- **The only real brake is the craft-driven one at fixed stage, and it
  exists by accident.** With the world held at stage 4, income is FLAT (8
  dust per boss win) while cost climbs `tier^1.1`: 3.5 boss wins per
  craft at tier 1, 16.1 at tier 25, 43.4 at tier 70 — a 12.4x
  degradation. That brake works **only because no dust source reads the
  crafted item's tier.** It is not a designed mechanism; it is a gap.

- **Raising `loot_mult` removes it.** Loot scales income linearly, so it
  cancels straight out of the ratio: at tier 51 it moves boss-wins-per-
  craft 1.01 -> 0.50 -> 0.25 -> 0.05 across loot 0.5/1/2/10. It moves the
  level, never the slope, and it lifts both regimes at once.

- **Disenchant is the dominant dust source**, 5-7x boss income at every
  tier (770 vs 100 at tier 11; 14,070 vs 2,000 at tier 201) — and it is
  stage-driven like the rest.

**The two levers act on two different regimes and must not be confused
for each other.** `craft_tier_exponent` prices tier and moves the
stage-driven economy, where there is nearly no brake to move.
`craft_tier_bump_mult` (new, this branch) governs how fast an individual
item climbs away from its own world, which is the regime where the brake
actually lives. Turning one expecting the other's effect will read as the
dial not working.

### What shipped

RULING 1, the unification. A veiled craft committed through
`apply_craft_affix` and applied NO tier bump; an unveiled one went
through `Character::craft` and always did. **Ticking the Veil checkbox
exempted a player from the tier growth and, since the 2026-09-02 cost
curve prices tier, from the compounding cost growth too** — for 50 dust.
Both paths now call one helper, `Character::apply_craft_tier_bump`, which
is the only place a craft moves a tier. Magnitude deliberately unchanged
in the same commit, per the ruling: unify first, tune second. Unique
Shard is excluded because it has no unveiled counterpart to be identical
TO — bumping it would invent growth on the veiled side rather than match
it.

Evidence the loophole was real, from my own two production crafts on
2026-09-02: the token (auto-veiled) Regal left the item at tier 1; the
unveiled Exalt took it 1 -> 4.

RULING 2, the dial. `craft_tier_bump_mult`, default **1.0 = today's
behaviour exactly**, range 0.0-3.0. One field over all three bands: the
bands are a designed shape and a dial per band would let them drift apart
from an admin page. 0.0 switches per-craft tier growth off, which is the
setting to use while watching the exponent in isolation. 3.0 is +9 per
craft, so one Hideout Warrior click takes a fresh item from tier 1 to
tier 40 — past any plausible intent, and above that is a typo.
**`round`, not `ceil`** — the opposite of `scaled_base_cost`, deliberately:
a nonzero PRICE must never round away to free, but a fractional GROWTH
must be able to reach zero or the bottom of the range is a cliff.

RULING 3 is recorded in `docs/world2_build_plan.md` §7 — `boss_stats_for`
scales on stage and level and is blind to gear tier, so craft-driven
power is the one growth vector with nothing opposing it. Not patched, by
order.

RULING 4 is the doc block on `Character::apply_craft_tier_bump`: the bump
dates from the **initial commit**, was written as a growth reward when
tier had no price attached, and only became a cost escalator when the
2026-09-02 release made tier a price input. Two features designed apart,
never reconciled. A future reader must not mistake it for a recent
addition.

CORRECTION — my first FOUND line on this named the wrong mechanism. The
tier 1 -> 4 I saw on production was the +3 craft bump, not the
character-level sync. The level sync (`grow_krangled_items`, on every
level-up) is real but applies **only to Krangled items**.

PATCH NOTE DRAFT (for whoever deploys this — NOT written to
C:/PathofDust or the box by this session, which did not deploy):

  "Crafting: veiling no longer skips the tier growth" —
  "Crafting an item has always raised that item's tier — +3 tiers below
   tier 25, +2 below 50, +1 above — which raises its power and every
   modifier on it. It turns out that only happened when you crafted
   WITHOUT ticking Veil. A veiled craft quietly skipped it."
  "That is now fixed: veiled and unveiled crafts grow an item the same
   amount. Nothing about the amount has changed, and an unveiled craft
   behaves exactly as it did yesterday."
  "Worth knowing, because yesterday's cost change made it matter: the
   per-tier part of a craft's price is charged on the item's CURRENT
   tier, so crafting an item makes its next craft a little dearer.
   Veiling used to dodge that too."
  HONESTY NOTE, must survive whatever rewording ships: for anyone who
  was veiling, this IS a nerf — their items will now grow (a buff) and
  their subsequent crafts will now cost more (a nerf). Say both.

### 2026-09-03 — CRAFT-TIER-BUMP deploy record (release `craft-tier-bump`)

Third of four queued releases. Rebased from `ba6b15c`; its base was
`4abecde`, which **predates the affix tier curve**, so the interaction
with the curve was the whole risk of this one.

| | |
|---|---|
| merge | `bae43ef` |
| binary before | `7897d6a2…70c943` |
| binary after | `417b00f445902bbc0b4c41e802d93b57c035d87b78733819176ee4652ef77959` |
| downtime | **0.14 s** |
| suite on the box | **807 passed / 0 failed / 0 ignored, 37 suites** (799 + 8) |
| rollback slot | `/var/backups/pathofdust/deploy-pre-craft-tier-bump/game.pre-craft-tier-bump` |

### Why the curve interaction is safe, structurally rather than by luck

The concern was that a branch written before the curve would reintroduce
linear tier-growth math and silently revert part of it — yesterday's
incident in a different costume.

It cannot, and the reason is compositional: **this branch changes only
how many TIERS a craft adds; it does not touch how values scale with
tier.** `apply_craft_tier_bump` computes a bump and then calls
`Item::sync_tier_to`, and `sync_tier_to` lives in `item.rs`, which this
branch does not modify at all. Master's curve had already rewritten that
function to scale affixes by `affix_tier_growth_ratio` — `f(new)/f(old)`,
not `new/old`. `roll_recombine` and `reforge_item`, the other two growth
sites, are untouched too.

Verified in the merged tree rather than assumed: `sync_tier_to` still
calls `affix_tier_growth_ratio`, and `apply_craft_tier_bump` still
delegates to `sync_tier_to`.

Conflicts were **docs only** — `WIKI_IMPACT.md` and
`docs/session_journal.md`, both append-only keep-both.
`character.rs`, `manager.rs`, `adventure_web.rs` and
`world2_build_plan.md` auto-merged; they were read rather than trusted.

### The `TunablesForm` trap, checked in BOTH directions

This branch adds a form field, which is the exact shape CLAUDE.md warns
about twice:

| | |
|---|---|
| `#[serde(default = "default_craft_tier_bump_mult")]` on the field | present |
| an `<input name="craft_tier_bump_mult">` really rendered | present — live page shows `value="1"` |
| the form test derives its POST set from the rendered page | yes — `admin_tunables_splash_http.rs` splits on `name="` out of the fetched form |

So drift in either direction fails the suite, including the direction no
hand-maintained superset body can catch.

### FOUND — the branch's own rationale is now half stale, and overstates by 4x

`docs/world2_build_plan.md` gains an audit note from this branch reading
*"`Item::sync_tier_to` rescales `power` and **every affix value** by the
tier ratio, both of which are linear in tier"*, and quantifies the loop
as *"+15 tiers from tier 1 — power and every modifier ×16"*.

Post-curve, that is half right:

| | scaling for tier 1 → 16 |
|---|---|
| `power` — `compute_power` is `base × tier × roll` | **×16**, still linear, claim holds |
| affixes — `affix_tier_growth_ratio` = `f(16)/f(1)` = `sqrt(16)` | **×4**, not ×16 |

The text was written against a pre-curve tree and is accurate for the
tree it was written against, so per the append-only rule it stays as
written; this dated note is the correction. **The practical effect is
that the audit overstates the modifier half of the craft-power loop by a
factor of four**, which matters because that document is explicitly
framed as the input to an owner ruling on whether to damp the bump. The
loop is real; it is smaller than the document says.

### NOT verified live, stated rather than glossed

**The veiled-craft change itself was not click-through verified.** The
shipped behaviour change is that a veiled craft now applies the same tier
bump an unveiled one always did. Exercising that on production means
spending a real player's dust and permanently raising a real player's
item tier, which is not a thing to do to somebody's character to satisfy
a check. It is covered by the branch's tests and by the dial rendering at
its default. What *was* confirmed live: the new tunable exists on
`/admin/tunables` at `value="1"`, so the magnitude is unchanged until the
owner moves it.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **18 = 18** |
| 4 | live sha256 | `417b00f4…` = candidate |
| 5 | `/characters`, `/passives` (auth) | 200 / 80,075 B, 200 / 91,109 B |
| 6 | anon `/admin/tunables` | **404**, 73,730 B |
| 7 | anon `POST /api/commands/join` | **404** |

Public tunnel **HTTP 200 in 0.090 s**. Zero panics or ERROR lines.

### Patch notes

One section at the top of the September 3 block (9 → 10), pre-edit copy
`/root/patch-notes.pre-craft-tier-bump.json`. Headed *"Veiling no longer
exempts a craft from tier growth. This is partly a nerf"* — because it
is. Veiling for 50 dust previously skipped both the tier growth and the
escalating per-tier dust surcharge, so it was a way to craft an item
indefinitely without its price ever climbing. The note says that plainly,
names the nerf in capitals, and also states the other side (veiled crafts
now make the item stronger, which they did not before).

### Build parallelism capped mid-release (owner standing constraint)

Adopted while this release's tests were running. Four sessions compile
this workspace concurrently with isolated target dirs and no shared
cache. The uncapped test run was measured at **load average 8.02 on 8
vCPUs** — a box that is also serving production. The in-flight run was
stopped and re-run under `CARGO_BUILD_JOBS=4` plus `-j 4`; load settled
to **3.22**, and the suite returned the same clean result.

The transient unit was stopped **by unit name** (`systemctl stop
pod-build-craft-tier-bump`), never by process image — `taskkill /IM`-shaped
kills are banned here precisely because they match production's `game`
binary too.

Worth knowing for whoever inherits the constraint: `-j 4` and
`CARGO_BUILD_JOBS` cap **compile** jobs. The test binaries' own harness
threads are a separate knob (`--test-threads`), deliberately not touched,
because this suite has known flaky-under-parallel tests and changing
their concurrency changes what the run means. Load settled acceptably
without it.

## 2026-09-03 — GEAR-SLOTS: four new equipment slots (spec §8)

Branch `feature/gear-slots` off `origin/master` @ `ea5ef88`. Built,
tested, pushed. NOT merged, NOT deployed — the deploy session holds that.

### What shipped

`Ring1`/`Ring2` (crit chance, 0.01/tier), `Amulet` (crit multiplier,
0.025/tier), `Pants` (% increased life, 0.03/tier). Each slot's base
power equals exactly one affix of that type at the item's tier (R5),
runs through `affix_tier_curve` while the original five stay linear
(D11), and rolls against the affix jitter band 0.85..1.15 rather than
`POWER_ROLL_RANGE` (D13). Implicits land in the same additive gear pool
`sum_affix` feeds, via a new `Character::slot_implicit`.

### Spec premises checked, and one refuted

- The six hardcoded five-slot lists in `adventure_web.rs` were still
  exactly six; every line number in §8 had moved (3155→3867, 3903→4815,
  4019→4932, 5128→6041, 5236→6160, 5748→6767). Replaced by
  `DISPLAY_SLOTS` + `BAG_SLOT_ROWS`, with tests asserting both cover
  `EQUIP_SLOTS`. The seven `EQUIP_SLOTS[gen_range]` loot sites were also
  still exactly seven.
- **REFUTED — §8.7's rng hazard does not reach the golden corpus.**
  §8.7 predicts the 17 fixtures diverge because `EQUIP_SLOTS.len()`
  changes `gen_range(0..5)` to `gen_range(0..9)` at seven loot sites.
  `run_scenario` (golden_corpus.rs:414-418) never calls those sites — it
  equips five slots EXPLICITLY from the seeded rng and runs combat only,
  with no loot path anywhere in the scenario. Predicted before building
  that all 17 fixtures would stay byte-identical; **they did.** Nothing
  regenerated, nothing to regenerate. §8.7's two-commit sequencing advice
  is sound in general and its stated reason is wrong for this repo.

### FOUND

- **A seventh five-slot list §8 missed:** `ALL_SLOTS` in
  `affix.rs:662`, test-only and therefore not compiler-caught. It would
  have gone on asserting the 17-affix pool for the original five and
  silently stopped covering the new four. Now reads `EQUIP_SLOTS`.
- **A seventh D13 touch point §12.2 missed:** `Item::has_polish_room`
  compares `power_roll` against `POWER_ROLL_RANGE.end`. Left alone, a
  maxed ring at 1.15 reports polish room forever and Polishing charges
  sand for a no-op — the exact 2026-08-17 live bug, re-opened for four
  slots. Now reads `roll_range_for_slot`.
- `migrations.rs`'s `all_five_slots_sharing_the_same_unique...` asserted
  a hardcoded `5`; the only test in the suite that failed on slot count
  alone. Renamed and now counts `EQUIP_SLOTS.len()`.
- `migrate_power_roll_backfill` reverse-engineers a roll as
  `power / (base * tier)` — linear, so it would mis-recover a new slot's
  roll. Inert today (marker-gated, already run, and no legacy items exist
  in the new slots) and deliberately not touched, since it is a
  historical one-shot. Worth knowing if it is ever re-run.

### BOARD — recorded deliberately, not acted on

1. **Crit stops being a build choice (spec §8.5b).** With the implicits
   guaranteed, 74% of a typical character's crit chance and 59% of their
   crit multiplier arrive automatically with a filled ring/amulet slot,
   at every tier — the ratio is constant because implicit and rolled
   contributions ride the same `f(T)`. The consequence worth deciding:
   **rolling `CritChance` or `CritMultiplier` on gear is now the worst
   affix outcome in the game**, because it tops up a stat the character
   already has a large guaranteed base in while every other affix starts
   from zero. That is a drop-quality regression nobody ordered. The fix
   is a spec change (which stats the implicits grant), not an
   implementation choice — owner ruling 2026-09-03 is to see it live
   first and decide deliberately.
2. **Ring1 cannot be recombined with Ring2.** Automatic consequence of
   distinct variants plus recombine's same-slot rule. ACCEPTED and
   recorded rather than fixed (owner ruling 2026-09-03): unblocking it
   means loosening the same-slot rule, and the duplicate-unique guard
   leans on that rule. A minor inconvenience is a better trade than a
   hole in unique-duplication protection.
3. **The loot dial is coupled to tier inflation.** Nine slots cut each
   slot's drop share 20% → 11.1% and take the fill-every-slot-once
   expectation from 11.4 to 25.5 drops. Ruling: ship as-is, do not
   compensate in code — `loot_mult` is live-tunable and the call can be
   made with data. **The coupling whoever reaches for that dial must
   know:** raising `loot_mult` erases the only real brake on craft-driven
   tier growth. At a fixed stage, dust income is flat while crafting cost
   climbs, and that gap is the sole thing limiting tier inflation.
   Raising loot trades a loot-feel problem for a tier-inflation one. It
   is not free. Recorded in `EQUIP_SLOTS`'s own doc too, since that is
   where someone will be standing when they think of it.
4. **The ×1.31 is a buff, not an offset (spec §8.4).** The four slots are
   worth ×1.05 at T=1 rising to ×1.65 at T=200; the crit halving is worth
   ×0.995 to ×1.000 and is invisible in this range. They are not in
   balance and are barely in the same conversation. Noted at fit-report
   time that "characters who took the cut" and "characters who get the
   buff" are different populations — owner ruling: true in principle and
   nearly empty in practice, since world 2 is one day old at stage 13
   with 14 characters and nobody has an established set. That is the
   argument for shipping it now rather than in three weeks.

### CROSS-REPO — owner must relay to kibukah, this session cannot fix it

`C:\PathOfDust_Desktop-replay` is a separate repo with a separate
maintainer. Its equipment rendering is not data-driven: it hardcodes the
five-slot list in `bag.html:655`, `builds.html:317`,
`solver/advisor-core.mjs:416`, `item-codec.js:50` and
`extension/shared/item-codec.js:50`, plus a five-key `SLOT_EMOJI` in
`bag.html:215`, `character.html:116` and `extension/card.js:6`.

The message: **the four new wire strings are `ring1`, `ring2`, `amulet`,
`pants`** (`EquipSlot` is `#[serde(rename_all = "lowercase")]`), and
**`item-codec.js`'s `SLOTS` array is POSITIONAL — append only, never
insert or reorder.** `shrink()` stores `SLOTS.indexOf(it.s)` as an
integer and the payload version is chosen by size, not by schema, so
reordering silently decodes every previously-shared v2 item link to the
WRONG slot with no error and no version bump. Appending is safe for old
links, and until the append happens the code already degrades gracefully
(`s < 0 ? it.s : s` stores the raw string — a longer payload, not a wrong
one). The same append-only constraint applies to `LABELS` in that codec,
which is a positional affix-label array and already lacks `echo`.
Separately and already true before this change: `advisor-core.mjs`
carries its own damage model with no knowledge of `f(T)`, the halved crit
coefficient, or any implicit, and will keep giving confident wrong advice.

### Patch-notes draft — NOT written to the box

Deploy session: this is a draft for `C:/PathofDust/patch-notes.json`,
deliberately not written from here.

> **Four new gear slots.** You can now equip two Rings, an Amulet and
> Pants alongside your weapon, helm, body, gloves and boots. Rings give
> crit chance, the Amulet gives crit damage, Pants give max hp — and
> unlike an affix, that stat is guaranteed on every one of them. Nobody
> starts with them: they drop like any other gear, and everyone starts
> the hunt from zero.
>
> Two honest notes. **Nine slots means each individual slot drops less
> often** — a weapon now shows up about 44% less than it did, and filling
> out a full set takes roughly twice as many drops. That is the cost of
> having more to chase, and we are watching the drop rate.
>
> And the obvious question: **yes, this is a buff, and no, it is not an
> apology for last night.** The affix cut is what makes gear scale sanely
> at high tiers, and it stays. These slots are a separate thing — more
> places to put gear, which is worth about 30% more power across the
> range once you have filled them. Both land on the same characters
> because world 2 is one day old and nobody has a set worth mourning yet.
> That timing is deliberate: this is the cheapest moment it will ever be.

### 2026-09-03 — GEAR-SLOTS deploy record (release `gear-slots`)

Fourth and last of the queued releases, deployed last on the owner's
instruction: largest surface, a net ×1.31 power buff, and it should land
on a world whose pacing has settled rather than into a transient. It did
— stage had recovered to 10 and boss W/L stood at 14/6 at the moment of
the swap.

| | |
|---|---|
| merge | `35801b7` |
| binary before | `417b00f4…f77959` |
| binary after | `28cf215130b65bcbae1b43ac56380c7879bdcc5bf8390bfbe27a5f761f094372` |
| downtime | **0.16 s** |
| suite on the box | **824 passed / 0 failed / 0 ignored, 37 suites** — `cargo test --release --workspace --quiet -j 4` in `/root/deploy-src-gear-slots` |
| rollback slot | `/var/backups/pathofdust/deploy-pre-gear-slots/game.pre-gear-slots` |

**Suite arithmetic.** 807 + 17 = 824. The branch adds exactly **17**
`#[test]` attributes and removes none, counted from the diff. The order
stated 13 new tests; that undercounts by four. Recorded because a count
that does not reconcile is the thing this project has been burned by, and
this one does reconcile — against the source, not against a memory of it.

**All four releases verified coexisting in the built tree before the
build ran**, since the wide `adventure_web.rs` diff auto-merged against
three earlier releases that all touched that file: the curve's
`affix_tier_growth_ratio` (4 refs), branch 1's `bug_reports.rs`, branch
2's `login_throttle_delay`, branch 3's `apply_craft_tier_bump` still
delegating to `sync_tier_to`, and `EQUIP_SLOTS: [EquipSlot; 9]`.

Conflicts were docs only. Golden corpus untouched — not one fixture in
the diff, as claimed.

### THE STOP-CONDITION FIRED, AND IT WAS RIGHT TO

The order carried an explicit stop: *"existing characters get four empty
slots via serde defaults, no migration and no marker. If anything on the
box suggests otherwise, stop."*

Something did. Post-deploy verification found **all 18 characters with
all four new slots FILLED** — 72 tier-1 items (Worn Signet, Rusty Loop,
Crude Talisman, Worn Greaves), implicits correctly rolled, already
persisted to disk. Work stopped there and the finding went to the owner
before anything else was touched.

**The branch is innocent and its code is correct.** `Character::new` sets
`ring1: None`, and the branch ships a test asserting exactly the
behaviour the order described:

```
assert!(bare.ring1.is_none() && ..., "the starter kit must NOT fill the
new slots (owner ruling 2026-09-03)");
```

That test passes. It is not wrong, and it did not fail to catch this,
because what it asserts is true: the starter kit does not fill the slots.

**The cause is pre-existing master code**, in `AdventureManager`'s
startup path (`manager.rs:2130`), dating back to the Stage 1 crate
extraction (`2fa9445`). Branch 4 never touches `manager.rs` at all:

```rust
// One-time backfill for anyone who joined before Character::new()
// started handing out a full starter kit - fills only EMPTY slots with
// a basic tier-1 item, never touching gear they already have.
// Idempotent: once everyone has all 5 slots filled, this is just a fast
// no-op scan on every future startup.
for slot in EQUIP_SLOTS {
    if character.equipped(slot).is_none() {
        character.equip(generate_item_at_tier(slot, 1, &mut rng));
    }
}
```

It iterates `EQUIP_SLOTS`. This release took that array from 5 to 9, and
a loop whose own comment guarantees it is a permanent no-op re-armed and
granted four items to every character on first startup.

### RULING (owner, 2026-09-03): ACCEPT. Player data is not touched.

The 72 items stay. They are tier 1, uniform across all 18 characters,
worth roughly one fight of loot, and they partly offset the drop dilution
that shipped in the same release. **Stripping them would mean
hand-editing live player data outside a marker-guarded migration, which
the affix-curve ruling already established is the larger risk**, and a
rollback would spend real resolved fights to undo a fair and trivial
grant.

Confirmed not harmful: existing gear untouched (weapon tiers still 13–22),
service `active`, `NRestarts=0`, all seven §13B.5 checks green, tunnel
200, zero panics or ERROR lines.

**The loop is deliberately NOT fixed in this release.** A separate
session owns that branch, and it deploys ahead of the catch-up fix. Until
it lands, the live binary carries this OPEN behaviour: `Character::new`
correctly leaves the four new slots empty, so **every new character will
have them auto-filled at the next service restart**, and any future slot
addition re-arms the same loop again.

### THE GENERALISABLE LESSON, which is bigger than this incident

**A one-time migration guarded by an INVARIANT rather than by STATE will
re-arm the moment the invariant changes.** The comment said permanent
no-op and was *true when written*. Growing an array falsified it
silently: no compiler error, no test failure, no marker file to check,
nothing to notice. Contrast every other migration in this codebase, which
is guarded by a marker file on disk — STATE, which cannot be falsified by
editing a constant somewhere else.

The rule that follows: **a migration whose guard is "this condition can
no longer occur" is not guarded.** If it must not run twice, give it a
marker.

**This was the eighth hardcoded-or-implicit five-slot assumption in this
release, and the only one that reached production.** The other seven were
found and fixed while the branch was being written:

| | where | outcome |
|---|---|---|
| 1–6 | six hardcoded five-slot lists | replaced by `DISPLAY_SLOTS` |
| 7 | `ALL_SLOTS` in `affix.rs` | fixed |
| 8 | `has_polish_room` | fixed |
| **9** | **the startup backfill's `EQUIP_SLOTS` loop** | **reached production** |

The seven that were caught were all *in the files the branch was already
editing*. The one that escaped was in a file the branch never opened.
That is the shape of the trap, and it is worth stating plainly: a
widening constant's blast radius is every consumer of that constant, not
every file in the diff.

### Patch notes

Two sections at the top of the September 3 block (10 → 12), pre-edit copy
`/root/patch-notes.pre-gear-slots.json`. The second is headed **"Drops
are spread across nine slots now, so any one slot drops less often. This
is a nerf"** — each slot's share falls 20% → 11.1%, a weapon drops ~44%
less often, and filling every slot once goes from ~11 drops to ~25. The
note says it is deliberate, uncompensated, that `loot_mult` is a dial to
be set from real data, and asks for feedback. It also states the upside
honestly so players can judge the trade.

The notes were written before the backfill was discovered and describe
the four slots as starting empty. That is now wrong for the 18 existing
characters, who found them filled. Not amended here — patch notes are a
dated record of what was announced, and a correction belongs in the next
entry rather than in a silent rewrite of this one.


### 2026-09-03 — BACKFILL-BOUND: the starter-kit backfill is guarded by state, not by an invariant

Branch `fix/backfill-bound` off `origin/master` (`35801b7`). Not merged,
not deployed — session c holds that authority. Committed and pushed in
two steps (`eb2e9f5` WIP, then the completion) because the machine lost
power twice today and an unpushed branch is one power cut from gone.

#### What was wrong

`AdventureManager::new`'s starter-kit backfill iterated `EQUIP_SLOTS` and
claimed in its own comment to be *"idempotent: once everyone has all 5
slots filled, this is just a fast no-op scan on every future startup."*
That was not a guard. It was an **invariant**, resting on the data
converging on a state where the `if` never matches. The gear-slots
release took `EQUIP_SLOTS` from 5 to 9 and falsified it: the loop
re-armed, granted **72 tier-1 items across 18 live characters**, and
persisted them. Nothing failed to compile and no test failed.

Worse forward, and the reason this was urgent rather than merely untidy:
`Character::new` leaves the four §8 slots empty by owner ruling, so
**every future character would have had them auto-filled at the next
service restart** — the ruling permanently defeated by a startup path
nobody would think to look at.

The 72 items are KEPT by owner ruling. No migration removes them.

#### The fix: two halves, neither redundant

1. **The marker** (`adventure-starter-kit-backfill-marker.json`), same
   shape as the eight other one-off grants in that fn. This is the state
   guard: once written, the loop cannot run again whatever `EQUIP_SLOTS`
   becomes. **Written even when nothing changed** — a marker written only
   on a change would leave the guard unarmed on exactly the installs
   where the loop was a no-op, which is every install that matters.
2. **A frozen `STARTER_KIT_BACKFILL_SLOTS: [EquipSlot; 5]`** replacing
   `EQUIP_SLOTS` in the loop.

Half 2 is the part worth recording, because the marker alone looks
sufficient and is not. **The marker does not exist on the live box yet.**
The first restart after this ships finds no marker and runs the loop one
final time. For the 18 characters already granted that is a genuine
no-op — but any character created since the gear-slots release has four
empty slots and would be filled on that last pass, **defeating the ruling
one more time on the way to enforcing it.** Over the frozen five the
final run is a provable no-op.

Alternatives rejected: marker only (grants on deploy day, as above);
frozen list only (structurally safe, but leaves the migration guarded by
an invariant a future editor swaps back to `EQUIP_SLOTS` "for
consistency"); the sprite-count pattern of storing `EQUIP_SLOTS.len()`
and firing on growth (right for a standing policy that SHOULD re-arm,
which is why it is correct for `ALL_SPRITES` — here re-arming on growth
*is* the defect); deleting the loop (defensible, its population is empty,
but the order said bound, not remove).

#### The test, and the mutation check

`game/tests/starter_kit_backfill_bound.rs`, through the real constructor,
asserting against what lands on disk. Phase 1 (marker absent): the five
starter-kit slots get filled, and every slot **outside** them stays empty
— that set is computed from `EQUIP_SLOTS` at runtime, so a future tenth
slot lands in it automatically. Phase 2 (marker present): reopen a
starter-kit slot, restart, assert it is left alone. The test states its
own `ORIGINAL_FIVE` rather than importing the production constant, so it
verifies behaviour instead of restating the implementation.

**Both halves were verified by mutation, not by assertion alone**, and
this is the part that should be copied by anyone writing a guard here:

| mutation | result |
|---|---|
| loop restored to `EQUIP_SLOTS` | FAILS — *"the startup backfill filled Ring1, which is not a starter-kit slot"* |
| marker check replaced with `if true` | FAILS — *"the backfill ran a second time"* |

A guard without a test that proves the guard holds is the same class of
promise the original comment made.

#### Survey — is any OTHER startup migration guarded by an invariant?

Asked for explicitly; this was the eighth implicit five-slot assumption
found in one release. Every path in `AdventureManager::new`:

| migration | guard | verdict |
|---|---|---|
| starter-kit backfill | invariant | **the defect — the only one** |
| `run_item_migrations` | state, per-entry marker | ok |
| crit-reforge equipped backfill | state | ok — iterates `EQUIP_SLOTS`, sealed |
| `run_character_migrations` | state, per-entry marker | ok |
| craft-token backfill + v2 | state | ok |
| pity launch grant | state | ok |
| wings launch grant | state | ok |
| passive key rename | state | ok |
| kibukah compensation | state | ok — iterates `EQUIP_SLOTS`, sealed |
| sprite-growth free change | state (stored count) | ok — re-arms on growth **by design** |
| `run_storage_migration` | state | ok |
| `highest_stage` backfill | invariant (`max`) | **not the same class** |

**Answer: no other startup migration in `manager.rs` is guarded by a
falsifiable invariant.** `highest_stage` is invariant-guarded but `max`
is a fixed point, so it is genuinely idempotent for any future value, and
it must stay unguarded to self-heal a world file hand-edited backwards.
Three migrations iterate `EQUIP_SLOTS`, but two of them are sealed behind
markers that have already fired.

#### Two lying comments, fixed rather than filed

Owner ruled these get fixed in this change, not journalled and left:

- The kibukah compensation block said *"A full 5-slot item assortment"*.
  Its loop reads `EQUIP_SLOTS`, so it would grant 9 if its marker were
  ever cleared. Now says so explicitly.
- `Character::new`'s first doc line said *"a basic tier-1 item in every
  slot"*, contradicted by its own comment nine lines below. Now names the
  five starter-kit slots and points at the backfill's frozen list.

Both are exactly the class of comment-that-lies this project has been
bitten by repeatedly, and both sit in the file a reader opens when the
next slot is added.

Suite: **825 passed, 0 failed**, `cargo test --release --workspace
--quiet -j 4 --target-dir target-backfill`. Clippy clean on touched code.

CORRECTION to two earlier counts in this session's own records: the
2026-09-03 CATCHUP-FIX journal entry and commit `ed5c529` report "796
passed". That number came from an `awk` field split that mis-parsed the
`test result:` line and undercounted. The verdict it supported — 0
failures — was independently confirmed by grep and stands. The true total
for that run was higher; anyone re-running that branch and getting a
different number is seeing the counting bug, not drift.

#### Patch note draft — NOT written to the box

> **New characters keep their empty gear slots**
> - A startup routine meant for players who joined before starter kits
>   existed had been quietly filling every empty gear slot with a basic
>   item. When rings, amulet and pants were added it started filling
>   those too — 72 free items went out on 2026-09-03.
> - **Those items are yours and are not being taken back.**
> - From now on a new character starts with five items and four empty
>   slots (two rings, amulet, pants). You fill those from drops, which is
>   how it was meant to work.

### 2026-09-03 — BACKFILL-BOUND deploy record (release `backfill-bound`)

First of four in the deploy-queue order. The release that stops the
startup backfill re-arming.

| | |
|---|---|
| master commit deployed | `fac1215` |
| merge | `cd8f0e6` (branch `fix/backfill-bound`, rebased from `bbae4bc`) |
| binary before | `28cf2151…f94372` |
| binary after | `d793678d035ffb71b54c641282e8d6ef29b1f8d2851ab103dbfe37affc3980f6` |
| downtime | **0.35 s** |
| suite on the box | **825 passed / 0 failed / 0 ignored, 38 suites** — `cargo test --release --workspace --quiet -j 4` |
| rollback slot | `/var/backups/pathofdust/deploy-pre-backfill-bound/` (**old naming — `fix/rollback-slot-collision` is queue item 2, not yet live**) |

824 + 1 = 825 and 37 + 1 = 38: the branch adds one test file with one
test. Both reconcile.

### The verification that mattered, and how it was made falsifiable

The change guards the loop with a marker **and** freezes it to the five
slots it was written for. The marker did not exist on the box, so the
first startup after the swap necessarily ran the loop one last time —
that pass is the thing that had to be proved harmless.

`xayse`, the one character created since the gear-slots release, is the
witness. After the deploy:

```
xayse | empty spec-8 slots: ['ring1', 'amulet']
        original five:      weapon, helm, body, gloves, boots  (all filled)
```

**If the final pass had iterated `EQUIP_SLOTS`, that list would be empty
— all four §8 slots would have been filled.** Two are still empty, and
the two that are filled (`ring2`, `pants`) came from drops: the service
had `NRestarts=0` since 08:27, so no startup ran between that character's
creation and this deploy. The frozen list held, and the proof does not
depend on reading the code.

The marker is now written (`true`), so the loop cannot run again whatever
`EQUIP_SLOTS` becomes.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **19 = 19** |
| 4 | live sha256 | `d793678d…` = candidate |
| 5 | `/characters`, `/passives` (auth) | 200 / 80,369 B, 200 / 91,110 B |
| 6 | anon `/admin/tunables` | **404**, 73,730 B |
| 7 | anon `POST /api/commands/join` | **404** |

Public tunnel HTTP 200 in 0.100 s. Zero panics or ERROR lines, and no
backfill persist error in the journal.

### The patch note was CORRECTED, not supplemented — and this reverses an earlier call

**Owner ruling, 2026-09-03**, overriding the position taken in the
gear-slots deploy record above, which argued that a dated announcement
should not be rewritten and that a correction belonged in the next
entry. That entry stays as written; **this is the dated correction to
it**, and the ruling is the reason:

> *A player reading the old sentence and looking at a filled ring slot
> concludes the game is broken.*

That is the deciding fact, and it beats the archival argument. The
supplement-elsewhere approach helps a reader who finds the supplement;
the false sentence is the one they are actually reading, next to the
evidence contradicting it. **Correct the sentence a player is standing
in front of.** Archival integrity is served by saying the note was
corrected and when — which the new text does, in the player's own view —
not by leaving a falsehood in place.

Verified live on `/patch-notes` (217,491 B): the old sentence renders
**0** times, the correction **1**, the new section **1**. Pre-edit copy
at `/root/patch-notes.pre-backfill-bound.json`. The correction names the
date, says the items are kept, and says characters made from now on
really do start empty.

### Completed on the way past — the marker was not in the backup allow-list

The branch adds a new marker file and did not add it to
`backup-game-data.sh`'s hand-maintained `MARKER_FILES`. Added here.

Nothing was at risk: the drift check stages every marker the glob finds
whether listed or not. But that leads to a second, worse finding —

**A comment in `backup-game-data.sh` was lying about the script's own
behaviour.** It read: *"the check near the bottom of this script will
TELL you when a marker on disk is missing from here, but it will not back
the file up for you."* The code 145 lines below says the opposite —
`Anything the glob finds IS backed up regardless`, and
`for f in "${DRIFT[@]}"; do stage_one "$f"; done`.

The false claim was repeated into the affix-curve journal entry
(2026-09-03, "there is a drift check … but it warns — it does not back
the file up") and believed from there. That is how a wrong sentence about
a recovery path propagates: written once, quoted once, then treated as
established. Corrected in place with a dated note rather than a silent
edit, because the false version is what people have been reading.

The distinction worth keeping: **this marker is a GUARD, not a record.**
Most markers here say "a migration ran". This one says "the loop must not
run". A restore that brought back characters without it would re-arm the
loop — harmless now that it is frozen to five slots every character has
filled, but the marker is what keeps it that way.

### Not done, deliberately

The 72 items granted on 2026-09-03 stay, per the owner ruling recorded in
the gear-slots entry. This release stops it recurring; it does not undo
it.
---

## 2026-09-03 — BOARD: two items owned by nobody, recorded so they stop being rediscovered

Neither is being worked. Both were found during the four-release deploy
day and would otherwise be found again by the next session that trips
over them.

### OWNED BY NOBODY — the off-box backup puller stalls for minutes at a time

`C:\pod-backup-pull\pull-linux-backups.ps1` intermittently stalls on a
single archive transfer. Two stalls measured directly on 2026-09-03:
**4 m 09 s** and **7 m 05 s**, against a normal per-archive time of
**7–8 s** for the same 4.5 MB files over the same link.

**This is not a fault in the puller, and it is not data loss** — both
stalls recovered on their own and the runs completed with every archive
verified. It matters because of what it *looks* like: the 7 m 05 s stall
was read as a dead run by an observer checking mid-flight, and produced a
detailed and entirely wrong incident report. A transfer that hangs for
seven minutes and then succeeds is indistinguishable from one that has
died, and that ambiguity has already cost one investigation.

**Prime suspect, UNTESTED:** Windows Defender real-time protection is on,
archive scanning is enabled, and `C:\pod-backups-linux` has **no
exclusion** — so every 4.5 MB `.tar.gz` is unpacked and scanned as it
lands. Measured on the box: `Get-MpPreference` lists only
`C:\Program Files (x86)\Diablo II` and a uTorrent path.

Cheapest experiment if anyone picks it up: time a pull with a temporary
exclusion on `C:\pod-backups-linux`, compare against the 7–8 s baseline,
remove the exclusion. **Nobody is on this.** Do not treat a stalled
transfer as a failure without checking whether it later completed —
`pull end` in `pull-linux-backups.log` is the only authority.

### FOR THE NEXT RELEASE, whoever ships it — one line of patch-note correction

`gear-slots` shipped a patch note saying the four new slots start empty.
**That is now wrong for the 18 characters that existed at the time**,
which all loaded with the slots filled by the startup backfill (see the
gear-slots deploy record above and the owner ruling accepting the 72
items).

The dated announcement is deliberately NOT being rewritten — patch notes
record what was announced on the day, and silently editing one is how a
record stops being trustworthy. **The correction belongs in the next
release's notes as one line**, whichever release that is. Suggested
wording, to be adjusted to fit:

> *Correction to yesterday's note: the four new gear slots did not start
> empty for characters that already existed — everyone was given a basic
> tier-1 item in each. That was not intended, it is being kept, and it is
> being prevented from happening again.*

This is a note to the NEXT deploy session, not a task with an owner.

---

## 2026-09-03 — ROLLBACK-SLOT COLLISION (branch `fix/rollback-slot-collision`)

Not deployed. Changes no game code — two shell scripts and §13B.

### The bug, and why it is worse than losing a file

`deploy-linux.sh` named its rollback slot `deploy-pre-$NAME` — the release
name and nothing else — and created it with `mkdir -p`, which succeeds
silently on a directory that already exists. Redeploying a release name
therefore landed on the previous slot and overwrote three things: the
rollback binary, `SHA256SUMS`, and (by merge rather than replace) the
pinned fight corpus.

It fired on 2026-09-03, when `player-facing-batch` was deployed onto the
slot the previous evening's incident had created. **It was harmless only
because the two binaries were byte-identical by coincidence** — both were
`ab49d679`, once because the affix-curve-restore had just installed it and
once because it was still live.

**The sentence that justifies the whole change:** the collision overwrites
`SHA256SUMS` *alongside* the binary, so `rollback-linux.sh` would have
rolled forward to the WRONG binary and **passed its integrity check while
reporting success**. A lost file announces itself. A lost file with a
matching checksum does not. The verification travels with the corruption,
which is what separates this from ordinary data loss.

Same class as the shared `/root/deploy-src` that let one deploy delete
another's build: a single fixed path shared between actors that cannot
see each other.

### A second defect, found while measuring the first

**`ls -lt` lies about slot order, and lies plausibly.** `deploy-linux.sh`
saves the slot with `cp -a`, which preserves the source timestamp — so a
slot's binary carries the mtime of the binary it *saved*, i.e. its
**predecessor's** install time:

```
deploy-pre-gear-slots              dir 08:27:56 | binary 08:09:23
deploy-pre-craft-tier-bump         dir 08:09:23 | binary 07:50:34
deploy-pre-small-isolated-defects  dir 07:50:34 | binary 05:54:32
```

Every entry is off by one deploy. Not obviously wrong — *consistently*
wrong, which is the worse kind under pressure, because it looks like an
answer. And the one field that is right, the directory mtime, is
destroyed by any collision. Written into §13B where somebody would reach
for it.

### What shipped

| | |
|---|---|
| slot name | `deploy-pre-<YYYYMMDD-HHMMSS>-<release-name>` |
| ordering | timestamp FIRST, so lexical sort **is** chronological sort |
| convention | matches `pod-backup-YYYYMMDD-HHMMSS` — one convention on the box, not two |
| guard | `mkdir` without `-p` + explicit `-e` check → **refuses**, never overwrites |
| commit | recorded inside `SHA256SUMS` as a `#` comment, with release/slot/save-time |
| newest | `deploy-pre-LATEST` symlink, relative target, repointed by `mv -T` (rename(2), atomic) **only after the live hash is confirmed** |

Commit SHA was considered and rejected **for the name**: it does not sort,
it still collides when the same commit is redeployed, and it means nothing
to a human at 3am. Inside the slot it answers a different and useful
question, so that is where it went. The optional third argument to
`deploy-linux.sh` carries it, because the build tree is a `git archive`
extraction with no `.git` to derive it from.

`rollback-linux.sh` became a resolver rather than a path template:

| form | behaviour |
|---|---|
| *(no argument)* | follow `LATEST` — the incident form, "undo what just happened" |
| `<release-name>` | newest matching slot, **across both the new and legacy shapes** |
| `<slot-dir-name>` | exact |
| `--list` | every slot, newest first, with hash, date and shape |

It prints the chosen slot, **why** it was chosen, the verified hash and
the currently-live hash before stopping anything — the part that makes it
safe to use in a hurry.

### The 14 existing slots were deliberately left alone

Not renamed, not moved, not deleted; the resolver understands the legacy
shape. **Renaming recovery material to tidy it is the same class of risk
as the bug being fixed** — the one moment a rename is unsafe is the one
moment you need the slot. They age out on their own.

**No pruning was added, deliberately.** Deleting rollback binaries is a
destructive default, 273 G free makes it moot at ~18 MB per deploy, and a
keep-N would have been deleting recovery material *during* the incident
that prompted this. If a bound is ever wanted it is its own change,
keep-N with N ≥ 20, never touching `LATEST`.

### §13B.8 was the half that would have been missed

Template-only releases build the slot **by hand**, in the document, so
fixing only the script would have left the collision live and hidden it
better. `deploy-pre-craft-confirm` is on the box now, waiting for a second
template release of the same name. It gets the same scheme and the same
`mkdir`-without-`-p` guard, plus one rule the binary path does not need:
**never repoint `LATEST` at a template slot.** It holds no `game.pre-*`,
so pointing the binary-rollback path at one would arm a trap for whoever
reaches for the no-argument form. `rollback-linux.sh` also refuses such a
slot by name, with a message that says which procedure they want instead.

### Verification — rehearsed, not reasoned

Both scripts were run against a scratch backup root with fake binaries and
`systemctl`/`curl`/`chown` stubbed, on the box, so the real GNU
`mv -T`/`stat`/`date` behaviour was exercised. **25 checks, 25 passed.**
The two that matter:

- **The fix:** deploying the same release name twice produced **two
  distinct slots**, and the first slot's binary was **byte-unchanged**.
  Under the old scheme this is exactly where the recovery path died.
- **The guard:** forcing an exact slot collision was **refused with a
  non-zero exit and a message naming the reason**, and the pre-existing
  slot's content was verified untouched afterwards.

Also covered: legacy-slot resolution by release name against a
legacy-shaped `SHA256SUMS` with no comment lines; `--list` ordering and
its `LATEST` line; the no-argument form following `LATEST`; a
template-only slot refused clearly; and a release name matching multiple
slots choosing the newest and saying so.

**The rehearsal earned its cost — it found two defects that reading the
script did not:**

1. **Ordering was non-deterministic on a tied timestamp.** Two slots can
   share a second, and with equal keys `sort` fell through to comparing
   slot *names*, i.e. alphabetically. The sort key now carries a trailing
   rank digit (1 = stamped, 0 = legacy) so a tie resolves in favour of the
   slot whose timestamp is exact rather than the one whose timestamp is
   only its directory's mtime.
2. **`${bin:+$(basename "$bin")}${bin:-<none>}` printed both branches.**
   It reads like an if/else and is not: when `bin` is set, `:+` yields the
   basename *and* `:-` yields the value rather than the fallback, so the
   column came out as `game.pre-alpha/full/path/to/game.pre-alpha`.
   Replaced with a plain `if`.

A third finding was a bug in the **test**, not the script: the ordering
check asserted that a named slot was newest, but the harness keeps
creating slots after that point, so a later one legitimately took the
lead. The assertion now checks the property that actually matters — that
rows come out in descending time order — rather than naming an expected
winner. Worth recording because an assertion that encodes an expected
*answer* rather than the *property* goes stale the moment the fixture
grows.

The harness is not committed; it is a scratch artifact. The technique is
the reusable part and is written down here: copy the real scripts, `sed`
the three production paths to a scratch root, prepend no-op
`systemctl`/`chown` and a `curl` that echoes 200, then construct the
failure and watch the old code fail it.

### Not done here

`deploy-linux.sh` still keys nothing on the *content* of what it saves —
two slots holding byte-identical binaries are stored twice. Noted, not
fixed; deduplication would trade a simple recovery path for a clever one,
which is the wrong trade for this file.

### 2026-09-03, addendum — two general rules from the rollback-slot work (owner ruling)

Both were approved as general rules rather than as local conveniences, so
they are recorded as rules rather than left inside the change that
prompted them.

**1. Rank the recorded fact above the inferred one.** When
`rollback-linux.sh` sorts slots and two share a timestamp, the stamped
slot wins over the legacy slot. That is not a tiebreak convenience. A
stamped slot's time is a **fact recorded at the moment of the deploy** —
written into the name by the process that did the thing. A legacy slot's
apparent time is its **directory mtime**, which is an *inference* about
when the deploy happened, and one this very codebase has already shown to
be unreliable: `cp -a` preserves source timestamps, so mtimes here are
routinely somebody else's. **Where a fact and an inference disagree, or
tie, the fact ranks first.** The general form: prefer the value the
system wrote down at the time over the value you can deduce afterwards.

**2. An assertion must encode the PROPERTY, not the expected ANSWER.**
The harness's ordering check originally asserted "slot X is newest". It
passed, then broke — not because the code regressed but because a later
test created another slot, and X was legitimately no longer newest. The
assertion now checks that rows descend in time, which is what was
actually meant. An assertion that names an expected answer is pinned to
the fixture as it stood the day it was written, and goes stale the moment
the fixture grows.

**This is the same defect class as pinning a gate to a number a human
typed, rather than to a value the system produces, and it has now cost
this project four times:**

| # | where | the pinned value | how it broke |
|---|---|---|---|
| 1 | `admin_tunables_splash_http.rs` | a hand-maintained POST field list | a superset body kept passing while the page stopped rendering a field; every real browser save 422'd silently |
| 2 | the suite baseline | "581 passed", then "758" | a stale number read a correct result as a failure, and a real regression as fine |
| 3 | `manager.rs` startup backfill | *"once everyone has all **5** slots filled, this is a no-op"* | growing `EQUIP_SLOTS` to 9 falsified the comment and re-armed the loop; 72 items granted |
| 4 | the harness ordering check | "slot X is newest" | a later fixture addition made a different slot newest |

The cure is the same in all four: **derive the expectation from the
system at the moment of the check.** Scrape the field names out of the
rendered page. Count the tests you actually ran. Guard a migration with a
marker on disk, which is state, not with a claim about an invariant.
Assert the ordering property rather than naming the winner.

Stated as a rule, so it is checkable in review: *if an assertion, guard or
baseline contains a literal that a human typed from observation, ask what
falsifies it and whether anything would notice.* Four times, nothing did.

### 2026-09-03 — ROLLBACK-SLOT-COLLISION deploy record (release `rollback-slot-collision`)

Queue item 2. **No binary swap, no service stop, no downtime** — this
release changes two shell scripts, `REFACTOR_PLAN` §13B/§13B.8, a harness
and docs. It is the first release on this box deployed as a
scripts-and-docs change rather than a binary swap.

| | |
|---|---|
| master commit deployed | `aace3b1` |
| binary before / after | `d793678d…3980f6` — **unchanged, bit-identical** |
| downtime | **none** — `systemctl` was not invoked |
| `NRestarts` | `0`, still, and the service has been up since 13:47:02 |
| scripts installed | `deploy-linux.sh` `3e5570017cf0…`, `rollback-linux.sh` `fe08d80abbf8…` |
| previous scripts | `/root/deploy-linux.sh.pre-rollback-slot-collision`, `/root/rollback-linux.sh.pre-rollback-slot-collision` |

`deploy-linux.sh` was **not** used to deploy this. It aborts when the
candidate hash equals the live one, and that abort is correct here — the
same category §13B.1 already names. `bin/` is not refreshed by the deploy
script, so both files were installed by hand, exactly as
`backup-game-data.sh` was for the previous two releases.

### Shipped on a red suite, deliberately, on an explicit and narrow licence

The full suite was **783 passed / 1 failed** on this tree. It was shipped
anyway, on the owner's ruling, and the reasoning is recorded because the
verdict on its own would be a bad precedent:

> *"The suite must be green" is not the actual requirement. It is a proxy
> for "this change did not break anything," and a proxy is only worth what
> it measures.*

The real question was answered directly and more strongly than the proxy
could: `diff -r` over `game/src` between this tree and the one already
serving players returns **IDENTICAL**, and both binaries hash
`d793678d…`. **A test suite cannot tell you more about a binary than the
binary's own hash does.** The suite is doubly irrelevant here — it does
not exercise shell scripts at all. The real gate for this change is the
rehearsal harness, and that is 25/25.

The failing test is `an_empty_new_slot_contributes_exactly_nothing`, a
gear-slots unit test live in production since 08:27, flaky at ~8%
(25 isolated runs, 23/2). Diagnosed here, **fixed by window b** — it sits
in `new_slot_migration_tests`, which b owns and is editing.

**THE LICENCE DOES NOT GENERALISE, and it is written down so nobody
quotes it later as precedent.** It applies to a change whose bit-identity
is *provable* and to nothing else. Every other item in the queue alters
the binary and waits for a green run. **If you find yourself reaching for
this reasoning on a change that alters the binary, you have misread it —
stop and ask.**

### Verified against production, not only against the harness

`rollback-linux.sh --list` was run against the real box — read-only, and
the only form of the command that is safe to run on production, since
every other form performs a rollback. All 15 slots resolved:

- **Ordering is correct against the real deploy chain**: backfill-bound
  13:47 → gear-slots 08:27:56 → craft-tier-bump 08:09:23 →
  small-isolated-defects 07:50:34 → affix-curve-restore 19:24:51 →
  player-facing-batch 19:20:55 → affix-tier-curve 19:19:51 → … That is
  the actual order those releases happened.
- **Each slot holds its predecessor's binary**, which is the invariant
  that makes a rollback slot useful: `deploy-pre-backfill-bound` holds
  `28cf2151` (the gear-slots binary), `deploy-pre-gear-slots` holds
  `417b00f4` (craft-tier-bump's), and so on down the chain.
- **The template-only slot is flagged, not crashed on**:
  `deploy-pre-craft-confirm` prints `<none - template-only>` with no
  hash, which is §13B.8's slot shape being handled rather than tripped
  over.
- `LATEST -> (not set yet)`, correctly — no deploy has yet run under the
  timestamped scheme. The next binary deploy creates it.

All 15 legacy slots remain, none renamed or removed, and all 15 still
resolve by release name.

### The gap between now and the next binary deploy, stated plainly

Until the next binary deploy writes `deploy-pre-LATEST`, the no-argument
form of `rollback-linux.sh` has nothing to follow. It does not guess: it
refuses, says why, and prints the full list so the operator can choose.
The most recent slot is `deploy-pre-backfill-bound`, reachable as
`rollback-linux.sh backfill-bound`. That is a working recovery path today,
and it is the error message itself that hands it to you — which is the
behaviour the design was aiming at.

### 2026-09-03 — ~92% is the dangerous pass rate (general note, owner-directed)

Recorded as its own note rather than inside the release that tripped over
it, because the lesson is about gates in general.

`an_empty_new_slot_contributes_exactly_nothing` fails about 8% of runs
(25 isolated runs: 23 passed, 2 failed). It shipped with gear-slots and
**passed by luck twice in the same day** — at 824 and at 825 — before
failing on the third full run.

**That rate is the worst possible one.** A test that fails half the time
is obviously broken and gets fixed within the hour. A test that fails one
run in a thousand is noise nobody sees. A test that fails one run in
twelve is **frequent enough to look green and rare enough that the first
failure reads as "something I did"** — so the person who hits it goes
looking for their own mistake, finds nothing, re-runs, sees green, and
moves on.

**The lesson people learn from a flaky gate is "re-run until green", and
that is exactly how a real failure gets waved through.** The cost is not
the wasted run. It is that the gate stops being evidence: once re-running
is normal, a genuine regression produces the same ritual and the same
shrug. A gate that is right 92% of the time does not give you 92% of a
gate — past some threshold it gives you none of one, because it has
trained its readers to discount it.

Two practical consequences:

1. **A flaky test is a broken test, at any rate above zero.** It is not a
   lower-priority class than a failing one; it is a failing one that has
   learned to hide. Fix it or delete it, but do not leave it green enough
   to tolerate.
2. **Confirm in isolation before flagging, and confirm with a COUNT.** The
   count is what separates "flaky" from "regression" and turns an
   irritation into a diagnosis. 23/2 with the failure values clustering
   at 0.10 + ~0.096 pointed straight at `roll_affixes` adding a second
   CritChance on top of the implicit; a single red run would have pointed
   nowhere and would probably have been re-run away.

Related: this was the fifth instance of the class recorded above — an
assertion encoding an expected ANSWER rather than the PROPERTY — and the
**first one caught by the rule rather than by an incident.** The other
four pinned an expectation to a literal a human typed; this one pinned it
to a random draw. Same shape, and the same cure: derive the expectation
from the system at the moment of the check.


### 2026-09-03 — FLAKY-EMPTY-SLOT-TEST deploy record (release `flaky-empty-slot-test`)

Queue item 3, from window b. The fix for the ~8% flaky test that halted
the queue.

| | |
|---|---|
| master commit deployed | `7afbe53` |
| binary before | `d793678d…3980f6` |
| binary after | `6e35ad68e1ff30249d9062a6bfbf2aa5e4f8106e270aa3e71de8f2349e97b6e3` |
| downtime | **0.18 s** |
| suite on the box | **827 passed / 0 failed / 0 ignored, 38 suites** (`-j 4`) — 825 + 2 |

### FIRST PRODUCTION USE OF THE TIMESTAMPED SLOT SCHEME

This is the first deploy under the scheme item 2 shipped, and it behaved:

```
rollback slot: /var/backups/pathofdust/deploy-pre-20260903-151649-flaky-empty-slot-test/game.pre-flaky-empty-slot-test
deploy-pre-LATEST -> deploy-pre-20260903-151649-flaky-empty-slot-test
```

The slot's `SHA256SUMS` carries what the filename deliberately does not:

```
# release: flaky-empty-slot-test
# slot:    deploy-pre-20260903-151649-flaky-empty-slot-test
# commit:  7afbe53a61a537f80a06a8c347b7535624feaa6a
# saved:   2026-09-03T15:17:02+02:00
d793678d…  …/game.pre-flaky-empty-slot-test
```

`--list` puts it first, marked `stamped`, with the fifteen legacy slots
below marked `legacy`. The slot holds `d793678d` — the predecessor
binary, which is the invariant that makes a slot worth having. **`LATEST`
now exists**, so the no-argument rollback has something to follow for the
first time; the gap recorded in item 2's entry is closed.

### The flake is gone, confirmed independently

b reported 40 consecutive module runs, 0 failures. Not taken on trust —
**20 consecutive runs of `new_slot_migration_tests` against my own built
tree: 20 passed, 0 failed.** The full suite passing once would not have
been evidence, for exactly the reason recorded in the 92% note above: the
old test passed twice today before failing.

b confirmed the diagnosis rather than re-deriving it, then went further:
forcing `roll_affixes` to return a `CritChance` on every item produced
0.10 + 0.096 = **0.196**, against the three observed failures at 0.19277,
0.19903 and 0.19617. **A forced value reproducing observed failures to two
decimals is what turns "probably a rolled affix" into "a rolled affix"** —
it rules out drift in the implicit itself, which is the other thing those
numbers could have meant.

The one assertion became three: the isolated implicit is worth exactly one
affix at its tier; a rolled `CritChance` stacks on top without disturbing
it (which encodes what the old test was accidentally measuring); and a
filled slot contributes strictly more than an empty one, so it cannot pass
against a slot that is not wired up at all. The fixture strips rolled
affixes **and asserts it succeeded** — a fixture that silently failed to
isolate would put the flake straight back.

### FINDING — a `#[cfg(test)]`-only change moved the release binary by exactly 88 bytes

Worth recording because item 2 shipped on a bit-identity argument, and
this is the boundary of that argument.

The only source difference between this tree and item 2's is inside
`#[cfg(test)] mod new_slot_migration_tests`, which is the **last item in
`character.rs`** — nothing compiled into the release binary follows it.
`Cargo.lock` and `game/Cargo.toml` are identical. The release binary
should have been unchanged. It was not:

| | |
|---|---|
| size | 17,230,896 → 17,230,984 — **exactly +88 bytes** |
| diff added | **exactly 88 lines** |
| differing bytes | 2,482,407 (i.e. everything after the insertion shifted) |

The 88-line ↔ 88-byte correspondence points at a line-indexed structure
retained in the release build, and the 2.4 MB of differing bytes is what
an 88-byte insertion does to every offset after it. **The mechanism is not
proven and is not asserted here** — what matters is the consequence.

**The consequence, which is the useful part: bit-identity is SUFFICIENT
but not NECESSARY for "this change cannot alter behaviour."** Item 2's
matching hashes were decisive evidence. This release's *differing* hashes
are evidence of nothing at all — a test-only edit moved them. So:

- A matching hash still proves the binary is the same binary. Keep using
  it; it is the strongest evidence available.
- **A differing hash proves only that the file differs, never that
  behaviour differs.** Do not reason backwards from it.

The narrow licence granted for item 2 is unaffected and stays exactly as
narrow: it rests on bit-identity being *present*, which is a claim this
finding does not weaken.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **20 = 20** |
| 4 | live sha256 | `6e35ad68…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 80,690 B, 200 / 91,110 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Public tunnel 200. Zero panics or ERROR lines. Roster is now 20.

No patch note: this release changes test code only and has no
player-visible effect.

### 2026-09-03 — REPAIR-NEW-SLOTS deploy record (release `repair-new-slots`)

Queue item 4, from window b. Highest player impact in the queue.

| | |
|---|---|
| master commit deployed | `d11424b` |
| binary before | `6e35ad68…7b6e3` |
| binary after | `a09c9c362a87866dbc12e8a5924524f78c693d9e7314788e3a0cf4e9a5c2261e` |
| downtime | **0.59 s** |
| suite on the box | **830 passed / 0 failed / 0 ignored, 38 suites** (827 + 3) |
| slot | `deploy-pre-20260903-154244-repair-new-slots`, `LATEST` repointed |

### The bug's footprint, measured rather than described

`owned_items_mut_unguarded` names the equipped fields **by hand** — the
borrow checker cannot yield nine simultaneous mutable references from a
loop — and still listed the original five. `repair_all_cost` reads
`EQUIP_SLOTS`. So the price counted nine slots and the repair touched
five, and the call returned `Ok`. Durability wear also iterates
`EQUIP_SLOTS`, so the new-slot items kept wearing out with no way back.

Counted on the live roster at deploy time:

| slot class | worn out / equipped |
|---|---|
| the original five | **1 / 100** |
| the four new | **44 / 77** |

**57% versus 1%.** That gap is not wear variance; it is the shape of a
population that has been wearing out and never being repaired, next to one
that has been repaired all along. It is also the cleanest statement of why
this was the most urgent item in the queue: a third of all equipped
new-slot items in the game were dead, and every paid Repair All was
charging for them.

Everything comes back on the first repair after this deploy — nothing was
permanently lost. What is not recoverable is the dust already spent on
repairs that did nothing.

### The guard is the durable part

The fix adds the four fields. The part worth keeping is what it adds
alongside them:

```
EQUIP_SLOTS.len() == 9,
"EQUIP_SLOTS has changed size. `owned_items_mut_unguarded` names every
 equipped field BY HAND (the borrow checker cannot do it in a loop) - add
 the new slot's field to the array below and update this count, or repair,
 Krangle re-tiering and every item migration will silently skip that slot."
```

**This is the third time today the same shape has been the root cause**,
and the second time in three releases that the cure was the same: a
hand-maintained membership needs something that **fires when the invariant
changes**, not a comment asserting it will not. The starter-kit backfill
needed a marker; this needed a length check. Note the message names the
three things that break rather than restating the rule — the next person
adding a slot is told the consequence, which is what makes a guard get
obeyed instead of edited away.

Three new tests, one of which names this live bug in its failure message,
so a regression reads as *this incident* rather than as a puzzle.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **20 = 20** |
| 4 | live sha256 | `a09c9c36…` = candidate |
| 5 | `/characters`, `/inventory` | 200 / 80,691 B, 200 / 117,594 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

**Not verified by click-through, and stated rather than glossed:** the
repair itself was not exercised on production, because doing so means
spending a real player's dust and consuming a real item's durability.
It is covered by the branch's three tests and by the census above, which
is the observable consequence.

### Patch notes

One section, top of the September 3 block (13 → 14). Renders at 218,635 B.
Pre-edit copy `/root/patch-notes.pre-repair-new-slots.json`.

Written nerf-honest in the direction that matters here — the note leads
with *"has been charging you … without repairing them"*, gives the 44/77
against 1/100 figures so players can check it against their own gear, says
plainly that the spent dust does **not** come back automatically, and
offers to sort out anyone badly hit individually. It also says the mistake
was ours and avoidable, and what now prevents it.

### FOUND — a refund question the owner may want to answer

No dust refund was issued and none was ordered. The amount is not
reconstructible from what is stored: repair spend is not itemised
anywhere, so there is no way to compute per-player compensation after the
fact. The patch note therefore asks affected players to come forward
rather than promising a number nobody can calculate. **If a blanket
goodwill grant is wanted instead, it is an owner decision and a separate
change.**


### 2026-09-03 — "An artifact from a previous run, mistaken for the current one" (general note, owner-directed)

Third instance in a day, so it is recorded as a class rather than inside
any one of them.

**The three:**

1. **The frozen `C:\PathofDust` checkout.** A working copy that had
   stopped moving while everyone kept reading it as current. It misled
   five sessions before anyone asked how old it was.
2. **`ls -lt` on the rollback slots.** `cp -a` preserves source
   timestamps, so every slot's binary carries its *predecessor's* install
   time. The listing is not garbage — it is plausible, and consistently
   wrong by exactly one deploy.
3. **A stale `test-<rel>.log`, today.** The cleanup line used `\$REL`,
   which expands on the REMOTE side where `REL` is unset, so it deleted
   `/root/test-.log` and nothing else. A leftover `test exit: 101` from a
   previous failed run then sat next to a build that was still compiling,
   and read exactly like a fresh failure.

**What makes this class dangerous is that the artifact is not corrupt.**
Every one of the three was internally consistent, well-formed, and
truthful *about the moment it was made*. Nothing announces itself. A
corrupt file throws; a stale one answers confidently.

**The general cure: ask when the artifact was produced before believing
what it says.** Not "is it valid" — it is — but "is it *current*". In
practice that means carrying a timestamp or a generation marker alongside
anything you will later read as a result:

- The stale log was resolved by comparing `stat -c %y` on the log against
  the unit's `ActiveEnterTimestamp`. Two timestamps settled in one command
  what re-reading the tail could never settle, because re-reading a stale
  file just re-reads the stale file.
- The slot ordering was fixed by putting the timestamp **in the name**,
  where it cannot be inherited from something else, and the `ls -lt`
  warning was written into §13B at the place someone would reach for it.
- The frozen checkout was resolved by checking its ref against the remote
  rather than reading its contents.

Note the family resemblance to the other class recorded above — *an
assertion encoding an expected ANSWER rather than the PROPERTY*. Both are
the same underlying error: **trusting a value that was true when it was
captured, in a context that has since moved.** One captures a number, the
other captures a file. The cure is the same shape too — derive it from the
system at the moment you need it, rather than reading back something
recorded earlier.

**Stated as a check, so it is usable in review:** before believing any
file, log, listing or cached result, establish when it was produced and
whether anything has happened since. If you cannot tell, treat it as
unknown rather than as evidence.

### 2026-09-03 — CATCHUP-FIX: catch-up keys off the leader, not the median

Branch `fix/catchup-degeneration` off `origin/master` (`0c4dffa`). Fixes
the defect recorded above under *FOUND — catch-up XP degenerates into a
flat global multiplier on a bunched roster*. Not merged, not deployed —
session c holds that authority.

**CORRECTION to this session's own fit report.** The fit report modelled
the live state as `win_xp_mult` 0.5 and labelled a table row "live now"
on that basis. That was wrong: the counterweight was recommended in the
2026-09-02 ruling but **never applied**, and the live admin page has
`win_xp_mult` at **1.0**. Session a's independent measurement confirms it
(a level-11 character earning 37 XP per win, the level-2 newcomer 39 —
the mult-1.0 numbers exactly). The corrected table is below; the proposal
and the conclusion are unchanged, and simplified, because no dial change
now accompanies this fix.

#### The formula, before and after

Before (`manager.rs:catchup_multiplier`), keyed on `median_u32`:

| branch | bonus |
|---|---|
| `l <= median` | +200% at min, tapering to **+100% at the median** |
| `l > median`, `max > median` | +100% at the median down to +0% at max |
| `max == median` | +0% |

The first branch reads no leader level at all and floors at +100%. On a
bunched roster the median EQUALS the maximum, so the whole lead pack
lands in it.

After: `deficit = (max - level) / max`, `bonus = 200% * min(1, deficit /
catchup_full_deficit)`, band unchanged at 1.0–3.0. Every character at
`max` has a deficit of exactly zero, so a bunched roster returns exactly
1.0 — no epsilon, no special case — and the leader is 1.0 on every roster
shape. The deficit is relative rather than an absolute level gap because
`xp_to_next_level` is quadratic: ten levels behind is a chasm at level 11
and a rounding error at level 200.

Why median was there in the first place, since it is not obvious: no
design record survives (`git log -S"catchup_multiplier"` reaches only the
crate extraction and the win-XP commits). The only stated rationale is
the function's own doc — *"so the median member still gets a real boost
(not just the trailing half)"*. Median was chosen to make the payout
GENEROUS, not to measure a gap. That is why it degenerated: a generosity
floor with no reference to the leader.

#### The live roster, corrected

Roster at 2026-09-03 02:52Z, `win_xp_mult` **1.0** (live), grants are
exact `win_xp_for_win` output:

| level | n | today mult | today grant | fix mult | fix grant |
|---|---|---|---|---|---|
| 11 | 14 | 2.000 | **37** | **1.000** | **18** |
| 9 | 1 | 2.222 | 38 | 1.727 | 29 |
| 8 | 1 | 2.333 | 38 | 2.091 | 34 |
| 2 | 1 | 3.000 | 39 | 3.000 | 39 |

Stated plainly: **today the level-2 newcomer earns 39 against the pack's
37 — a 5% edge over people nine levels ahead. Under the fix they earn 39
against 18, a 2.17x edge**, which is the mechanic finally doing its job.
The two mid-table stragglers go from a meaningless 1.03x/1.03x relative
to the pack to a real 1.6x/1.9x.

#### Rate, and the dial

Levels/day at level 11, 96 wins/day (144 encounters at the live 2:1
ratio):

| config | pack mult | grant | levels/day |
|---|---|---|---|
| today (median), mult 1.0 — **live** | 2.00 | 37 | **11.61** |
| **fix, mult 1.0** | 1.00 | 18 | **5.65** |
| fix, mult 0.5 | 1.00 | 9 | 2.82 |

Seven-day per-win simulation from the live roster, levels advancing and
catch-up recomputed every fight, against the approved curve:

| day | fix + mult 1.0 | approved curve |
|---|---|---|
| 1 | 15 | 16 |
| 3 | 23 | 23 |
| 5 | 29 | 26 |
| 7 | 35 | 32 |

**`win_xp_mult` needs no change and must not be touched.** It is already
at 1.0, and the fix at 1.0 reproduces the approved curve to within a
level, decaying 4 → 3 levels/day across the week and continuing to decay
as the quadratic cost bites. Setting it to 0.5 on top of this fix would
double-count the correction and run the game at half the approved curve.
The 11.61 → 5.65 drop at level 11 is the whole overshoot, closed by the
formula rather than by a counterweight.

#### Scope

`catchup_multiplier` is shared: it feeds the loot/dust pity payout
(`pity_reward_count`) as well as win XP. The same degeneracy was paying
the whole bunched pack ~2 pity rewards instead of 1. Fixing the shared
function fixes both; an XP-only fork was considered and rejected as the
larger change. Owner approved the shared fix.

`median_u32` is untouched — `prioritize_above_median` (enemy targeting)
still uses it, and its doc now says so and records that catch-up no
longer does.

#### The knob

`catchup_full_deficit`, default `CATCHUP_FULL_DEFICIT` 0.5, range
0.01–1.0, rendered on `/admin/tunables` under Experience. Unit is a
fraction of the leader's level. **The 0.01 floor is load-bearing and is
the one place on that page where a zero floor would be wrong**: the field
is a DIVISOR, and 0.0 would pay the full 3x to everyone standing one
level below the leader — the same flat-multiplier defect in a new shape.
Rejected by the handler, clamped again inside `catchup_multiplier` for a
hand-edited tunables file, and `#[serde(default =
"default_catchup_full_deficit")]` resolves an omitted field to the
shipped constant. `admin_tunables_win_xp_http.rs` already scrapes its
field set off the rendered page, so the new field entered every POST body
automatically; it now also asserts the render, the round trip, the
persistence, the out-of-range rejection (0, -0.5, 1.5) and the
omitted-field fallback.

Tests: `manager::catchup_multiplier_tests`, five cases — the degenerate
bunched roster resolving to exactly 1.0 (asserted against the real live
roster shape, 14×L11 + 9 + 8 + 2, and against four other shapes), a real
laggard still paid and paid monotonically, the knob scaling the taper and
surviving a 0.0, no-spread/solo/empty at 1.0, and the 1.0–3.0 band held
across the knob's full range.

Suite: **796 passed, 0 failed**, `cargo test --release --workspace
--quiet -j 4 --target-dir target-catchup`. Clippy clean on touched code.

#### Patch note draft — NOT written to the box

Session c writes this at deploy. Honest about the nerf, per the rule:

> **Catch-up now finds the people who are actually behind**
> - Catch-up bonuses (bonus XP, dust and drop odds for trailing players)
>   used to be measured against the middle of the pack. Once everyone
>   bunched up, the middle WAS the top — so the leaders were quietly
>   collecting a 2x bonus meant for newcomers, and a level-2 player was
>   out-earning a level-11 player by 5%.
> - It is now measured against the highest-level character in the fight.
>   If you are level with the leader you get no bonus at all; the further
>   behind you are, the bigger it gets, up to 3x.
> - **This is a nerf for established characters.** A level-11 character
>   in today's group goes from 37 XP a win to 18. That is the intended
>   rate — the group has been levelling at roughly twice the designed
>   speed since the world opened, and this is where the extra came from.
> - Trailing players are unaffected: the level-2 newcomer still earns 39
>   a win, and now actually gains ground instead of standing still.
> - No other XP settings changed.

FOUND — the doc block describing `catchup_multiplier` was physically
attached to `median_u32` (it sat above `median_u32`'s own doc, so rustdoc
concatenated both onto the median helper and `catchup_multiplier` itself
rendered undocumented). Repaired while rewriting the function.


### 2026-09-04 — CATCHUP-DEGENERATION deploy record (release `catchup-degeneration`)

Queue item 5. The real fix for the XP overshoot measured on 2026-09-03,
of which `win_xp_mult` was only ever a counterweight.

| | |
|---|---|
| master commit deployed | `b090246` |
| binary before | `a09c9c36…c2261e` |
| binary after | `84854f16053f8611469eabd6a20174d162bcabb7eb1fc92d0a3d932ce1107714` |
| downtime | **0.32 s** |
| suite on the box | **793 passed / 1 failed** — one known failure, shipped under an explicit licence, see below |
| slot | `deploy-pre-20260903-181713-catchup-degeneration`, `LATEST` repointed |

**Build provenance, stated because it is not the usual case.** The binary
was built from the tree of `ac3c9f8`, an earlier merge of the same branch
commit. `git diff --name-only ac3c9f8 b090246` returns exactly one file —
`docs/session_journal.md` — and `game/`, `src/`, `templates/`, `wiki/`,
`public_adventure_overlay/`, `Cargo.toml` and `Cargo.lock` are all
byte-identical. So the binary and every deployed asset correspond exactly
to `b090246`; only an uncompiled, undeployed docs file differs. Recorded
rather than glossed, because "the binary was built from a different
commit than the one recorded" is the sort of thing that reads as a defect
later if the reason is not written down.

### SHIPPED ON A RED SUITE, UNDER THE SECOND LICENCE OF THIS SHAPE

The one failure is
`stage_gate_tests::fighting_never_grants_a_craft_token_but_the_starter_set_is_intact`
— a `CelestialShard` dropped during the test's 12 fights, and the test
excludes `UniqueShard` from its comparison but not `CelestialShard`.

**The licence was granted on a reachability proof, not on a green run,
and the distinction is the whole point:**

- `gated_manager` joins **exactly one character**.
- `catchup_multiplier` opens with `if max <= min { return 1.0 }` in
  **both** the old and the new version.
- With one character `min == max`, so **both versions return before
  reaching a single line this release changed.**
- The branch's only other drop-related edits are doc comments, and
  `CelestialShard` is granted at `manager.rs:2704` in the tree that was
  already serving players.

**The changed code cannot execute in the failing test.** A green run
would only have said the low-probability failure did not fire that time;
this says the change is not connected to the failure at all.

**The bound, recorded so nobody quotes this loosely.** The licence is for
a proof demonstrated *in code* that the changed lines are unreachable
from the failing assertion. **It is not for "this looks unrelated", not
for "the failure is in another module", and not for a plausibility
argument however good. If you cannot point at the specific guard that
returns before your change, you do not have this licence and you stop.**

Thirty isolated runs of that test against the live tree came back 30
passed / 0 failed. **That was deliberately NOT offered as evidence.** At
`celestial_shard_drop_chance = 0.002` a low-single-digit failure rate
survives thirty clean runs easily, and treating an underpowered clean
result as proof is the same error as re-running until green.

The flake is the **sixth** instance of the assertion-encodes-an-ANSWER
class — a literal token list asserted against a fixture with a random
element in it — and goes to b, as the ring test did.

### What changed, against the live roster

The roster at deploy time was the degenerate case exactly:

```
levels: [6, 9, 13, 13, 15, 15, 16 × 14]
max 16   median 16   min 6      14 of 20 at the top
```

**Median equals maximum**, so every one of those fourteen was collecting
the full +100% intended for stragglers. Multipliers before and after:

| level | old (median-keyed) | new (leader-keyed) |
|---|---|---|
| 16 (×14, the pack) | **2.00x** | **1.00x** |
| 15 | 2.10x | 1.25x |
| 13 | 2.30x | 1.75x |
| 9 | 2.70x | **2.75x** |
| 6 | 3.00x | 3.00x |

Note the shape: the pack halves, and the genuinely-behind players are
untouched or very slightly better. That is the mechanic doing what it was
named for, rather than paying everyone.

In grant terms for a level-16 character: `xp_to_next(16) = 516`, so the
shape term is `12 + 0.0208333 × 516 = 22.75`, and the per-win grant goes
**46 → 23**.

`catchup_full_deficit` renders live at its shipped `0.5`, and
`win_xp_mult` stays `1.0` — verified on the box, not taken from an order.
The curve's shape is untouched; only the axis changed.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **20 = 20** |
| 4 | live sha256 | `84854f16…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 80,698 B, 200 / 91,056 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

### Patch notes

One section at the top of the September 3 block (14 → 15). Pre-edit copy
`/root/patch-notes.pre-catchup-degeneration.json`.

Nerf-first, as the rule requires: the heading says *"Catch-up XP was
paying the leaders. That is fixed, and it is a nerf if you are near the
top"*, and the body says in capitals that a level-with-the-leaders
player's XP per win halves, and that on the current roster that is most
of them. It also says what is NOT taken away — every level already earned
— and that a genuinely behind player still gets the full 3x.

### The hook check, done rather than assumed

Window a installed hooks on `C:\PathofDust\.git` that refuse commits and
pushes across every worktree sharing it. This checkout was confirmed
unaffected before relying on it: `C:/dust-work/c/.git` is a real
directory, not a worktree pointer; `--git-common-dir` resolves to itself;
no non-sample hooks are installed and `core.hooksPath` is unset. Then
confirmed by effect — the push of `b090246` succeeded. **No `--no-verify`
was used, and none would have been**: overriding another window's safety
control to get past it proves it can be ignored, which is worse than a
short wait.

#### 2026-09-04, addendum — item 5 verified by effect, and my observer's blind spot

**The prediction was made before the measurement, and it held exactly.**
For a level-16 character, `xp_to_next(16) = 516`, so the win-XP shape term
is `12 + 0.0208333 × 516 = 22.75`, giving a per-win grant of **46 at the
old 2.00x catch-up and 23 at the new 1.00x**. Measured on production
across the first real grant after the swap:

| char | before | after | gained |
|---|---|---|---|
| `galquin` | L16 + 497 | L17 + 4 | **23** |
| `gorshie` | L16 + 497 | L17 + 4 | **23** |
| `jachiny` | L16 + 489 | L16 + 512 | **23** |

Three characters, two of which levelled *through* the boundary, all
gaining exactly the predicted 23. The mechanic is doing on production
what the arithmetic said it would.

**My first observer missed it entirely, and the reason is the day's own
lesson turned on me.** It compared each character's `xp` between two
snapshots and reported a grant only when the level was unchanged —
`$2==$4 && $5>$3` — because a level-up wraps `xp` back toward zero and
would otherwise look like a *loss*. Every level-16 character then levelled
to 17 on that very grant, so every grant was filtered out and the run
reported *"no grant observed within 20 minutes."*

That is **an assertion encoding an expected SHAPE rather than the
PROPERTY**, which is the same class as the five instances recorded above,
committed by me while verifying a fix for one of them. The property was
"XP was granted"; what I encoded was "XP rose without the level
changing". The cure was the same one every other instance needed: measure
the thing itself — cumulative XP across the level boundary,
`(xp_to_next(old) − old_xp) + Σ xp_to_next(intervening) + new_xp` —
rather than a proxy that happens to hold in the common case.

**Worth noting how close this came to being read as a failure.** "No
grant observed in 20 minutes" against a live game resolving a fight every
~2.3 minutes is an alarming sentence, and the obvious reading is that
win-XP had stopped. The check that dissolved it was looking at the
characters directly rather than re-reading the observer's conclusion —
they were level 17, which is only reachable by being paid.

## 2026-09-03 — WIKI-TRUTH-UP (branch `wiki/truth-up`)

Corrected the wiki against the code after several mechanics shipped without
the pages following. Rewrote getting-started, crafting, combat, items and
dashboard; surgical fixes to commands, golems and landing. Deleted five
constants (plus two Lingering Effect ones) that existed in game code for no
reason except that wiki.rs read them.

The root defect was not any individual wrong number: it was that the
placeholder map resolved compiled constants for values that had since become
LiveTunables. A constant a tunable has taken over does not stop compiling when
it goes stale — it renders a confident wrong number forever. Fixed as a rule
rather than page by page (the COMPILED-ONLY RULE, on `wiki_placeholder_map`'s
doc comment), and the drifting placeholders were retired rather than
re-pointed, because re-pointing only moves the next break.

FOUND — `adventure-live-tunables.toml` in the deploy directory is a frozen
pre-cutover snapshot, not production state. This session read a live craft
tunable out of it and reported a stale value to the owner; the owner caught
it and named it as the fourth session it has misled in two days. Recorded in
WIKI_IMPACT.md as a standing warning. Nothing in this repo can answer "what is
this tunable set to right now" — only /admin/tunables can.

FOUND — boss pierce has no player-visible surface anywhere. The wiki now
documents the mechanic, but deliberately states no number, because unlike
crafting prices there is no in-game readout to point a player at. Suggested
follow-up: surface the current pierce share somewhere a player can see it.

FOUND — the order's premise "every Twitch chat command is gone" was wrong.
The ~50 bot-native commands (music, playlists, themes, poe.ninja, !hug,
!bugreport) are all still registered and were left alone on the owner's
ruling; only the 9 adventure rows and !checkpatreon were deleted. Refuting
the premise before building on it saved deleting a working page.

FOUND — `stage_gate_tests::fighting_never_grants_a_craft_token_but_the_starter_set_is_intact`
failed one full-workspace run (stray CelestialShard from sibling-test
contamination of the shared data_path store), passed in isolation, and the
full suite passed twice more after. Intermittent, not caused by this branch —
a clean tree was also run to confirm. Candidate for the known-flaky list.

### 2026-09-04 — WIKI-TRUTH-UP deploy record (release `wiki-truth-up`)

Queue item 6, from the wiki session.

| | |
|---|---|
| master commit deployed | `79e1207` |
| binary before | `84854f16…07714` |
| binary after | `a0819cfc1703fe146a79d07a722e4aa21bff3e79ad3629d69dd702354e0b7c27` |
| downtime | **0.28 s** |
| suite on the box | **835 passed / 0 failed / 0 ignored, 38 suites** |
| slot | `deploy-pre-20260903-185600-wiki-truth-up`, `LATEST` repointed |

The run was green, and it included
`stage_gate_tests::fighting_never_grants_a_craft_token_but_the_starter_set_is_intact`
— the flake item 5 shipped past. It passed this time, which is what an
8%-failure test does 92% of the time and is **not** evidence it is fixed.
It remains b's to fix.

### Not a docs-only change, and it was checked rather than trusted

The label was "clean fast-forward". It deletes **51 lines from
`manager.rs` and 10 from `combat.rs`**: `ACTIVITY_XP_COOLDOWN`,
`ACTIVITY_XP_AMOUNT`, `LINGERING_EFFECT_TICK_INTERVAL_MS` and
`LINGERING_EFFECT_TICKS`. Each had no remaining caller and each carried a
comment saying it survived **only** because `adventure_web/wiki.rs` read
it to template a wiki section, and asking the wiki session to remove it on
its own schedule.

This release removes the readers and the constants together, in that
order. That is the only safe sequence and it belongs to exactly one
session: deleting the constants from anywhere else breaks the wiki
module's build out from under it, and deleting the wiki section without
the constants leaves dead code pointing at a reader that no longer exists.
Verified in the built tree — both constant pairs return **0** references.

### Verified by effect, on the thing the release is actually for

| page | result |
|---|---|
| `/wiki` | 200, 77,782 B |
| `/wiki/crafting` | 200, 100,212 B |
| `/wiki/combat` | 200, 96,045 B |
| `/wiki/items` | 200, 84,691 B |
| `/wiki/getting-started` | 200, 84,887 B |

**The wiki still mentions "Lingering Effect", and that is correct.** It
was checked rather than assumed stale — the live pages read *"Echo
replaced Lingering Effect entirely"*, and:

> *If a piece of your gear still displays "Lingering Effect", it hasn't
> been converted yet. Existing items convert to Echo automatically at half
> their stored value, but the timing of that conversion is not guaranteed
> … Either way the old affix does nothing.*

That is the wiki doing its job: telling players about a name they can
still see on their own gear, rather than pretending the rename was
instantaneous. A page that simply deleted the term would have left a
player staring at an affix the wiki claims does not exist.

### The `lingeringEffect` startup warning persists, and that is also correct

Still one per boot:

```
WARN game::adventure::affix: adventure-item-balance.toml:
'lingeringEffect' is a retired affix with no live base value to override, ignoring
```

It does **not** come from the constants this release deleted. Its source
is an empty `[affixes.lingeringEffect]` section header in
`adventure-item-balance.toml`, which is runtime data on the box with no
copy in the repo. Owner ruling: leave it — an empty header for a retired
affix is harmless. Recorded so the next person who removes a
`LINGERING_EFFECT` symbol and expects the warning to disappear knows where
it actually comes from.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **20 = 20** |
| 4 | live sha256 | `a0819cfc…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 80,698 B, 200 / 91,056 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

No patch note: the wiki is documentation, and the release changes no
mechanic, cost, chance or timer. The one player-visible consequence — that
the wiki now says what the game actually does — is the wiki's own content.


## 2026-09-03 — BOSS-SECONDARY-CURVE: the freeze is gone, on seven dials (branch `feature/boss-secondary-curve`)

Ordered from `C:\dust-work\orders\d.md`, implementing §10 of
`docs/dynamic_pacing_design_pass.md` (which lives on
`origin/design/dynamic-pacing`, not master). The fit report for this is
`C:\dust-work\reports\BOSS-SECONDARY-CURVE-2026-09-03.md`; all four of
its findings were accepted and every one of them is now recorded against
the design document itself, on `design/dynamic-pacing-corrections`.

### What shipped

Seven organic boss secondaries in `boss_stats_for` were
`min(stage × slope, cap)` corners that froze between stage 36 and 150.
They are now `cap × s/(s + h)` — `pacing::top_layer_for_stage`'s shape —
through **one** shared `boss_secondary_ramp`, with the seven `h` values
promoted to LiveTunables under a new "Boss Secondary Curves" admin
section.

**Shipped at k = 1, and that was the ruling, not an omission.** Each
default is the stat's old `cap / slope`, which reproduces the old slope
exactly at stage 0 and is also its old freeze stage. Below the freeze
stage nothing changes; above it everything unfreezes.

### The design error that changed the decision

§10.3 ratified a ×2 stretch on the reasoning that the asymptotic form
sits below today above the freeze stage — "the opposite of the goal
**until `h` is stretched**". That is backwards. `cap·s/(s+h)` is
**monotonically decreasing in `h`**, so the stretch lowers every value
and *maximises* the shift of resistance into raw hp/atk it was offered as
the cure for. At stage 800, damage reduction is 0.632 at `h = 150` and
**0.545** at `h = 300`, against today's 0.750.

The owner had approved the stretch repeating that reason as load-bearing,
and disregarded the instruction once shown. The deciding argument became
§10.7's own approval reason #2: **at k = 1 shipping changes nothing below
the freeze stage; at k = 2 every one of the 42 cells is below today and
sub-stage-100 secondaries roughly halve — precisely the range the live
world occupies.** The case for a stretch survives as a tuning decision to
make from live data. That is what the dials are for.

### Two more findings against a ratified document

- **§10.5 said the golden corpus would move. It does not, and that
  removed a stated blocker.** `golden_corpus.rs` excludes
  `boss_stats_for`/`basic_enemy_stats_for` by name in its own header,
  because both roll un-seeded `thread_rng()` jitter — an un-seeded RNG
  could never have been captured into a fixture. **Confirmed by running
  it, not asserted:** 4 passed, zero fixtures regenerated,
  `git status` clean.
- **§10.1's `crit_chance` cap was wrong — 0.70, not 0.75, with a flat
  0.05 base the table omitted.** Every other cell in §10 is already
  consistent with 0.70. A literal implementation of the cell as written
  would have dropped the base *and* missed the document's own numbers.

### Known limitation, ruled in scope-refusal rather than fixed

`apply_dynamic_scaling` multiplies secondaries by `sqrt(dmg_mult)` and
re-caps at `BOSS_DEFENSE_CAP`. So for **evasion, block and damage
reduction** the unfreezing only holds while Controller B is near
baseline: at ×√4 all three re-pin at 0.75, and the flatness returns
exactly when it matters most. `increased_damage`, `crit_multiplier` and
`splash` are unaffected.

Both available fixes were **refused explicitly**: raising
`BOSS_DEFENSE_CAP` (a safety rail — raising one as a side effect of a
variety change is how rails stop meaning anything) and exempting
secondaries from `sqrt(dmg_mult)` (a real change to controller/boss
interaction that deserves its own pass). It is on the board as its own
item, and the admin section carries it in its own hint text so nobody
tunes the three defensive dials without knowing the ceiling can take the
movement back.

### Patch-note DRAFT (not published — this session does not deploy)

> **Bosses stop being the same boss forever.** Every one of a boss's
> seven secondary stats — evasion, block, damage reduction, crit chance,
> crit damage, increased damage and splash — used to stop growing at a
> fixed world stage and never move again. Crit damage froze at stage 36.
> Evasion froze at 50. The last of them, damage reduction, froze at 150.
> Past that, the only thing about a boss that changed with stage was how
> much HP it had and how hard it hit.
>
> They now keep climbing for as long as the world does, approaching their
> ceilings instead of slamming into them. A stage-800 boss is genuinely
> harder to hit and harder to hurt than a stage-300 one, and your
> accuracy, pierce and mitigation choices keep mattering instead of being
> solved on day two.
>
> **Honest about the trade: above the old freeze stages, the numbers
> themselves are lower than they were.** A stage-800 boss now sits around
> 63% damage reduction where it used to sit at a flat 75%. That is the
> price of never flattening — and overall difficulty does not change,
> because dynamic pacing re-balances through HP and damage as it always
> has. Below the old freeze stages nothing changed at all.

### FOUND

- `boss_stats_for` ends `let stats = BossStats { … }; stats` — clippy's
  `let_and_return`. Pre-existing (verified against `88d5420`), untouched.
## 2026-09-03 — GEAR-TIER-EXCESS: boss difficulty learns to see crafted gear, shipped inert (branch `feature/gear-tier-excess`)

Ordered from `C:\dust-work\orders\d.md`, implementing Option C of the
undamped-power-loop pass. Fit report:
`C:\dust-work\reports\UNDAMPED-POWER-LOOP-FIT-2026-09-03.md`.

### The correction the owner returned

That fit report measured `C:\PathofDust` and found 67 characters at stage
7380, not the order's stated 19 characters at stage 10. Flagged rather
than assumed. The owner's answer: **that checkout is World 1, retired —
production moved to a Linux box on 2026-09-02 and World 2 was reset that
morning.** `C:\PathofDust` "has now misled five separate sessions" and is
never to be read as production data again. Recorded here so a sixth
session does not repeat it: **production world data lives on the Linux
box, not on this Windows checkout.**

Consequence for the report's own findings: §2.1/§2.2 (the 42%-carry-more-
than-2×-their-level figure) described World 1's population and cannot be
quoted about World 2 — re-run against the live box before citing
anywhere. §2.3 (the mechanics-derived day-one argument) stands regardless
of which world it's read against. Leak 1 (Controller A's pool cap
saturated at its own maximum) is not live in World 2 (A at 11.88 of a 50
ceiling) but is judged the most valuable finding in the report anyway:
World 1 ran a year and arrived exactly there, with its documented escape
hatch fully spent, and World 2 will walk the same path if nothing
changes.

### What shipped

`boss_stats_for` is now generated against `effective_avg_level` = party
average level + `boss_gear_tier_weight` × party average gear-tier
EXCESS, `max(0, mean equipped tier − level)`, not raw tier. The excess
measure is the deciding property: `grow_krangled_items` pins a Krangled
item's tier to exactly the character's level, so a Krangled build's
excess is zero by construction and cannot be double-billed on top of
`level_mult`, which already charges for it. Reading raw tier would have
billed hardest the players who did the sanctioned thing — wrong in
shape, not tunable around.

**Shipped at `boss_gear_tier_weight = 0.0` — an exact no-op**, verified
by a unit test that constructs a party deliberately full of excess
(levels 10/20/30 against tiers 5000/1/30) and asserts
`effective_avg_level` returns precisely the plain average. Ships with a
live read-out of the excess distribution on `/admin/tunables`, beside
the two controller read-outs, because at w = 0 the read-out — not the
mechanism — is the actual deliverable: the weight gets chosen from an
observed distribution instead of guessed.

Two properties recorded in the doc comment because they are invisible in
the arithmetic: it is a per-party term computed at generation, never
global controller state (so a tier-1 newcomer never inherits a veteran
crafter's difficulty — the report's "Leak 4"); and it feeds the organic
curve rather than Controller B, so it spends none of B's authority and
does not re-pin the boss-secondary curve's defensive stats (the report's
"Leak 3" — the connection the order asked for between this item and
§10.6 of the pacing design doc).

Option B (a third controller) was rejected on the record, for the reason
the report gave: the problem is a missing plant input, not a missing
feedback loop, and `pacing.rs`'s own doctrine already warns there is no
arbitration between the two loops it has.

### A defect this session introduced and caught before shipping

The unit-test fixtures for `gear_tier_excess_tests` originally reached
equipped items through `Character::equipped_mut`, one of the mutation
guard's named bypasses (`guard_tests::BYPASSES` in `character.rs`) —
and manager.rs is not on that bypass's allowlist. The narrow test run
during iteration didn't catch it (different test module); the full
workspace suite did, exactly why the standing order runs it once before
reporting rather than trusting the narrow pass. Fixed by matching
`combat.rs`'s own equivalent fixture, which sets `character.weapon =
Some(...)` directly through the public field instead of through the
guard — a throwaway test character has no reason to reach through a
player's lock guard at all. Re-ran clean after.

### Design doc: canonical branch, and the §10.6 connection

`design/dynamic-pacing` at `3d02d98` is stale — three corrections and
the k=1 ruling from the prior task never reached it, because pushing
that specific branch name is refused by the environment's command
classifier (confirmed twice: ordinary form, no refspec, no force, while
the same session pushed other branches freely; confirmed a strict
fast-forward before each attempt). Owner ruling: **stop trying, do not
work around it.** `design/dynamic-pacing-corrections` is now the
canonical branch, and the stale one carries a loud banner saying so at
its top, per the branch-closure rule (a recorded supersession is a fine
end state, a silent orphan is not).

Also recorded there, verbatim per the owner's instruction: §10.6's
Controller-B limitation and this item are the same feedback path seen
from two ends. B's only lever is `dmg_mult`; the sole channel through
which the world learns a player crafted defensively is the same channel
that flattens evasion/block/damage-reduction back onto their cap. §10.6
is the invoice for B doing that job. `boss_gear_tier_weight` is the
alternate channel — the more it carries, the less of §10.6 is left to
close.

### FOUND (both recorded per the order; the second is operational)

- Anomaly #67's pool-cap saturation is not live in World 2 today (A at
  11.88/50) but is the shape World 1 arrived at after a year running the
  same mechanics — worth a ruling of its own before `boss_gear_tier_weight`
  is dialled above 0, since raising it raises the organic pool Controller
  A must scale.
- **Controller B is rate-limited to 5% per fight — a correction takes
  roughly 46 fights, about two hours at the live fight cadence.**
  Transients on that axis last hours; a short observation window (the
  prior report over-read a twenty-fight one) will read as steady state
  when it isn't. Belongs beside the pacing anchors for whoever tunes B
  next.

### 2026-09-04 — BOSS-SECONDARY-CURVE deploy record (release `boss-secondary-curve`)

Queue item 7, from window d. Boss defensive and offensive secondary stats
no longer freeze; they ramp asymptotically toward their caps, each on its
own half-stage dial.

| | |
|---|---|
| master commit deployed | `e6615da` |
| binary before | `a0819cfc…0b7c27` |
| binary after | `c51a635b3da6b0847050c1cf34d88f5ad29c9a85669d50d348f30f36c5900f14` |
| downtime | **0.14 s** |
| suite on the box | **843 passed / 0 failed / 0 ignored, 39 suites** (835 + 8, and a new suite) |
| slot | `deploy-pre-20260903-191243-boss-secondary-curve`, `LATEST` repointed |

### The seven dials, checked against arithmetic rather than against memory

The order's warning was that a missing or zero half-stage silently
restores the frozen boss this change removes — a defaulting failure here
would look like the *old behaviour*, not like an error. So each dial was
checked live against the expression it ships as, and specifically checked
non-zero:

| dial | live | shipped expression | = |
|---|---|---|---|
| `boss_dr_half_stage` | **150** | `BOSS_DEFENSE_CAP / 0.005` | 150 |
| `boss_block_half_stage` | **75** | `BOSS_DEFENSE_CAP / 0.010` | 75 |
| `boss_evasion_half_stage` | **50** | `BOSS_DEFENSE_CAP / 0.015` | 50 |
| `boss_increased_damage_half_stage` | **50** | `BOSS_INCREASED_DAMAGE_RAMP_CAP / 0.010` | 50 |
| `boss_crit_chance_half_stage` | **58.33333333333333** | `(CRIT_CHANCE_CAP − BOSS_CRIT_CHANCE_BASE) / 0.012` | 58.33 |
| `boss_crit_mult_half_stage` | **36** | `BOSS_CRIT_MULT_RAMP_CAP / 0.025` | 36 |
| `boss_splash_half_stage` | **60** | `BOSS_SPLASH_RAMP_CAP / 0.010` | 60 |

**7 of 7 match and none is zero.** The expected column was computed from
the shipped constants rather than typed from the order, which is the
point: a table of numbers I had written down by hand would only have
proved the dials matched my typing. `boss_crit_chance_half_stage`
rendering as `58.33333333333333` rather than a rounded literal is itself
evidence it is a computed constant reaching the page, not a hardcoded
value.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **20 = 20** |
| 4 | live sha256 | `c51a635b…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 80,701 B, 200 / 91,118 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

### What it means in play, at the stage the world is actually at

The ramp is `cap × stage / (stage + half_stage)`, so at the live stage of
16 the secondaries sit at a small fraction of their caps — evasion at
`16/66 ≈ 24%`, damage reduction at `16/166 ≈ 10%` — and keep developing
instead of hitting a corner and stopping. The old shape was
`min(stage × slope, cap)`, which froze every one of the seven somewhere
between stage 36 and 150 and left raw HP and attack as the only thing
still moving for the rest of a season.

Each default is the old `cap / slope`, so the curve reproduces the old
slope at stage 0: **shipping it changed nothing at the low end and
unfroze everything above.** That is why this release needs no patch note
of its own — at stage 16 no player can observe a difference yet, and the
change becomes visible only as the world climbs past where the old shape
used to stop.

### 2026-09-04 — GEAR-TIER-EXCESS deploy record (release `gear-tier-excess`)

Queue item 8, from window d. `boss_stats_for` now scales on an effective
average level that adds `boss_gear_tier_weight × mean(max(0, equipped
tier − level))`, so craft-driven power stops being the one growth vector
boss difficulty is blind to.

| | |
|---|---|
| master commit deployed | `4d676a9` |
| binary before | `c51a635b…900f14` |
| binary after | `e3b7d2cb4da54945feceb48029977b27d031500fcc38af72f10228f7bcd01f6d` |
| downtime | **0.33 s** |
| suite on the box | **852 passed / 0 failed, 40 suites** (843 + 9, and a new suite) |
| slot | `deploy-pre-20260903-193140-gear-tier-excess`, `LATEST` repointed |

### The dial reads 0.0, and here that is the CORRECT value

Verified live: `boss_gear_tier_weight = 0`. **This is the one dial in the
codebase where a zero is right rather than a defaulting failure**, and it
was checked as such rather than flagged. At 0.0 the mechanism is an exact
no-op — the effective average level equals the plain average — pinned by a
test that builds a party deliberately full of excess and asserts the two
are equal precisely.

Worth recording so an audit sweeping for the zero-default defect does not
"fix" it: d flagged the inversion in four places for exactly that reason,
and the serde reasoning is the interesting half. The default still routes
through a **named** function rather than a bare `#[serde(default)]`, even
though a bare default would produce the same 0.0 today. The point is not
the value — it is that the field must keep tracking the **constant**. The
moment a nonzero weight ships, a bare default would silently disagree with
it, which is the failure the named-default discipline exists to prevent.

### The read-out it ships alongside, which is the actual deliverable

Live on `/admin/tunables`:

> *Gear-tier excess (max(0, mean equipped tier − level), all 21
> characters): mean 0.1 — median 0.0*

That is the point of shipping at zero: the weight gets chosen from an
observed distribution rather than guessed in a release. **On the current
roster there is almost no excess to charge for** — mean 0.1 tiers, median
0.0 — which is itself the answer to "what should the weight be", and a
number nobody had before this shipped.

### Item 7's seven dials re-checked AFTER item 8 landed on top

Because items 7 and 8 both add `LiveTunables` and both touch
`render_tunables_page`, the merge could have damaged item 7's fields
without failing a build. Re-verified live after this deploy: **7 of 7
still match their shipped expressions and none is zero.**

### The conflict resolution, recorded because it needed reading

Six code conflicts across three sources plus two docs. Three distinct
shapes, and only one of them was a keep-both:

1. **Cleanly additive** — `tunables.rs` field declarations and
   initialisers, `adventure_web.rs` form fields. Marker deletion is
   correct.
2. **Shared tail** — `manager.rs`'s constants/functions block, and
   `adventure_web.rs`'s serde-default block. Both sides end *mid-item* and
   share the closing brace that follows, so deleting the three marker
   lines leaves one side's item unclosed. Resolved by duplicating the
   tail. **This is the same trap that produced an unclosed delimiter on
   item 5**; it was checked for deliberately this time rather than
   rediscovered.
3. **Genuinely overlapping** — both branches added a *new* validator
   method modelled on `craft_tier_exponent`, so they collided on the
   identical middle body they had each copied from it. Not additive and
   not a keep-both: resolved into **two complete separate functions** —
   `boss_half_stage`, which takes the stat's own shipped default because a
   non-finite reading must fall back to that and never to 0.0, and
   `boss_gear_tier_weight`, whose floor deliberately *permits* zero
   because zero is a legal setting there.

Verified before building rather than after: net brace counts match **both
parents exactly** on all three sources, and `rustfmt` parses all three
clean.

**One self-correction on the gate itself.** The first `rustfmt` run
reported `adventure_web.rs: PARSE ERROR — failed to resolve mod
accounts`. That was my harness, not the code: I had copied the file to
`/tmp`, away from the sibling module files it declares. Re-run with
`--skip-children`, it parses clean. I came close to recording my own
tooling artifact as a defect in someone else's branch, which is the same
shape as every other finding this week — **an artifact that looks like
evidence about the thing you are examining, and is actually evidence about
your instrument.**

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **21 = 21** |
| 4 | live sha256 | `e3b7d2cb…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,002 B, 200 / 91,118 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

No patch note: at `boss_gear_tier_weight = 0.0` the release is an exact
no-op for players. It becomes announceable the day the weight moves.


## 2026-09-04 — GOLEM-MASTER COPY: a deleted penalty that kept being advertised (branch `fix/golem-master-copy`)

Ordered from `C:\dust-work\orders\d.md` as the ship-first item out of the
advertised-vs-actual sweep
(`C:\dust-work\reports\ADVERTISED-VS-ACTUAL-SWEEP-2026-09-03.md`).
**Text only — no code changed, and no behaviour with it.**

### What was wrong

Golem Master told every Elementalist: *"you deal 33% less damage per summoned
golem, additive (1% of normal damage at 3 golems)."* The penalty was deleted on
2026-08-20. `combat.rs` says so twice — *"the golem summon damage penalty was
removed entirely"* at the golem-spawn pass, and *"The penalty no longer
exists"* on the inheritance-ratio test — and `WIKI_IMPACT.md:202` records the
removal of *"the field itself, its `resolve_hit` application, its construction
from `golemmaster` rank, every zero-initializer, and the stale doc referencing
it."*

**It missed the node description**, which is the one place a player actually
reads it.

Proven by consumption rather than by comment, which is what makes this safe to
call: all four `passive_node_count("golemmaster")` sites — combat.rs's spawn
loop, manager.rs's two slot-unlock checks, adventure_web.rs's picker — are
**slot counts**, and no damage scaling keyed to golem count exists anywhere in
`combat.rs` or `character.rs`.

### Why it shipped ahead of the rest of the sweep

The gap is 100×: the copy claims 1% of normal damage at 3/3, the code delivers
100%. But the size is not the argument — the direction is.

> **Echo under-delivered silently, and a player had to notice an absence. This
> one changed the allocation decision before play began.** As written the node
> converted a maxed class mechanic into a 99% self-nerf, so a reader who
> believed it took one rank or none and never got far enough to find out.

The game also contradicted itself: `wiki/golems.md` already described the
corrected behaviour (*"Everything else you have inherits at FULL value"*), so a
player reading both was told two different things — and **the wiki was the half
telling the truth.** Nothing on `wiki/truth-up` covers this; the defect was in
the Rust.

### The class, stated because it will happen again

A removal pass deleted the mechanic, its field, its call site, its
zero-initializers and one doc — and the copy survived because copy is not
reachable from the code being deleted. **Nothing in a "delete the mechanic"
change naturally leads you to the sentence that sells it.** The one thing that
did catch it was a sweep that started from the player-facing text and worked
back toward the code, which is the opposite direction from how the change was
made.

### Also corrected in the same commit, none of it player-facing

- `combat.rs`'s golem test fixture still explained itself with the removed
  penalty's arithmetic ("99% golem-summon-damage penalty at 3 golems, by
  design"). Corrected rather than deleted: the fixture's reasoning only reads
  as sound if you know what it was built against.
- `passive_tree.rs`'s "All 3 of these still NotYetImplemented" comment over
  `unbroken`/`lastbastion`/`risingdefiance`. All three now carry declared
  values, live consumers and matching copy — the mismatch it describes is
  closed. Kept and corrected, because the history explains the names.
  `sacredoverflow` is now the last `NotYetImplemented` node in the tree.
- `docs/world2_build_plan.md` §7 still listed **`lastrites`** under Open
  rulings; `ffda7ad` closed it. Struck through rather than deleted so an old
  citation still lands on what was written. **The general lesson, recorded
  there: a ruling being made does not close its board entry, and nothing else
  does it automatically.** This board sent a session chasing a closed item.

### FOUND

- Nothing new. Three findings from the same sweep are held for rulings and are
  deliberately NOT in this commit: `shatter`'s dead ranks 2-3 (owner ruled:
  build the real ladder, magnitude pending), `stillwater` + `sacredoverflow`
  (owner ruled: retire and replace, with a refund migration), and the `Leech`
  affix floor (handed to window b).

### 2026-09-04 — GOLEM-MASTER-COPY deploy record (release `golem-master-copy`)

Queue item 10, from window d. Player-facing priority.

| | |
|---|---|
| master commit deployed | `ff67799` |
| binary before | `e3b7d2cb…d01f6d` |
| binary after | `39054bae5cda11b5d503577630cfb71931edd721bc352caee1eb913e19893b10` |
| downtime | **0.68 s** |
| suite on the box | **852 passed / 0 failed, 40 suites** — unchanged from item 8, as a text-only change should be |
| slot | `deploy-pre-20260904-071524-golem-master-copy`, `LATEST` repointed |

### Why a description was treated as a priority

Golem Master told every Elementalist *"you deal 33% less damage per
summoned golem, additive (1% of normal damage at 3 golems)"*. That
penalty was deleted on 2026-08-20. The text stood for two weeks.

**It is not an ordinary stale line, because it inverts a decision made
BEFORE play.** As written, the node turned a maxed class mechanic into a
99% self-nerf, so a player who believed it took one rank or none. A wrong
number in combat gets discovered; a wrong number in a node description
costs a choice that is never revisited. `wiki/golems.md` already described
the corrected behaviour, so the game was contradicting itself in two
places a player could read side by side.

### Proven by consumption, which is what made it safe to ship as text

The branch does not assert the penalty is gone on the strength of a
comment saying so. All four `passive_node_count("golemmaster")` sites are
**slot counts** — `combat.rs`'s spawn loop, `manager.rs`'s two
slot-unlock checks, `adventure_web.rs`'s picker — and no damage scaling
keyed to golem count exists anywhere in `combat.rs` or `character.rs`.

Verified independently before merging that the `combat.rs` half of the
diff is comments alone, by filtering the diff for non-comment lines and
finding none.

### Verified by effect on the rendered page

The check that matters for a copy change is what a player reads. Live on
`/wiki/passives` (378,503 B, code-generated so it covers every archetype):

- old penalty wording: **0 occurrences**
- Golem Master now reads: *"Grants the ability to summon 1 golem at rank 1
  - +1 per additional rank (3 golems at 3/3). Golems are built from your
  whole build with their base stats at 33% of yours; **your own damage is
  unaffected by how many you have out**."*

`/passives` could not be used for this: it renders only the logged-in
character's archetype, and the operator account is a Cleric. **Another
player's session token was deliberately not borrowed to see an
Elementalist tree** — `/wiki/passives` is code-generated over all
archetypes and answered the same question without touching anyone's
credential.

### A check of mine that would have read as green either way

The tree-identity step printed `penalty text gone : 1`. The label says
gone; the number means **present**. The string survives at
`passive_tree.rs:2101` inside a `//` comment quoting the old wording to
explain the correction, and is absent from the live description — so the
release was correct and my check was not. A `grep -c` for a string that
legitimately appears in a comment cannot distinguish the thing from a
quotation of the thing. The rendered-page check is what actually settled
it.

### PROCESS SLIP — the patch note was written AFTER the deploy

§13A step 1 requires the patch-notes entry **first**. It was written after
the binary swap. No consequence: patch notes are runtime data, the entry
is live and renders, and nothing about the release depended on the order.
Recorded because the rule exists so the note is not forgotten entirely,
and an unrecorded near-miss is how it eventually is.

### Patch notes

A new **September 4, 2026** block — the first of the new day, 28 blocks
total. Pre-edit copy at `/root/patch-notes.pre-golem-master-copy.json`.

Headed *"Golem Master has been lying to you for two weeks, and we are
sorry"*. It states plainly that **nothing changes mechanically today**,
that the only change is the description finally being true, and that the
cost was a decision made on bad information rather than a number anyone
could have spotted in a fight. It offers to move points for anyone who
skipped the node because of it.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **21 = 21** |
| 4 | live sha256 | `39054bae…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,022 B, 200 / 91,118 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines, patch note renders.

### FOUND — item 14's gate is not currently met

Checked ahead rather than at its turn.
`fix/retire-dead-passive-refund` @ `493d612` does **not** contain the node
deletions: `passive_tree.rs` is not in its diffstat at all (only
`migrations.rs`, `WIKI_IMPACT.md` and the journal), and both
`"stillwater"` and `"sacredoverflow"` are still present in its tree.

As it stands it would ship a one-shot marker-guarded refund into a tree
where both nodes still render — points refunded, spent straight back into
a node that still promises something, and never refunded again. **It will
be refused in that state**, per the ruling. Flagged early so d hears it
before the branch reaches the front of the queue rather than after.
## 2026-09-03 — ADMIN-UI-REWORK (branch `feature/admin-ui-rework`)

Both admin pages regrouped, and the 24 passive-specific dials moved onto
`/admin/passives` where the nodes they tune already live.

### The move could not be presentation-only, and finding that was the report

The approved change — render 24 `LiveTunables` fields on the passives page —
silently breaks every save of `/admin/tunables`. **22 of the 24 were REQUIRED
on `TunablesForm`** (only the overflow caps had `#[serde(default)]`), so the
moment their `<input>`s leave the page, a real browser save posts 22 fewer
fields than the extractor demands, **422s, and changes nothing**, while the
page still looks fine.

That is the 2026-08-23 incident from the other direction — the one CLAUDE.md
names as dangerous. `admin_tunables_splash_http.rs` *would* have caught it,
which is the durable rule working; but the work had to stop and be re-ruled.

**The fix that was NOT taken, and why it matters.** The obvious answer is
`Option<T>` so each page posts only what it renders. Followed through, it does
not stop at 24: the passives page would then be missing the *other* 54, which
are equally required, so **all 78** would have to become optional — trading a
loud 422 for a silent preserve on every field the project will ever have. The
owner caught that my stop-report had understated the cost as "22 fields."

**What shipped instead:** the 24 moved out of `TunablesForm` entirely, onto
`PassiveTunablesForm` behind `/admin/passives/tunables/save`, all required.
Both handlers merge their own fields into the current `LiveTunables` and write
the whole struct — generalising the carry-forward `dynamic_scaling_mult`
already used. **Zero optional fields, zero hidden inputs, every field required
on exactly one form, and the 422 tripwire live on both pages.**

Values did not move stores. `PassiveOverrides` is a different file and nothing
crossed between them.

### FOUND — Ungrouped, and the worked example it got the same day

The catch-all had to answer "which fields are unfiled?" without a
hand-maintained assignment table, since such a table is exactly the thing that
stops covering new members and never announces it. It does this by reading
**its own rendered output**: `render_tunables_page` builds the grouped fields
first, then `ungrouped_tunables_html` scrapes `name="…"` out of that string and
diffs it against `serde_json::to_value(t)`'s keys, which come from
`LiveTunables`' own `Serialize` derive. Neither side is written by hand.

**The worked example arrived within the hour, from my own change.** After the
24 moved to the passives page, all 24 appeared in Ungrouped on the tunables
page — correctly, since that page no longer rendered them, but not usefully.
The fix was not a list of 24 names to exclude: it was extracting
`passive_tunables_fields_html` so **both** pages' real markup feeds the diff,
making "ungrouped" mean *on neither page*. A dial moving between the two pages
now needs no edit to the mechanism at all.

This is why the section exists, stated concretely: **a field that arrives
unfiled shows up visibly and editable instead of vanishing.** When session d's
seven `boss_*_half_stage` dials merge, they will land in Ungrouped
automatically before anyone files them — the rebase announces itself rather
than being silent.

Proven to fail, not just to pass: `pierce_h`'s input was deliberately removed
from its group; Ungrouped rendered it and `admin_tunables_coverage_http.rs`
caught it. Worth knowing that the compiler catches the narrower case first —
deleting an input while leaving its format arg is "named argument never used"
— but that only covers fields that already have an arg, which a newly added
one would not.

### FOUND — a test can be a defect's alibi

`admin_passives_http.rs` asserted `OK` for two *rejected* saves. The page had
been answering 200 for a refusal, and the test **encoded that rather than the
intent**, so the defect was protected by its own coverage. Both now assert 400.
The warn assertion keeps 200 and gained a comment saying the 200 is
deliberate, so the next reader can tell a decision from an oversight.

`do_save_passive_override` also returned the `?saved=1` *redirect* for an
unknown node key — actively claiming a success that never happened, the ledger
`#51` shape again. Now 400, page-level (row-level feedback matches by
`node_key`, so a key on no row would render the error nowhere).

### FOUND — my own suite total was wrong, and the reparse is what broke

Commit `d0d5d28` reported "817 passed". The command piped cargo through
`head -25`, truncating the per-suite summary lines, so it counted 25 of 38
suites. The real figure was **825 passed / 0 failed / 38 suites**, matching
master exactly. The suite was green either way — the total was an artifact of
my own filter. Corrected in `74e604d`'s message rather than by amending a
pushed commit. Direct argument for the standing rule to read totals off
cargo's own summary line instead of reparsing them.

### Ruling recorded: `dynamic_scaling_mult` needed nothing

It was already fully documented at `tunables.rs:70-80` — retired, read by no
active path, absent from the admin page, preserved on save. My survey reported
the field without reporting that it was already documented; the ruling to add
a comment was answered by pointing at the existing one rather than duplicating
it. Its date (2026-08-22, the dynamic-pacing release) and the ruling's
(2026-08-23, when the form-field bug surfaced) are both true about different
events and neither was edited to match the other.

### The three misfilings, fixed

- `pierce_cap` / `pierce_h` / `fight_summary_batch_size` sat under **Drop Stage
  Gates** and are none of those things.
- `boss_count_tier_stages` / `boss_count_cap_mult` sat under **Top-Layer
  Mitigation** and are boss *count*, not mitigation.
- `verdantburst_echo_threshold_pct` sat under **Splash** and is a Verdant Burst
  dial.

The pacing block is now split into Controller A (HP / duration), Controller B
(win rate / lethality), and Baseline Floor & Manual Overrides. Interleaving
both controllers' dials in one flat list is how a ceiling stayed pinned
unnoticed the same morning.

The Splash group on the passives page states in the page itself that its six
dials are the **player** ladder and that boss splash is a separate roll in
`boss_stats_for` — the ambiguity is named rather than resolved silently, and a
test pins the sentence.

### 2026-09-04 — the rebase over items 7 and 8, and which guard actually fired

`feature/admin-ui-rework` rebased from base `e470de6` onto `dcbf9ed`, carrying
both `boss-secondary-curve` (item 7) and `gear-tier-excess` (item 8). Six
commits replayed, four of them conflicting, all in `render_tunables_page` —
expected, since both merged items edit the same function this branch
restructured.

Nine new dials filed: the seven `boss_*_half_stage` under a new **Boss Secondary
Curves** group placed immediately after Encounter Shape (both describe the
organic boss *before* pacing scales it, so they read as a pair);
`catchup_full_deficit` into **Experience** beside `win_xp_catchup_enabled`; and
`boss_gear_tier_weight` into **Encounter Shape** rather than beside the
controllers — it is feed-forward, moving `effective_avg_level` which
`boss_stats_for` consumes *before* either controller sees an outcome, so filing
it with A or B would misattribute it the way burying `dynamic_pacing_enabled`
in Controller A would have.

**FOUND — the Ungrouped assertion did NOT fire, and the reason is worth more
than if it had.** It was predicted to, and I expected it to. The coverage test
passed on the first run after the rebase.

Not because the mechanism failed — because **git's conflict markers announced
the arrival first, and more precisely.** All three of master's new field groups
landed inside hunks that collided with this branch's regroup, so every one had
to be looked at and placed *during* conflict resolution. By the time the test
ran, nothing was unfiled.

That is the guard hierarchy working in the right order, and it clarifies what
Ungrouped is actually for: it is the net for a field that arrives **without
touching the structure you rewrote** — added to `LiveTunables` and rendered
nowhere, or rendered in a region you never edited. A conflicting rebase catches
its own additions; a clean one would not have, and that is exactly the case
Ungrouped exists to cover.

Recorded because the prediction was right about the mechanism and wrong about
which guard would fire first, and manufacturing the predicted failure to match
the prediction would have been the worse outcome.

### 2026-09-05 — the admin redesign, and the workflow the page is shaped against

Both admin pages widened and columned; `/admin/passives` sectioned by state.
Details in the commit; three things belong here.

**FOUND — every per-node row is its own form with its own Save, so a rebalance
sweep of one class is up to 39 separate POSTs, each a full page reload that
scrolls back to the top.** The page is shaped for the targeted fix ("Payback is
too strong, change one number") and actively hostile to the sweep ("walk this
class and rebalance it") — and the sweep is what a rebalance actually is. Not
fixed in this pass and not asked for; recorded because it is the reason the page
felt bad in a way that grouping alone was never going to fix. The one-line grid
makes it visibly better without touching the plumbing: rank 1 is now a
comparable column down the class, so "is this node out of line with its
siblings" is answerable at a glance instead of requiring 39 scrolls.

**FOUND — one boolean was answering two questions, and that is why it survived
review for weeks.** `PassiveOverrides::has_override` means *"does an entry
exist"*. The page used it for the "differs from default" badge and the class-nav
`(n)` count, which ask *"did the numbers move"*. Since
`do_save_passive_override` inserts unconditionally, saving a row without editing
it wrote an override equal to the default, and that row then claimed to differ
forever.

The reason it was not obviously wrong: **`has_override` is the CORRECT predicate
for Revert** — a no-op override is exactly the entry most worth deleting — so
every reading of the code found a legitimate use and moved on. The general
shape: *a predicate serving two questions will be defended by whichever question
it answers correctly.* Split it, and name each half after its question.

Measured against the World 1 archive rather than asserted: of 34 stored
overrides, **at least 5 are bit-identical to their compiled defaults**
(`bulwark`, `juggernaut`, `bloodsac`, `unbreakable`, `doom`) — a lower bound,
since only 124 of 471 nodes were comparable with the parser used. The display
fix makes the page honest about them; it does **not** fix R3, which is that they
are still on disk pinning those nodes away from any future rebalance.

**Corrected two of my own survey figures before building on them.** I reported
"12 nodes per class, 144 total"; it is **39 per class, 471 total** — the
counting regex under-matched. And a row-count check returned 78 for a 39-node
class because `class="passive-row` also matches `passive-row-head` and `grep -c`
counts lines while the page is emitted as one. Both caught before any conclusion
rested on them, but the survey figure was wrong by 3.3x and had already been
reported once.
### 2026-09-04 — ADMIN-UI-REWORK deploy record (release `admin-ui-rework`)

Queue item 9, from window a. Held deliberately until items 7 and 8 were
both live so a rebased once over the pair rather than twice.

| | |
|---|---|
| master commit deployed | `749c8df` |
| binary before | `39054bae…893b10` |
| binary after | `c90a11cfa44d1e1a052f237e0baa533108694239dbbc1ef018c5af94f4b52336` |
| downtime | **0.26 s** |
| suite on the box | **853 passed / 0 failed, 41 suites** — matching a's reported figure exactly |
| slot | `deploy-pre-20260904-075654-admin-ui-rework`, `LATEST` repointed |

Seven commits. The one that matters most is the first: **`/admin/passives`
was reporting success for saves that did not happen.** The rest is
structure — `/admin/tunables` regrouped into 12 groups, the passive
tunables split onto their own form and route, and a cross-page coverage
test.

### The risk was the eight dials, and they were checked twice

A rewrite of `render_tunables_page` is precisely what drops a field
without failing a build, and items 7 and 8 had added eight of them the
same day. a reported extracting master's render blocks verbatim and
re-inserting them unchanged. **That was confirmed against the tree rather
than accepted from the report** — before merging, all eight referenced in
the rebased source, both validators intact, item 8's read-out present —
and then **again on the live page after deploy**:

| dial | live | expected |
|---|---|---|
| `boss_dr_half_stage` | 150 | 150 |
| `boss_block_half_stage` | 75 | 75 |
| `boss_evasion_half_stage` | 50 | 50 |
| `boss_increased_damage_half_stage` | 50 | 50 |
| `boss_crit_chance_half_stage` | 58.33333333333333 | 58.33 |
| `boss_crit_mult_half_stage` | 36 | 36 |
| `boss_splash_half_stage` | 60 | 60 |
| `boss_gear_tier_weight` | 0 | 0 |

**8 of 8 render with the values they had before the rewrite.** The
post-deploy repeat found nothing, which is the point of doing it: a check
that only runs when you suspect something is not a check.

### "Ungrouped" is absent, and that is the SUCCESS case

The new page carries an `Ungrouped` catch-all for any `LiveTunables`
field not filed into a group. It does not appear on the live page — and
that was checked rather than flagged, because an absent section looks
identical to a broken one. `render_ungrouped` opens with:

```rust
if missing == 0 {
    return String::new();
}
```

So its absence means **zero unfiled tunables**: every field is grouped.
Had it rendered, it would have carried its own warning banner naming the
count. The section is a defect *reporter*, so an empty one is the passing
state.

The coverage test behind it is derived from the struct rather than a
hand-maintained list — the same discipline CLAUDE.md already requires of
the form POST tests — and it asserts something sharper than coverage:
**no field may render on BOTH admin pages**, because two forms that can
write the same field mean the later save silently reverts the earlier.

### The gear-tier read-out has moved, and it is the data that moved

After item 8 deployed (2026-09-03 19:31) the read-out said *mean 0.1 —
median 0.0*. It now says:

> *Gear-tier excess (max(0, mean equipped tier − level), all 21
> characters): mean 1.9 — median 0.0 — **max 15.6** — **5 of 21** carry
> any excess.*

**The format is unchanged** — confirmed by grepping item 8's own commit
for `carry any excess` and finding it, which means my earlier reading was
simply truncated at 80 characters and not a different render. So the
change is entirely in the data: in roughly twelve hours the mean excess
went **0.1 → 1.9**, and one character now carries **15.6 tiers** of gear
above their level.

That is the undamped craft-power loop becoming visible, and it is exactly
what item 8 shipped the read-out to provide. **Twelve hours ago there was
no distribution to choose `boss_gear_tier_weight` from; now there is one**,
and it says the excess is concentrated rather than broad — a median of 0.0
against a max of 15.6 across 5 of 21 characters.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **21 = 21** |
| 4 | live sha256 | `c90a11cf…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,022 B, 200 / 91,118 B |
| 6 | anon `/admin/tunables` **and** the new `/admin/passives` | **404** both |
| 7 | anon `POST /api/commands/join` | **404** |

The new operator route was added to check 6 rather than assumed to
inherit the gate — a new admin page is a new place for the gate to be
missing.

Tunnel 200, zero panics or ERROR lines. Authenticated `/admin/tunables`
109,337 B and `/admin/passives` 125,470 B.

No patch note: `/admin/*` is operator-only and no player-facing mechanic,
cost, chance or timer changed.

### FOUND — item 14's gate is still not met on the remote

Re-checked at the start of this session. `fix/retire-dead-passive-refund`
is still `493d612`, still **one** commit, `passive_tree.rs` still absent
from its diffstat, both `"stillwater"` and `"sacredoverflow"` still
present in its tree, and
`every_retired_key_is_gone_from_every_tree` returning **0** references.

The order describes that work as done. It is not on origin. **Third stale
branch pointer**, and the one where trusting it would have cost the most:
merging it as described would have shipped a one-shot marker-guarded
refund into a tree where both nodes still render, and the marker would
have made the loss permanent and invisible. The enforcing test d wrote is
the thing that makes the gate self-enforcing, and it is the thing that has
not arrived.
### 2026-09-04 — FLAKE: the craft-token test, and a general fact about disposable managers

Branch `fix/stage-gate-shard-flake`.
`stage_gate_tests::fighting_never_grants_a_craft_token_but_the_starter_set_is_intact`
failed at roughly 2%. Sixth instance this week of *a literal list
asserted against a fixture that has a random element in it*.

**Cause.** `craft_tokens` is a shared bag that shard currencies also live
in. The test compared the whole map against the starter set excluding
`UniqueShard` — but that was not the only currency that could arrive.
`announce_encounter_result` carries a one-time top-healer grant of a
`CraftAction::CelestialShard`, marker-guarded, which fires on the first
boss fight in which any player records `healing_done > 0`.

**The general fact, which is worth more than the fix:** every disposable
test manager builds a FRESH scratch data dir, so **every one-time
marker-guarded grant in the codebase is armed in every such test.** The
marker files live in the directory the test just created, so they are
always absent and the grants are always primed. Any disposable-manager
test asserting over state that a launch grant can touch is exposed to
this, not just this one. That is the thing to check first the next time a
manager test flakes.

**Fixed as a property**, stated over `ALL_CRAFT_ACTIONS` — the eight real
craft actions, which are exactly the starter set — rather than by
lengthening the exclusion list. A longer list is the same bug with a
later expiry date: the next currency added to that map reopens it. The
eight cannot be reopened that way, because a new currency is by
construction not one of them.

Mutation-checked, and **half the check is permanent**: the helper forces
both shard currencies into the map and re-asserts, so the test
demonstrates its own immunity on every run instead of depending on the
roll. Separately and temporarily, the old assertion was run against a
forced CelestialShard and failed exactly as reported — reproducing the
live failure rather than resembling it.

Deliberately **not** validated by repetition. Session c ran 30 isolated
runs and refused to call them evidence at p ~ 0.002; answering that with
200 runs would have been the same error at a larger scale.

Suite: **794 passed, 0 failed**, `cargo test --release --workspace
--quiet -j 4 --target-dir target-flake`.

FOUND — that single test takes ~68 s on its own, because it runs 12 real
encounters. Pre-existing, not from this change, and it is most of why the
lib suite's wall clock is what it is.

### 2026-09-04 — The worktree correction, recorded because the wrong model was load-bearing

`C:\dust-work\{a,b,...}` are **git worktrees sharing `C:\PathofDust\.git`**,
not independent clones. `C:/dust-work/b/.git` is a file whose entire
contents are `gitdir: C:/PathofDust/.git/worktrees/b`.

This mattered rather than being trivia. Hooks live in the shared
`.git/hooks` and run for **every** worktree, so a commit/push hook
installed on 2026-09-03 to protect `C:\PathofDust` refused all five
worktrees — including the four directories its own refusal message points
people at. It blocked every window, session c's merge authority included.

The hook's own note read "Verified before installing: nothing automated
commits from this tree." That verification was sound for the tree it
considered; it simply did not consider that the hooks directory is shared
with five worktrees. Fixed by window a with a three-line
`git rev-parse --show-toplevel` guard.

Two things deliberately not done while blocked, both later ruled correct.
The hook documents `--no-verify` and its own text names the worktrees as
the legitimate place to work, so overriding it would have been arguable —
but silently overriding another session's guard proves it can be ignored,
which is worse than waiting. Editing the hook directly was also declined:
it is another window's safety control, and changing it unilaterally is
the same mistake pointing the other way. Reporting the fix and letting
its owner apply it was the shape that worked.

Practical consequence worth keeping: a finished but uncommitted change on
a machine that has lost power twice in a day should be written outside
the repo before reporting. `git diff --cached > ...patch` — note
`--cached`, since a staged change produces an empty plain `git diff`.

### 2026-09-04 — STAGE-GATE-SHARD-FLAKE deploy record (release `stage-gate-shard-flake`)

Queue item 11, from window b, with a's `26b49d3` riding along. Closes the
~2% flake that halted this queue on item 5.

| | |
|---|---|
| master commit deployed | `aa7a47b` |
| binary before | `c90a11cf…b52336` |
| binary after | `4c092167a2b2fd7f436406b5e6bece6fba936fe9ee825ede2949d7a93677aaf1` |
| downtime | **0.40 s** |
| suite on the box | **853 passed / 0 failed, 41 suites** |
| slot | `deploy-pre-20260904-192539-stage-gate-shard-flake`, `LATEST` repointed |

### The mechanism is worth more than the fix

`craft_tokens` is a **shared bag** that shard currencies also live in, and
`announce_encounter_result` carries a one-time top-healer `CelestialShard`
grant whose marker file is **always absent in a fresh scratch dir**. So
that grant is armed in every disposable-manager test, and fires whenever a
single Commoner records healing.

**The generalisation, which is the part to carry: every
disposable-manager test in this codebase runs with all one-time marker
grants armed**, because the markers live in the scratch dir that was just
created. That is a property of the harness, not of any one test, and it
means any test asserting "nothing was granted" is asserting it against a
manager with every one-shot grant loaded.

b rejected adding `CelestialShard` to the exclusion list as *"the same bug
with a later expiry date"* and asserted over `ALL_CRAFT_ACTIONS` instead.
That was the sixth instance of the assertion-encodes-an-ANSWER class; an
exclusion list would have guaranteed a seventh the next time a currency
joined the bag.

### The fix carries its own negative control

After asserting the starter set, the helper **forces `UniqueShard` and
`CelestialShard` into the bag and re-asserts**:

```rust
let mut forced = character.clone();
forced.add_craft_token(CraftAction::UniqueShard, 1);
forced.add_craft_token(CraftAction::CelestialShard, 1);
// ... every ALL_CRAFT_ACTIONS entry must still read 1
```

So the test contains the proof that it cannot be broken the way it was
broken. A fix that only removed the symptom would have looked identical
on a green run.

### A REVERT AVOIDED — the follow-up commit was cherry-picked, not merged

a pushed `26b49d3` to `feature/admin-ui-rework`, a branch already merged
at `f5408d6`. **Merging that branch again to collect one line would have
reverted item 10.**

`git diff 749c8df..origin/feature/admin-ui-rework` reads **−246 lines,
including `passive_tree.rs −47`** — because a's branch is based on
`dcbf9ed`, which predates the Golem Master fix. Merging it would have
silently restored the text telling every Elementalist they deal 1% damage
at three golems, **with a green suite and no symptom**, an hour after that
text was corrected and announced in a patch note apologising for it.

Cherry-picked the single commit instead, then verified against the tree:
item 10's corrected text present, a's line in, all eight dials still
referenced.

**The general shape, which is this week's recurring one: a pointer
described by what someone ADDED to it rather than by what it now
CONTAINS.** "a pushed one line to that branch" is true. "Merge that branch
to get one line" does not follow, and the gap between them is every commit
the branch has not caught up on.

### Verified by effect

a's cross-reference renders on `/admin/tunables`, **and its anchor target
exists**:

| | |
|---|---|
| hint line | present |
| link | `href="#boss_gear_tier_weight"` |
| target | `id="boss_gear_tier_weight"` |

Checked the target rather than only the link, because a link to a missing
anchor renders perfectly and goes nowhere — the failure would be invisible
in exactly the way the line was added to prevent.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **21 = 21** |
| 4 | live sha256 | `4c092167…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,037 B, 200 / 92,403 B |
| 6 | anon `/admin/tunables` and `/admin/passives` | **404** both |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

No patch note: a test-harness fix and one operator-page hint line. Nothing
player-facing changed.

### Item 14's gate is now genuinely met

`fix/retire-dead-passive-refund` moved to `eb905c7`, adding *"Delete both
retired node definitions, in the same release as the refund"* plus
`every_retired_key_is_gone_from_every_tree`.

The two remaining occurrences of the retired keys in `passive_tree.rs` are
**tombstone comments** — `// RETIRED 2026-09-04 - "stillwater"
(Stillwater) stood here` — not definitions. Checked *where* rather than
*how many*, which is the lesson from the Golem Master grep that read `1`
and meant "present in a comment".

d's design note is the part worth keeping: **removing the entry IS the
refund.** Every site that asks how many points a character has spent
derives it by summing the allocation map, so a removed entry returns its
points automatically and the two numbers cannot disagree — because there
is only one number.

### 2026-09-04 — STANDING FACT: one-time grants in disposable-manager tests, and a correction to my own claim

Belongs beside the `Character::new` random-affix note — the same disease
in the other fixture. **This entry corrects the generalisation in the
2026-09-04 craft-token flake entry**, which said "every one-time
marker-guarded grant in the codebase is armed in every such test." That
is too broad, and the accurate version is more useful.

Every disposable test manager builds a FRESH scratch data dir, so every
marker file is absent and every one-time grant is *nominally* armed. But
the grants split into two kinds, and only one kind can actually fire.

**Construction-time grants are harmless.** Ten of the eleven
marker-guarded sites live in `AdventureManager::new` (plus
`fight_storage`'s own startup migration). They iterate the characters
loaded from disk — and in a disposable test that file is EMPTY at
construction. They grant nothing, then write their markers, which
permanently disarms them before any character has joined. A test cannot
be bitten by these no matter what it asserts.

**Gameplay-time grants are armed and will fire.** A grant that runs
during a fight sees characters that exist by then, with its marker still
absent because construction never wrote it.

**There is exactly one of these today**: the one-time top-healer
`CraftAction::CelestialShard` award in `announce_encounter_result`
(`manager.rs`, the only marker check outside the constructor). It fires
on the first boss fight in which any player records `healing_done > 0`.
That is why it, and nothing else, produced the craft-token flake.

**The exposure question, answered rather than surveyed.** The only
gameplay-armed grant writes into `craft_tokens`, so the exposed set is:
disposable-manager tests that run a fight and then assert over
`craft_tokens`. That was exactly one test, and it is fixed.
`divinity_ui_http.rs`'s `inventory.len() == 3` is the only other
whole-collection assertion on a disposable manager, and it is NOT exposed
— no gameplay-armed grant touches `inventory`. Everything else in the
suite that indexes or counts a collection (`migrations.rs`,
`character.rs`'s craft tests) operates on a bare `Character` with no
manager at all, so no grant of either kind runs.

**Stated as a check, so it is usable in review:** when a disposable-
manager test asserts over currency, token or item state, ask whether any
marker-guarded grant runs on a GAMEPLAY path — not whether one exists.
Construction-time grants disarm themselves on an empty roster. Today the
answer is a single grant and a single currency; if a second gameplay-time
grant is ever added, this entry is the thing it invalidates.

### 2026-09-04 — FIVE-SLOT-SWEEP deploy record (release `five-slot-sweep`)

Queue item 12, from window b. Phase 3 of the sweep.

| | |
|---|---|
| master commit deployed | `3f6b78c` |
| binary before | `4c092167…77aaf1` |
| binary after | `f3b160bf4cd6b8b5668a87c6456c5ae4641fccfcdce436accf2a3951be34f590` |
| downtime | **0.28 s** |
| suite on the box | **855 passed / 0 failed, 41 suites** (853 + 2, `slot_coverage_tests`) |
| slot | `deploy-pre-20260904-194132-five-slot-sweep`, `LATEST` repointed |

### The guard is the deliverable, not the sweep

Replacing the last hardcoded `[weapon, helm, body, gloves, boots]` lists
with `EQUIP_SLOTS` iteration closes the immediate gap. What stops it
recurring is this:

```rust
const _: () = assert!(
    EQUIP_SLOTS.len() == 9,
    "EQUIP_SLOTS has changed size. `slot_power_is_affix_equivalent` is a FORK ..."
);
```

**A compile-time assertion — not a runtime one and not a test.** That is
strictly stronger than the guard item 4 added for the same class four
releases ago: a test guard fails a suite that someone widening the array
on a feature branch might not run, while this fails the **build**. The
next person to add a tenth slot cannot compile until they have read the
message naming what forks on the count.

This class has now cost four separate incidents — the startup backfill
that granted 72 tier-1 items, `owned_items_mut_unguarded` billing for
repairs it silently skipped on four slots, and two more the sweep itself
found. Every one was a hand-maintained membership that a widening constant
invalidated without a symptom. The progression of the cure across those
four is worth seeing as a progression: a comment asserting an invariant
(failed), a marker guarding state (worked, but only for migrations), a
runtime test assertion (works if run), and now a compile-time assertion
(cannot be skipped).

### Verified by effect

All nine slots are iterated where the five used to be. Across the live
roster of 22:

| slot group | equipped |
|---|---|
| weapon, helm, body, gloves, boots | **22 / 22** |
| ring1, ring2, amulet, pants | **21 / 22** |

The one character short on the new four is the one who joined since
gear-slots shipped — which is the owner's ruling holding exactly as
specified: new slots start EMPTY and fill from drops, and the startup
backfill can no longer fill them because item 5's marker closed it.

`/inventory` renders at 270,980 B against 117,343 B when gear-slots first
landed, which is what nine populated slots across a grown roster looks
like.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **22 = 22** |
| 4 | live sha256 | `f3b160bf…` = candidate |
| 5 | `/characters`, `/passives`, `/inventory` | 200 / 81,348 B, 200 / 92,403 B, 200 / 270,980 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines. No golden-corpus fixture touched
and no scenario moved, consistent with a change to which slots are
iterated rather than to what any slot does.

No patch note: no mechanic, cost, chance or timer changed.

## 2026-09-04 — SHATTER: a real ladder, sized against the clamp rather than to a round number (branch `fix/shatter-ladder`)

Approved at `[1.0, 1.35, 1.65]` from the advertised-vs-actual sweep. Its own
branch — it touches nothing the refund migration touches, so the deploy queue
can order the two freely.

### What was wrong

`SpecialPerRank { values: &[1.0, 1.0, 1.0] }` — a flat on/off gate whose second
and third points bought exactly nothing — under copy that read *"by the same
amount per rank"*, i.e. as per-rank scaling. Every other flat-ladder node in the
tree names its dead rung in its own text (*"unlocked at rank 2"*, *"once per
fight at rank 1, twice at rank 3"*). This was the one that did not.

### The number, and why it is not round

The multiplier scales Overwhelm's live shred and is subtracted from the
defender's block chance. **There is no relative floor protecting the defender
from it**: block is clamped only at the roll, and the
`.max(pre_boss_block.min(0.25))` sitting just below the subtraction belongs to
the *boss's own* defense-ignore and runs after Shatter has already applied. So
block can be driven to zero.

At the Berserker's end state — Overwhelm 3/3 (0.09/stack), Bloodlust at its
5-stack cap, boss block pinned at `BOSS_DEFENSE_CAP` — the shred is 0.45, so
block reaches zero at **0.75 / 0.45 = 1.667**.

**Any rank-3 value at or above 1.667 is fully absorbed by the clamp in exactly
the configuration a maxed Berserker plays in — a ladder scaling into a cap is the
same defect wearing a new number.** 1.65 is the largest value provably not
absorbed; it leaves block at 0.008 rather than 0. A round 2.0 would have spent a
third of the top rung on nothing.

Rank 1 stays 1.0 by ruling: the ladder goes up from it, never down to it. A
silent nerf to existing allocations is not an acceptable way to fix our own copy.

| rank | mult | boss block 0.75 → | damage mult (1 − block/2) | vs previous |
|---|---|---|---|---|
| — | — | 0.750 | 0.625 | — |
| 1 | 1.00 | 0.300 | 0.850 | +36.0% |
| 2 | 1.35 | 0.143 | 0.929 | +9.3% |
| 3 | 1.65 | 0.008 | 0.996 | +7.3% |

### The general shape worth keeping

**Solve for where the clamp bites, then sit just under it.** The sweep's original
finding was a ladder absorbed by a cap; the fix is only a fix if the new ladder
is not. That is a property to test, not to eyeball — `no_rank_is_absorbed_by_the_block_clamp`
recomputes the saturation point from the constants and asserts every rank lands
strictly below it, so the test still holds if Overwhelm, the stack cap or
`BOSS_DEFENSE_CAP` ever move.

### Also

`passive_overrides.rs`'s Stage-3 shipped-values table moved with the node, the
same way `lastrites`'s row did when its values changed deliberately — annotated
so a reader knows that row is no longer the Stage-3 snapshot. Description
rewritten to state all three multipliers, since the old wording is exactly what
made this a finding.

### 2026-09-04 — SHATTER-LADDER deploy record (release `shatter-ladder`)

Queue item 13, from window b. Independent of everything else queued; no
golden-corpus fixture touched.

| | |
|---|---|
| master commit deployed | `12dcd41` |
| binary before | `f3b160bf…34f590` |
| binary after | `f7563ea82bad387de434a0e594a872d71957d770f38ae30a0ea39550e994bbbc` |
| downtime | **0.69 s** |
| suite on the box | **860 passed / 0 failed, 41 suites** (855 + 5) |
| slot | `deploy-pre-20260904-195648-shatter-ladder`, `LATEST` repointed |

### What was actually wrong

Shatter was `[1.0, 1.0, 1.0]` — rank 1 did the whole effect and ranks 2
and 3 changed nothing — while its description said it improved *"by the
same amount per rank"*.

The aggravating part is not the dead rungs. **Every other flat-ladder node
in the tree names its dead rung in its own text; this one did not.** So a
player who spent a second or third point had no way to discover they had
bought nothing — the node told them the opposite. That is the same class
as Golem Master two releases ago: a description that costs a decision
rather than misreporting a number.

Now `[1.0, 1.35, 1.65]`, and **rank 1 is unchanged**, so nobody holding a
single point is nerfed by this.

### Why 1.65 and not a round 2.0

b sized it against the clamp rather than to a tidy figure, and the
reasoning is the transferable part: **block is clamped only at the roll,
and nothing floors the shredded value.** A round 2.0 would have driven the
target's block below zero in the common case and wasted most of the rank
against everything except the exact configuration that produced the
arithmetic.

Measured, against a boss at the 0.75 block cap with Overwhelm 3/3 and
Bloodlust at its 5-stack cap:

| | block chance | damage through |
|---|---|---|
| unshattered | 0.75 | 0.625 |
| rank 1 (unchanged) | 0.300 | 0.850 |
| rank 2 | 0.143 | 0.929 |
| rank 3 | 0.008 | 0.996 |

So the marginal point buys about **+9.3% then +7.3%** damage — and
**nothing at all against a target that does not block**.

### Verified by effect

Live on `/wiki/passives`, the rendered node description now reads:

> *Overwhelm's damage-reduction shred also applies to the target's block
> chance — at 100% of the shred at rank 1, **135% at rank 2, 165% at rank
> 3**.*

Real per-rank values where the old text promised scaling that did not
exist.

**Extraction note, since it cost two wrong attempts:** the node
description lives in a `data-tip` attribute that PRECEDES the node name in
the HTML, so a `grep` anchored forwards from "Shatter" returns the next
node's tooltip, not this one's. A first pattern matched the heading and
returned the bare word; a second, `grep -o ".\{0,420\}Shatter</div>"`,
backtracked catastrophically on a 378 KB page and had to be killed. Parsed
the attribute properly instead. A verification that returns *something*
plausible is worse than one that returns nothing.

### Patch notes — written BEFORE the swap this time

Correcting item 10's ordering slip. Prepended into the September 4 block
(2 sections), pre-edit copy at `/root/patch-notes.pre-shatter-ladder.json`.

Headed *"Shatter's 2nd and 3rd points were buying nothing. Now they buy
something"*. It says the description was wrong, that **rank 1 is
completely unchanged**, gives the honest marginal value (+9% then +7%),
states plainly that Shatter does nothing at any rank against a target
that does not block — always true, better said than discovered — and
explains why rank 3 is 165% rather than a round 200%.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **22 = 22** |
| 4 | live sha256 | `f7563ea8…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,349 B, 200 / 92,403 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines, patch note renders.

### 2026-09-05 — BOUND-PASSWORD-HASHING deploy record (release `bound-password-hashing`)

From window b, taken ahead of the queue: no corpus interaction, and
`/account/register` is reachable from the internet through the Cloudflare
tunnel with unbounded argon2 behind it.

| | |
|---|---|
| master commit deployed | `1f491e3` |
| binary before | `f7563ea8…4bbbc` |
| binary after | `5e56995b1c36c541ebb0883b92f160c8e132a788ac4c40600e26c2d7b6365832` |
| downtime | **0.92 s** |
| suite on the box | **865 passed / 0 failed, 42 suites** (860 + 5, new suite) |
| slot | `deploy-pre-20260904-211654-bound-password-hashing`, `LATEST` repointed |

### The size of the thing, in numbers

Unbounded concurrent argon2 is up to **512 blocking threads × 19 MiB ≈
9.5 GiB** on a 16 GiB box. Bounded at 4 permits it is **~76 MiB**. That is
not a tuning improvement; it is the difference between a request pattern
anyone on the internet can generate and an OOM.

**Both entry points, which is what makes it a fix rather than a
mitigation.** Registration hashes and login verifies cost the same and
land on the same blocking pool, so bounding one would have left the
cheaper-to-reach half open while looking solved. On saturation a request
is turned away **without hashing**, with a warn line naming which entry
point — rather than queueing behind the bound, which would convert a
memory problem into an unbounded queue.

### The floor is the dangerous edge, and it is guarded three ways

Verified against the tree rather than taken from the report, because this
is the one setting that can lock everybody out:

1. `apply_permit_limit` clamps into `MIN..=MAX`
2. the form validator range-checks against the same constants
3. a test asserts `PASSWORD_HASH_PERMITS_MIN == 1` **itself**, with a
   message naming the consequence

That third one is the good part. **A 0-permit semaphore is not "no limit"
— it locks every sign-in and registration out of the game with no error
that explains why.** Guarding only the *values* would let someone lower
the floor to 0 and ship it; asserting the floor means that fails the
suite.

Shipped at 4, MIN 1, MAX 64, live tunable.

### Verified by effect, and the limit of that verification

| check | result |
|---|---|
| `/account/login` renders | 200 |
| bad-password login | **401 in 0.0013 s** — refused, and responding |
| argon2 saturation warnings | **0** |
| `password_hash_permits` on the live page | **4** |

**What that probe does NOT prove, stated rather than glossed:** 1.3 ms is
too fast to have run argon2. The username did not exist, so the path
short-circuits before hashing — exactly as it did for the login-throttle
probe on 2026-09-03. So this confirms the login path still responds and
still refuses; it does **not** exercise the semaphore around a real hash.

Doing that on production would need either a real account's correct
password or registering a throwaway account, and the second writes to
`adventure-accounts.json`. Neither is worth doing to satisfy a checklist —
the semaphore's behaviour under contention is covered by the branch's own
tests, including one that asserts the N+1st hash waits for a permit to
come back.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **22 = 22** |
| 4 | live sha256 | `5e56995b…` = candidate |
| 5 | `/characters` | 200 / 81,350 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

No patch note: at 4 permits and roughly 100 ms per hash that is about 40
sign-ins per second of headroom, far beyond this game's load, so no player
will observe the bound. If that proves wrong the warn line names the entry
point that was turned away.
## 2026-09-04 — REFUND MIGRATION: retiring two dead passive nodes without stealing the points (branch `fix/retire-dead-passive-refund`)

Stage 1 of the retirement ordered in `C:\dust-work\orders\d.md`. **The refund
alone, on its own branch, before either replacement node exists**, per the
sequencing — it is correct regardless of what replaces either slot.

### The standing lesson this whole thread produced

Recorded here because the owner asked for it to be, and because it generalises
past the node that produced it:

> **Nothing in a "delete the mechanic" change naturally leads you to the sentence
> that sells it. The sweep caught it only because it started from the
> player-facing text and worked back toward the code, which is the opposite
> direction from how the change was made.**

`WIKI_IMPACT.md:202` records the 2026-08-20 golem-penalty removal deleting the
field, its `resolve_hit` application, its construction from rank, every
zero-initializer *and* a stale doc — a thorough change that still missed the one
place a player reads it. **This is the standing reason the advertised-vs-actual
sweep runs from the text side, and why it should be re-run from that side after
any mechanic is removed.**

### What shipped

`migrate_refund_retired_dead_nodes` — one row in `CHARACTER_MIGRATIONS`,
marker-guarded (`adventure-refund-retired-dead-nodes-marker.json`), honouring the
existing save-then-mark-done-per-migration contract. A refund is precisely the
migration that must never re-run.

**Refund, not remap** (owner ruling). `migrate_flowlikewater_swap` remaps and was
right to — that was the same mechanic moving between tiers. This is different
mechanics arriving, and a player who chose a defensive-uptime node and silently
received a party-support node would have been wronged in a new way by the fix for
the old one.

**Removing the entry IS the refund**, and that is the property worth remembering:
every "points spent" site derives the total by summing the allocation map
(`manager.rs`'s allocate-time guard plus three `adventure_web.rs` render sites all
do `passive_allocations.values().sum()`). There is no separate available-points
counter, so there is nothing that can fall out of sync — the two numbers cannot
disagree because there is only one number.

**Both trees.** Split Personality can run Monk or Paladin as a *secondary*, so an
affected allocation can live only in `secondary_passive_allocations`. A migration
touching one map would silently miss exactly those characters and — being
marker-guarded — never get a second chance at them. Its own test.

### The test I got wrong first, and what it taught

I wrote `the_retired_nodes_contribute_nothing_even_before_the_refund` asserting
`passive_node_magnitude("stillwater") == 0.0`. **It failed, and the code was
right.** `stillwater` DECLARES `Special { at_rank_1: 1.0, .. }`, so its magnitude
at rank 3 is 3.0 — not zero.

What makes it inert is not a property of the node at all: **it is that no call
site ever passes its key.** That is a property of the CONSUMERS. The test now
scans `combat.rs`, `character.rs`, `manager.rs` and `adventure_web.rs` for either
retired key — the same `include_str!` technique `character.rs`'s `guard_tests`
uses to pin the mutation-guard bypasses — and fails the moment a consumer
appears, which is exactly when the refund would stop being balance-neutral.

Worth stating as a general shape: **"this node does nothing" is never provable
from the node.** It is only provable from the absence of readers.

### FOUND — one thing that needs a ruling before this deploys

**The refund is one-shot, but the dead nodes still render in the tree.** Stage 1
ships the refund alone; the replacements come later under new keys. In the window
between them a player sees their refunded points and a Stillwater node that still
promises something, can spend them straight back into it, and — because the
marker means the migration never runs again — those points are stranded
permanently with no second refund.

Not fixed here: the order said the refund ships *alone*, and removing the node
definitions is a scope call that belongs to the owner, not to me mid-build. The
cheap closure is to drop both node definitions from the tree in the same release
as this migration, which makes re-allocation impossible by construction rather
than dependent on release timing. Raised in the report rather than acted on.

### Deliberately not in this commit

Shatter's approved `[1.0, 1.35, 1.65]` ladder (its own branch — it touches
nothing this migration touches), Shared Aegis, and Still Water.

## 2026-09-04 — THE DELETION SHIPS WITH THE REFUND (branch `fix/retire-dead-passive-refund`, second commit)

Ruling on the gap I raised and deliberately did not close mid-build. The two node
definitions come out of the tree **in the same release as the refund migration**.

### The rule, stated so it survives this branch

> **A refund that ships into a tree where the money can be re-lost is worse than
> no refund, because it looks like the problem was solved.** And it fails in the
> direction that hurts most: the player who trusts the refund and spends it is
> precisely the one who loses it.

The refund is marker-guarded and therefore one-shot. A surviving definition means
a player can spend the returned points straight back into a node that still does
nothing, with no second refund coming. Deleting the definitions makes that
**unrepresentable** rather than dependent on how long stage 1 sits before stage 2
lands — the same shape as the new-keys ruling and as `max(0, tier − level)`.

**The constraint now travels with the code, not with the order file.**
`every_retired_key_is_gone_from_every_tree` fails if a retired key is defined in
any archetype's tree, so the pairing cannot be separated by a future rebase,
cherry-pick or partial deploy. A commit message would only have described it.

### The standing rule from the failed test

Recorded here at the owner's instruction, because it generalises well past the
node that produced it:

> **"This node does nothing" is never provable from the node. It is only provable
> from the absence of readers.**

I had asserted `passive_node_magnitude("stillwater") == 0.0`. **The test failed
and the code was right** — the node declares
`Special { at_rank_1: 1.0, per_additional_rank: 1.0 }` and returns 3.0 at rank 3.
The sweep's conclusion held, but for a different reason than I had written down,
and I would never have found that had I asserted the conclusion instead of the
mechanism. Rebuilt as an `include_str!` scan across the four consumer files, it
fails the moment a reader appears — which is exactly when the refund would stop
being balance-neutral. **A claim about today became a guard on tomorrow.**

### The three things the order asked me to verify rather than assume

1. **Startup ordering — confirmed, no window exists.**
   `AdventureManager::new` is a *synchronous* fn and calls
   `run_character_migrations` at `manager.rs:2236`, well before it returns its
   `Arc<Self>`. `main.rs` then spawns the encounter loops and only afterwards
   awaits `start_adventure_web_server` (`main.rs:153`). So migrations complete
   before the web server binds a port *and* before any fight loop starts — there
   is no instant at which a definition is gone and an allocation has not been
   refunded.
2. **`passive_overrides.rs` — one entry, now removed.** `stillwater` was the sole
   member of `UNWIRED_NODES`, the list that tells `/admin/passives` "an override
   here would change nothing". `sacredoverflow` was never in that file at all —
   the list's own doc distinguishes unwired nodes from `NotYetImplemented` ones,
   which declare no value. The list is now empty, documented as a real state
   rather than an oversight, and kept because the classification is still right
   for the next node that lands in it. **Its own test
   (`every_unwired_key_still_exists_in_the_tree`) is what would have caught a
   dangling entry** — the guard worked.
3. **Two visible gaps — checked, and the layout does not care.**
   `compute_passive_layout` derives `mods_count` by filtering nodes at runtime and
   already branches on `mods_count > 0.0`, so a Specialization with fewer (or
   zero) Modifier children just reserves a narrower row. Nothing anywhere asserts
   a node count: `passive_nodes().len()` appears nowhere in the tree.

### FOUND

- `sacredoverflow` was the tree's last `PassiveEffect::NotYetImplemented`. With it
  gone **no node in the game uses that variant.** The variant itself is left in
  place — it is the honest declaration for a future node in that state, and
  `magnitude_at_rank` still handles it — but a reader should know it currently has
  no users.

### The full suite earned its keep again, on my own FOUND note

I recorded as a FOUND that `sacredoverflow` was the tree's last
`PassiveEffect::NotYetImplemented`. I did not follow the thought through to its
consequence, and the full workspace run did:
`admin_passives_tests::a_not_yet_implemented_node_is_shown_but_not_editable`
searched the whole tree for such a node and `.expect()`ed one. Deleting the last
one made it panic.

**Every narrow run I did was green** — the migration tests, the golden corpus,
the targeted module. Only `--workspace` saw it, because the test lives in a
different module from everything I touched. That is the second time in three
sessions the standing "full suite once before reporting" rule has caught a real
break that no narrow run could (the first was `guard_tests` on the gear-tier
fixtures).

**Fixed by making the test hold in both states rather than assume one.** The
rendering arm it guards (`not_yet` → *"No mechanic yet"*) is still live in
`render_passives_page`; there is simply no node in that state right now. So it
now checks the arm when such a node exists and asserts the absence explicitly
when none does — the test re-arms itself automatically the moment a node enters
that state again, and a reader learns the arm is dormant by fact rather than by
silence. Deleting the test would have thrown away a live guard because its
subject happened to be temporarily empty.

### 2026-09-05 — RETIRE-DEAD-PASSIVE-REFUND deploy record (release `retire-dead-passive-refund`)

Queue item 14, from window d. Both commits together, which is the gate.

| | |
|---|---|
| master commit deployed | `e7b005d` |
| binary before | `5e56995b…365832` |
| binary after | `5978bc7028bba76b59008ad9147761f3cba3b46b37fda28d1018170d799cf808` |
| downtime | **0.56 s** |
| suite on the box | **871 passed / 0 failed, 42 suites** (865 + 6) |
| slot | `deploy-pre-20260904-213300-retire-dead-passive-refund`, `LATEST` repointed |

### The gate, checked on the tree being merged

Stillwater and Sacred Overflow read 0.0 at every rank while still counting
against a character's point budget. The refund is one-shot and
marker-guarded, so shipped alone into a tree where the nodes still render,
a player sees refunded points, spends them straight back into a node that
still promises something, and the marker means **they are never refunded
again** — permanent and invisible, and worse than no refund because it
looks solved.

Verified on the rebased tree, not on the branch as reported and **not by
counting**: `grep "stillwater"` and `grep "sacredoverflow"` return **zero
non-comment lines** in `passive_tree.rs`, leaving two
`// RETIRED 2026-09-04 … stood here` tombstones. That distinction is the
Golem Master lesson — the same grep there returned `1` and meant "present
in a comment".

d made the constraint self-enforcing rather than documented:
`every_retired_key_is_gone_from_every_tree` fails if any archetype's tree
still defines a retired key, so the pairing cannot be split by a rebase,
cherry-pick, revert or partial deploy.

### Verified by effect — and it was a real falsification opportunity

An allocation baseline was captured **before** the swap so the migration's
effect could be measured rather than asserted:

| | before | after |
|---|---|---|
| characters | 22 | 22 |
| total points allocated | **97** | **97** |
| characters whose total moved | — | **0** |
| per-character digest | `6846ad277e04fff8` | `6846ad277e04fff8` |

Marker written (`true`), so the migration ran and is now guarded. Neither
retired node renders: **0** occurrences of "Stillwater" or "Sacred
Overflow" on `/wiki/passives`.

**This was the first live test of the census I ran for window d on
2026-09-04** — zero holders, zero points. Had any allocation total moved,
that census would have been wrong, and so would the sizing d worked from.
It came back identical to the byte. A migration that correctly does
nothing is only distinguishable from one that silently failed if you
measured beforehand.

d's design note is the elegant part: **removing the entry IS the refund.**
Every site that asks how many points a character has spent derives it by
summing the allocation map, so a removed entry returns its points
automatically and the two numbers cannot disagree — there is only one
number.

### A new marker, and a gap it exposed

The migration introduces
`adventure-refund-retired-dead-nodes-marker.json`, registered in the
table at `migrations.rs:561`. It was **not** listed in
`backup-game-data.sh`'s `MARKER_FILES` — the same completion the
starter-kit marker needed on 2026-09-03. Added, with the distinction
recorded in place: this marker is a **guard**, not a record. While it
exists the refund cannot fire again.

`.gitignore` already covers it through the `adventure-*-marker.json`
glob — confirmed with `git check-ignore` rather than inferred from the
pattern being present, which is the near-miss that caught me on
`bugreports.json`.

Confirmed by effect: a real `pathofdust-backup.service` run afterwards
reported **`verdict=clean`, 40 archives, 40 verified, and no
`MANIFEST DRIFT` line.**

### Two of my own checks that returned misleading numbers

**`refund marker path defined : 0`.** Reads as "the marker is missing". It
was not — I had grepped for `REFUND_RETIRED_DEAD_NODES`, a constant name
I guessed, and no such constant exists. The `0` meant *my guess was
wrong*, not *the thing is absent*. Third instance this session of a bare
count reading as a verdict: the Golem `1` that meant "in a comment", the
`penalty text gone : 1` whose label inverted its own number, and this.
**A bare count is only evidence if you already know what a correct answer
looks like.**

**The backup script installed from a stale tree.** I installed
`backup-game-data.sh` from `/root/deploy-src-retire-dead-passive-refund/`,
which was archived from `e7b005d` — *before* the marker line was committed
at `18d7ec6`. The verification grep returned `0` and caught it; reinstalled
by piping the current repo copy. The archive tree is a snapshot, and a
snapshot taken before an edit does not contain the edit — the same
stale-artifact class recorded three times already, this time in my own
deploy step.

### §13B.5, all seven

| # | check | result |
|---|---|---|
| 1 | `is-active` | `active` |
| 2 | `NRestarts` | `0` |
| 3 | loaded vs file | **22 = 22** |
| 4 | live sha256 | `5978bc70…` = candidate |
| 5 | `/characters`, `/passives` | 200 / 81,355 B, 200 / 92,405 B |
| 6 | anon `/admin/tunables` | **404** |
| 7 | anon `POST /api/commands/join` | **404** |

Tunnel 200, zero panics or ERROR lines.

No patch note: on this roster the refund moves nothing and the two removed
nodes were unreachable dead weight that read 0.0 at every rank. Nothing a
player can observe changed.

### 2026-09-05 — CORRECTION: merging `feature/admin-ui-rework` would NOT have reverted item 10

**This corrects the entry "journal: stage-gate-shard-flake deploy record,
and a revert avoided by cherry-picking" (`4e58cd4`, 2026-09-04) and the
claim repeated in reports `2026-09-04e` and `2026-09-05b`. The original
entries stay as written and wrong, per the append-only rule; this is the
dated correction.**

**What I claimed.** That merging `feature/admin-ui-rework` to collect a's
one-line follow-up would have reverted item 10's Golem Master fix —
restoring text telling Elementalists they deal 1% damage at three golems,
"with a green suite and no symptom", an hour after a patch note apologised
for it. I cited
`git diff 749c8df..origin/feature/admin-ui-rework` showing **−246 lines
including `passive_tree.rs −47`**.

**What is actually true.** Tested empirically today with a real
`git merge --no-commit` of that branch into current master:

| after a genuine merge attempt | present? |
|---|---|
| item 10's `your own damage is unaffected` | **yes** |
| item 13's Shatter `1.35` | **yes** |
| item 14's two `// RETIRED` tombstones | **yes** |
| the password-hash semaphore in `accounts.rs` | **yes** (4 refs) |

**Nothing was reverted.** The merge produces a **conflict in
`adventure_web.rs`** plus the two append-only docs, and stops for
resolution. That is real work and real risk if resolved carelessly — it is
not a silent revert.

**The mistake, named precisely: I read a two-dot `git diff A..B` as a
preview of a merge.** It is not. `A..B` shows the difference between two
snapshots, so every commit `A` has that `B` lacks appears as a deletion.
Any branch behind master shows large negative numbers; that is the normal
state of a branch, not a hazard. A merge is computed from the **merge
base** and combines both sides, so master's later commits survive. The
number I quoted was a true number answering a different question.

**Why this one is worth a full entry rather than a line.** The failure I
described was *silent* — green suite, no symptom, discovered later. That
is the most alarming shape a claim can have, and it is the shape I have
spent this week flagging in other people's work: plausible, specific,
confidently stated, and wrong. It went into two reports and a commit
message, and the owner ratified it, before I checked it. **A claim about a
destructive outcome deserves the same standard as a claim about a passing
test: run it, do not reason about it.** Reasoning is what produced the
error; one `git merge --no-commit` against a scratch branch refuted it in
seconds and could have been run at any point.

**What was still correct.** Cherry-picking a single-line commit was the
right choice — simpler, no conflict resolution, nothing to get wrong. The
general lesson about pointers ("a branch described by what someone added
to it rather than by what it now contains") also stands, and was what
prompted checking the branch at all. Only the specific mechanism was
false: the risk of merging a stale branch here is *a conflict resolved
badly*, not *a silent revert*.
### 2026-09-03 — ECHO COEFFICIENT: the tier-curve migration missed one affix

Branch `fix/echo-coefficient` off `origin/master` (`f306f64`). Not merged,
not deployed — merges after today's queue drains, in the same window as
the deliberate golden-corpus regeneration.

Owner ruling, and the owner supplied the root cause rather than waiting
for the investigation: Echo's `per_tier` was never re-derived when the
2026-09-02 affix tier curve migration landed. It was carried forward
unchanged and the shared `f(T)` crushed it. **Not drift — a gap in that
migration's scope.**

The symptom, measured before the change: an Echo affix was worth
**0.0125% at tier 1** and still only **0.243% at tier 1000**. The
smallest magnitude in the affix pool by an order of magnitude, and
indistinguishable from zero in play.

#### The change, and the arithmetic on record

`Echo.default_per_tier` `0.000125` → `0.01`, against the unchanged shared
curve:

| | value |
|---|---|
| `0.01 × f(1)` | **1.000000%** |
| `0.01 × f(100)` | **10.000000%** |
| `0.01 × f(1000)` | **19.453601%** |
| `f(1000) = 10 × 10^0.289` | 19.4536008162 |

A **pure coefficient change**: uniformly 80× at every tier, which is what
"no curve deviation" means arithmetically. Echo gets no exponent of its
own, stays purely multiplicative in `f(T)`, and so
`affix_tier_growth_ratio`, `affix_quality_percent` and
`Item::sync_tier_to` all keep working on it exactly as they do for the
other sixteen affixes. Nothing became per-affix. The additive floor
scoped in the previous fit report would have broken all three — the
ruling is simpler *and* safer, and that is worth recording as the reason
the design question was worth asking before building.

Source of truth: the in-code `AffixDef` default.
`adventure-item-balance.toml` is a sparse override file, carries no
`echo` key, and no copy exists in the repo — the live file is runtime
data on the box. **If the live file does carry an `echo` override it
would win over the code default; flagged for verification at deploy.**

#### FORWARD ONLY

No migration, no rescale, no code path touches a stored Echo value that
already exists. Only fresh rolls — drops, crafts, Krangle re-rolls — see
the corrected coefficient. Per the ruling no retroactive path was built,
not even as a flagged option.

#### The affected population — what could and could not be determined

The order asked for a count of live items carrying an Echo affix. **It
cannot be answered from anything in this repo, and the reason is worth
recording rather than papering over.** The only character data available
to a feature session is `tests/fixtures/characters_pseudonymized.json`,
captured 2026-08-18 — which **predates Echo entirely** (the
LingeringEffect → Echo rework was 2026-08-21). It contains 0 `echo`
affixes and 301 `lingeringEffect` ones.

The closest honest proxy, from that snapshot: **301 of 2873 items
(10.48%) across 46 of 52 characters** carried `lingeringEffect`, and
those are precisely the items `migrate_lingering_effect_to_echo`
converted into Echo at half value. Stored values ran 0.00022 to 0.191,
median 0.0354. So the population holding Echo is **large, not marginal** —
roughly one item in ten, and nearly every character — which is worth
knowing given the ruling that none of them are touched.

A true live count needs the game box and was not attempted; the order
also ruled out querying live data for design purposes.

#### Golden corpus — the prediction was wrong, in the safe direction

The order predicted "most or all of the 17 scenarios" would diverge.
**Measured: 4 of 17.**

`warrior_vs_lich_stage50`, `warlock_vs_dragon_stage500`,
`slayer_vs_tough_boss_stage3000`, `mage_passives_vs_cthulhu_stage1000`.

Nothing regenerated. The explanation, and it is the same fact that makes
the number small: a corpus scenario only moves if its seeded gear
actually rolled an Echo affix. Echo is 1 of 17 affixes at weight 1.0 and
each item draws 1–4, so most scenarios' gear carries none and is
bit-identical.

Worth contrasting with the abandoned `fix/echo-floor` branch, which
diverged **17 of 17**: a floor on `Character::combat_echo_pct` gave every
character a nonzero echo chance, inserting an `rng.gen_bool` draw into
every attack and heal in every fight. The coefficient change touches only
items that already rolled the affix. **The divergence count is itself
evidence of the blast radius**, and it says the ruled design is the
narrower one.

#### Carried forward, and now load-bearing

`echo_geared_character` strips the starter kit's random affixes and
asserts zero earned Echo before pushing its own. This was hygiene when
found; the coefficient change makes it necessary. At 80×, a stray tier-1
Echo roll is worth ~0.85–1.15% instead of 0.0125% — enough to push a
fixture built at 5.99 past the 6.0 rung and change `roll_echo`'s
GUARANTEED repeat count from 5 to 6.

**Generalises, and should be treated as a standing fact about this
codebase: any test asserting an exact `sum_affix` value on a
`Character::new` fixture is latently flaky, for every affix in the pool,
because the starter kit rolls random affixes and every affix is eligible
on every slot.** This is the third instance of the measurement-
contamination class this week.

#### Patch note draft — NOT written to the box

> **Echo was broken and is now fixed — on new items**
> - Echo's strength was never recalculated when item modifier scaling was
>   reworked on 2026-09-02, so it has been rolling about 80x weaker than
>   intended ever since. An Echo modifier was worth 0.0125% at tier 1 —
>   effectively nothing.
> - Fixed: Echo now rolls **1% at tier 1**, 10% at tier 100 and 19.45% at
>   tier 1000, before the usual roll variance.
> - **This applies to new rolls only.** Echo modifiers on gear you already
>   own are unchanged, and there is no retroactive fix — if you check an
>   old item you will see the old number. New drops, new crafts and
>   Krangle re-rolls get the corrected value.
> - No other modifier changed.
### 2026-09-04 — LEECH COEFFICIENT: the second affix the tier-curve migration missed

Branch `fix/leech-coefficient` off `origin/master`. Not merged, not
deployed — merges in the same window as `fix/echo-coefficient`, after the
deliberate golden-corpus regeneration.

Owner ruling, from window d's sweep putting every affix coefficient into
one table. Leech is Echo's nearest sibling and has **the same cause**:
the coefficient was never re-derived when the 2026-09-02 affix tier curve
migration landed, so the shared `f(T)` left it far below the pool. Two
affixes now, from one migration's scope gap — worth recording as a pair
rather than as two incidents, because the next question is whether a
third was missed, and d's table is the artifact that answers it.

The symptom: **0.10% at tier 1, 1.00% at tier 100, 1.95% at tier 1000.**
At tier 100 a Leech affix healed 1% of damage dealt while every competing
affix on the same item returned 10-30%.

#### The change, and the arithmetic on record

`Leech.default_per_tier` `0.001` to `0.01`, against the unchanged shared
curve:

| | new | old |
|---|---|---|
| `0.01 * f(1)` | **1.000000%** | 0.100000% |
| `0.01 * f(100)` | **10.000000%** | 1.000000% |
| `0.01 * f(1000)` | **19.453601%** | 1.945360% |

In line with CritChance. A **pure coefficient change**: uniformly 10x at
every tier from T1 to T5000, which is what "no curve deviation" means
arithmetically, and which is a test rather than a claim.

**`default_weight` is UNCHANGED at 0.1**, pinned by its own test. Rarity
was never the complaint, magnitude was, and "make Leech better" is an
easy instruction to over-apply — raising both would have been a different
change from the one ruled.

#### Forward only

No migration, no rescale, nothing touches a stored Leech value. Only
fresh rolls. Same ruling as Echo, and the patch-note draft says so.

#### Golden corpus — the predicted direction, confirmed

**1 of 17 diverged**: `mage_passives_vs_cthulhu_stage1000`. Nothing
regenerated.

Against Echo's 4 this is smaller, which is the direction the order asked
to have checked: Leech's 0.1 roll weight means far fewer seeded scenarios
carry one. A *larger* number would have meant something was wrong. The
observation from the Echo entry now has a second data point behind it:
**the divergence count is a measure of blast radius**, and it tracks roll
weight the way it should.

#### The fixture discipline, applied before it was needed

No existing test asserted an exact Leech value on a `Character::new`
fixture, so nothing was broken. The new tests establish the pattern
anyway, because at 10x a stray tier-1 Leech roll is worth ~0.85-1.15%
instead of ~0.085-0.115% — the same latent-flake shape Echo had.

#### Patch note draft — NOT written to the box

> **Life Leech was broken and is now fixed — on new items**
> - Life Leech's strength was never recalculated when item modifier
>   scaling was reworked on 2026-09-02, the same miss that affected Echo.
>   A Leech modifier healed 1% of your damage at tier 100 while other
>   modifiers on the same item gave 10-30%.
> - Fixed: Leech now rolls **1% at tier 1**, 10% at tier 100 and 19.45% at
>   tier 1000, before the usual roll variance — in line with Crit Chance.
> - **Still just as rare.** Leech is deliberately 10x rarer to roll than
>   any other modifier and that has not changed. It is meant to be a find.
> - **This applies to new rolls only.** Leech modifiers on gear you
>   already own are unchanged, and there is no retroactive fix — if you
>   check an old item you will see the old number. New drops, new crafts
>   and Krangle re-rolls get the corrected value.
## 2026-09-05 — ARCHETYPE CURVE: every class mirrors an item affix (branch `feature/archetype-affix-curve`)

Built after a report-before-implementing pass whose numbers killed three of the
twelve arms — which is what that instruction is for. All six questions came back
ruled; this is the build.

### What shipped

`Archetype::bonus_at(level, w)`. The advantage was
`per_unit × (1 + 0.10 × level)` — linear, uncapped, worth **5–8×** the matching
affix's own `per_tier`. It is now that affix's own curve at
`ARCHETYPE_AFFIX_MULTIPLE` (2.5) times its coefficient, level→tier 1:1, blended
on `archetype_bonus_curve_weight` and **shipped at w = 1**.

**`affix_tier_curve` is called, not reimplemented.** That is what makes "the same
equation as item affixes" true by construction rather than by a comment that can
rot, and a test divides the delivered advantage by its coefficient and asserts the
quotient *is* `affix_tier_curve(L)`.

### The three deliberate non-uniformities, each with its guard

- **Slayer held out entirely.** Its leech is `0.001` against the Leech affix's own
  `0.001` — **1.0×**, where every other class is 5–8×. The rule would have *buffed*
  it 2.5× now and 25× once the pending affix raise lands: the largest buff in the
  game, arriving inside a change advertised as a cut.
  `slayer_is_held_out_of_the_ruling_at_every_weight` is the guard against someone
  later "finishing the job".
- **Warlock anchored, not re-coefficiented.** There is **no attack-speed affix at
  all**; the nearest quantity is gloves slot base power, which **D11 ratified stays
  linear**. Its multiplier reaches 100 at tier 100 where `f` reaches 10, so treating
  it as an affix coefficient would import a 10× mismatch and cut Warlock ~77%
  against everyone else's ~25%.
- **Cleric and Paladin heal power flat at 100%**, composed differently — Cleric as
  `combat_heal_power`'s 0.5 base plus 0.5 here, Paladin (a Melee function, 0.0 base)
  carrying the whole 1.0 alone. Named separately and doc-warned against collapsing,
  the same reason `BOSS_CRIT_CHANCE_BASE` sits outside its ramp.

### A number I got wrong, and the fix that generalises

Warlock's anchor was first written as a transcribed decimal, `0.09979589711327115`.
Its own test failed: **off by 2.3e-7**, enough to miss the anchor it exists to hit.

Replaced with a function computing `0.15 × (1 + 0.10 × 19) / f(19)` from the old
expression it anchors to. **A hand-rounded literal is exact only until someone reads
it back** — the same lesson as writing the boss half-stages as `cap / slope` rather
than as decimals. Derive the constant from the thing it must equal, and the
equality cannot drift.

### Divine Power ships, and is documented as NOT compensation

Cleric's new scaling advantage, mirroring the divine damage affix at the same
multiple. The doc records the arithmetic so nobody re-derives it hopefully: the
affix is a **proc chance** for a **+1%-per-stack** buff over a 4s window, so
sustainable stacks ≈ `chance × actions_per_4s`. **Even at a 100% chance and four
actions per window that is four stacks, +4%** — against a ~49% healing-output cut
at level 19. No value of the multiple closes two orders of magnitude, because the
multiple scales a chance already near its useful ceiling. Compensation is a
separate future change to the Cleric tree.

It blends from `old_per_unit = 0.0`, so `w` is its on/off switch and the package
reverts coherently: at w = 0 Cleric has its old scaling heal power **and** no
Divine Power — exactly the pre-change class, with no double-strength state at any
setting.

### The dial ships at 1, which is unusual here

Every other dial this week shipped as a no-op. This one is the **walk-back, not the
rollout**, and that is only acceptable because `bonus_at` is computed on every read
and nothing is stored: w = 0 reverts every character instantly, with no migration
needed and none possible.

### Corpus: 14 of 17 moved, and the three that did not are the interesting part

**Nothing regenerated.** `slayer_vs_tough_boss_stage3000` did **not** move — a live
confirmation that the hold-out works, not just that the test passes.
`mage_vs_cthulhu_stage200` and `ranger_vs_lich_stage3000` also held; the likely
reason is that the changed stat never bound in those seeded fights (no crit rolled,
no second splash target alive), but I have **not verified that** and it is worth one
look before regeneration rather than assuming.

My earlier estimate of "up to 23" was wrong in its denominator — there are **17**
scenarios, not 23; I had counted `Scenario {` and `name: "` lines together.

### FOUND

- `Archetype::description()` keeps its signature and resolves the weight to the
  compiled constant rather than the live dial, because `adventure_web/wiki.rs`
  calls it and the wiki module is another session's workspace. That is the wiki's
  own **compiled-only rule** applied deliberately, not an oversight: at the shipped
  w = 1 the two agree exactly, and if an operator moves the dial the class picker
  will drift from live combat. Worth a ruling if the dial is ever moved.

### 2026-09-07 — HEALER-COMPENSATION deploy record (release `healer-compensation`)

| | |
|---|---|
| master commit | `1882fc433c14e1114b362bafcdc9d1a533801e7f` |
| live binary | `9025f32c7ef10ff6d7ce46138756461285e7559e27652e86473ef2f7f034c668` |
| previous binary | `5978bc7028bba76b59008ad9147761f3cba3b46b37fda28d1018170d799cf808` (item 14) |
| rollback slot | `deploy-pre-20260907-183217-healer-compensation` |
| downtime | **0.23 s** |
| suite | **900 passed / 0 failed / 42 suites**, `--no-fail-fast`, on the box |
| seven §13B.5 checks | all pass |

Contents: the corpus window (Echo, Leech, nine-slot scenarios, archetype affix
curve), plus the golem fixture fix, the Mage crit companion, and the Cleric /
Paladin healer compensation. The compensation shipped **with** the cut rather
than after it.

#### The window's stages, each checked against a named set derived BEFORE the run

A control came first: master at `4de6312` ran the corpus clean, so every
divergence attributed to a merge rather than to prior drift.

Echo diverged exactly its four named scenarios, and was attributed by
MAGNITUDE, not only by name: `warrior_vs_lich_stage50` moved a probability
`0.00037723369097563336 -> 0.030178695278050668`, a ratio of **exactly 80.0**,
which is Echo's `0.000125 -> 0.01` to the digit. Leech diverged exactly one, a
strict subset of Echo's four. The nine-slot branch diverged **0** and captured
5 — and that zero is what proved "added, not widened", because eleven existing
scenario names appear as added lines in its diff, which fits "moved" and
"modified" equally.

#### Two general rules earned here

**A reason that predicts an unseen case is a different class of evidence from
one that explains a seen case.** d's explanation for `ranger_vs_lich_stage3000`
holding predicted a hold regardless of gear slots; `ranger_nine_slot_vs_lich_stage3000`
then held too, in a fixture that did not exist when the reason was written.

**A scenario named for a class is only a regression net for that class if the
class's advantage actually fires in it.** `mage_nine_slot_vs_cthulhu_stage200`
DIVERGED while `mage_vs_cthulhu_stage200` HELD — same class, same boss, same
stage — which turned the Mage coverage gap from an inference into a
measurement. The companion fixture added to close it was verified non-hollow:
its captured baseline records 9 attacks with `isCrit` true exactly once.

#### CATCHING AN INSTRUMENT ERROR DOES NOT INOCULATE YOU AGAINST IT

Diagnosing the golem flake, a Windows-vs-Linux comparison of a regenerated
fixture returned **1,016 differing lines** — the identical number produced by a
CRLF artifact hours earlier the same day. Nearly filed as a cross-platform
simulation divergence, which would have been the serious finding. Re-measured
with `--strip-trailing-cr`: **0 real differences.** The corpus is
platform-stable.

The flake itself was neither platform-dependent nor a regression: 8 failures in
22 runs on the release tree against 9 of 9 passing on the pre-window control.
d fixed it by removing the entropy (`Character::new` rolling a starter kit from
an un-seeded `thread_rng`) rather than out-running it, and re-tuning `boss_atk`
`0.70 -> 0.45`. Verified **30 passes of 30** on the box where it had failed ~40%
of the time.

#### FOUND — the passive form has no scraped drift guard

`admin_tunables_splash_http.rs` is cited in CLAUDE.md as the fixed shape, and it
IS — but only for `TunablesForm`. `PassiveTunablesForm` is still tested with a
hand-maintained superset body. The compensation added three required fields with
no `#[serde(default)]`; the page renders them; the hand-maintained body did not
send them, so extraction 422'd and the suite went red at 899/1.

A superset body catches a field you FORGOT TO ADD — which is what fired — but
can never catch a field the page STOPPED RENDERING, which is the direction that
silently broke every real browser save on 2026-08-23 while the suite stayed
green. **`PassiveTunablesForm` currently has no protection in that direction.**

Fixed minimally (three entries) rather than converted, because the passive body
posts real baseline values and asserts they round-trip, so the drift guard's
filler values would change what the test verifies. That is a design decision,
reported rather than made.

#### Patch note

Written before this record. Compensation stated as shipped, not promised; the
curve change called a nerf in those words; Echo quantified; **Leech deliberately
not quantified** because gear, class and tree sum under one cap and the felt
effect is unmeasured; item 14's retirement included with an admission it should
have been noted on 2026-09-04. Verified in the SERVED page, not only on disk.

### 2026-09-08 — The passive form's dormant arm, and a guard that would have changed live state

Branch `fix/passive-form-drift-guard` off `origin/master` `15d6672`.

CLAUDE.md cites `admin_tunables_splash_http.rs` as the fixed shape for
form-body drift — *"derive its field set from the rendered page … never
from a hand-maintained list"* — and it does, **but only for
`TunablesForm`**. `PassiveTunablesForm` had been on a hand-maintained
superset body since the 2026-09-03 split.

**A superset body catches a field you forgot to ADD; it can never catch a
field the page STOPPED RENDERING.** The second direction is the one with
the incident behind it (2026-08-23: an `<input>` dropped while the field
stayed required, so every real browser save 422'd while the suite stayed
green). The healer compensation on 2026-09-07 exercised only the safe
direction — three new required fields the hand list did not send, caught
at once — which is precisely why the hole was still there to find. **A
guard that has only ever fired the safe way has not been shown to work.**

#### Two responsibilities, two tests

The existing body posts REAL baseline values and asserts they round-trip.
Converting it to a filler-value scrape would have quietly deleted that
assertion — the test would have kept its name and stopped doing its job.
So the scrape is a SECOND test asserting only that the field set
extracts, which is a different question and wants a different body.

The guard posts back **what the page itself rendered** rather than a
filler constant: every value is in-range by construction, so no
out-of-range 400 can mask the 422 it is actually looking for.

#### The checkbox, which is the finding worth keeping

**A checkbox renders `value="1"` whether or not it is ticked.** `checked`
is what says it is on, and a browser posts it ONLY when ticked.

My first draft echoed every rendered value unconditionally. That would
have posted `shattering_enabled=1` for an unticked box and **silently
turned Shattering on** — a state-changing save wearing the costume of a
no-op, in a test whose whole point is to prove a save is safe.

It surfaced through a set-equality assertion added for an unrelated
reason (the rendered set and the round-trip body must describe the same
required fields). It fired on `shattering_enabled` being rendered but
absent from the body — which is *correct*, since absent means false for a
checkbox — and chasing that down is what exposed the echo bug. The scrape
now parses per `<input>` tag, so `type` and `checked` (which sit before
`name` in the markup) are visible, and an assertion pins that the echo
did not move the checkbox in either direction.

**Generalises past this test:** any "post the page back to itself" check
has to model what a browser actually posts, and for a checkbox that is
presence, not value.

#### The mutation took three attempts to become honest

1. **Renamed an input** → caught, but by an *existing* hardcoded
   five-field assertion, not by the new guard. A mutation caught by
   something other than the guard under test proves nothing about the
   guard.
2. **Deleted the `<input>` line alone** → **compile error**: the `format!`
   named argument goes unused. For this rendering style the compiler
   already catches a bare input deletion.
3. **Deleted the input AND its `format!` argument** — which compiles, and
   is the real shape of the 2026-08-23 incident → the guard fires.

**Attempt 2 is the durable part: a dropped input is only silent if its
format argument goes with it.** That narrows the window this class of
defect can even occur in, and it is worth knowing before someone assumes
every dropped field is invisible.

#### Other forms — derived rather than eyeballed

Counted required fields (no `#[serde(default)]`, not `Option`) across all
21 `Form<T>` structs in the workspace:

| form | required fields |
|---|---|
| `PassiveTunablesForm` | **26** — guarded as of today |
| `TunablesForm` | **19** — guarded 2026-08-23 |
| `PassiveOverrideForm` | 5 |
| every other form | ≤ 2 |

**Only the two wide tunables forms carried real exposure.**
`PassiveOverrideForm` is a FIXED shape — class, node key, three ranks —
not a growing list of dials, so a dropped field there is immediately
visible rather than silent. It would want the same treatment only if the
rank count ever grew, which is worth remembering rather than acting on.

The shape of the risk is worth stating as a rule: **the danger scales
with how many required fields a form has AND whether that number grows
over time.** A wide form that gains a field every few weeks is where this
defect lives; a narrow fixed one is not.

Suite: **855 passed, 0 failed**, zero FAILED lines across all 42 result
lines of a fully captured 309-line run. Golden corpus matched — nothing
regenerated, tree clean.

Test-only change, no player-facing behaviour, so no WIKI_IMPACT line.

### 2026-09-08 — ADMIN-LAYOUT deploy record (release `admin-layout`)

| | |
|---|---|
| master commit | `d9183a9aa29e3918cbc393bf5f65f7f02ed109b2` |
| live binary | `e2cfdece22c4ba5576bda30701e7dd95afafd9831d872c4603fde3a1351a0024` |
| previous | `9025f32c7ef10ff6d7ce46138756461285e7559e27652e86473ef2f7f034c668` |
| rollback slot | `deploy-pre-20260908-082358-admin-layout` |
| downtime | **0.24 s** |
| suite | **901 passed / 0 failed / 43 suites**, `--no-fail-fast`, on the box |
| seven §13B.5 checks | all pass |

Three merges in one release: the cleric grace companion (last of the corpus
work), b's passive-form drift guard, and item 15's admin page rewrite. Grouped
because the first two change no runtime behaviour at all, so deploying them
alone would spend a release and a patch note on a binary that behaves
identically.

#### THE ORDER OF THE MERGES WAS THE POINT

The drift guard was merged BEFORE the page rewrite, which makes the rewrite the
guard's first real exercise. If the rewrite had renamed or dropped a rendered
input, the guard's scraped body would fail extraction instead of shipping green
— which is exactly the 422 that went red in release 16, and exactly the
direction a hand-maintained superset body can never catch.

#### VERIFIED ON THE LIVE REWRITTEN PAGE, NOT IN SOURCE

A page rewrite is where a rendered field goes missing without failing a build,
and this rewrite landed on top of a branch that had just ADDED fields to that
page. Fetched both admin pages as the operator and counted what actually
rendered: **smite dials 3, archetype curve dial 1, boss dials 13, splash dials 6,
38 distinct inputs on the passives page.** The compensation's dials survived.

#### AN UNCHANGED SUITE TOTAL NEEDS THE SAME SCRUTINY AS A MOVED ONE

Items 1+2 left the total at exactly 900. Confirmed that was correct rather than
assumed: `#[test]` attribute counts are 900 at both commits, **0 new `#[test]`**,
and 7 assertion lines added to the drift-guard file — b's guard is a second block
inside an existing test function. Item 3 then moved it 900 → 901 / 42 → 43
suites, attributable to exactly one new test FILE (a's R3 test) carrying exactly
one `#[test]`.

#### CATCHING AN INSTRUMENT ERROR STILL DOES NOT INOCULATE YOU

A box-side tree-identity grep reported `smite inputs rendered : 0` where the
local one said `3`, on an archive whose hash matched at both ends. That reads as
the rewrite having dropped the compensation's brand-new dials — the single most
likely real defect in this release. **It was my own nested ssh quoting mangling
the pattern.** Re-run with sane quoting: 3.

Sixth bare count this week to read as a defect, and the second where the
instrument was mine. The habit that keeps catching it: when a count disagrees
with something already established, check the instrument before believing the
number.

#### FOUND — an order premise that is wrong, recorded so it does not persist

The order states that `feature/store-classification`'s no-op guard "is already
merged into master via `939ec8f`". `939ec8f` is *"Merge remote-tracking branch
'origin/master' INTO feature/store-classification"* — the opposite direction. It
is not an ancestor of master, and master contains no store-classification code.
Nothing is blocked (that branch is not ready), but left standing the belief
would have a later session skip merging a's work as already done.

#### The sprite-manifest hazard, cleared ahead of its turn

Re-ran the live check against `bef24bd` rather than reusing the `861c1a4`
result. Box `custom/` and manifest agree 14/14 in both directions. Better than
the file comparison: the two sprites ACTUALLY EQUIPPED on live — `custom/kibukah`
and `custom/Sitch89`, found in the character `model` field — are both owned by
their users under the new manifest. No live player loses a sprite.

Correction to the order's account, not affecting the ruling: there is no kmart
login on the World 2 roster at all (22 accounts, 22 characters, zero). The
`kmartbikes1` story is World 1; World 2 reset on 2026-09-02.
### 2026-09-08 — `maxHp` on the summary tier, and the one field the golem rollup must not touch

Branch `feat/summary-tier-max-hp` off `origin/master` `15d6672`. Field first, by
owner ruling, because it changes no behaviour and every day it trails the Slayer
coefficient is a day of measurement not collected.

#### Why the measurement could not be made

The leech-saturation question needs damage-per-second against the player's own HP
pool, because the cap is `LIFE_LEECH_CAP_PER_SEC` **of the leecher's own max hp**.
The coarse tier carries `maxHp` and retains `COARSE_FIGHTS_CAPACITY` = **5**
fights — of which exactly one was a winning boss fight, so session c's run of my
query produced n=1: twenty characters in a single stage-59 fight, median 0.19,
max 9.31, seven of the twenty dealing literally zero damage to the boss. Not an
answer, and c was right to refuse to read one off it.

The summary tier retains **200** fights, ~33 of them winning boss fights, and
already carried `damageDealt`, `realDurationMs`, `stage`, `won` and per-player
`archetype`. **It lacked exactly one field.** No new tier, no new file, no raised
capacity — five coarse files are already 5.5 MB, one of them 2.3 MB, so raising
`COARSE_FIGHTS_CAPACITY` would have bought the same thing for orders of magnitude
more disk.

#### Recorded, not recomputed

`full_player_fight_stats` already receives `&[CombatUnitInfo]` and seeds its map
from it, so `max_hp: u.max_hp` is the whole change — the same value the coarse
tier writes, from the sim itself. Recomputing it from character state would have
been a second implementation of "what was their HP pool", which is the exact
class of thing this week has been spent removing.

#### THE FINDING: `max_hp` is a pool, not a tally

The golem-attribution merge pass folds every golem's row into its owner's and
drops it — `damage_dealt`, `damage_taken`, `healing_done`, `hits`, `crits`,
`evaded`, `dot_ticks`, `dot_damage`. All eight are **tallies**: a golem's
contribution genuinely is its owner's contribution, so summing is right.

`max_hp` is not a tally. It is the owner's own HP pool, and it is the
**denominator of the ratio the field was added for**. A golem's HP raises nobody's
leech ceiling. Summing it would inflate the denominator and make an Elementalist
running three golems read as unsaturated when they are not.

**The failure would have been silent.** The number stays entirely plausible —
1330 is a believable HP pool — and only the conclusion drawn from it is wrong.
Nothing in the fight record would look off; the measurement would just quietly
answer the wrong question for exactly the builds most likely to be near the cap.

So a field-by-field pass now has one field conspicuously absent, which is an
invitation to "complete" it. The comment saying why is not decoration, and the
test is what makes the comment enforceable: the helpers are `player` = 1000 and
`golem` = 330, so a summed implementation reads 1330 (or 1660 with two golems)
and there is no value both answers share.

Mutation-checked by adding `owner.max_hp += golem_stats.max_hp;` to the pass:
**25 passed, 1 failed**, and the one was
`a_golems_max_hp_is_never_folded_into_its_owners_pool`. Nothing else in the module
noticed, which is the point — no existing test covered this, and none would have.

#### The denominator is `real_duration_ms`

Owner ruling, recorded in the field's own doc rather than left in an order: the
simulated fight length, not `display_duration_ms`, which is stretched or
compressed for the overlay (`MIN_DISPLAY_MS` and the display window above it). A
rate computed against the display figure is on a made-up clock. Putting it at the
point of contact means the next session forming the ratio reads it where they are
already looking.

#### `0` means unrecorded, never "a player with no HP"

`#[serde(default)]`, so the ~200 summaries already on disk deserialize rather than
failing — but they read back `0`, and a ratio must **skip** those rows, not divide
by them. Stated in the doc and pinned by a test that deserializes a pre-field
record.

#### FOUND

Wire safety was checked, not assumed. `replay_bundle/writer-output.v1.json` is
byte-pinned and embeds a whole `FightSummarySnapshot` — but its `"players"` is
`[]`, an empty array, so no per-player field can move it. Checking the one fixture
that pins bytes before adding a serialised field is the check that gets skipped.

Correction to my own fit report: I wrote that every existing `PlayerFightStats`
construction uses `..Default::default()`. Three of four do. The `player_stats`
helper in `fight_summary_tests` lists every field explicitly and failed to compile
until `max_hp: 0` was added. The claim was checked by grepping for the type name
and reading the call sites, and one of them was read too quickly.

**`maxHp` accumulates only from the deploy forward.** On day one the tier holds no
record carrying it; ~33 winning boss fights is the steady state. The better leech
answer arrives some days after this ships, not with it.

### 2026-09-08 — SUMMARY-MAXHP deploy record (release `summary-maxhp`)

| | |
|---|---|
| master commit | `78e18732430db165e61687ddc1c7eb2128fc266c` |
| live binary | `4a245c0255602eea1cf9133cdf862b4b0f07599fdbc8a1496a531c175179ea31` |
| previous | `e2cfdece22c4ba5576bda30701e7dd95afafd9831d872c4603fde3a1351a0024` |
| rollback slot | `deploy-pre-20260908-101901-summary-maxhp` |
| downtime | **0.35 s** |
| suite | **903 passed / 0 failed / 43 result-lines**, `--no-fail-fast`, on the box |
| seven §13B.5 checks | all pass |

Shipped alone rather than grouped with the all-items button, because its whole
value is starting to accumulate immediately: the field only appears on records
written from the deploy forward, so every hour of delay is an hour of data that
does not exist later.

#### VERIFIED BY EFFECT, WITH THE BASELINE CAPTURED FIRST

| | before | after |
|---|---|---|
| summary files | 200 | 201 |
| carrying `maxHp` | **0** | new record does |

`fight-0000003565.json`, the first summary written after the deploy, carries
`maxHp` on every player record: `{"id": "xayse", ..., "maxHp": 195,
"damageDealt": 461, ...}`. Counting the zero BEFORE is what makes that a
measurement rather than an observation — the same shape as item 14's
do-nothing migration.

#### TWO CLAIMS, KEPT SEPARATE

**The field writes** — proven by the new record above.

**The 200 existing files still parse** — a different property, and the one that
breaks silently. Adding a serialised field to a record type with 200 files
already on disk is exactly where a deploy dies quietly. b's test names that
property directly, `max_hp_round_trips_through_a_summary_and_older_records_read_as_zero`,
rather than only round-tripping the new field. Same shape as release 16's
`#[serde(default)]` trap, arriving on the storage side instead of the form side.

#### b's FINDING, WORTH KEEPING: `max_hp` is a pool, not a tally

The per-owner rollup sums damage, healing, hits and crits — quantities that add.
`max_hp` sits in the same struct and is the owner's own HP pool, so folding a
golem's into its owner's would inflate it by the number of summoned golems.
Guarded by `a_golems_max_hp_is_never_folded_into_its_owners_pool`, which b
mutation-checked by actually adding the bad line and confirming the test fails.
A guard that has never been seen to fail is a guess.

Suite delta attributable to exactly those two tests: 901 -> 903, 2 new
`#[test]`, no new suite.

#### The result-line count is now printed on every run, not only failing ones

The standing instruction makes `--no-fail-fast` mandatory where a test
legitimately fails, with the result-line count proving the suite ran. Adopted it
in the harness generally: 828/6 and 903/43 look identical in shape, and only the
line count separates a complete run from one that stopped at the first failing
binary.

### 2026-09-06 — The all-items Hideout Warrior button, and three guards that failed their own mutation checks

Branch `feature/hideout-warrior-all-items` off `origin/master` `4de6312`.
Not merged, not deployed.

Divinity's operation bought with dust instead of a Unique Shard: same
bag-only set, same chain, same skips, one application path branching only
at the charge. `apply_divinity` now accumulates `report.dust_cost` on
every run; Divinity discards that field and spends a shard, the new button
charges it and spends none. **That single discarded field is the entire
difference between the two buttons**, which is what stops them becoming
two implementations that merely resemble each other.

#### The fact that made this a stop-and-ask rather than a build

The order described the new button as hitting "all items". The two
existing sets are NOT the same set, and the difference is a ruling:

* the single-item button targets `all_items` — **equipped + bag**;
* Divinity targets `self.inventory` — **bag only**, because the chain ends
  in Krangle and Krangle is irreversible.

A dust-priced all-items button following the single button's set would
have bulk-Krangled everything a player was WEARING for one click. That was
not a detail to resolve while building; it was the whole shape of the
feature, and it was the owner's to decide. Ruled bag-only, and the reason
is now recorded on `apply_hideout_warrior_all` — **an asymmetry with a
stated reason is a decision; without one it reads as a bug and somebody
eventually "fixes" it.**

#### Price: quoted and charged from one function, and the honest gap between them

`craft_dust_cost` is new and is now the single source for what one craft
action costs. `craft_item_ex`'s inline price expression was REPLACED by a
call to it, so the bulk quote and every single-item charge read the same
code rather than agreeing by coincidence.

The quote and the charge are still two numbers, and the distinction is
worth keeping straight:

* the **quote** (`hideout_warrior_quote`) prices every step as though it
  will land, walking the tier forward with `craft_tier_bump` as a real run
  does;
* the **charge** (`report.dust_cost`) counts only steps that actually
  landed, at the tier each ran at.

So `charge <= quote`, always. That asymmetry is deliberate in both
directions: charging the quote would bill for steps that did not happen,
breaking "it costs what the single button would have cost"; and making the
quote predict which steps match would be a second implementation of the
chain's eligibility rules, free to drift from the real one. **The property
that matters is that a player is never charged more than the number on the
button**, and that is the one guaranteed.

#### THREE GUARDS FAILED THEIR OWN MUTATION CHECKS TODAY, AND THE PATTERN IS THE POINT

Worth recording together, because they are the same mistake wearing three
faces.

1. **The semaphore test** (2026-09-05) asserted against a `Semaphore` the
   test constructed itself. It proved tokio counts correctly and would have
   passed unchanged if the bound were never applied.
2. **This feature's set guard**, first draft, inspected `plan_divinity`'s
   output. Mutating `apply_hideout_warrior_all` to append equipped items to
   its own copy of the target list — the exact defect this feature has
   already had once — sailed straight past it. It was testing the shared
   planner, not the entry point. The price test caught the mutation
   incidentally by item count, **and a guard that relies on another test
   noticing is not a guard.** Rewritten to snapshot every equipped item's
   tier and locked state, run the REAL entry point, and assert both
   unchanged per slot; it now fails with *"Weapon's EQUIPPED item was
   modified by the all-items button"*.
3. **`guard_tests::every_unguarded_item_accessor_is_a_named_exemption`**
   then caught ME. My new test fixture seeded equipped gear through
   `equipped_mut(`, a bypass accessor `manager.rs` is not on the allowlist
   for. Fixed by using `Character::equip` — which turns out to be a plain
   per-slot overwrite with no displacement — rather than widening a
   deliberate guard over production code to accommodate a test fixture.

**The durable form: a guard is worth exactly what its mutation check
proves, and the check has to run the real entry point rather than the
primitive underneath it.** Two of these were mine and one was somebody
else's catching mine, which is the argument for mutation-checking every
guard rather than the ones that feel risky.

#### CORRECTION to `feature/corpus-nine-slot-scenarios`

That branch's `run_scenario` comment says it assigns through
`equipped_mut` rather than `equip` because *"`equip` has swap-out
behaviour"*. **It does not** — `equip` is a nine-arm match that overwrites
the slot, with no displacement into the bag. The code on that branch is
still correct (the draw sequence is identical either way) but the stated
reason is wrong. Flagged here rather than edited from this branch; it
wants fixing on its own.

#### A third variant of the evidence-truncation trap

Reported this branch's suite as having "zero FAILED lines" off a command
that ended in `head -8`. **The head truncated the stream at eight lines, so
a later FAILED would have been cut off before I saw it** — the output could
not have established what I said it did.

That is the third variant this week of the same underlying error, reading a
SUMMARY of the evidence instead of the evidence:

* a zero exit code that belonged to `grep`, not cargo;
* a nonzero exit code that also belonged to `grep`;
* and a filtered stream where the absence of a failure was an artifact of
  `head`.

**The fix is the same each time: capture the full output to a file, then
interrogate the file.** Re-run captured 309 lines and 42 result lines, with
`grep -c FAILED` over the whole file returning 0 — which is a claim the
evidence actually supports.

Suite: **830 passed, 0 failed** in the lib crate, zero FAILED lines across
all 42 result lines. Golden corpus ran inside it and matched — **0
scenarios diverged, nothing regenerated**, and no fixture file was written
(17 tracked, tree clean).

### 2026-09-06 — Craft prices become stated relationships, and the class closes

Branch `feature/craft-price-rules`, **stacked on
`feature/hideout-warrior-all-items`**. The stacking is forced rather than
chosen: the all-items button only exists on that branch, and the ruling
was that all three Hideout Warrior actions land in the price table in this
pass rather than as a follow-up. **c must merge the Hideout Warrior branch
first.**

#### What was actually wrong, and it was not carelessness

The 2026-09-02 cost cut multiplied every ordinary action's flat fee by
`craft_base_cost_mult` and gave the per-tier surcharge an exponent. Five
prices did not follow: panel Reforge's `30 * tier`, Recombine's veiled
`500 + 500/modifier`, the dashboard Reforge Now's flat `1000`, Polishing's
sand cost, and the Divine Dust apply cost.

Four of those were **spotted at the time**, written into WIKI_IMPACT with
their numbers and the words *"flagged for a follow-up ruling rather than
silently shipped"* — and then that follow-up sat unscheduled for four
days. The fifth (Reforge Now) was not spotted at all.

**That is a different failure from Echo's, and it wants a different fix.**
Echo was a value nobody noticed. This was a ruling nobody scheduled. The
record did its job; what failed was the follow-up. And the reason a
follow-up was needed AT ALL is the structural part: **a price declared in
one place and charged in another cannot propagate, so somebody has to
remember it.**

#### The fix: the relationship is the thing written down

`PriceRule` states how a price is derived - `Standard { base }`,
`MultipleOfStandard { times, base }`, `PerCountedUnit { flat, times, base }`,
`ChainSummedOverSet`, `Exception { currency, reason }`, `TokenOnly`. A
future move of `craft_base_cost_mult` now carries every dependent price
with it, and the only prices left behind are the ones that said out loud
that they wanted to be.

`price` is a **required field** on `CraftActionDef`, so a new
`CraftAction` cannot be added without stating a rule — that half is
enforced by the compiler, not a test, which is the stronger form.
`COMPOSITE_PRICES` covers the operations that are not a single action and
so get no help from it at all: there is no enum for a `match` to be
exhaustive over, which is exactly why they need a named list and a test
that walks it.

#### The agreement test is what closes the class

> what an action DECLARES and what `craft_item_ex` CHARGES must be the
> same number, at every tier.

The compiler cannot give this. A future action can declare
`Standard { base: 500 }`, hardcode its own number at the charge site, and
everything compiles and every other test passes. **That is precisely how
all five of these drifted: the declaration and the charge lived apart and
only one of them moved.**

Mutation-checked by changing Chancing's declared base from 800 to 900
while leaving its charge alone → *"Chancing DECLARES 93 dust at tier 1 but
is CHARGED 83. The declaration and the charge have drifted apart."*

#### The prices, all moving down at live tiers

| tier | panel Reforge | Reforge Now | Recombine veiled, 4 mods |
|---|---|---|---|
| 3 | 90 → 85 | 1000 → **34** | 2500 → **1094** |
| 20 | 600 → 435 | 1000 → 174 | 2500 → 1374 |
| 35 | 1050 → 780 | 1000 → 312 | 2500 → 1650 |

Reforge Now is priced off the **highest eligible equipped tier**. It picks
its slot at random inside `reforge_equipped_item`, so no per-item tier is
knowable before the roll; the highest is the one choice that is
deterministic, quotable before the press, and cannot charge less than the
item it lands on is worth. Flagged as a judgement call rather than a
derivation.

Polishing and the Divine Dust apply are unchanged and are now declared
exceptions with written reasons. `dust_at` returns `None` for them, so
there is no dust number a caller can accidentally use — the exception is
enforced by the type rather than by everyone remembering.

#### A test of mine was wrong, and the wrong version is the instructive one

The recombine test first asserted that the price-to-Scour ratio stayed
within half of its low-tier value. It **failed** at 4.9x against 30.4x.

The rule was right and the test was not. That ratio *cannot* hold flat:
both prices carry a flat term, and this rule's flat term is four Krangles'
worth, so it washes out with tier on both sides at different rates.
"The ratio must not move" is the intuitive assertion here and it is the
wrong one.

The property actually worth pinning is that the price never decays toward
parity — which is what the old flat price did, at 69x a Scour at tier 3
and **0.42x** at tier 1000. A price that starts as the most expensive
thing in the game and ends up cheaper than the cheapest action in it is
not badly tuned, it is pointing the wrong way. Rewritten to assert >= 3x
at every tier, and the reasoning left in the test so the next person does
not re-derive the wrong assertion.

Suite: **837 passed, 0 failed** in the lib crate, zero FAILED lines across
all 42 result lines of a fully captured run. Golden corpus matched — 0
scenarios diverged, nothing regenerated, 17 fixtures, tree clean.

#### Patch note draft — NOT written to the box

> **Crafting prices: three big drops**
> - When crafting costs were cut on 2026-09-02, three prices were missed
>   and stayed where they were. They have now been brought in line, and
>   **every one of them goes down.**
> - **Reforge Now** (the dashboard button, random slot) was a flat 1,000
>   dust no matter what your gear was worth. It now scales with your gear
>   — at current tiers that is **1,000 → 34–312 dust**, the single biggest
>   drop.
> - **Veiled Recombine** was a flat 500 + 500 per modifier. It now scales
>   too — a 4-modifier veiled recombine goes **2,500 → about 1,100–1,650**
>   at current tiers.
> - **Reforge** in the crafting panel was 30 dust per tier. It is now
>   about **6–28% cheaper** across the tiers people are actually at.
> - Polishing and applying Divine Dust are **unchanged** — they are paid
>   in sand and Divine Dust, which have their own economies.
> - Nothing else about crafting changed: same odds, same outcomes, same
>   modifiers. Only what it costs.

### 2026-09-08 — CRAFT-PRICE-RULES deploy record (release `craft-price-rules`)

| | |
|---|---|
| master commit | `6a1bd44112774318987e8b12afd6f256ce8af2a4` |
| live binary | `1139f9224d16740fb8320a599fe1b8166ab73bd0e254b5938bc1be8ba0f42538` |
| previous | `a2e5b5869e4823a92d048d87677c1a863842020583ae372db3f40726bf7a8ddc` |
| rollback slot | `deploy-pre-20260908-162037-craft-price-rules` |
| downtime | **0.41 s** |
| suite | **914 passed / 0 failed / 43 result-lines**, `--no-fail-fast`, on the box |
| seven §13B.5 checks | all pass |

Three crafting prices drop, every one of them down, and every price is now a
stated relationship rather than a standalone literal. Suite delta 907 -> 914,
attributable to exactly seven named price-rule tests.

**The load-bearing one is `the_declared_price_and_the_charged_price_agree`.** A
price rule that declares one thing while the charge site does another is worse
than no rule at all — it reads as documentation and behaves as fiction.

#### Check 3 moved and the equality absorbed it

The roster went 22 -> 23 between releases; check 3 read `loaded 23 characters`
against 23 in the file and passed. A literal expectation would have raised a
false alarm on a healthy deploy for the third time in this project's history.
That is the whole reason the row is an equality.

#### A TOOLTIP DEFECT THAT ONLY THE LIVE PAGE COULD FIND

Release 19 shipped `HIDEOUT_WARRIOR_ALL_TIP` with nine bare `{2014}`, `{2192}`
and `{1F512}` sequences where Rust needs `\u{...}`. Without the `\u` they are not
escapes, so players read `crafted {2014} if you cannot afford it` and
`ticked {1F512} Keep`.

**It compiled cleanly, no test asserts on tooltip prose, and the suite was green
at 907/0/43 with the defect present.** It was found by fetching `/inventory` as
the operator after that deploy and reading the rendered button context.

Scope was checked rather than assumed: 87 `{NNNN}` sequences in the file, 78
already escaped, and the 9 unescaped all on that one line. The adjacent
`DIVINITY_TIP` — the same tooltip for the shard-paid twin — uses `\u{2014}`
correctly, which is what makes the intended form unambiguous rather than a
judgement call. Fixed in `654919f`, shipped with this release, and verified on
the live page after: **0 literal sequences, em-dash renders.**

Two smaller instrument notes from the same stretch. `sed -i` silently did nothing
twice — no error, no change — and was only caught by re-counting afterwards
instead of trusting the command's success. And two attempts to extract new test
names from a diff returned empty, which reads as "no new tests" exactly as it
reads as "bad pattern"; switching instrument entirely — extracting the test-name
set at both commits and taking the difference — answered it immediately and does
not depend on diff formatting at all.

#### b's own wrong test, kept because the wrong version is instructive

b's recombine test first asserted the price-to-Scour ratio stays within half its
low-tier value. It failed, 4.9x against 30.4x. The rule was right and the
assertion was wrong: both prices carry flat terms that wash out with tier at
different rates, so that ratio cannot hold flat. The property actually worth
pinning is that the price never decays toward parity — the old flat price went
from 69x a Scour at tier 3 to **0.42x** at tier 1000, which is not mistuned, it
is pointing the wrong way. Rewritten to assert >= 3x at every tier with the
reasoning left in the test.

#### Adopted from a: the explicit-add completeness check

After `git add <paths>`, `git status --short` must show no remaining ` M`. The
house rule against `-a` means an explicit-path commit is only ever as complete as
its list, and nothing warns you when the list is short — which is how `2bac806`
was pushed missing two files, compiling for its author and passing `cargo test`
because the affected sites are `#[cfg(not(test))]`-gated. Checked on this
release's push: 0 remaining.

### 2026-09-08 — REFORGE-FLAT-1000 deploy record (release 21, hotfix + refund)

| | |
|---|---|
| master commit | `4902663b746dab8b96336e3df83966a6d07e5167` |
| live binary | `710a6b4f7ba6d74eec21fa07e0d6fe88403db3bf373aa200b4fc5f32194377ab` |
| previous | `1139f9224d16740fb8320a599fe1b8166ab73bd0e254b5938bc1be8ba0f42538` |
| rollback slot | `deploy-pre-20260908-215429-reforge-flat-1000` |
| downtime | **0.50 s** |
| suite | **919 passed / 0 failed / 43 result-lines** on the box |
| seven §13B.5 checks | all pass |

Release 20 repriced Reforge Now from a flat 1000 to `2 x Standard { 60 }`. Two
players reported being charged ~7,000. Hotfixed back to a declared flat 1000 and
refunded, rather than rolled back, because release 20's other three price cuts
were wanted and the exposure was bounded at five uses.

#### THE CAUSE WAS NOT THE FORMULA

`MultipleOfStandard { times: 2, base: 60 }` evaluated exactly as designed. **The
live `craft_tier_exponent` is 1.5; the design, the approved cost table and every
test are written against 1.1.**

| tier | exp 1.1 (design) | exp 1.5 (live) | old flat |
|---|---|---|---|
| 35 | 312 | 1,256 | 1,000 |
| 107 | 1,038 | **6,654** | 1,000 |

At the exponent the design assumed, the reprice was near-neutral at high tier —
1,038 against 1,000. The flat price had **insulated** this action from the live
curve; connecting it to the curve is what exposed the discrepancy. The live
tunables file is dated 2026-09-04, four days before release 20, so this release
did not change the exponent. Crossover is around tier 30.

**The exponent discrepancy is the larger finding and it is with the owner.** It
affects every price on the curve, not this button.

#### THE DISPLAY WAS A SECOND, SEPARATE DEFECT

The dashboard rendered a hardcoded `Reforge Now (1000d)` while charging
`2 x Standard(highest tier)`. Two independent expressions that agreed until one
moved. The fix makes `WEB_REFORGE_DUST_COST` the single source for the label, the
affordability gate and the charge — **so the class is closed structurally rather
than by making two numbers match again.**

#### VERIFIED BY EFFECT, IN A 21-SECOND WINDOW

Dust moves continuously from fights, so "nobody else's moved" is only provable
across a tight window. Baseline captured at 21:54:29, deploy, re-capture at
21:54:50.

| character | before | after | delta | expected |
|---|---|---|---|---|
| wright | 38,702 | 44,356 | +5,654 | 5,654 |
| merkosh | 53,009 | 58,385 | +5,376 | 5,376 |
| jachiny | 12,634 | 17,646 | +5,012 | 5,012 |
| roxus | 4,447 | 8,413 | +3,966 | 3,966 |
| kibukah | 76,725 | 78,185 | +1,460 | 1,460 |

All five exact, **21,468 total, and characters outside the list whose dust moved:
NONE.** Marker `adventure-refund-reforge-now-overcharge-marker.json` present, so a
restart cannot re-grant. Live control re-verified rendering `1000`.

#### THIRD RELEASE RUNNING THAT A MARKER WAS MISSING FROM THE BACKUP ALLOW-LIST

`adventure-refund-reforge-now-overcharge-marker.json` was not in
`backup-game-data.sh`'s `MARKER_FILES`. Added in `4902663`.

**This is the first one where losing it costs currency.** Every other marker on
that list guards something whose second run is harmless or merely wasteful; a
backup restored without this one re-grants 21,468 dust silently on the next start.
`.gitignore` coverage confirmed with `git check-ignore` rather than inferred.

#### The named flake did not recur

b saw `live_reload_tests::editing_a_template_takes_effect_without_a_rebuild` fail
in its refund run and once more in isolation, then pass. This merge run was the
tiebreak: **0 FAILED, 0 panicked, 0 `live_reload` mentions** locally and on the
box. The ship-anyway clause was not needed.

#### The sweep's boundary case, resolved by b rather than assumed

Cooldown records store `current_hour_bucket()` — epoch hours — so a record dates a
use to the hour, not the minute. `roxus` and `kibukah` sat in bucket 496910, which
straddles the 16:20:37 deploy, and no persisted state on the live box could
separate them. Reported as an explicit unresolved group rather than folded in.
b resolved it from a verified backup snapshot taken inside the deploy window,
reading the cooldown file as it stood at 16:20:48. Evidence class: persisted state
from a checksum-verified snapshot, not inference.

### 2026-09-09 — CRAFT-LABEL-AGREEMENT deploy record (release 22)

| | |
|---|---|
| master commit | `ef86ec72b9965ab392ef4ef83eaf17c6cc5812e7` |
| live binary | `0aa514b8f114b0dff8b419e20e8f2898ca0e6444f9b340b948a4ad9cc7437af4` |
| previous | `710a6b4f7ba6d74eec21fa07e0d6fe88403db3bf373aa200b4fc5f32194377ab` |
| rollback slot | `deploy-pre-20260909-182757-craft-label-agreement` |
| downtime | **0.68 s** |
| suite | **920 passed / 0 failed / 44 result-lines** on the box |
| seven §13B.5 checks | all pass |

Suite delta 919 -> 920, exactly one new test in one new file:
`the_displayed_price_equals_the_declared_and_charged_price_at_every_tier`. Release
21 bound label to charge for ONE button via a shared constant; this binds all of
them via the rule itself, and `dust_at` is expressed THROUGH `display_params` so
the preview cannot drift from the charge unless an attribute drifts first.

Owner's condition verified: **no `PriceRule` table entry changed** — zero changed
lines among the `("name", PriceRule::…)` tuples.

#### THE TEMPLATE IS A SECOND ARTIFACT AND THE BINARY DEPLOY DOES NOT CARRY IT

`templates/base.html` is read at runtime from `/var/lib/pathofdust/templates/`,
which `deploy-linux.sh` does not touch. The label lives in that file's inline JS,
so **deploying the binary alone would have fixed nothing** — the whole release is
in the template.

Caught before deploying by hashing the live file against the candidate:

| | sha256 (first 32) | live `30 * tier` |
|---|---|---|
| live, before | `8036dcb2a682143f870266bf01ef9634` | **1, executable** |
| candidate | `622e5550f4b7910b44303bc4e30eec19` | 0 (2 in comments) |
| live, after | `622e5550f4b7910b44303bc4e30eec19` | 0 |

§13B.8 followed: hashed before AND after. The before-hash differing is what proves
the copy was not a no-op; matching the candidate after is what proves it landed.
Binary first, then template, so the ordering never makes the live state worse than
it already was.

**This is the second template/data artifact in two items** — the sprite manifest
stopped for the same reason yesterday. The difference is that `owners.toml` had no
documented install path (remedy ambiguous, correctly escalated) while `base.html`
has one in §13B.8 (remedy unambiguous, correctly executed).

#### THE LABELS VERIFIED AGAINST THE RULES, NOT AGAINST EACH OTHER

Server-rendered attributes, read off the live page after deploy:

| control | attributes | at tier 103 | the rule |
|---|---|---|---|
| Reforge | `base=6 times=5 flat=0 mult=3 exp=1.5` | 5 × (6 + 3136) = **15,710** | `5 × standard(60)` |
| veiled Recombine | `base=250 flat=50 per-unit=1 mult=3 exp=1.5` | 50 + 4 × (250+3136) = **13,594** | `50 + pool × standard(2500)` |

Both bases arrive pre-scaled from the server (`ceil(60×0.1)=6`,
`ceil(2500×0.1)=250`) and the browser evaluates only the rule's shape. Q1's ruling
— parameters, never formulas, in the browser — holds by construction.

Recombine's tier is the RESULT's, `floor((a+b)/2)+1`, matching `recombine_gear` —
b's third mismatch, and the one nobody reported.

#### A COUNT THAT LOOKED LIKE A DEFECT AND WAS NOT

A tree-identity check labelled "old 30*tier formula gone" returned **2**. Checked
by location rather than believed: both are `//` comments documenting the retired
formula. The decisive check was `dustCost = 30`, which returns **0** — the
assignment is what mattered, not the string.

#### The patch note corrects release 20's, in the note itself

Release 20 told players a 4-modifier veiled Recombine would go "2,500 → about
1,100–1,650". At the live exponent it is ~13,594. **That note announced a cut on
an action whose price rose about 5×**, because its figures came from the 1.1 design
table rather than the live 1.5 curve. Release 22's note says so plainly rather than
quietly restating the prices — a correction a player can see is a different thing
from a correction only the code knows about.

### 2026-09-10 — SPRITE-MANIFEST deploy record (release 23, item 3)

| | |
|---|---|
| master commit | `63a9ce40463231bc84c355e1f292c81768ae7115` |
| live binary | `e945caa39df21bed504b87f8b4ce09c037e0d108f1ec083b3aeae8cb732c130e` |
| previous | `0aa514b8f114b0dff8b419e20e8f2898ca0e6444f9b340b948a4ad9cc7437af4` |
| rollback slot | `deploy-pre-20260909-191434-sprite-manifest` |
| downtime | **0.46 s** |
| suite | **925 passed / 0 failed / 46 result-lines** on the box |
| seven §13B.5 checks | all pass |

Suite delta 920 -> 925: five tests in two new files, all named, all manifest-related.

#### THE DATA WENT DOWN BEFORE THE BINARY, DELIBERATELY

`owners.toml` was installed at the resolved path **before** the deploy, so no window
existed in which the enforcement ran without its data. The old binary does not
read the file at all, which is what makes installing early inert rather than
risky; installing late would have opened exactly the window a's new test
represents — sprite present, manifest absent, nobody can equip.

| | |
|---|---|
| before | **ABSENT** |
| after | `bc53b2eda6c4cb7ab82edef82caea044` — identical to the checkout |
| perms | `pathofdust:pathofdust`, 664, 14 entries |

`DATA_DIR` confirmed unset in the unit before relying on the resolved path, rather
than taking the order's word for it.

#### THE FIX IS THAT ONE FUNCTION ANSWERS BOTH QUESTIONS

`custom_sprite_dir()` is now `data_path(CUSTOM_SPRITE_DIR)`, and the manifest is
`custom_sprite_dir().join(CUSTOM_SPRITE_MANIFEST_FILE)`. **Listing the sprites and
resolving their ownership can no longer point at different directories**, which
was the actual defect — not the missing file, but a path that resolved one way in
a checkout and another way in production.

a's new test represents the box's condition directly: sprite present, manifest
absent -> nobody can equip; drop the file in -> recovers with no restart. That is
the test that would have caught what the stop caught.

#### VERIFIED BY EFFECT ON THE LIVE PAGE

Fetching `/characters` as each owner and checking the picker's contents:

| login | own sprite offered |
|---|---|
| `kibukah` | `custom/kibukah` — **yes** |
| `sitch89` | `custom/Sitch89` — **yes** |

The failure mode here is silent unselectability, which a binary hash and a green
suite cannot see. Only asking the page, as the user, closes it.

#### The named flake, confirmed by procedure and by structure

The local run was 924/1 on
`live_reload_tests::editing_a_template_takes_effect_without_a_rebuild`. Confirmed
in isolation per the house rule: **6 of 6 passed**. Structural argument on top:
this change touches **zero** template files, so it cannot reach template
live-reload. The box run was 925/0 with no recurrence.

#### FOUND — an order premise that did not hold

The order stated a pushed new heads on **both** `feature/sprite-manifest` and
`feature/store-classification`. Only the first moved: `bef24bd -> a6a6980`.
`feature/store-classification` is still `54f9e7c`, unchanged from the previous
order's listing. Item 9 is far off and nothing is blocked, but the head must be
re-read at its turn rather than assumed to have moved.

### 2026-09-10 — PACING-RELAX deploy record (release 24)

| | |
|---|---|
| master commit | `22584c2fc9dc4f0bc88a127c802425c0d4f00aad` |
| live binary | `7239a11297bf47668e0e13b75baa2e8b79f0e354e3b7a98567e1e4d49df3da32` |
| previous | `e945caa39df21bed504b87f8b4ce09c037e0d108f1ec083b3aeae8cb732c130e` |
| rollback slot | `deploy-pre-20260910-080403-pacing-relax` |
| downtime | **0.36 s** |
| suite | **929 passed / 0 failed / 46 result-lines** on the box |
| seven §13B.5 checks | all pass |

Controller A's relaxation trigger becomes a clock rather than a loss count. Suite
delta 925 -> 929, four named pacing tests, no new suite. The load-bearing one is
`the_same_elapsed_time_decides_identically_whichever_rampage_setting_is_live` —
a time-based trigger is cadence-independent, so the 60 s-vs-600 s discrepancy that
started this cannot recur.

#### BOTH HALVES OF THE DEPLOY-DAY TRAP, PROVEN LIVE

`WorldState::last_boss_win_unix_secs` is new with `#[serde(default)]`, so on the
first read after deploy it is 0. The trap is that 0 must mean "no win recorded"
and not "the last win was at epoch 0" — the latter yields ~1.79 billion seconds
elapsed and relaxes Controller A immediately.

| | value |
|---|---|
| `hp_pacing_mult` before deploy | **50.0** (at the ceiling) |
| immediately after restart | **50.0** |
| **after the first post-deploy boss fight** | **50.0 — did not relax** |
| `last_boss_win_unix_secs` after that win | **1789020862 = 2026-09-10 08:14:22** |

The first post-deploy boss fight (`fight-0000004700`, 08:14:22, **won**) exercised
the guard AND the write path in one event: the guard held, and the field now
carries the real win time. A being pinned at its ceiling made this unusually
decisive — any downward movement at all would have been the bug, with no need to
distinguish it from ordinary controller drift.

#### MY WATCHER MISSED A FIGHT THAT HAPPENED ON TIME

A background watcher polled for the first post-deploy boss fight and reported
nothing. The order reasonably read hours of silence as a fact about the live game
and asked why no boss fight came.

**One did.** `fight-0000004700` landed at 08:14:22, ten minutes after the 08:04
deploy, exactly at `ENCOUNTER_INTERVAL` = 600 s. Basic fights ran continuously
either side of it (4696…4701). The game was working correctly the whole time; the
instrument was not.

**I cannot determine why the watcher missed it.** Its only output was its start
line, and it kept no log of its own polling, so a post-mortem would be
construction rather than diagnosis. Recording that as an unknown instead of
inventing a cause.

The durable lesson does not depend on the cause, and is now standing: **never poll
for a game event longer than one encounter interval; if the event does not come,
the absence is the report.** And the corollary this taught: a watcher that reports
nothing is indistinguishable from a world in which nothing happened — so the check
should have been two direct reads after the fact, which is exactly how it was
finally closed and takes seconds.
### 2026-09-08 — Slayer's leech to 9×, and a hold-out that outlived its own argument

Branch `feat/slayer-leech-9x` off `origin/master` `15d6672`. `0.001` → `0.009`
on `Archetype::Slayer`'s `life_leech_pct`, a bare literal alongside its ten
sibling coefficients (owner ruling: making one class's number tunable while ten
sit as literals trades one friction for a worse one).

#### What actually happened to this class

Nothing about Slayer was ever rebalanced. On **2026-09-04** my own coefficient
sweep corrected `Leech.default_per_tier` 0.001 → 0.01 on the AFFIX. The archetype
coefficient did not move with it, so the free-with-the-class version fell from
*within a factor of two of one gear affix* to about *one tenth* of one. A number
Slayer was measured against moved out from under it, silently, in a change
advertised as being about affixes.

`0.009` is half a Leech affix. It restores the pre-2026-09-04 relationship rather
than inventing a new one.

#### Why 9× and not 17×, on evidence rather than caution

A full affix (~0.017) was on the table and was declined, and the reason is not
"be conservative":

**9× behaves identically with and without `endlessthirst`; 17× does not.** At
0.9–2.6% the leech sits far below `LIFE_LEECH_CAP_PER_SEC` (20% of max HP/sec) at
any DPS in the observed distribution, so every Slayer collects the full value
whether or not they have invested in the passive. 17× would pay out fully for an
`endlessthirst` 3/3 build and be substantially cap-absorbed for one without it —
**widening the gap between builds instead of fixing the class**, and doing it on
data that cannot currently support the choice.

It is also the reversible direction. 9× → 17× later is a one-line change with a
stated reason. Walking a shipped buff back is not.

#### The measurement that could not be made, and why it stayed unmade

Session c ran my saturation query exactly as written and it returned **n = 1** —
`COARSE_FIGHTS_CAPACITY` is 5, and exactly one retained fight was a winning boss
fight. Twenty characters in a single stage-59 fight: median 0.19, mean 1.72, max
9.31, with seven of the twenty dealing literally zero damage to the boss, so the
median rests on non-combatants (1.48 across the 13 who actually damaged it).

c refused to pick a row off that, which was right. But the distribution shows the
exact thing I warned about: **a median of 0.19 and a maximum of 9.31 are the same
distribution.** Four of twenty at or above 4.5, a fifth at 4.17, the top at more
than double the saturation threshold. The ruling was deliberately taken on the
anchor that does not depend on the answer.

#### The hold-out is about the SHAPE, and it stands

Both comment blocks argued the hold-out from "0.001 against the Leech affix's own
0.001 — a 1.0× ratio" and "a *pending* affix raise". **The raise had landed four
days earlier.** An argument that cites a number the code no longer holds is
exactly how a hold-out survives past its reason, which is the whole story of this
coefficient — so rewriting them was part of the commit, not a tidy-up.

`slayer_is_held_out_of_the_ruling_at_every_weight` **stays**, with its literal
updated and its doc saying why it is not obsolete: it guards that `w` does nothing
here, so Slayer never rides the affix curve. A session tidying it away as "already
changed" would delete the guard proving the hold-out still holds.

#### Corpus divergence — one scenario, and the win flag did NOT move

`slayer_vs_tough_boss_stage3000` is the only fixture with a Slayer (grepped, not
read off names), L80 where `mult = 9.0`, so leech goes 0.9% → 8.1%. It diverged.
**Regenerated nothing** — a capture is c's, at merge.

Probed with a throwaway test rather than by deleting the fixture, so the numbers
below are measured:

| leaf | old | new | ratio |
|---|---|---|---|
| `events[11].amount` | 19 | 171 | 9.00× |
| `events[13].amount` | 18 | 166 | 9.22× |
| `events[16].amount` | 9 | 81 | 9.00× |
| `events[19].amount` | 19 | 172 | 9.05× |

Four leech heals and the four `targetHpAfter` values they move — **eight gameplay
leaves, and nothing else.** Event count identical at 25; the fight took the same
shape. The remaining ~25 differing leaves are 1-ULP drift on
`0.20921834854734825` in the roll log, which the corpus's own `approx_eq`
tolerance is documented to accept and which is not what failed it.

**`won` is `false` before and `false` after.** Worth stating plainly because a
9× buff to a survival stat reads as though it should flip an outcome: at stage
3000 against a 2,000,000-HP boss it does not come close. And 171 against a
1,675/sec cap (20% of an 8,375 pool) confirms the coefficient multiplied cleanly
with **no cap absorption at all** at the most extreme level in the corpus — which
is the 9×-vs-17× argument holding up in the one place it could have been checked.

#### FOUND

The cap itself is now a live balance lever nobody has looked at — if 4 in 20 are
saturating, `LIFE_LEECH_CAP_PER_SEC` is doing real work. Owner has it as its own
board item; not this branch.

### 2026-09-10 — SLAYER-LEECH-9X deploy record (release 25, item 4)

| | |
|---|---|
| master commit | `bb2ac834299735b3d6a76012d259ac0a276484a4` |
| live binary | `e6632b6a5ce98aff72d41fa3bc2c1f12456dc49d1b33f36e98584203b3d53656` |
| previous | `7239a11297bf47668e0e13b75baa2e8b79f0e354e3b7a98567e1e4d49df3da32` |
| rollback slot | `deploy-pre-20260910-091207-slayer-leech-9x` |
| downtime | **0.17 s** |
| suite | **929 passed / 0 failed / 46 result-lines** on the box |
| seven §13B.5 checks | all pass |

`Archetype::Slayer` `life_leech_pct` 0.001 -> 0.009. One corpus fixture
regenerated. Suite unchanged at 929 with **0 new `#[test]`** — the two existing
assertions were updated rather than new ones added, which is why the count is
correctly flat.

#### THE EXPECTED SET WAS BOUNDED BEFORE THE RUN

The corpus holds **exactly one `Archetype::Slayer` scenario**, so one divergence
was the only possible correct answer and a second would have been unattributable.
That is a stronger check than matching a name afterwards, and it is available
whenever a change's reach is knowable in advance.

#### FIVE INDEPENDENT THINGS AGREED

| evidence | result |
|---|---|
| structural bound | 1 Slayer scenario -> at most 1 divergence |
| observed divergence | exactly `slayer_vs_tough_boss_stage3000` |
| source constant | `0.001 -> 0.009` = **exactly 9.000x** |
| fixture leaf ratios | **9.00 / 9.22 / 9.00 / 9.05**, b's figures reproduced in order |
| invariants | `won` false->false, attacks 14->14, heals 4->4 |

**The SPREAD in those ratios is evidence, not noise.** Leech heals are integers
clamped by missing HP, so a clean 9.00x on all four would have meant the clamp was
engaging nowhere — which at stage 3000 would itself deserve investigation. Uneven
ratios are what a correct implementation looks like here.

`won` was checked deliberately rather than inherited from b's report: a 9x buff to
a survival stat is exactly the change that feels like it should flip an outcome,
which is what makes an unexamined assumption there dangerous.

#### CHECK 3 RETURNED EMPTY AND IT WAS THE INSTRUMENT AGAIN

The first run of check 3a produced no `loaded N characters` line. That is the one
check that exists because a binary loading ZERO characters still answers 200, so
an empty result there is not something to wave through.

Re-queried with a wider window: the line **is** present, `loaded 24 characters` at
09:12:24, against 24 in the file. **Check 3 passes, 24 = 24.** The first query ran
about six seconds after the restart, before journald had flushed it.

Durable: **do not query journald for a startup line immediately after a restart** —
give it a few seconds or widen the window, or the check reports absence where there
is only latency.

#### The patch note says correction, not buff

Slayer's leech was measured against the Leech AFFIX, whose coefficient was
corrected 10x on 2026-09-04 while the archetype's was not moved with it. Nothing
about Slayer was rebalanced that day; a number it was measured against moved out
from under it, and its class perk silently fell from about one gear affix to about
a tenth of one. 0.009 is half a Leech affix, restoring the prior relationship.

A full affix (~0.017, 17x) was declined for a reason worth keeping: at 0.9-2.6%
the leech stays far below `LIFE_LEECH_CAP_PER_SEC` at observed DPS, so every
Slayer gets full value regardless of `endlessthirst`; at 17x it would pay out fully
for a 3/3 build and be substantially cap-absorbed for one without — widening the
gap between builds instead of fixing the class.
## 2026-09-08 — THE GAME'S LOG SINK GETS THE SAME RETENTION POLICY (branch `feature/game-log-retention`)

Cut from master (`3a3253c`, which had moved past the `4cb054f` the order
named). The game crate is untouched by both bot branches, so this does
not queue behind them.

`bot/src/logging.rs` is the template. Four things were not carbon copies.

### 1. WHERE IT ACTUALLY LANDS, established before touching anything

A retention policy pointed at the wrong directory prunes nothing and
looks fine, so this was derived rather than assumed:

  * the unit sets `WorkingDirectory=/var/lib/pathofdust`
    (`docs/linux_staging.md:126`)
  * it sets exactly THREE `Environment=` lines - `OPERATOR_LOGIN`,
    `ADVENTURE_WEB_PORT`, `ADVENTURE_OVERLAY_SERVER_PORT` - and
    **`GAME_DATA_DIR` is not among them** (:155-157; :48 says so in
    words)
  * so `data_path` joins onto its EMPTY default base and
    `data_path("logs")` is the bare relative path `logs`

**=> `/var/lib/pathofdust/logs/game.log.<YYYY-MM-DD>`**

**Cross-checked independently of that arithmetic**, because a chain of
three documents agreeing with each other is still one source: the unit
runs `ProtectSystem=strict` with `ReadWritePaths=/var/lib/pathofdust`, so
if the logs resolved anywhere else the process could not have written
them at all. The files existing is itself evidence of the path.

### THE UNIT FILE ALREADY SAID JOURNALD DOES NOT COVER IT

`docs/linux_staging.md:137-140`, in the service definition's own comment:

> journald is the log of record. The binary ALSO writes logs/game.log via
> its tracing_appender layer (main.rs) - that is in the code, not
> configurable here, and lands under WorkingDirectory like every other
> data file.

The separation was written down **at the moment the unit was authored**,
and nobody carried it one step further to "therefore nothing prunes it".
Three later sessions then recorded the opposite as the mitigation. The
fact was never missing; the inference was.

### 2. THE NUMBER, ARGUED FOR THIS PROCESS RATHER THAN INHERITED

**Its own constant, deliberately equal to the bot's rather than shared
with it.** The two processes have different log profiles and different
uptime, so a future measurement must be able to move one without moving
the other. A shared constant would force the next person to choose
between changing both and changing neither.

**The order's premise - "the game is a busier process than the bot" - is
true of requests and false of logging.** Emission sites:

  game  18 info / 19 warn / 58 error = 95
  bot   49 info / 43 warn / 35 error = 128

and site counts understate it, because what matters is which sites sit in
a hot path. **The game has none.** Every `info!` in the crate is startup
(`"loaded N characters"`, the two server-started lines), a one-time
migration, a balance-file override, or a rare operator/player action
(`!pinfight`, a Unique Shard apply, a login). There is no per-fight and no
per-request logging at all, and the crate's dominant category is
`error!`, which only fires when something is already wrong. A healthy day
is small; an unhealthy day is exactly the one worth keeping.

**The wall-clock window is SHORTER here for the same number, which is the
right direction.** `rolling::daily` writes a file only on a day something
is logged. The bot runs when the stream is on, so its 30 files span more
than 30 calendar days; the game runs continuously under `Restart=always`,
so its 30 files are 30 days almost exactly. The always-on process gets
the tighter bound from the identical constant.

**30 rather than fewer** because the game's diagnostic unit is the
RELEASE and c deploys roughly daily - a month of files is a month of
releases, which matches how far back the anomaly ledger actually cites.
Fewer would put "this started a few releases ago" outside the window.

Stated rather than implied, same as the bot: this bounds FILES, not
BYTES. The emission sites above are counted from source, not sampled from
production. Growth was UNBOUNDED and is now BOUNDED, which is the defect.

### 3. THE RESTART GUARANTEE, WHICH BINDS HARDER HERE THAN ON THE BOT

The bot's case was a crash-restart. The game's is a DEPLOY, and c deploys
roughly daily, so the frequent path is not a crash at all. **A deploy that
pruned the current day's log would destroy the evidence of whatever went
wrong in the release before it** - precisely the log a rollback decision
is made from.

It cannot: `prune_old_logs` sorts ascending by creation time and deletes
from the FRONT, keeping the newest `max_files - 1`, and today's file is
either the newest that exists or does not exist yet. Pinned by
`todays_log_survives_a_restart_or_deploy_that_prunes`.

### PLATFORM NOTE - the ranking path differs from the bot's and both are correct

The bot's twin was verified on Windows, where `metadata.created()` is a
real birth time. This one runs on Linux in production, where `statx` may
or may not report a btime depending on the filesystem. Both of
`prune_old_logs`'s ranking paths order these correctly: with a btime the
creation timestamps ascend with write order, and without one
`parse_date_from_filename` reads the `YYYY-MM-DD` suffix and orders by
that. The test seeds in ascending date AND ascending creation order so
the two agree - which is also what a real rotation produces.

### 4. FILENAMES UNCHANGED, for the reason that is a failure mode

`game.log.<date>`, byte for byte, so the files already on the box are
adopted by the policy. `prune_old_logs` filters on the prefix, so a
builder emitting a different name would leave every existing file
permanently invisible to a policy that reported success - a working
pruner that eats nothing. Same shape as the `-IncludeEnv` bug in the
bot's backup script; third time this week that this exact failure
signature has been caught before shipping.

### A nuance worth recording about the disk-growth finding

`docs/linux_deploy.md:176` says `/var/lib/pathofdust` went 40 MB -> 7.0 GB
and then **plateaued**. That is a statement about the FIGHT TIERS, which
prune themselves via `fight_storage.rs`'s capacities. **Logs sit in the
same directory underneath that plateau and did not participate in it** -
so "plateaued" was true and still left an unbounded component inside the
number.

### Verified

Four tests, same shape as the bot's: six seeded with max 3 -> four oldest
gone; today's file created last with max 2 -> survives, content intact; an
incident-notes file in the log directory survives; the shipped
configuration writes `game.log.<YYYY-MM-DD>`.

Full workspace suite as one `--workspace --no-fail-fast` invocation.

**THIS ONE DEPLOYS.** It is a live production binary, so c takes it with
a release. The resolved path is in this entry and in the report so it can
be verified on the box afterwards: `/var/lib/pathofdust/logs/` should stop
at 30 `game.log.*` files.

No WIKI_IMPACT line: no cost, chance, formula, timer, boss behaviour,
crafting rule or command name changed.

### 2026-09-10 — LOG-RETENTION-FIXED deploy record (release 26 = item 5 + b's fix)

| | |
|---|---|
| master commit | `14af32b1d7d723dbcd0e06c2de1cefa91d1847ef` |
| live binary | `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| previous | `e6632b6a5ce98aff72d41fa3bc2c1f12456dc49d1b33f36e98584203b3d53656` |
| rollback slot | `deploy-pre-20260910-164947-log-retention-fixed` |
| downtime | **0.54 s** |
| suite | **936 passed / 0 failed / 46 result-lines** on the box |
| seven §13B.5 checks | all pass |

The Linux red from the first attempt is gone: the two retention tests that failed
deterministically on the box now pass there.

#### THE CORRECTION THAT MATTERED WAS MINE

I reported the first red as "a TEST defect, not a product defect", reasoning that
production is ext4 with one log per day and nine distinct btimes, so **"production
never produces the tie the test constructs."**

That inference was wrong and the owner caught it. `tar -xzf` stamps every extracted
`game.log.*` with the same btime, so a **restore** produces exactly that tie — on
the first start after a restore, which is the one start where the current day's log
is most worth keeping. The test found on tmpfs what a restore would have found on
ext4.

**The general form, which is the durable part: observing a property of the current
state is not establishing an invariant of the system.** The measurement was sound;
the boundary was drawn in the wrong place.

#### THE FIX REPLACES THE LIBRARY PRUNER RATHER THAN REORDERING IT

I had assumed b would demote btime within an ordering of ours. There was none:
`tracing_appender`'s `prune_old_logs` uses btime first and the filename only as
`or_else`, so it could not be inverted from outside. Leaving `max_log_files` set
would have kept the library pruning at construction and re-ranking a restored
tie-btime directory whatever our code did afterwards.

Verified in the deployed tree: **`.max_log_files(` appears 0 times**, and
`prune_by_filename_date` is ours. A prefixed file with no parseable date is never
deleted.

#### VERIFIED BY EFFECT, THREE WAYS — AND THE THIRD IS THE ONLY REAL ONE

| check | result |
|---|---|
| directory listing, before vs after | **IDENTICAL** — removed nothing, added nothing |
| today's log size | 1,623 -> 2,324 B (grew; the restart appended) |
| **today's first line** | `2026-09-10T01:40:07…` — the ORIGINAL 01:40 line |

The count was never going to be the check: 9 files either side is equally consistent
with one deleted and today's recreated. And b's mutation check had already shown
`exists()` vacuous — a deleted log is reopened empty microseconds later.

**The first line is the proof.** A recreated file would begin at the restart's own
first line, around 16:49. Beginning at 01:40 proves the file was appended to.

#### KNOWN GAP, ACCEPTED AND ON THE BOARD

Pruning runs at appender construction only. With `Restart=always` and near-daily
deploys that is bounded by restarts; an uptime beyond 30 days without a restart
would exceed the limit until the next start.

#### The named flake, confirmed twice over

Local was 935/1 on `live_reload_tests::editing_a_template_takes_effect_without_a_rebuild`.
**6 of 6 in isolation**, and this release changes exactly one file —
`game/src/logging.rs`, zero templates — so it cannot reach template live-reload. The
box run was 936/0.
## 2026-09-08 — THE BOT GETS A BACKUP, AND MOVES INTO `bot/` (branch `feature/bot-into-subdirectory`)

Two things, in the order the owner ruled: the backup first because it is
independent of the move and worse than the move, then the move itself.

### The gap the bot-extraction survey found

`tokens.json`, `commands.json`, `entrance-themes.json`,
`personal-playlists.json` and `song-queue.json` existed in exactly one
place. `backup-game-data.ps1` is an explicit allow-list and every entry
in it is a game file; `backup-game-data.sh` archives
`/var/lib/pathofdust`, the LINUX game data root, which holds no bot file.
The game moved to Linux and the bot did not, so the nightly backup that
had been running all week covered none of it.

`backup-bot-data.ps1` is `backup-game-data.ps1`'s shape, not a new
design: same parameters, same share-mode copy, same verify-then-prune
ordering, same manifest and verdict, same earliest-of-day retention. Only
the differences are re-argued in it.

**11 files backed up, 4 excluded WITH REASONS** rather than by omission —
`search-cache.json` (a YouTube cache, rebuilt by re-querying: losing it
costs quota, not data), `daily-greeted.json` (`GreetedToday` carries its
own date and self-invalidates, so its maximum lifetime is one day),
`commands-data.json`/`themes-data.json` (derived public-site outputs,
regenerated on every load, and written into `PUBLIC_SITE_DIR` rather than
the bot's directory). `.env` is opt-in behind `-IncludeEnv`: copying live
secrets into up to 54 retained snapshots multiplies where a leak can come
from, to protect values that are all re-issuable.

**The manifest was derived twice and the narrow derivation was wrong.**
Grepping `PathBuf::from(...)` misses three files — `playrandom.rs` holds
its path in a `const STATE_PATH: &str` and the two public-site outputs
are built with `dir.join(...)`. Grepping every file literal in `src/**`
finds all of them. That is why the list is derived twice and
cross-checked, and the script says so.

**THE HAZARD IS DIFFERENT FROM THE GAME'S AND SMALLER, so the comment
saying otherwise was not copied.** The game persists with
`std::fs::write` (truncate, then write), and a copy taken inside that
window is a valid, useless file — that window is why its script retries.
The bot has no such window: `state.rs`'s `save_json` goes through
`write_atomic` (temp file, fsync, rename), so a reader sees the complete
old file or the complete new one. Verification is kept anyway, for the
two failures atomicity does not cover — a copy failing part-way, and a
source that was already corrupt before the run.

### THE REAL RUN FOUND A REAL BUG, which is the entire argument for running it

First live run with `-IncludeEnv`: **every run degraded, and a degraded
run skips pruning.** `Test-DataFile` assumed JSON, `.env` is `KEY=value`,
so the switch would have silently disabled retention forever while still
appearing to back up — failing in the direction where the thing looks
healthy. Fixed by deciding the check from the file's name inside
`Test-DataFile`, so the dry run and the live run can never disagree about
which kind a file is.

Verified across six runs against synthetic scratch trees, never the
running bot's files: 11 copied / verdict clean; the four exclusions
absent from the snapshot; zero-length and corrupt-JSON both hard
failures; a UTF-8 BOM copied but flagged (serde_json will not parse
through one, so a BOM means the LIVE file is already broken); degraded
skipping the prune and exiting 1; retention pruning 3 of 6 aged snapshots
by the right rule; `-IncludeEnv` clean after the fix.

### The move — option (b), virtual workspace manifest

`src/` -> `bot/src/`, the three overlay asset directories with it, the
root package into `bot/Cargo.toml`, and the root reduced to
`[workspace] members = ["bot", "game"]`.

`resolver = "2"` is EXPLICIT and load-bearing. A virtual manifest
defaults to resolver 1 regardless of what edition its members declare,
whereas the previous root was a 2021-edition package and got resolver 2
implicitly. Omitting the line would have changed feature unification
across the whole workspace as a side effect of a directory move.

**`Cargo.lock` did not change by one byte**, which is the evidence that
the dependency graph after the split is the same graph.

**Zero source changes.** The bot has no path indirection at all — every
one of its 16 runtime paths is a bare CWD-relative literal — so the
working directory IS the data directory, and relocating it relocates all
sixteen. The property that would have made this expensive is the one that
made it free.

### watchdog.ps1 does NOT hold what the order believed, and moving it would have broken it

The order named "watchdog.ps1's working directory and binary path". It
holds neither. It holds `$TaskName` and `$ExpectedPathRoot`, the latter
defaulting to `$PSScriptRoot` and compared against the LISTENING
PROCESS'S IMAGE PATH.

Cargo puts every workspace member's binary in one shared `target\`, so
moving the crate moved no binary: the exe is still
`target\release\twitch-bot-rs.exe`, a sibling of the script and not of
the bot's sources. **Move watchdog.ps1 into `bot/` and
`$ExpectedPathRoot` becomes `...\bot`, the live bot's own exe stops
testing as "under my root", and the watchdog reads a healthy process as
foreign.** So it stays at the root, unchanged except for a comment
recording why — otherwise the next session tidies it into `bot/` and
un-protects the bot.

The working directory genuinely does change, to `bot\`. That lives in the
`TwitchBotRS` scheduled task, which is on the box rather than in this
repo, so it is a cutover step and not a code change.

**Checked rather than assumed: `maintenance-flag.ps1` is unaffected.** It
resolves the authoritative root from the `TwitchBotRS-Watchdog` task's
`-File` argument (:146, :168), which points at `watchdog.ps1` at the
repository root. That path does not move, so the flag still lands where
the running watchdog looks.

### REFACTOR_PLAN section 13's conditional bot redeploy rule keyed on `src/**`

Amended, under the rule's own instruction to re-derive the dependency set
"only if the workspace structure changes" — this is that change.
`src/**` no longer exists at the repository root, so the pre-amendment
path list would have matched nothing and **silently skipped every bot
redeploy.** Amendment appended rather than the original rewritten, the
same way the 2026-08-22 decoupling amendment was.

Section 10's note that "a plain `cargo build --release` from the root
only builds the root package in this workspace shape" also goes stale —
a virtual manifest builds every member — but it is a dated record of a
past stage rather than authoritative procedure, and CLAUDE.md's
`--workspace` instruction stays correct either way. Not touched. FOUND,
one line, here.

FOUND — the root `.env.example` after the split documents ONE of the
game's five keys (`OPERATOR_LOGIN`). `GAME_DATA_DIR`,
`OPERATOR_BOOTSTRAP`, `ADVENTURE_WEB_PORT` and
`ADVENTURE_OVERLAY_SERVER_PORT` were never in it. Pre-existing, not
caused by the split, not fixed here.

### The cutover is written and NOT run

`docs/bot_move_cutover_runbook.md`. It stops the live bot mid-stream if
run at the wrong time, so the owner picks the window. The ordering is the
value: suppress the bot watchdog FIRST, because left armed it restarts
the old bot from the old directory while the state is being copied and
two processes then hold `tokens.json`. Stop by PID or by the scheduled
task, never by image name. Copy, never move, because the copy is the
rollback. Leave the old directory for one full stream.

Its step 5 ends on the check that actually proves the cutover took: that
`bot\tokens.json` gains a newer timestamp after the first token refresh.
Every other check passes just as well against a bot still running happily
from the old directory.

No WIKI_IMPACT line: no cost, chance, formula, timer, boss behaviour,
crafting rule or command name changed.

### 2026-09-10 — BOT-INTO-SUBDIRECTORY (release 27) — MERGED, AND CORRECTLY NOT DEPLOYED

| | |
|---|---|
| master commit | `5efda83eb08d33750107b66037c6c0b649feeaae` |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** |
| suite | **936 passed / 0 failed / 46 result-lines** on the box |
| rollback slot | **none created** — nothing was swapped |

`deploy-linux.sh` stopped with:

```
FATAL: new binary is identical to the live one - nothing to deploy
```

**That is the right outcome and it is also the branch's own proof.** This is a
workspace *layout* move: the bot's source goes from `src/` to `bot/`, plus
`Cargo.toml`, `.gitignore`, `README.md`, `REFACTOR_PLAN.md`, `watchdog.ps1`. It
touches nothing in the `game` crate, so `game` compiles to the **same bytes** as
release 26's binary.

A layout change that leaves the shipped artifact byte-identical is exactly what
"the move changes nothing about the running game" should look like, and the deploy
gate demonstrated it rather than anyone asserting it. No restart occurred (service
still up from 16:50:04), so no patch-notes entry is owed — the rule is one entry
per deploy, and there was no deploy.

#### THE PROPERTIES, VERIFIED ON THE BOX'S OWN EXTRACTED TREE

| property | result |
|---|---|
| `members = ["bot", "game"]` | present |
| `resolver = "2"` | present — a virtual manifest defaults to resolver 1 |
| `bot/src` present, root `src/` gone | correct |
| `watchdog.ps1` at root, absent from `bot/` | correct |
| `Cargo.lock` changed by the move | **0 lines** |
| cutover runbook shipped | yes, unrun |

`Cargo.lock` byte-unchanged across a 52-file move is the strongest of these: the
dependency graph is **the same graph**, which one differently-resolved dependency
would expose.

Both members build from **one** `--workspace` invocation into one shared target,
verified on the box rather than only locally:

```
target/release/game            17,469,192 bytes
target/release/twitch-bot-rs   23,137,048 bytes
```

That shared target is also why `watchdog.ps1` must stay at the repository root:
moving it into `bot/` would resolve `$ExpectedPathRoot` to `…\bot` while the binary
it guards sits in the shared `target\release\`, and the watchdog would read the
healthy live bot as foreign.

#### A CLAIM OF MINE THAT EXPIRES

I recorded on 2026-09-10 that **the bot has zero `#[test]` functions**, and used it
to qualify the one-invocation check — correctly, since an unchanged suite total
could not have demonstrated bot coverage that did not exist.

That was true when measured and is already going stale: d's
`fix/insert-backstop-desync` adds the first two. Retiring it here rather than
leaving it to be cited later as standing, which is the same failure mode as the
`939ec8f` premise and the `1fa0beb` head.

#### Not run

The cutover. `docs/bot_move_cutover_runbook.md` ships with the branch; the owner
picks the window. Merging changed nothing about the running bot, and no bot process
runs on this box.

### 2026-09-11 — INSERT-BACKSTOP-DESYNC (item 7a) — merged, bot-only, deploy correctly refused

| | |
|---|---|
| master commit | `fcb2d1fca642b9c61327f899db688d007bcbc4b4` |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** — byte-identical |
| box suite | **938 passed / 0 failed / 46 result-lines** |
| rollback slot | none created |

Bot-only, so the game binary cannot change and the gate said so. Second item in a
row where the refusal is the expected result rather than a failure.

#### THE RENAME-AWARE MERGE RESOLVED ITSELF — AND THE CHECK WAS NOT "NO CONFLICTS"

d's branch was written against **root** paths before release 27 moved the bot:
`src/song_requests.rs`, `src/song_overlay_server.rs`,
`public_song_overlay/overlay.html`, `WIKI_IMPACT.md`. Git followed the renames and
applied all four hunks to the `bot/` paths automatically — **112 insertions, 0
deletions, matching d's own figures exactly.**

**The dangerous outcome here was never a conflict.** A conflict is loud. The quiet
failure is git *succeeding at the old paths*: recreating `src/song_requests.rs`
beside `bot/src/song_requests.rs`, so the workspace builds the bot from `bot/` while
d's desync fix sits in an orphaned root copy — compiling, passing, shipping, and
doing nothing.

So the check was **the absence of `src/` and `public_song_overlay/`**, verified in
the merged tree and again in the box's extracted tree, not the absence of conflict
markers.

#### A CLAIM OF MINE EXPIRES, EXACTLY AS FLAGGED

On 2026-09-10 I recorded that the bot has **zero `#[test]` functions**, used it to
qualify release 27's one-invocation check, and said explicitly that d's branch would
retire it.

Measured here: **bot `#[test]` count 0 -> 2** —
`stuck_backstop_for_a_superseded_insert_stays_a_no_op` and
`stuck_backstop_relays_skip_insert_to_the_overlay`.

That also gives release 27's property real content. "One `--workspace` invocation
covers both members" was structurally true and **empty** for the bot while it had no
tests; the bot's two now run in the same invocation as the game's 936.

### 2026-09-11 — RECONNECT-STORM-LOG-AND-CREDENTIALS (item 7b) — merged, bot-only, deploy correctly refused

| | |
|---|---|
| master commit | `c4f198755f0a0c89c1261b26a15727f7229c2421` |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** — byte-identical |
| box suite | **946 passed / 0 failed / 46 result-lines** |
| rollback slot | none created |

Third consecutive item where the refusal is the expected result. 4 files, 408
insertions / 4 deletions, one new — matching d's figures.

#### IT SHARES NOTHING WITH ITEM 7's FAILURE MODE, WHICH IS WHY IT COULD GO WHILE 7 WAITS

Both branches aim at "the bot writes too much log", and it would have been easy to
hold this one by association. Measured instead: `bot/src/log_rate_limit.rs` has
**0 `max_log_files`, 0 `btime`/`created()`, 0 `fs::`**. It is an in-memory tracing
layer that bounds emission, not a pruner that ranks files by creation time. The two
solve the same complaint at different layers and only one depends on filesystem
ordering.

#### THE EIGHT TESTS, AND THE TWO THAT ARE NOT VACUOUS

Rate limiter: `a_synthetic_flood_is_bounded_and_the_total_is_reported`,
`only_the_two_flooding_targets_are_limited`, `the_cap_is_per_target`,
**`the_layer_actually_keeps_suppressed_events_out_of_the_output`**.

Auth: `a_still_valid_cached_token_is_reused_instead_of_erroring`,
`an_expired_cached_token_is_still_reused_rather_than_spinning`,
`the_expiry_boundary_is_not_treated_as_expired`, **`an_empty_token_still_fails`**.

The two in bold are the ones that would be absent from a naive version. A rate
limiter that COUNTS suppressions while still emitting them passes every other
limiter test; asserting the output is what makes it real. And "reuse the cached
token when refresh fails" is a fallback made more permissive — which is exactly how
it becomes "accept anything" — so the empty-token rejection is the counterweight,
with the expiry-boundary test closing the off-by-one.
## 2026-09-08 — THE LOG SINK GETS A RETENTION POLICY (branch `feature/bot-log-retention`)

Cut from `feature/bot-into-subdirectory` rather than master, because the
policy lives in the bot crate and the bot crate is only at `bot/` there.

### What the sink actually did, read rather than inferred

`tracing_appender::rolling::daily("logs", "bot.log")`. It rotates daily
and **deletes nothing, ever** — there is no retention parameter on that
constructor at all. `logs/` reached several GB once already; the sink was
disabled outright on 2026-08-17 and re-enabled after a ONE-TIME MANUAL
CLEANUP, which is not a policy, it is the same incident waiting on the
same interval.

### THE MITIGATION EVERYONE WAS WAITING FOR WOULD NOT HAVE WORKED

Three sessions, including two of mine, recorded that this resolves "at
the Linux move where journald owns rotation". **It does not.** journald
owns a process's STDOUT. It has nothing to do with a file appender that
opens its own files and writes around the supervisor entirely.

The proof is already in production: `game/src/main.rs:86` builds the
IDENTICAL unpruned appender, and the game has been on Linux under systemd
since release 16, writing `data_path("logs")` with journald running and
pruning exactly nothing. The platform was never the variable.

So the answer to "is a Windows-side pruner throwaway work given the
Linux move" is that the question had a false premise on both halves: it
is not a pruner and it is not Windows-side.

### The fix is a configuration change to the sink that already exists

`tracing-appender` grew the policy since this code was written. 0.2.5 —
already the pinned version, no dependency change —
has `RollingFileAppender::builder().max_log_files(n)`, and
`Inner::prune_old_logs` runs both at construction and on every rotation.

So `bot/src/logging.rs` configures the existing appender instead of
adding a second mechanism beside it. That is what satisfies "it travels
with `bot/`" **by construction rather than by discipline**: there is no
path to keep in step, no scheduled task to register, and it behaves
identically on Windows and Linux, so it survives the bot's own eventual
move without an edit.

**Filenames are unchanged, and that is load-bearing.** `rolling::daily`
is `RollingFileAppender::new(DAILY, dir, prefix)`, and `join_date`
formats `(DAILY, Some(prefix), None)` as `"{prefix}.{date}"`. The builder
sets the same rotation and prefix and no suffix, so it produces
`bot.log.YYYY-MM-DD` byte for byte. **Existing files are adopted by the
policy rather than orphaned beside it** — and the orphaned case is the
one that looks like success, because `prune_old_logs` filters on the
prefix and would simply never see them.

### The number: 30, and what it does not claim

Matches the daily tier of both backup scripts, so there is ONE retention
horizon to remember rather than a separately-optimal second one.

**The count is per ACTIVE day, not per calendar day** — `rolling::daily`
creates a file only when something is logged. A bot that runs three days
a week reaches 30 files in about ten calendar weeks; one that runs daily
reaches it in a month. That is the right direction on both ends: the
quiet install keeps a longer window because its days are scarcer. A
calendar-age rule would have given it less history for the same disk.

**Stated rather than implied: this bounds files, not bytes, and no
session has measured a real day's volume** — the live box is not this
window's to read. The honest claim is that growth was UNBOUNDED and is
now BOUNDED, which is the actual defect. One constant to change if a
day's volume ever makes 30 too many.

### It cannot delete the log being written, and the reason is the library's

`prune_old_logs` sorts ascending by creation timestamp and deletes from
the FRONT, keeping the newest `max_files - 1` "because we will create
another log file". Today's file is either the newest that exists or does
not exist yet, so it is never inside the deleted prefix. That holds at
construction too, which is the case that matters: the watchdog exists to
restart the bot, every restart builds a fresh appender, and **a restart
that pruned today's file would destroy the evidence of the crash that
caused the restart** — the exact log anyone would go looking for.

Pinned by `todays_log_survives_a_restart_that_prunes` rather than left to
this paragraph.

### Verified by real runs against synthetic aged trees

Four tests, the same shape as the backup script's verification: files
seeded in ascending creation order ON PURPOSE, because the pruner ranks
by filesystem creation time and falls back to the name only when metadata
is unreadable — seeding in the wrong order would test a ranking that
never occurs.

  * six seeded, max 3 -> four oldest gone, two newest kept
  * today's file created last, max 2 -> survives, content intact
  * `crash-dump-keep-me.txt` and `notes.md` in the log directory -> both
    survive; the prefix filter means the pruner can only ever eat its own
  * the shipped configuration writes `bot.log.<YYYY-MM-DD>`

The pruning test deliberately does not use `MAX_LOG_FILES`: seeding 30+
files to exercise the shipped constant proves the same thing slower, and
would silently stop testing anything if someone lowered the constant
below the seed count.

### FOUND — THE GAME HAS THE IDENTICAL DEFECT, LIVE ON LINUX TODAY

`game/src/main.rs:86` is the same `rolling::daily` call with the same
absence of retention, writing to `data_path("logs")` on the production
box right now. It is NOT covered by the Linux backup (that script stages
an explicit `CORE_FILES` allow-list, so logs are neither backed up nor
inflating the archives) and it is NOT covered by journald, for the reason
above.

**Not fixed here — the order scoped this to the bot, and the game is a
live production binary whose change belongs to a session that can deploy
it.** The fix is the same three lines; `bot/src/logging.rs` is the
template. Needs an order.

### Also, the two stale lines the order offered

REFACTOR_PLAN section 10's parenthesised reason ("root `Cargo.toml` is
both the workspace definition and its own package") expired with the
virtual manifest. **Annotated with a dated note rather than rewritten** —
it is a dated record of that release, `--workspace` remains correct and
required either way, and the note says the REASON is historical, not the
COMMAND.

The root `.env.example` documented one of the game's five keys. Now all
five: `ADVENTURE_WEB_PORT` and `ADVENTURE_OVERLAY_SERVER_PORT` with their
defaults and the source line, `GAME_DATA_DIR` commented out with the
warning that production runs it UNSET and that changing it on a live
deployment strands the existing data, and `OPERATOR_BOOTSTRAP` commented
out with what it is for and the instruction to remove it after use.

No WIKI_IMPACT line: no cost, chance, formula, timer, boss behaviour,
crafting rule or command name changed.

### 2026-09-11 — BOT-LOG-RETENTION + FIX (item 7) — merged as one, box-green, deploy correctly refused

| | |
|---|---|
| master commit | `f7c16046d13bbd480f8c57b29fbdfed75a3291cb` |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** — byte-identical (fourth consecutive) |
| box suite | **953 passed / 0 failed / 46 result-lines** |

`a1f7389` contains `891095b`, so one merge carried the feature and its fix — the way
release 26 carried item 5.

#### THE CHECK WAS NAMED TESTS, NOT A TOTAL

The box total was 953/0. **That alone could not distinguish "both green" from
"neither ran"** — the `--quiet` run prints no passing test names, and the bot's test
suite went from zero to existing two items ago, so "the tests are not there" was a
live possibility rather than a pedantic one.

Run by name on Linux instead:

```
logging::tests::constructing_the_appender_prunes_down_to_the_retention_limit ... ok
logging::tests::todays_log_survives_a_restart_that_prunes ... ok
logging::restore_condition_tests::identically_timestamped_logs_still_keep_today ... ok
logging::restore_condition_tests::a_prefixed_file_with_no_date_is_never_pruned ... ok
logging::restore_condition_tests::the_date_key_accepts_only_the_shape_rotation_writes ... ok
```

The first two are the ones that failed deterministically on this same box on
2026-09-11. The third is the condition the whole defect turns on.

#### THE ARC, FOR THE RECORD

2026-09-10: item 5's retention went red on Linux, green on Windows. I measured the
mechanism — six identical btimes on tmpfs, nine distinct on ext4 — and concluded
**"a TEST defect, not a product defect"** because production never ties.

**That inference was wrong.** `tar -xzf` stamps every extracted log with one btime, so
a restore ties them — on the first start after a restore, the one start where the
current day's log matters most. *Observing a property of the current state is not
establishing an invariant of the system.*

Release 26 fixed the game. I then predicted **by name**, before running anything,
that the bot branch carried the same defect, and measured 938/2 on the box failing
exactly those two tests. This is the port, proven on the platform where it failed.

#### The `lib.rs` conflict was the first real code conflict of the bot sequence

7b's `pub mod log_rate_limit;` and item 7's `pub mod logging;` claimed the same
alphabetical slot. Mechanical, not semantic: independent modules, neither replacing
the other, both files present — keep-both is the only resolution that compiles, and
it happens to preserve alphabetical order since `_` sorts before `g`.

#### b's duplication choice, and why it is the honest one

b copied the game's pruner rather than sharing it, on the manifest's own stated
property that the two crates share no file, with each pruner's doc naming the other as
its twin. A shared crate would re-couple builds that `chore/bot-decoupling`
deliberately separated. The risk is the copies drifting; naming the twin in both docs
is what makes that visible rather than silent.

**On the board, from b:** the rotation gap is restart-only and **worse on the bot** —
no `Restart=always`, no daily deploys — and `tracing-appender` 0.2.5 exposes no hook.

### 2026-09-11 — WALKON-SKIPS-COMMANDS (item 7c) — merged, bot-only, deploy correctly refused

| | |
|---|---|
| master commit | `115dcddea2c5f1df6528b2f17b508f5b757c45ef` |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** — byte-identical (fifth consecutive) |
| box suite | **958 passed / 0 failed / 46 result-lines** |

#### THE RISKIEST SHAPE THAT LOOKS SAFE: AN EXTRACTION PLUS A BEHAVIOUR CHANGE IN ONE COMMIT

`parse_command`/`is_command` are lifted out of the dispatcher's inline parse — meant
to be behaviour-preserving — while `entrance_themes` gains a real new behaviour, a
command no longer spending the walk-on.

**If the extracted parser diverges from the inline original, the symptom appears in
command dispatch generally, not in walk-ons, and reads as unrelated.** So the
question worth asking of the tests was whether they cover the EXTRACTION, not only
the feature.

They do. Five added, and the two that matter are:

- **`a_bare_bang_is_chat_but_an_unknown_command_is_still_a_command`** — the
  extraction boundary. A bare `!` versus an unrecognised `!foo` is exactly where a
  rewritten parser drifts, and it is pinned.
- **`a_normal_message_first_fires_once_and_only_once`** — the counterweight: the
  UNCHANGED path still fires, and exactly once, so the refactor cannot silently break
  ordinary walk-ons while the new feature looks fine.

Plus `commands_all_day_never_fire_and_never_spend_it` (degenerate) and
`two_commands_then_a_normal_message_fires_on_the_third` (sequencing).

#### THE NAMED-TEST CHECK, APPLIED AGAIN

958/0 would read identically whether the five new tests ran or did not exist, and a
`--quiet` run prints no passing names. So the boundary test was run explicitly on the
box:

```
entrance_themes::walk_on_ordering_tests::a_bare_bang_is_chat_but_an_unknown_command_is_still_a_command ... ok
```

Second module in two items named for the behaviour rather than the mechanism —
`restore_condition_tests`, now `walk_on_ordering_tests`. A reader who breaks one
learns from the module name what they broke.

---

## 2026-09-11 — item 7d merged (song-queue QOL). Sixth consecutive byte-identical refusal. The bot has never had a deploy.

| | |
|---|---|
| branch | `feature/song-queue-qol`, head `0583f3a` (6 commits) |
| merge | **`4dcd614`** onto `69d41d8` — **this is the head d's cutover package names** |
| local suite | **980 passed / 0 failed / 46 result-lines** |
| box suite | **980 passed / 0 failed / 46 result-lines** — identical to local |
| archive | sha256 `e72d2c69edd6df01…`, **verified identical at both ends** |
| live binary | **unchanged** — `4ff1adb96b1179d8cd52a4ff4ba94540cc17eb0dd1d66e038d00b17e25912de2` |
| deploy | **refused, correctly** — byte-identical (sixth consecutive) |
| service | untouched: active since 2026-09-10 16:50:04, `NRestarts` 0, no new backup slot |

### THE MERGE

Only `WIKI_IMPACT.md` conflicted — keep-both, per the append-only rule, 4 added lines.
`commands.rs` auto-merged, confirming d's prediction that its hunks do not overlap a's.
Verified both contributors' work survived rather than trusting the auto-merge:

| symbol | count in `bot/src/commands.rs` |
|---|---|
| a's `parse_command`/`is_command` (7c) | 4 |
| d's `YOUTUBE_API_KEYS` | 19 |
| d's `voteskip` | 7 |

**Correcting a figure I carried forward:** I had `voteskip` at 17 in `commands.rs`. It is
**7** there (40 across `bot/src`, the bulk of it in `song_requests.rs`). The conclusion —
both hunks present, no overlap — is unchanged.

### THE NAMED-TEST CHECK, THIRD ITEM RUNNING

980/0 cannot distinguish "the 22 new tests ran" from "they were never compiled in", and a
`--quiet` run prints no passing names. A grep for song/queue/voteskip names in the suite
log returned **nothing**, which looks like absence and is just `--quiet` again. So they
were run by name on the box:

```
playrandom::tests::the_play_log_is_capped_at_ten_thousand_newest_kept ... ok
playrandom::tests::an_oversized_log_is_brought_back_in_one_write ... ok
playrandom::tests::continuous_mode_stands_down_while_a_request_is_queued ... ok
song_requests::tests::region_restrictions_are_read_against_the_streams_own_region ... ok
song_requests::tests::one_vote_skips_a_song_nobody_requested ... ok
```

The first two are the play-log cap — the commit d added *after* naming `ef23176`, and the
reason the order said to take the head d names rather than the one quoted.

### FOUND — THE BOT HAS NEVER BEEN DEPLOYED BY ANYTHING

Answering d's question for the cutover package. **`deploy-linux.sh` has never built or
installed the bot, and neither has anything else.** `grep -Eic "bot|twitch"` on it: **0**.
`/opt/pathofdust/bin/` holds no bot binary; the box has no bot binary outside build trees,
no bot process, and zero systemd units mentioning one. The five Windows `.ps1` scripts
contain **0** `cargo build`. The live bot runs from
`C:\PathofDust\target\release\twitch-bot-rs.exe` — the **cargo output directory**, not an
install location. There is no install step to have skipped because there is no installed
copy.

**§13's conditional-bot-redeploy wording therefore describes a step nobody has ever run**
(§13B already says so outright: *"There is no bot on this box"*). It should be corrected,
not cited.

**Correcting my own record:** the 7a/7b/7/7c entries say the deploy "refused as
byte-identical". True of the *game* binary and of the script's output — but it must not be
read as "the bot was considered and found unchanged." **The script never considers the
bot.** Six refusals say nothing whatsoever about it.

The cutover package's step 2 is thus the **first bot deploy since Sep 2**. Measured gap:
**239 commits on master**, of which **17 touch `bot/`**.

### TWO INSTRUMENT ERRORS CAUGHT WHILE GATHERING THAT

Both would have entered the report as evidence against my own conclusion.

- `pgrep -c -f twitch-bot` on the box returned **1** — reading as "a bot process runs
  there." It was matching **its own command line**. `ps -eo pid,comm,args` shows none.
- `grep -Eic "bot|twitch" rollback-linux.sh` returned **1** — the word **BOTH** in a
  comment.

Same class as the `slots:`/`golem_slots:` and `max_log_files`-in-comments errors. A count
is not an observation until you have looked at what it counted.

### NOT DONE, DELIBERATELY

**The cutover was not started.** The order grants it "on the owner's word"; no go appears
in the order file. Step 2 onward stays pending.

From item 8 the game binary changes again and real deploys resume.

---

## 2026-09-11 — item 8 merged (`docs/pacing-board-rulings`). Seventh refusal, recorded as its own result. FOUND: #86's present-tense claim is false today.

| | |
|---|---|
| branch | `docs/pacing-board-rulings` `c7a672a` — **remote head matched the order, no drift** |
| merge | **`e53872c`**, clean, no conflict |
| content | docs-only: `docs/anomaly_ledger.md`, **272 insertions, 0 deletions, 0 modified lines** |
| local suite | **980 / 0 / 46** (`--no-fail-fast`) |
| box suite | **980 / 0 / 46** — identical |
| archive | sha256 `bf68df5587c644b7…`, identical both ends |
| deploy | **refused, byte-identical — seventh consecutive** |
| service | untouched: active since 2026-09-10 16:50:04, `NRestarts` 0, no new slot |

### THE DOCS-ONLY CLAIM, PROVED AGAINST THE TREE RATHER THAN THE DIFF

`git diff --name-only` says docs-only, but that trusts the diff. The extracted tree was
compared directly against the previous release's tree on the box:

```
files under game/src differing from the 7d tree: 0
files under bot/src  differing from the 7d tree: 0
```

Zero and zero, so the binary could not move, and the refusal was predicted with certainty
before the gate ran. 980 unchanged from 7d confirms it from the other side.

### THE SUITE ABORTED, AND THE COUNT SAID SO

The first run returned `passed=883 failed=1` with **`result-lines=1`**. Without
`--no-fail-fast` cargo stops at the first failing binary, so that is **not** "one test
failed out of 883" — it is *one test binary ran at all*. The result-line count is what
distinguishes those, which is exactly why the rule exists.

The failure was
`adventure_web::render::live_reload_tests::editing_a_template_takes_effect_without_a_rebuild`
— one of the three CLAUDE.md names as flaky under parallel, and the rule says confirm in
isolation before flagging. In isolation, single-threaded: **ok, 0.05 s**. The rerun with
`--no-fail-fast` came back **980 / 0 / 46** and the flake did not reproduce.

A docs-only merge cannot break a template-reload test. Had I reported the first run's
883/1 as this branch's result, it would have read as a regression caused by item 8.

### FOUND — #86 SAYS "NOT SATURATED TODAY"; TODAY IT IS SATURATED

Entry `#86`, shipped by this merge, states Controller A "moved to **11.88 of 50**, i.e.
not saturated today." Read live from `/var/lib/pathofdust` on 2026-09-11 18:30:

| dial | live |
|---|---|
| `hp_pacing_mult` | **50.0** |
| `hp_multiplier_ceiling` | **50.0** |

A is pinned **at** the ceiling. `boss_power_mult` 3.657; `enemy_hp_pool_hard_cap` 1e15.

**I am not claiming the entry was wrong when written.** The pre-deploy snapshots carry only
the binary, the fight-summary tier and `SHA256SUMS` — no world state — so 2026-09-08's
value cannot be recovered, and reconstructing it would be construction. What is established
is only this: **the sentence is false as of today.**

The irony is the entry's own subject. `#86` closes a board item that "read as unactionable
for a week" because a present-tense claim went stale and nobody updated it — and `#86`
carries a present-tense claim of its own. *A dated measurement stays true; "today" does not.*

Under the append-only rule this is **not mine to edit**: ledger numbering is the parser's,
`#86` is `b`'s, and a correction is a new dated entry, never an overwrite. Recorded here and
in the report for whichever session owns it.

### THE REFUSAL, AS ITS OWN RESULT

The order asked that item 8's refusal be recorded as its own result rather than as the
streak continuing. It is: **item 8 is docs-only and the binary is provably unchanged**, a
different fact from items 7/7a–7d being bot-only. Seven refusals, three distinct causes.
From item 9 the game binary changes and real deploys resume.
