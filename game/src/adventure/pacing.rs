//! Dynamic pacing (2026-08-22, branch feature/dynamic-pacing) - TWO
//! independent feedback controllers replacing the old win-margin boss
//! rubber-band (`post_win_power_boost`/`adaptive_difficulty_scale` and
//! friends, all removed):
//!
//! **Controller A - dynamic health (the duration axis, "how long").**
//! Every fight targets a real-clock duration window (default 30-45 s,
//! midpoint 37.5 s). A rolling measure of party damage output is kept
//! over the last `pacing_window_fights` fights; at fight generation the
//! enemy HP POOL is scaled so expected kill time lands near the window
//! midpoint. Per-enemy relative HP weights are never touched - the
//! controller scales the pool, never the distribution. The multiplier is
//! APPLIED to every enemy, boss or filler; it is MEASURED from boss
//! encounters only (2026-08-23 ruling - see "Boss encounters only"
//! below).
//!
//! **Controller B - dynamic damage (the lethality axis, "how hard").**
//! The old rubber-band's punishing role is KEPT and retargeted: enemy
//! damage output scales off the party's rolling win:loss ratio over the
//! same window, targeting `target_win_loss_ratio` (default 2.0). With a
//! win advancing the stage +1 and a loss regressing it -2 (see
//! `run_encounter_inner`), 2 wins : 1 loss is EXACTLY neutral
//! progression - the party only climbs by beating that ratio, which is
//! why B aims at it.
//!
//! **Independence doctrine:** HP answers "how long", damage answers "how
//! hard". A writes only `WorldState::hp_pacing_mult` +
//! `recent_win_dps`; B writes only `WorldState::boss_power_mult`. Neither
//! reads the other's variable, and neither's update consults the other's
//! inputs. Their per-fight rate limits (`*_max_step_per_fight`) are the
//! ONLY damping - there is no arbitration, no priority, no alternation.
//! Duration and lethality remain coupled in OUTCOME (a longer fight is
//! also more lethal in total), even though their VARIABLES are separate;
//! if live behavior oscillates, arbitration between the controllers is
//! the known next step (see docs/dynamic_pacing_spec.md).
//!
//! **Wins-only sampling (owner ruling).** A lost fight carries no
//! meaningful duration signal (a wipe reads as a short fight, which
//! would inflate HP and cause MORE wipes - a death spiral). Controller
//! A samples real duration and party DPS from WINNING fights only.
//!
//! **Boss encounters only, BOTH controllers (owner ruling 2026-08-23).**
//! `permanent_rampage = true` is the expected steady state in production:
//! players vote it on constantly and boss encounters run back-to-back
//! with instant revives, so the filler loop sits out entirely. Non-boss
//! fights exist to slow the game down when nobody is pushing for a
//! rampage, and they are the wrong signal for both axes - filler pools
//! come from `basic_enemy_stats_for`, a different curve from
//! `boss_stats_for`, so a filler DPS sample would steer the HP
//! multiplier that governs BOSS pools using a measurement taken against
//! something else. A's sampler is therefore called from the boss path
//! only; B's outcome window already was. Losses still reach B (an
//! outcome) and still never reach A (no duration sample) - the wins-only
//! rule exists to keep wipes out of the duration average, and an instant
//! revive is not a back door around it: a wipe is `won == false` at the
//! single sample site, and reviving does not re-run the encounter.
//!
//! **Per-stage baseline floor (owner ruling).** A hand-authored curve
//! (`baseline_stage_anchors`/`baseline_hp_anchors`/
//! `baseline_atk_anchors`, linear interpolation between anchors) gives
//! the minimum effective difficulty as a FRACTION of the organic
//! stage/level/party formula. Controllers scale RELATIVE to the organic
//! curve and can NEVER pull effective difficulty below the baseline:
//! effective mult = max(controller mult, baseline(stage)). The floor
//! binds only when a controller wants to go below it; the anchors are
//! owner-shaped from the dashboard and deliberately NOT derived from
//! live player gear (that would be circular and the floor could never
//! bind).
//!
//! **Numeric-limit safety (owner-required).** Live values reach
//! trillions and feedback loops multiply. Every value these controllers
//! produce or scale passes through: non-finite substitution (a NaN
//! tunable becomes its shipped default - NaN must never reach a float->
//! int cast, which maps it to 0 rather than saturating), saturating u64
//! rounding, explicit clamp chains applied BEFORE any cast, and hard
//! absolute ceilings/floors that hold regardless of tunable settings
//! (compile-time consts - structural safety caps under the Decision-16
//! shared-constant exception, same precedent as BOSS_DEFENSE_CAP):
//! `ENEMY_HP_POOL_HARD_CAP` = 1e15 (below f64's exact-integer bound 2^53,
//! far below u64::MAX; live stage-3222 pools are ~1e7-1e8),
//! `DYNAMIC_MULT_HARD_CEILING` = 1e6, `MULT_HARD_FLOOR` = 0.05, and
//! `TOP_LAYER_ABSOLUTE_CAP` = 0.95 (the stage-tied mitigation ceiling
//! stays strictly below 1.0 no matter what the tunable says - an
//! unkillable enemy is a worse failure than a long fight).

use super::*;

/// Hard absolute cap on any SCALED enemy HP pool (organic stat x
/// controller/baseline multiplier), regardless of tunable settings.
pub(crate) const ENEMY_HP_POOL_HARD_CAP: f64 = 1.0e15;

/// Hard absolute ceiling on either controller's multiplier, regardless of
/// tunable settings.
pub(crate) const DYNAMIC_MULT_HARD_CEILING: f64 = 1.0e6;

/// Hard absolute floor beneath either controller's tunable floor - a bad
/// dashboard edit can never zero an enemy's stats into degenerate
/// instant-win fights.
pub(crate) const MULT_HARD_FLOOR: f64 = 0.05;

/// Hard absolute cap for the stage-tied top-layer mitigation, strictly
/// below 1.0. The LIVE value is additionally bounded by the
/// `top_layer_cap_pct` tunable; this const bounds THE TUNABLE itself.
pub(crate) const TOP_LAYER_ABSOLUTE_CAP: f64 = 0.95;

/// Shipped defaults for every dynamic-pacing tunable - the single source
/// both `LiveTunables::default()` and this module's own non-finite
/// substitution fall back to, so the two lists can never drift apart.
pub(crate) mod defaults {
    pub const PACING_WINDOW_FIGHTS: u32 = 20;
    pub const TARGET_DURATION_MIN_S: f64 = 30.0;
    pub const TARGET_DURATION_MAX_S: f64 = 45.0;
    pub const HP_MAX_STEP_PER_FIGHT: f64 = 0.25;
    pub const HP_MULTIPLIER_FLOOR: f64 = 0.4;
    pub const HP_MULTIPLIER_CEILING: f64 = 6.0;
    pub const TARGET_WIN_LOSS_RATIO: f64 = 2.0;
    pub const DMG_MAX_STEP_PER_FIGHT: f64 = 0.15;
    pub const DMG_MULTIPLIER_FLOOR: f64 = 0.4;
    pub const DMG_MULTIPLIER_CEILING: f64 = 4.0;
    /// Initial hand-authored baseline anchors (owner-shapable from the
    /// dashboard; deliberately NOT derived from live player gear).
    /// Reasoning for these first-pass values: party power has historically
    /// outrun the LINEAR stage curve increasingly at higher stages (the
    /// old rubber-band climbed to 3.9x and once pinned an old 5.0
    /// ceiling), so the room the floor grants below the organic curve
    /// widens with stage - modest early while parties track the curve,
    /// widest deep where over-performance was actually observed.
    pub const BASELINE_STAGE_ANCHORS: &[u32] = &[0, 500, 1000, 2000, 3000];
    pub const BASELINE_HP_ANCHORS: &[f64] = &[1.0, 0.92, 0.82, 0.68, 0.55];
    pub const BASELINE_ATK_ANCHORS: &[f64] = &[1.0, 0.94, 0.86, 0.74, 0.62];
    pub const TOP_LAYER_ENABLED: bool = true;
    pub const TOP_LAYER_CAP_PCT: f64 = 0.60;
    pub const TOP_LAYER_HALF_STAGE: f64 = 1500.0;
}

/// Substitutes a non-finite live-tunable read with its shipped default -
/// NaN/inf must never propagate toward a float->int cast (which maps NaN
/// to 0 rather than saturating).
pub(crate) fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// Every numeric input the controllers need, sanitized. Built fresh from a
/// `LiveTunables` snapshot at every use site (tunables are re-read every
/// fight by convention). ONE struct instead of ten function parameters:
/// keeps clippy's arity lint happy and makes the kill-switch testable.
#[derive(Debug, Clone)]
pub(crate) struct PacingParams {
    /// Master kill-switch - `false` makes BOTH controllers completely
    /// inert: no sampling, no updates. Multipliers freeze wherever they
    /// sit; the old margin-ratchet does NOT come back (owner ruling).
    pub enabled: bool,
    /// Rolling window length for BOTH controllers (sanitized 1..=200).
    pub window: usize,
    pub duration_min_s: f64,
    pub duration_max_s: f64,
    pub hp_step: f64,
    pub hp_floor: f64,
    pub hp_ceiling: f64,
    pub wl_target: f64,
    pub dmg_step: f64,
    pub dmg_floor: f64,
    pub dmg_ceiling: f64,
}

impl PacingParams {
    pub fn from_tunables(t: &LiveTunables) -> Self {
        // 0 is not "a one-fight window" - it is an unset/cleared dial, so
        // the integer axis substitutes its shipped default exactly like
        // `finite_or` does on every float axis.
        let window_fights = if t.pacing_window_fights == 0 { defaults::PACING_WINDOW_FIGHTS } else { t.pacing_window_fights };
        let window = window_fights.clamp(1, 200) as usize;
        let min_s = finite_or(t.target_duration_min_s, defaults::TARGET_DURATION_MIN_S).max(0.001);
        let max_s = finite_or(t.target_duration_max_s, defaults::TARGET_DURATION_MAX_S).max(0.001);
        let (min_s, max_s) = if min_s <= max_s { (min_s, max_s) } else { (max_s, min_s) };
        PacingParams {
            enabled: t.dynamic_pacing_enabled,
            window,
            duration_min_s: min_s,
            duration_max_s: max_s,
            hp_step: finite_or(t.hp_max_step_per_fight, defaults::HP_MAX_STEP_PER_FIGHT).clamp(0.0, 100.0),
            hp_floor: finite_or(t.hp_multiplier_floor, defaults::HP_MULTIPLIER_FLOOR),
            hp_ceiling: finite_or(t.hp_multiplier_ceiling, defaults::HP_MULTIPLIER_CEILING),
            wl_target: finite_or(t.target_win_loss_ratio, defaults::TARGET_WIN_LOSS_RATIO).max(0.001),
            dmg_step: finite_or(t.dmg_max_step_per_fight, defaults::DMG_MAX_STEP_PER_FIGHT).clamp(0.0, 100.0),
            dmg_floor: finite_or(t.dmg_multiplier_floor, defaults::DMG_MULTIPLIER_FLOOR),
            dmg_ceiling: finite_or(t.dmg_multiplier_ceiling, defaults::DMG_MULTIPLIER_CEILING),
        }
    }
}

/// Validates an anchor pair-list and returns the slices unchanged, or
/// `None` when the owner's edit is malformed (empty, length mismatch,
/// non-ascending stages, non-finite/non-positive values). Callers treat
/// `None` as "baseline neutral (1.0)".
pub(crate) fn validated_anchors<'a>(stages: &'a [u32], values: &'a [f64]) -> Option<(&'a [u32], &'a [f64])> {
    if stages.is_empty() || stages.len() != values.len() {
        return None;
    }
    if stages.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    if values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    Some((stages, values))
}

/// Linear interpolation across the hand-authored baseline anchors.
/// Below the first anchor -> the first value; at/above the last -> the
/// LAST value (flat - correct for a MINIMUM: growth above it comes from
/// the controllers themselves). Any malformed configuration reads as
/// `None` (callers substitute neutral 1.0, floor == organic curve).
pub(crate) fn baseline_curve_at(stage: u32, stages: &[u32], values: &[f64]) -> Option<f64> {
    let (stages, values) = validated_anchors(stages, values)?;
    let s = stage as f64;
    if s <= stages[0] as f64 {
        return Some(values[0]);
    }
    let last = stages.len() - 1;
    if s >= stages[last] as f64 {
        return Some(values[last]);
    }
    for (i, pair) in stages.windows(2).enumerate() {
        if s < pair[1] as f64 {
            let lo = pair[0] as f64;
            let hi = pair[1] as f64;
            let frac = (s - lo) / (hi - lo);
            return Some(values[i] + (values[i + 1] - values[i]) * frac);
        }
    }
    Some(values[last])
}

/// What fight generation ultimately scales with, AFTER the baseline floor
/// binds: effective = max(controller, baseline), per axis independently.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EffectiveMultipliers {
    pub hp_mult: f64,
    pub dmg_mult: f64,
    /// The baselines themselves, for admin-page visibility (a pinned
    /// controller must be visible, not silent).
    pub hp_baseline: f64,
    pub dmg_baseline: f64,
    /// The controllers' OWN (sanitized, pre-max) values. Kept because
    /// `hp_mult`/`dmg_mult` have already absorbed the max() - without
    /// these the pinned state is unrecoverable from the result.
    pub hp_controller: f64,
    pub dmg_controller: f64,
}

impl EffectiveMultipliers {
    /// True when the controller's OWN value sits strictly below its
    /// baseline - i.e. the floor is doing the work and the party is
    /// performing below the stage baseline. Surfaced verbatim on the
    /// admin page ("pinned at baseline floor") instead of being silently
    /// absorbed by the max().
    pub fn hp_pinned(&self) -> bool {
        is_pinned_to_baseline(self.hp_controller, self.hp_baseline)
    }
    pub fn dmg_pinned(&self) -> bool {
        is_pinned_to_baseline(self.dmg_controller, self.dmg_baseline)
    }
}

/// Both axes' baselines for one stage. The three anchor lists are ONE
/// hand-authored table - stage, HP, ATK read across as columns - so they
/// are validated together: if any of the three disagrees in length the
/// TABLE is malformed, and BOTH axes read neutral (baseline == the
/// organic curve). A half-edited table must never leave one axis floored
/// by anchors that no longer line up with the stage column they were
/// written against; a bad edit may only ever loosen the floor, never
/// corrupt difficulty (owner ruling 3).
pub(crate) fn baseline_pair_at(stage: u32, t: &LiveTunables) -> (f64, f64) {
    let stages = &t.baseline_stage_anchors;
    if stages.len() != t.baseline_hp_anchors.len() || stages.len() != t.baseline_atk_anchors.len() {
        return (1.0, 1.0);
    }
    (
        baseline_curve_at(stage, stages, &t.baseline_hp_anchors).unwrap_or(1.0),
        baseline_curve_at(stage, stages, &t.baseline_atk_anchors).unwrap_or(1.0),
    )
}

/// Composes the per-axis effective multipliers for a fight generated at
/// `stage`: the stored CONTROLLER values (which may sit anywhere in
/// [MULT_HARD_FLOOR, DYNAMIC_MULT_HARD_CEILING]) raised to at least the
/// hand-authored stage baseline. This is the ONLY place the floor binds;
/// the controllers themselves never read it, preserving independence.
pub(crate) fn effective_multipliers(hp_controller: f64, dmg_controller: f64, stage: u32, t: &LiveTunables) -> EffectiveMultipliers {
    let (hp_baseline, dmg_baseline) = baseline_pair_at(stage, t);
    let hp_controller = sanitize_mult(hp_controller);
    let dmg_controller = sanitize_mult(dmg_controller);
    EffectiveMultipliers {
        hp_mult: hp_controller.max(hp_baseline),
        dmg_mult: dmg_controller.max(dmg_baseline),
        hp_baseline,
        dmg_baseline,
        hp_controller,
        dmg_controller,
    }
}

/// Caps a composed HP multiplier so the SCALED pool cannot exceed
/// `ENEMY_HP_POOL_HARD_CAP` regardless of how large the organic pool or
/// the multipliers got. Applied at generation time, before any cast.
pub(crate) fn capped_hp_mult_for_pool(base_pool: f64, hp_mult: f64) -> f64 {
    // A non-finite pool means the cap is uncomputable - there is no
    // multiplier we can honestly trust against it, so generation falls
    // back to neutral rather than scaling an already-broken number.
    if !base_pool.is_finite() {
        return 1.0;
    }
    // A zero/negative pool imposes no cap at all (nothing to overflow).
    if base_pool <= 0.0 {
        return sanitize_mult(hp_mult);
    }
    let pool_cap_mult = ENEMY_HP_POOL_HARD_CAP / base_pool;
    sanitize_mult(hp_mult.min(pool_cap_mult))
}

/// Rounds a scaled stat to u64 WITHOUT ever wrapping: Rust's float->int
/// casts saturate, but only for FINITE inputs (NaN maps to 0), so the
/// finite case is handled first and non-finite collapses to 1 - a visible,
/// resolvable unit, never an unkillable or absent one. Unreachable given
/// upstream sanitization; this is the last-line guard.
pub(crate) fn sat_round_stat(value: f64) -> u64 {
    if !value.is_finite() {
        return 1;
    }
    let rounded = value.round();
    if rounded < 1.0 {
        1
    } else if rounded >= u64::MAX as f64 {
        u64::MAX
    } else {
        rounded as u64
    }
}

/// Clamps a raw multiplier read into the safe operating range:
/// non-finite -> neutral 1.0, otherwise [MULT_HARD_FLOOR,
/// DYNAMIC_MULT_HARD_CEILING]. Every multiplier passes through here
/// before touching a stat.
pub(crate) fn sanitize_mult(value: f64) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
    value.clamp(MULT_HARD_FLOOR, DYNAMIC_MULT_HARD_CEILING)
}

/// Sanitizes an ADMIN OVERRIDE write (manual dashboard set) - same hard
/// range as the controllers, floored at the legacy rubber-band minimum.
pub(crate) fn sanitize_override_mult(value: f64) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
    value.clamp(super::BOSS_POWER_MULT_MIN, DYNAMIC_MULT_HARD_CEILING)
}

/// The shared rate-limit + clamp chain, applied to BOTH controllers:
///
/// 1. sanitize prev (non-finite -> neutral) and read the direction of
///    travel; a NaN desired means "no signal" and holds at prev,
///    +/-inf is an honest saturation request and travels,
/// 2. rate-limit the move, then
/// 3. clamp into the operating window = the configured
///    `[floor, ceiling]` hard-capped into
///    `[MULT_HARD_FLOOR, DYNAMIC_MULT_HARD_CEILING]`, widened to include
///    `prev`.
///
/// Three asymmetries, each load-bearing:
///
/// * **Escalation is damped, relief is not.** Only UPWARD moves take the
///   `[.., prev*(1+step)]` band. Easing an over-tuned fight immediately is
///   never the dangerous direction - the death-spiral this module exists
///   to prevent is built out of difficulty that ratchets up faster than
///   the party can answer, so the configured floor is reachable the fight
///   the controller asks for it, while the ceiling is walked to one step
///   at a time.
/// * **The structural hard caps are not walked to.** The band damps
///   movement inside the OWNER-CONFIGURED window. When the configured
///   ceiling sits beyond `DYNAMIC_MULT_HARD_CEILING` the owner has said
///   "no ceiling on this side" and the structural cap is the only bound
///   left - a safety cap is not a balance knob, so it binds at once
///   rather than after N fights of climbing.
/// * **The window never slams `prev`, but it does CONVERGE.** A stored
///   multiplier already outside its configured window (a dashboard edit
///   tightened the range underneath it, or an older save) is never yanked
///   in mid-flight - but the widening that admits it shrinks by one
///   rate-limited step per fight, unconditionally, until it is gone. It
///   was previously permanent (`cfg.min(prev)` / `cfg.max(prev)`), which
///   made a configured bound advisory: it could only be honored if the
///   controller happened to request that direction by itself, so an
///   operator lowering a ceiling on a runaway got neither effect nor
///   feedback. Only the hard caps still bind instantly.
pub(crate) fn clamp_rate_limited(prev: f64, desired: f64, step: f64, floor_v: f64, ceil_v: f64) -> f64 {
    let prev = sanitize_mult(prev);
    let step = if step.is_finite() { step.clamp(0.0, 100.0) } else { 0.0 };
    let desired = if desired.is_nan() { prev } else { desired };
    let cfg_lo = floor_v.min(ceil_v);
    let cfg_hi = floor_v.max(ceil_v);
    let hard_lo = sanitize_mult(cfg_lo);
    let hard_hi = sanitize_mult(cfg_hi);
    // The widening SHRINKS (2026-08-23). While `prev` sits outside the
    // configured window the effective bound is not `prev` itself - it is
    // ONE rate-limited step of `prev` taken TOWARD the configured value.
    // So the widening closes by a step every fight until it is gone, and
    // the bound applies regardless of what the fight signal asked for:
    // this clamp is the last thing to run, so an outside-the-window
    // multiplier is pulled in even on a fight where the controller wanted
    // to go the other way.
    //
    // Before this, the bounds were `cfg.min(prev)` / `cfg.max(prev)` - the
    // window simply absorbed `prev` and never let go, so a configured
    // bound was ADVISORY: an operator who lowered a ceiling to rein in a
    // runaway got no effect and no feedback, because the only thing that
    // could bring the value back was the controller happening to request
    // that direction on its own.
    //
    // The no-slam property is preserved exactly - nothing is yanked
    // mid-flight, the move is still capped at one step per fight. A `step`
    // of 0 means "this controller may not move at all per fight", and the
    // widening correspondingly cannot close; that is coherent rather than
    // a special case.
    let lo = if prev < hard_lo { hard_lo.min(prev * (1.0 + step)) } else { hard_lo };
    let hi = if prev > hard_hi { hard_hi.max(prev / (1.0 + step)) } else { hard_hi };
    let limited = if desired > prev && cfg_hi <= DYNAMIC_MULT_HARD_CEILING {
        desired.min(prev * (1.0 + step))
    } else {
        desired
    };
    limited.clamp(lo, hi)
}

/// CONTROLLER A - one per-winning-fight update of the HP multiplier.
///
/// `base_pool` is THIS fight's UNSCALED organic HP pool (controller
/// multipliers at 1.0); `dps_window` is the rolling per-win DPS sample
/// list (oldest first). Returns `None` (= leave the stored multiplier
/// untouched) while warming up (< full window) or whenever any input is
/// unusable; otherwise the rate-limited, clamped new value targeting the
/// window MIDPOINT duration:
///
///   required_mult = (rolling_dps * mid_target_s) / base_pool
///
/// Losing fights NEVER reach the sampler that feeds this window (see
/// `push_dps_sample`) - owner ruling, death-spiral prevention.
pub(crate) fn update_hp_pacing_mult(prev: f64, base_pool: f64, dps_window: &[f64], p: &PacingParams) -> Option<f64> {
    if !p.enabled || dps_window.len() < p.window {
        return None;
    }
    let mut sum = 0.0f64;
    for &d in dps_window {
        sum += if d.is_finite() { d } else { 0.0 };
    }
    // A window whose honest samples OVERFLOW f64 is not a broken reading -
    // it is a saturation signal, and it travels as one (`clamp_rate_limited`
    // resolves +inf against the hard caps). Only a window with no usable
    // signal at all (all samples dropped, or a nonsensical <= 0 mean) skips
    // the update.
    let mean_dps = sum / p.window as f64;
    if mean_dps.is_nan() || mean_dps <= 0.0 {
        return None;
    }
    if !base_pool.is_finite() || base_pool <= 0.0 {
        return None;
    }
    let mid_target_s = (p.duration_min_s + p.duration_max_s) / 2.0;
    let desired_pool = mean_dps * mid_target_s;
    let required = desired_pool / base_pool;
    Some(clamp_rate_limited(prev, required, p.hp_step, p.hp_floor, p.hp_ceiling))
}

/// CONTROLLER B - one per-BOSS-fight update of the damage multiplier from
/// the rolling win/loss history (boss outcomes only - basic fights don't
/// record outcomes by design, owner-confirmed asymmetry). Above target ->
/// grow one step; below target -> ease one step; AT target -> no change
/// (2 wins : 1 loss is exactly neutral progression given the +1/-2 stage
/// walk). Same warmup, sanitization, rate-limit and clamps as A.
pub(crate) fn update_dmg_pacing_mult(prev: f64, outcomes: &[bool], p: &PacingParams) -> Option<f64> {
    if !p.enabled || outcomes.len() < p.window {
        return None;
    }
    // `outcomes` IS the rolling window - the caller trims it to
    // `p.window` as it pushes. The length check above is the WARMUP gate
    // (no update until a full window exists), not a second trim:
    // re-slicing here would silently throw away outcomes the caller
    // chose to keep and would read a 3:1 window as 2:1.
    let wins = outcomes.iter().filter(|&&w| w).count();
    let losses = outcomes.len() - wins;
    // Direction of THIS update. Undefeated/all-loss windows take explicit
    // branches - never inf arithmetic; a mixed window compares its exact
    // integer-derived win:loss ratio against the target.
    enum Dir {
        Up,
        Down,
        Hold,
    }
    let dir = if losses == 0 {
        Dir::Up
    } else if wins == 0 {
        Dir::Down
    } else {
        let ratio = wins as f64 / losses as f64;
        if !ratio.is_finite() {
            return None;
        }
        if ratio > p.wl_target {
            Dir::Up
        } else if ratio < p.wl_target {
            Dir::Down
        } else {
            Dir::Hold
        }
    };
    if matches!(dir, Dir::Hold) {
        return None;
    }
    let cur = sanitize_mult(prev);
    let desired = if matches!(dir, Dir::Up) {
        cur * (1.0 + p.dmg_step)
    } else {
        cur / (1.0 + p.dmg_step)
    };
    Some(clamp_rate_limited(prev, desired, p.dmg_step, p.dmg_floor, p.dmg_ceiling))
}

/// Records ONE fight's DPS sample into Controller A's rolling window.
/// WINS ONLY (a loss is never sampled - owner ruling) and FINITE ONLY
/// (a non-finite measurement is dropped, never stored). Retention is
/// trimmed to `cap` (already sanitized to >= 1 by `PacingParams`).
pub(crate) fn push_dps_sample(window: &mut std::collections::VecDeque<f64>, won: bool, dps: f64, cap: usize) {
    if !won || !dps.is_finite() {
        return;
    }
    window.push_back(dps);
    let cap = cap.max(1);
    while window.len() > cap {
        window.pop_front();
    }
}

/// ADDITION 4 - the stage-tied TOP-LAYER mitigation percentage for
/// enemies generated at `stage`: an asymptotic ramp reaching half of the
/// tunable cap at `top_layer_half_stage` and approaching (never reaching)
/// the cap, mirroring boss pierce's shape. The TUNABLE cap shapes the
/// curve; the RESULT is clamped into [0, TOP_LAYER_ABSOLUTE_CAP], so an
/// owner who dials the cap past the structural limit gets exactly the
/// structural limit (strictly below 1.0, no matter what the dashboard
/// says) rather than the structural limit minus the ramp's own
/// asymptotic deficit. Deliberately
/// keyed to STAGE ONLY, never to Controller A: it is a property of
/// tougher enemies at higher stages (predictable, explainable), while A
/// keeps HP as its lever so gear upgrades still visibly shorten fights.
pub(crate) fn top_layer_for_stage(stage: u32, t: &LiveTunables) -> f64 {
    if !t.top_layer_enabled {
        return 0.0;
    }
    // The tunable cap shapes the CURVE at its configured value; the
    // structural cap bounds the RESULT. Clamping the tunable down first
    // instead would leave an owner who asked for "everything" permanently
    // short of the structural cap by the curve's own asymptotic deficit -
    // the ramp would approach 0.95 without ever being allowed to reach it,
    // which is not what the hard cap means.
    let cap = finite_or(t.top_layer_cap_pct, defaults::TOP_LAYER_CAP_PCT).max(0.0);
    let half = finite_or(t.top_layer_half_stage, defaults::TOP_LAYER_HALF_STAGE).max(1.0);
    let s = stage as f64;
    (cap * s / (s + half)).clamp(0.0, TOP_LAYER_ABSOLUTE_CAP)
}

/// Admin-page saturation signal for ONE axis: the controller is PINNED
/// when its own stored value sits strictly below the baseline that
/// actually governed generation - i.e. the floor did the work and the
/// party is performing below the stage baseline.
pub(crate) fn is_pinned_to_baseline(controller_value: f64, baseline: f64) -> bool {
    controller_value < baseline
}

/// Everything the admin page renders about both controllers: each
/// controller's OWN value, the stage baseline under it, and - the number
/// that actually governs generation - the effective multiplier
/// `max(controller, baseline)`. Showing only the first two left an
/// operator to do the max() in their head to know what the next fight
/// will be built with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PacingStatus {
    pub hp_mult: f64,
    pub dmg_mult: f64,
    pub hp_baseline: f64,
    pub dmg_baseline: f64,
    /// What generation multiplies by - `max(controller, baseline)` per
    /// axis, exactly as `effective_multipliers` composed it.
    pub hp_effective: f64,
    pub dmg_effective: f64,
    hp_pinned: bool,
    dmg_pinned: bool,
}

impl PacingStatus {
    /// True when the baseline floor - not this controller - is setting
    /// the difficulty on this axis. Computed once by
    /// `EffectiveMultipliers`, which is the type that knows both halves;
    /// re-deriving it from the rendered numbers is how the two drifted
    /// apart the first time.
    pub fn hp_pinned(&self) -> bool {
        self.hp_pinned
    }
    pub fn dmg_pinned(&self) -> bool {
        self.dmg_pinned
    }
}

pub(crate) fn pacing_status(hp_mult: f64, dmg_mult: f64, stage: u32, t: &LiveTunables) -> PacingStatus {
    let eff = effective_multipliers(hp_mult, dmg_mult, stage, t);
    PacingStatus {
        hp_mult: eff.hp_controller,
        dmg_mult: eff.dmg_controller,
        hp_baseline: eff.hp_baseline,
        dmg_baseline: eff.dmg_baseline,
        hp_effective: eff.hp_mult,
        dmg_effective: eff.dmg_mult,
        hp_pinned: eff.hp_pinned(),
        dmg_pinned: eff.dmg_pinned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> PacingParams {
        PacingParams {
            enabled: true,
            window: 3,
            duration_min_s: 30.0,
            duration_max_s: 45.0,
            hp_step: 0.25,
            hp_floor: 0.4,
            hp_ceiling: 6.0,
            wl_target: 2.0,
            dmg_step: 0.15,
            dmg_floor: 0.4,
            dmg_ceiling: 4.0,
        }
    }

    #[test]
    fn controller_a_converges_toward_the_window_midpoint() {
        let p = params();
        // True party DPS is 100; organic pool 1000 -> unscaled duration 10s,
        // far below the 37.5s midpoint. Feed a full window of honest
        // samples and iterate: the multiplier must climb until the EXPECTED
        // duration lands near the midpoint, then hold.
        let window = vec![100.0, 100.0, 100.0];
        let mut mult = 1.0_f64;
        for _ in 0..80 {
            match update_hp_pacing_mult(mult, 1000.0, &window, &p) {
                Some(next) => mult = next,
                None => panic!("full window must always update"),
            }
        }
        assert!((mult - 3.75).abs() < 0.01, "converged to {mult}");
        let expected_s = 1000.0 * mult / 100.0;
        assert!(expected_s >= 30.0 && expected_s <= 45.0, "duration {expected_s}s outside window");
    }

    #[test]
    fn controller_a_rate_limits_per_fight() {
        let p = params();
        let window = vec![1.0e12, 1.0e12, 1.0e12];
        let next = update_hp_pacing_mult(1.0, 1000.0, &window, &p).expect("updates");
        assert!(next <= (1.0 + p.hp_step) + 1e-12, "moved {next}, band tops at {}", 1.0 + p.hp_step);
        assert!(next >= 1.0 / (1.0 + p.hp_step) - 1e-12);
    }

    #[test]
    fn controller_a_warms_up_until_the_window_is_full() {
        let p = params();
        assert_eq!(update_hp_pacing_mult(1.0, 1000.0, &[100.0, 100.0], &p), None, "short window");
        assert!(update_hp_pacing_mult(1.0, 1000.0, &[100.0, 100.0, 100.0], &p).is_some());
    }

    #[test]
    fn controller_a_honors_floor_ceiling_and_hard_caps() {
        let p = params();
        let big = vec![1.0e18, 1.0e18, 1.0e18];
        let next = update_hp_pacing_mult(5.9, 1.0, &big, &p).expect("updates");
        assert!((next - p.hp_ceiling).abs() < 1e-9, "{next} vs ceiling {}", p.hp_ceiling);
        let tiny = vec![1.0e-9, 1.0e-9, 1.0e-9];
        let next = update_hp_pacing_mult(1.0, 1.0e12, &tiny, &p).expect("updates");
        assert!((next - p.hp_floor).abs() < 1e-9, "{next} vs floor {}", p.hp_floor);
        let mut p2 = params();
        p2.hp_ceiling = 1.0e18;
        let next = update_hp_pacing_mult(1.0e5, 1.0, &big, &p2).expect("updates");
        assert_eq!(next, DYNAMIC_MULT_HARD_CEILING, "tunable ceiling above the hard cap still saturates at it");
        let mut p3 = params();
        p3.hp_floor = 1.0e-12;
        p3.hp_step = 100.0;
        let next = update_hp_pacing_mult(0.06, 1.0e12, &tiny, &p3).expect("updates");
        assert_eq!(next, MULT_HARD_FLOOR);
    }

    #[test]
    fn controller_a_ignores_non_finite_samples_and_inputs() {
        let p = params();
        // Non-finite samples contribute 0; a fully-poisoned window yields a
        // zero mean -> NO update rather than a garbage multiplier.
        assert!(update_hp_pacing_mult(1.0, 1000.0, &[f64::NAN, f64::INFINITY, 100.0], &p).is_some());
        assert_eq!(update_hp_pacing_mult(1.0, 1000.0, &[f64::NAN, f64::NAN, f64::NAN], &p), None);
        assert_eq!(update_hp_pacing_mult(1.0, f64::INFINITY, &[100.0, 100.0, 100.0], &p), None);
    }

    #[test]
    fn controller_b_converges_to_two_wins_per_one_loss() {
        let p = params();
        // At exactly 2:1 (window 3 = WWL) B must HOLD - neutral progression.
        assert_eq!(update_dmg_pacing_mult(1.5, &[true, true, false], &p), None);
        // Over-performing (WWW) grows one step...
        let up = update_dmg_pacing_mult(1.5, &[true, true, true], &p).expect("updates");
        assert!(up > 1.5 && up <= 1.5 * (1.0 + p.dmg_step) + 1e-12, "{up}");
        // ...under-performing (WLL) eases one step.
        let down = update_dmg_pacing_mult(1.5, &[true, false, false], &p).expect("updates");
        assert!(down < 1.5 && down >= 1.5 / (1.0 + p.dmg_step) - 1e-12, "{down}");
        // A mixed window ABOVE target still grows (3:1 > 2:1).
        assert!(update_dmg_pacing_mult(1.5, &[true, true, true, false], &p).unwrap() > 1.5);
        // Zero losses counts as above target; zero wins as below.
        assert!(update_dmg_pacing_mult(1.0, &[true; 3], &p).unwrap() > 1.0);
        assert!(update_dmg_pacing_mult(1.0, &[false; 3], &p).unwrap() < 1.0);
    }

    #[test]
    fn controller_b_rate_limit_clamps_and_hard_caps_mirror_a() {
        let p = params();
        let up = update_dmg_pacing_mult(1.0, &[true; 3], &p).expect("updates");
        assert!(up <= 1.0 * (1.0 + p.dmg_step) + 1e-12);
        let mut p2 = params();
        p2.dmg_ceiling = 1.0e18;
        let up = update_dmg_pacing_mult(DYNAMIC_MULT_HARD_CEILING, &[true; 3], &p2).expect("updates");
        assert_eq!(up, DYNAMIC_MULT_HARD_CEILING, "hard ceiling holds at saturation");
        let mut p3 = params();
        p3.dmg_floor = 1.0e-12;
        p3.dmg_step = 100.0;
        let down = update_dmg_pacing_mult(MULT_HARD_FLOOR, &[false; 3], &p3).expect("updates");
        assert_eq!(down, MULT_HARD_FLOOR, "hard floor holds at saturation");
        assert!(down.is_finite());
    }

    #[test]
    fn kill_switch_off_means_complete_passthrough_for_both_controllers() {
        let mut p = params();
        p.enabled = false;
        assert_eq!(update_hp_pacing_mult(1.0, 1000.0, &[100.0, 100.0, 100.0], &p), None);
        assert_eq!(update_dmg_pacing_mult(1.0, &[true, true, true], &p), None);
    }

    #[test]
    fn dps_samples_are_wins_only_and_finite_only() {
        let mut q = std::collections::VecDeque::new();
        push_dps_sample(&mut q, false, 500.0, 20);
        assert!(q.is_empty(), "a losing fight is never sampled");
        push_dps_sample(&mut q, true, f64::NAN, 20);
        push_dps_sample(&mut q, true, f64::INFINITY, 20);
        assert!(q.is_empty(), "non-finite measurements are dropped");
        push_dps_sample(&mut q, true, 100.0, 2);
        push_dps_sample(&mut q, true, 200.0, 2);
        push_dps_sample(&mut q, true, 300.0, 2);
        assert_eq!(q.len(), 2, "retention trims to the window");
        assert_eq!(q.front(), Some(&200.0));
        assert_eq!(q.back(), Some(&300.0));
    }

    #[test]
    fn the_two_controllers_are_independent_variables() {
        // Behavioral independence: A's answer is identical no matter what
        // B's history looks like, and vice versa - each update reads ONLY
        // its own history (the write paths are disjoint WorldState fields;
        // see the module doc).
        let p = params();
        let a_hist = vec![100.0, 100.0, 100.0];
        let b_hist_win = vec![true, true, true];
        let b_hist_even = vec![true, true, false];
        let _ = update_dmg_pacing_mult(1.0, &b_hist_win, &p);
        let a1 = update_hp_pacing_mult(1.0, 1000.0, &a_hist, &p);
        let _ = update_dmg_pacing_mult(1.0, &b_hist_even, &p);
        let a2 = update_hp_pacing_mult(1.0, 1000.0, &a_hist, &p);
        assert_eq!(a1, a2, "A is unaffected by B's inputs");
        let b1 = update_dmg_pacing_mult(1.5, &b_hist_win, &p);
        let _ = update_hp_pacing_mult(9.9, 1000.0, &a_hist, &p);
        let b2 = update_dmg_pacing_mult(1.5, &b_hist_win, &p);
        assert_eq!(b1, b2, "B is unaffected by A's inputs");
    }

    #[test]
    fn baseline_curve_interpolates_and_validates() {
        let stages = [0u32, 100, 200];
        let values = [1.0, 0.8, 0.6];
        assert_eq!(baseline_curve_at(0, &stages, &values), Some(1.0));
        assert_eq!(baseline_curve_at(50, &stages, &values), Some(0.9));
        assert_eq!(baseline_curve_at(150, &stages, &values), Some(0.7));
        assert_eq!(baseline_curve_at(300, &stages, &values), Some(0.6), "flat after the last anchor");
        assert_eq!(baseline_curve_at(10, &[], &[]), None);
        assert_eq!(baseline_curve_at(10, &[0, 100], &[1.0]), None, "length mismatch");
        assert_eq!(baseline_curve_at(10, &[100, 0], &[1.0, 0.5]), None, "descending stages");
        assert_eq!(baseline_curve_at(10, &[0, 100], &[1.0, f64::NAN]), None);
        assert_eq!(baseline_curve_at(10, &[0, 100], &[1.0, -0.5]), None);
    }

    #[test]
    fn the_baseline_floor_binds_no_matter_what_the_controllers_do() {
        let mut t = LiveTunables::default();
        t.baseline_stage_anchors = vec![0, 1000];
        t.baseline_hp_anchors = vec![1.0, 0.5];
        t.baseline_atk_anchors = vec![1.0, 0.5];
        // Controllers begging to go to the very bottom of their range...
        let eff = effective_multipliers(MULT_HARD_FLOOR, MULT_HARD_FLOOR, 1000, &t);
        assert!((eff.hp_mult - 0.5).abs() < 1e-12, "HP can never undercut the baseline");
        assert!((eff.dmg_mult - 0.5).abs() < 1e-12);
        assert!(eff.hp_pinned() && eff.dmg_pinned(), "saturation is visible");
        // ...and NO tunable combination at ANY stage drops below baseline.
        for stage in [0u32, 250, 999, 1000, 4000] {
            let eff = effective_multipliers(1.0e-300, 1.0e-300, stage, &t);
            let hp_b = baseline_curve_at(stage, &t.baseline_stage_anchors, &t.baseline_hp_anchors).unwrap_or(1.0);
            let dmg_b = baseline_curve_at(stage, &t.baseline_stage_anchors, &t.baseline_atk_anchors).unwrap_or(1.0);
            assert!(eff.hp_mult >= hp_b - 1e-12, "stage {stage}");
            assert!(eff.dmg_mult >= dmg_b - 1e-12, "stage {stage}");
        }
        // Malformed anchors fall back to neutral (floor == organic curve).
        t.baseline_stage_anchors = vec![0];
        t.baseline_hp_anchors = vec![0.5];
        let eff = effective_multipliers(0.2, 0.2, 1000, &t);
        assert!((eff.hp_baseline - 1.0).abs() < 1e-12, "length mismatch -> neutral");
    }

    #[test]
    fn the_hp_pool_hard_cap_holds_for_any_input() {
        let m = capped_hp_mult_for_pool(1.0e14, 1.0e6);
        assert!((m - 10.0).abs() < 1e-6, "{m} caps pool at 1e15");
        let m = capped_hp_mult_for_pool(1.0e15, 1.0e6);
        assert!((m - 1.0).abs() < 1e-9);
        assert_eq!(capped_hp_mult_for_pool(f64::INFINITY, 2.0), 1.0, "non-finite pool -> sanitized mult");
        let base = 9.0e14;
        let m = capped_hp_mult_for_pool(base, 50.0);
        assert!(base * m <= ENEMY_HP_POOL_HARD_CAP + 1.0);
    }

    #[test]
    fn saturated_multipliers_stay_finite_and_never_wrap_through_rounding() {
        // Near-limit feed: everything huge, every output still finite and
        // inside the hard range - never wraps, never NaN/inf.
        let p = params();
        // `prev` here sits far ABOVE the configured ceiling (6.0). Since
        // 2026-08-23 that widening SHRINKS - one rate-limited step back
        // per fight - instead of the window absorbing prev forever, so
        // this asserts convergence rather than the old "holds at the hard
        // ceiling". The saturation property the test exists for is
        // unchanged: the value stays finite and never wraps.
        let next = update_hp_pacing_mult(1.0e6, 1.0, &[f64::MAX, f64::MAX, f64::MAX], &p).expect("updates");
        assert!(next.is_finite() && next > 0.0, "{next}");
        assert!((next - 1.0e6 / (1.0 + p.hp_step)).abs() < 1e-6, "one rate-limited step back toward the configured ceiling, got {next}");
        assert!(next < 1.0e6, "an out-of-window multiplier must converge, not hold");
        assert_eq!(sat_round_stat(1.0e15 * DYNAMIC_MULT_HARD_CEILING), u64::MAX, "saturates, never wraps");
        assert_eq!(sat_round_stat(f64::NAN), 1, "NaN never produces a zero-stat enemy");
        assert_eq!(sat_round_stat(-5.0), 1);
        assert_eq!(sanitize_mult(f64::NAN), 1.0);
        assert_eq!(sanitize_mult(f64::NEG_INFINITY), 1.0);
        assert_eq!(sanitize_override_mult(f64::NAN), 1.0);
        assert_eq!(clamp_rate_limited(f64::NAN, 100.0, 0.25, 0.4, 6.0).is_finite(), true);
        assert!(clamp_rate_limited(1.0, f64::MAX, 0.25, 0.4, 6.0).is_finite());
    }

    #[test]
    fn poisoned_tunables_substitute_shipped_defaults() {
        let mut t = LiveTunables::default();
        t.target_duration_min_s = f64::NAN;
        t.target_duration_max_s = f64::INFINITY;
        t.hp_max_step_per_fight = f64::NAN;
        t.target_win_loss_ratio = f64::NAN;
        t.pacing_window_fights = 0;
        let p = PacingParams::from_tunables(&t);
        assert_eq!(p.duration_min_s, defaults::TARGET_DURATION_MIN_S);
        assert_eq!(p.duration_max_s, defaults::TARGET_DURATION_MAX_S);
        assert_eq!(p.wl_target, defaults::TARGET_WIN_LOSS_RATIO);
        assert_eq!(p.window, defaults::PACING_WINDOW_FIGHTS.clamp(1, 200) as usize);
        // Swapped min/max are silently reordered, never inverted windows.
        let mut t2 = LiveTunables::default();
        t2.target_duration_min_s = 45.0;
        t2.target_duration_max_s = 30.0;
        let p2 = PacingParams::from_tunables(&t2);
        assert_eq!(p2.duration_min_s, 30.0);
        assert_eq!(p2.duration_max_s, 45.0);
    }

    #[test]
    fn top_layer_ramps_with_stage_and_respects_both_caps() {
        let mut t = LiveTunables::default();
        // Half the tunable cap at the half-stage point.
        assert!((top_layer_for_stage(1500, &t) - 0.30).abs() < 1e-9, "{}", top_layer_for_stage(1500, &t));
        // Monotone increasing, asymptotic below the cap.
        assert_eq!(top_layer_for_stage(0, &t), 0.0);
        assert!(top_layer_for_stage(750, &t) < top_layer_for_stage(1500, &t));
        assert!(top_layer_for_stage(100_000, &t) < 0.60);
        // An absurd TUNABLE cap is clamped to the hard cap (strictly < 1).
        t.top_layer_cap_pct = 50.0;
        assert!((top_layer_for_stage(1_000_000, &t) - TOP_LAYER_ABSOLUTE_CAP).abs() < 1e-12);
        assert!(top_layer_for_stage(1_000_000, &t) < 1.0);
        // Disabled reads zero.
        t.top_layer_enabled = false;
        assert_eq!(top_layer_for_stage(3000, &t), 0.0);
        // Poisoned tunables substitute shipped defaults.
        t.top_layer_enabled = true;
        t.top_layer_cap_pct = f64::NAN;
        t.top_layer_half_stage = f64::NAN;
        assert!((top_layer_for_stage(1500, &t) - 0.30).abs() < 1e-9);
    }

    #[test]
    fn stage_walk_is_two_wins_per_one_loss_neutral() {
        // The pure stage walk Controller B targets: win +1, loss -2,
        // floored at 1 (see run_encounter_inner's post-fight block).
        let stage_after_win = |s: u32| s.saturating_add(1);
        let stage_after_loss = |s: u32| s.saturating_sub(2).max(1);
        assert_eq!(stage_after_loss(stage_after_win(stage_after_win(100))), 100, "2 wins : 1 loss == net zero");
        assert_eq!(stage_after_loss(3222), 3220, "a loss regresses exactly 2");
        assert_eq!(stage_after_loss(2), 1, "floored at 1");
        assert_eq!(stage_after_loss(1), 1, "already at the floor");
    }

    /// FIX 1 coverage (2026-08-23): Controller A's request must be
    /// PROPORTIONAL to the error, so the step shrinks automatically as
    /// measured duration approaches target.
    ///
    /// A's request is `mean_dps * midpoint / base_pool` - the exact
    /// multiplier that puts kill time on the target - so the move it asks
    /// for IS the error. This test pins that: sampled from multipliers
    /// where the rate-limit band is NOT the binding constraint, each
    /// successive step toward the target is strictly smaller than the
    /// last. A fixed-step law (`prev * (1 + step)`, which is what
    /// Controller B does) would move a CONSTANT 25% of `prev` at every one
    /// of these points and fail immediately.
    #[test]
    fn controller_a_steps_shrink_as_measured_duration_approaches_target() {
        let p = params();
        // True party DPS 100 against an organic pool of 1000: the
        // multiplier that lands kill time exactly on the 37.5s midpoint is
        // 100 * 37.5 / 1000 = 3.75.
        let window = vec![100.0, 100.0, 100.0];
        let mut previous_step = f64::INFINITY;
        for mult in [3.2_f64, 3.5, 3.7, 3.74, 3.749] {
            let next = update_hp_pacing_mult(mult, 1000.0, &window, &p).expect("a full window always updates");
            let step_taken = (next - mult).abs();
            let fixed_step_would_be = mult * p.hp_step;
            assert!(
                step_taken < previous_step,
                "from {mult}x the step was {step_taken}, not smaller than the previous {previous_step} - A is not braking on approach"
            );
            assert!(
                step_taken < fixed_step_would_be,
                "from {mult}x A moved {step_taken}, which is not less than the {fixed_step_would_be} a fixed-step law would take - the request is not proportional to the error"
            );
            previous_step = step_taken;
        }
        assert!(previous_step < 0.01, "the final approach step must be tiny, was {previous_step}");
    }

    /// FIX 1 coverage: a long win streak must not walk PAST the target
    /// window. This is the shape of the live incident (A ratcheted to 30x
    /// over ~15 wins), so it is asserted every fight of the streak rather
    /// than only at the end - an overshoot that is corrected later would
    /// still have shipped an unwinnable fight.
    #[test]
    fn a_long_win_streak_does_not_overshoot_the_target_window() {
        let p = params();
        let window = vec![100.0, 100.0, 100.0];
        let mut mult = 1.0_f64;
        for fight in 0..200 {
            mult = update_hp_pacing_mult(mult, 1000.0, &window, &p).expect("a full window always updates");
            let expected_s = 1000.0 * mult / 100.0;
            assert!(
                expected_s <= p.duration_max_s + 1e-9,
                "fight {fight}: A drove expected duration to {expected_s}s, past the {}s window ceiling",
                p.duration_max_s
            );
        }
        // And it actually arrived, rather than passing the test by never
        // moving at all.
        let settled_s = 1000.0 * mult / 100.0;
        assert!(settled_s >= p.duration_min_s, "A must reach the window, settled at {settled_s}s");
    }
}











