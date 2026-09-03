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
