use super::*;

/// A secondary roll an item can carry on top of its slot's primary stat
/// (see `Item::affixes`/`roll_affixes`) - defensive stats mitigate
/// incoming damage, offensive stats boost damage dealt. All damage in
/// `simulate_battle` flows through `resolve_hit`, which is where every
/// one of these actually gets applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Affix {
    /// Flat % reduction applied to incoming damage, after block.
    DamageReduction,
    /// % chance an incoming hit is blocked, halving its damage.
    BlockChance,
    /// % chance an incoming hit is avoided entirely (0 damage).
    Evasion,
    /// % more damage dealt, multiplicative, stacks additively across items.
    IncreasedDamage,
    /// % chance a hit is a critical strike.
    CritChance,
    /// Bonus added to the baseline 2.0x critical strike multiplier.
    CritMultiplier,
    /// Fraction of a hit's damage also dealt to other alive enemies (see
    /// `PLAYER_SPLASH_MAX_TARGETS`/`ENEMY_SPLASH_MAX_TARGETS`) - or, for
    /// a Heal-function unit, the same fraction of a heal also applied to
    /// other injured allies (see `HEAL_SPLASH_MAX_TARGETS`). One stat,
    /// two contexts depending on whether the unit is attacking or
    /// healing that turn.
    Splash,
    /// Applies an unavoidable damage-over-time debuff to whoever this
    /// character hits (see `apply_lingering_effect`'s doc) - reworked
    /// 2026-08-15 from the old `HealingPower` affix (which fed
    /// `Character::combat_heal_power` directly). Existing items' stored
    /// values were divided by 100 in the same migration that renamed this
    /// variant (500% Healing Power -> 5% Lingering Effect) - gear no
    /// longer contributes to heal-power conversion at all now; Cleric/
    /// Druid's archetype baseline and passive-tree investment both got
    /// doubled to compensate (see `Archetype::bonus`'s `heal_power_pct`
    /// and the passive tree's own heal-power nodes).
    LingeringEffect,
    /// % of damage dealt to OTHER alive party members this character
    /// instead takes onto themselves (see `Character::combat_intervene`/
    /// `simulate_battle`'s boss-attack handling) - a "protector" stat.
    /// Rolls low per tier (see `affix_base_value`) and hard-caps at 50%
    /// total (gear + the Paladin archetype's own bonus combined) -
    /// reaching the cap takes real investment across several
    /// tiers/items, not a cheap one-item swing.
    Intervene,
    /// Fraction of a hit's actual damage healed back to the wielder - the
    /// same mechanic as `Archetype::Slayer`'s advantage (see
    /// `Character::combat_life_leech`, which sums both sources), just
    /// obtainable on gear too now. Deliberately 10x rarer than every
    /// other affix to actually roll (see `affix_weight`) - this is meant
    /// to feel like a rare, exciting find, not a normal-odds stat.
    Leech,
    /// % more max hp, multiplicative - stacks additively with the
    /// archetype's own `max_hp_pct` bonus (see `Character::combat_max_hp`),
    /// same "sum every source into one fraction, apply once" pattern as
    /// `IncreasedDamage`.
    IncreasedLife,
    /// Flat +hp, added to the base pool BEFORE `IncreasedLife`/the
    /// archetype's max_hp_pct multiply it - see `Character::combat_max_hp`.
    FlatLife,
    /// One of 5 elemental damage types (Cold/Fire/Lightning/Divine/Chaos -
    /// see the other 4 below). Read as a PROC CHANCE, not a flat damage
    /// bonus (2026-08-15 rework - replaced the type's original "just
    /// mechanically identical to IncreasedDamage" placeholder behavior
    /// entirely, not an addition on top of it): on a landed DAMAGE hit,
    /// this % is the chance to apply a 4s debuff to the target -
    /// Cold reduces their evasion, Fire their damage reduction, Chaos
    /// their block chance (each by 1% per rank/roll-magnitude, stacking
    /// as fully independent per-proc instances - see
    /// `CombatSimUnit::fire_dr_debuff`'s doc - floored so a target's
    /// affected stat can never drop below 25%). On a landed HEAL instead,
    /// the SAME roll instead buffs whoever was healed by the same amount
    /// (additive, not run through `combine_reduction_sources`, capped at
    /// the normal 75% ceiling) - "a healer with one of these modifiers"
    /// per the live request, i.e. whichever action type this swing
    /// actually was, not an archetype gate. `LightningDamage`/
    /// `DivineDamage` below don't touch a defense stat at all - see their
    /// own docs. Only rollable on Weapon/Helm (see `is_eligible_for_slot`) -
    /// the two slots with an attacking implicit, representing that
    /// implicit being retyped from plain physical to this element instead
    /// of literally replacing it with a separate mechanic.
    ColdDamage,
    /// Same shape as `ColdDamage` (see its own doc) - debuffs the
    /// target's damage reduction on a hit, buffs the healed ally's
    /// damage reduction on a heal.
    FireDamage,
    /// A hit has this % chance to stack +1% "increased damage taken" on
    /// the target for 4s, fully independent per-proc stacking (same
    /// primitive `ColdDamage`'s trio uses), capped at 200 stacks (200%)
    /// - see `CombatSimUnit::lightning_dmg_taken`'s doc. No heal-side
    /// variant - this one only ever debuffs, healer or not.
    LightningDamage,
    /// A hit has this % chance to stack +1% "healing received reduced"
    /// on the target for 4s (same independent-stacking primitive,
    /// capped at 100 stacks/100% - healing can't go negative). A heal
    /// instead stacks +1% "healing done" on the HEALER THEMSELVES (not
    /// the healed ally, unlike Cold/Fire/Chaos's heal-side buff) for 4s,
    /// capped at 200 stacks/200% - see `CombatSimUnit::divine_heal_reduction`/
    /// `divine_heal_power_buff`'s docs.
    DivineDamage,
    /// Same shape as `ColdDamage` (see its own doc) - debuffs the
    /// target's block chance on a hit, buffs the healed ally's block
    /// chance on a heal.
    ChaosDamage,
}

impl Affix {
    /// Whether `self` can appear on an item in `slot` at all - every
    /// affix except the 5 damage types is slot-agnostic. See
    /// `ColdDamage`'s doc for why those 5 are Weapon/Helm only.
    pub(crate) fn is_eligible_for_slot(self, slot: EquipSlot) -> bool {
        affix_def(self).eligible_slots.map_or(true, |slots| slots.contains(&slot))
    }
}

impl Default for Affix {
    /// Only used as serde's `default` for the (empty-in-practice) case a
    /// `(Affix, f64)` pair gets deserialized without its own data - real
    /// backward compat for pre-affix items is `affixes: Vec::new()`
    /// (see `Item::affixes`), not this.
    fn default() -> Self {
        Affix::IncreasedDamage
    }
}

/// Every affix type - what `roll_affixes` picks distinct entries from,
/// and what a reforge/recombine "crit" (see `GearCritEvent`) picks a
/// brand-new one from.
pub(crate) const ALL_AFFIXES: [Affix; 17] = [
    Affix::DamageReduction,
    Affix::BlockChance,
    Affix::Evasion,
    Affix::IncreasedDamage,
    Affix::CritChance,
    Affix::CritMultiplier,
    Affix::Splash,
    Affix::LingeringEffect,
    Affix::Intervene,
    Affix::Leech,
    Affix::IncreasedLife,
    Affix::FlatLife,
    Affix::ColdDamage,
    Affix::FireDamage,
    Affix::LightningDamage,
    Affix::DivineDamage,
    Affix::ChaosDamage,
];

/// Everything about one `Affix` variant - display text, slot eligibility,
/// and its default (pre-override) coefficients - collapsed into one match
/// arm per variant instead of the 5 separate matches this replaces
/// (`is_eligible_for_slot`/`affix_name`/`affix_display`/`affix_base_value`/
/// `affix_weight`). `default_per_tier`/`default_weight` are the fallback of
/// record: `adventure-item-balance.toml` can sparsely override either
/// (see `affix_balance`), but a key absent from that file always means
/// "use what's written here." Adding a new affix touches exactly this one
/// arm; tuning an existing one's numbers touches only the TOML file.
pub(crate) struct AffixDef {
    /// Bare noun for chat announcements - was `affix_name`.
    name: &'static str,
    /// Suffix text after the formatted number - was `affix_display`'s
    /// per-arm tail.
    label: &'static str,
    /// 0 for the normal 1-decimal-of-a-percent affixes, 2 for the
    /// sub-1%-per-tier ones (Leech, LingeringEffect, the 5 elementals).
    decimals: usize,
    /// false only for FlatLife (raw hp number, no `%`/no `*100`).
    is_percent: bool,
    /// `None` = every slot; `Some(&[...])` = only these - was
    /// `is_eligible_for_slot`. Only the 5 elemental damage types
    /// restrict this today (Weapon/Helm only - see `Affix::ColdDamage`'s
    /// doc for why).
    eligible_slots: Option<&'static [EquipSlot]>,
    /// Per-tier coefficient - was `affix_base_value`'s per-arm value.
    default_per_tier: f64,
    /// Relative roll weight - was `affix_weight` (1.0 for everything but
    /// Leech, deliberately 10x rarer).
    default_weight: f64,
}

pub(crate) fn affix_def(affix: Affix) -> AffixDef {
    use Affix::*;
    const ELEMENTAL_SLOTS: &[EquipSlot] = &[EquipSlot::Weapon, EquipSlot::Helm];
    match affix {
        DamageReduction => AffixDef { name: "damage taken reduction", label: "dmg taken reduction", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.02, default_weight: 1.0 },
        BlockChance => AffixDef { name: "block chance", label: "block chance", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.02, default_weight: 1.0 },
        Evasion => AffixDef { name: "evasion", label: "evasion", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.016, default_weight: 1.0 },
        IncreasedDamage => AffixDef { name: "damage dealt", label: "dmg dealt", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.03, default_weight: 1.0 },
        // Both crit affixes cut 50% from their original 0.02/0.10 per
        // tier - per a live request to cut back crit/crit-damage rolls
        // on gear (and separately, class advantages - see
        // Archetype::bonus's Rogue/Mage cases) across the board.
        CritChance => AffixDef { name: "crit chance", label: "crit chance", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.01, default_weight: 1.0 },
        // A bonus ADDED to Character::combat_crit_multiplier's 2.0
        // baseline, not the multiplier itself.
        CritMultiplier => AffixDef { name: "crit damage dealt", label: "crit dmg dealt", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.05, default_weight: 1.0 },
        Splash => AffixDef { name: "splash", label: "splash", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.03, default_weight: 1.0 },
        // 100x smaller than the old HealingPower formula it replaced
        // (0.025 * t) - matches the 2026-08-15 conversion factor applied
        // to every existing item's stored value, so a freshly-rolled item
        // lands in the same range a converted legacy item would.
        LingeringEffect => AffixDef { name: "lingering effect", label: "lingering effect", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.00025, default_weight: 1.0 },
        // Deliberately the smallest per-tier value here - Intervene
        // caps at 50% (see Character::combat_intervene) and is meant to
        // take real investment across several tiers/items to approach,
        // not one lucky roll.
        Intervene => AffixDef { name: "intervene", label: "intervene", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.01, default_weight: 1.0 },
        // Deliberately the smallest per-tier value in the whole table -
        // "starting at 0.1% before scaling" per the live request, and
        // 10x rarer to even roll at all in the first place - meant to
        // feel like a genuine rare find, not a normal-strength stat.
        Leech => AffixDef { name: "life leech", label: "life leech", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.001, default_weight: 0.1 },
        IncreasedLife => AffixDef { name: "max hp", label: "max hp", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.03, default_weight: 1.0 },
        // Flat hp, not a percentage - scaled roughly like a slot's own
        // primary stat (see base_power_for_slot's Body entry, 12.0/tier)
        // rather than the tiny per-tier coefficients above, since this
        // has to read as a meaningful raw hp number, not a fraction.
        FlatLife => AffixDef { name: "max hp", label: "max hp", decimals: 0, is_percent: false, eligible_slots: None, default_per_tier: 5.0, default_weight: 1.0 },
        // The 5 damage types (2026-08-15 rework - see Affix::ColdDamage's
        // doc) dropped 100x from their old flat-IncreasedDamage-equivalent
        // coefficient (0.03*t) to make room for their new bespoke on-hit/
        // on-heal proc mechanics, which originally read this same rolled
        // % directly as a PROC CHANCE - a same-day follow-up brought the
        // roll back up 75x (0.0003*t -> 0.0225*t, i.e. 75% of the
        // original pre-rework value) once the proc-chance formula itself
        // changed to divide the roll by 50 instead of reading it directly
        // (see `roll_elemental_proc`'s call sites) AND the roll went back
        // to ALSO contributing flat increased damage again (see
        // `Character::combat_increased_damage`) - both changes together
        // meant the tiny 0.0003x scale was leaving proc chances (and the
        // restored flat damage) far too small to matter.
        ColdDamage => AffixDef { name: "cold damage dealt", label: "cold damage (evasion debuff chance)", decimals: 2, is_percent: true, eligible_slots: Some(ELEMENTAL_SLOTS), default_per_tier: 0.0225, default_weight: 1.0 },
        FireDamage => AffixDef { name: "fire damage dealt", label: "fire damage (dmg reduction debuff chance)", decimals: 2, is_percent: true, eligible_slots: Some(ELEMENTAL_SLOTS), default_per_tier: 0.0225, default_weight: 1.0 },
        LightningDamage => AffixDef { name: "lightning damage dealt", label: "lightning damage (dmg taken debuff chance)", decimals: 2, is_percent: true, eligible_slots: Some(ELEMENTAL_SLOTS), default_per_tier: 0.0225, default_weight: 1.0 },
        DivineDamage => AffixDef { name: "divine damage dealt", label: "divine damage (heal debuff/buff chance)", decimals: 2, is_percent: true, eligible_slots: Some(ELEMENTAL_SLOTS), default_per_tier: 0.0225, default_weight: 1.0 },
        ChaosDamage => AffixDef { name: "chaos damage dealt", label: "chaos damage (block debuff chance)", decimals: 2, is_percent: true, eligible_slots: Some(ELEMENTAL_SLOTS), default_per_tier: 0.0225, default_weight: 1.0 },
    }
}

/// Bare noun for one affix, no value - used in chat announcements (see
/// `GearCritEvent`) where a specific number doesn't fit the sentence.
pub fn affix_name(affix: Affix) -> &'static str {
    affix_def(affix).name
}

/// Human-readable "+X% <name>" for one rolled affix and its (already
/// wear-decayed, if called via `effective_affix_total`) value - shared
/// by `Item::affix_label` and anywhere a raw, non-decayed value needs
/// the same wording (e.g. a character's aggregate `combat_*` totals on
/// the web dashboard).
pub fn affix_display(affix: Affix, value: f64) -> String {
    let d = affix_def(affix);
    if d.is_percent {
        format!("+{:.*}% {}", d.decimals, value * 100.0, d.label)
    } else {
        format!("+{:.*} {}", d.decimals, value, d.label)
    }
}

/// Resolved (per_tier, weight) for every `Affix`, computed once and
/// cached - `AffixDef`'s code defaults with `adventure-item-balance.toml`'s
/// overrides (if any) applied on top. Lazy rather than loaded eagerly by
/// `AdventureManager` because `affix_base_value`/`affix_weight` are free
/// functions called from deep inside `Character`/combat code with no
/// handle to the manager.
pub(crate) static AFFIX_BALANCE: std::sync::OnceLock<HashMap<Affix, (f64, f64)>> = std::sync::OnceLock::new();

pub(crate) fn affix_balance(affix: Affix) -> (f64, f64) {
    AFFIX_BALANCE.get_or_init(|| {
        let raw = load_item_balance_file().affixes;
        let mut resolved: HashMap<Affix, (f64, f64)> = ALL_AFFIXES
            .iter()
            .map(|&a| {
                let d = affix_def(a);
                (a, (d.default_per_tier, d.default_weight))
            })
            .collect();
        for (key, ov) in raw {
            match Affix::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&key)) {
                Ok(affix) => {
                    // An uncommented `[affixes.x]` header with every field
                    // still commented out deserializes to a real (but
                    // empty) entry here - both fields None, matching
                    // AffixOverride::default(). Only log/apply when a
                    // field is ACTUALLY Some(_), so a section header alone
                    // doesn't produce a misleading "overridden" log line
                    // for every affix at every startup.
                    let entry = resolved.get_mut(&affix).expect("ALL_AFFIXES covers every Affix variant");
                    if let Some(v) = ov.per_tier {
                        entry.0 = v;
                    }
                    if let Some(v) = ov.weight {
                        entry.1 = v;
                    }
                    if ov.per_tier.is_some() || ov.weight.is_some() {
                        tracing::info!("{ITEM_BALANCE_PATH}: {key} overridden to per_tier={:?} weight={:?}", ov.per_tier, ov.weight);
                    }
                }
                Err(_) => tracing::warn!("{ITEM_BALANCE_PATH}: unknown affix key '{key}', ignoring"),
            }
        }
        resolved
    })[&affix]
}

/// Per-tier magnitude for one rolled `Affix`, before its own jitter (see
/// `roll_affixes`) - reads `AffixDef`'s code default, sparsely overridable
/// via `adventure-item-balance.toml` (see `affix_balance`).
///
/// KNOWN SIDE EFFECT: `affix_quality_percent`/`craft_affix_value_range`
/// recompute a "how good was this roll" percentage live from
/// `stored_value / affix_base_value(tier)` rather than storing it - so
/// overriding an affix's per_tier here will retroactively change the
/// DISPLAYED quality% of every existing item carrying that affix the
/// moment the bot restarts. This is cosmetic only: actual combat math
/// reads an item's stored `value` directly and is unaffected.
pub(crate) fn affix_base_value(affix: Affix, tier: u32) -> f64 {
    affix_balance(affix).0 * tier as f64
}

/// The (min, max) `Character::roll_craft_affix_value` could have landed
/// at this tier/Perfect-state - same 0.85..1.15 jitter band and
/// `PERFECT_QUALITY_MULT`, just evaluated at both ends instead of one
/// random draw. Web-dashboard-only (2026-08-15, a live request: "show
/// the range on the mod added when finishing a craft") - lets the
/// crafting popup show how good a roll actually was, not just the
/// number itself.
pub fn craft_affix_value_range(tier: u32, affix: Affix, perfect: bool) -> (f64, f64) {
    let mult = if perfect { PERFECT_QUALITY_MULT } else { 1.0 };
    let base = affix_base_value(affix, tier);
    (base * 0.85 * mult, base * 1.15 * mult)
}

/// Where THIS specific rolled affix's implicit jitter (0.85-1.15, the
/// same range every affix roll ever draws from - see `roll_affixes`/
/// `roll_craft_affix_value`/every bonus-affix crit roll) landed, as a
/// 0-100% - same idea as `Item::quality_percent`, just per-modifier
/// instead of per-item. Computed on the fly from the already-stored
/// `value` rather than tracked as its own field - jitter is never
/// re-rolled after the fact (reforge/recombine/Krangle's tier growth all
/// rescale an existing value by the tier ratio instead of drawing a
/// fresh one - see `Item::sync_tier_to`), so `value / affix_base_value(tier)`
/// recovers the original jitter exactly. This is also why "retroactively
/// add this to all existing items" needed no migration at all - it's
/// derived live from data every item already has.
/// `perfect` MUST be `Item::perfect` - a Perfect Quality item's stored
/// `value` has `PERFECT_QUALITY_MULT` baked on top of its jitter (see
/// `make_item_perfect`), so `value / base` alone always lands past the
/// 1.15 ceiling and every Perfect item's affixes clamped to a flat 100%
/// regardless of their real roll - divide the multiplier back out first
/// so a deliberately-varied Perfect item's rolls actually show as varied.
pub fn affix_quality_percent(affix: Affix, value: f64, tier: u32, perfect: bool) -> f64 {
    let base = affix_base_value(affix, tier.max(1));
    if base <= 0.0 {
        return 100.0;
    }
    let value = if perfect { value / PERFECT_QUALITY_MULT } else { value };
    let jitter = (value / base).clamp(0.85, 1.15);
    ((jitter - 0.85) / 0.30 * 100.0).clamp(0.0, 100.0)
}

/// Rolls a freshly-DROPPED item's secondary affixes (see `Affix`) - on
/// top of its slot's unchanged primary stat above. Usually just one
/// (84% of the time); a rare banded roll (10% chance of two, 5% of
/// three, 1% of four - checked as one draw against cumulative
/// thresholds, not three independent rolls) adds more, always distinct
/// types so one item never rolls the same affix twice. Each affix's
/// value gets its own Â±15% jitter, same spirit as the primary `power`
/// roll's `jitter` above. NOT used for a reforge (see
/// `reforge_equipped_item`, which rolls its own much-rarer single-affix
/// chance) or a recombine (see `Character::recombine`, which inherits
/// affixes from the two source items instead of rolling fresh ones).
/// Collapses to one entry per Affix TYPE, keeping the higher value of any
/// duplicates - the hard guarantee that no item can ever end up with more
/// than one modifier of the same type, applied as a final defensive step
/// wherever multiple affixes get combined from more than one source (see
/// `roll_recombine`, which is the only place that legitimately merges
/// two independent affix lists together - a live report caught a veiled
/// recombine of two items that both happened to roll the same affix type
/// producing two entries of it instead of one).
pub(crate) fn dedup_affixes(affixes: Vec<(Affix, f64)>) -> Vec<(Affix, f64)> {
    let mut result: Vec<(Affix, f64)> = Vec::with_capacity(affixes.len());
    for (affix, value) in affixes {
        match result.iter_mut().find(|(a, _)| *a == affix) {
            Some(existing) => existing.1 = existing.1.max(value),
            None => result.push((affix, value)),
        }
    }
    result
}

/// Relative odds of `affix` being the one picked wherever gear affixes
/// are drawn from `ALL_AFFIXES` (a fresh drop's `roll_affixes`, and the
/// rare bonus-affix draws on a recombine/reforge crit) - every affix is
/// equal odds except `Affix::Leech`, deliberately 10x rarer than
/// everything else per the live request ("items will also have the
/// potential to roll leech but it will be 10x more rare than other
/// affixes"). Consumed by `weighted_affix_pick`.
pub(crate) fn affix_weight(affix: Affix) -> f64 {
    affix_balance(affix).1
}

/// Weighted pick of up to `count` DISTINCT affixes from `pool`, by
/// `affix_weight` - repeatedly rolls a point on the remaining pool's
/// total-weight number line and walks it to find which entry it landed
/// on, same "roll then walk the line" idiom as `pity_reward_count`,
/// removing the winner before the next round so nothing is ever picked
/// twice. Returns fewer than `count` if the pool itself is smaller.
pub(crate) fn weighted_affix_pick(pool: &[Affix], count: usize, rng: &mut impl Rng) -> Vec<Affix> {
    let mut remaining: Vec<Affix> = pool.to_vec();
    let mut picked = Vec::with_capacity(count.min(remaining.len()));
    while !remaining.is_empty() && picked.len() < count {
        let total_weight: f64 = remaining.iter().copied().map(affix_weight).sum();
        let mut roll = rng.gen_range(0.0..total_weight);
        let mut winner = remaining.len() - 1;
        for (i, &a) in remaining.iter().enumerate() {
            roll -= affix_weight(a);
            if roll <= 0.0 {
                winner = i;
                break;
            }
        }
        picked.push(remaining.remove(winner));
    }
    picked
}

pub(crate) fn roll_affixes(slot: EquipSlot, tier: u32, rng: &mut impl Rng) -> Vec<(Affix, f64)> {
    let eligible: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| a.is_eligible_for_slot(slot)).collect();
    let roll: f64 = rng.gen_range(0.0..1.0);
    let count = (if roll < 0.01 {
        4
    } else if roll < 0.06 {
        3
    } else if roll < 0.16 {
        2
    } else {
        1
    })
    .min(eligible.len());

    weighted_affix_pick(&eligible, count, rng)
        .into_iter()
        .map(|affix| {
            let jitter = rng.gen_range(0.85..1.15);
            (affix, affix_base_value(affix, tier) * jitter)
        })
        .collect()
}

