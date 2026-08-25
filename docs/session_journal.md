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
