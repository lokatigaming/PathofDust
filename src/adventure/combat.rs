use super::*;

/// One archetype-granted combat skill - the extensible framework a live
/// request to "add lots of skills with different effects" asked for.
/// Deliberately a plain enum (same idiom as `Affix`/`BossKind`/
/// `CraftAction` elsewhere in this file), NOT a trait object - adding
/// skill #2 means one new variant plus whichever of the `on_*` hook
/// methods below it actually needs, never a new `CombatSimUnit` field or
/// a new line at every unit-build site the way each stat (Leech, etc.)
/// has needed so far. `Archetype::skills()` is the single source of
/// truth for which archetype grants which skill(s); `CombatSimUnit::skills`
/// is just that list copied in at fight-build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchetypeSkill {
    /// Berserker - a killing blow has a chance to grant this unit one
    /// immediate extra attack against a fresh random target on the
    /// opposing side. The pilot skill this framework was proven out
    /// with - on-kill was picked deliberately as the cheapest trigger
    /// type (no new event-queue clock needed, unlike a periodic/ramping
    /// skill would).
    Frenzy,
    /// Slayer - a periodic burst: every `FLICKER_STRIKE_COOLDOWN_MS`, this
    /// unit dashes to `FLICKER_STRIKE_HITS` random targets on the
    /// opposing side (not its normal single target) and strikes each one
    /// with +100 percentage points of splash on top of its own (pushing
    /// most Slayers into the splash-overflow bonus-target territory
    /// already established by `SPLASH_OVERFLOW_BONUS_TARGETS`). The
    /// "100% more attack speed" half of the request is realized as the
    /// hit count itself (roughly 2 normal attacks' worth of time,
    /// delivered in one instant burst at double the rate) rather than a
    /// separate temporary attack-interval buff, since a burst that
    /// resolves instantly has no real "speed" to modify beyond how many
    /// hits land - see `on_periodic_tick`, driven by
    /// `CombatSimUnit::next_flicker_at_ms` (see its own doc for why this
    /// needed a real periodic clock, unlike `Frenzy`'s reactive hook).
    /// Fires a `CombatEvent::SkillCast` so the OBS overlay knows when to
    /// show the effect gif.
    FlickerStrike,
}

/// Frenzy's proc chance - first-pass number, same "will need real
/// tuning" caveat as the rest of this file's balance constants.
pub(crate) const FRENZY_PROC_CHANCE: f64 = 0.25;
/// Flicker Strike's cooldown - flat for now (a live request floated
/// scaling this down by a future skill-point/passives system, but that
/// system doesn't exist as real game state yet, so this stays flat until
/// it does).
pub(crate) const FLICKER_STRIKE_COOLDOWN_MS: u32 = 5_000;
/// How many random targets one Flicker Strike burst hits - see
/// `ArchetypeSkill::FlickerStrike`'s doc for why this stands in for "100%
/// more attack speed" instead of a temporary buff.
pub(crate) const FLICKER_STRIKE_HITS: usize = 4;
/// Bonus splash (on top of the unit's own) during a Flicker Strike hit -
/// the request's "100% bonus splash", added as a one-off argument to
/// `apply_splash` per hit rather than mutating the unit's real `splash`
/// field, so it never lingers past the burst.
pub(crate) const FLICKER_STRIKE_BONUS_SPLASH: f64 = 1.0;

/// How long Slayer's Martyrdom shield (see `CombatSimUnit::shield_hp`)
/// lasts before it expires unused - the design text doesn't specify a
/// duration for this one (unlike Endless Thirst's explicit "5-second
/// shield"), so this is a first-pass "long enough to matter for the rest
/// of a typical fight" pick, same tuning-caveat spirit as every other
/// first-pass number in this file.
pub(crate) const BLOODPACT_SHIELD_DURATION_MS: u32 = 15_000;
/// Slayer's Bloodpact - base real cooldown between uses (see
/// `next_bloodpact_at_ms`'s doc), reduced per Blood Sacrifice rank.
pub(crate) const BLOODPACT_BASE_COOLDOWN_MS: u32 = 4_000;
/// Slayer's Warlord's Resolve - how long its party-wide increased-damage
/// grant lasts, per its own "for 10s" text.
pub(crate) const BLOODPACT_WARLORDSRESOLVE_DURATION_MS: u32 = 10_000;
/// Base duration Slayer's Open Wound lasts before it lazily expires (see
/// `CombatSimUnit::wound_expires_at_ms`) - the design text doesn't state
/// a base either, only that Festering Wound extends it by a %, so this is
/// the same kind of first-pass pick as `BLOODPACT_SHIELD_DURATION_MS`.
pub(crate) const WOUND_BASE_DURATION_MS: u32 = 6_000;

/// How long the temporary buffs Cleric's Guardian Spirit (Divine
/// Intervention/Final Blessing) grant after a save last - the design text
/// states this explicitly ("+damage reduction... for 5s", "+healing
/// power... for 5s"), unlike the two durations above.
pub(crate) const GUARDIAN_SPIRIT_SAVE_BUFF_DURATION_MS: u32 = 5_000;
/// Wild Instinct (Druid) - how long the damage-reduction buff a heal
/// grants its target lasts. Same 3s duration as this branch's other
/// short per-action buffs (Serenity, Silent Prowl).
pub(crate) const WILDINSTINCT_DR_DURATION_MS: u32 = 3_000;
/// Unyielding Roots' taunt window length within each
/// `unyieldingroots_cycle_ms` cycle - fixed at 2s regardless of rank (only
/// the cycle length itself scales with rank).
pub(crate) const UNYIELDINGROOTS_TAUNT_DURATION_MS: u32 = 2_000;
/// Wild Roar's fear duration - fixed 1s per the node's own text (only the
/// charge count scales with rank, not this).
pub(crate) const WILDROAR_FEAR_DURATION_MS: u32 = 1_000;
/// Berserker's Warlord's Resolve - how long the party-wide increased-damage
/// grant lasts once triggered (refreshed every hit landed while Bloodlust
/// is at max stacks, so effectively continuous while maintained).
pub(crate) const WARLORD_BUFF_DURATION_MS: u32 = 5_000;
/// Warlock's Doom - how long a Doom-tracked curse runs before it detonates
/// (see `curse_expires_at_ms`'s doc) - long enough to bank a few hits'
/// worth of damage first, short enough to matter within a normal fight.
pub(crate) const DOOM_CURSE_DURATION_MS: u32 = 6_000;
/// Warlock's Soul Stone - the flat outgoing-damage penalty stacked once
/// per use for the rest of the fight (see `soul_stone_uses_this_fight`'s
/// doc) - a live request: "reduces his hit damage by 33% for each use of
/// soulstone."
pub(crate) const SOUL_STONE_DMG_PENALTY_PER_USE: f64 = 0.33;
/// Druid's Entangle - how recently a second attacker must have also hit
/// this unit to count as "multiple enemies hit you" (see
/// `recent_attackers`'s doc for why this is a window, not a literal turn).
pub(crate) const ENTANGLE_WINDOW_MS: u32 = 3_000;
/// Rogue's Vanish - base duration of its evasion buff (extended by
/// Fadeaway, once that's wired in a future pass).
pub(crate) const VANISH_DURATION_MS: u32 = 3_000;
/// Warrior's Thornedhide - shared expiry window for its stacking debuff
/// (same "one shared window, refreshed on each new stack" simplification
/// `add_speed_stack`'s own doc already establishes).
pub(crate) const THORNEDHIDE_DURATION_MS: u32 = 4_000;
/// Warrior's Adrenaline Surge - duration of its attack-speed buff.
pub(crate) const ADRENALINE_SURGE_DURATION_MS: u32 = 3_000;
/// Berserker's Vengeful Blood - duration of the shield Vigor's heal also
/// grants.
pub(crate) const VENGEFUL_BLOOD_SHIELD_DURATION_MS: u32 = 8_000;
/// Monk's Rising Storm - duration of its increased-damage burst.
pub(crate) const RISING_STORM_DURATION_MS: u32 = 3_000;
/// Monk's Rising Tide/Harmonize - duration of their respective buffs.
pub(crate) const RISING_TIDE_DURATION_MS: u32 = 3_000;
pub(crate) const HARMONIZE_DR_DURATION_MS: u32 = 3_000;
/// Monk's Guardian Spirit (Temple Guardian) - cooldown between its
/// periodic self-heal ticks.
pub(crate) const TEMPLE_GUARDIAN_HEAL_INTERVAL_MS: u32 = 5_000;
/// Paladin's Eternal Vow - duration of the shield it grants.
pub(crate) const ETERNAL_VOW_SHIELD_DURATION_MS: u32 = 8_000;
/// Paladin's Purify - duration of the damage-dealt debuff it applies.
pub(crate) const PURIFY_DEBUFF_DURATION_MS: u32 = 3_000;
/// Paladin's Purging Flame - duration of its healing-received debuff.
pub(crate) const PURGING_FLAME_DEBUFF_DURATION_MS: u32 = 3_000;
/// Ranger's Armor Breaker/Scorched Earth - duration of their respective
/// splash debuffs.
pub(crate) const ARMORBREAKER_DEBUFF_DURATION_MS: u32 = 3_000;
pub(crate) const SCORCHED_EARTH_DEBUFF_DURATION_MS: u32 = 3_000;
/// Ranger's Vanishing Shot/Fleeting Shadow - duration of their respective
/// evade-triggered buffs.
pub(crate) const VANISHING_SHOT_DURATION_MS: u32 = 3_000;
pub(crate) const FLEETING_SHADOW_DURATION_MS: u32 = 3_000;
/// Mage's Volatile Magic - max nearby enemies its crit-splash can reach.
pub(crate) const VOLATILE_MAGIC_MAX_TARGETS: usize = 2;
/// Mage's Static Field - duration of its attack-speed debuff.
pub(crate) const STATIC_FIELD_DEBUFF_DURATION_MS: u32 = 3_000;
/// Warlock's Dreadful Death - duration of Doom's DR-shred debuff.
pub(crate) const DREADFUL_DEATH_DEBUFF_DURATION_MS: u32 = 3_000;
/// Warlock's Dark Ritual - duration of its post-kill damage buff.
pub(crate) const DARK_RITUAL_DURATION_MS: u32 = 5_000;
/// Warlock's Unbreakable Bond - duration of Dark Communion's DR buff.
pub(crate) const UNBREAKABLE_BOND_DR_DURATION_MS: u32 = 3_000;
/// Cleric's Compassion - duration of its DR buff.
pub(crate) const COMPASSION_DR_DURATION_MS: u32 = 3_000;
/// Slayer's Insatiable - how much each proc extends Endless Thirst's own
/// cap-bonus window by.
pub(crate) const INSATIABLE_EXTEND_MS: u32 = 2_000;
/// Slayer's Overflow Vessel - duration of its overcapped-leech shield.
pub(crate) const OVERFLOW_VESSEL_SHIELD_DURATION_MS: u32 = 5_000;
/// Rogue's Final Cut - duration of its attack-speed buff.
pub(crate) const FINAL_CUT_DURATION_MS: u32 = 3_000;
/// Rogue's Predator - duration of its damage-taken mark.
pub(crate) const PREDATOR_MARK_DURATION_MS: u32 = 4_000;
/// Base duration Cleric's Overflowing Grace shield lasts before it
/// decays - the design text doesn't state a base (only that Rift of Mercy
/// extends it by a flat amount per rank), same first-pass-pick spirit as
/// `BLOODPACT_SHIELD_DURATION_MS`/`WOUND_BASE_DURATION_MS` above.
pub(crate) const OVERFLOW_GRACE_SHIELD_BASE_DURATION_MS: u32 = 5_000;
/// Base duration Cleric's Divine Favor shield (granted on a Prayer of
/// Mending bounce) lasts - same first-pass-pick spirit, extended by
/// Warding Light's per-rank bonus.
pub(crate) const DIVINE_FAVOR_SHIELD_BASE_DURATION_MS: u32 = 5_000;
/// How long Healing Touch's temporary healing-power buff lasts on a
/// bounced ally - the design text states this explicitly ("+5% healing
/// power per rank for 3s").
pub(crate) const HEALING_TOUCH_DURATION_MS: u32 = 3_000;
/// How long Mage's Arcane Shield lasts before it decays - the design text
/// doesn't state a duration ("a crit grants you a shield worth..."), same
/// first-pass-pick spirit as `OVERFLOW_GRACE_SHIELD_BASE_DURATION_MS`.
pub(crate) const ARCANE_SHIELD_DURATION_MS: u32 = 5_000;
/// How long Eternal Hunger's shield (off a Soul Harvest kill-heal) lasts -
/// same first-pass-pick spirit, the design text only states the value
/// ("a small shield worth 25% of the heal per rank"), not a duration.
pub(crate) const ETERNAL_HUNGER_SHIELD_DURATION_MS: u32 = 5_000;
/// How long Warrior's Aegis shield (off a blocked hit) lasts - same first-
/// pass-pick spirit, the design text only states the value ("shields your
/// lowest-HP ally for 20% of the blocked damage"), not a duration; Bastion
/// (still deferred) would extend this per rank.
pub(crate) const AEGIS_SHIELD_DURATION_MS: u32 = 5_000;
/// Momentum's max stacks/per-stack duration - states explicitly in the
/// design text ("max 5 stacks"/"for 4s").
pub(crate) const MOMENTUM_STACK_MAX: u32 = 5;
pub(crate) const MOMENTUM_STACK_DURATION_MS: u32 = 4_000;
/// Fleetfoot's max stacks/per-stack duration - states explicitly in the
/// design text ("max 3 stacks"/"for 3s").
pub(crate) const FLEETFOOT_STACK_MAX: u32 = 3;
pub(crate) const FLEETFOOT_STACK_DURATION_MS: u32 = 3_000;
/// Bloodlust's max stacks/per-stack duration - states explicitly in the
/// design text ("max 5 stacks"/"for 5s").
pub(crate) const BLOODLUST_STACK_MAX: u32 = 5;
pub(crate) const BLOODLUST_STACK_DURATION_MS: u32 = 5_000;
/// Flowing Strikes' max stacks/per-stack duration - states explicitly in
/// the design text ("max 5 stacks"/"for 4s"); Hundred Fists/Relentless
/// Assault extend these further per rank at construction.
pub(crate) const FLOWING_STACK_BASE_MAX: u32 = 5;
pub(crate) const FLOWING_STACK_DURATION_MS: u32 = 4_000;
/// Relentless Pursuit's (Ranger) and Flow State's (Mage) max stacks/
/// per-stack duration - both share IDENTICAL design text to Momentum's
/// own ("max 5 stacks"/"for 3s"), reusing the same `stack_speed_*`
/// bundle Warrior/Rogue/Berserker already populate (safe - only one
/// archetype's magnitude is ever nonzero per unit).
pub(crate) const RELENTLESS_PURSUIT_STACK_MAX: u32 = 5;
pub(crate) const RELENTLESS_PURSUIT_STACK_DURATION_MS: u32 = 3_000;
pub(crate) const FLOWSTATE_STACK_MAX: u32 = 5;
pub(crate) const FLOWSTATE_STACK_DURATION_MS: u32 = 3_000;
/// Fel Rush's buff duration - "A kill grants +8%/rank attack speed for
/// 4s", explicit in the design text.
pub(crate) const FEL_RUSH_DURATION_MS: u32 = 4_000;
/// Blood Frenzy's/Endless Thirst's buff duration - "Each FlickerStrike
/// dash grants... for 4s", explicit in both nodes' design text.
pub(crate) const FLICKER_FRENZY_DURATION_MS: u32 = 4_000;
/// Elemental damage rework (2026-08-15) - every Cold/Fire/Chaos/
/// Lightning/Divine proc's own duration, explicit in the design text
/// ("...for 4s") for all 5 types alike.
pub(crate) const ELEMENTAL_PROC_DURATION_MS: u32 = 4_000;
/// A Fire/Cold/Chaos proc-debuff can never push the affected stat below
/// this, no matter how many independent stacks are active - explicit in
/// the design text ("defenses can never be reduced below 25% for
/// enemies").
pub(crate) const ELEMENTAL_DEFENSE_FLOOR: f64 = 0.25;
/// A Fire/Cold/Chaos proc-buff (the healer-targets-an-ally variant) can
/// never push the affected stat above this - explicit in the design text
/// ("capped at the normal caps"), matching the same 75% ceiling
/// DR/Block/Evasion already respect everywhere else.
pub(crate) const ELEMENTAL_DEFENSE_CEILING: f64 = 0.75;
/// Hard ceiling on any single `OverflowConversion` passive node's OWN
/// contribution, per invested rank - see `Character::passive_overflow_bonus`'s
/// doc for why this exists (2026-08-16: "for something like overflow
/// where its scaling based on an outside source, we just put a ceiling
/// ... capped for each point individually at 10% more contribution").
/// A 3-rank node maxes at 30% no matter how much overflow is actually
/// available to convert, matching the ~10%-per-point budget the rest of
/// the tree targets.
pub(crate) const OVERFLOW_CONVERSION_CAP_PER_RANK: f64 = 0.10;
/// Lightning's damage-taken stack cap - explicit in the design text
/// ("lightning damage debuff can stack up to 200% increased damage
/// taken"), 1% per stack.
pub(crate) const ELEMENTAL_LIGHTNING_MAX_STACKS: usize = 200;
/// Divine's enemy-side healing-received-reduction stack cap - NOT
/// explicit in the design text (only Lightning/Divine's own self-buff
/// got an explicit number), but healing received going negative makes
/// no sense, so 100 stacks/100% (fully negated) is the natural technical
/// ceiling, same "sane default, flagged rather than silently invented"
/// reasoning as every other unstated-but-necessary cap in this codebase.
pub(crate) const ELEMENTAL_DIVINE_ENEMY_MAX_STACKS: usize = 100;
/// Twin Strikes'/Spell Echo's follow-up strike damage share - "strike
/// again at 50% damage", explicit in the design text.
pub(crate) const TWIN_STRIKE_BASE_DMG_PCT: f64 = 0.50;
/// Monk's Serenity - "grants +DR per rank for 3s", explicit in the
/// design text.
pub(crate) const SERENITY_DR_DURATION_MS: u32 = 3_000;
/// Mage's Frost Nova - no explicit duration in the design text, a first-
/// pass pick same spirit as every other timed effect this tree adds
/// without one, matching Serenity's own 3s (both are "evade/hit-driven
/// timed debuff/buff" nodes at the same tier).
pub(crate) const FROSTNOVA_DEBUFF_DURATION_MS: u32 = 3_000;
/// Lingering Effect (the 2026-08-15 Healing Power rework) - originally
/// "damage over 4 seconds" ticking once per second (4 ticks = 4000ms,
/// per the design text's own worked example: 100 dmg x 4% = 4 total,
/// 1/sec). Cadence dropped to 50ms (2026-08-16, a live request, explicitly
/// acknowledged as "a huge increase in its participation") - but the tick
/// COUNT was left at 4 instead of scaling up with it, an unintended side
/// effect (not a deliberate design call) that silently collapsed total
/// duration from the intended 4000ms down to just 200ms. Fixed
/// (2026-08-17): tick count raised to 80 (`4000ms / 50ms`), restoring the
/// original 4-second total duration at the current fast cadence. Since
/// `amount_per_tick` (= total / `LINGERING_EFFECT_TICKS`) already uses
/// this constant as its divisor, this single change keeps the total
/// amount delivered identical - now spread over 80 small ticks across a
/// real 4s instead of 4 big ticks across 200ms.
pub(crate) const LINGERING_EFFECT_TICK_INTERVAL_MS: u32 = 50;
pub(crate) const LINGERING_EFFECT_TICKS: u32 = 80;
/// Seed of Life's per-tick shield duration - long enough (vs.
/// `LINGERING_EFFECT_TICK_INTERVAL_MS`'s 50ms cadence) that consecutive
/// ticks from the same still-active Lingering Effect instance land before
/// the previous grant expires, so `grant_shield`'s own "still active ->
/// add, else replace" rule makes them genuinely stack across the DoT's
/// whole (now 80-tick) lifetime instead of each one replacing the last.
pub(crate) const SEEDOFLIFE_SHIELD_DURATION_MS: u32 = 5_000;
/// Cthulhu's Bubble rework (2026-08-16) - base % less damage AND healing
/// dealt per stack, before scaling by his own dynamic boss power (see
/// `CombatSimUnit::boss_dynamic_power_mult`'s doc). The actual applied
/// reduction is additionally floored at 90% total (see `resolve_hit`/
/// `apply_heal`'s `.max(0.1)`/`.min(0.9)`), regardless of how high stacks
/// or dynamic power push the raw per-stack rate.
pub(crate) const CTHULHU_DEBUFF_BASE_PCT_PER_STACK: f64 = 0.05;
/// How long a single Bubble stack lasts - shorter than
/// `CTHULHU_DEBUFF_CADENCE_MS`, so a single Cthulhu's own casts never
/// overlap on the same player; only a second Cthulhu's independent pick
/// can make it genuinely stack.
pub(crate) const CTHULHU_DEBUFF_DURATION_MS: u32 = 2_000;
/// How often Cthulhu recasts Bubble on a fresh, independently-rolled half
/// of the party.
pub(crate) const CTHULHU_DEBUFF_CADENCE_MS: u32 = 3_000;
/// Hard floor on Cthulhu's Bubble - the combined per-target damage/
/// healing reduction can never exceed this, regardless of stacks or
/// `boss_dynamic_power_mult` (see `CTHULHU_DEBUFF_BASE_PCT_PER_STACK`'s
/// doc). Named 2026-08-18 so the wiki's constant audit can wire it - was
/// a bare `0.9` at both `resolve_hit`'s and `apply_heal`'s Cthulhu reads.
/// Thornedhide/Soul Stone/Bloodpact Triage each independently cap their
/// own unrelated debuff at the same 0.9 value by their own convention -
/// this constant only feeds Cthulhu's own two read sites, not those.
pub(crate) const CTHULHU_DEBUFF_CAP: f64 = 0.9;
/// Dragon's aura - how much slower (as an `attack_interval_ms`
/// multiplier) every non-boss unit attacks for the whole fight. Named
/// 2026-08-18 for the wiki's constant audit - see the aura's own
/// application site in `simulate_battle`.
pub(crate) const DRAGON_SLOW_MULT: f64 = 1.5;
/// Fire Demon's aura - multiplier applied to every heal amount,
/// fight-wide (0.5 = -50%). Named 2026-08-18 for the wiki's constant
/// audit, same as `DRAGON_SLOW_MULT` above.
pub(crate) const FIRE_DEMON_HEAL_MULT: f64 = 0.5;
/// How often the Lich casts Raise Dead (see the `BossAbility::Lich`
/// handling in `simulate_battle`) - both the initial per-fight seed and
/// the recast interval after each cast. Named 2026-08-18 for the wiki's
/// constant audit.
pub(crate) const LICH_SUMMON_CADENCE_MS: u32 = 2_000;
/// How many adds one Raise Dead cast summons, before `LICH_MAX_ADDS`
/// clamps the total. Named 2026-08-18 for the wiki's constant audit.
pub(crate) const LICH_ADDS_PER_SUMMON: u32 = 5;
/// How many adds the Lich can summon in total across a fight, however
/// long it runs - without this, "5 more every 2 seconds" could spiral
/// into hundreds of units on an extended fight. Hoisted out of
/// `simulate_battle`'s own function body to module level (2026-08-18) so
/// the wiki's constant audit can see and wire it - it was already a
/// `const` there, just function-local and therefore invisible outside
/// `simulate_battle`; behavior is unchanged.
pub(crate) const LICH_MAX_ADDS: u32 = 20;
/// Gelatinous Cube's capture ability - how often it rotates a fresh batch
/// of players into its body (see the `BossAbility::GelatinousCube` arm's
/// own doc). Also the exact window each captured player is locked out for
/// - the two share one constant since the capture is a one-time forward
/// push on `next_action_at_ms`, not a standing flag with its own expiry.
/// Paladin's Zealous Charge (Guardian's Wrath) - duration of its temporary
/// self attack-speed buff.
pub(crate) const ZEALOUS_CHARGE_DURATION_MS: u32 = 3_000;
/// Paladin's Unwavering / Cleric's Unyielding Faith - refresh window for
/// their doubled-party-DR broadcast, same spirit/magnitude as
/// `WARLORD_BUFF_DURATION_MS` but its own named constant per this
/// codebase's "each buff gets its own duration constant" convention.
pub(crate) const UNWAVERING_BUFF_DURATION_MS: u32 = 5_000;
/// Cleric's Eternal Light - refresh window per heal landed. 3s, matching
/// the original text's own "persists for 3s" number even though the
/// mechanic itself was rewritten (see `eternallight_bonus_pct`'s doc).
pub(crate) const ETERNAL_LIGHT_DURATION_MS: u32 = 3_000;
pub(crate) const CUBE_CAPTURE_CADENCE_MS: u32 = 3_000;
/// Fraction of currently-alive players captured per cycle - clamped to at
/// least 1 by the call site, never a flat count (a live request: scale
/// with party size instead of a fixed 3).
pub(crate) const CUBE_CAPTURE_PCT: f64 = 0.10;
/// Gelatinous Cube's defense shred - % reduced effective mitigation per
/// stack, refreshed on every landed hit (primary or splash) against a
/// player.
pub(crate) const CUBE_SHRED_PCT_PER_STACK: f64 = 0.10;
/// Caps stacking at exactly 50% total reduced defenses (5 * 10%) per the
/// request's explicit clamp.
pub(crate) const CUBE_SHRED_MAX_STACKS: u32 = 5;
/// How long a single shred stack lasts before lazy-expiring back to 0
/// (Thornedhide-style) - matches the request's "for 3 seconds."
pub(crate) const CUBE_SHRED_DURATION_MS: u32 = 3_000;
/// Paladin's Divine Shield - base cast interval before any cooldown
/// reduction (Divine Shield's own rank, Grace Period) is applied. States
/// explicitly in the design text ("Every 8s").
pub(crate) const DIVINE_SHIELD_BASE_COOLDOWN_MS: u32 = 8_000;
/// Divine Shield's base shield amount, as a fraction of the caster's own
/// max HP - states explicitly in the design text ("a flat 10% of your
/// max HP"), amplified further by Bulwark of Light per rank.
pub(crate) const DIVINE_SHIELD_BASE_AMOUNT_PCT: f64 = 0.10;
/// How long a Divine Shield (primary or Consecration's party shield)
/// lasts before it decays - the design text doesn't state a duration,
/// same first-pass-pick spirit as `OVERFLOW_GRACE_SHIELD_BASE_DURATION_MS`.
pub(crate) const DIVINE_SHIELD_DURATION_MS: u32 = 5_000;
/// How long Poison Thorns' damage-dealt debuff lasts - states explicitly
/// in the design text ("reduces the attacker's damage dealt by... for 3s").
pub(crate) const POISON_THORNS_DEBUFF_DURATION_MS: u32 = 3_000;
/// Berserker's Frenzy - flat trigger chance per attack once invested at
/// all, states explicitly in the design text ("at a rate of 10%") and
/// does NOT scale with Frenzy's own rank (rank instead scales how many
/// times it strikes - see `frenzy_extra_hits`'s doc).
pub(crate) const FRENZY_BASE_STRIKE_CHANCE: f64 = 0.10;
/// Second Wind's shield value - a flat fraction of the Bloodletting heal
/// it rides on, NOT rank-scaled (Second Wind's own rank instead scales
/// the CHANCE the shield happens at all - see `frenzy_shield_chance`'s
/// doc).
pub(crate) const FRENZY_SHIELD_VALUE_PCT: f64 = 0.20;
/// How long Second Wind's shield lasts - the design text doesn't state a
/// duration, same first-pass-pick spirit as every other undocumented
/// shield duration in this file.
pub(crate) const FRENZY_SHIELD_DURATION_MS: u32 = 5_000;

impl ArchetypeSkill {
    pub fn name(self) -> &'static str {
        match self {
            ArchetypeSkill::Frenzy => "Frenzy",
            ArchetypeSkill::FlickerStrike => "Flicker Strike",
        }
    }

    /// Player-facing blurb - what `Archetype::description` appends for
    /// any archetype with skills, and what the dashboard can show
    /// standalone later.
    pub fn description(self) -> &'static str {
        match self {
            ArchetypeSkill::Frenzy => "killing blows have a chance to grant an immediate extra attack",
            ArchetypeSkill::FlickerStrike => "every 5s, dashes to 4 random enemies with +100% splash on each hit",
        }
    }

    /// Fires once per kill THIS unit lands - see `apply_hit`'s Defeat
    /// branch, the only call site. Every OTHER hook (`on_hit_dealt`/
    /// `on_hit_taken`/`on_evade`/`on_fight_start`) is deliberately NOT
    /// stubbed out here yet - per the "build the skeleton, then add hooks
    /// as a skill actually needs one" plan, they get added the moment the
    /// first skill that needs them lands, not speculatively ahead of
    /// that. `on_periodic_tick` (below) is the one hook type that DID
    /// need adding, for `FlickerStrike`.
    pub(crate) fn on_kill(self, units: &mut [CombatSimUnit], attacker_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
        match self {
            ArchetypeSkill::Frenzy => {
                if !rng.gen_bool(FRENZY_PROC_CHANCE) {
                    return;
                }
                let attacker_is_boss = units[attacker_idx].is_boss;
                let candidates: Vec<usize> =
                    units.iter().enumerate().filter(|&(i, u)| u.alive && u.is_boss != attacker_is_boss && i != attacker_idx).map(|(i, _)| i).collect();
                if candidates.is_empty() {
                    return;
                }
                let target_idx = candidates[rng.gen_range(0..candidates.len())];
                // Same base-damage roll a normal attack action uses (see
                // `attacker_base_damage`'s doc) - this bonus hit is a
                // real extra attack, not a scaled-down echo of the
                // killing blow.
                let base_damage = attacker_base_damage(&units[attacker_idx], rng);
                apply_hit(units, attacker_idx, target_idx, base_damage, at_ms, events, rolls, rng, true, false);
            }
            ArchetypeSkill::FlickerStrike => {}
        }
    }

    /// Fires on this unit's own periodic clock - see
    /// `CombatSimUnit::next_flicker_at_ms`, the main loop's
    /// `NextEvent::FlickerStrike` handling (the only call site, which
    /// also advances the clock itself before calling this - this method
    /// only performs the burst, it doesn't reschedule anything). Pushes a
    /// `CombatEvent::SkillCast` regardless of whether any hit actually
    /// lands, so the overlay's dash effect plays even against a
    /// nearly-empty enemy side.
    pub(crate) fn on_periodic_tick(self, units: &mut [CombatSimUnit], actor_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
        match self {
            ArchetypeSkill::Frenzy => {}
            ArchetypeSkill::FlickerStrike => {
                events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: self.name().to_string() });
                // Blood Frenzy / Endless Thirst - refreshed the moment
                // this dash actually starts (the node text's trigger is
                // "each FlickerStrike dash", not each individual hit
                // within it), each its own independent timer.
                if units[actor_idx].flicker_frenzy_speed_bonus > 0.0 {
                    // Unrelenting - approximated as extending the shared
                    // duration rather than true "decays slower" - at 3/3
                    // ("stops decaying entirely") this duration is large
                    // enough to functionally never lapse within a normal
                    // fight.
                    let unrelenting_bonus_ms = units[actor_idx].unrelenting_duration_bonus_ms;
                    units[actor_idx].flicker_frenzy_expires_at_ms = at_ms + FLICKER_FRENZY_DURATION_MS + unrelenting_bonus_ms;
                }
                if units[actor_idx].endless_thirst_cap_bonus > 0.0 || units[actor_idx].endless_thirst_uncapped {
                    units[actor_idx].endless_thirst_expires_at_ms = at_ms + FLICKER_FRENZY_DURATION_MS;
                }
                // War Cry - the same dash also grants the whole party a
                // temporary attack-speed buff (see
                // `temp_party_attack_speed_bonus`'s doc).
                let warcry_pct = units[actor_idx].warcry_party_speed_pct;
                if warcry_pct > 0.0 {
                    for u in units.iter_mut() {
                        if !u.is_boss && u.alive {
                            u.temp_party_attack_speed_bonus = warcry_pct;
                            u.temp_party_attack_speed_bonus_expires_at_ms = at_ms + FLICKER_FRENZY_DURATION_MS;
                        }
                    }
                }
                let attacker_is_boss = units[actor_idx].is_boss;
                let mut candidates: Vec<usize> =
                    units.iter().enumerate().filter(|&(i, u)| u.alive && u.is_boss != attacker_is_boss && i != actor_idx).map(|(i, _)| i).collect();
                // Reaper's Momentum - bonus targets banked from a PRIOR
                // dash's kills get spent on THIS dash, then reset - "your
                // NEXT dash" per the node's own text, not a permanent
                // increase to FLICKER_STRIKE_HITS.
                let hit_count = FLICKER_STRIKE_HITS + units[actor_idx].reapers_momentum_banked as usize;
                units[actor_idx].reapers_momentum_banked = 0;
                // Adrenaline - a one-off crit-mult override for just this
                // dash's own direct hits (same scoped-override convention
                // Piercing Shots/Sanctified Touch already use).
                let adrenaline_bonus = units[actor_idx].adrenaline_crit_mult_bonus;
                let original_crit_multiplier = units[actor_idx].crit_multiplier;
                if adrenaline_bonus > 0.0 {
                    units[actor_idx].crit_multiplier += adrenaline_bonus;
                }
                // Reaper's Momentum bank starts this dash - Chain
                // Reaper/Death Spiral read it below to know which of
                // THIS dash's targets are "bonus" ones (banked from a
                // prior dash's kills) versus the base hit count.
                let bonus_targets_this_dash = units[actor_idx].reapers_momentum_banked;
                let mut hits_so_far = 0u32;
                for _ in 0..hit_count {
                    if candidates.is_empty() {
                        break;
                    }
                    let pick_at = rng.gen_range(0..candidates.len());
                    let target_idx = candidates[pick_at];
                    let base_damage = attacker_base_damage(&units[actor_idx], rng);
                    apply_hit(units, actor_idx, target_idx, base_damage, at_ms, events, rolls, rng, true, false);
                    // Chain Reaper - each of THIS dash's bonus targets
                    // (beyond the base FLICKER_STRIKE_HITS) heals the
                    // Slayer on hit, landed or not.
                    hits_so_far += 1;
                    if hits_so_far > FLICKER_STRIKE_HITS as u32 && hits_so_far <= FLICKER_STRIKE_HITS as u32 + bonus_targets_this_dash {
                        let chainreaper_pct = units[actor_idx].chainreaper_heal_pct;
                        if chainreaper_pct > 0.0 && units[actor_idx].alive {
                            let heal_amount = units[actor_idx].max_hp as f64 * chainreaper_pct;
                            apply_heal(units, actor_idx, actor_idx, heal_amount, at_ms, events, rng);
                        }
                        // Death Spiral - a KILL from one of those bonus
                        // targets heals a flat amount on top.
                        if !units[target_idx].alive {
                            let deathspiral_pct = units[actor_idx].deathspiral_heal_pct;
                            if deathspiral_pct > 0.0 && units[actor_idx].alive {
                                let heal_amount = units[actor_idx].max_hp as f64 * deathspiral_pct;
                                apply_heal(units, actor_idx, actor_idx, heal_amount, at_ms, events, rng);
                            }
                        }
                    }
                    // Reaper's Momentum - a kill from FlickerStrike's own
                    // direct hit (checked here, not via the generic
                    // `fire_on_kill` dispatch every other on-kill effect
                    // uses, since this is gated specifically to
                    // FlickerStrike) banks bonus targets for the NEXT
                    // dash. Splash kills from this same hit don't count -
                    // only the dash's own direct strike does.
                    if units[actor_idx].reapers_momentum_per_kill > 0 && !units[target_idx].alive {
                        units[actor_idx].reapers_momentum_banked += units[actor_idx].reapers_momentum_per_kill;
                    }
                    let boosted_splash = units[actor_idx].splash + FLICKER_STRIKE_BONUS_SPLASH;
                    apply_splash(units, actor_idx, target_idx, base_damage, boosted_splash, PLAYER_SPLASH_MAX_TARGETS, None, at_ms, events, rolls, rng);
                    // Insatiable - a chance for this hit to extend Endless
                    // Thirst's leech-cap bonus by 2s.
                    let insatiable_chance = units[actor_idx].insatiable_extend_chance;
                    if insatiable_chance > 0.0 && (units[actor_idx].endless_thirst_cap_bonus > 0.0 || units[actor_idx].endless_thirst_uncapped) && rng.gen_bool(insatiable_chance.clamp(0.0, 1.0)) {
                        units[actor_idx].endless_thirst_expires_at_ms += INSATIABLE_EXTEND_MS;
                    }
                    // Second Heartbeat - a chance for this hit to trigger
                    // an immediate bonus dash strike at one more random
                    // enemy (a genuinely extra hit, on top of the normal
                    // hit_count loop).
                    let secondheartbeat_chance = units[actor_idx].secondheartbeat_chance;
                    if secondheartbeat_chance > 0.0 && !candidates.is_empty() && units[actor_idx].alive && rng.gen_bool(secondheartbeat_chance.clamp(0.0, 1.0)) {
                        let bonus_pick = rng.gen_range(0..candidates.len());
                        let bonus_target_idx = candidates[bonus_pick];
                        let bonus_damage = attacker_base_damage(&units[actor_idx], rng);
                        apply_hit(units, actor_idx, bonus_target_idx, bonus_damage, at_ms, events, rolls, rng, true, false);
                    }
                    // Re-filter instead of removing the pick outright -
                    // Flicker Strike's random targets CAN repeat (it's a
                    // frenzied dash, not a guaranteed spread), the only
                    // thing that actually needs pruning is anyone that
                    // hit (or a splash from it) just killed.
                    candidates.retain(|&i| units[i].alive);
                }
                if adrenaline_bonus > 0.0 {
                    units[actor_idx].crit_multiplier = original_crit_multiplier;
                }
            }
        }
    }
}

/// Dispatches every skill `units[attacker_idx]` has to `on_kill` - the
/// generic loop `apply_hit`'s Defeat branch calls, so a new on-kill
/// skill never needs its own call site there. Skills list is cloned
/// first (small, `Copy` elements) so each skill's own logic is free to
/// take `&mut [CombatSimUnit]` without fighting the borrow checker over
/// `units[attacker_idx].skills` still being borrowed.
pub(crate) fn fire_on_kill(units: &mut [CombatSimUnit], attacker_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
    let skills = units[attacker_idx].skills.clone();
    for skill in skills {
        skill.on_kill(units, attacker_idx, at_ms, events, rolls, rng);
    }
}

/// Warlock's Doom - triggers the SAME detonation `NextEvent::CurseExpiry`
/// fires, but immediately, the moment a Doom-tracked target dies from
/// something else first (2026-08-16, a live design call - previously a
/// target dying before the curse's own timer naturally elapsed just
/// wasted the banked damage, since the scheduled `CurseExpiry` event
/// found the target already dead and skipped its whole block via the
/// `if units[target_idx].alive` guard there). Called alongside
/// `fire_on_kill` at every death site, victim-side (unlike `fire_on_kill`,
/// which is attacker-side). The victim is already dead/at 0 hp by the
/// time this runs, so there's no damage left to apply to THEM or a second
/// Defeat/`fire_on_kill` for their own death - only Apocalypse's splash
/// to OTHER enemies (which can still meaningfully kill something) still
/// matters; Dreadful Death's DR shred is skipped entirely (pointless on a
/// corpse). Clears the curse state the same way the timer-based path
/// does, so the originally-scheduled `CurseExpiry` event (still queued,
/// not cancelable) finds nothing to do when it eventually fires - already
/// dead either way, so its own `if alive` guard already prevented a
/// double-detonation regardless, this is just hygiene.
pub(crate) fn trigger_doom_on_death(units: &mut [CombatSimUnit], victim_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
    if units[victim_idx].curse_expires_at_ms == u32::MAX || units[victim_idx].curse_detonate_pct <= 0.0 {
        return;
    }
    let detonation = units[victim_idx].curse_damage_taken_total * units[victim_idx].curse_detonate_pct;
    if detonation > 0.0 {
        let source_id = units[victim_idx].curse_source_id.clone().unwrap_or_default();
        events.push(CombatEvent::SkillCast { at_ms, unit: source_id.clone(), skill: "Doom".to_string() });
        let source_idx = units.iter().position(|u| u.id == source_id);
        let apocalypse_pct = source_idx.map(|i| units[i].own_apocalypse_splash_pct).unwrap_or(0.0);
        if apocalypse_pct > 0.0 {
            let target_is_boss = units[victim_idx].is_boss;
            let splash_damage = detonation * apocalypse_pct;
            let others: Vec<usize> =
                units.iter().enumerate().filter(|(i, u)| *i != victim_idx && u.is_boss == target_is_boss && u.alive).map(|(i, _)| i).collect();
            for other_idx in others {
                // Monk's Chakra of Life - true damage still respects full immunity.
                if splash_damage <= 0.0 || at_ms <= units[other_idx].chakraoflife_immune_until_ms {
                    continue;
                }
                let hit_id = next_hit_id();
                let penalized = apply_late_stage_penalty(units, other_idx, splash_damage, at_ms, hit_id, &source_id, rolls);
                let other_final = penalized.round().max(0.0) as i64;
                if other_final <= 0 {
                    continue;
                }
                let other_new_hp = (units[other_idx].hp - other_final).max(0);
                units[other_idx].hp = other_new_hp;
                let other_id = units[other_idx].id.clone();
                events.push(CombatEvent::Attack {
                    at_ms,
                    attacker: source_id.clone(),
                    target: other_id.clone(),
                    damage: other_final.max(0) as u64,
                    unmitigated_damage: other_final.max(0) as u64,
                    target_hp_after: other_new_hp as u64,
                    is_crit: false,
                    evaded: false,
                    hit_id,
                });
                if other_new_hp == 0 {
                    units[other_idx].alive = false;
                    events.push(CombatEvent::Defeat { at_ms, unit: other_id });
                    if let Some(source_idx) = source_idx {
                        fire_on_kill(units, source_idx, at_ms, events, rolls, rng);
                    }
                }
            }
        }
    }
    units[victim_idx].curse_expires_at_ms = u32::MAX;
    units[victim_idx].next_curse_expiry_at_ms = u32::MAX;
    units[victim_idx].curse_dmg_taken_bonus = 0.0;
    units[victim_idx].curse_damage_taken_total = 0.0;
}

/// A living combatant during `simulate_battle` — built fresh from
/// `Character`'s derived combat stats for players, or `BossStats` for the
/// one boss unit. Nothing here persists between fights.
pub(crate) struct CombatSimUnit {
    id: String,
    display_name: String,
    is_boss: bool,
    /// `Some` for a real player, `None` for a boss/enemy/mid-fight add -
    /// carried through to `CombatUnitInfo`/`PlayerFightStats` (2026-08-18)
    /// so per-class performance questions are a one-line query against
    /// persisted fight records instead of a cross-reference against
    /// current character state (which drifts - a player who's since
    /// respecced doesn't retroactively change what class they played in
    /// an old fight).
    archetype: Option<Archetype>,
    /// When this unit entered the fight - `0` for anyone present at the
    /// start (every player, the main boss(es)), the real current `at_ms`
    /// for a mid-fight add (e.g. Lich's Raise Dead summons) so its OWN
    /// `boss_defense_ignore` growth starts from its own spawn, not the
    /// main boss's already-elapsed time.
    spawned_at_ms: u32,
    role: Option<CombatFunction>,
    hp: i64,
    max_hp: u64,
    atk: u64,
    /// Share of every attack action that converts to healing instead of
    /// damage (see `Character::combat_heal_power`) - 0.0 for bosses/
    /// enemies (always pure damage) and for a player with no healing
    /// investment. There's no separate heal action anymore: every
    /// attack rolls one damage instance (see `simulate_battle`'s
    /// unified action) and splits it between an enemy (scaled by
    /// `1.0 - heal_power`, floored at 0) and the neediest hurt ally
    /// (scaled by `heal_power` itself, uncapped) - both can happen the
    /// same turn, and at 100%+ the damage share floors out entirely.
    heal_power: f64,
    /// This unit's own contribution to the PARTY'S pooled Intervene
    /// (see `Character::combat_intervene`) - 0.0 for bosses/enemies and
    /// for a player with no Intervene investment. Checked only on the
    /// boss's attack (see `simulate_battle`'s is_boss branch): the
    /// party's RAW summed Intervene (can run past 100%) decides how
    /// much of the hit is redirected at all, capped at 50%, and that
    /// capped pool splits across every party member with Intervene
    /// proportional to their own raw share of the sum - each getting
    /// their own independent hit, rolled against their own defenses.
    intervene: f64,
    attack_interval_ms: u32,
    next_action_at_ms: u32,
    alive: bool,
    /// Helm's stacking dps buff: (dps-per-stack, stack-interval). Ticks
    /// on its own clock (`next_helm_at_ms`), independent of the unit's
    /// normal attack cadence - see the main loop below. `u32::MAX` on
    /// next_helm_at_ms (no helm equipped) means it can never be the
    /// soonest event, so it's effectively disabled without an Option
    /// match at every comparison site.
    helm_power: f64,
    helm_cooldown_ms: u32,
    next_helm_at_ms: u32,
    /// Accumulated helm stacks so far, in dps units - starts at 0.0,
    /// permanently gains `helm_power` every `helm_cooldown_ms` of the
    /// fight (see the main loop's `NextEvent::Helm` handling). Converted
    /// to a flat per-hit bonus (via the unit's own `attack_interval_ms`)
    /// at the moment of an actual attack, so it only ever pays off when
    /// the wearer is the one dealing damage - never a healer's heal.
    helm_stack_bonus: f64,
    /// Same idea for the boots' self-heal skill.
    boots_power: f64,
    boots_cooldown_ms: u32,
    next_boots_at_ms: u32,
    /// Secondary combat stats - see `Affix`/`resolve_hit`. Players get
    /// these from `Character::combat_*`; a real boss gets its own
    /// (`BossStats`' matching fields); a basic-encounter mob's are all
    /// zero (see `basic_enemy_stats_for`).
    damage_reduction: f64,
    block_chance: f64,
    evasion: f64,
    increased_damage: f64,
    crit_chance: f64,
    crit_multiplier: f64,
    splash: f64,
    /// A hard, unbypassable cap on damage DEALT TO this unit (2026-08-17, a
    /// live request following a real HP-overflow incident - see
    /// `BossStats.hp`'s doc) - 0.0 for everyone except a REAL boss (never
    /// players, never a basic-encounter mob), set once at construction from
    /// `simulate_battle`'s `stage` param: `stage / (stage + 2000)`, a
    /// hyperbolic decay reaching 50% at stage 2000 and climbing toward, but
    /// never reaching, 100%. Applied in `resolve_hit` upstream of
    /// `combine_reduction_sources`'s whole mitigation pipeline, so nothing
    /// (evasion-ignore, DR-shred, any of it) can bypass it - a permanent
    /// brake on player damage output that scales with how far past the
    /// originally-tuned stage range a fight has gone, independent of
    /// whatever integer width `BossStats.hp`/`atk` happen to be.
    late_stage_damage_penalty_pct: f64,
    /// A REAL boss fight (not a basic-encounter filler mob) always
    /// targets whichever alive player currently has the highest
    /// `survivability` instead of picking randomly - see
    /// `simulate_battle`'s boss-attack branch. Every consecutive hit on
    /// the SAME focused target adds another +10% here (starts at 10% on
    /// the first hit, not 0%), a straight damage-taken multiplier applied
    /// in `resolve_hit` - punishes turtling behind one tank instead of
    /// spreading survivability across the party. Resets to 0 the moment
    /// the boss switches to a new focus target (the previous one died).
    /// Always 0.0 and never touched for a basic-encounter mob or for the
    /// boss's own unit.
    boss_focus_stacks: f64,
    /// Which boss this is (see `BossKind`) - `Some` only for the one real
    /// boss unit in a real boss fight, `None` for a basic-encounter mob
    /// and for every player unit. Drives `simulate_battle`'s periodic
    /// `NextEvent::BossAbility` trigger (`next_ability_at_ms` below) as
    /// well as the survivability-focus targeting above.
    boss_ability: Option<BossKind>,
    /// When this boss's unique periodic ability next fires - `u32::MAX`
    /// (never) for Fire Demon/Dragon, which are passive fight-wide auras
    /// instead (applied once at fight setup - see `simulate_battle`),
    /// and for anything without a `boss_ability` at all.
    next_ability_at_ms: u32,
    /// Cthulhu's dynamic boss power for THIS fight (the same
    /// `WorldState::boss_power_mult` already baked into his HP/ATK via
    /// `scale_by_power_mult`) - kept as its own raw scalar here too so his
    /// ability (see `cthulhu_debuff_stacks`) can scale ITS OWN magnitude
    /// by it, which `BossStats`/`scale_by_power_mult` alone doesn't expose
    /// downstream. `1.0` (neutral) for every non-boss unit and any boss
    /// without a dynamic-power-scaled ability.
    boss_dynamic_power_mult: f64,
    /// Cthulhu's ability (reworked 2026-08-16, replacing the old
    /// permanent single-target -90% damage bubble) - a stacking debuff
    /// applied to roughly half the party every `CTHULHU_DEBUFF_CADENCE_MS`,
    /// each stack worth `cthulhu_debuff_pct_per_stack` less damage AND
    /// healing dealt (see `resolve_hit`/`apply_heal`), floored at 90%
    /// total reduction. Lazy-expiry, same "reset to 0 if the timer's
    /// already passed, then add" convention as Thornedhide
    /// (`thornedhide_stacks`) - `CTHULHU_DEBUFF_DURATION_MS` (2s) is
    /// shorter than the 3s cast cadence, so a single Cthulhu's own casts
    /// don't overlap on any one player; genuine stacking only happens if
    /// more than one Cthulhu is present in the same fight and both
    /// happen to pick the same player within the same window - "sometimes
    /// stacks, but not always" per the request.
    cthulhu_debuff_stacks: u32,
    cthulhu_debuff_expires_at_ms: u32,
    /// Set fresh on every stack applied (not accumulated) - see
    /// `boss_dynamic_power_mult`'s doc for where the scaling comes from.
    /// A uniform rate straight from the source (Cthulhu himself), unlike
    /// Thornedhide's own per-stack rate (which varies by which PLAYER
    /// applied it), so this never needs to vary stack-to-stack.
    cthulhu_debuff_pct_per_stack: f64,
    /// Gelatinous Cube's defense shred - same lazy-reset-then-increment
    /// shape as `thornedhide_stacks`. Fixed magnitude
    /// (`CUBE_SHRED_PCT_PER_STACK`, a boss-only constant, not
    /// player-tree-scaled), so unlike `cthulhu_debuff_pct_per_stack`
    /// there's no per-unit "own rate" field needed alongside it.
    cube_shred_stacks: u32,
    cube_shred_expires_at_ms: u32,
    /// Running total of actual (post-mitigation) damage this unit has
    /// dealt so far this fight - purely so Cthulhu's ability (see
    /// `boss_ability`) can find "the top DPS" at the moment it fires.
    /// Tracked for every unit (harmless/unused for the boss's own),
    /// updated in `apply_hit`.
    damage_dealt_total: u64,
    /// The character's level - `0` for every enemy/boss/add unit
    /// (never a valid target, so never read for them). Drives enemy
    /// target-priority: an enemy attack (boss, Lich add, basic mob, or
    /// any of their splash) prefers whoever's above the party's median
    /// level first, so higher-level heroes take the brunt of a fight
    /// instead of newer/lower-level players - see
    /// `prioritize_above_median`.
    level: u32,
    /// Fraction of actual damage dealt leeched back as self-healing - see
    /// `Character::combat_life_leech`/`ArchetypeBonus::life_leech_pct`.
    /// 0.0 for every enemy/boss/add unit and for a player with no leech
    /// investment (currently Slayer-only).
    life_leech_pct: f64,
    /// When `leech_gained_in_window` was last drained (see
    /// `LIFE_LEECH_CAP_PER_SEC`) - every leech attempt bleeds it down by
    /// `cap * (elapsed_ms / 1000.0)` first, a genuine leaky-bucket rolling
    /// window rather than a lump-sum reset once 1000ms has passed (a
    /// reset-based window let a leech build burst to ~2x the per-second
    /// cap by timing hits across the reset boundary). Despite the name
    /// (kept to avoid a struct/constructor-wide rename), this is a "last
    /// updated" timestamp, not a window's start. 0 (harmless) for a unit
    /// with no leech.
    leech_window_start_ms: u32,
    /// How much of this unit's rolling per-second leech budget is
    /// currently "spent" - drained continuously (see
    /// `leech_window_start_ms`), topped up by each leech landed, caps
    /// further leech once it hits `max_hp * LIFE_LEECH_CAP_PER_SEC`.
    leech_gained_in_window: f64,
    /// This unit's active archetype skills (see `ArchetypeSkill`) - from
    /// `Archetype::skills()` for a player, always empty for an enemy/
    /// boss/add unit. A skill is looked up from here, never hardcoded at
    /// a call site, so adding a new one never means touching every unit
    /// build site - just `Archetype::skills()`'s own match.
    skills: Vec<ArchetypeSkill>,
    /// Generic per-skill counter storage, keyed by the skill itself -
    /// e.g. a stacking ramp or a "already procced this fight" latch. Most
    /// skills need nothing here (a pure chance-on-trigger effect just
    /// reads `skills` and rolls), but this exists so a FUTURE skill that
    /// does need state never has to earn its own bespoke field the way
    /// `helm_stack_bonus`/`boots_power` did. Genuinely unused until the
    /// first stateful skill lands (Frenzy - the pilot - doesn't need it),
    /// so it's allowed to sit dead for now rather than be re-added later
    /// and reopen the exact per-skill-field problem this exists to avoid.
    #[allow(dead_code)]
    skill_stacks: HashMap<ArchetypeSkill, f64>,
    /// When this unit's `ArchetypeSkill::FlickerStrike` next fires -
    /// `u32::MAX` (never) for anyone without it, same "impossibly far
    /// away" convention as `next_helm_at_ms`/`next_ability_at_ms` for a
    /// unit with no helm/boss ability. The first periodic (own-clock)
    /// skill, unlike `Frenzy`'s reactive on-kill hook - see
    /// `ArchetypeSkill::on_periodic_tick`.
    next_flicker_at_ms: u32,
    /// Whether this unit has `UniqueAffix::CelestialConversion` equipped
    /// on ANY of its 5 slots (there's only ever one to check for so far -
    /// a plain bool, not a generic list, same "don't build the general
    /// case until there's a second instance" reasoning as `skills` was
    /// before it needed to be one). Always `false` for an enemy/boss/add
    /// unit. See the heal-application block in the main loop below for
    /// the actual effect.
    has_celestial_conversion: bool,
    /// Slayer's Open Wound, ATTACKER side - THIS unit's own
    /// wound-dealing potency, snapshotted at construction from their
    /// passive investment (see `passive_tree::SLAYER_NODES`'s `wound`/
    /// `festering`/`hemorrhage`/`necrotic` branch) - `apply_hit` reads
    /// these off the ATTACKER to know what to apply to whoever they hit.
    /// `wound_deal_max_stacks == 0` means "no Open Wound invested",
    /// gating the whole mechanic off in one check. 0/false defaults for
    /// anyone without it (everyone except a Slayer who's invested).
    wound_deal_leech_per_stack: f64,
    wound_deal_max_stacks: u32,
    wound_deal_duration_ms: u32,
    wound_deal_damage_dealt_debuff: f64,
    wound_deal_heal_received_debuff: f64,
    /// Hemorrhage - fraction of the wound's own banked damage (see
    /// `wound_damage_taken_total`) dealt as a bonus explosion the instant
    /// the wound reaches max stacks. 0.0 without Hemorrhage invested.
    wound_deal_explosion_pct: f64,
    /// Overflow - fraction of the explosion's damage leeched back to the
    /// Slayer, on top of their normal per-stack leech.
    wound_deal_explosion_self_leech_pct: f64,
    /// Arterial Spray - how many nearby enemies the explosion also hits,
    /// each at the same fraction.
    wound_deal_explosion_extra_targets: u32,
    /// Festering Wound - whether a wound also gets applied to this
    /// attacker's splash targets, not just their primary target.
    wound_deal_spreads_to_splash: bool,
    /// Contagion - chance for a wound to jump to a new host when its
    /// current one dies.
    contagion_chance: f64,
    /// Grave Chill - attack-speed slow applied to wounded enemies.
    gravechill_speed_debuff_pct: f64,
    /// Plague Bearer - extra nearby enemies Necrotic Grip's damage-dealt
    /// debuff also spreads to at wound-apply time.
    plaguebearer_extra_targets: u32,
    /// Slayer's Open Wound, DEFENDER side - the current wound STATUS
    /// inflicted ON this unit (any unit can carry these regardless of who
    /// wounded them - 0/harmless defaults for everyone until an actual
    /// wounding hit lands). Lazily treated as unwounded once
    /// `wound_expires_at_ms` has passed rather than needing a periodic
    /// tick to clear it - every read site checks the expiry itself.
    /// `wound_leech_per_stack`/`wound_damage_dealt_debuff`/
    /// `wound_heal_received_debuff` are copied from whichever attacker's
    /// `wound_deal_*` most recently applied/refreshed the wound - what
    /// gets CONSULTED while this unit is wounded, not what it deals.
    wound_stacks: u32,
    wound_max_stacks: u32,
    wound_expires_at_ms: u32,
    wound_leech_per_stack: f64,
    wound_damage_dealt_debuff: f64,
    wound_heal_received_debuff: f64,
    /// Running total of actual (post-mitigation, post-shield) damage
    /// dealt to this target across the wound's current lifetime - reset
    /// to 0 whenever a fresh wound starts (stacks go from expired/0 to
    /// 1), added to on every hit that refreshes the wound. Hemorrhage's
    /// explosion consumes this (see `wound_deal_explosion_pct`'s doc) -
    /// "the wound's remaining damage" is read as the real damage banked
    /// up from the hits that built the stacks, not an arbitrary fraction
    /// of the target's max HP (which would make the explosion trivially
    /// enormous against a high-HP boss, unrelated to how the fight is
    /// actually going).
    wound_damage_taken_total: f64,
    /// Vampiric Frenzy's real per-unit FlickerStrike cadence - replaces
    /// the bare `FLICKER_STRIKE_COOLDOWN_MS` constant both at
    /// construction AND at every reschedule (fixes a real bug where the
    /// old code only ever discounted the very FIRST cast - see the main
    /// loop's `NextEvent::FlickerStrike` handling). Equals the base
    /// constant for anyone without Vampiric Frenzy invested.
    flicker_cooldown_ms: u32,
    /// Slayer's Bloodpact - guaranteed charges banked for THIS fight (1
    /// base + Blood Sacrifice rank), consumed automatically on this
    /// unit's own first unified-attack action each fight (see the main
    /// loop's player-turn branch) - there's no live player input during
    /// an auto-battle sim, so the sim itself decides when to fire it.
    /// Redesigned (2026-08-16, live request: "give bloodpact a 4 second
    /// cooldown so it can be re-used to gain stacking effects through the
    /// fight") from a flat per-fight charge count into a real cooldown:
    /// fires again as soon as `at_ms >= next_bloodpact_at_ms`, checked at
    /// the SAME unified-attack turn trigger site as before (not its own
    /// independent `NextEvent`, since its payoff boosts that same attack's
    /// damage). `u32::MAX` (never) without `sacrifice` invested, same
    /// "impossibly far away" convention as `next_helm_at_ms`; `0` for
    /// anyone WITH it invested, so it's available on their very first
    /// attack, matching the original "fires on your first hit(s)" text.
    next_bloodpact_at_ms: u32,
    /// When Bloodpact last ACTUALLY fired (0 = never yet this fight) -
    /// separate from `next_bloodpact_at_ms`, which a reset source (Second
    /// Wind's explosion-triggered reset, Clean Slate's hit-triggered
    /// reset) can set to "ready right now." Both reset sites clamp against
    /// this instead of writing `at_ms` directly, so Bloodpact can never
    /// actually re-fire more than once per 1000ms REGARDLESS of how many
    /// reset sources stack up in a short window (a live report traced a
    /// Slayer's damage reaching the trillions back to Second Wind +
    /// Arterial Spray chaining multiple same-tick Hemorrhage explosions,
    /// each with its own shot at resetting the cooldown to "now").
    bloodpact_last_fired_at_ms: u32,
    /// Base 4s, reduced 500ms per Blood Sacrifice rank (floored at 2s) -
    /// Blood Sacrifice's role shifted from "+1 charge per rank" (charges no
    /// longer exist) to "fires more often" instead, same overall spirit.
    bloodpact_cooldown_ms: u32,
    /// How many times Bloodpact has fired so far THIS fight - persistent,
    /// never decays. Triage reads it to discount the HP cost further with
    /// each use; Warlord's Resolve reads it for its "3rd use" trigger.
    bloodpact_uses_this_fight: u32,
    /// Triage - HP cost reduction per rank, per PRIOR use this fight (see
    /// `bloodpact_uses_this_fight`'s doc) - the first use is never
    /// discounted, matching its own "each EXTRA charge" framing. 0.0
    /// without it invested.
    bloodpact_triage_pct: f64,
    /// Final Offering (2026-08-17, replacing its old "the LAST use of the
    /// fight is free" premise - structurally unknowable in advance, no
    /// fight-outcome lookahead exists anywhere in this sim). Real version:
    /// once `bloodpact_uses_this_fight` (read BEFORE incrementing, same as
    /// Triage) reaches this many PRIOR uses, every use after that gets
    /// `bloodpact_finaloffering_pct` off - `4 - rank` (3/2/1 prior uses,
    /// i.e. the 4th/3rd/2nd use onward). `u32::MAX` (never) without it
    /// invested, same "impossibly far away" sentinel convention as
    /// `next_helm_at_ms`.
    bloodpact_finaloffering_min_prior_uses: u32,
    /// Final Offering's discount once the threshold above is reached -
    /// flat 33%, not per-rank (rank instead lowers the threshold - see the
    /// field above). Combined with Triage's own discount multiplicatively,
    /// both capped together at 90% off (see the Bloodpact firing site) so
    /// the two can never make it free even at 3/3 both.
    bloodpact_finaloffering_pct: f64,
    /// Warlord's Resolve (Slayer) - broadcast via the same
    /// `temp_party_increased_damage_bonus` party-wide primitive Berserker's
    /// own Warlord's Resolve uses, fired specifically on Bloodpact's 3rd
    /// use this fight. 0.0 without it invested.
    bloodpact_warlordsresolve_pct: f64,
    /// Clean Slate - chance per rank for a successful Grim Bargain refund
    /// to also fully reset `next_bloodpact_at_ms` (i.e. Bloodpact becomes
    /// available again immediately). 0.0 without it invested.
    bloodpact_cleanslate_reset_chance: f64,
    /// Second Wind (Slayer's Open Wound branch) - chance per rank for a
    /// Hemorrhage explosion to do the same full cooldown reset. 0.0
    /// without it invested.
    bloodpact_secondwind_reset_chance: f64,
    /// % of CURRENT hp Bloodpact costs when it fires - snapshotted at
    /// construction from the Slayer's own rank (Triage/Final Offering
    /// discounts applied at the moment of firing, not baked in here).
    bloodpact_hp_cost_pct: f64,
    /// Bloodpact's guaranteed damage multiplier on the sacrificed hit -
    /// 2.0/3.0/4.0 at Bloodpact rank 1/2/3 (1.0 = no bonus, for anyone
    /// without it invested). Replaces the old "guaranteed crit" payoff,
    /// which was weak or worthless on a Slayer with low crit_multiplier -
    /// this is a flat, deterministic damage bonus instead.
    bloodpact_damage_mult: f64,
    /// > 0.0 when Martyrdom is invested: Bloodpact shields the lowest-HP
    /// ally for this fraction of the sacrificed HP instead of applying its
    /// damage multiplier. 0.0 (damage-boost mode) without it.
    bloodpact_martyrdom_shield_pct: f64,
    /// Grim Bargain's refund fraction of the sacrificed HP on a killing
    /// hit - 0.0 without it invested.
    bloodpact_kill_refund_pct: f64,
    /// Debt Collector's refund fraction of the sacrificed HP even when
    /// the Bloodpact hit DIDN'T kill - 0.0 without it invested.
    bloodpact_nonlethal_refund_pct: f64,
    /// Blood for Blood's bonus kill refund - this fraction of the TARGET's
    /// max HP, added on top of Grim Bargain's normal refund, only on a
    /// killing hit. 0.0 without it invested.
    bloodpact_bloodforblood_pct: f64,
    /// A temporary damage-absorption pool, consumed before `hp` on any
    /// incoming hit (see `apply_hit`) - expires at `shield_expires_at_ms`.
    /// Granted by Martyrdom's ally shield or Cleric's Overflowing Grace
    /// (see `apply_heal`'s doc) - both share this same pool/expiry pair.
    shield_hp: f64,
    shield_expires_at_ms: u32,
    /// Shield-absorb reflect - three archetypes (Cleric's Sacred Barrier,
    /// Paladin's Retribution Aura, Slayer's Guardian's Blood) all grant
    /// "when THIS unit's shield absorbs damage, reflect a fraction of the
    /// absorbed amount back at the attacker" off the SAME `shield_hp`
    /// pool above - just with different fine print, captured by these 3
    /// fields instead of collapsing into one: Sacred Barrier is chance-
    /// gated with a fixed 20% value; Retribution Aura and Guardian's
    /// Blood are both guaranteed with a per-rank-scaling value, but
    /// Retribution Aura ONLY fires when the shield absorbs the hit
    /// ENTIRELY (a partial absorption reflects nothing), while Guardian's
    /// Blood fires off any absorption at all, partial included. See
    /// `apply_reflect_damage`'s doc for how the reflected hit itself
    /// works (deliberately NOT routed back through `apply_hit`, to avoid
    /// two shielded+reflecting units bouncing the same hit forever).
    shield_reflect_pct: f64,
    /// 1.0 (always fires once absorption/full-absorb conditions are met)
    /// for Retribution Aura/Guardian's Blood; Sacred Barrier's own
    /// per-rank chance for Cleric.
    shield_reflect_chance: f64,
    /// Paladin-only gate - Retribution Aura's own text requires the
    /// shield to have absorbed the ENTIRE hit, not just part of it.
    /// Always `false` for Cleric/Slayer's versions (any absorption
    /// qualifies for those two).
    shield_reflect_requires_full_absorb: bool,
    /// Cleric's Guardian Spirit - charges banked for THIS fight (0 below
    /// rank 2, 1 at rank 2, 2 at rank 3 - a non-linear rank gate, not a
    /// smooth per-rank formula, per the node's own text). Checked in
    /// `apply_hit` before HP is allowed to reach 0 on ANY party member
    /// (including this Cleric themselves) - consumes a charge and heals
    /// instead of letting the killing blow land. 0 for anyone without it.
    guardian_spirit_charges: u32,
    /// % of max HP Guardian Spirit heals for when it saves someone - 20%
    /// flat (not rank-scaled itself) plus Second Chance's per-rank bonus,
    /// snapshotted at construction. 0.0 without Guardian Spirit invested.
    guardian_spirit_heal_pct: f64,
    /// Divine Intervention - +damage reduction granted to whoever Guardian
    /// Spirit just saved, for `GUARDIAN_SPIRIT_SAVE_BUFF_DURATION_MS`.
    /// 0.0 without it invested.
    guardian_spirit_save_dr_pct: f64,
    /// Final Blessing - +healing power granted to the WHOLE PARTY after a
    /// Guardian Spirit save, for `GUARDIAN_SPIRIT_SAVE_BUFF_DURATION_MS`.
    /// 0.0 without it invested. Applied via the generic
    /// `temp_heal_power_bonus`/`temp_heal_power_bonus_expires_at_ms` pair
    /// below (also reused by Merciful Touch's Healing Touch modifier).
    guardian_spirit_save_heal_power_pct: f64,
    /// Druid's Verdant Burst (2026-08-16 rework - see passive_tree.rs) -
    /// charges banked for THIS fight, LINEAR with rank (1 at rank 1, 2 at
    /// rank 2, 3 at rank 3 - unlike Guardian Spirit's non-linear gate,
    /// per the node's own "can trigger 1/2/3 times per fight" text).
    /// Checked in `apply_hit` alongside Guardian Spirit, but with its own
    /// extra condition: only saves the target if THIS unit's own total
    /// pending (not-yet-delivered) heal-flavor Lingering Effect healing
    /// on that target currently exceeds the lethal hit's own damage - see
    /// the trigger site's own `verdant_pending_by_source` doc. 0 for
    /// anyone without it (or not a Druid).
    verdantburst_charges: u32,
    /// A live, temporary bonus added on top of `heal_power` wherever it's
    /// read (lazy-expiry, same convention as `wound_expires_at_ms`) -
    /// granted by Final Blessing (party-wide, after a Guardian Spirit
    /// save) or Healing Touch (single bounced ally, after a Prayer of
    /// Mending bounce). 0.0/0 when nothing has granted it.
    temp_heal_power_bonus: f64,
    temp_heal_power_bonus_expires_at_ms: u32,
    /// Cleric's Eternal Light (2026-08-17) - THIS unit's own magnitude,
    /// written into `temp_heal_power_bonus` above on every heal they land
    /// (see `apply_heal`'s own hook). 0.0 without it invested.
    eternallight_bonus_pct: f64,
    /// Divine Intervention's own damage-reduction grant (see
    /// `guardian_spirit_save_dr_pct`'s doc) - lives on the SAVED unit,
    /// consulted in `resolve_hit` alongside their other reduction
    /// sources. Separate field from `temp_heal_power_bonus` since it's a
    /// different stat entirely.
    temp_damage_reduction_bonus: f64,
    temp_damage_reduction_bonus_expires_at_ms: u32,
    /// Cleric's Overflowing Grace - overheal (the part of a heal that
    /// would exceed the target's max HP) becomes a temporary shield worth
    /// this fraction of the overheal instead of being wasted (see
    /// `apply_heal`). 0.0 without it invested.
    overflow_grace_shield_pct: f64,
    overflow_grace_shield_duration_ms: u32,
    /// Balanced Faith - +damage reduction while THIS unit's shield (from
    /// any source) is still active. Lives on the shield holder, not the
    /// Cleric who granted it - consulted in `resolve_hit`.
    overflow_grace_shield_dr_pct: f64,
    /// Cleric's Sanctified Touch - a heal that crits deals this much MORE
    /// on top of the normal crit multiplier (rank-gated: 0.0 below rank
    /// 2, since the node text says the whole effect is "unlocked at rank
    /// 2"). 0.0 without it invested (or invested below rank 2).
    heal_crit_bonus_mult: f64,
    /// Sanctified Touch rank 3 - a flat bonus added to `crit_chance` for
    /// the heal roll specifically, not the attack share. 0.0 below rank 3.
    heal_crit_chance_bonus: f64,
    /// Radiance - a critical heal also splashes this fraction of its
    /// value to the rest of the party (reuses `apply_heal_splash`). 0.0
    /// without it invested.
    heal_crit_splash_pct: f64,
    /// Gracious Spirit - +healing power specifically applied to the
    /// lowest-HP ally's incoming heal (the primary heal target always IS
    /// the lowest-HP hurt ally already - see `heal_target_idx`'s
    /// selection - so this is a flat multiplier on the primary heal
    /// share only, never the splash/bounce extras). 0.0 without it.
    grace_lowest_ally_bonus_pct: f64,
    /// Prayer of Mending - chance for a heal to bounce to another hurt
    /// ally (Swift Mending's bonus folded in at construction). 0.0
    /// without `prayer` invested.
    prayer_chance: f64,
    /// How many total bounce targets one successful Prayer proc chains
    /// through - 1 base, +1 per Chain of Light rank (capped 3). 0 without
    /// `prayer` invested (chance is also 0 in that case, so this never
    /// matters on its own).
    prayer_bounce_targets: u32,
    /// Value fraction of the PRIMARY heal each bounce target receives -
    /// 50% baseline from Prayer itself, overridden by Merciful Touch's
    /// own rank-scaled value once invested (50%-80%), plus Gentle
    /// Touch's flat bonus on top either way.
    prayer_bounce_value_pct: f64,
    /// Cleric's Unbroken Prayer - after the deterministic bounce pass
    /// above completes, this chance is rolled AGAIN, repeatedly: each
    /// success reaches one more ally not already healed by this SAME
    /// proc (excluding the primary target and every bounce already hit),
    /// at the same `prayer_bounce_value_pct` value, stopping on the first
    /// failed roll or once no eligible ally remains - so the theoretical
    /// max reach is the whole party (see `apply_heal_bounce`'s doc for the
    /// exclusion-set loop). 0.0 without it invested.
    unbroken_prayer_chance: f64,
    /// Divine Favor - each bounce also shields its target for this
    /// fraction of the bounce heal's value (Aegis of Mercy's bonus folded
    /// in at construction). 0.0 without it invested.
    divine_favor_shield_pct: f64,
    divine_favor_shield_duration_ms: u32,
    /// Healing Touch - the bounced ally gets a temporary healing-power
    /// buff for `HEALING_TOUCH_DURATION_MS` (applied via the generic
    /// `temp_heal_power_bonus` pair above). 0.0 without it invested.
    healing_touch_pct: f64,
    /// Mage's Arcane Shield - a crit grants THIS unit a shield worth this
    /// fraction of their own max HP (checked in `apply_hit` off
    /// `outcome.is_crit`, granted via `grant_shield`). 0.0 without it
    /// invested.
    crit_shield_max_hp_pct: f64,
    /// Warlock's Soul Harvest - a kill heals this unit for this fraction
    /// of their own max HP, on top of Soul Siphon's per-hit leech (checked
    /// in `apply_hit`'s Defeat branches, alongside `fire_on_kill`). 0.0
    /// without it invested.
    soul_harvest_heal_pct: f64,
    /// Dark Ritual - a temporary self increased-damage buff on a kill
    /// (self-only write into the shared `temp_party_increased_damage_bonus`
    /// field).
    darkritual_dmg_pct: f64,
    /// Eternal Hunger - Soul Harvest's heal is also guaranteed to grant a
    /// shield worth this fraction of the heal actually restored. 0.0
    /// without it invested (Soul Harvest's own heal still lands either
    /// way, just without the shield).
    eternal_hunger_shield_pct: f64,
    /// Paladin's Divine Shield - a periodic self-cast, same shape as
    /// Helm/Boots (`next_helm_at_ms`/`next_boots_at_ms` above): fires on
    /// its own clock (`next_divine_shield_at_ms`), independent of this
    /// unit's normal attack cadence. Shields whoever currently has the
    /// lowest HP in the party (self included) for this fraction of THIS
    /// unit's own max HP (Bulwark of Light's bonus folded in at
    /// construction). `u32::MAX` on `next_divine_shield_at_ms` (not
    /// invested) means it can never be the soonest event, same convention
    /// as an unequipped Helm.
    divine_shield_amount_pct: f64,
    /// Base 8s, reduced by Divine Shield's own rank (Grace Period's bonus
    /// folded in at construction) - see the main loop's
    /// `NextEvent::DivineShield` handling.
    divine_shield_cooldown_ms: u32,
    next_divine_shield_at_ms: u32,
    /// Consecration - Divine Shield's cast also grants a smaller shield to
    /// the REST of the party (everyone except whoever got the primary
    /// shield) worth this fraction of THIS unit's own max HP (Wider
    /// Blessing's bonus folded in at construction). 0.0 without
    /// `consecration` invested.
    consecration_shield_pct: f64,
    /// Communion - Consecration also grants the party a temporary
    /// healing-power buff for its own duration. 0.0 without it invested.
    communion_heal_power_pct: f64,
    /// Purify - temporary damage-dealt debuff applied to an attacker whose
    /// hit Retribution Aura/Holy Vengeance fully reflects.
    purify_dmg_debuff_pct: f64,
    /// Last Judgment - chance for the same fully-reflected hit to also
    /// skip the attacker's next action.
    lastjudgment_skip_chance: f64,
    /// Base 5s (Shared Light's bonus folded in at construction) - separate
    /// from the primary shield's own duration since Shared Light only
    /// extends Consecration's party shield specifically, not Divine
    /// Shield's own.
    consecration_shield_duration_ms: u32,
    /// Paladin's Radiant Smite (redesigned 2026-08-15 around offensive
    /// healing, replacing the original "vs whoever's targeting you" text -
    /// see the live design conversation this was rebuilt from) - fires on
    /// EVERY unified action this unit takes (not gated on the damage share
    /// actually landing, unlike a normal attack - see the main loop's own
    /// doc for why: a 100%-heal-power Paladin still needs this to fire so
    /// Holy Fire has something to convert), healing up to
    /// `HEAL_SPLASH_MAX_TARGETS` (+`smite_extra_targets`, +
    /// `SPLASH_OVERFLOW_BONUS_TARGETS` more past 100% splash - same
    /// convention `apply_heal_splash` already uses) nearby hurt allies
    /// (self included) for this fraction of THIS unit's own max HP each.
    /// 0.0 without `smite` invested.
    smite_heal_pct: f64,
    /// Zealotry - adds this much more to `smite_heal_pct` (5/10/15% per
    /// rank, additive) AND unlocks `smite_extra_targets` below. 0.0
    /// without it invested.
    smite_zealotry_bonus_pct: f64,
    /// Zealotry's own text says "1 additional target", not "+1 per rank" -
    /// a flat unlock at any invested rank, not further rank-scaled. 0 (no
    /// bonus) without Zealotry invested.
    smite_extra_targets: u32,
    /// Desperate Grace (Martyr's Call, 2026-08-17) - bonus to Radiant
    /// Smite's heal against any target CURRENTLY below 50% HP, checked
    /// per-target inside `apply_radiant_smite_heal`'s own loop (unlike
    /// Judgment above, which gates off the DAMAGE share's boss target, a
    /// different condition entirely). 0.0 without it invested.
    zealotry_martyrscall_bonus_pct: f64,
    /// United Front (Rising Fervor, 2026-08-17) - scales the WHOLE cast's
    /// heal amount by how many allies it actually reaches this cast
    /// (known before the per-target loop starts). 0.0 without it invested.
    zealotry_risingfervor_pct_per_ally: f64,
    /// Zealous Charge (Guardian's Wrath, 2026-08-17) - THIS unit's own
    /// magnitude; grants a temporary self attack-speed buff (see
    /// `zealotry_guardianswrath_speed_bonus`/`zealouscharge_multiplier`)
    /// whenever a Smite cast heals at least one ally below 50% HP. 0.0
    /// without it invested.
    zealotry_guardianswrath_speed_pct: f64,
    /// Live state for the buff above - same lazy-expiry pair every other
    /// timed self-buff here uses (e.g. `fel_rush_speed_bonus`/
    /// `fel_rush_expires_at_ms`).
    zealotry_guardianswrath_speed_bonus: f64,
    zealotry_guardianswrath_expires_at_ms: u32,
    /// Judgment - adds this much more to `smite_heal_pct`, but ONLY when
    /// the enemy THIS action's damage share is hitting is below 50% HP
    /// (checked live against that specific target, same "read the live
    /// hp%" convention as Berserker's Gambit/Blood Scent) - never applies
    /// on a 100%-heal-power action with no enemy target at all. 0.0
    /// without it invested.
    smite_judgment_bonus_pct: f64,
    /// Final Judgment - raises Judgment's own below-X%-HP threshold (0.0 =
    /// use the default 50%).
    judgment_threshold: f64,
    /// Holy Fire - this fraction of THIS unit's TOTAL healing done on one
    /// unified action (the normal heal-power share AND Radiant Smite's own
    /// heal, summed) is dealt as damage to EVERY alive enemy - the one
    /// piece that lets a 100%-heal-power Paladin (whose normal damage
    /// share is floored to 0, per `heal_power`'s doc) still deal any
    /// damage at all, and lets a 0%-heal-power Paladin still generate
    /// healing output purely off Smite's own flat per-hit heal. 0.0
    /// without it invested.
    smite_holyfire_dmg_pct: f64,
    /// Purging Flame - THIS unit's own magnitude, applied as a temporary
    /// healing-received debuff to whoever Holy Fire strikes.
    purgingflame_heal_reduction_pct: f64,
    /// Live healing-received debuff currently affecting THIS unit (from
    /// someone else's Purging Flame).
    temp_heal_reduction_pct: f64,
    temp_heal_reduction_expires_at_ms: u32,
    /// Executioner's Blessing - Judgment kill heals this fraction of max
    /// HP.
    executionersblessing_heal_pct: f64,
    /// Wrath of the Heavens - chance for Judgment to also splash 50% of
    /// its value to nearby enemies.
    wrathoftheheavens_chance: f64,
    /// Druid's Unyielding Roots (2026-08-16 rework - replaces its old
    /// "doubles Living Armor's DR below an HP threshold" role entirely,
    /// see this field's own git history for that version) - a real taunt:
    /// every `unyieldingroots_cycle_ms` of fight time, for the first
    /// `UNYIELDINGROOTS_TAUNT_DURATION_MS` of that window, every boss
    /// attack targets THIS unit specifically, bypassing the normal
    /// above-median-priority/survivability target pick entirely (see the
    /// main loop's own boss-targeting block). Computed lazily off `at_ms`
    /// alone (`at_ms % cycle_ms < DURATION_MS`) rather than a scheduled
    /// event - deterministic, no extra per-unit clock/state needed. 0
    /// (never taunts) without it invested.
    unyieldingroots_cycle_ms: u32,
    /// Berserker's Gambit - +this much crit chance for every 20% of this
    /// unit's OWN max HP currently missing, checked live in
    /// `roll_attacker_damage` (which already receives the attacker with
    /// current hp/max_hp). 0.0 without it invested.
    gambit_crit_per_missing_20pct: f64,
    /// Berserker's Death Defiant (2026-08-17) - how long a frozen Gambit
    /// bonus lingers after a heal moves this unit to a lower missing-HP
    /// bucket (see `apply_heal`'s own hook) - `3000 * rank` ms, 0 (never
    /// freezes anything) without it invested.
    deathdefiant_grace_ms: u32,
    /// Live frozen-bonus state written by the hook above, consumed in
    /// `roll_attacker_damage` as a floor under the live Gambit bonus.
    deathdefiant_frozen_crit_bonus: f64,
    deathdefiant_frozen_crit_bonus_expires_at_ms: u32,
    /// Druid's Bramblegrowth - a hit that gets reduced (by ANY combined
    /// mitigation source, not just Thorned Barrier specifically - see
    /// `passive_tree::DRUID_NODES`'s doc for why) reflects this fraction
    /// of the reduced amount back at the attacker, via the same
    /// `apply_reflect_damage` Sacred Barrier/Retribution Aura/Guardian's
    /// Blood use. Thornlash's bonus folded in at construction. 0.0
    /// without `bramblegrowth` invested.
    bramble_reflect_pct: f64,
    /// Poison Thorns - THIS unit's own Bramblegrowth reflect also applies
    /// a temporary damage-dealt debuff to whoever it reflects onto (see
    /// `temp_damage_dealt_debuff` below, which lives on the ATTACKER -
    /// the one who gets debuffed - not here). 0.0 without it invested.
    poison_thorns_debuff_pct: f64,
    /// Entangle - THIS unit's own chance for a Bramblegrowth reflect to
    /// also land on a SECOND, distinct attacker who's hit them within the
    /// last `ENTANGLE_WINDOW_MS` (see `recent_attackers`'s doc). 0.0
    /// without it invested.
    entangle_chance: f64,
    /// Rolling list of distinct attacker ids that have hit THIS unit
    /// recently, each paired with when that entry expires - reframes
    /// Entangle's "multiple enemies hit you this turn" as a short window
    /// instead, since the sim is event-driven with no shared turn concept
    /// (see the passive_tree.rs doc for why). Only ever populated when
    /// `bramble_reflect_pct > 0.0` (nobody else reads this), pruned lazily
    /// at the one site that consults it.
    recent_attackers: Vec<(String, u32)>,
    /// A temporary damage-dealt debuff currently affecting THIS unit
    /// (granted by someone else's Poison Thorns), same shape/convention
    /// as `temp_damage_reduction_bonus` above (a flat multiplier + its
    /// own expiry, lazily treated as expired past `_expires_at_ms`). 0.0
    /// when nothing has debuffed this unit.
    temp_damage_dealt_debuff: f64,
    temp_damage_dealt_debuff_expires_at_ms: u32,
    /// Berserker's Frenzy (redefined 2026-08-15, replacing its old
    /// "kill grants an extra attack" shape) - see `fire_frenzy`'s doc for
    /// the whole mechanic. A flat 10% (`FRENZY_BASE_STRIKE_CHANCE`) chance
    /// per attack once `frenzy` is invested at all, plus Rising Fury/
    /// Frenzied Assault's bonuses. 0.0 without it invested.
    frenzy_strike_chance: f64,
    /// How many total times a triggered Frenzy strikes the SAME target -
    /// directly Frenzy's own rank (2/3/4 total hits at rank 1/2/3). 0
    /// without it invested.
    frenzy_extra_hits: u32,
    /// Blood Scent - doubles `frenzy_strike_chance` against a target at
    /// or below this HP% (0.0 = not invested/never triggers, 0.50 at
    /// rank 2, 0.65 at rank 3 - same "compare hp_pct directly, not
    /// missing%" convention as `unyielding_roots_threshold`, and the same
    /// bug class already fixed there).
    frenzy_bloodscent_threshold: f64,
    /// Overkill - each Frenzy extra strike reduces the TARGET's damage
    /// reduction by this much for just that one hit (a temporary
    /// override on `resolve_hit`'s `def.damage_reduction`, restored
    /// immediately after - same one-off convention as the crit_chance/
    /// crit_multiplier overrides elsewhere). 0.0 without it invested.
    frenzy_dr_shred_pct: f64,
    /// Berserking + Onslaught - Frenzy's extra strikes deal this much
    /// MORE damage (a multiplier on the strike's own base, not the
    /// primary hit). 0.0 without either invested.
    frenzy_extra_dmg_pct: f64,
    /// Culling Strike - any Frenzy strike against a target AT OR BELOW
    /// this HP% instead outright kills them (bypassing normal damage/
    /// mitigation entirely - a genuine execute, not overkill damage).
    /// 0.0 (never triggers) without it invested; 2%/rank up to 6% at 3/3.
    frenzy_culling_threshold: f64,
    /// Bloodletting + Vitality Surge - each Frenzy strike heals the
    /// attacker for this fraction of the damage THAT STRIKE actually
    /// dealt (post-mitigation, same "leech off real landed damage"
    /// convention as life leech). 0.0 without either invested.
    frenzy_heal_pct: f64,
    /// Second Wind - a Frenzy strike's own Bloodletting heal has this
    /// chance to also grant a shield (Second Wind's own rank scales the
    /// CHANCE; the value is a flat `FRENZY_SHIELD_VALUE_PCT` of the heal,
    /// not rank-scaled). 0.0 without it invested.
    frenzy_shield_chance: f64,
    /// Undying Fury - charges banked for THIS fight, same non-linear
    /// rank-gate convention as Cleric's `guardian_spirit_charges` (0
    /// below rank 2, 1 at rank 2, 2 at rank 3) but SELF-only, not
    /// party-wide - consumed in `apply_hit` alongside (but independently
    /// of) Guardian Spirit's own check.
    frenzy_undying_charges: u32,
    /// Chain Frenzy - chance for a Frenzy trigger to fire `fire_frenzy`
    /// again on the same target. `frenzy_chain_max_extra` (this node's
    /// own rank) hard-caps how many EXTRA chains one original trigger can
    /// produce, so this can never recurse unboundedly even at 100%
    /// chance - see `fire_frenzy`'s `chain_depth` parameter.
    frenzy_chain_chance: f64,
    frenzy_chain_max_extra: u32,
    /// Warrior's Spike Barrier - a BLOCKED hit (see `HitOutcome::is_blocked`)
    /// reflects this fraction of the total mitigated amount back at the
    /// attacker, via the same `apply_reflect_damage` Bramblegrowth/shield-
    /// absorb-reflect use. 0.0 without it invested.
    spike_barrier_reflect_pct: f64,
    /// Aegis - a blocked hit also shields this unit's lowest-HP ally for
    /// this fraction of the blocked amount (same "total mitigated on this
    /// hit" quantity `spike_barrier_reflect_pct` reflects). 0.0 without it
    /// invested.
    aegis_shield_pct: f64,
    /// Bastion - Aegis's shield duration, base + 1s/rank.
    aegis_shield_duration_ms: u32,
    /// Rally - the ally Aegis shields also gets this much temporary attack
    /// speed for the shield's duration (written into the shared
    /// `temp_party_attack_speed_bonus` field - a single-target write is
    /// harmless there, same field, just not broadcast to everyone). 0.0
    /// without it invested.
    aegis_rally_speed_pct: f64,
    /// Ironcircle - how many EXTRA lowest-HP allies Aegis shields, beyond
    /// the base 1 (so 1+this total, up to all 3 party members). 0 without
    /// it invested.
    aegis_extra_targets: u32,
    /// Thornedhide - THIS unit's own investment: Spike Barrier's reflect
    /// stacks a damage-dealt debuff on the attacker worth this much per
    /// stack (max 5, shared expiry - same simplification `add_speed_stack`
    /// already established). 0.0 without it invested. Copied onto whoever
    /// gets hit by it as `thornedhide_debuff_pct_per_stack` (a SEPARATE
    /// field - see its doc for why this can't reuse the same name).
    thornedhide_pct_per_stack: f64,
    /// Live stack count/expiry/value of Thornedhide's debuff currently
    /// affecting THIS unit (from someone else's Spike Barrier) - kept
    /// separate from `thornedhide_pct_per_stack` above (that one is always
    /// THIS unit's OWN investment) so a Warrior who both has Thornedhide
    /// AND gets hit by someone else's doesn't have one overwrite the
    /// other.
    thornedhide_stacks: u32,
    thornedhide_expires_at_ms: u32,
    thornedhide_debuff_pct_per_stack: f64,
    /// Retribution - chance per rank for a Spike Barrier reflect to crit
    /// for double. 0.0 without it invested.
    spike_retribution_chance: f64,
    /// Unyielding - chance per rank for Spike Barrier to also trigger off
    /// an unblocked (but still DR-reduced) hit, off the same reduced-
    /// amount quantity Bramblegrowth's own reflect uses. 0.0 without it
    /// invested.
    spike_unyielding_chance: f64,
    /// Second Skin - overrides the flat `BLOCK_DAMAGE_REDUCTION` constant
    /// with this unit's own rank-scaled value (65%/70%/75% at rank 1/2/3)
    /// - `BLOCK_DAMAGE_REDUCTION` itself for anyone without it invested.
    block_damage_reduction_pct: f64,
    /// Stonewall - THIS unit's first N hits taken each fight are
    /// automatically blocked (N = rank). 0 without it invested.
    stonewall_auto_block_hits: u32,
    /// How many hits THIS unit has taken (landed against them) so far this
    /// fight - tracked for everyone (harmless/unused without Stonewall),
    /// read live by `resolve_hit` against `stonewall_auto_block_hits`.
    hits_taken_this_fight: u32,
    /// Momentum (Warrior)/Fleetfoot (Rogue)/Bloodlust (Berserker) - a
    /// stacking, timed "each hit lands" buff, both the attack-speed AND
    /// increased-damage flavors sharing one counter (only one archetype's
    /// fields are ever nonzero on a given unit, sharing this bundle the
    /// same way Cleric/Druid share `prayer_chance`'s fields).
    /// `stack_speed_per_stack` is the per-stack ATTACK SPEED magnitude
    /// (Momentum/Fleetfoot, plus Frenzied Blows' bonus folded into
    /// Berserker's); `stack_dmg_per_stack` is the per-stack INCREASED
    /// DAMAGE magnitude (Bloodlust only); `stack_shred_per_stack` is
    /// Overwhelm's per-stack target-damage-reduction shred, scaled off
    /// THIS unit's own current stack count when THEY attack (not the
    /// target's). `stack_speed_max_stacks`/`_duration_ms` are the shared
    /// cap/decay window; `stack_speed_current`/`_expires_at_ms` are live
    /// sim state, both 0 at rest - see `add_speed_stack`'s doc for why a
    /// full reset-on-expiry, not a per-stack sliding window.
    stack_speed_per_stack: f64,
    stack_dmg_per_stack: f64,
    /// Avalanche - each of THIS unit's own Momentum stacks also adds this
    /// much increased damage (a separate per-stack rate from Bloodlust's
    /// own `stack_dmg_per_stack`, though both read the same
    /// `stack_speed_current` counter - mutually exclusive by archetype).
    /// 0.0 without it invested.
    stack_avalanche_dmg_per_stack: f64,
    /// Mage's Riptide - live crit-chance bonus per active Flow State
    /// stack (same shared `stack_speed_current` counter Bloodlust/
    /// Avalanche also read).
    stack_crit_per_stack: f64,
    /// Shatter - Overwhelm's shred also reduces the DEFENDER's block
    /// chance by the same live amount. 0.0 without it invested.
    shatter_shred_pct: f64,
    /// Exposed - extends how long `stack_shred_bonus` keeps reading a
    /// stale (post-expiry) stack count for, in ms, on top of Bloodlust's
    /// own shared expiry. 0 without it invested.
    overwhelm_shred_linger_ms: u32,
    /// Crush - Overwhelm's shred doubles against a target whose OWN
    /// current damage reduction is already below this threshold (0.0 = not
    /// invested).
    crush_dr_threshold: f64,
    /// Hurricane - each active Bloodlust/Frenzied-Blows stack also adds
    /// this much splash, read alongside `stack_dmg_per_stack`/
    /// `stack_shred_per_stack` off the same live count.
    stack_splash_per_stack: f64,
    /// Windfury - chance for `add_speed_stack` to grant a 2nd stack on the
    /// same trigger. 0.0 without it invested.
    windfury_chance: f64,
    stack_shred_per_stack: f64,
    stack_speed_max_stacks: u32,
    stack_speed_duration_ms: u32,
    stack_speed_current: u32,
    stack_speed_expires_at_ms: u32,
    /// Monk's Flowing Strikes - a SEPARATE stacking timed buff from the
    /// bundle above, since its trigger is "consecutive hit on the SAME
    /// target" rather than "any hit lands" (see `add_flowing_stack`'s
    /// doc). `flowing_speed_per_stack` is the per-stack attack-speed
    /// magnitude; `flowing_crit_per_stack` is Pressure Point's per-stack
    /// crit-chance magnitude. `flowing_max_stacks` folds in Hundred
    /// Fists' bonus; `flowing_duration_ms` folds in Relentless Assault's
    /// rank-3 bonus. `flowing_last_target` is the unit index this unit's
    /// streak is currently against (`usize::MAX` sentinel = no streak
    /// yet, same "impossible index means none" convention as elsewhere).
    flowing_speed_per_stack: f64,
    flowing_crit_per_stack: f64,
    flowing_max_stacks: u32,
    flowing_duration_ms: u32,
    /// Rising Storm - THIS unit's own magnitude, granted (self-only, via
    /// the shared `temp_party_increased_damage_bonus` field) on reaching
    /// max Flowing Strikes stacks. 0.0 without it invested.
    risingstorm_dmg_pct: f64,
    /// Nerve Strike - flat crit-damage bonus for anyone with Pressure
    /// Point invested. 0.0 without it invested.
    nervestrike_crit_mult_bonus: f64,
    /// Vital Points - live target-DR-shred per active Flowing Strikes
    /// stack. 0.0 without it invested.
    vitalpoints_shred_per_stack: f64,
    /// Eternal Flow - bonus stacks added on each Relentless Assault
    /// refresh. 0 without it invested.
    eternalflow_bonus_stacks: u32,
    /// Flow like Water (2026-08-17 as "One Hundred Hands", renamed and
    /// promoted from modifier to Specialization 2026-08-18) - extra
    /// Flowing Strikes stacks added on a Pressure Point crit, on TOP of
    /// `add_flowing_stack`'s own normal +1 (no target-match gating, unlike
    /// that normal refresh - crit is crit regardless of target
    /// continuity). 0 without it invested.
    onehundredhands_bonus_stacks: u32,
    /// Stormfront - live splash bonus while at MAX Flowing Strikes stacks
    /// specifically (not scaled per-stack below max). 0.0 without it
    /// invested.
    stormfront_splash_pct: f64,
    flowing_current: u32,
    flowing_expires_at_ms: u32,
    flowing_last_target: usize,
    /// Hundred Fists' 3 support leaves (2026-08-17). `chakra_of_many_pct` -
    /// a landed hit also fires a second `apply_hit` at this fraction of the
    /// primary hit's own base damage, same "independent follow-up hit that
    /// rolls its own crit/mitigation and can trigger other on-hit effects"
    /// idiom as Celestial Shard's DPS proc (`has_celestial_conversion`) -
    /// see that call site's own doc. 0.0 without it invested.
    chakra_of_many_pct: f64,
    /// Chakra of Light - each landed hit also pushes guaranteed (+ one
    /// fractional) stacks of the real Lightning Damage debuff
    /// (`lightning_dmg_taken`) worth this fraction of the attacker's OWN
    /// `increased_damage`, per `roll_chakra_of_light_stacks`'s doc. 0.0
    /// without it invested.
    chakra_of_light_pct: f64,
    /// Chakra of Life - a hit that would kill this unit instead grants
    /// this many ms of full damage immunity (`chakraoflife_immune_until_ms`
    /// below), after which the unit dies unconditionally
    /// (`next_chakraoflife_expiry_at_ms`, the main loop's own scheduler
    /// field). 0 without it invested - see the "would-kill" branch chain in
    /// `apply_hit` and `NextEvent::ChakraOfLifeExpiry`.
    chakraoflife_duration_ms: u32,
    /// Live state - `at_ms <= this` means every damage source (normal
    /// hits, reflect, Volatile Magic splash, Lingering Effect DAMAGE ticks,
    /// Doom detonation) is blocked outright. 0 = not currently immune.
    chakraoflife_immune_until_ms: u32,
    /// Live scheduler state for the main loop's `NextEvent` scan - `u32::MAX`
    /// sentinel = no delayed death pending, same convention as
    /// `next_curse_expiry_at_ms`.
    next_chakraoflife_expiry_at_ms: u32,
    /// Ranger's Hunter's Mark / Warlock's Curse of Weakness - both are
    /// "on THIS unit's first landed hit each fight, apply a persistent
    /// debuff to that target, readable by anyone attacking them
    /// afterward" (see `apply_first_hit_mark`). The `own_*` fields below
    /// are THIS unit's snapshotted kit (from their own passive
    /// investment, same "folded in at construction" convention as
    /// everywhere else); the un-prefixed `mark_*`/`curse_dmg_taken_bonus`
    /// fields below THOSE are what's currently applied TO this unit by
    /// whoever marked/cursed them - two independent halves of the same
    /// mechanic, same "attacker-side kit vs. defender-side applied state"
    /// split as the `wound_deal_*`/`wound_*` bundle.
    /// Hunter's Mark's own PERSONAL crit-chance bonus (Predator's Eye's
    /// crit-mult bonus and Kill Zone's low-hp damage bonus are personal
    /// too - only Pack Tactics extends to allies). 0.0 without Mark
    /// invested.
    own_mark_crit_chance: f64,
    own_mark_crit_mult: f64,
    own_mark_low_hp_dmg: f64,
    /// Pack Tactics - crit chance granted to ANY ally (not just this
    /// unit) attacking the marked target. 0.0 without it invested.
    own_mark_ally_crit_chance: f64,
    /// Alpha's Predator - +increased damage for allies attacking the
    /// marked target (same "any OTHER player" scope as
    /// `own_mark_ally_crit_chance`).
    own_mark_ally_dmg_pct: f64,
    /// Hunter's Focus (2026-08-17) - a FRACTION (1/3 per rank, full value
    /// at 3/3) of this Ranger's own `own_mark_crit_mult` (Predator's Eye +
    /// Apex Hunter), shared with allies attacking the marked target - same
    /// "any OTHER ally" scope as `own_mark_ally_crit_chance`, just for
    /// crit damage instead of crit chance. 0.0 without it invested.
    own_mark_ally_crit_mult: f64,
    /// Wider Pack - Hunter's Mark also applies to this many additional
    /// random enemies at apply time (same spread-loop shape Contagious
    /// Curse already established).
    own_mark_spread_count: u32,
    /// Final Blow - raises Kill Zone's own below-25%-HP threshold (0.0 =
    /// use the default).
    killzone_threshold: f64,
    /// Clean Kill - chance for a Kill Zone kill to immediately re-mark a
    /// new target for free (bypassing the normal one-shot-per-fight gate).
    cleankill_remark_chance: f64,
    /// Hunter's Reward - self-heal on a kill while Hunter's Mark is
    /// invested (approximated as any kill, not narrowly gated to a real
    /// Kill Zone proc - see the call site's own doc).
    huntersreward_heal_pct: f64,
    /// Curse of Weakness - damage-TAKEN bonus applied to the cursed
    /// target, read by ANY attacker (no personal gating, unlike Mark -
    /// nothing in its text restricts it to the caster). Amplify Curse's
    /// bonus folds in directly here at construction. 0.0 without Curse
    /// invested.
    own_curse_dmg_taken: f64,
    /// Contagious Curse - how many additional random alive enemies also
    /// get cursed (full value, not a fraction) the moment this unit's
    /// curse first applies. 0 without it invested.
    own_curse_spread_count: u32,
    /// Warlock's Doom - THIS unit's own detonation efficiency (3%/rank of
    /// damage dealt to the cursed target while Doom-tracked). 0.0 without
    /// it invested. Copied onto the cursed target as `curse_detonate_pct`
    /// at apply time (see that field's doc).
    own_doom_detonate_pct: f64,
    /// Withering Curse - THIS unit's own magnitude, copied onto the
    /// cursed target as `curse_heal_reduction_bonus`.
    own_curse_heal_reduction_pct: f64,
    /// Epidemic - bonus damage-taken magnitude applied specifically to
    /// Contagious Curse's SPREAD copies (on top of the primary target's
    /// own value).
    own_curse_spread_bonus_pct: f64,
    /// Soul Stone (2026-08-17, repurposed from the formerly-inert
    /// "Virulence") - how many soul stones this Warlock can bank at once
    /// this fight (1/2/3 by rank, `c.passive_node_rank("virulence")` - the
    /// passive-tree KEY stays `"virulence"`, only the label/effect changed,
    /// same "keep the key, redefine the ability" precedent as Mage's
    /// Finite Loop). 0 without it invested.
    own_soul_stone_max: u32,
    /// Cursed Blood (2026-08-17, repurposed from its former "persist after
    /// expiry" flavor, which needed a real curse expiry - i.e. Doom - to
    /// mean anything and so stayed dormant) - how many random enemies get
    /// an immediate Curse of Weakness the moment a fight starts, 1/2/3 by
    /// rank. Applied once in `simulate_battle`, before the main event loop
    /// (see the fight-start setup pass). 0 without it invested.
    own_cursed_blood_target_count: u32,
    /// Dreadful Death/Apocalypse - Doom's own detonation amplifiers
    /// (Harbinger folds directly into `own_doom_detonate_pct` instead,
    /// same construction-site pattern as every other "extends the base
    /// magnitude" modifier here).
    own_dreadfuldeath_shred_pct: f64,
    own_apocalypse_splash_pct: f64,
    /// Whether this unit has already applied their Mark/Curse this fight -
    /// both are a ONE-TIME "first landed hit" trigger (see
    /// `apply_first_hit_mark`), not a per-hit reapplication.
    has_applied_mark_this_fight: bool,
    /// Which unit (by `id`, not index - stable across the `Vec` shuffling
    /// a defeated unit never causes, but avoids needing attacker/target
    /// indices threaded into `resolve_hit`) currently holds Hunter's Mark
    /// on THIS unit, if any. `None` for an unmarked unit and for anyone
    /// merely cursed (Curse's bonus is unconditional - see
    /// `curse_dmg_taken_bonus` - so it never needs this).
    mark_source_id: Option<String>,
    mark_crit_chance_bonus: f64,
    mark_crit_multiplier_bonus: f64,
    mark_low_hp_damage_bonus: f64,
    mark_ally_crit_chance_bonus: f64,
    /// Alpha's Predator - the marking Ranger's own bonus, copied onto the
    /// marked target so any ally attacking them reads it (mirrors
    /// `mark_ally_crit_chance_bonus`'s own copy-at-apply-time shape).
    mark_ally_dmg_bonus: f64,
    /// Hunter's Focus - see `own_mark_ally_crit_mult`'s doc. Same
    /// copy-at-apply-time shape as `mark_ally_crit_chance_bonus`.
    mark_ally_crit_multiplier_bonus: f64,
    /// Curse of Weakness's damage-taken bonus currently active on this
    /// unit - read as a negative reduction source in `resolve_hit`, same
    /// slot Overwhelm's shred already established. Persists for the rest
    /// of the fight once applied (no expiry - see `apply_first_hit_mark`'s
    /// doc for why Doom/Cursed Blood, whose whole premise is "when the
    /// curse expires", stay dormant as a result, same "left
    /// implemented-but-inert" precedent as Slayer's Rot/Withering Touch).
    curse_dmg_taken_bonus: f64,
    /// Soul Stone - how many this Warlock currently has banked (0..=
    /// `own_soul_stone_max`). Incremented (capped at the max) every time a
    /// curse this Warlock cast actually lands on an enemy - both the
    /// primary target and each Contagious Curse spread copy, see
    /// `apply_first_hit_mark` - and consumed 1-per-use in the death-save
    /// chain in `apply_hit`.
    soul_stones: u32,
    /// Soul Stone - total times THIS unit has been saved by it so far this
    /// fight (never decreases, unlike `soul_stones` itself) - drives the
    /// permanent 33%-per-use outgoing damage penalty in `resolve_hit` (see
    /// `SOUL_STONE_DMG_PENALTY_PER_USE`).
    soul_stone_uses_this_fight: u32,
    /// Warlock's Doom - real curse expiry, added on top of the base
    /// (permanent-for-the-fight) curse above. `u32::MAX` (never expires,
    /// the original/default behavior) unless the cursing attacker has
    /// Doom invested, in which case it's set to the moment the curse will
    /// detonate. `next_curse_expiry_at_ms` is the same value mirrored into
    /// the main loop's scheduling cache (see `NextEvent::CurseExpiry`),
    /// kept as a separate field purely so the scheduling scan (which reads
    /// EVERY unit's clocks every iteration) doesn't need a branch to
    /// distinguish "no curse" from "curse but not Doom-tracked" - both
    /// just read as `u32::MAX`, same "impossibly far away" convention as
    /// `next_helm_at_ms`/`next_ability_at_ms` for a unit without one.
    curse_expires_at_ms: u32,
    next_curse_expiry_at_ms: u32,
    /// Doom - actual (post-mitigation) damage this cursed target has taken
    /// since the curse was last (re)applied, banked for the detonation to
    /// consume - same "real damage banked up, not a % of max HP" idiom as
    /// `wound_damage_taken_total`. Only accumulated while
    /// `curse_expires_at_ms != u32::MAX` (i.e. Doom is actually tracking
    /// this curse) - harmless/unused otherwise.
    curse_damage_taken_total: f64,
    /// Doom - the cursing attacker's own detonation efficiency, copied
    /// onto THIS (cursed) unit at apply time (see `own_doom_detonate_pct`'s
    /// doc) so the detonation, which fires from the main loop's scheduling
    /// dispatch (no attacker index handy there), doesn't need to look the
    /// caster back up. 0.0 without Doom invested by whoever cursed this
    /// unit.
    curse_detonate_pct: f64,
    /// Doom - the cursing attacker's `id`, so the detonation (fired from
    /// the main loop's scheduling dispatch, which has no attacker index
    /// handy) can credit the right unit's `damage_dealt_total`/on-kill
    /// effects and label the `CombatEvent::Attack`'s `attacker` field -
    /// same "damage source resolved by id, not a live index" convention
    /// `LingeringDot::source_id` already uses. `None` without Doom
    /// invested by whoever cursed this unit.
    curse_source_id: Option<String>,
    /// Withering Curse - the cursing attacker's own magnitude, copied onto
    /// THIS (cursed) unit at apply time - a healing-received debuff, read
    /// in `apply_heal` alongside Purging Flame's own.
    curse_heal_reduction_bonus: f64,
    /// Warlock's Fel Rush - a KILL grants a flat (not stacking) attack-
    /// speed buff for `FEL_RUSH_DURATION_MS`, refreshed on every kill
    /// while active - a genuinely different shape from the Momentum/
    /// Bloodlust/Flowing Strikes per-hit stacking bundles above (no
    /// per-hit trigger, no stack count), so it gets its own small pair
    /// instead of reusing `stack_speed_*` (which fires on every landed
    /// hit unconditionally - reusing it here would incorrectly also grant
    /// Fel Rush's bonus on ordinary attacks, not just kills). 0.0 without
    /// it invested.
    fel_rush_speed_bonus: f64,
    /// Warp Speed - Fel Rush's own duration, overriding the flat constant.
    fel_rush_duration_ms: u32,
    /// Ravage (rank 3) - additional additive bonus per stack, one stack
    /// banked per kill while Fel Rush is active (capped at 3).
    ravage_stack_pct: f64,
    fel_rush_stacks: u32,
    fel_rush_expires_at_ms: u32,
    /// Mage's Timewarp / Warlock's Demonic Speed - see
    /// `early_fight_speed_multiplier`'s own doc for the full mechanic.
    /// This unit's own magnitude (Quickcast's or Fel Haste's current
    /// total, whichever archetype applies), 0.0 without either invested.
    early_fight_speed_bonus_pct: f64,
    /// The fight-opening window this bonus is active for - `5000 +
    /// 2000*rank` ms, set once at construction (never re-derived live).
    early_fight_speed_window_end_ms: u32,
    /// Slayer's Blood Frenzy - each FlickerStrike dash refreshes a flat
    /// (not stacking) attack-speed buff for `FLICKER_FRENZY_DURATION_MS` -
    /// same "flat refreshed timed buff" shape as Warlock's Fel Rush above,
    /// just triggered per-dash (see `ArchetypeSkill::on_periodic_tick`)
    /// instead of per-kill. 0.0 without it invested.
    flicker_frenzy_speed_bonus: f64,
    /// Unrelenting - extends Blood Frenzy's own shared expiry window.
    unrelenting_duration_bonus_ms: u32,
    /// Adrenaline - bonus crit multiplier for THIS dash's own direct hits.
    adrenaline_crit_mult_bonus: f64,
    /// Chain Reaper/Death Spiral - self-heal per Reaper's Momentum bonus
    /// target hit/killed this dash.
    chainreaper_heal_pct: f64,
    deathspiral_heal_pct: f64,
    /// Insatiable - chance per FlickerStrike hit to extend Endless
    /// Thirst's own cap-bonus window.
    insatiable_extend_chance: f64,
    /// Second Heartbeat - chance per FlickerStrike hit to trigger one more
    /// bonus strike at a random additional enemy.
    secondheartbeat_chance: f64,
    /// Overflow Vessel - fraction of leech lost to the per-second cap that
    /// becomes a temporary shield instead.
    overflowvessel_shield_pct: f64,
    flicker_frenzy_expires_at_ms: u32,
    /// Slayer's Endless Thirst - each FlickerStrike dash also refreshes a
    /// temporary raise to the leech-per-second cap (see
    /// `LIFE_LEECH_CAP_PER_SEC`), same trigger/duration as Blood Frenzy
    /// above but its own independent timer. At rank 3 the node's own text
    /// replaces the linear +% with "the cap is removed entirely" instead -
    /// `endless_thirst_uncapped` flags that case directly rather than
    /// trying to express "infinite" as a magnitude; `endless_thirst_cap_bonus`
    /// only matters at ranks 1-2. Both 0.0/false without it invested.
    endless_thirst_cap_bonus: f64,
    endless_thirst_uncapped: bool,
    endless_thirst_expires_at_ms: u32,
    /// Slayer's Reaper's Momentum - a kill from FlickerStrike itself
    /// (checked directly inside `ArchetypeSkill::on_periodic_tick`'s dash
    /// loop, not the generic `fire_on_kill` dispatch every other on-kill
    /// effect uses, since this is gated specifically to FlickerStrike
    /// kills) banks this many bonus targets for THIS unit's next dash.
    /// `reapers_momentum_per_kill` is the node's own rank magnitude (0
    /// without it invested, otherwise added once per kill - multiple
    /// kills in one dash bank additively); `reapers_momentum_banked` is
    /// the live counter, consumed (and reset to 0) the moment the next
    /// dash actually starts.
    reapers_momentum_per_kill: u32,
    reapers_momentum_banked: u32,
    /// Mage's Temporal Rift / Warlock's Unstable Power - both identical:
    /// attack speed above 100% "total" converts excess into increased
    /// damage. "Total" here means this unit's baseline gear+archetype+tree
    /// attack-speed investment (the same two terms `Character::
    /// attack_interval_ms` itself sums) - NOT live per-fight stacking
    /// buffs (Flow State/Momentum/Fel Rush), which stay a separate layer
    /// same as every other live-vs-baseline split in this sim. Snapshotted
    /// once at construction, read live in `roll_attacker_damage`.
    attack_speed_pct: f64,
    /// The node's own rank magnitude (30%/rank efficiency, shared field -
    /// mutually exclusive by archetype like every other shared bundle).
    /// 0.0 without either invested.
    speed_overflow_dmg_pct: f64,
    /// Paradox - a second, parallel conversion of the same excess attack
    /// speed into crit chance instead of damage.
    speed_overflow_crit_pct: f64,
    /// Eternal Moment - lowers the baseline-attack-speed threshold excess
    /// starts converting past (1.0 = 100%, the default).
    speed_overflow_threshold: f64,
    /// Paladin's Unbreakable Faith - heals this unit for this fraction of
    /// whatever Intervene redirects to them (see the boss-attack branch's
    /// `protector_share`), on top of the redirected hit itself. 0.0
    /// without it invested.
    unbreakable_faith_heal_pct: f64,
    /// Eternal Vow - chance for Unbreakable Faith's self-heal to also
    /// fully shield the protector.
    eternalvow_shield_chance: f64,
    /// Gracious Burden - fraction of the redirected damage also healed to
    /// the original (protected) ally.
    graciousburden_heal_pct: f64,
    /// Bonded Devotion - temporary DR granted to the intervened ally.
    bondeddevotion_dr_pct: f64,
    bondeddevotion_duration_ms: u32,
    /// Rogue's Twin Strikes / Mage's Spell Echo - both identical: a crit
    /// has a chance to immediately strike/cast again at 50% damage, same
    /// family as Berserker's Frenzy (`fire_frenzy`) but crit-gated instead
    /// of a flat per-attack chance. `_chance` is the trigger rate (15%/
    /// rank); `_dmg_pct` is the follow-up strike's damage share (0.50
    /// base). 0.0 without either invested.
    twin_strike_chance: f64,
    twin_strike_dmg_pct: f64,
    /// Mage's Finite Loop (renamed from "Infinite Loop" 2026-08-16 - see
    /// `passive_tree.rs`'s doc) - a REAL chain on top of Twin Strikes/
    /// Spell Echo's own single flat-chance follow-up above: once that
    /// first follow-up lands, each additional repeat rolls the SAME base
    /// `twin_strike_chance` again (2026-08-17 rework - no longer its own
    /// separate/boosted chance, a live request: "not increase the chance
    /// for echoing spells"), hard-capped at `finiteloop_max_repeats` (3/6/9
    /// at rank 1/2/3, restored from an earlier 1/2/3 retune) total extra
    /// hits so the chain can never run away regardless of luck. 0 without
    /// it invested - the base follow-up above is completely unaffected.
    finiteloop_max_repeats: u32,
    /// Rogue's Double Tap (2026-08-16) - Finite Loop's own mirror on Twin
    /// Strikes instead of Spell Echo, identical shape and identical
    /// 2026-08-17 rework: repeats reuse the base `twin_strike_chance`
    /// instead of their own separate chance, hard-capped at
    /// `doubletap_max_repeats` (3/6/9 at rank 1/2/3).
    doubletap_max_repeats: u32,
    /// Reentrancy guard for Mage's Volatile Magic (2026-08-16, a live
    /// crash fix - see `apply_splash`'s own doc for the full bug: a crit
    /// inside `apply_hit` triggers Volatile Magic's own `apply_splash`
    /// call with NO guard at all, and each of THAT splash's own targets
    /// independently rolls its own crit via the exact same `apply_hit`
    /// path, which could trigger Volatile Magic again, calling
    /// `apply_splash` again, unboundedly - genuine mutual recursion with
    /// no depth limit, the real cause of repeat `STATUS_STACK_OVERFLOW`
    /// crashes and of one live report of a single 6.7B-damage hit). Set
    /// true for the duration of `apply_splash`'s own resolution on this
    /// attacker; Volatile Magic's trigger check is skipped while true, so
    /// a splash target's own crit can still deal damage and still trigger
    /// its own Twin Strike/Finite Loop follow-up, just never its own
    /// NESTED Volatile Magic splash.
    in_splash_resolution: bool,
    /// Druid's Pack Instinct/Symbiosis - THIS unit's own granting
    /// magnitude (evasion/damage reduction respectively), applied live to
    /// whoever is currently the party's lowest-HP ally (see `apply_hit`'s
    /// computation) - not a bonus this unit ever grants itself. 0.0
    /// without either invested.
    own_pack_instinct_evasion_pct: f64,
    own_symbiosis_dr_pct: f64,
    /// Shared Strength - extra allies (beyond the base 1) Temple Guardian
    /// protects at once. 0 without it invested.
    sharedstrength_extra_targets: u32,
    /// Guardian Spirit (Temple Guardian, Monk) - periodic self-heal for
    /// whoever's currently protected, gated by its own cooldown (separate
    /// name from Cleric's unrelated Guardian Spirit ward). 0.0 without it
    /// invested.
    templeguardian_heal_pct: f64,
    next_templeguardian_heal_at_ms: u32,
    /// Lingering Effect (the 2026-08-15 Healing Power gear-affix rework) -
    /// THIS unit's own magnitude, applied to whoever they hit OR heal (see
    /// `apply_lingering_effect`'s doc - symmetric, DoT on a struck enemy,
    /// HoT on a healed ally). 0.0 without any rolled.
    lingering_effect_pct: f64,
    /// DoT/HoT instances currently ticking ON this unit, one per
    /// qualifying hit/heal that landed - "independent stacking" per a
    /// live design call, so several from different (or the same) source
    /// can be active at once, each with its own remaining ticks/timer,
    /// rather than one refreshing/replacing another's. Almost always
    /// empty.
    lingering_dots: Vec<LingeringDot>,
    /// The soonest `next_tick_at_ms` across every entry in `lingering_dots`
    /// - `u32::MAX` (never) when the list is empty, same "impossibly far
    /// away" convention as every other unused clock here. Recomputed by
    /// `apply_lingering_effect`/the main loop's tick handling any time
    /// `lingering_dots` changes, so the main loop's own soonest-event scan
    /// can treat this exactly like any other single per-unit clock instead
    /// of needing to know about the list at all.
    next_lingering_tick_at_ms: u32,
    /// Seed of Life (Druid only, 2026-08-16 rework - see passive_tree.rs's
    /// own doc) - THIS unit's own rate. Every time one of THEIR
    /// heal-flavor Lingering Effect ticks lands (see `tick_lingering_dots`),
    /// the target also gets a stacking shield worth this fraction of that
    /// tick's own amount, on top of the heal itself. 0.0 without any
    /// invested (harmless on every non-Druid archetype too - no
    /// "seedoflife" key exists outside Druid's own tree).
    seedoflife_shield_pct: f64,
    /// Wild Heart (Druid only, 2026-08-16 rework) - THIS unit's own rate.
    /// Any heal they land on someone ELSE also heals THEMSELVES for this
    /// fraction of the amount actually delivered - see `apply_heal`'s own
    /// hook. Self-heals don't double-dip (already "on the druid himself").
    /// 0.0 without any invested.
    wildheart_self_heal_pct: f64,
    /// Wild Instinct (Druid only, 2026-08-16 rework) - THIS unit's own
    /// rate. Any heal they land (`apply_heal`, any target including
    /// themselves) also grants that target a temporary, multiplicative
    /// damage-taken reduction for `WILDINSTINCT_DR_DURATION_MS` - reuses
    /// the same shared `temp_damage_reduction_bonus`/`_expires_at_ms`
    /// slot Guardian Spirit's Divine Intervention already writes to (last
    /// writer wins if both are somehow active on the same unit at once,
    /// same accepted "shared single-slot field" convention as every other
    /// party-broadcast bonus in this file). 0.0 without any invested.
    wildinstinct_dr_pct: f64,
    /// Druid's Wild Roar (2026-08-16 rework, replacing Living Bond's old
    /// "extends Symbiosis's DR" role) - charges banked for THIS fight,
    /// LINEAR with rank (1/2/3, per the node's own "1/2/3 times per
    /// fight" text). Triggers off ANY party member's death (see
    /// `apply_hit`'s death-trigger block): fears every alive enemy for
    /// `WILDROAR_FEAR_DURATION_MS`. 0 without it invested.
    wildroar_charges: u32,
    /// Druid's Nature's Embrace (2026-08-16 rework, replacing its old
    /// "heals the Symbiosis-protected ally periodically" role) - how many
    /// OTHER alive party members get instantly, fully healed every time a
    /// party member dies (same death-trigger site as Wild Roar above).
    /// 0 without it invested (1/2/3 by rank otherwise).
    naturesembrace_heal_targets: u32,
    /// Druid's Werebear (2026-08-16 rework, replacing Symbiosis's old
    /// "+DR to lowest-HP ally" role) - Thick Hide's cleanse cycle length,
    /// 0 without it invested (6000/5000/4000ms at rank 1/2/3 otherwise).
    /// Checked at THIS unit's own regular attack turn (see the main
    /// fight loop's player-turn branch) rather than a dedicated scheduled
    /// event - same "piggyback on the unit's own existing turn cadence"
    /// approximation Bloodpact's real cooldown already uses, since this
    /// unit's own attack interval is virtually always faster than 4-6s
    /// anyway. Clears `boss_focus_stacks` (the one enemy-inflicted debuff
    /// that actually exists as a stored per-unit field in this sim today -
    /// see this field's own doc) on Thick Hide's protected targets.
    thickhide_cycle_ms: u32,
    /// Next time (fight-clock ms) Thick Hide's cleanse is next eligible to
    /// fire - starts at 0 (eligible immediately on this unit's first turn).
    next_thickhide_cleanse_at_ms: u32,
    /// How many party members (the Druid themselves plus Rooted Network's
    /// extension) Thick Hide protects per cleanse. 0 without Werebear
    /// invested; 1 (self only) with just Werebear; Rooted Network adds
    /// more (its own rank, PLUS 1 more for every 100% splash the Druid
    /// has - see its own construction-site doc).
    thickhide_target_count: u32,
    /// Elemental damage rework (2026-08-15, a live request) - THIS unit's
    /// own rolled % for each of the 5 damage types, read once here from
    /// `Character::sum_affix` (see `Affix::ColdDamage`'s doc for the full
    /// mechanic). Each is a PROC CHANCE: on a landed hit this unit deals,
    /// rolled independently per type to debuff the target; on a landed
    /// heal this unit casts, rolled the same way to buff the target (or,
    /// for `divine_damage_pct`, to buff the healer - see
    /// `divine_heal_power_buff`'s doc) instead. 0.0 for any type never
    /// rolled.
    fire_damage_pct: f64,
    cold_damage_pct: f64,
    chaos_damage_pct: f64,
    lightning_damage_pct: f64,
    divine_damage_pct: f64,
    /// Fire's on-hit debuff on THIS unit (as the target) - each landed
    /// fire-damage hit against this unit that procs pushes one entry,
    /// the expiry timestamp (`at_ms + ELEMENTAL_PROC_DURATION_MS`) for
    /// THAT specific proc. Deliberately fully independent per-proc
    /// stacking (a live request: "it should be difficult to maintain a
    /// large stack") rather than one shared counter+expiry the way
    /// Slayer's Wound stacks work - every entry decays on its own clock,
    /// so sustaining N stacks needs N procs landing within the same
    /// rolling 4s window, not just one proc every so often. Read (and
    /// lazily pruned of expired entries) by `elemental_stack_count` -
    /// each active entry is worth 1% damage reduction reduced on this
    /// unit, floored so the combined stat can never drop below 25% (see
    /// `resolve_hit`'s own application). Empty for any unit nobody's
    /// ever fire-hit.
    fire_dr_debuff: Vec<u32>,
    /// Cold's on-hit debuff on THIS unit - same shape as `fire_dr_debuff`,
    /// reduces evasion instead (same 25% floor).
    cold_evasion_debuff: Vec<u32>,
    /// Chaos's on-hit debuff on THIS unit - same shape as `fire_dr_debuff`,
    /// reduces block chance instead (same 25% floor).
    chaos_block_debuff: Vec<u32>,
    /// Lightning's on-hit debuff on THIS unit - same independent-stacking
    /// shape as `fire_dr_debuff`, but the effect is "+1% damage taken
    /// from ALL sources" per active entry instead of a defense reduction,
    /// and it has an explicit stack cap (see `ELEMENTAL_LIGHTNING_MAX_STACKS`)
    /// rather than a floor - pushing past the cap while already at it is
    /// simply a no-op (see the push site in `apply_hit`).
    lightning_dmg_taken: Vec<u32>,
    /// Divine's on-hit debuff on THIS unit - same independent-stacking
    /// shape as `lightning_dmg_taken`, but reduces healing THIS unit
    /// receives instead (capped at `ELEMENTAL_DIVINE_ENEMY_MAX_STACKS` -
    /// 100 stacks/100%, since healing can't usefully go negative).
    divine_heal_reduction: Vec<u32>,
    /// Fire's on-HEAL buff on THIS unit (as the healed ally) - "a healer
    /// with one of these modifiers... increase the defenses of their
    /// allies with heals at the same rates" per the live request. Same
    /// independent-per-proc-stacking shape as the debuffs above, but
    /// ADDITIVE (not run through `combine_reduction_sources`) onto this
    /// unit's damage reduction, capped at the normal 75% ceiling instead
    /// of floored - see `resolve_hit`'s own application, which also folds
    /// this into `elemental_overflow_dmg_bonus` for whoever has real
    /// overflow-conversion invested (see that field's doc).
    fire_dr_buff: Vec<u32>,
    /// Cold's on-heal buff on THIS unit - same shape as `fire_dr_buff`,
    /// buffs evasion instead.
    cold_evasion_buff: Vec<u32>,
    /// Chaos's on-heal buff on THIS unit - same shape as `fire_dr_buff`,
    /// buffs block chance instead.
    chaos_block_buff: Vec<u32>,
    /// Divine's on-heal buff - unlike Fire/Cold/Chaos's ally-targeted
    /// buff above, this lands on the HEALER themselves (per the live
    /// request's own wording: "divine damage for healers will cause
    /// THEM to gain a buff"), boosting healing done rather than a
    /// defense stat. Originally capped at 200 stacks/200%, but per a
    /// 2026-08-16 follow-up ("make sure it has no limit on stacks") the
    /// cap was removed entirely - `roll_elemental_proc` is called with
    /// `usize::MAX` for this one, same as Fire/Cold/Chaos's self-limiting
    /// buffs/debuffs.
    divine_heal_power_buff: Vec<u32>,
    /// Warrior's Unbreakable/Druid's Shifting Form (the only two real
    /// OverflowConversion nodes whose input is Block/Evasion and whose
    /// output is IncreasedDamage) - THIS unit's own invested rate,
    /// snapshotted once here so `resolve_hit` can convert whatever
    /// marginal excess `fire_dr_buff`/`cold_evasion_buff`/
    /// `chaos_block_buff` pushes past the normal 75% cap into a live,
    /// temporary damage buff instead of just discarding it - "it can
    /// contribute to overflow stats for characters with overflow" per
    /// the live request. This is the first LIVE (mid-fight-reactive)
    /// overflow conversion in the sim - every other OverflowConversion
    /// node is a static pre-fight snapshot via `combined_stat_overflow`,
    /// since nothing else here changes a capped stat mid-fight the way
    /// this buff does. No DR-input equivalent exists in the tree today
    /// (nothing converts DamageReduction overflow into damage), so
    /// `fire_dr_buff`'s own excess is simply never converted - a real,
    /// narrower scope than the other two, not an oversight.
    block_overflow_dmg_rate: f64,
    evasion_overflow_dmg_rate: f64,
    /// Live damage buff granted by the marginal-overflow conversion
    /// above (see `block_overflow_dmg_rate`'s doc) - accumulates (not
    /// overwrites) across multiple pushes within the same window, reset
    /// to 0.0 once expired (lazy, same convention as every other timed
    /// field here). Read by `roll_attacker_damage` alongside every other
    /// live increased-damage source.
    elemental_overflow_dmg_bonus: f64,
    elemental_overflow_dmg_bonus_expires_at_ms: u32,
    /// Ranger's Volley / Mage's Chain Lightning (redesigned 2026-08-15
    /// around a live request, replacing the original "splash overflow
    /// converts to extra targets" text) - per rank, how much MORE damage
    /// this unit's primary attack deals for every target it's CAPABLE of
    /// reaching (1 primary + its own max splash target count, overflow
    /// bonus included) - not how many it actually hits this specific
    /// action, deliberately avoiding any live target-count tracking (see
    /// the main loop's own computation). 0.0 without either invested.
    volley_dmg_per_target_pct: f64,
    /// This action's own live `volley_dmg_per_target_pct * targets_reachable`
    /// result (2026-08-17 rework) - set fresh by the main loop right before
    /// each primary action, read by `resolve_hit` as one more additive term
    /// alongside `increased_damage` instead of a separate multiplicative
    /// pass (the OLD shape, which compounded on top of `increased_damage`
    /// rather than combining with it). 0.0 whenever Volley/Chain Lightning
    /// aren't invested, or between actions.
    splash_target_dmg_bonus: f64,
    /// Rogue's Exploit Weakness - this unit's own live crit-multiplier
    /// bonus against a target currently below 50% HP, computed directly
    /// in `resolve_hit` off `atk`/`def` alone (no external state needed,
    /// unlike Mark/Symbiosis). 0.0 without it invested.
    exploit_weakness_crit_mult_pct: f64,
    /// Vital Strike - raises Exploit Weakness's own "below X% HP" threshold
    /// (0.5 default, 0.65/0.80 with it invested).
    exploit_weakness_threshold: f64,
    /// Weak Point - +crit CHANCE (not multiplier) against the same
    /// below-threshold target Exploit Weakness already checks. 0.0 without
    /// it invested.
    weakpoint_crit_chance_pct: f64,
    /// Rogue's Nightstalker - this unit's own live evasion bonus
    /// specifically against a boss attacker, same "computed directly in
    /// `resolve_hit`" shape as Exploit Weakness. 0.0 without it invested.
    nightstalker_evasion_pct: f64,
    /// Coup de Grace - bonus crit multiplier on a guaranteed (force_crit)
    /// hit. 0.0 without it invested.
    assassinate_crit_mult_bonus: f64,
    /// Silent Blade - THIS unit's own magnitude, granted (via the shared
    /// `temp_evasion_buff` field Vanish also uses) after Assassinate's
    /// guaranteed crit lands. 0.0 without it invested.
    silentblade_evasion_pct: f64,
    /// Fadeaway - extends Vanish's own duration.
    fadeaway_duration_bonus_ms: u32,
    /// Backstab - THIS unit's own magnitude, granted as a one-shot bonus
    /// for the next hit while Vanish is active.
    backstab_dmg_pct: f64,
    /// Live one-shot Backstab bonus pending for THIS unit's next hit -
    /// consumed (cleared) by `apply_hit` right after `resolve_hit` reads
    /// it, same convention as `force_crit_next_hit`.
    backstab_pending_dmg_pct: f64,
    /// Smokescreen - THIS unit's own magnitude, granted to the lowest-HP
    /// ally alongside Vanish's own self buff.
    smokescreen_evasion_pct: f64,
    /// Marked for Death - how many of THIS unit's next hits are
    /// guaranteed crits (a live counter, decremented as they land).
    markedfordeath_hits_remaining: u32,
    /// Marked for Death - THIS unit's own magnitude: how many guaranteed
    /// crit hits a qualifying Cutthroat crit grants.
    markedfordeath_hit_count: u32,
    /// Final Cut - THIS unit's own magnitude, granted (self-only, via the
    /// shared `temp_party_attack_speed_bonus` field) on a Cutthroat-
    /// eligible kill.
    finalcut_speed_pct: f64,
    /// Mage's Empowered Bolt - guarantees this unit's first hit each fight
    /// crits (rank 2+); `empoweredbolt_crit_mult_bonus` is rank 3's extra
    /// crit damage on that same guaranteed hit.
    empoweredbolt_invested: bool,
    empoweredbolt_crit_mult_bonus: f64,
    /// Mage's Volatile Magic - fraction of a crit's damage also splashed
    /// to nearby enemies.
    volatilemagic_splash_pct: f64,
    /// Mage's Arcane Instability - adds `arcaneinstability_bonus_pct` to
    /// `crit_multiplier` above this HP threshold on the target (0.0 = not
    /// invested). Flat 0.65 at every invested rank (2026-08-17 rework -
    /// used to be rank-dependent, 65%/80%/inactive at rank 3/2/1; now the
    /// threshold is fixed and only the bonus scales per rank).
    arcaneinstability_threshold: f64,
    /// Mage's Arcane Instability - the flat crit-damage bonus itself
    /// (5%/9%/12% at rank 1/2/3), added to `crit_multiplier` when
    /// `arcaneinstability_threshold` is met. See that field's doc for the
    /// 2026-08-17 rework away from a `2.0x` doubling.
    arcaneinstability_bonus_pct: f64,
    /// Premeditation - chance to refund Assassinate's charge on a
    /// non-lethal use. 0.0 without it invested.
    premeditation_refund_chance: f64,
    /// Silent Steps - live evasion bonus per active Fleetfoot stack.
    stack_evasion_per_stack: f64,
    /// Hunter's Instinct - +crit chance when the target is a boss. 0.0
    /// without it invested.
    huntersinstinct_crit_vs_boss_pct: f64,
    /// Druid's Nature's Ward (2026-08-16 rework) - an extra multiplicative
    /// damage-reduction source, live-gated to only apply when the
    /// ATTACKER is a boss (same `if atk.is_boss { def.X } else { 0.0 }`
    /// pattern Hunter's Instinct/Nightstalker already use) - see
    /// `resolve_hit`'s own DR-sources list. 0.0 without it invested.
    naturesward_dr_vs_boss_pct: f64,
    /// Silent Killer - bonus damage on THIS unit's first hit landed
    /// against a boss this fight. 0.0 without it invested.
    silentkiller_dmg_pct: f64,
    /// Whether THIS unit has already landed a qualifying hit against a
    /// boss this fight (see `silentkiller_dmg_pct`'s doc) - read BEFORE
    /// being set, same "read stale state, then update" convention as
    /// `hits_landed_this_fight`.
    has_hit_boss_this_fight: bool,
    /// Rogue's Assassinate - charges banked for THIS fight (0 below rank
    /// 2, 1 at rank 2, 2 at rank 3 - same non-linear rank gate as
    /// Guardian Spirit/Undying Fury), each guaranteeing this unit's NEXT
    /// hit is a crit (consumed in `apply_hit`, before `resolve_hit` rolls
    /// anything). 0 without it invested.
    assassinate_charges: u32,
    /// Warlock's Dark Communion - this fraction of Soul Siphon's own
    /// leech heal ALSO goes to this unit's current lowest-HP ally (see
    /// the life-leech handling in `apply_hit`). 0.0 without it invested.
    dark_communion_pct: f64,
    /// Compassion - Merciful Touch's bounce prioritizes the lowest-HP
    /// ally (rank 2+); rank 3 also grants that ally a temporary DR buff.
    compassion_prioritize_lowest: bool,
    compassion_dr_pct: f64,
    /// Covenant - fraction of Dark Communion's own value also applied to
    /// the 2nd-lowest-HP ally (0.0/0.5/1.0 at rank <2/2/3).
    covenant_pct: f64,
    /// Unbreakable Bond - temporary DR granted to whoever Dark Communion
    /// heals.
    unbreakablebond_dr_pct: f64,
    /// Berserker's Vigor - a kill heals this unit for this fraction of
    /// their own max HP (Reckless Swing's trade itself is folded directly
    /// into `increased_damage`/`damage_reduction` at construction, not
    /// tracked as its own field - see the construction site). 0.0 without
    /// it invested.
    vigor_heal_pct: f64,
    /// Vengeful Blood - fraction of Vigor's heal also granted as a shield.
    /// 0.0 without it invested.
    vengefulblood_shield_pct: f64,
    /// Second Gale - how long a kill grants immunity to Reckless Swing/
    /// Death Wish's extra-damage-taken penalty for. 0 without it invested.
    secondgale_duration_ms: u32,
    /// Live expiry of Second Gale's immunity window, if currently active.
    temp_reckless_immunity_expires_at_ms: u32,
    /// The combined Reckless Swing + Death Wish "extra damage taken"
    /// penalty magnitude, snapshotted at construction purely so Second
    /// Gale's immunity window knows how much to cancel out (see
    /// `temp_reckless_immunity_expires_at_ms`'s doc) without needing to
    /// re-derive `combat_damage_reduction`'s own rank-matched helpers live.
    reckless_penalty_offset: f64,
    /// Last Laugh - Gambit's live missing-HP crit bonus doubles below 25%
    /// HP; at rank 3 it also doubles crit damage in that state. 0.0/false
    /// without it invested.
    lastlaugh_crit_bonus: bool,
    lastlaugh_crit_mult: bool,
    /// Rage Fueled - Gambit's unused crit chance (above 80% HP) converts
    /// to attack speed instead. 0.0 without it invested.
    ragefueled_speed_pct: f64,
    /// Warrior's Retaliation - chance for a hit THIS unit takes to
    /// trigger an immediate counter-attack against whoever hit them (see
    /// the boss-attack branch's own handling). `_dmg_pct`/`_heal_pct` are
    /// Vengeance/Bloodied Resolve's bonuses on that counter; `_below_25pct`
    /// is Last Stand's live below-25%-HP trigger-chance bonus. 0.0/0.0
    /// without it invested.
    retaliation_chance: f64,
    retaliation_dmg_pct: f64,
    retaliation_heal_pct: f64,
    retaliation_laststand_bonus: f64,
    /// Grudge - Vengeance's counter deals +5%/rank more damage for each
    /// prior LANDED hit THIS unit has taken from the SAME attacker this
    /// fight (capped at 5 stacks). Tracked per distinct attacker id, same
    /// shape as `recent_attackers` but a running count instead of a
    /// pruned window (never decays - "this fight", not "recently"). 0.0
    /// without it invested.
    grudge_pct_per_hit: f64,
    grudge_hit_counts: Vec<(String, u32)>,
    /// Executioner's Mark - a temporary crit-chance bonus applied ONLY for
    /// Retaliation's own counter-attack call (a one-off stat override on
    /// the scoped call, same convention Piercing Shots/Sanctified Touch
    /// already use). 0.0 without it invested.
    retaliation_crit_bonus: f64,
    /// Payback - Retaliation's counter always crits when the attacker is
    /// below this HP fraction (0.0 = not invested/no threshold).
    retaliation_payback_threshold: f64,
    /// One-shot "the next hit this unit lands is a guaranteed crit" flag -
    /// set by Payback right before Retaliation's counter-attack call,
    /// consumed (cleared) the moment `apply_hit` reads it, same spot
    /// Assassinate's own charge consumption already happens.
    force_crit_next_hit: bool,
    /// Adrenaline Surge - THIS unit's own magnitude, granted (via the
    /// shared `temp_party_attack_speed_bonus` field - a single-target
    /// write there is harmless) on a successful Retaliation. 0.0 without
    /// it invested.
    retaliation_surge_pct: f64,
    /// Hardened - a persistent (never-decaying) stacking DR bonus, one
    /// stack per successful Retaliation this fight, capped at 5.
    hardened_stacks: u32,
    hardened_pct_per_stack: f64,
    /// Second Wind (Warrior) - Retaliation's total trigger chance DOUBLES
    /// while THIS unit is below this HP fraction (0.0 = not invested).
    retaliation_secondwind_threshold: f64,
    /// Defiance/Berserk Vigor - live DR/increased-damage bonuses while
    /// THIS unit is below Last Stand's own 25%-HP threshold (matching the
    /// threshold `retaliation_laststand_bonus` itself already uses as
    /// "Last Stand active"). 0.0 without each invested.
    laststand_defiance_pct: f64,
    laststand_berserkvigor_pct: f64,
    /// Immovable - reduces damage taken from enemy CRITICAL hits by this
    /// much (a DR source only pushed in when `is_crit`). 0.0 without it
    /// invested.
    immovable_crit_dr_pct: f64,
    /// Endless Reserves - increases healing THIS unit receives FROM
    /// ALLIES (not self-heals) by this much. 0.0 without it invested.
    reserves_heal_received_pct: f64,
    /// Monk's Unbroken - THIS unit's own evasion-overflow-into-ignore
    /// conversion (see `Character::combat_unbroken_ignore_evasion_pct`'s
    /// doc), read live by `resolve_hit` off the ATTACKER to reduce
    /// whatever the DEFENDER's evasion check rolls against. 0.0 without
    /// it invested.
    unbroken_ignore_evasion_pct: f64,
    /// Crippling Grip (Last Bastion, 2026-08-17) - a second, independent
    /// conversion channel off the SAME evasion overflow Unbroken draws
    /// from, converting into a flat DR shred on whoever this unit hits
    /// instead. 0.0 without it invested. See `resolve_hit`'s own
    /// `sources` push for the consumption side.
    unbroken_crippling_grip_dr_pct: f64,
    /// Last Stand (Unyielding Spirit, 2026-08-17) - below this much HP,
    /// `unbroken_ignore_evasion_pct` doubles (capped at 75% total). Rank
    /// RAISES the threshold (0.35/0.45/0.55 at rank 1/2/3) rather than
    /// scaling the doubling itself. `0.0` (never triggers, since hp% is
    /// always > 0) without it invested.
    unyieldingspirit_threshold: f64,
    /// Mage's Frost Nova - a temporary evasion debuff currently affecting
    /// THIS unit (granted by someone else's splash - see `apply_splash`'s
    /// doc), same shape/convention as `temp_damage_reduction_bonus`
    /// (a flat value + its own expiry, lazily treated as expired).
    temp_evasion_debuff: f64,
    temp_evasion_debuff_expires_at_ms: u32,
    /// Mage's Frost Nova - THIS unit's own magnitude, applied to whoever
    /// their splash hits (see `apply_splash`). 0.0 without it invested.
    frostnova_evasion_debuff_pct: f64,
    /// Blizzard/Permafrost - Frost Nova's own duration, overriding the
    /// flat `FROSTNOVA_DEBUFF_DURATION_MS` constant.
    frostnova_duration_ms: u32,
    /// Absolute Zero - doubles Frost Nova's debuff against a target below
    /// this HP threshold (0.0 = not invested).
    absolutezero_threshold: f64,
    /// Static Field - THIS unit's own magnitude, applied as a temporary
    /// attack-speed debuff to whoever Chain Lightning's splash hits.
    staticfield_speed_debuff_pct: f64,
    /// Live attack-speed DEBUFF currently affecting THIS unit (from
    /// someone else's Static Field) - read as a divisor at the scheduling
    /// site, same lazy-expiry convention as every other temp_* field.
    temp_attack_speed_debuff: f64,
    temp_attack_speed_debuff_expires_at_ms: u32,
    /// Infernal Pact - self-heal per enemy Wildfire's splash hits.
    infernalpact_heal_pct: f64,
    /// Stormcaller - extra guaranteed splash targets for this unit's own
    /// primary attack (same shape as Storm of Arrows/Wider Burst).
    stormcaller_extra_targets: u32,
    /// Ranger's Piercing Shots - THIS unit's own flat crit-chance bonus
    /// for splash hits specifically (rank 3 only - a non-linear unlock,
    /// same "unlocked at rank 3" gate as Assassinate's own charges).
    /// Applied as a temporary override on `crit_chance` inside
    /// `apply_splash`, not a live resolve_hit param. 0.0 without it
    /// invested at rank 3.
    piercing_shots_crit_chance_bonus: f64,
    /// Wind Pierce (2026-08-16, Deadeye removed - see `combat_crit_chance`'s
    /// doc, it now boosts the main hit directly instead) - Piercing Shots'
    /// OWN splash crit chance, extra splash crit (same one-off override
    /// Piercing Shots' rank-3 bonus uses).
    windpierce_splash_crit_pct: f64,
    /// Armor Breaker - DR shred applied to whoever Piercing Shots' splash
    /// crits.
    armorbreaker_dr_shred_pct: f64,
    /// Scorched Earth - damage-dealt debuff applied to whoever Explosive
    /// Tips' splash hits.
    scorchedearth_dmg_debuff_pct: f64,
    /// True Strike - bonus crit chance against the PRIMARY target only
    /// (not splash), applied as a one-off override at the primary hit's
    /// own call site.
    truestrike_primary_crit_pct: f64,
    /// Storm of Arrows/Wider Burst - extra guaranteed splash targets for
    /// this unit's own primary attack.
    stormofarrows_extra_targets: u32,
    widerburst_extra_targets: u32,
    /// Monk's Inner Focus/Meditation/Chi Burst/Serenity - all four fire
    /// off the SAME "this unit successfully evaded a hit" trigger (see
    /// `apply_hit`'s evaded branch). `_heal_pct` is Inner Focus's own
    /// self-heal; `_meditation_bonus` is Meditation's PER-10%-evasion
    /// heal bonus (not the total - scaled live off this unit's own
    /// current evasion at the trigger site); `_chiburst_pct` is Chi
    /// Burst's share of that SAME heal sent to the lowest-HP ally;
    /// `_serenity_dr_pct` is Serenity's temporary DR grant. 0.0/0.0
    /// without each invested.
    inner_focus_heal_pct: f64,
    inner_focus_meditation_bonus: f64,
    inner_focus_chiburst_pct: f64,
    inner_focus_serenity_dr_pct: f64,
    /// Rising Tide - self healing-power buff on Inner Focus's trigger.
    risingtide_heal_power_pct: f64,
    /// Wide Circle - extra Chi Burst heal targets beyond the base 1.
    widecircle_extra_targets: u32,
    /// Harmonize - temporary DR granted to whoever Chi Burst heals.
    harmonize_dr_pct: f64,
    /// Unshakable/Unmovable - Serenity's own duration/magnitude,
    /// overriding the flat `SERENITY_DR_DURATION_MS` constant.
    serenity_dr_duration_ms: u32,
    /// Clarity - whether Inner Focus also triggers on a blocked hit.
    clarity_triggers_on_block: bool,
    /// Rogue's Voidstep/Monk's Counterflow/Druid's Wild Fury - all three
    /// share this one field (mutually exclusive by archetype, same
    /// convention as `own_pack_instinct_evasion_pct`/Temple Guardian): a
    /// successful evade has this chance to trigger an immediate free
    /// counter-attack, modeled directly on Warrior's Retaliation (see that
    /// block at the end of `apply_hit`) but fired from the evaded branch
    /// instead, gated on `outcome.evaded` rather than `!outcome.evaded`.
    /// 0.0 without any of the three invested.
    evade_counter_chance: f64,
    /// When the evade-counter above last actually fired (0 = never yet
    /// this fight) - the trigger below requires a full 1000ms to have
    /// passed since this before rolling again, capping Voidstep/
    /// Counterflow/Wild Fury at 1 real counter-attack per second no
    /// matter how many evades land in that window (a live request).
    evade_counter_last_fired_at_ms: u32,
    /// Party-wide temporary buffs, broadcast on a trigger to every alive
    /// non-boss unit - same `for u in units.iter_mut() { if !u.is_boss &&
    /// u.alive { ... } }` idiom Guardian Spirit's Final Blessing/Paladin's
    /// Consecration already use for `temp_heal_power_bonus`/shields, just
    /// two more stats. Lazy-expiry read at whichever site the self-only
    /// version of that stat is already read (`roll_attacker_damage`'s
    /// increased-damage multiplier; the scheduling site's speed multiplier
    /// product). Berserker's Warlord's Resolve/Slayer's Warlord's Resolve
    /// write `temp_party_increased_damage_bonus`; Slayer's War Cry writes
    /// `temp_party_attack_speed_bonus`.
    temp_party_attack_speed_bonus: f64,
    temp_party_attack_speed_bonus_expires_at_ms: u32,
    temp_party_increased_damage_bonus: f64,
    temp_party_increased_damage_bonus_expires_at_ms: u32,
    /// Paladin's Unwavering / Cleric's Unyielding Faith (2026-08-17) -
    /// same party-broadcast idiom as `temp_party_increased_damage_bonus`
    /// above, a SEPARATE field from the self/ally-targeted
    /// `temp_damage_reduction_bonus` (that one's already dual-purpose,
    /// signed for both buffs and debuffs - reusing it for a party
    /// broadcast would be an unrelated third meaning on one field).
    temp_party_damage_reduction_bonus: f64,
    temp_party_damage_reduction_bonus_expires_at_ms: u32,
    /// Berserker's Warlord's Resolve - THIS unit's own magnitude, broadcast
    /// to the party (see `temp_party_increased_damage_bonus`'s doc) every
    /// time their Bloodlust stacks are at max. 0.0 without it invested.
    warlord_party_dmg_pct: f64,
    /// Paladin's Unwavering / Cleric's Unyielding Faith (2026-08-17) -
    /// THIS unit's own party-DR-grant total (Paladin: vowofprotection +
    /// beaconoflight + hallowedground; Cleric: sanctuary +
    /// consecratedearth + wardingprayer - mutually exclusive by
    /// archetype), broadcast to the whole party (doubled, see the trigger
    /// site) while this unit is below `low_hp_party_dr_threshold`. 0.0
    /// without the respective modifier invested.
    low_hp_party_dr_pct: f64,
    /// Below this much of THIS unit's own HP, the broadcast above fires -
    /// 0.50 at rank 1/2, 0.65 at rank 3. `0.0` (never triggers, since hp%
    /// is always > 0) without it invested.
    low_hp_party_dr_threshold: f64,
    /// Slayer's War Cry - THIS unit's own magnitude, broadcast to the
    /// party (see `temp_party_attack_speed_bonus`'s doc) on every
    /// FlickerStrike dash. 0.0 without it invested.
    warcry_party_speed_pct: f64,
    /// Berserker's Neverending - when invested, `add_speed_stack` switches
    /// Bloodlust's stacks from the default "all decay together the moment
    /// the shared expiry lapses" to genuinely independent per-stack
    /// timestamps (see `bloodlust_stack_expiries`), so stacks fall off one
    /// at a time as each one's own window closes. `false` (default
    /// all-at-once behavior) without it invested - every other stack
    /// investor (Momentum/Fleetfoot/Relentless Pursuit/Flow State) is
    /// unaffected either way since this only ever gets set `true` for a
    /// Berserker with Bloodlust+Neverending.
    neverending_invested: bool,
    bloodlust_stack_expiries: Vec<u32>,
    /// Rogue's Opportunist (redesigned 2026-08-16 per a live design call -
    /// see the module doc's history) - THIS unit's own first N hits each
    /// fight are guaranteed to land, N = the skill's own rank. 0 without
    /// it invested.
    opportunist_guaranteed_hits: u32,
    /// How many of THIS unit's own hits have landed so far this fight -
    /// tracked for everyone (harmless/unused without Opportunist), read
    /// live by `resolve_hit` against `opportunist_guaranteed_hits`.
    hits_landed_this_fight: u32,
    /// Ambush (Opportunist's "final leaf") - fraction of the target's
    /// damage reduction a guaranteed-landing hit ignores. 0.0 without it
    /// invested.
    ambush_dr_cut_pct: f64,
    /// Opening Move - cooldown between recharges of Opportunist's
    /// guaranteed-landing treatment, independent of the fight-opening
    /// budget itself. 0 without it invested.
    openingmove_cooldown_ms: u32,
    next_openingmove_at_ms: u32,
    /// Cold Steel - THIS unit's own chance for a guaranteed-landing hit
    /// they land to leave a pass-along debuff on the target.
    coldsteel_pass_chance: f64,
    /// Live pending Cold Steel debuff currently on THIS (defender) unit,
    /// left by an earlier guaranteed-landing hit from any ally - the next
    /// hit against them (from anyone) rolls `coldsteel_pass_chance_pending`
    /// to also get the treatment, using `coldsteel_ambush_pct_pending` as
    /// the DR-cut value (the ORIGINAL rogue's own Ambush investment, not
    /// necessarily the consuming attacker's).
    coldsteel_pending: bool,
    coldsteel_pass_chance_pending: f64,
    coldsteel_ambush_pct_pending: f64,
    /// Predator - THIS unit's own magnitude: an enemy struck by one of
    /// this unit's guaranteed-landing hits takes more damage from
    /// EVERYONE for a few seconds.
    predator_dmg_taken_pct: f64,
    /// Live Predator mark currently on THIS unit (from anyone's
    /// investment) - read unconditionally, same "any attacker benefits"
    /// shape as `curse_dmg_taken_bonus`.
    predator_dmg_taken_bonus: f64,
    predator_expires_at_ms: u32,
    /// Cutthroat - a crit against a target below 25% HP deals this much
    /// more damage. 0.0 without it invested.
    cutthroat_low_hp_dmg_pct: f64,
    /// Vanish - THIS unit's own magnitude, granted as `temp_evasion_buff`
    /// (see that field's doc) on landing a crit. 0.0 without it invested.
    vanish_evasion_pct: f64,
    /// A temporary evasion BUFF currently affecting THIS unit (from
    /// Vanish, granted on their own crit) - a positive source fed
    /// straight into `combine_reduction_sources` alongside `evasion`
    /// itself, same lazy-expiry convention as `temp_evasion_debuff` (which
    /// is the negative/DEBUFF counterpart of this, from Frost Nova).
    temp_evasion_buff: f64,
    temp_evasion_buff_expires_at_ms: u32,
    /// Vanishing Shot - THIS unit's own magnitude, granted (via
    /// `temp_crit_chance_buff`) on a successful evade.
    vanishingshot_crit_pct: f64,
    temp_crit_chance_buff: f64,
    temp_crit_chance_buff_expires_at_ms: u32,
    /// Fleeting Shadow - THIS unit's own magnitude, granted (via the
    /// shared `temp_party_attack_speed_bonus` field, single-target write)
    /// on a successful evade.
    fleetingshadow_speed_pct: f64,
}

/// Test-only zeroed baseline (2026-08-17, Phase 2) - `CombatSimUnit` has
/// no real `Default` (430 fields, deliberately: every REAL construction
/// site must be explicit about every stat, so a real fight can never
/// silently inherit a zero it didn't mean to). This impl is `#[cfg(test)]`-
/// gated specifically so it can never leak into a real build and can
/// never accidentally become a shortcut for production code - it exists
/// purely so tests can write `CombatSimUnit { id: "x".into(), damage_reduction:
/// 0.5, ..Default::default() }` instead of a 430-field literal, matching
/// this codebase's existing fixed-seed test conventions elsewhere.
#[cfg(test)]
impl Default for CombatSimUnit {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            is_boss: false,
            archetype: None,
            spawned_at_ms: 0,
            role: None,
            hp: 0,
            max_hp: 0,
            atk: 0,
            heal_power: 0.0,
            intervene: 0.0,
            attack_interval_ms: 0,
            next_action_at_ms: 0,
            alive: false,
            helm_power: 0.0,
            helm_cooldown_ms: 0,
            next_helm_at_ms: 0,
            helm_stack_bonus: 0.0,
            boots_power: 0.0,
            boots_cooldown_ms: 0,
            next_boots_at_ms: 0,
            damage_reduction: 0.0,
            block_chance: 0.0,
            evasion: 0.0,
            increased_damage: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 0.0,
            splash: 0.0,
            late_stage_damage_penalty_pct: 0.0,
            boss_focus_stacks: 0.0,
            boss_ability: None,
            next_ability_at_ms: 0,
            boss_dynamic_power_mult: 0.0,
            cthulhu_debuff_stacks: 0,
            cthulhu_debuff_expires_at_ms: 0,
            cthulhu_debuff_pct_per_stack: 0.0,
            cube_shred_stacks: 0,
            cube_shred_expires_at_ms: 0,
            damage_dealt_total: 0,
            level: 0,
            life_leech_pct: 0.0,
            leech_window_start_ms: 0,
            leech_gained_in_window: 0.0,
            skills: Vec::new(),
            skill_stacks: HashMap::new(),
            next_flicker_at_ms: 0,
            has_celestial_conversion: false,
            wound_deal_leech_per_stack: 0.0,
            wound_deal_max_stacks: 0,
            wound_deal_duration_ms: 0,
            wound_deal_damage_dealt_debuff: 0.0,
            wound_deal_heal_received_debuff: 0.0,
            wound_deal_explosion_pct: 0.0,
            wound_deal_explosion_self_leech_pct: 0.0,
            wound_deal_explosion_extra_targets: 0,
            wound_deal_spreads_to_splash: false,
            contagion_chance: 0.0,
            gravechill_speed_debuff_pct: 0.0,
            plaguebearer_extra_targets: 0,
            wound_stacks: 0,
            wound_max_stacks: 0,
            wound_expires_at_ms: 0,
            wound_leech_per_stack: 0.0,
            wound_damage_dealt_debuff: 0.0,
            wound_heal_received_debuff: 0.0,
            wound_damage_taken_total: 0.0,
            flicker_cooldown_ms: 0,
            next_bloodpact_at_ms: 0,
            bloodpact_last_fired_at_ms: 0,
            bloodpact_cooldown_ms: 0,
            bloodpact_uses_this_fight: 0,
            bloodpact_triage_pct: 0.0,
            bloodpact_finaloffering_min_prior_uses: 0,
            bloodpact_finaloffering_pct: 0.0,
            bloodpact_warlordsresolve_pct: 0.0,
            bloodpact_cleanslate_reset_chance: 0.0,
            bloodpact_secondwind_reset_chance: 0.0,
            bloodpact_hp_cost_pct: 0.0,
            bloodpact_damage_mult: 0.0,
            bloodpact_martyrdom_shield_pct: 0.0,
            bloodpact_kill_refund_pct: 0.0,
            bloodpact_nonlethal_refund_pct: 0.0,
            bloodpact_bloodforblood_pct: 0.0,
            shield_hp: 0.0,
            shield_expires_at_ms: 0,
            shield_reflect_pct: 0.0,
            shield_reflect_chance: 0.0,
            shield_reflect_requires_full_absorb: false,
            guardian_spirit_charges: 0,
            guardian_spirit_heal_pct: 0.0,
            guardian_spirit_save_dr_pct: 0.0,
            guardian_spirit_save_heal_power_pct: 0.0,
            verdantburst_charges: 0,
            temp_heal_power_bonus: 0.0,
            temp_heal_power_bonus_expires_at_ms: 0,
            eternallight_bonus_pct: 0.0,
            temp_damage_reduction_bonus: 0.0,
            temp_damage_reduction_bonus_expires_at_ms: 0,
            overflow_grace_shield_pct: 0.0,
            overflow_grace_shield_duration_ms: 0,
            overflow_grace_shield_dr_pct: 0.0,
            heal_crit_bonus_mult: 0.0,
            heal_crit_chance_bonus: 0.0,
            heal_crit_splash_pct: 0.0,
            grace_lowest_ally_bonus_pct: 0.0,
            prayer_chance: 0.0,
            prayer_bounce_targets: 0,
            prayer_bounce_value_pct: 0.0,
            unbroken_prayer_chance: 0.0,
            divine_favor_shield_pct: 0.0,
            divine_favor_shield_duration_ms: 0,
            healing_touch_pct: 0.0,
            crit_shield_max_hp_pct: 0.0,
            soul_harvest_heal_pct: 0.0,
            darkritual_dmg_pct: 0.0,
            eternal_hunger_shield_pct: 0.0,
            divine_shield_amount_pct: 0.0,
            divine_shield_cooldown_ms: 0,
            next_divine_shield_at_ms: 0,
            consecration_shield_pct: 0.0,
            communion_heal_power_pct: 0.0,
            purify_dmg_debuff_pct: 0.0,
            lastjudgment_skip_chance: 0.0,
            consecration_shield_duration_ms: 0,
            smite_heal_pct: 0.0,
            smite_zealotry_bonus_pct: 0.0,
            smite_extra_targets: 0,
            zealotry_martyrscall_bonus_pct: 0.0,
            zealotry_risingfervor_pct_per_ally: 0.0,
            zealotry_guardianswrath_speed_pct: 0.0,
            zealotry_guardianswrath_speed_bonus: 0.0,
            zealotry_guardianswrath_expires_at_ms: 0,
            smite_judgment_bonus_pct: 0.0,
            judgment_threshold: 0.0,
            smite_holyfire_dmg_pct: 0.0,
            purgingflame_heal_reduction_pct: 0.0,
            temp_heal_reduction_pct: 0.0,
            temp_heal_reduction_expires_at_ms: 0,
            executionersblessing_heal_pct: 0.0,
            wrathoftheheavens_chance: 0.0,
            unyieldingroots_cycle_ms: 0,
            gambit_crit_per_missing_20pct: 0.0,
            deathdefiant_grace_ms: 0,
            deathdefiant_frozen_crit_bonus: 0.0,
            deathdefiant_frozen_crit_bonus_expires_at_ms: 0,
            bramble_reflect_pct: 0.0,
            poison_thorns_debuff_pct: 0.0,
            entangle_chance: 0.0,
            recent_attackers: Vec::new(),
            temp_damage_dealt_debuff: 0.0,
            temp_damage_dealt_debuff_expires_at_ms: 0,
            frenzy_strike_chance: 0.0,
            frenzy_extra_hits: 0,
            frenzy_bloodscent_threshold: 0.0,
            frenzy_dr_shred_pct: 0.0,
            frenzy_extra_dmg_pct: 0.0,
            frenzy_culling_threshold: 0.0,
            frenzy_heal_pct: 0.0,
            frenzy_shield_chance: 0.0,
            frenzy_undying_charges: 0,
            frenzy_chain_chance: 0.0,
            frenzy_chain_max_extra: 0,
            spike_barrier_reflect_pct: 0.0,
            aegis_shield_pct: 0.0,
            aegis_shield_duration_ms: 0,
            aegis_rally_speed_pct: 0.0,
            aegis_extra_targets: 0,
            thornedhide_pct_per_stack: 0.0,
            thornedhide_stacks: 0,
            thornedhide_expires_at_ms: 0,
            thornedhide_debuff_pct_per_stack: 0.0,
            spike_retribution_chance: 0.0,
            spike_unyielding_chance: 0.0,
            block_damage_reduction_pct: 0.0,
            stonewall_auto_block_hits: 0,
            hits_taken_this_fight: 0,
            stack_speed_per_stack: 0.0,
            stack_dmg_per_stack: 0.0,
            stack_avalanche_dmg_per_stack: 0.0,
            stack_crit_per_stack: 0.0,
            shatter_shred_pct: 0.0,
            overwhelm_shred_linger_ms: 0,
            crush_dr_threshold: 0.0,
            stack_splash_per_stack: 0.0,
            windfury_chance: 0.0,
            stack_shred_per_stack: 0.0,
            stack_speed_max_stacks: 0,
            stack_speed_duration_ms: 0,
            stack_speed_current: 0,
            stack_speed_expires_at_ms: 0,
            flowing_speed_per_stack: 0.0,
            flowing_crit_per_stack: 0.0,
            flowing_max_stacks: 0,
            flowing_duration_ms: 0,
            risingstorm_dmg_pct: 0.0,
            nervestrike_crit_mult_bonus: 0.0,
            vitalpoints_shred_per_stack: 0.0,
            eternalflow_bonus_stacks: 0,
            onehundredhands_bonus_stacks: 0,
            stormfront_splash_pct: 0.0,
            flowing_current: 0,
            flowing_expires_at_ms: 0,
            flowing_last_target: 0,
            chakra_of_many_pct: 0.0,
            chakra_of_light_pct: 0.0,
            chakraoflife_duration_ms: 0,
            chakraoflife_immune_until_ms: 0,
            next_chakraoflife_expiry_at_ms: 0,
            own_mark_crit_chance: 0.0,
            own_mark_crit_mult: 0.0,
            own_mark_low_hp_dmg: 0.0,
            own_mark_ally_crit_chance: 0.0,
            own_mark_ally_dmg_pct: 0.0,
            own_mark_ally_crit_mult: 0.0,
            own_mark_spread_count: 0,
            killzone_threshold: 0.0,
            cleankill_remark_chance: 0.0,
            huntersreward_heal_pct: 0.0,
            own_curse_dmg_taken: 0.0,
            own_curse_spread_count: 0,
            own_doom_detonate_pct: 0.0,
            own_curse_heal_reduction_pct: 0.0,
            own_curse_spread_bonus_pct: 0.0,
            own_soul_stone_max: 0,
            own_cursed_blood_target_count: 0,
            own_dreadfuldeath_shred_pct: 0.0,
            own_apocalypse_splash_pct: 0.0,
            has_applied_mark_this_fight: false,
            mark_source_id: None,
            mark_crit_chance_bonus: 0.0,
            mark_crit_multiplier_bonus: 0.0,
            mark_low_hp_damage_bonus: 0.0,
            mark_ally_crit_chance_bonus: 0.0,
            mark_ally_dmg_bonus: 0.0,
            mark_ally_crit_multiplier_bonus: 0.0,
            curse_dmg_taken_bonus: 0.0,
            soul_stones: 0,
            soul_stone_uses_this_fight: 0,
            curse_expires_at_ms: 0,
            next_curse_expiry_at_ms: 0,
            curse_damage_taken_total: 0.0,
            curse_detonate_pct: 0.0,
            curse_source_id: None,
            curse_heal_reduction_bonus: 0.0,
            fel_rush_speed_bonus: 0.0,
            fel_rush_duration_ms: 0,
            ravage_stack_pct: 0.0,
            fel_rush_stacks: 0,
            fel_rush_expires_at_ms: 0,
            early_fight_speed_bonus_pct: 0.0,
            early_fight_speed_window_end_ms: 0,
            flicker_frenzy_speed_bonus: 0.0,
            unrelenting_duration_bonus_ms: 0,
            adrenaline_crit_mult_bonus: 0.0,
            chainreaper_heal_pct: 0.0,
            deathspiral_heal_pct: 0.0,
            insatiable_extend_chance: 0.0,
            secondheartbeat_chance: 0.0,
            overflowvessel_shield_pct: 0.0,
            flicker_frenzy_expires_at_ms: 0,
            endless_thirst_cap_bonus: 0.0,
            endless_thirst_uncapped: false,
            endless_thirst_expires_at_ms: 0,
            reapers_momentum_per_kill: 0,
            reapers_momentum_banked: 0,
            attack_speed_pct: 0.0,
            speed_overflow_dmg_pct: 0.0,
            speed_overflow_crit_pct: 0.0,
            speed_overflow_threshold: 0.0,
            unbreakable_faith_heal_pct: 0.0,
            eternalvow_shield_chance: 0.0,
            graciousburden_heal_pct: 0.0,
            bondeddevotion_dr_pct: 0.0,
            bondeddevotion_duration_ms: 0,
            twin_strike_chance: 0.0,
            twin_strike_dmg_pct: 0.0,
            finiteloop_max_repeats: 0,
            doubletap_max_repeats: 0,
            in_splash_resolution: false,
            own_pack_instinct_evasion_pct: 0.0,
            own_symbiosis_dr_pct: 0.0,
            sharedstrength_extra_targets: 0,
            templeguardian_heal_pct: 0.0,
            next_templeguardian_heal_at_ms: 0,
            lingering_effect_pct: 0.0,
            lingering_dots: Vec::new(),
            next_lingering_tick_at_ms: 0,
            seedoflife_shield_pct: 0.0,
            wildheart_self_heal_pct: 0.0,
            wildinstinct_dr_pct: 0.0,
            wildroar_charges: 0,
            naturesembrace_heal_targets: 0,
            thickhide_cycle_ms: 0,
            next_thickhide_cleanse_at_ms: 0,
            thickhide_target_count: 0,
            fire_damage_pct: 0.0,
            cold_damage_pct: 0.0,
            chaos_damage_pct: 0.0,
            lightning_damage_pct: 0.0,
            divine_damage_pct: 0.0,
            fire_dr_debuff: Vec::new(),
            cold_evasion_debuff: Vec::new(),
            chaos_block_debuff: Vec::new(),
            lightning_dmg_taken: Vec::new(),
            divine_heal_reduction: Vec::new(),
            fire_dr_buff: Vec::new(),
            cold_evasion_buff: Vec::new(),
            chaos_block_buff: Vec::new(),
            divine_heal_power_buff: Vec::new(),
            block_overflow_dmg_rate: 0.0,
            evasion_overflow_dmg_rate: 0.0,
            elemental_overflow_dmg_bonus: 0.0,
            elemental_overflow_dmg_bonus_expires_at_ms: 0,
            volley_dmg_per_target_pct: 0.0,
            splash_target_dmg_bonus: 0.0,
            exploit_weakness_crit_mult_pct: 0.0,
            exploit_weakness_threshold: 0.0,
            weakpoint_crit_chance_pct: 0.0,
            nightstalker_evasion_pct: 0.0,
            assassinate_crit_mult_bonus: 0.0,
            silentblade_evasion_pct: 0.0,
            fadeaway_duration_bonus_ms: 0,
            backstab_dmg_pct: 0.0,
            backstab_pending_dmg_pct: 0.0,
            smokescreen_evasion_pct: 0.0,
            markedfordeath_hits_remaining: 0,
            markedfordeath_hit_count: 0,
            finalcut_speed_pct: 0.0,
            empoweredbolt_invested: false,
            empoweredbolt_crit_mult_bonus: 0.0,
            volatilemagic_splash_pct: 0.0,
            arcaneinstability_threshold: 0.0,
            arcaneinstability_bonus_pct: 0.0,
            premeditation_refund_chance: 0.0,
            stack_evasion_per_stack: 0.0,
            huntersinstinct_crit_vs_boss_pct: 0.0,
            naturesward_dr_vs_boss_pct: 0.0,
            silentkiller_dmg_pct: 0.0,
            has_hit_boss_this_fight: false,
            assassinate_charges: 0,
            dark_communion_pct: 0.0,
            compassion_prioritize_lowest: false,
            compassion_dr_pct: 0.0,
            covenant_pct: 0.0,
            unbreakablebond_dr_pct: 0.0,
            vigor_heal_pct: 0.0,
            vengefulblood_shield_pct: 0.0,
            secondgale_duration_ms: 0,
            temp_reckless_immunity_expires_at_ms: 0,
            reckless_penalty_offset: 0.0,
            lastlaugh_crit_bonus: false,
            lastlaugh_crit_mult: false,
            ragefueled_speed_pct: 0.0,
            retaliation_chance: 0.0,
            retaliation_dmg_pct: 0.0,
            retaliation_heal_pct: 0.0,
            retaliation_laststand_bonus: 0.0,
            grudge_pct_per_hit: 0.0,
            grudge_hit_counts: Vec::new(),
            retaliation_crit_bonus: 0.0,
            retaliation_payback_threshold: 0.0,
            force_crit_next_hit: false,
            retaliation_surge_pct: 0.0,
            hardened_stacks: 0,
            hardened_pct_per_stack: 0.0,
            retaliation_secondwind_threshold: 0.0,
            laststand_defiance_pct: 0.0,
            laststand_berserkvigor_pct: 0.0,
            immovable_crit_dr_pct: 0.0,
            reserves_heal_received_pct: 0.0,
            unbroken_ignore_evasion_pct: 0.0,
            unbroken_crippling_grip_dr_pct: 0.0,
            unyieldingspirit_threshold: 0.0,
            temp_evasion_debuff: 0.0,
            temp_evasion_debuff_expires_at_ms: 0,
            frostnova_evasion_debuff_pct: 0.0,
            frostnova_duration_ms: 0,
            absolutezero_threshold: 0.0,
            staticfield_speed_debuff_pct: 0.0,
            temp_attack_speed_debuff: 0.0,
            temp_attack_speed_debuff_expires_at_ms: 0,
            infernalpact_heal_pct: 0.0,
            stormcaller_extra_targets: 0,
            piercing_shots_crit_chance_bonus: 0.0,
            windpierce_splash_crit_pct: 0.0,
            armorbreaker_dr_shred_pct: 0.0,
            scorchedearth_dmg_debuff_pct: 0.0,
            truestrike_primary_crit_pct: 0.0,
            stormofarrows_extra_targets: 0,
            widerburst_extra_targets: 0,
            inner_focus_heal_pct: 0.0,
            inner_focus_meditation_bonus: 0.0,
            inner_focus_chiburst_pct: 0.0,
            inner_focus_serenity_dr_pct: 0.0,
            risingtide_heal_power_pct: 0.0,
            widecircle_extra_targets: 0,
            harmonize_dr_pct: 0.0,
            serenity_dr_duration_ms: 0,
            clarity_triggers_on_block: false,
            evade_counter_chance: 0.0,
            evade_counter_last_fired_at_ms: 0,
            temp_party_attack_speed_bonus: 0.0,
            temp_party_attack_speed_bonus_expires_at_ms: 0,
            temp_party_increased_damage_bonus: 0.0,
            temp_party_increased_damage_bonus_expires_at_ms: 0,
            temp_party_damage_reduction_bonus: 0.0,
            temp_party_damage_reduction_bonus_expires_at_ms: 0,
            warlord_party_dmg_pct: 0.0,
            low_hp_party_dr_pct: 0.0,
            low_hp_party_dr_threshold: 0.0,
            warcry_party_speed_pct: 0.0,
            neverending_invested: false,
            bloodlust_stack_expiries: Vec::new(),
            opportunist_guaranteed_hits: 0,
            hits_landed_this_fight: 0,
            ambush_dr_cut_pct: 0.0,
            openingmove_cooldown_ms: 0,
            next_openingmove_at_ms: 0,
            coldsteel_pass_chance: 0.0,
            coldsteel_pending: false,
            coldsteel_pass_chance_pending: 0.0,
            coldsteel_ambush_pct_pending: 0.0,
            predator_dmg_taken_pct: 0.0,
            predator_dmg_taken_bonus: 0.0,
            predator_expires_at_ms: 0,
            cutthroat_low_hp_dmg_pct: 0.0,
            vanish_evasion_pct: 0.0,
            temp_evasion_buff: 0.0,
            temp_evasion_buff_expires_at_ms: 0,
            vanishingshot_crit_pct: 0.0,
            temp_crit_chance_buff: 0.0,
            temp_crit_chance_buff_expires_at_ms: 0,
            fleetingshadow_speed_pct: 0.0,
        }
    }
}

/// One active Lingering Effect instance - see `lingering_dots`' doc.
/// Lingering Effect is symmetric: a landed HIT leaves a damage-over-time
/// debuff on the enemy struck (`is_heal: false`), and a landed HEAL
/// leaves an EQUIVALENT heal-over-time buff on the ally healed
/// (`is_heal: true`) - same 80-tick/4-second shape either way, just
/// applied as healing instead of damage (see `apply_lingering_effect`'s
/// doc for both spawn sites). A Druid with Seed of Life invested (see
/// `seedoflife_shield_pct`) ALSO gets a stacking shield alongside each
/// heal-flavor tick, at their own configured rate - a Druid-specific
/// addition on top of this base mechanic, not a change to it; every
/// other source's heal-flavor ticks are still plain direct hp
/// restoration. `remaining_ticks` counts DOWN (starts at
/// `LINGERING_EFFECT_TICKS`); `amount_per_tick` is fixed at creation
/// (computed from the triggering action's own pre-mitigation
/// damage/heal amount), but for the DAMAGE flavor the target's flat
/// damage reduction is re-read fresh at EACH tick, not baked in up
/// front, so a mid-DoT change to the target's own mitigation still
/// matters for whatever ticks haven't landed yet - the HEAL flavor has
/// no equivalent mitigation concept, so its ticks land at full value.
#[derive(Clone)]
pub(crate) struct LingeringDot {
    source_id: String,
    amount_per_tick: f64,
    remaining_ticks: u32,
    next_tick_at_ms: u32,
    is_heal: bool,
}

/// Safety valve — if a fight somehow hasn't resolved by this point (e.g.
/// an all-Support roster that can never damage the boss), it just ends
/// as a loss rather than looping forever.
pub(crate) const MAX_FIGHT_DURATION_MS: u32 = 90_000;

/// Flat multiplier applied to a hit's damage when the defender's block
/// roll succeeds (see `resolve_hit`) - a block halves the hit, it
/// doesn't reduce it by the defender's own block CHANCE value.
pub(crate) const BLOCK_DAMAGE_REDUCTION: f64 = 0.5;
/// Ceiling on Slayer's life leech (see `ArchetypeBonus::life_leech_pct`) -
/// no more than this fraction of the leecher's own max hp can be regained
/// from it in any trailing 1-second window, no matter how much raw damage
/// is actually dealt in that window. See `apply_hit`'s leech handling.
pub const LIFE_LEECH_CAP_PER_SEC: f64 = 0.20;
/// How many OTHER alive enemies a player's splash also hits, on top of
/// the primary target - a fixed cap, not scaled by the splash % itself
/// (see `apply_splash`).
pub(crate) const PLAYER_SPLASH_MAX_TARGETS: usize = 2;
/// Same idea in reverse - a boss with splash (its own "cleave") hits up
/// to this many extra players, kept lower than the player-side cap since
/// this is a threat, not a reward.
pub(crate) const ENEMY_SPLASH_MAX_TARGETS: usize = 1;
/// Gelatinous Cube's splash - 4 ADDITIONAL targets beyond whichever player
/// its normal attack already targeted (`apply_splash`'s own `max_targets`
/// excludes the primary), giving 5 TOTAL players hit per swing per the
/// "hits 5 random players" request.
pub(crate) const CUBE_SPLASH_MAX_TARGETS: usize = 4;

pub(crate) struct HitOutcome {
    /// Actual HP lost - after evasion (0 if evaded)/block/damage
    /// reduction all apply. What actually happens to the target.
    damage: u64,
    /// What `damage` would have been with none of the target's
    /// mitigation (evasion, block, damage reduction) applied - the
    /// attacker's own crit/increased-damage IS included, since that's
    /// offense, not something being "mitigated". Reported instead of
    /// `damage` for "damage taken" stats (see `summarize_fight`) so a
    /// heavily-defensive character's stats reflect the real incoming
    /// threat they shrugged off, not just what leaked through - a 75%
    /// damage-reduction character who actually loses 1000 hp "took" 4000.
    unmitigated_damage: u64,
    is_crit: bool,
    evaded: bool,
    /// Whether the block-chance roll succeeded on this hit (independent of
    /// `evaded` - a hit can only be blocked if it wasn't evaded first).
    /// Feeds Warrior's Aegis/Spike Barrier (a blocked hit shields an ally /
    /// reflects damage) - see their handling in `apply_hit`.
    is_blocked: bool,
    /// Whether THIS hit was granted Opportunist's guaranteed-landing
    /// treatment, from any source (the base fight-opening budget, Opening
    /// Move's cooldown recharge, or Cold Steel's passed-along debuff) -
    /// read by `apply_hit` to know whether to apply Predator's mark and/or
    /// set a fresh Cold Steel debuff, without re-deriving (and
    /// potentially re-rolling Cold Steel's own chance) after the fact.
    opportunist_guaranteed: bool,
    /// Warlock's Curse of Weakness family (2026-08-16, a live design call:
    /// "the damage contribution of Curse of Weakness and all subsequent
    /// nodes" - Amplify Curse/Contagious Curse/Epidemic, everything that
    /// manifests as `def.curse_dmg_taken_bonus` - "should be accounted
    /// for in the warlock's DPS, not the other actor's") - how much of
    /// `damage` above only exists BECAUSE the target was cursed: the
    /// marginal difference between the real (curse-included) damage and
    /// what this same hit would have dealt with the curse's own negative
    /// damage-reduction source excluded from the mitigation combine.
    /// `apply_hit` splits `damage` into two separate `CombatEvent::Attack`
    /// entries using this - the attacker's own (reduced) share, and a
    /// second one crediting the cursing Warlock for this share - rather
    /// than letting it silently inflate the actual ATTACKER's own DPS.
    /// 0.0 whenever the target isn't currently cursed.
    curse_bonus_damage: f64,
    /// Every genuine `rng.gen_bool` roll resolved while computing this
    /// hit, PLUS Stonewall's deterministic auto-block (2026-08-17, full-
    /// detail combat log, Wiring Phase 1) - crit-chance remainder,
    /// evasion, block (rolled or auto), Cold Steel pass-along. Each
    /// tuple is `(category, source name, probability actually rolled
    /// against - `None` for a deterministic source like Stonewall's
    /// auto-block, whether it succeeded)`. Only ONE entry per roll/
    /// deterministic-trigger that actually happened - a roll that's
    /// short-circuited (e.g. no evasion roll at all on an
    /// `opportunist_guaranteed` hit) adds nothing here, same "no roll,
    /// nothing to log" reasoning as every other `RollEvent` filter in
    /// this system. `apply_hit` (which owns the actual `hit_id`/
    /// `RollEvent` sink) turns these into real `RollEvent`s once the
    /// hit's outcome is known.
    probabilistic_rolls: Vec<(RollCategory, &'static str, Option<f64>, bool)>,
    /// Every deterministic (non-probabilistic) source that contributed to
    /// this hit (2026-08-17, Phase 2) - DR/block/evasion sources feeding
    /// `combine_reduction_sources`, crit-chance/multiplier sources, and
    /// increased-damage sources (including the attacker-side `raw_dmg *=`
    /// debuffs, e.g. Thornedhide/Cthulhu/the late-stage penalty, as a
    /// NEGATIVE magnitude under the same `IncreasedDamage` bucket - this
    /// field is about pipeline stage, not sign). Each tuple is `(category,
    /// source name, magnitude)` - no probability/succeeded, these aren't
    /// rolls. Same "only non-zero/active sources" filter as
    /// `probabilistic_rolls`. Sibling field, same `apply_hit` conversion
    /// treatment, just a different tuple shape (no roll outcome to carry).
    deterministic_sources: Vec<(RollCategory, &'static str, f64)>,
}

/// Attacker-side base damage before any of the crit/increased-damage/
/// defender pipeline: `atk` plus the helm's accumulated stacking bonus
/// (see `CombatSimUnit::helm_stack_bonus`, converted from dps units to
/// a flat per-hit amount at THIS unit's own attack cadence), jittered
/// ±15%. Shared by a real attack's `base_damage` and a heal action's
/// roll (see `roll_attacker_damage`) - both start from the exact same
/// number, since healing is strictly converted damage now.
/// Rough effective-HP estimate - what the real boss's survivability-first
/// targeting (see `simulate_battle`'s boss-attack branch) sorts by.
/// Multiplicative, not additive: max_hp scaled up by damage reduction,
/// evasion, and block (weighted by how much a block actually reduces a
/// hit, `BLOCK_DAMAGE_REDUCTION`) each independently making the unit
/// harder to bring down. `.max(0.0)` on damage_reduction only - a
/// NEGATIVE damage reduction (see that field's doc) makes a unit take
/// MORE damage, which should never count as extra survivability, but
/// evasion/block_chance can't go negative in the first place.
pub(crate) fn survivability(u: &CombatSimUnit) -> f64 {
    u.max_hp as f64 * (1.0 + u.damage_reduction.max(0.0)) * (1.0 + u.evasion) * (1.0 + u.block_chance * BLOCK_DAMAGE_REDUCTION)
}

/// Narrows a pool of alive-player target candidates down to just whoever's
/// above the party's median level, when at least one such candidate is
/// still alive - "the strongest heroes die first" per the request,
/// applied to EVERY enemy attack (the real boss, a Lich add, a basic
/// mob) and their splash, not just the boss's own primary hit. Falls
/// back to the full candidate list once nobody above median is left
/// standing (or the party never had any spread to begin with), so a
/// fight doesn't stall refusing to hit anyone once the "stronger" half
/// is dead - the weaker half becomes fair game exactly like before this
/// existed. Any existing secondary selection (survivability-max for the
/// real boss, a random pick for everything else) still runs on top of
/// whatever this returns.
pub(crate) fn prioritize_above_median(candidates: &[usize], units: &[CombatSimUnit], median_level: f64) -> Vec<usize> {
    let above: Vec<usize> = candidates.iter().copied().filter(|&i| units[i].level as f64 > median_level).collect();
    if above.is_empty() { candidates.to_vec() } else { above }
}

/// Berserker's Reckless Swing/Death Wish - "15% dealt / 8% taken at rank
/// 1, +10%/+5% per additional rank" isn't expressible as ONE linear
/// `PassiveStat` formula (dealt and taken scale by different amounts per
/// rank), so both halves are rank-matched directly here instead - used
/// once, at `CombatSimUnit` construction, folded straight into
/// `increased_damage`/`damage_reduction` rather than tracked as their own
/// fields (see that construction site's own doc).
pub(crate) fn reckless_swing_dealt_pct(rank: u32) -> f64 {
    match rank {
        1 => 0.15,
        2 => 0.25,
        3.. => 0.35,
        0 => 0.0,
    }
}
pub(crate) fn reckless_swing_taken_pct(rank: u32) -> f64 {
    match rank {
        1 => 0.08,
        2 => 0.13,
        3.. => 0.18,
        0 => 0.0,
    }
}
/// Death Wish - "another 10% dealt / 5% taken per rank, stacking with
/// [Reckless Swing's] base" - same rank-matched shape as its parent.
pub(crate) fn death_wish_dealt_pct(rank: u32) -> f64 {
    match rank {
        1 => 0.10,
        2 => 0.20,
        3.. => 0.30,
        0 => 0.0,
    }
}
pub(crate) fn death_wish_taken_pct(rank: u32) -> f64 {
    match rank {
        1 => 0.05,
        2 => 0.10,
        3.. => 0.15,
        0 => 0.0,
    }
}

pub(crate) fn attacker_base_damage(unit: &CombatSimUnit, rng: &mut impl Rng) -> f64 {
    let helm_bonus = unit.helm_stack_bonus * (unit.attack_interval_ms as f64 / 1000.0);
    (unit.atk as f64 + helm_bonus) * rng.gen_range(0.85..1.15)
}

/// Attacker-side portion of a damage roll - crit (chance + multiplier)
/// then increased damage (multiplicative) - WITHOUT any defender-side
/// mitigation. Shared by `resolve_hit` (a real attack, which layers
/// evasion/block/damage_reduction on top against a specific defender)
/// and a heal action (there's no "defender" to mitigate against an
/// ally being healed, so this attacker-side roll IS the whole thing,
/// before `Character::combat_heal_power`'s conversion - see
/// `simulate_battle`'s heal branch). Returns (damage, was it a crit,
/// the fractional crit-chance remainder actually rolled against, and
/// whether that roll succeeded) - the last two feed `resolve_hit`'s own
/// `RollEvent` logging (2026-08-17, full-detail combat log); the heal
/// call site just ignores them, nothing there needs roll-level detail.
///
/// `crit_chance` is uncapped (see `Character::combat_crit_chance`'s
/// doc) - every full 100% of it is a GUARANTEED crit stack (e.g. 250%
/// crit chance = 2 guaranteed stacks + a 50% chance of a 3rd), and each
/// stack adds another full `crit_multiplier` bonus on top of the base,
/// not just one capped crit roll. `E[crit_stacks]` always equals
/// `crit_chance` exactly this way, so the dashboard's simpler
/// `combat_dps`/`combat_hps` EV formula (`1 + crit_chance *
/// (crit_multiplier - 1)`) stays correct without needing to know about
/// stacking at all.
/// Halves the actual damage BONUS a crit grants (the `- 1.0` part of
/// `crit_multiplier`, e.g. the baseline 2.0x's "extra" 1.0), applied
/// once here at the point a crit's damage is actually computed - a live
/// request to cut every crit roll's numeric value in half, broader than
/// the earlier pass that only cut how much crit chance/multiplier gear
/// and archetypes could roll in the first place (see
/// `affix_base_value`'s CritChance/CritMultiplier cases and
/// `Archetype::bonus`'s Rogue/Mage cases) - this one also reaches the
/// universal 5%/2.0x baseline those deliberately left alone. Applied
/// per crit STACK (`crit_stacks`, uncapped past 100% chance - see
/// `Character::combat_crit_chance`), so a guaranteed double/triple crit
/// still compounds correctly with this halving, not just a single-stack
/// case.
pub(crate) const CRIT_BONUS_MULT: f64 = 0.5;

/// Overcrit saturation curve (2026-08-18, a live request: "give better
/// control over how far crit stacking can run away at very high crit
/// chance") - a rectangular-hyperbola curve, `A * x / (x + h)`, that
/// asymptotically approaches `A` as `x` grows without ever reaching it:
/// `overcrit_curve(OVERCRIT_CURVE_H) == OVERCRIT_CURVE_A / 2` by
/// construction (`h` is literally "the input at which you've earned half
/// of `A`"). Used by `crit_stack_bonus` below to cap how much bonus crit
/// stacks PAST the first can ever contribute, instead of the old flat
/// linear growth that let very high crit chance compound an already-huge
/// hit without bound (part of what caused Hemorrhage's own
/// trillions-of-damage incident earlier this session, on the other half
/// of that same formula).
pub(crate) const OVERCRIT_CURVE_A: f64 = 1.5;
pub(crate) const OVERCRIT_CURVE_H: f64 = 1.0;

fn overcrit_curve(x: f64) -> f64 {
    OVERCRIT_CURVE_A * x / (x + OVERCRIT_CURVE_H)
}

/// The crit-stacking bonus TERM (everything `crit_bonus_mult` below adds
/// on top of its leading `1.0`) for a given whole number of crit stacks -
/// shared by `roll_attacker_damage` (real combat, `crit_stacks` from one
/// actual per-hit roll) and `Character::combat_total_output_per_sec` (the
/// dashboard's DPS/HPS EV estimate, evaluated at `crit_chance`'s two
/// possible whole-stack outcomes - see that function's own doc for why
/// that stays an EXACT expectation, not an approximation, despite this
/// being nonlinear in `crit_stacks`). The first stack (up to a real 100%
/// crit chance) still pays the flat `CRIT_BONUS_MULT` rate exactly like
/// before this change - only stacks PAST the first ("overcrit") run
/// through `overcrit_curve` instead of growing linearly forever, so the
/// total achievable bonus caps out at `(1.0 + OVERCRIT_CURVE_A) * (crit_multiplier - 1.0) * CRIT_BONUS_MULT` no matter how high crit chance
/// climbs.
pub(crate) fn crit_stack_bonus(crit_stacks: f64, crit_multiplier: f64) -> f64 {
    let overcrit = (crit_stacks - 1.0).max(0.0);
    (crit_stacks.min(1.0) + overcrit_curve(overcrit)) * (crit_multiplier - 1.0) * CRIT_BONUS_MULT
}

/// Named return for `roll_attacker_damage` (2026-08-17, Phase 2 -
/// replaces the old plain `(f64, bool, f64, bool)` tuple now that a 5th,
/// heap-allocated value is joining it). `deterministic_sources` carries
/// every non-zero crit-chance/crit-multiplier/increased-damage source
/// this roll actually used - `resolve_hit` merges it into its own
/// `HitOutcome.deterministic_sources`. The heal-roll call site
/// (`apply_heal`) only reads `damage`/`is_crit`, ignoring the rest -
/// nothing there needs roll-level detail.
pub(crate) struct RollAttackerDamageResult {
    damage: f64,
    is_crit: bool,
    crit_remainder: f64,
    crit_remainder_roll: bool,
    deterministic_sources: Vec<(RollCategory, &'static str, f64)>,
}

/// `mark_crit_bonus`/`mark_crit_mult_bonus`/`mark_dmg_bonus` are Hunter's
/// Mark/Predator's Eye/Kill Zone's live bonuses - computed by
/// `resolve_hit` (the only caller, which has both `atk` AND `def` and can
/// check eligibility) and passed in as plain magnitudes rather than
/// re-deriving them here, since this function only ever sees the
/// attacker - see `resolve_hit`'s own mark-eligibility computation.
pub(crate) fn roll_attacker_damage(
    base_damage: f64,
    atk: &CombatSimUnit,
    at_ms: u32,
    rng: &mut impl Rng,
    mark_crit_bonus: f64,
    mark_crit_mult_bonus: f64,
    mark_dmg_bonus: f64,
    force_crit: bool,
    arcane_instability_active: bool,
) -> RollAttackerDamageResult {
    // Full-detail combat log (2026-08-17, Phase 2) - every non-zero
    // crit-chance/multiplier/increased-damage source gets named here as
    // it's computed, same "only what's actually non-zero" filter as
    // everywhere else in this system.
    let mut deterministic_sources: Vec<(RollCategory, &'static str, f64)> = Vec::new();
    // Berserker's Gambit - a live missing-HP-scaling crit bonus, stepped
    // in 20%-missing increments (not continuous) per the node's own "for
    // every 20% max HP missing" text. Checked here rather than baked into
    // `crit_chance` at construction since it needs the attacker's CURRENT
    // hp, not a value snapshotted once at fight start.
    let gambit_bonus = if atk.gambit_crit_per_missing_20pct > 0.0 && atk.max_hp > 0 {
        let hp_pct = atk.hp as f64 / atk.max_hp as f64;
        let missing_frac = (1.0 - hp_pct).max(0.0);
        let mut bonus = (missing_frac / 0.20).floor() * atk.gambit_crit_per_missing_20pct;
        // Last Laugh (rank 2) - a flat +15 percentage point bump to
        // Gambit's own crit-chance bonus below 25% HP (2026-08-17,
        // reworked from `*= 2.0` - a real doubling here would compound
        // with rank 3's own crit-damage doubling in the SAME
        // `crit_bonus_mult` formula below, since this bonus feeds
        // `crit_stacks` and rank 3's feeds `crit_multiplier`, both
        // multiplied together).
        if atk.lastlaugh_crit_bonus && hp_pct < 0.25 {
            bonus += 0.15;
        }
        bonus
    } else {
        0.0
    };
    // Death Defiant - a frozen Gambit bonus from before a recent heal
    // moved this unit to a lower missing-HP bucket (see `apply_heal`'s
    // own hook), still active for a grace window. Never LOWERS the live
    // bonus above, only backstops it if the live value just dropped.
    let gambit_bonus = if atk.deathdefiant_frozen_crit_bonus > 0.0 && at_ms <= atk.deathdefiant_frozen_crit_bonus_expires_at_ms {
        gambit_bonus.max(atk.deathdefiant_frozen_crit_bonus)
    } else {
        gambit_bonus
    };
    // Monk's Pressure Point - live crit-chance bonus from Flowing
    // Strikes' stacks (see `flowing_crit_bonus`'s doc).
    let flowing_bonus = flowing_crit_bonus(atk, at_ms);
    // Ranger's Vanishing Shot - a temporary crit-chance buff off a recent
    // evade.
    let vanishingshot_bonus = if atk.temp_crit_chance_buff > 0.0 && at_ms <= atk.temp_crit_chance_buff_expires_at_ms { atk.temp_crit_chance_buff } else { 0.0 };
    // Mage's Riptide - live crit-chance bonus per active Flow State stack.
    let riptide_bonus = if atk.stack_crit_per_stack > 0.0 && at_ms <= atk.stack_speed_expires_at_ms { atk.stack_speed_current as f64 * atk.stack_crit_per_stack } else { 0.0 };
    // Paradox - Temporal Rift's excess-attack-speed conversion also feeds
    // crit chance, in parallel with its own damage conversion (computed
    // again, identically, where the damage half is applied further down -
    // cheap, no side effects, avoids threading this value across the
    // whole function).
    let paradox_excess_speed = (atk.attack_speed_pct - if atk.speed_overflow_threshold > 0.0 { atk.speed_overflow_threshold } else { 1.0 }).max(0.0);
    let paradox_bonus = paradox_excess_speed * atk.speed_overflow_crit_pct;
    let crit_chance = (atk.crit_chance + gambit_bonus + flowing_bonus + mark_crit_bonus + vanishingshot_bonus + riptide_bonus + paradox_bonus).max(0.0);
    if atk.crit_chance > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Crit chance", atk.crit_chance));
    }
    if gambit_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Gambit", gambit_bonus));
    }
    if flowing_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Pressure Point", flowing_bonus));
    }
    if mark_crit_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Hunter's Mark family", mark_crit_bonus));
    }
    if vanishingshot_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Vanishing Shot", vanishingshot_bonus));
    }
    if riptide_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Riptide", riptide_bonus));
    }
    if paradox_bonus > 0.0 {
        deterministic_sources.push((RollCategory::Crit, "Paradox", paradox_bonus));
    }
    let guaranteed_stacks = crit_chance.floor();
    let remainder = crit_chance - guaranteed_stacks;
    let remainder_roll = rng.gen_bool(remainder);
    let mut crit_stacks = guaranteed_stacks + if remainder_roll { 1.0 } else { 0.0 };
    // Rogue's Assassinate - guarantees AT LEAST one crit stack on this
    // hit, on top of (not instead of) whatever the normal roll already
    // produced.
    if force_crit {
        crit_stacks = crit_stacks.max(1.0);
    }
    let is_crit = crit_stacks > 0.0;
    // Ranger's Predator's Eye - a live crit-multiplier bonus against the
    // marked target only (see `mark_crit_multiplier_bonus`'s doc).
    let mut crit_multiplier = atk.crit_multiplier + mark_crit_mult_bonus;
    // Last Laugh (rank 3) - a flat +50% crit-damage bonus below 25% HP
    // (2026-08-17, reworked from `*= 2.0` - see the rank-2 doubling's own
    // doc just above for why a real multiplicative double here was
    // dangerous).
    if atk.lastlaugh_crit_mult && atk.max_hp > 0 && (atk.hp as f64 / atk.max_hp as f64) < 0.25 {
        crit_multiplier += 0.50;
    }
    // Coup de Grace - bonus crit damage on a guaranteed (force_crit) hit.
    if force_crit && atk.assassinate_crit_mult_bonus > 0.0 {
        crit_multiplier += atk.assassinate_crit_mult_bonus;
    }
    // Empowered Bolt (rank 3) - bonus crit damage specifically on its own
    // guaranteed first-hit crit.
    if force_crit && atk.empoweredbolt_invested && atk.hits_landed_this_fight == 0 && atk.empoweredbolt_crit_mult_bonus > 0.0 {
        crit_multiplier += atk.empoweredbolt_crit_mult_bonus;
    }
    // Monk's Nerve Strike - flat crit-damage bonus for anyone with
    // Pressure Point invested.
    if atk.nervestrike_crit_mult_bonus > 0.0 {
        crit_multiplier += atk.nervestrike_crit_mult_bonus;
    }
    // Mage's Arcane Instability - a flat crit-damage bonus (5%/9%/12% at
    // rank 1/2/3, `arcaneinstability_bonus_pct`) against a target above
    // 65% HP (2026-08-17, reworked from a `2.0x` doubling of the whole
    // crit-damage bonus term - real doubling here compounded with
    // Arcane Mastery/Overload/Cataclysm's own multiplicative crit-damage
    // product). Added into `crit_multiplier` alongside every other
    // conditional crit-damage bonus above (Coup de Grace, Empowered Bolt,
    // Last Laugh) instead of a separate multiplier on the final formula.
    if arcane_instability_active {
        crit_multiplier += atk.arcaneinstability_bonus_pct;
    }
    // Crit-MULTIPLIER sources only actually affect anything when this
    // hit is a crit (`crit_bonus_mult` below multiplies by `crit_stacks`,
    // which is 0 on a non-crit) - gated on `is_crit` so a non-crit hit
    // doesn't log a bunch of sources that contributed nothing this time.
    if is_crit {
        deterministic_sources.push((RollCategory::Crit, "Crit multiplier", atk.crit_multiplier));
        if mark_crit_mult_bonus > 0.0 {
            deterministic_sources.push((RollCategory::Crit, "Predator's Eye/Exploit Weakness", mark_crit_mult_bonus));
        }
        if atk.lastlaugh_crit_mult && atk.max_hp > 0 && (atk.hp as f64 / atk.max_hp as f64) < 0.25 {
            deterministic_sources.push((RollCategory::Crit, "Last Laugh", 0.50));
        }
        if force_crit && atk.assassinate_crit_mult_bonus > 0.0 {
            deterministic_sources.push((RollCategory::Crit, "Coup de Grace", atk.assassinate_crit_mult_bonus));
        }
        if force_crit && atk.empoweredbolt_invested && atk.hits_landed_this_fight == 0 && atk.empoweredbolt_crit_mult_bonus > 0.0 {
            deterministic_sources.push((RollCategory::Crit, "Empowered Bolt", atk.empoweredbolt_crit_mult_bonus));
        }
        if atk.nervestrike_crit_mult_bonus > 0.0 {
            deterministic_sources.push((RollCategory::Crit, "Nerve Strike", atk.nervestrike_crit_mult_bonus));
        }
        if arcane_instability_active {
            deterministic_sources.push((RollCategory::Crit, "Arcane Instability", atk.arcaneinstability_bonus_pct));
        }
    }
    let crit_bonus_mult = 1.0 + crit_stack_bonus(crit_stacks, crit_multiplier);
    let mut dmg = base_damage * crit_bonus_mult;
    // Mage's Temporal Rift / Warlock's Unstable Power - baseline attack
    // speed above 100% converts excess into increased damage (see
    // `attack_speed_pct`'s doc for what "total" means here).
    // Eternal Moment - lowers the threshold excess attack speed starts
    // converting past (1.0 = 100% default).
    let speed_overflow_threshold = if atk.speed_overflow_threshold > 0.0 { atk.speed_overflow_threshold } else { 1.0 };
    let excess_speed = (atk.attack_speed_pct - speed_overflow_threshold).max(0.0);
    let speed_overflow_bonus = excess_speed * atk.speed_overflow_dmg_pct;
    // Berserker's Bloodlust - live increased-damage bonus from its own
    // stacks (see `stack_damage_bonus`'s doc), folded into the same
    // multiplier as the archetype/gear increased-damage total. Ranger's
    // Kill Zone (`mark_dmg_bonus`) rides the same multiplier.
    // Elemental damage rework's marginal-overflow conversion (see
    // `CombatSimUnit::elemental_overflow_dmg_bonus`'s doc) rides it too -
    // lazy-expiry, same convention as every other timed field here.
    let elemental_overflow_bonus = if atk.elemental_overflow_dmg_bonus > 0.0 && at_ms <= atk.elemental_overflow_dmg_bonus_expires_at_ms { atk.elemental_overflow_dmg_bonus } else { 0.0 };
    // Berserker's Warlord's Resolve / Slayer's Warlord's Resolve - a
    // party-wide temporary increased-damage grant (see
    // `temp_party_increased_damage_bonus`'s doc), lazy-expiry same as
    // every other temp_* field.
    let party_dmg_bonus = if atk.temp_party_increased_damage_bonus > 0.0 && at_ms <= atk.temp_party_increased_damage_bonus_expires_at_ms { atk.temp_party_increased_damage_bonus } else { 0.0 };
    // Warrior's Berserk Vigor - live increased-damage bonus while below
    // Last Stand's own 25%-HP threshold.
    let berserkvigor_bonus = if atk.laststand_berserkvigor_pct > 0.0 && atk.max_hp > 0 && (atk.hp as f64 / atk.max_hp as f64) < 0.25 { atk.laststand_berserkvigor_pct } else { 0.0 };
    // Warrior's Avalanche - each active Momentum stack also adds
    // increased damage, same live stack count Bloodlust's own
    // `stack_damage_bonus` reads.
    let avalanche_bonus = if atk.stack_avalanche_dmg_per_stack > 0.0 && at_ms <= atk.stack_speed_expires_at_ms { atk.stack_speed_current as f64 * atk.stack_avalanche_dmg_per_stack } else { 0.0 };
    let bloodlust_bonus = stack_damage_bonus(atk, at_ms);
    dmg *= 1.0
        + atk.increased_damage
        + bloodlust_bonus
        + mark_dmg_bonus
        + speed_overflow_bonus
        + elemental_overflow_bonus
        + party_dmg_bonus
        + berserkvigor_bonus
        + avalanche_bonus
        + atk.splash_target_dmg_bonus;
    if atk.increased_damage > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Increased damage (gear/tree)", atk.increased_damage));
    }
    if bloodlust_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Bloodlust", bloodlust_bonus));
    }
    if mark_dmg_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Kill Zone/Alpha's Predator", mark_dmg_bonus));
    }
    if speed_overflow_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Temporal Rift/Unstable Power", speed_overflow_bonus));
    }
    if elemental_overflow_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Elemental overflow", elemental_overflow_bonus));
    }
    if party_dmg_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Warlord's Resolve", party_dmg_bonus));
    }
    if berserkvigor_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Berserk Vigor", berserkvigor_bonus));
    }
    if avalanche_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Avalanche", avalanche_bonus));
    }
    if atk.splash_target_dmg_bonus > 0.0 {
        deterministic_sources.push((RollCategory::IncreasedDamage, "Splash target bonus", atk.splash_target_dmg_bonus));
    }
    RollAttackerDamageResult { damage: dmg, is_crit, crit_remainder: remainder, crit_remainder_roll: remainder_roll, deterministic_sources }
}

/// Resolves one hit's actual damage from a base roll, running it through
/// the full offense/defense pipeline in order: evasion (all-or-nothing
/// miss, checked first since a dodged hit skips everything else) →
/// attacker-side crit/increased damage (see `roll_attacker_damage`) →
/// block (a flat halving if triggered) → damage reduction (one more
/// flat multiplicative layer on whatever's left). Every damage instance
/// in the fight - normal attacks either direction, and splash hits -
/// goes through this one function (via `apply_hit`) so they all behave
/// identically.
/// `pack_instinct_evasion_bonus`/`symbiosis_dr_bonus` are Druid's Pack
/// Instinct/Symbiosis - live, computed by `apply_hit` (which has the full
/// `units` slice needed to find "the party's current lowest-HP ally",
/// something this function's plain `atk`/`def` refs can't see) and passed
/// in as plain magnitudes, same reasoning as `roll_attacker_damage`'s own
/// mark-bonus params. `force_crit` is Rogue's Assassinate - a charge
/// consumed (and turned into this bool) by `apply_hit` BEFORE calling
/// here, since consuming a charge needs `&mut` access this function's
/// plain `atk: &CombatSimUnit` doesn't have.
/// `target_fire_debuff`/`target_fire_buff`/`target_cold_debuff`/
/// `target_cold_buff`/`target_chaos_debuff`/`target_chaos_buff`/
/// `target_lightning_dmg_taken` are the elemental damage rework's own
/// live per-fight state (2026-08-15, see `CombatSimUnit::fire_dr_debuff`'s
/// doc) - already pruned-and-counted plain percentages, same "caller
/// resolves it, passes a plain magnitude in" reasoning as
/// `pack_instinct_evasion_bonus`/`symbiosis_dr_bonus` above (this
/// function's own `def: &CombatSimUnit` can't prune anything itself,
/// it's not `&mut`).
/// The same universal "damage to a real boss is capped by how far past
/// its tuned stage range the fight now is" cut `resolve_hit` applies to
/// every real attack (see `CombatSimUnit::late_stage_damage_penalty_pct`'s
/// doc) - a no-op for anyone but a real boss. Every true-damage path that
/// bypasses `resolve_hit` (reflect, Volatile Magic splash, Lingering
/// Effect ticks, Hemorrhage explosions, Doom's detonation + Apocalypse
/// splash) runs its raw amount through this before applying it, so the
/// penalty really is unbypassable per the live "nothing can bypass it"
/// design directive - not just something `resolve_hit`'s own callers get
/// for free. Pushes a matching `RollEvent` (same convention as
/// `resolve_hit`'s own `"Late-stage damage penalty"` entry) whenever it
/// actually cut something, so these true-damage hits stay visible to the
/// same roll-signature analysis a real attack already supports.
fn apply_late_stage_penalty(
    units: &[CombatSimUnit],
    target_idx: usize,
    amount: f64,
    at_ms: u32,
    hit_id: u64,
    actor_id: &str,
    rolls: &mut Vec<RollEvent>,
) -> f64 {
    let pct = units[target_idx].late_stage_damage_penalty_pct;
    if pct <= 0.0 {
        return amount;
    }
    rolls.push(RollEvent {
        event_id: next_hit_id(),
        hit_id,
        caused_by: None,
        at_ms,
        category: RollCategory::IncreasedDamage,
        source: std::borrow::Cow::Borrowed("Late-stage damage penalty"),
        actor: actor_id.to_string(),
        target: Some(units[target_idx].id.clone()),
        probability: None,
        succeeded: None,
        magnitude: Some(-pct),
    });
    amount * (1.0 - pct)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_hit(
    base_damage: f64,
    atk: &CombatSimUnit,
    def: &CombatSimUnit,
    at_ms: u32,
    rng: &mut impl Rng,
    pack_instinct_evasion_bonus: f64,
    symbiosis_dr_bonus: f64,
    force_crit: bool,
    target_fire_debuff: f64,
    target_fire_buff: f64,
    target_cold_debuff: f64,
    target_cold_buff: f64,
    target_chaos_debuff: f64,
    target_chaos_buff: f64,
    target_lightning_dmg_taken: f64,
) -> HitOutcome {
    // Ranger's Hunter's Mark - `def.mark_source_id` names whoever marked
    // this defender (if anyone). Predator's Eye/Kill Zone are PERSONAL
    // (only the marking Ranger gets them); Pack Tactics' crit bonus is
    // the one exception, granted to any OTHER player attacking the
    // marked target (see `own_mark_ally_crit_chance`'s doc - "your
    // allies", not "you too", so it deliberately excludes the source).
    let is_mark_source = def.mark_source_id.as_deref() == Some(atk.id.as_str());
    let is_mark_ally = !is_mark_source && !atk.is_boss && def.mark_source_id.is_some();
    // Weak Point - +crit chance against the same below-threshold target
    // Exploit Weakness's own multiplier bonus checks.
    let weakness_threshold = if atk.exploit_weakness_threshold > 0.0 { atk.exploit_weakness_threshold } else { 0.5 };
    let is_below_weakness_threshold = def.max_hp > 0 && (def.hp as f64 / def.max_hp as f64) < weakness_threshold;
    let weakpoint_bonus = if is_below_weakness_threshold { atk.weakpoint_crit_chance_pct } else { 0.0 };
    // Hunter's Instinct - +crit chance when the target is a boss.
    let huntersinstinct_bonus = if def.is_boss { atk.huntersinstinct_crit_vs_boss_pct } else { 0.0 };
    let mark_crit_bonus = (if is_mark_source { def.mark_crit_chance_bonus } else { 0.0 })
        + (if is_mark_ally { def.mark_ally_crit_chance_bonus } else { 0.0 })
        + weakpoint_bonus
        + huntersinstinct_bonus;
    // Rogue's Exploit Weakness - live crit-multiplier bonus, only against
    // a target currently below its own threshold (raised by Vital Strike).
    let exploit_weakness_bonus = if is_below_weakness_threshold { atk.exploit_weakness_crit_mult_pct } else { 0.0 };
    let mark_crit_mult_bonus = (if is_mark_source { def.mark_crit_multiplier_bonus } else { 0.0 })
        // Hunter's Focus - shares a fraction of Predator's Eye's own crit
        // damage bonus with allies too, same "any OTHER ally" shape Pack
        // Tactics/Alpha's Predator already use for crit chance/damage.
        + (if is_mark_ally { def.mark_ally_crit_multiplier_bonus } else { 0.0 })
        + exploit_weakness_bonus;
    // Kill Zone - only the marking Ranger, and only below its own
    // threshold (raised by Final Blow, 25% default).
    let killzone_threshold = if def.max_hp > 0 && atk.killzone_threshold > 0.0 { atk.killzone_threshold } else { 0.25 };
    let is_below_killzone = def.max_hp > 0 && (def.hp as f64 / def.max_hp as f64) <= killzone_threshold;
    let mark_dmg_bonus = (if is_mark_source && is_below_killzone { def.mark_low_hp_damage_bonus } else { 0.0 })
        // Alpha's Predator - any OTHER ally also gets a damage bonus
        // against the marked target (unconditional, not gated on Kill
        // Zone's low-HP threshold).
        + (if is_mark_ally { def.mark_ally_dmg_bonus } else { 0.0 });
    // Attacker-side damage (crit/increased-damage) is always rolled,
    // evasion or not - it's what feeds `unmitigated_damage` even on a
    // dodge, since the whole point is reporting what the hit WOULD have
    // done. (No longer skippable-on-evasion the way it used to be, back
    // when only the final post-mitigation `damage` mattered.)
    let arcane_instability_active = atk.arcaneinstability_threshold > 0.0 && def.max_hp > 0 && (def.hp as f64 / def.max_hp as f64) > atk.arcaneinstability_threshold;
    let attacker_roll = roll_attacker_damage(base_damage, atk, at_ms, rng, mark_crit_bonus, mark_crit_mult_bonus, mark_dmg_bonus, force_crit, arcane_instability_active);
    let (mut raw_dmg, is_crit, crit_remainder, crit_remainder_roll) =
        (attacker_roll.damage, attacker_roll.is_crit, attacker_roll.crit_remainder, attacker_roll.crit_remainder_roll);
    // Full-detail combat log (2026-08-17, Wiring Phase 1) - every genuine
    // probabilistic roll this hit resolves, in the order they're rolled.
    // A remainder of exactly 0.0 means crit_chance was a whole number
    // (nothing fractional to actually roll against) - same "no roll,
    // nothing to log" filter every other source here uses.
    let mut probabilistic_rolls: Vec<(RollCategory, &'static str, Option<f64>, bool)> = Vec::new();
    if crit_remainder > 0.0 {
        probabilistic_rolls.push((RollCategory::Crit, "Crit chance remainder", Some(crit_remainder), crit_remainder_roll));
    }
    // Full-detail combat log (2026-08-17, Phase 2) - the attacker-side
    // `raw_dmg *=` debuffs below (Cthulhu through the late-stage penalty)
    // aren't from `roll_attacker_damage` - they're applied here, directly
    // in `resolve_hit` - so they get their own local accumulator, merged
    // into the final combined `deterministic_sources` alongside
    // `attacker_roll`'s own and the DR/evasion sources further down.
    let mut attacker_side_debuffs: Vec<(RollCategory, &'static str, f64)> = Vec::new();
    // Cthulhu's stacking debuff (see `cthulhu_debuff_stacks`) - an
    // attacker-side nerf, not a defensive mitigation, so it's baked into
    // the roll itself and DOES carry into `unmitigated_damage` too (that
    // field is about what the DEFENDER avoided, not about hiding the
    // attacker's own reduced output). Same lazy-expiry + floor shape as
    // Thornedhide just below.
    if atk.cthulhu_debuff_stacks > 0 && at_ms <= atk.cthulhu_debuff_expires_at_ms {
        let mult = (atk.cthulhu_debuff_stacks as f64 * atk.cthulhu_debuff_pct_per_stack).min(CTHULHU_DEBUFF_CAP);
        raw_dmg *= 1.0 - mult;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Cthulhu's debuff", -mult));
    }
    // Slayer's Necrotic Grip - the ATTACKER's own outgoing damage is
    // reduced while THEY themselves are wounded (lazily treated as
    // expired once `wound_expires_at_ms` has passed - see
    // `CombatSimUnit`'s doc, same convention as every other wound read).
    if atk.wound_stacks > 0 && at_ms <= atk.wound_expires_at_ms {
        raw_dmg *= 1.0 - atk.wound_damage_dealt_debuff;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Necrotic Grip", -atk.wound_damage_dealt_debuff));
    }
    // Poison Thorns - a temporary damage-dealt debuff from a recent
    // Bramblegrowth reflect, same lazy-expiry convention as every other
    // timed field here.
    if atk.temp_damage_dealt_debuff > 0.0 && at_ms <= atk.temp_damage_dealt_debuff_expires_at_ms {
        raw_dmg *= 1.0 - atk.temp_damage_dealt_debuff;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Poison Thorns", -atk.temp_damage_dealt_debuff));
    }
    // Warrior's Thornedhide - a live stacking damage-dealt debuff on the
    // ATTACKER from a prior Spike Barrier trigger (see
    // `thornedhide_stacks`'s doc), same shape as Poison Thorns' own
    // single-value version just above.
    if atk.thornedhide_stacks > 0 && at_ms <= atk.thornedhide_expires_at_ms {
        let mult = (atk.thornedhide_stacks as f64 * atk.thornedhide_debuff_pct_per_stack).min(0.9);
        raw_dmg *= 1.0 - mult;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Thornedhide", -mult));
    }
    // Warlock's Soul Stone - every time it's saved THIS attacker from a
    // killing blow (see the death-save chain in `apply_hit`), it stacks a
    // PERMANENT (no expiry, unlike Thornedhide/Cthulhu above) 33%-per-use
    // outgoing damage debuff - same floor convention as those two so 3+
    // uses can't push a hit to/below zero.
    if atk.soul_stone_uses_this_fight > 0 {
        let mult = (atk.soul_stone_uses_this_fight as f64 * SOUL_STONE_DMG_PENALTY_PER_USE).min(0.9);
        raw_dmg *= 1.0 - mult;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Soul Stone penalty", -mult));
    }
    // Rogue's Cutthroat - a crit against a target below 25% HP deals extra
    // damage.
    if is_crit && atk.cutthroat_low_hp_dmg_pct > 0.0 && def.max_hp > 0 && (def.hp as f64 / def.max_hp as f64) < 0.25 {
        raw_dmg *= 1.0 + atk.cutthroat_low_hp_dmg_pct;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Cutthroat", atk.cutthroat_low_hp_dmg_pct));
    }
    // Rogue's Silent Killer - bonus damage on this unit's first hit
    // landed against a boss this fight. Read BEFORE `apply_hit` marks it
    // used (see `has_hit_boss_this_fight`'s doc), so this hit itself
    // qualifies.
    if def.is_boss && !atk.has_hit_boss_this_fight && atk.silentkiller_dmg_pct > 0.0 {
        raw_dmg *= 1.0 + atk.silentkiller_dmg_pct;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Silent Killer", atk.silentkiller_dmg_pct));
    }
    // Rogue's Backstab - a one-shot bonus for the next hit while Vanish is
    // active, cleared by `apply_hit` right after this call (see
    // `backstab_pending_dmg_pct`'s doc).
    if atk.backstab_pending_dmg_pct > 0.0 {
        raw_dmg *= 1.0 + atk.backstab_pending_dmg_pct;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Backstab", atk.backstab_pending_dmg_pct));
    }
    // Late-stage damage penalty (see `CombatSimUnit::late_stage_damage_penalty_pct`'s
    // doc) - a hard, unbypassable cap on damage dealt TO a real boss this
    // far past the originally-tuned stage range. Deliberately the LAST
    // attacker-side modifier before `raw_dmg` is finalized, so both
    // `damage` and `unmitigated_damage` (derived from it below) reflect
    // the same universal cut - and deliberately upstream of every
    // defender-side mitigation source below (`combine_reduction_sources`,
    // `boss_focus_stacks`, curse), so nothing can counteract it. 0.0 for
    // every unit except a real boss, so this is a no-op everywhere else.
    // Finally answerable per-hit (2026-08-17, Phase 2) - this exact
    // `RollEvent` is what the "is the late-stage penalty functioning as
    // intended" question earlier today needed and couldn't get from the
    // coarse-tier log alone.
    if def.late_stage_damage_penalty_pct > 0.0 {
        raw_dmg *= 1.0 - def.late_stage_damage_penalty_pct;
        attacker_side_debuffs.push((RollCategory::IncreasedDamage, "Late-stage damage penalty", -def.late_stage_damage_penalty_pct));
    }
    let unmitigated_damage = raw_dmg.round().max(0.0) as u64;
    // Rogue's Nightstalker - live evasion bonus specifically against a
    // boss attacker.
    let nightstalker_bonus = if atk.is_boss { def.nightstalker_evasion_pct } else { 0.0 };
    // Monk's Unbroken - the ATTACKER's own evasion-overflow conversion
    // reduces the DEFENDER's evasion roll (see `unbroken_ignore_evasion_pct`'s
    // doc) - a negative source, floored by the same final clamp every
    // other evasion source already goes through.
    // Last Stand (Unyielding Spirit) - doubles the above while the
    // ATTACKER is below their own (rank-raised) HP threshold, capped at a
    // flat 75% so it can never approach a guaranteed-ignore even stacked
    // with Overgrown Reach/Earthen Will off the same overflow pool.
    let unbroken_ignore = if atk.max_hp > 0 && (atk.hp as f64 / atk.max_hp as f64) < atk.unyieldingspirit_threshold {
        (atk.unbroken_ignore_evasion_pct * 2.0).min(0.75)
    } else {
        atk.unbroken_ignore_evasion_pct
    };
    // Mage's Frost Nova - a temporary evasion debuff from a recent splash
    // hit (see `apply_splash`'s own handling), same lazy-expiry
    // convention as every other timed field here.
    let frostnova_debuff = if def.temp_evasion_debuff > 0.0 && at_ms <= def.temp_evasion_debuff_expires_at_ms { def.temp_evasion_debuff } else { 0.0 };
    // Elemental damage rework (2026-08-15) - Cold's evasion debuff/buff,
    // applied here as its own adjustment (not just summed straight into
    // the roll below) so its OWN floor/ceiling (see
    // `ELEMENTAL_DEFENSE_FLOOR`/`_CEILING`'s docs) holds regardless of
    // whatever this unit's other evasion sources already add up to.
    // 2026-08-16 bugfix: `nightstalker_bonus`/`pack_instinct_evasion_bonus`
    // used to be raw-added straight on top of `def.evasion` (itself
    // already gear+tree combined via `combine_reduction_sources`,
    // typically ~80% for a real evasion build) - live fight logs confirmed
    // this could reach exactly 100% (a Rogue with maxed gear+tree evasion
    // plus Nightstalker vs a boss: 80% + 20% = 100%, a GUARANTEED dodge
    // every single swing - one real fight showed 0 landed hits out of
    // 1246 attempts). Routed through `combine_reduction_sources` instead,
    // same diminishing-returns treatment every other multi-source
    // defensive stat here already gets, so no combination of sources can
    // make a character literally unhittable.
    // Rogue's Vanish - a temporary self evasion buff from a recent crit
    // (see `temp_evasion_buff`'s doc), fed in as one more positive source
    // alongside `def.evasion` itself.
    let vanish_buff = if def.temp_evasion_buff > 0.0 && at_ms <= def.temp_evasion_buff_expires_at_ms { def.temp_evasion_buff } else { 0.0 };
    // Silent Steps - live evasion bonus per active Fleetfoot stack.
    let silentsteps_bonus =
        if def.stack_evasion_per_stack > 0.0 && at_ms <= def.stack_speed_expires_at_ms { def.stack_speed_current as f64 * def.stack_evasion_per_stack } else { 0.0 };
    // Full-detail combat log (2026-08-17, Phase 2) - named alongside the
    // plain values so each contributing source can be logged as its own
    // `RollEvent` (category `Evasion`, same bucket the evasion ROLL
    // itself uses - the roll-vs-deterministic distinction lives in which
    // `HitOutcome` field a source ends up in, not the category). Same
    // "only log what's actually non-zero" filter as everywhere else.
    let mut evasion_sources: Vec<(&'static str, f64)> = Vec::new();
    if def.evasion > 0.0 {
        evasion_sources.push(("Evasion", def.evasion));
    }
    if nightstalker_bonus > 0.0 {
        evasion_sources.push(("Nightstalker", nightstalker_bonus));
    }
    if pack_instinct_evasion_bonus > 0.0 {
        evasion_sources.push(("Pack Instinct", pack_instinct_evasion_bonus));
    }
    if vanish_buff > 0.0 {
        evasion_sources.push(("Vanish", vanish_buff));
    }
    if silentsteps_bonus > 0.0 {
        evasion_sources.push(("Silent Steps", silentsteps_bonus));
    }
    let evasion_combine_values: Vec<f64> = evasion_sources.iter().map(|(_, v)| *v).collect();
    // Hard cap at 95% (2026-08-17, a live request) - each individual
    // source is already capped at 75% on its own (`capped_stat_with_overflow`), but MULTIPLE 75% sources combined multiplicatively
    // (e.g. gear + Nightstalker + Pack Instinct) can still exceed that -
    // two 75% sources alone already combine to 93.75%. This is a real
    // ceiling on the COMBINED result specifically, not a change to any
    // individual source's own cap.
    let pre_boss_evasion = combine_reduction_sources(&evasion_combine_values).min(0.95) - unbroken_ignore - frostnova_debuff;
    // Base boss buff - see `boss_defense_ignore`'s own doc. Floored
    // relative to this defender's OWN pre-boss value: never pushed below
    // 25% by the boss's ignore effect specifically if they naturally had
    // at least that much, never artificially raised up to 25% if they
    // didn't (a floor, not a guarantee).
    let boss_ignore_evasion = boss_defense_ignore(atk, at_ms);
    let base_evasion = (pre_boss_evasion - boss_ignore_evasion).max(pre_boss_evasion.min(0.25));
    // Unbroken/Frost Nova/Boss Pressure are post-combine adjustments, not
    // combine inputs (see above) - logged as their own negative-magnitude
    // sources rather than folded into the combine's own named vec.
    if unbroken_ignore > 0.0 {
        evasion_sources.push(("Unbroken (evasion-ignore)", -unbroken_ignore));
    }
    if frostnova_debuff > 0.0 {
        evasion_sources.push(("Frost Nova", -frostnova_debuff));
    }
    if boss_ignore_evasion > 0.0 {
        evasion_sources.push(("Boss Pressure", -boss_ignore_evasion));
    }
    // Converted once here (rather than at each `HitOutcome` construction
    // site below) since `evasion_sources` itself is only used to build
    // `evasion_combine_values` above - this is the single place that
    // needs to survive to every return point, cloned where more than one
    // does.
    let evasion_roll_sources: Vec<(RollCategory, &'static str, f64)> = evasion_sources.into_iter().map(|(name, mag)| (RollCategory::Evasion, name, mag)).collect();
    // Rogue's Opportunist - your first N hits each fight (N = the skill's
    // own rank) are guaranteed to land: this hit skips the evasion roll
    // AND the block roll entirely (see the block-roll site below) if it's
    // still within that guaranteed count. `hits_landed_this_fight` is only
    // ever incremented for a LANDED hit (see `apply_hit`'s doc), so it's
    // exactly "how many of my guaranteed hits have I already used."
    let opportunist_base = atk.hits_landed_this_fight < atk.opportunist_guaranteed_hits;
    // Opening Move - a cooldown-gated RECHARGE of the same guaranteed-
    // landing treatment, independent of the fight-opening budget above.
    // Whether this fired is re-derived (cheaply, no roll involved) by
    // `apply_hit` right after this call to decide whether to consume the
    // cooldown - see that call site's own doc.
    let openingmove_ready = !opportunist_base && atk.openingmove_cooldown_ms > 0 && at_ms >= atk.next_openingmove_at_ms;
    // Cold Steel - a pending debuff left on THIS defender by an earlier
    // guaranteed-landing hit (from ANY ally, not necessarily this
    // attacker) has a chance to pass the same treatment along. Consumed
    // (cleared) by `apply_hit` right after this call, on this attempt,
    // whether or not the roll actually succeeds.
    let coldsteel_eligible = !opportunist_base && !openingmove_ready && def.coldsteel_pending;
    let coldsteel_triggered = if coldsteel_eligible {
        let chance = def.coldsteel_pass_chance_pending.clamp(0.0, 1.0);
        let rolled = rng.gen_bool(chance);
        probabilistic_rolls.push((RollCategory::GuaranteedHit, "Cold Steel pass-along", Some(chance), rolled));
        rolled
    } else {
        false
    };
    let opportunist_guaranteed = opportunist_base || openingmove_ready || coldsteel_triggered;
    // The floor/ceiling only ever bounds what THIS debuff/buff itself can
    // do - `.min(FLOOR)`/`.max(CEILING)` against the PRE-adjustment value
    // means an already-below-25%/-above-75% stat (for any other reason)
    // is left exactly where it was, never artificially dragged toward
    // 25%/75% by a debuff/buff that isn't actually the thing pushing it
    // there.
    let evasion_after_elemental_debuff = if target_cold_debuff > 0.0 { (base_evasion - target_cold_debuff).max(base_evasion.min(ELEMENTAL_DEFENSE_FLOOR)) } else { base_evasion };
    let evasion_after_elemental =
        if target_cold_buff > 0.0 { (evasion_after_elemental_debuff + target_cold_buff).min(evasion_after_elemental_debuff.max(ELEMENTAL_DEFENSE_CEILING)) } else { evasion_after_elemental_debuff };
    // Monk's Chakra of Life - full damage immunity for the granted window
    // (see the "would-kill" branch in `apply_hit` that sets
    // `chakraoflife_immune_until_ms`). Checked BEFORE the normal evasion
    // roll and short-circuits the same way a real evade does - zero
    // damage, no crit/mitigation, none of the on-hit effects below ever
    // run - so an already-immune target can't re-trigger the "would-kill"
    // branch again either (a 0-damage hit is never `>= hp`). Overrides
    // even a guaranteed (`opportunist_guaranteed`) hit, since immunity is
    // a stronger guarantee than any offensive "can't miss" effect.
    if at_ms <= def.chakraoflife_immune_until_ms {
        return HitOutcome {
            damage: 0,
            unmitigated_damage,
            is_crit: false,
            evaded: true,
            is_blocked: false,
            opportunist_guaranteed,
            curse_bonus_damage: 0.0,
            probabilistic_rolls,
            deterministic_sources: evasion_roll_sources
                .clone()
                .into_iter()
                .chain(attacker_roll.deterministic_sources.clone())
                .chain(attacker_side_debuffs.clone())
                .collect(),
        };
    }
    // Druid's Pack Instinct - +evasion while THIS unit is the party's
    // current lowest-HP ally (see `apply_hit`'s live computation).
    if !opportunist_guaranteed {
        let chance = evasion_after_elemental.clamp(0.0, 1.0);
        let evaded = rng.gen_bool(chance);
        probabilistic_rolls.push((RollCategory::Evasion, "Evasion", Some(chance), evaded));
        if evaded {
            return HitOutcome {
                damage: 0,
                unmitigated_damage,
                is_crit: false,
                evaded: true,
                is_blocked: false,
                opportunist_guaranteed,
                curse_bonus_damage: 0.0,
                probabilistic_rolls,
                deterministic_sources: evasion_roll_sources
                    .into_iter()
                    .chain(attacker_roll.deterministic_sources)
                    .chain(attacker_side_debuffs)
                    .collect(),
            };
        }
    }
    // Block and damage reduction are separate SOURCES, combined via
    // `combine_reduction_sources` rather than each taking their own
    // sequential cut - see that function's doc for why (and for where a
    // future 3rd source, the passive tree, plugs in). Named (2026-08-17,
    // Phase 2) so each can be logged as its own `RollEvent` (category
    // `Mitigation`) - `combine_reduction_sources` itself still only ever
    // sees the plain `&[f64]` derived below, its signature is untouched.
    let mut sources: Vec<(&'static str, f64)> = Vec::new();
    // Elemental damage rework - Chaos's block-chance debuff/buff, same
    // "own floor/ceiling, applied before the roll" shape as Cold's
    // evasion adjustment above.
    let block_after_elemental_debuff = if target_chaos_debuff > 0.0 { (def.block_chance - target_chaos_debuff).max(def.block_chance.min(ELEMENTAL_DEFENSE_FLOOR)) } else { def.block_chance };
    let block_after_elemental_buff =
        if target_chaos_buff > 0.0 { (block_after_elemental_debuff + target_chaos_buff).min(block_after_elemental_debuff.max(ELEMENTAL_DEFENSE_CEILING)) } else { block_after_elemental_debuff };
    // Berserker's Shatter - Overwhelm's live shred also reduces the
    // DEFENDER's block chance for THIS roll, by the same amount.
    let pre_boss_block =
        if atk.shatter_shred_pct > 0.0 { block_after_elemental_buff - stack_shred_bonus(atk, at_ms) * atk.shatter_shred_pct } else { block_after_elemental_buff };
    // Base boss buff - see `boss_defense_ignore`'s own doc. Same relative-
    // floor shape as evasion above; a no-op (leaves `pre_boss_block`
    // untouched) whenever `atk` isn't a boss, since `boss_ignore_block`
    // is 0.0 then.
    let boss_ignore_block = boss_defense_ignore(atk, at_ms);
    let block_after_elemental = (pre_boss_block - boss_ignore_block).max(pre_boss_block.min(0.25));
    // Warrior's Stonewall - this unit's first N hits TAKEN each fight are
    // automatically blocked (N = rank), bypassing the block roll the same
    // way Opportunist bypasses evasion/block above.
    let stonewall_auto_block = def.hits_taken_this_fight < def.stonewall_auto_block_hits;
    let block_rolled = if stonewall_auto_block {
        false
    } else if !opportunist_guaranteed {
        let chance = block_after_elemental.clamp(0.0, 1.0);
        let rolled = rng.gen_bool(chance);
        probabilistic_rolls.push((RollCategory::Block, "Block chance", Some(chance), rolled));
        rolled
    } else {
        false
    };
    let is_blocked = stonewall_auto_block || block_rolled;
    if stonewall_auto_block {
        // Deterministic, not a probability roll - still worth a
        // `RollEvent` (category Block, `probability` `None`) so the log
        // can tell "blocked because Stonewall's auto-block was still
        // active" apart from "blocked because the roll actually
        // succeeded" rather than treating both the same.
        probabilistic_rolls.push((RollCategory::Block, "Stonewall auto-block", None, true));
    }
    if is_blocked {
        // Second Skin - overrides the flat block-reduction constant with
        // this unit's own rank-scaled value.
        sources.push(("Block reduction (Second Skin)", def.block_damage_reduction_pct));
    }
    // Ambush - a guaranteed-landing hit also ignores a fraction of the
    // target's damage reduction (a negative source, same slot Overwhelm's
    // shred/Curse of Weakness already use) - at 3/3 Ambush + 3/3
    // Opportunist, the 3rd guaranteed hit fully bypasses DR, matching the
    // node's own "cannot be stopped or reduced" text.
    if opportunist_guaranteed {
        // Cold Steel passes along the ORIGINAL rogue's own Ambush value
        // (stored on the debuff), not this consuming attacker's - they
        // may not have Ambush invested themselves at all.
        let ambush_pct = if coldsteel_triggered { def.coldsteel_ambush_pct_pending } else { atk.ambush_dr_cut_pct };
        if ambush_pct > 0.0 {
            sources.push(("Ambush", -ambush_pct));
        }
    }
    // Unlike block, damage reduction alone allows negative - see
    // Character::combat_damage_reduction's doc. A negative value means
    // this source genuinely deals MORE damage, not just "no reduction";
    // combine_reduction_sources still floors it at -0.75 so it can't
    // push the combined multiplier past sanity. Elemental damage rework -
    // Fire's damage-reduction debuff/buff, same "own floor/ceiling"
    // shape as Cold/Chaos above, applied to the base value going INTO
    // `sources` rather than as a separate source of its own (this stat's
    // multiple sources already combine multiplicatively via
    // `combine_reduction_sources`, unlike evasion/block's single roll -
    // folding it in here keeps it part of THIS unit's own DR figure
    // instead of becoming a same-weighted extra source).
    let dr_after_elemental_debuff = if target_fire_debuff > 0.0 { (def.damage_reduction - target_fire_debuff).max(def.damage_reduction.min(ELEMENTAL_DEFENSE_FLOOR)) } else { def.damage_reduction };
    let dr_after_elemental =
        if target_fire_buff > 0.0 { (dr_after_elemental_debuff + target_fire_buff).min(dr_after_elemental_debuff.max(ELEMENTAL_DEFENSE_CEILING)) } else { dr_after_elemental_debuff };
    if dr_after_elemental != 0.0 {
        sources.push(("Damage reduction", dr_after_elemental));
    }
    // Warrior's Hardened - a persistent, never-decaying stacking DR bonus
    // built up from prior Retaliations this fight.
    if def.hardened_stacks > 0 {
        sources.push(("Hardened", def.hardened_stacks as f64 * def.hardened_pct_per_stack));
    }
    // Warrior's Defiance - live DR bonus while below Last Stand's own
    // 25%-HP threshold.
    if def.laststand_defiance_pct > 0.0 && def.max_hp > 0 && (def.hp as f64 / def.max_hp as f64) < 0.25 {
        sources.push(("Defiance", def.laststand_defiance_pct));
    }
    // Warrior's Immovable - extra DR specifically against a critical hit.
    if is_crit && def.immovable_crit_dr_pct > 0.0 {
        sources.push(("Immovable", def.immovable_crit_dr_pct));
    }
    // Berserker's Second Gale - temporarily cancels out Reckless Swing/
    // Death Wish's own extra-damage-taken penalty (which is otherwise
    // baked permanently into `damage_reduction` - see
    // `reckless_penalty_offset`'s doc) while its window is active.
    if def.reckless_penalty_offset > 0.0 && at_ms <= def.temp_reckless_immunity_expires_at_ms {
        sources.push(("Second Gale", def.reckless_penalty_offset));
    }
    // Lightning's damage-TAKEN stack - a negative source, same "unit's
    // own damage-taken debuff, no attacker-identity check" shape as
    // Curse of Weakness's own negative source below.
    if target_lightning_dmg_taken > 0.0 {
        sources.push(("Lightning damage-taken stack", -target_lightning_dmg_taken));
    }
    // Druid's Symbiosis - same live lowest-HP-ally gate as Pack Instinct,
    // its own separate mitigation source.
    if symbiosis_dr_bonus > 0.0 {
        sources.push(("Symbiosis", symbiosis_dr_bonus));
    }
    // Divine Intervention - a temporary bonus for whoever Guardian Spirit
    // just saved (lazy-expiry, same convention as every other timed
    // field on `CombatSimUnit`).
    if def.temp_damage_reduction_bonus > 0.0 && at_ms <= def.temp_damage_reduction_bonus_expires_at_ms {
        sources.push(("Guardian Spirit (Divine Intervention)", def.temp_damage_reduction_bonus));
    }
    // Paladin's Unwavering / Cleric's Unyielding Faith (2026-08-17,
    // shared mechanic - see `low_hp_party_dr_pct`'s doc) - a party-wide
    // broadcast, separate field from the self/ally-targeted
    // `temp_damage_reduction_bonus` above.
    if def.temp_party_damage_reduction_bonus > 0.0 && at_ms <= def.temp_party_damage_reduction_bonus_expires_at_ms {
        sources.push(("Unwavering/Unyielding Faith", def.temp_party_damage_reduction_bonus));
    }
    // Balanced Faith - +damage reduction while Overflowing Grace's shield
    // is still active on this unit (the same `shield_hp`/
    // `shield_expires_at_ms` pool Martyrdom's shield also uses).
    if def.overflow_grace_shield_dr_pct > 0.0 && def.shield_hp > 0.0 && at_ms <= def.shield_expires_at_ms {
        sources.push(("Balanced Faith", def.overflow_grace_shield_dr_pct));
    }
    // Nature's Ward (2026-08-16 rework) - a multiplicative DR source
    // gated to only apply when the ATTACKER is a boss, same
    // `if atk.is_boss { def.X }` pattern Hunter's Instinct/Nightstalker
    // already use elsewhere in this function.
    let naturesward_bonus = if atk.is_boss { def.naturesward_dr_vs_boss_pct } else { 0.0 };
    if naturesward_bonus > 0.0 {
        sources.push(("Nature's Ward", naturesward_bonus));
    }
    // Overwhelm - a NEGATIVE source (see `stack_shred_bonus`'s doc),
    // scaled off the ATTACKER's own current Bloodlust stack count, not
    // the defender's anything - reduces the defender's effective
    // mitigation for this hit specifically.
    let mut shred = stack_shred_bonus(atk, at_ms);
    // Exposed - keeps reading a stale (post-Bloodlust-expiry) stack count
    // for a little longer, purely for the shred's own purposes.
    if shred == 0.0 && atk.overwhelm_shred_linger_ms > 0 && at_ms <= atk.stack_speed_expires_at_ms + atk.overwhelm_shred_linger_ms {
        shred = atk.stack_shred_per_stack * atk.stack_speed_current as f64;
    }
    // Crush - doubles the shred against a target whose OWN current DR is
    // already below the threshold.
    if shred > 0.0 && atk.crush_dr_threshold > 0.0 && def.damage_reduction < atk.crush_dr_threshold {
        shred *= 2.0;
    }
    if shred > 0.0 {
        sources.push(("Overwhelm/Crush shred", -shred));
    }
    // Monk's Vital Points - live target-DR-shred per active Flowing
    // Strikes stack.
    if atk.vitalpoints_shred_per_stack > 0.0 && at_ms <= atk.flowing_expires_at_ms {
        sources.push(("Vital Points", -(atk.flowing_current as f64 * atk.vitalpoints_shred_per_stack)));
    }
    // Monk's Crippling Grip (Last Bastion, 2026-08-17) - a second,
    // independent conversion channel off the SAME evasion-overflow pool
    // Unbroken's own evasion-ignore draws from (see
    // `unbroken_crippling_grip_dr_pct`'s doc), applied as a flat DR shred
    // instead - unconditional once invested, no live gate needed (unlike
    // Vital Points above, which scales with a live stack count).
    if atk.unbroken_crippling_grip_dr_pct > 0.0 {
        sources.push(("Crippling Grip", -atk.unbroken_crippling_grip_dr_pct));
    }
    // Warlock's Curse of Weakness - a NEGATIVE source too, but unlike
    // Overwhelm's shred (scaled off the attacker) this is unconditional
    // once applied to `def` - ANY attacker's hit against a cursed target
    // benefits, no source-identity check (see `curse_dmg_taken_bonus`'s
    // doc for why it never needs `mark_source_id`). Its OWN index into
    // `sources` is recorded (not assumed to be the last entry - Predator's
    // own source, pushed just below, would break that assumption whenever
    // both are active on the same target) so `curse_bonus_damage` below
    // can drop exactly this one entry back out again, regardless of
    // whatever else gets pushed after it.
    let curse_source_idx = if def.curse_dmg_taken_bonus > 0.0 { Some(sources.len()) } else { None };
    if curse_source_idx.is_some() {
        sources.push(("Curse of Weakness", -def.curse_dmg_taken_bonus));
    }
    // Rogue's Predator - a live, unconditional damage-taken debuff on
    // whoever a guaranteed-landing hit just struck, same "any attacker
    // benefits" shape as Curse of Weakness above.
    if def.predator_dmg_taken_bonus > 0.0 && at_ms <= def.predator_expires_at_ms {
        sources.push(("Predator", -def.predator_dmg_taken_bonus));
    }
    // Gelatinous Cube's shred - same unconditional "any attacker benefits
    // once applied to def" shape as Curse of Weakness/Predator above. The
    // `.min(0.5)` here is redundant with the `.min(CUBE_SHRED_MAX_STACKS)`
    // cap already enforced where stacks accumulate (in `apply_hit`), kept
    // as explicit defense-in-depth matching the request's literal
    // "clamping to 50%" wording.
    if def.cube_shred_stacks > 0 && at_ms <= def.cube_shred_expires_at_ms {
        sources.push(("Gelatinous Cube shred", -(def.cube_shred_stacks as f64 * CUBE_SHRED_PCT_PER_STACK).min(0.5)));
    }
    // Base boss buff - see `boss_defense_ignore`'s own doc. Snapshotted
    // BEFORE this source is pushed so the floor below can tell "how much
    // DR this defender had without any boss pressure at all" apart from
    // "how much survives once it's included" - same "snapshot, then
    // recompute" idiom `curse_bonus_damage` below already uses for an
    // analogous marginal-contribution question.
    // Hard cap at 95% (2026-08-17, a live request) - Block's own reduction
    // (pushed into `sources` above when `is_blocked`) combines through
    // this SAME call alongside every DR source, so "cap Block+DR combined
    // mitigation" is just capping this one combined value - a landed
    // (non-evaded) hit always deals at least 5% of its raw damage,
    // however stacked a defender's block+DR sources get.
    let dr_pre_boss = combine_reduction_sources(&sources.iter().map(|(_, v)| *v).collect::<Vec<_>>()).min(0.95);
    let boss_ignore_dr = boss_defense_ignore(atk, at_ms);
    if boss_ignore_dr > 0.0 {
        sources.push(("Boss Pressure", -boss_ignore_dr));
    }
    // `combine_reduction_sources` returns the combined REDUCTION fraction
    // (0.875 for "87.5% reduced" per its own doc) - damage that actually
    // gets through is the COMPLEMENT of that, `1.0 - reduction`. Critical
    // live bug found here: this used to multiply by the reduction
    // fraction directly, which is exactly backwards - a character with
    // ZERO damage reduction/block (reduction fraction ~0) took ZERO
    // damage on every non-evaded hit, while stacking DR/block gear made a
    // character take MORE damage, up toward the raw unmitigated amount at
    // max mitigation. Confirmed against a live fight log: a player
    // running pure evasion with no DR/block affixes at all was taking
    // exactly 0 damage on every one of 285 non-evaded hits in a single
    // fight, including several individual hits over 15,000 unmitigated.
    let source_values: Vec<f64> = sources.iter().map(|(_, v)| *v).collect();
    // Floored the same relative way as evasion/block above - the boss's
    // OWN pressure can never push this defender's effective DR below 25%
    // if they naturally had at least that much, never raises it if they
    // didn't.
    let dr_combined = combine_reduction_sources(&source_values).min(0.95).max(dr_pre_boss.min(0.25));
    let mut dmg = raw_dmg * (1.0 - dr_combined);
    // The real boss's survivability-focus debuff (see
    // `boss_focus_stacks`) - a target-side vulnerability, applied last,
    // after every other mitigation has already taken its cut.
    dmg *= 1.0 + def.boss_focus_stacks;
    // Curse of Weakness family (2026-08-16, see `HitOutcome::curse_bonus_damage`'s
    // doc) - the marginal damage that ONLY exists because the curse's own
    // negative source was in the mitigation combine above. Recomputed by
    // dropping exactly that one source (by its recorded index, not
    // position) and re-running the exact same combine/boss-focus pipeline
    // on the hypothetical no-curse reduction, so this stays correct
    // regardless of how many OTHER sources are present (Predator's
    // included) or how `combine_reduction_sources` weighs them against
    // each other.
    let curse_bonus_damage = if let Some(idx) = curse_source_idx {
        let source_values_without_curse: Vec<f64> = source_values.iter().enumerate().filter(|(i, _)| *i != idx).map(|(_, &v)| v).collect();
        let dmg_without_curse = raw_dmg * (1.0 - combine_reduction_sources(&source_values_without_curse)) * (1.0 + def.boss_focus_stacks);
        (dmg - dmg_without_curse).max(0.0)
    } else {
        0.0
    };
    // Full-detail combat log (2026-08-17, Phase 2) - the named DR/block
    // sources (category `Mitigation`) plus the evasion sources captured
    // earlier (category `Evasion`, logged here too since a hit that
    // DIDN'T evade still rolled - and was influenced by - the same
    // evasion figure).
    let mut deterministic_sources: Vec<(RollCategory, &'static str, f64)> = evasion_roll_sources;
    deterministic_sources.extend(sources.into_iter().map(|(name, mag)| (RollCategory::Mitigation, name, mag)));
    deterministic_sources.extend(attacker_roll.deterministic_sources);
    deterministic_sources.extend(attacker_side_debuffs);
    HitOutcome {
        damage: dmg.round().max(0.0) as u64,
        unmitigated_damage,
        is_crit,
        evaded: false,
        is_blocked,
        opportunist_guaranteed,
        curse_bonus_damage,
        probabilistic_rolls,
        deterministic_sources,
    }
}

/// Grants (or tops up) `units[target_idx]`'s shield - the single place
/// every shield source (Martyrdom, Overflowing Grace, Divine Favor)
/// should go through, rather than each hand-rolling `shield_hp += ...`
/// itself. Critically, this clears a STALE shield first: `shield_hp`
/// isn't reset to 0 anywhere when it lazily expires (the read sites just
/// stop using it once `at_ms > shield_expires_at_ms`, same convention as
/// wound stacks) - a live audit found every grant site adding onto
/// whatever `shield_hp` happened to still be sitting there, so an
/// expired-and-never-consumed shield could get silently "resurrected"
/// and re-extended by the next grant, indefinitely. Compare to how Open
/// Wound's own stack refresh explicitly resets to 0 first when stale
/// (`apply_hit`'s wound block below) - this is the same fix, applied to
/// the shield pool.
pub(crate) fn grant_shield(units: &mut [CombatSimUnit], healer_idx: usize, target_idx: usize, amount: f64, at_ms: u32, duration_ms: u32, events: &mut Vec<CombatEvent>) {
    if amount <= 0.0 {
        return;
    }
    if at_ms > units[target_idx].shield_expires_at_ms {
        units[target_idx].shield_hp = 0.0;
    }
    // Clamped to 400% of the target's own max hp (raised from 100% ->
    // 300% -> 400% across a series of live requests on 2026-08-16) - a
    // single shared cap here covers every grant site at once (Divine
    // Shield, Overflowing Grace, Seed of Life, Arcane Shield,
    // Consecration, etc.), same "one shared formula" principle as
    // `Item::disenchant_multiplier`'s own doc. Logs only the amount that
    // ACTUALLY landed (post-clamp), not the raw requested amount - an
    // already-capped target grants nothing further and logs no event at
    // all, so `summarize_fight`'s "healing done" stat never overstates
    // what a shield genuinely contributed.
    let before = units[target_idx].shield_hp;
    let cap = units[target_idx].max_hp as f64 * 4.0;
    units[target_idx].shield_hp = (before + amount).min(cap.max(0.0));
    units[target_idx].shield_expires_at_ms = at_ms + duration_ms;
    let applied = units[target_idx].shield_hp - before;
    if applied <= 0.0 {
        return;
    }
    // A shield is a source of healing too (see `CombatEvent::Shield`'s
    // doc) - reported here, centrally, so every grant site gets this for
    // free instead of needing to remember it individually.
    let healer_id = units[healer_idx].id.clone();
    let target_id = units[target_idx].id.clone();
    events.push(CombatEvent::Shield { at_ms, healer: healer_id, target: target_id, amount: applied.round().max(0.0) as u64 });
}

/// Applies a REFLECTED hit - a flat amount already computed by the
/// caller (no crit/evasion roll of its own, no mitigation against the
/// reflect's own target - reflected damage is true damage, same
/// convention as Slayer's self-leech). Deliberately NOT routed back
/// through `apply_hit` - that would re-check the reflect target's OWN
/// shield/reflect fields, letting two shielded+reflecting units bounce
/// the same hit back and forth into each other indefinitely. Still
/// writes hp, pushes an `Attack` event, and fires `Defeat`/`fire_on_kill`
/// if it's lethal, so a reflect that finishes off an attacker behaves
/// like any other killing blow.
pub(crate) fn apply_reflect_damage(units: &mut [CombatSimUnit], source_idx: usize, target_idx: usize, amount: f64, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
    // Monk's Chakra of Life - true damage still respects full immunity.
    if amount <= 0.0 || !units[target_idx].alive || at_ms <= units[target_idx].chakraoflife_immune_until_ms {
        return;
    }
    let source_id = units[source_idx].id.clone();
    let hit_id = next_hit_id();
    let penalized = apply_late_stage_penalty(units, target_idx, amount, at_ms, hit_id, &source_id, rolls);
    let final_damage = penalized.round().max(0.0) as i64;
    let new_hp = (units[target_idx].hp - final_damage).max(0);
    units[target_idx].hp = new_hp;
    units[source_idx].damage_dealt_total += final_damage.max(0) as u64;
    let target_id = units[target_idx].id.clone();
    events.push(CombatEvent::Attack {
        at_ms,
        attacker: source_id,
        target: target_id.clone(),
        damage: final_damage.max(0) as u64,
        unmitigated_damage: final_damage.max(0) as u64,
        target_hp_after: new_hp as u64,
        is_crit: false,
        evaded: false,
        hit_id,
    });
    if new_hp == 0 {
        units[target_idx].alive = false;
        events.push(CombatEvent::Defeat { at_ms, unit: target_id });
        fire_on_kill(units, source_idx, at_ms, events, rolls, rng);
        trigger_doom_on_death(units, target_idx, at_ms, events, rolls, rng);
    }
}

/// Mage's Volatile Magic (2026-08-17, reworked from a real splash HIT to
/// true damage - see `CombatSimUnit::volatilemagic_splash_pct`'s doc for
/// why): a crit splashes a flat, already-computed amount to up to
/// `VOLATILE_MAGIC_MAX_TARGETS` other enemies on the same side as
/// `primary_target_idx`. Deliberately mirrors `apply_reflect_damage`'s
/// shape (real hp change, a real `Attack` event, fires on-kill/Doom on a
/// kill) rather than `apply_splash`'s (which routes through `apply_hit` -
/// a real hit that independently rolls crit/mitigation and can trigger
/// wound/leech/other on-hit reactions) - explicitly "this is not a hit
/// and will not trigger any other on-hit effects" per the live request,
/// and structurally CANNOT re-trigger Volatile Magic itself since it
/// never touches `resolve_hit`/`outcome.is_crit` at all.
pub(crate) fn apply_volatile_magic_splash(
    units: &mut [CombatSimUnit],
    attacker_idx: usize,
    primary_target_idx: usize,
    amount_per_target: f64,
    max_targets: usize,
    at_ms: u32,
    events: &mut Vec<CombatEvent>,
    rolls: &mut Vec<RollEvent>,
    rng: &mut impl Rng,
) {
    if amount_per_target <= 0.0 || max_targets == 0 {
        return;
    }
    let target_side_is_boss = units[primary_target_idx].is_boss;
    let mut candidates: Vec<usize> =
        units.iter().enumerate().filter(|(i, u)| *i != primary_target_idx && u.is_boss == target_side_is_boss && u.alive).map(|(i, _)| i).collect();
    let pick_count = max_targets.min(candidates.len());
    for _ in 0..pick_count {
        let pick_at = rng.gen_range(0..candidates.len());
        let target_idx = candidates.remove(pick_at);
        // Monk's Chakra of Life - true damage still respects full immunity.
        if !units[target_idx].alive || at_ms <= units[target_idx].chakraoflife_immune_until_ms {
            continue;
        }
        let attacker_id = units[attacker_idx].id.clone();
        let hit_id = next_hit_id();
        let penalized = apply_late_stage_penalty(units, target_idx, amount_per_target, at_ms, hit_id, &attacker_id, rolls);
        let final_damage = penalized.round().max(0.0) as i64;
        if final_damage <= 0 {
            continue;
        }
        let new_hp = (units[target_idx].hp - final_damage).max(0);
        units[target_idx].hp = new_hp;
        units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
        let target_id = units[target_idx].id.clone();
        events.push(CombatEvent::Attack {
            at_ms,
            attacker: attacker_id,
            target: target_id.clone(),
            damage: final_damage.max(0) as u64,
            unmitigated_damage: final_damage.max(0) as u64,
            target_hp_after: new_hp as u64,
            is_crit: false,
            evaded: false,
            hit_id,
        });
        if new_hp == 0 {
            units[target_idx].alive = false;
            events.push(CombatEvent::Defeat { at_ms, unit: target_id });
            fire_on_kill(units, attacker_idx, at_ms, events, rolls, rng);
            trigger_doom_on_death(units, target_idx, at_ms, events, rolls, rng);
        }
    }
}

/// Lingering Effect - spawns a new DoT (or HoT) instance on
/// `units[target_idx]` off `units[source_idx]`'s own magnitude and this
/// action's own pre-mitigation `base_amount` (per the design's own worked
/// example: 100 pre-defense damage x 4% = 4 total over
/// `LINGERING_EFFECT_TICKS` ticks - the heal flavor works identically,
/// just off the heal's own pre-cap amount instead). Called from BOTH
/// `apply_hit` (`is_heal: false`, `target_idx` is whoever got struck) and
/// `apply_heal` (`is_heal: true`, `target_idx` is whoever got healed) -
/// deliberately symmetric, per a live correction: Lingering Effect was
/// NOT meant to be damage-only, every action it triggers from gets its
/// own matching-flavor lingering instance. A no-op without
/// `lingering_effect_pct` invested. "Independent stacking" - always
/// PUSHES a new instance, never merges with/refreshes an already-active
/// one from the same or a different source (see `lingering_dots`' doc for
/// why).
pub(crate) fn apply_lingering_effect(units: &mut [CombatSimUnit], source_idx: usize, target_idx: usize, base_amount: f64, is_heal: bool, at_ms: u32) {
    let pct = units[source_idx].lingering_effect_pct;
    if pct <= 0.0 || base_amount <= 0.0 {
        return;
    }
    let amount_per_tick = (base_amount * pct) / LINGERING_EFFECT_TICKS as f64;
    if amount_per_tick <= 0.0 {
        return;
    }
    let source_id = units[source_idx].id.clone();
    let first_tick_at_ms = at_ms + LINGERING_EFFECT_TICK_INTERVAL_MS;
    units[target_idx].lingering_dots.push(LingeringDot { source_id, amount_per_tick, remaining_ticks: LINGERING_EFFECT_TICKS, next_tick_at_ms: first_tick_at_ms, is_heal });
    units[target_idx].next_lingering_tick_at_ms = units[target_idx].next_lingering_tick_at_ms.min(first_tick_at_ms);
}

/// Resolves every `lingering_dots` entry on `units[target_idx]` that's due
/// at or before `at_ms` (almost always exactly one, but a target hit by
/// several Lingering Effect sources in the same second can have more) -
/// a damage-flavor tick rolls its own flat-damage-reduction-only
/// mitigation (no block/evasion, per the design's own "unavoidable"
/// requirement, and no shield absorption/Guardian Spirit-style death-
/// prevention either - a deliberately simpler "true damage" pipeline,
/// same precedent `apply_reflect_damage`'s own doc already established
/// for a secondary damage source); a heal-flavor tick has no mitigation
/// concept at all and just restores hp, capped at max (same as any other
/// heal). Reschedules `next_lingering_tick_at_ms` to whatever's now
/// soonest (`u32::MAX` if nothing's left), and drops any instance that
/// just took its last tick.
pub(crate) fn tick_lingering_dots(units: &mut [CombatSimUnit], target_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
    if !units[target_idx].alive {
        units[target_idx].next_lingering_tick_at_ms = u32::MAX;
        return;
    }
    // Flat damage reduction only - the same handful of sources
    // `resolve_hit` combines minus block/evasion (which the design
    // explicitly excludes) and minus the live Pack Instinct/Symbiosis
    // lowest-HP-ally check (no natural "attacker" for a DoT tick to
    // evaluate that against - a documented scope simplification). Only
    // ever consulted for a damage-flavor tick below.
    let def = &units[target_idx];
    // Named (2026-08-17, Phase 2) - same treatment as `resolve_hit`'s own
    // (much larger) DR combine, just this function's own smaller 4-source
    // set (no block/evasion, by design - see this function's own doc).
    let mut sources: Vec<(&'static str, f64)> = Vec::new();
    if def.damage_reduction != 0.0 {
        sources.push(("Damage reduction", def.damage_reduction));
    }
    if def.temp_damage_reduction_bonus > 0.0 && at_ms <= def.temp_damage_reduction_bonus_expires_at_ms {
        sources.push(("Guardian Spirit (Divine Intervention)", def.temp_damage_reduction_bonus));
    }
    if def.temp_party_damage_reduction_bonus > 0.0 && at_ms <= def.temp_party_damage_reduction_bonus_expires_at_ms {
        sources.push(("Unwavering/Unyielding Faith", def.temp_party_damage_reduction_bonus));
    }
    if def.overflow_grace_shield_dr_pct > 0.0 && def.shield_hp > 0.0 && at_ms <= def.shield_expires_at_ms {
        sources.push(("Balanced Faith", def.overflow_grace_shield_dr_pct));
    }
    // Nature's Ward's new (2026-08-16) "vs boss attacker" condition has no
    // meaningful attacker context here - a Lingering Effect DoT tick isn't
    // "a boss attacking," so it deliberately doesn't apply to this
    // mitigation pass at all (the old Unyielding Roots DR-double this
    // block used to include here is gone for the same reason its
    // `resolve_hit` counterpart is - see `unyieldingroots_cycle_ms`'s doc).
    let source_values: Vec<f64> = sources.iter().map(|(_, v)| *v).collect();
    let reduction = combine_reduction_sources(&source_values);

    let mut due_indices = Vec::new();
    for (i, dot) in units[target_idx].lingering_dots.iter().enumerate() {
        if dot.next_tick_at_ms <= at_ms {
            due_indices.push(i);
        }
    }
    for &i in &due_indices {
        if !units[target_idx].alive {
            break;
        }
        let dot = units[target_idx].lingering_dots[i].clone();
        let target_id = units[target_idx].id.clone();
        if dot.is_heal {
            // Direct hp restoration - the base Lingering Effect heal
            // flavor, same for EVERY source (any archetype can roll
            // `Affix::LingeringEffect` on gear, not just Druids). A tick
            // landing on an already-full-HP target is capped at "room"
            // and wasted - a real, known limitation, but NOT something to
            // fix by changing this universal base mechanic (see Seed of
            // Life just below for the actual, correctly-scoped fix).
            let room = (units[target_idx].max_hp as i64 - units[target_idx].hp).max(0);
            let healed = (dot.amount_per_tick.round().max(0.0) as i64).min(room);
            units[target_idx].hp += healed;
            events.push(CombatEvent::Heal { at_ms, healer: dot.source_id.clone(), target: target_id, amount: healed.max(0) as u64, target_hp_after: units[target_idx].hp as u64 });
            // Seed of Life (Druid-specific, 2026-08-16) - the SOURCE's own
            // rate, ADDITIONAL to the direct heal above, not a replacement
            // for it. Off the tick's full `amount_per_tick` (not the
            // HP-room-clamped `healed`), matching "at the same rate" as
            // the heal itself per the request, not diminished by whatever
            // room happened to be left. 0.0 (a no-op) for every source
            // that isn't a Druid with Seed of Life invested.
            if let Some(source_idx) = units.iter().position(|u| u.id == dot.source_id) {
                let seedoflife_pct = units[source_idx].seedoflife_shield_pct;
                if seedoflife_pct > 0.0 {
                    grant_shield(units, source_idx, target_idx, dot.amount_per_tick * seedoflife_pct, at_ms, SEEDOFLIFE_SHIELD_DURATION_MS, events);
                }
            }
            continue;
        }
        // Monk's Chakra of Life - full immunity blocks the damage-flavor
        // tick outright (a heal-flavor tick above is unaffected - only
        // incoming damage is blocked, not healing).
        if at_ms <= units[target_idx].chakraoflife_immune_until_ms {
            continue;
        }
        let hit_id = next_hit_id();
        let penalized_amount = apply_late_stage_penalty(units, target_idx, dot.amount_per_tick, at_ms, hit_id, &dot.source_id, rolls);
        let final_damage = (penalized_amount * (1.0 - reduction)).round().max(0.0) as i64;
        let new_hp = (units[target_idx].hp - final_damage).max(0);
        units[target_idx].hp = new_hp;
        events.push(CombatEvent::Attack {
            at_ms,
            attacker: dot.source_id.clone(),
            target: target_id.clone(),
            damage: final_damage.max(0) as u64,
            unmitigated_damage: dot.amount_per_tick.round().max(0.0) as u64,
            target_hp_after: new_hp as u64,
            is_crit: false,
            evaded: false,
            hit_id,
        });
        for (name, mag) in &sources {
            rolls.push(RollEvent {
                event_id: next_hit_id(),
                hit_id,
                caused_by: None,
                at_ms,
                category: RollCategory::Mitigation,
                source: std::borrow::Cow::Borrowed(*name),
                actor: target_id.clone(),
                target: Some(dot.source_id.clone()),
                probability: None,
                succeeded: None,
                magnitude: Some(*mag),
            });
        }
        if new_hp == 0 {
            units[target_idx].alive = false;
            events.push(CombatEvent::Defeat { at_ms, unit: target_id });
            if let Some(source_idx) = units.iter().position(|u| u.id == dot.source_id) {
                fire_on_kill(units, source_idx, at_ms, events, rolls, rng);
            }
            trigger_doom_on_death(units, target_idx, at_ms, events, rolls, rng);
        }
    }
    // Advance/drop the ticked instances, then reschedule off whatever's
    // left (including any NOT due this pass, and any brand-new instance
    // another hit added mid-loop before this ran - can't happen within
    // one tick resolution here, but the recompute is correct regardless).
    let target = &mut units[target_idx];
    for dot in target.lingering_dots.iter_mut() {
        if dot.next_tick_at_ms <= at_ms {
            dot.remaining_ticks = dot.remaining_ticks.saturating_sub(1);
            dot.next_tick_at_ms += LINGERING_EFFECT_TICK_INTERVAL_MS;
        }
    }
    target.lingering_dots.retain(|d| d.remaining_ticks > 0);
    target.next_lingering_tick_at_ms = target.lingering_dots.iter().map(|d| d.next_tick_at_ms).min().unwrap_or(u32::MAX);
}

/// Warrior's Momentum / Rogue's Fleetfoot - adds one stack to
/// `units[idx]`'s timed attack-speed buff (see `stack_speed_per_stack`'s
/// doc), a no-op if that unit has neither invested (`stack_speed_max_stacks
/// == 0`). Deliberately a simple "any trigger while stacks are still live
/// refreshes the WHOLE stack's expiry and adds one more" - not a per-stack
/// sliding window where each individual stack decays on its own clock -
/// same simplification Frenzy/Helm's stacking fields already use
/// elsewhere, and indistinguishable in practice at a 3-5s window with
/// sub-second attack intervals.
pub(crate) fn add_speed_stack(units: &mut [CombatSimUnit], idx: usize, at_ms: u32) {
    let unit = &mut units[idx];
    if unit.stack_speed_max_stacks == 0 {
        return;
    }
    // Berserker's Neverending - stacks decay one at a time off their own
    // independent timestamps instead of the default shared-expiry
    // all-at-once reset (see `bloodlust_stack_expiries`'s doc).
    if unit.neverending_invested {
        if (unit.bloodlust_stack_expiries.len() as u32) < unit.stack_speed_max_stacks {
            unit.bloodlust_stack_expiries.push(at_ms + unit.stack_speed_duration_ms);
        }
        let current = prune_and_count(&mut unit.bloodlust_stack_expiries, at_ms);
        unit.stack_speed_current = current;
        unit.stack_speed_expires_at_ms = unit.bloodlust_stack_expiries.iter().copied().max().unwrap_or(at_ms);
        return;
    }
    if at_ms > unit.stack_speed_expires_at_ms {
        unit.stack_speed_current = 0;
    }
    unit.stack_speed_current = (unit.stack_speed_current + 1).min(unit.stack_speed_max_stacks);
    unit.stack_speed_expires_at_ms = at_ms + unit.stack_speed_duration_ms;
}

/// Live attack-speed multiplier from `add_speed_stack`'s stacks, read at
/// the point a unit's next action gets scheduled (see the main loop's
/// `units[actor_idx].next_action_at_ms += ...`) rather than baked into
/// `attack_interval_ms` itself - same lazy-expiry-at-the-read-site
/// convention as `temp_damage_reduction_bonus`/`temp_heal_power_bonus`.
/// Returns 1.0 (no change) once the stacks have lazily expired or none
/// were ever invested.
pub(crate) fn speed_stack_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.stack_speed_current == 0 || at_ms > unit.stack_speed_expires_at_ms {
        return 1.0;
    }
    1.0 + unit.stack_speed_current as f64 * unit.stack_speed_per_stack
}

/// Berserker's Bloodlust - live increased-damage bonus from the same
/// stacks `speed_stack_multiplier` reads, consulted at the point a
/// damage roll actually happens (`roll_attacker_damage`) instead of the
/// pre-fight `combat_*` getters, same lazy-expiry convention. 0.0
/// without Bloodlust invested or once the stacks have expired.
pub(crate) fn stack_damage_bonus(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.stack_speed_current == 0 || at_ms > unit.stack_speed_expires_at_ms {
        return 0.0;
    }
    unit.stack_dmg_per_stack * unit.stack_speed_current as f64
}

/// Elemental damage rework (2026-08-15) - shared prune-and-count for
/// every one of the 9 `Vec<u32>` proc-stack fields on `CombatSimUnit`
/// (`fire_dr_debuff`, `lightning_dmg_taken`, etc.) - each entry is one
/// proc's own expiry timestamp, fully independent of every other entry
/// (a live request: "full independent per proc stack durations, it
/// should be difficult to maintain a large stack" - unlike Slayer's
/// Wound, which refreshes ONE shared expiry per hit, every proc here
/// decays on its own clock, so sustaining N stacks needs N procs still
/// within their own individual 4s windows, not just one recent hit).
/// Drops every already-expired entry in place (so the Vec doesn't grow
/// unboundedly across a long fight) and returns how many are still
/// active.
pub(crate) fn prune_and_count(stacks: &mut Vec<u32>, at_ms: u32) -> u32 {
    stacks.retain(|&expires_at| expires_at > at_ms);
    stacks.len() as u32
}

/// Combat logging (2026-08-15, a live request: "a robust log system...
/// live buff/debuff stack counts") - every currently-active live/timed
/// buff, debuff, charge, or stack on `unit` at `at_ms`, named and paired
/// with its current magnitude (a stack/charge COUNT for discrete things,
/// the live percentage/amount for continuous ones) - whatever's actually
/// non-zero and unexpired right now, nothing padded in at 0. Fed into
/// `CombatEvent::BuffSnapshot` (see its own doc) rather than requiring a
/// dedicated log call at every one of these mechanics' own many
/// individual mutation sites - one place reads the same fields every
/// other live-buff consumer here (`resolve_hit`, `roll_attacker_damage`,
/// etc.) already checks directly, so a newly-added timed mechanic only
/// needs one more line here to show up in the log too. Read-only (`&`,
/// not `&mut`) - the elemental `Vec<u32>` fields are counted, not pruned,
/// same "just count what's still live" reasoning `prune_and_count`'s own
/// mutation isn't needed for a snapshot.
pub(crate) fn active_buffs_snapshot(unit: &CombatSimUnit, at_ms: u32) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    {
        let mut push = |name: &str, value: f64| {
            if value > 0.0 {
                out.push((name.to_string(), value));
            }
        };
        // Slayer's Open Wound (defender side).
        if unit.wound_stacks > 0 && at_ms <= unit.wound_expires_at_ms {
            push("wound_stacks", unit.wound_stacks as f64);
        }
        // Momentum/Fleetfoot/Bloodlust/Relentless Pursuit/Flow State
        // (shared per-hit stacking bundle).
        if unit.stack_speed_current > 0 && at_ms <= unit.stack_speed_expires_at_ms {
            push("speed_stacks", unit.stack_speed_current as f64);
        }
        // Monk's Flowing Strikes.
        if unit.flowing_current > 0 && at_ms <= unit.flowing_expires_at_ms {
            push("flowing_stacks", unit.flowing_current as f64);
        }
        // Warlock's Fel Rush.
        if unit.fel_rush_speed_bonus > 0.0 && at_ms <= unit.fel_rush_expires_at_ms {
            push("fel_rush_speed_bonus", unit.fel_rush_speed_bonus);
        }
        // Slayer's Blood Frenzy.
        if unit.flicker_frenzy_speed_bonus > 0.0 && at_ms <= unit.flicker_frenzy_expires_at_ms {
            push("blood_frenzy_speed_bonus", unit.flicker_frenzy_speed_bonus);
        }
        // Slayer's Endless Thirst - `_uncapped` (rank 3) has no magnitude
        // of its own, logged as a flat 1.0 "active" marker instead.
        if (unit.endless_thirst_cap_bonus > 0.0 || unit.endless_thirst_uncapped) && at_ms <= unit.endless_thirst_expires_at_ms {
            push("endless_thirst_cap_bonus", if unit.endless_thirst_uncapped { 1.0 } else { unit.endless_thirst_cap_bonus });
        }
        // Bloodpact - real cooldown now (see `next_bloodpact_at_ms`'s doc),
        // logged as its use count so far this fight rather than a
        // remaining-charges count.
        if unit.bloodpact_cooldown_ms != u32::MAX {
            push("bloodpact_uses_this_fight", unit.bloodpact_uses_this_fight as f64);
        }
        push("guardian_spirit_charges_remaining", unit.guardian_spirit_charges as f64);
        push("assassinate_charges_remaining", unit.assassinate_charges as f64);
        push("undying_will_charges_remaining", unit.frenzy_undying_charges as f64);
        // Generic shield pool (Overflowing Grace/Divine Favor/Martyrdom/
        // Arcane Shield - see `grant_shield`).
        if unit.shield_hp > 0.0 && at_ms <= unit.shield_expires_at_ms {
            push("shield_hp", unit.shield_hp);
        }
        // Temporary buffs/debuffs.
        if unit.temp_heal_power_bonus > 0.0 && at_ms <= unit.temp_heal_power_bonus_expires_at_ms {
            push("temp_heal_power_bonus", unit.temp_heal_power_bonus);
        }
        if unit.temp_damage_reduction_bonus > 0.0 && at_ms <= unit.temp_damage_reduction_bonus_expires_at_ms {
            push("temp_damage_reduction_bonus", unit.temp_damage_reduction_bonus);
        }
        if unit.temp_damage_dealt_debuff > 0.0 && at_ms <= unit.temp_damage_dealt_debuff_expires_at_ms {
            push("temp_damage_dealt_debuff", unit.temp_damage_dealt_debuff);
        }
        if unit.temp_evasion_debuff > 0.0 && at_ms <= unit.temp_evasion_debuff_expires_at_ms {
            push("temp_evasion_debuff", unit.temp_evasion_debuff);
        }
        // Mark/Curse of Weakness - persistent for the rest of the fight
        // once applied, no expiry to check (see `apply_first_hit_mark`'s
        // doc).
        if unit.mark_source_id.is_some() {
            push("marked", 1.0);
        }
        if unit.curse_dmg_taken_bonus > 0.0 {
            push("curse_dmg_taken_bonus", unit.curse_dmg_taken_bonus);
        }
        // Lingering Effect - independent DoT/HoT instances (already
        // pruned by the main loop's own tick handling, not lazily here).
        if !unit.lingering_dots.is_empty() {
            push("lingering_dot_count", unit.lingering_dots.len() as f64);
        }
        // Elemental damage rework's 9 independent-per-proc stack lists -
        // counted (not pruned - read-only) same as everywhere else these
        // are consulted.
        let count_active = |stacks: &[u32]| stacks.iter().filter(|&&t| t > at_ms).count() as f64;
        push("fire_dr_debuff_stacks", count_active(&unit.fire_dr_debuff));
        push("cold_evasion_debuff_stacks", count_active(&unit.cold_evasion_debuff));
        push("chaos_block_debuff_stacks", count_active(&unit.chaos_block_debuff));
        push("lightning_dmg_taken_stacks", count_active(&unit.lightning_dmg_taken));
        push("divine_heal_reduction_stacks", count_active(&unit.divine_heal_reduction));
        push("fire_dr_buff_stacks", count_active(&unit.fire_dr_buff));
        push("cold_evasion_buff_stacks", count_active(&unit.cold_evasion_buff));
        push("chaos_block_buff_stacks", count_active(&unit.chaos_block_buff));
        push("divine_heal_power_buff_stacks", count_active(&unit.divine_heal_power_buff));
        if unit.elemental_overflow_dmg_bonus > 0.0 && at_ms <= unit.elemental_overflow_dmg_bonus_expires_at_ms {
            push("elemental_overflow_dmg_bonus", unit.elemental_overflow_dmg_bonus);
        }
    }
    out
}

/// Divides the raw rolled elemental % by this to get the actual proc
/// chance (2026-08-15, a same-day follow-up: the roll itself went back
/// up 75x - see `affix_base_value`'s doc - specifically so it could ALSO
/// contribute real flat damage again via `Character::combat_increased_damage`,
/// so the proc-chance formula changed here to compensate, instead of
/// reading the (now much bigger) raw roll directly as the chance the way
/// it briefly did.
/// Lowered 50 -> 10 (2026-08-18, a live request: "amp up the application
/// of ailments by a factor of 5") - a straight 5x buff to how OFTEN a
/// proc starts, not how strong one is once active (per-stack magnitude/
/// `ELEMENTAL_DEFENSE_FLOOR`/`_CEILING`/stack caps/`ELEMENTAL_PROC_DURATION_MS`
/// are all untouched). Checked against the live 49-character roster
/// before this landed - only a handful of the heaviest single-element
/// investments (e.g. lokati_gaming's combined Lightning, ~14%) cross the
/// 100% clamp under the new divisor; everyone else's chance just scales
/// up proportionally.
pub(crate) const ELEMENTAL_PROC_CHANCE_DIVISOR: f64 = 10.0;

/// Elemental damage rework (2026-08-15) - rolls one damage type's own
/// chance (`raw_pct / ELEMENTAL_PROC_CHANCE_DIVISOR` - `raw_pct` already
/// read once per unit at construction, see
/// `CombatSimUnit::fire_damage_pct`'s doc) and, on success, pushes a new
/// independent proc instance expiring `ELEMENTAL_PROC_DURATION_MS` from
/// now. A no-op if `stacks` is already at `max_stacks` - only Lightning/
/// Divine's two capped mechanics ever pass a real limit here; Fire/Cold/
/// Chaos's debuff/buff pass `usize::MAX`, since those three are
/// self-limiting by the floor/ceiling clamp at read time (see
/// `ELEMENTAL_DEFENSE_FLOOR`/`_CEILING`) rather than a stack count.
/// Returns `(did it proc, the chance actually rolled against - `None`
/// when nothing was rolled at all, i.e. `chance <= 0.0` or already at
/// `max_stacks`)` - the second value feeds `apply_hit`'s `RollEvent`
/// logging (2026-08-17, full-detail combat log, Wiring Phase 1).
pub(crate) fn roll_elemental_proc(raw_pct: f64, stacks: &mut Vec<u32>, max_stacks: usize, at_ms: u32, rng: &mut impl Rng) -> (bool, Option<f64>) {
    let chance = raw_pct / ELEMENTAL_PROC_CHANCE_DIVISOR;
    if chance <= 0.0 || stacks.len() >= max_stacks {
        return (false, None);
    }
    let chance = chance.clamp(0.0, 1.0);
    if !rng.gen_bool(chance) {
        return (false, Some(chance));
    }
    stacks.push(at_ms + ELEMENTAL_PROC_DURATION_MS);
    (true, Some(chance))
}

/// Builds one elemental-proc `RollEvent` (2026-08-17, full-detail combat
/// log, Wiring Phase 1) - the shared shape `apply_hit`'s 5 elemental-proc
/// call sites all construct identically, factored out purely to avoid
/// repeating the same 8-field literal 5 times.
#[allow(clippy::too_many_arguments)]
pub(crate) fn elemental_roll_event(hit_id: u64, at_ms: u32, source: &'static str, actor: &str, target: &str, chance: f64, succeeded: bool) -> RollEvent {
    RollEvent {
        event_id: next_hit_id(),
        hit_id,
        caused_by: None,
        at_ms,
        category: RollCategory::ElementalProc,
        source: std::borrow::Cow::Borrowed(source),
        actor: actor.to_string(),
        target: Some(target.to_string()),
        probability: Some(chance),
        succeeded: Some(succeeded),
        magnitude: None,
    }
}

/// Divine healing's self-buff proc (2026-08-16 follow-up) - unlike every
/// other elemental proc, this one reads the raw rolled % directly AS the
/// chance, with no `ELEMENTAL_PROC_CHANCE_DIVISOR` factoring-down and no
/// flat multiplier. Whole-number points below 100 are guaranteed stacks
/// (378% -> 3 guaranteed) and the leftover fractional point becomes one
/// more roll (the remaining 78% -> a 78% chance at a 4th) - overflow
/// past 100% compounds rather than being wasted or capped. Stacks
/// pushed are independent and uncapped, same as every other push here.
pub(crate) fn roll_divine_heal_power_proc(raw_pct: f64, stacks: &mut Vec<u32>, at_ms: u32, rng: &mut impl Rng) {
    if raw_pct <= 0.0 {
        return;
    }
    let guaranteed = raw_pct.floor() as u32;
    for _ in 0..guaranteed {
        stacks.push(at_ms + ELEMENTAL_PROC_DURATION_MS);
    }
    let remainder = raw_pct - raw_pct.floor();
    if remainder > 0.0 && rng.gen_bool(remainder.clamp(0.0, 1.0)) {
        stacks.push(at_ms + ELEMENTAL_PROC_DURATION_MS);
    }
}

/// Monk's Chakra of Light (2026-08-17) - same guaranteed-stacks +
/// fractional-roll shape as `roll_divine_heal_power_proc` just above
/// (378% -> 3 guaranteed + a 78% chance at a 4th), but pushed into the
/// REAL Lightning Damage debuff (`lightning_dmg_taken`) and respecting
/// that debuff's own `max_stacks` cap - unlike Divine's proc, which is
/// deliberately uncapped by design, staying consistent with the real
/// affix's cap here matters since `raw_pct` can be enormous (hundreds of
/// percent, scaled off the attacker's OWN increased damage).
pub(crate) fn roll_chakra_of_light_stacks(raw_pct: f64, stacks: &mut Vec<u32>, max_stacks: usize, at_ms: u32, rng: &mut impl Rng) {
    if raw_pct <= 0.0 {
        return;
    }
    let guaranteed = raw_pct.floor() as u32;
    for _ in 0..guaranteed {
        if stacks.len() >= max_stacks {
            return;
        }
        stacks.push(at_ms + ELEMENTAL_PROC_DURATION_MS);
    }
    let remainder = raw_pct - raw_pct.floor();
    if remainder > 0.0 && stacks.len() < max_stacks && rng.gen_bool(remainder.clamp(0.0, 1.0)) {
        stacks.push(at_ms + ELEMENTAL_PROC_DURATION_MS);
    }
}

/// Elemental damage rework (2026-08-15) - see
/// `CombatSimUnit::block_overflow_dmg_rate`'s doc for why this exists at
/// all. Called right after a NEW ally-buff stack (Cold's evasion buff or
/// Chaos's block buff - Fire's DR buff has no such conversion node to
/// feed, see that field's own doc, so it never calls this) actually
/// pushed - works out how much of THIS marginal stack pushed the live
/// total past `ELEMENTAL_DEFENSE_CEILING` (not yet reflected in `base`,
/// the unit's own static pre-fight snapshot value) and converts just
/// that excess into a live, accumulating, lazily-expiring damage buff
/// via `rate`. A no-op whenever `rate` is 0 (no relevant overflow-
/// conversion node invested) or this stack didn't actually cross the
/// ceiling.
pub(crate) fn convert_elemental_overflow(unit: &mut CombatSimUnit, base: f64, stack_count_before_this_push: u32, rate: f64, at_ms: u32) {
    if rate <= 0.0 {
        return;
    }
    let before = (base + stack_count_before_this_push as f64 * 0.01 - ELEMENTAL_DEFENSE_CEILING).max(0.0);
    let after = (base + (stack_count_before_this_push + 1) as f64 * 0.01 - ELEMENTAL_DEFENSE_CEILING).max(0.0);
    let marginal_excess = after - before;
    if marginal_excess <= 0.0 {
        return;
    }
    unit.elemental_overflow_dmg_bonus += marginal_excess * rate;
    unit.elemental_overflow_dmg_bonus_expires_at_ms = at_ms + ELEMENTAL_PROC_DURATION_MS;
}

/// Overwhelm - live target-damage-reduction shred scaled off the
/// ATTACKER's own current Bloodlust stack count, consulted as one more
/// `resolve_hit` mitigation source (that function's own doc already
/// calls out this as the place a future passive-tree source plugs in).
/// 0.0 without Overwhelm invested or once the attacker's stacks have
/// expired.
pub(crate) fn stack_shred_bonus(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.stack_speed_current == 0 || at_ms > unit.stack_speed_expires_at_ms {
        return 0.0;
    }
    unit.stack_shred_per_stack * unit.stack_speed_current as f64
}

/// Base boss buff (2026-08-17, a live request) - every boss ignores 2% of
/// the defender's evasion/block/damage-reduction per second they've been
/// alive in the fight, unconditional and independent of any passive tree
/// or gear (a mid-fight add like a Lich's Raise Dead summon starts this
/// from its OWN `spawned_at_ms`, not the main boss's already-elapsed
/// time - see that field's own doc). Left deliberately uncapped here -
/// `resolve_hit`'s own 3 call sites are each responsible for flooring the
/// DEFENDER's resulting stat at a relative 25% (never below that if they
/// naturally had at least that much, never artificially raised if they
/// didn't), not this function.
pub(crate) const BOSS_DEFENSE_IGNORE_PER_SEC: f64 = 0.02;

pub(crate) fn boss_defense_ignore(atk: &CombatSimUnit, at_ms: u32) -> f64 {
    if !atk.is_boss {
        return 0.0;
    }
    let secs_alive = at_ms.saturating_sub(atk.spawned_at_ms) as f64 / 1000.0;
    secs_alive * BOSS_DEFENSE_IGNORE_PER_SEC
}

/// Berserker's Hurricane - live splash bonus from Bloodlust/Frenzied
/// Blows' shared stack count, same read shape as `stack_damage_bonus`/
/// `stack_shred_bonus`.
pub(crate) fn stack_splash_bonus(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.stack_speed_current == 0 || at_ms > unit.stack_speed_expires_at_ms {
        return 0.0;
    }
    unit.stack_splash_per_stack * unit.stack_speed_current as f64
}

/// Monk's Stormfront - live splash bonus while at MAX Flowing Strikes
/// stacks specifically, same read shape as `stack_splash_bonus`.
pub(crate) fn stormfront_splash_bonus(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.stormfront_splash_pct <= 0.0 || unit.flowing_max_stacks == 0 || unit.flowing_current < unit.flowing_max_stacks || at_ms > unit.flowing_expires_at_ms {
        return 0.0;
    }
    unit.stormfront_splash_pct
}

/// Monk's Flowing Strikes - adds a stack to `units[attacker_idx]` ONLY
/// when this hit lands on the SAME target as their last one (unlike
/// `add_speed_stack`'s "any hit lands" trigger); switching targets (or
/// the previous streak having lazily expired) resets to a single fresh
/// stack rather than carrying the old count over. A no-op on any unit
/// that hasn't invested (`flowing_max_stacks == 0`). Only ever called for
/// a genuine new attack ACTION, not a follow-up/bonus hit chained onto
/// one (see the `!is_followup` gate at this fn's call site) - Twin
/// Strikes' crit-proc follow-up and a counter-attack both used to also
/// add their own stack here, so a single consecutive swing could
/// silently count as 2 "consecutive attacks" (2026-08-16 fix: "every
/// consecutive attack, not every hit").
/// Monk's Inner Focus/Meditation/Chi Burst/Serenity (see
/// `CombatSimUnit::inner_focus_heal_pct`'s doc) - extracted into its own
/// function (2026-08-16) so Clarity can call it from a second trigger
/// site (a blocked hit, not just an evade) without duplicating the body.
pub(crate) fn trigger_inner_focus(units: &mut [CombatSimUnit], target_idx: usize, at_ms: u32, events: &mut Vec<CombatEvent>, rng: &mut impl Rng) {
    let inner_focus_base = units[target_idx].inner_focus_heal_pct;
    if inner_focus_base <= 0.0 {
        return;
    }
    let meditation_bonus = (units[target_idx].evasion / 0.10).floor() * units[target_idx].inner_focus_meditation_bonus;
    let heal_amount = units[target_idx].max_hp as f64 * (inner_focus_base + meditation_bonus);
    apply_heal(units, target_idx, target_idx, heal_amount, at_ms, events, rng);
    // Rising Tide - a temporary self healing-power buff off the same
    // trigger (shared field/read-site every other `temp_heal_power_bonus`
    // grant already uses).
    let risingtide_pct = units[target_idx].risingtide_heal_power_pct;
    if risingtide_pct > 0.0 {
        units[target_idx].temp_heal_power_bonus = risingtide_pct;
        units[target_idx].temp_heal_power_bonus_expires_at_ms = at_ms + RISING_TIDE_DURATION_MS;
    }
    let chiburst_pct = units[target_idx].inner_focus_chiburst_pct;
    if chiburst_pct > 0.0 {
        // Wide Circle - heals 1+N lowest-HP allies instead of just one.
        let extra = units[target_idx].widecircle_extra_targets as usize;
        let mut allies: Vec<usize> = units.iter().enumerate().filter(|(i, u)| *i != target_idx && !u.is_boss && u.alive).map(|(i, _)| i).collect();
        allies.sort_by_key(|&i| units[i].hp);
        let harmonize_pct = units[target_idx].harmonize_dr_pct;
        for &ally_idx in allies.iter().take(1 + extra) {
            apply_heal(units, target_idx, ally_idx, heal_amount * chiburst_pct, at_ms, events, rng);
            // Harmonize - the healed ally also gets a temporary DR buff.
            if harmonize_pct > 0.0 {
                units[ally_idx].temp_damage_reduction_bonus = harmonize_pct;
                units[ally_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + HARMONIZE_DR_DURATION_MS;
            }
        }
    }
    let serenity_pct = units[target_idx].inner_focus_serenity_dr_pct;
    if serenity_pct > 0.0 {
        units[target_idx].temp_damage_reduction_bonus = serenity_pct;
        units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + units[target_idx].serenity_dr_duration_ms;
    }
}

pub(crate) fn add_flowing_stack(units: &mut [CombatSimUnit], attacker_idx: usize, target_idx: usize, at_ms: u32) {
    let unit = &mut units[attacker_idx];
    if unit.flowing_max_stacks == 0 {
        return;
    }
    if unit.flowing_last_target == target_idx && at_ms <= unit.flowing_expires_at_ms {
        // Eternal Flow - the refresh also adds bonus stacks on top of the
        // normal +1.
        let gain = 1 + unit.eternalflow_bonus_stacks;
        unit.flowing_current = (unit.flowing_current + gain).min(unit.flowing_max_stacks);
    } else {
        unit.flowing_current = 1;
    }
    unit.flowing_last_target = target_idx;
    unit.flowing_expires_at_ms = at_ms + unit.flowing_duration_ms;
    // Rising Storm - reaching max stacks grants a temporary self
    // increased-damage burst (same shared field the party-wide grants
    // use, single-target here).
    if unit.flowing_current >= unit.flowing_max_stacks && unit.risingstorm_dmg_pct > 0.0 {
        unit.temp_party_increased_damage_bonus = unit.risingstorm_dmg_pct;
        unit.temp_party_increased_damage_bonus_expires_at_ms = at_ms + RISING_STORM_DURATION_MS;
    }
}

/// Live attack-speed multiplier from `add_flowing_stack`'s stacks - same
/// read-at-the-scheduling-site convention as `speed_stack_multiplier`,
/// just a separate counter since Flowing Strikes' trigger differs.
pub(crate) fn flowing_stack_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.flowing_current == 0 || at_ms > unit.flowing_expires_at_ms {
        return 1.0;
    }
    1.0 + unit.flowing_current as f64 * unit.flowing_speed_per_stack
}

/// Pressure Point - live crit-chance bonus from Flowing Strikes' stacks,
/// consulted in `roll_attacker_damage` alongside Berserker's Gambit's own
/// live crit bonus. 0.0 without Pressure Point invested or once the
/// stacks have expired.
pub(crate) fn flowing_crit_bonus(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.flowing_current == 0 || at_ms > unit.flowing_expires_at_ms {
        return 0.0;
    }
    unit.flowing_crit_per_stack * unit.flowing_current as f64
}

/// Slayer's War Cry - live attack-speed multiplier from a party-wide grant
/// (see `temp_party_attack_speed_bonus`'s doc), same lazy-expiry-at-the-
/// read-site convention as `speed_stack_multiplier`, consulted at the same
/// scheduling call site. Returns 1.0 (no change) once expired or without
/// War Cry invested (either on this unit or a party member who has it).
pub(crate) fn party_speed_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.temp_party_attack_speed_bonus <= 0.0 || at_ms > unit.temp_party_attack_speed_bonus_expires_at_ms {
        return 1.0;
    }
    1.0 + unit.temp_party_attack_speed_bonus
}

/// Berserker's Rage Fueled - Gambit's crit chance is wasted above 80% HP
/// (missing-HP scaling can't apply at all there), so this converts that
/// otherwise-unused investment into attack speed instead. Live off current
/// hp%, same "no expiry needed, it's just always true or false right now"
/// shape as `party_speed_multiplier`'s siblings.
/// Mage's Static Field - live attack-speed PENALTY from a recent Chain
/// Lightning splash hit, read as a divisor at the same scheduling site
/// every other speed multiplier is.
pub(crate) fn static_field_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.temp_attack_speed_debuff <= 0.0 || at_ms > unit.temp_attack_speed_debuff_expires_at_ms {
        return 1.0;
    }
    (1.0 - unit.temp_attack_speed_debuff).max(0.1)
}

pub(crate) fn ragefueled_speed_multiplier(unit: &CombatSimUnit) -> f64 {
    if unit.ragefueled_speed_pct <= 0.0 || unit.max_hp == 0 {
        return 1.0;
    }
    if unit.hp as f64 / unit.max_hp as f64 > 0.80 {
        1.0 + unit.ragefueled_speed_pct
    } else {
        1.0
    }
}

/// Warlock's Fel Rush - live attack-speed multiplier from a recent kill,
/// same lazy-expiry-at-the-read-site convention as `speed_stack_multiplier`
/// (consulted at the exact same call site), but a flat single value
/// instead of a per-stack one - see `fel_rush_speed_bonus`'s doc for why
/// it isn't just another `stack_speed_*` investor. Returns 1.0 (no
/// change) once expired or without Fel Rush invested.
pub(crate) fn fel_rush_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.fel_rush_speed_bonus <= 0.0 || at_ms > unit.fel_rush_expires_at_ms {
        return 1.0;
    }
    // Ravage (rank 3) - additional stacked bonus banked from kills while
    // Fel Rush stays active.
    1.0 + unit.fel_rush_speed_bonus + unit.fel_rush_stacks as f64 * unit.ravage_stack_pct
}

/// Paladin's Zealous Charge (Guardian's Wrath, 2026-08-17) - same
/// lazy-expiry shape as `fel_rush_multiplier` immediately above. Returns
/// 1.0 (no change) once expired or without it invested.
pub(crate) fn zealouscharge_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.zealotry_guardianswrath_speed_bonus <= 0.0 || at_ms > unit.zealotry_guardianswrath_expires_at_ms {
        return 1.0;
    }
    1.0 + unit.zealotry_guardianswrath_speed_bonus
}

/// Mage's Timewarp / Warlock's Demonic Speed (2026-08-17, a shared
/// cluster - identical text and identical gap on both: Quickcast/Fel
/// Haste are baseline-only `FlatStat{AttackSpeed}` nodes folded once into
/// `attack_interval_ms()` at construction, with no separately-addressable
/// "slice" left to double mid-fight). Rather than reconstructing a
/// doubled-baseline ratio (two formulas that would have to stay in sync
/// forever), this grants a separate, additive temporary speed bonus equal
/// to that node's own magnitude, active only for the fight's opening
/// window - same player-facing "your early speed is doubled" result,
/// without touching `attack_interval_ms()` at all. Mutually exclusive by
/// archetype (only ever one of Mage/Warlock), same sharing convention
/// `evade_counter_chance` already uses across archetypes. Returns 1.0 (no
/// change) once the window's elapsed or without either invested.
pub(crate) fn early_fight_speed_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.early_fight_speed_bonus_pct <= 0.0 || at_ms > unit.early_fight_speed_window_end_ms {
        return 1.0;
    }
    1.0 + unit.early_fight_speed_bonus_pct
}

/// Slayer's Blood Frenzy - live attack-speed multiplier from a recent
/// FlickerStrike dash, same lazy-expiry-at-the-read-site convention as
/// `fel_rush_multiplier` (its own trigger is a kill; this one's is the
/// dash itself - see `ArchetypeSkill::on_periodic_tick`). Returns 1.0 (no
/// change) once expired or without it invested.
pub(crate) fn flicker_frenzy_multiplier(unit: &CombatSimUnit, at_ms: u32) -> f64 {
    if unit.flicker_frenzy_speed_bonus <= 0.0 || at_ms > unit.flicker_frenzy_expires_at_ms {
        return 1.0;
    }
    1.0 + unit.flicker_frenzy_speed_bonus
}

/// Slayer's Endless Thirst - live leech-cap addition from a recent
/// FlickerStrike dash, same lazy-expiry convention as
/// `flicker_frenzy_multiplier` (same trigger, independent timer). Returns
/// (extra_cap_pct, uncapped) - at rank 3 `uncapped` is true and the
/// caller bypasses `LIFE_LEECH_CAP_PER_SEC` entirely, since "the cap is
/// removed entirely" isn't expressible as a magnitude the way ranks 1-2's
/// linear +% is (see the node's own text). (0.0, false) once expired or
/// without it invested.
pub(crate) fn endless_thirst_bonus(unit: &CombatSimUnit, at_ms: u32) -> (f64, bool) {
    if (unit.endless_thirst_cap_bonus <= 0.0 && !unit.endless_thirst_uncapped) || at_ms > unit.endless_thirst_expires_at_ms {
        return (0.0, false);
    }
    (unit.endless_thirst_cap_bonus, unit.endless_thirst_uncapped)
}

/// Ranger's Hunter's Mark / Warlock's Curse of Weakness - on
/// `units[attacker_idx]`'s first LANDED hit each fight (mirrors the "you
/// swung and connected" gate `add_speed_stack`/`add_flowing_stack` use -
/// called from the same spot in `apply_hit`), applies whatever this unit
/// has invested in either onto `units[target_idx]`. A no-op for anyone
/// without either invested (`own_mark_crit_chance` and `own_curse_dmg_taken`
/// both 0.0) or already past their first hit this fight
/// (`has_applied_mark_this_fight`). Contagious Curse additionally spreads
/// the SAME curse value (full, not a fraction - its own text doesn't say
/// otherwise) to up to `own_curse_spread_count` other random alive
/// enemies on the same side as the primary target.
pub(crate) fn apply_first_hit_mark(units: &mut [CombatSimUnit], attacker_idx: usize, target_idx: usize, at_ms: u32, rng: &mut impl Rng) {
    if units[attacker_idx].has_applied_mark_this_fight {
        return;
    }
    let has_mark = units[attacker_idx].own_mark_crit_chance > 0.0;
    let has_curse = units[attacker_idx].own_curse_dmg_taken > 0.0;
    if !has_mark && !has_curse {
        return;
    }
    units[attacker_idx].has_applied_mark_this_fight = true;
    if has_mark {
        let source_id = units[attacker_idx].id.clone();
        units[target_idx].mark_source_id = Some(source_id);
        units[target_idx].mark_crit_chance_bonus = units[attacker_idx].own_mark_crit_chance;
        units[target_idx].mark_crit_multiplier_bonus = units[attacker_idx].own_mark_crit_mult;
        units[target_idx].mark_low_hp_damage_bonus = units[attacker_idx].own_mark_low_hp_dmg;
        units[target_idx].mark_ally_crit_chance_bonus = units[attacker_idx].own_mark_ally_crit_chance;
        units[target_idx].mark_ally_dmg_bonus = units[attacker_idx].own_mark_ally_dmg_pct;
        units[target_idx].mark_ally_crit_multiplier_bonus = units[attacker_idx].own_mark_ally_crit_mult;
        // Wider Pack - the SAME mark also applies to N additional random
        // enemies at apply time (identical spread-loop shape to
        // Contagious Curse just below).
        let mark_spread = units[attacker_idx].own_mark_spread_count;
        if mark_spread > 0 {
            let target_is_boss = units[target_idx].is_boss;
            let source_id = units[attacker_idx].id.clone();
            let mut others: Vec<usize> = units
                .iter()
                .enumerate()
                .filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive)
                .map(|(i, _)| i)
                .collect();
            for _ in 0..mark_spread.min(others.len() as u32) {
                let pick = rng.gen_range(0..others.len());
                let other_idx = others.remove(pick);
                units[other_idx].mark_source_id = Some(source_id.clone());
                units[other_idx].mark_crit_chance_bonus = units[attacker_idx].own_mark_crit_chance;
                units[other_idx].mark_crit_multiplier_bonus = units[attacker_idx].own_mark_crit_mult;
                units[other_idx].mark_low_hp_damage_bonus = units[attacker_idx].own_mark_low_hp_dmg;
                units[other_idx].mark_ally_crit_chance_bonus = units[attacker_idx].own_mark_ally_crit_chance;
                units[other_idx].mark_ally_dmg_bonus = units[attacker_idx].own_mark_ally_dmg_pct;
                units[other_idx].mark_ally_crit_multiplier_bonus = units[attacker_idx].own_mark_ally_crit_mult;
            }
        }
    }
    if has_curse {
        let bonus = units[attacker_idx].own_curse_dmg_taken;
        let curser_id = units[attacker_idx].id.clone();
        units[target_idx].curse_dmg_taken_bonus = bonus;
        // Soul Stone - "when a Warlock curses an enemy he creates a soul
        // stone," capped at `own_soul_stone_max` (1/2/3 by rank). Fires
        // here on the primary target, and again per Contagious Curse
        // spread copy below - each landed curse is its own event.
        grant_soul_stone(&mut units[attacker_idx]);
        // Withering Curse - a healing-received debuff riding the same
        // application.
        units[target_idx].curse_heal_reduction_bonus = units[attacker_idx].own_curse_heal_reduction_pct;
        // Curse source attribution (2026-08-16, moved out from under the
        // Doom-only gate below - see `HitOutcome::curse_bonus_damage`'s
        // doc) - every cursed target now always knows who cursed them,
        // not just when Doom is also invested, so `apply_hit` can credit
        // the Warlock's own damage-reduction-shred contribution on every
        // subsequent hit against them, Doom or not.
        units[target_idx].curse_source_id = Some(curser_id.clone());
        // Warlock's Doom - only when invested does the curse gain a real
        // expiry+detonation cycle at all (see `curse_expires_at_ms`'s doc);
        // without it the curse stays permanent-for-the-fight, unchanged
        // from its original behavior.
        let doom_pct = units[attacker_idx].own_doom_detonate_pct;
        if doom_pct > 0.0 {
            units[target_idx].curse_expires_at_ms = at_ms + DOOM_CURSE_DURATION_MS;
            units[target_idx].next_curse_expiry_at_ms = at_ms + DOOM_CURSE_DURATION_MS;
            units[target_idx].curse_damage_taken_total = 0.0;
            units[target_idx].curse_detonate_pct = doom_pct;
        }
        let spread = units[attacker_idx].own_curse_spread_count;
        if spread > 0 {
            let target_is_boss = units[target_idx].is_boss;
            let mut others: Vec<usize> = units
                .iter()
                .enumerate()
                .filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive)
                .map(|(i, _)| i)
                .collect();
            // Epidemic - spread copies get a bonus ON TOP of the primary
            // target's own curse value.
            let spread_bonus = bonus + units[attacker_idx].own_curse_spread_bonus_pct;
            for _ in 0..spread.min(others.len() as u32) {
                let pick = rng.gen_range(0..others.len());
                let other_idx = others.remove(pick);
                units[other_idx].curse_dmg_taken_bonus = spread_bonus;
                units[other_idx].curse_heal_reduction_bonus = units[attacker_idx].own_curse_heal_reduction_pct;
                units[other_idx].curse_source_id = Some(curser_id.clone());
                grant_soul_stone(&mut units[attacker_idx]);
            }
        }
    }
}

/// Warlock's Soul Stone - increments the caster's banked stones, capped at
/// `own_soul_stone_max` (1/2/3 by rank). Shared by every place a curse this
/// unit cast actually lands on an enemy: `apply_first_hit_mark`'s primary
/// target and its Contagious Curse spread copies, and Cursed Blood's
/// fight-start cast in `simulate_battle`.
pub(crate) fn grant_soul_stone(caster: &mut CombatSimUnit) {
    if caster.own_soul_stone_max > 0 {
        caster.soul_stones = (caster.soul_stones + 1).min(caster.own_soul_stone_max);
    }
}

/// Process-lifetime monotonic source for `CombatEvent::Attack.hit_id`
/// (and, from Wiring Phase 1 onward, `RollEvent.hit_id`/`event_id`) - a
/// plain global atomic rather than a counter threaded through `apply_hit`
/// and its dozen-plus recursive call sites (splash, follow-ups,
/// Intervene, explosions, ...). Ids only need to be unique enough to
/// correlate events within a single fight's own log, so a counter that
/// keeps climbing across fights/restarts is fine - nothing reads it as
/// "hit number N of this fight."
static NEXT_HIT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub(crate) fn next_hit_id() -> u64 {
    NEXT_HIT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Rolls (`resolve_hit`) and applies one hit from `units[attacker_idx]`
/// onto `units[target_idx]` - mutates the target's hp, pushes the
/// resulting `Attack` event (and a `Defeat` event if it just dropped to
/// 0) onto `events`. The one place that actually writes hp/pushes
/// events for a damage instance, shared by every call site below.
/// `is_followup` is `true` only for Twin Strikes/Spell Echo's own
/// recursive call to itself (see below) - keeps that trigger from
/// re-firing off its own follow-up strike without needing to thread
/// `outcome.is_crit` back out to every one of this function's 11 other
/// call sites (which would mean changing its return type instead of just
/// adding one parameter).
/// Leaky-bucket drain for the life-leech per-second cap (see
/// `CombatSimUnit::leech_window_start_ms`'s doc - 2026-08-18, wiki audit
/// finding #3). Bleeds `gained` down by `cap` per elapsed second since
/// `last_update_ms`, floored at 0 - a genuine rolling window, unlike the
/// lump-sum reset this replaced (which let a leech build burst to ~2x
/// `LIFE_LEECH_CAP_PER_SEC` by timing hits across the reset boundary: a
/// full window's worth right before the reset, then another full
/// window's worth right after). Pure function so the drain math itself
/// is unit-testable without going through the whole `apply_hit`
/// pipeline.
fn drain_leech_window(gained: f64, cap: f64, last_update_ms: u32, at_ms: u32) -> f64 {
    let elapsed_ms = at_ms.saturating_sub(last_update_ms);
    let drained = cap * (elapsed_ms as f64 / 1000.0);
    (gained - drained).max(0.0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_hit(
    units: &mut [CombatSimUnit],
    attacker_idx: usize,
    target_idx: usize,
    base_damage: f64,
    at_ms: u32,
    events: &mut Vec<CombatEvent>,
    rolls: &mut Vec<RollEvent>,
    rng: &mut impl Rng,
    applies_wound: bool,
    is_followup: bool,
) {
    // A splash hit no longer counts as "a hit" for the purpose of
    // triggering any of THIS attacker's own reactive on-hit procs
    // (2026-08-16, a live request following a real report of runaway
    // damage from splash targets each independently re-triggering their
    // own Twin Strikes/Double Tap/Finite Loop/Volatile Magic chains) -
    // every existing `!is_followup` gate on a reactive proc below is now
    // `counts_as_primary_hit` instead, so splash joins Twin-Strike-style
    // follow-ups as something that can still DEAL damage and still crit,
    // but can never itself trigger a secondary proc. Scoped to the
    // ATTACKER (see `in_splash_resolution`'s own doc) since that's who's
    // mid-`apply_splash` here, not the target.
    let counts_as_primary_hit = !is_followup && !units[attacker_idx].in_splash_resolution;
    // Druid's Pack Instinct/Symbiosis - both read "am I THE party's
    // current lowest-HP ally right now", which needs the full `units`
    // slice (see `resolve_hit`'s doc for why this can't live there
    // directly). "Ally" excludes the target being their own source, same
    // spirit as `heal_target_idx`'s own self-exclusion; if multiple Druids
    // are alive, their magnitudes simply sum (same convention as every
    // other multi-source stat here).
    let (pack_instinct_evasion_bonus, symbiosis_dr_bonus) = if !units[target_idx].is_boss {
        // Shared Strength (Monk's Temple Guardian) - protects the K
        // lowest-HP allies at once instead of just the single lowest, K =
        // 1 + the highest Shared Strength rank among any alive non-boss
        // unit (a party-scoped simplification for the rare case of
        // multiple differently-ranked Monks, rather than per-source K).
        let extra_targets = units.iter().filter(|u| !u.is_boss && u.alive).map(|u| u.sharedstrength_extra_targets).max().unwrap_or(0);
        let mut allies_by_hp: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
        allies_by_hp.sort_by_key(|&i| units[i].hp);
        let is_protected = allies_by_hp.iter().take(1 + extra_targets as usize).any(|&i| i == target_idx);
        if is_protected {
            let (evasion, dr) = units.iter().enumerate().filter(|(i, u)| *i != target_idx && !u.is_boss && u.alive).fold((0.0, 0.0), |(evasion, dr), (_, u)| {
                (evasion + u.own_pack_instinct_evasion_pct, dr + u.own_symbiosis_dr_pct)
            });
            // Guardian Spirit (Temple Guardian) - the protected ally also
            // gets a small periodic self-heal, gated by its own cooldown.
            let guardianspirit_pct = units.iter().filter(|u| !u.is_boss && u.alive).map(|u| u.templeguardian_heal_pct).fold(0.0, f64::max);
            if guardianspirit_pct > 0.0 && at_ms >= units[target_idx].next_templeguardian_heal_at_ms {
                let heal_amount = units[target_idx].max_hp as f64 * guardianspirit_pct;
                apply_heal(units, target_idx, target_idx, heal_amount, at_ms, events, rng);
                units[target_idx].next_templeguardian_heal_at_ms = at_ms + TEMPLE_GUARDIAN_HEAL_INTERVAL_MS;
            }
            (evasion, dr)
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };
    // Rogue's Assassinate - consumes a banked charge (if any) to guarantee
    // THIS hit crits, whichever hit happens to be this unit's next one
    // (primary, splash, a Twin Strikes follow-up, etc. - "next hit"
    // literally, not narrowed to any one call site).
    let mut assassinate_triggered = false;
    let force_crit = if units[attacker_idx].assassinate_charges > 0 {
        units[attacker_idx].assassinate_charges -= 1;
        assassinate_triggered = true;
        events.push(CombatEvent::SkillCast { at_ms, unit: units[attacker_idx].id.clone(), skill: "Assassinate".to_string() });
        true
    } else if units[attacker_idx].force_crit_next_hit {
        // Warrior's Payback - consumed the instant it's read, same
        // one-shot spirit as Assassinate's own charge.
        units[attacker_idx].force_crit_next_hit = false;
        true
    } else if units[attacker_idx].empoweredbolt_invested && units[attacker_idx].hits_landed_this_fight == 0 {
        // Mage's Empowered Bolt - the first hit each fight is a
        // guaranteed crit.
        true
    } else if units[attacker_idx].markedfordeath_hits_remaining > 0 {
        // Rogue's Marked for Death - one guaranteed crit consumed per
        // attempt (evaded or not, same one-shot-per-attempt spirit as
        // Backstab).
        units[attacker_idx].markedfordeath_hits_remaining -= 1;
        true
    } else {
        false
    };
    // Elemental damage rework (2026-08-15) - `resolve_hit` only ever
    // takes IMMUTABLE `atk`/`def` refs (it's consulted from other spots
    // too, and pruning needs `&mut`), so - same "caller resolves live
    // per-fight state, passes plain values in" pattern
    // `pack_instinct_evasion_bonus`/`symbiosis_dr_bonus` above already
    // use - the target's own active elemental debuff/buff stacks are
    // pruned and converted to plain percentages HERE, in `apply_hit`
    // (which already has full mutable `units` access), then handed to
    // `resolve_hit` as plain numbers.
    let target_fire_debuff = prune_and_count(&mut units[target_idx].fire_dr_debuff, at_ms) as f64 * 0.01;
    let target_fire_buff = prune_and_count(&mut units[target_idx].fire_dr_buff, at_ms) as f64 * 0.01;
    let target_cold_debuff = prune_and_count(&mut units[target_idx].cold_evasion_debuff, at_ms) as f64 * 0.01;
    let target_cold_buff = prune_and_count(&mut units[target_idx].cold_evasion_buff, at_ms) as f64 * 0.01;
    let target_chaos_debuff = prune_and_count(&mut units[target_idx].chaos_block_debuff, at_ms) as f64 * 0.01;
    let target_chaos_buff = prune_and_count(&mut units[target_idx].chaos_block_buff, at_ms) as f64 * 0.01;
    let target_lightning_dmg_taken = prune_and_count(&mut units[target_idx].lightning_dmg_taken, at_ms) as f64 * 0.01;
    let outcome = resolve_hit(
        base_damage,
        &units[attacker_idx],
        &units[target_idx],
        at_ms,
        rng,
        pack_instinct_evasion_bonus,
        symbiosis_dr_bonus,
        force_crit,
        target_fire_debuff,
        target_fire_buff,
        target_cold_debuff,
        target_cold_buff,
        target_chaos_debuff,
        target_chaos_buff,
        target_lightning_dmg_taken,
    );
    // Allocated once for this hit's resolution, shared by every branch
    // below that can end up pushing this hit's `Attack` event(s) (only
    // one branch fires per call) - lets every `RollEvent` a later phase
    // logs for this hit correlate back to whichever `Attack` it produced.
    let hit_id = next_hit_id();
    // Full-detail combat log (2026-08-17, Wiring Phase 1) - every genuine
    // probabilistic roll `resolve_hit` already collected (crit remainder,
    // evasion, block/Stonewall, Cold Steel pass-along) becomes a real
    // `RollEvent` here, tagged with this hit's own `hit_id`/actors. Kept
    // as a separate step (not built inline inside `resolve_hit`) since
    // that function only sees `atk: &CombatSimUnit`/`def: &CombatSimUnit`
    // (ids, not the full `units` slice or the `RollEvent` sink itself).
    {
        let roll_attacker_id = units[attacker_idx].id.clone();
        let roll_target_id = units[target_idx].id.clone();
        for (category, source, probability, succeeded) in &outcome.probabilistic_rolls {
            // Evasion/Block are the DEFENDER's own roll (they're the one
            // dodging/blocking) - every other category here (Crit, Cold
            // Steel's guaranteed-landing pass-along) is the ATTACKER's.
            let (actor, target) = match category {
                RollCategory::Evasion | RollCategory::Block => (roll_target_id.clone(), roll_attacker_id.clone()),
                _ => (roll_attacker_id.clone(), roll_target_id.clone()),
            };
            rolls.push(RollEvent {
                event_id: next_hit_id(),
                hit_id,
                caused_by: None,
                at_ms,
                category: *category,
                source: std::borrow::Cow::Borrowed(*source),
                actor,
                target: Some(target),
                probability: *probability,
                succeeded: Some(*succeeded),
                magnitude: None,
            });
        }
        // Full-detail combat log (2026-08-17, Phase 2) - sibling loop for
        // `outcome.deterministic_sources`. Actor attribution follows the
        // same per-source judgment call as Evasion/Block above: a
        // defender's own personal stat/buff attributes to the defender;
        // a source that's really the ATTACKER's own mechanic reducing
        // the defender's effective DR (Ambush, Overwhelm/Crush, Vital
        // Points, Crippling Grip, Gelatinous Cube's shred) attributes to
        // the attacker; Predator/the Lightning proc stack are explicitly
        // documented as "any attacker benefits, no source-identity
        // check" (see their own doc comments in `resolve_hit`), so they
        // attribute to whoever's CURRENTLY benefiting rather than an
        // untracked original applier. Curse of Weakness is the one
        // source with a real tracked applier (`curse_source_id`, same
        // field the credit-split below already reads) - attributed
        // there specifically, matching the original "should count
        // toward the Warlock's own DPS" design this whole system traces
        // back to.
        for (category, source, magnitude) in &outcome.deterministic_sources {
            let (actor, target) = match *category {
                RollCategory::Evasion => (roll_target_id.clone(), roll_attacker_id.clone()),
                RollCategory::Mitigation => match *source {
                    "Curse of Weakness" => (units[target_idx].curse_source_id.clone().unwrap_or_else(|| roll_attacker_id.clone()), roll_target_id.clone()),
                    "Ambush" | "Overwhelm/Crush shred" | "Vital Points" | "Crippling Grip" | "Gelatinous Cube shred" | "Lightning damage-taken stack" | "Predator" => {
                        (roll_attacker_id.clone(), roll_target_id.clone())
                    }
                    _ => (roll_target_id.clone(), roll_attacker_id.clone()),
                },
                _ => (roll_attacker_id.clone(), roll_target_id.clone()),
            };
            rolls.push(RollEvent {
                event_id: next_hit_id(),
                hit_id,
                caused_by: None,
                at_ms,
                category: *category,
                source: std::borrow::Cow::Borrowed(*source),
                actor,
                target: Some(target),
                probability: None,
                succeeded: None,
                magnitude: Some(*magnitude),
            });
        }
    }
    // Backstab is one-shot - consumed the instant it's read above,
    // regardless of whether this hit landed or was evaded (it was "used
    // up" on the attempt either way, matching every other one-shot flag's
    // "read once, then clear" convention here).
    units[attacker_idx].backstab_pending_dmg_pct = 0.0;
    // Opening Move - if this hit's guaranteed-landing treatment came from
    // the cooldown recharge specifically (not the base fight-opening
    // budget, which stays untouched), consume the cooldown now. Re-derives
    // the same deterministic (no-roll) condition `resolve_hit` just used.
    if units[attacker_idx].hits_landed_this_fight >= units[attacker_idx].opportunist_guaranteed_hits
        && units[attacker_idx].openingmove_cooldown_ms > 0
        && at_ms >= units[attacker_idx].next_openingmove_at_ms
    {
        units[attacker_idx].next_openingmove_at_ms = at_ms + units[attacker_idx].openingmove_cooldown_ms;
    }
    // Cold Steel - a pending debuff on the TARGET is consumed by this
    // attempt regardless of outcome (evaded, blocked, or landed - "the
    // next hit" used it up either way).
    units[target_idx].coldsteel_pending = false;

    // Rogue's Twin Strikes / Mage's Spell Echo - a crit has a chance to
    // strike again at `twin_strike_dmg_pct` of THIS hit's own base (same
    // "shared pre-roll base, each strike independently re-rolls its own
    // crit/mitigation" convention as Frenzy's extra strikes), against the
    // same target. Checked before evasion/alive-state matter for the
    // ORIGINAL hit's own outcome below - the follow-up call re-checks
    // both fresh, same as any other `apply_hit` call.
    if counts_as_primary_hit && outcome.is_crit {
        let chance = units[attacker_idx].twin_strike_chance;
        if chance > 0.0 && units[attacker_idx].alive && units[target_idx].alive && rng.gen_bool(chance.clamp(0.0, 1.0)) {
            let follow_damage = base_damage * units[attacker_idx].twin_strike_dmg_pct;
            apply_hit(units, attacker_idx, target_idx, follow_damage, at_ms, events, rolls, rng, applies_wound, true);
            // Mage's Finite Loop (renamed from "Infinite Loop" 2026-08-16,
            // see `finiteloop_max_repeats`'s doc) - a REAL chain on top of
            // the single flat-chance follow-up above: once that first
            // follow-up lands, each additional repeat rolls the SAME base
            // `chance` (`twin_strike_chance`) again - not its own separate/
            // boosted chance (2026-08-17 rework, a live request: "not
            // increase the chance for echoing spells") - hard-capped at
            // `finiteloop_max_repeats` total extra hits (an explicit
            // bounded `for` loop, not recursion with a threaded depth
            // parameter) so this can never run away regardless of luck.
            // Cap restored to 3/6/9 (was retuned down to 1/2/3 earlier).
            // The outer `counts_as_primary_hit` check above already keeps
            // this whole block from ever running off a splash hit at all,
            // so no separate inner guard is needed here.
            let finiteloop_max_repeats = units[attacker_idx].finiteloop_max_repeats;
            if finiteloop_max_repeats > 0 {
                for _ in 0..finiteloop_max_repeats {
                    if !(units[attacker_idx].alive && units[target_idx].alive && rng.gen_bool(chance.clamp(0.0, 1.0))) {
                        break;
                    }
                    let follow_damage = base_damage * units[attacker_idx].twin_strike_dmg_pct;
                    apply_hit(units, attacker_idx, target_idx, follow_damage, at_ms, events, rolls, rng, applies_wound, true);
                }
            }
            // Rogue's Double Tap (2026-08-16 - same treatment as Finite
            // Loop just above, its Mage-side mirror) - same 2026-08-17
            // rework: reuses the base `chance` instead of its own separate
            // chance, cap restored to 3/6/9.
            let doubletap_max_repeats = units[attacker_idx].doubletap_max_repeats;
            if doubletap_max_repeats > 0 {
                for _ in 0..doubletap_max_repeats {
                    if !(units[attacker_idx].alive && units[target_idx].alive && rng.gen_bool(chance.clamp(0.0, 1.0))) {
                        break;
                    }
                    let follow_damage = base_damage * units[attacker_idx].twin_strike_dmg_pct;
                    apply_hit(units, attacker_idx, target_idx, follow_damage, at_ms, events, rolls, rng, applies_wound, true);
                }
            }
        }
    }

    // Celestial Shard's unique affix, DPS-role side (2026-08-16 fix, live
    // request) - a non-Heal wielder's landed hit strikes the SAME target
    // again for CELESTIAL_CONVERSION_PCT of THIS hit's own base damage
    // (same "% of base_damage" convention Twin Strikes uses just above,
    // not the post-mitigation dealt amount). Modeled directly on Twin
    // Strikes' shape - an independent `apply_hit` call, not flat bonus
    // damage, so it rolls its own crit/mitigation and CAN trigger other
    // on-hit effects (Leech, elemental procs, etc.), matching the live
    // request's "an additional hit that can proc effects." Gated on
    // `!outcome.evaded` ("a target they deal damage to") and `!is_followup`
    // (so this hit's own follow-up doesn't chain into another one).
    // Healers get the existing heal-triggered version instead (see the
    // heal-share block above in `simulate_battle`) - the two are mutually
    // exclusive by `role`, not just by which action happened to fire.
    if counts_as_primary_hit
        && !outcome.evaded
        && units[attacker_idx].has_celestial_conversion
        && units[attacker_idx].role != Some(CombatFunction::Heal)
        && units[attacker_idx].alive
        && units[target_idx].alive
    {
        let bonus_damage = base_damage * CELESTIAL_CONVERSION_PCT;
        apply_hit(units, attacker_idx, target_idx, bonus_damage, at_ms, events, rolls, rng, applies_wound, true);
    }

    // Monk's Chakra of Many (2026-08-17) - same "independent follow-up
    // hit" shape as Celestial Shard just above, at `chakra_of_many_pct` of
    // THIS hit's own base damage (10/20/30% per rank). Flavored as a
    // spectral clone attacking alongside the Monk, but mechanically it's
    // just a second real `apply_hit` call - it naturally rolls its own
    // crit/mitigation and can independently trigger every other on-hit
    // effect (Lingering Effect, elemental procs, Chakra of Light, etc.),
    // which is what "doubles the application of on-hit effects" cashes out
    // to without any new replay/duplication plumbing. Gated the same way
    // as Celestial Shard (`counts_as_primary_hit`/`!outcome.evaded`) so a
    // Twin Strikes/Chakra of Many follow-up doesn't chain into another one.
    if counts_as_primary_hit && !outcome.evaded && units[attacker_idx].chakra_of_many_pct > 0.0 && units[attacker_idx].alive && units[target_idx].alive {
        let bonus_damage = base_damage * units[attacker_idx].chakra_of_many_pct;
        apply_hit(units, attacker_idx, target_idx, bonus_damage, at_ms, events, rolls, rng, applies_wound, true);
    }

    // Warrior's Momentum/Berserker's Bloodlust - every hit THIS unit
    // lands (whether or not it gets evaded/blocked on the far end - the
    // trigger is "you swung and connected", not "it dealt full damage")
    // adds a stack. Rogue's Fleetfoot is the mirror on the defending
    // side - every hit THIS unit evades adds a stack instead. See
    // `add_speed_stack`'s doc; a no-op on any unit that hasn't invested
    // in any of the three. Monk's Flowing Strikes is a genuinely separate
    // counter (same-target-gated - see `add_flowing_stack`'s doc), fired
    // alongside on the same "landed" trigger - but gated on `!is_followup`
    // too (2026-08-16 fix), since its stack is meant to track consecutive
    // ATTACKS, not consecutive HITS - a Twin Strikes follow-up or a
    // counter-attack chained onto this one shouldn't silently double it up.
    if !outcome.evaded {
        add_speed_stack(units, attacker_idx, at_ms);
        // Berserker's Windfury - a chance for the same trigger to grant a
        // 2nd stack.
        let windfury_chance = units[attacker_idx].windfury_chance;
        if windfury_chance > 0.0 && rng.gen_bool(windfury_chance.clamp(0.0, 1.0)) {
            add_speed_stack(units, attacker_idx, at_ms);
        }
        // Rogue's Opportunist - counts this landed hit toward the
        // guaranteed-hit budget `resolve_hit` already consulted for THIS
        // hit (see `opportunist_guaranteed_hits`'s doc) - incremented
        // after the fact, same "read-before, write-after" convention as
        // every other per-fight counter here.
        units[attacker_idx].hits_landed_this_fight += 1;
        units[target_idx].hits_taken_this_fight += 1;
        if units[target_idx].is_boss {
            units[attacker_idx].has_hit_boss_this_fight = true;
        }
        // Predator/Cold Steel - both fire off any landed hit that just
        // got Opportunist's guaranteed-landing treatment, from whichever
        // source granted it (base budget, Opening Move, or a passed-along
        // Cold Steel debuff).
        if outcome.opportunist_guaranteed {
            // Predator - marks the struck target: +damage taken from ALL
            // attackers for 4s.
            let predator_pct = units[attacker_idx].predator_dmg_taken_pct;
            if predator_pct > 0.0 {
                units[target_idx].predator_dmg_taken_bonus = predator_pct;
                units[target_idx].predator_expires_at_ms = at_ms + PREDATOR_MARK_DURATION_MS;
            }
            // Cold Steel - leaves a fresh pending debuff on the target so
            // the NEXT hit from any ally can also pass along the
            // treatment, at this unit's own chance/value.
            let coldsteel_chance = units[attacker_idx].coldsteel_pass_chance;
            if coldsteel_chance > 0.0 {
                units[target_idx].coldsteel_pending = true;
                units[target_idx].coldsteel_pass_chance_pending = coldsteel_chance;
                units[target_idx].coldsteel_ambush_pct_pending = units[attacker_idx].ambush_dr_cut_pct;
            }
        }
        // Rogue's Marked for Death - a Cutthroat-eligible crit (approximated
        // off the target's post-hit hp%, since resolve_hit's own pre-hit
        // check isn't threaded back out) banks guaranteed crits for this
        // unit's next hits.
        if outcome.is_crit && units[attacker_idx].markedfordeath_hit_count > 0 && units[target_idx].max_hp > 0 && (units[target_idx].hp as f64 / units[target_idx].max_hp as f64) < 0.25 {
            units[attacker_idx].markedfordeath_hits_remaining = units[attacker_idx].markedfordeath_hit_count;
        }
        // Final Cut - a Cutthroat-eligible kill grants a temporary self
        // attack-speed buff.
        if outcome.is_crit && !units[target_idx].alive && units[attacker_idx].finalcut_speed_pct > 0.0 {
            units[attacker_idx].temp_party_attack_speed_bonus = units[attacker_idx].finalcut_speed_pct;
            units[attacker_idx].temp_party_attack_speed_bonus_expires_at_ms = at_ms + FINAL_CUT_DURATION_MS;
        }
        // Rogue's Silent Blade - a temporary evasion buff (same shared
        // field/read-site Vanish already uses) after Assassinate's
        // guaranteed crit lands.
        if assassinate_triggered {
            let silentblade_pct = units[attacker_idx].silentblade_evasion_pct;
            if silentblade_pct > 0.0 {
                units[attacker_idx].temp_evasion_buff = silentblade_pct;
                units[attacker_idx].temp_evasion_buff_expires_at_ms = at_ms + VANISH_DURATION_MS;
            }
        }
        // Warrior's Grudge - tracks how many times each distinct attacker
        // has hit THIS unit this fight (never decays), consulted by
        // Retaliation's own counter-damage bonus below.
        if units[target_idx].grudge_pct_per_hit > 0.0 {
            let attacker_id = units[attacker_idx].id.clone();
            if let Some(entry) = units[target_idx].grudge_hit_counts.iter_mut().find(|(id, _)| *id == attacker_id) {
                entry.1 += 1;
            } else {
                units[target_idx].grudge_hit_counts.push((attacker_id, 1));
            }
        }
        // Rogue's Vanish - a crit grants THIS unit a temporary evasion
        // buff (see `temp_evasion_buff`'s doc).
        if outcome.is_crit {
            let vanish_pct = units[attacker_idx].vanish_evasion_pct;
            if vanish_pct > 0.0 {
                let vanish_duration = VANISH_DURATION_MS + units[attacker_idx].fadeaway_duration_bonus_ms;
                units[attacker_idx].temp_evasion_buff = vanish_pct;
                units[attacker_idx].temp_evasion_buff_expires_at_ms = at_ms + vanish_duration;
                // Backstab - while Vanish is active, the NEXT hit deals
                // bonus damage (one-shot, consumed the moment it's read -
                // see `backstab_pending_dmg_pct`'s doc).
                let backstab_pct = units[attacker_idx].backstab_dmg_pct;
                if backstab_pct > 0.0 {
                    units[attacker_idx].backstab_pending_dmg_pct = backstab_pct;
                }
                // Smokescreen - also grants the lowest-HP ally evasion for
                // Vanish's own duration.
                let smokescreen_pct = units[attacker_idx].smokescreen_evasion_pct;
                if smokescreen_pct > 0.0 {
                    let ally_idx = units.iter().enumerate().filter(|(i, u)| *i != attacker_idx && !u.is_boss && u.alive).min_by_key(|(_, u)| u.hp).map(|(i, _)| i);
                    if let Some(ally_idx) = ally_idx {
                        units[ally_idx].temp_evasion_buff = smokescreen_pct;
                        units[ally_idx].temp_evasion_buff_expires_at_ms = at_ms + vanish_duration;
                    }
                }
            }
            // Mage's Volatile Magic - a crit also splashes a fraction of
            // its damage to nearby enemies AS TRUE DAMAGE, not a hit (see
            // `apply_volatile_magic_splash`'s own doc - 2026-08-17 rework,
            // replacing the old `apply_splash`-based version that routed
            // through `apply_hit` and could re-trigger itself). Still
            // gated on `!in_splash_resolution` - not because this call
            // could recurse into itself anymore (it structurally can't),
            // but because a DIFFERENT splash source's own real hit (e.g.
            // Chain Lightning's splash, which still routes through
            // `apply_hit`) could crit while `in_splash_resolution` is set
            // for that unrelated splash, and this trigger should stay
            // suppressed during that window same as before.
            let volatilemagic_pct = units[attacker_idx].volatilemagic_splash_pct;
            if volatilemagic_pct > 0.0 && outcome.damage > 0 && !units[attacker_idx].in_splash_resolution {
                let splash_amount = outcome.damage as f64 * volatilemagic_pct;
                apply_volatile_magic_splash(units, attacker_idx, target_idx, splash_amount, VOLATILE_MAGIC_MAX_TARGETS, at_ms, events, rolls, rng);
            }
        }
        // Berserker's Warlord's Resolve - reaching max Bloodlust stacks
        // grants the whole party a temporary increased-damage buff (see
        // `temp_party_increased_damage_bonus`'s doc), refreshed every hit
        // landed while the stacks stay maxed.
        if units[attacker_idx].stack_speed_max_stacks > 0 && units[attacker_idx].stack_speed_current >= units[attacker_idx].stack_speed_max_stacks {
            let warlord_pct = units[attacker_idx].warlord_party_dmg_pct;
            if warlord_pct > 0.0 {
                for u in units.iter_mut() {
                    if !u.is_boss && u.alive {
                        u.temp_party_increased_damage_bonus = warlord_pct;
                        u.temp_party_increased_damage_bonus_expires_at_ms = at_ms + WARLORD_BUFF_DURATION_MS;
                    }
                }
            }
        }
        // Paladin's Unwavering / Cleric's Unyielding Faith - broadcasts a
        // doubled party-DR grant while the source is below their own
        // (rank-raised) HP threshold, refreshed every hit they land (same
        // "extend the shared window on next trigger" approximation as
        // every other party broadcast here - a source who drops below
        // threshold and dies before their next action simply never
        // broadcasts that cycle).
        if units[attacker_idx].max_hp > 0
            && (units[attacker_idx].hp as f64 / units[attacker_idx].max_hp as f64) < units[attacker_idx].low_hp_party_dr_threshold
        {
            let doubled = units[attacker_idx].low_hp_party_dr_pct * 2.0;
            if doubled > 0.0 {
                for u in units.iter_mut() {
                    if !u.is_boss && u.alive {
                        u.temp_party_damage_reduction_bonus = doubled;
                        u.temp_party_damage_reduction_bonus_expires_at_ms = at_ms + UNWAVERING_BUFF_DURATION_MS;
                    }
                }
            }
        }
        if counts_as_primary_hit {
            add_flowing_stack(units, attacker_idx, target_idx, at_ms);
            // Flow like Water - a crit tops up EXTRA Flowing Strikes
            // stacks on top of add_flowing_stack's own normal +1, even
            // against a new target (no target-match gating - crit is crit
            // regardless of target continuity).
            if outcome.is_crit && units[attacker_idx].onehundredhands_bonus_stacks > 0 {
                let unit = &mut units[attacker_idx];
                unit.flowing_current = (unit.flowing_current + unit.onehundredhands_bonus_stacks).min(unit.flowing_max_stacks);
            }
        }
        // Ranger's Hunter's Mark/Warlock's Curse of Weakness - same
        // "landed hit" gate, one-time per fight (see
        // `apply_first_hit_mark`'s doc).
        apply_first_hit_mark(units, attacker_idx, target_idx, at_ms, rng);
        // Gelatinous Cube's shred - every landed hit this boss deals
        // against a player stacks it (10%/stack, capped at
        // CUBE_SHRED_MAX_STACKS == 50% total). Lives HERE (inside
        // apply_hit, gated on the attacker) rather than
        // apply_first_hit_mark - unlike that one-shot-per-fight helper,
        // this refreshes/stacks on EVERY landed hit. apply_hit is the one
        // function every hit this boss ever lands funnels through
        // (apply_splash calls apply_hit per splash target internally), so
        // this single gate covers the primary target AND all 4 splash
        // targets automatically, no per-call-site special-casing.
        if units[attacker_idx].boss_ability == Some(BossKind::GelatinousCube) && !units[target_idx].is_boss {
            if at_ms > units[target_idx].cube_shred_expires_at_ms {
                units[target_idx].cube_shred_stacks = 0;
            }
            units[target_idx].cube_shred_stacks = (units[target_idx].cube_shred_stacks + 1).min(CUBE_SHRED_MAX_STACKS);
            units[target_idx].cube_shred_expires_at_ms = at_ms + CUBE_SHRED_DURATION_MS;
        }
        // Lingering Effect - every landed hit spawns its own DoT instance
        // (see `apply_lingering_effect`'s doc), using this hit's own
        // pre-defense damage as the basis.
        apply_lingering_effect(units, attacker_idx, target_idx, outcome.unmitigated_damage as f64, false, at_ms);
        // Elemental damage rework (2026-08-15) - see Affix::ColdDamage's
        // doc. Each of the attacker's own 5 damage-type %s independently
        // rolls to debuff the TARGET - splash hits reach here too, since
        // `apply_splash` calls this same function per target, so this
        // needs no separate hook there.
        let attacker_id_for_rolls = units[attacker_idx].id.clone();
        let target_id_for_rolls = units[target_idx].id.clone();
        let fire_pct = units[attacker_idx].fire_damage_pct;
        let (fire_proc, fire_chance) = roll_elemental_proc(fire_pct, &mut units[target_idx].fire_dr_debuff, usize::MAX, at_ms, rng);
        if let Some(chance) = fire_chance {
            rolls.push(elemental_roll_event(hit_id, at_ms, "Fire proc", &attacker_id_for_rolls, &target_id_for_rolls, chance, fire_proc));
        }
        let cold_pct = units[attacker_idx].cold_damage_pct;
        let (cold_proc, cold_chance) = roll_elemental_proc(cold_pct, &mut units[target_idx].cold_evasion_debuff, usize::MAX, at_ms, rng);
        if let Some(chance) = cold_chance {
            rolls.push(elemental_roll_event(hit_id, at_ms, "Cold proc", &attacker_id_for_rolls, &target_id_for_rolls, chance, cold_proc));
        }
        let chaos_pct = units[attacker_idx].chaos_damage_pct;
        let (chaos_proc, chaos_chance) = roll_elemental_proc(chaos_pct, &mut units[target_idx].chaos_block_debuff, usize::MAX, at_ms, rng);
        if let Some(chance) = chaos_chance {
            rolls.push(elemental_roll_event(hit_id, at_ms, "Chaos proc", &attacker_id_for_rolls, &target_id_for_rolls, chance, chaos_proc));
        }
        let lightning_pct = units[attacker_idx].lightning_damage_pct;
        let (lightning_proc, lightning_chance) = roll_elemental_proc(lightning_pct, &mut units[target_idx].lightning_dmg_taken, ELEMENTAL_LIGHTNING_MAX_STACKS, at_ms, rng);
        if let Some(chance) = lightning_chance {
            rolls.push(elemental_roll_event(hit_id, at_ms, "Lightning proc", &attacker_id_for_rolls, &target_id_for_rolls, chance, lightning_proc));
        }
        // Monk's Chakra of Light - a SEPARATE trigger onto the same real
        // Lightning Damage debuff stack above, scaled off the attacker's
        // own `increased_damage` rather than a gear-rolled proc chance (see
        // `roll_chakra_of_light_stacks`'s doc). Purely a target-side debuff
        // application - never touches this hit's own damage.
        let chakraoflight_pct = units[attacker_idx].chakra_of_light_pct;
        if chakraoflight_pct > 0.0 {
            // No *100 here - `roll_chakra_of_light_stacks` (like
            // `roll_divine_heal_power_proc`, its template) reads its input
            // as a plain fraction where 1.0 = 1 guaranteed stack, same
            // scale `increased_damage` itself is already stored in (30.0 =
            // 3000%). 3000% increased damage * 10% (rank 1) = 3.0 -> 3
            // guaranteed stacks, matching the designed example exactly.
            let raw_pct = units[attacker_idx].increased_damage * chakraoflight_pct;
            roll_chakra_of_light_stacks(raw_pct, &mut units[target_idx].lightning_dmg_taken, ELEMENTAL_LIGHTNING_MAX_STACKS, at_ms, rng);
        }
        let divine_pct = units[attacker_idx].divine_damage_pct;
        let (divine_proc, divine_chance) = roll_elemental_proc(divine_pct, &mut units[target_idx].divine_heal_reduction, ELEMENTAL_DIVINE_ENEMY_MAX_STACKS, at_ms, rng);
        if let Some(chance) = divine_chance {
            rolls.push(elemental_roll_event(hit_id, at_ms, "Divine proc", &attacker_id_for_rolls, &target_id_for_rolls, chance, divine_proc));
        }
        // Combat logging - see `CombatEvent::BuffSnapshot`'s doc. Fired
        // for both participants right after this hit's own direct
        // effects land, not necessarily the ABSOLUTE final word on their
        // state this action (a few rarer downstream branches - shield
        // absorb, Retaliation, an on-kill effect - can still adjust
        // things after this point), same "good enough, not exhaustive"
        // scope `active_buffs_snapshot`'s own doc already accepts.
        let attacker_id = units[attacker_idx].id.clone();
        let attacker_buffs = active_buffs_snapshot(&units[attacker_idx], at_ms);
        if !attacker_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: attacker_id, buffs: attacker_buffs });
        }
        let target_id = units[target_idx].id.clone();
        let target_buffs = active_buffs_snapshot(&units[target_idx], at_ms);
        if !target_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: target_id, buffs: target_buffs });
        }
    } else {
        add_speed_stack(units, target_idx, at_ms);
        // Ranger's Vanishing Shot/Fleeting Shadow - temporary crit-chance/
        // attack-speed buffs off this same "I evaded" trigger.
        let vanishingshot_pct = units[target_idx].vanishingshot_crit_pct;
        if vanishingshot_pct > 0.0 {
            units[target_idx].temp_crit_chance_buff = vanishingshot_pct;
            units[target_idx].temp_crit_chance_buff_expires_at_ms = at_ms + VANISHING_SHOT_DURATION_MS;
        }
        let fleetingshadow_pct = units[target_idx].fleetingshadow_speed_pct;
        if fleetingshadow_pct > 0.0 {
            units[target_idx].temp_party_attack_speed_bonus = fleetingshadow_pct;
            units[target_idx].temp_party_attack_speed_bonus_expires_at_ms = at_ms + FLEETING_SHADOW_DURATION_MS;
        }
        // Monk's Inner Focus - a successful evade heals THIS unit (the
        // one who just evaded), with Meditation scaling the heal further
        // by their own live evasion total (in 10%-increment steps, same
        // "floor(stat/step) * per-step bonus" idiom as Gambit's missing-
        // HP scaling), Chi Burst sharing a fraction of that SAME heal
        // with their lowest-HP ally, and Serenity granting a temporary DR
        // buff off the same trigger.
        trigger_inner_focus(units, target_idx, at_ms, events, rng);
        // Rogue's Voidstep/Monk's Counterflow/Druid's Wild Fury - a
        // successful evade has a chance to trigger an immediate free
        // counter-attack, modeled directly on Warrior's Retaliation
        // (`evade_counter_chance`'s doc) but gated on `outcome.evaded`
        // instead, and reusing the same `!is_followup` guard so a counter
        // that itself gets evaded can't chain into a second free counter.
        if counts_as_primary_hit && units[target_idx].alive && units[attacker_idx].alive {
            let counter_chance = units[target_idx].evade_counter_chance;
            // Capped at 1 real trigger per second (a live request) - many
            // evades landing in quick succession (multiple enemies/adds
            // attacking) could otherwise roll this far more often than
            // intended.
            let counter_ready = at_ms >= units[target_idx].evade_counter_last_fired_at_ms.saturating_add(1_000);
            if counter_chance > 0.0 && counter_ready && rng.gen_bool(counter_chance.clamp(0.0, 1.0)) {
                units[target_idx].evade_counter_last_fired_at_ms = at_ms;
                let counter_base = attacker_base_damage(&units[target_idx], rng);
                apply_hit(units, target_idx, attacker_idx, counter_base, at_ms, events, rolls, rng, false, true);
            }
        }
        // Combat logging - see `CombatEvent::BuffSnapshot`'s doc, same
        // "both participants, right after this hit's own direct effects"
        // scope as the landed branch above.
        let attacker_id = units[attacker_idx].id.clone();
        let attacker_buffs = active_buffs_snapshot(&units[attacker_idx], at_ms);
        if !attacker_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: attacker_id, buffs: attacker_buffs });
        }
        let target_id = units[target_idx].id.clone();
        let target_buffs = active_buffs_snapshot(&units[target_idx], at_ms);
        if !target_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: target_id, buffs: target_buffs });
        }
    }

    // Slayer's Martyrdom shield (see `shield_hp`'s doc) - consumed before
    // hp, lazily treated as expired once `shield_expires_at_ms` has
    // passed. The event log reports whatever actually came off hp (post-
    // shield), not the shield-absorbed portion, so `damage`/
    // `target_hp_after` always stay internally consistent.
    let mut final_damage = outcome.damage as i64;
    if final_damage > 0 && units[target_idx].shield_hp > 0.0 && at_ms <= units[target_idx].shield_expires_at_ms {
        let pre_absorb_damage = final_damage;
        let absorbed = units[target_idx].shield_hp.min(final_damage as f64);
        units[target_idx].shield_hp -= absorbed;
        final_damage -= absorbed.round() as i64;
        // Shield-absorb reflect (Sacred Barrier/Retribution Aura/
        // Guardian's Blood - see `shield_reflect_pct`'s doc).
        let reflect_pct = units[target_idx].shield_reflect_pct;
        if absorbed > 0.0 && reflect_pct > 0.0 && units[attacker_idx].alive {
            let fully_absorbed = final_damage <= 0 && absorbed.round() as i64 >= pre_absorb_damage;
            let full_absorb_ok = !units[target_idx].shield_reflect_requires_full_absorb || fully_absorbed;
            let chance = units[target_idx].shield_reflect_chance;
            let should_reflect = full_absorb_ok && (chance >= 1.0 || rng.gen_bool(chance.clamp(0.0, 1.0)));
            if should_reflect {
                let reflect_amount = absorbed * reflect_pct;
                apply_reflect_damage(units, target_idx, attacker_idx, reflect_amount, at_ms, events, rolls, rng);
                // Purify/Last Judgment - both specifically require a FULLY
                // reflected hit (Retribution Aura's own gate).
                if fully_absorbed {
                    let purify_pct = units[target_idx].purify_dmg_debuff_pct;
                    if purify_pct > 0.0 {
                        units[attacker_idx].temp_damage_dealt_debuff = purify_pct;
                        units[attacker_idx].temp_damage_dealt_debuff_expires_at_ms = at_ms + PURIFY_DEBUFF_DURATION_MS;
                    }
                    let lastjudgment_chance = units[target_idx].lastjudgment_skip_chance;
                    if lastjudgment_chance > 0.0 && rng.gen_bool(lastjudgment_chance.clamp(0.0, 1.0)) {
                        units[attacker_idx].next_action_at_ms += units[attacker_idx].attack_interval_ms;
                    }
                }
            }
        }
    }

    // Warlock's Curse of Weakness family (2026-08-16, a live design call -
    // see `HitOutcome::curse_bonus_damage`'s doc) - the marginal damage
    // this hit only dealt because the target was cursed is split back out
    // here and credited to the CURSING Warlock instead of this attacker,
    // both for `damage_dealt_total` (Cthulhu's live "top DPS" read) and
    // the `CombatEvent::Attack` pushed further below (what `summarize_fight`'s
    // own "Top DPS" leaderboard actually sums). Computed once, capped to
    // whatever `final_damage` actually has left post-shield-absorption
    // (can't credit more than what actually landed), reused at both sites
    // below rather than recomputed - `final_damage` itself is untouched
    // from here on, so this stays correct for either use. 0 whenever the
    // target isn't cursed, or the curser IS the attacker (crediting
    // yourself for cursing your own target would just be a no-op split).
    // Disabled (2026-08-18, explicit request) - the curse's marginal
    // damage no longer gets carved out of the real attacker's credit and
    // handed to the cursing Warlock; a hit against a cursed target now
    // counts entirely toward whoever actually landed it, same as any
    // other hit. `curse_credit_id`/`curse_share` below are still computed
    // (harmless, and keeps the two use sites below compiling
    // unconditionally) - only their two actual USE sites are gated behind
    // this flag, so flipping it back to `true` restores the original
    // credit-to-Warlock behavior exactly, without needing to re-derive
    // any of this - same "left implemented-but-inert" precedent as
    // Slayer's Rot/Withering Touch.
    const CURSE_CREDITS_WARLOCK_DAMAGE: bool = false;
    let curse_credit_id = units[target_idx].curse_source_id.clone().filter(|id| *id != units[attacker_idx].id);
    let curse_share = if curse_credit_id.is_some() { (outcome.curse_bonus_damage.round().max(0.0) as i64).min(final_damage.max(0)) } else { 0 };

    // Druid's Bramblegrowth - a hit that got reduced by DR/block reflects
    // a fraction of the REDUCED amount (unmitigated minus what actually
    // landed, pre-shield - see `bramble_reflect_pct`'s doc for why this
    // is the total combined reduction rather than isolating Thorned
    // Barrier's own slice) back at the attacker. Poison Thorns' debuff
    // rides along on the same trigger.
    let bramble_pct = units[target_idx].bramble_reflect_pct;
    if bramble_pct > 0.0 && units[attacker_idx].alive {
        // Entangle - track distinct attacker ids that have hit THIS unit
        // within a short rolling window (see `recent_attackers`'s doc for
        // why this is a window rather than a literal "same turn" - the sim
        // has no shared turn boundary). Pruned before use, then this hit's
        // own attacker recorded, so the check below only ever sees a
        // DIFFERENT, still-recent attacker.
        units[target_idx].recent_attackers.retain(|(_, expires)| *expires > at_ms);
        let reduced = outcome.unmitigated_damage.saturating_sub(outcome.damage);
        if reduced > 0 {
            let reflect_amount = reduced as f64 * bramble_pct;
            apply_reflect_damage(units, target_idx, attacker_idx, reflect_amount, at_ms, events, rolls, rng);
            let debuff_pct = units[target_idx].poison_thorns_debuff_pct;
            if debuff_pct > 0.0 {
                units[attacker_idx].temp_damage_dealt_debuff = debuff_pct;
                units[attacker_idx].temp_damage_dealt_debuff_expires_at_ms = at_ms + POISON_THORNS_DEBUFF_DURATION_MS;
            }
            let entangle_chance = units[target_idx].entangle_chance;
            if entangle_chance > 0.0 {
                let attacker_id = units[attacker_idx].id.clone();
                let other_id = units[target_idx].recent_attackers.iter().find(|(id, _)| *id != attacker_id).map(|(id, _)| id.clone());
                if let Some(other_id) = other_id {
                    if rng.gen_bool(entangle_chance.clamp(0.0, 1.0)) {
                        if let Some(other_idx) = units.iter().position(|u| u.id == other_id && u.alive) {
                            apply_reflect_damage(units, target_idx, other_idx, reflect_amount, at_ms, events, rolls, rng);
                        }
                    }
                }
            }
        }
        let attacker_id = units[attacker_idx].id.clone();
        units[target_idx].recent_attackers.retain(|(id, _)| *id != attacker_id);
        units[target_idx].recent_attackers.push((attacker_id, at_ms + ENTANGLE_WINDOW_MS));
    }

    // Warrior's Spike Barrier/Aegis - triggered specifically by a
    // successful BLOCK (`outcome.is_blocked`), not just any mitigation the
    // way Bramblegrowth is above - reuses the exact same "total mitigated
    // on this hit" quantity though, per the same "total reduction, gated
    // on investment" precedent.
    // Monk's Clarity - Inner Focus also triggers on a blocked hit, not
    // just an evade (a flat "any DR mitigation" approximation rather than
    // distinguishing "Iron Body specifically" for its rank-3 clause -
    // `resolve_hit` doesn't expose which DR source actually mitigated).
    if outcome.is_blocked && units[target_idx].clarity_triggers_on_block {
        trigger_inner_focus(units, target_idx, at_ms, events, rng);
    }
    if outcome.is_blocked {
        let blocked_amount = outcome.unmitigated_damage.saturating_sub(outcome.damage);
        if blocked_amount > 0 {
            let spike_pct = units[target_idx].spike_barrier_reflect_pct;
            if spike_pct > 0.0 && units[attacker_idx].alive {
                let mut reflect_amount = blocked_amount as f64 * spike_pct;
                // Retribution - a chance for the reflect to crit for
                // double.
                let retribution_chance = units[target_idx].spike_retribution_chance;
                if retribution_chance > 0.0 && rng.gen_bool(retribution_chance.clamp(0.0, 1.0)) {
                    reflect_amount *= 2.0;
                }
                apply_reflect_damage(units, target_idx, attacker_idx, reflect_amount, at_ms, events, rolls, rng);
                // Thornedhide - stacks a damage-dealt debuff on the
                // attacker every time Spike Barrier reflects.
                if units[target_idx].thornedhide_pct_per_stack > 0.0 && units[attacker_idx].alive {
                    if at_ms > units[attacker_idx].thornedhide_expires_at_ms {
                        units[attacker_idx].thornedhide_stacks = 0;
                    }
                    units[attacker_idx].thornedhide_stacks = (units[attacker_idx].thornedhide_stacks + 1).min(5);
                    units[attacker_idx].thornedhide_expires_at_ms = at_ms + THORNEDHIDE_DURATION_MS;
                    units[attacker_idx].thornedhide_debuff_pct_per_stack = units[target_idx].thornedhide_pct_per_stack;
                }
            }
            let aegis_pct = units[target_idx].aegis_shield_pct;
            if aegis_pct > 0.0 {
                let shield_amount = blocked_amount as f64 * aegis_pct;
                let duration = units[target_idx].aegis_shield_duration_ms;
                // Ironcircle - shields the 1+N lowest-HP allies instead of
                // just the single lowest.
                let extra = units[target_idx].aegis_extra_targets as usize;
                let mut allies: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
                allies.sort_by_key(|&i| units[i].hp);
                let rally_pct = units[target_idx].aegis_rally_speed_pct;
                for &ally_idx in allies.iter().take(1 + extra) {
                    grant_shield(units, target_idx, ally_idx, shield_amount, at_ms, duration, events);
                    // Rally - the shielded ally also gets temporary attack
                    // speed for the shield's duration.
                    if rally_pct > 0.0 {
                        units[ally_idx].temp_party_attack_speed_bonus = rally_pct;
                        units[ally_idx].temp_party_attack_speed_bonus_expires_at_ms = at_ms + duration;
                    }
                }
            }
        }
    } else {
        // Unyielding - Spike Barrier has a chance to also trigger off an
        // unblocked hit that was still DR-reduced, off the same reduced-
        // amount quantity Bramblegrowth's own reflect uses.
        let unyielding_chance = units[target_idx].spike_unyielding_chance;
        let spike_pct = units[target_idx].spike_barrier_reflect_pct;
        if unyielding_chance > 0.0 && spike_pct > 0.0 && units[attacker_idx].alive {
            let reduced = outcome.unmitigated_damage.saturating_sub(outcome.damage);
            if reduced > 0 && rng.gen_bool(unyielding_chance.clamp(0.0, 1.0)) {
                let reflect_amount = reduced as f64 * spike_pct;
                apply_reflect_damage(units, target_idx, attacker_idx, reflect_amount, at_ms, events, rolls, rng);
            }
        }
    }

    // Cleric's Guardian Spirit (see `guardian_spirit_charges`'s doc) - a
    // reactive party-wide ward, checked here (after shield absorption,
    // before hp is allowed to reach 0) on any non-boss target, including
    // the Cleric themselves. Looks for ANY alive Cleric in the party
    // still holding a charge - not necessarily the target - and consumes
    // it to heal instead of letting the killing blow land at all. Skips
    // this hit's normal wound/leech consequences below entirely (a
    // prevented death reads as a full negation, not a normal hit that
    // also happened to wound/leech on the way through).
    if !units[target_idx].is_boss && final_damage >= units[target_idx].hp {
        // Precomputed once, reused by Verdant Burst's else-if branch below
        // (Druid, 2026-08-16 rework) - total pending (not-yet-delivered)
        // healing from each active heal-flavor Lingering Effect instance
        // currently on this target, grouped by source id. Computed up
        // front (rather than inline in the closure below) specifically to
        // avoid indexing `units[target_idx]` a second time from inside a
        // closure that's also iterating `units` via `.position()`.
        let mut verdant_pending_by_source: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for dot in &units[target_idx].lingering_dots {
            if dot.is_heal {
                *verdant_pending_by_source.entry(dot.source_id.clone()).or_insert(0.0) += dot.amount_per_tick * dot.remaining_ticks as f64;
            }
        }
        if let Some(saver_idx) = units.iter().position(|u| !u.is_boss && u.alive && u.guardian_spirit_charges > 0) {
            units[saver_idx].guardian_spirit_charges -= 1;
            events.push(CombatEvent::SkillCast { at_ms, unit: units[saver_idx].id.clone(), skill: "Guardian Spirit".to_string() });
            let heal_pct = units[saver_idx].guardian_spirit_heal_pct;
            let new_hp = (units[target_idx].max_hp as f64 * heal_pct).round().max(1.0).min(units[target_idx].max_hp as f64) as i64;
            units[target_idx].hp = new_hp;
            units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
            let attacker_id = units[attacker_idx].id.clone();
            let target_id = units[target_idx].id.clone();
            events.push(CombatEvent::Attack {
                at_ms,
                attacker: attacker_id,
                target: target_id.clone(),
                damage: final_damage.max(0) as u64,
                unmitigated_damage: outcome.unmitigated_damage,
                target_hp_after: new_hp as u64,
                is_crit: outcome.is_crit,
                evaded: outcome.evaded,
                hit_id,
            });
            let saver_id = units[saver_idx].id.clone();
            events.push(CombatEvent::Heal { at_ms, healer: saver_id, target: target_id, amount: new_hp as u64, target_hp_after: new_hp as u64 });
            // Divine Intervention - +damage reduction for the saved unit.
            let dr_bonus = units[saver_idx].guardian_spirit_save_dr_pct;
            if dr_bonus > 0.0 {
                units[target_idx].temp_damage_reduction_bonus = dr_bonus;
                units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + GUARDIAN_SPIRIT_SAVE_BUFF_DURATION_MS;
            }
            // Final Blessing - +healing power for the WHOLE PARTY, not
            // just the saved unit.
            let heal_power_bonus = units[saver_idx].guardian_spirit_save_heal_power_pct;
            if heal_power_bonus > 0.0 {
                for u in units.iter_mut() {
                    if !u.is_boss && u.alive {
                        u.temp_heal_power_bonus = heal_power_bonus;
                        u.temp_heal_power_bonus_expires_at_ms = at_ms + GUARDIAN_SPIRIT_SAVE_BUFF_DURATION_MS;
                    }
                }
            }
            return;
        } else if units[target_idx].frenzy_undying_charges > 0 {
            // Berserker's Undying Fury - a SELF-only fallback (unlike
            // Guardian Spirit above, this never looks at anyone else's
            // charges), same lethal-hit gate. Leaves the holder at 1 HP
            // instead of healing them.
            units[target_idx].frenzy_undying_charges -= 1;
            units[target_idx].hp = 1;
            units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
            let attacker_id = units[attacker_idx].id.clone();
            let target_id = units[target_idx].id.clone();
            events.push(CombatEvent::Attack {
                at_ms,
                attacker: attacker_id,
                target: target_id,
                damage: final_damage.max(0) as u64,
                unmitigated_damage: outcome.unmitigated_damage,
                target_hp_after: 1,
                is_crit: outcome.is_crit,
                evaded: outcome.evaded,
                hit_id,
            });
            return;
        } else if let Some(druid_idx) =
            units.iter().position(|u| !u.is_boss && u.alive && u.verdantburst_charges > 0 && verdant_pending_by_source.get(&u.id).copied().unwrap_or(0.0) > final_damage as f64)
        {
            // Druid's Verdant Burst (2026-08-16 rework) - only saves the
            // target if this Druid's own pending Lingering Effect healing
            // on them was already enough to have outpaced the blow; leaves
            // them at 1 HP (not healed to a %, unlike Guardian Spirit -
            // the pending lingering heals keep ticking normally afterward).
            units[druid_idx].verdantburst_charges -= 1;
            events.push(CombatEvent::SkillCast { at_ms, unit: units[druid_idx].id.clone(), skill: "Verdant Burst".to_string() });
            units[target_idx].hp = 1;
            units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
            let attacker_id = units[attacker_idx].id.clone();
            let target_id = units[target_idx].id.clone();
            events.push(CombatEvent::Attack {
                at_ms,
                attacker: attacker_id,
                target: target_id,
                damage: final_damage.max(0) as u64,
                unmitigated_damage: outcome.unmitigated_damage,
                target_hp_after: 1,
                is_crit: outcome.is_crit,
                evaded: outcome.evaded,
                hit_id,
            });
            return;
        } else if units[target_idx].soul_stones > 0 {
            // Warlock's Soul Stone - a SELF-only fallback (unlike Guardian
            // Spirit above, this never looks at anyone else's stones),
            // same lethal-hit gate. Heals to FULL (unlike Undying
            // Fury/Verdant Burst's 1-HP save), but stacks a permanent
            // outgoing-damage penalty for the rest of the fight (see
            // `soul_stone_uses_this_fight`/`SOUL_STONE_DMG_PENALTY_PER_USE`
            // in `resolve_hit`) - direct field writes, not a call into
            // `apply_heal`, same "no recursive proc pipeline" convention
            // every other branch in this chain already follows.
            units[target_idx].soul_stones -= 1;
            units[target_idx].soul_stone_uses_this_fight += 1;
            events.push(CombatEvent::SkillCast { at_ms, unit: units[target_idx].id.clone(), skill: "Soul Stone".to_string() });
            let max_hp = units[target_idx].max_hp as i64;
            let healed = (max_hp - units[target_idx].hp).max(0);
            units[target_idx].hp = max_hp;
            units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
            let attacker_id = units[attacker_idx].id.clone();
            let target_id = units[target_idx].id.clone();
            events.push(CombatEvent::Attack {
                at_ms,
                attacker: attacker_id,
                target: target_id.clone(),
                damage: final_damage.max(0) as u64,
                unmitigated_damage: outcome.unmitigated_damage,
                target_hp_after: max_hp as u64,
                is_crit: outcome.is_crit,
                evaded: outcome.evaded,
                hit_id,
            });
            events.push(CombatEvent::Heal { at_ms, healer: target_id.clone(), target: target_id, amount: healed as u64, target_hp_after: max_hp as u64 });
            return;
        } else if units[target_idx].chakraoflife_duration_ms > 0 {
            // Monk's Chakra of Life - a SELF-only fallback (unlike Guardian
            // Spirit above, always available, no charge/resource to
            // consume), same lethal-hit gate. Doesn't touch hp at all - the
            // hit is fully negated, same "prevented death reads as a full
            // negation" convention every branch in this chain follows -
            // instead grants full damage immunity for `chakraoflife_duration_ms`
            // (enforced by `resolve_hit`'s evasion-equivalent check and the
            // handful of true-damage call sites outside the normal hit
            // pipeline - see each site's own doc), then schedules an
            // unconditional death via `NextEvent::ChakraOfLifeExpiry` once
            // that window ends - no attacker credited for that kill, it's a
            // timer death, not a normal one.
            units[target_idx].chakraoflife_immune_until_ms = at_ms + units[target_idx].chakraoflife_duration_ms;
            units[target_idx].next_chakraoflife_expiry_at_ms = at_ms + units[target_idx].chakraoflife_duration_ms;
            events.push(CombatEvent::SkillCast { at_ms, unit: units[target_idx].id.clone(), skill: "Chakra of Life".to_string() });
            units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
            let attacker_id = units[attacker_idx].id.clone();
            let target_id = units[target_idx].id.clone();
            events.push(CombatEvent::Attack {
                at_ms,
                attacker: attacker_id,
                target: target_id,
                damage: final_damage.max(0) as u64,
                unmitigated_damage: outcome.unmitigated_damage,
                target_hp_after: units[target_idx].hp as u64,
                is_crit: outcome.is_crit,
                evaded: outcome.evaded,
                hit_id,
            });
            return;
        }
    }

    let new_hp = (units[target_idx].hp - final_damage).max(0);
    units[target_idx].hp = new_hp;
    // Purely so Cthulhu's ability can find "the top DPS" later - see
    // `damage_dealt_total`'s doc. Harmless to track on a boss/add
    // attacker too, just never read for those. Curse's own share (see
    // `curse_share`'s doc above) would go to the cursing Warlock instead,
    // while `CURSE_CREDITS_WARLOCK_DAMAGE` is enabled - disabled today, so
    // this attacker just keeps the full `final_damage` credit like any
    // other hit.
    if CURSE_CREDITS_WARLOCK_DAMAGE {
        units[attacker_idx].damage_dealt_total += (final_damage - curse_share).max(0) as u64;
        if let Some(curse_source_id) = &curse_credit_id {
            if let Some(curser_idx) = units.iter().position(|u| u.id == *curse_source_id) {
                units[curser_idx].damage_dealt_total += curse_share.max(0) as u64;
            }
        }
    } else {
        units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
    }

    // Warlock's Doom - bank actual damage dealt to this target while its
    // curse is being Doom-tracked (see `curse_damage_taken_total`'s doc),
    // for the detonation to consume once the curse expires. A no-op
    // (harmless) for anyone not currently curse-tracked.
    if units[target_idx].curse_expires_at_ms != u32::MAX && at_ms <= units[target_idx].curse_expires_at_ms {
        units[target_idx].curse_damage_taken_total += final_damage.max(0) as f64;
    }

    // Rogue's Premeditation - refunds Assassinate's charge if the
    // triggering hit didn't kill.
    if assassinate_triggered && new_hp > 0 {
        let premeditation_chance = units[attacker_idx].premeditation_refund_chance;
        if premeditation_chance > 0.0 && rng.gen_bool(premeditation_chance.clamp(0.0, 1.0)) {
            units[attacker_idx].assassinate_charges += 1;
        }
    }

    // Slayer's Open Wound (see `CombatSimUnit`'s doc) - a successful
    // (non-evaded) hit from an attacker with it invested refreshes the
    // target's wound: stacks up (capped at the attacker's own
    // `wound_deal_max_stacks`), duration resets, and the target's
    // leech/damage-dealt/heal-received snapshot is copied fresh from the
    // attacker's CURRENT investment. Reaching max stacks triggers
    // Hemorrhage's explosion.
    // `applies_wound` is `false` only for a splash hit whose attacker
    // doesn't have Festering Wound invested (see `apply_splash`) - Open
    // Wound's own text only ever says "hits apply a stacking wound",
    // with splash spread specifically called out as Festering's OWN
    // bonus, not the baseline.
    if applies_wound && !outcome.evaded && units[attacker_idx].wound_deal_max_stacks > 0 && units[target_idx].alive {
        let still_wounded = units[target_idx].wound_stacks > 0 && at_ms <= units[target_idx].wound_expires_at_ms;
        let current_stacks = if still_wounded { units[target_idx].wound_stacks } else { 0 };
        let new_stacks = (current_stacks + 1).min(units[attacker_idx].wound_deal_max_stacks);
        units[target_idx].wound_stacks = new_stacks;
        units[target_idx].wound_max_stacks = units[attacker_idx].wound_deal_max_stacks;
        units[target_idx].wound_expires_at_ms = at_ms + units[attacker_idx].wound_deal_duration_ms;
        units[target_idx].wound_leech_per_stack = units[attacker_idx].wound_deal_leech_per_stack;
        units[target_idx].wound_damage_dealt_debuff = units[attacker_idx].wound_deal_damage_dealt_debuff;
        units[target_idx].wound_heal_received_debuff = units[attacker_idx].wound_deal_heal_received_debuff;
        // Grave Chill - wounded enemies are also slowed (reuses the same
        // temp attack-speed debuff/read-site Static Field established).
        let gravechill_pct = units[attacker_idx].gravechill_speed_debuff_pct;
        if gravechill_pct > 0.0 {
            units[target_idx].temp_attack_speed_debuff = gravechill_pct;
            units[target_idx].temp_attack_speed_debuff_expires_at_ms = units[target_idx].wound_expires_at_ms;
        }
        // Plague Bearer - Necrotic Grip's damage-dealt debuff also spreads
        // to nearby enemies at wound-apply time (same spread-loop shape
        // Contagious Curse/Wider Pack already established).
        let plaguebearer_extra = units[attacker_idx].plaguebearer_extra_targets;
        if plaguebearer_extra > 0 && units[attacker_idx].wound_deal_damage_dealt_debuff > 0.0 {
            let target_is_boss = units[target_idx].is_boss;
            let mut others: Vec<usize> = units.iter().enumerate().filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive).map(|(i, _)| i).collect();
            for _ in 0..plaguebearer_extra.min(others.len() as u32) {
                let pick = rng.gen_range(0..others.len());
                let other_idx = others.remove(pick);
                units[other_idx].wound_damage_dealt_debuff = units[attacker_idx].wound_deal_damage_dealt_debuff;
                units[other_idx].wound_expires_at_ms = units[target_idx].wound_expires_at_ms;
            }
        }
        // A fresh wound (this hit is what STARTED it, not a refresh of an
        // already-ticking one) starts the banked-damage tally over -
        // see `wound_damage_taken_total`'s doc.
        if !still_wounded {
            units[target_idx].wound_damage_taken_total = 0.0;
        }
        units[target_idx].wound_damage_taken_total += final_damage.max(0) as f64;

        // Festering Wound - the SAME wound also lands on this attacker's
        // splash targets, not just their primary target here. Only
        // meaningful when `apply_hit` was itself called for a splash hit
        // (see `apply_splash`) - a primary hit's own target is already
        // handled above, this only reaches OTHER units already hit this
        // action via splash, so it's a no-op double-apply guard by
        // construction (nothing else calls `apply_hit` twice for the
        // same target in one action).
        let _ = units[attacker_idx].wound_deal_spreads_to_splash; // consulted by apply_splash's caller context, not here directly

        if new_stacks >= units[attacker_idx].wound_deal_max_stacks && units[attacker_idx].wound_deal_explosion_pct > 0.0 {
            // Hemorrhage - a bonus explosion off the wound's own banked
            // damage (see `wound_damage_taken_total`'s doc), consumed
            // (reset to 0) the instant it fires so a LATER wound on the
            // same target starts its own tally fresh. Arterial Spray adds
            // more targets from the same side; Overflow leeches a slice
            // back.
            // `wound_stacks` ALSO resets to 0 here now (2026-08-16, a real
            // bug fix, not a numeric-overflow one - a live report of a
            // Slayer's damage reaching the tens of billions traced back to
            // this) - previously only `wound_damage_taken_total` reset,
            // but `wound_stacks` never decreases on its own once it hits
            // max (only a full wound EXPIRY zeroed it), so this condition
            // (`new_stacks >= max_stacks`) stayed true on literally every
            // subsequent connecting hit for as long as the wound kept
            // getting refreshed - a full-damage explosion on nearly every
            // hit instead of once per genuine stack-buildup, for the rest
            // of the fight. Resetting stacks forces a real rebuild (another
            // `max_stacks` hits) before the next explosion can fire.
            let secondwind_chance = units[attacker_idx].bloodpact_secondwind_reset_chance;
            // Second Wind (wound -> hemorrhage -> hemorrhagesecondwind) is a
            // SEPARATE branch from Sacrifice/Bloodpact (wound_deal_max_stacks
            // > 0 alone doesn't imply it) - a Slayer can have this without
            // ever investing in Bloodpact, in which case `bloodpact_cooldown_ms`
            // is still its "never invested" u32::MAX sentinel. Gating on that
            // (real investment only) fixes a live crash: writing a small real
            // `next_bloodpact_at_ms` for such a character while
            // `bloodpact_cooldown_ms` stays u32::MAX made the very next
            // `at_ms + bloodpact_cooldown_ms` (below) overflow u32 and panic,
            // killing the fight this character was in.
            if secondwind_chance > 0.0 && units[attacker_idx].bloodpact_cooldown_ms < u32::MAX && rng.gen_bool(secondwind_chance.clamp(0.0, 1.0)) {
                // Clamped so Bloodpact can never actually re-fire more than
                // once per 1000ms from ANY reset source (this one or Clean
                // Slate's) - "ready now" only if a full second has already
                // passed since it last really fired; otherwise pushed out
                // to that 1s floor instead of granting an immediate re-use.
                units[attacker_idx].next_bloodpact_at_ms = at_ms.max(units[attacker_idx].bloodpact_last_fired_at_ms + 1_000);
            }
            let explosion_base = units[target_idx].wound_damage_taken_total * units[attacker_idx].wound_deal_explosion_pct;
            units[target_idx].wound_damage_taken_total = 0.0;
            units[target_idx].wound_stacks = 0;
            let mut explosion_targets = vec![target_idx];
            if units[attacker_idx].wound_deal_explosion_extra_targets > 0 {
                let target_is_boss = units[target_idx].is_boss;
                let extra: Vec<usize> = units
                    .iter()
                    .enumerate()
                    .filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive)
                    .map(|(i, _)| i)
                    .take(units[attacker_idx].wound_deal_explosion_extra_targets as usize)
                    .collect();
                explosion_targets.extend(extra);
            }
            let self_leech_pct = units[attacker_idx].wound_deal_explosion_self_leech_pct;
            for explosion_target in explosion_targets {
                if !units[explosion_target].alive {
                    continue;
                }
                // Flat, unmodifiable true damage (2026-08-17, a live
                // request) - explosion_base itself IS the damage dealt, no
                // crit roll and no `resolve_hit`/mitigation pass at all.
                // Previously routed through `resolve_hit`, which meant this
                // already-huge "% of banked damage" base ALSO got crit-
                // multiplied - with high enough crit chance (guaranteed
                // extra crit-multiplier stacks past 100%, see
                // `roll_attacker_damage`'s doc), that compounded an already
                // large number into the trillions, a live-reported bug.
                // Same "true damage, not a hit" shape as
                // `apply_reflect_damage`/Volatile Magic's splash - always an
                // enemy target (see `explosion_targets`' own same-side
                // filter above), so Pack Instinct/Symbiosis (ally-only)
                // never apply here regardless.
                // Shield absorption - the same block `apply_hit` runs,
                // duplicated here since this explosion bypasses apply_hit
                // entirely (it needs its own target list/self-leech
                // handling, not a single attacker/target pair). A live
                // audit found this missing outright: an active shield used
                // to do nothing at all against a Hemorrhage explosion -
                // still respected here even though crit/mitigation aren't.
                let ex_attacker_id = units[attacker_idx].id.clone();
                let hit_id = next_hit_id();
                let penalized_base = apply_late_stage_penalty(units, explosion_target, explosion_base, at_ms, hit_id, &ex_attacker_id, rolls);
                let mut final_damage = penalized_base.round().max(0.0) as i64;
                if final_damage > 0 && units[explosion_target].shield_hp > 0.0 && at_ms <= units[explosion_target].shield_expires_at_ms {
                    let absorbed = units[explosion_target].shield_hp.min(final_damage as f64);
                    units[explosion_target].shield_hp -= absorbed;
                    final_damage -= absorbed.round() as i64;
                }
                let ex_new_hp = (units[explosion_target].hp - final_damage).max(0);
                units[explosion_target].hp = ex_new_hp;
                // Same audit: this explosion's damage updated hp and the
                // event log correctly, but never counted toward
                // damage_dealt_total - the live counter Cthulhu's
                // "bubble the top DPS" ability reads directly, so a
                // Slayer leaning on Hemorrhage explosions could be
                // undervalued there.
                units[attacker_idx].damage_dealt_total += final_damage.max(0) as u64;
                let ex_target_id = units[explosion_target].id.clone();
                events.push(CombatEvent::Attack {
                    at_ms,
                    attacker: ex_attacker_id,
                    target: ex_target_id.clone(),
                    damage: final_damage.max(0) as u64,
                    // Pre-shield, post-late-stage-penalty flat amount -
                    // shields still reduce what actually lands
                    // (`final_damage`), but there's no mitigation step to
                    // distinguish "unmitigated" from otherwise, same
                    // convention `apply_reflect_damage` uses.
                    unmitigated_damage: penalized_base.round().max(0.0) as u64,
                    target_hp_after: ex_new_hp as u64,
                    is_crit: false,
                    evaded: false,
                    hit_id,
                });
                if self_leech_pct > 0.0 && final_damage > 0 {
                    let self_heal = (final_damage as f64 * self_leech_pct).round().max(0.0) as i64;
                    let healed_hp = (units[attacker_idx].hp + self_heal).min(units[attacker_idx].max_hp as i64);
                    let healed = (healed_hp - units[attacker_idx].hp) as u64;
                    units[attacker_idx].hp = healed_hp;
                    if healed > 0 {
                        let id = units[attacker_idx].id.clone();
                        events.push(CombatEvent::Heal { at_ms, healer: id.clone(), target: id, amount: healed, target_hp_after: healed_hp as u64 });
                    }
                }
                if ex_new_hp == 0 {
                    units[explosion_target].alive = false;
                    events.push(CombatEvent::Defeat { at_ms, unit: ex_target_id });
                    fire_on_kill(units, attacker_idx, at_ms, events, rolls, rng);
                    trigger_doom_on_death(units, explosion_target, at_ms, events, rolls, rng);
                }
            }
        }
    }

    // Life leech (Slayer - see `life_leech_pct`'s doc): a slice of the
    // damage that just actually landed heals the attacker back, capped
    // per trailing 1-second window so a leech build can't out-heal more
    // than LIFE_LEECH_CAP_PER_SEC of its own max hp/sec no matter how
    // much raw damage gets dealt in that window. Open Wound's per-stack
    // bonus (against a target THIS attacker's own hit just found already
    // wounded) blends into the SAME effective fraction before that cap,
    // rather than a second uncapped heal bolted on afterward.
    let wound_leech_bonus =
        if units[target_idx].wound_stacks > 0 && at_ms <= units[target_idx].wound_expires_at_ms {
            units[target_idx].wound_stacks as f64 * units[target_idx].wound_leech_per_stack
        } else {
            0.0
        };
    let effective_leech_pct = units[attacker_idx].life_leech_pct + wound_leech_bonus;
    if effective_leech_pct > 0.0 && final_damage > 0 && units[attacker_idx].alive {
        // Slayer's Endless Thirst - a recent FlickerStrike dash can raise
        // (ranks 1-2) or remove entirely (rank 3) the cap below.
        let (thirst_cap_bonus, thirst_uncapped) = endless_thirst_bonus(&units[attacker_idx], at_ms);
        let cap = units[attacker_idx].max_hp as f64 * (LIFE_LEECH_CAP_PER_SEC + thirst_cap_bonus);
        // Leaky-bucket drain (2026-08-18, wiki audit finding #3) - see
        // `drain_leech_window`'s own doc for why this replaced a
        // lump-sum reset. `leech_window_start_ms` is repurposed as "last
        // drain time" here rather than a window's start - same field, so
        // no struct/constructor changes were needed.
        units[attacker_idx].leech_gained_in_window =
            drain_leech_window(units[attacker_idx].leech_gained_in_window, cap, units[attacker_idx].leech_window_start_ms, at_ms);
        units[attacker_idx].leech_window_start_ms = at_ms;
        let room_left = if thirst_uncapped { f64::MAX } else { (cap - units[attacker_idx].leech_gained_in_window).max(0.0) };
        let raw_leech_potential = final_damage as f64 * effective_leech_pct;
        let leech_amount = raw_leech_potential.min(room_left);
        // Overflow Vessel - whatever leech got capped away becomes a
        // temporary shield instead of being wasted.
        let overflowvessel_pct = units[attacker_idx].overflowvessel_shield_pct;
        if overflowvessel_pct > 0.0 {
            let overcapped = (raw_leech_potential - leech_amount).max(0.0);
            if overcapped > 0.0 {
                grant_shield(units, attacker_idx, attacker_idx, overcapped * overflowvessel_pct, at_ms, OVERFLOW_VESSEL_SHIELD_DURATION_MS, events);
            }
        }
        if leech_amount > 0.0 {
            units[attacker_idx].leech_gained_in_window += leech_amount;
            let healed_hp = (units[attacker_idx].hp + leech_amount.round() as i64).min(units[attacker_idx].max_hp as i64);
            let healed = (healed_hp - units[attacker_idx].hp) as u64;
            units[attacker_idx].hp = healed_hp;
            if healed > 0 {
                let id = units[attacker_idx].id.clone();
                events.push(CombatEvent::Heal { at_ms, healer: id.clone(), target: id, amount: healed, target_hp_after: healed_hp as u64 });
            }
            // Warlock's Dark Communion - a fraction of this SAME leeched
            // amount also goes to the attacker's current lowest-HP ally.
            let communion_pct = units[attacker_idx].dark_communion_pct;
            if communion_pct > 0.0 {
                let mut allies: Vec<usize> = units.iter().enumerate().filter(|(i, u)| !u.is_boss && u.alive && *i != attacker_idx).map(|(i, _)| i).collect();
                allies.sort_by_key(|&i| units[i].hp);
                // Covenant - also applies to the 2nd-lowest-HP ally, at a
                // fraction of the same value.
                let covenant_pct = units[attacker_idx].covenant_pct;
                let unbreakablebond_pct = units[attacker_idx].unbreakablebond_dr_pct;
                for (rank, &ally_idx) in allies.iter().take(2).enumerate() {
                    let value_pct = if rank == 0 { communion_pct } else { communion_pct * covenant_pct };
                    if value_pct <= 0.0 {
                        continue;
                    }
                    let healed = apply_heal(units, attacker_idx, ally_idx, leech_amount * value_pct, at_ms, events, rng);
                    if healed > 0 && unbreakablebond_pct > 0.0 {
                        units[ally_idx].temp_damage_reduction_bonus = unbreakablebond_pct;
                        units[ally_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + UNBREAKABLE_BOND_DR_DURATION_MS;
                    }
                }
            }
        }
    }

    let attacker_id = units[attacker_idx].id.clone();
    let target_id = units[target_idx].id.clone();
    if CURSE_CREDITS_WARLOCK_DAMAGE && curse_share > 0 {
        // Warlock's Curse of Weakness family - split into two events for
        // the SAME real hit (see `curse_share`'s doc above): the
        // attacker's own share first, with an INTERMEDIATE `target_hp_after`
        // (mathematically exact - `new_hp + curse_share` is the hp the
        // target would be at after JUST this portion), then the cursing
        // Warlock's share second, whose `target_hp_after` is the REAL
        // final value. `unmitigated_damage` stays entirely on the first
        // event (0 on the second) - the target only took ONE real hit, so
        // "damage taken" stats shouldn't double-count it across both
        // halves of this split, only "damage dealt" credit is being
        // divided here.
        events.push(CombatEvent::Attack {
            at_ms,
            attacker: attacker_id,
            target: target_id.clone(),
            damage: (final_damage - curse_share).max(0) as u64,
            unmitigated_damage: outcome.unmitigated_damage,
            target_hp_after: (new_hp + curse_share) as u64,
            is_crit: outcome.is_crit,
            evaded: outcome.evaded,
            hit_id,
        });
        events.push(CombatEvent::Attack {
            at_ms,
            attacker: curse_credit_id.clone().unwrap_or_default(),
            target: target_id.clone(),
            damage: curse_share.max(0) as u64,
            unmitigated_damage: 0,
            target_hp_after: new_hp as u64,
            is_crit: false,
            evaded: false,
            hit_id,
        });
    } else {
        events.push(CombatEvent::Attack {
            at_ms,
            attacker: attacker_id,
            target: target_id.clone(),
            damage: final_damage.max(0) as u64,
            unmitigated_damage: outcome.unmitigated_damage,
            target_hp_after: new_hp as u64,
            is_crit: outcome.is_crit,
            evaded: outcome.evaded,
            hit_id,
        });
    }
    // Mage's Arcane Shield - a crit grants the ATTACKER (not the target)
    // a shield worth a fraction of their own max HP. Checked after the
    // Attack event so a crit that also kills its target still banks the
    // shield first.
    let crit_shield_pct = units[attacker_idx].crit_shield_max_hp_pct;
    if outcome.is_crit && crit_shield_pct > 0.0 && units[attacker_idx].alive {
        let shield_amount = units[attacker_idx].max_hp as f64 * crit_shield_pct;
        grant_shield(units, attacker_idx, attacker_idx, shield_amount, at_ms, ARCANE_SHIELD_DURATION_MS, events);
    }
    if new_hp == 0 {
        units[target_idx].alive = false;
        events.push(CombatEvent::Defeat { at_ms, unit: target_id });
        fire_on_kill(units, attacker_idx, at_ms, events, rolls, rng);
        trigger_doom_on_death(units, target_idx, at_ms, events, rolls, rng);
        // Druid's Wild Roar + Nature's Embrace (2026-08-16 rework) - both
        // trigger off ANY party member's death, checked against every
        // OTHER alive party member (not just the killer) since either
        // could be invested by a Druid who wasn't even involved in this
        // particular hit. Deliberately iterates ALL qualifying Druids (not
        // just the first found) - unlike Guardian Spirit's single shared
        // charge pool, each Druid banks their own independent charges.
        if !units[target_idx].is_boss {
            let druid_indices: Vec<usize> = units.iter().enumerate().filter(|(i, u)| *i != target_idx && !u.is_boss && u.alive).map(|(i, _)| i).collect();
            for &druid_idx in &druid_indices {
                if units[druid_idx].wildroar_charges > 0 {
                    units[druid_idx].wildroar_charges -= 1;
                    events.push(CombatEvent::SkillCast { at_ms, unit: units[druid_idx].id.clone(), skill: "Wild Roar".to_string() });
                    for enemy_idx in units.iter().enumerate().filter(|(_, u)| u.is_boss && u.alive).map(|(i, _)| i).collect::<Vec<usize>>() {
                        units[enemy_idx].next_action_at_ms = units[enemy_idx].next_action_at_ms.max(at_ms + WILDROAR_FEAR_DURATION_MS);
                    }
                }
                let embrace_targets = units[druid_idx].naturesembrace_heal_targets;
                if embrace_targets > 0 {
                    let mut candidates: Vec<usize> =
                        units.iter().enumerate().filter(|(i, u)| *i != target_idx && !u.is_boss && u.alive && u.hp < u.max_hp as i64).map(|(i, _)| i).collect();
                    candidates.sort_by_key(|&i| units[i].hp);
                    events.push(CombatEvent::SkillCast { at_ms, unit: units[druid_idx].id.clone(), skill: "Nature's Embrace".to_string() });
                    for &heal_idx in candidates.iter().take(embrace_targets as usize) {
                        let full_heal_amount = units[heal_idx].max_hp as f64;
                        apply_heal(units, druid_idx, heal_idx, full_heal_amount, at_ms, events, rng);
                    }
                }
            }
        }
        // Contagion - a wound has a chance to jump to a new target when
        // its host dies (approximated off the KILLING attacker's own
        // investment, since they're the one whose wound-dealing kit this
        // almost always is).
        let contagion_chance = units[attacker_idx].contagion_chance;
        if contagion_chance > 0.0 && units[target_idx].wound_stacks > 0 && at_ms <= units[target_idx].wound_expires_at_ms && rng.gen_bool(contagion_chance.clamp(0.0, 1.0)) {
            let target_is_boss = units[target_idx].is_boss;
            let new_host = units.iter().enumerate().filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive).map(|(i, _)| i).min_by_key(|&i| units[i].hp);
            if let Some(new_host_idx) = new_host {
                units[new_host_idx].wound_stacks = units[target_idx].wound_stacks;
                units[new_host_idx].wound_max_stacks = units[target_idx].wound_max_stacks;
                units[new_host_idx].wound_expires_at_ms = units[target_idx].wound_expires_at_ms;
                units[new_host_idx].wound_leech_per_stack = units[target_idx].wound_leech_per_stack;
                units[new_host_idx].wound_damage_dealt_debuff = units[target_idx].wound_damage_dealt_debuff;
                units[new_host_idx].wound_heal_received_debuff = units[target_idx].wound_heal_received_debuff;
                units[new_host_idx].wound_damage_taken_total = 0.0;
            }
        }
        // Warlock's Soul Harvest - a kill heals the attacker for a
        // fraction of their own max HP, on top of Soul Siphon's per-hit
        // leech. Eternal Hunger then guarantees a shield off whatever
        // actually got restored (0 if the attacker was already full).
        let soul_harvest_pct = units[attacker_idx].soul_harvest_heal_pct;
        if soul_harvest_pct > 0.0 && units[attacker_idx].alive {
            let heal_amount = units[attacker_idx].max_hp as f64 * soul_harvest_pct;
            let healed = apply_heal(units, attacker_idx, attacker_idx, heal_amount, at_ms, events, rng);
            let shield_pct = units[attacker_idx].eternal_hunger_shield_pct;
            if healed > 0 && shield_pct > 0.0 {
                let shield_amount = healed as f64 * shield_pct;
                grant_shield(units, attacker_idx, attacker_idx, shield_amount, at_ms, ETERNAL_HUNGER_SHIELD_DURATION_MS, events);
            }
        }
        // Dark Ritual - a kill also grants a temporary self increased-
        // damage buff.
        let darkritual_pct = units[attacker_idx].darkritual_dmg_pct;
        if darkritual_pct > 0.0 && units[attacker_idx].alive {
            units[attacker_idx].temp_party_increased_damage_bonus = darkritual_pct;
            units[attacker_idx].temp_party_increased_damage_bonus_expires_at_ms = at_ms + DARK_RITUAL_DURATION_MS;
        }
        // Warlock's Fel Rush - a flat attack-speed buff, (re)applied on
        // every kill (see `fel_rush_speed_bonus`'s doc for why it isn't
        // just another `stack_speed_*` investor).
        if units[attacker_idx].fel_rush_speed_bonus > 0.0 && units[attacker_idx].alive {
            let already_active = at_ms <= units[attacker_idx].fel_rush_expires_at_ms;
            units[attacker_idx].fel_rush_expires_at_ms = at_ms + units[attacker_idx].fel_rush_duration_ms;
            // Ravage (rank 3) - a kill while Fel Rush is ALREADY active
            // banks another stack (capped at 3).
            if already_active && units[attacker_idx].ravage_stack_pct > 0.0 {
                units[attacker_idx].fel_rush_stacks = (units[attacker_idx].fel_rush_stacks + 1).min(3);
            }
        }
        // Ranger's Hunter's Reward - self-heal on a kill (approximated as
        // any kill while Hunter's Mark is invested, not narrowly gated to
        // a real Kill Zone proc, since there's no cheap way to know from
        // here whether THIS specific kill was a Kill Zone-boosted hit).
        let huntersreward_pct = units[attacker_idx].huntersreward_heal_pct;
        if huntersreward_pct > 0.0 && units[attacker_idx].alive {
            let heal_amount = units[attacker_idx].max_hp as f64 * huntersreward_pct;
            apply_heal(units, attacker_idx, attacker_idx, heal_amount, at_ms, events, rng);
        }
        // Clean Kill - a chance to immediately re-mark a new target for
        // free, bypassing the normal one-shot-per-fight gate.
        let cleankill_chance = units[attacker_idx].cleankill_remark_chance;
        if cleankill_chance > 0.0 && rng.gen_bool(cleankill_chance.clamp(0.0, 1.0)) {
            units[attacker_idx].has_applied_mark_this_fight = false;
        }
        // Berserker's Vigor - a kill heals THIS unit for a fraction of
        // their own max HP (Reckless Swing's own trade is folded directly
        // into the base stats at construction - see that site's doc).
        let vigor_pct = units[attacker_idx].vigor_heal_pct;
        if vigor_pct > 0.0 && units[attacker_idx].alive {
            let heal_amount = units[attacker_idx].max_hp as f64 * vigor_pct;
            apply_heal(units, attacker_idx, attacker_idx, heal_amount, at_ms, events, rng);
            // Vengeful Blood - the same heal also grants a shield.
            let vengefulblood_pct = units[attacker_idx].vengefulblood_shield_pct;
            if vengefulblood_pct > 0.0 {
                grant_shield(units, attacker_idx, attacker_idx, heal_amount * vengefulblood_pct, at_ms, VENGEFUL_BLOOD_SHIELD_DURATION_MS, events);
            }
        }
        // Second Gale - a kill while Reckless Swing is active grants
        // temporary immunity to its own extra-damage-taken penalty.
        let secondgale_duration = units[attacker_idx].secondgale_duration_ms;
        if secondgale_duration > 0 && units[attacker_idx].alive {
            units[attacker_idx].temp_reckless_immunity_expires_at_ms = at_ms + secondgale_duration;
        }
    }
    // Warrior's Retaliation - a hit THIS unit (the TARGET, not the
    // attacker) just took and SURVIVED has a chance to trigger an
    // immediate counter-attack back at whoever hit them. `!is_followup`
    // keeps the counter-attack itself (and Twin Strikes' own follow-up)
    // from chaining into another Retaliation check - same guard,
    // deliberately reused rather than adding a second flag for the exact
    // same "this is a derived hit, don't re-trigger reactive procs off
    // it" purpose. Player-only (a boss taking a hit never retaliates via
    // this - that's what a boss's own kit already is).
    if !outcome.evaded && counts_as_primary_hit && !units[target_idx].is_boss && units[target_idx].alive && units[attacker_idx].alive {
        let base_chance = units[target_idx].retaliation_chance;
        if base_chance > 0.0 {
            // Last Stand - live below-25%-HP bonus to the trigger chance.
            let hp_pct = if units[target_idx].max_hp > 0 { units[target_idx].hp as f64 / units[target_idx].max_hp as f64 } else { 1.0 };
            let laststand_bonus = if hp_pct < 0.25 { units[target_idx].retaliation_laststand_bonus } else { 0.0 };
            let mut trigger_chance = base_chance + laststand_bonus;
            // Second Wind (Warrior) - doubles the total trigger chance
            // below its own threshold.
            let secondwind_threshold = units[target_idx].retaliation_secondwind_threshold;
            if secondwind_threshold > 0.0 && hp_pct < secondwind_threshold {
                trigger_chance *= 2.0;
            }
            if rng.gen_bool(trigger_chance.clamp(0.0, 1.0)) {
                // Grudge - bonus damage per prior hit from this SAME
                // attacker this fight (capped at 5 stacks).
                let grudge_pct = units[target_idx].grudge_pct_per_hit;
                let grudge_bonus = if grudge_pct > 0.0 {
                    let attacker_id = units[attacker_idx].id.clone();
                    let stacks = units[target_idx].grudge_hit_counts.iter().find(|(id, _)| *id == attacker_id).map(|(_, c)| (*c).min(5)).unwrap_or(0);
                    grudge_pct * stacks as f64
                } else {
                    0.0
                };
                // Executioner's Mark - a temporary crit-chance override for
                // just this one counter-attack call (one-off scoped stat
                // override, same convention as Piercing Shots/Sanctified
                // Touch).
                let mark_bonus = units[target_idx].retaliation_crit_bonus;
                if mark_bonus > 0.0 {
                    units[target_idx].crit_chance += mark_bonus;
                }
                // Payback - the counter always crits against a low-HP
                // attacker.
                let payback_threshold = units[target_idx].retaliation_payback_threshold;
                if payback_threshold > 0.0 && units[attacker_idx].max_hp > 0 {
                    let attacker_hp_pct = units[attacker_idx].hp as f64 / units[attacker_idx].max_hp as f64;
                    if attacker_hp_pct < payback_threshold {
                        units[target_idx].force_crit_next_hit = true;
                    }
                }
                // Vengeance - bonus damage on the counter itself.
                let counter_base = attacker_base_damage(&units[target_idx], rng) * (1.0 + units[target_idx].retaliation_dmg_pct + grudge_bonus);
                let attacker_hp_before = units[attacker_idx].hp;
                apply_hit(units, target_idx, attacker_idx, counter_base, at_ms, events, rolls, rng, false, true);
                if mark_bonus > 0.0 {
                    units[target_idx].crit_chance -= mark_bonus;
                }
                // Bloodied Resolve - self-heal off the counter's own
                // ACTUAL damage dealt (post-mitigation), same "leech off
                // real landed damage" convention as life leech.
                let heal_pct = units[target_idx].retaliation_heal_pct;
                if heal_pct > 0.0 && units[target_idx].alive {
                    let dealt = (attacker_hp_before - units[attacker_idx].hp).max(0) as f64;
                    if dealt > 0.0 {
                        apply_heal(units, target_idx, target_idx, dealt * heal_pct, at_ms, events, rng);
                    }
                }
                if units[target_idx].alive {
                    // Adrenaline Surge - a temporary attack-speed buff off
                    // the SAME shared field War Cry/Rally already use.
                    let surge_pct = units[target_idx].retaliation_surge_pct;
                    if surge_pct > 0.0 {
                        units[target_idx].temp_party_attack_speed_bonus = surge_pct;
                        units[target_idx].temp_party_attack_speed_bonus_expires_at_ms = at_ms + ADRENALINE_SURGE_DURATION_MS;
                    }
                    // Hardened - a persistent, never-decaying stacking DR
                    // bonus, one stack per successful Retaliation.
                    if units[target_idx].hardened_pct_per_stack > 0.0 {
                        units[target_idx].hardened_stacks = (units[target_idx].hardened_stacks + 1).min(5);
                    }
                }
            }
        }
    }
}

/// Berserker's Frenzy (redefined 2026-08-15) - rolls `frenzy_strike_chance`
/// (doubled by Blood Scent if the target's HP% qualifies); on success,
/// strikes `frenzy_extra_hits` MORE times against the SAME `target_idx`
/// (a combo, not a fresh-target spread - see the design conversation this
/// was decided in). Each extra strike, in order:
/// 1. Culling Strike - if the target is already at or below
///    `frenzy_culling_threshold`, this strike outright kills them
///    (bypassing normal damage/mitigation) instead of rolling at all.
/// 2. Overkill - temporarily shreds the target's `damage_reduction` for
///    just this one `apply_hit` call, restored immediately after.
/// 3. Berserking/Onslaught's damage bonus, applied to this strike's own
///    base (not the triggering hit's).
/// 4. Bloodletting/Vitality Surge - heals the attacker off however much
///    THIS strike actually landed, which Second Wind can then also
///    shield a fraction of.
/// Chain Frenzy can fire this whole function again on a success, capped
/// at `frenzy_chain_max_extra` extra chains via `chain_depth` - bounded
/// at that node's own max rank (3), so this can never recurse unboundedly
/// even at 100% chain chance.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fire_frenzy(
    units: &mut [CombatSimUnit],
    attacker_idx: usize,
    target_idx: usize,
    base_damage: f64,
    at_ms: u32,
    events: &mut Vec<CombatEvent>,
    rolls: &mut Vec<RollEvent>,
    rng: &mut impl Rng,
    chain_depth: u32,
) {
    let chance = units[attacker_idx].frenzy_strike_chance;
    if chance <= 0.0 || !units[target_idx].alive || !units[attacker_idx].alive {
        return;
    }
    let bloodscent_threshold = units[attacker_idx].frenzy_bloodscent_threshold;
    let effective_chance = if bloodscent_threshold > 0.0 && units[target_idx].max_hp > 0 {
        let target_hp_pct = units[target_idx].hp as f64 / units[target_idx].max_hp as f64;
        if target_hp_pct <= bloodscent_threshold { (chance * 2.0).min(1.0) } else { chance }
    } else {
        chance
    };
    if !rng.gen_bool(effective_chance.clamp(0.0, 1.0)) {
        return;
    }
    let extra_hits = units[attacker_idx].frenzy_extra_hits;
    let strike_damage = base_damage * (1.0 + units[attacker_idx].frenzy_extra_dmg_pct);
    let culling_threshold = units[attacker_idx].frenzy_culling_threshold;
    let dr_shred = units[attacker_idx].frenzy_dr_shred_pct;
    let heal_pct = units[attacker_idx].frenzy_heal_pct;
    let shield_chance = units[attacker_idx].frenzy_shield_chance;
    for _ in 0..extra_hits {
        if !units[target_idx].alive || !units[attacker_idx].alive {
            break;
        }
        // Culling Strike - a genuine execute, not overkill damage. Skips
        // the normal hit pipeline entirely (no crit/evasion/mitigation
        // rolled at all) since the point is a GUARANTEED kill below the
        // threshold, not a bigger chance at one.
        if culling_threshold > 0.0 && units[target_idx].max_hp > 0 {
            let target_hp_pct = units[target_idx].hp as f64 / units[target_idx].max_hp as f64;
            if target_hp_pct <= culling_threshold {
                units[target_idx].alive = false;
                units[attacker_idx].damage_dealt_total += units[target_idx].hp.max(0) as u64;
                let attacker_id = units[attacker_idx].id.clone();
                let target_id = units[target_idx].id.clone();
                events.push(CombatEvent::Attack {
                    at_ms,
                    attacker: attacker_id,
                    target: target_id.clone(),
                    damage: units[target_idx].hp.max(0) as u64,
                    unmitigated_damage: units[target_idx].hp.max(0) as u64,
                    target_hp_after: 0,
                    is_crit: false,
                    evaded: false,
                    hit_id: next_hit_id(),
                });
                units[target_idx].hp = 0;
                events.push(CombatEvent::Defeat { at_ms, unit: target_id });
                fire_on_kill(units, attacker_idx, at_ms, events, rolls, rng);
                trigger_doom_on_death(units, target_idx, at_ms, events, rolls, rng);
                break;
            }
        }
        // Overkill - temporary DR shred, restored right after this one
        // `apply_hit` call (same one-off-override convention as the
        // crit_chance overrides elsewhere in this file).
        let original_dr = units[target_idx].damage_reduction;
        if dr_shred > 0.0 {
            units[target_idx].damage_reduction -= dr_shred;
        }
        let hp_before = units[target_idx].hp;
        apply_hit(units, attacker_idx, target_idx, strike_damage, at_ms, events, rolls, rng, true, false);
        if dr_shred > 0.0 {
            units[target_idx].damage_reduction = original_dr;
        }
        // Bloodletting - self-heal off the ACTUAL damage this strike
        // dealt (post-mitigation), same "leech is off real landed
        // damage" convention as life leech.
        if heal_pct > 0.0 && units[attacker_idx].alive {
            let actual_dealt = (hp_before - units[target_idx].hp).max(0) as f64;
            if actual_dealt > 0.0 {
                let heal_amount = actual_dealt * heal_pct;
                let healed = apply_heal(units, attacker_idx, attacker_idx, heal_amount, at_ms, events, rng);
                if healed > 0 && shield_chance > 0.0 && rng.gen_bool(shield_chance.clamp(0.0, 1.0)) {
                    let shield_amount = healed as f64 * FRENZY_SHIELD_VALUE_PCT;
                    grant_shield(units, attacker_idx, attacker_idx, shield_amount, at_ms, FRENZY_SHIELD_DURATION_MS, events);
                }
            }
        }
    }
    // Chain Frenzy - see this function's own doc for the recursion-safety
    // argument.
    let chain_chance = units[attacker_idx].frenzy_chain_chance;
    let max_chain = units[attacker_idx].frenzy_chain_max_extra;
    if chain_chance > 0.0 && chain_depth < max_chain && units[attacker_idx].alive && units[target_idx].alive && rng.gen_bool(chain_chance.clamp(0.0, 1.0)) {
        fire_frenzy(units, attacker_idx, target_idx, base_damage, at_ms, events, rolls, rng, chain_depth + 1);
    }
}

/// How many extra splash targets 100%+ of overflow splash buys, on top
/// of the normal max-targets cap - see `apply_splash`/`apply_heal_splash`.
pub(crate) const SPLASH_OVERFLOW_BONUS_TARGETS: usize = 2;

/// After a normal attack's primary hit resolves, splashes a fraction of
/// that SAME base roll (not the primary hit's actual post-mitigation
/// result - each splash target rolls its own crit/evasion/block/
/// reduction fresh via `apply_hit`, same as the primary did) onto up to
/// `max_targets` other currently-alive units on the primary target's
/// side, chosen at random from the above-median-level pool first when
/// splashing onto players (`median_level: Some(...)` - see
/// `prioritize_above_median`'s doc; always `None` when players splash
/// onto enemies, since that side has no "level" to prioritize by) -
/// plus `SPLASH_OVERFLOW_BONUS_TARGETS` more if `splash_fraction` is
/// over 1.0 (see `Character::combat_splash`'s doc). A no-op whenever
/// `splash_fraction <= 0` (basic-encounter mobs always have splash 0.0,
/// so this is naturally inert for them without any special-casing).
pub(crate) fn apply_splash(
    units: &mut [CombatSimUnit],
    attacker_idx: usize,
    primary_target_idx: usize,
    primary_base_damage: f64,
    splash_fraction: f64,
    max_targets: usize,
    median_level: Option<f64>,
    at_ms: u32,
    events: &mut Vec<CombatEvent>,
    rolls: &mut Vec<RollEvent>,
    rng: &mut impl Rng,
) {
    if splash_fraction <= 0.0 || max_targets == 0 {
        return;
    }
    // Reentrancy guard (2026-08-16, a crash fix - see `in_splash_resolution`'s
    // own doc for the full bug) - saved/restored rather than blindly reset
    // to `false` at the end, so this stays correct even if some future
    // caller ever nests one attacker's splash inside another's.
    let was_in_splash_resolution = units[attacker_idx].in_splash_resolution;
    units[attacker_idx].in_splash_resolution = true;
    let max_targets = if splash_fraction > 1.0 { max_targets + SPLASH_OVERFLOW_BONUS_TARGETS } else { max_targets };
    let target_side_is_boss = units[primary_target_idx].is_boss;
    let all_candidates: Vec<usize> =
        units.iter().enumerate().filter(|(i, u)| *i != primary_target_idx && u.is_boss == target_side_is_boss && u.alive).map(|(i, _)| i).collect();
    let mut candidates = match median_level {
        Some(median) if !target_side_is_boss => prioritize_above_median(&all_candidates, units, median),
        _ => all_candidates,
    };
    let splash_damage = primary_base_damage * splash_fraction.min(1.0);
    // Ranger's Piercing Shots (rank 3 only - rank 1/2's "splash can crit
    // independently" clause is already true unconditionally, since every
    // `apply_hit` call - splash included - already rolls its own
    // independent crit via `roll_attacker_damage`; nothing in this sim
    // ever stripped that out, so those ranks are effectively banked
    // toward rank 3's real bonus, same "an early rank can be a no-op"
    // precedent as Assassinate's own rank-gating) - a temporary override
    // on the ATTACKER's crit_chance for just these splash rolls, same
    // one-off-override convention Sanctified Touch's heal-crit bonus
    // already uses.
    // Wind Pierce - extends the same one-off crit-chance override
    // Piercing Shots' own rank-3 bonus already uses for splash rolls.
    let piercing_bonus = units[attacker_idx].piercing_shots_crit_chance_bonus + units[attacker_idx].windpierce_splash_crit_pct;
    let original_crit_chance = units[attacker_idx].crit_chance;
    if piercing_bonus > 0.0 {
        units[attacker_idx].crit_chance += piercing_bonus;
    }
    let pick_count = max_targets.min(candidates.len());
    for _ in 0..pick_count {
        let pick_at = rng.gen_range(0..candidates.len());
        let target_idx = candidates.remove(pick_at);
        // Festering Wound gates whether a splash hit spreads the wound
        // too - see `apply_hit`'s `applies_wound` doc. Every other
        // splash consequence (damage, leech, shield absorption) still
        // applies regardless.
        apply_hit(units, attacker_idx, target_idx, splash_damage, at_ms, events, rolls, rng, units[attacker_idx].wound_deal_spreads_to_splash, false);
        // Mage's Frost Nova - a temporary evasion debuff on whoever this
        // splash hits (only ever nonzero for a real player attacker, so
        // this never fires off a boss's own splash onto players).
        let mut frostnova_pct = units[attacker_idx].frostnova_evasion_debuff_pct;
        if frostnova_pct > 0.0 && units[target_idx].alive {
            // Absolute Zero - doubles the debuff against a low-HP target.
            let absolutezero_threshold = units[attacker_idx].absolutezero_threshold;
            if absolutezero_threshold > 0.0 && units[target_idx].max_hp > 0 && (units[target_idx].hp as f64 / units[target_idx].max_hp as f64) < absolutezero_threshold {
                frostnova_pct *= 2.0;
            }
            units[target_idx].temp_evasion_debuff = frostnova_pct;
            // Permafrost - extends the debuff's own duration.
            units[target_idx].temp_evasion_debuff_expires_at_ms = at_ms + units[attacker_idx].frostnova_duration_ms;
        }
        // Static Field - Chain Lightning's splash also slows the target.
        let staticfield_pct = units[attacker_idx].staticfield_speed_debuff_pct;
        if staticfield_pct > 0.0 && units[target_idx].alive {
            units[target_idx].temp_attack_speed_debuff = staticfield_pct;
            units[target_idx].temp_attack_speed_debuff_expires_at_ms = at_ms + STATIC_FIELD_DEBUFF_DURATION_MS;
        }
        // Infernal Pact - Wildfire's splash also heals the caster per
        // enemy hit.
        let infernalpact_pct = units[attacker_idx].infernalpact_heal_pct;
        if infernalpact_pct > 0.0 && units[attacker_idx].alive {
            let heal_amount = units[attacker_idx].max_hp as f64 * infernalpact_pct;
            apply_heal(units, attacker_idx, attacker_idx, heal_amount, at_ms, events, rng);
        }
        // Armor Breaker - Piercing Shots' splash crit also shreds the
        // target's DR (reused `temp_damage_reduction_bonus` field, a
        // negative value here instead of Serenity's own positive buff use
        // - low collision risk since they're granted to different units
        // in practice).
        let armorbreaker_pct = units[attacker_idx].armorbreaker_dr_shred_pct;
        if armorbreaker_pct > 0.0 && units[target_idx].alive {
            units[target_idx].temp_damage_reduction_bonus = -armorbreaker_pct;
            units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + ARMORBREAKER_DEBUFF_DURATION_MS;
        }
        // Scorched Earth - Explosive Tips' splash also reduces the
        // target's own damage dealt for a few seconds.
        let scorchedearth_pct = units[attacker_idx].scorchedearth_dmg_debuff_pct;
        if scorchedearth_pct > 0.0 && units[target_idx].alive {
            units[target_idx].temp_damage_dealt_debuff = scorchedearth_pct;
            units[target_idx].temp_damage_dealt_debuff_expires_at_ms = at_ms + SCORCHED_EARTH_DEBUFF_DURATION_MS;
        }
    }
    if piercing_bonus > 0.0 {
        units[attacker_idx].crit_chance = original_crit_chance;
    }
    units[attacker_idx].in_splash_resolution = was_in_splash_resolution;
}

/// Applies one resolved heal AMOUNT (already crit/heal-power-resolved by
/// the caller - this function doesn't roll anything itself) from
/// `units[healer_idx]` onto `units[target_idx]` - the one shared place
/// that actually writes hp/pushes `CombatEvent::Heal` for a genuine heal
/// ACTION (the unified attack's heal share, `apply_heal_splash`'s extra
/// targets, `apply_heal_bounce`'s bounce targets, and Boots' self-heal -
/// NOT leech/refund side-effects like Slayer's life leech or Bloodpact's
/// HP refund, which stay separate inline since they're damage-derived
/// rather than a cast heal). Mirrors how `apply_hit` is the single shared
/// entry point for a damage instance. Caps at the target's max hp, then
/// feeds any overheal into Cleric's Overflowing Grace shield (see
/// `overflow_grace_shield_pct`'s doc) instead of wasting it. Returns how
/// much hp was actually restored (0 if the target was already full or
/// `amount` rounds to 0 or less).
pub(crate) fn apply_heal(units: &mut [CombatSimUnit], healer_idx: usize, target_idx: usize, amount: f64, at_ms: u32, events: &mut Vec<CombatEvent>, rng: &mut impl Rng) -> u64 {
    // Cleric's Eternal Light (2026-08-17, replacing its old "first heal
    // only, once per fight" premise - Radiant Light is a PERMANENT stat
    // stack, not a temporary buff, so there was nothing to "persist";
    // rewritten to refresh on EVERY heal instead, giving it real identity
    // as a keep-casting uptime bonus rather than a strictly-smaller copy
    // of its sibling Luminous). Doesn't affect THIS heal itself - reuses
    // the already-real `temp_heal_power_bonus` field, only read separately
    // at the unified action's own heal-share computation.
    let eternallight_bonus = units[healer_idx].eternallight_bonus_pct;
    if eternallight_bonus > 0.0 {
        units[healer_idx].temp_heal_power_bonus = eternallight_bonus;
        units[healer_idx].temp_heal_power_bonus_expires_at_ms = at_ms + ETERNAL_LIGHT_DURATION_MS;
    }
    // Divine damage rework (2026-08-15) - the TARGET's own active
    // healing-received debuff (from being hit by Divine damage) shrinks
    // this heal; the HEALER's own active healing-power buff (from Divine
    // procs off their OWN past heals) grows it - both read (and lazily
    // pruned) BEFORE rounding/capping, so every downstream use of
    // `amount` (Lingering Effect, the overflow-shield calc below) already
    // reflects the adjusted size, same as everything else here already
    // treats `amount` as the "true" heal.
    let heal_reduction = (prune_and_count(&mut units[target_idx].divine_heal_reduction, at_ms) as f64 * 0.01).min(1.0);
    let heal_power_buff = prune_and_count(&mut units[healer_idx].divine_heal_power_buff, at_ms) as f64 * 0.01;
    // Slayer's Rot/Withering Touch - a wounded target's healing received
    // is further reduced, same lazy-expiry gate `wound_heal_received_debuff`
    // itself already uses everywhere else. Stays inert in every fight
    // today since nothing currently heals a boss/enemy unit (confirmed via
    // audit of every `apply_heal` call site) - real and correct, just
    // waiting on a future pass that gives any boss/add real self-healing.
    let wound_heal_reduction = if units[target_idx].wound_stacks > 0 && at_ms <= units[target_idx].wound_expires_at_ms {
        units[target_idx].wound_heal_received_debuff.clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Warrior's Endless Reserves - increases healing received specifically
    // FROM ALLIES (a self-heal doesn't count as "received from allies").
    let reserves_bonus = if healer_idx != target_idx { units[target_idx].reserves_heal_received_pct } else { 0.0 };
    // Paladin's Purging Flame - a temporary healing-received debuff from a
    // recent Holy Fire hit.
    let purgingflame_reduction =
        if units[target_idx].temp_heal_reduction_pct > 0.0 && at_ms <= units[target_idx].temp_heal_reduction_expires_at_ms { units[target_idx].temp_heal_reduction_pct } else { 0.0 };
    // Warlock's Withering Curse - a live healing-received debuff.
    let curse_heal_reduction = units[target_idx].curse_heal_reduction_bonus.clamp(0.0, 1.0);
    // Cthulhu's stacking debuff (see `cthulhu_debuff_stacks`) - a
    // HEALER-side "healing dealt" nerf, same magnitude/floor as its
    // damage-dealt counterpart in `resolve_hit`, just read off
    // `healer_idx` here instead of an attacker.
    let cthulhu_heal_dealt_reduction = if units[healer_idx].cthulhu_debuff_stacks > 0 && at_ms <= units[healer_idx].cthulhu_debuff_expires_at_ms {
        (units[healer_idx].cthulhu_debuff_stacks as f64 * units[healer_idx].cthulhu_debuff_pct_per_stack).min(CTHULHU_DEBUFF_CAP)
    } else {
        0.0
    };
    let amount = amount
        * (1.0 + heal_power_buff)
        * (1.0 - heal_reduction)
        * (1.0 - wound_heal_reduction)
        * (1.0 + reserves_bonus)
        * (1.0 - purgingflame_reduction)
        * (1.0 - curse_heal_reduction)
        * (1.0 - cthulhu_heal_dealt_reduction);
    let amount = amount.round().max(0.0) as i64;
    if amount <= 0 {
        return 0;
    }
    let hp_before = units[target_idx].hp;
    let room = (units[target_idx].max_hp as i64 - units[target_idx].hp).max(0);
    let applied = amount.min(room);
    units[target_idx].hp += applied;
    let healed = applied as u64;
    if healed > 0 {
        let healer_id = units[healer_idx].id.clone();
        let target_id = units[target_idx].id.clone();
        events.push(CombatEvent::Heal { at_ms, healer: healer_id, target: target_id, amount: healed, target_hp_after: units[target_idx].hp as u64 });
    }
    // Berserker's Death Defiant (2026-08-17) - if this heal moved the
    // target to a LOWER missing-HP 20%-bucket (the same bucket math
    // Gambit's own live crit bonus already uses), freeze whatever Gambit's
    // bonus was at the OLD (higher) bucket for a grace window, so healing
    // back up doesn't instantly strip an active Gambit bonus. Gated on
    // the TARGET's own Gambit/Death Defiant investment, not the healer's -
    // this is about the Berserker being healed, self or by an ally.
    if healed > 0 && units[target_idx].max_hp > 0 && units[target_idx].deathdefiant_grace_ms > 0 && units[target_idx].gambit_crit_per_missing_20pct > 0.0 {
        let max_hp = units[target_idx].max_hp as f64;
        let old_missing_frac = (1.0 - hp_before as f64 / max_hp).max(0.0);
        let new_missing_frac = (1.0 - units[target_idx].hp as f64 / max_hp).max(0.0);
        let old_bucket = (old_missing_frac / 0.20).floor();
        let new_bucket = (new_missing_frac / 0.20).floor();
        if new_bucket < old_bucket {
            let mut frozen = old_bucket * units[target_idx].gambit_crit_per_missing_20pct;
            // Last Laugh (rank 2) - same flat +15pp bump the live
            // computation uses (see `roll_attacker_damage`'s own
            // `gambit_bonus` doc, 2026-08-17 rework), so a frozen snapshot
            // matches what the live bonus actually was at the old bucket.
            if units[target_idx].lastlaugh_crit_bonus && hp_before as f64 / max_hp < 0.25 {
                frozen += 0.15;
            }
            units[target_idx].deathdefiant_frozen_crit_bonus = frozen;
            units[target_idx].deathdefiant_frozen_crit_bonus_expires_at_ms = at_ms + units[target_idx].deathdefiant_grace_ms;
        }
    }
    // Wild Heart (Druid only, 2026-08-16 rework) - a slice of any heal
    // landed on someone ELSE also heals the Druid themselves. Recurses
    // into `apply_heal` with healer==target so the self-heal gets the same
    // overheal/Lingering Effect/etc. treatment a normal self-heal would -
    // the `healer_idx != target_idx` guard below is what stops that
    // recursive call from re-triggering Wild Heart on itself.
    if healer_idx != target_idx && healed > 0 {
        let wildheart_pct = units[healer_idx].wildheart_self_heal_pct;
        if wildheart_pct > 0.0 {
            let self_heal_amount = healed as f64 * wildheart_pct;
            apply_heal(units, healer_idx, healer_idx, self_heal_amount, at_ms, events, rng);
        }
    }
    // Wild Instinct (Druid only, 2026-08-16 rework) - any landed heal also
    // grants ITS target a temporary damage-reduction buff, any target
    // including the healer's own self-heals.
    if healed > 0 {
        let wildinstinct_pct = units[healer_idx].wildinstinct_dr_pct;
        if wildinstinct_pct > 0.0 {
            units[target_idx].temp_damage_reduction_bonus = wildinstinct_pct;
            units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + WILDINSTINCT_DR_DURATION_MS;
        }
    }
    // Lingering Effect - symmetric with the damage side (see
    // `apply_lingering_effect`'s doc): a heal-over-time on whoever just
    // got healed, off the healer's own Lingering Effect investment and
    // this heal's own pre-cap `amount` (mirrors `apply_hit`'s use of
    // pre-mitigation `unmitigated_damage`, not the post-mitigation
    // `damage` that actually lands).
    apply_lingering_effect(units, healer_idx, target_idx, amount as f64, true, at_ms);
    let overflow = amount - applied;
    if overflow > 0 && units[healer_idx].overflow_grace_shield_pct > 0.0 {
        let shield_amount = overflow as f64 * units[healer_idx].overflow_grace_shield_pct;
        let duration_ms = units[healer_idx].overflow_grace_shield_duration_ms;
        grant_shield(units, healer_idx, target_idx, shield_amount, at_ms, duration_ms, events);
    }
    // Elemental damage rework - Fire/Cold/Chaos buff the HEALED ally;
    // Divine buffs the HEALER'S OWN future healing instead (see
    // `Affix::ColdDamage`'s doc for why "a healer with one of these
    // modifiers" means whichever action this actually was, not an
    // archetype gate). Only rolled off a heal that actually did
    // something (healed > 0), same "landed" gate `apply_hit`'s own
    // elemental procs use.
    if healed > 0 {
        let fire_pct = units[healer_idx].fire_damage_pct;
        roll_elemental_proc(fire_pct, &mut units[target_idx].fire_dr_buff, usize::MAX, at_ms, rng);
        let cold_pct = units[healer_idx].cold_damage_pct;
        let cold_count_before = prune_and_count(&mut units[target_idx].cold_evasion_buff, at_ms);
        if roll_elemental_proc(cold_pct, &mut units[target_idx].cold_evasion_buff, usize::MAX, at_ms, rng).0 {
            let base_evasion = units[target_idx].evasion;
            let rate = units[target_idx].evasion_overflow_dmg_rate;
            convert_elemental_overflow(&mut units[target_idx], base_evasion, cold_count_before, rate, at_ms);
        }
        let chaos_pct = units[healer_idx].chaos_damage_pct;
        let chaos_count_before = prune_and_count(&mut units[target_idx].chaos_block_buff, at_ms);
        if roll_elemental_proc(chaos_pct, &mut units[target_idx].chaos_block_buff, usize::MAX, at_ms, rng).0 {
            let base_block = units[target_idx].block_chance;
            let rate = units[target_idx].block_overflow_dmg_rate;
            convert_elemental_overflow(&mut units[target_idx], base_block, chaos_count_before, rate, at_ms);
        }
        // 2026-08-16 follow-up: divine healing's self-buff no longer
        // shares the other elements' factored-down proc formula - see
        // `roll_divine_heal_power_proc`'s own doc.
        let divine_pct = units[healer_idx].divine_damage_pct;
        roll_divine_heal_power_proc(divine_pct, &mut units[healer_idx].divine_heal_power_buff, at_ms, rng);
        // Combat logging - see `CombatEvent::BuffSnapshot`'s doc, same
        // "both participants, right after this action's own direct
        // effects" scope as `apply_hit`'s own emission sites.
        let healer_id = units[healer_idx].id.clone();
        let healer_buffs = active_buffs_snapshot(&units[healer_idx], at_ms);
        if !healer_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: healer_id, buffs: healer_buffs });
        }
        let target_id = units[target_idx].id.clone();
        let target_buffs = active_buffs_snapshot(&units[target_idx], at_ms);
        if !target_buffs.is_empty() {
            events.push(CombatEvent::BuffSnapshot { at_ms, unit: target_id, buffs: target_buffs });
        }
    }
    healed
}

/// How many OTHER injured allies a Heal-function unit's splash also
/// heals, on top of the primary heal target - same fixed-cap idea as
/// `apply_splash`, just for healing (see `Affix::Splash`'s doc).
pub(crate) const HEAL_SPLASH_MAX_TARGETS: usize = 2;

/// After a heal's primary target resolves, splashes the same fraction of
/// that heal onto up to `HEAL_SPLASH_MAX_TARGETS` OTHER currently-hurt
/// allies (excluding the primary target and the healer's own already-
/// resolved turn), chosen at random - plus `SPLASH_OVERFLOW_BONUS_TARGETS`
/// more if `splash_fraction` is over 1.0 (see `Character::combat_splash`'s
/// doc). A no-op whenever `splash_fraction` is 0 (every non-Heal-function
/// unit's splash never reaches this - only called from the heal branch
/// below).
pub(crate) fn apply_heal_splash(units: &mut [CombatSimUnit], healer_idx: usize, primary_target_idx: usize, base_heal: u32, splash_fraction: f64, at_ms: u32, events: &mut Vec<CombatEvent>, rng: &mut impl Rng) {
    if splash_fraction <= 0.0 {
        return;
    }
    let splash_heal = (base_heal as f64 * splash_fraction.min(1.0)).round() as u32;
    if splash_heal == 0 {
        return;
    }
    let max_targets = if splash_fraction > 1.0 { HEAL_SPLASH_MAX_TARGETS + SPLASH_OVERFLOW_BONUS_TARGETS } else { HEAL_SPLASH_MAX_TARGETS };
    let mut candidates: Vec<usize> =
        units.iter().enumerate().filter(|(i, u)| *i != primary_target_idx && !u.is_boss && u.alive && u.hp < u.max_hp as i64).map(|(i, _)| i).collect();
    let pick_count = max_targets.min(candidates.len());
    for _ in 0..pick_count {
        let pick_at = rng.gen_range(0..candidates.len());
        let target_idx = candidates.remove(pick_at);
        apply_heal(units, healer_idx, target_idx, splash_heal as f64, at_ms, events, rng);
    }
}

/// Paladin's Radiant Smite - fires once per unified action (see
/// `smite_heal_pct`'s doc), healing up to `HEAL_SPLASH_MAX_TARGETS`
/// (+`smite_extra_targets`, +`SPLASH_OVERFLOW_BONUS_TARGETS` past 100%
/// splash) hurt allies - candidates include `healer_idx` themselves
/// ("targets around the Paladin" per the live design conversation this
/// was built from, unlike `apply_heal_splash`'s primary-heal-adjacent
/// splash which deliberately excludes the primary target/self) - each for
/// the FULL `heal_pct` of the healer's own max HP, not a diminishing
/// splash fraction of a primary target's heal (there IS no primary target
/// here - every hit target gets the identical amount). Returns the TOTAL
/// actually restored across every target, which Holy Fire (see
/// `smite_holyfire_dmg_pct`'s doc) needs to know how much damage to deal.
pub(crate) fn apply_radiant_smite_heal(units: &mut [CombatSimUnit], healer_idx: usize, heal_pct: f64, splash_fraction: f64, extra_targets: u32, at_ms: u32, events: &mut Vec<CombatEvent>, rng: &mut impl Rng) -> u64 {
    if heal_pct <= 0.0 {
        return 0;
    }
    let base_max_targets = HEAL_SPLASH_MAX_TARGETS + extra_targets as usize;
    let max_targets = if splash_fraction > 1.0 { base_max_targets + SPLASH_OVERFLOW_BONUS_TARGETS } else { base_max_targets };
    let mut candidates: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive && u.hp < u.max_hp as i64).map(|(i, _)| i).collect();
    let pick_count = max_targets.min(candidates.len());
    // United Front - scales the WHOLE cast's heal amount by how many
    // allies it actually reaches, known before the per-target loop below
    // starts.
    let risingfervor_bonus = units[healer_idx].zealotry_risingfervor_pct_per_ally * pick_count as f64;
    let heal_amount = units[healer_idx].max_hp as f64 * heal_pct * (1.0 + risingfervor_bonus);
    let martyrscall_bonus_pct = units[healer_idx].zealotry_martyrscall_bonus_pct;
    let mut total_healed = 0u64;
    let mut healed_low_hp_ally = false;
    for _ in 0..pick_count {
        let pick_at = rng.gen_range(0..candidates.len());
        let target_idx = candidates.remove(pick_at);
        // Desperate Grace - bonus heal to any target currently below 50%
        // HP, checked live per-target (not the damage share's boss target
        // Judgment gates off - a different condition entirely).
        let is_low_hp = units[target_idx].max_hp > 0 && (units[target_idx].hp as f64 / units[target_idx].max_hp as f64) < 0.5;
        if is_low_hp {
            healed_low_hp_ally = true;
        }
        let target_heal_amount = if is_low_hp && martyrscall_bonus_pct > 0.0 { heal_amount * (1.0 + martyrscall_bonus_pct) } else { heal_amount };
        total_healed += apply_heal(units, healer_idx, target_idx, target_heal_amount, at_ms, events, rng);
    }
    // Zealous Charge - a temporary self attack-speed buff whenever this
    // cast healed at least one ally below 50% HP (see
    // `zealouscharge_multiplier` for the consumption side).
    let guardianswrath_bonus = units[healer_idx].zealotry_guardianswrath_speed_pct;
    if healed_low_hp_ally && guardianswrath_bonus > 0.0 {
        units[healer_idx].zealotry_guardianswrath_speed_bonus = guardianswrath_bonus;
        units[healer_idx].zealotry_guardianswrath_expires_at_ms = at_ms + ZEALOUS_CHARGE_DURATION_MS;
    }
    total_healed
}

/// Holy Fire (Paladin) - `total_healed` (the unified action's normal
/// heal-power share PLUS Radiant Smite's own heal, summed by the caller)
/// converts this fraction into damage dealt to EVERY alive enemy, each an
/// independent `apply_hit` (its own crit/mitigation roll, same pipeline
/// every other damage instance uses - not a flat unmitigated number) -
/// the one path that lets a 100%-heal-power Paladin (whose normal damage
/// share is floored to 0) still put out real boss damage.
pub(crate) fn apply_holy_fire_damage(units: &mut [CombatSimUnit], healer_idx: usize, total_healed: u64, dmg_pct: f64, at_ms: u32, events: &mut Vec<CombatEvent>, rolls: &mut Vec<RollEvent>, rng: &mut impl Rng) {
    if dmg_pct <= 0.0 || total_healed == 0 {
        return;
    }
    let damage = total_healed as f64 * dmg_pct;
    let purgingflame_pct = units[healer_idx].purgingflame_heal_reduction_pct;
    let enemy_targets: Vec<usize> = units.iter().enumerate().filter(|(_, u)| u.is_boss && u.alive).map(|(i, _)| i).collect();
    for enemy_idx in enemy_targets {
        if !units[healer_idx].alive {
            break;
        }
        apply_hit(units, healer_idx, enemy_idx, damage, at_ms, events, rolls, rng, false, false);
        // Purging Flame - reduces the struck enemy's own healing received.
        if purgingflame_pct > 0.0 && units[enemy_idx].alive {
            units[enemy_idx].temp_heal_reduction_pct = purgingflame_pct;
            units[enemy_idx].temp_heal_reduction_expires_at_ms = at_ms + PURGING_FLAME_DEBUFF_DURATION_MS;
        }
    }
}

/// Cleric's Prayer of Mending - after a primary heal lands, rolls
/// `prayer_chance`; on success, chains the SAME heal action through up to
/// `prayer_bounce_targets` OTHER hurt allies (excluding the primary
/// target and each other as they're picked), each receiving
/// `prayer_bounce_value_pct` of the PRIMARY heal amount - "chain" here
/// means "reaches more targets off one proc", not "each hop's value
/// decays off the last one". Structurally distinct from
/// `apply_heal_splash` (a fixed, unconditional multi-target heal every
/// time): this is chance-gated, and its target count scales
/// independently via Chain of Light rather than a fixed constant.
pub(crate) fn apply_heal_bounce(units: &mut [CombatSimUnit], healer_idx: usize, primary_target_idx: usize, primary_heal: u32, at_ms: u32, events: &mut Vec<CombatEvent>, rng: &mut impl Rng) {
    let chance = units[healer_idx].prayer_chance;
    let bounce_targets = units[healer_idx].prayer_bounce_targets;
    if chance <= 0.0 || bounce_targets == 0 || primary_heal == 0 {
        return;
    }
    if !rng.gen_bool(chance.clamp(0.0, 1.0)) {
        return;
    }
    let bounce_value = primary_heal as f64 * units[healer_idx].prayer_bounce_value_pct;
    // Excludes the primary target too - Unbroken Prayer's re-bounces below
    // must never re-heal whoever the ORIGINAL heal already reached, not
    // just this function's own deterministic bounce candidates.
    let mut candidates: Vec<usize> =
        units.iter().enumerate().filter(|(i, u)| *i != primary_target_idx && !u.is_boss && u.alive && u.hp < u.max_hp as i64).map(|(i, _)| i).collect();
    // Compassion (rank 2+) - guarantees the lowest-HP ally is picked
    // first, by sorting the candidate pool ascending by hp before the
    // normal random-pick loop below consumes it front-to-back.
    if units[healer_idx].compassion_prioritize_lowest {
        candidates.sort_by_key(|&i| units[i].hp);
    }
    let pick_count = (bounce_targets as usize).min(candidates.len());
    let divine_favor_pct = units[healer_idx].divine_favor_shield_pct;
    let divine_favor_duration = units[healer_idx].divine_favor_shield_duration_ms;
    let healing_touch_pct = units[healer_idx].healing_touch_pct;
    let compassion_prioritize = units[healer_idx].compassion_prioritize_lowest;
    let compassion_dr_pct = units[healer_idx].compassion_dr_pct;
    let mut already_reached: Vec<usize> = vec![primary_target_idx];
    for pick_num in 0..pick_count {
        // Compassion - deterministically takes index 0 (already sorted
        // ascending by hp above) instead of a random index, so the
        // lowest-HP ally is always picked first.
        let pick_at = if compassion_prioritize { 0 } else { rng.gen_range(0..candidates.len()) };
        let target_idx = candidates.remove(pick_at);
        already_reached.push(target_idx);
        let healed = apply_heal(units, healer_idx, target_idx, bounce_value, at_ms, events, rng);
        if healed == 0 {
            continue;
        }
        // Compassion (rank 3) - the prioritized lowest-HP ally also gets
        // a temporary DR buff.
        if compassion_prioritize && pick_num == 0 && compassion_dr_pct > 0.0 {
            units[target_idx].temp_damage_reduction_bonus = compassion_dr_pct;
            units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + COMPASSION_DR_DURATION_MS;
        }
        // Divine Favor - the bounce also shields its target.
        if divine_favor_pct > 0.0 {
            let shield_amount = healed as f64 * divine_favor_pct;
            grant_shield(units, healer_idx, target_idx, shield_amount, at_ms, divine_favor_duration, events);
        }
        // Healing Touch - the bounced ally gets a temporary healing-power
        // buff of their own.
        if healing_touch_pct > 0.0 {
            units[target_idx].temp_heal_power_bonus = healing_touch_pct;
            units[target_idx].temp_heal_power_bonus_expires_at_ms = at_ms + HEALING_TOUCH_DURATION_MS;
        }
    }
    // Unbroken Prayer - keep rolling for one MORE ally not yet reached by
    // this same proc (see `unbroken_prayer_chance`'s doc), stopping on the
    // first failed roll or once the whole party's already been healed.
    let unbroken_chance = units[healer_idx].unbroken_prayer_chance;
    if unbroken_chance > 0.0 {
        loop {
            let eligible: Vec<usize> = units
                .iter()
                .enumerate()
                .filter(|(i, u)| !already_reached.contains(i) && !u.is_boss && u.alive && u.hp < u.max_hp as i64)
                .map(|(i, _)| i)
                .collect();
            if eligible.is_empty() || !rng.gen_bool(unbroken_chance.clamp(0.0, 1.0)) {
                break;
            }
            let target_idx = eligible[rng.gen_range(0..eligible.len())];
            already_reached.push(target_idx);
            let healed = apply_heal(units, healer_idx, target_idx, bounce_value, at_ms, events, rng);
            if healed == 0 {
                continue;
            }
            if divine_favor_pct > 0.0 {
                let shield_amount = healed as f64 * divine_favor_pct;
                grant_shield(units, healer_idx, target_idx, shield_amount, at_ms, divine_favor_duration, events);
            }
            if healing_touch_pct > 0.0 {
                units[target_idx].temp_heal_power_bonus = healing_touch_pct;
                units[target_idx].temp_heal_power_bonus_expires_at_ms = at_ms + HEALING_TOUCH_DURATION_MS;
            }
        }
    }
}

/// Resolves one full fight instantly: every player and the boss act on
/// their own attack-interval clock (whoever's next action is soonest
/// goes next). Every player's action is a single unified attack that
/// splits between damaging a random alive enemy and healing the
/// lowest-HP hurt ally, by `Character::combat_heal_power` (see its doc
/// and this fight loop's per-unit handling below) - not a separate
/// heal-or-attack choice. The boss just attacks a random alive player.
/// Continues until the boss or every player is dead (or the safety
/// valve trips). Returns whether the party won, each unit's starting
/// info, and the full ordered event log (real timestamps — see
/// `compress_events` for what actually gets broadcast).
pub(crate) fn simulate_battle(
    characters: &HashMap<String, Character>,
    enemies: Vec<(BossStats, Option<BossKind>, f64)>,
    stage: u32,
) -> (bool, Vec<CombatUnitInfo>, Vec<CombatEvent>, Vec<RollEvent>) {
    // Fixed for the whole fight (not recomputed as players die) - see
    // `prioritize_above_median`'s doc.
    let median_level = median_u32(&characters.values().map(|c| c.level).collect::<Vec<u32>>());
    // Late-stage damage penalty (see `CombatSimUnit::late_stage_damage_penalty_pct`'s
    // doc) - only ever applied to a REAL boss (kind.is_some() below), never
    // players or a basic-encounter mob, so this is computed once here
    // regardless of what kind of fight it turns out to be.
    let late_stage_penalty = stage as f64 / (stage as f64 + 2000.0);
    let mut units: Vec<CombatSimUnit> = characters
        .iter()
        .map(|(id, c)| {
            let (helm_power, helm_cooldown_ms) = c.helm_skill().unwrap_or((0.0, u32::MAX));
            let (boots_power, boots_cooldown_ms) = c.boots_skill().unwrap_or((0.0, u32::MAX));
            // Mage's Timewarp / Warlock's Demonic Speed - see
            // `early_fight_speed_multiplier`'s own doc. Both fields gated
            // on the SAME rank check (Timewarp/Demonic Speed's own
            // investment, not just Quickcast/Fel Haste's) so a Mage/
            // Warlock with the parent flat-speed node but not THIS
            // modifier correctly gets no early-fight burst at all.
            let early_fight_speed_rank = match c.archetype {
                Archetype::Mage => c.passive_node_rank("timewarp"),
                Archetype::Warlock => c.passive_node_rank("demonicspeed"),
                _ => 0,
            };
            let early_fight_speed_bonus_pct = if early_fight_speed_rank > 0 {
                match c.archetype {
                    Archetype::Mage => c.passive_node_magnitude("quickcast"),
                    Archetype::Warlock => c.passive_node_magnitude("felhaste"),
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let early_fight_speed_window_end_ms = if early_fight_speed_rank > 0 { 5_000 + 2_000 * early_fight_speed_rank } else { 0 };
            // Paladin's Unwavering / Cleric's Unyielding Faith - see
            // `low_hp_party_dr_pct`'s own doc. Mutually exclusive by
            // archetype, same convention as `early_fight_speed_*` above.
            let (low_hp_party_dr_pct, low_hp_party_dr_rank) = match c.archetype {
                Archetype::Paladin => (
                    c.passive_node_magnitude("vowofprotection") + c.passive_node_magnitude("beaconoflight") + c.passive_node_magnitude("hallowedground"),
                    c.passive_node_rank("unwavering"),
                ),
                Archetype::Cleric => (
                    c.passive_node_magnitude("sanctuary") + c.passive_node_magnitude("consecratedearth") + c.passive_node_magnitude("wardingprayer"),
                    c.passive_node_rank("unyieldingfaith"),
                ),
                _ => (0.0, 0),
            };
            // "Unlocked at rank 2" per the original text - rank 1 alone
            // does nothing (threshold 0.0, never triggers).
            let low_hp_party_dr_threshold = if low_hp_party_dr_rank >= 3 {
                0.65
            } else if low_hp_party_dr_rank >= 2 {
                0.50
            } else {
                0.0
            };
            // Vampiric Frenzy's real per-unit FlickerStrike cadence,
            // reduced from the base constant by the Slayer's own
            // investment (see `CombatSimUnit::flicker_cooldown_ms`'s
            // doc) - Slayer's own node key is "vampiricfrenzy" (renamed
            // 2026-08-17 to be globally unique - Berserker has its own
            // unrelated "frenzy" skill), so this Slayer gate is now just
            // defense in depth rather than the only thing preventing a
            // cross-archetype key collision.
            let flicker_cooldown_ms = if c.archetype == Archetype::Slayer {
                (FLICKER_STRIKE_COOLDOWN_MS as f64 * (1.0 - c.passive_node_magnitude("vampiricfrenzy").clamp(0.0, 0.9))).round().max(200.0) as u32
            } else {
                FLICKER_STRIKE_COOLDOWN_MS
            };
            // Slayer's Bloodpact - see `next_bloodpact_at_ms`'s doc.
            // `u32::MAX` (never invested) for every other archetype.
            let bloodpact_invested = c.archetype == Archetype::Slayer && c.passive_node_rank("sacrifice") > 0;
            let bloodpact_cooldown_ms =
                if bloodpact_invested { (BLOODPACT_BASE_COOLDOWN_MS as f64 - c.passive_node_rank("bloodsac") as f64 * 500.0).max(2000.0) as u32 } else { u32::MAX };
            // Paladin's Divine Shield - see `CombatSimUnit`'s doc. Gated
            // to Paladin specifically so this never accidentally reads a
            // same-keyed "shield" node on a different archetype.
            let divine_shield_invested = c.archetype == Archetype::Paladin && c.passive_node_rank("shield") > 0;
            let divine_shield_amount_pct = if divine_shield_invested {
                DIVINE_SHIELD_BASE_AMOUNT_PCT + c.passive_node_magnitude("bulwarkoflight")
            } else {
                0.0
            };
            let divine_shield_cooldown_ms = if divine_shield_invested {
                let cdr = (c.passive_node_magnitude("shield") + c.passive_node_magnitude("graceperiod")).clamp(0.0, 0.9);
                (DIVINE_SHIELD_BASE_COOLDOWN_MS as f64 * (1.0 - cdr)).round().max(200.0) as u32
            } else {
                u32::MAX
            };
            CombatSimUnit {
                id: id.clone(),
                display_name: c.display_name.clone(),
                is_boss: false,
                archetype: Some(c.archetype),
                spawned_at_ms: 0,
                role: Some(c.archetype.combat_function()),
                // Warlock's Life Tap - drains a flat % of max HP once at
                // construction (same "the trade is just always on" spirit
                // as Reckless Swing's own fold-in) in exchange for a
                // fixed 2x-that-percentage increased-damage bonus below
                // (3%/rank HP -> 6%/rank damage is a constant 1:2 ratio at
                // every rank, so `passive_node_magnitude("lifetap")` alone
                // - the HP side - covers both without needing a second
                // rank-matched function the way Reckless Swing's genuinely
                // asymmetric dealt/taken slopes did).
                // Pain Bond - reduces the HP COST specifically, without
                // touching the damage bonus (which stays keyed off the
                // full, un-discounted `lifetap` magnitude above).
                hp: (c.combat_max_hp() as f64 * (1.0 - (c.passive_node_magnitude("lifetap") - c.passive_node_magnitude("painbond")).max(0.0))).round().max(1.0) as i64,
                max_hp: (c.combat_max_hp() as f64 * (1.0 - (c.passive_node_magnitude("lifetap") - c.passive_node_magnitude("painbond")).max(0.0))).round().max(1.0) as u64,
                atk: c.combat_atk() as u64,
                heal_power: c.combat_heal_power(),
                intervene: c.combat_intervene(),
                attack_interval_ms: c.attack_interval_ms(),
                next_action_at_ms: 0,
                alive: true,
                helm_power,
                helm_cooldown_ms,
                // Waits one full cooldown before the first proc, same as
                // any other equipped skill would - not an instant t=0 burst.
                next_helm_at_ms: helm_cooldown_ms,
                helm_stack_bonus: 0.0,
                boots_power,
                boots_cooldown_ms,
                next_boots_at_ms: boots_cooldown_ms,
                // Berserker's Reckless Swing/Death Wish's "taken" half
                // now lives directly inside `combat_damage_reduction()`
                // itself (as a proper multiplicative negative-reduction
                // source, not a flat subtraction) - see that method's own
                // doc for why this reads it directly instead of repeating
                // the formula here.
                // Demonic Resilience - a flat DR bonus for the rest of the
                // fight, on top of the character's own combined DR.
                damage_reduction: c.combat_damage_reduction() + c.passive_node_magnitude("demonicresilience"),
                block_chance: c.combat_block_chance(),
                evasion: c.combat_evasion(),
                // Warrior's Overwhelming Force/Berserker's Reckless
                // Swing+Death Wish/Warlock's Life Tap all live directly
                // inside `combat_increased_damage()` itself now - see
                // that method's own doc for why this reads it directly
                // instead of repeating any of their formulas here (it
                // used to, which is exactly how these 4 nodes went
                // missing from the dashboard's own DPS/Increased Dmg
                // Dealt display for so long).
                increased_damage: c.combat_increased_damage(),
                crit_chance: c.combat_crit_chance(),
                crit_multiplier: c.combat_crit_multiplier(),
                // Entropic Force - Unstable Power's excess-baseline-
                // attack-speed conversion also feeds a flat splash bonus
                // at construction (baseline only, not live - same
                // "duplicated computation, no shared param" approach
                // Paradox's crit-chance half takes).
                splash: c.combat_splash()
                    + (c.combat_attack_speed_pct() - if c.passive_node_rank("chaostheory") >= 3 { 0.70 } else if c.passive_node_rank("chaostheory") >= 2 { 0.80 } else if c.passive_node_rank("chaostheory") >= 1 { 0.90 } else { 1.0 }).max(0.0) * c.passive_node_magnitude("entropicforce"),
                late_stage_damage_penalty_pct: 0.0,
                boss_focus_stacks: 0.0,
                boss_ability: None,
                next_ability_at_ms: u32::MAX,
                boss_dynamic_power_mult: 1.0,
                cthulhu_debuff_stacks: 0,
                cthulhu_debuff_expires_at_ms: 0,
                cthulhu_debuff_pct_per_stack: 0.0,
                cube_shred_stacks: 0,
                cube_shred_expires_at_ms: 0,
                damage_dealt_total: 0,
                level: c.level,
                life_leech_pct: c.combat_life_leech(),
                leech_window_start_ms: 0,
                leech_gained_in_window: 0.0,
                skills: c.archetype.skills().to_vec(),
                skill_stacks: HashMap::new(),
                // Waits one full (already-discounted) cooldown before the
                // first burst, same "not an instant t=0 freebie"
                // convention as Helm's own next_helm_at_ms.
                next_flicker_at_ms: if c.archetype.skills().contains(&ArchetypeSkill::FlickerStrike) { flicker_cooldown_ms } else { u32::MAX },
                flicker_cooldown_ms,
                divine_shield_amount_pct,
                divine_shield_cooldown_ms,
                // Waits one full (already-discounted) cooldown before the
                // first cast, same convention as Helm/FlickerStrike.
                next_divine_shield_at_ms: divine_shield_cooldown_ms,
                consecration_shield_pct: if divine_shield_invested && c.passive_node_rank("consecration") > 0 {
                    0.40 + c.passive_node_magnitude("consecration") + c.passive_node_magnitude("widerblessing")
                } else {
                    0.0
                },
                consecration_shield_duration_ms: DIVINE_SHIELD_DURATION_MS + (c.passive_node_magnitude("sharedlight") * 1000.0).round() as u32,
                communion_heal_power_pct: c.passive_node_magnitude("communion"),
                purify_dmg_debuff_pct: c.passive_node_magnitude("purify"),
                lastjudgment_skip_chance: c.passive_node_magnitude("lastjudgment"),
                // Paladin's Radiant Smite (offensive healing redesign) -
                // see `smite_heal_pct`'s doc. Zealotry's "1 additional
                // target" is a flat unlock at any invested rank, not
                // itself rank-scaled - only its heal-% bonus scales.
                smite_heal_pct: c.passive_node_magnitude("smite"),
                smite_zealotry_bonus_pct: c.passive_node_magnitude("zealotry"),
                smite_extra_targets: if c.passive_node_rank("zealotry") > 0 { 1 } else { 0 },
                zealotry_martyrscall_bonus_pct: c.passive_node_magnitude("martyrscall"),
                zealotry_risingfervor_pct_per_ally: c.passive_node_magnitude("risingfervor"),
                zealotry_guardianswrath_speed_pct: c.passive_node_magnitude("guardianswrath"),
                zealotry_guardianswrath_speed_bonus: 0.0,
                zealotry_guardianswrath_expires_at_ms: 0,
                smite_judgment_bonus_pct: c.passive_node_magnitude("judgment"),
                // Final Judgment's own text assumes a 20% base threshold
                // ("raised to 30/35/40% instead of 20%"), but Judgment's
                // real base (see `smite_judgment_bonus_pct`'s doc) is 50% -
                // applying the same stated +10/+15/+20 DELTA on top of the
                // real 50% base instead of taking its absolute numbers
                // literally, since a lower absolute value would shrink the
                // window rather than the "raised"/"more often" the text
                // clearly intends.
                judgment_threshold: if c.passive_node_rank("finaljudgment") >= 3 {
                    0.70
                } else if c.passive_node_rank("finaljudgment") >= 2 {
                    0.65
                } else if c.passive_node_rank("finaljudgment") >= 1 {
                    0.60
                } else {
                    0.0
                },
                smite_holyfire_dmg_pct: c.passive_node_magnitude("holyfire") + c.passive_node_magnitude("holyfirewildfire"),
                purgingflame_heal_reduction_pct: c.passive_node_magnitude("purgingflame"),
                temp_heal_reduction_pct: 0.0,
                temp_heal_reduction_expires_at_ms: 0,
                executionersblessing_heal_pct: c.passive_node_magnitude("executionersblessing"),
                wrathoftheheavens_chance: c.passive_node_magnitude("wrathoftheheavens"),
                // Paladin's Unbreakable Faith.
                unbreakable_faith_heal_pct: c.passive_node_magnitude("unbreakablefaith") + c.passive_node_magnitude("martyrsblessing"),
                eternalvow_shield_chance: c.passive_node_magnitude("eternalvow"),
                graciousburden_heal_pct: c.passive_node_magnitude("graciousburden"),
                bondeddevotion_dr_pct: c.passive_node_magnitude("bondeddevotion"),
                bondeddevotion_duration_ms: (c.passive_node_magnitude("steadfast") * 1000.0).round() as u32,
                // Mage's Temporal Rift / Warlock's Unstable Power - share
                // one field bundle (mutually exclusive by archetype).
                attack_speed_pct: c.combat_attack_speed_pct(),
                // Dilation extends Temporal Rift's own conversion rate.
                speed_overflow_dmg_pct: c.passive_node_magnitude("temporalrift") + c.passive_node_magnitude("unstablepower") + c.passive_node_magnitude("dilation") + c.passive_node_magnitude("voidenergy"),
                speed_overflow_crit_pct: c.passive_node_magnitude("paradox"),
                speed_overflow_threshold: if c.passive_node_rank("eternalmoment") >= 3 || c.passive_node_rank("chaostheory") >= 3 {
                    0.70
                } else if c.passive_node_rank("eternalmoment") >= 2 || c.passive_node_rank("chaostheory") >= 2 {
                    0.80
                } else if c.passive_node_rank("eternalmoment") >= 1 || c.passive_node_rank("chaostheory") >= 1 {
                    0.90
                } else {
                    1.0
                },
                // Rogue's Twin Strikes / Mage's Spell Echo - share one
                // field bundle (mutually exclusive by archetype). The
                // follow-up strike's damage share starts at Twin Strikes/
                // Spell Echo's own base 50% - Echoing Power (a deeper
                // Modifier-tier node) would raise this further but stays
                // deferred along with every other Modifier this pass.
                // Flurry - +10%/rank trigger chance. Double Tap - a real
                // recursive re-trigger would need a chain-depth parameter
                // threaded through `apply_hit`'s 11+ call sites just for
                // this one node; approximated instead as an equivalent
                // flat bump to the base trigger chance (same expected-
                // extra-strikes order of magnitude), documented rather
                // than silently diverging from its own text. Finite Loop
                // (`infiniteloop`) and Double Tap (`doubletap`) are
                // deliberately NOT summed in here anymore (2026-08-16) -
                // they're their own dedicated capped chains now (see
                // `finiteloop_max_repeats`/`doubletap_max_repeats`'s docs),
                // not another flat bump to this base one-shot chance.
                twin_strike_chance: c.passive_node_magnitude("twinstrikes")
                    + c.passive_node_magnitude("spellecho")
                    + c.passive_node_magnitude("flurry")
                    + c.passive_node_magnitude("resonance"),
                // Echo/Echoing Power - second-hit damage raised from the
                // 50% base.
                twin_strike_dmg_pct: TWIN_STRIKE_BASE_DMG_PCT + c.passive_node_magnitude("echo") + c.passive_node_magnitude("echoingpower"),
                // 2026-08-17 rework: cap restored to 3/6/9 (rank * 3), and
                // no longer paired with a separate chance field - repeats
                // reuse the base `twin_strike_chance` (see
                // `finiteloop_max_repeats`'s own doc).
                finiteloop_max_repeats: c.passive_node_rank("infiniteloop") * 3,
                doubletap_max_repeats: c.passive_node_rank("doubletap") * 3,
                in_splash_resolution: false,
                // Druid's Pack Instinct / Symbiosis - see `apply_hit`'s
                // live lowest-HP-ally computation.
                // Monk's Temple Guardian is mechanically identical to
                // Pack Instinct (grants evasion to the party's current
                // lowest-HP ally) - shares the same field/live-computation,
                // mutually exclusive by archetype like every other shared
                // bundle.
                // Iron Will - extends Temple Guardian's own contribution
                // to this shared bundle.
                // Pathfinder/Living Bond extend Pack Instinct/Symbiosis'
                // own magnitudes directly (same shared field Monk's
                // Temple Guardian/Iron Will already ride, mutually
                // exclusive by archetype).
                own_pack_instinct_evasion_pct: c.passive_node_magnitude("packinstinct") + c.passive_node_magnitude("templeguardian") + c.passive_node_magnitude("ironwill") + c.passive_node_magnitude("pathfinder"),
                own_symbiosis_dr_pct: c.passive_node_magnitude("symbiosis") + c.passive_node_magnitude("livingbond"),
                // United Pack/Rooted Network extend the same "protect N
                // allies" count Shared Strength already established.
                sharedstrength_extra_targets: c.passive_node_rank("sharedstrength") + c.passive_node_rank("unitedpack") + c.passive_node_rank("rootednetwork"),
                // Wild Guardian/Nature's Embrace extend the same periodic
                // protected-ally heal Guardian Spirit already established.
                templeguardian_heal_pct: c.passive_node_magnitude("templeguardianspirit") + c.passive_node_magnitude("wildguardian") + c.passive_node_magnitude("naturesembrace"),
                next_templeguardian_heal_at_ms: 0,
                lingering_effect_pct: c.combat_lingering_effect_pct(),
                lingering_dots: Vec::new(),
                next_lingering_tick_at_ms: u32::MAX,
                seedoflife_shield_pct: c.passive_node_magnitude("seedoflife"),
                wildheart_self_heal_pct: c.passive_node_magnitude("wildheart"),
                wildinstinct_dr_pct: c.passive_node_magnitude("wildinstinct"),
                wildroar_charges: if c.archetype == Archetype::Druid { c.passive_node_rank("livingbond") } else { 0 },
                naturesembrace_heal_targets: if c.archetype == Archetype::Druid { c.passive_node_rank("naturesembrace") } else { 0 },
                thickhide_cycle_ms: if c.archetype == Archetype::Druid && c.passive_node_rank("symbiosis") > 0 { c.passive_node_magnitude("symbiosis") as u32 } else { 0 },
                next_thickhide_cleanse_at_ms: 0,
                // Rooted Network - its own rank PLUS 1 more protected
                // target for every 100% splash the Druid has (splash is
                // stored as a 0.0-1.0+ fraction, so `.floor()` directly
                // gives "how many full 100%s").
                thickhide_target_count: if c.archetype == Archetype::Druid && c.passive_node_rank("symbiosis") > 0 {
                    1 + c.passive_node_magnitude("rootednetwork") as u32 + c.combat_splash().floor() as u32
                } else {
                    0
                },
                // Elemental damage rework (2026-08-15).
                fire_damage_pct: c.sum_affix(Affix::FireDamage),
                cold_damage_pct: c.sum_affix(Affix::ColdDamage),
                chaos_damage_pct: c.sum_affix(Affix::ChaosDamage),
                lightning_damage_pct: c.sum_affix(Affix::LightningDamage),
                divine_damage_pct: c.sum_affix(Affix::DivineDamage),
                fire_dr_debuff: Vec::new(),
                cold_evasion_debuff: Vec::new(),
                chaos_block_debuff: Vec::new(),
                lightning_dmg_taken: Vec::new(),
                divine_heal_reduction: Vec::new(),
                fire_dr_buff: Vec::new(),
                cold_evasion_buff: Vec::new(),
                chaos_block_buff: Vec::new(),
                divine_heal_power_buff: Vec::new(),
                block_overflow_dmg_rate: c.passive_node_magnitude("unbreakable"),
                evasion_overflow_dmg_rate: c.passive_node_magnitude("shiftingform"),
                elemental_overflow_dmg_bonus: 0.0,
                elemental_overflow_dmg_bonus_expires_at_ms: 0,
                // Chain Shot/Thunderstruck extend the same per-target rate.
                volley_dmg_per_target_pct: c.passive_node_magnitude("volley") + c.passive_node_magnitude("chainlightning") + c.passive_node_magnitude("chainshot") + c.passive_node_magnitude("thunderstruck"),
                splash_target_dmg_bonus: 0.0,
                // Surgical Strike (rank 3) - a flat approximation of
                // "also applies to splash, doubled at 3/3": since
                // `resolve_hit` already applies this bonus identically to
                // every landed hit regardless of caller (splash sub-hits
                // included - splash independently re-rolls its own crit,
                // same "already true" precedent Piercing Shots' own rank
                // 1/2 banked value established), the only real lever left
                // is rank 3's doubling, applied here as a flat multiplier
                // on the whole magnitude rather than splash-only.
                exploit_weakness_crit_mult_pct: c.passive_node_magnitude("exploitweakness") * if c.passive_node_rank("surgicalstrike") >= 3 { 2.0 } else { 1.0 },
                exploit_weakness_threshold: if c.passive_node_rank("vitalstrike") >= 3 {
                    0.80
                } else if c.passive_node_rank("vitalstrike") >= 2 {
                    0.65
                } else {
                    0.50
                },
                weakpoint_crit_chance_pct: c.passive_node_magnitude("weakpoint"),
                // Apex Predator - extends Nightstalker's own vs-boss
                // evasion bonus directly.
                nightstalker_evasion_pct: c.passive_node_magnitude("nightstalker") + c.passive_node_magnitude("apexpredator"),
                assassinate_crit_mult_bonus: c.passive_node_magnitude("coupdegrace"),
                silentblade_evasion_pct: c.passive_node_magnitude("silentblade"),
                fadeaway_duration_bonus_ms: (c.passive_node_magnitude("fadeaway") * 1000.0).round() as u32,
                backstab_dmg_pct: c.passive_node_magnitude("backstab"),
                backstab_pending_dmg_pct: 0.0,
                smokescreen_evasion_pct: c.passive_node_magnitude("smokescreen"),
                markedfordeath_hits_remaining: 0,
                markedfordeath_hit_count: if c.passive_node_rank("markedfordeath") >= 3 {
                    3
                } else if c.passive_node_rank("markedfordeath") >= 2 {
                    2
                } else {
                    0
                },
                finalcut_speed_pct: c.passive_node_magnitude("finalcut"),
                empoweredbolt_invested: c.passive_node_rank("empoweredbolt") >= 2,
                empoweredbolt_crit_mult_bonus: if c.passive_node_rank("empoweredbolt") >= 3 { 0.20 } else { 0.0 },
                volatilemagic_splash_pct: c.passive_node_magnitude("volatilemagic"),
                arcaneinstability_threshold: if c.passive_node_rank("arcaneinstability") >= 1 { 0.65 } else { 0.0 },
                arcaneinstability_bonus_pct: if c.passive_node_rank("arcaneinstability") >= 3 {
                    0.12
                } else if c.passive_node_rank("arcaneinstability") >= 2 {
                    0.09
                } else if c.passive_node_rank("arcaneinstability") >= 1 {
                    0.05
                } else {
                    0.0
                },
                premeditation_refund_chance: c.passive_node_magnitude("premeditation"),
                stack_evasion_per_stack: c.passive_node_magnitude("silentsteps"),
                huntersinstinct_crit_vs_boss_pct: c.passive_node_magnitude("huntersinstinct"),
                silentkiller_dmg_pct: c.passive_node_magnitude("silentkiller"),
                has_hit_boss_this_fight: false,
                // Assassinate - same non-linear rank gate as Guardian
                // Spirit/Undying Fury (0 below rank 2, 1 at rank 2, 2 at
                // rank 3), per its own "unlocked at rank 2... rank 3
                // grants a second use" text.
                assassinate_charges: if c.passive_node_rank("assassinate") >= 3 {
                    2
                } else if c.passive_node_rank("assassinate") >= 2 {
                    1
                } else {
                    0
                },
                dark_communion_pct: c.passive_node_magnitude("darkcommunion") + c.passive_node_magnitude("sharedsuffering"),
                compassion_prioritize_lowest: c.passive_node_rank("compassion") >= 2,
                compassion_dr_pct: if c.passive_node_rank("compassion") >= 3 { 0.05 } else { 0.0 },
                covenant_pct: if c.passive_node_rank("covenant") >= 3 {
                    1.0
                } else if c.passive_node_rank("covenant") >= 2 {
                    0.5
                } else {
                    0.0
                },
                unbreakablebond_dr_pct: c.passive_node_magnitude("unbreakablebond"),
                vigor_heal_pct: c.passive_node_magnitude("vigor") + c.passive_node_magnitude("bloodpump"),
                vengefulblood_shield_pct: c.passive_node_magnitude("vengefulblood"),
                secondgale_duration_ms: (c.passive_node_magnitude("secondgale") * 1000.0).round() as u32,
                temp_reckless_immunity_expires_at_ms: 0,
                reckless_penalty_offset: reckless_swing_taken_pct(c.passive_node_rank("reckless")) + death_wish_taken_pct(c.passive_node_rank("deathwish")),
                lastlaugh_crit_bonus: c.passive_node_rank("lastlaugh") >= 2,
                lastlaugh_crit_mult: c.passive_node_rank("lastlaugh") >= 3,
                ragefueled_speed_pct: c.passive_node_magnitude("ragefueled"),
                retaliation_chance: c.passive_node_magnitude("retaliation"),
                retaliation_dmg_pct: c.passive_node_magnitude("vengeance"),
                retaliation_heal_pct: c.passive_node_magnitude("bloodresolve"),
                retaliation_laststand_bonus: c.passive_node_magnitude("laststand"),
                grudge_pct_per_hit: c.passive_node_magnitude("grudge"),
                grudge_hit_counts: Vec::new(),
                retaliation_crit_bonus: c.passive_node_magnitude("executionersmark"),
                retaliation_payback_threshold: if c.passive_node_rank("payback") >= 3 {
                    0.45
                } else if c.passive_node_rank("payback") >= 2 {
                    0.30
                } else {
                    0.0
                },
                force_crit_next_hit: false,
                retaliation_surge_pct: c.passive_node_magnitude("adrenalinesurge"),
                hardened_stacks: 0,
                hardened_pct_per_stack: c.passive_node_magnitude("hardened"),
                retaliation_secondwind_threshold: if c.archetype == Archetype::Warrior && c.passive_node_rank("secondwind") >= 3 {
                    0.65
                } else if c.archetype == Archetype::Warrior && c.passive_node_rank("secondwind") >= 2 {
                    0.50
                } else {
                    0.0
                },
                laststand_defiance_pct: c.passive_node_magnitude("defiance"),
                laststand_berserkvigor_pct: c.passive_node_magnitude("berserkvigor"),
                immovable_crit_dr_pct: c.passive_node_magnitude("immovable"),
                reserves_heal_received_pct: c.passive_node_magnitude("reserves"),
                unbroken_ignore_evasion_pct: c.combat_unbroken_ignore_evasion_pct(),
                unbroken_crippling_grip_dr_pct: c.combat_crippling_grip_dr_pct(),
                unyieldingspirit_threshold: {
                    let rank = c.passive_node_rank("unyieldingspirit");
                    if rank > 0 { 0.25 + 0.10 * rank as f64 } else { 0.0 }
                },
                temp_evasion_debuff: 0.0,
                temp_evasion_debuff_expires_at_ms: 0,
                // Blizzard extends Frost Nova's own magnitude directly.
                frostnova_evasion_debuff_pct: c.passive_node_magnitude("frostnova") + c.passive_node_magnitude("blizzard"),
                frostnova_duration_ms: FROSTNOVA_DEBUFF_DURATION_MS + (c.passive_node_magnitude("permafrost") * 1000.0).round() as u32,
                absolutezero_threshold: if c.passive_node_rank("absolutezero") >= 3 {
                    0.65
                } else if c.passive_node_rank("absolutezero") >= 2 {
                    0.50
                } else {
                    0.0
                },
                staticfield_speed_debuff_pct: c.passive_node_magnitude("staticfield"),
                temp_attack_speed_debuff: 0.0,
                temp_attack_speed_debuff_expires_at_ms: 0,
                infernalpact_heal_pct: c.passive_node_magnitude("infernalpact"),
                stormcaller_extra_targets: c.passive_node_rank("stormcaller"),
                piercing_shots_crit_chance_bonus: if c.passive_node_rank("piercingshots") >= 3 { 0.10 } else { 0.0 },
                windpierce_splash_crit_pct: c.passive_node_magnitude("windpierce"),
                armorbreaker_dr_shred_pct: c.passive_node_magnitude("armorbreaker"),
                scorchedearth_dmg_debuff_pct: c.passive_node_magnitude("scorchedearth"),
                truestrike_primary_crit_pct: c.passive_node_magnitude("truestrike"),
                stormofarrows_extra_targets: c.passive_node_rank("stormofarrows"),
                widerburst_extra_targets: c.passive_node_rank("widerburst"),
                inner_focus_heal_pct: c.passive_node_magnitude("innerfocus"),
                // Inner Peace - extends Meditation's per-10%-evasion rate.
                inner_focus_meditation_bonus: c.passive_node_magnitude("meditation") + c.passive_node_magnitude("innerpeace"),
                // Sanctuary - extends Chi Burst's own ally-heal fraction.
                inner_focus_chiburst_pct: c.passive_node_magnitude("chiburst") + c.passive_node_magnitude("chiburstsanctuary"),
                // Unmovable - extends Serenity's own DR magnitude.
                inner_focus_serenity_dr_pct: c.passive_node_magnitude("serenity") + c.passive_node_magnitude("unmovable"),
                risingtide_heal_power_pct: c.passive_node_magnitude("risingtide"),
                widecircle_extra_targets: c.passive_node_rank("widecircle"),
                harmonize_dr_pct: c.passive_node_magnitude("harmonize"),
                serenity_dr_duration_ms: SERENITY_DR_DURATION_MS + (c.passive_node_magnitude("unshakable") * 1000.0).round() as u32,
                clarity_triggers_on_block: c.passive_node_rank("clarity") >= 2,
                evade_counter_chance: c.passive_node_magnitude("voidstep") + c.passive_node_magnitude("counterflow") + c.passive_node_magnitude("wildfury"),
                evade_counter_last_fired_at_ms: 0,
                temp_party_attack_speed_bonus: 0.0,
                temp_party_attack_speed_bonus_expires_at_ms: 0,
                temp_party_increased_damage_bonus: 0.0,
                temp_party_increased_damage_bonus_expires_at_ms: 0,
                temp_party_damage_reduction_bonus: 0.0,
                temp_party_damage_reduction_bonus_expires_at_ms: 0,
                low_hp_party_dr_pct,
                low_hp_party_dr_threshold,
                warlord_party_dmg_pct: c.passive_node_magnitude("warlord"),
                warcry_party_speed_pct: c.passive_node_magnitude("warcry"),
                neverending_invested: c.passive_node_rank("neverending") > 0,
                bloodlust_stack_expiries: Vec::new(),
                opportunist_guaranteed_hits: c.passive_node_rank("opportunist"),
                hits_landed_this_fight: 0,
                ambush_dr_cut_pct: c.passive_node_magnitude("ambush"),
                openingmove_cooldown_ms: (c.passive_node_magnitude("openingmove") * 1000.0).round() as u32,
                next_openingmove_at_ms: 0,
                coldsteel_pass_chance: c.passive_node_magnitude("coldsteel"),
                coldsteel_pending: false,
                coldsteel_pass_chance_pending: 0.0,
                coldsteel_ambush_pct_pending: 0.0,
                predator_dmg_taken_pct: c.passive_node_magnitude("predator"),
                predator_dmg_taken_bonus: 0.0,
                predator_expires_at_ms: 0,
                // Bloody Knife extends Cutthroat's own bonus directly.
                cutthroat_low_hp_dmg_pct: c.passive_node_magnitude("cutthroat") + c.passive_node_magnitude("bloodyknife"),
                vanish_evasion_pct: c.passive_node_magnitude("vanish"),
                temp_evasion_buff: 0.0,
                temp_evasion_buff_expires_at_ms: 0,
                // Silent Prowl (Druid) rides the exact same evade-
                // triggered temp-crit-buff mechanic as Vanishing Shot -
                // mutually exclusive by archetype.
                vanishingshot_crit_pct: c.passive_node_magnitude("vanishingshot") + c.passive_node_magnitude("silentprowl"),
                temp_crit_chance_buff: 0.0,
                temp_crit_chance_buff_expires_at_ms: 0,
                fleetingshadow_speed_pct: c.passive_node_magnitude("fleetingshadow"),
                has_celestial_conversion: EQUIP_SLOTS.iter().any(|&slot| c.equipped(slot).as_ref().is_some_and(|i| i.unique_affix == Some(UniqueAffix::CelestialConversion))),
                // Slayer's Open Wound (attacker-side potency) - see
                // `CombatSimUnit`'s doc. All 0/false (not invested) for
                // every other archetype and for a Slayer without a point
                // in `wound` yet.
                wound_deal_leech_per_stack: c.passive_node_magnitude("wound"),
                wound_deal_max_stacks: if c.passive_node_rank("wound") > 0 { 5 + c.passive_node_rank("blooddebt") } else { 0 },
                wound_deal_duration_ms: (WOUND_BASE_DURATION_MS as f64 * (1.0 + c.passive_node_magnitude("festering"))).round() as u32,
                wound_deal_damage_dealt_debuff: c.passive_node_magnitude("necrotic"),
                wound_deal_heal_received_debuff: c.passive_node_magnitude("rot") + c.passive_node_magnitude("witheringtouch"),
                wound_deal_explosion_pct: c.passive_node_magnitude("hemorrhage"),
                wound_deal_explosion_self_leech_pct: c.passive_node_magnitude("overflow"),
                wound_deal_explosion_extra_targets: c.passive_node_rank("arterialspray"),
                wound_deal_spreads_to_splash: c.passive_node_rank("festering") > 0,
                contagion_chance: c.passive_node_magnitude("contagion"),
                gravechill_speed_debuff_pct: c.passive_node_magnitude("gravechill"),
                plaguebearer_extra_targets: c.passive_node_rank("plaguebearer"),
                // Open Wound, defender-side (current status inflicted ON
                // this unit) - nobody starts a fight already wounded.
                wound_stacks: 0,
                wound_max_stacks: 0,
                wound_expires_at_ms: 0,
                wound_leech_per_stack: 0.0,
                wound_damage_dealt_debuff: 0.0,
                wound_heal_received_debuff: 0.0,
                wound_damage_taken_total: 0.0,
                next_bloodpact_at_ms: if bloodpact_invested { 0 } else { u32::MAX },
                bloodpact_last_fired_at_ms: 0,
                bloodpact_cooldown_ms,
                bloodpact_uses_this_fight: 0,
                bloodpact_hp_cost_pct: c.passive_node_magnitude("sacrifice").max(0.0),
                bloodpact_damage_mult: if c.passive_node_rank("sacrifice") > 0 { 1.0 + c.passive_node_rank("sacrifice") as f64 } else { 1.0 },
                bloodpact_martyrdom_shield_pct: c.passive_node_magnitude("martyrdom"),
                bloodpact_kill_refund_pct: c.passive_node_magnitude("grimbargain"),
                bloodpact_nonlethal_refund_pct: c.passive_node_magnitude("debtcollector"),
                bloodpact_bloodforblood_pct: c.passive_node_magnitude("bloodforblood"),
                bloodpact_triage_pct: c.passive_node_magnitude("triage"),
                bloodpact_finaloffering_min_prior_uses: {
                    let rank = c.passive_node_rank("finaloffering");
                    if rank > 0 { 4 - rank } else { u32::MAX }
                },
                bloodpact_finaloffering_pct: if c.passive_node_rank("finaloffering") > 0 { 0.33 } else { 0.0 },
                bloodpact_warlordsresolve_pct: c.passive_node_magnitude("warlordsresolve"),
                bloodpact_cleanslate_reset_chance: c.passive_node_magnitude("cleanslate"),
                bloodpact_secondwind_reset_chance: c.passive_node_magnitude("hemorrhagesecondwind"),
                shield_hp: 0.0,
                shield_expires_at_ms: 0,
                // Shield-absorb reflect - see `shield_reflect_pct`'s doc
                // for why these three archetypes share the fields but not
                // identical semantics.
                shield_reflect_pct: if c.archetype == Archetype::Cleric && c.passive_node_rank("sacredbarrier") > 0 {
                    0.20
                } else if c.archetype == Archetype::Paladin && c.passive_node_rank("retributionaura") > 0 {
                    c.passive_node_magnitude("retributionaura") + c.passive_node_magnitude("holyvengeance")
                } else if c.archetype == Archetype::Slayer && c.passive_node_rank("guardiansblood") > 0 {
                    c.passive_node_magnitude("guardiansblood")
                } else {
                    0.0
                },
                shield_reflect_chance: if c.archetype == Archetype::Cleric && c.passive_node_rank("sacredbarrier") > 0 {
                    c.passive_node_magnitude("sacredbarrier")
                } else {
                    1.0
                },
                shield_reflect_requires_full_absorb: c.archetype == Archetype::Paladin && c.passive_node_rank("retributionaura") > 0,
                // Druid's Unyielding Roots (2026-08-16 rework) - see
                // `unyieldingroots_cycle_ms`'s own doc. `magnitude_at_rank`
                // already gives 8000/6000/4000 directly (Special{8000,-2000}
                // in passive_tree.rs), no manual rank match needed.
                unyieldingroots_cycle_ms: if c.archetype == Archetype::Druid && c.passive_node_rank("unyieldingroots") > 0 {
                    c.passive_node_magnitude("unyieldingroots") as u32
                } else {
                    0
                },
                // Nature's Ward (2026-08-16 rework) - see `resolve_hit`'s
                // `naturesward_bonus` read site.
                naturesward_dr_vs_boss_pct: c.passive_node_magnitude("naturesward"),
                // Berserker's Gambit - see `roll_attacker_damage`'s doc.
                gambit_crit_per_missing_20pct: c.passive_node_magnitude("gambit"),
                deathdefiant_grace_ms: c.passive_node_rank("deathdefiant") * 3_000,
                deathdefiant_frozen_crit_bonus: 0.0,
                deathdefiant_frozen_crit_bonus_expires_at_ms: 0,
                // Druid's Bramblegrowth (+Thornlash's bonus folded in) and
                // Poison Thorns - see `apply_hit`'s doc.
                bramble_reflect_pct: c.passive_node_magnitude("bramblegrowth") + c.passive_node_magnitude("thornlash"),
                poison_thorns_debuff_pct: c.passive_node_magnitude("poisonthorns"),
                entangle_chance: c.passive_node_magnitude("entangle"),
                recent_attackers: Vec::new(),
                temp_damage_dealt_debuff: 0.0,
                temp_damage_dealt_debuff_expires_at_ms: 0,
                // Berserker's Frenzy (redefined 2026-08-15) - see
                // `fire_frenzy`'s doc. Key mapping onto the tree's
                // EXISTING node slots (no topology change, so no
                // orphaned investment from the old kill-proc design,
                // which never had a real effect anyway): bloodfury/
                // deathmark = chance, bloodscent = its own HP-gated
                // chance-doubler, cullingblow = Overkill's DR shred,
                // killingspree/chainkiller = extra-strike damage,
                // massacre = Culling Strike's execute threshold,
                // reaperscall = Chain Frenzy, savagemomentum/unbridled =
                // Bloodletting's heal, warpath = Second Wind's shield
                // chance, bloodrush = Undying Fury's charges.
                frenzy_strike_chance: if c.passive_node_rank("frenzy") > 0 {
                    FRENZY_BASE_STRIKE_CHANCE + c.passive_node_magnitude("bloodfury") + c.passive_node_magnitude("deathmark")
                } else {
                    0.0
                },
                frenzy_extra_hits: c.passive_node_rank("frenzy"),
                frenzy_bloodscent_threshold: match c.passive_node_rank("bloodscent") {
                    0 | 1 => 0.0,
                    2 => 0.50,
                    _ => 0.65,
                },
                frenzy_dr_shred_pct: c.passive_node_magnitude("cullingblow"),
                frenzy_extra_dmg_pct: c.passive_node_magnitude("killingspree") + c.passive_node_magnitude("chainkiller"),
                frenzy_culling_threshold: c.passive_node_magnitude("massacre"),
                frenzy_heal_pct: c.passive_node_magnitude("savagemomentum") + c.passive_node_magnitude("unbridled"),
                frenzy_shield_chance: c.passive_node_magnitude("warpath"),
                // Undying Will (Warrior) and Glorious Death (Berserker's
                // Death Wish branch) share this same self-only "don't die,
                // go to 1 HP" charge mechanic with Berserker's own Undying
                // Fury - a Berserker CAN have both Undying Fury and
                // Glorious Death, in which case the higher of the two
                // applies rather than stacking (same "take the better one,
                // don't double-count" spirit as every other shared-field
                // convention here). Glorious Death's own gate starts a
                // point earlier ("once at rank 1") than the other two's
                // ("first charge at rank 2").
                frenzy_undying_charges: match c.passive_node_rank("bloodrush").max(c.passive_node_rank("undyingwill")) {
                    0 | 1 => 0,
                    2 => 1,
                    _ => 2,
                }
                .max(match c.passive_node_rank("gloriousdeath") {
                    0 => 0,
                    1 | 2 => 1,
                    _ => 2,
                })
                // Slayer's Undying (Reaper's Momentum branch) - same gate.
                .max(match c.passive_node_rank("undying") {
                    0 => 0,
                    1 | 2 => 1,
                    _ => 2,
                }),
                frenzy_chain_chance: c.passive_node_magnitude("reaperscall"),
                frenzy_chain_max_extra: c.passive_node_rank("reaperscall"),
                // Cleric's Guardian Spirit - see `CombatSimUnit`'s doc.
                // Charge count is rank-gated (not a smooth per-rank
                // formula): 0 below rank 2, 1 at rank 2, 2 at rank 3,
                // matching the node's own "unlocked at rank 2... rank 3
                // grants a second use" text.
                guardian_spirit_charges: if c.archetype == Archetype::Cleric {
                    match c.passive_node_rank("guardianspirit") {
                        0 | 1 => 0,
                        2 => 1,
                        _ => 2,
                    }
                } else if c.archetype == Archetype::Slayer && c.passive_node_rank("lastrites") > 0 {
                    // Last Rites - a party-wide "prevent one death" charge,
                    // same shared mechanic Guardian Spirit uses (its own
                    // per-rank CHANCE collapses to "invested at all grants
                    // one use" here, since the shared interception check
                    // is a deterministic charge count, not a live roll).
                    1
                } else {
                    0
                },
                guardian_spirit_heal_pct: if c.archetype == Archetype::Cleric && c.passive_node_rank("guardianspirit") >= 2 {
                    0.20 + c.passive_node_magnitude("secondchance")
                } else {
                    0.0
                },
                guardian_spirit_save_dr_pct: c.passive_node_magnitude("divineintervention"),
                guardian_spirit_save_heal_power_pct: c.passive_node_magnitude("finalblessing"),
                verdantburst_charges: if c.archetype == Archetype::Druid { c.passive_node_rank("verdantburst") } else { 0 },
                temp_heal_power_bonus: 0.0,
                temp_heal_power_bonus_expires_at_ms: 0,
                eternallight_bonus_pct: c.passive_node_magnitude("eternallight"),
                temp_damage_reduction_bonus: 0.0,
                temp_damage_reduction_bonus_expires_at_ms: 0,
                // Cleric's Overflowing Grace - see `apply_heal`'s doc.
                // Rift of Mercy's bonus is stored in whole seconds
                // (matches its own "+2s per rank" text), converted to ms
                // here alongside the base duration.
                overflow_grace_shield_pct: c.passive_node_magnitude("overflowinggrace") + c.passive_node_magnitude("graciousoverflow"),
                overflow_grace_shield_duration_ms: OVERFLOW_GRACE_SHIELD_BASE_DURATION_MS + (c.passive_node_magnitude("riftofmercy") * 1000.0).round() as u32,
                // Balanced Faith (Cleric) and Radiant Barrier (Paladin) are
                // the same mechanic on different trees - both grant +DR
                // while THIS unit's shield (from any source, `shield_hp`)
                // is active, so they sum into the same field with zero
                // extra plumbing.
                overflow_grace_shield_dr_pct: c.passive_node_magnitude("balancedfaith") + c.passive_node_magnitude("radiantbarrier"),
                // Cleric's Sanctified Touch - rank-gated same spirit as
                // Guardian Spirit above ("unlocked at rank 2... rank 3
                // also grants..."). Druid's Nature's Blessing (2026-08-15)
                // is a word-for-word clone sharing these same fields -
                // `bloomstrike`/`wildinstinct` are Holy Crit/Divine
                // Clarity's Druid-side twins, `verdantburst` sums into the
                // same splash field as `radiance`.
                heal_crit_bonus_mult: if c.archetype == Archetype::Cleric && c.passive_node_rank("sanctifiedtouch") >= 2 {
                    0.50 + c.passive_node_magnitude("holycrit")
                } else if c.archetype == Archetype::Druid && c.passive_node_rank("naturesblessing") >= 2 {
                    0.50 + c.passive_node_magnitude("bloomstrike")
                } else {
                    0.0
                },
                heal_crit_chance_bonus: if c.archetype == Archetype::Cleric && c.passive_node_rank("sanctifiedtouch") >= 3 {
                    0.10 + c.passive_node_magnitude("divineclarity")
                } else if c.archetype == Archetype::Druid && c.passive_node_rank("naturesblessing") >= 3 {
                    0.10 + c.passive_node_magnitude("wildinstinct")
                } else {
                    0.0
                },
                heal_crit_splash_pct: c.passive_node_magnitude("radiance") + c.passive_node_magnitude("verdantburst"),
                grace_lowest_ally_bonus_pct: c.passive_node_magnitude("graciousspirit"),
                // Cleric's Prayer of Mending - see `apply_heal_bounce`'s
                // doc. Bounce target count uses RANK directly (a flat
                // +1/rank count, same convention as Slayer's Blood
                // Sacrifice charges), not magnitude. Druid's Rejuvenation
                // (2026-08-15) is the same clone pattern: `bloomingfield`/
                // `seedoflife`/`evergrowth` are Chain of Light/Swift
                // Mending/Merciful Touch's Druid-side twins.
                prayer_chance: c.passive_node_magnitude("prayer") + c.passive_node_magnitude("swiftmending") + c.passive_node_magnitude("rejuvenation") + c.passive_node_magnitude("seedoflife"),
                prayer_bounce_targets: if c.archetype == Archetype::Cleric && c.passive_node_rank("prayer") > 0 {
                    (1 + c.passive_node_rank("chainoflight") + c.passive_node_magnitude("wideningcircle").round() as u32).min(5)
                } else if c.archetype == Archetype::Druid && c.passive_node_rank("rejuvenation") > 0 {
                    (1 + c.passive_node_rank("bloomingfield")).min(3)
                } else {
                    0
                },
                unbroken_prayer_chance: c.passive_node_magnitude("unbrokenprayer"),
                prayer_bounce_value_pct: if c.passive_node_rank("mercifultouch") > 0 {
                    c.passive_node_magnitude("mercifultouch") + c.passive_node_magnitude("gentletouch")
                } else {
                    0.50 + c.passive_node_magnitude("gentletouch") + c.passive_node_magnitude("evergrowth")
                },
                divine_favor_shield_pct: c.passive_node_magnitude("divinefavor") + c.passive_node_magnitude("aegisofmercy"),
                divine_favor_shield_duration_ms: DIVINE_FAVOR_SHIELD_BASE_DURATION_MS + (c.passive_node_magnitude("wardinglight") * 1000.0).round() as u32,
                healing_touch_pct: c.passive_node_magnitude("healingtouch"),
                crit_shield_max_hp_pct: c.passive_node_magnitude("arcaneshield"),
                soul_harvest_heal_pct: c.passive_node_magnitude("soulharvest") + c.passive_node_magnitude("reaping"),
                darkritual_dmg_pct: c.passive_node_magnitude("darkritual"),
                eternal_hunger_shield_pct: c.passive_node_magnitude("eternalhunger"),
                // Warrior's Spike Barrier/Aegis - see `apply_hit`'s
                // block-triggered handling.
                spike_barrier_reflect_pct: c.passive_node_magnitude("spikebarrier"),
                aegis_shield_pct: c.passive_node_magnitude("aegis"),
                aegis_shield_duration_ms: AEGIS_SHIELD_DURATION_MS + (c.passive_node_magnitude("bastion") * 1000.0).round() as u32,
                aegis_rally_speed_pct: c.passive_node_magnitude("rally"),
                aegis_extra_targets: c.passive_node_magnitude("ironcircle").round() as u32,
                thornedhide_pct_per_stack: c.passive_node_magnitude("thornedhide"),
                thornedhide_stacks: 0,
                thornedhide_expires_at_ms: 0,
                thornedhide_debuff_pct_per_stack: 0.0,
                spike_retribution_chance: c.passive_node_magnitude("retribution"),
                spike_unyielding_chance: c.passive_node_magnitude("unyielding"),
                block_damage_reduction_pct: if c.passive_node_rank("secondskin") > 0 { c.passive_node_magnitude("secondskin") } else { BLOCK_DAMAGE_REDUCTION },
                stonewall_auto_block_hits: c.passive_node_rank("stonewall"),
                hits_taken_this_fight: 0,
                // Warrior's Momentum / Rogue's Fleetfoot / Berserker's
                // Bloodlust / Ranger's Relentless Pursuit / Mage's Flow
                // State - see `stack_speed_per_stack`'s doc for why these
                // five share one field bundle (mutually exclusive by
                // archetype, safe to sum the per-stack magnitudes directly
                // - Frenzied Blows' bonus folds into the same attack-speed
                // field Momentum/Fleetfoot use).
                stack_speed_per_stack: c.passive_node_magnitude("momentum") + c.passive_node_magnitude("fleetfoot") + c.passive_node_magnitude("frenziedblows") + c.passive_node_magnitude("relentlesspursuit") + c.passive_node_magnitude("flowstate"),
                stack_dmg_per_stack: c.passive_node_magnitude("bloodlust") + c.passive_node_magnitude("furyunleashed"),
                stack_avalanche_dmg_per_stack: c.passive_node_magnitude("avalanche"),
                stack_crit_per_stack: c.passive_node_magnitude("riptide"),
                shatter_shred_pct: if c.passive_node_rank("shatter") > 0 { 1.0 } else { 0.0 },
                overwhelm_shred_linger_ms: (c.passive_node_magnitude("exposed") * 1000.0).round() as u32,
                crush_dr_threshold: if c.passive_node_rank("crush") >= 3 {
                    0.65
                } else if c.passive_node_rank("crush") >= 2 {
                    0.50
                } else {
                    0.0
                },
                stack_splash_per_stack: c.passive_node_magnitude("hurricane") + c.passive_node_magnitude("huntersstride"),
                windfury_chance: c.passive_node_magnitude("windfury"),
                stack_shred_per_stack: c.passive_node_magnitude("overwhelm"),
                stack_speed_max_stacks: if c.archetype == Archetype::Warrior && c.passive_node_rank("momentum") > 0 {
                    // Unstoppable - +1 max stack per rank, up to 8 (from
                    // the base 5).
                    (MOMENTUM_STACK_MAX + c.passive_node_rank("unstoppable")).min(8)
                } else if c.archetype == Archetype::Rogue && c.passive_node_rank("fleetfoot") > 0 {
                    // Windrunner - +1 max stack per rank, up to 6 (from
                    // the base 3).
                    (FLEETFOOT_STACK_MAX + c.passive_node_rank("windrunner")).min(6)
                } else if c.archetype == Archetype::Berserker && c.passive_node_rank("bloodlust") > 0 {
                    BLOODLUST_STACK_MAX
                } else if c.archetype == Archetype::Ranger && c.passive_node_rank("relentlesspursuit") > 0 {
                    // Windborn - +1 max stack per rank, up to 8 (from the
                    // base 5).
                    (RELENTLESS_PURSUIT_STACK_MAX + c.passive_node_rank("windborn")).min(8)
                } else if c.archetype == Archetype::Mage && c.passive_node_rank("flowstate") > 0 {
                    // Perpetual Motion - +1 max stack per rank, up to 8.
                    (FLOWSTATE_STACK_MAX + c.passive_node_rank("perpetualmotion")).min(8)
                } else {
                    0
                },
                stack_speed_duration_ms: if c.archetype == Archetype::Warrior && c.passive_node_rank("momentum") > 0 {
                    // Rampage - +2s per rank.
                    MOMENTUM_STACK_DURATION_MS + (c.passive_node_magnitude("rampage") * 1000.0).round() as u32
                } else if c.archetype == Archetype::Rogue && c.passive_node_rank("fleetfoot") > 0 {
                    FLEETFOOT_STACK_DURATION_MS
                } else if c.archetype == Archetype::Berserker && c.passive_node_rank("bloodlust") > 0 {
                    BLOODLUST_STACK_DURATION_MS + (c.passive_node_magnitude("unendingrage") * 1000.0).round() as u32
                } else if c.archetype == Archetype::Ranger && c.passive_node_rank("relentlesspursuit") > 0 {
                    // Never Winded - approximated as extending the shared
                    // expiry window rather than true "only holds longer
                    // once already at max" logic (same simplification
                    // spirit as Warrior's Rampage/Unbroken Chain).
                    RELENTLESS_PURSUIT_STACK_DURATION_MS + (c.passive_node_magnitude("neverwinded") * 1000.0).round() as u32
                } else if c.archetype == Archetype::Mage && c.passive_node_rank("flowstate") > 0 {
                    // Unbroken Rhythm - approximated as extending the
                    // shared expiry window (see Warrior's Rampage /
                    // Ranger's Never Winded for the same simplification).
                    FLOWSTATE_STACK_DURATION_MS + (c.passive_node_magnitude("unbrokenrhythm") * 1000.0).round() as u32
                } else {
                    0
                },
                // Tempo - Frenzied Blows grants free stacks the moment
                // combat starts (a fight always begins at ms 0, so a
                // fixed initial current-stack value is exactly equivalent
                // to rolling them in at construction).
                // Quickdraw (Rogue) - free Fleetfoot stacks on entering
                // combat, 1 at rank 2 / 2 at rank 3 (non-linear, matching
                // its own text).
                stack_speed_current: if c.archetype == Archetype::Berserker {
                    c.passive_node_magnitude("tempo").round() as u32
                } else if c.archetype == Archetype::Rogue {
                    match c.passive_node_rank("quickdraw") {
                        0 | 1 => 0,
                        2 => 1,
                        _ => 2,
                    }
                } else {
                    0
                },
                stack_speed_expires_at_ms: if c.archetype == Archetype::Berserker && c.passive_node_rank("tempo") > 0 {
                    BLOODLUST_STACK_DURATION_MS + (c.passive_node_magnitude("unendingrage") * 1000.0).round() as u32
                } else if c.archetype == Archetype::Rogue && c.passive_node_rank("quickdraw") >= 2 {
                    FLEETFOOT_STACK_DURATION_MS
                } else {
                    0
                },
                // Monk's Flowing Strikes - see `flowing_speed_per_stack`'s
                // doc. Hundred Fists/Relentless Assault fold into the
                // max-stacks/duration here at construction.
                // Windwalker extends the per-stack speed rate directly.
                flowing_speed_per_stack: c.passive_node_magnitude("flowingstrikes") + c.passive_node_magnitude("windwalker"),
                flowing_crit_per_stack: c.passive_node_magnitude("pressurepoint"),
                flowing_max_stacks: if c.archetype == Archetype::Monk && c.passive_node_rank("flowingstrikes") > 0 {
                    FLOWING_STACK_BASE_MAX + c.passive_node_magnitude("hundredfists").round() as u32
                } else {
                    0
                },
                // Unbroken Chain/Unending Cycle both widen the same
                // tolerance window (Unbroken Chain's own "persist through
                // a missed hit" and Unending Cycle's "extended duration"
                // both cash out as more real time before the streak
                // resets, in this event-driven sim's terms).
                flowing_duration_ms: if c.archetype == Archetype::Monk && c.passive_node_rank("flowingstrikes") > 0 {
                    FLOWING_STACK_DURATION_MS
                        + if c.passive_node_rank("relentlessassault") >= 3 { 2_000 } else { 0 }
                        + (c.passive_node_magnitude("unbrokenchain") * 1000.0).round() as u32
                        + (c.passive_node_magnitude("unendingcycle") * 1000.0).round() as u32
                } else {
                    0
                },
                risingstorm_dmg_pct: c.passive_node_magnitude("risingstorm"),
                nervestrike_crit_mult_bonus: c.passive_node_magnitude("nervestrike"),
                vitalpoints_shred_per_stack: c.passive_node_magnitude("vitalpoints"),
                eternalflow_bonus_stacks: c.passive_node_rank("eternalflow"),
                // `.min(3)` because this reads the RAW rank, and Flow like
                // Water became a Specialization (max_rank 4) in the
                // 2026-08-18 swap - without the clamp its 4th point would
                // silently grant a 4th bonus stack, breaking the shared
                // convention that a spec's 4th point only unlocks its
                // modifiers (what `magnitude_at_rank` enforces for every
                // magnitude-based spec) and contradicting this node's own
                // "up to 3 at 3/3" text.
                onehundredhands_bonus_stacks: c.passive_node_rank("onehundredhands").min(3),
                stormfront_splash_pct: c.passive_node_magnitude("stormfront"),
                flowing_current: 0,
                flowing_expires_at_ms: 0,
                flowing_last_target: usize::MAX,
                chakra_of_many_pct: c.passive_node_magnitude("chakraofmany"),
                chakra_of_light_pct: c.passive_node_magnitude("chakraoflight"),
                chakraoflife_duration_ms: c.passive_node_rank("chakraoflife") * 1_000,
                chakraoflife_immune_until_ms: 0,
                next_chakraoflife_expiry_at_ms: u32::MAX,
                // Ranger's Hunter's Mark - Predator's Eye/Kill Zone/Pack
                // Tactics are all personal-investment amplifiers, folded
                // in directly at construction same as everywhere else.
                own_mark_crit_chance: c.passive_node_magnitude("mark") + c.passive_node_magnitude("trueshot"),
                own_mark_crit_mult: c.passive_node_magnitude("predatorseye") + c.passive_node_magnitude("apexhunter"),
                own_mark_low_hp_dmg: c.passive_node_magnitude("killzone"),
                own_mark_ally_crit_chance: c.passive_node_magnitude("packtactics") + c.passive_node_magnitude("coordinatedstrike"),
                own_mark_ally_dmg_pct: c.passive_node_magnitude("alphaspredator"),
                // Hunter's Focus - a fraction (1/3 per rank) of the
                // Ranger's OWN Predator's Eye+Apex Hunter total, not an
                // independent magnitude.
                own_mark_ally_crit_mult: (c.passive_node_magnitude("predatorseye") + c.passive_node_magnitude("apexhunter"))
                    * (c.passive_node_rank("huntersfocus") as f64 / 3.0),
                own_mark_spread_count: c.passive_node_rank("widerpack"),
                killzone_threshold: if c.passive_node_rank("finalblow") >= 3 {
                    0.45
                } else if c.passive_node_rank("finalblow") >= 2 {
                    0.40
                } else if c.passive_node_rank("finalblow") >= 1 {
                    0.35
                } else {
                    0.0
                },
                cleankill_remark_chance: c.passive_node_magnitude("cleankill"),
                huntersreward_heal_pct: c.passive_node_magnitude("huntersreward"),
                // Warlock's Curse of Weakness - Amplify Curse's bonus
                // folds directly into the base magnitude.
                own_curse_dmg_taken: c.passive_node_magnitude("curse") + c.passive_node_magnitude("amplifycurse") + c.passive_node_magnitude("hexmastery"),
                own_curse_spread_count: c.passive_node_magnitude("contagiouscurse").round() as u32 + c.passive_node_rank("plagueoflocusts"),
                own_doom_detonate_pct: c.passive_node_magnitude("doom") + c.passive_node_magnitude("harbinger"),
                own_curse_heal_reduction_pct: c.passive_node_magnitude("witheringcurse"),
                own_curse_spread_bonus_pct: c.passive_node_magnitude("epidemic"),
                own_soul_stone_max: c.passive_node_rank("virulence"),
                own_cursed_blood_target_count: c.passive_node_rank("cursedblood"),
                own_dreadfuldeath_shred_pct: c.passive_node_magnitude("dreadfuldeath"),
                own_apocalypse_splash_pct: c.passive_node_magnitude("apocalypse"),
                has_applied_mark_this_fight: false,
                mark_source_id: None,
                mark_crit_chance_bonus: 0.0,
                mark_crit_multiplier_bonus: 0.0,
                mark_low_hp_damage_bonus: 0.0,
                mark_ally_crit_chance_bonus: 0.0,
                mark_ally_dmg_bonus: 0.0,
                mark_ally_crit_multiplier_bonus: 0.0,
                curse_dmg_taken_bonus: 0.0,
                soul_stones: 0,
                soul_stone_uses_this_fight: 0,
                curse_expires_at_ms: u32::MAX,
                next_curse_expiry_at_ms: u32::MAX,
                curse_damage_taken_total: 0.0,
                curse_detonate_pct: 0.0,
                curse_source_id: None,
                curse_heal_reduction_bonus: 0.0,
                // Warlock's Fel Rush.
                // Death March extends Fel Rush's own magnitude directly.
                fel_rush_speed_bonus: c.passive_node_magnitude("felrush") + c.passive_node_magnitude("deathmarch"),
                fel_rush_duration_ms: FEL_RUSH_DURATION_MS + (c.passive_node_magnitude("warpspeed") * 1000.0).round() as u32,
                // Ravage (rank 3) - a small additive stack on top of Fel
                // Rush's own flat bonus, one stack per kill while active,
                // capped modestly (real approximation of "stacks
                // additively" without unbounded growth).
                ravage_stack_pct: if c.passive_node_rank("ravage") >= 3 { c.passive_node_magnitude("felrush") * 0.5 } else { 0.0 },
                fel_rush_stacks: 0,
                fel_rush_expires_at_ms: 0,
                early_fight_speed_bonus_pct,
                early_fight_speed_window_end_ms,
                // Slayer's Blood Frenzy / Endless Thirst / Reaper's Momentum.
                flicker_frenzy_speed_bonus: c.passive_node_magnitude("bloodfrenzy"),
                unrelenting_duration_bonus_ms: if c.passive_node_rank("unrelenting") >= 3 {
                    600_000
                } else {
                    (c.passive_node_magnitude("unrelenting") * 1333.0).round() as u32
                },
                adrenaline_crit_mult_bonus: c.passive_node_magnitude("adrenaline"),
                chainreaper_heal_pct: c.passive_node_magnitude("chainreaper"),
                deathspiral_heal_pct: c.passive_node_magnitude("deathspiral"),
                insatiable_extend_chance: c.passive_node_magnitude("insatiable"),
                secondheartbeat_chance: c.passive_node_magnitude("secondheartbeat"),
                overflowvessel_shield_pct: c.passive_node_magnitude("overflowvessel"),
                flicker_frenzy_expires_at_ms: 0,
                endless_thirst_cap_bonus: if c.passive_node_rank("endlessthirst") >= 3 { 0.0 } else { c.passive_node_magnitude("endlessthirst") },
                endless_thirst_uncapped: c.passive_node_rank("endlessthirst") >= 3,
                endless_thirst_expires_at_ms: 0,
                reapers_momentum_per_kill: c.passive_node_magnitude("reapers").round() as u32,
                reapers_momentum_banked: 0,
            }
        })
        .collect();
    // One CombatSimUnit per enemy - a real boss fight passes 1 (or,
    // stage 50+, 2 - see run_encounter) real bosses each paired with
    // its own `BossKind`, a basic encounter passes one per member of
    // its group (see `run_basic_encounter`), all paired with `None` -
    // no ability/focus-targeting behavior for plain mobs. Each real
    // boss keeps its OWN ability/focus-targeting independently, whether
    // there's 1 of them or 2.
    let enemy_count = enemies.len();
    for (i, (enemy, kind, boss_dynamic_power_mult)) in enemies.into_iter().enumerate() {
        let this_unit_kind = kind;
        // Cthulhu/Lich fire periodically (5s/2s). Fire Demon is a passive
        // fight-wide aura applied once below instead, so it never needs a
        // periodic trigger. Dragon's Breath (2026-08-16) is no longer a
        // separate periodic ability layered on top of its normal attacks -
        // it now IS the Dragon's normal attack, every swing, at its usual
        // attack_interval_ms cadence (see the main boss-attack branch
        // below) - so Dragon gets no separate ability timer either.
        let next_ability_at_ms = match this_unit_kind {
            Some(BossKind::Cthulhu) => CTHULHU_DEBUFF_CADENCE_MS,
            Some(BossKind::Lich) => LICH_SUMMON_CADENCE_MS,
            Some(BossKind::GelatinousCube) => CUBE_CAPTURE_CADENCE_MS,
            _ => u32::MAX,
        };
        units.push(CombatSimUnit {
            id: enemy_unit_id(i),
            display_name: match this_unit_kind {
                Some(kind) => kind.display_name().to_string(),
                None if enemy_count == 1 => "Boss".to_string(),
                None => format!("Enemy {}", i + 1),
            },
            is_boss: true,
            archetype: None,
            spawned_at_ms: 0,
            role: None,
            hp: enemy.hp as i64,
            max_hp: enemy.hp,
            atk: enemy.atk,
            heal_power: 0.0,
            intervene: 0.0,
            attack_interval_ms: enemy.attack_interval_ms,
            next_action_at_ms: 0,
            alive: true,
            helm_power: 0.0,
            helm_cooldown_ms: u32::MAX,
            next_helm_at_ms: u32::MAX,
            helm_stack_bonus: 0.0,
            boots_power: 0.0,
            boots_cooldown_ms: u32::MAX,
            next_boots_at_ms: u32::MAX,
            damage_reduction: enemy.damage_reduction,
            block_chance: enemy.block_chance,
            evasion: enemy.evasion,
            increased_damage: enemy.increased_damage,
            crit_chance: enemy.crit_chance,
            crit_multiplier: enemy.crit_multiplier,
            splash: enemy.splash,
            // Only a REAL boss (this_unit_kind.is_some()) - a basic-
            // encounter mob (None) never gets this, regardless of stage.
            late_stage_damage_penalty_pct: if this_unit_kind.is_some() { late_stage_penalty } else { 0.0 },
            boss_focus_stacks: 0.0,
            boss_ability: this_unit_kind,
            next_ability_at_ms,
            boss_dynamic_power_mult,
            cthulhu_debuff_stacks: 0,
            cthulhu_debuff_expires_at_ms: 0,
            cthulhu_debuff_pct_per_stack: 0.0,
            cube_shred_stacks: 0,
            cube_shred_expires_at_ms: 0,
            damage_dealt_total: 0,
            level: 0,
            life_leech_pct: 0.0,
            leech_window_start_ms: 0,
            leech_gained_in_window: 0.0,
            skills: Vec::new(),
            skill_stacks: HashMap::new(),
            next_flicker_at_ms: u32::MAX,
            flicker_cooldown_ms: FLICKER_STRIKE_COOLDOWN_MS,
            has_celestial_conversion: false,
            wound_deal_leech_per_stack: 0.0,
            wound_deal_max_stacks: 0,
            wound_deal_duration_ms: 0,
            wound_deal_damage_dealt_debuff: 0.0,
            wound_deal_heal_received_debuff: 0.0,
            wound_deal_explosion_pct: 0.0,
            wound_deal_explosion_self_leech_pct: 0.0,
            wound_deal_explosion_extra_targets: 0,
            wound_deal_spreads_to_splash: false,
            contagion_chance: 0.0,
            gravechill_speed_debuff_pct: 0.0,
            plaguebearer_extra_targets: 0,
            wound_stacks: 0,
            wound_max_stacks: 0,
            wound_expires_at_ms: 0,
            wound_leech_per_stack: 0.0,
            wound_damage_dealt_debuff: 0.0,
            wound_heal_received_debuff: 0.0,
            wound_damage_taken_total: 0.0,
            next_bloodpact_at_ms: u32::MAX,
            bloodpact_last_fired_at_ms: 0,
            bloodpact_cooldown_ms: u32::MAX,
            bloodpact_uses_this_fight: 0,
            bloodpact_hp_cost_pct: 0.0,
            bloodpact_damage_mult: 1.0,
            bloodpact_martyrdom_shield_pct: 0.0,
            bloodpact_kill_refund_pct: 0.0,
            bloodpact_nonlethal_refund_pct: 0.0,
            bloodpact_bloodforblood_pct: 0.0,
            bloodpact_triage_pct: 0.0,
            bloodpact_finaloffering_min_prior_uses: u32::MAX,
            bloodpact_finaloffering_pct: 0.0,
            bloodpact_warlordsresolve_pct: 0.0,
            bloodpact_cleanslate_reset_chance: 0.0,
            bloodpact_secondwind_reset_chance: 0.0,
            shield_hp: 0.0,
            shield_expires_at_ms: 0,
            shield_reflect_pct: 0.0,
            shield_reflect_chance: 1.0,
            shield_reflect_requires_full_absorb: false,
            unyieldingroots_cycle_ms: 0,
            naturesward_dr_vs_boss_pct: 0.0,
            gambit_crit_per_missing_20pct: 0.0,
            deathdefiant_grace_ms: 0,
            deathdefiant_frozen_crit_bonus: 0.0,
            deathdefiant_frozen_crit_bonus_expires_at_ms: 0,
            bramble_reflect_pct: 0.0,
            poison_thorns_debuff_pct: 0.0,
            entangle_chance: 0.0,
            recent_attackers: Vec::new(),
            temp_damage_dealt_debuff: 0.0,
            temp_damage_dealt_debuff_expires_at_ms: 0,
            frenzy_strike_chance: 0.0,
            frenzy_extra_hits: 0,
            frenzy_bloodscent_threshold: 0.0,
            frenzy_dr_shred_pct: 0.0,
            frenzy_extra_dmg_pct: 0.0,
            frenzy_culling_threshold: 0.0,
            frenzy_heal_pct: 0.0,
            frenzy_shield_chance: 0.0,
            frenzy_undying_charges: 0,
            frenzy_chain_chance: 0.0,
            frenzy_chain_max_extra: 0,
            spike_barrier_reflect_pct: 0.0,
            aegis_shield_pct: 0.0,
            aegis_shield_duration_ms: AEGIS_SHIELD_DURATION_MS,
            aegis_rally_speed_pct: 0.0,
            aegis_extra_targets: 0,
            thornedhide_pct_per_stack: 0.0,
            thornedhide_stacks: 0,
            thornedhide_expires_at_ms: 0,
            thornedhide_debuff_pct_per_stack: 0.0,
            spike_retribution_chance: 0.0,
            spike_unyielding_chance: 0.0,
            block_damage_reduction_pct: BLOCK_DAMAGE_REDUCTION,
            stonewall_auto_block_hits: 0,
            hits_taken_this_fight: 0,
            stack_speed_per_stack: 0.0,
            stack_dmg_per_stack: 0.0,
            stack_avalanche_dmg_per_stack: 0.0,
            stack_crit_per_stack: 0.0,
            shatter_shred_pct: 0.0,
            overwhelm_shred_linger_ms: 0,
            crush_dr_threshold: 0.0,
            stack_splash_per_stack: 0.0,
            windfury_chance: 0.0,
            stack_shred_per_stack: 0.0,
            stack_speed_max_stacks: 0,
            stack_speed_duration_ms: 0,
            stack_speed_current: 0,
            stack_speed_expires_at_ms: 0,
            flowing_speed_per_stack: 0.0,
            flowing_crit_per_stack: 0.0,
            flowing_max_stacks: 0,
            flowing_duration_ms: 0,
            risingstorm_dmg_pct: 0.0,
            nervestrike_crit_mult_bonus: 0.0,
            vitalpoints_shred_per_stack: 0.0,
            eternalflow_bonus_stacks: 0,
            onehundredhands_bonus_stacks: 0,
            stormfront_splash_pct: 0.0,
            flowing_current: 0,
            flowing_expires_at_ms: 0,
            flowing_last_target: usize::MAX,
            chakra_of_many_pct: 0.0,
            chakra_of_light_pct: 0.0,
            chakraoflife_duration_ms: 0,
            chakraoflife_immune_until_ms: 0,
            next_chakraoflife_expiry_at_ms: u32::MAX,
            own_mark_crit_chance: 0.0,
            own_mark_crit_mult: 0.0,
            own_mark_low_hp_dmg: 0.0,
            own_mark_ally_crit_chance: 0.0,
            own_mark_ally_dmg_pct: 0.0,
            own_mark_ally_crit_mult: 0.0,
            own_mark_spread_count: 0,
            killzone_threshold: 0.0,
            cleankill_remark_chance: 0.0,
            huntersreward_heal_pct: 0.0,
            own_curse_dmg_taken: 0.0,
            own_curse_spread_count: 0,
            own_doom_detonate_pct: 0.0,
            own_curse_heal_reduction_pct: 0.0,
            own_curse_spread_bonus_pct: 0.0,
            own_soul_stone_max: 0,
            own_cursed_blood_target_count: 0,
            own_dreadfuldeath_shred_pct: 0.0,
            own_apocalypse_splash_pct: 0.0,
            has_applied_mark_this_fight: false,
            mark_source_id: None,
            mark_crit_chance_bonus: 0.0,
            mark_crit_multiplier_bonus: 0.0,
            mark_low_hp_damage_bonus: 0.0,
            mark_ally_crit_chance_bonus: 0.0,
            mark_ally_dmg_bonus: 0.0,
            mark_ally_crit_multiplier_bonus: 0.0,
            curse_dmg_taken_bonus: 0.0,
            soul_stones: 0,
            soul_stone_uses_this_fight: 0,
            curse_expires_at_ms: u32::MAX,
            next_curse_expiry_at_ms: u32::MAX,
            curse_damage_taken_total: 0.0,
            curse_detonate_pct: 0.0,
            curse_source_id: None,
            curse_heal_reduction_bonus: 0.0,
            fel_rush_speed_bonus: 0.0,
            fel_rush_duration_ms: 0,
            ravage_stack_pct: 0.0,
            fel_rush_stacks: 0,
            fel_rush_expires_at_ms: 0,
            early_fight_speed_bonus_pct: 0.0,
            early_fight_speed_window_end_ms: 0,
            flicker_frenzy_speed_bonus: 0.0,
            unrelenting_duration_bonus_ms: 0,
            adrenaline_crit_mult_bonus: 0.0,
            chainreaper_heal_pct: 0.0,
            deathspiral_heal_pct: 0.0,
            insatiable_extend_chance: 0.0,
            secondheartbeat_chance: 0.0,
            overflowvessel_shield_pct: 0.0,
            flicker_frenzy_expires_at_ms: 0,
            endless_thirst_cap_bonus: 0.0,
            endless_thirst_uncapped: false,
            endless_thirst_expires_at_ms: 0,
            reapers_momentum_per_kill: 0,
            reapers_momentum_banked: 0,
            guardian_spirit_charges: 0,
            guardian_spirit_heal_pct: 0.0,
            guardian_spirit_save_dr_pct: 0.0,
            guardian_spirit_save_heal_power_pct: 0.0,
            verdantburst_charges: 0,
            temp_heal_power_bonus: 0.0,
            temp_heal_power_bonus_expires_at_ms: 0,
            eternallight_bonus_pct: 0.0,
            temp_damage_reduction_bonus: 0.0,
            temp_damage_reduction_bonus_expires_at_ms: 0,
            overflow_grace_shield_pct: 0.0,
            overflow_grace_shield_duration_ms: 0,
            overflow_grace_shield_dr_pct: 0.0,
            heal_crit_bonus_mult: 0.0,
            heal_crit_chance_bonus: 0.0,
            heal_crit_splash_pct: 0.0,
            grace_lowest_ally_bonus_pct: 0.0,
            prayer_chance: 0.0,
            prayer_bounce_targets: 0,
            prayer_bounce_value_pct: 0.0,
            unbroken_prayer_chance: 0.0,
            divine_favor_shield_pct: 0.0,
            divine_favor_shield_duration_ms: 0,
            healing_touch_pct: 0.0,
            crit_shield_max_hp_pct: 0.0,
            soul_harvest_heal_pct: 0.0,
            darkritual_dmg_pct: 0.0,
            eternal_hunger_shield_pct: 0.0,
            divine_shield_amount_pct: 0.0,
            divine_shield_cooldown_ms: u32::MAX,
            next_divine_shield_at_ms: u32::MAX,
            consecration_shield_pct: 0.0,
            consecration_shield_duration_ms: 0,
            communion_heal_power_pct: 0.0,
            purify_dmg_debuff_pct: 0.0,
            lastjudgment_skip_chance: 0.0,
            smite_heal_pct: 0.0,
            smite_zealotry_bonus_pct: 0.0,
            smite_extra_targets: 0,
            zealotry_martyrscall_bonus_pct: 0.0,
            zealotry_risingfervor_pct_per_ally: 0.0,
            zealotry_guardianswrath_speed_pct: 0.0,
            zealotry_guardianswrath_speed_bonus: 0.0,
            zealotry_guardianswrath_expires_at_ms: 0,
            smite_judgment_bonus_pct: 0.0,
            judgment_threshold: 0.0,
            smite_holyfire_dmg_pct: 0.0,
            purgingflame_heal_reduction_pct: 0.0,
            temp_heal_reduction_pct: 0.0,
            temp_heal_reduction_expires_at_ms: 0,
            executionersblessing_heal_pct: 0.0,
            wrathoftheheavens_chance: 0.0,
            unbreakable_faith_heal_pct: 0.0,
            eternalvow_shield_chance: 0.0,
            graciousburden_heal_pct: 0.0,
            bondeddevotion_dr_pct: 0.0,
            bondeddevotion_duration_ms: 0,
            attack_speed_pct: 0.0,
            speed_overflow_dmg_pct: 0.0,
            speed_overflow_crit_pct: 0.0,
            speed_overflow_threshold: 0.0,
            twin_strike_chance: 0.0,
            twin_strike_dmg_pct: 0.0,
            finiteloop_max_repeats: 0,
            doubletap_max_repeats: 0,
            in_splash_resolution: false,
            own_pack_instinct_evasion_pct: 0.0,
            own_symbiosis_dr_pct: 0.0,
            sharedstrength_extra_targets: 0,
            templeguardian_heal_pct: 0.0,
            next_templeguardian_heal_at_ms: 0,
            lingering_effect_pct: 0.0,
            lingering_dots: Vec::new(),
            next_lingering_tick_at_ms: u32::MAX,
            seedoflife_shield_pct: 0.0,
            wildheart_self_heal_pct: 0.0,
            wildinstinct_dr_pct: 0.0,
            wildroar_charges: 0,
            naturesembrace_heal_targets: 0,
            thickhide_cycle_ms: 0,
            next_thickhide_cleanse_at_ms: 0,
            thickhide_target_count: 0,
            fire_damage_pct: 0.0,
            cold_damage_pct: 0.0,
            chaos_damage_pct: 0.0,
            lightning_damage_pct: 0.0,
            divine_damage_pct: 0.0,
            fire_dr_debuff: Vec::new(),
            cold_evasion_debuff: Vec::new(),
            chaos_block_debuff: Vec::new(),
            lightning_dmg_taken: Vec::new(),
            divine_heal_reduction: Vec::new(),
            fire_dr_buff: Vec::new(),
            cold_evasion_buff: Vec::new(),
            chaos_block_buff: Vec::new(),
            divine_heal_power_buff: Vec::new(),
            block_overflow_dmg_rate: 0.0,
            evasion_overflow_dmg_rate: 0.0,
            elemental_overflow_dmg_bonus: 0.0,
            elemental_overflow_dmg_bonus_expires_at_ms: 0,
            volley_dmg_per_target_pct: 0.0,
            splash_target_dmg_bonus: 0.0,
            exploit_weakness_crit_mult_pct: 0.0,
            exploit_weakness_threshold: 0.0,
            weakpoint_crit_chance_pct: 0.0,
            nightstalker_evasion_pct: 0.0,
            assassinate_crit_mult_bonus: 0.0,
            silentblade_evasion_pct: 0.0,
            fadeaway_duration_bonus_ms: 0,
            backstab_dmg_pct: 0.0,
            backstab_pending_dmg_pct: 0.0,
            smokescreen_evasion_pct: 0.0,
            markedfordeath_hits_remaining: 0,
            markedfordeath_hit_count: 0,
            finalcut_speed_pct: 0.0,
            empoweredbolt_invested: false,
            empoweredbolt_crit_mult_bonus: 0.0,
            volatilemagic_splash_pct: 0.0,
            arcaneinstability_threshold: 0.0,
            arcaneinstability_bonus_pct: 0.0,
            premeditation_refund_chance: 0.0,
            stack_evasion_per_stack: 0.0,
            huntersinstinct_crit_vs_boss_pct: 0.0,
            silentkiller_dmg_pct: 0.0,
            has_hit_boss_this_fight: false,
            assassinate_charges: 0,
            dark_communion_pct: 0.0,
            compassion_prioritize_lowest: false,
            compassion_dr_pct: 0.0,
            covenant_pct: 0.0,
            unbreakablebond_dr_pct: 0.0,
            vigor_heal_pct: 0.0,
            vengefulblood_shield_pct: 0.0,
            secondgale_duration_ms: 0,
            temp_reckless_immunity_expires_at_ms: 0,
            reckless_penalty_offset: 0.0,
            lastlaugh_crit_bonus: false,
            lastlaugh_crit_mult: false,
            ragefueled_speed_pct: 0.0,
            retaliation_chance: 0.0,
            retaliation_dmg_pct: 0.0,
            retaliation_heal_pct: 0.0,
            retaliation_laststand_bonus: 0.0,
            grudge_pct_per_hit: 0.0,
            grudge_hit_counts: Vec::new(),
            retaliation_crit_bonus: 0.0,
            retaliation_payback_threshold: 0.0,
            force_crit_next_hit: false,
            retaliation_surge_pct: 0.0,
            hardened_stacks: 0,
            hardened_pct_per_stack: 0.0,
            retaliation_secondwind_threshold: 0.0,
            laststand_defiance_pct: 0.0,
            laststand_berserkvigor_pct: 0.0,
            immovable_crit_dr_pct: 0.0,
            reserves_heal_received_pct: 0.0,
            unbroken_ignore_evasion_pct: 0.0,
            unbroken_crippling_grip_dr_pct: 0.0,
            unyieldingspirit_threshold: 0.0,
            temp_evasion_debuff: 0.0,
            temp_evasion_debuff_expires_at_ms: 0,
            frostnova_evasion_debuff_pct: 0.0,
            frostnova_duration_ms: 0,
            absolutezero_threshold: 0.0,
            staticfield_speed_debuff_pct: 0.0,
            temp_attack_speed_debuff: 0.0,
            temp_attack_speed_debuff_expires_at_ms: 0,
            infernalpact_heal_pct: 0.0,
            stormcaller_extra_targets: 0,
            piercing_shots_crit_chance_bonus: 0.0,
            windpierce_splash_crit_pct: 0.0,
            armorbreaker_dr_shred_pct: 0.0,
            scorchedearth_dmg_debuff_pct: 0.0,
            truestrike_primary_crit_pct: 0.0,
            stormofarrows_extra_targets: 0,
            widerburst_extra_targets: 0,
            inner_focus_heal_pct: 0.0,
            inner_focus_meditation_bonus: 0.0,
            inner_focus_chiburst_pct: 0.0,
            inner_focus_serenity_dr_pct: 0.0,
            risingtide_heal_power_pct: 0.0,
            widecircle_extra_targets: 0,
            harmonize_dr_pct: 0.0,
            serenity_dr_duration_ms: 0,
            clarity_triggers_on_block: false,
            evade_counter_chance: 0.0,
            evade_counter_last_fired_at_ms: 0,
            temp_party_attack_speed_bonus: 0.0,
            temp_party_attack_speed_bonus_expires_at_ms: 0,
            temp_party_increased_damage_bonus: 0.0,
            temp_party_increased_damage_bonus_expires_at_ms: 0,
            temp_party_damage_reduction_bonus: 0.0,
            temp_party_damage_reduction_bonus_expires_at_ms: 0,
            low_hp_party_dr_pct: 0.0,
            low_hp_party_dr_threshold: 0.0,
            warlord_party_dmg_pct: 0.0,
            warcry_party_speed_pct: 0.0,
            neverending_invested: false,
            bloodlust_stack_expiries: Vec::new(),
            opportunist_guaranteed_hits: 0,
            hits_landed_this_fight: 0,
            ambush_dr_cut_pct: 0.0,
            openingmove_cooldown_ms: 0,
            next_openingmove_at_ms: 0,
            coldsteel_pass_chance: 0.0,
            coldsteel_pending: false,
            coldsteel_pass_chance_pending: 0.0,
            coldsteel_ambush_pct_pending: 0.0,
            predator_dmg_taken_pct: 0.0,
            predator_dmg_taken_bonus: 0.0,
            predator_expires_at_ms: 0,
            cutthroat_low_hp_dmg_pct: 0.0,
            vanish_evasion_pct: 0.0,
            temp_evasion_buff: 0.0,
            temp_evasion_buff_expires_at_ms: 0,
            vanishingshot_crit_pct: 0.0,
            temp_crit_chance_buff: 0.0,
            temp_crit_chance_buff_expires_at_ms: 0,
            fleetingshadow_speed_pct: 0.0,
        });
    }

    // Dragon's aura: everyone in the party attacks 50% slower - applied
    // once here (a bigger attack_interval_ms IS slower), not re-applied
    // per-hit, so it's a flat fight-wide handicap from the first turn on.
    // Checked across ALL enemy units (not a single fight-wide `boss_kind`
    // anymore) so a stage-50+ 2-boss fight still applies it if EITHER
    // boss happens to be the Dragon - "never the same" kind guarantees
    // at most one Dragon among them regardless, so this can't double-apply.
    if units.iter().any(|u| u.boss_ability == Some(BossKind::Dragon)) {
        for u in units.iter_mut() {
            if !u.is_boss {
                u.attack_interval_ms = (u.attack_interval_ms as f64 * DRAGON_SLOW_MULT).round() as u32;
            }
        }
    }
    // Fire Demon's aura: -50% on every heal amount, fight-wide - a plain
    // multiplier applied at each of the 3 places healing actually gets
    // computed below (the unified attack's heal share, apply_heal_splash,
    // Boots' self-heal), not a per-unit stat, since it's the SAME aura
    // over the whole battlefield regardless of who's healing. Same
    // any-boss check as Dragon's aura above.
    let heal_mult = if units.iter().any(|u| u.boss_ability == Some(BossKind::FireDemon)) { FIRE_DEMON_HEAL_MULT } else { 1.0 };

    // Cleric's Blessed Resilience/Sanctuary/Radiant Aegis (party-wide
    // grants) - `combat_*` getters are strictly per-character with no
    // view of party siblings, so this needed its own new plumbing rather
    // than reusing an existing getter. Applied once here, after every
    // `CombatSimUnit` exists, by summing every alive Cleric's own
    // investment and adding the total to EVERY party member's unit,
    // Cleric included - "party" reads as inclusive here, unlike
    // Martyrdom's shield which explicitly names "your lowest-HP ally".
    // Same aggregate-then-mutate shape as the Dragon/FireDemon auras
    // above, just player-driven instead of boss-driven. Multiple Clerics
    // in the same party simply stack (sum), same as any other source.
    let mut party_max_hp_pct = 0.0;
    let mut party_damage_reduction_pct = 0.0;
    let mut party_evasion_pct = 0.0;
    let mut party_attack_speed_pct = 0.0;
    for u in units.iter() {
        if u.is_boss {
            continue;
        }
        if let Some(c) = characters.get(&u.id) {
            if c.archetype == Archetype::Cleric {
                party_max_hp_pct += c.passive_node_magnitude("resilience");
                // Warding Prayer's "vs bosses specifically" clause
                // approximated as a flat party DR bonus, same construction-
                // time-aggregate simplification as Paladin's Hallowed
                // Ground.
                party_damage_reduction_pct +=
                    c.passive_node_magnitude("sanctuary") + c.passive_node_magnitude("consecratedearth") + c.passive_node_magnitude("wardingprayer");
                party_evasion_pct += c.passive_node_magnitude("radiantaegis") + c.passive_node_magnitude("windsofgrace");
                party_attack_speed_pct += c.passive_node_magnitude("swiftblessing");
                // Haloed Steps - Radiant Aegis's own OVERFLOW (past its
                // 75% cap) converts to party DR.
                let haloedsteps_pct = c.passive_node_magnitude("haloedsteps");
                if haloedsteps_pct > 0.0 {
                    party_damage_reduction_pct += c.passive_overflow_bonus().evasion * haloedsteps_pct;
                }
            }
            // Paladin's Vow of Protection - the same party-wide broadcast
            // shape as Cleric's Sanctuary, just a different source archetype
            // summing into the SAME `party_damage_reduction_pct` total.
            if c.archetype == Archetype::Paladin {
                // Beacon of Light extends Vow of Protection's own
                // magnitude directly. Hallowed Ground's "vs bosses
                // specifically" clause is approximated as a flat party DR
                // bonus here (this aggregate is construction-time/fight-
                // wide, with no per-attacker-type distinction available at
                // the point it's summed) rather than left unimplemented.
                party_damage_reduction_pct += c.passive_node_magnitude("vowofprotection") + c.passive_node_magnitude("beaconoflight") + c.passive_node_magnitude("hallowedground");
            }
        }
    }
    if party_max_hp_pct > 0.0 || party_damage_reduction_pct > 0.0 || party_evasion_pct > 0.0 || party_attack_speed_pct > 0.0 {
        for u in units.iter_mut() {
            if u.is_boss {
                continue;
            }
            if party_max_hp_pct > 0.0 {
                let bonus_hp = (u.max_hp as f64 * party_max_hp_pct).round() as i64;
                u.max_hp = (u.max_hp as i64 + bonus_hp).max(1) as u64;
                u.hp += bonus_hp;
            }
            u.damage_reduction += party_damage_reduction_pct;
            u.evasion += party_evasion_pct;
            if party_attack_speed_pct > 0.0 {
                u.attack_interval_ms = (u.attack_interval_ms as f64 / (1.0 + party_attack_speed_pct)).round().max(200.0) as u32;
            }
        }
    }

    let mut events = Vec::new();
    let mut rolls: Vec<RollEvent> = Vec::new();
    let mut rng = rand::thread_rng();

    // Warlock's Cursed Blood (2026-08-17, repurposed) - immediately curses
    // N random enemies the instant a fight starts, at ms 0, before any hit
    // lands - same curse-application field set `apply_first_hit_mark`'s
    // primary-target branch uses (silent, no CombatEvent, matching that
    // function's own convention). Doesn't touch `has_applied_mark_this_fight`,
    // so a Warlock with real Curse of Weakness investment still gets their
    // normal first-hit application too (see
    // `own_cursed_blood_target_count`'s doc for why these aren't mutually
    // exclusive - re-cursing an already-cursed target here is a harmless
    // overwrite). "Bypasses defenses" (per the node's own tooltip) isn't
    // extra logic to write - a direct field write here, with no evasion
    // roll or `resolve_hit`/DR check anywhere in this loop, is already
    // unconditional and unmitigated by construction, unlike the normal
    // curse cast which only fires once a hit has actually landed.
    let cursed_blood_casters: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.own_cursed_blood_target_count > 0).map(|(i, _)| i).collect();
    for caster_idx in cursed_blood_casters {
        let count = units[caster_idx].own_cursed_blood_target_count;
        let bonus = units[caster_idx].own_curse_dmg_taken;
        let heal_reduction = units[caster_idx].own_curse_heal_reduction_pct;
        let doom_pct = units[caster_idx].own_doom_detonate_pct;
        let caster_id = units[caster_idx].id.clone();
        let mut enemies: Vec<usize> = units.iter().enumerate().filter(|(_, u)| u.is_boss && u.alive).map(|(i, _)| i).collect();
        for _ in 0..count.min(enemies.len() as u32) {
            let pick = rng.gen_range(0..enemies.len());
            let enemy_idx = enemies.remove(pick);
            units[enemy_idx].curse_dmg_taken_bonus = bonus;
            units[enemy_idx].curse_heal_reduction_bonus = heal_reduction;
            units[enemy_idx].curse_source_id = Some(caster_id.clone());
            if doom_pct > 0.0 {
                units[enemy_idx].curse_expires_at_ms = DOOM_CURSE_DURATION_MS;
                units[enemy_idx].next_curse_expiry_at_ms = DOOM_CURSE_DURATION_MS;
                units[enemy_idx].curse_damage_taken_total = 0.0;
                units[enemy_idx].curse_detonate_pct = doom_pct;
            }
            grant_soul_stone(&mut units[caster_idx]);
        }
    }

    // A unit's normal turn (attack/heal) and its helm/boots skill procs
    // (if any gear is equipped) all tick on their OWN independent
    // schedules rather than sharing one clock - see CombatSimUnit. Each
    // loop iteration finds whichever of ALL those clocks (across every
    // alive unit) comes due soonest and resolves just that one thing.
    #[derive(Clone, Copy)]
    enum NextEvent {
        Turn(usize),
        Helm(usize),
        Boots(usize),
        BossAbility(usize),
        FlickerStrike(usize),
        DivineShield(usize),
        LingeringTick(usize),
        CurseExpiry(usize),
        ChakraOfLifeExpiry(usize),
    }

    // Which player the real boss is currently focus-targeting (see the
    // survivability-sort below) - tracked across iterations so
    // `boss_focus_stacks` knows whether the NEXT hit is a continuation
    // (stack another +10%) or a fresh focus (reset to 10%). `None` until
    // the boss's first attack, and permanently for a basic encounter
    // (which never sets it, so its targeting stays the old random pick).
    let mut boss_focus_target: Option<usize> = None;
    // How many adds Lich has summoned so far this fight - see
    // `LICH_MAX_ADDS`'s own doc for the cap this counts against.
    let mut lich_summon_count: u32 = 0;

    loop {
        let boss_alive = units.iter().any(|u| u.is_boss && u.alive);
        let any_player_alive = units.iter().any(|u| !u.is_boss && u.alive);
        if !boss_alive || !any_player_alive {
            break;
        }

        let mut best: Option<(u32, NextEvent)> = None;
        for (i, u) in units.iter().enumerate() {
            if !u.alive {
                continue;
            }
            if best.is_none() || u.next_action_at_ms < best.unwrap().0 {
                best = Some((u.next_action_at_ms, NextEvent::Turn(i)));
            }
            if !u.is_boss {
                if u.next_helm_at_ms < best.unwrap().0 {
                    best = Some((u.next_helm_at_ms, NextEvent::Helm(i)));
                }
                if u.next_boots_at_ms < best.unwrap().0 {
                    best = Some((u.next_boots_at_ms, NextEvent::Boots(i)));
                }
                if u.next_flicker_at_ms < best.unwrap().0 {
                    best = Some((u.next_flicker_at_ms, NextEvent::FlickerStrike(i)));
                }
                if u.next_divine_shield_at_ms < best.unwrap().0 {
                    best = Some((u.next_divine_shield_at_ms, NextEvent::DivineShield(i)));
                }
            } else if u.next_ability_at_ms < best.unwrap().0 {
                best = Some((u.next_ability_at_ms, NextEvent::BossAbility(i)));
            }
            // Lingering Effect - unlike Helm/Boots/FlickerStrike/Divine
            // Shield (player-only mechanics), a DoT can be ticking on
            // EITHER side (a player's own Lingering Effect investment
            // lands on whichever boss they hit), so this check sits
            // outside the is_boss split above.
            if u.next_lingering_tick_at_ms < best.unwrap().0 {
                best = Some((u.next_lingering_tick_at_ms, NextEvent::LingeringTick(i)));
            }
            // Warlock's Doom - same "can land on either side, so it sits
            // outside the is_boss split" reasoning as Lingering Effect,
            // though in practice a curse only ever lands on an enemy
            // (nothing but a player ever invests in Curse of Weakness).
            if u.next_curse_expiry_at_ms < best.unwrap().0 {
                best = Some((u.next_curse_expiry_at_ms, NextEvent::CurseExpiry(i)));
            }
            // Monk's Chakra of Life - same "can land on either side"
            // reasoning, though in practice only ever a player (nothing
            // grants a boss this passive).
            if u.next_chakraoflife_expiry_at_ms < best.unwrap().0 {
                best = Some((u.next_chakraoflife_expiry_at_ms, NextEvent::ChakraOfLifeExpiry(i)));
            }
        }
        let Some((at_ms, next_event)) = best else {
            break;
        };
        if at_ms > MAX_FIGHT_DURATION_MS {
            break;
        }

        let actor_idx = match next_event {
            NextEvent::Turn(i) => i,
            NextEvent::Helm(actor_idx) => {
                // Another stack - permanently raises this unit's base dps
                // for the rest of the fight (see `helm_stack_bonus`'s
                // doc), realized as a bonus on their NEXT actual attack
                // (below), not as its own hit here. Ticks regardless of
                // role/whether an enemy is currently targetable - it's a
                // clock, not a proc against something.
                units[actor_idx].helm_stack_bonus += units[actor_idx].helm_power;
                units[actor_idx].next_helm_at_ms += units[actor_idx].helm_cooldown_ms;
                continue;
            }
            NextEvent::Boots(actor_idx) => {
                // Self-heal - simpler and more predictable than routing
                // through the party's lowest-HP ally like Support's heal
                // does; reads fine as "boots keep their own wearer up".
                let u = &units[actor_idx];
                if u.hp < u.max_hp as i64 {
                    let heal = units[actor_idx].boots_power * rng.gen_range(0.85..1.15) * heal_mult;
                    apply_heal(&mut units, actor_idx, actor_idx, heal, at_ms, &mut events, &mut rng);
                }
                units[actor_idx].next_boots_at_ms += units[actor_idx].boots_cooldown_ms;
                continue;
            }
            NextEvent::FlickerStrike(actor_idx) => {
                // Vampiric Frenzy's own per-unit cadence, not the bare
                // constant - fixes a real bug where the discount used to
                // only ever apply to the FIRST cast (only the initial
                // `next_flicker_at_ms` at construction was discounted,
                // every reschedule after that used the raw constant,
                // silently reverting to the full 5s cadence forever
                // after).
                units[actor_idx].next_flicker_at_ms += units[actor_idx].flicker_cooldown_ms;
                ArchetypeSkill::FlickerStrike.on_periodic_tick(&mut units, actor_idx, at_ms, &mut events, &mut rolls, &mut rng);
                continue;
            }
            NextEvent::DivineShield(actor_idx) => {
                // Divine Shield - shields whoever currently has the
                // lowest HP in the party (self included, unlike the
                // unified heal share's "never itself while anyone else
                // needs it" convention - a shield isn't wasted on someone
                // already topped off the way a heal would be, so there's
                // no reason to exclude the caster).
                events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Divine Shield".to_string() });
                let target_idx = units
                    .iter()
                    .enumerate()
                    .filter(|(_, u)| !u.is_boss && u.alive)
                    .min_by_key(|(_, u)| u.hp)
                    .map(|(i, _)| i)
                    .unwrap_or(actor_idx);
                let amount = units[actor_idx].max_hp as f64 * units[actor_idx].divine_shield_amount_pct;
                grant_shield(&mut units, actor_idx, target_idx, amount, at_ms, DIVINE_SHIELD_DURATION_MS, &mut events);
                // Consecration - a smaller shield to everyone ELSE in the
                // party (whoever didn't just get the primary shield).
                let consecration_pct = units[actor_idx].consecration_shield_pct;
                if consecration_pct > 0.0 {
                    let consecration_amount = units[actor_idx].max_hp as f64 * consecration_pct;
                    let consecration_duration = units[actor_idx].consecration_shield_duration_ms;
                    let others: Vec<usize> = units.iter().enumerate().filter(|(i, u)| !u.is_boss && u.alive && *i != target_idx).map(|(i, _)| i).collect();
                    let communion_pct = units[actor_idx].communion_heal_power_pct;
                    for other_idx in others {
                        grant_shield(&mut units, actor_idx, other_idx, consecration_amount, at_ms, consecration_duration, &mut events);
                        // Communion - Consecration also grants a temporary
                        // party-wide healing-power buff, off the same
                        // shared field/read-site every other
                        // `temp_heal_power_bonus` grant already uses.
                        if communion_pct > 0.0 {
                            units[other_idx].temp_heal_power_bonus = communion_pct;
                            units[other_idx].temp_heal_power_bonus_expires_at_ms = at_ms + consecration_duration;
                        }
                    }
                    if communion_pct > 0.0 {
                        units[actor_idx].temp_heal_power_bonus = communion_pct;
                        units[actor_idx].temp_heal_power_bonus_expires_at_ms = at_ms + consecration_duration;
                    }
                }
                units[actor_idx].next_divine_shield_at_ms += units[actor_idx].divine_shield_cooldown_ms;
                continue;
            }
            NextEvent::LingeringTick(target_idx) => {
                tick_lingering_dots(&mut units, target_idx, at_ms, &mut events, &mut rolls, &mut rng);
                continue;
            }
            NextEvent::CurseExpiry(target_idx) => {
                // Warlock's Doom - the curse detonates for a burst of true
                // damage (no crit/evasion/mitigation roll - a detonation,
                // not an attack, same "true damage" convention as
                // Lingering Effect's own damage-flavor tick) equal to
                // `curse_detonate_pct` of whatever real damage was banked
                // while the curse was active. The curse itself is
                // consumed - nothing re-applies it, matching "Doom
                // detonates WHEN it expires" as a one-time payoff rather
                // than a permanent debuff once Doom is invested.
                if units[target_idx].alive {
                    let detonation = units[target_idx].curse_damage_taken_total * units[target_idx].curse_detonate_pct;
                    // Monk's Chakra of Life - true damage still respects full immunity.
                    if detonation > 0.0 && at_ms > units[target_idx].chakraoflife_immune_until_ms {
                        let source_id = units[target_idx].curse_source_id.clone().unwrap_or_default();
                        let hit_id = next_hit_id();
                        let penalized = apply_late_stage_penalty(&units, target_idx, detonation, at_ms, hit_id, &source_id, &mut rolls);
                        let final_damage = penalized.round().max(0.0) as i64;
                        if final_damage > 0 {
                            let new_hp = (units[target_idx].hp - final_damage).max(0);
                            units[target_idx].hp = new_hp;
                            let target_id = units[target_idx].id.clone();
                            events.push(CombatEvent::SkillCast { at_ms, unit: source_id.clone(), skill: "Doom".to_string() });
                            events.push(CombatEvent::Attack {
                                at_ms,
                                attacker: source_id.clone(),
                                target: target_id.clone(),
                                damage: final_damage.max(0) as u64,
                                unmitigated_damage: final_damage.max(0) as u64,
                                target_hp_after: new_hp as u64,
                                is_crit: false,
                                evaded: false,
                                hit_id,
                            });
                            if new_hp == 0 {
                                units[target_idx].alive = false;
                                events.push(CombatEvent::Defeat { at_ms, unit: target_id });
                                if let Some(source_idx) = units.iter().position(|u| u.id == source_id) {
                                    fire_on_kill(&mut units, source_idx, at_ms, &mut events, &mut rolls, &mut rng);
                                }
                            } else {
                                // Dreadful Death - the detonation also shreds
                                // the target's DR for a few seconds.
                                let source_idx = units.iter().position(|u| u.id == source_id);
                                let dreadfuldeath_pct = source_idx.map(|i| units[i].own_dreadfuldeath_shred_pct).unwrap_or(0.0);
                                if dreadfuldeath_pct > 0.0 {
                                    units[target_idx].temp_damage_reduction_bonus = -dreadfuldeath_pct;
                                    units[target_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + DREADFUL_DEATH_DEBUFF_DURATION_MS;
                                }
                                // Apocalypse - the detonation also splashes to
                                // nearby enemies at a fraction of value (of
                                // the raw, pre-late-stage-penalty detonation -
                                // each splash instance gets its own penalty
                                // applied below, same as the primary hit did).
                                let apocalypse_pct = source_idx.map(|i| units[i].own_apocalypse_splash_pct).unwrap_or(0.0);
                                if apocalypse_pct > 0.0 {
                                    let target_is_boss = units[target_idx].is_boss;
                                    let splash_damage = detonation * apocalypse_pct;
                                    let others: Vec<usize> =
                                        units.iter().enumerate().filter(|(i, u)| *i != target_idx && u.is_boss == target_is_boss && u.alive).map(|(i, _)| i).collect();
                                    for other_idx in others {
                                        // Monk's Chakra of Life - true damage still respects full immunity.
                                        if splash_damage <= 0.0 || at_ms <= units[other_idx].chakraoflife_immune_until_ms {
                                            continue;
                                        }
                                        let other_hit_id = next_hit_id();
                                        let other_penalized = apply_late_stage_penalty(&units, other_idx, splash_damage, at_ms, other_hit_id, &source_id, &mut rolls);
                                        let other_final = other_penalized.round().max(0.0) as i64;
                                        if other_final <= 0 {
                                            continue;
                                        }
                                        let other_new_hp = (units[other_idx].hp - other_final).max(0);
                                        units[other_idx].hp = other_new_hp;
                                        let other_id = units[other_idx].id.clone();
                                        events.push(CombatEvent::Attack {
                                            at_ms,
                                            attacker: source_id.clone(),
                                            target: other_id.clone(),
                                            damage: other_final.max(0) as u64,
                                            unmitigated_damage: other_final.max(0) as u64,
                                            target_hp_after: other_new_hp as u64,
                                            is_crit: false,
                                            evaded: false,
                                            hit_id: other_hit_id,
                                        });
                                        if other_new_hp == 0 {
                                            units[other_idx].alive = false;
                                            events.push(CombatEvent::Defeat { at_ms, unit: other_id });
                                            if let Some(source_idx) = source_idx {
                                                fire_on_kill(&mut units, source_idx, at_ms, &mut events, &mut rolls, &mut rng);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                units[target_idx].curse_expires_at_ms = u32::MAX;
                units[target_idx].next_curse_expiry_at_ms = u32::MAX;
                units[target_idx].curse_dmg_taken_bonus = 0.0;
                units[target_idx].curse_damage_taken_total = 0.0;
                continue;
            }
            NextEvent::ChakraOfLifeExpiry(target_idx) => {
                // Monk's Chakra of Life - the immunity window granted by
                // the "would-kill" branch in `apply_hit` has run out.
                // Unconditional death, no attacker credited (confirmed: a
                // timer death, not a normal kill) - `fire_on_kill`
                // deliberately not called, unlike every other Defeat site
                // in this file.
                units[target_idx].next_chakraoflife_expiry_at_ms = u32::MAX;
                if units[target_idx].alive {
                    units[target_idx].hp = 0;
                    units[target_idx].alive = false;
                    let target_id = units[target_idx].id.clone();
                    events.push(CombatEvent::Defeat { at_ms, unit: target_id });
                    trigger_doom_on_death(&mut units, target_idx, at_ms, &mut events, &mut rolls, &mut rng);
                }
                continue;
            }
            NextEvent::BossAbility(actor_idx) => {
                match units[actor_idx].boss_ability {
                    Some(BossKind::Cthulhu) => {
                        // Purple bubble rework (2026-08-16, replacing the
                        // old permanent single-target -90% damage debuff) -
                        // now a stacking debuff on roughly HALF the party
                        // (ceiling division - the odd one out on a 5-player
                        // party gets bubbled too), each stack worth
                        // `CTHULHU_DEBUFF_BASE_PCT_PER_STACK` scaled by his
                        // own dynamic boss power (see
                        // `boss_dynamic_power_mult`'s doc), floored at 90%
                        // total reduction to both damage AND healing dealt
                        // (see `resolve_hit`/`apply_heal`) - lasts
                        // `CTHULHU_DEBUFF_DURATION_MS`, recast every
                        // `CTHULHU_DEBUFF_CADENCE_MS`.
                        events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Bubble".to_string() });
                        let alive_players: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
                        if !alive_players.is_empty() {
                            let mut candidates = alive_players.clone();
                            candidates.shuffle(&mut rng);
                            let target_count = (alive_players.len() + 1) / 2;
                            let pct_per_stack = CTHULHU_DEBUFF_BASE_PCT_PER_STACK * units[actor_idx].boss_dynamic_power_mult;
                            for &idx in candidates.iter().take(target_count) {
                                if at_ms > units[idx].cthulhu_debuff_expires_at_ms {
                                    units[idx].cthulhu_debuff_stacks = 0;
                                }
                                units[idx].cthulhu_debuff_stacks += 1;
                                units[idx].cthulhu_debuff_expires_at_ms = at_ms + CTHULHU_DEBUFF_DURATION_MS;
                                units[idx].cthulhu_debuff_pct_per_stack = pct_per_stack;
                            }
                        }
                        units[actor_idx].next_ability_at_ms += CTHULHU_DEBUFF_CADENCE_MS;
                    }
                    Some(BossKind::Lich) => {
                        // 5 more weak adds (stats scaled off the Lich's
                        // own, not the world stage - simplest source of
                        // truth already on hand), staggered a couple
                        // hundred ms apart so they don't all act on the
                        // exact same instant. Capped overall (LICH_MAX_ADDS)
                        // so "every 2 seconds" for a long fight can't
                        // spiral into hundreds of units.
                        events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Raise Dead".to_string() });
                        let to_summon = LICH_ADDS_PER_SUMMON.min(LICH_MAX_ADDS.saturating_sub(lich_summon_count));
                        let boss_id = units[actor_idx].id.clone();
                        let boss_max_hp = units[actor_idx].max_hp;
                        let boss_atk = units[actor_idx].atk;
                        for j in 0..to_summon {
                            units.push(CombatSimUnit {
                                id: add_unit_id(&boss_id, (lich_summon_count + j) as usize),
                                display_name: "Skeleton".to_string(),
                                is_boss: true,
                                archetype: None,
                                spawned_at_ms: at_ms,
                                role: None,
                                hp: (boss_max_hp / 10).max(20) as i64,
                                max_hp: (boss_max_hp / 10).max(20),
                                atk: (boss_atk / 5).max(3),
                                heal_power: 0.0,
                                intervene: 0.0,
                                attack_interval_ms: 1_500,
                                next_action_at_ms: at_ms + 200 * j,
                                alive: true,
                                helm_power: 0.0,
                                helm_cooldown_ms: u32::MAX,
                                next_helm_at_ms: u32::MAX,
                                helm_stack_bonus: 0.0,
                                boots_power: 0.0,
                                boots_cooldown_ms: u32::MAX,
                                next_boots_at_ms: u32::MAX,
                                damage_reduction: 0.0,
                                block_chance: 0.0,
                                evasion: 0.0,
                                increased_damage: 0.0,
                                crit_chance: 0.0,
                                crit_multiplier: 1.5,
                                splash: 0.0,
                                // Always real-boss content (only reached
                                // inside Some(BossKind::Lich)) - see
                                // CombatSimUnit::late_stage_damage_penalty_pct's
                                // doc.
                                late_stage_damage_penalty_pct: late_stage_penalty,
                                boss_focus_stacks: 0.0,
                                boss_ability: None,
                                next_ability_at_ms: u32::MAX,
                                boss_dynamic_power_mult: 1.0,
                                cthulhu_debuff_stacks: 0,
                                cthulhu_debuff_expires_at_ms: 0,
                                cthulhu_debuff_pct_per_stack: 0.0,
                                cube_shred_stacks: 0,
                                cube_shred_expires_at_ms: 0,
                                damage_dealt_total: 0,
                                level: 0,
                                life_leech_pct: 0.0,
                                leech_window_start_ms: 0,
                                leech_gained_in_window: 0.0,
                                skills: Vec::new(),
                                skill_stacks: HashMap::new(),
                                next_flicker_at_ms: u32::MAX,
                                flicker_cooldown_ms: FLICKER_STRIKE_COOLDOWN_MS,
                                has_celestial_conversion: false,
                                wound_deal_leech_per_stack: 0.0,
                                wound_deal_max_stacks: 0,
                                wound_deal_duration_ms: 0,
                                wound_deal_damage_dealt_debuff: 0.0,
                                wound_deal_heal_received_debuff: 0.0,
                                wound_deal_explosion_pct: 0.0,
                                wound_deal_explosion_self_leech_pct: 0.0,
                                wound_deal_explosion_extra_targets: 0,
                                wound_deal_spreads_to_splash: false,
                                contagion_chance: 0.0,
                                gravechill_speed_debuff_pct: 0.0,
                                plaguebearer_extra_targets: 0,
                                wound_stacks: 0,
                                wound_max_stacks: 0,
                                wound_expires_at_ms: 0,
                                wound_leech_per_stack: 0.0,
                                wound_damage_dealt_debuff: 0.0,
                                wound_heal_received_debuff: 0.0,
                                wound_damage_taken_total: 0.0,
                                next_bloodpact_at_ms: u32::MAX,
                                bloodpact_last_fired_at_ms: 0,
                                bloodpact_cooldown_ms: u32::MAX,
                                bloodpact_uses_this_fight: 0,
                                bloodpact_hp_cost_pct: 0.0,
                                bloodpact_damage_mult: 1.0,
                                bloodpact_martyrdom_shield_pct: 0.0,
                                bloodpact_kill_refund_pct: 0.0,
                                bloodpact_nonlethal_refund_pct: 0.0,
                                bloodpact_bloodforblood_pct: 0.0,
                                bloodpact_triage_pct: 0.0,
            bloodpact_finaloffering_min_prior_uses: u32::MAX,
            bloodpact_finaloffering_pct: 0.0,
                                bloodpact_warlordsresolve_pct: 0.0,
                                bloodpact_cleanslate_reset_chance: 0.0,
                                bloodpact_secondwind_reset_chance: 0.0,
                                shield_hp: 0.0,
                                shield_expires_at_ms: 0,
                                shield_reflect_pct: 0.0,
                                shield_reflect_chance: 1.0,
                                shield_reflect_requires_full_absorb: false,
                                unyieldingroots_cycle_ms: 0,
                                naturesward_dr_vs_boss_pct: 0.0,
                                gambit_crit_per_missing_20pct: 0.0,
                                deathdefiant_grace_ms: 0,
                                deathdefiant_frozen_crit_bonus: 0.0,
                                deathdefiant_frozen_crit_bonus_expires_at_ms: 0,
                                bramble_reflect_pct: 0.0,
                                poison_thorns_debuff_pct: 0.0,
                                entangle_chance: 0.0,
                                recent_attackers: Vec::new(),
                                temp_damage_dealt_debuff: 0.0,
                                temp_damage_dealt_debuff_expires_at_ms: 0,
                                frenzy_strike_chance: 0.0,
                                frenzy_extra_hits: 0,
                                frenzy_bloodscent_threshold: 0.0,
                                frenzy_dr_shred_pct: 0.0,
                                frenzy_extra_dmg_pct: 0.0,
                                frenzy_culling_threshold: 0.0,
                                frenzy_heal_pct: 0.0,
                                frenzy_shield_chance: 0.0,
                                frenzy_undying_charges: 0,
                                frenzy_chain_chance: 0.0,
                                frenzy_chain_max_extra: 0,
                                spike_barrier_reflect_pct: 0.0,
                                aegis_shield_pct: 0.0,
                                aegis_shield_duration_ms: AEGIS_SHIELD_DURATION_MS,
                                aegis_rally_speed_pct: 0.0,
                                aegis_extra_targets: 0,
                                thornedhide_pct_per_stack: 0.0,
                                thornedhide_stacks: 0,
                                thornedhide_expires_at_ms: 0,
                                thornedhide_debuff_pct_per_stack: 0.0,
                                spike_retribution_chance: 0.0,
                                spike_unyielding_chance: 0.0,
                                block_damage_reduction_pct: BLOCK_DAMAGE_REDUCTION,
                                stonewall_auto_block_hits: 0,
                                hits_taken_this_fight: 0,
                                stack_speed_per_stack: 0.0,
                                stack_dmg_per_stack: 0.0,
                                stack_avalanche_dmg_per_stack: 0.0,
                                stack_crit_per_stack: 0.0,
                                shatter_shred_pct: 0.0,
                                overwhelm_shred_linger_ms: 0,
                                crush_dr_threshold: 0.0,
                                stack_splash_per_stack: 0.0,
                                windfury_chance: 0.0,
                                stack_shred_per_stack: 0.0,
                                stack_speed_max_stacks: 0,
                                stack_speed_duration_ms: 0,
                                stack_speed_current: 0,
                                stack_speed_expires_at_ms: 0,
                                flowing_speed_per_stack: 0.0,
                                flowing_crit_per_stack: 0.0,
                                flowing_max_stacks: 0,
                                flowing_duration_ms: 0,
                                risingstorm_dmg_pct: 0.0,
                                nervestrike_crit_mult_bonus: 0.0,
                                vitalpoints_shred_per_stack: 0.0,
                                eternalflow_bonus_stacks: 0,
            onehundredhands_bonus_stacks: 0,
                                stormfront_splash_pct: 0.0,
                                flowing_current: 0,
                                flowing_expires_at_ms: 0,
                                flowing_last_target: usize::MAX,
                                chakra_of_many_pct: 0.0,
                                chakra_of_light_pct: 0.0,
                                chakraoflife_duration_ms: 0,
                                chakraoflife_immune_until_ms: 0,
                                next_chakraoflife_expiry_at_ms: u32::MAX,
                                own_mark_crit_chance: 0.0,
                                own_mark_crit_mult: 0.0,
                                own_mark_low_hp_dmg: 0.0,
                                own_mark_ally_crit_chance: 0.0,
                                own_mark_ally_dmg_pct: 0.0,
                                own_mark_ally_crit_mult: 0.0,
                                own_mark_spread_count: 0,
                                killzone_threshold: 0.0,
                                cleankill_remark_chance: 0.0,
                                huntersreward_heal_pct: 0.0,
                                own_curse_dmg_taken: 0.0,
                                own_curse_spread_count: 0,
                                own_doom_detonate_pct: 0.0,
                                own_curse_heal_reduction_pct: 0.0,
                                own_curse_spread_bonus_pct: 0.0,
                                own_soul_stone_max: 0,
                                own_cursed_blood_target_count: 0,
                                own_dreadfuldeath_shred_pct: 0.0,
                                own_apocalypse_splash_pct: 0.0,
                                has_applied_mark_this_fight: false,
                                mark_source_id: None,
                                mark_crit_chance_bonus: 0.0,
                                mark_crit_multiplier_bonus: 0.0,
                                mark_low_hp_damage_bonus: 0.0,
                                mark_ally_crit_chance_bonus: 0.0,
                                mark_ally_dmg_bonus: 0.0,
                mark_ally_crit_multiplier_bonus: 0.0,
                                curse_dmg_taken_bonus: 0.0,
                                soul_stones: 0,
                                soul_stone_uses_this_fight: 0,
                                curse_expires_at_ms: u32::MAX,
                                next_curse_expiry_at_ms: u32::MAX,
                                curse_damage_taken_total: 0.0,
                                curse_detonate_pct: 0.0,
                                curse_source_id: None,
                                curse_heal_reduction_bonus: 0.0,
                                fel_rush_speed_bonus: 0.0,
                                fel_rush_duration_ms: 0,
                                ravage_stack_pct: 0.0,
                                fel_rush_stacks: 0,
                                fel_rush_expires_at_ms: 0,
                                early_fight_speed_bonus_pct: 0.0,
                                early_fight_speed_window_end_ms: 0,
                                flicker_frenzy_speed_bonus: 0.0,
                                unrelenting_duration_bonus_ms: 0,
                                adrenaline_crit_mult_bonus: 0.0,
                                chainreaper_heal_pct: 0.0,
                                deathspiral_heal_pct: 0.0,
                                insatiable_extend_chance: 0.0,
                                secondheartbeat_chance: 0.0,
                                overflowvessel_shield_pct: 0.0,
                                flicker_frenzy_expires_at_ms: 0,
                                endless_thirst_cap_bonus: 0.0,
                                endless_thirst_uncapped: false,
                                endless_thirst_expires_at_ms: 0,
                                reapers_momentum_per_kill: 0,
                                reapers_momentum_banked: 0,
                                guardian_spirit_charges: 0,
                                guardian_spirit_heal_pct: 0.0,
                                guardian_spirit_save_dr_pct: 0.0,
                                guardian_spirit_save_heal_power_pct: 0.0,
                                verdantburst_charges: 0,
                                temp_heal_power_bonus: 0.0,
                                temp_heal_power_bonus_expires_at_ms: 0,
                                eternallight_bonus_pct: 0.0,
                                temp_damage_reduction_bonus: 0.0,
                                temp_damage_reduction_bonus_expires_at_ms: 0,
                                overflow_grace_shield_pct: 0.0,
                                overflow_grace_shield_duration_ms: 0,
                                overflow_grace_shield_dr_pct: 0.0,
                                heal_crit_bonus_mult: 0.0,
                                heal_crit_chance_bonus: 0.0,
                                heal_crit_splash_pct: 0.0,
                                grace_lowest_ally_bonus_pct: 0.0,
                                prayer_chance: 0.0,
                                prayer_bounce_targets: 0,
                                prayer_bounce_value_pct: 0.0,
            unbroken_prayer_chance: 0.0,
                                divine_favor_shield_pct: 0.0,
                                divine_favor_shield_duration_ms: 0,
                                healing_touch_pct: 0.0,
                                crit_shield_max_hp_pct: 0.0,
                                soul_harvest_heal_pct: 0.0,
                                darkritual_dmg_pct: 0.0,
                                eternal_hunger_shield_pct: 0.0,
                                divine_shield_amount_pct: 0.0,
                                divine_shield_cooldown_ms: u32::MAX,
                                next_divine_shield_at_ms: u32::MAX,
                                consecration_shield_pct: 0.0,
                                consecration_shield_duration_ms: 0,
                                communion_heal_power_pct: 0.0,
                                purify_dmg_debuff_pct: 0.0,
                                lastjudgment_skip_chance: 0.0,
                                smite_heal_pct: 0.0,
                                smite_zealotry_bonus_pct: 0.0,
                                smite_extra_targets: 0,
            zealotry_martyrscall_bonus_pct: 0.0,
            zealotry_risingfervor_pct_per_ally: 0.0,
            zealotry_guardianswrath_speed_pct: 0.0,
            zealotry_guardianswrath_speed_bonus: 0.0,
            zealotry_guardianswrath_expires_at_ms: 0,
                                smite_judgment_bonus_pct: 0.0,
                                judgment_threshold: 0.0,
                                smite_holyfire_dmg_pct: 0.0,
                                purgingflame_heal_reduction_pct: 0.0,
                                temp_heal_reduction_pct: 0.0,
                                temp_heal_reduction_expires_at_ms: 0,
                                executionersblessing_heal_pct: 0.0,
                                wrathoftheheavens_chance: 0.0,
                                unbreakable_faith_heal_pct: 0.0,
                                eternalvow_shield_chance: 0.0,
                                graciousburden_heal_pct: 0.0,
                                bondeddevotion_dr_pct: 0.0,
                                bondeddevotion_duration_ms: 0,
                                attack_speed_pct: 0.0,
                                speed_overflow_dmg_pct: 0.0,
                                speed_overflow_crit_pct: 0.0,
                                speed_overflow_threshold: 0.0,
                                twin_strike_chance: 0.0,
                                twin_strike_dmg_pct: 0.0,
                                finiteloop_max_repeats: 0,
                                doubletap_max_repeats: 0,
                                in_splash_resolution: false,
                                own_pack_instinct_evasion_pct: 0.0,
                                own_symbiosis_dr_pct: 0.0,
                                sharedstrength_extra_targets: 0,
                                templeguardian_heal_pct: 0.0,
                                next_templeguardian_heal_at_ms: 0,
                                lingering_effect_pct: 0.0,
                                lingering_dots: Vec::new(),
                                next_lingering_tick_at_ms: u32::MAX,
                                seedoflife_shield_pct: 0.0,
            wildheart_self_heal_pct: 0.0,
            wildinstinct_dr_pct: 0.0,
            wildroar_charges: 0,
            naturesembrace_heal_targets: 0,
            thickhide_cycle_ms: 0,
            next_thickhide_cleanse_at_ms: 0,
            thickhide_target_count: 0,
                                fire_damage_pct: 0.0,
                                cold_damage_pct: 0.0,
                                chaos_damage_pct: 0.0,
                                lightning_damage_pct: 0.0,
                                divine_damage_pct: 0.0,
                                fire_dr_debuff: Vec::new(),
                                cold_evasion_debuff: Vec::new(),
                                chaos_block_debuff: Vec::new(),
                                lightning_dmg_taken: Vec::new(),
                                divine_heal_reduction: Vec::new(),
                                fire_dr_buff: Vec::new(),
                                cold_evasion_buff: Vec::new(),
                                chaos_block_buff: Vec::new(),
                                divine_heal_power_buff: Vec::new(),
                                block_overflow_dmg_rate: 0.0,
                                evasion_overflow_dmg_rate: 0.0,
                                elemental_overflow_dmg_bonus: 0.0,
                                elemental_overflow_dmg_bonus_expires_at_ms: 0,
                                volley_dmg_per_target_pct: 0.0,
                                splash_target_dmg_bonus: 0.0,
                                exploit_weakness_crit_mult_pct: 0.0,
                                exploit_weakness_threshold: 0.0,
                                weakpoint_crit_chance_pct: 0.0,
                                nightstalker_evasion_pct: 0.0,
                                assassinate_crit_mult_bonus: 0.0,
                                silentblade_evasion_pct: 0.0,
                                fadeaway_duration_bonus_ms: 0,
                                backstab_dmg_pct: 0.0,
                                backstab_pending_dmg_pct: 0.0,
                                smokescreen_evasion_pct: 0.0,
                                markedfordeath_hits_remaining: 0,
                                markedfordeath_hit_count: 0,
                                finalcut_speed_pct: 0.0,
                                empoweredbolt_invested: false,
                                empoweredbolt_crit_mult_bonus: 0.0,
                                volatilemagic_splash_pct: 0.0,
                                arcaneinstability_threshold: 0.0,
                                arcaneinstability_bonus_pct: 0.0,
                                premeditation_refund_chance: 0.0,
                                stack_evasion_per_stack: 0.0,
                                huntersinstinct_crit_vs_boss_pct: 0.0,
                                silentkiller_dmg_pct: 0.0,
                                has_hit_boss_this_fight: false,
                                assassinate_charges: 0,
                                dark_communion_pct: 0.0,
                                compassion_prioritize_lowest: false,
                                compassion_dr_pct: 0.0,
                                covenant_pct: 0.0,
                                unbreakablebond_dr_pct: 0.0,
                                vigor_heal_pct: 0.0,
                                vengefulblood_shield_pct: 0.0,
                                secondgale_duration_ms: 0,
                                temp_reckless_immunity_expires_at_ms: 0,
                                reckless_penalty_offset: 0.0,
                                lastlaugh_crit_bonus: false,
                                lastlaugh_crit_mult: false,
                                ragefueled_speed_pct: 0.0,
                                retaliation_chance: 0.0,
                                retaliation_dmg_pct: 0.0,
                                retaliation_heal_pct: 0.0,
                                retaliation_laststand_bonus: 0.0,
                                grudge_pct_per_hit: 0.0,
                                grudge_hit_counts: Vec::new(),
                                retaliation_crit_bonus: 0.0,
                                retaliation_payback_threshold: 0.0,
                                force_crit_next_hit: false,
                                retaliation_surge_pct: 0.0,
                                hardened_stacks: 0,
                                hardened_pct_per_stack: 0.0,
                                retaliation_secondwind_threshold: 0.0,
                                laststand_defiance_pct: 0.0,
                                laststand_berserkvigor_pct: 0.0,
                                immovable_crit_dr_pct: 0.0,
                                reserves_heal_received_pct: 0.0,
                                unbroken_ignore_evasion_pct: 0.0,
            unbroken_crippling_grip_dr_pct: 0.0,
            unyieldingspirit_threshold: 0.0,
                                temp_evasion_debuff: 0.0,
                                temp_evasion_debuff_expires_at_ms: 0,
                                frostnova_evasion_debuff_pct: 0.0,
                                frostnova_duration_ms: 0,
                                absolutezero_threshold: 0.0,
                                staticfield_speed_debuff_pct: 0.0,
                                temp_attack_speed_debuff: 0.0,
                                temp_attack_speed_debuff_expires_at_ms: 0,
                                infernalpact_heal_pct: 0.0,
                                stormcaller_extra_targets: 0,
                                piercing_shots_crit_chance_bonus: 0.0,
                                windpierce_splash_crit_pct: 0.0,
                                armorbreaker_dr_shred_pct: 0.0,
                                scorchedearth_dmg_debuff_pct: 0.0,
                                truestrike_primary_crit_pct: 0.0,
                                stormofarrows_extra_targets: 0,
                                widerburst_extra_targets: 0,
                                inner_focus_heal_pct: 0.0,
                                inner_focus_meditation_bonus: 0.0,
                                inner_focus_chiburst_pct: 0.0,
                                inner_focus_serenity_dr_pct: 0.0,
                                risingtide_heal_power_pct: 0.0,
                                widecircle_extra_targets: 0,
                                harmonize_dr_pct: 0.0,
                                serenity_dr_duration_ms: 0,
                                clarity_triggers_on_block: false,
                                evade_counter_chance: 0.0,
                                evade_counter_last_fired_at_ms: 0,
                                temp_party_attack_speed_bonus: 0.0,
                                temp_party_attack_speed_bonus_expires_at_ms: 0,
                                temp_party_increased_damage_bonus: 0.0,
                                temp_party_increased_damage_bonus_expires_at_ms: 0,
                                temp_party_damage_reduction_bonus: 0.0,
                                temp_party_damage_reduction_bonus_expires_at_ms: 0,
                                low_hp_party_dr_pct: 0.0,
                                low_hp_party_dr_threshold: 0.0,
                                warlord_party_dmg_pct: 0.0,
                                warcry_party_speed_pct: 0.0,
                                neverending_invested: false,
                                bloodlust_stack_expiries: Vec::new(),
                                opportunist_guaranteed_hits: 0,
                                hits_landed_this_fight: 0,
                                ambush_dr_cut_pct: 0.0,
                                openingmove_cooldown_ms: 0,
                                next_openingmove_at_ms: 0,
                                coldsteel_pass_chance: 0.0,
                                coldsteel_pending: false,
                                coldsteel_pass_chance_pending: 0.0,
                                coldsteel_ambush_pct_pending: 0.0,
                                predator_dmg_taken_pct: 0.0,
                                predator_dmg_taken_bonus: 0.0,
                                predator_expires_at_ms: 0,
                                cutthroat_low_hp_dmg_pct: 0.0,
                                vanish_evasion_pct: 0.0,
                                temp_evasion_buff: 0.0,
                                temp_evasion_buff_expires_at_ms: 0,
                                vanishingshot_crit_pct: 0.0,
                                temp_crit_chance_buff: 0.0,
                                temp_crit_chance_buff_expires_at_ms: 0,
                                fleetingshadow_speed_pct: 0.0,
                            });
                        }
                        lich_summon_count += to_summon;
                        units[actor_idx].next_ability_at_ms += LICH_SUMMON_CADENCE_MS;
                    }
                    Some(BossKind::GelatinousCube) => {
                        // Rotating capture, scaled to party size (a live
                        // request: max(1, 10% of currently-alive players),
                        // NOT a flat 3). Captured players stay fully valid
                        // targets for damage/heals - nothing here touches
                        // any targeting filter. "Can't act on their own
                        // turn" is the ONLY effect, enforced purely by
                        // pushing next_action_at_ms forward - the exact
                        // Wild Roar mechanism (see
                        // WILDROAR_FEAR_DURATION_MS). This is inherently
                        // temporary and self-resolving: it's a one-time
                        // forward push, not a standing "captured" flag, so
                        // a player is simply free again once their pushed
                        // clock naturally elapses - no release/cleanup
                        // logic needed anywhere else.
                        let alive_players: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
                        if !alive_players.is_empty() {
                            let capture_count = ((alive_players.len() as f64 * CUBE_CAPTURE_PCT).floor() as usize).max(1);
                            let mut candidates = alive_players.clone();
                            candidates.shuffle(&mut rng);
                            for &idx in candidates.iter().take(capture_count) {
                                units[idx].next_action_at_ms = units[idx].next_action_at_ms.max(at_ms + CUBE_CAPTURE_CADENCE_MS);
                                // Overlay cue: pull this player's sprite
                                // into the Cube and suppress it for the
                                // capture window, then restore it; also
                                // doubles as the cue to alternate the
                                // Cube's own displayed body between
                                // bosses/cube1 and bosses/cube2.
                                events.push(CombatEvent::SkillCast { at_ms, unit: units[idx].id.clone(), skill: "Gelatinous Cube Capture".to_string() });
                            }
                        }
                        units[actor_idx].next_ability_at_ms += CUBE_CAPTURE_CADENCE_MS;
                    }
                    // Fire Demon is a passive fight-wide aura (applied
                    // once at setup, above) - it never actually reaches a
                    // BossAbility tick since its next_ability_at_ms
                    // starts at u32::MAX, but this covers None/anything
                    // else defensively.
                    _ => {
                        units[actor_idx].next_ability_at_ms = u32::MAX;
                    }
                }
                continue;
            }
        };

        if units[actor_idx].is_boss {
            let targets: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
            if !targets.is_empty() {
                // Every enemy attack (real boss, Lich add, basic mob)
                // targets from the above-median-level pool first - see
                // `prioritize_above_median`'s doc, "strongest heroes die
                // first". The real boss (has a `boss_ability`) then
                // still further narrows that down to whoever's hardest
                // to kill; a Lich add/basic mob just picks randomly
                // within it. `targets` itself (the FULL alive pool, not
                // this narrowed one) stays what Intervene's pooling
                // below uses - that's a defensive mechanic protecting
                // the whole party, not a targeting choice, so it isn't
                // priority-filtered.
                let priority_targets = prioritize_above_median(&targets, &units, median_level);
                // Druid's Unyielding Roots (2026-08-16 rework) - overrides
                // target selection entirely while active, bypassing both
                // the above-median-priority filter and the boss's own
                // survivability pick: every `unyieldingroots_cycle_ms` of
                // fight time, for the first `UNYIELDINGROOTS_TAUNT_DURATION_MS`
                // of that window, every boss attack targets this Druid
                // specifically. Computed lazily off `at_ms` alone (no
                // scheduled event/extra clock needed - see the field's
                // own doc). Checked against the FULL `targets` pool, not
                // `priority_targets` - a taunt works even if the Druid
                // wouldn't otherwise have qualified for priority targeting.
                let taunt_idx = targets.iter().copied().find(|&i| {
                    let cycle_ms = units[i].unyieldingroots_cycle_ms;
                    cycle_ms > 0 && at_ms % cycle_ms < UNYIELDINGROOTS_TAUNT_DURATION_MS
                });
                let target_idx = if let Some(idx) = taunt_idx {
                    idx
                } else if units[actor_idx].boss_ability.is_some() {
                    *priority_targets.iter().max_by(|&&a, &&b| survivability(&units[a]).total_cmp(&survivability(&units[b]))).unwrap()
                } else {
                    priority_targets[rng.gen_range(0..priority_targets.len())]
                };
                // Every consecutive hit on the SAME focus target stacks
                // another +10% damage taken (starts at 10% on the first
                // hit); switching to a new focus target resets it - see
                // `boss_focus_stacks`'s doc. Only the real boss does
                // this (a Lich add doesn't touch boss_focus_target at
                // all, so the real boss's own focus tracking survives
                // adds attacking in between).
                if units[actor_idx].boss_ability.is_some() {
                    if boss_focus_target == Some(target_idx) {
                        units[target_idx].boss_focus_stacks += 0.10;
                    } else {
                        units[target_idx].boss_focus_stacks = 0.10;
                        boss_focus_target = Some(target_idx);
                    }
                }
                let base_damage = attacker_base_damage(&units[actor_idx], &mut rng);

                // Intervene: the WHOLE party's Intervene stats pool
                // together - the raw sum (which can run past 100% with
                // several protectors) determines how much of THIS hit
                // gets redirected at all, hard-capped at 50% no matter
                // how high the raw sum goes. That capped pool is then
                // split among every alive party member who has some
                // Intervene, each getting their own RAW share of the
                // raw sum (so 40%-of-100%-raw gets 40% of the pool,
                // regardless of the cap) - the highest-Intervene member
                // eats the most of it. The target still takes their own
                // un-redirected share directly, and (if they themselves
                // have Intervene) also gets their own cut of the pool on
                // top - their own contribution just comes back to them.
                // Every recipient here is its own independent hit,
                // rolled against their own defenses (same "each portion
                // is its own hit" pattern as splash).
                let raw_group_intervene: f64 = targets.iter().map(|&i| units[i].intervene).sum();
                let pool_fraction = raw_group_intervene.min(0.5);
                if pool_fraction > 0.0 {
                    let target_share = base_damage * (1.0 - pool_fraction);
                    apply_hit(&mut units, actor_idx, target_idx, target_share, at_ms, &mut events, &mut rolls, &mut rng, true, false);

                    let pool_damage = base_damage * pool_fraction;
                    for &protector_idx in &targets {
                        let protector_intervene = units[protector_idx].intervene;
                        if protector_intervene <= 0.0 {
                            continue;
                        }
                        let protector_share = pool_damage * (protector_intervene / raw_group_intervene);
                        // This call's `target_idx` is `protector_idx`, not
                        // the enemy's original `target_idx` above - a
                        // Paladin who intervenes genuinely WAS hit by this
                        // attack, mechanically, not just "took some damage
                        // near a hit". Any future "was I hit" trigger
                        // (Warrior's Retaliation, or anything else reacting
                        // to being attacked - see the passive tree, not
                        // real game state yet) should hook into
                        // `apply_hit`'s `target_idx` / `CombatEvent::Attack`'s
                        // `target` generically, the same way life leech/
                        // kill-triggers already do off `attacker_idx` -
                        // never a narrower "was I the enemy's PRIMARY
                        // per-turn target" check, which would silently miss
                        // this call (and the splash targets in
                        // `apply_splash` below, and any other apply_hit
                        // call where this unit ends up as the target for a
                        // reason other than direct targeting).
                        apply_hit(&mut units, actor_idx, protector_idx, protector_share, at_ms, &mut events, &mut rolls, &mut rng, true, false);
                        // Paladin's Unbreakable Faith - heals the protector
                        // for a fraction of what they just redirected onto
                        // themselves, on top of the hit itself.
                        let faith_pct = units[protector_idx].unbreakable_faith_heal_pct;
                        if faith_pct > 0.0 {
                            let self_heal = protector_share * faith_pct;
                            let healed = apply_heal(&mut units, protector_idx, protector_idx, self_heal, at_ms, &mut events, &mut rng);
                            // Eternal Vow - a chance for the same heal to
                            // also fully shield the protector.
                            let eternalvow_chance = units[protector_idx].eternalvow_shield_chance;
                            if healed > 0 && eternalvow_chance > 0.0 && rng.gen_bool(eternalvow_chance.clamp(0.0, 1.0)) {
                                grant_shield(&mut units, protector_idx, protector_idx, healed as f64, at_ms, ETERNAL_VOW_SHIELD_DURATION_MS, &mut events);
                            }
                        }
                        // Gracious Burden - also heals the ORIGINAL target
                        // (the ally whose hit got redirected), for a
                        // fraction of the redirected amount.
                        let graciousburden_pct = units[protector_idx].graciousburden_heal_pct;
                        if graciousburden_pct > 0.0 && target_idx != protector_idx {
                            apply_heal(&mut units, protector_idx, target_idx, protector_share * graciousburden_pct, at_ms, &mut events, &mut rng);
                        }
                        // Bonded Devotion - the intervened ally also gets
                        // a temporary DR buff.
                        let bondeddevotion_pct = units[protector_idx].bondeddevotion_dr_pct;
                        if bondeddevotion_pct > 0.0 {
                            units[protector_idx].temp_damage_reduction_bonus = bondeddevotion_pct;
                            units[protector_idx].temp_damage_reduction_bonus_expires_at_ms = at_ms + units[protector_idx].bondeddevotion_duration_ms;
                        }
                    }
                } else {
                    apply_hit(&mut units, actor_idx, target_idx, base_damage, at_ms, &mut events, &mut rolls, &mut rng, true, false);
                }

                // Dragon's Breath (2026-08-16 rework) - the Dragon no
                // longer has a separate periodic AoE layered on top of its
                // normal attacks (see the `next_ability_at_ms`/`BossAbility`
                // match above); its EVERY attack now sweeps the entire
                // party at full value instead of the normal splash-stat-
                // limited cleave (`ENEMY_SPLASH_MAX_TARGETS` == 1) every
                // other boss uses - "flies back and forth breathing fire
                // the whole fight" per the request. `median_level: None`
                // (not `Some`) skips the above-median-prioritization a
                // normal cleave uses - a true sweeping breath hits
                // EVERYONE, not just the stronger half of the party. Still
                // pushes the same "Dragon's Breath" `SkillCast` the old
                // periodic version did, so the overlay's flight/breath
                // choreography now just runs continuously every swing
                // instead of once every 5s.
                // A match, not a sibling boolean like the old `is_dragon`
                // alone - now 3 distinct splash shapes (default cleave,
                // Dragon's full-party sweep, Cube's capped-5), kept as one
                // source of truth.
                let (splash, max_splash_targets, splash_median) = match units[actor_idx].boss_ability {
                    Some(BossKind::Dragon) => (1.0, units.len(), None),
                    // "Hits 5 random players" - CUBE_SPLASH_MAX_TARGETS
                    // (4) additional plus the 1 already-targeted primary =
                    // 5 total. median: None keeps selection genuinely
                    // uniform-random among candidates (apply_splash's own
                    // remove-at-random-index loop), not biased toward the
                    // above-median half a normal cleave's Some(median_level)
                    // would use.
                    Some(BossKind::GelatinousCube) => (1.0, CUBE_SPLASH_MAX_TARGETS, None),
                    _ => {
                        let splash = units[actor_idx].splash + stack_splash_bonus(&units[actor_idx], at_ms) + stormfront_splash_bonus(&units[actor_idx], at_ms);
                        (splash, ENEMY_SPLASH_MAX_TARGETS, Some(median_level))
                    }
                };
                apply_splash(&mut units, actor_idx, target_idx, base_damage, splash, max_splash_targets, splash_median, at_ms, &mut events, &mut rolls, &mut rng);
                if units[actor_idx].boss_ability == Some(BossKind::Dragon) {
                    events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Dragon's Breath".to_string() });
                }
            }
        } else {
            // Every player takes ONE unified attack action, not a
            // separate heal-or-attack choice anymore - it splits between
            // a damage share (sent at a random alive enemy) and a heal
            // share (sent at the neediest hurt ally), both rolled off
            // the SAME pre-crit base (`attacker_base_damage`) and both
            // able to happen the same turn (see `Character::
            // combat_heal_power`'s "healing is strictly converted
            // damage" doc). `heal_power` 0.0 (the default off a Melee/
            // Ranged archetype with no tree investment - gear no longer
            // grants any, see `combat_heal_power`'s doc) means 100% stays
            // damage - unchanged from before this existed. `heal_power`
            // 100%+ (a Heal archetype's own baseline, Paladin's own flat
            // bonus, or enough tree investment on anyone else) floors the
            // damage share at 0 - nothing left to attack with, a pure
            // heal action.
            // Druid's Werebear/Thick Hide (see `thickhide_cycle_ms`'s doc) -
            // same "piggyback on this unit's own turn cadence, approximate
            // rather than a dedicated scheduled event" shape as Bloodpact
            // just below. Clears `boss_focus_stacks` (the one real,
            // existing "enemy-inflicted debuff" stored per-unit in this
            // sim) on the Druid themselves plus up to `thickhide_target_count
            // - 1` other party members, picked lowest-HP-first.
            if units[actor_idx].thickhide_cycle_ms > 0 && at_ms >= units[actor_idx].next_thickhide_cleanse_at_ms {
                units[actor_idx].next_thickhide_cleanse_at_ms = at_ms + units[actor_idx].thickhide_cycle_ms;
                let target_count = units[actor_idx].thickhide_target_count;
                let mut cleanse_targets = vec![actor_idx];
                let mut others: Vec<usize> = units.iter().enumerate().filter(|(i, u)| *i != actor_idx && !u.is_boss && u.alive).map(|(i, _)| i).collect();
                others.sort_by_key(|&i| units[i].hp);
                cleanse_targets.extend(others.into_iter().take(target_count.saturating_sub(1) as usize));
                let mut cleansed_anything = false;
                for &idx in &cleanse_targets {
                    if units[idx].boss_focus_stacks > 0.0 {
                        units[idx].boss_focus_stacks = 0.0;
                        cleansed_anything = true;
                    }
                }
                if cleansed_anything {
                    events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Thick Hide".to_string() });
                }
            }
            // Slayer's Bloodpact (see `next_bloodpact_at_ms`'s doc) - fires
            // automatically whenever its real cooldown has elapsed, since
            // there's no live player input during an auto-battle sim to
            // activate it manually. Checked at this same unified-attack
            // turn trigger (not its own independent event) since its
            // payoff boosts THIS turn's own damage.
            let mut bloodpact_boosted = false;
            let mut bloodpact_cost: i64 = 0;
            if at_ms >= units[actor_idx].next_bloodpact_at_ms {
                units[actor_idx].next_bloodpact_at_ms = at_ms + units[actor_idx].bloodpact_cooldown_ms;
                units[actor_idx].bloodpact_last_fired_at_ms = at_ms;
                // Triage - each PRIOR use this fight discounts the cost
                // further (see `bloodpact_triage_pct`'s doc) - read before
                // incrementing the counter below, so the first use is
                // never discounted.
                let triage_reduction = (units[actor_idx].bloodpact_triage_pct * units[actor_idx].bloodpact_uses_this_fight as f64).min(0.9);
                // Final Offering - once enough PRIOR uses have happened
                // this fight (read before incrementing below, same as
                // Triage), every use after that gets a flat 33% off too.
                // Combined with Triage MULTIPLICATIVELY and capped
                // together at 90% off (see `bloodpact_finaloffering_pct`'s
                // doc) so the two can never make Bloodpact free even at
                // 3/3 both.
                let finaloffering_reduction =
                    if units[actor_idx].bloodpact_uses_this_fight >= units[actor_idx].bloodpact_finaloffering_min_prior_uses {
                        units[actor_idx].bloodpact_finaloffering_pct
                    } else {
                        0.0
                    };
                let combined_remaining_frac = ((1.0 - triage_reduction) * (1.0 - finaloffering_reduction)).max(0.10);
                units[actor_idx].bloodpact_uses_this_fight += 1;
                events.push(CombatEvent::SkillCast { at_ms, unit: units[actor_idx].id.clone(), skill: "Bloodpact".to_string() });
                // Cost is a % of CURRENT hp (not max) - scales down as the
                // Slayer takes damage over the fight, so it can never be a
                // disproportionate gut-punch relative to how much HP is
                // actually left.
                bloodpact_cost = (units[actor_idx].hp as f64 * units[actor_idx].bloodpact_hp_cost_pct * combined_remaining_frac).round().max(0.0) as i64;
                // Never lets the sacrifice itself be lethal - floors at 1
                // hp, same "the cost is real but can't be a death
                // sentence" spirit as every other self-damage mechanic
                // here (e.g. Reckless Swing's trade, once that lands).
                units[actor_idx].hp = (units[actor_idx].hp - bloodpact_cost).max(1);
                // Warlord's Resolve - the 3rd use this fight also grants
                // the party a temporary increased-damage buff, via the
                // same party-broadcast primitive Berserker's own version
                // uses.
                if units[actor_idx].bloodpact_uses_this_fight == 3 {
                    let warlordsresolve_pct = units[actor_idx].bloodpact_warlordsresolve_pct;
                    if warlordsresolve_pct > 0.0 {
                        for u in units.iter_mut() {
                            if !u.is_boss && u.alive {
                                u.temp_party_increased_damage_bonus = warlordsresolve_pct;
                                u.temp_party_increased_damage_bonus_expires_at_ms = at_ms + BLOODPACT_WARLORDSRESOLVE_DURATION_MS;
                            }
                        }
                    }
                }
                if units[actor_idx].bloodpact_martyrdom_shield_pct > 0.0 {
                    // Martyrdom: shields the lowest-HP ally instead of
                    // boosting this hit's damage.
                    let shield_amount = bloodpact_cost as f64 * units[actor_idx].bloodpact_martyrdom_shield_pct;
                    let shield_target =
                        units.iter().enumerate().filter(|(i, u)| !u.is_boss && u.alive && *i != actor_idx).min_by_key(|(_, u)| u.hp).map(|(i, _)| i);
                    if let Some(shield_idx) = shield_target {
                        grant_shield(&mut units, actor_idx, shield_idx, shield_amount, at_ms, BLOODPACT_SHIELD_DURATION_MS, &mut events);
                        // Shared Pain - self-heal off the shield's value.
                        let sharedpain_pct = characters.get(&units[actor_idx].id).map(|c| c.passive_node_magnitude("sharedpain")).unwrap_or(0.0);
                        if sharedpain_pct > 0.0 {
                            let self_heal = (shield_amount * sharedpain_pct).round().max(0.0) as i64;
                            let new_hp = (units[actor_idx].hp + self_heal).min(units[actor_idx].max_hp as i64);
                            let healed = (new_hp - units[actor_idx].hp) as u64;
                            units[actor_idx].hp = new_hp;
                            if healed > 0 {
                                let id = units[actor_idx].id.clone();
                                events.push(CombatEvent::Heal { at_ms, healer: id.clone(), target: id, amount: healed, target_hp_after: new_hp as u64 });
                            }
                        }
                    }
                } else {
                    bloodpact_boosted = true;
                }
            }

            let base = attacker_base_damage(&units[actor_idx], &mut rng);
            // Final Blessing/Healing Touch's temporary healing-power
            // buffs (see `temp_heal_power_bonus`'s doc) fold straight
            // into the same `heal_power` this action already splits
            // damage/healing by - so a buffed unit correctly shifts MORE
            // of this turn toward healing too, same as a permanent
            // investment would.
            let temp_heal_power = if at_ms <= units[actor_idx].temp_heal_power_bonus_expires_at_ms { units[actor_idx].temp_heal_power_bonus } else { 0.0 };
            let heal_power = units[actor_idx].heal_power + temp_heal_power;
            let damage_fraction = (1.0 - heal_power).max(0.0);

            // Paladin's Radiant Smite/Judgment - captures which enemy (if
            // any) this action's damage share actually hit, so Judgment's
            // live below-50%-HP check (below, after both branches) has
            // something to read. `None` on a 100%-heal-power action (no
            // damage share at all) or if every enemy is already dead.
            let mut smite_boss_idx: Option<usize> = None;
            if damage_fraction > 0.0 {
                let enemy_targets: Vec<usize> = units.iter().enumerate().filter(|(_, u)| u.is_boss && u.alive).map(|(i, _)| i).collect();
                if !enemy_targets.is_empty() {
                    let boss_idx = enemy_targets[rng.gen_range(0..enemy_targets.len())];
                    smite_boss_idx = Some(boss_idx);
                    let damage_base = base * damage_fraction;
                    // Ranger's Volley / Mage's Chain Lightning (redesigned
                    // 2026-08-15, then reworked again 2026-08-17 - see
                    // `volley_dmg_per_target_pct`'s doc) - sized by how many
                    // targets this attack COULD reach, not how many it
                    // actually does, no live target-counting needed. Used
                    // to be its OWN separate multiplicative pass on
                    // `damage_base` (compounding on top of the character's
                    // normal `increased_damage`, applied again inside
                    // `resolve_hit`) - now just SET on the attacking unit
                    // and read as one more additive term in `resolve_hit`'s
                    // own `increased_damage` combination, same shared-pool
                    // pattern every other damage bonus there already uses.
                    let volley_per_target = units[actor_idx].volley_dmg_per_target_pct;
                    if volley_per_target > 0.0 {
                        let splash = units[actor_idx].splash + stack_splash_bonus(&units[actor_idx], at_ms) + stormfront_splash_bonus(&units[actor_idx], at_ms);
                        let max_splash_targets = PLAYER_SPLASH_MAX_TARGETS + if splash > 1.0 { SPLASH_OVERFLOW_BONUS_TARGETS } else { 0 };
                        let max_targets_reachable = 1 + max_splash_targets;
                        units[actor_idx].splash_target_dmg_bonus = volley_per_target * max_targets_reachable as f64;
                    } else {
                        units[actor_idx].splash_target_dmg_bonus = 0.0;
                    }
                    // Bloodpact's guaranteed payoff: a flat, deterministic
                    // damage multiplier on this one hit (2x/3x/4x at
                    // Bloodpact rank 1/2/3 - see `bloodpact_damage_mult`'s
                    // doc). Only the primary hit is boosted, not splash.
                    let primary_damage = if bloodpact_boosted { damage_base * units[actor_idx].bloodpact_damage_mult } else { damage_base };
                    // Ranger's True Strike - a one-off crit-chance override
                    // for just this primary hit (not splash).
                    let truestrike_bonus = units[actor_idx].truestrike_primary_crit_pct;
                    let original_crit_chance = units[actor_idx].crit_chance;
                    if truestrike_bonus > 0.0 {
                        units[actor_idx].crit_chance += truestrike_bonus;
                    }
                    apply_hit(&mut units, actor_idx, boss_idx, primary_damage, at_ms, &mut events, &mut rolls, &mut rng, true, false);
                    if truestrike_bonus > 0.0 {
                        units[actor_idx].crit_chance = original_crit_chance;
                    }
                    if bloodpact_boosted {
                        // Grim Bargain/Debt Collector - refund a fraction
                        // of the sacrificed HP, more if the hit killed.
                        // Blood for Blood adds a bonus refund on a kill
                        // scaled off the TARGET's max HP, on top of that.
                        let killed = !units[boss_idx].alive;
                        let refund_pct = if killed {
                            units[actor_idx].bloodpact_kill_refund_pct
                        } else {
                            units[actor_idx].bloodpact_nonlethal_refund_pct
                        };
                        let mut refund = (bloodpact_cost as f64 * refund_pct).round().max(0.0) as i64;
                        if killed && units[actor_idx].bloodpact_bloodforblood_pct > 0.0 {
                            refund += (units[boss_idx].max_hp as f64 * units[actor_idx].bloodpact_bloodforblood_pct).round().max(0.0) as i64;
                        }
                        if refund > 0 {
                            let new_hp = (units[actor_idx].hp + refund).min(units[actor_idx].max_hp as i64);
                            let healed = (new_hp - units[actor_idx].hp) as u64;
                            units[actor_idx].hp = new_hp;
                            if healed > 0 {
                                let id = units[actor_idx].id.clone();
                                events.push(CombatEvent::Heal { at_ms, healer: id.clone(), target: id, amount: healed, target_hp_after: new_hp as u64 });
                            }
                            // Clean Slate - a successful Grim Bargain
                            // refund has a chance to also fully reset
                            // Bloodpact's cooldown.
                            let cleanslate_chance = units[actor_idx].bloodpact_cleanslate_reset_chance;
                            // Defensive, same reasoning as Second Wind's own
                            // guard above - Clean Slate's tree prerequisites
                            // (cleanslate -> grimbargain -> sacrifice) already
                            // guarantee real Bloodpact investment whenever
                            // this can fire at all, but this reset shouldn't
                            // silently rely on that staying true forever.
                            if cleanslate_chance > 0.0 && units[actor_idx].bloodpact_cooldown_ms < u32::MAX && rng.gen_bool(cleanslate_chance.clamp(0.0, 1.0)) {
                                // Same 1s-from-any-source floor as Second Wind's reset above.
                                units[actor_idx].next_bloodpact_at_ms = at_ms.max(units[actor_idx].bloodpact_last_fired_at_ms + 1_000);
                            }
                        }
                    }
                    let splash = units[actor_idx].splash + stack_splash_bonus(&units[actor_idx], at_ms) + stormfront_splash_bonus(&units[actor_idx], at_ms);
                    // Storm of Arrows/Wider Burst - extra guaranteed
                    // splash targets on top of the base cap.
                    let extra_splash_targets =
                        (units[actor_idx].stormofarrows_extra_targets + units[actor_idx].widerburst_extra_targets + units[actor_idx].stormcaller_extra_targets) as usize;
                    apply_splash(&mut units, actor_idx, boss_idx, damage_base, splash, PLAYER_SPLASH_MAX_TARGETS + extra_splash_targets, None, at_ms, &mut events, &mut rolls, &mut rng);
                    // Berserker's Frenzy - a chance for THIS attack to
                    // strike the same target extra times (see
                    // `fire_frenzy`'s doc). `damage_base` (not
                    // `primary_damage`) since Frenzy's extra strikes are
                    // their own independent hits, not a continuation of
                    // Bloodpact's one-off multiplier (which is Slayer-only
                    // anyway - always 0 for a Berserker).
                    fire_frenzy(&mut units, actor_idx, boss_idx, damage_base, at_ms, &mut events, &mut rolls, &mut rng, 0);
                }
            }

            // Paladin's Holy Fire - tallies the heal-power share's own
            // restored amount (below) alongside Radiant Smite's separate
            // heal (after this block) so Holy Fire has ONE combined total
            // to convert into damage.
            let mut heal_share_healed: u64 = 0;
            if heal_power > 0.0 {
                // Lowest-HP alive ally (never itself while anyone else
                // needs it) - `None` (a no-op heal share) if nobody's
                // actually hurt, same as before this existed, just no
                // longer gating whether the damage share above happened
                // too.
                let heal_target_idx = units
                    .iter()
                    .enumerate()
                    .filter(|(i, u)| !u.is_boss && u.alive && u.hp < u.max_hp as i64 && *i != actor_idx)
                    .min_by_key(|(_, u)| u.hp)
                    .map(|(i, _)| i)
                    .or_else(|| {
                        let u = &units[actor_idx];
                        (u.hp < u.max_hp as i64).then_some(actor_idx)
                    })
                    .or_else(|| {
                        // Nobody's hurt at all (2026-08-16, a live request) -
                        // rather than no-op this share entirely, dump it on
                        // a random ally (self included). Many nodes trigger
                        // off the ACT of healing regardless of whether the
                        // direct hp portion has any room to land - Lingering
                        // Effect, Wild Heart, Wild Instinct, Seed of Life's
                        // shield, Rejuvenation's bounce, Sanctified Touch's
                        // heal-crit chance, etc. - so this pre-positions
                        // those buffs/shields/HoTs before a hit actually
                        // comes in instead of wasting a fully-invested
                        // healer's entire turn the instant the party tops
                        // off. `None` only in the genuine edge case of a
                        // solo fight with nobody else alive.
                        let alive_party: Vec<usize> = units.iter().enumerate().filter(|(_, u)| !u.is_boss && u.alive).map(|(i, _)| i).collect();
                        if alive_party.is_empty() {
                            None
                        } else {
                            Some(alive_party[rng.gen_range(0..alive_party.len())])
                        }
                    });
                if let Some(target_idx) = heal_target_idx {
                    // Own independent crit roll off this share's own
                    // slice of the base (same as a splash target rolls
                    // its own crit off the primary hit's shared base) -
                    // no defender-side mitigation, there's no "defense"
                    // against being healed. Capped at the 100% baseline
                    // (see `Character::combat_hps`'s doc) - heal_power
                    // past 100% no longer makes THIS number bigger, it
                    // already made this unit's `attack_interval_ms`
                    // shorter instead (see `Character::attack_interval_ms`),
                    // so the same-sized heal just happens more often.
                    // Gracious Spirit - a flat bonus specifically because
                    // this target IS the lowest-HP hurt ally (which the
                    // primary heal share always targets by construction -
                    // see `heal_target_idx` above), not a general boost.
                    let grace_bonus = units[actor_idx].grace_lowest_ally_bonus_pct;
                    let heal_base = base * heal_power.min(1.0) * (1.0 + grace_bonus);
                    // Sanctified Touch rank 3 - a flat bonus to
                    // `crit_chance` for THIS roll only, restored right
                    // after (same one-off-override convention Bloodpact
                    // used to use for its old guaranteed-crit payoff).
                    let heal_crit_chance_bonus = units[actor_idx].heal_crit_chance_bonus;
                    let original_crit_chance = units[actor_idx].crit_chance;
                    if heal_crit_chance_bonus > 0.0 {
                        units[actor_idx].crit_chance = (original_crit_chance + heal_crit_chance_bonus).min(1.0);
                    }
                    let heal_roll = roll_attacker_damage(heal_base, &units[actor_idx], at_ms, &mut rng, 0.0, 0.0, 0.0, false, false);
                    let (raw, is_crit) = (heal_roll.damage, heal_roll.is_crit);
                    if heal_crit_chance_bonus > 0.0 {
                        units[actor_idx].crit_chance = original_crit_chance;
                    }
                    // Sanctified Touch (rank 2+) - extra value specifically
                    // on a heal crit, on top of the normal crit_multiplier
                    // `roll_attacker_damage` already applied.
                    let crit_boosted = if is_crit && units[actor_idx].heal_crit_bonus_mult > 0.0 { raw * (1.0 + units[actor_idx].heal_crit_bonus_mult) } else { raw };
                    let heal = (crit_boosted * heal_mult).round().max(0.0) as u32;
                    let healed = apply_heal(&mut units, actor_idx, target_idx, heal as f64, at_ms, &mut events, &mut rng);
                    heal_share_healed = healed;
                    if healed > 0 {
                        // Celestial Shard's unique affix, Heal-role side -
                        // lets a healer deal real damage from the SAME
                        // action instead of 100%+ heal power leaving them
                        // with literally 0 DPS (see combat_heal_power's
                        // doc). Only the PRIMARY heal share triggers this,
                        // not apply_heal_splash's extra targets below -
                        // keeps this to one bonus hit per action instead
                        // of one per splash target too. Gated on `role ==
                        // Heal` (2026-08-16 fix) - a non-Heal archetype
                        // with a sliver of passive-tree heal_power
                        // investment would otherwise get THIS mechanic
                        // AND the DPS-side follow-up hit in `apply_hit`
                        // simultaneously; "instead of a heal bonus" per
                        // the live request means the two are mutually
                        // exclusive by role, not just by "did a heal
                        // happen to land this swing."
                        if units[actor_idx].has_celestial_conversion && units[actor_idx].role == Some(CombatFunction::Heal) {
                            let enemy_targets: Vec<usize> = units.iter().enumerate().filter(|(_, u)| u.is_boss && u.alive).map(|(i, _)| i).collect();
                            if let Some(&boss_idx) = enemy_targets.get(rng.gen_range(0..enemy_targets.len().max(1))) {
                                let bonus_damage = healed as f64 * CELESTIAL_CONVERSION_PCT;
                                apply_hit(&mut units, actor_idx, boss_idx, bonus_damage, at_ms, &mut events, &mut rolls, &mut rng, true, false);
                            }
                        }
                        // Radiance - a critical heal also splashes to the
                        // rest of the party, separate from (and on top
                        // of) the unit's own gear-based Splash below.
                        let heal_crit_splash_pct = units[actor_idx].heal_crit_splash_pct;
                        if is_crit && heal_crit_splash_pct > 0.0 {
                            apply_heal_splash(&mut units, actor_idx, target_idx, heal, heal_crit_splash_pct, at_ms, &mut events, &mut rng);
                        }
                    }
                    let heal_splash = units[actor_idx].splash;
                    apply_heal_splash(&mut units, actor_idx, target_idx, heal, heal_splash, at_ms, &mut events, &mut rng);
                    // Prayer of Mending - a chance for this same heal to
                    // chain onward to more hurt allies.
                    apply_heal_bounce(&mut units, actor_idx, target_idx, heal, at_ms, &mut events, &mut rng);
                }
            }

            // Paladin's Radiant Smite - fires unconditionally, once per
            // unified action, regardless of whether the damage/heal-power
            // shares above did anything (see `smite_heal_pct`'s doc for
            // why: a 100%-heal-power Paladin still needs this to fire so
            // Holy Fire has something to convert into real boss damage).
            let smite_base_pct = units[actor_idx].smite_heal_pct;
            if smite_base_pct > 0.0 {
                let mut heal_pct = smite_base_pct + units[actor_idx].smite_zealotry_bonus_pct;
                // Judgment - only when THIS action's damage share actually
                // hit an enemy currently below 50% HP.
                if let Some(boss_idx) = smite_boss_idx {
                    let boss = &units[boss_idx];
                    let judgment_threshold = if units[actor_idx].judgment_threshold > 0.0 { units[actor_idx].judgment_threshold } else { 0.5 };
                    if boss.max_hp > 0 && (boss.hp as f64 / boss.max_hp as f64) < judgment_threshold {
                        heal_pct += units[actor_idx].smite_judgment_bonus_pct;
                        // Executioner's Blessing/Wrath of the Heavens -
                        // both keyed off the damage share's OWN kill
                        // (already resolved earlier this same action).
                        if !units[boss_idx].alive {
                            let executioner_pct = units[actor_idx].executionersblessing_heal_pct;
                            if executioner_pct > 0.0 {
                                let self_heal = units[actor_idx].max_hp as f64 * executioner_pct;
                                apply_heal(&mut units, actor_idx, actor_idx, self_heal, at_ms, &mut events, &mut rng);
                            }
                            let wrath_chance = units[actor_idx].wrathoftheheavens_chance;
                            if wrath_chance > 0.0 && rng.gen_bool(wrath_chance.clamp(0.0, 1.0)) {
                                let other_enemies: Vec<usize> = units.iter().enumerate().filter(|(i, u)| *i != boss_idx && u.is_boss && u.alive).map(|(i, _)| i).collect();
                                for other_idx in other_enemies {
                                    let splash_base = attacker_base_damage(&units[actor_idx], &mut rng) * 0.5;
                                    apply_hit(&mut units, actor_idx, other_idx, splash_base, at_ms, &mut events, &mut rolls, &mut rng, false, false);
                                }
                            }
                        }
                    }
                }
                let splash = units[actor_idx].splash + stack_splash_bonus(&units[actor_idx], at_ms) + stormfront_splash_bonus(&units[actor_idx], at_ms);
                let extra_targets = units[actor_idx].smite_extra_targets;
                let smite_healed = apply_radiant_smite_heal(&mut units, actor_idx, heal_pct, splash, extra_targets, at_ms, &mut events, &mut rng);
                // Holy Fire - the heal-power share's restored amount AND
                // Smite's own heal, combined, converted into damage dealt
                // to every alive enemy.
                let holyfire_pct = units[actor_idx].smite_holyfire_dmg_pct;
                let total_healed = heal_share_healed + smite_healed;
                apply_holy_fire_damage(&mut units, actor_idx, total_healed, holyfire_pct, at_ms, &mut events, &mut rolls, &mut rng);
            }
        }

        // Momentum/Fleetfoot/Bloodlust/Relentless Pursuit/Flow State,
        // Flowing Strikes, Fel Rush, and Blood Frenzy's live bonuses (see
        // `speed_stack_multiplier`/`flowing_stack_multiplier`/
        // `fel_rush_multiplier`/`flicker_frenzy_multiplier`'s docs) -
        // consulted here rather than baked into `attack_interval_ms`
        // itself, so each naturally lazily expires without ever needing a
        // revert. All four multipliers are independent and never more
        // than one is non-1.0 on the same unit (mutually exclusive by
        // archetype), so multiplying them together is safe either way.
        let speed_mult = speed_stack_multiplier(&units[actor_idx], at_ms)
            * flowing_stack_multiplier(&units[actor_idx], at_ms)
            * fel_rush_multiplier(&units[actor_idx], at_ms)
            * flicker_frenzy_multiplier(&units[actor_idx], at_ms)
            * party_speed_multiplier(&units[actor_idx], at_ms)
            * ragefueled_speed_multiplier(&units[actor_idx])
            * static_field_multiplier(&units[actor_idx], at_ms)
            * zealouscharge_multiplier(&units[actor_idx], at_ms)
            * early_fight_speed_multiplier(&units[actor_idx], at_ms);
        let effective_interval = (units[actor_idx].attack_interval_ms as f64 / speed_mult).round().max(200.0) as u32;
        units[actor_idx].next_action_at_ms += effective_interval;
    }

    // Won iff every enemy is dead - not just "the first one found", now
    // that a basic encounter can have several (see enemy_targets above).
    let won = !units.iter().any(|u| u.is_boss && u.alive);
    // Built here, from the FINAL roster, not before the loop ran - a
    // Lich's mid-fight summons (see NextEvent::BossAbility) don't exist
    // yet at the top of this function, so capturing unit_infos early
    // left them completely unregistered with the overlay: their Attack
    // events referenced ids the client had never heard of, so nothing
    // ever rendered for them (confirmed live - "fought the lich boss
    // twice but did not see any summons").
    let unit_infos: Vec<CombatUnitInfo> = units
        .iter()
        .map(|u| CombatUnitInfo { id: u.id.clone(), display_name: u.display_name.clone(), is_boss: u.is_boss, archetype: u.archetype, role: u.role, max_hp: u.max_hp })
        .collect();
    (won, unit_infos, events, rolls)
}

/// How long the overlay should actually take to play a fight back,
/// regardless of how long the real simulated fight naturally ran — a
/// one-hit boss kill and a 90-second slugfest both need to read well as
/// an on-screen encounter. Rescales every event's timestamp by the same
/// factor, so the real fight's *shape* (who acted when, relative to
/// everyone else) is preserved, just sped up or slowed down uniformly.
pub(crate) const MIN_DISPLAY_MS: u32 = 6_000;
pub(crate) const MAX_DISPLAY_MS: u32 = 35_000;

pub(crate) fn compress_events(events: Vec<CombatEvent>) -> (Vec<CombatEvent>, u32) {
    let real_duration = events.iter().map(|e| e.at_ms()).max().unwrap_or(0).max(1);
    let display_duration = real_duration.clamp(MIN_DISPLAY_MS, MAX_DISPLAY_MS);
    let scale = display_duration as f64 / real_duration as f64;
    let rescaled = events
        .into_iter()
        .map(|e| {
            let new_at_ms = (e.at_ms() as f64 * scale).round() as u32;
            e.with_at_ms(new_at_ms)
        })
        .collect();
    (rescaled, display_duration)
}

/// Purely presentational thinning for the overlay's own live WebSocket
/// broadcast (2026-08-17, a live request: "the overlay is just for show,"
/// after severe overlay lag was traced to real fights now producing
/// hundreds of thousands of events - `compress_events` above only
/// rescales TIME into a fixed 6-35s window, it never bounded event COUNT,
/// so event density (and the client's `requestAnimationFrame` replay
/// load) scaled up freely with build complexity (Frenzy multi-strikes,
/// splash, Hemorrhage explosions, ...). NEVER applied to the saved
/// fight-history file or any real game logic (both `run_encounter`/
/// `run_basic_encounter` call sites finish reading/persisting the FULL
/// `events` - including `newly_downed`'s revive-timer scan - before this
/// runs, right before `encounter_tx.send`) - this only ever touches the
/// copy that goes out over the wire to the overlay.
///
/// Buckets already-`compress_events`-rescaled events into 1-second
/// windows of the FINAL display timeline, and independently caps how many
/// PLAYER-caused vs BOSS-caused events survive each window (classified by
/// `CombatEvent::actor_id`'s presence in `units`' `is_boss` set) -
/// dropping the overflow once a window's cap is hit, keeping insertion
/// (chronological) order for everything that survives.
pub(crate) const OVERLAY_MAX_PLAYER_EVENTS_PER_SEC: usize = 500;
pub(crate) const OVERLAY_MAX_BOSS_EVENTS_PER_SEC: usize = 1000;

pub(crate) fn thin_events_for_overlay(events: Vec<CombatEvent>, units: &[CombatUnitInfo]) -> Vec<CombatEvent> {
    let boss_ids: std::collections::HashSet<&str> = units.iter().filter(|u| u.is_boss).map(|u| u.id.as_str()).collect();
    let mut player_count_by_sec: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut boss_count_by_sec: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    events
        .into_iter()
        .filter(|e| {
            let sec = e.at_ms() / 1_000;
            let is_boss_actor = boss_ids.contains(e.actor_id());
            let (counter, cap) =
                if is_boss_actor { (&mut boss_count_by_sec, OVERLAY_MAX_BOSS_EVENTS_PER_SEC) } else { (&mut player_count_by_sec, OVERLAY_MAX_PLAYER_EVENTS_PER_SEC) };
            let entry = counter.entry(sec).or_insert(0);
            *entry += 1;
            *entry <= cap
        })
        .collect()
}

#[cfg(test)]
mod overlay_event_thinning_tests {
    use super::*;

    fn attack_at(at_ms: u32, attacker: &str) -> CombatEvent {
        CombatEvent::Attack {
            at_ms,
            attacker: attacker.to_string(),
            target: "someone".to_string(),
            damage: 1,
            unmitigated_damage: 1,
            target_hp_after: 0,
            is_crit: false,
            evaded: false,
            hit_id: 0,
        }
    }

    #[test]
    fn caps_player_and_boss_events_independently_per_second() {
        let units = vec![
            CombatUnitInfo { id: "a_player".to_string(), display_name: "P".to_string(), is_boss: false, archetype: None, role: None, max_hp: 100 },
            CombatUnitInfo { id: "__enemy_0__".to_string(), display_name: "B".to_string(), is_boss: true, archetype: None, role: None, max_hp: 100 },
        ];
        let mut events = Vec::new();
        for _ in 0..600 {
            events.push(attack_at(500, "a_player"));
        }
        for _ in 0..1200 {
            events.push(attack_at(500, "__enemy_0__"));
        }
        let thinned = thin_events_for_overlay(events, &units);
        let player_count = thinned.iter().filter(|e| e.actor_id() == "a_player").count();
        let boss_count = thinned.iter().filter(|e| e.actor_id() == "__enemy_0__").count();
        assert_eq!(player_count, OVERLAY_MAX_PLAYER_EVENTS_PER_SEC, "player events must cap at the player limit, not the boss one");
        assert_eq!(boss_count, OVERLAY_MAX_BOSS_EVENTS_PER_SEC, "boss events must cap at the boss limit, independent of the player count");
    }

    #[test]
    fn different_seconds_each_get_their_own_budget() {
        let units = vec![CombatUnitInfo { id: "a_player".to_string(), display_name: "P".to_string(), is_boss: false, archetype: None, role: None, max_hp: 100 }];
        let mut events = Vec::new();
        for _ in 0..(OVERLAY_MAX_PLAYER_EVENTS_PER_SEC * 2) {
            events.push(attack_at(500, "a_player"));
        }
        for _ in 0..(OVERLAY_MAX_PLAYER_EVENTS_PER_SEC * 2) {
            events.push(attack_at(1_500, "a_player"));
        }
        let thinned = thin_events_for_overlay(events, &units);
        assert_eq!(thinned.len(), OVERLAY_MAX_PLAYER_EVENTS_PER_SEC * 2, "each 1-second window gets its own independent budget");
    }

    #[test]
    fn under_the_cap_nothing_is_dropped() {
        let units = vec![CombatUnitInfo { id: "a_player".to_string(), display_name: "P".to_string(), is_boss: false, archetype: None, role: None, max_hp: 100 }];
        let events = vec![attack_at(0, "a_player"), attack_at(10, "a_player"), attack_at(999, "a_player")];
        let thinned = thin_events_for_overlay(events, &units);
        assert_eq!(thinned.len(), 3);
    }
}

#[cfg(test)]
mod full_detail_combat_log_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    // `roll_elemental_proc`/`RollEvent` are the two pure, cheaply-testable
    // pieces of Wiring Phase 1 (`resolve_hit`'s own 300+-field
    // `CombatSimUnit` had no `Default` yet when these were written - see
    // the `#[cfg(test)] impl Default for CombatSimUnit` above, added in
    // Phase 2, which is what the `resolve_hit`-level tests further down
    // this module build on instead).

    #[test]
    fn crit_stack_bonus_is_zero_at_zero_stacks() {
        assert_eq!(crit_stack_bonus(0.0, 3.0), 0.0);
    }

    #[test]
    fn crit_stack_bonus_at_exactly_one_stack_matches_the_old_linear_formula() {
        // At/below a real 100% crit chance, this must be byte-identical
        // to the pre-overcrit-curve formula: 1.0 * (crit_multiplier - 1.0)
        // * CRIT_BONUS_MULT - the curve only ever applies to stacks PAST
        // the first.
        let crit_multiplier = 3.0;
        let expected = 1.0 * (crit_multiplier - 1.0) * CRIT_BONUS_MULT;
        assert_eq!(crit_stack_bonus(1.0, crit_multiplier), expected);
    }

    #[test]
    fn crit_stack_bonus_approaches_but_never_reaches_its_asymptote() {
        let crit_multiplier = 3.0;
        let asymptote = (1.0 + OVERCRIT_CURVE_A) * (crit_multiplier - 1.0) * CRIT_BONUS_MULT;
        let at_10_stacks = crit_stack_bonus(10.0, crit_multiplier);
        let at_10_000_stacks = crit_stack_bonus(10_000.0, crit_multiplier);
        assert!(at_10_stacks < asymptote, "10 stacks should still be under the asymptote");
        assert!(at_10_000_stacks < asymptote, "even 10,000 stacks should never reach the asymptote");
        assert!(at_10_000_stacks > at_10_stacks, "more overcrit stacks should still mean strictly more bonus, just with diminishing returns");
        assert!((asymptote - at_10_000_stacks) < 0.001, "10,000 stacks should be within a hair of the asymptote");
    }

    #[test]
    fn elemental_proc_does_not_roll_at_all_below_zero_chance() {
        // raw_pct <= 0 must short-circuit BEFORE touching the rng - the
        // `None` second value is what tells `apply_hit` "no roll
        // happened here, log nothing."
        let mut stacks = Vec::new();
        let mut rng = StdRng::seed_from_u64(1);
        let (proc, chance) = roll_elemental_proc(0.0, &mut stacks, usize::MAX, 0, &mut rng);
        assert!(!proc);
        assert_eq!(chance, None);
        assert!(stacks.is_empty());
    }

    #[test]
    fn elemental_proc_does_not_roll_at_all_when_already_at_cap() {
        let mut stacks = vec![1_000, 2_000];
        let mut rng = StdRng::seed_from_u64(1);
        let (proc, chance) = roll_elemental_proc(50.0, &mut stacks, 2, 0, &mut rng);
        assert!(!proc, "already at max_stacks - no new stack should be pushed");
        assert_eq!(chance, None, "capped out means no real roll happened, same as raw_pct <= 0");
        assert_eq!(stacks.len(), 2);
    }

    #[test]
    fn elemental_proc_reports_the_real_chance_it_rolled_against() {
        // raw_pct=5 / ELEMENTAL_PROC_CHANCE_DIVISOR(10) = a real 50%
        // chance - regardless of whether this particular seed happens to
        // hit or miss, a genuine roll happened and must be reported as
        // `Some(0.5)`, not `None`. Deliberately mid-range (not clamped)
        // so this still tests a real rolled chance, not the 100% clamp
        // path - see `elemental_proc_pushes_a_stack_only_on_success` for
        // that.
        let mut stacks = Vec::new();
        let mut rng = StdRng::seed_from_u64(7);
        let (_, chance) = roll_elemental_proc(5.0, &mut stacks, usize::MAX, 1_000, &mut rng);
        assert_eq!(chance, Some(0.5));
    }

    #[test]
    fn elemental_proc_pushes_a_stack_only_on_success() {
        let mut stacks = Vec::new();
        // 1000% raw_pct / 10 divisor = chance 100.0, clamped to 1.0 -
        // deterministic success regardless of seed, isolating "does a
        // successful proc actually push its expiry stack" from the RNG
        // itself.
        let mut rng = StdRng::seed_from_u64(42);
        let (proc, chance) = roll_elemental_proc(1000.0, &mut stacks, usize::MAX, 5_000, &mut rng);
        assert!(proc);
        assert_eq!(chance, Some(1.0));
        assert_eq!(stacks, vec![5_000 + ELEMENTAL_PROC_DURATION_MS]);
    }

    #[test]
    fn roll_event_round_trips_through_json_with_cow_source() {
        // The exact bug this shape fixes: `source: &'static str` can't
        // implement `Deserialize` (would require the deserializer's own
        // input to be `'static`) - `Cow<'static, str>` can, reading back
        // as an owned `String` wrapped in `Cow::Owned`. Confirms the
        // round-trip actually works, not just that it compiles.
        let original = RollEvent {
            event_id: 1,
            hit_id: 7,
            caused_by: Some(3),
            at_ms: 1_234,
            category: RollCategory::Block,
            source: std::borrow::Cow::Borrowed("Stonewall auto-block"),
            actor: "lokati_gaming".to_string(),
            target: Some("__enemy_0__".to_string()),
            probability: None,
            succeeded: Some(true),
            magnitude: None,
        };
        let json = serde_json::to_string(&original).expect("RollEvent must serialize");
        let parsed: RollEvent = serde_json::from_str(&json).expect("RollEvent must deserialize back");
        assert_eq!(parsed.event_id, 1);
        assert_eq!(parsed.hit_id, 7);
        assert_eq!(parsed.caused_by, Some(3));
        assert_eq!(parsed.category, RollCategory::Block);
        assert_eq!(parsed.source.as_ref(), "Stonewall auto-block");
        assert_eq!(parsed.actor, "lokati_gaming");
        assert_eq!(parsed.target.as_deref(), Some("__enemy_0__"));
        assert_eq!(parsed.succeeded, Some(true));
        // `#[serde(skip_serializing_if = "Option::is_none")]` on every
        // optional field - `probability`/`magnitude` were both `None`
        // above, so they must be fully absent from the JSON, not present
        // as an explicit `null` (the whole point of the attribute: an
        // absent field costs nothing on disk, a `null` still costs a
        // field name).
        assert!(!json.contains("probability"), "None fields must be omitted entirely, not serialized as null: {json}");
        assert!(!json.contains("magnitude"), "None fields must be omitted entirely, not serialized as null: {json}");
        assert!(parsed.probability.is_none());
        assert!(parsed.magnitude.is_none());
    }

    #[test]
    fn hit_id_is_shared_across_the_curse_of_weakness_credit_split_shape() {
        // Doesn't run the real split (needs the full `apply_hit`
        // pipeline - see the module doc above) - instead locks in the
        // CONTRACT the split relies on: two `RollEvent`s sharing the same
        // `hit_id` really do serialize/deserialize as belonging to the
        // same hit, which is all `apply_hit`'s own construction of them
        // actually depends on.
        let hit_id = next_hit_id();
        let attacker_share = RollEvent {
            event_id: next_hit_id(),
            hit_id,
            caused_by: None,
            at_ms: 0,
            category: RollCategory::DamageCredit,
            source: std::borrow::Cow::Borrowed("Curse of Weakness"),
            actor: "attacker".to_string(),
            target: Some("target".to_string()),
            probability: None,
            succeeded: None,
            magnitude: Some(40.0),
        };
        let warlock_share = RollEvent { event_id: next_hit_id(), actor: "warlock".to_string(), magnitude: Some(10.0), ..attacker_share.clone() };
        assert_eq!(attacker_share.hit_id, warlock_share.hit_id, "both halves of one real hit must share the same hit_id");
        assert_ne!(attacker_share.event_id, warlock_share.event_id, "each RollEvent still needs its own distinct event_id");
    }

    // Phase 2 - real `resolve_hit`-level tests, using the `#[cfg(test)]
    // impl Default for CombatSimUnit` added this phase specifically so
    // these could exist. `neutral_attacker`/`neutral_defender` are both
    // fully zeroed (evasion/block/crit chance all 0.0) so a hit
    // deterministically lands, unblocked, non-crit - each test then
    // overrides only the field(s) it cares about via `..Default::default()`
    // struct-update syntax.

    fn neutral_attacker() -> CombatSimUnit {
        CombatSimUnit { id: "attacker".to_string(), display_name: "Attacker".to_string(), alive: true, hp: 100, max_hp: 100, ..Default::default() }
    }

    fn neutral_defender() -> CombatSimUnit {
        CombatSimUnit { id: "defender".to_string(), display_name: "Defender".to_string(), alive: true, hp: 100, max_hp: 100, ..Default::default() }
    }

    #[test]
    fn hardened_only_defender_logs_exactly_one_mitigation_source() {
        let atk = neutral_attacker();
        let def = CombatSimUnit { hardened_stacks: 3, hardened_pct_per_stack: 0.05, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(1);
        let outcome = resolve_hit(100.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(!outcome.evaded, "zero evasion must never dodge");
        let mitigation: Vec<_> = outcome.deterministic_sources.iter().filter(|(cat, ..)| *cat == RollCategory::Mitigation).collect();
        assert_eq!(mitigation.len(), 1, "only Hardened is invested - expected exactly 1 Mitigation source, got {mitigation:?}");
        let (_, name, magnitude) = mitigation[0];
        assert_eq!(*name, "Hardened");
        assert!((*magnitude - 0.15).abs() < 1e-9, "3 stacks * 5% = 15%, got {magnitude}");
    }

    #[test]
    fn zero_investment_defender_logs_no_mitigation_or_evasion_sources() {
        let atk = neutral_attacker();
        let def = neutral_defender();
        let mut rng = StdRng::seed_from_u64(2);
        let outcome = resolve_hit(100.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let mitigation_or_evasion: Vec<_> =
            outcome.deterministic_sources.iter().filter(|(cat, ..)| matches!(cat, RollCategory::Mitigation | RollCategory::Evasion)).collect();
        assert!(mitigation_or_evasion.is_empty(), "a fully zeroed defender should log nothing - got {mitigation_or_evasion:?}");
    }

    #[test]
    fn late_stage_penalty_logs_as_its_own_negative_increased_damage_source() {
        // The exact mechanic the "is our new hyperbolic damage reduction
        // functioning as intended" question earlier today needed - this
        // is what makes it directly answerable from a real fight's
        // detail-tier log going forward.
        let atk = neutral_attacker();
        let def = CombatSimUnit { late_stage_damage_penalty_pct: 0.1939, is_boss: true, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(3);
        let outcome = resolve_hit(1000.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let penalty = outcome.deterministic_sources.iter().find(|(_, name, _)| *name == "Late-stage damage penalty");
        let (category, _, magnitude) = penalty.expect("late-stage penalty must be logged when non-zero");
        assert_eq!(*category, RollCategory::IncreasedDamage);
        assert!((*magnitude - (-0.1939)).abs() < 1e-9, "must log the real negative magnitude, not just a flag - got {magnitude}");
        // The penalty is applied upstream of `unmitigated_damage`'s own
        // capture (see its doc) - both `damage` and `unmitigated_damage`
        // already reflect the same cut, so with no OTHER mitigation
        // invested they should be numerically equal to each other, and
        // both well below the raw 1000 base damage.
        assert_eq!(outcome.damage, outcome.unmitigated_damage);
        // 1000 base * (1 - 0.1939) = 806.1, rounds to 806 - the neutral
        // attacker has zero crit/increased-damage, so this is the only
        // thing touching the roll.
        assert_eq!(outcome.unmitigated_damage, 806);
    }

    #[test]
    fn crit_chance_sources_only_logged_when_they_actually_contributed() {
        // Gambit/Pressure Point/etc. are all 0 for a neutral attacker -
        // a base `crit_chance` alone should be the ONLY Crit-chance
        // source (crit-MULTIPLIER sources are separately gated on
        // `is_crit`, covered by the next test).
        let atk = CombatSimUnit { crit_chance: 0.5, ..neutral_attacker() };
        let def = neutral_defender();
        let mut rng = StdRng::seed_from_u64(4);
        let outcome = resolve_hit(100.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let crit_sources: Vec<_> = outcome.deterministic_sources.iter().filter(|(cat, ..)| *cat == RollCategory::Crit).collect();
        let chance_source = crit_sources.iter().find(|(_, name, _)| *name == "Crit chance");
        assert!(chance_source.is_some(), "a real crit_chance investment must be logged");
        assert!((chance_source.unwrap().2 - 0.5).abs() < 1e-9);
        assert!(
            crit_sources.iter().all(|(_, name, _)| *name != "Gambit" && *name != "Pressure Point"),
            "unused crit-chance sources must not appear - got {crit_sources:?}"
        );
    }

    #[test]
    fn hit_id_is_shared_between_the_attack_style_deterministic_sources_and_probabilistic_rolls() {
        // Not a `RollEvent`-level test (that correlation is `apply_hit`'s
        // job, covered by Phase 1's own test) - this locks in the
        // PRECONDITION `apply_hit` relies on: both vecs coming back off
        // the SAME `resolve_hit` call describe the same one hit, so
        // tagging them with the same `hit_id` downstream is actually
        // meaningful and not an accident of unrelated data.
        let atk = CombatSimUnit { crit_chance: 0.5, ..neutral_attacker() };
        let def = CombatSimUnit { damage_reduction: 0.1, evasion: 0.1, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(5);
        let outcome = resolve_hit(100.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(!outcome.probabilistic_rolls.is_empty(), "evasion/block/crit-remainder rolls should all fire with these inputs");
        assert!(!outcome.deterministic_sources.is_empty(), "DR/evasion/crit-chance sources should all be non-empty with these inputs");
    }

    // Base boss buff (2026-08-17, a live request) - `boss_defense_ignore`
    // itself, plus the relative-floor formula each of evasion/block/DR
    // applies it through in `resolve_hit`.

    #[test]
    fn boss_defense_ignore_is_zero_at_the_moment_of_spawn() {
        let boss = CombatSimUnit { is_boss: true, spawned_at_ms: 5_000, ..neutral_attacker() };
        assert_eq!(boss_defense_ignore(&boss, 5_000), 0.0);
    }

    #[test]
    fn boss_defense_ignore_grows_2pct_per_second_alive() {
        let boss = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        assert!((boss_defense_ignore(&boss, 20_000) - 0.40).abs() < 1e-9, "20s alive * 2%/s = 40%");
        // A mid-fight add's OWN spawn time, not the fight's global clock -
        // 5s alive (10_000 - 5_000), not 10s.
        let add = CombatSimUnit { is_boss: true, spawned_at_ms: 5_000, ..neutral_attacker() };
        assert!((boss_defense_ignore(&add, 10_000) - 0.10).abs() < 1e-9, "5s alive since ITS OWN spawn * 2%/s = 10%");
    }

    #[test]
    fn boss_defense_ignore_is_always_zero_for_a_non_boss() {
        let player = CombatSimUnit { is_boss: false, spawned_at_ms: 0, ..neutral_attacker() };
        assert_eq!(boss_defense_ignore(&player, 1_000_000), 0.0);
    }

    #[test]
    fn evasion_floor_never_drops_below_25pct_when_defender_naturally_had_more() {
        // Boss alive 60s -> ignores 120%, would otherwise crush evasion to
        // 0 (or attempt to go negative) without the floor.
        let atk = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        let def = CombatSimUnit { evasion: 0.50, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(6);
        let outcome = resolve_hit(100.0, &atk, &def, 60_000, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let chance = outcome.probabilistic_rolls.iter().find(|(cat, name, ..)| *cat == RollCategory::Evasion && *name == "Evasion").and_then(|(_, _, p, _)| *p);
        assert!((chance.expect("evasion roll must be logged") - 0.25).abs() < 1e-9, "must floor at exactly 25%, got {chance:?}");
    }

    #[test]
    fn evasion_floor_does_not_raise_a_naturally_lower_stat() {
        // Only 10% evasion to begin with - boss pressure has nothing to
        // "protect" above 10%, and must NOT artificially raise it to 25%.
        let atk = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        let def = CombatSimUnit { evasion: 0.10, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(7);
        let outcome = resolve_hit(100.0, &atk, &def, 60_000, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let chance = outcome.probabilistic_rolls.iter().find(|(cat, name, ..)| *cat == RollCategory::Evasion && *name == "Evasion").and_then(|(_, _, p, _)| *p);
        assert!((chance.expect("evasion roll must be logged") - 0.10).abs() < 1e-9, "must stay at the defender's own natural 10%, not be raised - got {chance:?}");
    }

    #[test]
    fn block_floor_never_drops_below_25pct_when_defender_naturally_had_more() {
        let atk = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        let def = CombatSimUnit { block_chance: 0.60, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(8);
        let outcome = resolve_hit(100.0, &atk, &def, 60_000, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let chance = outcome.probabilistic_rolls.iter().find(|(cat, name, ..)| *cat == RollCategory::Block && *name == "Block chance").and_then(|(_, _, p, _)| *p);
        assert!((chance.expect("block roll must be logged") - 0.25).abs() < 1e-9, "must floor at exactly 25%, got {chance:?}");
    }

    #[test]
    fn dr_boss_pressure_is_logged_as_its_own_negative_mitigation_source() {
        let atk = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        let def = CombatSimUnit { damage_reduction: 0.50, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(9);
        let outcome = resolve_hit(100.0, &atk, &def, 10_000, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        // "Boss Pressure" is logged in BOTH the evasion and DR source
        // lists now (this test's boss ignores evasion too) - filter by
        // category to find the DR (Mitigation) one specifically.
        let boss_pressure = outcome.deterministic_sources.iter().find(|(cat, name, _)| *cat == RollCategory::Mitigation && *name == "Boss Pressure");
        let (_, _, magnitude) = boss_pressure.expect("Boss Pressure must be logged in Mitigation for a boss alive 10s");
        assert!((*magnitude - (-0.20)).abs() < 1e-9, "10s alive * -2%/s = -20%, got {magnitude}");
    }

    #[test]
    fn dr_floor_never_drops_below_25pct_when_defender_naturally_had_more() {
        // 50% DR, boss alive long enough to ignore far more than needed to
        // crush it past 25% without the floor.
        let atk = CombatSimUnit { is_boss: true, spawned_at_ms: 0, ..neutral_attacker() };
        let def = CombatSimUnit { damage_reduction: 0.50, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(10);
        let outcome = resolve_hit(1000.0, &atk, &def, 60_000, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        // 1000 * (1 - 0.25) = 750 is the floor; boss pressure must never
        // push the real damage taken ABOVE that (i.e. DR below 25%).
        assert!(outcome.damage <= 750, "DR must never be floored below 25% by boss pressure alone - got damage {}", outcome.damage);
    }

    #[test]
    fn evasion_hard_cap_holds_even_with_heavy_multiplicative_stacking() {
        // Each individual source is only 75% (already at its own cap),
        // but 3 combined multiplicatively would reach 1-(1-0.75)^3 =
        // 98.4375% without the new hard cap - must clamp to 95% instead.
        let atk = CombatSimUnit { is_boss: true, ..neutral_attacker() };
        let def = CombatSimUnit { evasion: 0.75, nightstalker_evasion_pct: 0.75, temp_evasion_buff: 0.75, temp_evasion_buff_expires_at_ms: 10_000, ..neutral_defender() };
        let mut rng = StdRng::seed_from_u64(11);
        let outcome = resolve_hit(100.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let chance = outcome.probabilistic_rolls.iter().find(|(cat, name, ..)| *cat == RollCategory::Evasion && *name == "Evasion").and_then(|(_, _, p, _)| *p);
        assert!(chance.expect("evasion roll must be logged") <= 0.95 + 1e-9, "combined evasion must never exceed 95%, got {chance:?}");
    }

    #[test]
    fn block_plus_dr_combined_never_mitigates_past_95pct() {
        // Stacked well past what 95% alone would allow - at least 5% of
        // raw damage must always land on a hit that wasn't evaded.
        let atk = neutral_attacker();
        let def = CombatSimUnit {
            damage_reduction: 0.75,
            block_chance: 1.0,
            block_damage_reduction_pct: 0.75,
            hardened_stacks: 5,
            hardened_pct_per_stack: 0.5,
            ..neutral_defender()
        };
        let mut rng = StdRng::seed_from_u64(12);
        let outcome = resolve_hit(1000.0, &atk, &def, 1, &mut rng, 0.0, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(!outcome.evaded, "zero evasion must never dodge");
        assert!(outcome.damage >= 50, "at least 5% of 1000 raw damage (50) must always land on a non-evaded hit - got {}", outcome.damage);
    }

    // Universal late-stage penalty coverage (2026-08-18) - `apply_reflect_damage`
    // and `tick_lingering_dots` are two of the "true damage" paths that used
    // to skip `late_stage_damage_penalty_pct` entirely since neither calls
    // `resolve_hit` (see `apply_late_stage_penalty`'s own doc for the full
    // list). Both now run through the shared helper directly.

    #[test]
    fn apply_reflect_damage_respects_the_late_stage_penalty() {
        let source = CombatSimUnit { id: "shielded_player".to_string(), display_name: "Player".to_string(), alive: true, hp: 100, max_hp: 100, ..Default::default() };
        let boss = CombatSimUnit {
            id: "boss".to_string(),
            display_name: "Boss".to_string(),
            alive: true,
            hp: 1_000,
            max_hp: 1_000,
            is_boss: true,
            late_stage_damage_penalty_pct: 0.25,
            ..Default::default()
        };
        let mut units = vec![source, boss];
        let mut events = Vec::new();
        let mut rolls = Vec::new();
        let mut rng = StdRng::seed_from_u64(1);
        apply_reflect_damage(&mut units, 0, 1, 100.0, 1, &mut events, &mut rolls, &mut rng);
        // 100 raw * (1 - 0.25) = 75 - not the full 100 a pre-fix reflect would have dealt.
        assert_eq!(units[1].hp, 1_000 - 75, "reflected damage against a boss must be cut by the late-stage penalty");
        let penalty_roll = rolls
            .iter()
            .find(|r| r.source.as_ref() == "Late-stage damage penalty")
            .expect("a reflect hit against a boss must log the late-stage penalty roll");
        assert_eq!(penalty_roll.magnitude, Some(-0.25));
        assert_eq!(penalty_roll.actor, "shielded_player");
        assert_eq!(penalty_roll.target.as_deref(), Some("boss"));
    }

    #[test]
    fn tick_lingering_dots_respects_the_late_stage_penalty() {
        let mut boss = CombatSimUnit {
            id: "boss".to_string(),
            display_name: "Boss".to_string(),
            alive: true,
            hp: 1_000,
            max_hp: 1_000,
            is_boss: true,
            late_stage_damage_penalty_pct: 0.25,
            ..Default::default()
        };
        boss.lingering_dots.push(LingeringDot { source_id: "attacker".to_string(), amount_per_tick: 100.0, remaining_ticks: 3, next_tick_at_ms: 1, is_heal: false });
        let mut units = vec![boss];
        let mut events = Vec::new();
        let mut rolls = Vec::new();
        let mut rng = StdRng::seed_from_u64(1);
        tick_lingering_dots(&mut units, 0, 1, &mut events, &mut rolls, &mut rng);
        // 100 raw * (1 - 0.25) = 75, no other DR sources invested.
        assert_eq!(units[0].hp, 1_000 - 75, "a lingering DoT tick against a boss must be cut by the late-stage penalty");
        let penalty_roll = rolls
            .iter()
            .find(|r| r.source.as_ref() == "Late-stage damage penalty")
            .expect("a lingering DoT tick against a boss must log the late-stage penalty roll");
        assert_eq!(penalty_roll.magnitude, Some(-0.25));
        assert_eq!(penalty_roll.actor, "attacker");
        assert_eq!(penalty_roll.target.as_deref(), Some("boss"));
    }
}

#[cfg(test)]
mod chakra_of_light_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    // Same "test the pure helper directly, skip the full apply_hit
    // pipeline" reasoning as `full_detail_combat_log_tests`' own module
    // doc above - `roll_chakra_of_light_stacks` is the one genuinely new
    // piece of logic Chakra of Light adds.

    #[test]
    fn does_not_roll_at_all_below_zero_raw_pct() {
        let mut stacks = Vec::new();
        let mut rng = StdRng::seed_from_u64(1);
        roll_chakra_of_light_stacks(0.0, &mut stacks, 200, 0, &mut rng);
        assert!(stacks.is_empty());
    }

    #[test]
    fn matches_the_designed_300_600_900_example() {
        // The user's own worked example: 3000% increased damage (stored as
        // the fraction 30.0, same scale as `increased_damage` itself) at
        // rank 3 (30%) -> 30.0 * 0.30 = 9.0 raw -> 9 guaranteed stacks
        // (900% in the design's own percentage framing), no fractional
        // roll needed since it's an exact whole number.
        let mut stacks = Vec::new();
        let mut rng = StdRng::seed_from_u64(1);
        roll_chakra_of_light_stacks(9.0, &mut stacks, 200, 5_000, &mut rng);
        assert_eq!(stacks.len(), 9);
        assert!(stacks.iter().all(|&s| s == 5_000 + ELEMENTAL_PROC_DURATION_MS));

        let mut stacks = Vec::new();
        roll_chakra_of_light_stacks(3.0, &mut stacks, 200, 0, &mut rng);
        assert_eq!(stacks.len(), 3);

        let mut stacks = Vec::new();
        roll_chakra_of_light_stacks(6.0, &mut stacks, 200, 0, &mut rng);
        assert_eq!(stacks.len(), 6);
    }

    #[test]
    fn fractional_remainder_is_a_genuine_extra_roll() {
        // 3.78 (fraction) -> 3 guaranteed + a 78% chance at a 4th, same
        // shape as `roll_divine_heal_power_proc`'s own doc example (378%).
        let mut rng = StdRng::seed_from_u64(42);
        let mut hit_a_fourth = false;
        for _ in 0..20 {
            let mut stacks = Vec::new();
            roll_chakra_of_light_stacks(3.78, &mut stacks, 200, 0, &mut rng);
            assert!(stacks.len() == 3 || stacks.len() == 4, "must always be exactly 3 or 4, got {}", stacks.len());
            if stacks.len() == 4 {
                hit_a_fourth = true;
            }
        }
        assert!(hit_a_fourth, "a 78% chance across 20 tries should land at least once");
    }

    #[test]
    fn respects_the_max_stacks_cap_unlike_the_uncapped_divine_proc() {
        let mut stacks = vec![0u32; 198];
        let mut rng = StdRng::seed_from_u64(1);
        // Would push 9 guaranteed stacks uncapped - only 2 slots remain.
        roll_chakra_of_light_stacks(9.0, &mut stacks, 200, 0, &mut rng);
        assert_eq!(stacks.len(), 200, "must stop exactly at the cap, never exceed it");
    }
}

#[cfg(test)]
mod leech_leaky_bucket_tests {
    use super::*;

    #[test]
    fn no_time_elapsed_drains_nothing() {
        assert_eq!(drain_leech_window(50.0, 100.0, 1_000, 1_000), 50.0);
    }

    #[test]
    fn a_full_second_drains_the_entire_cap_worth() {
        assert_eq!(drain_leech_window(80.0, 100.0, 0, 1_000), 0.0);
    }

    #[test]
    fn a_half_second_drains_half_the_cap() {
        assert_eq!(drain_leech_window(80.0, 100.0, 0, 500), 30.0);
    }

    #[test]
    fn never_drains_below_zero() {
        assert_eq!(drain_leech_window(10.0, 100.0, 0, 5_000), 0.0);
    }

    /// The actual regression this whole refactor was for (wiki audit
    /// finding #3): under the OLD reset-based window, a leech hit at
    /// t=999ms (filling the cap right before a reset) followed by
    /// another at t=1000ms (the instant a fresh window opens) let a
    /// build gain ~2x LIFE_LEECH_CAP_PER_SEC within a single real-time
    /// millisecond. The leaky bucket must not allow that: only ~1ms of
    /// real time separates the two hits, so only ~1ms worth of the cap
    /// (a tiny fraction) should have drained between them.
    #[test]
    fn hits_straddling_the_old_1000ms_reset_boundary_cannot_double_the_cap() {
        let cap = 100.0;
        // First hit at t=999ms fills the bucket to the cap.
        let mut gained = drain_leech_window(0.0, cap, 0, 999);
        assert_eq!(gained, 0.0, "nothing drains from an empty bucket over 999ms");
        gained = cap; // simulates that hit's leech filling the bucket to the cap

        // Second hit at t=1000ms - only 1ms later.
        let after = drain_leech_window(gained, cap, 999, 1_000);
        // Only 1/1000th of the cap (0.1) should have drained - nowhere
        // close to the full reset-to-zero the old bug effectively granted.
        assert!(after >= cap - 1.0, "only ~1ms of drain should have occurred between hits 1ms apart, got room for {} more", cap - after);
    }
}
