use super::*;

/// How many tiers a reforge jumps above the item's CURRENT tier - the
/// point of spending points on it is a real, felt upgrade, not a
/// coinflip re-roll at the same power level. Tapers off at high tiers so
/// growth doesn't stay linear-with-uses forever: +2 to +4 below tier 50,
/// +1 to +2 from 50 up to (not including) 100, a flat +1 at tier 100+.
pub(crate) fn reforge_tier_jump(current_tier: u32, rng: &mut impl Rng) -> u32 {
    if current_tier >= 100 {
        1
    } else if current_tier >= 50 {
        rng.gen_range(1..=2)
    } else {
        rng.gen_range(2..=4)
    }
}

/// Dust cost of the web dashboard's "Reforge Now" button. It can't
/// actually spend a viewer's Twitch channel points (no API lets a website
/// click deduct those outside a real reward redemption), so it charges
/// dust instead of being a pure freebie - still shares the same
/// once-per-hour allowance as the real channel-points redemption.
pub const WEB_REFORGE_DUST_COST: u64 = 1000;

/// Dust cost of recombining two same-slot items into one - see
/// `AdventureManager::recombine_gear`.
pub const RECOMBINE_DUST_COST: u64 = 500;

/// A "veiled" craft (see `PendingVeil`) costs this much more (flat, not a
/// multiplier - a fixed surcharge stays proportionally small on the cheap
/// actions and doesn't run away on the expensive ones) than the base
/// action - rolls 3 candidate outcomes and lets the player pick instead of
/// auto-applying one.
///
/// Scaled by `craft_base_cost_mult` at the point of charge (2026-09-02,
/// an owner ruling) rather than getting its own dial - at 500 flat
/// against a 28-dust tier-1 Transmute the surcharge would BE the price.
/// The one exception is `recombine_gear`, whose veiled cost is this plus
/// 500 per combined modifier and was explicitly left out of the crafting
/// cost cut - see WIKI_IMPACT.md.
pub const VEIL_EXTRA_COST: u64 = 500;

/// A nominal per-tier surcharge on EVERY craft action (2026-08-15, per a
/// live request) - 3 dust x the item's CURRENT tier RAISED TO
/// `craft_tier_exponent` (2026-09-02; it was a flat x tier until then -
/// see `tier_surcharge`, which is the only place this constant is read as
/// a price), added on top of the action's own `base_cost()` scaled by
/// `craft_base_cost_mult` (and the veil surcharge above, likewise scaled,
/// if veiled).
/// Waived by a craft token same as everything else (see `craft_item`) -
/// a token craft stays entirely free, not just base-cost-free.
pub const TIER_CRAFT_DUST_COST: u64 = 3;

/// Shipped default for `LiveTunables::craft_base_cost_mult` (2026-09-02,
/// an owner ruling: "cut all base crafting costs by a factor of 10").
/// Every dust-priced action's flat `base_cost()` is multiplied by this
/// before it is charged, and so is the `VEIL_EXTRA_COST` surcharge that
/// rides on top of it - a surcharge that survived the cut would become
/// the entire price of a cheap craft, inverting its own meaning.
/// Deliberately NOT folded into `base_cost()` itself: that is a plain
/// `fn` with no access to the live tunables (they live behind
/// `AdventureManager`'s `RwLock`), and `adventure_web/wiki.rs` reads it
/// statically. See `scaled_base_cost`, the one place the multiply happens.
pub const CRAFT_BASE_COST_MULT: f64 = 0.1;

/// Lower bound on `craft_base_cost_mult`. **Zero is deliberately legal**
/// (an owner ruling): it zeroes the flat fee only, and the per-tier
/// surcharge below survives it, so even at 0.0 a craft on a tier-1 item
/// still costs `TIER_CRAFT_DUST_COST` dust. Crafting cannot be made free
/// with this dial - it is a "free-crafting weekend" lever with a floor
/// built in.
pub const CRAFT_BASE_COST_MULT_MIN: f64 = 0.0;

/// Upper bound on `craft_base_cost_mult`. The multiplier applies to the
/// UNCHANGED shipped base costs (Transmute is still the constant 250), so
/// **1.0 is the value that restores the pre-2026-09-02 prices exactly** -
/// "you can always put it back" lives inside the range, not at its edge.
/// 10.0 is a full order of magnitude ABOVE those old prices: enough
/// headroom to make crafting a real dust sink again if the economy ever
/// needs one, and far enough from any sane setting that a fat-fingered
/// extra digit is refused rather than charged.
pub const CRAFT_BASE_COST_MULT_MAX: f64 = 10.0;

/// Shipped default for `LiveTunables::craft_tier_exponent` (2026-09-02,
/// the same ruling): the per-tier surcharge is
/// `TIER_CRAFT_DUST_COST x tier^exponent`, not `x tier`. Polynomial, not
/// exponential - cost accelerates with tier, slowly, and never runs away
/// over the tier range the game reaches (tier is `1 + stage/5`, so stage
/// 1000 is tier 201). See `tier_surcharge`.
pub const CRAFT_TIER_EXPONENT: f64 = 1.1;

/// Lower bound on `craft_tier_exponent`. 1.0 is exactly the old linear
/// `3 x tier` curve, so the floor is "put it back". Sub-1 is REFUSED (an
/// owner ruling): a decelerating curve makes high-tier crafting
/// relatively cheaper as players progress, inverting the sink, and
/// `craft_base_cost_mult` already covers every "make it cheaper" case.
pub const CRAFT_TIER_EXPONENT_MIN: f64 = 1.0;

/// Upper bound on `craft_tier_exponent`. At 1.5 a tier-201 craft pays
/// 8,551 dust in surcharge and a tier-500 one pays 33,541; past that the
/// curve outruns the tier range the game actually reaches.
pub const CRAFT_TIER_EXPONENT_MAX: f64 = 1.5;

/// Resolves a live `craft_base_cost_mult` reading into the usable range -
/// non-finite falls back to the shipped default, otherwise clamped. Same
/// discipline as `pacing::sanitize_pool_cap`: the form's own min/max is
/// what reports an out-of-range value to the operator, this is the
/// defence-in-depth behind a hand-crafted POST.
pub fn sanitize_craft_base_cost_mult(value: f64) -> f64 {
    if !value.is_finite() {
        return CRAFT_BASE_COST_MULT;
    }
    value.clamp(CRAFT_BASE_COST_MULT_MIN, CRAFT_BASE_COST_MULT_MAX)
}

/// `sanitize_craft_base_cost_mult`'s twin for the exponent.
pub fn sanitize_craft_tier_exponent(value: f64) -> f64 {
    if !value.is_finite() {
        return CRAFT_TIER_EXPONENT;
    }
    value.clamp(CRAFT_TIER_EXPONENT_MIN, CRAFT_TIER_EXPONENT_MAX)
}

/// One action's flat fee after `craft_base_cost_mult` - the ONLY place
/// the base multiply happens, so the crafting panel's button preview and
/// the charge in `craft_item_ex` can never disagree about it.
///
/// **`ceil`, never `round`** - a nonzero base cost must never round down
/// to nothing. At the floor of the dial's range 250 x 0.004 is still 1
/// dust; the only way to reach 0 here is an operator deliberately setting
/// the multiplier to exactly 0.0.
///
/// `u64::MAX` passes through untouched: that is the "never affordable in
/// dust" sentinel `CelestialShard`/`UniqueShard` carry (see
/// `craft_action_def`), not a real price, and multiplying it would turn
/// an unaffordable action into an affordable one.
pub fn scaled_base_cost(base: u64, mult: f64) -> u64 {
    if base == u64::MAX {
        return u64::MAX;
    }
    (base as f64 * sanitize_craft_base_cost_mult(mult)).ceil() as u64
}

/// The per-tier surcharge: `TIER_CRAFT_DUST_COST x tier^exponent`,
/// `ceil`'d as its own term (NOT folded into the base term - the two are
/// rounded independently, then summed).
///
/// Order of operations matters and is load-bearing: at tier 10 this is
/// `ceil(3 x 10^1.1)` = 38, while `ceil((3 x 10)^1.1)` would be 43 and
/// `3 x ceil(10^1.1)` would be 39. See
/// `tier_surcharge_is_the_exponent_of_the_tier_not_of_the_product`.
pub fn tier_surcharge(tier: u32, exponent: f64) -> u64 {
    (TIER_CRAFT_DUST_COST as f64 * (tier as f64).powf(sanitize_craft_tier_exponent(exponent))).ceil() as u64
}

/// Shipped default for `LiveTunables::craft_tier_bump_mult` - 1.0, i.e.
/// the banded per-craft tier bump is applied exactly as it always has
/// been. The DIAL is the 2026-09-02 deliverable; the behaviour at the
/// default is unchanged.
pub const CRAFT_TIER_BUMP_MULT: f64 = 1.0;

/// Lower bound. **Zero is deliberately legal and is the setting most
/// likely to be wanted first**: it switches per-craft tier growth off
/// entirely without touching the bands, which is the clean way to watch
/// `craft_tier_exponent` in isolation.
pub const CRAFT_TIER_BUMP_MULT_MIN: f64 = 0.0;

/// Upper bound. At 3.0 the fastest band is +9 tiers per craft, so one
/// Hideout Warrior click (5 crafts) takes a fresh item from tier 1 to
/// roughly tier 40 - already past any plausible intent. Anything above
/// that is a typo rather than a lever.
pub const CRAFT_TIER_BUMP_MULT_MAX: f64 = 3.0;

/// `sanitize_craft_base_cost_mult`'s twin for the tier-bump dial.
pub fn sanitize_craft_tier_bump_mult(value: f64) -> f64 {
    if !value.is_finite() {
        return CRAFT_TIER_BUMP_MULT;
    }
    value.clamp(CRAFT_TIER_BUMP_MULT_MIN, CRAFT_TIER_BUMP_MULT_MAX)
}

/// How many tiers one successful craft adds to the item it crafted.
///
/// **The bands are the designed shape and are NOT tunable**: +3 below
/// tier 25, +2 below 50, +1 above. `mult` scales that shape as a whole -
/// one dial, so the relationship between the three bands can never drift
/// apart from an admin page.
///
/// **`round`, not `ceil`** - the opposite discipline to `scaled_base_cost`,
/// and deliberately so. There a nonzero PRICE must never round away to
/// free, so it rounds up. Here a fractional bump must be allowed to reach
/// zero: `ceil` would pin every nonzero multiplier at +1 minimum and turn
/// 0.0 into a cliff, when the whole point of the low end of the range is
/// to wind growth down gradually and then off.
///
/// See `Character::apply_craft_tier_bump`, the single call site, for what
/// this growth is and why it predates tier having a price.
pub fn craft_tier_bump(tier: u32, mult: f64) -> u32 {
    let banded = if tier < 25 {
        3.0
    } else if tier < 50 {
        2.0
    } else {
        1.0
    };
    (banded * sanitize_craft_tier_bump_mult(mult)).round() as u32
}

/// The fixed 5-step chain Hideout Warrior runs, in order - the only
/// sequence of existing `CraftAction::required_affix_count` preconditions
/// that takes a bare item all the way through to Krangled.
///
/// Lives here rather than in `adventure_web.rs` (where it was defined
/// until 2026-08-24) because it now has two consumers: `do_hideout_warrior`
/// and Divinity, which is defined as "Hideout Warrior over the whole bag".
/// One definition is the point - if this chain ever gains or loses a step,
/// a Divinity that had its own copy would silently stop meaning what its
/// own name says.
pub(crate) const HIDEOUT_WARRIOR_STEPS: [CraftAction; 5] =
    [CraftAction::Transmute, CraftAction::Augment, CraftAction::Regal, CraftAction::Exalt, CraftAction::Krangle];

/// Nickname stamped on every item Divinity Krangles (2026-08-24, an owner
/// ruling). Krangle normally opens a "name your item" prompt
/// (`render_nickname_prompt`), which shows one un-named locked item per
/// dashboard load - after a full-bag Divinity that would be up to 150
/// consecutive forced prompts. Naming them up front means the prompt has
/// nothing left to ask about.
pub const DIVINITY_NICKNAME: &str = "From Divinity";

/// Every `CraftAction` - what a new character's starter free-token grant
/// (see `Character::new`) and the existing-character backfill (see
/// `AdventureManager::new`) both hand out one of each from.
pub const ALL_CRAFT_ACTIONS: [CraftAction; 8] = [
    CraftAction::Transmute,
    CraftAction::Scour,
    CraftAction::Augment,
    CraftAction::Regal,
    CraftAction::Exalt,
    CraftAction::Krangle,
    CraftAction::Annulment,
    CraftAction::Chancing,
];

/// `ALL_CRAFT_ACTIONS` minus Scour - what every RANDOM token drop (boss
/// fight kills, both boss/basic pity payouts) picks from, per the
/// request that Scour tokens specifically should never drop, only be
/// handed out (still one of each, Scour included, via the deliberate
/// starter-kit grant/backfill above - "never drop" is about the random
/// roll, not the guaranteed onboarding freebie).
pub const DROPPABLE_CRAFT_ACTIONS: [CraftAction; 7] = [
    CraftAction::Transmute,
    CraftAction::Augment,
    CraftAction::Regal,
    CraftAction::Exalt,
    CraftAction::Krangle,
    CraftAction::Annulment,
    CraftAction::Chancing,
];

/// One of the six ARPG-style currency crafting actions - see
/// `Character::craft`. Ordered cheapest-to-most-expensive/most-committal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CraftAction {
    /// Adds a random modifier to a bare (0-affix) item.
    Transmute,
    /// Removes all additional modifiers (the item's base slot stat is
    /// never affected - there's nothing to "scour" off it).
    Scour,
    /// Adds a 2nd modifier to a 1-modifier item.
    Augment,
    /// Adds a 3rd modifier to a 2-modifier item.
    Regal,
    /// Adds a 4th modifier to a 3-modifier item.
    Exalt,
    /// Adds one final modifier to ANY unlocked item (no count
    /// precondition, can push past the normal 4-modifier cap) and
    /// permanently locks it - see `Item::locked`.
    Krangle,
    /// Removes one existing modifier. Non-veiled: a uniformly random one
    /// goes. Veiled: rolls up to 2 DISTINCT existing modifiers as removal
    /// candidates (only 1 if the item has just 1 modifier) and the player
    /// picks which one actually leaves - see
    /// `Character::annul_random_affix`/`apply_annulment_removal`.
    Annulment,
    /// A real chance-orb reroll: every existing modifier gets a brand-new
    /// TYPE (not just a new value for its old one), each at a fresh roll
    /// range for that new type - see `Character::chance_all_affixes`
    /// (2026-08-17, fixing this action's original wrong "value only"
    /// behavior). Also works on a Reforge/Recombine crit-bonus slot
    /// (`Item::crit_bonus_affixes`) - the slot stays marked special under
    /// its new type. Non-veiled: every slot rerolls at once. Veiled: one
    /// slot at a time, on the SAME shared `PendingVeil` panel every other
    /// veilable action uses (see `PendingVeil::chancing_remaining`/
    /// `AdventureManager::choose_veil_outcome`) - each step's pick commits
    /// immediately, even if the pass is never finished.
    Chancing,
    /// RETIRED (2026-08-19, Unified Unique Shards) - CelestialShard merged
    /// into `UniqueShard` below as one currency; this variant is kept
    /// PERMANENTLY as an inert, non-earnable legacy value purely for save-
    /// data safety (an old character record can still contain the literal
    /// string `"celestialshard"` in `craft_tokens` until
    /// `migrate_celestial_shard_into_unique_shard` next loads it - deleting
    /// this variant would make that record fail to deserialize at all,
    /// before the migration ever got a chance to run). Nothing grants it
    /// anymore (`maybe_drop_celestial_shard` is deleted, the one-time top-
    /// healer award still targets it as a historical record of what
    /// actually happened - see `announce_encounter_result` - but that
    /// marker already fired in production and can never fire again).
    /// `Character::craft_inner`'s own CelestialShard branch still applies
    /// it exactly as before (grants `UniqueAffix::CelestialConversion`,
    /// no picker) purely as a defensive fallback for a not-yet-migrated
    /// straggler token - unreachable through the UI (see
    /// `craft_token_count`-gated button visibility), and even if reached,
    /// the very next character load merges any resulting token into
    /// `UniqueShard` anyway.
    CelestialShard,
    /// The ONE surviving unique-crafting currency (2026-08-19: merged
    /// with the old CelestialShard above - "no more celestial shard, only
    /// Unique Shards"). Consumes one token to grant EITHER
    /// `UniqueAffix::CelestialConversion` or `UniqueAffix::SplitPersonality`
    /// to the target item, player's choice at apply time (see
    /// `AdventureManager::craft_item_ex`'s own UniqueShard branch and
    /// `ALL_UNIQUE_AFFIXES`) - no dust, no veil surcharge, always shown as
    /// a deterministic menu (this reuses `PendingVeil`'s storage/lifecycle
    /// machinery as a data structure, NOT its "pay extra to reveal rolled
    /// randomness" pricing model - there is no randomness here to reveal).
    /// Deliberately NOT in `ALL_CRAFT_ACTIONS`/`DROPPABLE_CRAFT_ACTIONS` -
    /// nobody starts with one, only obtained via its own rare drop (see
    /// `maybe_drop_unique_shard`) or the reusable one-time launch-giveaway
    /// table in main.rs.
    UniqueShard,
    /// Polishing (2026-08-15) - a fundamentally different currency shape
    /// from the other 6: costs SAND, not dust (see `Character::sand`'s
    /// doc), priced off the TARGET ITEM's own quality rather than a flat
    /// per-action fee, and never goes through `AdventureManager::
    /// craft_item`'s normal dust/token/veil machinery at all - see that
    /// function's own early Polishing branch, and `Character::polish`
    /// for the actual effect. Deliberately NOT in
    /// `ALL_CRAFT_ACTIONS`/`DROPPABLE_CRAFT_ACTIONS` for the same reason
    /// CelestialShard isn't - it doesn't fit either array's shared "flat
    /// dust cost, can drop as a token" assumption.
    Polishing,
    /// Crafting-panel Reforge (2026-08-15) - dust-denominated, like most
    /// actions, but priced at `30 * tier` instead of the shared
    /// base-cost-plus-per-tier-surcharge formula (see `base_cost`'s own
    /// arm and `AdventureManager::craft_item`'s early Reforge branch), and
    /// its result is a `ReforgeOutcome` (new tier + possible bonus affix),
    /// not a `CraftOutcome` - see `CraftResult::Reforged` and
    /// `Character::reforge_item`. Deliberately NOT in
    /// `ALL_CRAFT_ACTIONS`/`DROPPABLE_CRAFT_ACTIONS`, same reason as
    /// Polishing.
    Reforge,
    /// Divine Dust apply/reroll (2026-08-19, docs/divine_dust_spec.md) -
    /// same "fundamentally different currency shape" pattern as Polishing/
    /// Reforge: priced at `2 × item.tier` DIVINE DUST (not dust or sand),
    /// bypasses `AdventureManager::craft_item_ex`'s normal machinery
    /// entirely via its own early branch, and its result is a
    /// `DivineDustOutcome`, not a `CraftOutcome` - see
    /// `CraftResult::DivineDustApplied` and `Character::apply_divine_dust`.
    /// Deliberately NOT in `ALL_CRAFT_ACTIONS`/`DROPPABLE_CRAFT_ACTIONS`,
    /// same reason as Polishing/Reforge - it doesn't fit either array's
    /// shared "flat dust cost, can drop as a token" assumption, and Divine
    /// Dust craft tokens were never part of this feature's design.
    DivineDust,
}

/// Everything about one `CraftAction` variant - label, precondition,
/// veilability, and its default dust cost - collapsed into one match arm
/// per variant instead of the 4 separate matches this replaces
/// (`base_cost`/`required_affix_count`/`is_veilable`/`label`).
/// `default_cost` is the fallback of record for the 6 real dust-priced
/// actions (`ALL_CRAFT_ACTIONS`); `adventure-item-balance.toml` can
/// sparsely override it (see `craft_action_cost`). CelestialShard/
/// UniqueShard's `u64::MAX` sentinel and Polishing/Reforge's `0` are never
/// actually read as real prices (see their own doc comments below) and
/// are deliberately NOT exposed to the TOML override file.
pub(crate) struct CraftActionDef {
    label: &'static str,
    required_affix_count: Option<usize>,
    is_veilable: bool,
    default_cost: u64,
}

pub(crate) fn craft_action_def(action: CraftAction) -> CraftActionDef {
    use CraftAction::*;
    match action {
        Transmute => CraftActionDef { label: "Transmute", required_affix_count: Some(0), is_veilable: true, default_cost: 250 },
        Scour => CraftActionDef { label: "Scour", required_affix_count: None, is_veilable: false, default_cost: 250 },
        Augment => CraftActionDef { label: "Augment", required_affix_count: Some(1), is_veilable: true, default_cost: 500 },
        Regal => CraftActionDef { label: "Regal", required_affix_count: Some(2), is_veilable: true, default_cost: 750 },
        Exalt => CraftActionDef { label: "Exalt", required_affix_count: Some(3), is_veilable: true, default_cost: 1250 },
        Krangle => CraftActionDef { label: "Krangle", required_affix_count: None, is_veilable: true, default_cost: 2500 },
        // `required_affix_count: None` - both need "at least 1 existing
        // modifier," not an exact count, same reason Scour uses `None` and
        // does its own manual empty-check (see `annul_random_affix`/
        // `chance_all_affixes`).
        Annulment => CraftActionDef { label: "Annulment Orb", required_affix_count: None, is_veilable: true, default_cost: 1000 },
        Chancing => CraftActionDef { label: "Chancing", required_affix_count: None, is_veilable: true, default_cost: 800 },
        // Retired (see the enum variant's own doc) - this arm only still
        // matters for `Character::craft_inner`'s defensive legacy branch;
        // never reachable via the UI (no button can show it - see
        // `craft_token_count`-gated visibility).
        CelestialShard => CraftActionDef { label: "Celestial Shard", required_affix_count: None, is_veilable: false, default_cost: u64::MAX },
        // Never affordable in dust alone (see `AdventureManager::
        // craft_item_ex`'s own dedicated UniqueShard branch, which bypasses
        // this generic token/veil/dust machinery entirely, same shape as
        // Polishing/Reforge/DivineDust below) - a Unique Shard application
        // ONLY goes through with the actual token. `is_veilable: false`
        // here is moot for the same reason it is for Polishing/Reforge/
        // DivineDust - the picker is unconditional, not gated by this flag.
        UniqueShard => CraftActionDef { label: "Unique Shard", required_affix_count: None, is_veilable: false, default_cost: u64::MAX },
        // Not dust-denominated at all - see Polishing's own doc on the
        // enum. Never actually read (craft_item branches around it
        // before this could matter).
        Polishing => CraftActionDef { label: "Polishing", required_affix_count: None, is_veilable: false, default_cost: 0 },
        // Priced at `30 * tier` instead - see Reforge's own doc on the
        // enum. Never actually read, same reason as Polishing above.
        Reforge => CraftActionDef { label: "Reforge", required_affix_count: None, is_veilable: false, default_cost: 0 },
        // Priced at `2 * item.tier` DIVINE DUST instead - see DivineDust's
        // own doc on the enum. Never actually read, same reason as
        // Polishing/Reforge above.
        DivineDust => CraftActionDef { label: "Divine Dust", required_affix_count: None, is_veilable: false, default_cost: 0 },
    }
}

/// Resolved dust cost for every entry of `ALL_CRAFT_ACTIONS`, computed
/// once and cached - `CraftActionDef`'s code defaults with
/// `adventure-item-balance.toml`'s `[craft_action_cost]` overrides (if
/// any) applied on top. CelestialShard/Polishing/Reforge are excluded
/// (see `craft_action_def`'s doc) - `base_cost` returns their sentinel
/// directly without consulting this map.
pub(crate) static CRAFT_ACTION_COST: std::sync::OnceLock<HashMap<CraftAction, u64>> = std::sync::OnceLock::new();

pub(crate) fn craft_action_cost(action: CraftAction) -> u64 {
    *CRAFT_ACTION_COST
        .get_or_init(|| {
            let raw = load_item_balance_file().craft_action_cost;
            let mut resolved: HashMap<CraftAction, u64> = ALL_CRAFT_ACTIONS.iter().map(|&a| (a, craft_action_def(a).default_cost)).collect();
            for (key, cost) in raw {
                match CraftAction::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&key)) {
                    Ok(action) if resolved.contains_key(&action) => {
                        resolved.insert(action, cost);
                        tracing::info!("{ITEM_BALANCE_PATH}: craft_action_cost.{key} overridden to {cost}");
                    }
                    _ => tracing::warn!("{ITEM_BALANCE_PATH}: unknown/non-overridable craft_action_cost key '{key}', ignoring"),
                }
            }
            resolved
        })
        .get(&action)
        .unwrap_or(&0)
}

impl CraftAction {
    pub fn base_cost(self) -> u64 {
        match self {
            CraftAction::CelestialShard | CraftAction::UniqueShard | CraftAction::Polishing | CraftAction::Reforge | CraftAction::DivineDust => {
                craft_action_def(self).default_cost
            }
            _ => craft_action_cost(self),
        }
    }

    /// Exact affix count the target item must currently have -
    /// `None` means "no exact-count precondition" (Scour just needs
    /// ≥1 to actually remove something; Krangle/CelestialShard/UniqueShard
    /// need nothing at all - Krangle per the confirmed design works on any
    /// unlocked item, CelestialShard/UniqueShard's own precondition is
    /// "not already unique", checked separately - see
    /// `AdventureManager::craft_item_ex`'s own UniqueShard branch and
    /// `Character::craft_inner`'s legacy CelestialShard one - not an
    /// affix count). Polishing works on any unlocked item too (0 affixes
    /// just means only the quality bump applies, nothing to roll higher).
    pub fn required_affix_count(self) -> Option<usize> {
        craft_action_def(self).required_affix_count
    }

    /// Whether this action has real randomness worth veiling (see
    /// `PendingVeil`) - Scour is fully deterministic, nothing to choose
    /// between; legacy CelestialShard always grants the exact same single
    /// unique affix, equally nothing to choose between. Polishing/Reforge/
    /// DivineDust/UniqueShard never reach this check at all (each bypasses
    /// `craft_item_ex`'s normal veil machinery entirely via its own early
    /// branch - see their own docs), so its value here is moot for all
    /// four, but `false` is the honest answer regardless - UniqueShard's
    /// own picker is unconditional, not an OPTIONAL veil a player pays
    /// extra for.
    pub fn is_veilable(self) -> bool {
        craft_action_def(self).is_veilable
    }

    pub fn label(self) -> &'static str {
        craft_action_def(self).label
    }
}

/// Why a `Character::craft`/`AdventureManager::craft_item` attempt
/// didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum CraftError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// No such item id currently exists on this character.
    ItemNotFound,
    /// The item is locked (Krangled) - excluded from every crafting
    /// action.
    ItemLocked,
    /// The player has ticked "Keep" on this item (`Item::disenchant_protected`)
    /// - 2026-08-24, the tick-box was widened from a disenchant guard into
    /// a full "don't touch this" lock, so it now refuses every mutation
    /// too, not just destruction. Deliberately a SEPARATE variant from
    /// `ItemLocked` even though both mean "refused, this item is
    /// protected": `ItemLocked` is permanent and the game imposed it
    /// (Krangle), this one is the player's own choice and they can undo it
    /// from the item's own card, so the two need different player-facing
    /// text or the message is actively unhelpful.
    ItemProtected,
    /// The item's current affix count doesn't match what this action
    /// requires - see `CraftAction::required_affix_count`.
    PreconditionNotMet,
    /// Scour/Annulment on an item that already has 0 additional modifiers.
    NothingToRemove,
    /// Chancing on an item that already has 0 additional modifiers -
    /// nothing to reroll.
    NothingToReroll,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
    /// Every affix type is already present on the item - nothing left
    /// to roll (only realistically reachable after several Krangles
    /// pushed an item past the normal 4-modifier cap).
    NoCandidatesLeft,
    /// `CraftAction::CelestialShard` on an item that already has a
    /// `UniqueAffix` - one unique affix per item, no stacking/replacing.
    AlreadyUnique,
    /// `CraftAction::Krangle` on an item that already has a
    /// `UniqueAffix` - unique and locked are mutually exclusive states
    /// (see `Item::unique_affix`'s doc).
    CannotKrangleUnique,
    /// Not enough sand — `CraftAction::Polishing` only, carries the cost
    /// that was needed (see `Character::sand`'s doc for why this is a
    /// separate currency from `InsufficientDust`).
    InsufficientSand(u64),
    /// `CraftAction::Polishing` only (2026-08-17, a live report: sand was
    /// being charged even when nothing on the item could actually improve)
    /// - every affix is already pinned at its max roll AND, for a
    /// non-Perfect item, `power_roll` itself is already maxed too (a
    /// Perfect item never touches `power_roll` at all, so it's this alone
    /// for Perfect - see `Character::polish`). Checked BEFORE any cost is
    /// deducted, unlike every other precondition here which just happens
    /// to also be checked first.
    NothingToPolish,
    /// `CraftAction::DivineDust` only - not enough Divine Dust to apply/
    /// reroll a sacred affix (cost is `2 × item.tier`, see
    /// `AdventureManager::craft_item_ex`'s Divine Dust branch).
    InsufficientDivineDust(u64),
    /// `CraftAction::DivineDust` only, reroll case - the valid replacement
    /// pool (every `Affix` except the item's current sacred affix) is
    /// empty. Unreachable today (`Affix` has 17 variants, so excluding
    /// just one always leaves 16), but the spec calls for the guard
    /// explicitly, so it's implemented and tested defensively rather than
    /// assumed away.
    NoValidRerollTarget,
    /// 2026-08-21 duplicate-unique-effects fix - `CraftAction::UniqueShard`
    /// (and the legacy `CraftAction::CelestialShard` path) on an
    /// EQUIPPED item where every remaining unique-affix candidate would
    /// duplicate a unique already worn in another equipped slot (see
    /// `Character::has_conflicting_unique_affix_value`). An item sitting
    /// in the bag never hits this - a conflict there is only ever an
    /// equip-time concern, same as any other unique-bearing item already
    /// unequipped. Checked BEFORE token consumption, same convention
    /// `ItemLocked`/`AlreadyUnique` already use.
    ConflictingUniqueAffix,
}

/// Why `AdventureManager::apply_divinity` didn't run at all. Own error
/// type rather than a `CraftError` variant for the same reason
/// `DivineDustCraftError` has one: Divinity has no single target item, so
/// every item-shaped `CraftError` variant is meaningless for it. A
/// per-ITEM refusal inside a run is not an error at all - it is a skip,
/// counted in `DivinityReport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivinityError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// No Unique Shard banked - Divinity's only price (see
    /// `CraftAction::UniqueShard`). Checked before anything is planned, so
    /// a refusal never costs a shard.
    NoShard,
    /// The bag is empty. Distinct from `NothingEligible` below so the
    /// message can say which - "you have nothing" and "everything you have
    /// is locked" want very different responses from the player.
    EmptyBag,
    /// The bag has items but every one of them is Krangled or "Keep"-
    /// ticked. Deliberately an error rather than a zero-work success: a
    /// shard must never be spent on a run that could not touch anything.
    NothingEligible,
}

/// What one Divinity run planned to do, decided BEFORE anything is
/// mutated (see `Character::plan_divinity`). Splitting the decision from
/// the application is what lets the whole run happen inside a single
/// `characters` lock with a single persist at the end: the planning half
/// is pure reads, so it cannot fail partway and leave a half-crafted bag.
#[derive(Debug, Clone)]
pub struct DivinityPlan {
    /// Bag item ids to run the chain over, in bag order. Equipped gear is
    /// never included - Divinity is bag-only by ruling.
    pub targets: Vec<String>,
    /// Items skipped because they are already Krangled (`Item::locked`).
    pub skipped_krangled: usize,
    /// Items skipped because the player ticked "Keep"
    /// (`Item::disenchant_protected`).
    pub skipped_kept: usize,
    /// How many items were in the bag when this was planned.
    pub bag_items: usize,
}

/// What one Divinity run actually did - the summary the single completion
/// broadcast and the result popup are both built from. Deliberately
/// aggregate: a full bag is up to 150 items and ~560 craft steps, and a
/// per-item log of that is not something anyone can read.
#[derive(Debug, Clone, Default)]
pub struct DivinityReport {
    /// Bag size at plan time.
    pub bag_items: usize,
    /// Items that ended up with at least one step applied.
    pub items_changed: usize,
    /// Total craft steps that landed across every item.
    pub steps_applied: usize,
    /// How many items reached the Krangle step (and so got named
    /// `DIVINITY_NICKNAME`).
    pub krangled: usize,
    /// Already-Krangled items left alone.
    pub skipped_krangled: usize,
    /// "Keep"-ticked items left alone.
    pub skipped_kept: usize,
    /// Eligible items where every step turned out to be ineligible - a
    /// 4-modifier item carrying a unique affix, for instance: no affix-add
    /// step matches its count and Krangle refuses a unique.
    pub unchanged: usize,
}

/// Why `AdventureManager::craft_divine_dust` (the dust+sand → Divine Dust
/// recipe, `/craft`'s "Craft Divine Dust" row) didn't go through. Its own
/// error type, not folded into `CraftError` above - the recipe is a pure
/// currency conversion with no target item at all, so none of
/// `CraftError`'s item-shaped variants (`ItemNotFound`/`ItemLocked`/
/// `PreconditionNotMet`/etc.) could ever apply to it.
#[derive(Debug, Clone, Copy)]
pub enum DivineDustCraftError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
    /// Not enough sand — carries the cost that was needed.
    InsufficientSand(u64),
    /// The recipe is not unlocked yet (2026-09-02) — carries the world
    /// stage the group has to reach. A ONE-WAY LATCH on
    /// `WorldState::highest_stage`, not on the current stage: once the
    /// group has ever reached the threshold the recipe stays unlocked
    /// through any later boss-loss regression (owner ruling — "losing a
    /// recipe to a bad boss streak would be miserable"). Deliberately
    /// unlike the four stage-gated DROPS, which all read the live stage and
    /// really do pause on a regression.
    Locked(u32),
}

/// Result of a successful currency craft - see `Character::craft`. One
/// shape covers every action's result: Scour sets `affixes_removed`,
/// every affix-adding action sets `affix_added`, Krangle also sets
/// `now_locked`, UniqueShard (and the legacy CelestialShard path) sets
/// `unique_affix_added`.
#[derive(Debug, Clone)]
pub struct CraftOutcome {
    pub item_name: String,
    pub slot: EquipSlot,
    pub tier: u32,
    pub action: CraftAction,
    pub affix_added: Option<Affix>,
    /// The rolled value paired with `affix_added` - `None` whenever
    /// `affix_added` is `None`. Split out (rather than folded into a
    /// tuple) so `affix_added` alone stays a simple flag for callers
    /// that don't care about the magnitude.
    pub affix_value: Option<f64>,
    /// Set only by `CraftAction::Annulment` - the modifier type that was
    /// actually removed. `None` for every other action.
    pub affix_removed: Option<Affix>,
    /// The value `affix_removed` had right before it was removed - paired
    /// the same way `affix_value` pairs with `affix_added`. `None`
    /// whenever `affix_removed` is `None`.
    pub affix_removed_value: Option<f64>,
    pub affixes_removed: u32,
    pub now_locked: bool,
    /// Set by `CraftAction::UniqueShard` (via its apply-time picker - see
    /// `AdventureManager::craft_item_ex`'s own branch) or the legacy
    /// `CraftAction::CelestialShard` path - `None` for every other action.
    pub unique_affix_added: Option<UniqueAffix>,
    /// Set only by `CraftAction::Polishing` - which affix(es) got their
    /// roll raised (1 for a normal item, up to 2 for a Perfect one - see
    /// `Character::polish`), paired with their NEW value. Empty for every
    /// other action; `affix_added`/`affix_value` stay `None` for
    /// Polishing instead of trying to force this into that singular
    /// shape.
    pub polished_affixes: Vec<(Affix, f64)>,
    /// Set only by `CraftAction::Chancing` - the OLD affix type that used
    /// to occupy each slot in `polished_affixes` (same index, same
    /// length) before this reroll replaced it - lets the "what changed"
    /// popup show a real before→after (e.g. "Fire Damage → Crit Chance"),
    /// not just the new state alone. Empty for every other action.
    pub chancing_previous: Vec<Affix>,
    /// Set only by `CraftAction::Polishing`, and only when it actually
    /// raised the item's own quality (a normal item, not already-maxed
    /// Perfect one) - the item's new `quality_percent()` after the bump.
    /// `None` for every other action, and for a Polished Perfect item
    /// (nothing to raise there).
    pub new_quality_percent: Option<f64>,
    /// The item's own Perfect-Quality flag AFTER this craft - lets a
    /// caller compute the correct roll range for `affix_added` (see
    /// `craft_affix_value_range`) without a separate item lookup, since
    /// Perfect items roll every affix through `PERFECT_QUALITY_MULT`.
    pub perfect: bool,
}

/// What `AdventureManager::craft_item` handed back - `Applied` when it
/// went through immediately (the non-veiled, default path);
/// `PendingChoice` when it was veiled instead and is now waiting on
/// `AdventureManager::choose_veil_outcome` (see `PendingVeil`);
/// `Reforged` when `action` was `CraftAction::Reforge` - it never
/// produces a `CraftOutcome` at all (see `Character::reforge_item`),
/// so it gets its own result shape instead of trying to force a
/// `ReforgeOutcome` into `CraftOutcome`'s fields. `DivineDustApplied`
/// is the same idea for `CraftAction::DivineDust` - see
/// `Character::apply_divine_dust`/`DivineDustOutcome`.
#[derive(Debug, Clone)]
pub enum CraftResult {
    Applied(CraftOutcome),
    PendingChoice,
    Reforged(ReforgeOutcome),
    DivineDustApplied(DivineDustOutcome),
}

/// What kind of craft is behind an in-progress veiled choice - see
/// `PendingVeil`.
#[derive(Debug, Clone)]
pub enum PendingVeilAction {
    Currency { item_id: String, action: CraftAction },
    Recombine { item_id_a: String, item_id_b: String },
}

/// One candidate outcome of a veiled craft - the player picks between
/// up to 3 of these (see `PendingVeil`). A currency craft's candidates
/// are all `Currency`; a recombine's are all `Recombine`.
#[derive(Debug, Clone)]
pub enum VeilCandidate {
    Currency(CraftOutcome),
    Recombine(RecombineRoll),
}

/// What `AdventureManager::choose_veil_outcome` hands back - the REAL
/// applied result (same shapes `craft_item`/`recombine_gear`'s own
/// `Applied` variant carries for the non-veiled path), not the
/// pre-application `VeilCandidate` the player picked from.
#[derive(Debug, Clone)]
pub enum VeilChosenOutcome {
    Currency(CraftOutcome),
    Recombine(RecombineOutcome),
    /// A veiled Chancing step was applied, but more affix slots remain in
    /// this pass - a fresh `PendingVeil` for the next slot is already
    /// inserted. The caller should redirect back silently (the next
    /// render just picks up the new pending state, same POST-redirect-GET
    /// idiom every other veiled craft uses) rather than showing a "what
    /// changed" popup - only the LAST slot's pick returns `Currency`.
    ChancingContinues,
}

/// An in-progress veiled craft (+`VEIL_EXTRA_COST` cost, see
/// `CraftAction::is_veilable`) awaiting the player's choice between the
/// rolled `candidates` - nothing about the target item(s) is
/// mutated/consumed until `AdventureManager::choose_veil_outcome`
/// applies the picked one. Purely in-memory (see
/// `AdventureManager::pending_veils`).
#[derive(Debug, Clone)]
pub struct PendingVeil {
    pub action: PendingVeilAction,
    pub candidates: Vec<VeilCandidate>,
    /// `CraftAction::Chancing` only (2026-08-17) - empty for every other
    /// action. Chancing rerolls EVERY existing affix slot's TYPE, one slot
    /// at a time, reusing this SAME generic veil panel for each step
    /// instead of a separate wizard - this is the queue of affix types not
    /// yet rerolled this pass. When a Chancing candidate is picked and
    /// this is non-empty, `choose_veil_outcome` pops the next type, rolls
    /// its own 3 replacement candidates, and re-inserts a fresh
    /// `PendingVeil` for it (see `VeilChosenOutcome::ChancingContinues`) -
    /// same panel, next slot, no popup until the last one.
    pub chancing_remaining: Vec<Affix>,
    /// `CraftAction::Chancing` only - (old_affix, new_affix, new_value)
    /// for every slot already committed earlier in this same multi-slot
    /// pass, so the FINAL step's popup can summarize every slot that
    /// changed, not just the last one.
    pub chancing_committed: Vec<(Affix, Affix, f64)>,
}


#[cfg(test)]
mod cost_curve_tests {
    use super::*;

    /// The shipped curve, at the tiers that matter. Tier is `1 + stage/5`
    /// (see `generate_item`), so stage 1000 is tier 201 - the numbers
    /// below are the ones in the 2026-09-02 cost table the owner approved,
    /// and this test is what stops them drifting silently.
    #[test]
    fn tier_surcharge_matches_the_approved_cost_table() {
        for (tier, expected) in [(1u32, 3u64), (5, 18), (10, 38), (20, 81), (50, 222), (100, 476), (150, 743), (201, 1025)] {
            assert_eq!(tier_surcharge(tier, CRAFT_TIER_EXPONENT), expected, "tier {tier} surcharge");
        }
    }

    /// The base half of the same table - Transmute 250 -> 25, Krangle
    /// 2500 -> 250, and a full craft's total price at three tiers.
    #[test]
    fn a_whole_transmute_costs_what_the_approved_table_says() {
        let m = CRAFT_BASE_COST_MULT;
        assert_eq!(scaled_base_cost(250, m), 25);
        assert_eq!(scaled_base_cost(2500, m), 250);
        for (tier, expected) in [(1u32, 28u64), (10, 63), (100, 501), (201, 1050)] {
            assert_eq!(scaled_base_cost(250, m) + tier_surcharge(tier, CRAFT_TIER_EXPONENT), expected, "Transmute at tier {tier}");
        }
    }

    /// Exponent 1.0 is exactly the pre-2026-09-02 linear curve, and
    /// multiplier 10.0 is exactly the pre-cut base price. Both bounds are
    /// "put it back" bounds, and this is what makes that literally true.
    #[test]
    fn the_bounds_restore_the_old_curve_exactly() {
        for tier in [1u32, 7, 50, 201, 999] {
            assert_eq!(tier_surcharge(tier, CRAFT_TIER_EXPONENT_MIN), tier as u64 * TIER_CRAFT_DUST_COST, "tier {tier} at exponent 1.0");
        }
        // 1.0, NOT the ceiling: the multiplier scales the UNCHANGED shipped
        // constants, so the pre-cut price is the multiplier-of-1 price and
        // `CRAFT_BASE_COST_MULT_MAX` is ten times it.
        for base in [250u64, 500, 750, 800, 1000, 1250, 2500, VEIL_EXTRA_COST] {
            assert_eq!(scaled_base_cost(base, 1.0), base, "base {base} at multiplier 1 must be the pre-cut price");
            assert_eq!(scaled_base_cost(base, CRAFT_BASE_COST_MULT_MAX), base * 10, "base {base} at the ceiling is 10x the pre-cut price");
        }
    }

    /// The order of operations is load-bearing: the exponent applies to
    /// the TIER, not to the product, and the ceil comes last. Two
    /// plausible mis-writings of the same formula produce different
    /// numbers, and this pins which one is the price.
    #[test]
    fn tier_surcharge_is_the_exponent_of_the_tier_not_of_the_product() {
        let (mult, exp) = (TIER_CRAFT_DUST_COST as f64, CRAFT_TIER_EXPONENT);
        let tier = 10.0_f64;
        let correct = tier_surcharge(10, CRAFT_TIER_EXPONENT);
        let exponent_on_the_product = (mult * tier).powf(exp).ceil() as u64;
        let ceil_before_the_multiply = mult as u64 * tier.powf(exp).ceil() as u64;
        assert_eq!(correct, 38);
        assert_eq!(exponent_on_the_product, 43, "(3 x 10)^1.1 - a wrong reading that overcharges");
        assert_eq!(ceil_before_the_multiply, 39, "3 x ceil(10^1.1) - a wrong reading that rounds too early");
        assert_ne!(correct, exponent_on_the_product);
        assert_ne!(correct, ceil_before_the_multiply);
    }

    /// A nonzero base fee can never round away to nothing - the rounding
    /// is `ceil`, not `round`, at every step. The ONLY zero is the one an
    /// operator asks for explicitly, and even then the per-tier surcharge
    /// keeps a real craft costing dust.
    #[test]
    fn a_nonzero_base_cost_can_never_round_down_to_free() {
        for mult in [0.004, 0.001, 0.000_1, f64::MIN_POSITIVE] {
            assert_eq!(scaled_base_cost(250, mult), 1, "a base of 250 at multiplier {mult} must still cost 1 dust");
        }
        assert_eq!(scaled_base_cost(250, 0.0), 0, "exactly zero is the operator's deliberate choice and IS honoured");
        // ...but the craft still is not free, because the tier term stands.
        assert_eq!(tier_surcharge(1, CRAFT_TIER_EXPONENT), TIER_CRAFT_DUST_COST);
        for exp in [CRAFT_TIER_EXPONENT_MIN, CRAFT_TIER_EXPONENT, CRAFT_TIER_EXPONENT_MAX] {
            assert!(tier_surcharge(1, exp) >= TIER_CRAFT_DUST_COST, "a tier-1 craft must never be free at exponent {exp}");
        }
    }

    /// The `u64::MAX` "never affordable in dust" sentinel that
    /// `CelestialShard`/`UniqueShard` carry is NOT a price and must never
    /// enter the arithmetic - multiplying it would silently turn an
    /// unaffordable action into an affordable one (0.1 x u64::MAX is
    /// ~1.8e18 dust, which is a number a long-lived character could
    /// conceivably reach) and exponentiating it would overflow.
    #[test]
    fn the_shard_sentinel_is_untouched_by_either_dial_at_every_extreme() {
        for action in [CraftAction::CelestialShard, CraftAction::UniqueShard] {
            assert_eq!(action.base_cost(), u64::MAX, "{action:?} must still carry the sentinel");
            for mult in [CRAFT_BASE_COST_MULT_MIN, CRAFT_BASE_COST_MULT, CRAFT_BASE_COST_MULT_MAX, f64::NAN, f64::INFINITY, -1.0, 1.0e9] {
                assert_eq!(scaled_base_cost(action.base_cost(), mult), u64::MAX, "{action:?} at multiplier {mult}");
            }
        }
    }

    /// Out-of-range and non-finite readings never reach the formula. The
    /// FORM reports them (min/max on the rendered input - see
    /// `/admin/tunables`); this is the defence-in-depth behind a
    /// hand-crafted POST, same discipline as `pacing::sanitize_pool_cap`.
    #[test]
    fn out_of_range_dials_are_sanitised_before_they_can_price_anything() {
        assert_eq!(sanitize_craft_base_cost_mult(f64::NAN), CRAFT_BASE_COST_MULT);
        assert_eq!(sanitize_craft_base_cost_mult(f64::INFINITY), CRAFT_BASE_COST_MULT);
        assert_eq!(sanitize_craft_base_cost_mult(-5.0), CRAFT_BASE_COST_MULT_MIN);
        assert_eq!(sanitize_craft_base_cost_mult(1.0e9), CRAFT_BASE_COST_MULT_MAX);
        assert_eq!(sanitize_craft_tier_exponent(f64::NAN), CRAFT_TIER_EXPONENT);
        assert_eq!(sanitize_craft_tier_exponent(0.0), CRAFT_TIER_EXPONENT_MIN, "sub-1 clamps up to linear, never below");
        assert_eq!(sanitize_craft_tier_exponent(0.5), CRAFT_TIER_EXPONENT_MIN);
        assert_eq!(sanitize_craft_tier_exponent(9.0), CRAFT_TIER_EXPONENT_MAX);
        // A NaN dial must price like the default, not like free.
        assert_eq!(tier_surcharge(10, f64::NAN), tier_surcharge(10, CRAFT_TIER_EXPONENT));
        assert_eq!(scaled_base_cost(250, f64::NAN), scaled_base_cost(250, CRAFT_BASE_COST_MULT));
    }

    /// The `LiveTunables` defaults are the shipped constants, so a fresh
    /// install and a `Default::default()` price identically. Twin of
    /// `pacing`'s `default_pool_cap_matches_the_shipped_constant`.
    #[test]
    fn default_craft_dials_match_the_shipped_constants() {
        let t = LiveTunables::default();
        assert_eq!(t.craft_base_cost_mult, CRAFT_BASE_COST_MULT);
        assert_eq!(t.craft_tier_exponent, CRAFT_TIER_EXPONENT);
        assert_eq!(t.craft_tier_bump_mult, CRAFT_TIER_BUMP_MULT);
    }

    /// The bands are the designed shape and the dial scales them as a
    /// whole. At the shipped 1.0 the numbers are exactly what the initial
    /// commit wrote, which is what makes this release a no-op on live
    /// play.
    #[test]
    fn the_tier_bump_bands_are_unchanged_at_the_shipped_multiplier() {
        for (tier, expected) in [(1u32, 3u32), (24, 3), (25, 2), (49, 2), (50, 1), (100, 1), (5000, 1)] {
            assert_eq!(craft_tier_bump(tier, CRAFT_TIER_BUMP_MULT), expected, "band at tier {tier}");
        }
    }

    /// Zero must genuinely switch per-craft tier growth off - it is the
    /// setting an operator reaches for to watch `craft_tier_exponent` in
    /// isolation, and a bump of 1 sneaking through would defeat that.
    #[test]
    fn a_zero_multiplier_switches_tier_growth_off_in_every_band() {
        for tier in [1u32, 24, 25, 49, 50, 200] {
            assert_eq!(craft_tier_bump(tier, 0.0), 0, "tier {tier} must not grow at multiplier 0");
        }
    }

    /// `round`, not `ceil` - the opposite of `scaled_base_cost`, on
    /// purpose. A fractional multiplier has to be able to reach zero, or
    /// the bottom of the range becomes a cliff instead of a fade.
    #[test]
    fn the_bump_rounds_rather_than_ceils_so_the_low_end_fades_out() {
        assert_eq!(craft_tier_bump(1, 0.5), 2, "3 x 0.5 = 1.5 rounds to 2");
        assert_eq!(craft_tier_bump(25, 0.5), 1, "2 x 0.5 = 1 exactly");
        assert_eq!(craft_tier_bump(50, 0.5), 1, "1 x 0.5 = 0.5 rounds up to 1");
        assert_eq!(craft_tier_bump(50, 0.4), 0, "1 x 0.4 = 0.4 rounds DOWN to 0 - ceil would pin this at 1 forever");
        assert_eq!(craft_tier_bump(1, 0.1), 0, "3 x 0.1 = 0.3 rounds to 0");
    }

    /// The ceiling, and the reason it is where it is: at 3.0 the fastest
    /// band is +9, so one Hideout Warrior click (5 crafts) takes a fresh
    /// item from tier 1 to roughly tier 40.
    #[test]
    fn the_ceiling_is_nine_tiers_a_craft_and_a_hideout_warrior_click_lands_near_forty() {
        assert_eq!(craft_tier_bump(1, CRAFT_TIER_BUMP_MULT_MAX), 9);
        let mut tier = 1u32;
        for _ in 0..5 {
            tier += craft_tier_bump(tier, CRAFT_TIER_BUMP_MULT_MAX);
        }
        assert_eq!(tier, 40, "five crafts at the ceiling");
        // ...against the shipped multiplier's own five-step climb.
        let mut tier = 1u32;
        for _ in 0..5 {
            tier += craft_tier_bump(tier, CRAFT_TIER_BUMP_MULT);
        }
        assert_eq!(tier, 16, "five crafts at the default");
    }

    /// Out-of-range and non-finite readings never reach the growth, same
    /// discipline as the two cost dials.
    #[test]
    fn out_of_range_bump_multipliers_are_sanitised() {
        assert_eq!(sanitize_craft_tier_bump_mult(f64::NAN), CRAFT_TIER_BUMP_MULT);
        assert_eq!(sanitize_craft_tier_bump_mult(f64::INFINITY), CRAFT_TIER_BUMP_MULT);
        assert_eq!(sanitize_craft_tier_bump_mult(-1.0), CRAFT_TIER_BUMP_MULT_MIN);
        assert_eq!(sanitize_craft_tier_bump_mult(99.0), CRAFT_TIER_BUMP_MULT_MAX);
        assert_eq!(craft_tier_bump(1, f64::NAN), craft_tier_bump(1, CRAFT_TIER_BUMP_MULT));
    }
}
