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
pub const VEIL_EXTRA_COST: u64 = 500;

/// A nominal per-tier surcharge on EVERY craft action (2026-08-15, per a
/// live request) - 3 dust x the item's CURRENT tier, added on top of the
/// action's own `base_cost()` (and the veil surcharge above, if veiled).
/// Waived by a craft token same as everything else (see `craft_item`) -
/// a token craft stays entirely free, not just base-cost-free.
pub const TIER_CRAFT_DUST_COST: u64 = 3;

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

