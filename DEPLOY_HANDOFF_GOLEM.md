# Deploy handoff — ledger #35/#36, Thunder Golem reform-cleanse fix

Written by the fix session for the deploy session. Owner was asleep at
push time — no live go-ahead was sought for this handoff note itself
(the fix and observability work were both explicitly ordered). **Do not
merge or deploy from this file alone** — this is a handoff, not a merge
authorization; follow the standing deploy procedure/checklist as normal.

## Branch / commit

- Branch: `fix/thunder-reform-degradation`
- Pushed to origin, tracking `origin/fix/thunder-reform-degradation`
- Final commit: `81dcf4ded9cd8367cd374c2385f0f5b9af9d3c0f` (`81dcf4d`)
- Branched off `origin/master` at `2585c73234e4c1feaf4870b4928135228a52a8b3`
- Own worktree: `C:\PathofDust-thunderreform`, own `--target-dir target-thunderreform` — main checkout was never touched.

## What shipped (one commit, two parts)

**Part 1 — bugfix (ledger #35 root cause + fix).** Owner-reported: Thunder
Golems get WEAKER with every kill instead of stronger, despite Growing's
33/66/100%-per-reform max_hp bonus. Root-caused by full reform-lifecycle
trace, not assumed:

- Growing's own max_hp math was checked line-by-line and confirmed
  **correct** — additive off the golem's original spawn-time base, not
  compounding onto the already-grown value (deliberately fixed that way
  2026-08-19; re-verified here). Monotonically increasing every reform,
  proven with arithmetic in the fit report. **Not the bug.**
- The real bug: `reform_thunder_golem` (`game/src/adventure/combat.rs`)
  only ever reset `max_hp`/`hp`/`alive`/`golem_reform_at_ms`/
  `thundergolem_absorbed_this_incarnation` on reform. Every OTHER combat
  debuff a dying incarnation was carrying rode straight into the fresh
  incarnation, because reform mutates the same unit in place rather than
  rebuilding it. Concretely: Gelatinous Cube's shred (`cube_shred_stacks`,
  up to -50% DR, 3000ms lazy-expiry) and Festering Wound (`wound_stacks`).
  At Growing rank 2/3 the reform delay (3000ms/2000ms) is at-or-below that
  3000ms decay window, so a golem that died mid-shred could reform still
  under -50% DR before taking a single hit as the "bigger, cleansed"
  incarnation Growing's own formula promises.
- Fix: `reform_thunder_golem` now calls the existing, already-audited
  `cleanse_player_debuffs` (the same function Cleansing Flames uses) at
  the top of the function — one line, no new debuff-tracking logic.
  `boss_focus_stacks` is also cleared by this call even though it was
  separately proven to already self-reset via the reform gap's own
  forced target-switch (belt-and-suspenders, costs nothing).
- **Known open question, deliberately not resolved by this branch (per
  owner ruling):** outside Gelatinous Cube fights, no code-level
  mechanism was found that would make later incarnations weaker than
  earlier ones — Growing's math and the debuff-carryover fix should fully
  explain the complaint. If live data (via Part 2's new counters) shows
  incarnation lifespans still shrinking boss-agnostically once this ships,
  ledger #35 reopens with that constraint. If the pattern was Cube-bound
  and dies with this fix, #35 closes. **This is the parser session's call
  to make once live data exists — nothing further to investigate here
  first.**

**Part 2 — observability (ledger #35/#36).** `CombatUnitInfo` gains a new
field `thunder_incarnations: Vec<ThunderIncarnationInfo>`
(`game/src/adventure/manager.rs`) — one entry per Thunder Golem
incarnation this fight (oldest first), each carrying `absorbed` /
`redistributed` / `maxHp` / `lifespanMs`. The still-alive-at-fight-end
incarnation (if any) is included too, appended by the fight-end
`unit_infos` builder in `combat.rs`, with `redistributed: 0` (nothing
redistributed away from it yet — expected, not a bug, same "DoT armed
near fight end may never deliver" confound #36 already flagged).
Purely additive: `#[serde(default)]`, empty for every non-Thunder-Golem
unit and every pre-existing fight record on disk. This closes the
tank-credit observability gap #35/#36 were blocked on — the parser can
now measure per-incarnation absorbed/redistributed/max_hp/lifespan
directly from fight JSON instead of reconstructing incarnation
boundaries from raw event timestamps (which was ambiguous once a
redistribution's own "still owed" merge across deaths blends two
incarnations' ticks together).

## Patch note (player-facing, verbatim as approved)

> Thunder Golems now reform fully cleansed — the Gelatinous Cube's armor
> shred and other lingering debuffs no longer carry into a fresh
> incarnation. Reformed golems were coming back pre-weakened; now they
> come back clean and bigger, as designed.

This is a nerf-free bugfix (removes an unintended weakness, doesn't
reduce anything), worded honestly per house patch-notes doctrine — no
nerf language needed since there isn't one.

## Golden-corpus / fixture divergences — expected, attributed, NOT regenerated

Per BRANCH DISCIPLINE this branch does not regenerate fixtures —
regeneration happens at merge. Two test failures on this branch, both
fully attributed and inspected (diffed existing-vs-fresh JSON directly,
not assumed):

**1. `golden_corpus::golden_corpus_matches_committed_fixtures`** — all
**17 of 17** defined scenarios diverge (the full corpus). Diffed
`elementalist_thunder_golems_vs_dragon_stage1000` (the one golem
scenario) field-by-field against a fresh run. Every diff line is one of
exactly two causes, nothing else:
- `hitId`/`eventId`/roll `eventId` values shifted by a constant offset
  throughout (e.g. `4332` vs `3736`) — a pure artifact of this branch's
  new unit tests running earlier in the same test binary and consuming
  more of the shared global hit-id/event-id counter before this
  scenario's own fight simulates. **No damage/crit/evasion/targetHpAfter
  value differs anywhere in the diff** — combat outcomes are byte-
  identical. This is a pre-existing fragility of the counter scheme, not
  something this branch's logic changed.
- `units[].thunderIncarnations` appears where it didn't before (e.g.
  `undefined vs []` for non-golem units, `undefined vs
  [{"absorbed":3108,"lifespanMs":2185,"maxHp":5386,"redistributed":0}]`
  for the golem slots) — the new Part 2 wire field, working as designed
  and producing real, sane per-slot data.
- The `cleanse_player_debuffs` fix produced **zero** observable diff on
  this scenario specifically — expected, since it fights a Dragon, not
  Gelatinous Cube, so nothing was ever carried across its reforms to
  clean up. Confirms the fix is behavior-neutral outside Cube fights, as
  designed.
- Did not diff the other 16 non-golem scenarios individually — by
  construction they can only be hit by the same two causes above (no
  golem present to touch `reform_thunder_golem` at all, and every unit
  still gets the new wire field).

**2. `replay_bundle::writer_golden::the_writer_still_produces_the_committed_golden_bundle`**
— same root cause as above, narrower blast radius: only the `core`
bundle member's `bytes`/`sha256` changed (816 vs 791 bytes) — the member
that embeds `CombatUnitInfo`. `buffs`/`dot`/`playerVitals`/`replay`/
`rolls` members are byte-identical. Confirms the divergence is scoped
exactly to the new wire field, nothing else in the bundle format moved.

**Recommendation for merge:** regenerate both fixture sets at merge time
(delete + rerun per each module's own documented convention) rather than
treating either failure as a regression to chase.

## Test counts

Full `cargo test --release --workspace --quiet`: **577 passed, 2 failed**
(both above, attributed). New tests added this branch (all passing):
- `reforming_clears_cube_shred_wound_and_boss_focus_stacks_from_the_dead_incarnation`
  — pins the actual fix.
- `handle_golem_death_records_this_incarnations_absorbed_redistributed_max_hp_and_lifespan`
- `handle_golem_death_records_zero_redistributed_when_no_real_player_is_alive_to_receive_it`
- `successive_reforms_each_push_their_own_incarnation_record_with_correct_lifespans`
- `thunder_incarnations_sum_to_absorbed_minus_redistributed_matching_net_absorbed` —
  full `simulate_battle` round-trip, also asserts each successive
  incarnation's `maxHp` is strictly increasing (Growing's own real-fight
  behavior, not just the isolated formula test).
- Existing Growing-math tests (`golem_reform_growing_is_additive_not_compounding_across_many_reforms`,
  `full_sizing_formula_gigantify_then_growing_reforms_matches_the_documented_math`)
  kept unchanged as the lock — still passing, confirming this fix didn't
  touch the sizing formula at all.

`cargo clippy --release --workspace`: clean on every line this branch
touched (checked specifically, not just "no new warnings in the whole
build" — the whole build has several pre-existing warnings in unrelated
files, none touched by this branch).

## Warnings for the deployer

- Both fixture failures above are **expected and required** — if either
  is somehow already clean at merge time (e.g. someone else regenerated
  them first), that's fine, but if a merge attempt "fixes" them by
  reverting the `thunder_incarnations` field or the `cleanse_player_debuffs`
  call instead of regenerating, that's a real regression — don't do that.
- This branch never merged to master, never deployed, never regenerated
  fixtures — all per house rules. Merge, fixture regeneration, and deploy
  are the deploy session's own steps from here.
- No player-facing balance, cost, chance, timer, or formula changed
  besides the described debuff-carryover fix — WIKI_IMPACT.md has both
  entries appended (bugfix + the new observability field, the latter
  flagged not-player-facing for completeness).
