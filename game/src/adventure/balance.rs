use super::*;

/// One entry in `adventure-item-balance.toml`'s `[affixes.*]` tables - a
/// SPARSE override of `AffixDef`'s `default_per_tier`/`default_weight`.
/// Both fields `Option` so a TOML block can override just one without
/// respecifying the other; a key absent entirely (or the whole file
/// missing/corrupt) means "use the code default."
#[derive(Debug, Default, Deserialize)]
pub(crate) struct AffixOverride {
    pub(crate) per_tier: Option<f64>,
    pub(crate) weight: Option<f64>,
}

/// One entry in `[cooldown.*]` - overrides `Item::cooldown_ms`'s curve for
/// that key ("helm" or "default", see `cooldown_curve`). Both fields
/// `Option` for the same sparse-override reason as `AffixOverride`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CooldownOverride {
    pub(crate) base_ms: Option<f64>,
    pub(crate) per_tier_ms: Option<f64>,
    pub(crate) floor_ms: Option<f64>,
}

/// Deserialized shape of `adventure-item-balance.toml`. `affixes` is
/// keyed by `Affix`'s own camelCase serde representation (e.g.
/// "damageReduction") - NOT the Rust variant name - so it matches the
/// casing already used in `adventure-characters.json`. String keys
/// (rather than `Affix` directly) sidestep needing enum-as-TOML-map-key
/// support; each key is resolved through `Affix`'s own `Deserialize` impl
/// in `affix_balance` below instead of a hand-written name table.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ItemBalanceFile {
    #[serde(default)]
    pub(crate) affixes: HashMap<String, AffixOverride>,
    /// Keyed by `CraftAction`'s own lowercase serde name (e.g.
    /// "transmute") - only the 6 real dust-priced actions are
    /// overridable, see `craft_action_def`'s doc.
    #[serde(default)]
    pub(crate) craft_action_cost: HashMap<String, u64>,
    /// Keyed by `EquipSlot`'s own lowercase serde name (e.g. "weapon") -
    /// overrides `base_power_for_slot`.
    #[serde(default)]
    pub(crate) slot_base_power: HashMap<String, f64>,
    /// Same keying as `slot_base_power` - overrides `compute_power`'s
    /// per-slot cap (only "gloves" has one today, at 0.55).
    #[serde(default)]
    pub(crate) slot_power_cap: HashMap<String, f64>,
    /// Keyed by "helm" or "default" (every non-Helm slot shares the
    /// "default" curve) - overrides `Item::cooldown_ms`.
    #[serde(default)]
    pub(crate) cooldown: HashMap<String, CooldownOverride>,
}

pub(crate) const ITEM_BALANCE_PATH: &str = "adventure-item-balance.toml";

/// Fail-soft load of the balance override file - missing or unparseable
/// just means "no overrides," logged, never a boot failure. A live
/// Twitch bot should never fail to start over a balance-file typo.
pub(crate) fn load_item_balance_file() -> ItemBalanceFile {
    match std::fs::read_to_string(data_path(ITEM_BALANCE_PATH)) {
        Ok(contents) => match toml::from_str::<ItemBalanceFile>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::warn!("{ITEM_BALANCE_PATH} failed to parse, using built-in defaults: {err}");
                ItemBalanceFile::default()
            }
        },
        Err(_) => ItemBalanceFile::default(),
    }
}

