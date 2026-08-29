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
CWD-relative by the owner's explicit ruling — see addendum A1 in the audit,
which records the provisioning trap that decision creates). Full detail in
the audit addendum rather than repeated here.

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
