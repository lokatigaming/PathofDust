use super::*;

/// How often a single chat message can earn its sender XP — being
/// present/active is what's rewarded, not message volume, so this stops
/// spamming chat from being a level-up shortcut. Tripled from 60s as part
/// of a live "really slow down progression" request - at the old rate, a
/// long stream's worth of passive chatting alone (no fights needed) could
/// meaningfully out-pace the intended fight-driven leveling pace, and
/// would have become proportionally MORE dominant once victory XP got cut
/// (see the boss-win xp grant in `run_encounter`) if left untouched here.
pub(crate) const ACTIVITY_XP_COOLDOWN: Duration = Duration::from_secs(180);
pub(crate) const ACTIVITY_XP_AMOUNT: u64 = 4;

/// How often the joined roster auto-battles the next enemy.
pub(crate) const ENCOUNTER_INTERVAL: Duration = Duration::from_secs(600);

/// How many "Force Boss Fight" channel points redemptions are allowed
/// per `ENCOUNTER_INTERVAL` cycle - see `AdventureManager::forced_boss_count`.
pub(crate) const FORCE_BOSS_MAX_PER_CYCLE: u32 = 2;

/// What `AdventureManager::try_force_encounter` handed back - main.rs's
/// redemption handler turns each into the right chat line/redemption
/// status.
pub enum ForceBossOutcome {
    /// The fight actually ran.
    Triggered,
    /// Nobody was eligible to fight - the used slot was refunded.
    NobodyJoined,
    /// Already at `FORCE_BOSS_MAX_PER_CYCLE` for this cycle.
    CycleLimitReached,
}

/// What `AdventureManager::trigger_encounter_now` handed back -
/// commands.rs's !nextencounter turns each into the right chat line.
pub enum TriggerEncounterOutcome {
    /// The fight actually ran.
    Triggered,
    /// Nobody was eligible to fight.
    NobodyJoined,
    /// The optional boss-name argument didn't match anything
    /// `parse_forced_boss` recognizes.
    UnknownBoss,
}

/// `run_encounter`'s forced-boss parameter - two different "someone
/// deliberately picked the boss(es)" shapes, see each variant's own doc.
/// `None` (not a variant here) is the normal/natural roll, unaffected by
/// either of these. `Copy` (every field already is - `BossKind` and
/// `Option<&'static str>`) so `run_encounter` can read it twice (boss
/// selection, then sprite selection) without needing to thread a
/// reference through.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ForcedBoss {
    /// `!nextencounter`'s existing behavior (2026-08-15) - exactly ONE
    /// boss of this kind (+ optional sprite override for Dragon's two
    /// looks), bypassing stage-based count scaling AND the "never the
    /// same as last fight" exclusion entirely. Simplest, most predictable
    /// shape for testing a specific boss/ability.
    Single(BossKind, Option<&'static str>),
    /// `!event intro`'s behavior (2026-08-17) - every boss slot is this
    /// SAME kind, but the SLOT COUNT still follows the current stage's
    /// normal rules (see `boss_count_for_stage`), so a stage-90 intro
    /// fight still spawns 3 bosses, all the named kind - "an appropriate
    /// number of bosses/difficulty for the stage," per the request, just
    /// guaranteed to showcase the one boss being introduced. No sprite
    /// override (unlike `Single`) - forcing one specific look across every
    /// slot of a multi-boss fight would make every copy render
    /// identically, unlike a natural multi-Dragon roll's own per-instance
    /// coin flip, so each instance just picks its own look normally.
    StageScaled(BossKind),
}

/// How many bosses a real boss fight spawns at the given world `stage` -
/// extracted (2026-08-17) from `run_encounter`'s own natural-roll branch so
/// `ForcedBoss::StageScaled` (`!event intro`) can reuse the exact same
/// formula instead of duplicating it. Replaced (2026-08-17, a live
/// request: "1 boss + jitter, +1-2 bosses per 100 stages capped at
/// stage/100*1.5") the old fixed `two_boss_stage`/`three_boss_stage`/
/// `four_boss_stage`/`five_boss_stage` thresholds entirely - deliberately
/// random now (re-rolled fresh every call, i.e. every real boss fight),
/// not a deterministic function of stage alone, per the explicit ask for
/// "variance for challenge."
///
/// `tiers = stage / boss_count_tier_stages` (floored) - one independent
/// `1..=2` jitter roll per completed tier, summed onto a base of 1, then
/// clamped to never exceed `floor(tiers * boss_count_cap_mult)` (floored
/// at 1 so an empty roll never produces 0 bosses at low stages). At the
/// tier-size/cap-mult defaults (100, 1.5): stage 400 is 4 tiers, jitter
/// sums to 4-8, the cap is 6, so real fights land at 5 or 6 bosses - the
/// cap does almost all of the work of keeping this from spiraling, while
/// the jitter is what makes any two fights at the same stage genuinely
/// different.
fn boss_count_for_stage(stage: u32, tunables: &LiveTunables, rng: &mut impl Rng) -> usize {
    let tier_size = tunables.boss_count_tier_stages.max(1);
    let tiers = stage / tier_size;
    let jitter: u32 = (0..tiers).map(|_| rng.gen_range(1..=2)).sum();
    let raw = 1 + jitter;
    let cap = ((tiers as f64 * tunables.boss_count_cap_mult).floor() as u32).max(1);
    raw.min(cap) as usize
}

/// What `AdventureManager::register_rampage_vote` handed back -
/// commands.rs's !rampage (non-mod path) turns each into the right chat
/// line.
pub enum RampageVoteOutcome {
    /// This vote pushed the distinct-voter count to `RAMPAGE_VOTE_THRESHOLD`
    /// or beyond - a rampage was just started.
    Triggered,
    /// Still short of the threshold - carries the current distinct-voter
    /// count (including this vote).
    Counted(u32),
}

/// How often a much weaker, non-progression-advancing "basic enemy" fight
/// fires - see `run_basic_encounter`.
pub(crate) const BASIC_ENCOUNTER_INTERVAL: Duration = Duration::from_secs(180);

/// !rampage (mod tool, 2026-08-16) - how many boss encounters one
/// invocation queues up, per the exact request ("turns all encounters
/// into boss encounters for the next 50 encounters").
pub(crate) const RAMPAGE_ENCOUNTER_COUNT: u32 = 50;
/// The floor on how long `spawn_rampage_loop` waits between encounters -
/// per the exact request ("makes the timer between fights 1 minute (or
/// delays if the current fight is taking longer than 1 minute)"). The
/// actual wait is `max(RAMPAGE_MIN_INTERVAL, this fight's real overlay
/// playback time)`, so a long fight is never interrupted mid-replay.
pub(crate) const RAMPAGE_MIN_INTERVAL: Duration = Duration::from_secs(60);
/// !rampage vote (2026-08-17, a live request: "if 3 or more players use
/// !rampage it will start a rampage without a mod activating the
/// command, similar to a vote") - how many DISTINCT non-mod voters it
/// takes. A mod's own !rampage still triggers instantly, same as before;
/// this is purely the alternate viewer-driven path.
pub(crate) const RAMPAGE_VOTE_THRESHOLD: u32 = 3;
/// !rampage persistence (2026-08-17, a live request: "if a rampage was
/// active when the bot went down the bot should remember the rampage and
/// come back up where it left off") - unlike `rampage_votes`/
/// `forced_boss_count`, which stay deliberately in-memory-only,
/// `rampage_remaining` is now mirrored to this file on every change (see
/// `persist_rampage_remaining`) and reloaded at `AdventureManager::new`,
/// so a crash/restart mid-rampage resumes the countdown instead of
/// silently losing it. Permanent Rampage doesn't need this - it already
/// persists via `LiveTunables`/`adventure-live-tunables.toml`.
pub(crate) const RAMPAGE_STATE_PATH: &str = "adventure-rampage-state.json";

/// Flavor names for the basic-enemy encounter's "an assortment of
/// enemies" - purely cosmetic (chat wording), doesn't affect stats. No
/// dedicated sprite art for these yet (see `run_basic_encounter`'s doc
/// comment) - the overlay just reuses the boss art pool for now.
pub(crate) const BASIC_ENEMY_NAMES: &[&str] = &["a pack of Goblin Raiders", "a band of Bandits", "a horde of Skeleton Warriors", "a Dark Wizard's cultists", "a pack of Wild Wolves", "a pair of Cave Trolls"];

/// How long a knocked-out character sits out after their party's fight
/// ends before they're eligible to fight again. If the next encounter
/// (timer or !nextencounter) fires before this elapses, they're excluded
/// from that fight's roster entirely rather than fighting hurt.
pub(crate) const REVIVE_DURATION: Duration = Duration::from_secs(30);

/// How long a character with zero working gear (see
/// `Character::all_gear_worn_out`) waits before every piece of their
/// equipment auto-repairs for free - this only fixes the gear, it does
/// NOT bring them back onto the battlefield by itself (see
/// `AdventureManager::eligible_fighters`); that still needs an explicit
/// `!join` (see `JoinOutcome::Rejoined`). Repairing manually (dust cost,
/// scales with tier) or swapping in working gear from the bag also
/// doesn't auto-rejoin them anymore, for the same reason - it just means
/// there's nothing left to repair by the time they do `!join`.
pub const RETREAT_REPAIR_DURATION: Duration = Duration::from_secs(3600);

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorldState {
    stage: u32,
    /// Whichever `BossKind` the most recent REAL boss fight rolled - see
    /// `run_encounter`'s pick, which excludes this from the candidate
    /// list so the same boss (and its mechanic) never repeats back to
    /// back. Persisted (unlike the in-memory-only cooldown maps
    /// elsewhere) so a bot restart between fights still remembers who
    /// fought last, same durability as `stage` itself. `None` only ever
    /// before the very first boss fight.
    last_boss_kind: Option<BossKind>,
    /// Dynamic difficulty rubber band, multiplying every stat
    /// `boss_stats_for` produces (cascades into `basic_enemy_stats_for`
    /// too, same as every other boss-wide multiplier there) on top of the
    /// normal stage/level curve - see `post_win_power_boost`. Deliberately
    /// UNCAPPED (see `BOSS_POWER_MULT_MIN`'s doc) - per a live request,
    /// this should keep climbing for as long as the party keeps winning
    /// too comfortably, rather than pinning at an arbitrary ceiling once
    /// the party's own growth outpaces it (confirmed live: it hit the old
    /// 5.0 ceiling and sat there while stage-105 fights stayed trivial).
    /// Still floored so a losing streak can't grind it to a permanent
    /// zero-stat boss. Starts at 1.0 (`default_boss_power_mult`) - a
    /// no-op on top of the existing curve until the first win pushes it.
    #[serde(default = "default_boss_power_mult")]
    boss_power_mult: f64,
    /// Rolling window of the last `OUTCOME_WINDOW` real boss fights (true
    /// = win), oldest first - see `adaptive_difficulty_scale`'s doc. Used
    /// to keep `boss_power_mult`'s growth actually targeting a real win
    /// rate (per a live request, "an ideal 5:2 win:loss ratio") instead
    /// of just reacting to each fight's own margin in isolation, which
    /// has no way to know whether the party has been winning too much or
    /// too little lately. Empty (and thus a no-op multiplier, see
    /// `recent_win_rate`) until `OUTCOME_WINDOW` real fights have
    /// happened since this field was introduced.
    #[serde(default)]
    recent_boss_outcomes: std::collections::VecDeque<bool>,
}

/// Manual, not derived - a brand-new install goes through this (see
/// `AdventureManager::new`'s `unwrap_or_default()` when no world file
/// exists yet at all), which is a DIFFERENT path from serde's own
/// per-field `#[serde(default = ...)]` (that one only covers a field
/// missing from an EXISTING file). A derived `Default` would silently
/// hand a fresh install `boss_power_mult: 0.0` (every boss stat zeroed
/// out - a completely broken fresh game) instead of the real neutral
/// value.
impl Default for WorldState {
    fn default() -> Self {
        WorldState { stage: 0, last_boss_kind: None, boss_power_mult: default_boss_power_mult(), recent_boss_outcomes: std::collections::VecDeque::new() }
    }
}

pub(crate) fn default_boss_power_mult() -> f64 {
    1.0
}

#[derive(Debug, Clone)]
pub enum JoinOutcome {
    Joined,
    /// Characters are permanent, not re-rolled — !join again just reports
    /// where you're already at.
    AlreadyJoined { level: u32 },
    /// Was retreated (see `Character::retreated_since`) - !join is the
    /// explicit "I'm back" action that un-retreats them, same as the
    /// original design just without needing to wait out the free
    /// auto-repair or spend dust. `gear_still_worn` says whether their
    /// gear is still sitting at 0% (they'll fight at reduced power until
    /// they repair it) or the free hourly auto-repair already caught it.
    Rejoined { level: u32, gear_still_worn: bool },
}

/// One thing that happened during a simulated fight, with the timestamp
/// (ms from fight start, before display compression — see
/// `compress_events`) it happened at. `attacker`/`healer`/`target`/`unit`
/// are unit ids — either a lowercased username, or one of the
/// `enemy_unit_id` ids (see `ENEMY_ID_PREFIX`) for an enemy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CombatEvent {
    // rename_all on the enum only renames the `kind` tag values
    // (Attack -> "attack") - it does NOT cascade into each struct
    // variant's own fields, so at_ms/target_hp_after were serializing as
    // snake_case even though every client read them as atMs/
    // targetHpAfter. Since the overlay's event-firing loop gates on
    // `.atMs` (undefined, so `undefined <= x` is always false), this
    // silently meant NO event ever fired, for any fight, ever - not a
    // speed/easing problem, the combat log simply never played back.
    #[serde(rename_all = "camelCase")]
    Attack {
        at_ms: u32,
        attacker: String,
        target: String,
        damage: u64,
        /// What `damage` would have been without the target's evasion/
        /// block/damage-reduction mitigation - see `HitOutcome`'s doc.
        /// Only `summarize_fight`'s "damage taken" stat uses this;
        /// everything else (hp math, the overlay's hit-number display)
        /// still uses the real, post-mitigation `damage`.
        unmitigated_damage: u64,
        target_hp_after: u64,
        is_crit: bool,
        evaded: bool,
        /// Correlates this hit with every `RollEvent` that fed into it
        /// (2026-08-17, full-detail combat log) - shared by this event and
        /// the detail-tier-only rolls that computed it (evasion/block/crit
        /// rolls, mitigation sources, ...), never the other way around.
        /// `#[serde(default)]` so events saved before this field existed
        /// still parse, reading as 0 (a real "N/A", real hit ids start at 1).
        /// The Curse of Weakness credit-split (two `Attack` events for one
        /// real hit) gives both halves the SAME `hit_id` - it's one hit.
        #[serde(default)]
        hit_id: u64,
    },
    #[serde(rename_all = "camelCase")]
    Heal { at_ms: u32, healer: String, target: String, amount: u64, target_hp_after: u64 },
    /// A shield grant (Overflowing Grace, Divine Favor, Martyrdom - see
    /// `grant_shield`) - deliberately a SEPARATE variant from `Heal`, not
    /// reused for it, even though `summarize_fight`'s "healing done" stat
    /// counts both (see its doc): a shield doesn't change `hp` at all, so
    /// there's no `target_hp_after` field to report, and two real
    /// consumers specifically need to keep telling shields and real heals
    /// apart - `post_win_power_boost` only reads `Heal` when computing how
    /// much incoming damage the party "undid," and a shield's mitigation
    /// is ALREADY reflected there via the reduced `Attack.damage` it
    /// produced, so also counting it as a `Heal` would double-count the
    /// same mitigation twice; and the one-time Celestial Shard "top
    /// healer" award (main.rs) only rewards real healing, not shielding.
    #[serde(rename_all = "camelCase")]
    Shield { at_ms: u32, healer: String, target: String, amount: u64 },
    #[serde(rename_all = "camelCase")]
    Defeat { at_ms: u32, unit: String },
    /// A unit activated an `ArchetypeSkill` with a visible effect (e.g.
    /// Flicker Strike's dash) - purely a presentation signal for
    /// replay/overlay purposes, doesn't itself carry any combat math
    /// (the actual hits it triggers are still separate `Attack` events
    /// right after this one). `skill` is `ArchetypeSkill::name()`, a
    /// plain string rather than a typed enum, so the overlay can key off
    /// it without needing its own copy of every skill ever added.
    #[serde(rename_all = "camelCase")]
    SkillCast { at_ms: u32, unit: String, skill: String },
    /// Combat logging (2026-08-15, a live request: "a robust log
    /// system... live buff/debuff stack counts") - a full snapshot of
    /// every currently-active live buff/debuff/charge/stack `unit` has
    /// at `at_ms` (see `active_buffs_snapshot`'s doc for exactly what's
    /// covered), fired alongside every `Attack`/`Heal` event for both
    /// participants (see `apply_hit`/`apply_heal`'s own emission sites) -
    /// so a fight's full buff/debuff timeline is reconstructible after
    /// the fact without needing a dedicated event at every one of these
    /// mechanics' own dozens of individual mutation sites. Never fired
    /// with an empty `buffs` list (the vast majority of hits/heals touch
    /// a unit with nothing special active) - a live volume concern, not
    /// a data-completeness one: doubling+ the event count of every fight
    /// with mostly-empty snapshots would bloat the coarse-tier log's
    /// (`COARSE_FIGHTS_CAPACITY`-fight) history considerably for little
    /// analytical value, and
    /// "no snapshot logged" is already an unambiguous "nothing was
    /// active" for anyone parsing the log.
    #[serde(rename_all = "camelCase")]
    BuffSnapshot { at_ms: u32, unit: String, buffs: Vec<(String, f64)> },
}

impl CombatEvent {
    pub(crate) fn at_ms(&self) -> u32 {
        match self {
            CombatEvent::Attack { at_ms, .. }
            | CombatEvent::Heal { at_ms, .. }
            | CombatEvent::Shield { at_ms, .. }
            | CombatEvent::Defeat { at_ms, .. }
            | CombatEvent::SkillCast { at_ms, .. }
            | CombatEvent::BuffSnapshot { at_ms, .. } => *at_ms,
        }
    }

    /// The unit "responsible" for this event - the attacker/healer for
    /// Attack/Heal/Shield, the unit itself for Defeat/SkillCast/
    /// BuffSnapshot. Used by `thin_events_for_overlay` to classify each
    /// event as player-caused or boss-caused (via `units`' own `is_boss`).
    pub(crate) fn actor_id(&self) -> &str {
        match self {
            CombatEvent::Attack { attacker, .. } => attacker,
            CombatEvent::Heal { healer, .. } => healer,
            CombatEvent::Shield { healer, .. } => healer,
            CombatEvent::Defeat { unit, .. } => unit,
            CombatEvent::SkillCast { unit, .. } => unit,
            CombatEvent::BuffSnapshot { unit, .. } => unit,
        }
    }

    /// Returns a copy of this event with its timestamp replaced — used
    /// by `compress_events` to rescale a whole log at once.
    pub(crate) fn with_at_ms(self, at_ms: u32) -> Self {
        match self {
            CombatEvent::Attack { attacker, target, damage, unmitigated_damage, target_hp_after, is_crit, evaded, hit_id, .. } => {
                CombatEvent::Attack { at_ms, attacker, target, damage, unmitigated_damage, target_hp_after, is_crit, evaded, hit_id }
            }
            CombatEvent::Heal { healer, target, amount, target_hp_after, .. } => {
                CombatEvent::Heal { at_ms, healer, target, amount, target_hp_after }
            }
            CombatEvent::Shield { healer, target, amount, .. } => CombatEvent::Shield { at_ms, healer, target, amount },
            CombatEvent::Defeat { unit, .. } => CombatEvent::Defeat { at_ms, unit },
            CombatEvent::SkillCast { unit, skill, .. } => CombatEvent::SkillCast { at_ms, unit, skill },
            CombatEvent::BuffSnapshot { unit, buffs, .. } => CombatEvent::BuffSnapshot { at_ms, unit, buffs },
        }
    }
}

/// The kind of named mechanic a `RollEvent` records - deliberately a
/// small, stable set of BUCKETS (not one variant per mechanic - see
/// `RollEvent::source` for why), so a later phase adding a new passive
/// node/affix never needs a new match arm here, only a new `source`
/// string under whichever category it already fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RollCategory {
    /// A genuine `rng.gen_bool` roll - crit remainder, evasion, block,
    /// Cold Steel pass-along, elemental procs.
    Crit,
    Evasion,
    Block,
    /// Cold Steel's own pass-along roll ("does an earlier guaranteed-
    /// landing hit's debuff carry to this one too") - kept distinct from
    /// `Evasion`/`Block` since it's not itself a mitigation roll, it's
    /// what decides whether THOSE rolls happen at all this hit.
    GuaranteedHit,
    ElementalProc,
    /// A deterministic (non-probabilistic) damage-reduction source that
    /// contributed to `combine_reduction_sources` for this hit.
    Mitigation,
    /// A deterministic increased-damage source that scaled this hit's
    /// base damage up before mitigation.
    IncreasedDamage,
    StackBuilder,
    Leech,
    Reflect,
    OnKill,
    MarkOrCurse,
    DamageCredit,
    ShieldAbsorb,
}

/// One named mechanic's contribution to a single hit - the full-detail
/// combat log's core unit (2026-08-17, "every roll made needs to be
/// logged and every actor involved needs to be identified"). Extends the
/// ONE place this already happened before this event type existed
/// (Curse of Weakness's damage-credit split, a plain second `Attack`
/// event) to every other named mechanic in `resolve_hit`/`apply_hit`.
///
/// Deliberately ONE generic struct, not 40-50 bespoke event types - this
/// codebase already has a documented pattern for avoiding parallel-
/// match-arm sprawl (see the `AffixDef`/`CraftActionDef` unification).
/// `source` carries the specific mechanic name as a plain string
/// constant rather than its own giant enum, so adding a new passive node
/// never needs a match arm here, just a new `&'static str` at its own
/// call site - `&'static str` (not `String`) since these names are
/// always compile-time-known, avoiding a heap allocation on every roll
/// at this volume.
///
/// Deliberately NOT a `CombatEvent` variant - `EncounterResult.events`
/// is the same `Vec` broadcast live to the OBS overlay via
/// `encounter_tx` for real-time animation, not just persisted. Mixing
/// 10-40x more entries per hit into that stream would either break the
/// overlay's own exhaustive `match` over `CombatEvent` or wreck its
/// playback pacing within `compress_events`' display window. Kept in
/// its own `EncounterResult.rolls` vec instead - only the detail-tier
/// persistence path (`fight_storage`) ever reads it, and it's never
/// sent over `encounter_tx`.
///
/// Only logged when a source actually contributed something non-zero/
/// active - same filtering precedent `active_buffs_snapshot` already
/// established (see its own doc) - so event volume tracks what's
/// actually built/invested on a given character, not the total mechanic
/// count in the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollEvent {
    pub event_id: u64,
    /// Shared by this roll and every OTHER roll/event that fed into (or
    /// resulted from) the SAME hit - symmetric grouping, not a causal
    /// chain. See `CombatEvent::Attack::hit_id`'s doc.
    pub hit_id: u64,
    /// A genuine causal link to another event/roll's own id, for the
    /// cases `hit_id` alone can't express: one COMPLETED event later
    /// triggering a separate one (a hit causing a reflect, a kill
    /// triggering an on-kill proc, a `SkillCast` triggering the `Attack`
    /// it casts). `None` for the common case (a roll that's just one of
    /// this hit's own mitigation/crit/etc. sources).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<u64>,
    pub at_ms: u32,
    pub category: RollCategory,
    /// The specific named mechanic, e.g. "Hardened", "Curse of Weakness",
    /// "Overwhelm/Crush shred" - see the struct doc for why this is a
    /// plain string, not its own enum. `Cow<'static, str>` rather than a
    /// bare `&'static str`: every call site in `combat.rs` only ever
    /// constructs this from a compile-time string literal
    /// (`Cow::Borrowed`, zero allocation - the same reasoning that ruled
    /// out `String`), but `&'static str` can't implement `Deserialize`
    /// (would require the deserializer's own input to be `'static`,
    /// which JSON read from disk never is) - `Cow` gets both: free to
    /// construct in the hot path, and reads back fine as an owned
    /// `String` when a detail-tier file is loaded.
    pub source: std::borrow::Cow<'static, str>,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// `Some(chance)` for a genuine `rng.gen_bool` roll, `None` for a
    /// deterministic source (a flat DR% that just always applies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    /// The actual roll outcome, when `probability` is `Some` - `None` for
    /// deterministic sources (nothing to succeed/fail at).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub succeeded: Option<bool>,
    /// The numeric contribution - a DR fraction, a flat leech amount, a
    /// crit multiplier, etc.; meaning depends on `category`/`source`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,
}

/// One combatant's starting info for a fight — sent once per encounter
/// alongside its event log so the overlay knows everyone's max HP/role
/// (including the boss's) without having to infer it from the events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatUnitInfo {
    pub id: String,
    pub display_name: String,
    pub is_boss: bool,
    pub role: Option<CombatFunction>,
    pub max_hp: u64,
}

/// Post-fight leaderboard - the top 3 by damage dealt ("DPS"), top 3 by
/// damage taken ("tanks"), top 3 by healing done, and who died first,
/// computed after the fact from a fight's own event log (see
/// `summarize_fight`) rather than tracked live during `simulate_battle` -
/// purely a presentation-layer summary for the boss-fight chat
/// announcement (see main.rs), nothing here feeds back into the sim
/// itself. Each `Vec` is sorted descending by amount and capped at 3,
/// empty (not padded) if fewer than 3 players contributed to that
/// category at all - e.g. a 2-healer party's `top_healing_done` just has
/// 2 entries, not a 3rd fabricated zero.
#[derive(Debug, Clone, Default)]
pub struct FightSummary {
    pub top_damage_dealt: Vec<(String, u64)>,
    pub top_damage_taken: Vec<(String, u64)>,
    pub top_healing_done: Vec<(String, u64)>,
    pub first_to_die: Option<String>,
}

/// How many entries each `FightSummary` leaderboard keeps - "top 3" per
/// the request.
pub(crate) const FIGHT_SUMMARY_TOP_N: usize = 3;

/// Builds a `FightSummary` from one encounter's `units`/`events` (see
/// `EncounterResult`) - totals every `Attack`/`Heal`/`Shield` event's
/// amount per PLAYER (never the boss/enemies - `is_player` filters those
/// out on both the giving and receiving end), and picks out whichever
/// player's `Defeat` event has the earliest `at_ms`. `top_healing_done`
/// folds `Heal` and `Shield` into the SAME total - a shield is a source
/// of healing too, it prevents damage the same way a heal undoes it.
/// "Damage taken" specifically
/// uses each hit's `unmitigated_damage`, not `damage` - a heavily
/// defensive character (a "tank") should show the real incoming threat
/// they shrugged off (evasion/block/damage reduction all included), not
/// just what leaked through to their hp. "Damage dealt" still uses the
/// real, post-mitigation `damage` - that's what actually landed.
pub fn summarize_fight(units: &[CombatUnitInfo], events: &[CombatEvent]) -> FightSummary {
    let is_player = |id: &str| units.iter().find(|u| u.id == id).map(|u| !u.is_boss).unwrap_or(false);
    let display_name = |id: &str| units.iter().find(|u| u.id == id).map(|u| u.display_name.clone());

    let mut damage_dealt: HashMap<String, u64> = HashMap::new();
    let mut damage_taken: HashMap<String, u64> = HashMap::new();
    let mut healing_done: HashMap<String, u64> = HashMap::new();
    let mut first_death: Option<(u32, String)> = None;

    for event in events {
        match event {
            CombatEvent::Attack { attacker, target, damage, unmitigated_damage, .. } => {
                if is_player(attacker) {
                    *damage_dealt.entry(attacker.clone()).or_insert(0) += *damage as u64;
                }
                if is_player(target) {
                    *damage_taken.entry(target.clone()).or_insert(0) += *unmitigated_damage as u64;
                }
            }
            CombatEvent::Heal { healer, amount, .. } => {
                if is_player(healer) {
                    *healing_done.entry(healer.clone()).or_insert(0) += *amount as u64;
                }
            }
            // A shield is a source of healing too - it prevents damage
            // the same way a heal undoes it, per the request - counted
            // into the SAME "Top Heals" total as real Heal events. Kept
            // as its own event variant rather than reusing Heal (see
            // `CombatEvent::Shield`'s doc) since two OTHER consumers
            // (post_win_power_boost, the Celestial Shard award) need to
            // keep telling the two apart; this is the one place that
            // deliberately treats them the same.
            CombatEvent::Shield { healer, amount, .. } => {
                if is_player(healer) {
                    *healing_done.entry(healer.clone()).or_insert(0) += *amount as u64;
                }
            }
            CombatEvent::Defeat { unit, at_ms } => {
                if is_player(unit) && first_death.as_ref().map_or(true, |(t, _)| *at_ms < *t) {
                    first_death = Some((*at_ms, unit.clone()));
                }
            }
            CombatEvent::SkillCast { .. } => {}
            CombatEvent::BuffSnapshot { .. } => {}
        }
    }

    let top = |totals: HashMap<String, u64>| -> Vec<(String, u64)> {
        let mut ranked: Vec<(String, u64)> = totals.into_iter().filter_map(|(id, v)| display_name(&id).map(|name| (name, v))).collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        ranked.truncate(FIGHT_SUMMARY_TOP_N);
        ranked
    };

    FightSummary {
        top_damage_dealt: top(damage_dealt),
        top_damage_taken: top(damage_taken),
        top_healing_done: top(healing_done),
        first_to_die: first_death.and_then(|(_, id)| display_name(&id)),
    }
}

/// Same top-3-per-category shape `summarize_fight` produces, built from
/// an already-accurate `FightSummarySnapshot` instead of re-walking a
/// fight's own event log (2026-08-18, a live bug report) - for a
/// consumer (main.rs's chat report) that receives `EncounterResult` off
/// the broadcast AFTER `thin_events_for_overlay` has already run on its
/// `events`, where `summarize_fight(&result.units, &result.events)`
/// would silently under-count on a big fight. `summary` was computed
/// from the FULL untouched log before thinning (see `save_last_fight`),
/// so this always reflects the real totals. The `> 0` filters preserve
/// `summarize_fight`'s own documented "empty, not padded with fabricated
/// zeros" behavior for a category nobody contributed to.
pub fn fight_summary_from_snapshot(summary: &FightSummarySnapshot) -> FightSummary {
    let top = |mut entries: Vec<(String, u64)>| -> Vec<(String, u64)> {
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(FIGHT_SUMMARY_TOP_N);
        entries
    };
    FightSummary {
        top_damage_dealt: top(summary.players.iter().filter(|p| p.damage_dealt > 0).map(|p| (p.display_name.clone(), p.damage_dealt)).collect()),
        top_damage_taken: top(summary.players.iter().filter(|p| p.damage_taken > 0).map(|p| (p.display_name.clone(), p.damage_taken)).collect()),
        top_healing_done: top(summary.players.iter().filter(|p| p.healing_done > 0).map(|p| (p.display_name.clone(), p.healing_done)).collect()),
        first_to_die: summary.first_to_die.clone(),
    }
}

/// The display name of whichever player died first in this fight (by
/// `Defeat` event `at_ms`), if anyone did - the same logic
/// `summarize_fight` tracks inline, kept as its own standalone pass here
/// (rather than extracted out of `summarize_fight`) so that already-
/// deployed function's control flow and single-pass cost stay untouched.
/// Used by `full_player_fight_stats`'s summary builder.
fn first_player_to_die(units: &[CombatUnitInfo], events: &[CombatEvent]) -> Option<String> {
    let is_player = |id: &str| units.iter().find(|u| u.id == id).map(|u| !u.is_boss).unwrap_or(false);
    let display_name = |id: &str| units.iter().find(|u| u.id == id).map(|u| u.display_name.clone());
    let mut first_death: Option<(u32, String)> = None;
    for event in events {
        if let CombatEvent::Defeat { unit, at_ms } = event {
            if is_player(unit) && first_death.as_ref().map_or(true, |(t, _)| *at_ms < *t) {
                first_death = Some((*at_ms, unit.clone()));
            }
        }
    }
    first_death.and_then(|(_, id)| display_name(&id))
}

/// Per-player aggregates for one fight, one row per participant even if
/// they contributed nothing (unlike `FightSummary`'s top-3-only
/// leaderboards) - the lightweight data `FightSummarySnapshot` persists
/// and `/fights.json` serves (2026-08-18, the `/fights.json` size/latency
/// fix). `hits`/`crits`/`evaded` are about this player's own outgoing
/// attacks: `hits` = landed (non-evaded) attacks, `crits` a subset of
/// those, `evaded` = attacks this player threw that the target dodged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerFightStats {
    pub id: String,
    pub display_name: String,
    pub damage_dealt: u64,
    pub damage_taken: u64,
    pub healing_done: u64,
    pub hits: u32,
    pub crits: u32,
    pub evaded: u32,
}

/// Builds a full (never truncated) per-player breakdown for one fight -
/// deliberately a separate function from `summarize_fight` rather than a
/// refactor of it, since that function's top-3-truncated output already
/// backs the live chat announcement and `render_fights_page`, both
/// currently deployed. Seeded from `units` (every non-boss unit gets an
/// entry, even one nothing here ever touches) rather than only entries
/// event data happens to mention. `damage_taken` counts each hit's
/// `unmitigated_damage` regardless of `evaded` - same "real incoming
/// threat, not just what leaked through" semantic `summarize_fight`'s own
/// damage_taken already uses.
pub(crate) fn full_player_fight_stats(units: &[CombatUnitInfo], events: &[CombatEvent]) -> Vec<PlayerFightStats> {
    let mut stats: HashMap<String, PlayerFightStats> = units
        .iter()
        .filter(|u| !u.is_boss)
        .map(|u| {
            (
                u.id.clone(),
                PlayerFightStats { id: u.id.clone(), display_name: u.display_name.clone(), ..Default::default() },
            )
        })
        .collect();
    for event in events {
        match event {
            CombatEvent::Attack { attacker, target, damage, unmitigated_damage, is_crit, evaded, .. } => {
                if let Some(s) = stats.get_mut(attacker) {
                    if *evaded {
                        s.evaded += 1;
                    } else {
                        s.hits += 1;
                        s.damage_dealt += *damage as u64;
                        if *is_crit {
                            s.crits += 1;
                        }
                    }
                }
                if let Some(s) = stats.get_mut(target) {
                    s.damage_taken += *unmitigated_damage as u64;
                }
            }
            CombatEvent::Heal { healer, amount, .. } | CombatEvent::Shield { healer, amount, .. } => {
                if let Some(s) = stats.get_mut(healer) {
                    s.healing_done += *amount as u64;
                }
            }
            CombatEvent::Defeat { .. } | CombatEvent::SkillCast { .. } | CombatEvent::BuffSnapshot { .. } => {}
        }
    }
    stats.into_values().collect()
}

/// The lightweight per-fight aggregate `/fights.json` and the encounter
/// WebSocket broadcast serve (2026-08-18) instead of the full coarse-tier
/// snapshot (`LastFightSnapshot`, which carries the entire event log) -
/// a few KB regardless of how many events the real fight generated.
/// Persisted via `save_summary_fight`/`recent_summary_fights`
/// (`fight_storage.rs`); built once per fight in `save_last_fight` and
/// also carried on `EncounterResult::summary` for the broadcast, so it's
/// computed from the FULL event log exactly once, never from the
/// overlay-thinned copy (`thin_events_for_overlay` runs on `result.events`
/// AFTER this is built).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FightSummarySnapshot {
    pub kind: EncounterKind,
    pub stage: u32,
    pub won: bool,
    pub started_at_unix_ms: u64,
    pub display_duration_ms: u32,
    pub participants: usize,
    pub players: Vec<PlayerFightStats>,
    pub first_to_die: Option<String>,
    pub loot: Vec<LootDrop>,
    pub broken: Vec<BrokenItem>,
}

/// Boss (the !nextencounter/10-minute progression fight) or Basic (the
/// once-a-minute filler fight against a weaker assortment of enemies -
/// see `run_basic_encounter`) - main.rs uses this to pick the right chat
/// wording (only a Boss win advances the stage). The overlay doesn't
/// care - both play back through the identical animation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EncounterKind {
    #[default]
    Boss,
    Basic,
}

/// Broadcast the moment an encounter resolves — main.rs subscribes to
/// announce it in chat, and the overlay subscribes to the same event to
/// replay the fight.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterResult {
    pub kind: EncounterKind,
    /// The stage the party just fought at (not incremented yet even on a
    /// win — see `won`).
    pub stage: u32,
    pub won: bool,
    pub participants: Vec<String>,
    pub units: Vec<CombatUnitInfo>,
    pub events: Vec<CombatEvent>,
    /// How long the overlay should take to play `events` back — see
    /// `compress_events`, NOT simply the last event's `at_ms` (that's the
    /// fight's real, uncompressed length).
    pub display_duration_ms: u32,
    /// Empty on a loss - see `run_encounter`'s loot roll (one drop per 5
    /// participants, rounded up).
    pub loot: Vec<LootDrop>,
    /// Gear that hit the end of its lifespan and broke this fight - see
    /// the post-fight decay step in `run_encounter`.
    pub broken: Vec<BrokenItem>,
    /// Flavor name for the enemy group (e.g. "a pack of Goblin Raiders") -
    /// only set for `EncounterKind::Basic`; Boss fights don't need this,
    /// the overlay's own boss sprite variety covers that.
    pub enemy_name: Option<String>,
    /// How many individual enemies made up the group - only set for
    /// `EncounterKind::Basic` (randomized 0.5x-1.5x party size, see
    /// `run_basic_encounter`; drives both its difficulty and its loot
    /// roll). Boss fights are always exactly one boss.
    pub enemy_count: Option<u32>,
    /// Display names of anyone who just retreated (every equipped item
    /// hit 0% durability this fight) - only ever populated by a boss
    /// fight, since only those decay gear at all. See
    /// `Character::retreated_since`/`RETREAT_REPAIR_DURATION`.
    pub retreated: Vec<String>,
    /// Which sprite(s) the overlay should show for the enemy side, in
    /// enemy-index order - one entry (one of `BossKind::sprite`'s 4) per
    /// real boss for a real boss fight (2 entries at `TWO_BOSS_STAGE`+),
    /// empty for a basic encounter (the overlay still picks randomly
    /// from its own `BASIC_ENEMY_SPRITES` for those, unchanged).
    /// Server-authoritative now instead of the overlay guessing
    /// independently, since the sprite has to agree with which
    /// `BossKind` mechanic is actually running this fight.
    pub boss_sprites: Vec<String>,
    /// Full per-hit roll detail for this fight (2026-08-17, full-detail
    /// combat log) - `#[serde(skip)]` deliberately, on BOTH directions:
    /// never appears in the JSON this struct's own `Serialize` impl
    /// produces (the WS payload `encounter_tx` broadcasts to the OBS
    /// overlay, and the coarse-tier fight file via `LastFightSnapshot`'s
    /// `#[serde(flatten)]` of this struct), and never expected on
    /// deserialize either. The detail tier gets this data via a
    /// SEPARATE, explicit field on `DetailFightSnapshot` instead, built
    /// directly from `result.rolls.clone()` - not by (de)serializing
    /// through this struct at all. See `RollEvent`'s own doc for why it
    /// can't just ride along on `events`.
    #[serde(skip)]
    pub rolls: Vec<RollEvent>,
    /// The lightweight per-fight aggregate (2026-08-18, the `/fights.json`
    /// size/latency fix) - unlike `rolls` above, deliberately NOT
    /// `#[serde(skip)]`: this SHOULD ride along on the WebSocket broadcast
    /// this struct's `Serialize` impl produces, so a live consumer (e.g.
    /// the companion app's leaderboard) can be exact without needing the
    /// full (thinned, for the overlay) event log. Built once by
    /// `save_last_fight` from the FULL untouched `events`/`units` and
    /// assigned onto `result.summary` right after that call - the value
    /// in the struct literal at construction time is just a transient
    /// `Default` placeholder, the same idiom `events` itself already
    /// follows (built once, then reassigned before the struct goes
    /// anywhere).
    pub summary: FightSummarySnapshot,
}

/// One resolved fight, persisted to disk purely for after-the-fact
/// inspection - unlike `EncounterResult`'s broadcast above (consumed once,
/// live, by chat/the overlay and never seen again), nothing in this
/// process reads this back; it exists so "what happened in a recent
/// fight, and what were the boss's stats" is still answerable after the
/// moment's passed. `boss_stats` is empty for a basic encounter (its
/// mobs don't roll the same secondary-stat set - see
/// `basic_enemy_stats_for`'s doc), one entry for a normal real boss
/// fight, two for a `TWO_BOSS_STAGE`+ fight - same order as
/// `EncounterResult::boss_sprites`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastFightSnapshot {
    #[serde(flatten)]
    pub result: EncounterResult,
    pub boss_stats: Vec<BossStats>,
    /// Real wall-clock time this fight was saved, milliseconds since the
    /// Unix epoch (2026-08-15, a live request - "logs since the patch"
    /// wasn't actually answerable before this: `events`' own `at_ms` is
    /// relative to the FIGHT's own start, not real time, and nothing else
    /// here recorded when a fight actually happened). `#[serde(default)]`
    /// so old log entries saved before this field existed still parse -
    /// they'll just read as epoch 0 (year 1970), a real "unknown"
    /// signal, not a crash.
    #[serde(default)]
    pub started_at_unix_ms: u64,
}

/// The detail-tier's own per-fight file shape (2026-08-17, full-detail
/// combat log) - the SAME fields `LastFightSnapshot` already carries
/// (flattened in), plus the full `RollEvent` log `EncounterResult.rolls`
/// deliberately keeps out of `LastFightSnapshot`/`EncounterResult`'s own
/// serialization (see `EncounterResult::rolls`'s doc). Built explicitly
/// at the point of saving a fight (`save_last_fight`), not by
/// (de)serializing `rolls` through either of those - this is the one
/// place it round-trips to disk at all.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailFightSnapshot {
    #[serde(flatten)]
    pub snapshot: LastFightSnapshot,
    pub rolls: Vec<RollEvent>,
}

/// Old single-blob fight log path, superseded (2026-08-17) by per-fight
/// files under `COARSE_FIGHTS_DIR`/`DETAIL_FIGHTS_DIR` (see
/// `fight_storage.rs`) - a single ever-growing `Vec<LastFightSnapshot>`
/// fully read+deserialized+rewritten on EVERY fight save, confirmed at
/// 340MB on disk. Kept around only as the one-time migration's read
/// source (`run_storage_migration`) and doc-comment history; nothing
/// writes to this path anymore.
pub(crate) const LAST_FIGHTS_LOG_PATH: &str = "adventure-last-fights.json";

pub(crate) fn save_last_fight(result: &EncounterResult, boss_stats: Vec<BossStats>) -> FightSummarySnapshot {
    let started_at_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let snapshot = LastFightSnapshot { result: result.clone(), boss_stats, started_at_unix_ms };
    save_coarse_fight(&snapshot);
    let rolls = result.rolls.clone();
    save_detail_fight(&DetailFightSnapshot { snapshot, rolls });
    let summary = FightSummarySnapshot {
        kind: result.kind,
        stage: result.stage,
        won: result.won,
        started_at_unix_ms,
        display_duration_ms: result.display_duration_ms,
        participants: result.participants.len(),
        players: full_player_fight_stats(&result.units, &result.events),
        first_to_die: first_player_to_die(&result.units, &result.events),
        loot: result.loot.clone(),
        broken: result.broken.clone(),
    };
    save_summary_fight(&summary);
    summary
}

/// Web dashboard: the rolling coarse-tier fight-history log, newest
/// first - backs the streamer-only `/fights` breakdown page. Reads only
/// the `limit` most recent per-fight files (see `recent_coarse_fights`),
/// never the tier's whole history - the fix for the old single-blob
/// log's whole-file read on every request. The caller (`fights_page`)
/// wraps this in `spawn_blocking` since it's still real, synchronous
/// file I/O.
pub fn recent_fights(limit: usize) -> Vec<LastFightSnapshot> {
    recent_coarse_fights(limit)
}

/// A character as sent to the overlay/other external consumers — same
/// data as `Character` plus the stable lowercased id it's keyed by
/// internally (needed so the overlay can track "the same circle" across
/// updates; `display_name` alone isn't guaranteed stable/unique the way
/// the key is) and `xp_needed` precomputed so the overlay doesn't need to
/// know the level curve formula.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterView {
    pub id: String,
    pub display_name: String,
    pub level: u32,
    pub xp: u64,
    pub xp_needed: u64,
    pub wins: u32,
    pub losses: u32,
    /// The overlay's formation lane (see `CombatFunction`), not the
    /// specific `Archetype` - it only needs to bucket into melee/ranged/
    /// support the same way it always has.
    pub role: CombatFunction,
    /// Unix epoch ms of when this character revives, if they're currently
    /// sitting out a knockout from their last fight - `None` once revived
    /// (never sent stale/past). The overlay ticks its own countdown off
    /// this against its local clock rather than needing repeated pushes.
    pub downed_until_ms: Option<u64>,
    /// True while every piece of equipped gear is worn out (see
    /// `Character::retreated_since`/`all_gear_worn_out`) - they've pulled
    /// back to camp to repair, not just knocked out, so the overlay
    /// should leave them off the battlefield entirely rather than
    /// rendering them as a wandering ghost the way a downed_until does.
    pub retreated: bool,
    /// Which sprite to actually draw - see `Character::effective_sprite`.
    /// Always a concrete, already-resolved name (never the "no pick yet"
    /// `None` state `Character::model` itself can be in), so the overlay
    /// never needs its own fallback logic beyond keeping `ALL_SPRITES`
    /// hand-synced for its OWN unrelated need (the initial pre-connect
    /// paint, before any state message has arrived at all).
    pub model: String,
    /// True while this character has flight toggled on (see
    /// `Character::flying`/`AdventureManager::toggle_flying`) - the
    /// overlay renders them hovering above the crowd instead of
    /// walking/jumping on the ground whenever this is set. Always
    /// `false` for anyone who's never owned or toggled it, so a plain
    /// `!!cv.flying` on the JS side needs no extra `owns_wings` check.
    pub flying: bool,
}

/// Full roster + world state, pushed to the overlay on connect and again
/// on every change (join, level-up, encounter) — the overlay always
/// renders straight off the latest one of these rather than trying to
/// apply incremental diffs itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdventureSnapshot {
    pub stage: u32,
    pub characters: Vec<CharacterView>,
}

/// A Unique Shard win worth announcing in chat (2026-08-17) - fired from
/// every `maybe_drop_unique_shard` call site the instant that rare roll
/// actually succeeds (not just the one-time launch giveaway, which
/// already announces separately in main.rs - this covers every ONGOING
/// win too). Same "one broadcast channel, main.rs's only subscriber
/// turns it into a chat line" shape as `GearCritEvent`/`gear_crit_tx` -
/// not merged with that channel since a shard win isn't a gear crit, it
/// just happens to want the same announcement plumbing.
#[derive(Debug, Clone)]
pub struct UniqueShardEvent {
    pub display_name: String,
}

/// One user's in-progress `/passives` preview across BOTH trees at once
/// (2026-08-17, Split Personality) - `primary` is exactly what the old
/// bare `HashMap<String, u32>` preview used to be; `secondary` is the same
/// shape for `Character::effective_secondary_archetype`'s tree, empty
/// when there isn't one active. Saving/discarding always acts on both
/// sides together (see `save_passive_tree`/`discard_passive_preview`) -
/// there's one shared point pool and one Save/Reset button pair for the
/// whole page, not two independent ones.
#[derive(Debug, Clone, Default)]
pub struct PassivePreview {
    pub primary: HashMap<String, u32>,
    pub secondary: HashMap<String, u32>,
}

pub struct AdventureManager {
    characters: Mutex<HashMap<String, Character>>,
    characters_path: PathBuf,
    world: Mutex<WorldState>,
    world_path: PathBuf,
    /// Lowercased username -> last time they earned activity XP — purely
    /// in-memory rate limiting, doesn't need to survive a restart.
    last_activity_xp: Mutex<HashMap<String, Instant>>,
    /// Lowercased username -> when they revive after being knocked out —
    /// purely in-memory (a restart just revives everyone early, same
    /// tradeoff as last_activity_xp resetting). Entries are pruned lazily
    /// as they're read past, not on a timer.
    downed_until: Mutex<HashMap<String, SystemTime>>,
    /// The earliest instant a new fight is allowed to START (2026-08-17,
    /// a live request: fights were able to overlap - two of the several
    /// independent trigger paths, e.g. a scheduled tick and a channel-
    /// points redemption, could each reach `run_encounter`/
    /// `run_basic_encounter` at effectively the same moment, broadcasting
    /// a second fight's events to the overlay while the first was still
    /// mid-playback). Held as the lock guard itself for a fight's ENTIRE
    /// duration (not just read-and-release) in both of those functions -
    /// that's what actually prevents overlap, not just the timestamp
    /// value. Updated to `now + this fight's display_duration_ms + 5s`
    /// right before each of those functions returns, on every exit path
    /// including "nobody was eligible" - so even a no-op tick still
    /// enforces the flat 5s floor between fights. In-memory only (a
    /// restart just makes the next fight immediately eligible, same
    /// tradeoff as `last_activity_xp`/`downed_until`).
    fight_gate: Mutex<Instant>,
    /// Lowercased username -> the hour-bucket (see `current_hour_bucket`)
    /// they last used "Reforge Gear" in — resets for everyone at the top
    /// of every hour (UTC) together, not on a rolling per-user timer.
    /// Persisted to `reforge_cooldown_path` so a restart doesn't hand
    /// everyone a free extra reforge for whatever's left of the hour.
    reforge_cooldown: Mutex<HashMap<String, u64>>,
    reforge_cooldown_path: PathBuf,
    encounter_tx: broadcast::Sender<EncounterResult>,
    /// Fires on every roster/stage change (join, level-up, encounter) —
    /// see `AdventureSnapshot`. The overlay is the only current
    /// subscriber, but this doesn't depend on it existing.
    state_tx: broadcast::Sender<AdventureSnapshot>,
    /// Fires on a reforge/recombine "crit" (see `GearCritEvent`) -
    /// main.rs's only current subscriber turns this into a chat
    /// announcement.
    gear_crit_tx: broadcast::Sender<GearCritEvent>,
    /// Fires when a FINITE `!rampage`/vote-triggered countdown reaches 0
    /// naturally (2026-08-17, a live request: "there should also be an
    /// announcement when a rampage is complete") - main.rs's only
    /// subscriber turns this into a chat announcement, same pattern as
    /// `gear_crit_tx`. Deliberately does NOT fire when Permanent Rampage
    /// is toggled off by an admin - that's a manual stop, not a
    /// completion (see `spawn_rampage_loop`'s decrement branch, which is
    /// the only place this ever sends).
    rampage_complete_tx: broadcast::Sender<()>,
    /// Fires on a Unique Shard win from the normal ongoing random drop
    /// roll (see `UniqueShardEvent`/`maybe_drop_unique_shard`) - main.rs's
    /// only current subscriber turns this into a chat announcement, same
    /// pattern as `gear_crit_tx`. Separate from the ONE-TIME launch
    /// giveaway (main.rs's own `ITEM_LAUNCH_GIVEAWAYS`), which already
    /// announces on its own - this covers every win after that.
    unique_shard_tx: broadcast::Sender<UniqueShardEvent>,
    /// Lowercased username -> an in-progress veiled craft awaiting a
    /// choice (see `PendingVeil`/`choose_veil_outcome`) - purely
    /// in-memory, same tradeoff as `last_activity_xp`/`downed_until` (a
    /// restart just drops anyone's unresolved veil choice, which cost
    /// them dust already spent - an accepted rare edge case, not worth
    /// persisting a whole extra file for).
    pending_veils: Mutex<HashMap<String, PendingVeil>>,
    /// Lowercased username -> an in-progress passive-tree allocation
    /// preview awaiting Save/Reset - same in-memory, restart-drops-it
    /// tradeoff as `pending_veils` above, except a dropped passive preview
    /// costs nothing (no dust spent until Save). `PassivePreview` (not a
    /// bare map) since Split Personality's 2nd tree needs its own
    /// independent preview alongside the primary one - see its own doc.
    pending_passive_previews: Mutex<HashMap<String, PassivePreview>>,
    /// How many viewer-forced boss triggers (currently just the "Force
    /// Boss Fight" channel points redemption) have been used THIS
    /// `ENCOUNTER_INTERVAL` cycle - see `FORCE_BOSS_MAX_PER_CYCLE`/
    /// `try_force_encounter`. Reset to 0 every time the scheduler's own
    /// timer fires (see `spawn_encounter_loop`), not on a rolling
    /// per-use timer - purely in-memory, same "a restart just clears it"
    /// tradeoff as `reforge_cooldown`'s in-memory half.
    forced_boss_count: Mutex<u32>,
    /// !rampage (mod tool, 2026-08-16) - how many more encounters should
    /// be forced to be BOSS fights, counting down by 1 on every encounter
    /// (regardless of source) while active - see `spawn_rampage_loop`/
    /// `start_rampage`. Mirrored to `RAMPAGE_STATE_PATH` on every change
    /// (2026-08-17, see `persist_rampage_remaining`) and reloaded at
    /// startup - UNLIKE `forced_boss_count`, this one DOES survive a
    /// restart now, per a live request.
    rampage_remaining: Mutex<u32>,
    /// Wakes `spawn_rampage_loop` out of its idle wait the instant
    /// `start_rampage` sets `rampage_remaining` above 0 - without this the
    /// loop would only notice on its own next poll, adding up to a whole
    /// extra `RAMPAGE_INTERVAL` of delay before the first rampage fight.
    rampage_notify: Notify,
    /// !rampage vote (2026-08-17) - distinct non-mod chat logins who've
    /// typed !rampage since the last time the vote either triggered or
    /// (in the future, if ever needed) was otherwise cleared - see
    /// `register_rampage_vote`. A `HashSet` so the same viewer spamming
    /// the command repeatedly only ever counts once. Purely in-memory,
    /// same "a restart just clears it" convention as `rampage_remaining`.
    rampage_votes: Mutex<std::collections::HashSet<String>>,
    /// Live drop-rate/boss-difficulty dials, editable with no recompile AND
    /// no restart via the admin-only `/admin/tunables` web page - see
    /// `LiveTunables`'s own doc for why this is a plain `std::sync::RwLock`
    /// rather than the `OnceLock`-cached pattern `ItemBalanceFile` uses.
    /// `std::sync::RwLock` (not `tokio::sync::RwLock`) deliberately - every
    /// use is a quick clone-and-drop, never held across an `.await`.
    live_tunables: std::sync::RwLock<LiveTunables>,
}

/// Sacred items (2026-08-16, a live request; moved from 200 to 300 on
/// 2026-08-17) start dropping at this stage - deliberately a plain
/// compile-time const, not part of `LiveTunables`, since the request was a
/// fixed stage cutoff rather than a dial to iterate on live; can move into
/// `LiveTunables` later if that changes.
pub(crate) const SACRED_STAGE_THRESHOLD: u32 = 300;

impl AdventureManager {
    pub fn new(characters_path: PathBuf, world_path: PathBuf, reforge_cooldown_path: PathBuf) -> Arc<Self> {
        let mut characters: HashMap<String, Character> = crate::state::load_json(&characters_path).unwrap_or_default();

        // One-time backfill for anyone who joined before Character::new()
        // started handing out a full starter kit - fills only EMPTY
        // slots with a basic tier-1 item, never touching gear they
        // already have. Idempotent: once everyone has all 5 slots
        // filled, this is just a fast no-op scan on every future startup.
        {
            let mut rng = rand::thread_rng();
            let mut changed = false;
            for character in characters.values_mut() {
                for slot in EQUIP_SLOTS {
                    if character.equipped(slot).is_none() {
                        character.equip(generate_item_at_tier(slot, 1, &mut rng));
                        changed = true;
                    }
                }
            }
            if changed {
                if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                    tracing::error!("Failed to persist starter-kit backfill to {}: {err}", characters_path.display());
                }
            }
        }

        // One-time item-value corrections (helm rebalance, power_roll
        // backfill, Krangle/item accuracy passes, crit nerf) - see
        // `ITEM_MIGRATIONS`'s doc for what each does and why order matters.
        run_item_migrations(&characters_path, &mut characters);

        // One-time equipped-item repair (2026-08-18, following the same
        // live report as `migrate_crit_flag_to_affix_tracking` above,
        // which just ran as part of `run_item_migrations`): for every
        // currently-EQUIPPED item where `legacy_reforge_crit_used` was
        // `true` but that migration found nothing to point the new
        // tracking at (no surviving legacy-named affix, not over cap
        // either) - i.e. the item's own state insists it already won a
        // reforge crit, yet nothing on it shows for it - grants a fresh
        // random bonus affix right now and tags it, same as if the crit
        // had just landed for real. Deliberately EQUIPPED-ONLY, not every
        // item a character owns: this is a visible, one-time make-good
        // grant for gear players are actively using, not a silent
        // bag-wide backfill. Guarded by its own marker, same "not
        // naturally idempotent" reasoning as every other one-off grant
        // here.
        {
            const CRIT_REFORGE_EQUIPPED_BACKFILL_MARKER_PATH: &str = "adventure-crit-reforge-equipped-backfill-marker.json";
            if crate::state::load_json::<bool>(CRIT_REFORGE_EQUIPPED_BACKFILL_MARKER_PATH).is_none() {
                let mut rng = rand::thread_rng();
                let mut changed = false;
                for character in characters.values_mut() {
                    for slot in EQUIP_SLOTS {
                        let Some(item) = character.equipped_mut(slot) else { continue };
                        if !item.legacy_reforge_crit_used || item.reforge_crit_used() {
                            continue;
                        }
                        let present: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
                        let candidates: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| !present.contains(a) && a.is_eligible_for_slot(slot)).collect();
                        let Some(&affix) = weighted_affix_pick(&candidates, 1, &mut rng).first() else { continue };
                        let jitter = rng.gen_range(0.85..1.15);
                        let mult = if item.perfect { PERFECT_QUALITY_MULT } else { 1.0 };
                        item.affixes.push((affix, affix_base_value(affix, item.tier) * jitter * mult));
                        item.record_reforge_crit(affix);
                        changed = true;
                    }
                }
                if changed {
                    if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                        tracing::error!("Failed to persist crit-reforge equipped backfill to {}: {err}", characters_path.display());
                    }
                }
                if let Err(err) = crate::state::save_json(CRIT_REFORGE_EQUIPPED_BACKFILL_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist crit-reforge equipped backfill marker to {CRIT_REFORGE_EQUIPPED_BACKFILL_MARKER_PATH}: {err}");
                }
            }
        }

        // One-time character-field corrections (currently: the Flow like
        // Water / Hundred Fists tier swap's allocation move) - see
        // `CHARACTER_MIGRATIONS`'s doc.
        run_character_migrations(&characters_path, &mut characters);

        // One-time distribution: every character who joined before free
        // craft tokens existed gets one of each `CraftAction` right now
        // "so players can learn how to use it" - new characters get the
        // same starter set going forward via `Character::new`. Guarded
        // by its own marker file, same reasoning as the helm rebalance
        // above (granting isn't idempotent - running it twice would
        // grant twice).
        {
            const CRAFT_TOKEN_BACKFILL_MARKER_PATH: &str = "adventure-craft-token-backfill-marker.json";
            if crate::state::load_json::<bool>(CRAFT_TOKEN_BACKFILL_MARKER_PATH).is_none() {
                for character in characters.values_mut() {
                    for &action in &ALL_CRAFT_ACTIONS {
                        character.add_craft_token(action, 1);
                    }
                }
                if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                    tracing::error!("Failed to persist craft token backfill to {}: {err}", characters_path.display());
                }
                if let Err(err) = crate::state::save_json(CRAFT_TOKEN_BACKFILL_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist craft token backfill marker to {CRAFT_TOKEN_BACKFILL_MARKER_PATH}: {err}");
                }
            }
        }

        // Same reasoning as the backfill directly above, but for
        // `CraftAction::Annulment`/`Chancing` specifically (2026-08-17) -
        // the marker above already fired for every pre-existing character
        // before these two actions existed, so it won't hand them out;
        // this is a SEPARATE, independently-gated one-time grant so
        // existing players still get a free token of each to learn them
        // with, same "so they can learn how to use it" reasoning as the
        // original backfill.
        {
            const CRAFT_TOKEN_BACKFILL_V2_MARKER_PATH: &str = "adventure-craft-token-backfill-v2-marker.json";
            if crate::state::load_json::<bool>(CRAFT_TOKEN_BACKFILL_V2_MARKER_PATH).is_none() {
                for character in characters.values_mut() {
                    character.add_craft_token(CraftAction::Annulment, 1);
                    character.add_craft_token(CraftAction::Chancing, 1);
                }
                if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                    tracing::error!("Failed to persist craft token backfill v2 to {}: {err}", characters_path.display());
                }
                if let Err(err) = crate::state::save_json(CRAFT_TOKEN_BACKFILL_V2_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist craft token backfill v2 marker to {CRAFT_TOKEN_BACKFILL_V2_MARKER_PATH}: {err}");
                }
            }
        }

        // One-time launch grant: "immediately set everyone's pity to 100%
        // so the next fight rewards people" - both `item_pity` and
        // `craft_pity` (see those fields/`advance_pity`) start every
        // existing character right at the guaranteed-reward threshold, so
        // whatever fight they're next in pays out both a pity item AND a
        // pity craft token regardless of the normal roll. Guarded by its
        // own marker, same "not naturally idempotent" reasoning as every
        // other one-off grant above - this should fire exactly once, not
        // every restart.
        {
            const PITY_LAUNCH_MARKER_PATH: &str = "adventure-pity-launch-marker.json";
            if crate::state::load_json::<bool>(PITY_LAUNCH_MARKER_PATH).is_none() {
                for character in characters.values_mut() {
                    character.item_pity = PITY_THRESHOLD;
                    character.craft_pity = PITY_THRESHOLD;
                }
                if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                    tracing::error!("Failed to persist pity launch grant to {}: {err}", characters_path.display());
                }
                if let Err(err) = crate::state::save_json(PITY_LAUNCH_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist pity launch marker to {PITY_LAUNCH_MARKER_PATH}: {err}");
                }
            }
        }

        // One-time launch grant: hands the brand-new "Wings of Flight"
        // cosmetic (see `Character::owns_wings`) directly to lokati_gaming
        // to showcase it, bypassing the normal purchase/drop paths -
        // same guarded-by-marker, fire-once shape as the pity grant above.
        {
            const WINGS_LAUNCH_GRANT_MARKER_PATH: &str = "adventure-wings-launch-grant-marker.json";
            if crate::state::load_json::<bool>(WINGS_LAUNCH_GRANT_MARKER_PATH).is_none() {
                if let Some(character) = characters.get_mut("lokati_gaming") {
                    character.owns_wings = true;
                    if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                        tracing::error!("Failed to persist wings launch grant to {}: {err}", characters_path.display());
                    }
                }
                if let Err(err) = crate::state::save_json(WINGS_LAUNCH_GRANT_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist wings launch grant marker to {WINGS_LAUNCH_GRANT_MARKER_PATH}: {err}");
                }
            }
        }

        // One-time key rename in `passive_allocations` (2026-08-17, part of
        // Split Personality) - `passive_tree.rs` node `key` strings were
        // never required to be globally unique across archetypes before
        // now (a character only ever held ONE archetype's tree at a time,
        // so a collision like Warrior's "overwhelm" vs Berserker's own
        // unrelated "overwhelm" never actually met in the same flat map).
        // Split Personality's second tree changes that - `passive_node_rank`/
        // `passive_node_magnitude` now check a SECOND archetype's
        // allocations too, so 8 colliding keys were renamed at their
        // definition site (see passive_tree.rs) to stay unambiguous. Only
        // rename an existing character's entry when THEIR OWN archetype is
        // the one whose copy was renamed - the other archetype's identical
        // key string is a different node entirely and must be left alone.
        // Only 2 of the 8 renamed keys had any live data behind them
        // (checked directly against `adventure-characters.json` before
        // writing this): Warrior's "overwhelm" and Slayer's "frenzy".
        {
            const PASSIVE_KEY_RENAME_MARKER_PATH: &str = "adventure-passive-key-rename-marker.json";
            if crate::state::load_json::<bool>(PASSIVE_KEY_RENAME_MARKER_PATH).is_none() {
                for character in characters.values_mut() {
                    let renames: &[(&str, &str)] = match character.archetype {
                        Archetype::Warrior => &[("overwhelm", "overwhelmingforce")],
                        Archetype::Slayer => &[("frenzy", "vampiricfrenzy")],
                        _ => &[],
                    };
                    for &(old_key, new_key) in renames {
                        if let Some(rank) = character.passive_allocations.remove(old_key) {
                            character.passive_allocations.insert(new_key.to_string(), rank);
                        }
                    }
                }
                if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                    tracing::error!("Failed to persist passive key rename to {}: {err}", characters_path.display());
                }
                if let Err(err) = crate::state::save_json(PASSIVE_KEY_RENAME_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist passive key rename marker to {PASSIVE_KEY_RENAME_MARKER_PATH}: {err}");
                }
            }
        }

        // One-time manual compensation: kibukah was live-reported as
        // having received almost no items, later root-caused to the
        // catch-up-weighted recipient selection bug (see `catchup_multiplier`'s
        // history/the loot rework above that reverted it back to
        // uniform) - low-level alts were soaking up most of the drop
        // rolls in a 36-character roster. A full 5-slot item assortment
        // (one per equip slot, at whatever the world's currently at)
        // plus a dust lump sum and a couple craft tokens, standing in
        // for what pity would have paid out correctly over that
        // stretch. Same guarded-by-marker, fire-once shape as every
        // other one-off grant here.
        {
            const KIBUKAH_COMPENSATION_MARKER_PATH: &str = "adventure-kibukah-compensation-marker.json";
            if crate::state::load_json::<bool>(KIBUKAH_COMPENSATION_MARKER_PATH).is_none() {
                if let Some(character) = characters.get_mut("kibukah") {
                    let stage = crate::state::load_json::<WorldState>(&world_path).unwrap_or_default().stage;
                    let mut rng = rand::thread_rng();
                    for slot in EQUIP_SLOTS {
                        let item = generate_item(slot, stage, &mut rng);
                        character.receive_item(item);
                    }
                    character.dust += 1500;
                    let action = DROPPABLE_CRAFT_ACTIONS[rng.gen_range(0..DROPPABLE_CRAFT_ACTIONS.len())];
                    character.add_craft_token(action, 2);
                    if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                        tracing::error!("Failed to persist kibukah compensation grant to {}: {err}", characters_path.display());
                    }
                }
                if let Err(err) = crate::state::save_json(KIBUKAH_COMPENSATION_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist kibukah compensation marker to {KIBUKAH_COMPENSATION_MARKER_PATH}: {err}");
                }
            }
        }

        // Standing policy, not a one-off: "anytime sprites are added,
        // anyone with an old sprite gets an additional free change" -
        // tracks ALL_SPRITES' own size in a tiny state file across
        // restarts, and whenever it's grown since last recorded, grants
        // everyone who's already picked a model (see
        // `Character::free_model_changes`) one more free change before
        // recording the new size. `last_known_sprite_count > 0` skips
        // the very first run ever (no prior recorded size to compare
        // against - `free_model_changes`'s own default-as-migration
        // already covers everyone's very first grant) so this only ever
        // fires on a REAL increase, never on a normal restart at the
        // same pool size.
        {
            const SPRITE_COUNT_MARKER_PATH: &str = "adventure-sprite-count.json";
            let last_known_sprite_count: usize = crate::state::load_json(SPRITE_COUNT_MARKER_PATH).unwrap_or(0);
            // Compared with `!=`, not just `>` - a bad sprite batch can
            // get pulled and replaced with a differently-sized one (see
            // "sprites 2.png" -> "sprites 3.png"), which can shrink the
            // pool even though sprites were, net, just added. The grant
            // below only ever fires on a genuine increase; the marker
            // itself always gets corrected to the TRUE current count
            // either way, so a shrink doesn't leave a stale high-water
            // mark that would silently swallow the next real increase
            // (comparing against a now-inflated "last known" number).
            if ALL_SPRITES.len() != last_known_sprite_count {
                if ALL_SPRITES.len() > last_known_sprite_count && last_known_sprite_count > 0 {
                    let mut granted = false;
                    for character in characters.values_mut() {
                        if character.model.is_some() {
                            character.free_model_changes += 1;
                            granted = true;
                        }
                    }
                    if granted {
                        if let Err(err) = crate::state::save_json(&characters_path, &characters) {
                            tracing::error!("Failed to persist sprite-growth free-change grant to {}: {err}", characters_path.display());
                        }
                    }
                }
                if let Err(err) = crate::state::save_json(SPRITE_COUNT_MARKER_PATH, &ALL_SPRITES.len()) {
                    tracing::error!("Failed to persist sprite count marker to {SPRITE_COUNT_MARKER_PATH}: {err}");
                }
            }
        }

        // Full-detail combat log storage prerequisite (2026-08-17) - see
        // `run_storage_migration`'s own doc. Independent of `characters`,
        // so it doesn't need to sit inside any of the character-mutating
        // blocks above.
        run_storage_migration();

        let world: WorldState = crate::state::load_json(&world_path).unwrap_or_default();
        let reforge_cooldown: HashMap<String, u64> = crate::state::load_json(&reforge_cooldown_path).unwrap_or_default();
        let (encounter_tx, _rx) = broadcast::channel(16);
        let (state_tx, _rx) = broadcast::channel(16);
        let (gear_crit_tx, _rx) = broadcast::channel(16);
        let (rampage_complete_tx, _rx) = broadcast::channel(16);
        let (unique_shard_tx, _rx) = broadcast::channel(16);
        let rampage_remaining: u32 = crate::state::load_json(RAMPAGE_STATE_PATH).unwrap_or(0);
        Arc::new(Self {
            characters: Mutex::new(characters),
            characters_path,
            world: Mutex::new(world),
            world_path,
            last_activity_xp: Mutex::new(HashMap::new()),
            downed_until: Mutex::new(HashMap::new()),
            fight_gate: Mutex::new(Instant::now()),
            reforge_cooldown: Mutex::new(reforge_cooldown),
            reforge_cooldown_path,
            encounter_tx,
            state_tx,
            gear_crit_tx,
            rampage_complete_tx,
            unique_shard_tx,
            pending_veils: Mutex::new(HashMap::new()),
            pending_passive_previews: Mutex::new(HashMap::new()),
            forced_boss_count: Mutex::new(0),
            rampage_remaining: Mutex::new(rampage_remaining),
            rampage_notify: Notify::new(),
            rampage_votes: Mutex::new(std::collections::HashSet::new()),
            live_tunables: std::sync::RwLock::new(load_live_tunables()),
        })
    }

    /// Cheap clone (12 plain numbers) of the live drop-rate/boss-difficulty
    /// dials - see `LiveTunables`'s doc. Read fresh at the top of every
    /// fight/formula that needs one of these, so a saved admin-page edit
    /// takes effect on the very next encounter with no restart.
    pub fn live_tunables(&self) -> LiveTunables {
        self.live_tunables.read().expect("live_tunables lock poisoned").clone()
    }

    /// Admin-page save: updates the live in-memory copy immediately AND
    /// persists to `adventure-live-tunables.toml` so the change also
    /// survives a restart.
    pub fn save_live_tunables(&self, tunables: LiveTunables) -> std::io::Result<()> {
        save_live_tunables_file(&tunables)?;
        *self.live_tunables.write().expect("live_tunables lock poisoned") = tunables;
        // Permanent Rampage (see `spawn_rampage_loop`'s doc) - if that loop
        // is currently idle-waiting on `rampage_notify` (no `!rampage`
        // countdown in progress and this toggle was previously off), it
        // needs an explicit wake to notice a save that just turned it on.
        // Harmless to fire unconditionally on every save (whether this
        // particular save touched the toggle or not, and whether the loop
        // is currently waiting or not) - `Notify::notify_one` just stores
        // one permit for whenever it next waits if nobody's waiting yet,
        // which the loop's own `permanent`/`rampage_remaining` check
        // immediately falls back through as a harmless extra wake.
        self.rampage_notify.notify_one();
        Ok(())
    }

    /// Current value of the win/loss dynamic-difficulty ratchet (see
    /// `WorldState::boss_power_mult`'s doc) - for the admin tunables page's
    /// read-out, so a crash toward `BOSS_POWER_MULT_MIN` (or a runaway
    /// climb) is actually visible somewhere instead of only inferable from
    /// fight-log HP numbers.
    pub async fn current_boss_power_mult(&self) -> f64 {
        self.world.lock().await.boss_power_mult
    }

    /// Admin-page manual override of the win/loss dynamic-difficulty
    /// ratchet - same "read-out + override" pairing as
    /// `current_boss_power_mult`, for exactly the situation a bad losing
    /// streak leaves it stuck at `BOSS_POWER_MULT_MIN` for many fights in a
    /// row: lets the streamer just set it back to a sane value directly
    /// instead of waiting out a slow win-streak recovery. Floored the same
    /// way the automatic win/loss adjustment already is.
    pub async fn set_boss_power_mult(&self, value: f64) {
        let mut world = self.world.lock().await;
        world.boss_power_mult = value.max(BOSS_POWER_MULT_MIN);
        self.persist_world(&world);
    }

    pub fn subscribe_encounters(&self) -> broadcast::Receiver<EncounterResult> {
        self.encounter_tx.subscribe()
    }

    pub fn subscribe_state(&self) -> broadcast::Receiver<AdventureSnapshot> {
        self.state_tx.subscribe()
    }

    pub fn subscribe_gear_crits(&self) -> broadcast::Receiver<GearCritEvent> {
        self.gear_crit_tx.subscribe()
    }

    pub fn subscribe_rampage_complete(&self) -> broadcast::Receiver<()> {
        self.rampage_complete_tx.subscribe()
    }

    pub fn subscribe_unique_shard_wins(&self) -> broadcast::Receiver<UniqueShardEvent> {
        self.unique_shard_tx.subscribe()
    }

    /// Current roster + world stage — pushed to a freshly (re)connected
    /// overlay immediately, so it doesn't sit blank until the next change.
    pub async fn snapshot(&self) -> AdventureSnapshot {
        let stage = self.world.lock().await.stage;
        let characters = self.characters.lock().await;
        let downed_until = self.downed_until.lock().await;
        let now = SystemTime::now();
        let characters = characters
            .iter()
            .map(|(id, c)| CharacterView {
                id: id.clone(),
                display_name: c.display_name.clone(),
                level: c.level,
                xp: c.xp,
                xp_needed: c.xp_needed(),
                wins: c.wins,
                losses: c.losses,
                role: c.archetype.combat_function(),
                downed_until_ms: downed_until
                    .get(id)
                    .filter(|&&t| t > now)
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64),
                retreated: c.retreated_since.is_some(),
                model: c.effective_sprite(id),
                flying: c.owns_wings && c.flying,
            })
            .collect();
        AdventureSnapshot { stage, characters }
    }

    /// Re-reads current state and broadcasts it — called after anything
    /// that changes the roster or world stage. Takes the trouble of a
    /// fresh `snapshot()` call (briefly re-locking both mutexes) rather
    /// than threading the already-modified data through from each call
    /// site, since none of those sites are hot paths and this keeps them
    /// simple.
    async fn broadcast_state(&self) {
        let _ = self.state_tx.send(self.snapshot().await);
    }

    /// Fires a `GearCritEvent` (see `subscribe_gear_crits`) when `affix`
    /// is `Some` - a no-op otherwise. Shared by reforge's 1% roll and
    /// recombine's 5% roll so both funnel through one announcement path.
    pub(crate) fn announce_gear_crit(&self, display_name: String, source: GearCritSource, item_name: &str, slot: EquipSlot, tier: u32, affix: Option<Affix>) {
        if let Some(affix) = affix {
            let _ = self.gear_crit_tx.send(GearCritEvent { display_name, source, item_name: item_name.to_string(), slot, tier, affix });
        }
    }

    pub(crate) fn persist_characters(&self, characters: &HashMap<String, Character>) {
        if let Err(err) = crate::state::save_json(&self.characters_path, characters) {
            tracing::error!("Failed to persist {}: {err}", self.characters_path.display());
        }
    }

    pub(crate) fn persist_world(&self, world: &WorldState) {
        if let Err(err) = crate::state::save_json(&self.world_path, world) {
            tracing::error!("Failed to persist {}: {err}", self.world_path.display());
        }
    }

    pub(crate) fn persist_reforge_cooldown(&self, reforge_cooldown: &HashMap<String, u64>) {
        if let Err(err) = crate::state::save_json(&self.reforge_cooldown_path, reforge_cooldown) {
            tracing::error!("Failed to persist {}: {err}", self.reforge_cooldown_path.display());
        }
    }

    /// !join — adds `username` to the shared adventure roster.
    pub async fn join(&self, username: &str, display_name: &str) -> JoinOutcome {
        let key = username.to_lowercase();
        let mut characters = self.characters.lock().await;
        if let Some(existing) = characters.get_mut(&key) {
            let level = existing.level;
            if existing.retreated_since.is_some() {
                existing.retreated_since = None;
                let gear_still_worn = existing.all_gear_worn_out();
                self.persist_characters(&characters);
                drop(characters);
                self.broadcast_state().await;
                return JoinOutcome::Rejoined { level, gear_still_worn };
            }
            return JoinOutcome::AlreadyJoined { level };
        }
        characters.insert(key, Character::new(display_name.to_string()));
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        JoinOutcome::Joined
    }

    /// !character — `None` if they haven't !joined yet.
    pub async fn character(&self, username: &str) -> Option<Character> {
        self.characters.lock().await.get(&username.to_lowercase()).cloned()
    }

    /// Web dashboard's character list (see `adventure_web::render_character_list`) -
    /// every character that's ever `!join`ed, keyed by their lowercased
    /// login (same key `character` looks up by, so the list page's links
    /// go straight to `character`/the detail view with no extra lookup).
    /// Unsorted - the caller decides display order.
    pub async fn all_characters(&self) -> Vec<(String, Character)> {
        self.characters.lock().await.iter().map(|(login, c)| (login.clone(), c.clone())).collect()
    }

    /// Web dashboard: equips a specific bag item into its slot, swapping
    /// whatever was there back into the bag - a no-op if they haven't
    /// joined or don't have that item.
    pub async fn equip_item(&self, username: &str, item_id: &str) {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return };
        if character.equip_from_inventory(item_id) {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
    }

    /// Web dashboard: unequips whatever's in `slot` back into the bag - a
    /// no-op if they haven't joined, the slot's empty, or the bag's full.
    pub async fn unequip_item(&self, username: &str, slot: EquipSlot) {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return };
        if character.unequip_to_inventory(slot) {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
    }

    /// Web dashboard: disenchants a bag item into Thaumatergic Dust (see
    /// `Character::disenchant_from_inventory`) - a no-op if they haven't
    /// joined or don't have that item.
    pub async fn disenchant_item(&self, username: &str, item_id: &str) -> Option<DisenchantOutcome> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        // ThreadRng isn't Send, so it can't still be in scope at the
        // broadcast_state().await below - block scope forces it to drop here.
        let sand_mult = self.live_tunables().sand_mult;
        let outcome = {
            let mut rng = rand::thread_rng();
            character.disenchant_from_inventory(item_id, &mut rng, sand_mult)
        };
        if outcome.is_some() {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
        outcome
    }

    /// Web dashboard: disenchants every eligible bag item at once - see
    /// `Character::disenchant_all_from_inventory`. Returns
    /// (items disenchanted, dust granted), both 0 if the character
    /// hasn't joined or nothing was eligible.
    pub async fn disenchant_all(&self, username: &str) -> (usize, u64) {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return (0, 0) };
        let sand_mult = self.live_tunables().sand_mult;
        let (count, dust) = {
            let mut rng = rand::thread_rng();
            character.disenchant_all_from_inventory(&mut rng, sand_mult)
        };
        if count > 0 {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
        (count, dust)
    }

    /// Web dashboard: flips `Item::disenchant_protected` on one bag item
    /// - the tick-box on its own card. Returns the new state, or `None`
    /// if no such item was found (hasn't joined, or the id doesn't
    /// exist/isn't in the bag).
    pub async fn toggle_disenchant_protect(&self, username: &str, item_id: &str) -> Option<bool> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        let item = character.inventory.iter_mut().find(|i| i.id == item_id)?;
        item.disenchant_protected = !item.disenchant_protected;
        let new_state = item.disenchant_protected;
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Some(new_state)
    }

    /// Web dashboard: flips `Character::auto_repair` - see its doc.
    pub async fn toggle_auto_repair(&self, username: &str) -> Option<bool> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        character.auto_repair = !character.auto_repair;
        let new_state = character.auto_repair;
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Some(new_state)
    }

    /// Web dashboard: sets all 3 auto-disenchant fields at once (the
    /// checkbox + dropdown + number input all live in one self-submitting
    /// form - see `render_auto_disenchant_settings`) rather than toggling
    /// one value at a time like `toggle_auto_repair` does. Returns `false`
    /// if the character hasn't joined.
    pub async fn set_auto_disenchant(&self, username: &str, enabled: bool, tier: AutoDisenchantTier, min_percent: u32) -> bool {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return false };
        character.auto_disenchant_enabled = enabled;
        character.auto_disenchant_tier = tier;
        character.auto_disenchant_min_percent = min_percent.clamp(1, 100);
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        true
    }

    /// Web dashboard: repairs whatever's equipped in `slot` (1 dust per
    /// tier) - see `Character::repair_equipped`.
    pub async fn repair_equipped_item(&self, username: &str, slot: EquipSlot) -> Result<u64, RepairError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(RepairError::NoItem)?;
        let result = character.repair_equipped(slot);
        if result.is_ok() {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
        result
    }

    /// Web dashboard: repairs a specific bag item (1 dust per tier) - see
    /// `Character::repair_inventory_item`.
    pub async fn repair_inventory_item(&self, username: &str, item_id: &str) -> Result<u64, RepairError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(RepairError::NoItem)?;
        let result = character.repair_inventory_item(item_id);
        if result.is_ok() {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
        result
    }

    /// Web dashboard: repairs everything that needs it in one action, at
    /// a 10% dust premium - see `Character::repair_all`.
    pub async fn repair_all_gear_for_dust(&self, username: &str) -> Result<u64, RepairError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(RepairError::NoItem)?;
        let result = character.repair_all();
        if result.is_ok() {
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
        }
        result
    }

    /// "Repair All Gear" channel points redemption: fully repairs every
    /// equipped AND bagged item for free (unlike `repair_all_gear_for_dust`,
    /// which charges dust scaled to what's actually missing - this is a
    /// flat channel-points cost instead, set once at reward creation, so
    /// there's no in-game economy to charge against here). Reuses
    /// `Character::repair_all_gear` (the same unconditional full reset the
    /// passive 1-hour auto-repair-on-retreat uses) followed by
    /// `sync_retreat_status`, so a character whose worn-out gear had them
    /// retreated is immediately battle-eligible again - no separate
    /// `!join` needed, unlike the passive auto-repair path, since
    /// redeeming this is itself the explicit "I'm back" action. `None` if
    /// they haven't joined, or there was genuinely nothing to do (no item
    /// below full durability AND not retreated) - so a redemption that
    /// would do nothing gets refunded instead of silently succeeding.
    pub async fn repair_all_gear_free(&self, username: &str) -> Option<()> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        if character.repair_all_cost() == 0 && character.retreated_since.is_none() {
            return None;
        }
        character.repair_all_gear();
        character.sync_retreat_status();
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Some(())
    }

    /// !party/!adventure — (current stage, active fighters, total roster).
    /// "Active" matches exactly who the next encounter would actually
    /// fight with (see `eligible_fighters`) - anyone currently retreated
    /// (all gear worn out) or still on their post-knockout revive
    /// countdown doesn't count, even though they're still on the roster.
    pub async fn party_status(&self) -> (u32, usize, usize) {
        let stage = self.world.lock().await.stage;
        let active = self.eligible_fighters().await.len();
        let total = self.characters.lock().await.len();
        (stage, active, total)
    }

    /// Mod tool: grants every joined character one random-slot item right
    /// now, scaled to the current stage - lands in their bag like any
    /// other drop (see `Character::add_to_inventory`), not auto-equipped;
    /// they equip it themselves from the web dashboard. Returns how many
    /// characters actually got gear (0 if nobody's joined; a character
    /// whose bag is already full at the 50-item cap just doesn't get one).
    pub async fn grant_random_gear_to_all(&self) -> usize {
        let stage = self.world.lock().await.stage;
        let sand_mult = self.live_tunables().sand_mult;
        let mut characters = self.characters.lock().await;
        let mut granted = 0usize;
        // ThreadRng isn't Send, so it can't still be in scope at a later
        // .await (this fn gets spawned on tokio's multi-threaded runtime) -
        // a block scope forces it to actually drop here, since relying on
        // last-use inference wasn't enough for async fn's generator lowering.
        {
            let mut rng = rand::thread_rng();
            for character in characters.values_mut() {
                let slot = EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())];
                let item = generate_item(slot, stage, &mut rng);
                if !matches!(character.receive_item_with_auto_disenchant(item, &mut rng, sand_mult), ReceiveOutcome::BagFull) {
                    granted += 1;
                }
            }
        }
        if granted > 0 {
            self.persist_characters(&characters);
        }
        drop(characters);
        if granted > 0 {
            self.broadcast_state().await;
        }
        granted
    }

    /// Mod tool: grants `amount` dust to EVERY currently-joined character
    /// at once - see !giftdust. Returns how many characters actually
    /// received it (0 if nobody's joined).
    pub async fn grant_dust_to_all(&self, amount: u64) -> usize {
        let mut characters = self.characters.lock().await;
        for character in characters.values_mut() {
            character.dust += amount;
        }
        let count = characters.len();
        if count > 0 {
            self.persist_characters(&characters);
        }
        drop(characters);
        if count > 0 {
            self.broadcast_state().await;
        }
        count
    }

    /// Mod tool: grants `amount` dust to one specific character - see
    /// !giftdust. `false` if `username` hasn't joined.
    pub async fn grant_dust(&self, username: &str, amount: u64) -> bool {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return false };
        character.dust += amount;
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        true
    }

    /// Grants `amount` more of `action`'s craft token to one specific
    /// character - same shape as `grant_dust`, used by the one-time
    /// Celestial Shard top-healer award (see main.rs). `false` if
    /// `username` hasn't joined.
    pub async fn grant_craft_token(&self, username: &str, action: CraftAction, amount: u32) -> bool {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return false };
        character.add_craft_token(action, amount);
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        true
    }

    /// Atomically checks AND claims `username`'s "Reforge Gear" cooldown
    /// slot for the CURRENT hour in one lock acquisition —
    /// `Err(remaining_secs)` (time left until the next top of the hour)
    /// if they've already used it this hour, otherwise the slot is
    /// claimed immediately and this returns `Ok(())`. Doing the check and
    /// the claim as two separate steps (peek, then insert later) left a
    /// real gap two redemptions arriving milliseconds apart could both
    /// slip through before either claimed the slot, actually allowing
    /// more than one reforge inside the hour - this closes that. If the
    /// redemption ends up being refunded anyway (nothing equipped), call
    /// `release_reforge_cooldown` to give the slot back so it never costs
    /// them the rest of the hour for nothing.
    pub async fn try_claim_reforge_cooldown(&self, username: &str) -> Result<(), u64> {
        let mut used = self.reforge_cooldown.lock().await;
        let key = username.to_lowercase();
        let current_bucket = Self::current_hour_bucket();
        if used.get(&key) == Some(&current_bucket) {
            return Err(Self::seconds_until_next_hour());
        }
        used.insert(key, current_bucket);
        self.persist_reforge_cooldown(&used);
        Ok(())
    }

    /// Gives back a cooldown slot claimed by `try_claim_reforge_cooldown`
    /// for a redemption that turned out not to go through after all.
    pub async fn release_reforge_cooldown(&self, username: &str) {
        let mut used = self.reforge_cooldown.lock().await;
        used.remove(&username.to_lowercase());
        self.persist_reforge_cooldown(&used);
    }

    /// Web dashboard: whether `username` has already used "Reforge Gear"
    /// this hour, and the epoch-ms timestamp of the next global reset (the
    /// top of the next hour) - read-only, doesn't claim or consume anything.
    pub async fn reforge_status(&self, username: &str) -> (bool, u64) {
        let used = self.reforge_cooldown.lock().await;
        let current_bucket = Self::current_hour_bucket();
        let used_this_hour = used.get(&username.to_lowercase()) == Some(&current_bucket);
        let next_reset_ms = (current_bucket + 1) * 3600 * 1000;
        (used_this_hour, next_reset_ms)
    }

    /// Hours since the Unix epoch (UTC) - the shared clock "Reforge Gear"
    /// resets against, same for every viewer regardless of timezone.
    pub(crate) fn current_hour_bucket() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 3600
    }

    pub(crate) fn seconds_until_next_hour() -> u64 {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        3600 - (secs % 3600)
    }

    /// "Reforge Gear" channel points redemption: picks ONE random
    /// currently-equipped item (any slot, indestructible or not) and
    /// replaces it with a fresh one 2-4 tiers above what it was — always
    /// applied unconditionally (a deliberate paid action, not a drop roll
    /// that might not be an upgrade). Reforging an indestructible item
    /// keeps the result indestructible too — it's the same rare piece
    /// made stronger, not a fresh roll that happens to lose that
    /// property. `None` if they haven't joined or have nothing equipped.
    pub async fn reforge_random_gear(&self, username: &str) -> Option<ReforgeOutcome> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        let outcome = Self::reforge_equipped_item(character)?;
        let display_name = character.display_name.clone();
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        self.announce_gear_crit(display_name, GearCritSource::Reforge, &outcome.item_name, outcome.slot, outcome.new_tier, outcome.bonus_affix);
        Some(outcome)
    }

    /// Web dashboard's paid alternative to the free "Reforge Now" button -
    /// same once-per-hour allowance (claimed by the caller beforehand, see
    /// `try_claim_reforge_cooldown`), but since it can't actually spend a
    /// viewer's Twitch channel points (no API for that outside an actual
    /// reward redemption), it charges dust instead so it's not a pure
    /// freebie. Dust is only deducted once both an eligible slot AND
    /// sufficient dust are confirmed - never charges for a failed attempt.
    pub async fn reforge_random_gear_for_dust(&self, username: &str) -> Option<ReforgeOutcome> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase())?;
        if character.dust < WEB_REFORGE_DUST_COST {
            return None;
        }
        let outcome = Self::reforge_equipped_item(character)?;
        character.dust -= WEB_REFORGE_DUST_COST;
        let display_name = character.display_name.clone();
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        self.announce_gear_crit(display_name, GearCritSource::Reforge, &outcome.item_name, outcome.slot, outcome.new_tier, outcome.bonus_affix);
        Some(outcome)
    }

    /// Web dashboard: changes a character's archetype - free while
    /// `free_archetype_changes` is banked (consumed instead of dust;
    /// starts at 2 for everyone - see its doc, "so they can play around
    /// with different archetypes"), otherwise `ARCHETYPE_CHANGE_COST`
    /// dust. Picking `Commoner` itself is always rejected - it's a
    /// starting state, not a valid manual destination.
    pub async fn change_archetype(&self, username: &str, archetype: Archetype) -> Result<(), ChangeArchetypeError> {
        if archetype == Archetype::Commoner {
            return Err(ChangeArchetypeError::InvalidChoice);
        }
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(ChangeArchetypeError::NotJoined)?;
        let free = character.free_archetype_changes > 0;
        if free {
            character.free_archetype_changes -= 1;
        } else {
            if character.dust < ARCHETYPE_CHANGE_COST {
                return Err(ChangeArchetypeError::InsufficientDust(ARCHETYPE_CHANGE_COST));
            }
            character.dust -= ARCHETYPE_CHANGE_COST;
        }
        character.archetype = archetype;
        // Passive tree allocations are per-current-archetype only (see
        // `Character::passive_allocations`'s doc) - a real class change
        // clears them rather than carrying a stale tree from the old
        // archetype (whose node keys mean nothing for the new one).
        character.passive_allocations.clear();
        self.persist_characters(&characters);
        drop(characters);
        // Also drop any in-progress PREVIEW from the old archetype - same
        // reason `respec_passive_tree` clears it, and a real gap this one
        // used to miss: without this, a player who previewed allocations
        // (without saving) and then switched class kept seeing/spending
        // against the OLD archetype's leftover preview map on `/passives`
        // (some node keys are reused across different archetypes' trees,
        // e.g. "frenzy" on both Berserker and Slayer, so a stale entry
        // can silently look like a real allocation in the new tree, and
        // every other leftover entry still counts against the point
        // budget even though it renders nowhere).
        self.pending_passive_previews.lock().await.remove(&username.to_lowercase());
        self.broadcast_state().await;
        Ok(())
    }

    /// Split Personality's 2nd-class picker (2026-08-17) - unlike
    /// `change_archetype` above, this is always free (per a live
    /// decision: no dust cost, ever, no `free_*` counter to bank/spend)
    /// and only ever available while Split Personality is equipped
    /// somewhere (see `Character::effective_split_personality_item`).
    /// Re-submitting the SAME archetype already active is a deliberate
    /// no-op (doesn't touch `secondary_passive_allocations`) - only an
    /// actual CHANGE clears the secondary tree's points, same "changing
    /// it always refunds everything spent in it" rule unequipping the
    /// item follows too (see `Character::effective_secondary_archetype`).
    pub async fn set_secondary_archetype(&self, username: &str, archetype: Archetype) -> Result<(), SetSecondaryArchetypeError> {
        if archetype == Archetype::Commoner {
            return Err(SetSecondaryArchetypeError::InvalidChoice);
        }
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(SetSecondaryArchetypeError::NotJoined)?;
        if character.effective_split_personality_item().is_none() {
            return Err(SetSecondaryArchetypeError::NotEquipped);
        }
        if archetype == character.archetype {
            return Err(SetSecondaryArchetypeError::SameAsPrimary);
        }
        if character.effective_secondary_archetype() != Some(archetype) {
            character.secondary_passive_allocations.clear();
            character.secondary_archetype = Some(archetype);
        }
        self.persist_characters(&characters);
        drop(characters);
        // Same reasoning as `change_archetype`'s own preview-clear above -
        // a stale preview keyed against the OLD secondary tree shouldn't
        // silently keep counting against the shared point budget once the
        // secondary tree itself has just changed under it.
        self.pending_passive_previews.lock().await.remove(&username.to_lowercase());
        self.broadcast_state().await;
        Ok(())
    }

    /// Web dashboard: adjusts one passive-tree node's rank in `username`'s
    /// PREVIEW only - nothing is spent or saved to the real character
    /// until `save_passive_tree`, same "compare freely" idea
    /// `PendingVeil` established for crafting. `delta` is `+1` or `-1`
    /// (the dashboard's dot-row UI - see `render_passive_tree_page`).
    /// `secondary` picks which of the two trees this click targets (see
    /// `PassivePreview`) - `false` validates against the character's
    /// PRIMARY archetype's tree as before; `true` validates against
    /// `effective_secondary_archetype`'s tree instead, and fails with
    /// `NodeNotFound` if there isn't one active (matches "the node doesn't
    /// exist" - from this character's point of view right now, it
    /// doesn't). Node existence, prerequisite depth (checked against the
    /// PREVIEW state, so a same-request allocate-parent-then-child
    /// works), `[0, max_rank]`, and a Modifier's `unlock_at` gate are all
    /// otherwise unchanged from before. The BOTH-trees-combined spend can
    /// never exceed `Character::total_passive_points()` - one shared pool,
    /// not two independent budgets. Returns the resulting preview (both
    /// sides) on success.
    pub async fn preview_allocate_passive(&self, username: &str, node_key: &str, delta: i32, secondary: bool) -> Result<PassivePreview, PassiveError> {
        let key = username.to_lowercase();
        let characters = self.characters.lock().await;
        let character = characters.get(&key).ok_or(PassiveError::NotJoined)?;
        let target_archetype = if secondary { character.effective_secondary_archetype().ok_or(PassiveError::NodeNotFound)? } else { character.archetype };
        let nodes = target_archetype.passive_nodes();
        let node = nodes.iter().find(|n| n.key == node_key).ok_or(PassiveError::NodeNotFound)?;

        let mut previews = self.pending_passive_previews.lock().await;
        let preview = previews.entry(key.clone()).or_insert_with(|| PassivePreview {
            primary: character.passive_allocations.clone(),
            secondary: character.secondary_passive_allocations.clone(),
        });
        // Read the OTHER side's spend before taking a mutable borrow of
        // this side below - `side` stays borrowed mutably for the rest of
        // this function, so `preview.primary`/`preview.secondary` can't
        // be read again once it exists.
        let other_side_spent: u32 = if secondary { preview.primary.values().sum() } else { preview.secondary.values().sum() };
        let side = if secondary { &mut preview.secondary } else { &mut preview.primary };

        let current_rank = side.get(node_key).copied().unwrap_or(0);
        let new_rank = if delta >= 0 { current_rank.saturating_add(delta as u32) } else { current_rank.saturating_sub((-delta) as u32) };
        if new_rank > node.max_rank {
            return Err(PassiveError::MaxRankReached);
        }
        if new_rank > 0 {
            if let Some(parent_key) = node.parent {
                let parent_rank = side.get(parent_key).copied().unwrap_or(0);
                let required = node.unlock_at.unwrap_or(1);
                if parent_rank < required {
                    return Err(PassiveError::ParentNotInvested);
                }
            }
        }
        let mut trial_side = side.clone();
        if new_rank == 0 {
            trial_side.remove(node_key);
        } else {
            trial_side.insert(node_key.to_string(), new_rank);
        }
        // Budget against the TOTAL points this level (plus Split
        // Personality's own bonus, if active) has earned, not "remaining
        // relative to the last SAVED tree" - `trial_side` combined with
        // the OTHER (untouched) side is the preview's WHOLE spend across
        // both trees, not just this click's delta, so comparing it
        // against a "remaining since last save" number double-counts
        // everything already sitting in the preview. This used to reject
        // nearly every click for anyone who'd already spent more than
        // about half their level's points (i.e. anyone who actually
        // plays) - found from a live report of "I have points available
        // but nothing happens when I click."
        let total_spent: u32 = trial_side.values().sum::<u32>() + other_side_spent;
        if total_spent > character.total_passive_points() {
            return Err(PassiveError::InsufficientPoints);
        }
        *side = trial_side;
        Ok(preview.clone())
    }

    /// Web dashboard: read-only lookup of `username`'s in-progress
    /// passive-tree preview, if any - `None` means "no unsaved changes",
    /// in which case the dashboard just shows `Character::passive_allocations`/
    /// `secondary_passive_allocations` directly.
    pub async fn pending_passive_preview(&self, username: &str) -> Option<PassivePreview> {
        self.pending_passive_previews.lock().await.get(&username.to_lowercase()).cloned()
    }

    /// Web dashboard: commits `username`'s preview into their real
    /// `Character::passive_allocations`/`secondary_passive_allocations`
    /// (both sides together - one shared pool, one Save button), persists,
    /// and clears the preview entry. A no-op success (nothing to save, but
    /// not an error either) if there's no pending preview - the Save
    /// button is only ever shown enabled when one exists, so this is a
    /// defensive fallback, not a reachable UI path.
    pub async fn save_passive_tree(&self, username: &str) -> Result<(), PassiveError> {
        let key = username.to_lowercase();
        let mut previews = self.pending_passive_previews.lock().await;
        let Some(preview) = previews.remove(&key) else { return Ok(()) };
        drop(previews);
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&key).ok_or(PassiveError::NotJoined)?;
        character.passive_allocations = preview.primary;
        // Only commit the secondary side if a secondary tree is actually
        // still active - a preview built while it was equipped, saved
        // after it was unequipped mid-session, shouldn't resurrect a
        // secondary allocation that should have been refunded.
        if character.effective_secondary_archetype().is_some() {
            character.secondary_passive_allocations = preview.secondary;
        }
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Ok(())
    }

    /// Web dashboard: discards `username`'s in-progress preview for free,
    /// reverting the dashboard back to their last-saved tree - the
    /// explicit "Reset Preview" button next to Save Changes.
    pub async fn discard_passive_preview(&self, username: &str) {
        self.pending_passive_previews.lock().await.remove(&username.to_lowercase());
    }

    /// Web dashboard: fully resets `username`'s passive tree - free while
    /// `free_passive_respecs` is banked (starts at 1 for everyone, see
    /// its doc), otherwise `PASSIVE_RESPEC_COST` dust. Same skeleton as
    /// `change_archetype` above. Also clears any in-progress preview,
    /// since respeccing the SAVED tree while a preview is pending would
    /// otherwise leave a stale preview the next Save could resurrect.
    pub async fn respec_passive_tree(&self, username: &str) -> Result<(), PassiveError> {
        let key = username.to_lowercase();
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&key).ok_or(PassiveError::NotJoined)?;
        let free = character.free_passive_respecs > 0;
        if free {
            character.free_passive_respecs -= 1;
        } else {
            if character.dust < PASSIVE_RESPEC_COST {
                return Err(PassiveError::InsufficientDust(PASSIVE_RESPEC_COST));
            }
            character.dust -= PASSIVE_RESPEC_COST;
        }
        character.passive_allocations.clear();
        self.persist_characters(&characters);
        drop(characters);
        self.pending_passive_previews.lock().await.remove(&key);
        self.broadcast_state().await;
        Ok(())
    }

    /// Web dashboard: changes a character's model/sprite - free while
    /// `free_model_changes` is banked (consumed instead of dust; starts
    /// at 1 for everyone - see its doc - and gets topped up whenever
    /// `ALL_SPRITES` grows, see `AdventureManager::new`), otherwise
    /// `MODEL_CHANGE_COST` dust. Takes effect live: `broadcast_state` at
    /// the end pushes the updated `CharacterView.model` straight to the
    /// OBS overlay's open WebSocket (see `adventure_overlay_server.rs`),
    /// same as any other roster change. `model` is valid either as one of
    /// the curated `ALL_SPRITES`, or as a self-service custom drop-in
    /// (see `CUSTOM_SPRITE_DIR`/`is_valid_custom_sprite`).
    pub async fn change_model(&self, username: &str, model: String) -> Result<(), ChangeModelError> {
        let id = username.to_lowercase();
        if !ALL_SPRITES.contains(&model.as_str()) && !is_valid_custom_sprite(&id, &model) {
            return Err(ChangeModelError::InvalidChoice);
        }
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&id).ok_or(ChangeModelError::NotJoined)?;
        if MODEL_CHANGES_FREE_FOR_ALL {
            // Neither dust nor a banked token spent - see the flag's doc.
        } else if character.free_model_changes > 0 {
            character.free_model_changes -= 1;
        } else {
            if character.dust < MODEL_CHANGE_COST {
                return Err(ChangeModelError::InsufficientDust(MODEL_CHANGE_COST));
            }
            character.dust -= MODEL_CHANGE_COST;
        }
        character.model = Some(model);
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Ok(())
    }

    /// Web dashboard: buys the "Wings of Flight" cosmetic MTX outright
    /// for `WINGS_COST` dust - see `Character::owns_wings`. Purely
    /// cosmetic (no combat effect), and one-time (can't buy a second
    /// copy).
    pub async fn purchase_wings(&self, username: &str) -> Result<(), WingsError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(WingsError::NotJoined)?;
        if character.owns_wings {
            return Err(WingsError::AlreadyOwned);
        }
        if character.dust < WINGS_COST {
            return Err(WingsError::InsufficientDust);
        }
        character.dust -= WINGS_COST;
        character.owns_wings = true;
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Ok(())
    }

    /// Web dashboard: flips `Character::flying` on/off - only reachable
    /// once `owns_wings` is true (see `WingsError::NotOwned`). Takes
    /// effect live via `broadcast_state`, same as a model change - the
    /// overlay picks up the new hover-vs-walk rendering on its very next
    /// state push. Returns the new state so the caller doesn't need a
    /// second round-trip to know what it just toggled to.
    pub async fn toggle_flying(&self, username: &str) -> Result<bool, WingsError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(WingsError::NotJoined)?;
        if !character.owns_wings {
            return Err(WingsError::NotOwned);
        }
        character.flying = !character.flying;
        let new_state = character.flying;
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Ok(new_state)
    }

    /// One-off giveaway hook (not exposed to players themselves) - picks
    /// one random currently-joined character who doesn't already own the
    /// Wings of Flight cosmetic and grants it to them for free, same
    /// field `purchase_wings` sets. Returns their display name so the
    /// caller (see main.rs's startup hook) can announce it in chat.
    /// `None` if literally every joined character already owns it
    /// (shouldn't happen on a live roster this size).
    pub async fn grant_random_wings(&self) -> Option<String> {
        let mut characters = self.characters.lock().await;
        let candidates: Vec<String> = characters.iter().filter(|(_, c)| !c.owns_wings).map(|(id, _)| id.clone()).collect();
        if candidates.is_empty() {
            return None;
        }
        let pick = &candidates[rand::thread_rng().gen_range(0..candidates.len())];
        let character = characters.get_mut(pick)?;
        character.owns_wings = true;
        let display_name = character.display_name.clone();
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Some(display_name)
    }

    /// Web dashboard: names (or, if `nickname` is empty, permanently
    /// declines to name) a Krangled item - see `Item::nickname`/
    /// `render_nickname_prompt`. Free (no dust/token cost - this is
    /// flavor, not power). Silently no-ops rather than erroring if the
    /// item doesn't exist or isn't locked - not a reachable UI state
    /// under normal use (the prompt only ever shows a locked item's own
    /// id), so nothing meaningful to report back on failure.
    pub async fn name_item(&self, username: &str, item_id: &str, nickname: &str) {
        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&username.to_lowercase()) else { return };
        let Some(item) = character.find_item_by_id_mut(item_id) else { return };
        if !item.locked {
            return;
        }
        let trimmed = nickname.trim();
        let capped: String = trimmed.chars().take(NICKNAME_MAX_LEN).collect();
        item.nickname = Some(capped);
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
    }

    /// Web dashboard: recombines two of a character's same-slot items
    /// (see `Character::recombine`) for `RECOMBINE_DUST_COST` dust. A
    /// veiled recombine guarantees every affix from both sources carries
    /// over (see `Character::roll_recombine`'s `guaranteed` flag) instead
    /// of the normal 50%-per-affix coin flip, so it costs more the bigger
    /// that guaranteed pool is: `VEIL_EXTRA_COST` flat, plus 500 per
    /// combined affix across both source items. Checked/deducted here
    /// (not in `Character::recombine` itself, same split as every other
    /// paid action in this file) so a failed attempt (bad ids, mismatched
    /// slots, locked item) never costs anything. Veiled rolls THREE
    /// independent candidates - purely read-only against the real
    /// character, so no cloning needed - and stores them as a pending
    /// choice (see `choose_veil_outcome`) instead of consuming the source
    /// items yet. With `guaranteed` retention all 3 will usually be
    /// identical bar the independent 5% recomb-crit roll on each - a
    /// veiled recombine is really "pay for certainty", not "pick your
    /// favorite roll" the way a veiled currency craft is.
    pub async fn recombine_gear(&self, username: &str, item_id_a: &str, item_id_b: &str, veiled: bool) -> Result<RecombineResult, RecombineError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(RecombineError::NotJoined)?;
        let has_free = character.free_recombines > 0;
        // A free recombine always veils too, same reasoning as a free
        // craft token (see `craft_item`'s doc) - shows the full range of
        // outcomes instead of one auto-applied result. Still entirely
        // free either way - the pool-size surcharge below doesn't apply.
        let veiled = has_free || veiled;
        let pool_affix_count = character
            .find_item_by_id(item_id_a)
            .zip(character.find_item_by_id(item_id_b))
            .map(|(a, b)| (a.affixes.len() + b.affixes.len()) as u64)
            .unwrap_or(0);
        // A basic (non-veiled) recombine is free for everyone now, token
        // or not - only veiling it (paying for the guaranteed-transfer
        // certainty above) still costs anything. RECOMBINE_DUST_COST does
        // NOT factor in here anymore - that was the old paid baseline
        // from before basic recombine went free, and leaving it in the
        // veiled formula was a real live bug (a 3-modifier veiled
        // recombine charged 2500 instead of the intended 2000 -
        // VEIL_EXTRA_COST + 500/modifier, nothing else).
        let cost = if has_free || !veiled { 0 } else { VEIL_EXTRA_COST + 500 * pool_affix_count };
        if character.dust < cost {
            return Err(RecombineError::InsufficientDust(cost));
        }

        if veiled {
            // ThreadRng isn't Send, so it can't still be in scope at the
            // .await calls below (this fn gets spawned on tokio's
            // multi-threaded runtime) - a block scope forces it to
            // actually drop here.
            let candidates = {
                let mut rng = rand::thread_rng();
                let mut candidates = Vec::with_capacity(3);
                for _ in 0..3 {
                    candidates.push(VeilCandidate::Recombine(character.roll_recombine(item_id_a, item_id_b, true, &mut rng)?));
                }
                candidates
            };
            if has_free {
                character.free_recombines -= 1;
            } else {
                character.dust -= cost;
            }
            self.pending_veils.lock().await.insert(
                username.to_lowercase(),
                PendingVeil {
                    action: PendingVeilAction::Recombine { item_id_a: item_id_a.to_string(), item_id_b: item_id_b.to_string() },
                    candidates,
                    chancing_remaining: Vec::new(),
                    chancing_committed: Vec::new(),
                },
            );
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(RecombineResult::PendingChoice);
        }

        let outcome = {
            let mut rng = rand::thread_rng();
            character.recombine(item_id_a, item_id_b, &mut rng)?
        };
        if has_free {
            character.free_recombines -= 1;
        } else {
            character.dust -= cost;
        }
        character.last_crafted_item_id = Some(outcome.item_id.clone());
        let display_name = character.display_name.clone();
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        self.announce_gear_crit(display_name, GearCritSource::Recombine, &outcome.item_name, outcome.slot, outcome.new_tier, outcome.bonus_affix);
        Ok(RecombineResult::Applied(outcome))
    }

    /// Web dashboard: applies one of the six currency crafting actions
    /// (see `CraftAction`) to a specific item - `action.base_cost()`
    /// dust, +`VEIL_EXTRA_COST` if `veiled`.
    /// Checked/deducted here (not in `Character::craft`), only once
    /// every item-side precondition is confirmed, so a failed attempt
    /// never costs anything - same split as every other paid action in
    /// this file. A veiled, non-Scour craft (see
    /// `CraftAction::is_veilable`) doesn't apply anything yet - it rolls
    /// (see `Character::roll_craft_affix_value`) up to 3 DISTINCT
    /// candidate affixes purely read-only and stores them as a pending
    /// choice (see `choose_veil_outcome`) instead of mutating the real
    /// item.
    pub async fn craft_item(&self, username: &str, item_id: &str, action: CraftAction, veiled: bool) -> Result<CraftResult, CraftError> {
        self.craft_item_ex(username, item_id, action, veiled, true).await
    }

    /// Rolls 3 REPLACEMENT candidates for one veiled-Chancing step -
    /// `target` is the affix type currently occupying the slot being
    /// rerolled this step. Same weighted-pick-from-the-eligible-pool logic
    /// every other veiled currency craft's candidate-building already uses
    /// (`craftable_affix_pool` + `weighted_affix_pick`, see the generic
    /// veiled branch in `craft_item_ex`) - the only difference is each
    /// candidate's `CraftOutcome` carries BOTH `affix_removed`/
    /// `affix_removed_value` (this slot's current type/value) AND
    /// `affix_added`/`affix_value` (the candidate's new type/value), since
    /// this is a replacement, not a plain addition. `craftable_affix_pool`
    /// excluding every type already on the item (including `target`
    /// itself) means the new roll can never coincidentally match the slot
    /// it's replacing - an acceptable, even desirable, guarantee of an
    /// actual change.
    fn chancing_step_candidates(
        character: &Character,
        item_id: &str,
        target: Affix,
        item_name: &str,
        slot: EquipSlot,
        tier: u32,
        perfect: bool,
        rng: &mut impl Rng,
    ) -> Result<Vec<VeilCandidate>, CraftError> {
        let target_value = character
            .find_item_by_id(item_id)
            .and_then(|item| item.affixes.iter().find(|(a, _)| *a == target))
            .map(|&(_, v)| v)
            .ok_or(CraftError::ItemNotFound)?;
        let pool = character.craftable_affix_pool(item_id, CraftAction::Chancing)?;
        let picks = weighted_affix_pick(&pool, 3, rng);
        let mut candidates = Vec::with_capacity(picks.len());
        for new_affix in picks {
            let new_value = character.roll_craft_affix_value(item_id, new_affix, rng).ok_or(CraftError::ItemNotFound)?;
            candidates.push(VeilCandidate::Currency(CraftOutcome {
                item_name: item_name.to_string(),
                slot,
                tier,
                action: CraftAction::Chancing,
                affix_added: Some(new_affix),
                affix_value: Some(new_value),
                affix_removed: Some(target),
                affix_removed_value: Some(target_value),
                affixes_removed: 0,
                now_locked: false,
                unique_affix_added: None,
                polished_affixes: Vec::new(),
                chancing_previous: Vec::new(),
                new_quality_percent: None,
                perfect,
            }));
        }
        Ok(candidates)
    }

    /// Same as `craft_item`, except `allow_token_use` controls whether a
    /// banked free-use token is even consulted. Hideout Warrior (see
    /// `do_hideout_warrior` in adventure_web.rs) passes `false` - it runs
    /// 5 fixed steps in one click with no per-step choice UI of its own,
    /// and always pays the real dust cost of every step by design (a live
    /// request - it should never silently spend a banked token). `false`
    /// also sidesteps the "a token always forces `veiled = true`" rule
    /// below, since a token is never consulted at all in that case.
    pub(crate) async fn craft_item_ex(&self, username: &str, item_id: &str, action: CraftAction, veiled: bool, allow_token_use: bool) -> Result<CraftResult, CraftError> {
        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&username.to_lowercase()).ok_or(CraftError::NotJoined)?;
        // Polishing and Reforge each have their own, entirely different
        // cost currency/formula (sand-by-quality, dust-by-tier) - see
        // both actions' own docs - so they bypass the generic
        // token/veil/dust machinery below completely, same as
        // CelestialShard bypasses it for its own reasons.
        if action == CraftAction::Polishing {
            let item = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            let cost = if item.perfect { 12 } else { (item.quality_percent() / 10.0).ceil() as u64 };
            if character.sand < cost {
                return Err(CraftError::InsufficientSand(cost));
            }
            let outcome = {
                let mut rng = rand::thread_rng();
                character.polish(item_id, &mut rng)?
            };
            character.sand -= cost;
            character.last_crafted_item_id = Some(item_id.to_string());
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(CraftResult::Applied(outcome));
        }
        if action == CraftAction::Reforge {
            let item = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            let cost = item.tier as u64 * 30;
            if character.dust < cost {
                return Err(CraftError::InsufficientDust(cost));
            }
            let outcome = {
                let mut rng = rand::thread_rng();
                character.reforge_item(item_id, &mut rng)?
            };
            character.dust -= cost;
            character.last_crafted_item_id = Some(item_id.to_string());
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(CraftResult::Reforged(outcome));
        }
        let has_token = allow_token_use && character.craft_token_count(action) > 0;
        // Token crafts always veil, when there's real randomness to
        // choose between (see `CraftAction::is_veilable` - Scour has
        // nothing to pick between regardless) - shows a new player the
        // full range of outcomes instead of one auto-applied result,
        // "so they can learn how to use it". Still entirely free either
        // way - the token just guarantees the choice instead of an
        // auto-pick, it doesn't cost the usual veil surcharge on top.
        let veiled = action.is_veilable() && (has_token || veiled);
        let use_token = has_token;
        // A nominal per-tier surcharge on top of the action's own base
        // cost - 3 dust per tier, on EVERY craft action including Scour
        // (per a live request), waived by a token same as the base cost
        // is (a token craft stays "entirely free either way").
        let tier_cost = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?.tier as u64 * TIER_CRAFT_DUST_COST;
        let cost = if use_token { 0 } else { action.base_cost() + tier_cost + if veiled { VEIL_EXTRA_COST } else { 0 } };
        if character.dust < cost {
            return Err(CraftError::InsufficientDust(cost));
        }

        if veiled && action == CraftAction::Annulment {
            let item = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            if item.locked {
                return Err(CraftError::ItemLocked);
            }
            if item.affixes.is_empty() {
                return Err(CraftError::NothingToRemove);
            }
            let (item_name, slot, tier, perfect) = (item.name.clone(), item.slot, item.tier, item.perfect);
            let existing: Vec<(Affix, f64)> = item.affixes.clone();
            let candidates = {
                let mut rng = rand::thread_rng();
                let mut idxs: Vec<usize> = (0..existing.len()).collect();
                idxs.shuffle(&mut rng);
                idxs.truncate(2);
                idxs.into_iter()
                    .map(|i| {
                        let (affix, value) = existing[i];
                        VeilCandidate::Currency(CraftOutcome {
                            item_name: item_name.clone(),
                            slot,
                            tier,
                            action,
                            affix_added: None,
                            affix_value: None,
                            affix_removed: Some(affix),
                            affix_removed_value: Some(value),
                            affixes_removed: 0,
                            now_locked: false,
                            unique_affix_added: None,
                            polished_affixes: Vec::new(),
                            chancing_previous: Vec::new(),
                            new_quality_percent: None,
                            perfect,
                        })
                    })
                    .collect::<Vec<_>>()
            };
            if use_token {
                character.consume_craft_token(action);
            } else {
                character.dust -= cost;
            }
            self.pending_veils.lock().await.insert(
                username.to_lowercase(),
                PendingVeil { action: PendingVeilAction::Currency { item_id: item_id.to_string(), action }, candidates, chancing_remaining: Vec::new(), chancing_committed: Vec::new() },
            );
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(CraftResult::PendingChoice);
        }

        if veiled && action == CraftAction::Chancing {
            let item = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            if item.locked {
                return Err(CraftError::ItemLocked);
            }
            if item.affixes.is_empty() {
                return Err(CraftError::NothingToReroll);
            }
            let (item_name, slot, tier, perfect) = (item.name.clone(), item.slot, item.tier, item.perfect);
            let mut types: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
            let target = types.remove(0);
            let remaining = types;
            let candidates = {
                let mut rng = rand::thread_rng();
                Self::chancing_step_candidates(character, item_id, target, &item_name, slot, tier, perfect, &mut rng)?
            };
            if use_token {
                character.consume_craft_token(action);
            } else {
                character.dust -= cost;
            }
            self.pending_veils.lock().await.insert(
                username.to_lowercase(),
                PendingVeil {
                    action: PendingVeilAction::Currency { item_id: item_id.to_string(), action },
                    candidates,
                    chancing_remaining: remaining,
                    chancing_committed: Vec::new(),
                },
            );
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(CraftResult::PendingChoice);
        }

        if veiled {
            let pool = character.craftable_affix_pool(item_id, action)?;
            if pool.is_empty() {
                return Err(CraftError::NoCandidatesLeft);
            }
            let item = character.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            let (item_name, slot, tier, perfect) = (item.name.clone(), item.slot, item.tier, item.perfect);
            // ThreadRng isn't Send, so it can't still be in scope at the
            // .await calls below (this fn gets spawned on tokio's
            // multi-threaded runtime) - a block scope forces it to
            // actually drop here.
            let candidates = {
                let mut rng = rand::thread_rng();
                // Weighted, not a plain uniform sample - veiling must NOT
                // be a way to reliably see (and pick) Leech among the 3
                // choices when it's meant to be 10x rarer than everything
                // else (see `affix_weight`).
                let picks = weighted_affix_pick(&pool, 3, &mut rng);
                let mut candidates = Vec::with_capacity(picks.len());
                for affix in picks {
                    let value = character.roll_craft_affix_value(item_id, affix, &mut rng).ok_or(CraftError::ItemNotFound)?;
                    candidates.push(VeilCandidate::Currency(CraftOutcome {
                        item_name: item_name.clone(),
                        slot,
                        tier,
                        action,
                        affix_added: Some(affix),
                        affix_value: Some(value),
                        affix_removed: None,
                        affix_removed_value: None,
                        affixes_removed: 0,
                        now_locked: action == CraftAction::Krangle,
                        unique_affix_added: None,
                        polished_affixes: Vec::new(),
                        chancing_previous: Vec::new(),
                        new_quality_percent: None,
                        perfect,
                    }));
                }
                candidates
            };
            if use_token {
                character.consume_craft_token(action);
            } else {
                character.dust -= cost;
            }
            self.pending_veils.lock().await.insert(
                username.to_lowercase(),
                PendingVeil { action: PendingVeilAction::Currency { item_id: item_id.to_string(), action }, candidates, chancing_remaining: Vec::new(), chancing_committed: Vec::new() },
            );
            self.persist_characters(&characters);
            drop(characters);
            self.broadcast_state().await;
            return Ok(CraftResult::PendingChoice);
        }

        let outcome = {
            let mut rng = rand::thread_rng();
            character.craft(item_id, action, &mut rng)?
        };
        if use_token {
            character.consume_craft_token(action);
        } else {
            character.dust -= cost;
        }
        character.last_crafted_item_id = Some(item_id.to_string());
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        Ok(CraftResult::Applied(outcome))
    }

    /// Web dashboard: read-only check for whether `username` currently
    /// has a veiled craft awaiting a choice - lets the dashboard show
    /// the "pick your outcome" view instead of the normal crafting form.
    pub async fn pending_veil(&self, username: &str) -> Option<PendingVeil> {
        self.pending_veils.lock().await.get(&username.to_lowercase()).cloned()
    }

    /// Web dashboard: applies the candidate at `chosen_index` from
    /// `username`'s pending veiled craft (see `craft_item`/
    /// `recombine_gear` with `veiled: true`) and clears the pending
    /// state. This is where a veiled craft's item mutation/consumption
    /// ACTUALLY happens, using the EXACT rolled candidate the player
    /// already saw - nothing is re-rolled at commit time. Out-of-range
    /// `chosen_index` or no pending veil at all both just no-op rather
    /// than erroring - not a reachable UI state under normal use. Returns
    /// the real applied outcome (not just the pre-application candidate)
    /// so the caller has the same shape to work with as the immediate
    /// (non-veiled) `craft_item`/`recombine_gear` paths - e.g. for the
    /// web dashboard's "here's what just changed" popup. A veiled
    /// Chancing candidate is the one exception: it returns
    /// `VeilChosenOutcome::ChancingContinues` (no popup) and inserts a
    /// FRESH `PendingVeil` for the next affix slot instead, if
    /// `pending.chancing_remaining` isn't empty yet - see that field's own
    /// doc.
    pub async fn choose_veil_outcome(&self, username: &str, chosen_index: usize) -> Option<VeilChosenOutcome> {
        let key = username.to_lowercase();
        let pending = self.pending_veils.lock().await.remove(&key)?;
        let chosen = pending.candidates.get(chosen_index)?.clone();

        let mut characters = self.characters.lock().await;
        let character = characters.get_mut(&key)?;
        let mut result: Option<VeilChosenOutcome> = None;
        let mut recombine_crit: Option<(String, EquipSlot, u32, Option<Affix>)> = None;
        let mut next_pending: Option<PendingVeil> = None;
        match (&pending.action, &chosen) {
            (PendingVeilAction::Currency { item_id, action }, VeilCandidate::Currency(outcome))
                if *action == CraftAction::Chancing =>
            {
                // A Chancing candidate always carries BOTH affix_removed
                // (this slot's old type/value) and affix_added (the
                // picked new type/value) - see `chancing_step_candidates`.
                if let (Some(new_affix), Some(new_value), Some(old_affix)) = (outcome.affix_added, outcome.affix_value, outcome.affix_removed) {
                    if character.apply_chancing_reroll(item_id, old_affix, new_affix, new_value).is_some() {
                        character.last_crafted_item_id = Some(item_id.clone());
                        let mut committed = pending.chancing_committed.clone();
                        committed.push((old_affix, new_affix, new_value));
                        let mut remaining = pending.chancing_remaining.clone();
                        if let Some(next_target) = remaining.pop() {
                            if let Some(item) = character.find_item_by_id(item_id) {
                                let (item_name, slot, tier, perfect) = (item.name.clone(), item.slot, item.tier, item.perfect);
                                let mut rng = rand::thread_rng();
                                if let Ok(next_candidates) = Self::chancing_step_candidates(character, item_id, next_target, &item_name, slot, tier, perfect, &mut rng) {
                                    next_pending = Some(PendingVeil {
                                        action: PendingVeilAction::Currency { item_id: item_id.clone(), action: *action },
                                        candidates: next_candidates,
                                        chancing_remaining: remaining,
                                        chancing_committed: committed,
                                    });
                                    result = Some(VeilChosenOutcome::ChancingContinues);
                                }
                            }
                        } else if let Some(item) = character.find_item_by_id(item_id) {
                            let final_outcome = CraftOutcome {
                                item_name: item.name.clone(),
                                slot: item.slot,
                                tier: item.tier,
                                action: CraftAction::Chancing,
                                affix_added: None,
                                affix_value: None,
                                affix_removed: None,
                                affix_removed_value: None,
                                affixes_removed: 0,
                                now_locked: false,
                                unique_affix_added: None,
                                polished_affixes: committed.iter().map(|&(_, new_a, new_v)| (new_a, new_v)).collect(),
                                chancing_previous: committed.iter().map(|&(old_a, _, _)| old_a).collect(),
                                new_quality_percent: None,
                                perfect: item.perfect,
                            };
                            result = Some(VeilChosenOutcome::Currency(final_outcome));
                        }
                    }
                }
            }
            (PendingVeilAction::Currency { item_id, action }, VeilCandidate::Currency(outcome)) => {
                // Currency veil candidates either ADD an affix (every
                // action except Annulment) or REMOVE one (Annulment only,
                // see `CraftAction::Annulment`'s doc) - never both.
                let applied = match (outcome.affix_added, outcome.affix_value, outcome.affix_removed) {
                    (Some(affix), Some(value), _) => character.apply_craft_affix(item_id, *action, affix, value),
                    (_, _, Some(affix)) => character.apply_annulment_removal(item_id, affix),
                    _ => Err(CraftError::ItemNotFound),
                };
                if let Ok(applied) = applied {
                    character.last_crafted_item_id = Some(item_id.clone());
                    result = Some(VeilChosenOutcome::Currency(applied));
                }
            }
            (PendingVeilAction::Recombine { item_id_a, item_id_b }, VeilCandidate::Recombine(roll)) => {
                let mut rng = rand::thread_rng();
                let outcome = character.apply_recombine_roll(item_id_a, item_id_b, roll.clone(), &mut rng);
                recombine_crit = Some((outcome.item_name.clone(), outcome.slot, outcome.new_tier, outcome.bonus_affix));
                character.last_crafted_item_id = Some(outcome.item_id.clone());
                result = Some(VeilChosenOutcome::Recombine(outcome));
            }
            _ => {}
        }
        let display_name = character.display_name.clone();
        self.persist_characters(&characters);
        drop(characters);
        if let Some(next_pending) = next_pending {
            self.pending_veils.lock().await.insert(key, next_pending);
        }
        self.broadcast_state().await;
        if let Some((item_name, slot, tier, bonus_affix)) = recombine_crit {
            self.announce_gear_crit(display_name, GearCritSource::Recombine, &item_name, slot, tier, bonus_affix);
        }
        result
    }

    /// Picks one random currently-equipped item (any slot, indestructible
    /// or not) and replaces it with a fresh one 2-4 tiers above what it
    /// was — always applied unconditionally (a deliberate paid action, not
    /// a drop roll that might not be an upgrade). Reforging an
    /// indestructible item keeps the result indestructible too. `None` if
    /// nothing's equipped. Fully synchronous (never crosses an `.await`),
    /// so the `ThreadRng` inside is never alive across a suspend point.
    ///
    /// The reforged item ALWAYS keeps every affix the original had (a
    /// reforge upgrades what you've got, it doesn't gamble it away) -
    /// unlike a fresh drop's own independent roll (see `roll_affixes`).
    /// On top of that, a 1% "crit" adds ONE more bonus affix (a type not
    /// already present) that it wouldn't otherwise have gotten - a real,
    /// rare bonus, not the guaranteed extra roll a plain drop gets.
    /// That crit is reported back via `ReforgeOutcome::bonus_affix` so
    /// the caller can announce it specially.
    pub(crate) fn reforge_equipped_item(character: &mut Character) -> Option<ReforgeOutcome> {
        let eligible: Vec<EquipSlot> = EQUIP_SLOTS.into_iter().filter(|&slot| character.equipped(slot).as_ref().is_some_and(|i| !i.locked)).collect();
        if eligible.is_empty() {
            return None;
        }
        let mut rng = rand::thread_rng();
        let slot = eligible[rng.gen_range(0..eligible.len())];
        let existing = character.equipped(slot).as_ref().unwrap();
        let old_tier = existing.tier;
        let was_indestructible = existing.is_indestructible();
        // A unique affix (see `Item::unique_affix`'s doc) must carry
        // forward too, same as power_roll/indestructibility below - a
        // live report of a player's unique implicit vanishing off their
        // chest piece traced to this being the one field this function
        // forgot to copy, since `item` below starts as a completely
        // fresh `Item` (always `unique_affix: None`) rather than a
        // mutation of `existing`.
        let unique_affix = existing.unique_affix;
        // Perfect Quality (see `PERFECT_QUALITY_MULT`'s doc) needs to
        // survive a reforge too, same "don't silently drop a special
        // property" reasoning as unique_affix above.
        let was_perfect = existing.perfect;
        // Same "don't silently drop a special property" carry-over as
        // unique_affix above - Sacred's implicit (see
        // `Item::sacred_affix`'s doc) would otherwise vanish on every
        // "Reforge Now" use, the exact bug class a live report already
        // caught for unique_affix.
        let sacred_affix = existing.sacred_affix;
        // Once-per-lineage crit tracking (see `Item::crit_bonus_affixes`'s
        // doc) MUST carry forward here too - this fn rebuilds a brand new
        // `Item` every call (unlike `Character::reforge_item`, which
        // mutates in place), so without this, every single reforge would
        // silently reset the gate back to "never crit yet", defeating the
        // entire point of the tracking for this specific path.
        // carried_affixes below is a straight 1:1 type-preserving copy of
        // existing.affixes (just rescaled), so no filtering is needed here
        // the way roll_recombine needs it when merging two different
        // sources - every crit-tagged type that was present stays present.
        let crit_bonus_affixes = existing.crit_bonus_affixes.clone();
        // Captured from `existing` (the item BEING reforged), same as
        // `was_perfect` above - see `reforge_crit_chance`'s doc for why
        // this feeds the bonus-affix roll below a higher chance the
        // better this item's quality already was.
        let quality_percent = existing.quality_percent();
        // Carries the EXISTING item's power_roll forward instead of
        // rolling a fresh one - see that field's doc. Reforge only ever
        // raises tier (reforge_tier_jump is always positive), so reusing
        // the same roll guarantees power can only go up alongside it.
        let power_roll = existing.power_roll;
        let new_tier = old_tier + reforge_tier_jump(old_tier, &mut rng);
        // Every carried-over affix scales up by the same tier ratio power
        // does - exact, since affix_base_value is purely linear in tier
        // with no constant term (see Item::sync_tier_to, which uses the
        // identical ratio for Krangle's tier growth). A live report
        // caught this NOT happening - a reforged item's tier (and power)
        // went up but its existing modifiers stayed frozen at their old,
        // lower-tier values, same bug class Krangle had.
        let tier_ratio = new_tier as f64 / old_tier.max(1) as f64;
        let carried_affixes: Vec<(Affix, f64)> = existing.affixes.iter().map(|&(affix, value)| (affix, value * tier_ratio)).collect();
        let mut item = generate_item_at_tier_with_roll(slot, new_tier, power_roll, &mut rng);
        item.affixes = carried_affixes;
        item.unique_affix = unique_affix;
        item.sacred_affix = sacred_affix;
        item.crit_bonus_affixes = crit_bonus_affixes;
        if was_perfect {
            // `carried_affixes` above already preserves the 20% boost on
            // every affix correctly (tier_ratio scaling is a simple
            // multiply, so an already-boosted value stays proportionally
            // boosted) - only `power` needs reapplying, since
            // `generate_item_at_tier_with_roll` recomputed it fresh at
            // the new tier with no knowledge of the Perfect bonus. NOT
            // `make_item_perfect(item)` here - that would double-apply
            // the affix multiplier on top of the already-boosted values.
            item.power = compute_power(slot, new_tier, power_roll) * PERFECT_QUALITY_MULT;
            item.perfect = true;
        }
        // Once-per-lineage gate - see Character::reforge_item's identical
        // check/Item::crit_bonus_affixes's doc.
        let bonus_affix = if !item.reforge_crit_used() && rng.gen_bool(reforge_crit_chance(quality_percent, was_perfect)) {
            let present: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
            let candidates: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| !present.contains(a) && a.is_eligible_for_slot(slot)).collect();
            // Same Perfect-Quality-aware roll as `roll_craft_affix_value` -
            // `was_perfect`'s block above already set `item.perfect`, so a
            // reforge's rare bonus affix needs the same boost its 3-4
            // siblings already carry, not a plain roll.
            let mult = if was_perfect { PERFECT_QUALITY_MULT } else { 1.0 };
            weighted_affix_pick(&candidates, 1, &mut rng).first().copied().map(|affix| {
                let jitter = rng.gen_range(0.85..1.15);
                item.affixes.push((affix, affix_base_value(affix, new_tier) * jitter * mult));
                item.record_reforge_crit(affix);
                affix
            })
        } else {
            None
        };
        if was_indestructible {
            item.max_uses = None;
        }
        let item_name = item.name.clone();
        character.equip(item);
        character.sync_retreat_status();
        Some(ReforgeOutcome { item_name, slot, old_tier, new_tier, bonus_affix })
    }

    /// Called for every chat message — grants a small amount of passive
    /// XP to `username`'s character, if they've joined, rate-limited per
    /// user so spamming chat can't farm levels. Returns the new level if
    /// this happened to trigger a level-up.
    pub async fn grant_activity_xp(&self, username: &str) -> Option<u32> {
        let key = username.to_lowercase();

        {
            let mut last = self.last_activity_xp.lock().await;
            if let Some(prev) = last.get(&key) {
                if prev.elapsed() < ACTIVITY_XP_COOLDOWN {
                    return None;
                }
            }
            last.insert(key.clone(), Instant::now());
        }

        let mut characters = self.characters.lock().await;
        let Some(character) = characters.get_mut(&key) else { return None };
        let leveled = character.add_xp(ACTIVITY_XP_AMOUNT);
        self.persist_characters(&characters);
        drop(characters);
        self.broadcast_state().await;
        leveled
    }

    /// Runs forever, triggering an auto-battle encounter every
    /// `ENCOUNTER_INTERVAL` — called once from main.rs. The first
    /// (immediate) tick is skipped so an encounter doesn't fire the
    /// instant the bot starts up.
    pub fn spawn_encounter_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ENCOUNTER_INTERVAL);
            interval.tick().await;
            loop {
                interval.tick().await;
                // !rampage/Permanent Rampage (2026-08-16) - `spawn_rampage_loop`
                // is the sole driver of encounters while active, on its
                // own faster cadence; skip this tick entirely rather than
                // letting both loops fire fights at once.
                if self.rampage_active().await {
                    continue;
                }
                // A new scheduled cycle just started - viewers get a
                // fresh FORCE_BOSS_MAX_PER_CYCLE budget of forced fights
                // regardless of whether this natural tick's own fight
                // actually runs (nobody joined, etc.) - see
                // try_force_encounter.
                *self.forced_boss_count.lock().await = 0;
                self.run_encounter(None).await;
            }
        });
    }

    /// Runs forever, triggering a basic-enemy filler fight every
    /// `BASIC_ENCOUNTER_INTERVAL` — see `run_basic_encounter`. Separate
    /// timer/loop from the boss encounter above (60s vs 10min), so the two
    /// fire independently; if they land close together the overlay just
    /// cuts to whichever arrived last, same tradeoff as !nextencounter
    /// firing early already has.
    pub fn spawn_basic_encounter_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(BASIC_ENCOUNTER_INTERVAL);
            interval.tick().await;
            loop {
                interval.tick().await;
                // !rampage/Permanent Rampage (2026-08-16) - see the
                // matching guard in `spawn_encounter_loop`; every
                // encounter is a boss fight while a rampage is active, so
                // this filler loop sits out entirely rather than sneaking
                // in a basic fight between rampage encounters.
                if self.rampage_active().await {
                    continue;
                }
                self.run_basic_encounter().await;
            }
        });
    }

    /// !rampage (mod tool, 2026-08-16) - runs forever, idle until
    /// `start_rampage` sets `rampage_remaining` above 0 and wakes it via
    /// `rampage_notify`, OR the admin page's Permanent Rampage toggle
    /// (`LiveTunables::permanent_rampage`) is on. While active it's the
    /// SOLE driver of encounters (`spawn_encounter_loop`/
    /// `spawn_basic_encounter_loop` both sit out - see their own guards) -
    /// runs one boss encounter (`run_encounter` with `forced_boss: None`,
    /// same random pick a normal boss tick would use), then waits
    /// `max(RAMPAGE_MIN_INTERVAL, this fight's real overlay playback
    /// time)` before the next one - "the timer between fights is 1
    /// minute, or delays if the current fight is taking longer than 1
    /// minute", per the exact request. Playback time uses the same
    /// `700ms charge + display_duration_ms + 1800ms resolve` formula
    /// `run_encounter`'s own downed-revive delay already uses. Called
    /// once from main.rs, alongside the other two encounter loops.
    ///
    /// `rampage_remaining` is loaded from `RAMPAGE_STATE_PATH` at
    /// `AdventureManager::new` (2026-08-17) - a restart mid-rampage comes
    /// back up with the same count still loaded, and since a nonzero value
    /// makes the outer idle-wait skip entirely (see the loop body), this
    /// loop just resumes firing immediately rather than needing any
    /// special "was a rampage in progress" bootstrap logic.
    ///
    /// Permanent Rampage (2026-08-16) never touches `rampage_remaining`
    /// at all while active - it's read fresh from `live_tunables()` on
    /// every iteration instead, so toggling it off mid-fight just falls
    /// straight back to whatever `rampage_remaining` happens to be (0
    /// unless a `!rampage` countdown is ALSO independently in progress)
    /// rather than needing its own separate counter. Turning it ON while
    /// this loop is idle-waiting on `rampage_notify` still needs a wake -
    /// see `do_save_tunables`, which fires one on every save.
    pub fn spawn_rampage_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                let permanent = self.live_tunables().permanent_rampage;
                if !permanent && *self.rampage_remaining.lock().await == 0 {
                    self.rampage_notify.notified().await;
                }
                loop {
                    let permanent = self.live_tunables().permanent_rampage;
                    if !permanent && *self.rampage_remaining.lock().await == 0 {
                        break;
                    }
                    let duration_ms = self.run_encounter(None).await;
                    if !permanent {
                        let new_remaining = {
                            let mut remaining = self.rampage_remaining.lock().await;
                            *remaining = remaining.saturating_sub(1);
                            *remaining
                        };
                        self.persist_rampage_remaining(new_remaining);
                        if new_remaining == 0 {
                            let _ = self.rampage_complete_tx.send(());
                        }
                    }
                    let playback_ms = 700u64 + duration_ms.unwrap_or(0) as u64 + 1800;
                    let wait = Duration::from_millis(playback_ms).max(RAMPAGE_MIN_INTERVAL);
                    tokio::time::sleep(wait).await;
                }
            }
        });
    }

    /// Whether ANY form of rampage is currently active - either the
    /// finite `!rampage` countdown (`rampage_remaining`) or the admin
    /// page's Permanent Rampage toggle (`LiveTunables::permanent_rampage`,
    /// see its own doc). Every site that used to check
    /// `rampage_remaining > 0` directly now goes through this instead, so
    /// Permanent Rampage gets the exact same "boss fights only, instant
    /// revives, filler loops sit out" treatment `!rampage` already has,
    /// for free.
    pub(crate) async fn rampage_active(&self) -> bool {
        self.live_tunables().permanent_rampage || *self.rampage_remaining.lock().await > 0
    }

    /// !rampage (mod tool, 2026-08-16) - queues `RAMPAGE_ENCOUNTER_COUNT`
    /// forced boss encounters and wakes `spawn_rampage_loop` to start
    /// running them immediately. Calling this again while a rampage is
    /// already in progress just resets the remaining count back to
    /// `RAMPAGE_ENCOUNTER_COUNT` (extends it, doesn't stack on top).
    pub async fn start_rampage(&self) {
        *self.rampage_remaining.lock().await = RAMPAGE_ENCOUNTER_COUNT;
        self.persist_rampage_remaining(RAMPAGE_ENCOUNTER_COUNT);
        self.rampage_notify.notify_one();
    }

    /// Mirrors `rampage_remaining` to `RAMPAGE_STATE_PATH` so a restart can
    /// resume the countdown instead of losing it - called from both
    /// `start_rampage` and `spawn_rampage_loop`'s own decrement, the only
    /// two places that ever change the value.
    fn persist_rampage_remaining(&self, value: u32) {
        if let Err(err) = crate::state::save_json(RAMPAGE_STATE_PATH, &value) {
            tracing::error!("Failed to persist rampage state to {RAMPAGE_STATE_PATH}: {err}");
        }
    }

    /// !rampage vote (2026-08-17) - registers one non-mod chat login's
    /// vote (deduped, see `rampage_votes`' own doc) and either reports the
    /// running count or, once `RAMPAGE_VOTE_THRESHOLD` distinct voters is
    /// reached, clears the vote set and calls `start_rampage` directly -
    /// same trigger a mod's own !rampage uses, so it gets the exact same
    /// 50-fight/instant-revive behavior.
    pub async fn register_rampage_vote(&self, user: &str) -> RampageVoteOutcome {
        let count = {
            let mut votes = self.rampage_votes.lock().await;
            votes.insert(user.to_lowercase());
            votes.len() as u32
        };
        if count >= RAMPAGE_VOTE_THRESHOLD {
            self.rampage_votes.lock().await.clear();
            self.start_rampage().await;
            RampageVoteOutcome::Triggered
        } else {
            RampageVoteOutcome::Counted(count)
        }
    }

    /// !nextencounter (mod tool) — runs one encounter right now instead
    /// of waiting for the timer, for testing/streamer pacing control.
    /// `forced` is the command's optional boss-name argument (2026-08-15,
    /// e.g. `!nextencounter bahamut`) - `None` for the normal random
    /// pick, `Some(name)` to force a specific boss (and, for Dragon,
    /// optionally a specific look - see `BossKind::parse_forced`) right
    /// now regardless of stage/rotation. Unrecognized names report back
    /// as `UnknownBoss` rather than silently falling back to random, so a
    /// typo doesn't look like it worked.
    pub async fn trigger_encounter_now(self: &Arc<Self>, forced: Option<&str>) -> TriggerEncounterOutcome {
        let forced_boss = match forced {
            None => None,
            Some(name) => match BossKind::parse_forced(name) {
                Some((kind, sprite)) => Some(ForcedBoss::Single(kind, sprite)),
                None => return TriggerEncounterOutcome::UnknownBoss,
            },
        };
        if self.run_encounter(forced_boss).await.is_some() {
            TriggerEncounterOutcome::Triggered
        } else {
            TriggerEncounterOutcome::NobodyJoined
        }
    }

    /// `!event intro <boss>` (2026-08-17, a live request: announce a new
    /// boss the moment it ships) - forces a fight guaranteed to showcase
    /// exactly one `BossKind`, but at whatever boss COUNT the current
    /// world stage would normally use (see `ForcedBoss::StageScaled`/
    /// `boss_count_for_stage`) rather than `trigger_encounter_now`'s
    /// always-exactly-one-boss shape. commands.rs's handler is expected to
    /// follow a `Triggered` result with its own chat announcement + wiki
    /// link - this method only runs the fight.
    pub async fn trigger_boss_intro(self: &Arc<Self>, name: &str) -> TriggerEncounterOutcome {
        let Some((kind, _)) = BossKind::parse_forced(name) else {
            return TriggerEncounterOutcome::UnknownBoss;
        };
        if self.run_encounter(Some(ForcedBoss::StageScaled(kind))).await.is_some() {
            TriggerEncounterOutcome::Triggered
        } else {
            TriggerEncounterOutcome::NobodyJoined
        }
    }

    /// "Force Boss Fight" channel points redemption - same underlying
    /// action as !nextencounter, but rate-limited to
    /// `FORCE_BOSS_MAX_PER_CYCLE` uses per `ENCOUNTER_INTERVAL` window
    /// (see `forced_boss_count`/`spawn_encounter_loop`'s reset), since
    /// unlike the mod-only !nextencounter this is open to any viewer
    /// with enough points. The slot is claimed BEFORE the fight runs (so
    /// two redemptions arriving milliseconds apart can't both slip past
    /// the cap) and given back if it turns out nobody was eligible to
    /// fight - same "a refunded attempt never costs the resource" shape
    /// as the Reforge Gear redemption's cooldown handling.
    pub async fn try_force_encounter(self: &Arc<Self>) -> ForceBossOutcome {
        {
            let mut count = self.forced_boss_count.lock().await;
            if *count >= FORCE_BOSS_MAX_PER_CYCLE {
                return ForceBossOutcome::CycleLimitReached;
            }
            *count += 1;
        }
        if self.run_encounter(None).await.is_some() {
            ForceBossOutcome::Triggered
        } else {
            *self.forced_boss_count.lock().await -= 1;
            ForceBossOutcome::NobodyJoined
        }
    }

    /// !clearbattlefield (mod tool) — forces EVERY character OFF the
    /// battlefield and into "needs to !join again" state, regardless of
    /// whether their gear is actually worn or they were mid-fight/
    /// mid-revive - a clean-slate reset for the whole roster, not a
    /// "bring everyone back" shortcut. Reuses the same retreated_since/
    /// `!join` gate a real gear-driven retreat uses (see
    /// `JoinOutcome::Rejoined`) so there's one single "am I on the
    /// battlefield" mechanism rather than two - a character marked this
    /// way just has nothing to actually repair once they `!join` back.
    /// Anyone ALREADY retreated (their own gear-wear clock already
    /// running) is left exactly as they were - this doesn't reset an
    /// existing retreat's timer. Returns how many characters were newly
    /// pulled off the field, so the command can report "nobody was on
    /// it" instead of a no-op-looking success.
    pub async fn clear_battlefield(&self) -> usize {
        self.downed_until.lock().await.clear();

        let now = SystemTime::now();
        let mut characters = self.characters.lock().await;
        let mut affected = 0;
        for character in characters.values_mut() {
            if character.retreated_since.is_none() {
                character.retreated_since = Some(now);
                affected += 1;
            }
        }
        if affected > 0 {
            self.persist_characters(&characters);
        }
        drop(characters);
        self.broadcast_state().await;
        affected
    }

    /// Returns false (without doing anything else) if the roster is
    /// empty — the timer-driven loop just skips that tick, but
    /// `trigger_encounter_now` needs to know so !nextencounter can report
    /// it instead of looking like it silently did nothing.
    /// Who's actually eligible to fight right now - shared by both
    /// encounter types. Excludes anyone still sitting out a knockout
    /// (REVIVE_DURATION) or currently retreated; stale downed_until
    /// entries are pruned here rather than on a separate timer, since
    /// this runs at least once a minute via the basic encounter loop
    /// anyway.
    ///
    /// A retreat past RETREAT_REPAIR_DURATION gets its gear auto-repaired
    /// for free here AND clears `retreated_since` on its own - a
    /// gear-wear-out retreat auto-revives once the free repair lands, no
    /// `!join` needed (2026-08-17, reverted back to this original
    /// behavior after a since-undone 2026-08-16 change that required an
    /// explicit `!join` even after the free repair). `repair_all_cost() > 0`
    /// still does double duty here: it's what makes this a no-op for a
    /// `!clearbattlefield`-forced retreat (nothing to repair, so this
    /// block never runs for that case at all) - that path still requires
    /// an explicit `!join`, only a genuine gear-wear-out retreat
    /// auto-revives.
    async fn eligible_fighters(&self) -> HashMap<String, Character> {
        let now = SystemTime::now();

        let downed: std::collections::HashSet<String> = {
            let mut downed_until = self.downed_until.lock().await;
            downed_until.retain(|_, &mut t| t > now);
            downed_until.keys().cloned().collect()
        };

        let mut characters = self.characters.lock().await;
        let mut auto_repaired = false;
        for character in characters.values_mut() {
            if let Some(since) = character.retreated_since {
                // repair_all_cost() > 0 guards against redoing (and
                // repersisting) this every single tick forever once the
                // hour's up - only actually touches anything the first
                // time it finds something still worn. Also what keeps
                // this a no-op for a !clearbattlefield-forced retreat
                // (see this fn's own doc).
                if now.duration_since(since).unwrap_or_default() >= RETREAT_REPAIR_DURATION && character.repair_all_cost() > 0 {
                    character.repair_all_gear();
                    character.retreated_since = None;
                    auto_repaired = true;
                }
            }
        }
        if auto_repaired {
            self.persist_characters(&characters);
        }

        characters.iter().filter(|(id, c)| !downed.contains(*id) && c.retreated_since.is_none()).map(|(id, c)| (id.clone(), c.clone())).collect()
    }

    /// Thin gate wrapper around `run_encounter_inner` (2026-08-17, a live
    /// request: fights could overlap, since nothing serialized the several
    /// independent trigger paths that all end up here). Holds `fight_gate`
    /// for this call's entire duration - see that field's own doc for why
    /// that's what actually prevents overlap, not just a timestamp check.
    /// The returned `display_duration_ms` is only the fight ITSELF - the
    /// overlay also spends a fixed 700ms charge-in + 1800ms resolve banner
    /// around it (same `700 + display_duration_ms + 1800` total this same
    /// file already computes for the downed-revival delay, just duplicated
    /// here rather than shared - see that site's own doc), so the gate
    /// waits for the overlay's TRUE full playback, not just the fight's
    /// own slice of it, before adding the requested 5s floor on top.
    /// Updated on both the `Some` and `None` paths, so even a "nobody
    /// eligible" tick still enforces the flat 5s floor.
    async fn run_encounter(self: &Arc<Self>, forced_boss: Option<ForcedBoss>) -> Option<u32> {
        let mut gate = self.fight_gate.lock().await;
        let now = Instant::now();
        if now < *gate {
            tokio::time::sleep(*gate - now).await;
        }
        let result = self.run_encounter_inner(forced_boss).await;
        let overlay_playback_ms = result.map(|ms| 700u64 + ms as u64 + 1800).unwrap_or(0);
        *gate = Instant::now() + Duration::from_millis(overlay_playback_ms) + Duration::from_secs(5);
        result
    }

    /// Returns the fight's `display_duration_ms` (see `compress_events`)
    /// if it actually ran, `None` if nobody was eligible - `Some`/`None`
    /// rather than the old plain `bool`, since `spawn_rampage_loop` needs
    /// the real duration to know how long to wait before the next fight
    /// (2026-08-16 - see that fn's doc). Actual fight logic - see
    /// `run_encounter` for the overlap/spacing gate wrapped around this.
    async fn run_encounter_inner(self: &Arc<Self>, forced_boss: Option<ForcedBoss>) -> Option<u32> {
        let fighting = self.eligible_fighters().await;
        if fighting.is_empty() {
            return None;
        }
        // Read once up front - see `LiveTunables`'s doc. Every LOOT_MULT/
        // sand/wings/celestial-shard/boss-difficulty constant this function
        // used to read directly now comes off this snapshot instead.
        let tunables = self.live_tunables();
        let participants = fighting.values().map(|c| c.display_name.clone()).collect::<Vec<_>>();

        // Picked (and immediately persisted) here, excluding whichever
        // boss fought last - see WorldState::last_boss_kind's doc, "never
        // the same boss twice in a row" per the request. two/three-boss
        // stages spawn 2/3 fully DISTINCT bosses (the max possible once
        // last fight's kind is excluded from the 4-variant pool); four/
        // five-boss stages (2026-08-16) ask for more than that, so once
        // the distinct pool runs out, `random_excluding_multiple` fills
        // the rest with plain random repeats instead - see its own doc.
        let (stage, boss_kinds, power_mult) = {
            let mut world = self.world.lock().await;
            let stage = world.stage;
            // !nextencounter's forced boss (2026-08-15) - a single boss
            // of exactly the requested kind, bypassing the normal
            // 1/2/3-boss stage scaling AND the "never the same as last
            // fight" exclusion entirely (a deliberate override, not a
            // natural roll) - simplest, most predictable shape for
            // testing a specific boss/ability. Still updates
            // `last_boss_kind` below same as a natural pick, so the NEXT
            // untouched roll still excludes it.
            let mut boss_count_rng = rand::thread_rng();
            let boss_kinds = match forced_boss {
                Some(ForcedBoss::Single(kind, _)) => vec![kind],
                Some(ForcedBoss::StageScaled(kind)) => vec![kind; boss_count_for_stage(stage, &tunables, &mut boss_count_rng)],
                None => {
                    let count = boss_count_for_stage(stage, &tunables, &mut boss_count_rng);
                    BossKind::random_excluding_multiple(world.last_boss_kind, count, &mut boss_count_rng)
                }
            };
            world.last_boss_kind = boss_kinds.last().copied();
            self.persist_world(&world);
            (stage, boss_kinds, world.boss_power_mult)
        };
        let avg_level = fighting.values().map(|c| c.level as f64).sum::<f64>() / fighting.len() as f64;
        // Each boss is independently generated at FULL strength (not a
        // shared/split stat budget the way a basic encounter's multiple
        // weaker mobs are) - a stage-50+/90+ fight is meant to be
        // proportionally more threat, not the same total difficulty split
        // across more bodies. `boss_stats_for` also applies the flat
        // `LATE_CONTENT_DIFFICULTY_MULT` bump per-boss once `stage` is
        // past `LATE_CONTENT_STAGE`, same as everything else here.
        let bosses: Vec<(BossStats, Option<BossKind>, f64)> =
            boss_kinds.iter().map(|&kind| (boss_stats_for(stage, fighting.len(), avg_level, power_mult, &tunables), Some(kind), power_mult)).collect();
        let boss_stats_snapshot: Vec<BossStats> = bosses.iter().map(|(s, _, _)| s.clone()).collect();

        let (won, units, events, rolls) = simulate_battle(&fighting, bosses, stage);
        let (events, display_duration_ms) = compress_events(events);

        // Anyone this fight's log actually knocked out (a real Defeat
        // event, not just "didn't fight") sits out the next REVIVE_DURATION
        // worth of encounters.
        let newly_downed: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                CombatEvent::Defeat { unit, .. } if !is_enemy_unit_id(unit) => Some(unit.clone()),
                _ => None,
            })
            .collect();

        let mut loot: Vec<LootDrop> = Vec::new();
        let mut broken: Vec<BrokenItem> = Vec::new();
        let mut retreated: Vec<String> = Vec::new();

        // Snapshotted from `fighting` (pre-fight levels), not re-read from
        // `characters` below - a level-up mid-block (via add_xp) shouldn't
        // reshuffle who counts as "the group" for this fight's catch-up
        // math. See `catchup_multiplier`.
        let group_levels: Vec<u32> = fighting.values().map(|c| c.level).collect();
        let catchup: HashMap<String, f64> = fighting.iter().map(|(id, c)| (id.clone(), catchup_multiplier(c.level, &group_levels))).collect();

        {
            let mut characters = self.characters.lock().await;
            // Own scope, dropped before this block's later .await points -
            // same ThreadRng-isn't-Send reasoning as the loot roll below.
            let mut win_rng = rand::thread_rng();
            for id in fighting.keys() {
                let Some(character) = characters.get_mut(id) else { continue };
                if won {
                    character.wins += 1;
                    // Victory XP scales with stage so later fights stay
                    // worth doing, AND gets the same catch-up multiplier
                    // as pity's payout size (see `catchup_multiplier`) -
                    // per a live request to extend catch-up to XP too, so
                    // newer/lower-level players level up faster relative
                    // to the group, not just gear up faster. Unlike dust/
                    // the natural loot rolls below (still deliberately
                    // flat), XP has no per-fight "pool" to keep even, so
                    // there's no equivalent tradeoff to preserve here.
                    // Base/scaling both cut roughly in half (10+2*stage ->
                    // 5+stage) as part of a live "really slow down
                    // progression" request - stage has no ceiling and was
                    // making a single win worth more and more forever,
                    // independent of the character's own level (see
                    // `xp_to_next_level`'s doc for the other half of this
                    // change, and the combined effect).
                    let xp_catchup = catchup.get(id).copied().unwrap_or(1.0);
                    character.add_xp(((5 + stage as u64) as f64 * xp_catchup).round() as u64);
                    // Every boss kill also grants dust to everyone who
                    // fought - 1-3 per stage completed, rolled per player,
                    // +loot_mult on top. Deliberately NOT catch-up-scaled -
                    // per the request, the natural per-fight reward (dust
                    // and the drop rolls below) stays evenly distributed
                    // by party size/stage alone; catch-up only touches
                    // PITY's guaranteed payouts (see the pity pass below).
                    character.dust += ((win_rng.gen_range(1..=3) * stage) as f64 * tunables.loot_mult).round() as u64;
                    // Sand (2026-08-15, a live request) - every win grants
                    // 1-3 sand, rolled per player same as the dust grant
                    // above, PLUS a boss-specific 2-3 bonus on top (a real
                    // boss kill pays more than a basic-encounter win - see
                    // `run_basic_encounter`'s own, boss-bonus-less grant),
                    // all scaled by `sand_mult` (see `LiveTunables`'s doc -
                    // deliberately a separate dial from `loot_mult`).
                    character.sand += ((win_rng.gen_range(1..=3) + win_rng.gen_range(2..=3)) as f64 * tunables.sand_mult).round() as u64;

                    // Perfect Quality's per-character milestone (see
                    // `received_first_perfect`'s doc) - every character
                    // gets their OWN guaranteed Perfect item the first
                    // time they personally take part in a stage-90+ boss
                    // kill, independent of (and stacking with) the
                    // separate shared per-kill Perfect drop rolled below.
                    if stage >= tunables.late_content_stage && !character.received_first_perfect {
                        let slot = EQUIP_SLOTS[win_rng.gen_range(0..EQUIP_SLOTS.len())];
                        let item = make_item_perfect(generate_item(slot, stage, &mut win_rng));
                        let item_name = item.name.clone();
                        let item_tier = item.tier;
                        let item_affixes = item.affixes.clone();
                        let outcome = character.receive_item(item);
                        character.received_first_perfect = true;
                        loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    }
                    // Sacred's own per-character milestone (2026-08-16, a
                    // live request) - same shape as Perfect's above, just
                    // gated on `SACRED_STAGE_THRESHOLD` (200) instead of
                    // `late_content_stage` (90) and tracked independently
                    // (see `received_first_sacred`'s doc - not either/or
                    // with the Perfect milestone, a character can and
                    // usually will earn both, at different stages).
                    if stage >= SACRED_STAGE_THRESHOLD && !character.received_first_sacred {
                        let slot = EQUIP_SLOTS[win_rng.gen_range(0..EQUIP_SLOTS.len())];
                        let item = make_item_sacred(generate_item(slot, stage, &mut win_rng), &mut win_rng);
                        let item_name = item.name.clone();
                        let item_tier = item.tier;
                        let item_affixes = item.affixes.clone();
                        let outcome = character.receive_item(item);
                        character.received_first_sacred = true;
                        loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    }
                } else {
                    character.losses += 1;
                }

                // Gear wears down through use - every equipped item this
                // character actually fought with ages by one use
                // (indestructible gear, max_uses: None, never does). Once
                // an item hits its lifespan it just sits at 0 effective
                // power (see `effective_power`) rather than being
                // destroyed or unequipped - repairing it back to full is
                // a planned follow-up, not built yet. Only announced the
                // one time it actually crosses into 0%, not every fight
                // after. Basic-enemy filler fights (run_basic_encounter)
                // don't cost durability at all - only real boss fights do.
                for slot in EQUIP_SLOTS {
                    let Some(item) = character.equipped_mut(slot) else { continue };
                    let Some(max_uses) = item.max_uses else { continue };
                    if item.uses < max_uses {
                        item.uses += 1;
                        if item.uses >= max_uses {
                            let item_name = item.name.clone();
                            broken.push(BrokenItem { display_name: character.display_name.clone(), item_name, slot });
                        }
                    }
                }

                // Opt-in auto-repair (see `Character::auto_repair`'s doc) -
                // right after this fight's decay above so it's spending
                // dust on damage that JUST happened, and before the
                // retreat check below so a character who can afford it
                // never actually retreats over gear this auto-repaired.
                // `repair_all` itself is already a safe no-op (Err,
                // nothing spent) whenever nothing needs it or dust is
                // short, so this never needs its own precondition check.
                if character.auto_repair {
                    let _ = character.repair_all();
                }

                // If that decay just wore out the LAST piece of working
                // gear they had, they retreat - excluded from every
                // encounter (boss and basic) until they repair (dust) or
                // swap in working gear, or RETREAT_REPAIR_DURATION of
                // rest auto-repairs everything for free.
                if character.retreated_since.is_none() && character.all_gear_worn_out() {
                    character.retreated_since = Some(SystemTime::now());
                    retreated.push(character.display_name.clone());
                }
            }

            let mut rng = rand::thread_rng();
            let fighting_ids: Vec<&String> = fighting.keys().collect();
            // Tracks who actually won something off the random rolls
            // below, purely to feed the pity pass after them (see
            // advance_pity) - empty on a loss, since neither roll runs
            // then anyway.
            let mut item_recipients: HashSet<String> = HashSet::new();
            let mut token_recipients: HashSet<String> = HashSet::new();

            // Loot roll - wins only. One item per 5 players in the fight
            // (rounded up), +LOOT_MULT on top, each independently awarded
            // to a uniformly random participant in a random slot - NOT
            // catch-up-weighted (see the dust grant's doc above, same
            // "natural roll stays evenly distributed" reasoning) - always
            // lands in their bag (never auto-equipped; see
            // Character::add_to_inventory), unless the bag's already full
            // at 50, in which case it's lost.
            if won {
                let mut num_drops = ((((fighting_ids.len() + 4) / 5) as f64) * tunables.loot_mult).round() as usize;
                // Perfect Quality (see `make_item_perfect`'s doc) - every
                // stage-90+ BOSS kill (never a basic encounter - this is
                // `run_encounter` only) guarantees exactly one Perfect
                // item among its drops, never more. Floors the drop count
                // at 1 so a very small party's normal roll rounding down
                // to 0 can't skip the guarantee entirely.
                if stage >= tunables.late_content_stage {
                    num_drops = num_drops.max(1);
                }
                // Perfect's own guarantee only fires half as often once
                // Sacred is also active (2026-08-17, a live request:
                // "perfect items start dropping with half frequency when
                // sacreds are dropping") - rolled ONCE per fight, not once
                // per drop, so a multi-drop fight can't converge back
                // toward guaranteed by just getting more tries at the coin
                // flip. Below the Sacred threshold, unchanged (always
                // guaranteed once stage >= late_content_stage).
                let perfect_guarantee_active =
                    stage >= tunables.late_content_stage && (stage < SACRED_STAGE_THRESHOLD || rng.gen_bool(0.5));
                let mut perfect_awarded = false;
                let mut sacred_awarded = false;
                for _ in 0..num_drops {
                    // Recomputed every iteration, not once outside the
                    // loop - a multi-drop fight can fill someone's bag on
                    // an earlier iteration of this same loop.
                    let eligible = exclude_full_inventory(&fighting_ids, &characters);
                    let Some(&recipient_id) = eligible.get(rng.gen_range(0..eligible.len())) else { continue };
                    let slot = EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())];
                    let mut item = generate_item(slot, stage, &mut rng);
                    // Sacred (2026-08-16, a live request) - same "exactly
                    // one guaranteed per qualifying kill" shape as Perfect
                    // below, just gated at `SACRED_STAGE_THRESHOLD` (200).
                    // Takes priority over the plain-Perfect guarantee on
                    // THIS drop (a stage-200+ kill's first drop becomes
                    // Sacred, not merely Perfect) - but doesn't consume
                    // the separate Perfect guarantee, so a multi-drop
                    // stage-200+ fight can still also guarantee a second,
                    // ordinary Perfect item among its other drops.
                    if stage >= SACRED_STAGE_THRESHOLD && !sacred_awarded {
                        item = make_item_sacred(item, &mut rng);
                        sacred_awarded = true;
                    } else if perfect_guarantee_active && !perfect_awarded {
                        item = make_item_perfect(item);
                        perfect_awarded = true;
                    }
                    let Some(character) = characters.get_mut(recipient_id) else { continue };
                    let item_name = item.name.clone();
                    let item_tier = item.tier;
                    let item_affixes = item.affixes.clone();
                    let outcome = character.receive_item_with_auto_disenchant(item, &mut rng, tunables.sand_mult);
                    maybe_drop_wings(character, &mut rng, tunables.wings_drop_chance);
                    maybe_drop_celestial_shard(character, &mut rng, tunables.celestial_shard_drop_chance);
                    if maybe_drop_unique_shard(character, &mut rng, tunables.celestial_shard_drop_chance) {
                        let _ = self.unique_shard_tx.send(UniqueShardEvent { display_name: character.display_name.clone() });
                    }
                    loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    item_recipients.insert(recipient_id.clone());
                }

                // Craft token drop, same "one roll per N players" shape
                // as the loot roll above - 1 free token plus 1 more per
                // 10 players in the fight, +loot_mult on top, each
                // independently a random CraftAction handed to a
                // uniformly random participant. See `Character::craft_tokens`.
                let num_tokens = ((1 + fighting_ids.len() / 10) as f64 * tunables.loot_mult).round() as usize;
                for _ in 0..num_tokens {
                    let Some(&recipient_id) = fighting_ids.get(rng.gen_range(0..fighting_ids.len())) else { continue };
                    let action = DROPPABLE_CRAFT_ACTIONS[rng.gen_range(0..DROPPABLE_CRAFT_ACTIONS.len())];
                    if let Some(character) = characters.get_mut(recipient_id) {
                        character.add_craft_token(action, 1);
                        token_recipients.insert(recipient_id.clone());
                    }
                }
            }

            // Pity pass - every participant, win OR lose (a losing fight
            // never grants loot either, so it still counts as "no item
            // this fight" toward pity - see `advance_pity`). A boss fight
            // always uses the BOSS_*_PITY_GAIN rates, faster than a basic
            // fight's - see those constants' docs. A payout here is a
            // real item/token, generated and awarded exactly like a
            // lucky roll would (the item joins the same `loot` list, so
            // it shows up in the fight's chat summary too), just outside
            // the random roll - and, per the request, THIS is where
            // catch-up actually lives now: a below-median player's pity
            // payout is worth more (see `pity_reward_count`), not the
            // natural roll above.
            for &id in &fighting_ids {
                let catchup_mult = catchup.get(id).copied().unwrap_or(1.0);
                let got_item = item_recipients.contains(id);
                let item_triggered = match characters.get_mut(id) {
                    Some(character) => advance_pity(&mut character.item_pity, got_item, BOSS_ITEM_PITY_GAIN),
                    None => continue,
                };
                if item_triggered {
                    for _ in 0..pity_reward_count(catchup_mult, &mut rng) {
                        // A pity-guaranteed item is even more wasteful to
                        // lose than a natural roll's, so it redirects to
                        // another eligible (non-full) participant if THIS
                        // player's own bag is already full - same
                        // `exclude_full_inventory` pool the natural roll
                        // above draws from, just only consulted when
                        // needed (pity's whole point is THIS player gets
                        // it whenever they actually have room).
                        let bag_full = characters.get(id).is_some_and(|c| c.inventory.len() >= INVENTORY_CAPACITY);
                        let recipient_id: &String = if bag_full {
                            let eligible = exclude_full_inventory(&fighting_ids, &characters);
                            eligible.get(rng.gen_range(0..eligible.len())).copied().unwrap_or(id)
                        } else {
                            id
                        };
                        let slot = EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())];
                        let item = generate_item(slot, stage, &mut rng);
                        let Some(character) = characters.get_mut(recipient_id) else { continue };
                        let item_name = item.name.clone();
                        let item_tier = item.tier;
                        let item_affixes = item.affixes.clone();
                        let outcome = character.receive_item_with_auto_disenchant(item, &mut rng, tunables.sand_mult);
                        maybe_drop_wings(character, &mut rng, tunables.wings_drop_chance);
                        maybe_drop_celestial_shard(character, &mut rng, tunables.celestial_shard_drop_chance);
                        if maybe_drop_unique_shard(character, &mut rng, tunables.celestial_shard_drop_chance) {
                            let _ = self.unique_shard_tx.send(UniqueShardEvent { display_name: character.display_name.clone() });
                        }
                        loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    }
                }
                let got_token = token_recipients.contains(id);
                let Some(character) = characters.get_mut(id) else { continue };
                if advance_pity(&mut character.craft_pity, got_token, BOSS_CRAFT_PITY_GAIN) {
                    for _ in 0..pity_reward_count(catchup_mult, &mut rng) {
                        let action = DROPPABLE_CRAFT_ACTIONS[rng.gen_range(0..DROPPABLE_CRAFT_ACTIONS.len())];
                        character.add_craft_token(action, 1);
                    }
                }
            }

            self.persist_characters(&characters);
        }

        {
            let mut world = self.world.lock().await;
            // Rolling win/loss history for adaptive_difficulty_scale - see
            // its doc. Pushed here, before either branch below reads it,
            // so THIS fight's own outcome already counts toward the next
            // fight's adaptive scale (not a one-fight-stale lag).
            world.recent_boss_outcomes.push_back(won);
            while world.recent_boss_outcomes.len() > OUTCOME_WINDOW {
                world.recent_boss_outcomes.pop_front();
            }
            let adaptive = adaptive_difficulty_scale(&world.recent_boss_outcomes);
            if won {
                world.stage += 1;
                // Dynamic difficulty ratchet-up - reviews THIS fight's own
                // log and pushes boss_power_mult based on how comfortable
                // the win was (see `post_win_power_boost`'s doc), scaled by
                // `adaptive` so a party winning MORE than the target ~5:2
                // rate gets pushed harder, deliberately uncapped (see
                // `BOSS_POWER_MULT_MIN`'s doc for why no ceiling exists
                // anymore - `adaptive` easing off is what prevents a
                // runaway instead). `dynamic_scaling_mult` (2026-08-16,
                // consolidating 6 previously-hardcoded constants into one
                // live dial - see its own doc) scales how far this boost
                // actually moves `boss_power_mult` from wherever it
                // currently sits, toward the same target it would've hit
                // anyway - 1.0 reproduces the old hardcoded-only behavior
                // exactly, 0.0 freezes boss_power_mult against win/loss
                // streaks entirely.
                let raw_boost = post_win_power_boost(&units, &events) * adaptive;
                let scaled_boost = 1.0 + (raw_boost - 1.0) * tunables.dynamic_scaling_mult;
                world.boss_power_mult = (world.boss_power_mult * scaled_boost).max(BOSS_POWER_MULT_MIN);
            } else {
                // A wipe knocks the party back 1 stage (never below 1) so
                // losing streaks don't just stall forever at the same wall
                // (2026-08-16: cut back from 2 stages per a live request).
                world.stage = world.stage.saturating_sub(1).max(1);
                // Counterweight to the win-triggered ratchet-up above -
                // eases boss_power_mult back off, scaled by `adaptive` so
                // a party ALREADY losing more than the target rate
                // recovers faster (adaptive < 1 here makes the decay
                // gentler - dividing, not multiplying, since a smaller
                // `adaptive` should mean LESS reduction) than a flat rate
                // would, instead of grinding an already-too-hard streak
                // further into the ground. Same `dynamic_scaling_mult`
                // scaling as the win branch above.
                let raw_decay = (LOSS_POWER_DECAY / adaptive).clamp(0.3, 0.95);
                let scaled_decay = 1.0 - (1.0 - raw_decay) * tunables.dynamic_scaling_mult;
                world.boss_power_mult = (world.boss_power_mult * scaled_decay).max(BOSS_POWER_MULT_MIN);
            }
            self.persist_world(&world);
        }

        self.broadcast_state().await;
        let mut result = EncounterResult {
            kind: EncounterKind::Boss,
            stage,
            won,
            participants,
            units,
            events,
            display_duration_ms,
            loot,
            broken,
            enemy_name: None,
            enemy_count: None,
            retreated,
            boss_sprites: {
                let mut sprite_rng = rand::thread_rng();
                // A forced sprite (bahamut/purple, see BossKind::parse_forced)
                // only ever applies to `ForcedBoss::Single`'s one boss, so
                // there's no ambiguity about which of several bosses it'd
                // apply to - `StageScaled` (multi-boss) never carries one,
                // so every instance just rolls its own look normally (see
                // `ForcedBoss::StageScaled`'s own doc).
                let forced_sprite = match forced_boss {
                    Some(ForcedBoss::Single(_, sprite)) => sprite,
                    _ => None,
                };
                boss_kinds.iter().map(|k| forced_sprite.unwrap_or_else(|| k.sprite(&mut sprite_rng)).to_string()).collect()
            },
            // `rolls`' own `at_ms` values are real simulation time
            // (`simulate_battle`'s uncompressed clock) - NOT rescaled by
            // `compress_events` above the way `events`' `at_ms` just was,
            // since only `events` needs display-pacing compression for
            // the overlay. `hit_id` correlation between an `events` entry
            // and the `rolls` that fed it doesn't depend on `at_ms` at
            // all, so the two fields being on different timescales
            // doesn't affect correctness - just means `rolls.at_ms` isn't
            // directly comparable to `events.at_ms` in the same file.
            rolls,
            // Transient placeholder - overwritten below by
            // `save_last_fight`'s return, the same idiom `events` follows.
            summary: FightSummarySnapshot::default(),
        };
        result.summary = save_last_fight(&result, boss_stats_snapshot);
        // Presentational only - see `thin_events_for_overlay`'s own doc.
        // The full-fidelity `events` was already persisted above and
        // `newly_downed` already scanned it in full; only the copy going
        // out over the wire to the overlay gets thinned.
        result.events = thin_events_for_overlay(result.events, &result.units);
        let _ = self.encounter_tx.send(result);

        // The fight above is resolved instantly, but the overlay spends
        // the next several seconds actually playing it back (charge-in,
        // then the real fight compressed into display_duration_ms, then a
        // resolve banner - see overlay.html's CHARGE_MS/RESOLVE_MS).
        // Marking someone downed right now, before that plays out, would
        // make them render as a ghost (see isDowned in overlay.html) for
        // the ENTIRE replay of the very fight that defeated them, instead
        // of just showing their in-fight knockout like normal and only
        // then sitting out - exactly the "everyone's dead before the
        // fight even starts" bug this replaced. Delayed the same way
        // main.rs already delays the chat result announcement for the
        // same reason, just carried through the resolve banner too so the
        // ghost treatment only kicks in once the overlay is back to idle.
        // !rampage/Permanent Rampage (2026-08-16) - everyone gets
        // instantly revived after every rampage fight, per the exact
        // request, so the downed-timer insert below is skipped entirely
        // while active (rather than inserted-then-immediately-expired,
        // which would still briefly exclude them from `eligible_fighters`
        // between fights).
        let rampage_active = self.rampage_active().await;
        if !newly_downed.is_empty() && !rampage_active {
            let manager = Arc::clone(self);
            let delay_ms = 700u64 + display_duration_ms as u64 + 1800;
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                let revive_at = SystemTime::now() + REVIVE_DURATION;
                {
                    let mut downed_until = manager.downed_until.lock().await;
                    for id in &newly_downed {
                        downed_until.insert(id.clone(), revive_at);
                    }
                }
                manager.broadcast_state().await;
            });
        }

        Some(display_duration_ms)
    }

    /// A much weaker, frequent (every `BASIC_ENCOUNTER_INTERVAL`) filler
    /// fight against "an assortment of enemies" — unlike a boss fight,
    /// it does NOT advance the world stage or touch wins/losses/XP, it's
    /// purely a dust-and-loot side activity. Enemy count matches party
    /// size (`fighting.len()`), which also drives the loot roll: each
    /// enemy is a 5% chance of a drop, cumulative and rolling over past
    /// 100% into guaranteed extra drops rather than capping at one. Every
    /// participant's gear still wears down through use, same as a boss
    /// fight, and a knockout here sits them out the same
    /// `REVIVE_DURATION` window too - it's a real (if easier) fight, not
    /// a freebie.
    ///
    /// No dedicated sprite art for these yet — the "Basic Enemies" folder
    /// only had a broken Windows shortcut (a .url file pointing at a
    /// Discord CDN link, not an actual downloaded image), so the overlay
    /// currently just reuses the boss art pool for these too. Drop real
    /// PNGs in public_adventure_overlay/sprites/ and this can be pointed
    /// at them instead.
    /// Thin gate wrapper around `run_basic_encounter_inner` - identical
    /// reasoning/mechanism as `run_encounter`'s own wrapper (see that
    /// fn's doc), sharing the SAME `fight_gate` so a basic encounter and
    /// a boss encounter can never overlap each other either, not just
    /// their own kind.
    async fn run_basic_encounter(self: &Arc<Self>) -> bool {
        let mut gate = self.fight_gate.lock().await;
        let now = Instant::now();
        if now < *gate {
            tokio::time::sleep(*gate - now).await;
        }
        let result = self.run_basic_encounter_inner().await;
        let overlay_playback_ms = result.map(|ms| 700u64 + ms as u64 + 1800).unwrap_or(0);
        *gate = Instant::now() + Duration::from_millis(overlay_playback_ms) + Duration::from_secs(5);
        result.is_some()
    }

    /// Actual basic-encounter fight logic - see `run_basic_encounter` for
    /// the overlap/spacing gate wrapped around this. `Some(display_duration_ms)`
    /// if it ran, `None` if nobody was eligible - mirrors `run_encounter_inner`'s
    /// own shape now that both feed the same gate.
    async fn run_basic_encounter_inner(self: &Arc<Self>) -> Option<u32> {
        let fighting = self.eligible_fighters().await;
        if fighting.is_empty() {
            return None;
        }
        // See `run_encounter`'s identical snapshot/doc.
        let tunables = self.live_tunables();
        let participants = fighting.values().map(|c| c.display_name.clone()).collect::<Vec<_>>();
        // Randomized 0.5x-1.5x party size (rounded, never below 1) rather
        // than a fixed 1:1 match - a bigger roll means a tougher, more
        // rewarding fight (see basic_enemy_stats_for and the loot roll
        // below, both driven off this same number), so no two basic
        // encounters against the same roster feel identical.
        let num_enemies = {
            let mut rng = rand::thread_rng();
            let multiplier: f64 = rng.gen_range(0.5..=1.5);
            ((fighting.len() as f64 * multiplier).round() as usize).max(1)
        };

        let (stage, power_mult) = {
            let world = self.world.lock().await;
            (world.stage, world.boss_power_mult)
        };
        let avg_level = fighting.values().map(|c| c.level as f64).sum::<f64>() / fighting.len() as f64;
        let group_stats = basic_enemy_stats_for(stage, num_enemies, avg_level, power_mult, &tunables);
        let enemy_stats = split_into_enemies(group_stats, num_enemies);
        let enemy_name = BASIC_ENEMY_NAMES[rand::thread_rng().gen_range(0..BASIC_ENEMY_NAMES.len())].to_string();

        let (won, units, events, rolls) = simulate_battle(&fighting, enemy_stats.into_iter().map(|s| (s, None, 1.0)).collect(), stage);
        let (events, display_duration_ms) = compress_events(events);

        let newly_downed: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                CombatEvent::Defeat { unit, .. } if !is_enemy_unit_id(unit) => Some(unit.clone()),
                _ => None,
            })
            .collect();

        let mut loot: Vec<LootDrop> = Vec::new();
        let mut broken: Vec<BrokenItem> = Vec::new();

        // See run_encounter's identical snapshot - same "pre-fight group,
        // not reshuffled by anything that happens below" reasoning.
        let group_levels: Vec<u32> = fighting.values().map(|c| c.level).collect();
        let catchup: HashMap<String, f64> = fighting.iter().map(|(id, c)| (id.clone(), catchup_multiplier(c.level, &group_levels))).collect();

        {
            let mut characters = self.characters.lock().await;
            // Own scope, dropped before this block's later .await points -
            // same ThreadRng-isn't-Send reasoning as elsewhere in this file.
            let mut rng = rand::thread_rng();
            for id in fighting.keys() {
                let Some(character) = characters.get_mut(id) else { continue };
                if won {
                    // 50%-100% of the current stage, rolled per player,
                    // +loot_mult on top - deliberately no wins/losses/XP/
                    // stage change here, this is a supplementary dust/loot
                    // activity, not the main progression driver. NOT
                    // catch-up-scaled - see run_encounter's identical dust
                    // grant's doc, "natural roll stays evenly distributed,
                    // catch-up only touches pity" per the request.
                    let dust = (rng.gen_range(0.5..=1.0) * stage as f64 * tunables.loot_mult).round().max(1.0) as u64;
                    character.dust += dust;
                    // Sand (2026-08-15, a live request) - same flat 1-3
                    // per-win grant `run_encounter`'s own boss win gets,
                    // just without that fight's extra 2-3 boss-only bonus
                    // on top - this is the lighter filler-fight reward,
                    // scaled by `sand_mult` same as run_encounter's own.
                    character.sand += (rng.gen_range(1..=3) as f64 * tunables.sand_mult).round() as u64;
                }
                // Deliberately no gear decay here - only real boss fights
                // wear equipment down (see run_encounter); these lighter
                // filler fights don't cost durability at all.
            }

            let fighting_ids: Vec<&String> = fighting.keys().collect();
            let mut item_recipients: HashSet<String> = HashSet::new();

            // Loot roll - wins only. 5% per enemy (+LOOT_MULT on top),
            // cumulative AND surpassing: e.g. num_enemies=30 -> 150% ->
            // one guaranteed drop plus a 50% chance of a second, not just
            // capped at one. Recipient picked uniformly at random - NOT
            // catch-up-weighted, same as run_encounter's loot roll.
            if won {
                let total_chance = num_enemies as f64 * 0.05 * tunables.loot_mult;
                let guaranteed_drops = total_chance.floor() as u32;
                let remainder = (total_chance - guaranteed_drops as f64).clamp(0.0, 1.0);
                let num_drops = guaranteed_drops + if rng.gen_bool(remainder) { 1 } else { 0 };

                for _ in 0..num_drops {
                    let eligible = exclude_full_inventory(&fighting_ids, &characters);
                    let Some(&recipient_id) = eligible.get(rng.gen_range(0..eligible.len())) else { continue };
                    let slot = EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())];
                    let item = generate_item(slot, stage, &mut rng);
                    let Some(character) = characters.get_mut(recipient_id) else { continue };
                    let item_name = item.name.clone();
                    let item_tier = item.tier;
                    let item_affixes = item.affixes.clone();
                    let outcome = character.receive_item_with_auto_disenchant(item, &mut rng, tunables.sand_mult);
                    maybe_drop_wings(character, &mut rng, tunables.wings_drop_chance);
                    maybe_drop_celestial_shard(character, &mut rng, tunables.celestial_shard_drop_chance);
                    if maybe_drop_unique_shard(character, &mut rng, tunables.celestial_shard_drop_chance) {
                        let _ = self.unique_shard_tx.send(UniqueShardEvent { display_name: character.display_name.clone() });
                    }
                    loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    item_recipients.insert(recipient_id.clone());
                }
            }

            // Pity pass - every participant, win OR lose, same reasoning
            // as run_encounter's (see its doc, including catch-up living
            // here via `pity_reward_count` and nowhere else). A basic
            // fight never rolls for a craft token at all (only a real
            // boss fight does), so craft_pity simply advances every
            // single basic fight unconditionally - `received` is always
            // false here.
            for &id in &fighting_ids {
                let catchup_mult = catchup.get(id).copied().unwrap_or(1.0);
                let got_item = item_recipients.contains(id);
                let item_triggered = match characters.get_mut(id) {
                    Some(character) => advance_pity(&mut character.item_pity, got_item, BASIC_ITEM_PITY_GAIN),
                    None => continue,
                };
                if item_triggered {
                    for _ in 0..pity_reward_count(catchup_mult, &mut rng) {
                        // Redirect to another eligible (non-full)
                        // participant if THIS player's own bag is already
                        // full - see run_encounter's identical pity-pass
                        // handling for the full reasoning.
                        let bag_full = characters.get(id).is_some_and(|c| c.inventory.len() >= INVENTORY_CAPACITY);
                        let recipient_id: &String = if bag_full {
                            let eligible = exclude_full_inventory(&fighting_ids, &characters);
                            eligible.get(rng.gen_range(0..eligible.len())).copied().unwrap_or(id)
                        } else {
                            id
                        };
                        let slot = EQUIP_SLOTS[rng.gen_range(0..EQUIP_SLOTS.len())];
                        let item = generate_item(slot, stage, &mut rng);
                        let Some(character) = characters.get_mut(recipient_id) else { continue };
                        let item_name = item.name.clone();
                        let item_tier = item.tier;
                        let item_affixes = item.affixes.clone();
                        let outcome = character.receive_item_with_auto_disenchant(item, &mut rng, tunables.sand_mult);
                        maybe_drop_wings(character, &mut rng, tunables.wings_drop_chance);
                        maybe_drop_celestial_shard(character, &mut rng, tunables.celestial_shard_drop_chance);
                        if maybe_drop_unique_shard(character, &mut rng, tunables.celestial_shard_drop_chance) {
                            let _ = self.unique_shard_tx.send(UniqueShardEvent { display_name: character.display_name.clone() });
                        }
                        loot.push(LootDrop { display_name: character.display_name.clone(), item_name, slot, outcome, tier: item_tier, affixes: item_affixes });
                    }
                }
                let Some(character) = characters.get_mut(id) else { continue };
                if advance_pity(&mut character.craft_pity, false, BASIC_CRAFT_PITY_GAIN) {
                    for _ in 0..pity_reward_count(catchup_mult, &mut rng) {
                        let action = DROPPABLE_CRAFT_ACTIONS[rng.gen_range(0..DROPPABLE_CRAFT_ACTIONS.len())];
                        character.add_craft_token(action, 1);
                    }
                }
            }

            self.persist_characters(&characters);
        }

        self.broadcast_state().await;
        let mut result = EncounterResult {
            kind: EncounterKind::Basic,
            stage,
            won,
            participants,
            units,
            events,
            display_duration_ms,
            loot,
            broken,
            enemy_name: Some(enemy_name),
            enemy_count: Some(num_enemies as u32),
            retreated: Vec::new(),
            boss_sprites: Vec::new(),
            // See `run_encounter`'s matching field for why `rolls` isn't
            // rescaled the way `events` just was above.
            rolls,
            // Transient placeholder - overwritten below by
            // `save_last_fight`'s return, the same idiom `events` follows.
            summary: FightSummarySnapshot::default(),
        };
        result.summary = save_last_fight(&result, Vec::new());
        // Presentational only - see `thin_events_for_overlay`'s own doc.
        // The full-fidelity `events` was already persisted above and
        // `newly_downed` already scanned it in full; only the copy going
        // out over the wire to the overlay gets thinned.
        result.events = thin_events_for_overlay(result.events, &result.units);
        let _ = self.encounter_tx.send(result);

        // Same reasoning as run_encounter's identical block - delay
        // marking anyone downed until the overlay's finished actually
        // playing this fight back.
        if !newly_downed.is_empty() {
            let manager = Arc::clone(self);
            let delay_ms = 700u64 + display_duration_ms as u64 + 1800;
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                let revive_at = SystemTime::now() + REVIVE_DURATION;
                {
                    let mut downed_until = manager.downed_until.lock().await;
                    for id in &newly_downed {
                        downed_until.insert(id.clone(), revive_at);
                    }
                }
                manager.broadcast_state().await;
            });
        }

        Some(display_duration_ms)
    }
}

/// Stable id the boss uses in `CombatEvent`/`CombatUnitInfo` — usernames
/// are always lowercase alphanumeric-ish and never contain underscores
/// this way, so this can't collide with a real player id.
/// Stable id prefix every enemy uses in `CombatEvent`/`CombatUnitInfo` —
/// usernames are always lowercase alphanumeric-ish and never contain
/// underscores this way, so this can't collide with a real player id. A
/// boss fight always has exactly one enemy, at index 0; a basic
/// encounter's group (see `run_basic_encounter`) gets one per member, so
/// each can be simulated, targeted, and health-barred independently.
pub(crate) const ENEMY_ID_PREFIX: &str = "__enemy_";

pub(crate) fn enemy_unit_id(index: usize) -> String {
    format!("{ENEMY_ID_PREFIX}{index}__")
}

pub(crate) fn is_enemy_unit_id(id: &str) -> bool {
    id.starts_with(ENEMY_ID_PREFIX)
}

/// Id for a boss's own summoned add (e.g. the Lich's skeletons) - the
/// only such construction site (`Some(BossKind::Lich)`'s handling in
/// `simulate_battle`) used to build this with an ad hoc
/// `format!("{boss_id}-add-{n}")` inline, unlike every other enemy
/// (`enemy_unit_id` above). Full-detail combat log (2026-08-17) - every
/// actor in the log, player/boss/summon alike, now gets its id
/// constructed the same disciplined way, through a named helper here
/// rather than a bespoke format string at the call site.
pub(crate) fn add_unit_id(boss_id: &str, index: usize) -> String {
    format!("{boss_id}-add-{index}")
}

/// Which of the 4 named bosses a real boss encounter (never a basic
/// filler fight) rolled - decides both the sprite the overlay shows (see
/// `EncounterResult::boss_sprites`) and which unique mechanic triggers
/// during the fight (see `simulate_battle`'s boss-ability handling).
/// Previously the overlay picked its boss sprite completely at random,
/// client-side, with zero connection to the actual fight mechanics -
/// giving each boss a real unique ability meant that pick had to move
/// here instead, so sprite and mechanic always agree on which boss this
/// actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BossKind {
    Lich,
    FireDemon,
    Cthulhu,
    Dragon,
    /// 2026-08-17 - a rotating capture (see `CUBE_CAPTURE_CADENCE_MS`), a
    /// wide splash attack, and a stacking defense-shred debuff on every
    /// hit it lands. See combat.rs's `NextEvent::BossAbility` arm and the
    /// `apply_hit`/`resolve_hit` shred hooks for the actual mechanics.
    GelatinousCube,
}

impl BossKind {
    const ALL: [BossKind; 5] = [BossKind::Lich, BossKind::FireDemon, BossKind::Cthulhu, BossKind::Dragon, BossKind::GelatinousCube];

    /// Picks `count` DISTINCT kinds, excluding `exclude` from the
    /// candidate pool entirely (not just "not first") - a stage-50+
    /// 2-boss fight (see `run_encounter`) uses this so neither boss
    /// repeats the previous fight's kind AND the two picked this fight
    /// are always different from each other ("never the same" per the
    /// request) - up through 3 bosses, the max distinct count possible
    /// once `exclude` is removed from the 4-variant pool. A 4-boss or
    /// 5-boss fight (2026-08-16, live-tunable stage thresholds) asks for
    /// more bosses than there are distinct kinds available, so once the
    /// distinct pool is exhausted, remaining slots are filled with plain
    /// uniform-random repeats (candidates may include `exclude` and
    /// anything already picked this fight) rather than truncating short -
    /// per an explicit "allow repeats once past 3" decision, this only
    /// kicks in past what 3 distinct bosses can cover; the 1/2/3-boss
    /// cases above are completely unaffected.
    pub(crate) fn random_excluding_multiple(exclude: Option<BossKind>, count: usize, rng: &mut impl Rng) -> Vec<BossKind> {
        let mut candidates: Vec<BossKind> = Self::ALL.into_iter().filter(|&k| Some(k) != exclude).collect();
        let mut picked = Vec::with_capacity(count);
        for _ in 0..count.min(candidates.len()) {
            let idx = rng.gen_range(0..candidates.len());
            picked.push(candidates.remove(idx));
        }
        while picked.len() < count {
            picked.push(Self::ALL[rng.gen_range(0..Self::ALL.len())]);
        }
        picked
    }

    /// !nextencounter's optional boss-name argument (2026-08-15) - `None`
    /// means "no argument, normal random pick" (the caller never calls
    /// this for that case); `Some(name)` unrecognized returns `None` too,
    /// which the caller turns into `TriggerEncounterOutcome::UnknownBoss`.
    /// The second tuple element is Dragon's-two-looks-specific: `bahamut`/
    /// `purple` force which SPRITE shows up (bypassing `sprite()`'s own
    /// 50/50 coin flip for that one forced fight), `None` there just lets
    /// the normal coin flip happen for a plain "dragon" request.
    pub(crate) fn parse_forced(name: &str) -> Option<(BossKind, Option<&'static str>)> {
        match name.to_lowercase().as_str() {
            "lich" => Some((BossKind::Lich, None)),
            "demon" | "firedemon" | "fire" => Some((BossKind::FireDemon, None)),
            "cthulhu" => Some((BossKind::Cthulhu, None)),
            "dragon" => Some((BossKind::Dragon, None)),
            "bahamut" => Some((BossKind::Dragon, Some("bosses/boss-dragon-bahamut"))),
            "purple" => Some((BossKind::Dragon, Some("bosses/boss-dragon-purple"))),
            "cube" | "gelatinouscube" => Some((BossKind::GelatinousCube, None)),
            _ => None,
        }
    }

    /// Matches `public_adventure_overlay/overlay.html`'s own `BOSS_SPRITES`
    /// list - hand-synced, same as `ALL_SPRITES`. `Dragon` has two
    /// possible looks (2026-08-15) - a 50/50 coin flip per fight, not a
    /// fixed pick, so either can show up; every other kind still has
    /// just the one.
    pub(crate) fn sprite(self, rng: &mut impl Rng) -> &'static str {
        match self {
            BossKind::Lich => "bosses/boss-lich-green",
            BossKind::FireDemon => "bosses/boss-demon-fire",
            BossKind::Cthulhu => "bosses/boss-cthulhu-blue",
            BossKind::Dragon => {
                if rng.gen_bool(0.5) {
                    "bosses/boss-dragon-bahamut"
                } else {
                    "bosses/boss-dragon-purple"
                }
            }
            // The overlay alternates cube1/cube2 client-side, cued by the
            // "Gelatinous Cube Capture" SkillCast event (see combat.rs) -
            // this is just the default/base look sent down via
            // msg.bossSprites.
            BossKind::GelatinousCube => "bosses/cube1",
        }
    }

    /// Player-facing name for chat/the overlay's per-unit label -
    /// distinct per boss so a stage-50+ 2-boss fight's health bars and
    /// chat lines don't both just say "Boss".
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            BossKind::Lich => "The Lich",
            BossKind::FireDemon => "The Fire Demon",
            BossKind::Cthulhu => "Cthulhu",
            BossKind::Dragon => "The Dragon",
            BossKind::GelatinousCube => "The Gelatinous Cube",
        }
    }

    /// The wiki's `id="..."` anchor for this boss's section (see
    /// `render_wiki_bosses`, adventure_web.rs) and the `#fragment` `!event
    /// intro` links to in its chat announcement - one shared source of
    /// truth so the two never drift apart into two different slugs.
    pub(crate) fn wiki_slug(self) -> &'static str {
        match self {
            BossKind::Lich => "lich",
            BossKind::FireDemon => "fire-demon",
            BossKind::Cthulhu => "cthulhu",
            BossKind::Dragon => "dragon",
            BossKind::GelatinousCube => "gelatinous-cube",
        }
    }
}

/// First-pass balance numbers — will need real tuning once played with
/// for real. Scales off the CURRENT party's size and average level, not
/// just stage, so the fight stays meaningfully hard as the roster grows
/// instead of an ever-larger party trivially steamrolling a
/// stage-only-scaled boss.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossStats {
    pub hp: u64,
    pub atk: u64,
    pub attack_interval_ms: u32,
    /// Secondary stats (see `Affix`/`resolve_hit`) - all zero for a
    /// basic-encounter mob (`basic_enemy_stats_for`/`split_into_enemies`
    /// never set these). Only a real boss (`boss_stats_for`) rolls them,
    /// "more as monster power scales" per the request.
    pub damage_reduction: f64,
    pub block_chance: f64,
    pub evasion: f64,
    pub increased_damage: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub splash: f64,
}

/// Across-the-board difficulty bump (HP and per-hit damage both scale
/// with this) — applied here rather than scattered across every stat
/// formula, so it cascades to everything derived from `boss_stats_for`
/// (basic encounters via `basic_enemy_stats_for`/`split_into_enemies`
/// included) with one number to tune.
///
/// This, `BOSS_DIFFICULTY_DIAL`, `BOSS_DAMAGE_MULT`, and `BOSS_HP_MULT` are
/// now `LiveTunables::difficulty_mult`/`boss_difficulty_dial`/
/// `boss_damage_mult`/`boss_hp_mult` (see that struct's doc) - live-editable
/// via the admin-only `/admin/tunables` page with no restart, instead of
/// flat consts requiring a recompile+redeploy for every tuning pass. Their
/// tuning history up to the 2026-08-16 rebalance (each had only ever been
/// ratcheted up, compounding on top of the win/loss dynamic-difficulty
/// system) is preserved in git blame on this comment.
///
/// A further bump on top of `boss_damage_mult`, but on the SCALING rate
/// specifically (how fast atk grows with stage/level - see
/// atk_stage_mult/atk_level_mult below), not a flat baseline bump -
/// applied only to the 0.08/0.15 growth coefficients, not the "1.0 +"
/// floor those multipliers start from (BOSS_DAMAGE_MULT already covers a
/// flat bump). Deliberately atk-only - hp keeps the original
/// stage_mult/level_mult unchanged, so enemy damage scales faster into
/// the late game without also inflating enemy health scaling. Reset to
/// 1.0 (no extra scaling-rate bump) in the same rebalance pass as
/// DIFFICULTY_MULT above - was +30%.
pub(crate) const BOSS_DAMAGE_SCALING_MULT: f64 = 1.0;

/// Floor for `WorldState::boss_power_mult` - a loss-triggered decay
/// (`LOSS_POWER_DECAY`) could otherwise grind toward 0 (a zero-stat,
/// unkillable-by-boredom boss) over a long enough losing streak.
/// Deliberately NO ceiling anymore (a fixed `BOSS_POWER_MULT_MAX` used to
/// live here, brought down from 20.0 to 5.0 in an earlier "severe loss
/// rate" rebalance) - per a live request, the multiplier should keep
/// climbing for as long as the party keeps winning too comfortably,
/// confirmed live to actually be necessary: it hit that old 5.0 ceiling
/// and just sat there pinned while stage-105 fights stayed trivial, since
/// a party's own growth (gear/level/now-multiplicative passive tree) has
/// no ceiling of its own. `adaptive_difficulty_scale` (see its doc) is
/// what now keeps this from repeating the earlier 7.4x runaway instead
/// of a hard cap - it actively pulls growth back down whenever the
/// recent win rate drifts too far above the target ratio.
pub(crate) const BOSS_POWER_MULT_MIN: f64 = 0.5;

/// How much a loss eases `WorldState::boss_power_mult` back off by, in
/// the ORDINARY case (see `adaptive_difficulty_scale` for how this gets
/// adjusted based on the recent win rate) - bumped again (was 15%) in an
/// earlier "severe loss rate" rebalance pass - the 15% rate wasn't
/// enough to keep the live multiplier near its intended ~1.0 neutral
/// point (it had drifted to 7.4x), so recovery needed to be faster,
/// not just the ratchet-up capped lower.
pub(crate) const LOSS_POWER_DECAY: f64 = 0.75;

/// The margin-of-victory ratio `post_win_power_boost` aims the NEXT real
/// boss fight at - see that function's own doc for the full derivation.
/// Raised from an original 0.5 (which explicitly modeled the boss to
/// out-race the party ~2x, "very likely a loss") to 1.0 ("roughly even
/// odds") - per a live request to retarget the whole system from "every
/// win should probably be followed by a loss" toward "the party should
/// win noticeably more than it loses" (an ideal ~5:2 win:loss ratio, see
/// `TARGET_WIN_RATE`) - 1.0 is a reasoned starting point for that
/// gentler target, not something simulated/proven exactly, so it may
/// still need a further live tuning pass once real win/loss data comes
/// in under the new adaptive system.
pub(crate) const WIN_TARGET_MARGIN_RATIO: f64 = 1.0;

/// How many of the most recent real boss fights `WorldState::recent_boss_outcomes`
/// remembers - the window `adaptive_difficulty_scale` computes the
/// live win rate over. Small enough to react to a real shift in how the
/// fights are going within a reasonable number of fights, large enough
/// that one lucky/unlucky fight doesn't swing the measured rate wildly.
pub(crate) const OUTCOME_WINDOW: usize = 20;

/// The win:loss ratio `adaptive_difficulty_scale` steers toward - "an
/// ideal 5:2 ratio" per the request, i.e. winning roughly 5 fights for
/// every 2 losses (~71.4%), engaging but not punishing.
pub(crate) const TARGET_WIN_RATE: f64 = 5.0 / 7.0;

/// Scales the ORDINARY per-fight adjustment (`post_win_power_boost` on a
/// win, `LOSS_POWER_DECAY` on a loss) up or down based on how the recent
/// win rate (`WorldState::recent_boss_outcomes`, last `OUTCOME_WINDOW`
/// fights) compares to `TARGET_WIN_RATE` - the actual mechanism keeping
/// `boss_power_mult` from needing a hard ceiling at all (see
/// `BOSS_POWER_MULT_MIN`'s doc): winning MORE than the target rate means
/// fights have been too easy, so growth gets amplified (pushing the next
/// fights harder); winning AT OR BELOW the target rate means the current
/// difficulty is doing its job (or is already too hard), so growth gets
/// dampened - all the way down to actively easing off, not just growing
/// slower, once the win rate drops meaningfully below target. Returns
/// 1.0 (a pure no-op on top of the ordinary adjustment) until
/// `OUTCOME_WINDOW` fights of history exist, so the very first fights
/// after this system shipped aren't skewed by an empty/tiny sample.
/// Deliberately a smooth, continuous function of the deviation (not a
/// step/threshold), so there's no sudden cliff in behavior right at the
/// target rate - and clamped to a moderate range so no single fight's
/// outcome can swing growth as violently as the pre-adaptive system's
/// static formula alone once did (the 7.4x runaway this whole mechanism
/// exists to prevent from happening again).
pub(crate) fn adaptive_difficulty_scale(recent: &std::collections::VecDeque<bool>) -> f64 {
    if recent.len() < OUTCOME_WINDOW {
        return 1.0;
    }
    let win_rate = recent.iter().filter(|&&w| w).count() as f64 / recent.len() as f64;
    let deviation = win_rate - TARGET_WIN_RATE;
    (1.0 + deviation * 2.5).clamp(0.4, 1.8)
}

/// Safety cap on how much a SINGLE win can boost `WorldState::boss_power_mult`
/// by, however dominant that win was (an unbroken no-hit stomp would
/// otherwise compute an unbounded boost since it'd be dividing by zero
/// damage taken). Brought down from 6.0 (which let a single win multiply
/// in ~2.45x) to 3.0 (~1.73x max per win) in a live "severe loss rate"
/// rebalance pass, then to 1.75 (~1.32x max per win, ~2.38x worst case
/// once combined with `adaptive_difficulty_scale`'s own up-to-1.8x) on
/// 2026-08-16, per a live request - the old 3.0 cap let even one
/// dominant win overpower several losses' worth of recovery at once.
pub(crate) const WIN_MAX_BOOST: f64 = 1.75;

/// Reads a just-finished WINNING boss fight's own combat log (`units`/
/// `events`, straight out of `simulate_battle`) and returns the factor
/// `WorldState::boss_power_mult` should be multiplied by so the NEXT real
/// boss fight is tuned to very likely go the other way - per the request,
/// "after each boss fight that results in a win... difficulty should be
/// scaled through boss health/damage/defenses/ability affects such that
/// the next encounter is very likely a loss."
///
/// The margin-of-victory signal: (the party's total HP pool * the total
/// damage the party actually landed on the boss) vs (the boss's total HP
/// pool * the party's NET damage taken - total incoming damage minus
/// healing received, floored at 1 so a fully-negated fight doesn't divide
/// by zero). Net, not raw, matters here - a healer-sustained party can log
/// a huge raw "damage taken" total while never coming remotely close to a
/// wipe, and crediting that as a close call (raw damage taken close to the
/// party's HP pool) was a real bug: live fights were logging heavy raw
/// damage but nearly all of it was getting healed back, so the computed
/// margin barely budged `boss_power_mult` even though the party was
/// stomping every fight (confirmed against a live `adventure-world.json` -
/// after ~80 stage-ups it had only crawled from 1.0 to 1.32).
///
/// That net-damage ratio is exactly "how long the party could have kept
/// tanking the boss's demonstrated real output" vs "how long the boss
/// actually took to go down" - and the fight's own duration cancels out of
/// both sides algebraically, so it doesn't matter whether the fight was
/// quick or dragged out. Well above 1.0 means the party won with room to
/// spare; at/under `WIN_TARGET_MARGIN_RATIO` means it was already close
/// (returns 1.0, a no-op, rather than easing off - a win should never
/// DECREASE the multiplier, that's what `LOSS_POWER_DECAY` is for).
///
/// The damage-to-boss side is capped at the boss's own HP pool - a win
/// guarantees the party dealt AT LEAST that much, but a big splash/crit on
/// the final blow can log well past it (a live fight logged 155% off one
/// overkill hit), and that overkill doesn't reflect any real "spare
/// capacity" the way surviving damage does. Uncapped, that inflated the
/// ratio enough that a merely-decent win (47% of the party's own HP pool
/// taken, net of healing - a real fight, not a stomp) was landing the same
/// maxed-out `WIN_MAX_BOOST` as an actual near-flawless clear.
///
/// The returned factor is exactly what's needed to push that same margin
/// down to `WIN_TARGET_MARGIN_RATIO` (clamped to `WIN_MAX_BOOST`) -
/// scaling every boss stat by a factor `m` (see `scale_by_power_mult`)
/// drops the margin ratio by `m` squared (both the boss's effective HP
/// AND its effective damage output scale up together), so the needed `m`
/// is the SQUARE ROOT of how far the ratio needs to fall.
pub(crate) fn post_win_power_boost(units: &[CombatUnitInfo], events: &[CombatEvent]) -> f64 {
    let party_hp: u64 = units.iter().filter(|u| !u.is_boss).map(|u| u.max_hp as u64).sum();
    let boss_hp: u64 = units.iter().filter(|u| u.is_boss).map(|u| u.max_hp as u64).sum();
    let is_boss_id = |id: &str| units.iter().find(|u| u.id == id).is_some_and(|u| u.is_boss);

    let mut dealt_to_boss: u64 = 0;
    let mut dealt_to_party: u64 = 0;
    let mut healing_received_by_party: u64 = 0;
    for event in events {
        match event {
            CombatEvent::Attack { target, damage, .. } => {
                if is_boss_id(target) {
                    dealt_to_boss += *damage as u64;
                } else {
                    dealt_to_party += *damage as u64;
                }
            }
            CombatEvent::Heal { target, amount, .. } => {
                if !is_boss_id(target) {
                    healing_received_by_party += *amount as u64;
                }
            }
            // Deliberately NOT counted here (unlike Heal) - a shield's
            // mitigation is already reflected in a lower Attack.damage
            // above (apply_hit consumes shield_hp before final_damage is
            // computed), so also counting the grant here would double-
            // count the same avoided damage twice and over-inflate the
            // next fight's difficulty boost. See `CombatEvent::Shield`'s
            // doc.
            CombatEvent::Shield { .. } | CombatEvent::Defeat { .. } | CombatEvent::SkillCast { .. } | CombatEvent::BuffSnapshot { .. } => {}
        }
    }
    let net_dealt_to_party = dealt_to_party.saturating_sub(healing_received_by_party).max(1);
    let capped_dealt_to_boss = dealt_to_boss.min(boss_hp);

    let margin_ratio = (party_hp as f64 * capped_dealt_to_boss as f64) / (boss_hp as f64 * net_dealt_to_party as f64);
    let boost = (margin_ratio / WIN_TARGET_MARGIN_RATIO).clamp(1.0, WIN_MAX_BOOST);
    boost.sqrt()
}

/// Dust cost of the "Wings of Flight" cosmetic MTX (see
/// `AdventureManager::purchase_wings`) - purely cosmetic, no combat
/// effect at all, just a very expensive flex.
pub const WINGS_COST: u64 = 50_000;

/// Chance, EVERY time a character is handed a real item off any loot
/// roll (normal or pity, boss or basic), that the "Wings of Flight"
/// cosmetic ALSO drops alongside it - independent of, and on top of,
/// that item. Extremely rare by design (a live request for "very rarely
/// dropped"). No-op once already owned (see `maybe_drop_wings`). Was a
/// flat const; now `LiveTunables::wings_drop_chance` (see that struct's
/// doc), passed in by the caller.
///
/// Rolls the rare bonus Wings drop - called right after every
/// `Character::receive_item` in the loot-roll/pity paths. Silent (no chat
/// announcement, no special loot-log entry) by design for now - the reward
/// reveals itself the next time this character's dashboard loads a new
/// "Wings of Flight" section.
pub(crate) fn maybe_drop_wings(character: &mut Character, rng: &mut impl Rng, wings_drop_chance: f64) -> bool {
    if character.owns_wings {
        return false;
    }
    if rng.gen_bool(wings_drop_chance) {
        character.owns_wings = true;
        true
    } else {
        false
    }
}

/// Celestial Shard's ongoing rare drop rate - "give it a rare drop for
/// all players with healing power at a .1% drop rate" per the request,
/// 10x more common than Wings' own default drop chance (still rare, just
/// not "essentially never" rare - a crafting material meant to actually be
/// used eventually, not a one-of-a-kind cosmetic). Was a flat const; now
/// `LiveTunables::celestial_shard_drop_chance`, passed in by the caller.
///
/// Rolls the rare Celestial Shard drop - same "called right after every
/// `receive_item` in the loot-roll/pity paths" precedent as
/// `maybe_drop_wings`. Used to be gated to characters with some healing
/// power invested (a pure damage-dealer had no use for a unique affix that
/// only converted healing into bonus damage) - removed 2026-08-16 per a
/// live request ("make sure the shard can roll for everyone") once
/// `UniqueAffix::CelestialConversion` got a real non-Heal effect too (see
/// its own doc / `apply_hit`'s DPS-side Celestial Shard block): every
/// archetype can equally roll and make use of it now. Unlike Wings, not a
/// one-per-character cap - a character could earn several over time
/// (nothing stops banking multiple Celestial Shard tokens, only EQUIPPING
/// more than one active copy of the affix at once is restricted - see
/// `Character::has_conflicting_unique_affix`). Silent by design, same
/// reasoning as Wings.
pub(crate) fn maybe_drop_celestial_shard(character: &mut Character, rng: &mut impl Rng, celestial_shard_drop_chance: f64) -> bool {
    if rng.gen_bool(celestial_shard_drop_chance) {
        character.add_craft_token(CraftAction::CelestialShard, 1);
        true
    } else {
        false
    }
}

/// Rolls the rare Unique Shard drop (2026-08-17, grants
/// `UniqueAffix::SplitPersonality`) - same shape/call sites as
/// `maybe_drop_celestial_shard` above, an independent roll each time it's
/// called. Takes the rate as an explicit parameter rather than reading
/// `LiveTunables::celestial_shard_drop_chance` itself, even though every
/// current call site happens to pass that same field (see the admin page's
/// "Unique Shard Drop Rate" label, which now covers both shards under one
/// number per a live request) - so splitting this into its own tunable
/// later is a one-field-add plus swapping the argument at each call site,
/// not a signature change here.
pub(crate) fn maybe_drop_unique_shard(character: &mut Character, rng: &mut impl Rng, unique_shard_drop_chance: f64) -> bool {
    if rng.gen_bool(unique_shard_drop_chance) {
        character.add_craft_token(CraftAction::UniqueShard, 1);
        true
    } else {
        false
    }
}

/// Median-relative catch-up bonus applied on top of `LOOT_MULT`: a
/// below-median party member's dust and drop odds scale up, capping at
/// +200% for whoever's at the group's lowest level, tapering down to
/// exactly +100% right at the median, then continuing to taper from
/// +100% at the median down to +0% at the group's highest level - so
/// the median member still gets a real boost (not just the trailing
/// half), and only the top of the pack fights at the baseline rate.
/// Returns a multiplier (1.0-3.0) to multiply dust/drop-weight by
/// directly, not a raw percentage. A group with no level spread at all
/// (including a solo fighter, trivially their own min/max) has nothing
/// to catch up on, so everyone's flat at 1.0 - otherwise soloing would
/// be a free, repeatable way to always sit "at the median" for a 2x
/// bonus.
/// Standard median (average of the two middle values on an even count) -
/// shared by `catchup_multiplier` (loot/dust catch-up) and
/// `prioritize_above_median` (enemy target priority). `0.0` on an empty
/// slice - callers that can actually hit that case guard it themselves
/// first (see `catchup_multiplier`'s early min/max checks).
pub(crate) fn median_u32(values: &[u32]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    if n % 2 == 1 { sorted[n / 2] as f64 } else { (sorted[n / 2 - 1] as f64 + sorted[n / 2] as f64) / 2.0 }
}

pub(crate) fn catchup_multiplier(level: u32, group_levels: &[u32]) -> f64 {
    let Some(&min) = group_levels.iter().min() else { return 1.0 };
    let Some(&max) = group_levels.iter().max() else { return 1.0 };
    if max <= min {
        return 1.0;
    }
    let (min, max) = (min as f64, max as f64);
    let median = median_u32(group_levels);
    let l = level as f64;
    let bonus_pct = if l <= median {
        // median > min is guaranteed here: if median == min, every level
        // is >= min == median, so l <= median forces l == median too,
        // never reaching this branch's division.
        if median > min { 100.0 + 100.0 * (median - l) / (median - min) } else { 100.0 }
    } else if max > median {
        100.0 * (max - l) / (max - median)
    } else {
        0.0
    };
    1.0 + bonus_pct.clamp(0.0, 200.0) / 100.0
}

/// Turns a catch-up multiplier (see `catchup_multiplier`) into a whole
/// number of rewards for ONE pity payout - per the request, this is the
/// ONLY place catch-up actually touches loot now: the natural per-fight
/// roll (item/token recipients, dust) stays uniform/unscaled, evenly
/// distributed by party size/stage alone, while a below-median player's
/// guaranteed pity payout is worth more than the normal 1. The
/// fractional remainder becomes a probabilistic extra reward (same
/// "floor plus a remainder roll" idiom as the basic-encounter loot
/// roll's own guaranteed-drops-plus-chance-of-one-more shape) rather
/// than always rounding the same way - a 1.26x multiplier is a real 26%
/// chance at a second reward, not a rounding-rule coin flip. Always at
/// least 1 (the multiplier itself is never below 1.0).
pub(crate) fn pity_reward_count(catchup_mult: f64, rng: &mut impl Rng) -> u32 {
    let whole = catchup_mult.floor().max(1.0) as u32;
    let remainder = (catchup_mult - catchup_mult.floor()).clamp(0.0, 1.0);
    whole + if rng.gen_bool(remainder) { 1 } else { 0 }
}

/// Sand roll for a disenchant (2026-08-15) - a chance EQUAL to the
/// item's own `quality_percent()` (0-100%, read as a 0.0-1.0 probability
/// directly - a Perfect item's `quality_percent()` is always exactly
/// 100%, so it always grants sand) at 1-3 sand, 0 otherwise. Shared by
/// `disenchant_from_inventory`/`disenchant_all_from_inventory` so both
/// stay in sync.
pub(crate) fn roll_disenchant_sand(quality_percent: f64, rng: &mut impl Rng, sand_mult: f64) -> u64 {
    if rng.gen_bool((quality_percent / 100.0).clamp(0.0, 1.0)) {
        (rng.gen_range(1..=3) as f64 * sand_mult).round() as u64
    } else {
        0
    }
}

#[cfg(test)]
mod fight_summary_tests {
    use super::*;

    fn player(id: &str) -> CombatUnitInfo {
        CombatUnitInfo { id: id.to_string(), display_name: id.to_string(), is_boss: false, role: None, max_hp: 1000 }
    }

    fn boss(id: &str) -> CombatUnitInfo {
        CombatUnitInfo { id: id.to_string(), display_name: id.to_string(), is_boss: true, role: None, max_hp: 10_000 }
    }

    fn attack(at_ms: u32, attacker: &str, target: &str, damage: u64, unmitigated_damage: u64, is_crit: bool, evaded: bool) -> CombatEvent {
        CombatEvent::Attack {
            at_ms,
            attacker: attacker.to_string(),
            target: target.to_string(),
            damage,
            unmitigated_damage,
            target_hp_after: 0,
            is_crit,
            evaded,
            hit_id: 0,
        }
    }

    #[test]
    fn every_non_boss_unit_gets_a_row_even_with_zero_events() {
        let units = vec![player("alice"), player("bob"), boss("__enemy_0")];
        let stats = full_player_fight_stats(&units, &[]);
        assert_eq!(stats.len(), 2);
        assert!(stats.iter().all(|s| s.hits == 0 && s.crits == 0 && s.evaded == 0 && s.damage_dealt == 0 && s.damage_taken == 0 && s.healing_done == 0));
        assert!(stats.iter().any(|s| s.id == "alice"));
        assert!(stats.iter().any(|s| s.id == "bob"));
    }

    #[test]
    fn boss_units_never_get_their_own_row() {
        let units = vec![player("alice"), boss("__enemy_0")];
        let events = vec![attack(0, "__enemy_0", "alice", 50, 50, false, false)];
        let stats = full_player_fight_stats(&units, &events);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].id, "alice");
    }

    #[test]
    fn damage_taken_counts_unmitigated_damage_even_when_evaded() {
        let units = vec![player("alice"), boss("__enemy_0")];
        let events = vec![attack(0, "__enemy_0", "alice", 0, 500, false, true)];
        let stats = full_player_fight_stats(&units, &events);
        let alice = stats.iter().find(|s| s.id == "alice").unwrap();
        assert_eq!(alice.damage_taken, 500);
        assert_eq!(alice.damage_dealt, 0);
    }

    #[test]
    fn hits_crits_and_evaded_are_counted_from_the_attackers_own_swings() {
        let units = vec![player("alice"), boss("__enemy_0")];
        let events = vec![
            attack(0, "alice", "__enemy_0", 100, 100, false, false),
            attack(1, "alice", "__enemy_0", 300, 300, true, false),
            attack(2, "alice", "__enemy_0", 0, 0, false, true),
        ];
        let stats = full_player_fight_stats(&units, &events);
        let alice = stats.iter().find(|s| s.id == "alice").unwrap();
        assert_eq!(alice.hits, 2);
        assert_eq!(alice.crits, 1);
        assert_eq!(alice.evaded, 1);
        assert_eq!(alice.damage_dealt, 400);
    }

    #[test]
    fn heal_and_shield_events_both_count_toward_healing_done() {
        let units = vec![player("alice"), player("bob"), boss("__enemy_0")];
        let events = vec![
            CombatEvent::Heal { at_ms: 0, healer: "alice".to_string(), target: "bob".to_string(), amount: 40, target_hp_after: 0 },
            CombatEvent::Shield { at_ms: 1, healer: "alice".to_string(), target: "bob".to_string(), amount: 25 },
        ];
        let stats = full_player_fight_stats(&units, &events);
        let alice = stats.iter().find(|s| s.id == "alice").unwrap();
        assert_eq!(alice.healing_done, 65);
    }

    #[test]
    fn first_player_to_die_ignores_boss_defeats_and_picks_the_earliest() {
        let units = vec![player("alice"), player("bob"), boss("__enemy_0")];
        let events = vec![
            CombatEvent::Defeat { at_ms: 500, unit: "__enemy_0".to_string() },
            CombatEvent::Defeat { at_ms: 900, unit: "bob".to_string() },
            CombatEvent::Defeat { at_ms: 300, unit: "alice".to_string() },
        ];
        assert_eq!(first_player_to_die(&units, &events), Some("alice".to_string()));
    }

    #[test]
    fn first_player_to_die_is_none_when_nobody_died() {
        let units = vec![player("alice"), boss("__enemy_0")];
        assert_eq!(first_player_to_die(&units, &[]), None);
    }

    fn player_stats(id: &str, damage_dealt: u64, damage_taken: u64, healing_done: u64) -> PlayerFightStats {
        PlayerFightStats { id: id.to_string(), display_name: id.to_string(), damage_dealt, damage_taken, healing_done, hits: 0, crits: 0, evaded: 0 }
    }

    #[test]
    fn fight_summary_from_snapshot_ranks_and_truncates_to_top_3() {
        let snapshot = FightSummarySnapshot {
            players: vec![
                player_stats("alice", 500, 100, 0),
                player_stats("bob", 300, 200, 0),
                player_stats("carol", 700, 50, 0),
                player_stats("dave", 100, 400, 0),
            ],
            ..Default::default()
        };
        let summary = fight_summary_from_snapshot(&snapshot);
        assert_eq!(summary.top_damage_dealt, vec![("carol".to_string(), 700), ("alice".to_string(), 500), ("bob".to_string(), 300)]);
        assert_eq!(summary.top_damage_taken, vec![("dave".to_string(), 400), ("bob".to_string(), 200), ("alice".to_string(), 100)]);
    }

    #[test]
    fn fight_summary_from_snapshot_does_not_pad_empty_categories_with_fabricated_zeros() {
        // A zero-contribution player (present in `players`, per
        // full_player_fight_stats' own "every non-boss unit gets a row"
        // rule) must NOT show up as a fabricated "0 healing" entry -
        // matches summarize_fight's own documented behavior.
        let snapshot = FightSummarySnapshot { players: vec![player_stats("alice", 500, 100, 0), player_stats("bob", 300, 200, 0)], ..Default::default() };
        let summary = fight_summary_from_snapshot(&snapshot);
        assert!(summary.top_healing_done.is_empty(), "nobody healed - the list must be empty, not padded with 0-value entries");
    }
}

/// Narrows a pool of fight-participant ids down to just whoever still
/// has room in their bag, when at least one such candidate exists - a
/// random natural-loot item drop shouldn't get "allocated" to someone
/// whose bag is already full at `INVENTORY_CAPACITY`, since
/// `Character::receive_item` would just report it lost instead of
/// benefiting them at all. Falls back to the full candidate list once
/// EVERY participant's bag happens to be full (better than dropping the
/// roll entirely), same convention as `prioritize_above_median`. Only
/// applies to ITEM drops - craft tokens have no bag/capacity concept.
/// Used both by the natural roll (unconditionally, every drop) and by
/// the pity pass (conditionally - only consulted once a specific
/// player's own pity actually triggers AND their bag happens to be full
/// right then, redirecting that guaranteed item to someone with room
/// rather than letting a guarantee go to waste - see both pity passes'
/// own handling).
pub(crate) fn exclude_full_inventory<'a>(candidates: &[&'a String], characters: &HashMap<String, Character>) -> Vec<&'a String> {
    let has_room: Vec<&'a String> =
        candidates.iter().copied().filter(|id| characters.get(*id).map_or(true, |c| c.inventory.len() < INVENTORY_CAPACITY)).collect();
    if has_room.is_empty() { candidates.to_vec() } else { has_room }
}

/// Pity fraction (see `Character::item_pity`/`craft_pity`) that triggers
/// a guaranteed reward - 1.0 = 100%, "at 100% or greater" per the
/// request, though `advance_pity` never actually lets it overshoot since
/// it resets to 0 the instant this is crossed.
pub(crate) const PITY_THRESHOLD: f64 = 1.0;
/// Pity gained per fight participated in without winning an item off the
/// normal random roll - a real boss fight is rarer and more meaningful
/// than a basic filler fight, so it builds pity 5x faster (25% vs 5%,
/// i.e. a guaranteed item within 4 boss fights or 20 basic fights of bad
/// luck, whichever pity track gets there first).
pub(crate) const BOSS_ITEM_PITY_GAIN: f64 = 0.25;
pub(crate) const BASIC_ITEM_PITY_GAIN: f64 = 0.05;
/// Same idea, for craft-currency tokens (see `Character::craft_pity`) -
/// 10 boss fights or 50 basic fights of bad luck at most.
pub(crate) const BOSS_CRAFT_PITY_GAIN: f64 = 0.10;
pub(crate) const BASIC_CRAFT_PITY_GAIN: f64 = 0.02;

/// Advances one pity counter (`item_pity` or `craft_pity`) by one fight's
/// worth - resets it to 0 if `received` (they already got a reward this
/// fight off the normal random roll, whether that's an item or a craft
/// token depending on which counter this is), otherwise adds `gain` and
/// checks the threshold. Returns `true` exactly when this push just
/// crossed `PITY_THRESHOLD`, meaning the caller now owes a guaranteed
/// reward outside the normal roll - `pity` is already reset to 0 in that
/// case too, so a payout never leaves leftover progress toward the next one.
pub(crate) fn advance_pity(pity: &mut f64, received: bool, gain: f64) -> bool {
    if received {
        *pity = 0.0;
        return false;
    }
    *pity += gain;
    if *pity >= PITY_THRESHOLD {
        *pity = 0.0;
        true
    } else {
        false
    }
}

/// `TWO_BOSS_STAGE`/`THREE_BOSS_STAGE`/`LATE_CONTENT_STAGE`/
/// `LATE_CONTENT_DIFFICULTY_MULT` (from this world stage on, a real boss
/// fight spawns 2/3 DISTINCT bosses at once instead of 1, and boss
/// `hp`/`atk` get a flat further bump on top of everything else in
/// `boss_stats_for`) are now `LiveTunables::two_boss_stage`/
/// `three_boss_stage`/`late_content_stage`/`late_content_difficulty_mult` -
/// live-editable via the admin-only `/admin/tunables` page, same reasoning
/// as `DIFFICULTY_MULT`'s doc above.

/// Safety ceilings for the two boss stats `power_mult` (see
/// `WorldState::boss_power_mult`/`scale_by_power_mult`) can't rely on an
/// existing 0-1 probability clamp for - `crit_multiplier`/
/// `increased_damage` are plain multipliers with no natural ceiling, so a
/// long win streak's compounding power_mult needs its own explicit cap to
/// stay numerically sane (everything else in `BossStats` already clamps to
/// `BOSS_DEFENSE_CAP`/`CRIT_CHANCE_CAP`/1.0, which stays a valid ceiling
/// regardless of power_mult).
pub(crate) const BOSS_CRIT_MULT_CAP: f64 = 6.0;
pub(crate) const BOSS_INCREASED_DAMAGE_CAP: f64 = 10.0;
/// Moved up from a local `const` inside `boss_stats_for` so
/// `scale_by_power_mult` can reuse the same ceilings post-multiplier -
/// still exactly the "0.5%-1.5% per stage"/"offensive crit cap" values
/// described where they're actually applied, below. Brought down from 90%
/// to match `Character::combat_damage_reduction`/`combat_block_chance`/
/// `combat_evasion`'s own 75% cap - a live review of the first several
/// dynamic-difficulty fights found the party barely scratching the boss
/// at all (as low as 6-13% of its HP depleted even on a loss), and a boss
/// sitting 15 points above what any player's gear/archetype can ever
/// reach on the same three stats was a real contributor, on top of the
/// dynamic multiplier itself.
pub(crate) const BOSS_DEFENSE_CAP: f64 = 0.75;
pub(crate) const CRIT_CHANCE_CAP: f64 = 0.75;

/// Applies the dynamic difficulty multiplier (see
/// `WorldState::boss_power_mult`/`post_win_power_boost`) on top of a
/// boss's stage-derived stats. `hp`/`atk` take the FULL multiplier - those
/// are the only two stats `post_win_power_boost`'s margin-ratio math
/// actually models (a fight's outcome as a race between "party HP vs
/// boss's atk-driven damage rate" and "boss HP vs party's damage rate"),
/// so scaling them by the full `m` is exactly what that derivation calls
/// for. Every other stat here MULTIPLIES with `atk` (crit/increased-damage
/// in `roll_attacker_damage`) or with each other (damage reduction, block,
/// evasion all mitigating the same incoming hit) rather than standing on
/// its own - scaling them by the SAME full `m` on top of `atk` already
/// getting the full `m` would compound the realized outcome well past the
/// derivation's target (confirmed live: a modest 1.5x multiplier was
/// producing 200-380% party-wipe overkill). They still scale - "boss
/// health/damage/defenses/ability effects" per the original request - just
/// at the dampened `sqrt(power_mult)`, so a fight still gets harder on
/// every axis without the compounding blowing past what the win/loss
/// margin was actually tuned for.
/// `attack_interval_ms` is left untouched entirely - party-size-driven
/// attack frequency is its own separate concern.
pub(crate) fn scale_by_power_mult(stats: BossStats, power_mult: f64) -> BossStats {
    let secondary_mult = power_mult.sqrt();
    BossStats {
        hp: (stats.hp as f64 * power_mult).round() as u64,
        atk: (stats.atk as f64 * power_mult).round() as u64,
        attack_interval_ms: stats.attack_interval_ms,
        damage_reduction: (stats.damage_reduction * secondary_mult).min(BOSS_DEFENSE_CAP),
        block_chance: (stats.block_chance * secondary_mult).min(BOSS_DEFENSE_CAP),
        evasion: (stats.evasion * secondary_mult).min(BOSS_DEFENSE_CAP),
        increased_damage: (stats.increased_damage * secondary_mult).min(BOSS_INCREASED_DAMAGE_CAP),
        crit_chance: (stats.crit_chance * secondary_mult).min(CRIT_CHANCE_CAP),
        crit_multiplier: (stats.crit_multiplier * secondary_mult).min(BOSS_CRIT_MULT_CAP),
        splash: (stats.splash * secondary_mult).min(1.0),
    }
}

pub(crate) fn boss_stats_for(stage: u32, party_size: usize, avg_level: f64, power_mult: f64, tunables: &LiveTunables) -> BossStats {
    let party_size = party_size.max(1) as f64;
    // Tripled 0.05 -> 0.15 (2026-08-18, a live request) - HP now scales
    // +15%/stage instead of +5%, e.g. stage 400 goes from a ~2100%
    // multiplier to ~6100%.
    let stage_mult = 1.0 + stage as f64 * 0.15;
    let level_mult = 1.0 + avg_level * 0.15;
    // Raised 0.08 -> 0.10 (2026-08-18, same request) - damage now scales
    // +10%/stage instead of +8%.
    let atk_stage_mult = 1.0 + stage as f64 * 0.10 * BOSS_DAMAGE_SCALING_MULT;
    let atk_level_mult = 1.0 + avg_level * 0.15 * BOSS_DAMAGE_SCALING_MULT;
    let jitter = 1.0 + rand::thread_rng().gen_range(-0.1..0.1);

    // The boss can only ever hit one target per swing, but a bigger
    // party brings proportionally more attackers AND healers — without
    // this, a large roster's aggregate DPS/HPS badly outpaces a boss
    // that's still only ever threatening one player at a time (confirmed
    // via real fight logs: two 8-player fights were both total
    // blowouts). Attacking faster against a bigger party keeps its
    // threat roughly in step with the party's own aggregate action rate,
    // instead of just scaling HP/per-hit damage, which left frequency —
    // the actual bottleneck — untouched.
    //
    // Halved again on 2026-08-16 (attack twice as often, at half the
    // per-hit damage below) - same total damage dealt over time, just
    // delivered in smaller, more frequent hits instead of fewer big ones.
    let attack_interval_ms = (1100.0 / (1.0 + (party_size - 1.0) * 0.15) / 2.0).round() as u32;

    // A real boss's secondary stats scale off stage alone ("more as
    // monster power scales" per the request), not party size/level like
    // hp/atk above - and roll their own independent jitter so they don't
    // move in lockstep with the hp/atk roll. First-pass numbers, same
    // "will need real tuning" caveat as everything else here.
    //
    // Block/evasion/damage reduction each scale at their own fixed rate
    // within 0.5%-1.5% per stage (per the request) - damage reduction
    // stays at the low end since it stacks multiplicatively with
    // block/evasion's own damage-avoidance rather than replacing it,
    // block sits in the middle, and evasion (full avoidance, the
    // strongest of the three) gets the top rate - hard-capped at 90%
    // apiece so a high enough stage can't make a boss literally
    // unhittable. Crit chance keeps its own older, lower cap (unrelated
    // to this request - it's an offensive stat, not a defensive one).
    let s = stage as f64;
    let boss_jitter = 1.0 + rand::thread_rng().gen_range(-0.15..0.15);

    let stats = BossStats {
        // Bumped from 42 — live fights were resolving too quickly for the
        // new leap/scatter/formation choreography to actually read on
        // screen before the fight was already over. `boss_health` (2026-
        // 08-16) is a single consolidated dial replacing what used to be
        // 4 separate multipliers stacked together - see its own doc.
        hp: (74.0 * tunables.boss_health * party_size * level_mult * stage_mult * jitter).round() as u64,
        // Halved alongside the doubled attack_interval_ms above (2026-08-16)
        // - keeps total damage output roughly the same, just spread across
        // twice as many, smaller hits. `boss_power` is `boss_health`'s own
        // consolidated counterpart for ATK.
        atk: (3.5 * tunables.boss_power * atk_level_mult * atk_stage_mult).round() as u64,
        attack_interval_ms: attack_interval_ms.max(50),
        damage_reduction: ((s * 0.005).min(BOSS_DEFENSE_CAP) * boss_jitter).clamp(0.0, BOSS_DEFENSE_CAP),
        block_chance: ((s * 0.010).min(BOSS_DEFENSE_CAP) * boss_jitter).clamp(0.0, BOSS_DEFENSE_CAP),
        evasion: ((s * 0.015).min(BOSS_DEFENSE_CAP) * boss_jitter).clamp(0.0, BOSS_DEFENSE_CAP),
        increased_damage: ((s * 0.01).min(0.5) * boss_jitter).max(0.0),
        crit_chance: ((0.05 + s * 0.012).min(CRIT_CHANCE_CAP) * boss_jitter).clamp(0.0, CRIT_CHANCE_CAP),
        crit_multiplier: 1.4 + ((s * 0.025).min(0.9) * boss_jitter).max(0.0),
        splash: ((s * 0.01).min(0.6) * boss_jitter).clamp(0.0, 1.0),
    };
    scale_by_power_mult(stats, power_mult)
}

/// Still meaningfully weaker than `boss_stats_for` overall, but bumped
/// to at least 2x its original hp/atk (0.5/0.6 of the boss's own -> 1.0/1.2)
/// per the request that basic encounters weren't pulling their weight
/// difficulty-wise. Halving HP alone used to roughly halve how many hits
/// it takes to kill; this reverses that AND adds a real damage-output
/// bump on top, not just a wash back to boss-equivalent.
pub(crate) fn basic_enemy_stats_for(stage: u32, party_size: usize, avg_level: f64, power_mult: f64, tunables: &LiveTunables) -> BossStats {
    let boss = boss_stats_for(stage, party_size, avg_level, power_mult, tunables);
    BossStats {
        hp: (boss.hp as f64 * 1.0).round() as u64,
        atk: (boss.atk as f64 * 1.2).round() as u64,
        attack_interval_ms: ((boss.attack_interval_ms as f64 * 1.3).round() as u32).max(50),
        // Deliberately dropping boss's rolled secondary stats here (not
        // spreading them in) - basic-encounter mobs don't get crit/
        // evasion/block/etc. at all, confirmed scope (see BossStats).
        ..Default::default()
    }
}

/// Splits one aggregate group-power budget (see `basic_enemy_stats_for`)
/// into `count` individually weaker enemies, each with their own HP bar
/// (see `simulate_battle`) instead of one shared pool. HP is divided
/// evenly, so the total damage needed to clear the whole group matches
/// the old single-unit aggregate exactly; each individual's attack
/// interval is multiplied by `count` (atk itself unchanged) so the
/// GROUP's total attack frequency - and therefore total incoming damage
/// over the fight - also stays the same as that aggregate, even though
/// there are now `count` separate attackers instead of one.
pub(crate) fn split_into_enemies(aggregate: BossStats, count: usize) -> Vec<BossStats> {
    let count = count.max(1);
    let per_hp = ((aggregate.hp as f64 / count as f64).round() as u64).max(1);
    let per_interval = ((aggregate.attack_interval_ms as u64) * count as u64).min(u32::MAX as u64) as u32;
    (0..count).map(|_| BossStats { hp: per_hp, atk: aggregate.atk, attack_interval_ms: per_interval, ..Default::default() }).collect()
}

