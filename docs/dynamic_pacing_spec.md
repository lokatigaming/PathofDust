# Dynamic Pacing Spec (rewritten 2026-08-23, branch `feature/dynamic-pacing`)

Two independent feedback controllers replace the old win-margin boss
rubber-band (`post_win_power_boost`, `adaptive_difficulty_scale`,
`LOSS_POWER_DECAY`, `WIN_MAX_BOOST`, `TARGET_WIN_RATE`,
`WIN_TARGET_MARGIN_RATIO`, `OUTCOME_WINDOW`, and the live dial
`dynamic_scaling_mult` - all removed). Pure math lives in
`game/src/adventure/pacing.rs`; wiring lives in `manager.rs`
(`run_encounter_inner` / `run_basic_encounter_inner`) and
`adventure_web.rs` (admin page).

**This document was rewritten against the shipped code on 2026-08-23.**
The previous revision described a symmetric clamp chain and a
kill-switch that also disabled the baseline floor; neither matched the
implementation, and the owner ratified the code. Where this file and the
code disagree, the code is right and this file is the bug.

## THREE INDEPENDENT SYSTEMS

They are not one feature with one switch. Each answers a different
question, each has its own switch, and no switch reaches across:

| System | Question | Switch | Shipped default |
|---|---|---|---|
| Controllers A + B | "how long" / "how hard", adapting to this party | `dynamic_pacing_enabled` | **`true`** (the feature ships ON) |
| Top layer (ADDITION 4) | how tough enemies are at this STAGE | `top_layer_enabled` | `true` |
| Baseline floor | the minimum difficulty the owner authored | **none, by ruling** | always on |

**Baseline and top layer are separate systems with their own switches,
unaffected by the controller kill-switch.** Turning the controllers off
does not restore the old margin-ratchet, does not disable the stage-tied
top layer, and does not remove the baseline floor from generation.

The baseline floor gets **no switch at all**. It is hand-authored
content, and its escape hatch is the content itself: a malformed or
emptied anchor table reads as neutral (baseline = the organic curve), so
an owner who wants it gone empties it. That is deliberate - a bad edit
may loosen the floor, never corrupt difficulty.

## The independence doctrine

**HP answers "how long". Damage answers "how hard". Neither touches the
other's variable.**

| | Controller A | Controller B |
|---|---|---|
| Axis | duration (real clock) | lethality |
| Owns | `WorldState::hp_pacing_mult`, `recent_win_dps` | `WorldState::boss_power_mult`, `recent_boss_outcomes` |
| Reads | WON BOSS encounters' DPS samples | BOSS win/loss outcomes |
| Target | fight duration inside [min,max] s (midpoint) | rolling W:L = `target_win_loss_ratio` (2.0) |
| Rate limit | `hp_max_step_per_fight` (UPWARD only) | `dmg_max_step_per_fight` (UPWARD only) |
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

## The clamp chain (`clamp_rate_limited`) - THREE ASYMMETRIES

Each is load-bearing. The old spec described a symmetric band and was
wrong on all three counts.

1. **The band is UPWARD-ONLY: escalation is damped, relief is
   immediate.** An upward move is limited to `prev * (1 + step)` per
   fight. A downward move is not limited at all - the configured floor is
   reachable the fight the controller asks for it. The death spiral this
   module exists to prevent is built out of difficulty that ratchets up
   faster than the party can answer; making a fight easier at once has
   never been the dangerous direction.

2. **The structural hard caps bind IMMEDIATELY and are never approached
   gradually.** The band damps movement inside the owner-configured
   `[floor, ceiling]` window. When a configured bound lies beyond
   `MULT_HARD_FLOOR` (0.05) / `DYNAMIC_MULT_HARD_CEILING` (1e6), the
   owner has said "no bound on this side" and the structural cap is the
   only thing left - a safety cap is not a balance knob, so it applies at
   once instead of after N fights of climbing toward it.

3. **The operating window never slams `prev`, but it does CONVERGE**
   (revised 2026-08-23, branch `fix/pacing-controller-loop`). A stored
   multiplier already outside its configured window (a dashboard edit
   tightened the range underneath it, or an older save) is still never
   yanked mid-flight - but the widening that admits it now SHRINKS by one
   rate-limited step per fight, unconditionally, until it is gone.

   It was previously permanent (`lo = cfg_lo.min(prev)`,
   `hi = cfg_hi.max(prev)`), which made a configured bound **advisory**:
   the value could only come back if the controller happened to request
   that direction by itself, so an operator who lowered a ceiling to rein
   in a runaway got neither effect nor feedback. The step cap preserves
   the no-slam property; only the hard caps still bind instantly. A
   `*_max_step_per_fight` of 0 means the controller may not move at all,
   so the widening correspondingly cannot close.

## Owner rulings implemented

1. **Wins-only sampling (A), boss-only sampling (both).** Lost fights
   carry no meaningful duration signal; a wipe would read as a short
   fight, inflate HP, and cause more wipes (death spiral).
   `push_dps_sample` gates on `won`; losses never reach the window. Since
   2026-08-23 neither controller samples a non-boss fight either - see
   SAMPLING: BOSS ENCOUNTERS ONLY.
2. **Stage progression IS Controller B's mechanism.** Win = +1 stage;
   LOSS = **-2** (floored at 1; was -1 before this release -
   `announcements.rs::next_boss_stage`'s batch-replay already modeled -2,
   and reality now matches it). Therefore exactly 2 wins : 1 loss is
   neutral progression: the party climbs only while beating B's target
   ratio. The stage walk is NOT gated by the kill-switch - progression is
   the game's, not the controller's.
3. **Per-stage baseline floor, hand-authored.**
   `baseline_stage_anchors` / `baseline_hp_anchors` /
   `baseline_atk_anchors` define a minimum effective difficulty as a
   FRACTION of the organic stage/level/party curve (linear interp, flat
   after the last anchor). At generation:
   `effective = max(controller_value, baseline(stage))` per axis, ALWAYS
   - see the three-systems table. The controllers scale RELATIVE to the
   organic curve and can NEVER take effective difficulty below the
   baseline; `*_multiplier_floor` bound only how far below 1.0 each
   controller may drift on its own. The anchors are deliberately NOT
   derived from live player gear - that would be circular and the floor
   could never bind.
   **The three lists are ONE table and validate together**: stage, HP and
   ATK read across as columns, so if any list disagrees in length the
   table is malformed and BOTH axes read neutral. A half-edited table
   must never floor one axis against a stage column it no longer lines up
   with.
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
   asymptote reaching half of the tunable cap at `top_layer_half_stage`
   (default 1500 -> ~30%; ~41% at stage 3222). The tunable cap shapes the
   CURVE; the RESULT is clamped into `[0, TOP_LAYER_ABSOLUTE_CAP=0.95]`,
   so an owner who dials the cap past the structural limit gets exactly
   the structural limit rather than the limit minus the ramp's own
   asymptotic deficit. Strictly below 1.0 no matter what the dashboard
   says. Why: raises effective survivability WITHOUT inflating raw HP,
   keeping HP-keyed mechanics sane - Shattering icicles key off the dead
   enemy's max_hp and Ashes to Ashes' cull thresholds are absolute
   numbers.
5. **Kill-switch OFF** (`dynamic_pacing_enabled=false`, not the shipped
   default): both controllers completely inert - **no sampling and no
   updates**. Neither A's DPS window nor B's boss-outcome window records
   anything while disabled, because a controller that kept filling its
   window would come back with a full history of fights it never governed
   and step off it immediately on re-enabling. Multipliers freeze where
   they sit; the old margin-ratchet does NOT return. Baseline and top
   layer are separate systems with their own switches, unaffected by this
   switch.
6. **Saturation must be visible.** When either controller sits BELOW its
   stage baseline (party performing under baseline; the floor doing the
   work), the admin page says so explicitly ("PINNED AT BASELINE FLOOR")
   instead of silently pinning, and prints the multiplier actually **in
   force** (`max(controller, baseline)`) beside the controller's own
   value, so no operator has to do the max() in their head.

## Numeric-limit safety

Types on every scaled path: `BossStats.hp/atk: u64`,
`CombatSimUnit.hp: i64 / max_hp: u64 / atk: u64`, event amounts u64,
scaling arithmetic f64. Guards, all BEFORE any cast:

- non-finite tunables substitute shipped defaults (`finite_or`;
  NaN must never reach a float->int cast - Rust maps NaN to 0, not
  saturate);
- **a zero-valued integer dial is treated as UNSET and substitutes its
  shipped default**, exactly as `finite_or` does on the float axes.
  `pacing_window_fights = 0` means "cleared", not "a one-fight window" -
  clamping it to 1 would turn a cleared field into the twitchiest
  possible setting;
- clamp chain per update: upward rate-limit band, THEN the configured
  `[floor, ceiling]` window widened to include `prev`, THEN the hard
  `[MULT_HARD_FLOOR=0.05, DYNAMIC_MULT_HARD_CEILING=1e6]` - see THE CLAMP
  CHAIN above for which of these bind at once and which are walked to;
- HP pool hard cap `ENEMY_HP_POOL_HARD_CAP = 1e15` applied to A's
  composed multiplier as `min(mult, CAP/base_pool)` at generation
  (below f64's exact-integer bound 2^53 ~ 9.0e15, far below u64::MAX); a
  non-finite pool makes the cap uncomputable, so the multiplier falls
  back to neutral 1.0 rather than scaling an already-broken number;
- an OVERFLOWING DPS window is a saturation signal, not a broken
  reading: it travels as `+inf` into the clamp chain and resolves against
  the hard caps. Only a window with no usable signal at all (every sample
  dropped, or a `<= 0`/NaN mean) skips the update;
- saturating rounding via `sat_round_stat` (finite-checked last line;
  non-finite collapses to 1, never 0-stat, never wrap);
- DPS samples finite-only at the sampler; u64 sums saturating.

### Mitigation arithmetic: subtract, never multiply by a complement

Every mitigation multiply is written as **`damage - damage * fraction`**,
never as `damage * (1.0 - fraction)` with a precomputed complement.
`1.0 - 0.95` is not exactly representable in f64, and the residue is not
theoretical: at the 0.95 cap a 10,000 hit came out as
`500.00000000000045`, and that number reaches player-visible damage
numbers and fight logs. The subtraction form is exact at the cap and
identical everywhere else. `apply_top_layer_to` is the reference
implementation; any new mitigation layer copies its shape.

## Operating context: permanent rampage is NORMAL

`permanent_rampage = true` is live in production and is the **expected
steady state**, not a temporary test setting - players vote it on
constantly. While it is active, `spawn_rampage_loop` is the sole driver
of encounters: boss fights run back-to-back with instant revives (the
downed timer is skipped entirely rather than inserted-and-expired), and
both `spawn_encounter_loop` and `spawn_basic_encounter_loop` skip their
ticks. Non-boss fights are interim filler that exists only to slow the
game down when nobody is pushing for a rampage.

Design that follows from it: pacing must be tuned for a world where
almost every fight is a boss fight, and where a wipe is followed
immediately by another boss fight rather than by a 30-second sit-out.

## Sampling: BOSS ENCOUNTERS ONLY, both controllers

Owner ruling 2026-08-23, replacing the original "A samples every fight's
winners" asymmetry. **Neither controller samples a non-boss fight.**

- Controller A's sampler is called from ONE place: the boss path
  (`run_encounter_inner`). `run_basic_encounter_inner` computes no sample
  at all.
- Controller B's outcome window is pushed from that same single place.
- Both multipliers and the baseline floor are still APPLIED to filler
  generation. Only the feedback is boss-only.

Why filler is excluded: its enemy pools come from
`basic_enemy_stats_for`, a different curve from `boss_stats_for`. A
filler DPS sample would steer the HP multiplier that governs BOSS pools
using a measurement taken against something else - and under the normal
operating mode above, filler barely runs anyway.

**Losses reach B, never A.** A loss is an outcome (B needs it; that is
the ratio's whole point) but never a duration sample - the wins-only rule
exists precisely to keep wipes out of the duration average, since a wipe
reads as a short fight and would inflate HP into more wipes. An instant
revive is not a back door around this: a wipe is simply `won == false` at
the single sample site, and reviving does not re-run the encounter or
push a second outcome.

**Duration is in-fight time only.** `real_duration_ms` is
`max(event.at_ms)` over the fight's own event list, taken BEFORE
`compress_events` (so it is the real clock, not the display window).
`simulate_battle` reads no wall clock at all - its timeline starts at 0
each call and advances only through in-fight scheduling - so
inter-encounter time (the rampage loop's sleep, the `fight_gate` spacing,
overlay playback, revive waits) can never appear in a sample.

## Presentation clamps are SUBORDINATE to the pacing window

**Rule: no presentation clamp may bind inside Controller A's operating
range.** Simulated duration and player-experienced duration are the same
number inside `[target_duration_min_s, target_duration_max_s]`, by
construction - `compress_events`' scale is exactly 1.0 there and no
event's timestamp moves.

The upper display bound is therefore **derived, never authored**:
`combat::display_upper_bound_ms(&tunables)` reads
`target_duration_max_s`. Widen the pacing window and the display bound
widens with it. Do not replace it with a constant.

**What the failure looked like** (found 2026-08-23, fixed same day - this
paragraph exists so nobody re-hardcodes it):

`MAX_DISPLAY_MS` was a flat 35,000 ms, chosen as a presentation
preference ("a 90-second slugfest should still read well"). Controller A
targets 30-45 s. Everything from 35 s upward therefore rendered as
*exactly* 35 s of screen time:

| Real duration (A's axis) | Battle watched | Total on screen |
|---|---|---|
| 30.0 s | 30.0 s | 32.5 s |
| 37.5 s (A's midpoint target) | 35.0 s | 37.5 s |
| 45.0 s (window ceiling) | 35.0 s | 37.5 s |
| 90.0 s | 35.0 s | 37.5 s |

So A's top 10 seconds were invisible work: it inflated boss HP pools by
up to ~29% to move duration from 35 s toward 37.5 s, and no player could
see any of it as duration - only as tankier bosses. The trap that hid it:
`CHARGE_MS + 35,000 + RESOLVE_MS` = `700 + 35,000 + 1,800` = 37,500, so
"target 37.5 s, 37.5 s on screen" looks correct at exactly the midpoint.
That identity holds for *every* fight of 35 s or longer, because the
clamp has already saturated.

**The one hard bound left protects CADENCE, not readability.**
`PLAYBACK_CADENCE_CEILING_MS` = 52,500 ms, itself derived:
`RAMPAGE_MIN_INTERVAL_MS - OVERLAY_CHARGE_MS - OVERLAY_RESOLVE_MS -
FIGHT_GATE_MARGIN_MS` (60,000 - 700 - 1,800 - 5,000). After a fight
resolves at T the rampage loop sleeps `max(charge + display + resolve,
RAMPAGE_MIN_INTERVAL)` while the fight gate independently holds the next
fight until `T + charge + display + resolve + margin`, so the next
encounter starts at `T + max(60 s, charge + display + resolve + margin)`
- the 60 s design interval survives exactly while display <= 52.5 s. Past
that, every encounter stretches the interval it was meant to fit inside.
If `target_duration_max_s` is ever set above 52.5 s the cadence ceiling
wins (stacking encounters is a functional failure; invisible steering is
a fidelity one) and the correct fix is to raise `RAMPAGE_MIN_INTERVAL`,
which the ceiling derives from.

`MIN_DISPLAY_MS` (6,000 ms) is unchanged and cannot conflict with A: it
is a readability floor for short filler fights, and A's own floor sits
five times above it. It binds only below 6 s, where fights are stretched.

## Warmup and windows

Both controllers share `pacing_window_fights` (sanitized: 0 reads as
unset -> shipped default 20, then clamped 1..=200) and make no updates
until a FULL window exists (samples still collect while enabled). The
caller owns the window: it trims each history to `pacing_window_fights`
as it pushes, and the update functions read the whole slice they are
handed - the length check inside them is the warmup gate, not a second
trim. Encounters are serialized by `fight_gate`, so one encounter is
always exactly one outcome push and at most one duration sample.

## Couplings that shift (balance notes)

Shattering icicles scale linearly with enemy pools (strongest A coupling);
Ashes to Ashes' absolute cull crosses later against inflated pools;
Thunder Golem absorption fills faster under B; healing/shield economy
tightens under both; Culling Strike's relative threshold point is
unchanged (more procs in longer fights); Cthulhu's ability magnitude now
follows B ONLY (`boss_dynamic_power_mult`). Loot/pity/XP/dust untouched.

## Golden corpus

Fight-generation changes do NOT touch it (hand-authored `BossStats`,
`boss_dynamic_power_mult` fixed at 1.0, no `WorldState`) - so neither
controller can move a corpus scenario in either switch position.
ADDITION 4 changes combat RESOLUTION, so corpus fixtures **diverge by
design**: enemy-side final damage is multiplied by
`(1 - top_layer_for_stage(stage))` on every delivery path.

**14 scenarios were expected to diverge as of this branch** (verified
2026-08-23: with `top_layer_enabled = false` the corpus matches its
committed fixtures exactly, in both kill-switch positions; with the top
layer on, the same 14 diverge in both positions). Example:
`warrior_vs_lich_stage50`, layer 0.01935484 at stage 50, a 385 hit lands
as 378 (385 x 0.980645 = 377.55, rounded). Regeneration happens at merge
per house rules - never on a feature branch.

## Test isolation (2026-08-23)

`TUNABLES_PATH` resolves through `data_path()`, which is CWD-relative
unless `set_data_dir()` was called - and the lib's own test binary cannot
safely call it (process-global `OnceLock` any test can win the race for).
Unit tests therefore get `cfg(test)` twins: `load_live_tunables` returns
the shipped defaults and `save_live_tunables_file` is a no-op. A unit
test needing non-default tunables sets the manager's in-memory copy (the
same value every fight reads); a test proving PERSISTENCE belongs in
`game/tests/`, where `set_data_dir` sandboxes it into a temp dir. This
followed a live incident: the kill-switch test's saved
`dynamic_pacing_enabled = false` persisted in the worktree and silently
disabled the controllers for every later manager test AND every later
run.

## Admin surface

/admin/tunables gains a Dynamic Pacing section (kill-switch, window, both
controllers' knobs, three CSV anchor inputs, both override rows with
current-value labels, the multiplier in force, and pinned warnings) and a
Top-Layer section. `dynamic_scaling_mult`'s row is RETIRED (field kept
for TOML/save compatibility; a save preserves the stored value).
