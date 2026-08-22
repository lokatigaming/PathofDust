# Dynamic Pacing Spec (2026-08-22, branch `feature/dynamic-pacing`)

Two independent feedback controllers replace the old win-margin boss
rubber-band (`post_win_power_boost`, `adaptive_difficulty_scale`,
`LOSS_POWER_DECAY`, `WIN_MAX_BOOST`, `TARGET_WIN_RATE`,
`WIN_TARGET_MARGIN_RATIO`, `OUTCOME_WINDOW`, and the live dial
`dynamic_scaling_mult` - all removed). Pure math lives in
`game/src/adventure/pacing.rs`; wiring lives in `manager.rs`
(`run_encounter_inner` / `run_basic_encounter_inner`) and
`adventure_web.rs` (admin page).

## The independence doctrine

**HP answers "how long". Damage answers "how hard". Neither touches the
other's variable.**

| | Controller A | Controller B |
|---|---|---|
| Axis | duration (real clock) | lethality |
| Owns | `WorldState::hp_pacing_mult`, `recent_win_dps` | `WorldState::boss_power_mult` |
| Reads | winning fights' DPS samples | boss win/loss outcomes |
| Target | fight duration inside [min,max] s (midpoint) | rolling W:L = `target_win_loss_ratio` (2.0) |
| Rate limit | `hp_max_step_per_fight` (relative band vs prev) | `dmg_max_step_per_fight` |
| Bounds | `hp_multiplier_floor/ceiling` + hard caps | `dmg_multiplier_floor/ceiling` + hard caps |

No arbitration, no priority, no alternation - the per-fight rate limits
are the ONLY damping. **Known limit:** duration and lethality remain
coupled in OUTCOME (a longer fight is also more total damage taken), even
though their variables are separate. If live behavior oscillates,
arbitration between the controllers is the known next step.

Application split (`apply_dynamic_scaling`): A's effective multiplier
touches ONLY the HP pool; B's touches ATK fully and every secondary stat
(DR/block/evasion/crit/increased-damage/splash) at dampened sqrt(B),
each under its existing cap - exactly the old coupling shape, now sourced
solely from the damage axis. Per-enemy relative HP weights are never
touched by either controller (the pool scales; `split_into_enemies`' even
cut happens after scaling).

## Owner rulings implemented

1. **Wins-only sampling (A).** Lost fights carry no meaningful duration
   signal; a wipe would read as a short fight, inflate HP, and cause more
   wipes (death spiral). `push_dps_sample` gates on `won`; losses never
   reach the window.
2. **Stage progression IS Controller B's mechanism.** Win = +1 stage;
   LOSS = **-2** (floored at 1; was -1 before this release -
   `announcements.rs::next_boss_stage`'s batch-replay already modeled -2,
   and reality now matches it). Therefore exactly 2 wins : 1 loss is
   neutral progression: the party climbs only while beating B's target
   ratio. Verified nothing else writes or floors the stage.
3. **Per-stage baseline floor, hand-authored.**
   `baseline_stage_anchors` / `baseline_hp_anchors` /
   `baseline_atk_anchors` define a minimum effective difficulty as a
   FRACTION of the organic stage/level/party curve (linear interp, flat
   after the last anchor). At generation:
   `effective = max(controller_value, baseline(stage))` per axis. The
   controllers scale RELATIVE to the organic curve and can NEVER take
   effective difficulty below the baseline; `*_multiplier_floor` bound
   only how far below 1.0 each controller may drift on its own. The
   anchors are deliberately NOT derived from live player gear - that
   would be circular and the floor could never bind. Malformed anchor
   lists read as neutral (baseline = organic curve), so a bad edit can
   loosen the floor but never corrupt difficulty.
4. **Top-layer mitigation, tied to STAGE (not to A).** A final ABSOLUTE
   damage reduction on every enemy (`CombatSimUnit::top_layer_mitigation`,
   stamped from `pacing::top_layer_for_stage(stage)` at construction incl.
   Lich adds), applied at the very END of every enemy-targeted damage
   resolution - after crit/increased-damage/pierce-split/all mitigation,
   before shields. NOTHING bypasses it: no armor pen, no "ignores DR", no
   true-damage exemption, no Environmental-tag exemption. Structurally
   separate from `damage_reduction`: NOT routed through
   `combine_reduction_sources`, NOT bounded by `defensive_stat_hard_cap`.
   Execute-style threshold deaths (Ashes sweep, Culling Strike) interact
   with NO mitigation at all by design and so bypass trivially. Curve =
   asymptote reaching half of cap at `top_layer_half_stage` (default
   1500 -> ~30%; ~41% at stage 3222); ceiling double-clamped strictly
   below 1.0 (`top_layer_cap_pct` clamped into
   `[0, TOP_LAYER_ABSOLUTE_CAP=0.95]`). Why: raises effective
   survivability WITHOUT inflating raw HP, keeping HP-keyed mechanics
   sane - Shattering icicles key off the dead enemy's max_hp and Ashes to
   Ashes' cull thresholds are absolute numbers.

5. **Kill-switch OFF** (`dynamic_pacing_enabled=false`): both controllers
   completely inert (no sampling, no updates); multipliers freeze where
   they sit; generation passes through with NO baseline max(); the old
   margin-ratchet does NOT return. Baseline/top-layer are separate
   systems with their own switches.
6. **Saturation must be visible.** When either controller sits BELOW its
   stage baseline (party performing under baseline; the floor doing the
   work), the admin page says so explicitly ("PINNED AT BASELINE FLOOR")
   instead of silently pinning.

## Numeric-limit safety

Types on every scaled path: `BossStats.hp/atk: u64`,
`CombatSimUnit.hp: i64 / max_hp: u64 / atk: u64`, event amounts u64,
scaling arithmetic f64. Guards, all BEFORE any cast:

- non-finite tunables substitute shipped defaults (`finite_or`;
  NaN must never reach a float->int cast - Rust maps NaN to 0, not
  saturate);
- clamp chain per update: rate-limit band `[prev/(1+step), prev*(1+step)]`
  THEN `[floor, ceiling]` THEN hard `[MULT_HARD_FLOOR=0.05,
  DYNAMIC_MULT_HARD_CEILING=1e6]`;
- HP pool hard cap `ENEMY_HP_POOL_HARD_CAP = 1e15` applied to A's
  composed multiplier as `min(mult, CAP/base_pool)` at generation
  (below f64's exact-integer bound 2^53 ~ 9.0e15, far below u64::MAX);
- saturating rounding via `sat_round_stat` (finite-checked last line;
  non-finite collapses to 1, never 0-stat, never wrap);
- DPS samples finite-only at the sampler; u64 sums saturating.

## Warmup, windows, asymmetry

Both controllers share `pacing_window_fights` (clamped 1..=200) and make
no updates until a FULL window exists (samples still collect). B consumes
BOSS outcomes only - basic encounters deliberately record no outcomes
(pre-existing design, owner-confirmed). A samples EVERY fight's winners
(boss and filler). This asymmetry is deliberate.

## Couplings that shift (balance notes)

Shattering icicles scale linearly with enemy pools (strongest A coupling);
Ashes to Ashes' absolute cull crosses later against inflated pools;
Thunder Golem absorption fills faster under B; healing/shield economy
tightens under both; Culling Strike's relative threshold point is
unchanged (more procs in longer fights); Cthulhu's ability magnitude now
follows B ONLY (`boss_dynamic_power_mult`). Loot/pity/XP/dust untouched.

## Golden corpus

Fight-generation changes do NOT touch it (hand-authored stats). ADDITION
4 changes combat RESOLUTION, so corpus fixtures WILL diverge - expected,
attributed to the top layer (enemy-side final damage multiplied by
`(1 - top_layer_for_stage(stage))` on every delivery path). Regeneration
happens at merge per house rules.

## Admin surface

/admin/tunables gains a Dynamic Pacing section (kill-switch, window, both
controllers' knobs, three CSV anchor inputs, both override rows with
current-value labels and pinned warnings) and a Top-Layer section.
`dynamic_scaling_mult`'s row is RETIRED (field kept for TOML/save
compatibility; a save preserves the stored value).


