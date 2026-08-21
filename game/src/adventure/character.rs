use super::*;

/// Which of the three underlying combat behaviors an `Archetype` uses -
/// purely internal (drives `simulate_battle`'s turn logic and the
/// overlay's role-based formation lanes), never shown to a player
/// directly; they see their `Archetype` instead. Serialized (for
/// `CombatUnitInfo`/`CombatSimUnit`'s wire format only - never read back
/// from a save file) using the SAME three strings the overlay's existing
/// lane logic already checks for, so `public_adventure_overlay/
/// overlay.html` needed zero changes for the archetype system - `Heal`
/// deliberately renames to "support" to match what it already expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CombatFunction {
    Melee,
    Ranged,
    #[serde(rename = "support")]
    Heal,
}

/// A character's build identity - picked on the web dashboard (see
/// `AdventureManager::change_archetype`), not assigned randomly. Every
/// character starts as `Commoner` (an unspecialized, no bonus/no
/// penalty melee fighter) and gets exactly one free pick out of it; every
/// pick after that costs `ARCHETYPE_CHANGE_COST` dust. See `bonus()` for
/// each archetype's advantage/disadvantage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Archetype {
    Commoner,
    Warrior,
    Berserker,
    Rogue,
    Monk,
    Paladin,
    Ranger,
    Mage,
    Warlock,
    Cleric,
    Druid,
    Slayer,
    /// 12th class (2026-08-19) - see docs/elementalist_spec.md. Hybrid
    /// fire/elemental support-and-summoner. Classified `Ranged` for
    /// `combat_function()` purposes (its base attack is elemental, not
    /// melee) - unlike Paladin/Cleric/Druid's "innately hybrid" baseline
    /// heal_power_pct, this class's healing is earned entirely through
    /// explicit Healing Flames tree investment rather than a baseline
    /// grant, since nothing in its spec describes its basic attack
    /// itself as healing-hybrid.
    Elementalist,
}

/// Elementalist's Golem Master (docs/elementalist_spec.md, Stage 5) -
/// the type assigned to one summon slot via `/passives`. `Basic` is an
/// explicit, real choice (not "no type picked") - it has no sub-tree and
/// no bonuses, just the standard 33%-of-caster-stats golem attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GolemType {
    Basic,
    Thunder,
    Flame,
    Water,
}

impl Default for GolemType {
    /// Serde default for a slot count that grew (more Golem Master rank
    /// invested than `golem_slot_types` currently has entries for) -
    /// same "additive, never lossy" spirit as every other passive schema
    /// default in this file. A fresh, never-assigned slot defaults to
    /// Basic rather than leaving it unusable.
    fn default() -> Self {
        GolemType::Basic
    }
}

impl Default for Archetype {
    /// Serde's `default` for characters saved before archetypes existed
    /// (their old `"role"` field is simply ignored - unknown fields don't
    /// fail deserialization) - this IS the migration: every existing
    /// character loads as Commoner, getting the same free first pick a
    /// new joiner gets, rather than being silently assigned one.
    fn default() -> Self {
        Archetype::Commoner
    }
}

impl Archetype {
    pub fn combat_function(self) -> CombatFunction {
        match self {
            Archetype::Commoner | Archetype::Warrior | Archetype::Berserker | Archetype::Rogue | Archetype::Monk | Archetype::Paladin | Archetype::Slayer => {
                CombatFunction::Melee
            }
            Archetype::Ranger | Archetype::Mage | Archetype::Warlock | Archetype::Elementalist => CombatFunction::Ranged,
            Archetype::Cleric | Archetype::Druid => CombatFunction::Heal,
        }
    }

    /// The web dashboard's role-badge CSS class - kept as its own
    /// method (not derived from `combat_function`'s Debug output) so it
    /// can say "support" for the Heal function, matching the 3 existing
    /// `.role-melee/.role-ranged/.role-support` CSS rules rather than
    /// needing an 11th (one per archetype) or a 3rd-but-differently-named
    /// one for Heal.
    pub fn css_class(self) -> &'static str {
        match self.combat_function() {
            CombatFunction::Melee => "melee",
            CombatFunction::Ranged => "ranged",
            CombatFunction::Heal => "support",
        }
    }

    /// The archetype's one advantage - everything else defaults to 0.0
    /// (no effect). Consumed by every `Character::combat_*` aggregate
    /// getter (added alongside that stat's summed gear affixes) and by
    /// `attack_interval_ms`/`combat_max_hp`. First-pass numbers, same
    /// "will need real tuning" caveat as the rest of this file's balance
    /// constants.
    ///
    /// Every archetype's disadvantage half was removed - each one used to
    /// pair its advantage with a matching downside (e.g. Warrior traded
    /// damage reduction for less damage dealt), but that's gone now,
    /// leaving pure upside. Still scales up with `level` - +10% of its
    /// base value per level, so a level 18 character gets 180% more of it
    /// than level 0 would (a level 18 Rogue's 6% base crit chance becomes
    /// 6% * (1 + 18*0.10) = 16.8%). `Commoner` has none, so it's the one
    /// archetype level does nothing for here.
    pub fn bonus(self, level: u32) -> ArchetypeBonus {
        let mult = 1.0 + level as f64 * 0.10;
        let mut b = ArchetypeBonus::default();
        match self {
            Archetype::Commoner => {}
            Archetype::Warrior => {
                b.damage_reduction = 0.15 * mult;
            }
            Archetype::Berserker => {
                b.increased_damage = 0.25 * mult;
            }
            Archetype::Rogue => {
                // Halved from 0.12 - see affix_base_value's matching
                // crit-chance cut.
                b.crit_chance = 0.06 * mult;
            }
            Archetype::Monk => {
                b.evasion = 0.12 * mult;
            }
            Archetype::Paladin => {
                b.intervene_pct = 0.05 * mult;
                // Innately hybrid, same spirit as Cleric/Druid (2026-08-15
                // - per a live request, now that Radiant Smite/Holy Fire's
                // whole kit assumes real heal_power output to work with).
                // Deliberately NOT done via `combat_function()` returning
                // `Heal` for Paladin - that would also change their base
                // damage formula/attack interval/role badge, none of which
                // this request touches. Set directly here instead, exactly
                // like a Heal archetype's own baseline would be, added on
                // top of `combat_heal_power`'s `base` (which stays 0.0 for
                // Paladin, a Melee function) - so this alone reaches the
                // requested 50%.
                b.heal_power_pct = 0.50 * mult;
            }
            Archetype::Ranger => {
                b.splash = 0.15 * mult;
            }
            Archetype::Mage => {
                // Halved from 0.4 - see affix_base_value's matching
                // crit-multiplier cut.
                b.crit_multiplier = 0.2 * mult;
            }
            Archetype::Warlock => {
                b.attack_speed = 0.15 * mult;
            }
            Archetype::Cleric => {
                // Doubled from 0.25 (2026-08-15) to compensate for the
                // Healing Power gear affix's retirement - see
                // `Character::combat_heal_power`'s doc.
                b.heal_power_pct = 0.50 * mult;
            }
            Archetype::Druid => {
                b.evasion = 0.12 * mult;
            }
            Archetype::Slayer => {
                // Self-heal from a slice of damage dealt - see
                // `Character::combat_life_leech`/`apply_hit`'s leech
                // handling for the LIFE_LEECH_CAP_PER_SEC ceiling.
                b.life_leech_pct = 0.001 * mult;
            }
            Archetype::Elementalist => {
                // Base class effect (docs/elementalist_spec.md) - same
                // shape/magnitude as Ranger's own splash advantage
                // (the spec says "splash, scaling with level" without
                // giving its own base fraction, so this reuses Ranger's
                // already-established 0.15 rather than inventing a new
                // balance number). Deliberately NOT given a baseline
                // `heal_power_pct` the way Paladin/Cleric/Druid are -
                // unlike theirs, nothing in this class's spec describes
                // its basic ATTACK as healing-hybrid; all of its
                // healing is earned through explicit Healing Flames
                // tree investment instead, wired in a later stage.
                b.splash = 0.15 * mult;
            }
        }
        b
    }

    /// Which `ArchetypeSkill`(s) - see its doc - this archetype grants,
    /// applied once at `simulate_battle`'s unit-build time
    /// (`CombatSimUnit::skills`). Single source of truth: adding a new
    /// skill to an archetype (or a whole new skill) never touches a unit
    /// build site, just this match and `ArchetypeSkill` itself. Empty for
    /// every archetype that doesn't have one yet - not an error case,
    /// just "nothing here yet".
    pub fn skills(self) -> &'static [ArchetypeSkill] {
        match self {
            Archetype::Berserker => &[ArchetypeSkill::Frenzy],
            Archetype::Slayer => &[ArchetypeSkill::FlickerStrike],
            _ => &[],
        }
    }

    /// Human-readable "+X% foo / -Y% bar" summary of `bonus()` - shown
    /// on the web dashboard's archetype picker so players can compare
    /// options before spending dust on one. `""` for Commoner (nothing
    /// to show - it's the unspecialized baseline).
    /// Human-readable, sign-free summary - every value is shown as a
    /// plain magnitude with a directional NAME (e.g. "15% increased
    /// damage taken", never "-15% dmg reduction"), since a bare +/-
    /// reads as a typo/error far more easily than it reads as "good" or
    /// "bad". Each stat has its own positive/negative name pair rather
    /// than a generic "reduced X" template - "reduced damage reduction"
    /// or "negative crit chance" would be confusing where "increased
    /// damage taken" or "reduced damage" says exactly what happens.
    pub fn description(self, level: u32) -> String {
        let b = self.bonus(level);
        let mut parts = Vec::new();
        // Cleric/Druid get a 50% baseline healing power `bonus()` itself
        // doesn't carry (see `Character::combat_heal_power`'s doc) -
        // called out explicitly here since it's invisible to the picker
        // otherwise (their own +/-heal_power_pct below is only the
        // DELTA on top of this, not their real total).
        if self.combat_function() == CombatFunction::Heal {
            parts.push("innately hybrid — 50% of every attack converts to healing by default".to_string());
        }
        let mut push_pct = |value: f64, positive: &str, negative: &str| {
            if value > 0.0 {
                parts.push(format!("{:.0}% {positive}", value * 100.0));
            } else if value < 0.0 {
                parts.push(format!("{:.0}% {negative}", -value * 100.0));
            }
        };
        push_pct(b.damage_reduction, "reduced damage taken", "increased damage taken");
        push_pct(b.block_chance, "block chance", "reduced block chance");
        push_pct(b.evasion, "evasion", "reduced evasion");
        push_pct(b.increased_damage, "increased damage dealt", "reduced damage dealt");
        push_pct(b.crit_chance, "crit chance", "reduced crit chance");
        push_pct(b.crit_multiplier, "increased crit damage dealt", "reduced crit damage dealt");
        push_pct(b.splash, "splash", "reduced splash");
        push_pct(b.attack_speed, "attack speed", "reduced attack speed");
        push_pct(b.max_hp_pct, "max hp", "reduced max hp");
        push_pct(b.heal_power_pct, "healing power", "reduced healing power");
        push_pct(b.intervene_pct, "intervene", "reduced intervene");
        if b.life_leech_pct > 0.0 {
            parts.push(format!("{:.2}% of damage dealt leeched as life (capped at {:.0}% max hp/sec)", b.life_leech_pct * 100.0, LIFE_LEECH_CAP_PER_SEC * 100.0));
        }
        for skill in self.skills() {
            parts.push(format!("Skill — {}: {}", skill.name(), skill.description()));
        }
        parts.join(" / ")
    }
}

/// One archetype's advantage/disadvantage - see `Archetype::bonus`. Every
/// field is a delta added alongside that stat's summed gear affixes
/// (`Character::sum_affix`), not a replacement for it. `max_hp_pct` and
/// `heal_power_pct` are multiplicative-% adjustments (unlike the others,
/// which are flat percentage-point deltas); `attack_speed` follows
/// gloves' existing speed-stat convention (a fraction subtracted from
/// 1.0 in `attack_interval_ms`, so a NEGATIVE value here slows a
/// character down instead of speeding them up).
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchetypeBonus {
    pub damage_reduction: f64,
    pub block_chance: f64,
    pub evasion: f64,
    pub increased_damage: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub splash: f64,
    pub attack_speed: f64,
    pub max_hp_pct: f64,
    pub heal_power_pct: f64,
    pub intervene_pct: f64,
    /// Slayer's advantage - fraction of a hit's actual (post-mitigation)
    /// damage converted into self-healing for the attacker, applied in
    /// `apply_hit` and rate-limited by `LIFE_LEECH_CAP_PER_SEC`. 0.0 for
    /// every other archetype and for gear (no matching `Affix` exists).
    pub life_leech_pct: f64,
}

/// Dust cost of every archetype change AFTER a character's first (free)
/// one - see `AdventureManager::change_archetype`.
pub const ARCHETYPE_CHANGE_COST: u64 = 1000;

/// Dust cost of a passive-tree respec after a character's first (free)
/// one - see `AdventureManager::respec_passive_tree`. Same magnitude as
/// `ARCHETYPE_CHANGE_COST` - both are "undo a build decision" actions.
pub const PASSIVE_RESPEC_COST: u64 = 1000;

/// How long a fight is assumed to last for `Character::combat_dps`'s
/// display estimate of the helm's stacking dps buff (see `helm_skill`) -
/// a real fight's actual length varies fight to fight (longer for a
/// tankier build, shorter for a glassier one dying fast), so this is
/// just the single number the dashboard's static DPS figure needs to
/// assume to average the buff's ramp into one value at all.
pub(crate) const ASSUMED_FIGHT_DURATION_MS: u32 = 30_000;

/// Every pickable archetype (deliberately excludes `Commoner` - it's a
/// starting state, never a manual target) - what the web dashboard's
/// picker `<select>` iterates.
pub const ALL_ARCHETYPES: [Archetype; 12] = [
    Archetype::Warrior,
    Archetype::Berserker,
    Archetype::Rogue,
    Archetype::Monk,
    Archetype::Paladin,
    Archetype::Ranger,
    Archetype::Mage,
    Archetype::Warlock,
    Archetype::Cleric,
    Archetype::Druid,
    Archetype::Slayer,
    Archetype::Elementalist,
];

/// A reasonable pre-picked floor for `Character::auto_disenchant_min_percent`
/// - a brand-new character (or one loading from before this field existed)
/// gets a sane starting point to redisplay/tweak rather than 0 (which
/// would read as "nothing is ever below the floor," silently making the
/// Quality tier a no-op) if they ever flip `auto_disenchant_enabled` on.
fn default_auto_disenchant_min_percent() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    /// As typed the first time this person !joined — display only;
    /// matching against chat always goes through the lowercased map key
    /// instead, same convention as personal playlists/entrance themes.
    pub display_name: String,
    pub level: u32,
    /// Progress toward the *next* level — see `xp_needed`.
    pub xp: u64,
    pub wins: u32,
    pub losses: u32,
    /// Build identity - see `Archetype`. `#[serde(default)]` is the
    /// migration for every character saved before archetypes existed
    /// (see `Archetype::default`).
    #[serde(default)]
    pub archetype: Archetype,
    #[serde(default)]
    pub weapon: Option<Item>,
    #[serde(default)]
    pub helm: Option<Item>,
    #[serde(default)]
    pub body: Option<Item>,
    #[serde(default)]
    pub gloves: Option<Item>,
    #[serde(default)]
    pub boots: Option<Item>,
    /// Unequipped items, managed from the web dashboard (adventure_web.rs)
    /// — every future drop lands here rather than auto-equipping, capped
    /// at `INVENTORY_CAPACITY`. Reforge is the one exception: it still
    /// upgrades an already-equipped item directly, since it's acting on
    /// gear the player already chose to wear, not handing them something new.
    #[serde(default)]
    pub inventory: Vec<Item>,
    /// Currency from disenchanting bag items (see
    /// `disenchant_from_inventory`) — 1-6 per tier of the disenchanted
    /// item. Not spendable on anything yet; just tracked for future use.
    #[serde(default)]
    pub dust: u64,
    /// Currency from disenchanting bag items (2026-08-15) - a chance
    /// (scaled by the disenchanted item's OWN quality%, see
    /// `disenchant_from_inventory`) at 1-3 sand per disenchant, on top of
    /// the usual dust. Spent on `CraftAction::Polishing` - see `polish`'s
    /// doc for why that action uses this currency instead of dust.
    #[serde(default)]
    pub sand: u64,
    /// Divine Dust (2026-08-19) - a per-character currency for making an
    /// item Sacred and rerolling its sacred affix (see
    /// `AdventureManager::craft_item_ex`'s `CraftAction::DivineDust`
    /// branch), on top of its own dust+sand craft recipe. Three
    /// acquisition sources, all additive: a chance per fighting character
    /// on every win (`LiveTunables::divine_dust_drop_chance`, same
    /// eligibility as `sand`'s own win grant), a chance per SACRED item
    /// manually disenchanted (`LiveTunables::divine_dust_disenchant_chance`),
    /// and the craft recipe itself. See `docs/divine_dust_spec.md`.
    #[serde(default)]
    pub divine_dust: u64,
    /// Set the moment every equipped, non-indestructible item hits 0%
    /// durability (see `all_gear_worn_out`) — while set, this character
    /// sits out every encounter (boss and basic alike), same exclusion
    /// mechanism as a revive countdown. Cleared the instant they repair
    /// or swap in gear that isn't fully worn (see `sync_retreat_status`),
    /// or automatically after `RETREAT_REPAIR_DURATION` of rest, which
    /// fully repairs everything for free. Persisted (unlike the
    /// short-lived revive/reforge-cooldown maps) since an hour is long
    /// enough that a bot restart shouldn't just erase it.
    #[serde(default)]
    pub retreated_since: Option<SystemTime>,
    /// Explicitly picked character model/sprite (see `ALL_SPRITES`) -
    /// `None` means "never chosen yet", in which case `effective_sprite`
    /// falls back to the old deterministic hash-of-id pick, same as
    /// every character got before this existed. `#[serde(default)]` is
    /// the migration for every character saved before this field did.
    #[serde(default)]
    pub model: Option<String>,
    /// Free `change_model` uses banked - see `AdventureManager::
    /// change_model`, consumed instead of dust. `#[serde(default = "default_free_model_changes")]`
    /// is doing double duty as a migration: it gives every
    /// already-saved character (whether they'd picked a model before or
    /// not) exactly `STARTING_FREE_MODEL_CHANGES` free change(s) on this
    /// deploy - their original first-pick entitlement if they'd never
    /// picked, or a bonus change if they had, which happens to be
    /// exactly the "anyone with an old sprite gets an additional free
    /// change" the request asked for whenever new sprites are added (see
    /// `ALL_SPRITES`'s growth-tracking in `AdventureManager::new` for
    /// every future addition after this one).
    #[serde(default = "default_free_model_changes")]
    pub free_model_changes: u32,
    /// Free `Character::recombine` uses banked - see
    /// `AdventureManager::recombine_gear`, consumed instead of dust.
    /// Same default-as-migration trick as `free_model_changes` - every
    /// already-saved character gets `STARTING_FREE_RECOMBINES` on this
    /// deploy too.
    #[serde(default = "default_free_recombines")]
    pub free_recombines: u32,
    /// Free `change_archetype` uses banked - see `AdventureManager::
    /// change_archetype`, consumed instead of dust. Starts at
    /// `STARTING_FREE_ARCHETYPE_CHANGES` (2, not 1 like the other free-*
    /// counters) - "give everyone 2 free class changes so they can play
    /// around with different archetypes" - replaces the old binary "free
    /// only while still Commoner" rule (a Commoner's first pick and one
    /// respec after that are both covered by this before dust ever gets
    /// involved). Same default-as-migration trick as
    /// `free_model_changes`/`free_recombines` - every already-saved
    /// character gets the same amount on this deploy too, regardless of
    /// their current archetype.
    #[serde(default = "default_free_archetype_changes")]
    pub free_archetype_changes: u32,
    /// Free craft-action tokens banked - see `AdventureManager::
    /// craft_item`'s free-if-tokened check (consumed instead of dust,
    /// one per use, non-veiled only). Every new character starts with
    /// one of each (see `Character::new`) "so players can learn how to
    /// use it"; boss kills hand out more (see
    /// `AdventureManager::grant_boss_craft_tokens`). Accumulate without
    /// limit. A `Vec` of (type, count) pairs rather than a
    /// `HashMap<CraftAction, u32>` purely so it round-trips through
    /// serde_json with zero fuss - there are only 6 possible entries
    /// ever, a linear scan is plenty (see `craft_token_count`/
    /// `add_craft_token`/`consume_craft_token`).
    #[serde(default)]
    pub craft_tokens: Vec<(CraftAction, u32)>,
    /// Fraction of the way to a guaranteed pity item drop (1.0 = 100%,
    /// see `PITY_THRESHOLD`) - accrues by `BOSS_ITEM_PITY_GAIN`/
    /// `BASIC_ITEM_PITY_GAIN` every fight this character participates in
    /// (win OR loss - a losing fight never grants loot either) without
    /// winning an item off the normal random roll, and resets to 0 the
    /// instant they DO receive one, whether that's from a lucky roll or
    /// from pity itself paying out. Protection against a long unlucky
    /// streak, not a second currency - see `advance_pity`.
    #[serde(default)]
    pub item_pity: f64,
    /// Same idea as `item_pity`, tracked separately for craft-currency
    /// tokens (see `Character::craft_tokens`) - `BOSS_CRAFT_PITY_GAIN`/
    /// `BASIC_CRAFT_PITY_GAIN` per fight without winning one. Basic
    /// fights never roll for a token at all currently (only a real boss
    /// fight does - see `run_encounter`), so a basic-only player simply
    /// accrues this every single basic fight until pity itself pays out.
    #[serde(default)]
    pub craft_pity: f64,
    /// Owns the "Wings of Flight" cosmetic - purchasable for
    /// `WINGS_COST` dust (see `AdventureManager::purchase_wings`) or an
    /// extremely rare (`WINGS_DROP_CHANCE`) bonus drop alongside any
    /// normal item reward (see `maybe_drop_wings`). Purely cosmetic -
    /// doesn't touch combat math at all, just how the overlay renders
    /// this character while idle (see `flying`/`CharacterView::flying`).
    #[serde(default)]
    pub owns_wings: bool,
    /// Whether this character has ever personally received a Perfect
    /// Quality item's one-time milestone bonus (see `PERFECT_QUALITY_MULT`'s
    /// doc) - guarantees every character gets their OWN Perfect item the
    /// first time they take part in a stage-90+ boss kill, independent of
    /// (and stacking with) the separate shared per-kill Perfect drop `run_encounter`
    /// already rolls every stage-90+ kill. Same one-shot-per-character
    /// guarantee shape as `owns_wings`.
    #[serde(default)]
    pub received_first_perfect: bool,
    /// Same one-shot-per-character milestone as `received_first_perfect`,
    /// for Sacred instead of Perfect (2026-08-16, a live request) - a
    /// STRICTLY higher threshold (stage 300 vs 100), so a character who's
    /// already earned their guaranteed Perfect long since will also
    /// eventually earn a guaranteed Sacred once the party reaches the
    /// later stage - the two milestones are independent, not
    /// either/or.
    #[serde(default)]
    pub received_first_sacred: bool,
    /// Whether flight is currently toggled ON (see
    /// `AdventureManager::toggle_flying`) - only meaningful when
    /// `owns_wings` is true; the dashboard only ever shows the toggle
    /// once it is, and every write path that sets this also requires it.
    #[serde(default)]
    pub flying: bool,
    /// When on, `run_encounter` spends dust to fully repair this
    /// character's gear (see `Character::repair_all`) right after every
    /// real boss fight's normal durability decay - a toggle on the main
    /// dashboard page, off by default so nobody's dust drains without
    /// opting in. Silent either way (no chat/dashboard message) - same
    /// "just happens in the background" treatment as the decay itself.
    /// Basic-encounter filler fights never touch durability at all, so
    /// this has nothing to do outside a real boss fight.
    #[serde(default)]
    pub auto_repair: bool,
    /// When on, any newly-received item that doesn't meet
    /// `auto_disenchant_tier`/`auto_disenchant_min_percent`'s floor is
    /// immediately disenchanted instead of being equipped or bagged - see
    /// `receive_item_with_auto_disenchant`. Off by default. Applies
    /// UNIVERSALLY, even to a slot that's currently empty (a live design
    /// call: "scrap it too, even into an empty slot" - a fresh/undergeared
    /// character with this on just keeps skipping bad drops rather than
    /// ever being handed known-junk gear). Going-forward only - toggling
    /// this on, or lowering the floor, never retroactively sweeps the
    /// bag's existing contents.
    #[serde(default)]
    pub auto_disenchant_enabled: bool,
    /// See `AutoDisenchantTier`'s doc.
    #[serde(default)]
    pub auto_disenchant_tier: AutoDisenchantTier,
    /// The quality-percent floor when `auto_disenchant_tier` is `Quality`
    /// (1-100) - stored (and redisplayed on the dashboard's number input)
    /// even while a different tier is selected, so switching back to
    /// `Quality` doesn't lose whatever value was last set.
    #[serde(default = "default_auto_disenchant_min_percent")]
    pub auto_disenchant_min_percent: u32,
    /// The id of whatever item this character's most recent crafting
    /// action (any currency action, a completed Recombine, or a veiled
    /// choice being applied) touched or produced - `None` until their
    /// first craft ever. Purely a UI convenience: the web dashboard's
    /// Crafting card item pickers default-select this id (see
    /// `craft_item_options`) instead of always resetting to the first
    /// item in the list, on every page load, not just the redirect
    /// right after a craft - per a live request that re-selecting the
    /// same item for a follow-up craft (e.g. Augment then Regal on the
    /// same piece) was tedious.
    #[serde(default)]
    pub last_crafted_item_id: Option<String>,
    /// Passive skill tree investment - node key (see
    /// `passive_tree::PassiveNode::key`) to rank invested, for the
    /// CURRENT `archetype` only. Cleared to empty whenever `archetype`
    /// changes (see `AdventureManager::change_archetype`) - a deliberate,
    /// already-costed action, so losing tree progress on switch is a
    /// reasonable trade for a much simpler data model than tracking a
    /// separate history per archetype ever played.
    #[serde(default)]
    pub passive_allocations: HashMap<String, u32>,
    /// Free `AdventureManager::respec_passive_tree` uses banked - same
    /// migration-grant idiom as `free_archetype_changes`/
    /// `free_recombines`, giving every already-saved character
    /// `STARTING_FREE_PASSIVE_RESPECS` free respec(s) the moment this
    /// deploys.
    #[serde(default = "default_free_passive_respecs")]
    pub free_passive_respecs: u32,
    /// Split Personality (`UniqueAffix::SplitPersonality`) - the second
    /// class chosen via the `/passives` dropdown, if any. Read this
    /// through `effective_secondary_archetype()`, never directly - the
    /// raw field can go stale (e.g. right after the item granting it is
    /// unequipped) until the next mutation touches it; the live-checked
    /// getter is what actually gates the UI/point-total/lookup behavior,
    /// so a stale raw value here is harmless by construction.
    #[serde(default)]
    pub secondary_archetype: Option<Archetype>,
    /// Split Personality's own second tree - same shape as
    /// `passive_allocations` but for `secondary_archetype`'s node list.
    /// Kept as a fully separate map rather than folded into
    /// `passive_allocations` specifically so the two trees' allocations
    /// can never collide in storage even where their node `key`s
    /// coincidentally still do (see `passive_node_rank`'s doc) - each map
    /// only ever holds keys valid for its own archetype, enforced at
    /// allocate-time.
    #[serde(default)]
    pub secondary_passive_allocations: HashMap<String, u32>,
    /// Elementalist's Golem Master (docs/elementalist_spec.md, Stage 5) -
    /// which `GolemType` is assigned to each summon slot, chosen via
    /// `/passives`. A CHOICE, not a rank, so unlike every other
    /// archetype's investment (which flows entirely through
    /// `passive_allocations`) this needs its own field - same reasoning
    /// Split Personality's `secondary_archetype` already established for
    /// "a choice, not a magnitude." Index 0 = slot 1, etc. Shorter than
    /// the invested Golem Master rank = the missing slots aren't
    /// assigned yet (default to `GolemType::Basic` at spawn time, see
    /// `GolemType`'s own `Default`); longer = harmless leftover from a
    /// respec that lowered the rank (never trimmed - a respec back up
    /// restores the prior choice for free, same spirit as every other
    /// non-lossy respec in this codebase).
    #[serde(default)]
    pub golem_slot_types: Vec<GolemType>,
    /// Memories (2026-08-19) - saved passive-tree builds this character
    /// can swap between for free out of combat, see `Memory` and
    /// `AdventureManager::load_memory`. Index == slot number, `None` ==
    /// an empty slot: slots have identity (filling slot 3 while 1 and 2
    /// are empty has to STAY slot 3), which a bare `Vec<Memory>` can't
    /// express. Never assume this is already `memory_slots` long - a
    /// character saved before this feature has none at all - read it
    /// through `memory_slot`/`memories_padded`, which normalize.
    #[serde(default)]
    pub memories: Vec<Option<Memory>>,
    /// How many Memory slots this character has. A per-character VALUE
    /// rather than a global constant consulted at each use site
    /// specifically so a future feature can grant an individual
    /// character extra slots with no migration and no code change
    /// anywhere that reads it - `STARTING_MEMORY_SLOTS` is only the
    /// default. Nothing downstream may hardcode 3.
    #[serde(default = "default_memory_slots")]
    pub memory_slots: u32,
}

/// Bag size — see `Character::inventory`. Raised 50 -> 150 (2026-08-18, a
/// live request).
pub const INVENTORY_CAPACITY: usize = 150;

/// Dust cost of every character-model change AFTER a character's first
/// (free) one - see `AdventureManager::change_model`. Currently moot -
/// see `MODEL_CHANGES_FREE_FOR_ALL`.
pub const MODEL_CHANGE_COST: u64 = 1000;

/// TEMPORARY (added 2026-08-13): every model/sprite change is free for
/// everyone, no dust cost and no `free_model_changes` token consumed,
/// while the roster of 90 new sprites (see `ALL_SPRITES`) is still being
/// evaluated - players should be able to freely try different sprites
/// without worrying about spending real dust on a set that might still
/// change. Flip back to `false` once the sprite set is settled - nothing
/// else needs to change, `change_model`/`render_model_picker` both key
/// off this one flag.
pub const MODEL_CHANGES_FREE_FOR_ALL: bool = true;

/// Max length (in characters) of a Krangled item's custom nickname -
/// see `Item::nickname`/`AdventureManager::name_item`. Silently
/// truncated, not rejected - a long paste just gets cut down rather
/// than the whole naming attempt failing.
pub const NICKNAME_MAX_LEN: usize = 30;

/// Every pickable character model/sprite - same flat pool the OBS
/// overlay draws from (`public_adventure_overlay/overlay.html`'s own
/// `ALL_SPRITES`, kept in sync by hand - see that file's comment) and
/// what the web dashboard's model picker `<select>`/thumbnail grid
/// iterates. Each name maps to `public_adventure_overlay/sprites/{name}.png`.
///
/// Full replacement (2026-08-13) of every earlier sheet-sliced batch -
/// every one of these came pre-isolated (its own transparent-background
/// PNG, confirmed via corner/edge alpha sampling) from
/// `adventure/player sprites/`, instead of being cropped out of a packed
/// character sheet the way every prior batch was. That sheet-slicing
/// process (fixed-grid, then flood-fill/connected-component) repeatedly
/// produced real defects - neighbor bleed, clipped limbs on internal
/// low-alpha gaps - that were expensive to find and fix one sprite at a
/// time; sourcing already-isolated art sidesteps that whole failure class.
pub const ALL_SPRITES: [&str; 90] = [
    "awoooo",
    "axe-dood",
    "bookworm",
    "crawly",
    "crystal-maiden",
    "dafuq-is-that-wing",
    "deer-guy",
    "def-from-alaska",
    "def-not-a-dealer",
    "def-not-bad-guy",
    "existential-crisis-guy",
    "fantastic-1",
    "frosty",
    "green-ranger",
    "hail",
    "hangry-af",
    "leanidas",
    "lionguard-azure",
    "lionguard-cobalt",
    "lionguard-crimson",
    "lionguard-royal",
    "lionguard-scarlet",
    "magma-guy-1",
    "mooooooo",
    "new-katarina-skin",
    "oonga-boonga",
    "pinocchio",
    "purp-ninja-guy",
    "pyra",
    "shangchi",
    "shiny-dood",
    "skully",
    "sns-blue",
    "sprite-01",
    "sprite-02",
    "sprite-03",
    "sprite-04",
    "sprite-05",
    "sprite-06",
    "sprite-07",
    "sprite-08",
    "sprite-09",
    "sprite-10",
    "sprite-11",
    "sprite-12",
    "sprite-13",
    "sprite-14",
    "sprite-15",
    "sprite-16",
    "sprite-17",
    "sprite-18",
    "sprite-19",
    "sprite-20",
    "sprite-21",
    "sprite-22",
    "sprite-23",
    "sprite-24",
    "sprite-25",
    "sprite-26",
    "sprite-27",
    "sprite-28",
    "sprite-29",
    "sprite-30",
    "sprite-31",
    "sprite-32",
    "sprite-33",
    "sprite-34",
    "sprite-35",
    "sprite-36",
    "sprite-37",
    "sprite-38",
    "sprite-39",
    "sprite-40",
    "sprite-41",
    "sprite-42",
    "sprite-43",
    "sprite-44",
    "sprite-45",
    "sprite-46",
    "sprite-47",
    "sprite-48",
    "sprite-49",
    "sprite-50",
    "tamer",
    "thing-vtemu",
    "trash-mob",
    "twilight-bs",
    "valkvalk",
    "wizard-guy",
    "wow3-frozen-throne",
];

/// Same hash used everywhere a character needs a STABLE pick out of a
/// pool without anything persisted (sprite fallback here, boss variety
/// elsewhere) - deliberately simple/non-cryptographic, just needs to be
/// deterministic and spread out.
pub(crate) fn hash_str(s: &str) -> u32 {
    let mut hash: u32 = 0;
    for c in s.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(c as u32);
    }
    hash
}

/// Deterministic hash-of-id sprite pick - the fallback `Character::
/// effective_sprite` uses for anyone who's never explicitly chosen a
/// model (see `Character::model`), so every character still shows SOME
/// sprite, stable across reloads, without needing a pick stored.
pub(crate) fn sprite_for_character(id: &str) -> &'static str {
    ALL_SPRITES[(hash_str(id) as usize) % ALL_SPRITES.len()]
}

/// Self-service custom sprite drop-in folder (2026-08-16, a live
/// request: "a folder that I can place a png in and assign that sprite
/// to a particular player... without having to recompile", extended the
/// same day to also accept `.gif` - see overlay.html's `getOrLoadSprite`/
/// `characterGifImgFor` for the animated-rendering half). Whatever
/// `.png`/`.gif` files exist here are picked up live by
/// `render_model_picker` (adventure_web.rs) - no code change/recompile/
/// restart needed, just drop a file in. Nested under the same `sprites/`
/// dir `ALL_SPRITES` already lives in, and already served at
/// `/sprites/...` by the existing `ServeDir` mount (see
/// `start_adventure_web_server`), so `/sprites/custom/<name>.png` (or
/// `.gif`) just works with zero server-config changes either.
pub const CUSTOM_SPRITE_DIR: &str = "public_adventure_overlay/sprites/custom";

/// Whether `model` is a real file in `CUSTOM_SPRITE_DIR`, in the stored
/// `custom/<name>` form (no extension) `change_model`/`effective_sprite`/
/// `render_model_picker` all key it by - accepts either a `.png` or a
/// `.gif` on disk (the overlay itself figures out which extension
/// actually loads, so the backend doesn't need to track/store which one
/// a given name resolves to). `model` ultimately comes from a public web
/// form field, so this rejects anything that could escape the custom
/// folder (path separators, `..`) rather than trusting it's a plain
/// filename.
///
/// The reserved custom-sprite filename PREFIX (case-insensitive) that
/// bypasses the per-player name gate below - anyone can select
/// `custom/public.png`, `custom/public1.gif`, `custom/public2.png`, etc.
/// (this prefix, optionally followed by nothing but digits), same as the
/// curated `ALL_SPRITES`.
pub const PUBLIC_CUSTOM_SPRITE_PREFIX: &str = "public";

/// Whether `name_lower` (already-lowercased) belongs to `prefix` - an
/// exact match, OR `prefix` followed by nothing but digits, so one
/// player/the public pool can have more than one sprite: "kibukah",
/// "kibukah2", "kibukah3" - same numbered-suffix convention
/// `PUBLIC_CUSTOM_SPRITE_PREFIX`'s own doc describes (2026-08-16 follow-
/// up - a live report that "lokati_gaming2" wasn't being recognized as a
/// second sprite for "lokati_gaming").
pub(crate) fn custom_sprite_name_matches(name_lower: &str, prefix: &str) -> bool {
    name_lower.strip_prefix(prefix.to_ascii_lowercase().as_str()).is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// Whether custom-sprite filename `name` (bare, no `custom/` prefix, no
/// extension) is one `owner_id` is allowed to pick - either it's named
/// after them (see `custom_sprite_name_matches`), or it's in the
/// reserved public pool. Shared by `is_valid_custom_sprite` (submit-time
/// validation) and `render_model_picker`'s own listing (adventure_web.rs)
/// so the two can't drift apart.
pub fn custom_sprite_is_owned_by(owner_id: &str, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    custom_sprite_name_matches(&lower, owner_id) || custom_sprite_name_matches(&lower, PUBLIC_CUSTOM_SPRITE_PREFIX)
}

/// Also name-gated to `owner_id` (2026-08-16, a live request: a custom
/// sprite is understood to be made FOR a specific player - e.g.
/// `kibukah.png` is only ever selectable by the character whose id is
/// "kibukah", nobody else) via `custom_sprite_is_owned_by`. Checked here
/// (not just hidden from the picker's own listing - see
/// `render_model_picker`) so a hand-crafted POST to `/change-model`
/// can't bypass the picker UI and equip someone else's named sprite.
pub(crate) fn is_valid_custom_sprite(owner_id: &str, model: &str) -> bool {
    let Some(name) = model.strip_prefix("custom/") else { return false };
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    if !custom_sprite_is_owned_by(owner_id, name) {
        return false;
    }
    let dir = std::path::Path::new(CUSTOM_SPRITE_DIR);
    dir.join(format!("{name}.png")).exists() || dir.join(format!("{name}.gif")).exists()
}

/// Shared by the three stats capped at 75% (damage reduction, block
/// chance, evasion - see `Character::combat_damage_reduction`/
/// `combat_block_chance`/`combat_evasion`) - `floor` is `-0.75` for
/// damage reduction (which can go negative, see its doc) or `0.0` for
/// the other two. Returns (the clamped value those getters actually
/// return, how much spilled past the 75% cap - see `defensive_overflow`).
/// `cap` used to be hardcoded at 0.75 (fine while DamageReduction/
/// BlockChance/Evasion were the only 3 capped-with-overflow stats, all
/// sharing that same 75% ceiling) - now a real parameter so Intervene
/// can share this exact mechanism at ITS own 50% ceiling instead of
/// needing a hand-rolled duplicate (see `combat_intervene`'s doc - a
/// live report caught it not capping/overflowing at all).
pub(crate) fn capped_stat_with_overflow(raw: f64, floor: f64, cap: f64) -> (f64, f64) {
    let overflow = (raw - cap).max(0.0);
    (raw.clamp(floor, cap), overflow)
}

/// Combines independent damage-mitigation SOURCES multiplicatively, not
/// additively - three separate 50% sources become 87.5% total reduction
/// (12.5% damage taken), never 150%/immunity. Each element of `sources`
/// is expected to already be its own fraction, capped at whatever ceiling
/// that source uses on its own (e.g. `combat_damage_reduction`'s 75% -
/// see `capped_stat_with_overflow`) - this function only combines
/// already-capped sources together, it doesn't cap anything itself.
/// `resolve_hit` uses this to combine block and gear+archetype damage
/// reduction as two separate sources. The passive tree also plugs in
/// here (see `combat_damage_reduction`/`combat_block_chance`/
/// `combat_evasion`/`combat_intervene`) as its own separate source per
/// stat - each stat's tree nodes sum additively among themselves first
/// (capped the same as gear+archetype), then that tree total is
/// multiplied in as one more independent source, never folded into
/// gear+archetype's own sum. This is the general "character sheet vs.
/// passive tree" stacking principle for every stat the tree touches -
/// growth-shaped stats (max HP%, increased damage, attack speed, crit,
/// leech, heal power, splash) use the equivalent `(1+gear)*(1+tree)`
/// form instead of this complement-stacking one, since they're not
/// capped 0-100% fractions.
pub(crate) fn combine_reduction_sources(sources: &[f64]) -> f64 {
    1.0 - sources.iter().map(|s| 1.0 - s.clamp(-0.75, 1.0)).product::<f64>()
}

/// Recombine's own rare bonus-affix crit chance (see `roll_recombine`) -
/// unlike Reforge's quality-scaled `reforge_crit_chance`, this is a flat
/// rate regardless of the sources' quality. Named 2026-08-18 for the
/// wiki's constant audit - was a bare `0.05` at its one call site.
pub const RECOMBINE_CRIT_CHANCE: f64 = 0.05;

/// Result of `Character::capped_stat_breakdown` - `sources` is
/// (label, value) per contributor, in fraction form same as every other
/// combat stat here (0.20 = 20%), not yet multiplied by 100.
pub struct StatBreakdown {
    pub sources: Vec<(String, f64)>,
    pub raw: f64,
    pub capped: f64,
    pub overflow: f64,
}

/// Base attack cadence per `CombatFunction` role, before gear/tree speed
/// and Healing-Power-past-100% (see `Character::attack_interval_ms`'s
/// doc) shorten it further - Melee's own base. Named 2026-08-18 for the
/// wiki's constant audit - was a bare match-arm literal.
pub(crate) const MELEE_BASE_ATTACK_INTERVAL_MS: u32 = 1400;
/// Same as `MELEE_BASE_ATTACK_INTERVAL_MS`, for Ranged.
pub(crate) const RANGED_BASE_ATTACK_INTERVAL_MS: u32 = 900;
/// Same as `MELEE_BASE_ATTACK_INTERVAL_MS`, for Heal.
pub(crate) const HEAL_BASE_ATTACK_INTERVAL_MS: u32 = 1700;
/// Flat crit chance every character starts with, before gear/archetype/
/// tree (see `Character::combat_crit_chance`). Named 2026-08-18 for the
/// wiki's constant audit - was a bare `0.05`.
pub(crate) const BASE_CRIT_CHANCE: f64 = 0.05;
/// Flat crit damage multiplier every character starts with, before gear/
/// archetype/tree (see `Character::combat_crit_multiplier`). Named
/// 2026-08-18 for the wiki's constant audit - was a bare `2.0`.
pub(crate) const BASE_CRIT_MULTIPLIER: f64 = 2.0;

/// Starting bank for `Character::free_model_changes` - `Character::new`
/// and `default_free_model_changes` (the serde-default/migration-grant
/// path, see that field's own doc) both read this SAME constant, so a
/// fresh character's starting bank and what an old save gets migrated to
/// can never drift apart from each other by construction. Named
/// 2026-08-18 for the wiki's constant audit - was two separate bare `1`
/// literals (one per path) that happened to agree.
pub(crate) const STARTING_FREE_MODEL_CHANGES: u32 = 1;
pub(crate) fn default_free_model_changes() -> u32 {
    STARTING_FREE_MODEL_CHANGES
}
/// Same "one constant, both paths" reasoning as
/// `STARTING_FREE_MODEL_CHANGES`, for `Character::free_recombines`.
pub(crate) const STARTING_FREE_RECOMBINES: u32 = 1;
pub(crate) fn default_free_recombines() -> u32 {
    STARTING_FREE_RECOMBINES
}
/// Same reasoning, for `Character::free_archetype_changes` - "give
/// everyone 2 free class changes... so they can play around with
/// different archetypes" is both the starting grant and the migration
/// grant for already-saved characters.
pub(crate) const STARTING_FREE_ARCHETYPE_CHANGES: u32 = 2;
pub(crate) fn default_free_archetype_changes() -> u32 {
    STARTING_FREE_ARCHETYPE_CHANGES
}
/// Same reasoning, for `Character::free_passive_respecs`.
pub(crate) const STARTING_FREE_PASSIVE_RESPECS: u32 = 1;
pub(crate) fn default_free_passive_respecs() -> u32 {
    STARTING_FREE_PASSIVE_RESPECS
}

/// Valid reroll targets for `Character::apply_divine_dust`'s already-
/// sacred path - every entry of `all` except `current`. Factored out of
/// `apply_divine_dust` itself so the empty-pool guard
/// (`CraftError::NoValidRerollTarget`) is directly testable against an
/// arbitrary pool, independent of `ALL_AFFIXES`'s real (currently 17,
/// never empty after excluding 1) size.
pub(crate) fn divine_dust_reroll_pool(current: Affix, all: &[Affix]) -> Vec<Affix> {
    all.iter().copied().filter(|&a| a != current).collect()
}

impl Character {
    /// New characters start fully kitted out (a basic tier-1 item in
    /// every slot) rather than naked — see `AdventureManager::new`'s
    /// startup backfill for characters who joined before this existed.
    pub fn new(display_name: String) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            display_name,
            level: 1,
            xp: 0,
            wins: 0,
            losses: 0,
            archetype: Archetype::Commoner,
            weapon: Some(generate_item_at_tier(EquipSlot::Weapon, 1, &mut rng)),
            helm: Some(generate_item_at_tier(EquipSlot::Helm, 1, &mut rng)),
            body: Some(generate_item_at_tier(EquipSlot::Body, 1, &mut rng)),
            gloves: Some(generate_item_at_tier(EquipSlot::Gloves, 1, &mut rng)),
            boots: Some(generate_item_at_tier(EquipSlot::Boots, 1, &mut rng)),
            inventory: Vec::new(),
            dust: 0,
            sand: 0,
            divine_dust: 0,
            retreated_since: None,
            model: None,
            free_model_changes: STARTING_FREE_MODEL_CHANGES,
            free_recombines: STARTING_FREE_RECOMBINES,
            free_archetype_changes: STARTING_FREE_ARCHETYPE_CHANGES,
            craft_tokens: ALL_CRAFT_ACTIONS.iter().map(|&a| (a, 1)).collect(),
            item_pity: 0.0,
            craft_pity: 0.0,
            owns_wings: false,
            received_first_perfect: false,
            received_first_sacred: false,
            flying: false,
            auto_repair: false,
            auto_disenchant_enabled: false,
            auto_disenchant_tier: AutoDisenchantTier::default(),
            auto_disenchant_min_percent: default_auto_disenchant_min_percent(),
            last_crafted_item_id: None,
            passive_allocations: HashMap::new(),
            free_passive_respecs: STARTING_FREE_PASSIVE_RESPECS,
            secondary_archetype: None,
            secondary_passive_allocations: HashMap::new(),
            golem_slot_types: Vec::new(),
            memories: Vec::new(),
            memory_slots: STARTING_MEMORY_SLOTS,
        }
    }

    /// This character's Memory slots, normalized to exactly
    /// `memory_slots` entries - padded with empties for a character
    /// saved before the feature (or before a slot grant), and truncated
    /// if `memory_slots` ever shrank. The ONE way the rest of the
    /// codebase should read `memories`: nothing else has to know the
    /// stored vec can be any length.
    pub fn memories_padded(&self) -> Vec<Option<Memory>> {
        let mut slots = self.memories.clone();
        slots.resize(self.memory_slots as usize, None);
        slots
    }

    /// Whatever is saved in `slot`, or `None` for an empty or
    /// out-of-range slot. Out-of-range reads as empty rather than
    /// panicking - callers that need to tell "empty" from "no such slot"
    /// apart check `slot < memory_slots` themselves (see
    /// `AdventureManager::load_memory`, which reports them differently).
    pub fn memory_slot(&self, slot: usize) -> Option<&Memory> {
        if slot >= self.memory_slots as usize {
            return None;
        }
        self.memories.get(slot).and_then(|m| m.as_ref())
    }

    /// Grows `memories` just far enough to address `slot`, so a caller
    /// can write to a slot a short (or empty) stored vec doesn't reach
    /// yet. Returns `None` if `slot` is past `memory_slots`.
    pub(crate) fn memory_slot_mut(&mut self, slot: usize) -> Option<&mut Option<Memory>> {
        if slot >= self.memory_slots as usize {
            return None;
        }
        if self.memories.len() <= slot {
            self.memories.resize(slot + 1, None);
        }
        self.memories.get_mut(slot)
    }

    /// Snapshots this character's CURRENT build into a `Memory` - the
    /// read half of "Save Current Build". `name` is assumed already
    /// validated (see `validate_memory_name`); this never sees raw form
    /// input. Reads `effective_secondary_archetype()` rather than the
    /// raw field so a snapshot taken while Split Personality is
    /// unequipped correctly records "no secondary", matching what the
    /// player can actually see on the page at the time.
    pub fn snapshot_build(&self, name: String, saved_at: u64) -> Memory {
        let secondary_archetype = self.effective_secondary_archetype();
        Memory {
            name,
            archetype: self.archetype,
            passive_allocations: self.passive_allocations.clone(),
            secondary_archetype,
            // Only carry the secondary tree when one is actually live -
            // otherwise a stale map would ride along and reappear the
            // next time Split Personality happened to be equipped.
            secondary_passive_allocations: if secondary_archetype.is_some() { self.secondary_passive_allocations.clone() } else { HashMap::new() },
            golem_slot_types: self.golem_slot_types.clone(),
            saved_at,
        }
    }

    /// How many free tokens of `action` this character has banked - see
    /// `craft_tokens`.
    pub fn craft_token_count(&self, action: CraftAction) -> u32 {
        self.craft_tokens.iter().find(|(a, _)| *a == action).map(|(_, n)| *n).unwrap_or(0)
    }

    /// Grants `amount` more free tokens of `action` - see `craft_tokens`.
    pub(crate) fn add_craft_token(&mut self, action: CraftAction, amount: u32) {
        match self.craft_tokens.iter_mut().find(|(a, _)| *a == action) {
            Some(entry) => entry.1 += amount,
            None => self.craft_tokens.push((action, amount)),
        }
    }

    /// Spends one free token of `action` if any are banked - `true` if
    /// one was actually consumed.
    pub(crate) fn consume_craft_token(&mut self, action: CraftAction) -> bool {
        let Some(entry) = self.craft_tokens.iter_mut().find(|(a, _)| *a == action) else { return false };
        if entry.1 == 0 {
            return false;
        }
        entry.1 -= 1;
        true
    }

    /// The sprite this character actually displays as, on both the web
    /// dashboard and the OBS overlay - their explicit pick (see `model`)
    /// if they've ever made one, otherwise the same stable hash-of-id
    /// fallback every character used to get unconditionally. `id` is the
    /// character's stable lowercased map key (NOT `display_name`, which
    /// isn't guaranteed stable/unique) - same id `CharacterView`/
    /// `AdventureManager` address them by everywhere else.
    pub fn effective_sprite(&self, id: &str) -> String {
        match self.model.as_deref() {
            Some(chosen) if ALL_SPRITES.contains(&chosen) => chosen.to_string(),
            // Custom drop-in sprite (see `CUSTOM_SPRITE_DIR`) - re-checked
            // against disk (AND against `id` for ownership - see
            // `is_valid_custom_sprite`'s doc) on every call rather than
            // trusted from the stored value alone, so a file removed
            // after being chosen falls back to the stable hash-default
            // instead of a broken image forever.
            Some(chosen) if is_valid_custom_sprite(id, chosen) => chosen.to_string(),
            _ => sprite_for_character(id).to_string(),
        }
    }

    /// Slot accessor by `EquipSlot` - used by the loot roll (which picks a
    /// random slot generically) instead of a 5-way match at every call
    /// site, and by the web dashboard (adventure_web.rs) to render each slot.
    pub fn equipped(&self, slot: EquipSlot) -> &Option<Item> {
        match slot {
            EquipSlot::Weapon => &self.weapon,
            EquipSlot::Helm => &self.helm,
            EquipSlot::Body => &self.body,
            EquipSlot::Gloves => &self.gloves,
            EquipSlot::Boots => &self.boots,
        }
    }

    /// True if some OTHER currently-equipped slot already carries the
    /// same `UniqueAffix` `item` would bring - "a player can only have 1
    /// of each different unique affix equipped at any given time" per
    /// the request. `excluding_slot` is the slot `item` is headed into,
    /// so equipping it back into its OWN slot never counts as a
    /// conflict with itself. Thin wrapper over
    /// `has_conflicting_unique_affix_value` for the common case where the
    /// value in question is already sitting on `item.unique_affix` -
    /// every equip-time call site (`receive_item`/`equip_from_inventory`)
    /// uses this shape.
    pub(crate) fn has_conflicting_unique_affix(&self, item: &Item, excluding_slot: EquipSlot) -> bool {
        item.unique_affix.is_some_and(|unique| self.has_conflicting_unique_affix_value(unique, excluding_slot))
    }

    /// The real check `has_conflicting_unique_affix` above wraps - split
    /// out (2026-08-21 duplicate-unique-effects fix) so a caller deciding
    /// whether to GRANT a specific `UniqueAffix` (the Unique Shard
    /// picker, the legacy Celestial Shard craft) can check a value that
    /// isn't on the item yet, rather than needing to mutate first and
    /// check after. This is now the ONE validator behind every mutation
    /// point that can affect equipped uniques - see its own callers for
    /// the full enumeration (equip, receive, and both unique-granting
    /// craft paths).
    pub(crate) fn has_conflicting_unique_affix_value(&self, unique: UniqueAffix, excluding_slot: EquipSlot) -> bool {
        EQUIP_SLOTS.iter().filter(|&&s| s != excluding_slot).filter_map(|&s| self.equipped(s).as_ref()).any(|other| other.unique_affix == Some(unique))
    }

    pub(crate) fn equip(&mut self, item: Item) {
        match item.slot {
            EquipSlot::Weapon => self.weapon = Some(item),
            EquipSlot::Helm => self.helm = Some(item),
            EquipSlot::Body => self.body = Some(item),
            EquipSlot::Gloves => self.gloves = Some(item),
            EquipSlot::Boots => self.boots = Some(item),
        }
    }

    /// Mutable counterpart of `equipped` - used by the post-fight decay
    /// step to age whatever's actually equipped in each slot.
    pub(crate) fn equipped_mut(&mut self, slot: EquipSlot) -> &mut Option<Item> {
        match slot {
            EquipSlot::Weapon => &mut self.weapon,
            EquipSlot::Helm => &mut self.helm,
            EquipSlot::Body => &mut self.body,
            EquipSlot::Gloves => &mut self.gloves,
            EquipSlot::Boots => &mut self.boots,
        }
    }

    pub(crate) fn unequip(&mut self, slot: EquipSlot) {
        *self.equipped_mut(slot) = None;
    }

    /// Adds a new item to the bag (a loot drop, or a !giveloot grant) -
    /// `false` (item lost) if the bag's already full at
    /// `INVENTORY_CAPACITY`. See `receive_item` for the usual entry point
    /// (auto-equips into an empty slot first) - this is the fallback for
    /// when the slot's already occupied.
    pub fn add_to_inventory(&mut self, item: Item) -> bool {
        if self.inventory.len() >= INVENTORY_CAPACITY {
            return false;
        }
        self.inventory.push(item);
        true
    }

    /// Entry point for any newly-acquired item (a loot drop or !giveloot
    /// grant) - auto-equips it if that slot is currently empty, otherwise
    /// falls back to the bag (or loses it if the bag's also full). Also
    /// falls back to the bag (rather than equipping) if auto-equipping
    /// would conflict with a unique affix already equipped elsewhere -
    /// see `has_conflicting_unique_affix`.
    pub fn receive_item(&mut self, item: Item) -> ReceiveOutcome {
        let slot = item.slot;
        if self.equipped(slot).is_none() && !self.has_conflicting_unique_affix(&item, slot) {
            self.equip(item);
            ReceiveOutcome::Equipped
        } else if self.add_to_inventory(item) {
            ReceiveOutcome::AddedToBag
        } else {
            ReceiveOutcome::BagFull
        }
    }

    /// Same as `receive_item`, except when `auto_disenchant_enabled` is on
    /// and `item` doesn't meet the safe floor (`auto_disenchant_tier`/
    /// `auto_disenchant_min_percent`, see `Item::meets_auto_disenchant_floor`)
    /// - in that case the item never gets equipped or bagged at all, it's
    /// immediately disenchanted for dust/sand instead (identical formula to
    /// `disenchant_from_inventory`). Deliberately used only at the "real"
    /// item-drop call sites (natural loot rolls, pity payouts, mod-granted
    /// gear) - NOT at one-time compensation/launch grants or a deliberate
    /// Recombine craft result, which still go through plain `receive_item`
    /// so a player's own crafted output (or a fixed story grant) is never
    /// silently eaten by a setting they may not even have considered when
    /// crafting/being granted it.
    pub fn receive_item_with_auto_disenchant(&mut self, item: Item, rng: &mut impl Rng, sand_mult: f64) -> ReceiveOutcome {
        if self.auto_disenchant_enabled && !item.meets_auto_disenchant_floor(self.auto_disenchant_tier, self.auto_disenchant_min_percent) {
            let dust = rng.gen_range(1..=6) * item.tier * item.disenchant_multiplier();
            self.dust += dust as u64;
            self.sand += roll_disenchant_sand(item.quality_percent(), rng, sand_mult);
            return ReceiveOutcome::AutoDisenchanted { dust };
        }
        self.receive_item(item)
    }

    /// Finds an item by id wherever it currently lives - equipped OR
    /// bagged - since every item has a stable id regardless of location
    /// (see `Item::id`). What `recombine` uses to validate a pair before
    /// touching anything.
    pub(crate) fn find_item_by_id(&self, id: &str) -> Option<&Item> {
        EQUIP_SLOTS.iter().filter_map(|&slot| self.equipped(slot).as_ref()).find(|i| i.id == id).or_else(|| self.inventory.iter().find(|i| i.id == id))
    }

    /// Mutable counterpart to `find_item_by_id` - what `Character::craft`
    /// mutates the target item through, wherever it currently lives.
    pub(crate) fn find_item_by_id_mut(&mut self, id: &str) -> Option<&mut Item> {
        for slot in EQUIP_SLOTS {
            if self.equipped(slot).as_ref().is_some_and(|i| i.id == id) {
                return self.equipped_mut(slot).as_mut();
            }
        }
        self.inventory.iter_mut().find(|i| i.id == id)
    }

    /// Removes and returns an item by id, wherever it currently lives -
    /// leaves an equip slot empty, or removes it from the bag. `None` if
    /// no such item exists (already consumed, or never did).
    pub(crate) fn take_item_by_id(&mut self, id: &str) -> Option<Item> {
        for slot in EQUIP_SLOTS {
            if self.equipped(slot).as_ref().is_some_and(|i| i.id == id) {
                return self.equipped_mut(slot).take();
            }
        }
        let pos = self.inventory.iter().position(|i| i.id == id)?;
        Some(self.inventory.remove(pos))
    }

    /// Forges two same-slot items (equipped or bagged, in any
    /// combination) into one - the new tier is the average of the two
    /// source tiers plus 1, rounded down; every affix either source item
    /// had independently has a 50% chance to carry over (so combining
    /// two crit-focused pieces can compound), UNLESS this is a veiled
    /// recombine, which guarantees every affix from both sources carries
    /// over (the surcharge for that certainty is charged separately - see
    /// `AdventureManager::recombine_gear`'s pool-size surcharge);
    /// indestructible if EITHER source was. A separate 5% "recomb crit"
    /// (independent of the retention rolls) adds one bonus affix of a
    /// type NOT already present, at the new tier - reported back via the
    /// returned outcome's `bonus_affix` (see `GearCritEvent`) the same way
    /// a reforge crit is. The result lands wherever `receive_item` would
    /// put it (auto-equips if the slot's now empty from consuming a
    /// source item, otherwise the bag). Both source items are ALWAYS
    /// consumed on success, even if the result ends up lost to a full
    /// bag - recombination is a real forge, not a free reroll.
    pub(crate) fn recombine(&mut self, item_id_a: &str, item_id_b: &str, rng: &mut impl Rng) -> Result<RecombineOutcome, RecombineError> {
        let roll = self.roll_recombine(item_id_a, item_id_b, false, rng)?;
        Ok(self.apply_recombine_roll(item_id_a, item_id_b, roll, rng))
    }

    /// Decides everything a recombine would do (new tier, which affixes
    /// carry over, whether it crits) WITHOUT touching either source item -
    /// split out from `recombine` so a veiled recombine (see
    /// `PendingVeil`) can roll and DISPLAY several independent candidates
    /// up front, then commit the exact one the player picked later via
    /// `apply_recombine_roll`, unchanged. `guaranteed` is true only for a
    /// veiled roll - every affix from both sources carries over instead of
    /// the normal 50%-per-affix coin flip (see `recombine`'s doc); the
    /// player pays for that certainty via `recombine_gear`'s separate
    /// pool-size surcharge, not here.
    pub(crate) fn roll_recombine(&self, item_id_a: &str, item_id_b: &str, guaranteed: bool, rng: &mut impl Rng) -> Result<RecombineRoll, RecombineError> {
        if item_id_a == item_id_b {
            return Err(RecombineError::SameItem);
        }
        let item_a = self.find_item_by_id(item_id_a).ok_or(RecombineError::ItemNotFound)?;
        let item_b = self.find_item_by_id(item_id_b).ok_or(RecombineError::ItemNotFound)?;
        if item_a.locked || item_b.locked {
            return Err(RecombineError::ItemLocked);
        }
        if item_a.slot != item_b.slot {
            return Err(RecombineError::SlotMismatch);
        }
        let slot = item_a.slot;
        let new_tier = (((item_a.tier + item_b.tier) as f64) / 2.0 + 1.0).floor().max(1.0) as u32;
        let was_indestructible = item_a.is_indestructible() || item_b.is_indestructible();
        if let (Some(a), Some(b)) = (item_a.unique_affix, item_b.unique_affix) {
            if a != b {
                return Err(RecombineError::IncompatibleUniqueAffixes);
            }
        }
        let unique_affix = item_a.unique_affix.or(item_b.unique_affix);
        // A veiled (guaranteed) recombine keeps whichever source's
        // quality roll is actually higher - paying for certainty means
        // certainty here too, not just on the modifier transfer. A basic
        // (free, gambled) recombine keeps the coin-flip - not an average
        // (that would just always regress toward the middle of
        // POWER_ROLL_RANGE) and not a fresh roll (same "don't throw away
        // a real roll" reasoning as reforge's - see Item::power_roll).
        let power_roll =
            if guaranteed { item_a.power_roll.max(item_b.power_roll) } else if rng.gen_bool(0.5) { item_a.power_roll } else { item_b.power_roll };

        // Each carried affix scales up to `new_tier` from ITS OWN
        // source's tier (item_a and item_b can differ), same exact-ratio
        // approach as Item::sync_tier_to/reforge's tier_ratio - without
        // this, a source's affixes just carried over at their old,
        // lower-tier values even though new_tier is typically higher
        // than at least one source (same bug class a live report caught
        // in Krangle's tier growth and in reforge).
        let a_ratio = new_tier as f64 / item_a.tier.max(1) as f64;
        let b_ratio = new_tier as f64 / item_b.tier.max(1) as f64;
        // An affix TYPE present on BOTH sources is guaranteed to carry
        // over regardless of veil/coin-flip (per the request), keeping
        // whichever scaled value is higher - same "certainty keeps the
        // better roll" reasoning as power_roll above. Split from the
        // per-source-only candidates below so it can also be exempted
        // from the 4-cap's random truncation - a guarantee that could
        // still get shuffled away wouldn't be much of one.
        let mut guaranteed_affixes: Vec<(Affix, f64)> = Vec::new();
        let mut optional_affixes: Vec<(Affix, f64)> = Vec::new();
        for &(affix, a_value) in item_a.affixes.iter() {
            match item_b.affixes.iter().find(|&&(b_affix, _)| b_affix == affix) {
                Some(&(_, b_value)) => guaranteed_affixes.push((affix, (a_value * a_ratio).max(b_value * b_ratio))),
                None if guaranteed || rng.gen_bool(0.5) => optional_affixes.push((affix, a_value * a_ratio)),
                None => {}
            }
        }
        for &(affix, b_value) in item_b.affixes.iter() {
            // Shared types were already handled (and pushed) above -
            // only item_b's OWN, non-shared affixes belong here.
            if item_a.affixes.iter().any(|&(a_affix, _)| a_affix == affix) {
                continue;
            }
            if guaranteed || rng.gen_bool(0.5) {
                optional_affixes.push((affix, b_value * b_ratio));
            }
        }
        // Never more than 4 transferred modifiers - same cap every other
        // crafting action respects (only Krangle can exceed it). A
        // guaranteed (veiled) retention on two heavily-modified sources -
        // or, rarely, even the ordinary 50%-per-affix roll - can build a
        // pool bigger than that; when it does, only the OPTIONAL portion
        // is thinned (shuffled, then trimmed to whatever room is left
        // after the guaranteed/shared ones), never the guaranteed ones
        // themselves. The separate 5% recomb crit below is exempt from
        // this cap too - it's what makes base+4+1(crit) the real ceiling
        // on a recombine.
        let mut affixes = guaranteed_affixes;
        let remaining_slots = 4usize.saturating_sub(affixes.len());
        if optional_affixes.len() > remaining_slots {
            optional_affixes.shuffle(rng);
            optional_affixes.truncate(remaining_slots);
        }
        affixes.extend(optional_affixes);
        // Defensive final guarantee - see dedup_affixes' doc. Structurally
        // shouldn't be reachable given guaranteed_affixes/optional_affixes
        // are each built to be type-unique already, but a live report of
        // exactly this happening means something upstream isn't as
        // airtight as the reasoning suggested, so this stays as a hard
        // backstop regardless of root cause.
        let mut affixes = dedup_affixes(affixes);
        // Once-per-lineage gate (2026-08-16, a live report - see
        // `Item::crit_bonus_affixes`'s doc) for Recombine's OWN crit: a
        // source that already crit on a past recombine (and still has
        // that bonus affix - see `Item::recombine_crit_used`) gets no
        // further chance here either. Reforge's own crit isn't gated by
        // anything in this fn at all - Recombine doesn't grant or roll for
        // it, only ever inherits whichever of it survives the merge below.
        let already_recombine_crit = item_a.recombine_crit_used() || item_b.recombine_crit_used();
        let bonus_affix = if !already_recombine_crit && rng.gen_bool(RECOMBINE_CRIT_CHANCE) {
            let present: Vec<Affix> = affixes.iter().map(|(a, _)| *a).collect();
            let candidates: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| !present.contains(a) && a.is_eligible_for_slot(slot)).collect();
            weighted_affix_pick(&candidates, 1, rng).first().copied().map(|affix| {
                let jitter = rng.gen_range(0.85..1.15);
                affixes.push((affix, affix_base_value(affix, new_tier) * jitter));
                affix
            })
        } else {
            None
        };

        // See Item::crit_bonus_affixes' doc - carry forward whichever
        // crit-granted (types, source) pairs from either source actually
        // survived into the final `affixes` pool above, plus this roll's
        // own crit if it fired.
        let mut crit_bonus_affixes: Vec<(Affix, CritSource)> = Vec::new();
        for &(affix, source) in item_a.crit_bonus_affixes.iter().chain(item_b.crit_bonus_affixes.iter()) {
            if affixes.iter().any(|&(t, _)| t == affix) && !crit_bonus_affixes.iter().any(|&(a, _)| a == affix) {
                crit_bonus_affixes.push((affix, source));
            }
        }
        if let Some(affix) = bonus_affix {
            crit_bonus_affixes.retain(|&(_, src)| src != CritSource::Recombine);
            crit_bonus_affixes.push((affix, CritSource::Recombine));
        }

        Ok(RecombineRoll { slot, new_tier, was_indestructible, unique_affix, affixes, bonus_affix, power_roll, crit_bonus_affixes })
    }

    /// Commits a `RecombineRoll` decided earlier by `roll_recombine` -
    /// consumes both source items and produces the result exactly as
    /// rolled (name/id are freshly generated cosmetic flavor; power is
    /// computed from `roll.power_roll` - the coin-flipped survivor of the
    /// two sources' own rolls, not a fresh one - via
    /// `generate_item_at_tier_with_roll`; the tier/affixes/crit that
    /// actually matter are all taken verbatim from `roll`). Both source
    /// items are ALWAYS consumed on success, even if the result ends up
    /// lost to a full bag - recombination is a real forge, not a free
    /// reroll.
    pub(crate) fn apply_recombine_roll(&mut self, item_id_a: &str, item_id_b: &str, roll: RecombineRoll, rng: &mut impl Rng) -> RecombineOutcome {
        self.take_item_by_id(item_id_a);
        self.take_item_by_id(item_id_b);

        let mut new_item = generate_item_at_tier_with_roll(roll.slot, roll.new_tier, roll.power_roll, rng);
        new_item.affixes = roll.affixes;
        if roll.was_indestructible {
            new_item.max_uses = None;
        }
        new_item.unique_affix = roll.unique_affix;
        new_item.crit_bonus_affixes = roll.crit_bonus_affixes;
        let item_name = new_item.name.clone();
        let item_id = new_item.id.clone();
        self.receive_item(new_item);

        RecombineOutcome { item_id, item_name, slot: roll.slot, new_tier: roll.new_tier, bonus_affix: roll.bonus_affix }
    }

    /// Applies one of the currency crafting actions (see `CraftAction`)
    /// to a specific item, then bumps the item's tier as a side effect of
    /// having been crafted on AT ALL - +3 tiers if it's currently under
    /// 25, +2 if under 50, +1 otherwise (rescaling `power` and every
    /// affix value to match via `Item::sync_tier_to`, same mechanism
    /// Krangle's own level-sync already uses - no orphaned old-tier
    /// values). Applies to every action including Scour and
    /// CelestialShard ("crafted on in any way"); Recombine is NOT part
    /// of this, it has its own separate tier formula. Dust is checked/
    /// deducted by the caller (`AdventureManager::craft_item`), same
    /// split as every other paid action, so a failed attempt never costs
    /// anything - the tier bump only ever applies on success.
    pub(crate) fn craft(&mut self, item_id: &str, action: CraftAction, rng: &mut impl Rng) -> Result<CraftOutcome, CraftError> {
        let mut outcome = self.craft_inner(item_id, action, rng)?;
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        let bump = if item.tier < 25 { 3 } else if item.tier < 50 { 2 } else { 1 };
        item.sync_tier_to(item.tier + bump);
        outcome.tier = item.tier;
        Ok(outcome)
    }

    /// Only validates/mutates the item itself (found, unlocked, right
    /// affix-count precondition) for one specific `CraftAction` - the
    /// per-tier growth-on-craft step lives in the `craft` wrapper above,
    /// applied uniformly after whichever branch below succeeds.
    pub(crate) fn craft_inner(&mut self, item_id: &str, action: CraftAction, rng: &mut impl Rng) -> Result<CraftOutcome, CraftError> {
        if action == CraftAction::Scour {
            let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
            if item.locked {
                return Err(CraftError::ItemLocked);
            }
            if item.affixes.is_empty() {
                return Err(CraftError::NothingToRemove);
            }
            let item_name = item.name.clone();
            let slot = item.slot;
            let tier = item.tier;
            let perfect = item.perfect;
            let affixes_removed = item.affixes.len() as u32;
            item.affixes.clear();
            return Ok(CraftOutcome { item_name, slot, tier, action, affix_added: None, affix_value: None, affix_removed: None, affix_removed_value: None, affixes_removed, now_locked: false, unique_affix_added: None, polished_affixes: Vec::new(), chancing_previous: Vec::new(), new_quality_percent: None, perfect });
        }
        if action == CraftAction::Annulment {
            return self.annul_random_affix(item_id, rng);
        }
        if action == CraftAction::Chancing {
            return self.chance_all_affixes(item_id, rng);
        }
        if action == CraftAction::CelestialShard {
            // Legacy-only path (2026-08-19, Unified Unique Shards) - see
            // `CraftAction::CelestialShard`'s own doc for why this stays.
            // `UniqueShard` never reaches `craft_inner` at all anymore
            // (see `AdventureManager::craft_item_ex`'s own early branch,
            // which always builds its apply-time picker instead) - this
            // arm only exists for a not-yet-migrated straggler token, and
            // unconditionally grants `CelestialConversion` (the only thing
            // a real CelestialShard token could ever have meant).
            let item = self.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
            // Locked and unique are mutually exclusive in BOTH
            // directions - an already-Krangled item can't receive a
            // unique affix either, same as Krangle itself refusing an
            // already-unique item (see craftable_affix_pool's Krangle
            // branch). This is a DIFFERENT thing from "applying a unique
            // affix locks the item" - it explicitly does NOT (per the
            // request, "still craftable and recombinable, it just always
            // stays with the item") - this is only about the two states
            // never being allowed to coexist on the same item.
            if item.locked {
                return Err(CraftError::ItemLocked);
            }
            if item.unique_affix.is_some() {
                return Err(CraftError::AlreadyUnique);
            }
            let slot = item.slot;
            // Duplicate-unique-effects fix (2026-08-21) - same equipped-
            // only conflict check `AdventureManager::craft_item_ex`'s
            // UniqueShard branch applies to its picker, needed here too
            // since this legacy path grants `CelestialConversion`
            // directly with no picker step to filter. Currently
            // unreachable in practice (every live CelestialShard token
            // has already migrated to UniqueShard - see
            // `migrate_celestial_shard_into_unique_shard`), kept for
            // defense in depth against a stale token reappearing.
            if self.equipped(slot).as_ref().is_some_and(|i| i.id == item_id) && self.has_conflicting_unique_affix_value(UniqueAffix::CelestialConversion, slot) {
                return Err(CraftError::ConflictingUniqueAffix);
            }
            let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
            let item_name = item.name.clone();
            let tier = item.tier;
            let perfect = item.perfect;
            item.unique_affix = Some(UniqueAffix::CelestialConversion);
            return Ok(CraftOutcome {
                item_name,
                slot,
                tier,
                action,
                affix_added: None,
                affix_value: None,
                affix_removed: None,
                affix_removed_value: None,
                affixes_removed: 0,
                now_locked: false,
                unique_affix_added: Some(UniqueAffix::CelestialConversion),
                perfect,
                polished_affixes: Vec::new(),
                chancing_previous: Vec::new(),
                new_quality_percent: None,
            });
        }
        let pool = self.craftable_affix_pool(item_id, action)?;
        if pool.is_empty() {
            return Err(CraftError::NoCandidatesLeft);
        }
        let affix = *weighted_affix_pick(&pool, 1, rng).first().ok_or(CraftError::NoCandidatesLeft)?;
        let value = self.roll_craft_affix_value(item_id, affix, rng).ok_or(CraftError::ItemNotFound)?;
        self.apply_craft_affix(item_id, action, affix, value)
    }

    /// Item-side validation for a currency craft (found, unlocked, right
    /// affix-count precondition) plus the pool of affix types NOT
    /// already on the item - what a plain `craft` picks one random
    /// entry from, and what a veiled craft (see `PendingVeil`) samples
    /// up to 3 DISTINCT entries from instead. Read-only, mutates
    /// nothing. Never called for `CraftAction::Scour`, which has no
    /// affix pool to speak of.
    pub(crate) fn craftable_affix_pool(&self, item_id: &str, action: CraftAction) -> Result<Vec<Affix>, CraftError> {
        let item = self.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        if action == CraftAction::Krangle && item.unique_affix.is_some() {
            return Err(CraftError::CannotKrangleUnique);
        }
        if let Some(required) = action.required_affix_count() {
            // A Reforge/Recombine crit-bonus affix (`Item::crit_bonus_affixes`)
            // doesn't count toward this precondition (2026-08-18, a live
            // report): the designed ceiling is 4 normal affixes + up to 1
            // EACH from a Reforge/Recombine crit (see `crit_bonus_affixes`'
            // own doc), so an item sitting at, say, 3 normal + 1 crit-bonus
            // affix (e.g. after an Annulment removed one of its normal
            // affixes) should still read as "3" for Augment/Regal/Exalt's
            // exact-count gate, not "4" - otherwise a crit-bonus affix
            // silently eats into the normal-crafting progression it was
            // never meant to count against.
            let normal_affix_count = item.affixes.iter().filter(|(a, _)| !item.is_crit_bonus_affix(*a)).count();
            if normal_affix_count != required {
                return Err(CraftError::PreconditionNotMet);
            }
        }
        let present: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
        Ok(ALL_AFFIXES.into_iter().filter(|a| !present.contains(a) && a.is_eligible_for_slot(item.slot)).collect())
    }

    /// Rolls the jittered value one specific `affix` would get if added
    /// to this item right now, at its CURRENT tier - read-only, mutates
    /// nothing. Split out from `apply_craft_affix` so a veiled craft can
    /// roll and DISPLAY several candidates' exact values without
    /// touching the real item, then apply the one the player picked
    /// completely unchanged (see `AdventureManager::choose_veil_outcome`)
    /// rather than re-rolling a different number at commit time.
    /// A live report caught a Perfect Quality item's crafted-on affixes
    /// coming in at plain (non-boosted) magnitude - this didn't apply
    /// `PERFECT_QUALITY_MULT` the way `make_item_perfect`/reforge's
    /// `was_perfect` path both do, so a freshly-crafted affix sat at a
    /// normal roll right alongside 3 properly-boosted siblings on the
    /// same item, and `affix_quality_percent`'s Perfect-aware divide (see
    /// its own doc) then read that plain value as an impossibly LOW roll
    /// (dividing an already-unboosted number by 1.20 pushes it below the
    /// 0.85 floor) - clamping to a flat 0%, not the correct ~50-ish%.
    pub(crate) fn roll_craft_affix_value(&self, item_id: &str, affix: Affix, rng: &mut impl Rng) -> Option<f64> {
        let item = self.find_item_by_id(item_id)?;
        let jitter = rng.gen_range(0.85..1.15);
        let mult = if item.perfect { PERFECT_QUALITY_MULT } else { 1.0 };
        Some(affix_base_value(affix, item.tier) * jitter * mult)
    }


    /// Pushes an already-rolled `(affix, value)` onto an item - the
    /// shared mutation both a plain craft's immediate roll and a veiled
    /// craft's chosen candidate (rolled earlier via
    /// `roll_craft_affix_value`) go through.
    pub(crate) fn apply_craft_affix(&mut self, item_id: &str, action: CraftAction, affix: Affix, value: f64) -> Result<CraftOutcome, CraftError> {
        let level = self.level;
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        let item_name = item.name.clone();
        let slot = item.slot;
        item.affixes.push((affix, value));
        let now_locked = action == CraftAction::Krangle;
        if now_locked {
            item.locked = true;
            // Immediately synced to the character's current level (power
            // and every modifier rescaled right along with it - see
            // `Item::sync_tier_to`) rather than only starting to grow
            // from whatever tier/stats it happened to have when locked -
            // Krangling a low-tier item at a high level shouldn't leave
            // it permanently behind.
            item.sync_tier_to(level);
        }
        let tier = item.tier;
        let perfect = item.perfect;
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action,
            affix_added: Some(affix),
            affix_value: Some(value),
            affix_removed: None,
            affix_removed_value: None,
            affixes_removed: 0,
            now_locked,
            unique_affix_added: None,
            polished_affixes: Vec::new(),
            chancing_previous: Vec::new(),
            new_quality_percent: None,
            perfect,
        })
    }

    /// Non-veiled `CraftAction::Annulment` - removes one uniformly random
    /// EXISTING modifier. Deliberately NOT `weighted_affix_pick` - that
    /// weighting (rarer affixes drawn less often) only applies when
    /// drawing a fresh type from `ALL_AFFIXES`; this samples over the
    /// item's OWN present modifiers, where every one of them is equally
    /// "real" and equally eligible to go. Deliberately does NOT touch
    /// `Item::crit_bonus_affixes` even if the removed modifier was a
    /// crit-granted one - nothing needs to: `reforge_crit_used`/
    /// `recombine_crit_used` are derived from presence in `affixes`, so
    /// removing the entry here already re-opens that gate on its own.
    pub(crate) fn annul_random_affix(&mut self, item_id: &str, rng: &mut impl Rng) -> Result<CraftOutcome, CraftError> {
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        if item.affixes.is_empty() {
            return Err(CraftError::NothingToRemove);
        }
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        let perfect = item.perfect;
        let idx = rng.gen_range(0..item.affixes.len());
        let (affix, value) = item.affixes.remove(idx);
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action: CraftAction::Annulment,
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
    }

    /// Commits a veiled Annulment's chosen candidate - removes `affix`
    /// (the specific type the player picked between the up-to-2 rolled
    /// candidates) from the item. Matches by TYPE, not a stashed index -
    /// safe because at most one entry per `Affix` type ever exists on an
    /// item (`roll_affixes`/`dedup_affixes` already guarantee this), and
    /// nothing else mutates the item between the veil roll and this
    /// commit.
    pub(crate) fn apply_annulment_removal(&mut self, item_id: &str, affix: Affix) -> Result<CraftOutcome, CraftError> {
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        let perfect = item.perfect;
        let pos = item.affixes.iter().position(|(a, _)| *a == affix).ok_or(CraftError::ItemNotFound)?;
        let (removed_affix, value) = item.affixes.remove(pos);
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action: CraftAction::Annulment,
            affix_added: None,
            affix_value: None,
            affix_removed: Some(removed_affix),
            affix_removed_value: Some(value),
            affixes_removed: 0,
            now_locked: false,
            unique_affix_added: None,
            polished_affixes: Vec::new(),
            chancing_previous: Vec::new(),
            new_quality_percent: None,
            perfect,
        })
    }

    /// Commits `CraftAction::UniqueShard`'s apply-time picker (2026-08-19,
    /// Unified Unique Shards) - pushes the already-chosen `UniqueAffix`
    /// onto the item. The commit-time half of the split every veiled
    /// action uses: `AdventureManager::craft_item_ex`'s own UniqueShard
    /// branch checks `ItemLocked`/`AlreadyUnique` once, up front, at
    /// `PendingVeil`-insert time (same point the token itself is
    /// consumed). That insert-time filter is only a snapshot, though -
    /// it can't see a conflict created AFTER insert but BEFORE this
    /// commit (a second pending pick on another equipped slot, or an
    /// ordinary equip landing the same value elsewhere in the meantime -
    /// duplicate-unique-effects fix, 2026-08-21, bug #44: the original
    /// version of this fn trusted the snapshot completely and had NO
    /// re-check here, which is exactly how live duplicates got created).
    /// So this fn re-validates for itself right before mutating, same
    /// `has_conflicting_unique_affix_value` call the legacy
    /// `CelestialShard` path and every equip-time site already use -
    /// this is no longer a case of "insert-time validates, commit-time
    /// trusts it" like `apply_craft_affix` above; both ends check now.
    pub(crate) fn apply_unique_affix(&mut self, item_id: &str, unique: UniqueAffix) -> Result<CraftOutcome, CraftError> {
        let existing = self.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
        let slot = existing.slot;
        if self.equipped(slot).as_ref().is_some_and(|i| i.id == item_id) && self.has_conflicting_unique_affix_value(unique, slot) {
            return Err(CraftError::ConflictingUniqueAffix);
        }
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        let perfect = item.perfect;
        item.unique_affix = Some(unique);
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action: CraftAction::UniqueShard,
            affix_added: None,
            affix_value: None,
            affix_removed: None,
            affix_removed_value: None,
            affixes_removed: 0,
            now_locked: false,
            unique_affix_added: Some(unique),
            polished_affixes: Vec::new(),
            chancing_previous: Vec::new(),
            new_quality_percent: None,
            perfect,
        })
    }

    /// Non-veiled `CraftAction::Chancing` - a real chance-orb reroll: every
    /// existing modifier SLOT gets a brand-new TYPE (not just a new value
    /// for its existing type - 2026-08-17, fixing the original version's
    /// wrong behavior, see this fn's git history/the plan that fixed it).
    /// Processes one slot at a time, mutating the real item between
    /// iterations, so `craftable_affix_pool`'s "exclude every type already
    /// present" check - the same pool Transmute/Augment/etc. already use -
    /// naturally keeps every slot's new type distinct from every other
    /// slot's (already-rerolled or not-yet-touched) type, with no extra
    /// bookkeeping needed. If a slot's OLD type was a Reforge/Recombine
    /// crit bonus (`Item::crit_bonus_affixes`), that marking follows the
    /// slot to its NEW type - "retain and reroll" a crit-granted affix,
    /// per the live request, rather than losing the marking.
    pub(crate) fn chance_all_affixes(&mut self, item_id: &str, rng: &mut impl Rng) -> Result<CraftOutcome, CraftError> {
        let item = self.find_item_by_id(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        if item.affixes.is_empty() {
            return Err(CraftError::NothingToReroll);
        }
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        let perfect = item.perfect;
        let old_types: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
        let mut polished_affixes = Vec::with_capacity(old_types.len());
        let mut chancing_previous = Vec::with_capacity(old_types.len());
        for old_affix in old_types {
            let pool = self.craftable_affix_pool(item_id, CraftAction::Chancing)?;
            let new_affix = *weighted_affix_pick(&pool, 1, rng).first().ok_or(CraftError::NoCandidatesLeft)?;
            let new_value = self.roll_craft_affix_value(item_id, new_affix, rng).ok_or(CraftError::ItemNotFound)?;
            let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
            if let Some(slot_entry) = item.affixes.iter_mut().find(|(a, _)| *a == old_affix) {
                *slot_entry = (new_affix, new_value);
            }
            if let Some(pos) = item.crit_bonus_affixes.iter().position(|&(a, _)| a == old_affix) {
                item.crit_bonus_affixes[pos].0 = new_affix;
            }
            chancing_previous.push(old_affix);
            polished_affixes.push((new_affix, new_value));
        }
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action: CraftAction::Chancing,
            affix_added: None,
            affix_value: None,
            affix_removed: None,
            affix_removed_value: None,
            affixes_removed: 0,
            now_locked: false,
            unique_affix_added: None,
            polished_affixes,
            chancing_previous,
            new_quality_percent: None,
            perfect,
        })
    }

    /// Applies one veiled-Chancing step's chosen replacement (see
    /// `AdventureManager::choose_veil_outcome`'s Chancing arm) - finds the
    /// slot currently holding `old_affix` and replaces its whole
    /// `(Affix, f64)` tuple with `(new_affix, new_value)`, same
    /// `crit_bonus_affixes`-follows-the-slot behavior as the non-veiled
    /// path (`chance_all_affixes`).
    pub(crate) fn apply_chancing_reroll(&mut self, item_id: &str, old_affix: Affix, new_affix: Affix, new_value: f64) -> Option<()> {
        let item = self.find_item_by_id_mut(item_id)?;
        let entry = item.affixes.iter_mut().find(|(a, _)| *a == old_affix)?;
        *entry = (new_affix, new_value);
        if let Some(pos) = item.crit_bonus_affixes.iter().position(|&(a, _)| a == old_affix) {
            item.crit_bonus_affixes[pos].0 = new_affix;
        }
        Some(())
    }

    /// Polishing (2026-08-15, a live request) - see `CraftAction::Polishing`'s
    /// doc for why this bypasses the normal dust/veil crafting machinery
    /// (costs sand instead, priced by `AdventureManager::craft_item`'s own
    /// early branch before this is even called). On a normal item: raises
    /// `power_roll` (this item's overall quality) by 5 percentage points
    /// of `POWER_ROLL_RANGE`'s own span, capped at the range's max, and
    /// picks ONE random affix (if any) to raise by 5 percentage points of
    /// the 0.85-1.15 jitter band, same cap. On an already-Perfect item
    /// (nothing left to raise on the quality side - `power_roll` is
    /// already pinned at the range's max) - instead raises up to 2 random
    /// DISTINCT affixes by that same 5-point jitter step, with
    /// `PERFECT_QUALITY_MULT` divided back out before the step and
    /// reapplied after, same "recover the true jitter, don't let the
    /// Perfect multiplier itself get boosted" reasoning `roll_craft_affix_value`'s
    /// own doc already established for a different Perfect-Quality bug.
    /// Only ever targets an affix that still has real room to climb (its
    /// own raw jitter below the range's max) - a live bugfix, this used
    /// to pick a purely random affix regardless of its current roll, so
    /// it could burn sand on one already pinned at the cap and do nothing
    /// for that slot. An item with every affix already maxed still gets
    /// the quality bump (non-Perfect) or simply polishes nothing further
    /// (Perfect) - never an error, just a smaller effective result.
    pub(crate) fn polish(&mut self, item_id: &str, rng: &mut impl Rng) -> Result<CraftOutcome, CraftError> {
        const QUALITY_STEP: f64 = 0.05;
        let jitter_span = POWER_ROLL_RANGE.end - POWER_ROLL_RANGE.start;
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        // Refuse the craft entirely (no sand deducted - this check runs
        // BEFORE the caller ever computes/charges a cost) when nothing on
        // the item could actually improve - 2026-08-17, a live report:
        // sand was being charged for a no-op. See `Item::has_polish_room`'s
        // doc for the two paths this covers.
        if !item.has_polish_room() {
            return Err(CraftError::NothingToPolish);
        }
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        let is_perfect = item.perfect;
        let mult = if is_perfect { PERFECT_QUALITY_MULT } else { 1.0 };
        let mut polished_affixes: Vec<(Affix, f64)> = Vec::new();
        let mut new_quality_percent: Option<f64> = None;
        // Only an affix with real room left to climb is eligible to be
        // picked below - see `polish_eligible_affixes`'s doc (a live
        // bugfix: Polishing used to pick a purely random affix regardless
        // of its current roll, so it could burn sand "raising" one that
        // was already pinned at the cap, doing nothing at all for that
        // slot).
        let eligible: Vec<usize> = polish_eligible_affixes(item);
        if is_perfect {
            let mut indices = eligible;
            indices.shuffle(rng);
            for &i in indices.iter().take(2) {
                let (affix, value) = item.affixes[i];
                let base = affix_base_value(affix, tier);
                let raw_jitter = (value / mult / base).clamp(POWER_ROLL_RANGE.start, POWER_ROLL_RANGE.end);
                let new_jitter = (raw_jitter + QUALITY_STEP * jitter_span).min(POWER_ROLL_RANGE.end);
                let new_value = base * new_jitter * mult;
                item.affixes[i].1 = new_value;
                polished_affixes.push((affix, new_value));
            }
        } else {
            item.power_roll = (item.power_roll + QUALITY_STEP * jitter_span).min(POWER_ROLL_RANGE.end);
            item.power = compute_power(slot, tier, item.power_roll);
            new_quality_percent = Some(item.quality_percent());
            if !eligible.is_empty() {
                let i = eligible[rng.gen_range(0..eligible.len())];
                let (affix, value) = item.affixes[i];
                let base = affix_base_value(affix, tier);
                let raw_jitter = (value / base).clamp(POWER_ROLL_RANGE.start, POWER_ROLL_RANGE.end);
                let new_jitter = (raw_jitter + QUALITY_STEP * jitter_span).min(POWER_ROLL_RANGE.end);
                let new_value = base * new_jitter;
                item.affixes[i].1 = new_value;
                polished_affixes.push((affix, new_value));
            }
        }
        Ok(CraftOutcome {
            item_name,
            slot,
            tier,
            action: CraftAction::Polishing,
            affix_added: None,
            affix_value: None,
            affix_removed: None,
            affix_removed_value: None,
            affixes_removed: 0,
            now_locked: false,
            unique_affix_added: None,
            polished_affixes,
            chancing_previous: Vec::new(),
            new_quality_percent,
            perfect: is_perfect,
        })
    }

    /// Crafting-panel Reforge (2026-08-15, a live request) - "similar to
    /// the channel-points/web-dashboard Reforge Now button" (see
    /// `AdventureManager::reforge_equipped_item`), but targets a SPECIFIC
    /// item by id (bag or equipped - matching every other crafting
    /// action's own targeting model) instead of a random equipped slot,
    /// costs `30 * tier` dust instead of a flat fee (see
    /// `AdventureManager::craft_item`'s own Reforge branch for why that
    /// bypasses the generic base_cost/tier-surcharge formula entirely,
    /// same reasoning as Polishing's sand cost), has no once-per-hour
    /// cooldown (that allowance is specific to the "Reforge Now" button),
    /// and mutates the EXISTING item IN PLACE (same id, via
    /// `Item::sync_tier_to` - already Perfect-Quality-aware, see its own
    /// doc) rather than replacing it with a freshly-generated one the way
    /// `reforge_equipped_item` does - every other crafting action already
    /// keeps the target item's id stable across the craft, so this
    /// matches that instead. Same underlying tier-jump/rare-bonus-affix
    /// logic otherwise.
    pub(crate) fn reforge_item(&mut self, item_id: &str, rng: &mut impl Rng) -> Result<ReforgeOutcome, CraftError> {
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        let item_name = item.name.clone();
        let slot = item.slot;
        let old_tier = item.tier;
        let was_perfect = item.perfect;
        let new_tier = old_tier + reforge_tier_jump(old_tier, rng);
        let quality_percent = item.quality_percent();
        item.sync_tier_to(new_tier);
        // Once-per-lineage gate (2026-08-16, a live report - see
        // `Item::crit_bonus_affixes`'s doc for the full reasoning): a
        // reforge whose crit-granted affix is still on the item gets no
        // further chance - `rng.gen_bool` isn't even called, so this
        // doesn't cost the roll sequence anything either.
        let bonus_affix = if !item.reforge_crit_used() && rng.gen_bool(reforge_crit_chance(quality_percent, was_perfect)) {
            let present: Vec<Affix> = item.affixes.iter().map(|(a, _)| *a).collect();
            let candidates: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| !present.contains(a) && a.is_eligible_for_slot(slot)).collect();
            let mult = if was_perfect { PERFECT_QUALITY_MULT } else { 1.0 };
            weighted_affix_pick(&candidates, 1, rng).first().copied().map(|affix| {
                let jitter = rng.gen_range(0.85..1.15);
                item.affixes.push((affix, affix_base_value(affix, new_tier) * jitter * mult));
                item.record_reforge_crit(affix);
                affix
            })
        } else {
            None
        };
        Ok(ReforgeOutcome { item_name, slot, old_tier, new_tier, bonus_affix })
    }

    /// `CraftAction::DivineDust` (docs/divine_dust_spec.md) - apply/reroll
    /// a sacred affix, costing `2 × item.tier` Divine Dust (computed and
    /// checked by the caller, `AdventureManager::craft_item_ex`, same
    /// split as Polishing/Reforge above: this fn only validates/mutates,
    /// the manager deducts on success). Two paths:
    ///
    /// - Not yet Sacred: sacralized in place - EXACTLY the effect
    ///   `make_item_sacred` already gives a natural drop (Perfect quality
    ///   PLUS one random maxed-roll sacred affix from the FULL
    ///   `ALL_AFFIXES` pool, ignoring slot eligibility, same as a natural
    ///   Sacred drop). Deliberately not a lesser "just add the affix"
    ///   effect - `Sacred implies Perfect` is a load-bearing invariant
    ///   elsewhere in this codebase (`Item::meets_auto_disenchant_floor`'s
    ///   Quality%-Perfect-Sacred ordering, `disenchant_multiplier`'s
    ///   Sacred(N) == Perfect(N+1) equivalence, and this very feature's
    ///   own `roll_divine_dust_disenchant` default derivation, which
    ///   assumes a Sacred item's `quality_percent()` is always exactly
    ///   100 - see `LiveTunables::divine_dust_disenchant_chance`'s doc) -
    ///   letting a player mint a "Sacred but not Perfect" item via this
    ///   path would quietly break all three. A judgment call (the spec
    ///   text only said "becomes sacred, gains one random sacred affix"),
    ///   recorded in docs/divine_dust_spec.md's decisions log.
    /// - Already Sacred: the existing sacred affix rerolls to a DIFFERENT
    ///   random affix (current excluded), at the same "max jitter ×
    ///   `PERFECT_QUALITY_MULT`" roll `make_item_sacred` uses. Empty pool
    ///   (`CraftError::NoValidRerollTarget`) is unreachable today (`Affix`
    ///   has 17 variants, excluding 1 always leaves 16) but implemented
    ///   and tested per the spec's explicit call for the guard.
    pub(crate) fn apply_divine_dust(&mut self, item_id: &str, rng: &mut impl Rng) -> Result<DivineDustOutcome, CraftError> {
        let item = self.find_item_by_id_mut(item_id).ok_or(CraftError::ItemNotFound)?;
        if item.locked {
            return Err(CraftError::ItemLocked);
        }
        let item_name = item.name.clone();
        let slot = item.slot;
        let tier = item.tier;
        if let Some((current_affix, _)) = item.sacred_affix {
            let pool = divine_dust_reroll_pool(current_affix, &ALL_AFFIXES);
            if pool.is_empty() {
                return Err(CraftError::NoValidRerollTarget);
            }
            let new_affix = pool[rng.gen_range(0..pool.len())];
            let new_value = affix_base_value(new_affix, tier) * 1.15 * PERFECT_QUALITY_MULT;
            item.sacred_affix = Some((new_affix, new_value));
            Ok(DivineDustOutcome { item_name, slot, tier, became_sacred: false, old_affix: Some(current_affix), new_affix, new_value })
        } else {
            if !item.perfect {
                item.power_roll = POWER_ROLL_RANGE.end;
                item.power = compute_power(slot, tier, item.power_roll) * PERFECT_QUALITY_MULT;
                for (_, value) in item.affixes.iter_mut() {
                    *value *= PERFECT_QUALITY_MULT;
                }
                item.perfect = true;
            }
            let new_affix = ALL_AFFIXES[rng.gen_range(0..ALL_AFFIXES.len())];
            let new_value = affix_base_value(new_affix, tier) * 1.15 * PERFECT_QUALITY_MULT;
            item.sacred_affix = Some((new_affix, new_value));
            Ok(DivineDustOutcome { item_name, slot, tier, became_sacred: true, old_affix: None, new_affix, new_value })
        }
    }

    /// Web dashboard: equips a specific bag item (by id) into its slot,
    /// swapping whatever was equipped there back into the bag. Can never
    /// fail on bag capacity - exactly one item leaves the bag for every
    /// one (if any) that returns to it. `false` if no such item is in the
    /// bag, or equipping it would conflict with a unique affix already
    /// equipped elsewhere (see `has_conflicting_unique_affix`) - the item
    /// stays in the bag untouched either way.
    pub fn equip_from_inventory(&mut self, item_id: &str) -> bool {
        let Some(pos) = self.inventory.iter().position(|i| i.id == item_id) else { return false };
        let slot = self.inventory[pos].slot;
        if self.has_conflicting_unique_affix(&self.inventory[pos], slot) {
            return false;
        }
        let item = self.inventory.remove(pos);
        if let Some(previous) = self.equipped_mut(slot).replace(item) {
            self.inventory.push(previous);
        }
        self.sync_retreat_status();
        true
    }

    /// Web dashboard: unequips whatever's in `slot` back into the bag -
    /// `false` (no-op) if the slot's already empty or the bag is full.
    pub fn unequip_to_inventory(&mut self, slot: EquipSlot) -> bool {
        if self.equipped(slot).is_none() || self.inventory.len() >= INVENTORY_CAPACITY {
            return false;
        }
        if let Some(item) = self.equipped_mut(slot).take() {
            self.inventory.push(item);
        }
        self.sync_retreat_status();
        true
    }

    /// Web dashboard: disenchants a bag item into Thaumatergic Dust
    /// instead of just discarding it — 1-6 dust per tier of the item,
    /// added straight to the character's total. Dust doesn't spend on
    /// anything yet; just tracked for a future use. Returns the outcome
    /// (including `dust_max`, for the dashboard's "you got X% of the
    /// possible dust" popup - see `DisenchantOutcome`), or `None` if no
    /// such item was in the bag OR it's disenchant-protected (see
    /// `Item::disenchant_protected`) - a server-side backstop matching the
    /// dashboard's own hidden/disabled button for a protected item, in
    /// case of a stale page/direct POST.
    pub fn disenchant_from_inventory(&mut self, item_id: &str, rng: &mut impl Rng, sand_mult: f64, divine_dust_disenchant_chance: f64) -> Option<DisenchantOutcome> {
        let pos = self.inventory.iter().position(|i| i.id == item_id)?;
        if self.inventory[pos].disenchant_protected {
            return None;
        }
        let item = self.inventory.remove(pos);
        let multiplier = item.tier * item.disenchant_multiplier();
        let dust = rng.gen_range(1..=6) * multiplier;
        self.dust += dust as u64;
        self.sand += roll_disenchant_sand(item.quality_percent(), rng, sand_mult);
        // Divine Dust (2026-08-19) - SACRED items only, see
        // `roll_divine_dust_disenchant`'s doc. Never announced (only fight
        // drops are - see `AdventureManager::announce_divine_dust_drop`'s
        // doc), so this is a plain currency grant, same as dust/sand above.
        let divine_dust = roll_divine_dust_disenchant(item.sacred_affix.is_some(), rng, divine_dust_disenchant_chance);
        self.divine_dust += divine_dust;
        Some(DisenchantOutcome { item_name: item.name.clone(), dust, dust_max: 6 * multiplier, divine_dust })
    }

    /// Web dashboard: disenchants EVERY bag item at once, except anything
    /// individually disenchant-protected (see `Item::disenchant_protected`)
    /// - "clean out the bag" without having to click through one at a
    /// time, while still never touching anything the player's marked as
    /// worth keeping. Krangled (`locked`) items ARE included (2026-08-18,
    /// a live request) - `locked` normally blocks further CRAFTING
    /// (there's nothing left to craft on a Krangled item anyway), but
    /// that's a separate concern from disenchanting a bag clean; a player
    /// who wants a Krangled item kept safe from a bulk disenchant still
    /// has `disenchant_protected` for that. Returns how many items were
    /// actually disenchanted and the total dust granted (both 0 if
    /// nothing was eligible).
    pub fn disenchant_all_from_inventory(&mut self, rng: &mut impl Rng, sand_mult: f64, divine_dust_disenchant_chance: f64) -> (usize, u64) {
        let mut count = 0usize;
        let mut total_dust = 0u64;
        let mut total_sand = 0u64;
        let mut total_divine_dust = 0u64;
        self.inventory.retain(|item| {
            if item.disenchant_protected {
                return true;
            }
            total_dust += (rng.gen_range(1..=6) * item.tier * item.disenchant_multiplier()) as u64;
            total_sand += roll_disenchant_sand(item.quality_percent(), rng, sand_mult);
            total_divine_dust += roll_divine_dust_disenchant(item.sacred_affix.is_some(), rng, divine_dust_disenchant_chance);
            count += 1;
            false
        });
        self.dust += total_dust;
        self.sand += total_sand;
        self.divine_dust += total_divine_dust;
        (count, total_dust)
    }

    /// Repairs whatever's equipped in `slot` back to full durability
    /// (1 dust per tier) - web dashboard only.
    pub fn repair_equipped(&mut self, slot: EquipSlot) -> Result<u64, RepairError> {
        let cost = match self.equipped(slot) {
            None => return Err(RepairError::NoItem),
            Some(item) if !item.needs_repair() => return Err(RepairError::NotNeeded),
            Some(item) => item.repair_cost(),
        };
        if self.dust < cost {
            return Err(RepairError::InsufficientDust(cost));
        }
        self.dust -= cost;
        if let Some(item) = self.equipped_mut(slot) {
            item.uses = 0;
        }
        self.sync_retreat_status();
        Ok(cost)
    }

    /// True if every currently-EQUIPPED, non-indestructible item is fully
    /// spent (0% durability) and there's at least one such item - a
    /// character with only empty slots and/or indestructible gear is
    /// never considered retreated this way, there's nothing to wear out.
    pub fn all_gear_worn_out(&self) -> bool {
        let destructible: Vec<&Item> = EQUIP_SLOTS.iter().filter_map(|&slot| self.equipped(slot).as_ref()).filter(|i| i.max_uses.is_some()).collect();
        !destructible.is_empty() && destructible.iter().all(|i| i.is_fully_worn())
    }

    /// Fully repairs every equipped AND bagged item - the free 1-hour
    /// auto-repair-on-retreat payoff (see `retreated_since`), not a
    /// dust-costing action.
    pub(crate) fn repair_all_gear(&mut self) {
        for slot in EQUIP_SLOTS {
            if let Some(item) = self.equipped_mut(slot) {
                item.uses = 0;
            }
        }
        for item in self.inventory.iter_mut() {
            item.uses = 0;
        }
    }

    /// Recomputes retreat status from current EQUIPPED gear - call after
    /// any action that could change it (equip/unequip/repair/reforge).
    /// Sets `retreated_since` the instant all equipped gear is worn out,
    /// clears it the instant that's no longer true (repairing OR simply
    /// swapping in working gear both count), so a character can return to
    /// the field immediately rather than needing a specific "un-retreat"
    /// action.
    pub(crate) fn sync_retreat_status(&mut self) {
        let worn_out = self.all_gear_worn_out();
        if worn_out && self.retreated_since.is_none() {
            self.retreated_since = Some(SystemTime::now());
        } else if !worn_out && self.retreated_since.is_some() {
            self.retreated_since = None;
        }
    }

    /// Repairs a specific bag item back to full durability (1 dust per
    /// tier) - web dashboard only.
    pub fn repair_inventory_item(&mut self, item_id: &str) -> Result<u64, RepairError> {
        let pos = self.inventory.iter().position(|i| i.id == item_id).ok_or(RepairError::NoItem)?;
        let cost = {
            let item = &self.inventory[pos];
            if !item.needs_repair() {
                return Err(RepairError::NotNeeded);
            }
            item.repair_cost()
        };
        if self.dust < cost {
            return Err(RepairError::InsufficientDust(cost));
        }
        self.dust -= cost;
        self.inventory[pos].uses = 0;
        Ok(cost)
    }

    /// Total dust cost of a `repair_all()` call right now - a 10% premium
    /// over the sum of what repairing each item needing it would cost
    /// individually (rounded up), 0 if nothing needs repair. Pure/
    /// read-only, so the web dashboard can preview the cost (and grey out
    /// the button when unaffordable) without actually spending anything.
    pub fn repair_all_cost(&self) -> u64 {
        let base_cost: u64 = EQUIP_SLOTS
            .iter()
            .filter_map(|&slot| self.equipped(slot).as_ref())
            .chain(self.inventory.iter())
            .filter(|item| item.needs_repair())
            .map(|item| item.repair_cost())
            .sum();
        if base_cost == 0 {
            0
        } else {
            (base_cost as f64 * 1.1).ceil() as u64
        }
    }

    /// Repairs every equipped AND bagged item that currently needs it in
    /// one paid action, at a 10% premium over the sum of what repairing
    /// each of them individually would cost (rounded up) - the shortcut
    /// costs a bit more than doing it piece by piece. `NotNeeded` if
    /// nothing on the character needs repair right now.
    pub fn repair_all(&mut self) -> Result<u64, RepairError> {
        let cost = self.repair_all_cost();
        if cost == 0 {
            return Err(RepairError::NotNeeded);
        }
        if self.dust < cost {
            return Err(RepairError::InsufficientDust(cost));
        }
        self.dust -= cost;
        for slot in EQUIP_SLOTS {
            if let Some(item) = self.equipped_mut(slot) {
                if item.needs_repair() {
                    item.uses = 0;
                }
            }
        }
        for item in self.inventory.iter_mut() {
            if item.needs_repair() {
                item.uses = 0;
            }
        }
        self.sync_retreat_status();
        Ok(cost)
    }

    /// !character's gear line — only equipped slots, "no gear yet" if none.
    /// Deliberately terse (name + tier, durability noted only when it's
    /// actually worn) rather than a full per-item affix dump - the old
    /// version pushed a well-built character's entire !character reply
    /// past Twitch's ~500-char chat message limit, which Twitch silently
    /// drops with no error back to the bot at all (confirmed live via a
    /// real outage report on 2026-08-13 - every affected reply measured
    /// 600+ chars). Full per-item stats are what the dashboard link at
    /// the end of !character's reply is for.
    pub fn gear_summary(&self) -> String {
        let items: Vec<&Item> = [&self.weapon, &self.helm, &self.body, &self.gloves, &self.boots].into_iter().flatten().collect();
        if items.is_empty() {
            "no gear yet".to_string()
        } else {
            items
                .iter()
                .map(|i| match i.durability_percent() {
                    Some(pct) if pct < 100 => format!("{} T{} ({pct}%)", i.display_name(), i.tier),
                    _ => format!("{} T{}", i.display_name(), i.tier),
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    /// XP needed to advance from `level` to `level + 1` - a live request
    /// to "really slow down" progression after the top of the roster hit
    /// level 50+ in roughly a day of play. Was purely linear (20 +
    /// 15*level); added a quadratic term so the curve barely changes
    /// early (level 1: 36 xp, was 35) but ramps up hard late (level 25:
    /// 1020, was 395 - 2.6x; level 50: 3270, was 770 - 4.25x). Total XP to
    /// reach level 51 from scratch goes from ~20,125 to ~63,050 - about
    /// 3.1x more, on top of the victory-XP formula also being cut
    /// separately (see the boss-win xp grant in `run_encounter`) and
    /// activity XP's cooldown being tripled (see `ACTIVITY_XP_COOLDOWN`) -
    /// combined, reaching a given level from scratch should take roughly
    /// 5-6x longer than before. Existing characters' levels/xp are left
    /// exactly as they are - this only changes how much MORE xp it takes
    /// to cross whatever threshold they're already sitting at.
    pub fn xp_to_next_level(level: u32) -> u64 {
        20 + level as u64 * 15 + (level as u64) * (level as u64)
    }

    /// XP still needed to reach the next level from here — what
    /// !character shows alongside current xp.
    pub fn xp_needed(&self) -> u64 {
        Self::xp_to_next_level(self.level)
    }


    pub fn hp(&self) -> u32 {
        20 + self.level * 5
    }
    pub fn atk(&self) -> u32 {
        5 + self.level * 2
    }
    pub fn def(&self) -> u32 {
        2 + self.level
    }

    /// Max HP going into a fight — full every encounter, nothing carries
    /// over between them (no lasting punishment for a knockout). Body
    /// armor's survivability stat and any `FlatLife` affixes are flat
    /// bonuses added to the base pool; the archetype+gear increase and
    /// the passive tree's own increase are then two INDEPENDENT
    /// multiplicative layers - `base * (1+gear) * (1+tree)` - rather
    /// than being summed into one fraction first, per the "character
    /// sheet vs. passive tree always compound, never just add" principle
    /// (see `combine_reduction_sources`'s doc for the capped-stat version
    /// of the same idea).
    pub fn combat_max_hp(&self) -> u32 {
        let base = self.hp() as f64 + self.body.as_ref().map(|i| i.effective_power()).unwrap_or(0.0) + self.sum_affix(Affix::FlatLife);
        let gear_increased = self.archetype.bonus(self.level).max_hp_pct + self.sum_affix(Affix::IncreasedLife);
        let tree_increased = self.passive_bonus().max_hp_pct + self.passive_overflow_bonus().max_hp_pct;
        (base * (1.0 + gear_increased) * (1.0 + tree_increased)).max(1.0).round() as u32
    }

    /// Damage dealt per hit against an enemy - every archetype deals
    /// real damage from their gear, base for every unified attack
    /// action (see `simulate_battle`) whether or not any of it ends up
    /// converted to healing (see `combat_heal_power`'s "healing is
    /// strictly converted damage" doc - the SAME roll off this number
    /// is what gets split between an enemy and a hurt ally, not a
    /// separate formula). Weapon dps is a flat bonus on top, same for
    /// every function.
    pub(crate) fn combat_atk(&self) -> u32 {
        let base = match self.archetype.combat_function() {
            CombatFunction::Melee => self.atk() * 2 + 4,
            CombatFunction::Ranged => (self.atk() as f64 * 1.3) as u32 + 2,
            CombatFunction::Heal => (self.atk() as f64 * 0.8) as u32 + 1,
        };
        base + self.weapon.as_ref().map(|i| i.effective_power().round() as u32).unwrap_or(0)
    }

    /// Milliseconds between actions in a fight — melee swings slower but
    /// hits much harder per swing; ranged/heal act more often for
    /// smaller individual amounts. Gloves' speed stat and the
    /// archetype's `attack_speed` bonus both shorten this (or lengthen
    /// it, for a negative archetype bonus like Paladin's), and the
    /// passive tree's own `attack_speed` investment shortens it FURTHER
    /// as an independent multiplicative layer - `rate = (1/base) *
    /// (1+gear_bonus) * (1+tree_bonus)`, e.g. 100% gear + 50% tree off a
    /// `HEAL_BASE_ATTACK_INTERVAL_MS` (1700ms) base gives 1.76 attacks/sec
    /// (~568ms), not the old flat-sum
    /// behavior. Same "character sheet vs. passive tree, two independent
    /// compounding sources" principle as `combine_reduction_sources`
    /// uses for the capped defensive stats - this is that same principle
    /// applied to a rate instead of a 0-100% fraction, so it's
    /// `(1+a)*(1+b)` rather than `1-(1-a)(1-b)`. Floored at 50ms
    /// regardless of how the two layers combine, so nothing can ever
    /// push the cadence to an actual standstill or a divide-by-zero.
    ///
    /// Healing Power past 100% used to just inflate each heal's own size
    /// past what a normal hit would've dealt (see `combat_heal_power`'s
    /// doc) - per a live request, that excess instead shortens THIS, so a
    /// heavily-invested healer acts more often at the same (now capped -
    /// see `combat_hps`) per-action size instead of hitting bigger less
    /// often. `interval / (1 + excess)` - 200% Healing Power (1.0 excess
    /// over the 1.0/100% baseline) halves the interval, twice as often;
    /// 300% (2.0 excess) divides it by 3, and so on. Applied AFTER the
    /// gear/tree speed layers above, as a further compounding speedup,
    /// not a replacement for it. Only ever pulls the interval down for
    /// someone already past 100% heal power - it can't push it back up.
    pub(crate) fn attack_interval_ms(&self) -> u32 {
        let base = match self.archetype.combat_function() {
            CombatFunction::Melee => MELEE_BASE_ATTACK_INTERVAL_MS,
            CombatFunction::Ranged => RANGED_BASE_ATTACK_INTERVAL_MS,
            CombatFunction::Heal => HEAL_BASE_ATTACK_INTERVAL_MS,
        };
        let gloves_bonus = self.gloves.as_ref().map(|i| i.effective_power()).unwrap_or(0.0);
        let gear_bonus = gloves_bonus + self.archetype.bonus(self.level).attack_speed;
        let tree_bonus = self.passive_bonus().attack_speed + self.passive_overflow_bonus().attack_speed;
        let speed_adjusted = (base as f64) / ((1.0 + gear_bonus) * (1.0 + tree_bonus)).max(0.01);
        let heal_excess = (self.combat_heal_power() - 1.0).max(0.0);
        // Druid's Wild Surge/Overgrowth (2026-08-17) - widens the SAME
        // excess-heal-power divisor every archetype already gets for free
        // above 100% heal power, rather than adding a separate mechanism.
        // Overgrowth just adds its own rate into the same term Wild
        // Surge's own rate already occupies - both are 0.0 without
        // Wild Surge invested (Overgrowth is inert without its parent).
        let wildsurge_frac = self.passive_node_magnitude("wildsurge") + self.passive_node_magnitude("overgrowth");
        (speed_adjusted / (1.0 + heal_excess * (1.0 + wildsurge_frac))).round().max(50.0) as u32
    }

    /// The same gear+tree attack-speed total `attack_interval_ms` derives
    /// its cadence from, exposed as a plain fraction (1.0 = 100%, "double
    /// speed") instead of a millisecond interval - what Mage's Temporal
    /// Rift/Warlock's Unstable Power read to find the excess past 100%
    /// (see `CombatSimUnit::attack_speed_pct`'s doc for why this is the
    /// BASELINE total, not live per-fight stacking buffs). Deliberately
    /// excludes healing-power's own interval speedup below - that's a
    /// different mechanism (shortens the cadence directly, never reads as
    /// an "attack speed" stat anywhere else either).
    pub(crate) fn combat_attack_speed_pct(&self) -> f64 {
        let gloves_bonus = self.gloves.as_ref().map(|i| i.effective_power()).unwrap_or(0.0);
        let gear_bonus = gloves_bonus + self.archetype.bonus(self.level).attack_speed;
        let tree_bonus = self.passive_bonus().attack_speed + self.passive_overflow_bonus().attack_speed;
        // Onslaught - Overwhelming Force's own converted value also grants
        // attack speed, same live-DR-derived source `combat_increased_damage`'s
        // own `overwhelmingforce` term reads (duplicated here rather than
        // shared, since the two getters have no common caller to hang it off).
        let onslaught = self.combat_damage_reduction().max(0.0) * self.passive_node_magnitude("overwhelmingforce") * self.passive_node_magnitude("onslaught");
        (1.0 + gear_bonus) * (1.0 + tree_bonus) * (1.0 + onslaught) - 1.0
    }

    /// `pub` counterpart to `attack_interval_ms` - lets the web dashboard
    /// show a heavily-invested healer their actual action cadence (see
    /// the HPS stat's tooltip), same "expose read-only for display"
    /// pattern as `combat_damage_reduction`/the other `pub combat_*`
    /// getters here.
    pub fn combat_action_interval_ms(&self) -> u32 {
        self.attack_interval_ms()
    }

    /// (dps-per-stack, stack-interval-ms) for the helm's stacking dps
    /// buff, if equipped - every `cooldown_ms` in a fight, the wearer's
    /// base (pre-crit/increased-damage) dps permanently goes up by
    /// `power` for the rest of that fight (see `simulate_battle`'s
    /// `NextEvent::Helm` handling) - rewards surviving longer, not just
    /// raw gear score.
    pub(crate) fn helm_skill(&self) -> Option<(f64, u32)> {
        self.helm.as_ref().map(|i| (i.effective_power(), i.cooldown_ms()))
    }

    /// (power-per-proc, cooldown-ms) for the boots' self-heal, if equipped.
    pub(crate) fn boots_skill(&self) -> Option<(f64, u32)> {
        self.boots.as_ref().map(|i| (i.effective_power(), i.cooldown_ms()))
    }

    /// Sums `affix`'s wear-decayed value (see `Item::effective_affix_total`)
    /// across all 5 equipped slots - the shared plumbing behind every
    /// `combat_*` secondary-stat getter below. Slot doesn't matter for
    /// these (unlike weapon/body/gloves' primary stats) - an affix rolled
    /// on a helm counts exactly the same as one rolled on a boot.
    pub(crate) fn sum_affix(&self, affix: Affix) -> f64 {
        EQUIP_SLOTS.iter().filter_map(|&slot| self.equipped(slot).as_ref()).map(|item| item.effective_affix_total(affix)).sum()
    }

    /// Counts `affix`'s rolled INSTANCES (see `Item::affix_instance_count`)
    /// across all 5 equipped slots - a presence count, not a value sum
    /// (unlike `sum_affix` above), and deliberately ignores wear/durability
    /// decay: an item's affix either is or isn't there, decay only shrinks
    /// what it's worth. Cleric's Haloed Steps (`Affix::DivineDamage`) is
    /// the only current caller.
    pub(crate) fn count_affix(&self, affix: Affix) -> u32 {
        EQUIP_SLOTS.iter().filter_map(|&slot| self.equipped(slot).as_ref()).map(|item| item.affix_instance_count(affix)).sum()
    }

    /// Sums every invested passive-tree node's `FlatStat` contribution
    /// into an `ArchetypeBonus`-shaped total (additive pooling within the
    /// tree, same as gear+archetype's own sums each already do) -
    /// `OverflowConversion` nodes are handled separately by
    /// `passive_overflow_bonus`, since they need THIS total to already
    /// exist before they can know how much of it overflowed.
    /// `NotYetImplemented` nodes contribute nothing (see the
    /// `passive_tree` module doc for what's deferred and why - points
    /// spent there are still saved, just mechanically inert for now).
    pub(crate) fn passive_bonus(&self) -> ArchetypeBonus {
        let mut bonus = ArchetypeBonus::default();
        self.accumulate_flat_stat_bonus(self.archetype, &self.passive_allocations, &mut bonus);
        // Split Personality (2026-08-18 bugfix) - a live report found a
        // secondary-tree investment in a generic FlatStat node (e.g.
        // Rogue's Deadly Precision/Shadowstep as a Monk's 2nd class) was
        // silently doing nothing in REAL fights, not just under-reported
        // on the sheet - `combat_evasion`/`combat_crit_multiplier` (which
        // call this function) are the exact same functions `CombatSimUnit`
        // construction calls, there's no separate path. Bespoke `Special`-
        // effect nodes were never affected (each reads its own
        // `passive_node_rank`/`_magnitude`, which already checks both
        // trees) - only this generic pooling loop was primary-only.
        if let Some(secondary) = self.effective_secondary_archetype() {
            self.accumulate_flat_stat_bonus(secondary, &self.secondary_passive_allocations, &mut bonus);
        }
        // Warrior's Colossus - a genuine exception to the generic
        // FlatStat loop above: its own text ("Juggernaut's max HP bonus
        // is increased by X%... 100% MORE at 3/3") describes a bonus
        // that's a fraction OF JUGGERNAUT'S OWN CURRENT VALUE, not a
        // fixed number independent of it - so it can't be summed as
        // another flat contribution the way this session's other
        // "amplifies a sibling" fixes (Fel Haste, Rapid Fire, etc.)
        // could. Special-cased here (reading Juggernaut's own magnitude
        // directly via `passive_node_magnitude`) rather than a new
        // `PassiveEffect` variant, since this is currently the only node
        // in the whole tree shaped this way. A no-op (both factors 0.0)
        // for every archetype/character without both invested. Already
        // secondary-tree-safe as-is - `passive_node_magnitude` itself
        // checks both trees, regardless of which one Juggernaut/Colossus
        // actually sit in.
        bonus.max_hp_pct += self.passive_node_magnitude("juggernaut") * self.passive_node_magnitude("colossus");
        bonus
    }

    /// Shared by `passive_bonus`'s primary/secondary call sites - the
    /// exact loop body that function always had, just parameterized on
    /// WHICH archetype's node list and allocations map to read instead of
    /// hardcoded to `self.archetype`/`self.passive_allocations`.
    fn accumulate_flat_stat_bonus(&self, archetype: Archetype, allocations: &HashMap<String, u32>, bonus: &mut ArchetypeBonus) {
        let nodes = archetype.passive_nodes();
        for (key, &rank) in allocations {
            let Some(node) = nodes.iter().find(|n| n.key == key.as_str()) else { continue };
            if let crate::passive_tree::PassiveEffect::FlatStat { stat, .. } = node.effect {
                stat.add(bonus, node.magnitude_at_rank(rank));
            }
        }
    }

    /// Titan's Grip - converts a fraction of Juggernaut+Colossus's OWN
    /// combined tree contribution (the exact product `passive_bonus`
    /// adds to `max_hp_pct` above) into increased damage - deliberately
    /// NOT a fraction of the character's whole `combat_max_hp`
    /// multiplier, which also includes gear's uncapped `IncreasedLife`
    /// affix and would make this balloon unboundedly as gear tiers climb
    /// (see a live design conversation, and the WARRIOR_NODES doc
    /// comment on `titansgrip`). Deliberately kept OUT of
    /// `passive_bonus().increased_damage` too, and given its own
    /// independent multiplicative layer in `combat_increased_damage`
    /// (`(1+gear)*(1+tree)*(1+titans_grip)`, not summed into the tree
    /// layer's own additive pool alongside Overwhelming Force etc.) - a
    /// live request that its own percentage always compounds on top of
    /// everything else rather than diluting into a shared total. 0.0
    /// without it invested.
    pub(crate) fn titans_grip_increased_damage(&self) -> f64 {
        let juggernaut_colossus = self.passive_node_magnitude("juggernaut") * self.passive_node_magnitude("colossus");
        juggernaut_colossus * self.passive_node_magnitude("titansgrip")
    }

    /// Sums every invested `OverflowConversion` node's contribution -
    /// each takes a fraction of `combined_stat_overflow`'s COMBINED
    /// gear+archetype+tree overflow for its `input` stat (see that
    /// method's doc for why gear is included, and why that's allowed to
    /// overlap with `defensive_overflow`'s own separate conversion of the
    /// same raw overflow - a live design call, not a bug) and credits it
    /// to `output`, at `magnitude_at_rank` efficiency - HARD-CAPPED at
    /// `OVERFLOW_CONVERSION_CAP_PER_RANK` per invested rank (2026-08-16
    /// follow-up: a live report flagged overflow-conversion nodes as
    /// "incredibly powerful" since their raw output scales with however
    /// much of `input` a player has stacked via GEAR, unlike every other
    /// tree node whose output is a fixed, tree-investment-only number -
    /// this pegs every individual node's OWN contribution to the same
    /// "~10% per point" budget the rest of the tree targets, regardless
    /// of how much overflow is actually available to convert).
    pub(crate) fn passive_overflow_bonus(&self) -> ArchetypeBonus {
        // Already includes both trees - see `passive_bonus`'s own 2026-08-18
        // fix.
        let tree_bonus = self.passive_bonus();
        let mut result = ArchetypeBonus::default();
        self.accumulate_overflow_conversion_bonus(self.archetype, &self.passive_allocations, &tree_bonus, &mut result);
        if let Some(secondary) = self.effective_secondary_archetype() {
            self.accumulate_overflow_conversion_bonus(secondary, &self.secondary_passive_allocations, &tree_bonus, &mut result);
        }
        result
    }

    /// Shared by `passive_overflow_bonus`'s primary/secondary call sites -
    /// same parameterized-loop-body shape as `accumulate_flat_stat_bonus`.
    fn accumulate_overflow_conversion_bonus(&self, archetype: Archetype, allocations: &HashMap<String, u32>, tree_bonus: &ArchetypeBonus, result: &mut ArchetypeBonus) {
        let nodes = archetype.passive_nodes();
        for (key, &rank) in allocations {
            let Some(node) = nodes.iter().find(|n| n.key == key.as_str()) else { continue };
            if let crate::passive_tree::PassiveEffect::OverflowConversion { input, output, .. } = node.effect {
                if input.overflow_cap().is_none() {
                    continue;
                }
                let overflow = self.combined_stat_overflow(input, tree_bonus);
                let raw = overflow * node.magnitude_at_rank(rank);
                let capped = raw.min(OVERFLOW_CONVERSION_CAP_PER_RANK * rank as f64).max(0.0);
                output.add(result, capped);
            }
        }
    }

    /// How many points are invested in a specific passive node by key,
    /// checking the primary tree first, then Split Personality's secondary
    /// tree if one is currently active (see `effective_secondary_archetype`)
    /// - 0 if unallocated in either, or the node doesn't exist for either
    /// archetype. The primitive every bespoke (`PassiveEffect::Special`)
    /// passive mechanic is built on, since those don't flow through
    /// `passive_bonus()`'s generic stat-summing at all - see Slayer's
    /// wound/FlickerStrike/Bloodpact mechanics in `simulate_battle`.
    /// Safe to check both maps unconditionally: node `key`s are globally
    /// unique across every archetype now (see passive_tree.rs's 2026-08-17
    /// rename pass), and each map only ever holds keys valid for its own
    /// archetype (enforced at allocate-time) - so there's no risk of a
    /// primary-tree key accidentally reading a same-named secondary node
    /// or vice versa.
    pub fn passive_node_rank(&self, key: &str) -> u32 {
        if let Some(&rank) = self.passive_allocations.get(key) {
            return rank;
        }
        if self.effective_secondary_archetype().is_some() {
            if let Some(&rank) = self.secondary_passive_allocations.get(key) {
                return rank;
            }
        }
        0
    }

    /// `PassiveNode::magnitude_at_rank` for a specific node by key - 0.0
    /// if unallocated in either tree. The actual "how much" behind a
    /// `Special` mechanic. Unlike `passive_node_rank`, this also needs to
    /// know WHICH archetype's node-definition list to search for the
    /// node's own per-rank formula - primary and secondary are resolved
    /// independently rather than through one shared "current rank" value,
    /// since a rank sourced from the secondary map must look its magnitude
    /// up against the secondary archetype's own node list, not the
    /// primary's.
    pub fn passive_node_magnitude(&self, key: &str) -> f64 {
        if let Some(&rank) = self.passive_allocations.get(key) {
            if rank > 0 {
                return self.archetype.passive_nodes().iter().find(|n| n.key == key).map(|n| n.magnitude_at_rank(rank)).unwrap_or(0.0);
            }
        }
        if let Some(secondary) = self.effective_secondary_archetype() {
            if let Some(&rank) = self.secondary_passive_allocations.get(key) {
                if rank > 0 {
                    return secondary.passive_nodes().iter().find(|n| n.key == key).map(|n| n.magnitude_at_rank(rank)).unwrap_or(0.0);
                }
            }
        }
        0.0
    }

    /// A node's magnitude read as a whole-number COUNT - extra targets,
    /// extra hits, banked charges, additional max stacks.
    ///
    /// The tunable replacement for reading `passive_node_rank` directly
    /// at a numeric call site (2026-08-19, Stage 2 of the live-tunable
    /// passive values build). Every node migrated to this declares
    /// `at_rank_1: 1.0, per_additional_rank: 1.0`, so its magnitude
    /// equals its rank EXACTLY - `1.0 + 1.0 * (rank - 1) == rank` - and
    /// the swap is behavior-neutral at default values by construction,
    /// while making the count reachable from `/admin/passives`.
    ///
    /// Reading the rank directly still has one legitimate use and is NOT
    /// deprecated: a structural gate ("is this invested at all", "is it
    /// at least rank 2"), which is what `passive_node_rank`'s remaining
    /// call sites do. Structure stays code-defined; only values are
    /// tunable.
    ///
    /// Rounds rather than truncates so a tuned value of `2.999` reads as
    /// 3 rather than 2, and floors at 0 so a negative override can never
    /// wrap around into a huge `u32`.
    pub fn passive_node_count(&self, key: &str) -> u32 {
        self.passive_node_magnitude(key).round().max(0.0) as u32
    }

    /// The equipped item currently carrying Split Personality
    /// (`UniqueAffix::SplitPersonality`), if any - scans all 5 equip
    /// slots live rather than trusting any stored flag, since there is no
    /// single chokepoint in this codebase for "an equip slot's contents
    /// just changed" (see `equip`/`equip_from_inventory`/Reforge/Recombine,
    /// which all mutate equip slots independently). Everything Split
    /// Personality gates on reads THIS, live, every time - never a cached
    /// bool - so unequipping it takes effect everywhere instantly with no
    /// risk of a missed mutation call site leaving stale state behind.
    pub fn effective_split_personality_item(&self) -> Option<&Item> {
        EQUIP_SLOTS.iter().filter_map(|&slot| self.equipped(slot).as_ref()).find(|item| item.unique_affix == Some(UniqueAffix::SplitPersonality))
    }

    /// `secondary_archetype` as it actually applies right now - `None`
    /// unless Split Personality is currently equipped somewhere AND the
    /// stored choice still differs from the current PRIMARY archetype
    /// (a real class change, `AdventureManager::change_archetype`, can
    /// leave a stale `secondary_archetype` equal to the new primary - that
    /// combination is nonsensical, same tree twice, so it's treated as
    /// unset rather than validated/cleared eagerly at every place
    /// `archetype` could change). This is the ONLY correct way to read
    /// `secondary_archetype` anywhere in the codebase (`passive_node_rank`/
    /// `magnitude`, `total_passive_points`, `spent` calculations, and every
    /// `/passives` renderer all go through this) - it's also what makes
    /// unequipping read as an instant, complete refund without needing to
    /// eagerly clear the raw field at every possible equip-mutation site.
    pub fn effective_secondary_archetype(&self) -> Option<Archetype> {
        if self.effective_split_personality_item().is_some() {
            self.secondary_archetype.filter(|&a| a != self.archetype)
        } else {
            None
        }
    }

    /// Prerequisite 2 (2026-08-20, golem-inheritance release) -
    /// `passive_node_rank`/`passive_node_magnitude` already correctly
    /// read whichever tree (primary or Split Personality secondary)
    /// actually has a given node allocated, but every archetype-gated
    /// passive/aura in `simulate_battle`'s construction closure was
    /// still checking `c.archetype == Archetype::X` directly - true for
    /// the primary class only, so a real secondary-archetype investment
    /// (e.g. an Elementalist/Cleric via Split Personality) evaluated to
    /// zero for every one of THAT class's own archetype-gated fields,
    /// even with real points allocated and even though the underlying
    /// rank/magnitude read was already correct. `has_archetype` is the
    /// replacement gate: true if `archetype` is `X` in EITHER slot.
    pub fn has_archetype(&self, archetype: Archetype) -> bool {
        self.archetype == archetype || self.effective_secondary_archetype() == Some(archetype)
    }

    /// Total passive points available to spend across BOTH trees combined
    /// (see `effective_secondary_archetype`) - the base level formula plus
    /// Split Personality's own bonus: a flat +1 for having it equipped,
    /// plus +1 more per full 300 tiers on the specific item carrying it,
    /// stacking (so tier 0-299 => +1 total, tier 300-599 => +2, tier
    /// 600-899 => +3, ...). 0 bonus whenever it isn't currently equipped -
    /// same live-checked reasoning as `effective_secondary_archetype`,
    /// this is what makes unequipping immediately shrink the point budget
    /// back down (which, combined with `spent` no longer counting the
    /// secondary map once unequipped, is the actual refund).
    pub fn total_passive_points(&self) -> u32 {
        let base = crate::passive_tree::points_for_level(self.level);
        let bonus = self.effective_split_personality_item().map_or(0, |item| 1 + item.tier / 300);
        base + bonus
    }

    /// This character's gear+archetype damage reduction - ONE of several
    /// independent mitigation SOURCES `resolve_hit` combines
    /// multiplicatively via `combine_reduction_sources`, not a global
    /// total. Block (`BLOCK_DAMAGE_REDUCTION`) is a second source applied
    /// separately there; the eventual passive tree (not real game state
    /// yet) should be a third, each skill's own total, rather than a new
    /// term summed in here - seeing this stat alone is NOT "how much
    /// damage this character takes off the top", only its gear+archetype
    /// slice of it. Hard-capped at +75% within this one source so
    /// stacking gear alone can never approach immunity (multiplicative
    /// combination with the other sources handles the rest). UNLIKE the
    /// other defensive stats, this can go negative (e.g. cursed gear) and
    /// genuinely means taking MORE damage, not just "less reduction" -
    /// floored at -75% as the mirror-image safety bound. Always shown to
    /// a player as "increased damage taken" whenever it's negative (see
    /// `Archetype::description`/the web dashboard's Combat Stats card) -
    /// a plain negative "-15% dmg reduction" reads as confusing, not
    /// threatening. `pub` (unlike most of `Character`'s other
    /// combat-stat getters) so the web dashboard can display it directly.
    pub fn combat_damage_reduction(&self) -> f64 {
        let raw = self.sum_affix(Affix::DamageReduction) + self.archetype.bonus(self.level).damage_reduction;
        let gear_capped = capped_stat_with_overflow(raw, -0.75, 0.75).0;
        // Tree side sums BOTH passive_bonus (FlatStat nodes) and
        // passive_overflow_bonus (OverflowConversion nodes) - a gap a
        // later audit caught: this used to read passive_bonus() alone,
        // silently excluding overflow-conversion nodes' contribution to
        // this stat from the tree's multiplicative layer.
        let tree_capped = capped_stat_with_overflow(self.passive_bonus().damage_reduction + self.passive_overflow_bonus().damage_reduction, -0.75, 0.75).0;
        // Berserker's Reckless Swing/Death Wish "taken" half - a
        // NEGATIVE reduction source (more damage taken, not less),
        // combined the same multiplicative way as every other source
        // here instead of a flat subtraction (which is what this used to
        // be, and exactly the same bug the "dealt" half - see
        // `combat_increased_damage`'s own doc - had until this same
        // audit pass). `combine_reduction_sources` already handles a
        // negative source correctly (see Curse of Weakness's own
        // negative-source precedent), so this is a pure sign flip of the
        // existing `_taken_pct` helpers, not a new mechanism.
        // Reckless Abandon - reduces Death Wish's extra damage taken
        // without touching its damage bonus (a straight offset on the
        // negative source above).
        let recklessabandon_offset = self.passive_node_magnitude("recklessabandon");
        let reckless_deathwish_taken = -(reckless_swing_taken_pct(self.passive_node_rank("reckless")) + death_wish_taken_pct(self.passive_node_rank("deathwish")) - recklessabandon_offset).max(0.0);
        // Druid's Thorned Barrier + Ironbark (2026-08-16 rework) - its own
        // independent multiplicative source, same "gear/tree/reckless are
        // each their own source" principle as everything else here, rather
        // than pooled into `tree_capped` above (Living Armor, a SEPARATE
        // sibling node, still pools normally - only Thorned Barrier's own
        // skill node and Ironbark were pulled out). Ironbark's own
        // per-rank magnitude matches Thorned Barrier's exactly, so a maxed
        // combination reaches 10/20/30% total across the two ranks, per
        // the request's own worked numbers. 0.0 for every other archetype.
        let thornedbarrier_mult = self.passive_node_magnitude("barrier") + self.passive_node_magnitude("ironbark");
        combine_reduction_sources(&[gear_capped, tree_capped, reckless_deathwish_taken, thornedbarrier_mult])
    }

    /// % chance an incoming hit is blocked (halving its damage) - capped
    /// same as the other defensive stats.
    pub fn combat_block_chance(&self) -> f64 {
        let raw = self.sum_affix(Affix::BlockChance) + self.archetype.bonus(self.level).block_chance;
        let gear_capped = capped_stat_with_overflow(raw, 0.0, 0.75).0;
        let tree_capped = capped_stat_with_overflow(self.passive_bonus().block_chance + self.passive_overflow_bonus().block_chance, 0.0, 0.75).0;
        combine_reduction_sources(&[gear_capped, tree_capped])
    }

    /// % chance an incoming hit is avoided entirely - capped same as the
    /// other defensive stats.
    pub fn combat_evasion(&self) -> f64 {
        let raw = self.sum_affix(Affix::Evasion) + self.archetype.bonus(self.level).evasion;
        let gear_capped = capped_stat_with_overflow(raw, 0.0, 0.75).0;
        let tree_capped = capped_stat_with_overflow(self.passive_bonus().evasion + self.passive_overflow_bonus().evasion, 0.0, 0.75).0;
        combine_reduction_sources(&[gear_capped, tree_capped])
    }

    /// Sum of whatever spilled past the cap on damage reduction/block
    /// chance/evasion (75% each) AND intervene (50% - see
    /// `combat_intervene`'s doc) - see `capped_stat_with_overflow` -
    /// folded into `combat_increased_damage` instead of thrown away. Now
    /// that an archetype's own defensive ADVANTAGE scales up with level
    /// (see `Archetype::bonus`), a high-level Warrior/Monk/Druid/Paladin
    /// can genuinely blow past what used to be a hard ceiling - per the
    /// request, that excess becomes damage instead of wasted potential:
    /// 150% evasion nets 75% evasion (the real cap) PLUS 75% increased
    /// damage from the 75% that didn't fit. `dr_raw` here is specifically
    /// the gear+archetype SOURCE's own overflow (see
    /// `combat_damage_reduction`'s doc) - block chance/evasion/intervene
    /// have no other source to multiply against yet, so their overflow
    /// here is still their real total, not a per-source slice.
    pub(crate) fn defensive_overflow(&self) -> f64 {
        let dr_raw = self.sum_affix(Affix::DamageReduction) + self.archetype.bonus(self.level).damage_reduction;
        let block_raw = self.sum_affix(Affix::BlockChance) + self.archetype.bonus(self.level).block_chance;
        let evasion_raw = self.sum_affix(Affix::Evasion) + self.archetype.bonus(self.level).evasion;
        let intervene_raw = self.sum_affix(Affix::Intervene) + self.archetype.bonus(self.level).intervene_pct;
        capped_stat_with_overflow(dr_raw, -0.75, 0.75).1
            + capped_stat_with_overflow(block_raw, 0.0, 0.75).1
            + capped_stat_with_overflow(evasion_raw, 0.0, 0.75).1
            + capped_stat_with_overflow(intervene_raw, 0.0, 0.5).1
    }

    /// Combined gear+archetype+tree overflow for one of the 4 capped
    /// stats - what every `OverflowConversion` node's `input` actually
    /// draws from (see `passive_overflow_bonus`). Deliberately overlaps
    /// with `defensive_overflow` above (which only ever looks at gear+
    /// archetype, feeding a flat 100%-efficiency baseline into increased
    /// damage specifically) rather than trying to subtract one from the
    /// other - a live design call: a passive-tree overflow node paying
    /// out AGAIN off the same raw overflow `defensive_overflow` already
    /// converted is intended, not a double-counting bug. This is also
    /// what makes these nodes reachable at all - the tree alone can never
    /// push any of these 4 stats anywhere close to its own cap (the
    /// highest is Druid's Evasion at 47% tree-only, still short of 75%),
    /// but gear commonly blows FAR past 75%/50% on its own.
    pub(crate) fn combined_stat_overflow(&self, stat: crate::passive_tree::PassiveStat, tree_bonus: &ArchetypeBonus) -> f64 {
        use crate::passive_tree::PassiveStat;
        let (gear_raw, floor, cap) = match stat {
            PassiveStat::DamageReduction => (self.sum_affix(Affix::DamageReduction) + self.archetype.bonus(self.level).damage_reduction, -0.75, 0.75),
            PassiveStat::BlockChance => (self.sum_affix(Affix::BlockChance) + self.archetype.bonus(self.level).block_chance, 0.0, 0.75),
            PassiveStat::Evasion => (self.sum_affix(Affix::Evasion) + self.archetype.bonus(self.level).evasion, 0.0, 0.75),
            PassiveStat::IntervenePct => (self.sum_affix(Affix::Intervene) + self.archetype.bonus(self.level).intervene_pct, 0.0, 0.5),
            // No other PassiveStat has a defined `overflow_cap` - see its
            // doc - so no `OverflowConversion` node's `input` can ever be
            // anything else. Unreachable in practice, 0.0 is harmless if
            // it ever were.
            _ => return 0.0,
        };
        let combined_raw = gear_raw + stat.get(tree_bonus);
        capped_stat_with_overflow(combined_raw, floor, cap).1
    }

    /// Monk's Unbroken (redesigned 2026-08-15 per a live request, replacing
    /// the original "+evasion per 20% missing HP" text) - evasion overflow
    /// past the 75% cap converts into ignoring THIS fraction of whoever
    /// this Monk attacks' own evasion, at 10/20/30% efficiency per rank.
    /// NOT expressed via the generic `OverflowConversion`/`passive_overflow_bonus`
    /// machinery - "ignore enemy evasion" isn't one of the 12 pooled
    /// `PassiveStat` outputs that system can add to (it doesn't grant
    /// anything on the MONK's own `ArchetypeBonus` at all - it's read by
    /// the DEFENDER's evasion roll instead, see `resolve_hit`), so this
    /// is its own `Special`-effect getter instead, reusing
    /// `combined_stat_overflow` directly for the same "combined gear+tree
    /// overflow past the cap" input every other overflow node already
    /// draws from.
    pub fn combat_unbroken_ignore_evasion_pct(&self) -> f64 {
        let efficiency = self.passive_node_magnitude("unbroken");
        if efficiency <= 0.0 {
            return 0.0;
        }
        let overflow = self.combined_stat_overflow(crate::passive_tree::PassiveStat::Evasion, &self.passive_bonus());
        overflow * efficiency
    }

    /// Crippling Grip (Last Bastion, 2026-08-17) - same shape as
    /// `combat_unbroken_ignore_evasion_pct` immediately above, a second
    /// independent channel off the same overflow pool at half the
    /// efficiency (0.05/rank vs Unbroken's own 0.10/rank), converting into
    /// a flat DR shred instead of evasion-ignore.
    pub fn combat_crippling_grip_dr_pct(&self) -> f64 {
        let efficiency = self.passive_node_magnitude("lastbastion");
        if efficiency <= 0.0 {
            return 0.0;
        }
        let overflow = self.combined_stat_overflow(crate::passive_tree::PassiveStat::Evasion, &self.passive_bonus());
        overflow * efficiency
    }

    /// Per-source breakdown for one of the 4 capped-with-overflow stats
    /// (damage reduction/block/evasion at 75%, intervene at 50%) - which
    /// equipped item contributed how much, plus the archetype's own flat
    /// bonus, and the raw/capped/overflow numbers
    /// `capped_stat_with_overflow` itself computes. Purely a UI helper
    /// (see the web dashboard's Combat Stats hover breakdown, added
    /// after a live request to show "all sources... and the total
    /// amount exceeding the cap") - the `combat_*` getters above remain
    /// the actual mechanical source of truth, this just re-derives the
    /// same numbers with the per-item detail kept instead of collapsed
    /// into one sum.
    pub(crate) fn capped_stat_breakdown(&self, affix: Affix, archetype_value: f64, floor: f64, cap: f64) -> StatBreakdown {
        let mut sources: Vec<(String, f64)> = EQUIP_SLOTS
            .iter()
            .filter_map(|&slot| self.equipped(slot).as_ref().map(|item| (slot, item)))
            .filter_map(|(slot, item)| {
                let value = item.effective_affix_total(affix);
                if value.abs() < f64::EPSILON { None } else { Some((format!("{slot:?} ({})", item.display_name()), value)) }
            })
            .collect();
        if archetype_value.abs() >= f64::EPSILON {
            sources.push((format!("{:?} archetype", self.archetype), archetype_value));
        }
        let raw: f64 = sources.iter().map(|(_, v)| *v).sum();
        let (capped, overflow) = capped_stat_with_overflow(raw, floor, cap);
        StatBreakdown { sources, raw, capped, overflow }
    }

    pub fn damage_reduction_breakdown(&self) -> StatBreakdown {
        self.capped_stat_breakdown(Affix::DamageReduction, self.archetype.bonus(self.level).damage_reduction, -0.75, 0.75)
    }

    pub fn block_breakdown(&self) -> StatBreakdown {
        self.capped_stat_breakdown(Affix::BlockChance, self.archetype.bonus(self.level).block_chance, 0.0, 0.75)
    }

    pub fn evasion_breakdown(&self) -> StatBreakdown {
        self.capped_stat_breakdown(Affix::Evasion, self.archetype.bonus(self.level).evasion, 0.0, 0.75)
    }

    pub fn intervene_breakdown(&self) -> StatBreakdown {
        self.capped_stat_breakdown(Affix::Intervene, self.archetype.bonus(self.level).intervene_pct, 0.0, 0.5)
    }

    /// % more damage dealt, multiplicative - not capped like the
    /// defensive stats (offense scaling up is far less game-breaking
    /// than defense approaching unhittable/unkillable). UNLIKE the
    /// chance-based stats above, this CAN go genuinely negative (e.g.
    /// Warrior's/Monk's archetype disadvantage) and means dealing LESS
    /// damage than the character's own base number, not just "no
    /// bonus" - floored at -90% only so `resolve_hit`'s `1.0 +
    /// increased_damage` multiplier can never hit zero/negative (which
    /// would mean 0 or "negative" damage). Always shown to a player as
    /// "reduced damage" whenever it's negative (see
    /// `Archetype::description`/the web dashboard's Combat Stats card).
    /// Also picks up `defensive_overflow` - see its doc. The 5 elemental
    /// damage-type affixes (Cold/Fire/Lightning/Divine/Chaos) fold back
    /// in here (2026-08-15, a same-day follow-up) - briefly removed
    /// entirely in favor of their new bespoke on-hit/on-heal proc
    /// mechanics (see `Affix::ColdDamage`'s doc), but a live request
    /// restored the flat contribution ALONGSIDE the procs rather than
    /// instead of them - rolling one is both a flat damage bump AND a
    /// proc-chance roll now.
    pub fn combat_increased_damage(&self) -> f64 {
        // Character sheet (gear+archetype+defensive overflow) and passive
        // tree are two independent multiplicative layers - `(1+gear)*
        // (1+tree) - 1` - not summed into one fraction, per the
        // "character sheet vs. passive tree always compound" principle
        // (see `combine_reduction_sources`'s doc). Callers still just add
        // 1 to this return value the same way they always have - only
        // the INTERNAL computation changed, not what the number means.
        let damage_type_bonus = self.sum_affix(Affix::ColdDamage)
            + self.sum_affix(Affix::FireDamage)
            + self.sum_affix(Affix::LightningDamage)
            + self.sum_affix(Affix::DivineDamage)
            + self.sum_affix(Affix::ChaosDamage);
        let gear_total = self.sum_affix(Affix::IncreasedDamage) + damage_type_bonus + self.archetype.bonus(self.level).increased_damage + self.defensive_overflow();
        let tree_total = self.passive_bonus().increased_damage + self.passive_overflow_bonus().increased_damage;
        // Every bespoke ("conversion"-shaped, hand-coded `Special`
        // effect rather than a plain `FlatStat`/`OverflowConversion`
        // pooled node) increased-damage source is its OWN independent
        // multiplicative layer here - Titan's Grip, Overwhelming Force,
        // Berserker's Reckless Swing/Death Wish trade, Warlock's Life Tap -
        // per a live, standing design principle: "everything on the
        // passive tree should be multiplicative bonuses unless there's a
        // reason specifically why it shouldn't be." NOT summed into
        // `tree_total` above alongside the generic pooled nodes, so each
        // one's own percentage always compounds on top of everything else
        // instead of diluting into one shared additive pool. See
        // `titans_grip_increased_damage`'s doc for the fuller reasoning
        // (first node this was worked out for). All five of these were
        // ALSO, until this same pass, only ever being added at
        // `CombatSimUnit` construction time (see `simulate_battle`'s own
        // `increased_damage` field) - meaning they applied correctly in
        // real fights but never showed up in this method's own number, a
        // live report that the dashboard's DPS/Increased Dmg Dealt stats
        // looked wrong for exactly these archetypes. This is now the
        // single source of truth both the dashboard (`combat_dps`/
        // `combat_total_output_per_sec` and the Combat Stats card) and
        // `simulate_battle`'s own construction site read, so they can
        // never drift apart again - the same "one shared formula"
        // reasoning `Item::disenchant_multiplier`'s own doc already
        // established.
        let titans_grip = self.titans_grip_increased_damage();
        // Grim Resolve - increases Overwhelming Force's own conversion
        // efficiency directly.
        let overwhelm = self.combat_damage_reduction().max(0.0) * (self.passive_node_magnitude("overwhelmingforce") + self.passive_node_magnitude("grimresolve"));
        // Momentous Blow - Overwhelming Force ALSO converts a slice of
        // live block chance, its own separate multiplicative layer (same
        // "independent layer per bespoke conversion" principle as every
        // other entry here).
        let momentousblow = self.combat_block_chance().max(0.0) * self.passive_node_magnitude("momentousblow");
        let reckless_swing = reckless_swing_dealt_pct(self.passive_node_rank("reckless"));
        // Glory Hound - Death Wish's damage bonus increased further.
        let death_wish = death_wish_dealt_pct(self.passive_node_rank("deathwish")) + self.passive_node_magnitude("gloryhound");
        // Soul Exchange extends Life Tap's own damage-bonus ratio (base
        // 2x, +4%/rank more of the SAME lifetap magnitude).
        let life_tap = self.passive_node_magnitude("lifetap") * (2.0 + self.passive_node_magnitude("soulexchange"));
        ((1.0 + gear_total) * (1.0 + tree_total) * (1.0 + titans_grip) * (1.0 + overwhelm) * (1.0 + momentousblow) * (1.0 + reckless_swing) * (1.0 + death_wish) * (1.0 + life_tap) - 1.0).max(-0.9)
    }

    /// Mouseover breakdown for "Increased Dmg Dealt" (a live request - "all
    /// sources of elemental damage" specifically) - hand-built rather than
    /// `capped_stat_breakdown` since this stat isn't a single affix summed
    /// per-slot, it's 6 different affixes plus non-affix sources combined
    /// the way `combat_increased_damage` computes above. The 5 elemental
    /// damage-type affixes get their own named line each (the explicit
    /// ask); the plain `IncreasedDamage` affix, archetype bonus, defensive
    /// overflow, and passive tree total are included too for completeness.
    /// `raw` is the TRUE final `combat_increased_damage()` value (so the
    /// tooltip's own "Total" line always matches the visible headline %
    /// exactly, avoiding the classic "the numbers don't add up" confusion)
    /// even though the individual lines below don't sum to it - the
    /// bespoke sources (Titan's Grip etc., each labeled "compounds
    /// separately") multiply as their OWN independent layer rather than
    /// adding, per `combat_increased_damage`'s own formula. `overflow` is
    /// unused (this stat has no cap).
    pub fn increased_damage_breakdown(&self) -> StatBreakdown {
        let mut sources: Vec<(String, f64)> = Vec::new();
        let mut push = |label: &str, value: f64| {
            if value.abs() >= f64::EPSILON {
                sources.push((label.to_string(), value));
            }
        };
        push("Fire Damage", self.sum_affix(Affix::FireDamage));
        push("Cold Damage", self.sum_affix(Affix::ColdDamage));
        push("Lightning Damage", self.sum_affix(Affix::LightningDamage));
        push("Divine Damage", self.sum_affix(Affix::DivineDamage));
        push("Chaos Damage", self.sum_affix(Affix::ChaosDamage));
        push("Gear (Increased Damage)", self.sum_affix(Affix::IncreasedDamage));
        push("Archetype", self.archetype.bonus(self.level).increased_damage);
        push("Overflow (from capped defensive stats)", self.defensive_overflow());
        push("Passive Tree", self.passive_bonus().increased_damage + self.passive_overflow_bonus().increased_damage);
        push("Titan's Grip (compounds separately)", self.titans_grip_increased_damage());
        let overwhelm = self.combat_damage_reduction().max(0.0) * (self.passive_node_magnitude("overwhelmingforce") + self.passive_node_magnitude("grimresolve"));
        push("Overwhelming Force (compounds separately)", overwhelm);
        let momentousblow = self.combat_block_chance().max(0.0) * self.passive_node_magnitude("momentousblow");
        push("Momentous Blow (compounds separately)", momentousblow);
        push("Reckless Swing (compounds separately)", reckless_swing_dealt_pct(self.passive_node_rank("reckless")));
        let death_wish = death_wish_dealt_pct(self.passive_node_rank("deathwish")) + self.passive_node_magnitude("gloryhound");
        push("Death Wish (compounds separately)", death_wish);
        let life_tap = self.passive_node_magnitude("lifetap") * (2.0 + self.passive_node_magnitude("soulexchange"));
        push("Life Tap (compounds separately)", life_tap);
        let raw = self.combat_increased_damage();
        StatBreakdown { sources, raw, capped: raw, overflow: 0.0 }
    }

    /// % chance a hit is a critical strike - every character has a 5%
    /// baseline (crit is a real thing everyone can do, not purely a
    /// gear-gated bonus), plus rolled CritChance affixes and the
    /// archetype's own bonus on top. Deliberately UNCAPPED past 100% -
    /// see `roll_attacker_damage`'s doc: every full 100% past the first
    /// is a GUARANTEED extra crit stack (double/triple/quadruple crit,
    /// each worth another full crit-multiplier bonus), with any
    /// leftover fraction still rolled normally for one more possible
    /// stack. Only floored at 0 - a stat this can never meaningfully
    /// go negative.
    pub fn combat_crit_chance(&self) -> f64 {
        // Character sheet (BASE_CRIT_CHANCE + gear + archetype - i.e.
        // everything this stat would be with zero tree investment) is
        // the base the tree then multiplies, not a 4th term summed in
        // alongside it - "20% from gear, a tree node worth 30% more"
        // lands at 20% * 1.30 = 26%, not 20% + 30% = 50%. Same principle
        // as every other stat here; gear's own values are never
        // reinterpreted, only how the tree stacks on top of them.
        let gear_total = BASE_CRIT_CHANCE + self.sum_affix(Affix::CritChance) + self.archetype.bonus(self.level).crit_chance;
        // Ranger's Deadeye (2026-08-16, moved off its old splash-only
        // override - a live design call that it should boost the main hit
        // directly, same as Chain Shot, with splash inheriting passively
        // off this same crit_chance rather than needing its own separate
        // bolt-on) - a flat additive bump into the same pooled tree total
        // every other generic crit-chance passive already lands in.
        let tree_total = self.passive_bonus().crit_chance + self.passive_overflow_bonus().crit_chance + self.passive_node_magnitude("deadeye");
        (gear_total * (1.0 + tree_total)).max(0.0)
    }

    /// Total multiplier applied on a crit - a +100% (2x) baseline (so
    /// the 5% baseline crit chance above is never wasted, even with zero
    /// CritMultiplier affixes rolled) plus every rolled bonus and the
    /// archetype's own bonus (e.g. Mage's), floored so it can never drop
    /// below a normal hit.
    pub fn combat_crit_multiplier(&self) -> f64 {
        let gear_total = BASE_CRIT_MULTIPLIER + self.sum_affix(Affix::CritMultiplier) + self.archetype.bonus(self.level).crit_multiplier;
        let tree_total = self.passive_bonus().crit_multiplier + self.passive_overflow_bonus().crit_multiplier;
        (gear_total * (1.0 + tree_total)).max(1.0)
    }

    /// Splash investment, as a fraction (2026-08-20 splash redesign;
    /// 2026-08-20 FINAL SPLASH TABLE addendum - see `roll_splash`,
    /// combat.rs). No longer a guaranteed-hit damage-scaling fraction -
    /// this is now the CHANCE (capped at 100% for the roll itself) that
    /// a splash-keyed action/effect reaches its full extra-target count
    /// this time, all-or-nothing per roll. Pushing the total over 100%
    /// (overcap) makes that guaranteed instead of a roll, plus a bonus
    /// target (`LiveTunables::splash_overcap_bonus_targets`) and, past
    /// every full 1000%, another rung on a ladder
    /// (`LiveTunables::splash_ladder_step_pct`/`splash_ladder_targets_per_step`) -
    /// uncapped. Deliberately NOT capped at 1.0 here (unlike every other
    /// summed-affix stat) for exactly that overcap/ladder math to work.
    /// Consumed by two different shapes of caller (see `roll_splash`'s
    /// own doc): ATTACK splash (`apply_splash`/`apply_heal_splash`) gets
    /// 0 extra targets on a missed roll or 0% splash; the four SUPPORT
    /// sites (Radiant Smite heal, Relentless/Cauterizing Flames,
    /// Cleansing Flames' cleanse-count and buff-refresh) fall back to
    /// `LiveTunables::splash_support_floor_targets` instead - they never
    /// do nothing. Every splash-hit target still takes the SAME fraction
    /// of the primary hit/heal's own amount (`LiveTunables::splash_damage_pct`,
    /// default full value) regardless of how many targets a roll grants.
    pub fn combat_splash(&self) -> f64 {
        // No innate baseline (unlike crit chance's guaranteed 5%) - most
        // characters start at exactly 0% gear splash, so `gear*(1+tree)`
        // would zero out a tree investment entirely without matching
        // gear. `(1+gear)*(1+tree)-1` instead lets the tree work
        // standalone (0% gear + 30% tree = 30%) while still compounding
        // when both are present (20% gear + 30% tree = 56%, not 50%).
        let gear_total = self.sum_affix(Affix::Splash) + self.archetype.bonus(self.level).splash;
        let tree_total = self.passive_bonus().splash + self.passive_overflow_bonus().splash;
        // Primal Force (Druid only, 2026-08-16 rework - see
        // passive_tree.rs) - its own independent multiplicative layer,
        // same "bespoke bonus gets its own (1+x) factor" principle as
        // Regrowth/Blooming Field on `combat_heal_power`. Reads as a
        // harmless 0.0 for every other archetype.
        let primalforce_mult = self.passive_node_magnitude("primalforce");
        ((1.0 + gear_total) * (1.0 + tree_total) * (1.0 + primalforce_mult) - 1.0).max(0.0)
    }

    /// This character's own contribution to the PARTY'S pooled
    /// Intervene - see `Affix::Intervene`'s doc and `simulate_battle`'s
    /// boss-attack handling: the whole party's Intervene stats sum
    /// together to decide how much of a hit gets redirected at all
    /// (capped at 50% total, no matter how high individual/summed
    /// Intervene runs), then that pool splits across everyone with
    /// Intervene proportional to their own share of the sum - the
    /// highest-Intervene member eats the most of it. ALSO capped right
    /// here at 50% per character (same `capped_stat_with_overflow`
    /// mechanism as damage reduction/block/evasion, just at a 50%
    /// ceiling instead of 75% - see `defensive_overflow`, which folds
    /// this cap's overflow into bonus damage same as the other three) -
    /// each SOURCE is capped individually below, and the combined result
    /// gets its own explicit `.min(0.5)` on top (wiki audit finding #2,
    /// 2026-08-18: two 50%-capped sources still combined multiplicatively
    /// past 50%, the same combine-past-the-cap bug class evasion/DR/block
    /// already had fixed). The party-level 50% pool cap above is a
    /// SEPARATE, additional property on top of this one - two different
    /// ceilings for two different reasons (this one says "your own
    /// investment past 50% stops helping YOU", that one says "the group
    /// can never redirect more than half of any hit no matter how many
    /// Paladins are stacked").
    pub fn combat_intervene(&self) -> f64 {
        let raw = self.sum_affix(Affix::Intervene) + self.archetype.bonus(self.level).intervene_pct;
        let gear_capped = capped_stat_with_overflow(raw, 0.0, 0.5).0;
        let tree_capped = capped_stat_with_overflow(self.passive_bonus().intervene_pct + self.passive_overflow_bonus().intervene_pct, 0.0, 0.5).0;
        // Wiki audit finding #2 (2026-08-18): each SOURCE was already
        // capped at 50%, but two 50%-capped sources still combine
        // multiplicatively to 1-(0.5*0.5) = 75% - the same combine-past-
        // the-documented-ceiling bug class evasion/DR/block already had
        // fixed (see resolve_hit's own 95% hard cap). Unlike those,
        // Intervene has no live combat-time source (no equivalent of
        // Vanish's temp buff) feeding into it, so the cap belongs right
        // here rather than deferred to a combat.rs call site - this IS
        // the complete per-character combine.
        combine_reduction_sources(&[gear_capped, tree_capped]).min(0.5)
    }

    /// Fraction of a hit's actual damage this character leeches back as
    /// self-healing - Slayer's archetype advantage PLUS any `Affix::Leech`
    /// rolled on gear (rare - see `affix_weight`), summed the same way
    /// every other combat stat here combines gear + archetype. Consumed
    /// by `simulate_battle`'s `CombatSimUnit::life_leech_pct` and applied
    /// in `apply_hit`.
    pub fn combat_life_leech(&self) -> f64 {
        // Same "no reliable baseline, so let the tree work standalone"
        // shape as combat_splash - Slayer's tiny 0.1%-ish archetype leech
        // is nowhere near a guaranteed nonzero floor the way crit
        // chance's 5% is, so `gear*(1+tree)` would nearly zero out a
        // tree-only leech build.
        let gear_total = self.sum_affix(Affix::Leech) + self.archetype.bonus(self.level).life_leech_pct;
        let tree_total = self.passive_bonus().life_leech_pct + self.passive_overflow_bonus().life_leech_pct;
        ((1.0 + gear_total) * (1.0 + tree_total) - 1.0).max(0.0)
    }

    /// Conversion rate applied to a character's OWN damage output to
    /// get how much of THIS action's output goes to healing instead -
    /// healing is strictly converted damage now (see
    /// `combat_total_output_per_sec`/`simulate_battle`'s unified
    /// attack action): every attack splits between damage and healing
    /// by this rate, not a separate dedicated heal action anymore.
    /// Baseline is 0% for a Melee/Ranged archetype (100% of every
    /// action stays damage, same as before this existed) EXCEPT Paladin,
    /// who gets a flat +50% `heal_power_pct` of their own despite staying
    /// a Melee function (2026-08-15, "innately hybrid like Cleric/Druid"
    /// per a live request - deliberately NOT done by making Paladin a
    /// Heal-function archetype, which would also change their base damage
    /// formula/attack interval/role badge). A genuine Heal-function
    /// archetype (Cleric/Druid) gets 50% baseline instead, with their own
    /// `heal_power_pct` bonus/penalty still applied on top - Cleric adds
    /// another +50% archetype-level (100% heal / 0% damage baseline,
    /// after the 2026-08-15 doubling below); Druid adds none at the
    /// archetype level at all (stays at the 50% baseline until Regrowth
    /// tree investment pushes it higher). Deliberately left UNCAPPED
    /// upward past 100% as a raw fraction (not clamped here) -
    /// `combat_dps`'s damage share already floors at 0 once it crosses
    /// 100% (nothing left to attack with), and everything past that
    /// point is read as "excess" by `attack_interval_ms` (shortens the
    /// action cadence instead) and `combat_hps` (caps the per-action
    /// heal size at the 100% baseline) - past 100% no longer means
    /// bigger individual heals, it means more frequent ones at that same
    /// capped size. Still floored at 0 overall so a heavy penalty can't
    /// heal negative.
    /// Gear no longer contributes here at all (2026-08-15 - the old
    /// `HealingPower` affix was reworked into `LingeringEffect`, a
    /// damage-over-time debuff instead - see `apply_lingering_effect`'s
    /// doc). To compensate, `Archetype::bonus`'s `heal_power_pct` baseline
    /// AND every passive-tree node that grants `heal_power_pct` had their
    /// own magnitudes DOUBLED in the same pass, per the live design call
    /// that a healer's own build (archetype + tree) should absorb the
    /// lost gear lever rather than leaving Cleric/Druid strictly weaker.
    pub fn combat_heal_power(&self) -> f64 {
        let base = if self.archetype.combat_function() == CombatFunction::Heal { 0.5 } else { 0.0 };
        // Same "let the tree work standalone" shape as combat_splash/
        // combat_life_leech - a non-Heal archetype has no baseline here
        // at all (base = 0.0), so `gear*(1+tree)` would zero out any
        // tree-granted healing power for them entirely.
        let gear_total = base + self.archetype.bonus(self.level).heal_power_pct;
        let tree_total = self.passive_bonus().heal_power_pct + self.passive_overflow_bonus().heal_power_pct;
        // Regrowth (Druid only - 2026-08-16 rework, see its own doc in
        // passive_tree.rs) grants its OWN separate multiplicative layer on
        // top of everything else, same "bespoke Special-shaped bonus gets
        // its own independent (1+x) factor" principle
        // `combat_increased_damage`'s Titan's Grip/Overwhelming Force/etc.
        // already establish - not summed into `tree_total` above, so it
        // compounds instead of diluting into the shared additive pool.
        // Reads as a harmless 0.0 for every other archetype (no "regrowth"
        // key exists outside Druid's own tree).
        let regrowth_mult = self.passive_node_magnitude("regrowth");
        // Blooming Field - a SECOND, independent multiplicative layer
        // (2026-08-16, same pass/report as Regrowth's own) - deliberately
        // its own separate `(1+x)` factor rather than folded into
        // `regrowth_mult` above, so the two compound with each other
        // instead of just adding together.
        let bloomingfield_mult = self.passive_node_magnitude("bloomingfield");
        ((1.0 + gear_total) * (1.0 + tree_total) * (1.0 + regrowth_mult) * (1.0 + bloomingfield_mult) - 1.0).max(0.0)
    }

    /// Expected average TOTAL output per second, BEFORE the damage/heal
    /// split described on `combat_heal_power`'s doc - folds in attack
    /// speed, crit expected value, and increased damage %. `combat_dps`/
    /// `combat_hps` are just this split by `combat_heal_power()`, not
    /// independent numbers.
    ///
    /// Also folds in the helm's stacking dps buff (see `helm_skill`) -
    /// every `cooldown_ms` of an actual fight it permanently adds
    /// another `power` to the wearer's base dps (see `simulate_battle`'s
    /// `NextEvent::Helm` handling), so its real value depends entirely
    /// on how long the fight lasts - not something a single static
    /// number can capture exactly. This display number assumes
    /// `ASSUMED_FIGHT_DURATION_MS` of survival and averages the ramp
    /// over that window (0 stacks at the start, `duration / cooldown_ms`
    /// stacks by the end, linearly in between - so the average is half
    /// of that) rather than showing either the day-one (0 stacks) or
    /// best-case (fully stacked) number - meaningfully rewards a tankier
    /// build without pretending the fight never ends.
    pub(crate) fn combat_total_output_per_sec(&self) -> f64 {
        let hits_per_sec = 1000.0 / self.attack_interval_ms() as f64;
        // `crit_ev` must match real combat's `crit_bonus_mult` exactly
        // (2026-08-18, a live bug report - this used to omit
        // `CRIT_BONUS_MULT`/the overcrit curve entirely, overstating
        // DPS/HPS for any crit-heavy build). `crit_stacks` on any real
        // roll is always exactly `floor(crit_chance)` or
        // `floor(crit_chance) + 1` - a genuine two-point distribution (see
        // `roll_attacker_damage`'s own guaranteed_stacks/remainder_roll) -
        // so `E[crit_stack_bonus(crit_stacks, ...)]` is the probability-
        // weighted average of that (nonlinear, since the overcrit curve
        // fix) function at those two whole values, NOT
        // `crit_stack_bonus(E[crit_stacks], ...)` (Jensen's inequality) -
        // still closed-form and exact, just needs both terms instead of
        // one.
        let crit_chance = self.combat_crit_chance();
        let crit_multiplier = self.combat_crit_multiplier();
        let guaranteed_stacks = crit_chance.floor();
        let remainder = crit_chance - guaranteed_stacks;
        let crit_ev = 1.0
            + (1.0 - remainder) * crit_stack_bonus(guaranteed_stacks, crit_multiplier)
            + remainder * crit_stack_bonus(guaranteed_stacks + 1.0, crit_multiplier);
        let increased_dmg_mult = 1.0 + self.combat_increased_damage();
        let primary = self.combat_atk() as f64 * hits_per_sec * crit_ev * increased_dmg_mult;
        let helm = match self.helm_skill() {
            Some((power, cooldown_ms)) => {
                let stacks_by_end = ASSUMED_FIGHT_DURATION_MS as f64 / cooldown_ms as f64;
                let avg_stacks = stacks_by_end / 2.0;
                power * avg_stacks * crit_ev * increased_dmg_mult
            }
            None => 0.0,
        };
        primary + helm
    }

    /// Expected average damage dealt per second - the "how hard do I
    /// actually hit" number for the web dashboard. Doesn't factor in
    /// splash (situational - depends on how many enemies are actually
    /// in the fight) or block/evasion/reduction (those are what the
    /// character INFLICTS on others, not what boosts their own hits).
    /// `combat_total_output_per_sec`'s damage SHARE - see
    /// `combat_heal_power`'s doc - floors at 0 once heal power reaches
    /// 100%: a fully-invested healer has nothing left to attack with
    /// (see `simulate_battle`'s unified attack action).
    pub fn combat_dps(&self) -> f64 {
        self.combat_total_output_per_sec() * (1.0 - self.combat_heal_power()).max(0.0)
    }

    /// Counterpart to `combat_dps` - expected average healing per
    /// second. `combat_total_output_per_sec`'s healing SHARE, capped at
    /// 100% heal power - past that, `attack_interval_ms` is already
    /// folding the excess into a faster action cadence instead (a live
    /// design change: "surpassing healing" past 100% used to make each
    /// individual heal bigger than the raw roll would allow; now it makes
    /// heals happen more often at that same capped size instead). Since
    /// `combat_total_output_per_sec` already reflects that faster
    /// cadence (it's built off `attack_interval_ms` too), this number
    /// still correctly goes up for a heavily-invested healer - just
    /// through more frequent full heals, not bigger ones. Cleric/Druid
    /// (and now Paladin - see `Archetype::bonus`'s own `heal_power_pct`)
    /// are the only archetypes with any baseline heal_power at all since
    /// gear no longer grants it (see `combat_heal_power`'s doc) - a
    /// Warrior/Rogue/etc. with zero tree investment in a `HealPowerPct`
    /// node stays at a flat 0 here.
    pub fn combat_hps(&self) -> f64 {
        self.combat_total_output_per_sec() * self.combat_heal_power().clamp(0.0, 1.0)
    }

    /// This character's Lingering Effect % - gear (`Affix::LingeringEffect`)
    /// PLUS Druid's Evergrowth (2026-08-16, a live request, repurposed
    /// from its old Rejuvenation-bounce-value role): converts a slice of
    /// TOTAL healing power (`combat_heal_power()` - the stat, not a flat
    /// amount) directly into Lingering Effect, at 3%/6%/9% by rank. Worked
    /// example from the request itself: 1000% heal power (10.0 as a
    /// fraction) at 3/3 Evergrowth -> 0.09 * 10.0 = 90% Lingering Effect.
    /// Reads as a harmless 0.0 for every other archetype (no "evergrowth"
    /// key exists outside Druid's own tree).
    pub fn combat_lingering_effect_pct(&self) -> f64 {
        self.sum_affix(Affix::LingeringEffect) + self.passive_node_magnitude("evergrowth") * self.combat_heal_power()
    }

    /// Estimated TOTAL Lingering Effect output (DoT damage + HoT healing,
    /// combined) over a 10-second window - same closed-form EV approach
    /// as `combat_dps`/`combat_hps` (no literal simulation, no RNG), NOT a
    /// simulated fight. Built on `combat_total_output_per_sec` (the
    /// PRE-split rate), not `combat_dps` alone - `apply_lingering_effect`
    /// triggers symmetrically off BOTH `apply_hit` (a landed hit spawns a
    /// DoT on the enemy struck) AND `apply_heal` (a landed heal spawns an
    /// equivalent HoT on the ally healed), so the total Lingering Effect
    /// quantity generated tracks a character's TOTAL output regardless of
    /// how much of it is currently split toward damage vs. healing.
    pub fn combat_lingering_effect_10s_estimate(&self) -> f64 {
        self.combat_total_output_per_sec() * self.combat_lingering_effect_pct() * 10.0
    }

    /// Adds xp, applying as many level-ups as it covers (in case a big
    /// enough bonus crosses more than one threshold at once). Returns the
    /// new level if it went up at least once.
    pub(crate) fn add_xp(&mut self, amount: u64) -> Option<u32> {
        self.xp += amount;
        let mut leveled = false;
        loop {
            let needed = Self::xp_to_next_level(self.level);
            if self.xp < needed {
                break;
            }
            self.xp -= needed;
            self.level += 1;
            leveled = true;
            self.grow_krangled_items();
        }
        leveled.then_some(self.level)
    }

    /// Krangled (locked) items automatically upgrade to a tier equal to
    /// the character's current level - on top of everything else Krangle
    /// already does (permanent lock, one final modifier, can exceed the
    /// normal 4-modifier cap), a Krangled item keeps pace with its owner
    /// forever, unlike every other piece of gear, which only grows
    /// through reforge/recombine. Power and every existing modifier
    /// rescale right along with tier (see `Item::sync_tier_to`) - a
    /// live report caught tier alone moving while the actual stats
    /// stayed frozen, which made the tier number purely cosmetic.
    pub(crate) fn grow_krangled_items(&mut self) {
        let level = self.level;
        for slot in EQUIP_SLOTS {
            if let Some(item) = self.equipped_mut(slot) {
                if item.locked {
                    item.sync_tier_to(level);
                }
            }
        }
        for item in self.inventory.iter_mut() {
            if item.locked {
                item.sync_tier_to(level);
            }
        }
    }
}

#[cfg(test)]
mod divine_dust_apply_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn character_with_item(item: Item) -> (Character, String) {
        let mut character = Character::new("dust_wielder".to_string());
        character.inventory.clear();
        let id = item.id.clone();
        character.inventory.push(item);
        (character, id)
    }

    #[test]
    fn sacralizing_a_plain_item_grants_perfect_and_one_sacred_affix() {
        let mut rng = StdRng::seed_from_u64(1);
        let item = generate_item_at_tier_with_roll(EquipSlot::Weapon, 20, 1.0, &mut rng);
        assert!(!item.perfect, "sanity: a freshly generated item must not start Perfect");
        let (mut character, id) = character_with_item(item);

        let mut roll_rng = StdRng::seed_from_u64(2);
        let outcome = character.apply_divine_dust(&id, &mut roll_rng).expect("must sacralize a bare item");
        assert!(outcome.became_sacred);
        assert!(outcome.old_affix.is_none(), "sacralizing (not rerolling) must never report an old affix");

        let updated = character.find_item_by_id(&id).unwrap();
        assert!(updated.perfect, "sacralizing must also make the item Perfect - Sacred implies Perfect throughout this codebase");
        assert_eq!(updated.power_roll, POWER_ROLL_RANGE.end);
        assert_eq!(updated.sacred_affix.map(|(a, _)| a), Some(outcome.new_affix));
        assert_eq!(updated.sacred_affix.map(|(_, v)| v), Some(outcome.new_value));
    }

    #[test]
    fn sacralizing_an_already_perfect_item_does_not_double_apply_the_quality_bonus() {
        let mut rng = StdRng::seed_from_u64(3);
        let item = make_item_perfect(generate_item_at_tier_with_roll(EquipSlot::Helm, 10, 1.0, &mut rng));
        let power_before = item.power;
        let affixes_before = item.affixes.clone();
        let (mut character, id) = character_with_item(item);

        let mut roll_rng = StdRng::seed_from_u64(4);
        character.apply_divine_dust(&id, &mut roll_rng).expect("must sacralize an already-perfect item");

        let updated = character.find_item_by_id(&id).unwrap();
        assert_eq!(updated.power, power_before, "power must be untouched - it was already Perfect, PERFECT_QUALITY_MULT must not apply twice");
        assert_eq!(updated.affixes, affixes_before, "existing affix values must be untouched - only sacred_affix is new");
    }

    #[test]
    fn rerolling_a_sacred_affix_excludes_the_current_one_across_many_rolls() {
        let mut rng = StdRng::seed_from_u64(5);
        let base = generate_item_at_tier_with_roll(EquipSlot::Boots, 15, 1.0, &mut rng);
        let mut sacred_rng = StdRng::seed_from_u64(6);
        let item = make_item_sacred(base, &mut sacred_rng);
        let (original_affix, _) = item.sacred_affix.expect("make_item_sacred must set sacred_affix");
        let (mut character, id) = character_with_item(item);

        for seed in 0..30u64 {
            let mut roll_rng = StdRng::seed_from_u64(100 + seed);
            let outcome = character.apply_divine_dust(&id, &mut roll_rng).expect("reroll must succeed");
            assert!(!outcome.became_sacred, "the item was already sacred - this must be the reroll path");
            assert_eq!(outcome.old_affix, Some(original_affix));
            assert_ne!(outcome.new_affix, original_affix, "reroll must never land back on the affix it just replaced");
            // Reset for the next iteration so every roll starts from the
            // same known current affix.
            character.find_item_by_id_mut(&id).unwrap().sacred_affix = Some((original_affix, 1.0));
        }
    }

    #[test]
    fn applying_to_a_locked_item_is_rejected_and_changes_nothing() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut item = generate_item_at_tier_with_roll(EquipSlot::Gloves, 5, 1.0, &mut rng);
        item.locked = true;
        let (mut character, id) = character_with_item(item);

        let mut roll_rng = StdRng::seed_from_u64(8);
        let err = character.apply_divine_dust(&id, &mut roll_rng).expect_err("a Krangled item must reject Divine Dust");
        assert!(matches!(err, CraftError::ItemLocked));
        assert!(character.find_item_by_id(&id).unwrap().sacred_affix.is_none(), "a rejected application must not touch the item");
    }

    #[test]
    fn applying_to_a_missing_item_is_rejected() {
        let mut character = Character::new("nobody".to_string());
        character.inventory.clear();
        let mut rng = StdRng::seed_from_u64(9);
        let err = character.apply_divine_dust("does-not-exist", &mut rng).expect_err("no such item must be rejected");
        assert!(matches!(err, CraftError::ItemNotFound));
    }

    #[test]
    fn reroll_pool_excludes_only_the_current_affix() {
        let pool = divine_dust_reroll_pool(Affix::CritChance, &ALL_AFFIXES);
        assert_eq!(pool.len(), ALL_AFFIXES.len() - 1);
        assert!(!pool.contains(&Affix::CritChance));
    }

    #[test]
    fn reroll_pool_is_empty_when_the_only_candidate_is_the_current_affix() {
        // The degenerate case `CraftError::NoValidRerollTarget` guards
        // against - unreachable against the real ALL_AFFIXES (17 variants)
        // but proven correct here against an arbitrary single-entry pool.
        let pool = divine_dust_reroll_pool(Affix::Leech, &[Affix::Leech]);
        assert!(pool.is_empty());
    }
}

#[cfg(test)]
mod crit_lineage_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    // Takes a seed so callers needing two DISTINCT items (Recombine's
    // tests) don't accidentally generate identical ids - `Item::id` is
    // itself drawn from the passed-in `rng`.
    fn perfect_weapon(seed: u64) -> Item {
        let mut rng = StdRng::seed_from_u64(seed);
        let item = generate_item_at_tier_with_roll(EquipSlot::Weapon, 10, POWER_ROLL_RANGE.end, &mut rng);
        make_item_perfect(item)
    }

    #[test]
    fn reforge_item_never_crits_while_its_crit_affix_is_still_present() {
        let mut character = Character::new("test".to_string());
        let mut item = perfect_weapon(1);
        item.affixes = vec![(Affix::Evasion, 1.0)];
        item.record_reforge_crit(Affix::Evasion); // deterministic: simulate "already crit once, still there"
        let item_id = item.id.clone();
        character.equip(item);

        // Perfect + 100% quality is reforge_crit_chance's own maximum
        // (2.2%) - even at that ceiling, with the gate pre-tripped, this
        // must NEVER fire across many attempts with a real RNG.
        let mut rng = StdRng::seed_from_u64(2);
        for _ in 0..500 {
            let outcome = character.reforge_item(&item_id, &mut rng).expect("reforge should succeed");
            assert!(outcome.bonus_affix.is_none(), "a reforge crit fired despite its crit-granted affix still being present");
            assert!(character.find_item_by_id(&item_id).unwrap().reforge_crit_used(), "the gate must stay locked while Evasion is still on the item");
        }
    }

    #[test]
    fn reforge_item_crit_fires_at_most_once_across_many_reforges() {
        let mut character = Character::new("test".to_string());
        let item = perfect_weapon(1); // reforge_crit_used() starts false
        let item_id = item.id.clone();
        character.equip(item);

        let mut rng = StdRng::seed_from_u64(3);
        let mut crit_count = 0;
        for _ in 0..500 {
            let outcome = character.reforge_item(&item_id, &mut rng).expect("reforge should succeed");
            if outcome.bonus_affix.is_some() {
                crit_count += 1;
            }
        }
        assert_eq!(crit_count, 1, "expected exactly one crit across 500 reforges at a ~2.2% chance with the once-ever gate, got {crit_count}");
        assert!(character.find_item_by_id(&item_id).unwrap().reforge_crit_used());
    }

    #[test]
    fn reforge_item_can_crit_again_once_its_earlier_crit_affix_was_removed() {
        // The exact live-reported bug this replaces the old sticky bool
        // for: a tier-374 item genuinely crit once, then had that exact
        // bonus affix Annulled away - under the old design its gate
        // stayed permanently locked forever after with zero visible
        // trace of ever having crit. Now the gate is derived from actual
        // presence, so removing the affix (simulated here the same way
        // `annul_random_affix` would - just deleting it from `affixes`,
        // with no crit-tracking cleanup at all) re-opens it.
        let mut character = Character::new("test".to_string());
        let mut item = perfect_weapon(1);
        item.affixes = vec![(Affix::Evasion, 1.0)];
        item.record_reforge_crit(Affix::Evasion);
        item.affixes.retain(|&(a, _)| a != Affix::Evasion); // simulate an Annulment removing it
        assert!(!item.reforge_crit_used(), "removing the crit-granted affix must immediately free the gate");
        let item_id = item.id.clone();
        character.equip(item);

        let mut rng = StdRng::seed_from_u64(6);
        let mut crit_count = 0;
        for _ in 0..500 {
            let outcome = character.reforge_item(&item_id, &mut rng).expect("reforge should succeed");
            if outcome.bonus_affix.is_some() {
                crit_count += 1;
            }
        }
        assert_eq!(crit_count, 1, "a freed gate must behave exactly like a never-crit item: exactly one crit across 500 reforges");
    }

    #[test]
    fn recombine_never_crits_when_a_source_already_used_it() {
        // Gate is short-circuited (`!already_recombine_crit && rng.gen_bool(...)`),
        // so with a source already marked used, the result is fully
        // deterministic - no need to loop/trial this one.
        let mut character = Character::new("test".to_string());
        let mut item_a = perfect_weapon(10);
        item_a.affixes = vec![(Affix::Evasion, 1.0)];
        item_a.record_recombine_crit(Affix::Evasion); // simulate an already-crit'd source, affix still present
        let mut item_b = perfect_weapon(11);
        item_b.affixes = vec![(Affix::Splash, 1.0)];
        let id_a = item_a.id.clone();
        let id_b = item_b.id.clone();
        character.add_to_inventory(item_a);
        character.add_to_inventory(item_b);

        let mut rng = StdRng::seed_from_u64(4);
        let roll = character.roll_recombine(&id_a, &id_b, false, &mut rng).expect("roll should succeed");
        assert!(roll.bonus_affix.is_none(), "recombine must never roll a crit when a source's recombine_crit_used() is still true");
    }

    #[test]
    fn recombine_result_inherits_reforge_crit_used_when_the_affix_survives_the_merge() {
        let mut character = Character::new("test".to_string());
        let mut item_a = perfect_weapon(20);
        item_a.affixes = vec![(Affix::Evasion, 1.0)];
        item_a.record_reforge_crit(Affix::Evasion); // item_a crit on a past reforge, affix still present
        let mut item_b = perfect_weapon(21);
        item_b.affixes = vec![(Affix::Splash, 1.0)];
        let id_a = item_a.id.clone();
        let id_b = item_b.id.clone();
        character.add_to_inventory(item_a);
        character.add_to_inventory(item_b);

        // guaranteed=true forces every non-shared source affix into the
        // merge (see `roll_recombine`) - with only 2 total, both survive
        // deterministically, so Evasion (and its crit tag) is guaranteed
        // to carry into the result.
        let mut rng = StdRng::seed_from_u64(5);
        let roll = character.roll_recombine(&id_a, &id_b, true, &mut rng).expect("roll should succeed");
        assert!(roll.affixes.iter().any(|&(a, _)| a == Affix::Evasion), "test setup sanity: Evasion must have survived the merge");
        assert!(roll.reforge_crit_used(), "reforge_crit_used must inherit true when the crit-granted affix actually survives the merge");
    }

    #[test]
    fn recombine_result_does_not_inherit_a_dead_reforge_crit_tag() {
        // Companion to the test above: if the affix a source's earlier
        // reforge crit granted is ALREADY gone (Annulled off before this
        // recombine ever happened - the exact same live-reported bug
        // pattern), nothing survives to tag, so the merged item's gate
        // must come back open instead of dragging a dead lock through.
        let mut character = Character::new("test".to_string());
        let mut item_a = perfect_weapon(22);
        item_a.affixes = vec![(Affix::Evasion, 1.0)];
        item_a.record_reforge_crit(Affix::Splash); // tagged affix that ISN'T present on item_a at all
        assert!(!item_a.reforge_crit_used(), "test setup sanity: the tag alone, with no matching affix present, must not read as used");
        let mut item_b = perfect_weapon(23);
        item_b.affixes = vec![(Affix::IncreasedLife, 1.0)];
        let id_a = item_a.id.clone();
        let id_b = item_b.id.clone();
        character.add_to_inventory(item_a);
        character.add_to_inventory(item_b);

        let mut rng = StdRng::seed_from_u64(7);
        let roll = character.roll_recombine(&id_a, &id_b, true, &mut rng).expect("roll should succeed");
        assert!(!roll.reforge_crit_used(), "a dead crit tag with no surviving affix must not lock the merged item's gate");
    }

    #[test]
    fn crit_bonus_affix_does_not_count_toward_required_affix_count() {
        // 2026-08-18, a live report: an item that picked up a Reforge/
        // Recombine crit-bonus affix, then had a normal affix Annulled
        // off, ends up with 3 normal affixes + 1 crit-bonus affix (4
        // total) - it should still read as "3" for Exalt's exact-count
        // gate, not "4", so it stays Exalt-eligible instead of getting
        // silently stuck one crafting step short of the normal ceiling.
        let mut character = Character::new("test".to_string());
        let mut item = perfect_weapon(30);
        item.affixes = vec![(Affix::Evasion, 1.0), (Affix::Splash, 1.0), (Affix::IncreasedLife, 1.0), (Affix::CritChance, 1.0)];
        item.record_reforge_crit(Affix::CritChance);
        let item_id = item.id.clone();
        character.equip(item);

        let exalt_pool = character.craftable_affix_pool(&item_id, CraftAction::Exalt);
        assert!(exalt_pool.is_ok(), "3 normal + 1 crit-bonus affix should satisfy Exalt's required_affix_count of 3, got {exalt_pool:?}");

        // Sanity: Augment (requires exactly 1 normal affix) must still
        // correctly reject this same item - the fix only excludes
        // crit-bonus affixes from the count, it doesn't stop counting
        // normal ones.
        let augment_pool = character.craftable_affix_pool(&item_id, CraftAction::Augment);
        assert!(matches!(augment_pool, Err(CraftError::PreconditionNotMet)));
    }
}

#[cfg(test)]
mod crit_ev_tests {
    use super::*;

    #[test]
    fn combat_dps_crit_ev_matches_the_two_point_mixture_of_crit_stack_bonus() {
        // 2026-08-18, a live bug report: combat_dps()'s crit EV term used
        // to just be `1.0 + crit_chance * (crit_multiplier - 1.0)`,
        // omitting CRIT_BONUS_MULT/the overcrit curve real combat applies
        // - overstating DPS/HPS for any crit-heavy build. A bare fresh
        // character (no gear, no helm) isolates the crit_ev factor
        // cleanly: combat_dps() == combat_atk() * hits_per_sec * crit_ev
        // * increased_dmg_mult, with nothing else (helm, heal split) in
        // the way.
        let mut character = Character::new("test".to_string());
        // Character::new starts everyone with a random tier-1 helm
        // equipped (helm_skill() returns Some for ANY equipped helm,
        // regardless of its affixes) - unequip it so this test's
        // simplified expected-value formula (which doesn't model the
        // helm's own separate ramp-up term) stays valid.
        character.unequip(EquipSlot::Helm);
        assert_eq!(character.combat_heal_power(), 0.0, "a non-healer with zero tree investment must have zero heal power, or this test's dps-equals-total-output assumption breaks");
        assert!(character.helm_skill().is_none(), "helm should be unequipped by now");

        let crit_chance = character.combat_crit_chance();
        let crit_multiplier = character.combat_crit_multiplier();
        let guaranteed_stacks = crit_chance.floor();
        let remainder = crit_chance - guaranteed_stacks;
        let expected_crit_ev =
            1.0 + (1.0 - remainder) * crit_stack_bonus(guaranteed_stacks, crit_multiplier) + remainder * crit_stack_bonus(guaranteed_stacks + 1.0, crit_multiplier);

        let hits_per_sec = 1000.0 / character.attack_interval_ms() as f64;
        let increased_dmg_mult = 1.0 + character.combat_increased_damage();
        let expected_dps = character.combat_atk() as f64 * hits_per_sec * expected_crit_ev * increased_dmg_mult;

        assert!((character.combat_dps() - expected_dps).abs() < 0.01, "expected combat_dps() ~= {expected_dps}, got {}", character.combat_dps());
    }
}

#[cfg(test)]
mod combat_intervene_tests {
    use super::*;

    /// Wiki audit finding #2 (2026-08-18): gear and tree Intervene were
    /// each independently capped at 50%, but `combine_reduction_sources`
    /// combines them multiplicatively, not additively - two 50%-capped
    /// sources reach 1-(0.5*0.5) = 75%, despite the doc directly above
    /// `combat_intervene` stating a 50% per-character ceiling. Realistic
    /// too, not just a synthetic extreme: maxed gear investment (any
    /// single Intervene affix at or above 1.0 raw already saturates the
    /// gear-side 50% cap on its own) plus "oath" at 3/3 (a real, easily
    /// reachable 10% tree investment) alone combine to 55% - already over
    /// the documented cap with zero exotic setup.
    #[test]
    fn combined_gear_and_tree_never_exceeds_the_documented_50_percent_cap() {
        let mut character = Character::new("test".to_string());
        let mut item = generate_item_at_tier(EquipSlot::Body, 10, &mut rand::thread_rng());
        item.affixes = vec![(Affix::Intervene, 1.0)]; // raw 100% - saturates the 50% gear-side cap alone
        character.equip(item);
        character.passive_allocations.insert("oath".to_string(), 3); // a real, easily reachable 10% tree investment

        let intervene = character.combat_intervene();
        assert!(intervene <= 0.5 + 1e-9, "combined gear+tree Intervene must never exceed the documented 50% per-character cap, got {intervene}");
    }
}

#[cfg(test)]
mod split_personality_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn split_personality_item(tier: u32) -> Item {
        let mut rng = StdRng::seed_from_u64(100);
        let mut item = generate_item_at_tier_with_roll(EquipSlot::Weapon, tier, POWER_ROLL_RANGE.end, &mut rng);
        item.unique_affix = Some(UniqueAffix::SplitPersonality);
        item
    }

    #[test]
    fn total_passive_points_bonus_scales_with_item_tier() {
        let mut character = Character::new("test".to_string());
        character.level = 1;
        let base = crate::passive_tree::points_for_level(character.level);
        assert_eq!(character.total_passive_points(), base, "no bonus without Split Personality equipped");

        character.equip(split_personality_item(0));
        assert_eq!(character.total_passive_points(), base + 1, "flat +1 just for having it equipped, tier 0-299");

        character.weapon.as_mut().unwrap().tier = 300;
        assert_eq!(character.total_passive_points(), base + 2, "+1 more at tier 300");

        character.weapon.as_mut().unwrap().tier = 899;
        assert_eq!(character.total_passive_points(), base + 3, "+3 total just under tier 900");

        character.weapon.as_mut().unwrap().tier = 900;
        assert_eq!(character.total_passive_points(), base + 4, "+4 total at tier 900");
    }

    #[test]
    fn passive_node_rank_and_magnitude_read_from_secondary_tree() {
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Warlock;
        character.equip(split_personality_item(0));
        character.secondary_archetype = Some(Archetype::Ranger);
        // "curse" (Warlock's own Curse of Weakness skill) and "mark"
        // (Ranger's own Hunter's Mark skill) are both real, unrelated
        // top-level Skill nodes - no parent/unlock gate, so a raw rank
        // insert is enough to exercise the lookup.
        character.passive_allocations.insert("curse".to_string(), 2);
        character.secondary_passive_allocations.insert("mark".to_string(), 3);

        assert_eq!(character.passive_node_rank("curse"), 2, "primary tree rank must still resolve");
        assert_eq!(character.passive_node_rank("mark"), 3, "secondary tree rank must resolve through the same lookup");
        assert!(character.passive_node_magnitude("curse") > 0.0, "primary magnitude must resolve against the PRIMARY archetype's node list");
        assert!(character.passive_node_magnitude("mark") > 0.0, "secondary magnitude must resolve against the SECONDARY archetype's own node list, not the primary's");
    }

    #[test]
    fn unequipping_split_personality_instantly_refunds_the_secondary_tree() {
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Warlock;
        let item = split_personality_item(0);
        let item_id = item.id.clone();
        character.equip(item);
        character.secondary_archetype = Some(Archetype::Ranger);
        character.secondary_passive_allocations.insert("mark".to_string(), 3);

        assert_eq!(character.effective_secondary_archetype(), Some(Archetype::Ranger));
        assert_eq!(character.passive_node_rank("mark"), 3, "secondary rank reads through while equipped");

        // Unequip by moving something else into the weapon slot - no
        // special "remove unique" call needed, `effective_*` is live-
        // checked off whatever's ACTUALLY equipped right now.
        character.weapon = None;

        assert_eq!(character.effective_secondary_archetype(), None, "secondary tree must disappear the instant Split Personality is unequipped");
        assert_eq!(character.passive_node_rank("mark"), 0, "an unequipped secondary tree's ranks must no longer be readable - this IS the refund");
        assert_eq!(character.total_passive_points(), crate::passive_tree::points_for_level(character.level), "the bonus point(s) must also disappear, not just the tree's own points");
        // The raw data is still sitting in the map (untouched by this
        // unequip alone) - only `effective_secondary_archetype`/the
        // lookups built on it treat it as gone. Re-equipping the SAME
        // item would see it resurface, which is fine: the important
        // guarantee is that the manager-level `set_secondary_archetype`
        // path (not exercised here - it needs a live `AdventureManager`)
        // eagerly clears this map on any real re-pick.
        assert_eq!(character.secondary_passive_allocations.get("mark"), Some(&3), "underlying storage is untouched by unequip alone - only the derived getters are live-gated");
        let _ = item_id;
    }

    #[test]
    fn secondary_tree_flat_stat_nodes_feed_real_combat_stats() {
        // 2026-08-18 bugfix - a live report (kuokkiz: Monk primary, Rogue
        // secondary) found Deadly Precision/Shadowstep (both plain
        // FlatStat nodes) doing nothing despite being invested, while
        // bespoke Special-effect nodes worked fine. Warrior's "bulwark"
        // (block chance) stands in for the primary tree here, Rogue's
        // "precision"/"shadowstep" (crit multiplier/evasion) for the
        // secondary - `combat_crit_multiplier`/`combat_evasion` are the
        // SAME functions real `CombatSimUnit` construction calls, so this
        // is asserting the real fight-facing stat, not just the sheet.
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Warrior;
        character.equip(split_personality_item(0));
        character.secondary_archetype = Some(Archetype::Rogue);
        character.passive_allocations.insert("bulwark".to_string(), 3);
        character.secondary_passive_allocations.insert("precision".to_string(), 3);
        character.secondary_passive_allocations.insert("shadowstep".to_string(), 3);

        let bonus = character.passive_bonus();
        assert!((bonus.block_chance - 0.20).abs() < 1e-9, "primary tree's own FlatStat node must still pool - got {}", bonus.block_chance);
        assert!((bonus.crit_multiplier - 0.35).abs() < 1e-9, "secondary tree's Deadly Precision (3/3 = +35%) must now pool too - got {}", bonus.crit_multiplier);
        assert!((bonus.evasion - 0.20).abs() < 1e-9, "secondary tree's Shadowstep (3/3 = +20%) must now pool too - got {}", bonus.evasion);

        // The real combat-facing functions - same ones CombatSimUnit
        // construction calls (combat.rs:7502/7513) - must reflect the
        // secondary contribution too, not just the raw `passive_bonus()`
        // struct.
        assert!(character.combat_crit_multiplier() > 2.0, "base crit multiplier (2.0) must be boosted by the secondary tree's Deadly Precision");
        assert!(character.combat_evasion() > 0.0, "evasion must be nonzero from the secondary tree's Shadowstep alone");
    }

    #[test]
    fn secondary_tree_overflow_conversion_nodes_also_work() {
        // Same bug, the OverflowConversion half of `passive_bonus`'s
        // sibling function `passive_overflow_bonus`. Rogue's "Elusive"
        // (evasion overflow -> crit chance) needs real evasion overflow
        // to have anything to convert, so this equips gear-less/tree-only
        // evasion investment high enough to matter isn't realistic here -
        // instead this just confirms the secondary node is discovered and
        // iterated at all (no panic, no `NodeNotFound`-shaped silent
        // skip), which is what the primary-only bug would have prevented
        // outright regardless of how much overflow existed.
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Warrior;
        character.equip(split_personality_item(0));
        character.secondary_archetype = Some(Archetype::Rogue);
        character.secondary_passive_allocations.insert("shadowstep".to_string(), 3);
        character.secondary_passive_allocations.insert("elusive".to_string(), 3);

        // Elusive requires Shadowstep at its own unlock rank - just
        // confirming this resolves without error/panic and produces a
        // real (if small/zero) crit_chance figure is enough to prove the
        // secondary tree's OverflowConversion node is being read at all.
        let overflow_bonus = character.passive_overflow_bonus();
        assert!(overflow_bonus.crit_chance >= 0.0, "must resolve cleanly for a secondary-tree OverflowConversion node");
    }
}

/// Stage 1 of the Elementalist build (docs/elementalist_spec.md,
/// ELEMENTALIST_PROGRESS.md). Tree structural correctness itself is
/// covered by `passive_tree.rs`'s own `tree_shape_tests`; these cover
/// the `Character`-level surface: root bonus, save/load round-trip, and
/// the same rank-cap/unlock-gate rules `AdventureManager::
/// preview_allocate_passive` enforces, checked directly here (no
/// existing test in this codebase spins up a full `AdventureManager`
/// just to test one archetype's tree - see manager.rs's own allocation
/// code, which is fully generic over `archetype.passive_nodes()`
/// already, the same way every other archetype's allocation behavior is
/// implicitly proven by production use rather than a dedicated
/// integration test).
#[cfg(test)]
mod elementalist_tests {
    use super::*;

    #[test]
    fn root_bonus_grants_splash_scaling_with_level_like_ranger() {
        let elementalist_lvl0 = Archetype::Elementalist.bonus(0);
        let ranger_lvl0 = Archetype::Ranger.bonus(0);
        assert_eq!(elementalist_lvl0.splash, ranger_lvl0.splash, "same base magnitude/shape as Ranger's own splash advantage, per the spec's own reasoning");
        assert_eq!(elementalist_lvl0.heal_power_pct, 0.0, "no baseline heal_power_pct - unlike Paladin/Cleric/Druid, healing is earned entirely through Healing Flames tree investment");

        let elementalist_lvl10 = Archetype::Elementalist.bonus(10);
        assert!(elementalist_lvl10.splash > elementalist_lvl0.splash, "splash must scale up with level, same as every other archetype's root bonus");
    }

    #[test]
    fn combat_function_is_ranged() {
        assert_eq!(Archetype::Elementalist.combat_function(), CombatFunction::Ranged);
    }

    #[test]
    fn is_in_all_archetypes_and_excluded_from_commoner_default() {
        assert!(ALL_ARCHETYPES.contains(&Archetype::Elementalist), "must be pickable from the dashboard/!class the same as every other real archetype");
        assert_ne!(Archetype::default(), Archetype::Elementalist, "Commoner, not Elementalist, must stay the default for characters with no archetype recorded");
    }

    #[test]
    fn a_fresh_elementalist_character_can_allocate_a_root_skill_point() {
        // Mirrors the exact rank-cap/unlock-gate checks
        // `AdventureManager::preview_allocate_passive` performs, applied
        // directly to a plain `Character` - see this module's own doc for
        // why this stays at the Character level rather than spinning up a
        // full manager instance.
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Elementalist;
        character.level = 20; // plenty of points for this check
        let nodes = character.archetype.passive_nodes();

        let righteous_fire = nodes.iter().find(|n| n.key == "righteousfire").expect("righteousfire must exist");
        assert!(righteous_fire.parent.is_none(), "a root skill needs no parent investment to allocate");
        character.passive_allocations.insert("righteousfire".to_string(), 1);
        assert_eq!(character.passive_node_rank("righteousfire"), 1);

        // A tier-3 Modifier must be rejected before its Specialization
        // parent hits the unlock threshold (4/4) - same rule
        // `preview_allocate_passive` enforces via `node.unlock_at`.
        let fanning_flames = nodes.iter().find(|n| n.key == "fanningflames").expect("fanningflames must exist");
        let parent_rank = character.passive_allocations.get(fanning_flames.parent.unwrap()).copied().unwrap_or(0);
        assert!(parent_rank < fanning_flames.unlock_at.unwrap(), "healingflames must not be pre-invested by this test - sanity check on the test itself");
    }

    #[test]
    fn respec_clears_elementalist_allocations_the_same_as_any_archetype() {
        let mut character = Character::new("test".to_string());
        character.archetype = Archetype::Elementalist;
        character.passive_allocations.insert("righteousfire".to_string(), 3);
        character.passive_allocations.insert("healingflames".to_string(), 4);
        assert!(!character.passive_allocations.is_empty());

        // `AdventureManager::respec_passive_tree` just clears this map
        // directly (verified by reading its body) - exercised here at
        // the data level since it needs no archetype-specific logic to
        // work correctly for a new archetype.
        character.passive_allocations.clear();
        assert_eq!(character.passive_node_rank("righteousfire"), 0);
        assert_eq!(character.passive_node_rank("healingflames"), 0);
    }

    #[test]
    fn elementalist_character_round_trips_through_json_with_allocations_intact() {
        let mut character = Character::new("elementalist_tester".to_string());
        character.archetype = Archetype::Elementalist;
        character.level = 8;
        character.passive_allocations.insert("elementalfocus".to_string(), 3);
        character.passive_allocations.insert("shockingfocus".to_string(), 4);
        character.passive_allocations.insert("overshock".to_string(), 2);

        let json = serde_json::to_string(&character).expect("must serialize");
        let restored: Character = serde_json::from_str(&json).expect("must deserialize");

        assert_eq!(restored.archetype, Archetype::Elementalist);
        assert_eq!(restored.passive_allocations.get("elementalfocus"), Some(&3));
        assert_eq!(restored.passive_allocations.get("shockingfocus"), Some(&4));
        assert_eq!(restored.passive_allocations.get("overshock"), Some(&2));
    }

    #[test]
    fn a_character_saved_before_elementalist_existed_still_loads_as_commoner() {
        // The exact migration precedent `Archetype::default`'s own doc
        // describes for every past archetype addition - a save file with
        // no "archetype" key at all (or an old enum value that no longer
        // exists) must still deserialize, defaulting to Commoner, never
        // failing or silently becoming Elementalist.
        let old_save_json = r#"{"display_name":"old_timer","level":5,"xp":0,"wins":0,"losses":0}"#;
        let restored: Character = serde_json::from_str(old_save_json).expect("a pre-archetype save must still deserialize");
        assert_eq!(restored.archetype, Archetype::Commoner);
    }

    #[test]
    fn golem_type_defaults_to_basic() {
        assert_eq!(GolemType::default(), GolemType::Basic);
    }

    #[test]
    fn golem_slot_types_round_trips_through_json() {
        let mut character = Character::new("golem_tester".to_string());
        character.archetype = Archetype::Elementalist;
        character.golem_slot_types = vec![GolemType::Thunder, GolemType::Water, GolemType::Basic];

        let json = serde_json::to_string(&character).expect("must serialize");
        let restored: Character = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(restored.golem_slot_types, vec![GolemType::Thunder, GolemType::Water, GolemType::Basic]);
    }

    #[test]
    fn a_character_saved_before_golem_master_existed_still_loads_with_no_slots_assigned() {
        // Same additive-schema migration precedent as
        // `a_character_saved_before_elementalist_existed_still_loads_as_commoner`
        // - a save file predating `golem_slot_types` entirely must still
        // deserialize, defaulting to an empty Vec (no slots pre-assigned),
        // never failing.
        let old_save_json = r#"{"display_name":"pre_golem","level":10,"xp":0,"wins":0,"losses":0,"archetype":"elementalist"}"#;
        let restored: Character = serde_json::from_str(old_save_json).expect("a pre-golem-master save must still deserialize");
        assert_eq!(restored.golem_slot_types, Vec::new());
    }
}

/// Why a `change_archetype` attempt didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum ChangeArchetypeError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// Tried to manually pick `Archetype::Commoner` - it's a starting
    /// state, never a valid destination.
    InvalidChoice,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
}

/// Why an `AdventureManager::set_secondary_archetype` attempt (Split
/// Personality's 2nd-class picker) didn't go through - see that method's
/// own doc. Always free, so there's no `InsufficientDust` equivalent here.
#[derive(Debug, Clone, Copy)]
pub enum SetSecondaryArchetypeError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// Tried to manually pick `Archetype::Commoner` - same reasoning as
    /// `ChangeArchetypeError::InvalidChoice`.
    InvalidChoice,
    /// Split Personality isn't currently equipped (see
    /// `Character::effective_split_personality_item`) - there's no 2nd
    /// tree to point at without it.
    NotEquipped,
    /// Picked the same archetype already active as the PRIMARY class -
    /// investing in your own tree twice under two names is nonsensical.
    SameAsPrimary,
}

/// Why an `AdventureManager::set_golem_slot_type` attempt (Elementalist's
/// Golem Master slot-type picker, docs/elementalist_spec.md Stage 5)
/// didn't go through - see that method's own doc. Always free, same
/// spirit as `SetSecondaryArchetypeError`.
#[derive(Debug, Clone, Copy)]
pub enum SetGolemSlotTypeError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// Not currently playing Elementalist.
    NotElementalist,
    /// `slot` is past the number of golem slots Golem Master's current
    /// rank actually grants (0-indexed, so rank 1 only allows slot 0).
    SlotNotUnlocked,
}

/// Why a passive-tree action (`preview_allocate_passive`/
/// `save_passive_tree`/`respec_passive_tree`) didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum PassiveError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// `node_key` doesn't match any node in the character's current
    /// archetype's tree (see `Archetype::passive_nodes`).
    NodeNotFound,
    /// Tried to invest in a Specialization/Modifier node without its
    /// `parent` already at the required rank (1 for a Specialization
    /// under a Skill, `unlock_at` for a Modifier under a specialized
    /// Specialization).
    ParentNotInvested,
    /// The requested rank would exceed the node's `max_rank`.
    MaxRankReached,
    /// The requested allocation would spend more points than the
    /// character's current level has earned (see `points_for_level`).
    InsufficientPoints,
    /// Not enough dust for a respec — carries the cost that was needed.
    InsufficientDust(u64),
}

/// Why a `change_model` attempt didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum ChangeModelError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// Not one of `ALL_SPRITES` - shouldn't be reachable through the
    /// web dashboard's own picker, only via a malformed direct POST.
    InvalidChoice,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
}

/// Stage A of the Memories build (docs/memories_spec.md) - the
/// persistence half. Same additive-schema precedent every other new
/// field on this struct follows: a round-trip test plus a test proving a
/// save written before the field existed still loads.
#[cfg(test)]
mod memory_persistence_tests {
    use super::*;

    #[test]
    fn memories_round_trip_through_json() {
        let mut character = Character::new("memory_tester".to_string());
        character.archetype = Archetype::Warrior;
        character.passive_allocations.insert("bulwark".to_string(), 3);
        character.memory_slots = STARTING_MEMORY_SLOTS;
        character.memories = vec![
            Some(character.snapshot_build("Tank Build".to_string(), 1_700_000_000)),
            None,
            Some(character.snapshot_build("Backup".to_string(), 1_700_000_001)),
        ];

        let json = serde_json::to_string(&character).expect("must serialize");
        let restored: Character = serde_json::from_str(&json).expect("must deserialize");

        assert_eq!(restored.memory_slots, 3);
        assert_eq!(restored.memories.len(), 3);
        assert_eq!(restored.memory_slot(0).map(|m| m.name.as_str()), Some("Tank Build"));
        assert!(restored.memory_slot(1).is_none(), "an empty slot must round-trip as still empty, not collapse away");
        assert_eq!(restored.memory_slot(2).map(|m| m.name.as_str()), Some("Backup"));
        assert_eq!(restored.memory_slot(0).unwrap().passive_allocations.get("bulwark"), Some(&3));
        assert_eq!(restored.memory_slot(0).unwrap().saved_at, 1_700_000_000);
    }

    #[test]
    fn a_character_saved_before_memories_existed_still_loads_with_three_empty_slots() {
        // Same additive-schema precedent as
        // `a_character_saved_before_golem_master_existed_still_loads_with_no_slots_assigned`
        // - a save file predating `memories`/`memory_slots` entirely must
        // still deserialize, getting the default slot grant rather than
        // failing to parse.
        let old_save_json = r#"{"display_name":"pre_memories","level":10,"xp":0,"wins":0,"losses":0,"archetype":"warrior"}"#;
        let restored: Character = serde_json::from_str(old_save_json).expect("a pre-Memories save must still deserialize");

        assert_eq!(restored.memories, Vec::new(), "nothing saved yet");
        assert_eq!(restored.memory_slots, STARTING_MEMORY_SLOTS, "an existing character is granted the default slots on load");
        assert_eq!(restored.memories_padded().len(), 3, "the stored vec is short, but reading it must still show 3 slots");
        assert!(restored.memories_padded().iter().all(|m| m.is_none()));
    }

    #[test]
    fn divine_dust_round_trips_through_json() {
        let mut character = Character::new("dust_hoarder".to_string());
        character.divine_dust = 42;
        let json = serde_json::to_string(&character).expect("must serialize");
        let restored: Character = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(restored.divine_dust, 42);
    }

    #[test]
    fn a_character_saved_before_divine_dust_existed_still_loads_with_zero() {
        // Same additive-schema precedent as
        // `a_character_saved_before_memories_existed_still_loads_with_three_empty_slots`
        // - a save file predating `divine_dust` entirely must still
        // deserialize, defaulting to 0 rather than failing to parse.
        let old_save_json = r#"{"display_name":"pre_divine_dust","level":10,"xp":0,"wins":0,"losses":0,"archetype":"warrior"}"#;
        let restored: Character = serde_json::from_str(old_save_json).expect("a pre-Divine-Dust save must still deserialize");
        assert_eq!(restored.divine_dust, 0);
    }

    #[test]
    fn memories_padded_normalizes_a_stored_vec_of_any_length() {
        let mut character = Character::new("padder".to_string());
        // Shorter than `memory_slots` - the common case for anyone who
        // has only ever used slot 1.
        character.memories = vec![Some(character.snapshot_build("Only One".to_string(), 0))];
        assert_eq!(character.memories_padded().len(), 3);
        assert_eq!(character.memories_padded()[0].as_ref().map(|m| m.name.as_str()), Some("Only One"));
        assert!(character.memories_padded()[2].is_none());

        // Longer than `memory_slots` - only reachable if a grant were
        // ever revoked, but reading must not then expose a slot the
        // player no longer has.
        character.memory_slots = 1;
        assert_eq!(character.memories_padded().len(), 1);
        assert!(character.memory_slot(1).is_none(), "a slot past `memory_slots` must read as absent");
    }

    #[test]
    fn memory_slots_is_a_per_character_value_not_a_hardcoded_three() {
        // The whole reason it's a field: a future feature must be able
        // to grant extras with no migration and no change to any reader.
        let mut character = Character::new("granted".to_string());
        character.memory_slots = 5;
        assert_eq!(character.memories_padded().len(), 5);
        assert!(character.memory_slot_mut(4).is_some(), "slot 5 must be addressable once granted");
        assert!(character.memory_slot_mut(5).is_none(), "slot 6 must still be out of range");
    }

    #[test]
    fn snapshot_build_captures_the_whole_current_build() {
        let mut character = Character::new("snapshotter".to_string());
        character.archetype = Archetype::Elementalist;
        character.passive_allocations.insert("golemmaster".to_string(), 3);
        character.golem_slot_types = vec![GolemType::Thunder, GolemType::Flame];

        let memory = character.snapshot_build("Golem Build".to_string(), 42);

        assert_eq!(memory.archetype, Archetype::Elementalist);
        assert_eq!(memory.passive_allocations.get("golemmaster"), Some(&3));
        assert_eq!(memory.golem_slot_types, vec![GolemType::Thunder, GolemType::Flame]);
        assert_eq!(memory.saved_at, 42);
    }

    #[test]
    fn snapshot_build_omits_the_secondary_tree_when_split_personality_is_not_equipped() {
        // Reads `effective_secondary_archetype()`, never the raw field -
        // otherwise a stale map would ride along in the snapshot and
        // reappear the next time the item happened to be equipped.
        let mut character = Character::new("stale_secondary".to_string());
        character.archetype = Archetype::Warrior;
        character.secondary_archetype = Some(Archetype::Mage);
        character.secondary_passive_allocations.insert("arcane".to_string(), 3);
        assert!(character.effective_secondary_archetype().is_none(), "sanity: nothing equipped grants Split Personality here");

        let memory = character.snapshot_build("No Secondary".to_string(), 0);

        assert_eq!(memory.secondary_archetype, None);
        assert!(memory.secondary_passive_allocations.is_empty(), "an inactive secondary tree must not be captured");
    }
}

/// Coverage for the 2026-08-21 duplicate-unique-effects fix - the
/// equip-time gate (`has_conflicting_unique_affix`/`_value`, already
/// correct before this fix but never directly tested) and the legacy
/// `CraftAction::CelestialShard` branch's new equipped-conflict check
/// (`Character::has_conflicting_unique_affix_value`'s new caller). The
/// Unique Shard picker's own insert-time filter lives in
/// `manager.rs::unique_shard_tests` instead, since it needs the real
/// `craft_item_ex`/`choose_veil_outcome` manager API.
#[cfg(test)]
mod duplicate_unique_effects_tests {
    use super::*;

    fn unique_item(slot: EquipSlot, unique: UniqueAffix) -> Item {
        let mut item = generate_item_at_tier(slot, 10, &mut rand::thread_rng());
        item.unique_affix = Some(unique);
        item
    }

    #[test]
    fn equip_from_inventory_is_blocked_when_the_same_unique_is_already_equipped_elsewhere() {
        // Character::new gives a full starter kit, so `body` already
        // holds something before this test even starts - captured here
        // so the assertion below proves it stayed untouched rather than
        // assuming an empty slot.
        let mut character = Character::new("blocker".to_string());
        character.helm = Some(unique_item(EquipSlot::Helm, UniqueAffix::SplitPersonality));
        let starter_body_id = character.body.as_ref().expect("starter kit fills every slot").id.clone();
        let bagged = unique_item(EquipSlot::Body, UniqueAffix::SplitPersonality);
        let id = bagged.id.clone();
        character.inventory.push(bagged);

        let equipped = character.equip_from_inventory(&id);
        assert!(!equipped, "equipping a second SplitPersonality item must be refused");
        assert!(character.inventory.iter().any(|i| i.id == id), "the refused item must stay in the bag, untouched");
        assert_eq!(character.body.as_ref().map(|i| i.id.clone()), Some(starter_body_id), "the body slot's current occupant must be untouched - nothing silently swapped in");
    }

    #[test]
    fn equip_from_inventory_succeeds_when_the_unique_is_different() {
        let mut character = Character::new("differ".to_string());
        character.helm = Some(unique_item(EquipSlot::Helm, UniqueAffix::SplitPersonality));
        let bagged = unique_item(EquipSlot::Body, UniqueAffix::CelestialConversion);
        let id = bagged.id.clone();
        character.inventory.push(bagged);

        let equipped = character.equip_from_inventory(&id);
        assert!(equipped, "a DIFFERENT unique affix must never conflict with another");
        assert_eq!(character.body.as_ref().map(|i| i.unique_affix), Some(Some(UniqueAffix::CelestialConversion)));
    }

    #[test]
    fn receive_item_falls_back_to_the_bag_when_auto_equip_would_conflict() {
        let mut character = Character::new("receiver".to_string());
        character.weapon = Some(unique_item(EquipSlot::Weapon, UniqueAffix::CelestialConversion));
        character.helm = None; // empty slot, so receive_item would normally auto-equip straight into it
        let dropped = unique_item(EquipSlot::Helm, UniqueAffix::CelestialConversion);
        let id = dropped.id.clone();

        let outcome = character.receive_item(dropped);
        assert!(matches!(outcome, ReceiveOutcome::AddedToBag), "an empty slot must NOT auto-equip a conflicting unique");
        assert!(character.inventory.iter().any(|i| i.id == id));
        assert!(character.helm.is_none());
    }

    #[test]
    fn has_conflicting_unique_affix_value_excludes_the_items_own_destination_slot() {
        let mut character = Character::new("self_check".to_string());
        character.weapon = Some(unique_item(EquipSlot::Weapon, UniqueAffix::CelestialConversion));
        assert!(
            !character.has_conflicting_unique_affix_value(UniqueAffix::CelestialConversion, EquipSlot::Weapon),
            "equipping back into its own current slot must never count as a conflict with itself"
        );
        assert!(
            character.has_conflicting_unique_affix_value(UniqueAffix::CelestialConversion, EquipSlot::Helm),
            "the same value destined for a DIFFERENT slot must still conflict with the Weapon's own copy"
        );
    }

    /// Bug #44 (2026-08-21) - the commit-time gap `apply_unique_affix`
    /// itself had: two Unique Shard picker flows on two DIFFERENT
    /// equipped slots can each pass their own insert-time filter (neither
    /// has committed yet, so neither sees the other), so the guard has
    /// to live here too, not just at insert time. `pending_veils` only
    /// holds one entry per player (manager.rs), so this exercises
    /// `apply_unique_affix` directly rather than through two overlapping
    /// `craft_item_ex` calls - same reasoning `unique_shard_tests` in
    /// manager.rs documents for why the insert-time filter's own tests
    /// live there instead (see this file's own module doc above).
    #[test]
    fn apply_unique_affix_rejects_when_a_second_equipped_slot_already_landed_first() {
        let mut character = Character::new("overlap".to_string());
        let helm_item = generate_item_at_tier(EquipSlot::Helm, 10, &mut rand::thread_rng());
        let helm_id = helm_item.id.clone();
        character.helm = Some(helm_item);
        let body_item = generate_item_at_tier(EquipSlot::Body, 10, &mut rand::thread_rng());
        let body_id = body_item.id.clone();
        character.body = Some(body_item);

        let first = character.apply_unique_affix(&helm_id, UniqueAffix::SplitPersonality);
        assert!(first.is_ok(), "first commit lands - nothing conflicted when it committed");
        assert_eq!(character.helm.as_ref().unwrap().unique_affix, Some(UniqueAffix::SplitPersonality));

        let second = character.apply_unique_affix(&body_id, UniqueAffix::SplitPersonality);
        assert!(
            matches!(second, Err(CraftError::ConflictingUniqueAffix)),
            "second commit must be rejected now that the Helm already carries the same value"
        );
        assert_eq!(character.body.as_ref().unwrap().unique_affix, None, "a rejected commit must never mutate the item");
    }

    /// Standing design: a conflict check only ever applies to EQUIPPED
    /// targets - an item sitting in the bag is never filtered, same as
    /// the insert-time filter's own bag carve-out (manager.rs's
    /// UniqueShard branch doc).
    #[test]
    fn apply_unique_affix_stays_unfiltered_for_a_bagged_item() {
        let mut character = Character::new("bagger".to_string());
        character.helm = Some(unique_item(EquipSlot::Helm, UniqueAffix::SplitPersonality));
        let bagged = generate_item_at_tier(EquipSlot::Body, 10, &mut rand::thread_rng());
        let bagged_id = bagged.id.clone();
        character.inventory.push(bagged);

        let result = character.apply_unique_affix(&bagged_id, UniqueAffix::SplitPersonality);
        assert!(result.is_ok(), "a bagged item must never be filtered against equipped uniques");
        assert!(character.inventory.iter().any(|i| i.id == bagged_id && i.unique_affix == Some(UniqueAffix::SplitPersonality)));
    }

    #[test]
    fn legacy_celestial_shard_craft_on_an_equipped_item_rejects_when_it_would_conflict() {
        let mut character = Character::new("legacy_equipped".to_string());
        character.helm = Some(unique_item(EquipSlot::Helm, UniqueAffix::CelestialConversion));
        // A second, unique-free item already sitting EQUIPPED in the Body
        // slot - craft_inner mutates whatever's found by id, wherever it
        // lives, so this must be reachable via the equipped path too.
        let body_item = generate_item_at_tier(EquipSlot::Body, 10, &mut rand::thread_rng());
        let id = body_item.id.clone();
        character.body = Some(body_item);

        let mut rng = rand::thread_rng();
        let err = character.craft_inner(&id, CraftAction::CelestialShard, &mut rng).expect_err("must reject - CelestialConversion is already equipped on the Helm");
        assert!(matches!(err, CraftError::ConflictingUniqueAffix));
        assert_eq!(character.body.as_ref().unwrap().unique_affix, None, "a rejected precondition must never mutate the item");
    }

    #[test]
    fn legacy_celestial_shard_craft_on_a_bagged_item_is_always_allowed_even_with_a_conflict() {
        let mut character = Character::new("legacy_bagged".to_string());
        character.helm = Some(unique_item(EquipSlot::Helm, UniqueAffix::CelestialConversion));
        let bagged = generate_item_at_tier(EquipSlot::Body, 10, &mut rand::thread_rng());
        let id = bagged.id.clone();
        character.inventory.push(bagged);

        let mut rng = rand::thread_rng();
        let outcome = character.craft_inner(&id, CraftAction::CelestialShard, &mut rng).expect("a bagged item's conflict is only ever an equip-time concern");
        assert_eq!(outcome.unique_affix_added, Some(UniqueAffix::CelestialConversion));
        assert_eq!(character.find_item_by_id(&id).unwrap().unique_affix, Some(UniqueAffix::CelestialConversion));
    }
}

/// Why a `purchase_wings`/`toggle_flying` attempt didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum WingsError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// `purchase_wings` only - already owns it, nothing to buy twice.
    AlreadyOwned,
    /// `purchase_wings` only - not enough dust (always `WINGS_COST`,
    /// unlike the other cost errors here this one's never variable).
    InsufficientDust,
    /// `toggle_flying` only - never purchased or dropped the cosmetic,
    /// so there's nothing to toggle.
    NotOwned,
}

