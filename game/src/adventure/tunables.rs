use super::*;

/// The "main dials" for drop rates and boss difficulty, live-editable via
/// the admin-only `/admin/tunables` web page (see `adventure_web.rs`) with
/// no recompile AND no bot restart required - unlike
/// `ItemBalanceFile`/`load_item_balance_file` (`balance.rs`), which is only
/// ever read once per process lifetime (each consumer caches it behind a
/// `OnceLock`), this is held live in `AdventureManager` behind a
/// `std::sync::RwLock` and re-read on every fight, so a saved change takes
/// effect on the very next encounter. Deliberately a SEPARATE file/struct
/// from `ItemBalanceFile` rather than folded in - that one covers unrelated
/// craft/affix/item-power tuning, this one is specifically the "how much
/// loot drops" / "how hard is the boss" knobs (per an explicit live
/// request - not the full ~20+ constant list, just the ones actually worth
/// tuning day-to-day; the dynamic-difficulty rubber-band internals and the
/// hard safety caps stay compile-time constants for now).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveTunables {
    /// Was `LOOT_MULT` - global dust/item-drop/craft-token multiplier,
    /// applied on both boss and basic-encounter wins.
    pub loot_mult: f64,
    /// New - sand grants (boss win, basic win, disenchant roll) were
    /// previously hardcoded flat, deliberately unscaled by `loot_mult`.
    /// Defaults to 1.0 (matches shipped behavior); dial down to cut sand
    /// income without touching dust/item drops.
    pub sand_mult: f64,
    /// Was `WINGS_DROP_CHANCE`.
    pub wings_drop_chance: f64,
    /// Was `CELESTIAL_SHARD_DROP_CHANCE`.
    pub celestial_shard_drop_chance: f64,
    /// Consolidated (2026-08-16) from 5 overlapping dials that all used to
    /// multiply into boss HP together (`difficulty_mult`,
    /// `boss_difficulty_dial`, `boss_hp_mult`, and `late_content_difficulty_mult`
    /// past a stage threshold) - a live request that there were "way too
    /// many boss multipliers all over the place." One number now: a plain
    /// multiplier on top of `boss_stats_for`'s own base HP coefficient.
    /// Late-content difficulty is no longer its own automatic bump - just
    /// raise this (and `boss_power` below) manually as the world stage
    /// climbs.
    pub boss_health: f64,
    /// Same consolidation as `boss_health`, for boss ATK (was
    /// `difficulty_mult` * `boss_difficulty_dial` * `boss_damage_mult` *
    /// late-content bump).
    pub boss_power: f64,
    /// Consolidated (2026-08-16) from 6 compile-time constants
    /// (`LOSS_POWER_DECAY`, `WIN_MAX_BOOST`, `TARGET_WIN_RATE`,
    /// `WIN_TARGET_MARGIN_RATIO`, `OUTCOME_WINDOW`, `BOSS_POWER_MULT_MIN`)
    /// that drove the win/loss rubber-band (`WorldState::boss_power_mult`)
    /// with no single live-editable dial over how reactive it is. This one
    /// scales how far EVERY win's boost / loss's decay actually moves
    /// `boss_power_mult` from its current value, toward the same target it
    /// would've reached anyway: `1.0` = unchanged (identical to the old
    /// hardcoded behavior), `0.0` = win/loss streaks stop affecting boss
    /// power at all, `>1.0` = swings harder/faster in both directions. See
    /// `run_encounter`'s win/loss branch for the actual application.
    pub dynamic_scaling_mult: f64,
    /// Boss-count formula (2026-08-17 replacement of the old fixed
    /// `two_boss_stage`/`three_boss_stage`/`four_boss_stage`/
    /// `five_boss_stage` thresholds - a live request: "1 boss + jitter,
    /// +1-2 bosses per 100 stages capped at stage/100*1.5"). How many
    /// world-stage points make up one "tier" - `stage / this` (floored) is
    /// the tier count `boss_count_for_stage` rolls jitter and computes the
    /// cap from. 100 by default, matching the request exactly.
    pub boss_count_tier_stages: u32,
    /// Same formula - the hard ceiling on total boss count is
    /// `floor(tiers * this)`, so the jitter (which on its own could climb
    /// as high as 1 + 2*tiers) never produces an absurdly overloaded
    /// fight. 1.5 by default, matching the request exactly.
    pub boss_count_cap_mult: f64,
    /// Was `LATE_CONTENT_STAGE`. Kept as its own field even after the
    /// 2026-08-16 consolidation (see `boss_health`'s doc) - unlike
    /// `late_content_difficulty_mult`, which really was only ever a boss
    /// stat multiplier, this stage number is ALSO the gate for the
    /// guaranteed-Perfect-item-per-character/per-kill milestones in
    /// `run_encounter` (unrelated to boss combat stats), so it stays a
    /// real, separate dial.
    pub late_content_stage: u32,
    /// Permanent Rampage (2026-08-16, admin toggle) - unlike `!rampage`
    /// (which queues a finite `RAMPAGE_ENCOUNTER_COUNT`, see
    /// `AdventureManager::start_rampage`), this never runs out: while
    /// true, `spawn_rampage_loop` runs boss encounters back-to-back
    /// forever (same instant-revive-between-fights behavior) until an
    /// admin turns it back off. Persisted (unlike `rampage_remaining`,
    /// which is deliberately in-memory-only/cleared by a restart) - a
    /// toggle like this should survive a crash/restart, not silently
    /// revert.
    pub permanent_rampage: bool,
    /// Boss pierce (2026-08-18, a live design call) - the asymptotic
    /// ceiling `boss_pierce_pct` climbs toward as stage grows (see
    /// `simulate_battle`'s own computation) - a stage-scaled fraction of
    /// every REAL boss attack that resolves as unavoidable/unmitigable
    /// true damage, the rest still running the full normal mitigation
    /// pipeline. Never actually reached, only approached - a `pierce_cap`
    /// of 0.0 is exactly today's pre-pierce behavior (the formula floors
    /// to 0 at every stage regardless of `pierce_h`).
    pub pierce_cap: f64,
    /// The stage at which `boss_pierce_pct` reaches HALF of `pierce_cap`
    /// (the curve's own half-saturation point) - see `simulate_battle`'s
    /// computation. Lower means pierce ramps up faster at earlier stages.
    pub pierce_h: f64,
    /// Fight-announcement batching (2026-08-19, a live request to cut
    /// per-fight chat spam) - how many encounter results
    /// (`announce_encounter_result`'s per-fight "party of N heroes..."
    /// line, Basic and Boss alike) accumulate into one pending batch
    /// before `flush_fight_summary_batch` posts a single aggregated
    /// summary instead. See `announcements::aggregate_batch`/
    /// `format_batch_summary` for the aggregation/formatting itself -
    /// this is purely "how many," re-read on every fight so a change
    /// takes effect on the next accumulated fight, not the next restart.
    /// 1 would mean "batch of 1" - functionally identical to today's
    /// per-fight behavior, a safe way to fully disable batching without
    /// a separate on/off flag.
    pub fight_summary_batch_size: u32,
    /// Elementalist's Thunder Golem absorbed-damage redistribution
    /// (docs/elementalist_spec.md, Release 1 Part B5) - what fraction of
    /// an incarnation's total absorbed damage (`thundergolem_absorbed_this_incarnation`)
    /// gets split among the party as an unmitigated DoT when it dies. 0.0
    /// disables redistribution entirely (nothing ever gets scheduled -
    /// see `handle_golem_death`'s own `redistribution_pct > 0.0` guard).
    pub thunder_redistribution_pct: f64,
    /// Same mechanic - total time (seconds) the 2-tick redistribution DoT
    /// is spread across (tick 1 at half this, tick 2 at the full amount).
    pub thunder_redistribution_window_secs: f64,
    /// Warrior's Retaliation / the shared Rogue's Voidstep, Monk's
    /// Counterflow, Druid's Wild Fury group - the deliberate "at most one
    /// real counter-attack per this many ms" cap on the evade-counter
    /// group specifically (see `evade_counter_last_fired_at_ms`'s own
    /// doc; Retaliation itself has no such cap). Release 1.2 spec-owner
    /// ruling (2026-08-19): a LiveTunable, default matches the previous
    /// hardcoded 1000ms exactly - no behavior change at default.
    pub reactive_proc_cap_ms: u32,
    /// Divine Dust (2026-08-19, docs/divine_dust_spec.md) - chance per
    /// fighting character, per WIN (boss or basic, same eligibility as
    /// `sand`'s own unconditional win grant - see `run_encounter`/
    /// `run_basic_encounter`), of receiving exactly 1 Divine Dust. Same
    /// shared-value-at-both-encounter-kinds shape as `wings_drop_chance`/
    /// `celestial_shard_drop_chance`. Default (0.1) is 1/10th of sand's
    /// own fight-grant rate - sand's grant is UNCONDITIONAL (an implicit
    /// rate of 1.0), so 1/10th of that is 0.1.
    pub divine_dust_drop_chance: f64,
    /// Same mechanic - chance per SACRED item manually disenchanted (see
    /// `Character::disenchant_from_inventory`/`disenchant_all_from_inventory`;
    /// never reachable via the auto-disenchant path, since a Sacred item
    /// always meets every `AutoDisenchantTier` floor) of receiving 1
    /// Divine Dust. Default (0.1) is 1/10th of `roll_disenchant_sand`'s
    /// own chance for a Sacred item specifically - that chance is
    /// `quality_percent() / 100`, and a Sacred item is always `perfect`
    /// (quality 100%), so sand's own rate here is also an implicit 1.0.
    pub divine_dust_disenchant_chance: f64,
    /// Divine Dust craft recipe (docs/divine_dust_spec.md) - dust cost of
    /// crafting 1 Divine Dust on `/craft` (paired with
    /// `divine_dust_craft_sand_cost`, x1/x10/x50 batchable). Deliberately
    /// cheap relative to veteran dust holdings - `divine_dust_craft_sand_cost`
    /// is the intended pacing constraint, not this.
    pub divine_dust_craft_dust_cost: u64,
    /// Same recipe - sand cost.
    pub divine_dust_craft_sand_cost: u64,
    /// Same recipe - Divine Dust granted per craft (before the x1/x10/x50
    /// multiplier, which just repeats the whole recipe that many times).
    pub divine_dust_craft_output: u64,
}

impl Default for LiveTunables {
    fn default() -> Self {
        Self {
            loot_mult: 1.3,
            sand_mult: 1.0,
            wings_drop_chance: 0.0001,
            celestial_shard_drop_chance: 0.001,
            // Plain 1.0 baseline now that this is a single consolidated
            // dial (see the field's own doc) - the base HP/ATK coefficients
            // in `boss_stats_for` already encode the intended starting
            // difficulty, so 1.0 here means "as designed," not "off."
            boss_health: 1.0,
            boss_power: 1.0,
            dynamic_scaling_mult: 1.0,
            boss_count_tier_stages: 100,
            boss_count_cap_mult: 1.5,
            late_content_stage: 100,
            permanent_rampage: false,
            pierce_cap: 0.5,
            pierce_h: 2000.0,
            fight_summary_batch_size: 10,
            thunder_redistribution_pct: 0.50,
            thunder_redistribution_window_secs: 2.0,
            reactive_proc_cap_ms: 1_000,
            divine_dust_drop_chance: 0.1,
            divine_dust_disenchant_chance: 0.1,
            divine_dust_craft_dust_cost: 1000,
            divine_dust_craft_sand_cost: 10,
            divine_dust_craft_output: 1,
        }
    }
}

pub(crate) const TUNABLES_PATH: &str = "adventure-live-tunables.toml";

/// Fail-soft load, same spirit as `load_item_balance_file` - missing or
/// unparseable just means "use the shipped defaults above," logged, never
/// a boot failure.
pub(crate) fn load_live_tunables() -> LiveTunables {
    match std::fs::read_to_string(data_path(TUNABLES_PATH)) {
        Ok(contents) => match toml::from_str::<LiveTunables>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::warn!("{TUNABLES_PATH} failed to parse, using built-in defaults: {err}");
                LiveTunables::default()
            }
        },
        Err(_) => LiveTunables::default(),
    }
}

/// Persists a saved admin-page edit so it survives a restart too, on top
/// of updating the live in-memory copy (see `AdventureManager::save_live_tunables`).
pub(crate) fn save_live_tunables_file(tunables: &LiveTunables) -> std::io::Result<()> {
    let contents = toml::to_string_pretty(tunables).map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
    std::fs::write(data_path(TUNABLES_PATH), contents)
}
