# Boss / difficulty / enemy-generation tunables — reset reference

Written 2026-08-23 on `feature/dynamic-pacing`. **Documentation only — no
value in this repository was changed to produce it.** The owner resets
these through /admin/tunables at deploy time.

Every key below is a field of `LiveTunables` in
`game/src/adventure/tunables.rs`. There are no `#[serde(rename)]`
attributes in that file, so **the TOML key is the field name verbatim**;
these are the exact keys as they appear in `adventure-live-tunables.toml`.
"Shipped default" is `LiveTunables::default()` — what you get with the key
absent from the file, and what a fresh install runs.

TOML types: `true`/`false` for booleans, bare numbers for the rest, and
arrays for the three anchor lists (`baseline_stage_anchors = [0, 500,
1000, 2000, 3000]`).

## 1. Flat boss stat dials (pre-date this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 1 | `boss_health` | `1.0` | Plain multiplier on top of `boss_stats_for`'s own base HP coefficient. The 2026-08-16 consolidation of "way too many boss multipliers" collapsed several dials into this one. 1.0 means "as designed", not "off". |
| 2 | `boss_power` | `1.0` | The same consolidation for boss ATK (was `difficulty_mult` × `boss_difficulty_dial` × `boss_damage_mult` × a late-content bump). |

## 2. Enemy count per fight (pre-dates this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 3 | `boss_count_tier_stages` | `100` | How many world-stage points make one "tier"; `stage / this` (floored) is the tier count `boss_count_for_stage` rolls its jitter and cap from. |
| 4 | `boss_count_cap_mult` | `1.5` | Hard ceiling on total boss count per fight: `floor(tiers * this)`, so jitter can never produce an absurdly overloaded fight. |

## 3. Boss pierce (pre-dates this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 5 | `pierce_cap` | `0.5` | Asymptotic ceiling that `boss_pierce_pct` climbs toward as stage grows — the fraction of every real boss attack resolving as unavoidable, unmitigable true damage. Approached, never reached. `0.0` is exactly pre-pierce behaviour at every stage. |
| 6 | `pierce_h` | `2000.0` | Stage at which `boss_pierce_pct` reaches HALF of `pierce_cap`. Lower ramps pierce in earlier. |

## 4. Dynamic pacing — shared (this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 7 | `dynamic_pacing_enabled` | `true` | Master kill-switch for BOTH controllers. `false` makes them inert — no sampling, no updates — and freezes both multipliers where they sit. Does NOT affect the baseline floor or the top layer. |
| 8 | `pacing_window_fights` | `20` | Rolling window for both controllers: A keeps this many winning-fight DPS samples, B reads this many boss outcomes. `0` reads as unset and substitutes this default; otherwise clamped 1..=200. Both warm up (no updates) until a full window exists. |

## 5. Controller A — HP / duration axis (this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 9 | `target_duration_min_s` | `30.0` | Lower bound of the target real-clock fight-duration window, seconds. |
| 10 | `target_duration_max_s` | `45.0` | Upper bound. A aims the enemy HP pool at the window MIDPOINT (37.5 s at defaults). Reversed min/max are silently reordered. |
| 11 | `hp_max_step_per_fight` | `0.25` | Per-fight rate limit on the HP multiplier, as a fraction of its current value. **Upward only** — a downward move is not rate-limited. |
| 12 | `hp_multiplier_floor` | `0.4` | How far below 1.0 Controller A may drift on its own. Not the difficulty floor — the baseline anchors are (§7). |
| 13 | `hp_multiplier_ceiling` | `6.0` | How far above 1.0 Controller A may climb. |

## 6. Controller B — damage / lethality axis (this feature)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 14 | `target_win_loss_ratio` | `2.0` | The rolling boss win:loss ratio B steers toward. 2:1 is exactly neutral stage progression given the +1 win / -2 loss walk, so the party only climbs by beating it. |
| 15 | `dmg_max_step_per_fight` | `0.15` | Per-fight rate limit on the damage multiplier. **Upward only**, same asymmetry as #11. |
| 16 | `dmg_multiplier_floor` | `0.4` | How far below 1.0 Controller B may drift on its own. |
| 17 | `dmg_multiplier_ceiling` | `4.0` | How far above 1.0 Controller B may climb. |

## 7. Per-stage baseline floor (this feature, authored content)

Read across as columns of ONE table: anchor *i* is
`(stage[i], hp[i], atk[i])`. Values are FRACTIONS of the organic
stage/level/party curve, linearly interpolated between anchors and flat
after the last. Effective difficulty is `max(controller, baseline)` per
axis, always — this floor has **no switch** by ruling. If the three lists
disagree in length the table is malformed and both axes read neutral
(baseline = the organic curve); that is the documented escape hatch.

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 18 | `baseline_stage_anchors` | `[0, 500, 1000, 2000, 3000]` | The stage column. Must be strictly ascending. |
| 19 | `baseline_hp_anchors` | `[1.0, 0.92, 0.82, 0.68, 0.55]` | Minimum HP-axis difficulty at each anchor stage. Must be finite and > 0. |
| 20 | `baseline_atk_anchors` | `[1.0, 0.94, 0.86, 0.74, 0.62]` | Minimum ATK-axis difficulty at each anchor stage. Same validity rules. |

## 8. Top layer — ADDITION 4 (this feature, own switch)

| # | Key | Shipped default | What it does |
|---|---|---|---|
| 21 | `top_layer_enabled` | `true` | Own switch for the stage-tied absolute damage reduction on every enemy. Independent of `dynamic_pacing_enabled`. `false` reads as 0% at every stage. |
| 22 | `top_layer_cap_pct` | `0.60` | Shapes the ramp's ceiling. The RESULT is clamped into `TOP_LAYER_ABSOLUTE_CAP` (0.95, compile-time), so a value above that yields exactly 0.95. |
| 23 | `top_layer_half_stage` | `1500.0` | Stage at which mitigation reaches half the cap (~30% at defaults; ~41% at stage 3222). |

## 9. Retired

| # | Key | Shipped default | Status |
|---|---|---|---|
| 24 | `dynamic_scaling_mult` | `1.0` | **RETIRED by this feature.** It was the old margin-based rubber-band's reactivity dial; that system was replaced wholesale by the two controllers, which have their own explicit per-fight rate limits (#11, #15). The field stays declared with its old default purely so existing `adventure-live-tunables.toml` files keep deserializing. It is absent from the admin page, no active code path reads it, and a dashboard save preserves whatever value is already on file. **Nothing to reset — leave it alone; changing it does nothing.** |

## Judgement needed — flagged, not guessed

These affect difficulty in some sense but are not enemy stat generation,
so I have not put them in the reset set. Each needs an owner call.

1. **`defensive_stat_hard_cap`** (default `0.95`) — the universal damage-
   reduction ceiling. It bounds DR on *both* sides of a fight, so it
   changes effective difficulty, but it is a doctrine cap rather than a
   boss-scaling dial, and the pacing top layer is deliberately NOT bounded
   by it. Scope is DR only: evasion, block and Intervene have their own
   separate caps (75%/75%/50%). **Include in the reset set only if the
   reset is about effective difficulty rather than enemy generation.**
2. **`late_content_stage`** (default `100`) — named like a difficulty dial
   and it used to be one, but the 2026-08-16 consolidation folded
   `late_content_difficulty_mult` into `boss_health`/`boss_power`. Today
   every reader is a LOOT milestone gate (`manager.rs:4836/4937/4949` —
   guaranteed first Perfect item, per-kill milestones), not a boss stat.
   **Resetting it changes loot generosity, not difficulty.** I would leave
   it out of a boss-scaling reset.
3. **`permanent_rampage`** (default `false`) — an operational toggle, not a
   scaling number: while true, boss encounters run back-to-back forever
   with instant revives. It changes how much difficulty a party meets per
   hour without changing any enemy's stats. Include only if the reset
   covers encounter cadence.

Also worth knowing while planning the reset, though neither is a live
tunable and so neither can be reset from /admin/tunables:

- **The late-stage damage penalty is compile-time.**
  `LATE_STAGE_PENALTY_STAGE_OFFSET = 2000.0` (`combat.rs:440`) drives
  `stage / (stage + offset)`, applied to real bosses only. It is a real
  difficulty ramp with no dashboard dial.
- **`TOP_LAYER_ABSOLUTE_CAP` (0.95), `DYNAMIC_MULT_HARD_CEILING` (1e6),
  `MULT_HARD_FLOOR` (0.05) and `ENEMY_HP_POOL_HARD_CAP` (1e15)** are
  compile-time structural safety caps in `pacing.rs`, deliberately not
  tunable. They bound whatever the dials above are set to.
