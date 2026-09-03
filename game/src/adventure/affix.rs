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
    /// RETIRED (2026-08-21, replaced by `Echo` below) - permanent inert
    /// legacy variant, same treatment as `CraftAction::CelestialShard`
    /// after the Unique Shard merge: `Affix` has no `#[serde(other)]`
    /// catch-all and no aliases, so deleting this variant outright would
    /// fail to deserialize any not-yet-migrated item still holding
    /// `"lingeringEffect"` in its saved JSON. Excluded from `ALL_AFFIXES`
    /// (never rolled again) and converted to `Echo` at half value by the
    /// one-time `migrate_lingering_effect_to_echo` migration - no live
    /// item should carry this after that marker has run. Was: applies an
    /// unavoidable damage-over-time debuff to whoever this character hits,
    /// itself reworked 2026-08-15 from the old `HealingPower` affix.
    LingeringEffect,
    /// A % chance, rolled once per unified hit (the primary damage/heal
    /// share only - never a splash target, follow-up, or another echo),
    /// for that hit to fire again: floor(pct/100) GUARANTEED repeats plus
    /// a (pct mod 100)% chance of one more (see `roll_echo`'s doc for the
    /// exact ladder). The repeat re-runs the primary `apply_hit`/
    /// `apply_heal` resolution plus its paired `apply_splash`/
    /// `apply_heal_splash` call with fresh rolls off the SAME base amount -
    /// never Radiant Smite, Holy Fire, Bloodpact, or any other once-per-
    /// unified-action currency, and never consumes a second once-per-
    /// swing/once-per-fight charge (Assassinate, Marked for Death, a
    /// death-save charge) off the same original swing. Structurally
    /// cannot itself roll Echo again - see `roll_echo`'s doc. Replaces
    /// `LingeringEffect` (2026-08-21) - every existing `LingeringEffect`-
    /// affixed item was renamed to this at half its stored value by
    /// `migrate_lingering_effect_to_echo`.
    Echo,
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
    /// own docs. Rollable on every slot (2026-08-19 widen, a live
    /// request - previously Weapon/Helm only, "the two slots with an
    /// attacking implicit" per the original rationale; see
    /// `is_eligible_for_slot`). Existing items are unaffected - this only
    /// changes what a NEW roll can produce, nothing re-validates an
    /// item's already-rolled affixes against eligibility.
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
    /// affix is slot-agnostic (2026-08-19: the 5 elemental damage types
    /// were previously Weapon/Helm only - see `ColdDamage`'s own doc for
    /// the widen). `make_item_sacred`/`Character::apply_divine_dust`'s
    /// sacred-affix roll has always ignored this check entirely (draws
    /// from the full `ALL_AFFIXES` pool regardless of slot), so this
    /// widen is a no-op for those two paths.
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
/// brand-new one from. `LingeringEffect` is deliberately absent (retired,
/// never rolled again - see its own doc); `Echo` takes its place so the
/// pool stays the same size.
pub const ALL_AFFIXES: [Affix; 17] = [
    Affix::DamageReduction,
    Affix::BlockChance,
    Affix::Evasion,
    Affix::IncreasedDamage,
    Affix::CritChance,
    Affix::CritMultiplier,
    Affix::Splash,
    Affix::Echo,
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
    /// `is_eligible_for_slot`. Every affix is `None` as of 2026-08-19 -
    /// the 5 elemental damage types were the only ones ever restricted
    /// (Weapon/Helm only, see `Affix::ColdDamage`'s own doc for the
    /// widen) - kept as a real `Option` rather than deleted, so a future
    /// affix can still restrict itself the same way.
    eligible_slots: Option<&'static [EquipSlot]>,
    /// Per-tier coefficient - was `affix_base_value`'s per-arm value.
    default_per_tier: f64,
    /// Relative roll weight - was `affix_weight` (1.0 for everything but
    /// Leech, deliberately 10x rarer).
    default_weight: f64,
}

pub(crate) fn affix_def(affix: Affix) -> AffixDef {
    use Affix::*;
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
        // `default_per_tier` HALVED 0.05 -> 0.025 (2026-09-02,
        // docs/affix_curve_spec.md §7 / R4, owner-ratified). The single
        // explicit exception to the spec's Decision 3 that per-affix
        // coefficients are not part of the curve change: CritMultiplier is
        // both the highest coefficient in this table AND the only affix
        // opening an uncapped MORE layer (`combat_crit_multiplier` is
        // floored at 1.0 and capped nowhere), while its natural partner
        // CritChance is already braked by `overcrit_curve`'s 2.5 asymptote.
        // The existing brake was on the wrong half of the product.
        //
        // Deliberately in CODE rather than as an
        // `[affixes.critMultiplier]` block in adventure-item-balance.toml
        // (R4): a TOML override that permanently contradicts the code
        // default makes the code default a lie, and the only thing telling
        // a future reader otherwise would be a data file that is not in
        // the repository. This compounds with the tier curve - at T=100 the
        // two together take this affix from +500% to +25%.
        CritMultiplier => AffixDef { name: "crit damage dealt", label: "crit dmg dealt", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.025, default_weight: 1.0 },
        Splash => AffixDef { name: "splash", label: "splash", decimals: 0, is_percent: true, eligible_slots: None, default_per_tier: 0.03, default_weight: 1.0 },
        // Retired (see the variant's own doc) - never rolled (absent from
        // `ALL_AFFIXES`), this arm exists only so `affix_def` stays
        // exhaustive and any defensive/legacy-display code path that still
        // calls it on an unmigrated item doesn't panic. Numbers are the
        // pre-retirement ones, unchanged.
        LingeringEffect => AffixDef { name: "lingering effect", label: "lingering effect", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.00025, default_weight: 0.0 },
        // Echo (2026-08-21, replaces Lingering Effect) - half of
        // LingeringEffect's own former per-tier coefficient (0.00025 ->
        // 0.000125), matching the 2x value cut every existing item's
        // stored value took in the same migration that renamed it.
        Echo => AffixDef { name: "echo", label: "echo", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.000125, default_weight: 1.0 },
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
        // restored flat damage) far too small to matter. `eligible_slots`
        // widened to every slot 2026-08-19 (was Weapon/Helm only) - see
        // Affix::ColdDamage's own doc.
        ColdDamage => AffixDef { name: "cold damage dealt", label: "cold damage (evasion debuff chance)", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.0225, default_weight: 1.0 },
        FireDamage => AffixDef { name: "fire damage dealt", label: "fire damage (dmg reduction debuff chance)", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.0225, default_weight: 1.0 },
        LightningDamage => AffixDef { name: "lightning damage dealt", label: "lightning damage (dmg taken debuff chance)", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.0225, default_weight: 1.0 },
        DivineDamage => AffixDef { name: "divine damage dealt", label: "divine damage (heal debuff/buff chance)", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.0225, default_weight: 1.0 },
        ChaosDamage => AffixDef { name: "chaos damage dealt", label: "chaos damage (block debuff chance)", decimals: 2, is_percent: true, eligible_slots: None, default_per_tier: 0.0225, default_weight: 1.0 },
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
                    // A key can deserialize to a real `Affix` variant yet
                    // still be absent from `resolved` - a retired affix
                    // (e.g. `lingeringEffect`, deliberately excluded from
                    // `ALL_AFFIXES` - see its own doc) whose `[affixes.x]`
                    // header is still sitting in the live TOML from before
                    // retirement. No override is possible for a variant
                    // with no live base value to override, so this is
                    // "unknown for override purposes", same as the `Err(_)`
                    // branch below - not a crash (confirmed live 2026-08-21:
                    // this exact case panicked every request in production
                    // for the ~2.5 minutes before rollback).
                    let Some(entry) = resolved.get_mut(&affix) else {
                        tracing::warn!("{ITEM_BALANCE_PATH}: '{key}' is a retired affix with no live base value to override, ignoring");
                        continue;
                    };
                    // An uncommented `[affixes.x]` header with every field
                    // still commented out deserializes to a real (but
                    // empty) entry here - both fields None, matching
                    // AffixOverride::default(). Only log/apply when a
                    // field is ACTUALLY Some(_), so a section header alone
                    // doesn't produce a misleading "overridden" log line
                    // for every affix at every startup.
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
    })
    .get(&affix)
    .copied()
    .unwrap_or_else(|| {
        // A retired-but-still-declared affix (e.g. `LingeringEffect`,
        // deliberately absent from `ALL_AFFIXES` - see its own doc) isn't
        // in `resolved` at all. It can still legitimately be looked up
        // here: item-level migrations that run before this character's
        // own `migrate_lingering_effect_to_echo` has converted it (real
        // fixture data with no prior migration markers - the ordering
        // this ever surfaces under) call this on every existing affix
        // unconditionally. Falls back to the affix's own code-default
        // (no override file entry possible for a retired affix), same
        // values it always had, rather than panicking on a legacy item
        // that just hasn't been migrated yet.
        let d = affix_def(affix);
        (d.default_per_tier, d.default_weight)
    })
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
    affix_balance(affix).0 * affix_tier_curve(tier)
}

/// The affix tier curve `f(T)` (2026-09-02, `docs/affix_curve_spec.md`
/// §1-§3, owner-ratified):
///
/// ```text
/// f(T) = sqrt(T)                  for T <= 100
/// f(T) = 10 * (T / 100)^0.289     for T > 100
/// ```
///
/// It replaces the bare `per_tier * tier` that `affix_base_value` used
/// until this change. **Per-affix `per_tier` coefficients are unchanged**
/// (the single exception, `CritMultiplier` 0.05 -> 0.025, is a separate
/// ratified change in `affix_def` and is not part of this function), so
/// every affix keeps its exact relative weight against every other affix
/// at every tier.
///
/// # The three anchors, and why each is where it is
///
/// - `f(1) = 1` exactly: a new character's first drop reads exactly as it
///   always has. Nothing about the opening experience moves.
/// - `f(100) = 10` exactly: tier 100 now lands on what tier 10 used to
///   deliver. This is the compression point the whole curve is built
///   around.
/// - `11^0.289 = 1.99969`: the first 1,000-tier step past the compression
///   point is (near enough) a doubling.
///
/// The two halves are continuous in VALUE at T=100 - `sqrt(100)` and
/// `10 * 1^0.289` are both exactly 10 - but deliberately NOT in first
/// derivative (slope 0.05 from the left, 0.0289 to the right). Nothing in
/// the game reads the derivative of this function, so the kink is
/// harmless.
///
/// # Why the literal `0.289`
///
/// The exponent giving an exactly-exact doubling is
/// `ln(2)/ln(11) = 0.2890648263`. The ratified constant is the rounded
/// `0.289`, short of a true doubling by 0.0155%. The divergence between
/// the two peaks at 0.045% at T=100,000 and is never material; the
/// readable literal is reproducible by hand by anyone checking this
/// function against the spec. Spec §1 is explicit: "Use `0.289`."
///
/// # Why this is NOT a `LiveTunable`, against the standing doctrine
///
/// CLAUDE.md's TUNABLES DOCTRINE makes a LiveTunable the default for any
/// numeric aspect of a mechanic, with an exception for what is genuinely
/// structural. **This is that exception, and the reason is specific
/// rather than stylistic: affix magnitudes are STORED, not computed.**
/// `roll_affixes` evaluates this function once and persists the product;
/// combat reads the stored `f64` and never calls back here. So a live
/// edit to the exponent would apply to newly-rolled affixes and to future
/// tier growth while every already-stored value kept the shape it was
/// rolled under - a permanent, silent, un-fixable split in the item
/// population, with no migration behind it to close the gap. The curve
/// can only be changed safely by a code change shipped alongside a
/// rescale migration, which is exactly how it arrived. Making the
/// exponent hot would be handing an operator a dial that quietly
/// corrupts the item population.
///
/// `tier.max(1)` guards T=0: `generate_item` floors tier at 1, but
/// `sync_tier_to` and the migrations accept whatever is on disk.
pub(crate) fn affix_tier_curve(tier: u32) -> f64 {
    let t = tier.max(1) as f64;
    if t <= AFFIX_CURVE_KNEE {
        t.sqrt()
    } else {
        AFFIX_CURVE_KNEE.sqrt() * (t / AFFIX_CURVE_KNEE).powf(AFFIX_CURVE_EXPONENT)
    }
}

/// The tier at which `affix_tier_curve` switches from `sqrt` to the
/// power law, and at which it equals exactly 10 - see that function.
/// `sqrt(100) = 10` is what makes the two halves meet with no seam, so
/// this constant appears in both branches rather than being written as a
/// bare `10.0` in the second.
pub(crate) const AFFIX_CURVE_KNEE: f64 = 100.0;
/// The power-law exponent above the knee. See `affix_tier_curve` for why
/// the readable `0.289` is used rather than `ln(2)/ln(11)`.
pub(crate) const AFFIX_CURVE_EXPONENT: f64 = 0.289;

/// The factor by which a stored affix value must be multiplied to move it
/// from `old_tier` to `new_tier` **on the curve**.
///
/// This is the load-bearing half of the change, and the half that is easy
/// to miss (`docs/affix_curve_spec.md` §4.1). Affix magnitudes are stored,
/// and three separate sites grow a stored value when an item's tier rises:
/// `Item::sync_tier_to`, `Character::roll_recombine`, and
/// `AdventureManager::reforge_item`. Every one of them used a plain
/// `new_tier / old_tier` ratio, which was exact while `affix_base_value`
/// was linear in tier and is WRONG the moment it is not.
///
/// Left alone, those three sites would take an item that the rescale
/// migration had just placed on the curve and grow it back along the old
/// linear line from wherever it landed - so the curve would read as a
/// one-time cut that immediately began undoing itself, and a heavily
/// crafted item would sit above the curve again within a few tiers. Using
/// `f(new)/f(old)` instead keeps an item on the curve for the whole of its
/// life, however it got to its tier.
///
/// Note the spec named only two of the three sites; `reforge_item`'s
/// `tier_ratio` has the identical shape and was found by sweeping for it.
pub(crate) fn affix_tier_growth_ratio(old_tier: u32, new_tier: u32) -> f64 {
    let old = affix_tier_curve(old_tier);
    if old <= 0.0 {
        return 1.0;
    }
    affix_tier_curve(new_tier) / old
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
pub fn affix_weight(affix: Affix) -> f64 {
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

/// Cumulative roll cutoff for 4 affixes (a flat 1% chance) - see
/// `roll_affixes`. Named 2026-08-18 for the wiki's constant audit - was
/// a bare `0.01`.
pub(crate) const AFFIX_COUNT_4_CUMULATIVE_THRESHOLD: f64 = 0.01;
/// Cumulative cutoff for 3+ affixes (a further 5% on top of the 4-affix
/// slice above, 6% cumulative) - see `roll_affixes`. Named 2026-08-18
/// for the wiki's constant audit - was a bare `0.06`.
pub(crate) const AFFIX_COUNT_3_CUMULATIVE_THRESHOLD: f64 = 0.06;
/// Cumulative cutoff for 2+ affixes (a further 10% on top of the above,
/// 16% cumulative; the remaining 84% gets exactly 1 affix) - see
/// `roll_affixes`. Named 2026-08-18 for the wiki's constant audit - was
/// a bare `0.16`.
pub(crate) const AFFIX_COUNT_2_CUMULATIVE_THRESHOLD: f64 = 0.16;

pub(crate) fn roll_affixes(slot: EquipSlot, tier: u32, rng: &mut impl Rng) -> Vec<(Affix, f64)> {
    let eligible: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| a.is_eligible_for_slot(slot)).collect();
    let roll: f64 = rng.gen_range(0.0..1.0);
    let count = (if roll < AFFIX_COUNT_4_CUMULATIVE_THRESHOLD {
        4
    } else if roll < AFFIX_COUNT_3_CUMULATIVE_THRESHOLD {
        3
    } else if roll < AFFIX_COUNT_2_CUMULATIVE_THRESHOLD {
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

#[cfg(test)]
mod elemental_slot_widen_tests {
    use super::*;

    const ELEMENTAL_AFFIXES: [Affix; 5] = [Affix::ColdDamage, Affix::FireDamage, Affix::LightningDamage, Affix::DivineDamage, Affix::ChaosDamage];
    // Reads `EQUIP_SLOTS` rather than restating the list: this was a
    // hand-written five-element literal until 2026-09-03, and being
    // test-only it is not compiler-caught - when §8 added four slots it
    // would have gone on asserting the 17-affix pool for the original five
    // and silently stopped covering the new ones, which is precisely the
    // case worth covering (the new slots inherit the full pool because
    // every `AffixDef` has `eligible_slots: None`, and nothing else
    // checks that).
    use crate::adventure::EQUIP_SLOTS as ALL_SLOTS;

    #[test]
    fn every_elemental_affix_is_now_eligible_on_every_slot() {
        for affix in ELEMENTAL_AFFIXES {
            for slot in ALL_SLOTS {
                assert!(affix.is_eligible_for_slot(slot), "{affix:?} must be eligible on {slot:?} after the 2026-08-19 widen");
            }
        }
    }

    #[test]
    fn every_slot_now_has_the_full_17_affix_pool() {
        for slot in ALL_SLOTS {
            let eligible: Vec<Affix> = ALL_AFFIXES.into_iter().filter(|a| a.is_eligible_for_slot(slot)).collect();
            assert_eq!(eligible.len(), ALL_AFFIXES.len(), "{slot:?}'s eligible pool must now match ALL_AFFIXES exactly, got {eligible:?}");
        }
    }

    #[test]
    fn non_elemental_affixes_are_unaffected_by_the_widen() {
        // Sanity check the widen touched only the 5 elemental variants -
        // every other affix was already slot-agnostic and must stay so.
        for affix in ALL_AFFIXES {
            if ELEMENTAL_AFFIXES.contains(&affix) {
                continue;
            }
            for slot in ALL_SLOTS {
                assert!(affix.is_eligible_for_slot(slot), "{affix:?} was already eligible everywhere and must remain so on {slot:?}");
            }
        }
    }

    #[test]
    fn leechs_rarity_weight_is_unchanged_by_the_widen() {
        // The widen dilutes every affix's REALIZED pick-share on Body/
        // Gloves/Boots (bigger pool, same total-weight-relative draw),
        // but must never touch the underlying weight values themselves -
        // Leech's 10x-rarer-than-everything-else ratio is a property of
        // affix_weight, not of pool size.
        assert_eq!(affix_weight(Affix::Leech), 0.1);
        for affix in ALL_AFFIXES {
            if affix != Affix::Leech {
                assert_eq!(affix_weight(affix), 1.0, "{affix:?} must still be the default weight");
            }
        }
    }
}

