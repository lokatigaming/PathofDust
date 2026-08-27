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
