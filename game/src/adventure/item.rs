use super::*;

/// The five gear slots — one item each. A drop auto-equips into an empty
/// slot (see `run_encounter`'s loot roll) or lands in the bag otherwise;
/// a player can also manually equip/unequip/swap any bag item from the
/// web dashboard's `/inventory` page (see `Character::equip_from_inventory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum EquipSlot {
    Weapon,
    Helm,
    Body,
    Gloves,
    Boots,
}

/// Backward-compat default for items saved before durability existed —
/// gives them a normal (not indestructible) lifespan rather than silently
/// making every pre-existing item immortal.
pub(crate) fn default_max_uses() -> Option<u32> {
    Some(10)
}


/// The range a brand-new item's `power_roll` (see `Item::power_roll`) is
/// drawn from - fixed for that item's whole lifetime once rolled (a
/// reforge reuses the existing roll rather than drawing a new one, see
/// `reforge_equipped_item`), so gaining tiers can never make an item
/// weaker than it already was. Also what `Item::quality_percent` measures
/// against for the web dashboard's Quality display.
pub const POWER_ROLL_RANGE: std::ops::Range<f64> = 0.85..1.2;

/// Backward-compat default for items saved before `power_roll` existed -
/// 1.0 is the middle-ish of `POWER_ROLL_RANGE`, a fair neutral guess with
/// no way to recover what an old item's actual original roll was.
pub(crate) fn default_power_roll() -> f64 {
    1.0
}

/// A floating-point round-trip through `raw_jitter = value / mult / base`
/// (see `polish_eligible_affixes`) can come back a hair below the exact
/// `POWER_ROLL_RANGE.end` it was originally set to (e.g. 1.1499999999999998,
/// not 1.15 on the nose) - `f64::EPSILON` (~2.2e-16) is far too tight a
/// tolerance to absorb that. This is several orders of magnitude looser
/// than machine epsilon, but still tiny next to `Character::polish`'s own
/// 0.05 quality step, so it can never mistake a genuinely-improvable affix
/// for a maxed one.
pub(crate) const MAX_JITTER_TOLERANCE: f64 = 1e-6;

/// Which of `item.affixes` indices still have real room to climb from
/// Polishing (see `Character::polish`) - factored out so the actual polish
/// roll and `Item::has_polish_room`'s cheap yes/no check (used both
/// server-side, to refuse charging sand for a no-op - and client-side, via
/// a data attribute, to grey out the Polishing button) can never drift out
/// of sync with each other.
pub(crate) fn polish_eligible_affixes(item: &Item) -> Vec<usize> {
    let mult = if item.perfect { PERFECT_QUALITY_MULT } else { 1.0 };
    item.affixes
        .iter()
        .enumerate()
        .filter_map(|(i, &(affix, value))| {
            let base = affix_base_value(affix, item.tier);
            if base <= 0.0 {
                return None;
            }
            let raw_jitter = (value / mult / base).clamp(POWER_ROLL_RANGE.start, POWER_ROLL_RANGE.end);
            (raw_jitter < POWER_ROLL_RANGE.end - MAX_JITTER_TOLERANCE).then_some(i)
        })
        .collect()
}

/// A modifier from the "unique crafting" mechanic (see
/// `CraftAction::CelestialShard`) - a completely separate pool from
/// `Affix`, never rolled naturally on a drop/reforge/recombine/normal
/// currency craft, only ever granted by its own dedicated unique-crafting
/// material. Lives in `Item::unique_affix` (its own field, outside the
/// normal `affixes` Vec and its 4-modifier cap entirely) and is shown
/// ABOVE an item's tier in the UI, the same "implicit" treatment an ARPG
/// gives a unique item's built-in effect. A character can have at most
/// one EQUIPPED item carrying any given `UniqueAffix` at a time (see
/// `Character::has_conflicting_unique_affix`) - two Celestial Shard
/// items would just be redundant otherwise, not a stronger stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UniqueAffix {
    /// Celestial Shard's effect - see `CELESTIAL_CONVERSION_PCT`.
    /// Role-dependent (2026-08-16 live request): a Heal-role wielder gets
    /// the original heal-triggered bonus damage (`simulate_battle`'s
    /// heal-application block, in `manager.rs`); any other archetype
    /// instead gets a same-target follow-up hit on every landed strike
    /// (the DPS-side Celestial Shard block in `apply_hit`, `combat.rs`).
    CelestialConversion,
    /// Unique Shard's effect (2026-08-17) - lets the wielder invest passive
    /// points into a SECOND `Archetype`'s tree, chosen via a dropdown on
    /// `/passives`, spent from the same shared point pool. Also grants
    /// bonus points: a flat +1 for having it equipped, plus +1 more per
    /// full 300 tiers on the specific item carrying it (see
    /// `Character::total_passive_points`). Unlike Celestial Conversion,
    /// this has no combat-time effect of its own - its whole value is the
    /// second tree it unlocks; see `Character::effective_secondary_archetype`/
    /// `secondary_passive_allocations`.
    SplitPersonality,
}

/// Every `UniqueAffix` variant - what `AdventureManager::craft_item_ex`'s
/// `CraftAction::UniqueShard` picker offers one candidate per (2026-08-19,
/// Unified Unique Shards). Data-driven deliberately: a future 3rd variant
/// only needs its own `name()`/`description()` arm plus whatever combat
/// effect it implements - the picker itself (candidate building,
/// `choose_veil_outcome`'s apply, `render_veil_choice_card`'s display)
/// needs no changes, since all three already iterate/branch on
/// `Option<UniqueAffix>` generically rather than naming a variant.
pub const ALL_UNIQUE_AFFIXES: [UniqueAffix; 2] = [UniqueAffix::CelestialConversion, UniqueAffix::SplitPersonality];

/// Fraction of a heal `UniqueAffix::CelestialConversion` also deals as
/// bonus damage to a random enemy - flat, not tier-scaled (unlike normal
/// affixes) since a unique affix's whole point is being a fixed, iconic
/// effect rather than a rollable magnitude.
pub const CELESTIAL_CONVERSION_PCT: f64 = 0.10;

impl UniqueAffix {
    pub fn name(self) -> &'static str {
        match self {
            UniqueAffix::CelestialConversion => "Celestial Conversion",
            UniqueAffix::SplitPersonality => "Split Personality",
        }
    }

    /// Player-facing blurb - shown above the tier line on any item that
    /// has this, same "implicit" placement convention as the doc above.
    /// Role-dependent effect (see `apply_hit`'s DPS-side Celestial Shard
    /// block), but this fn only ever gets a bare `&Item` at its call
    /// sites (`unique_affix_html`), not the wielding `Character` - stating
    /// both branches in one line is simpler than threading archetype
    /// context through every item-card renderer just for tooltip text.
    pub fn description(self) -> String {
        match self {
            UniqueAffix::CelestialConversion => {
                let pct = CELESTIAL_CONVERSION_PCT * 100.0;
                format!("Healers: {pct:.0}% of your healing is also dealt as damage to a random enemy. Others: {pct:.0}% of each landed hit strikes the same target again.")
            }
            UniqueAffix::SplitPersonality => {
                "Pick a 2nd class on /passives and invest your same shared points into its tree too. Grants +1 passive point, plus +1 more per 300 tiers on this item.".to_string()
            }
        }
    }
}

/// Which crit source tagged a `crit_bonus_affixes` entry - see that
/// field's doc. Only ever compared, never displayed differently per
/// variant (both render identically in `gear_stat_line`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CritSource {
    Reforge,
    Recombine,
}

/// One dropped/equipped item. What `power` means depends on `slot` — see
/// `Character`'s combat-stat getters (weapon/body/gloves) and
/// `simulate_battle`'s helm/boots skill handling. Boots are "a periodic
/// skill" (self-heal), so `power` there is the per-proc magnitude and
/// `cooldown_ms` is how often it can fire. Helm is similar but stacking
/// rather than a one-off proc - `power` is the dps added PER STACK and
/// `cooldown_ms` is how often another stack accumulates (see
/// `Character::helm_skill`).
///
/// `power` is the item's full, undecayed strength as rolled — always used
/// for display and for "is this new drop an upgrade" comparisons.
/// Everywhere the item actually affects combat, use `effective_power()`
/// instead, which factors in wear (see `max_uses`/`uses`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Unique per-instance id (random hex, assigned at drop/reforge time) -
    /// how the web dashboard's equip/unequip/delete actions address one
    /// specific item among possibly-identical-looking others in a bag.
    /// Empty string on items saved before this existed (all already
    /// equipped, never sitting in a bag needing individual addressing, so
    /// this is harmless) — never actually shown to anyone.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub slot: EquipSlot,
    pub tier: u32,
    pub power: f64,
    /// Where in `POWER_ROLL_RANGE` this item's base stat (`power` above)
    /// landed - rolled once when the item is first generated and fixed
    /// for its whole lifetime after that, including every future reforge
    /// (see `reforge_equipped_item`, which reuses this instead of rolling
    /// fresh). Reforging always raises `tier`; reusing the same roll here
    /// is what guarantees `power` can only ever go up alongside it, never
    /// down - a live report confirmed a helm could reforge +3 tiers and
    /// come out weaker before this existed, because tier and jitter used
    /// to be independent rolls every time. See `quality_percent` for the
    /// player-facing version of this.
    #[serde(default = "default_power_roll")]
    pub power_roll: f64,
    /// `None` = indestructible (a rare 1% roll — never decays, and
    /// per the request, is never auto-replaced by a new drop, only by a
    /// future manual !equip-type command). `Some(n)` = worn down over `n`
    /// fights of actual use, reaching zero right at the nth - it then
    /// just sits at 0 effective power (see `effective_power`) rather than
    /// being destroyed/unequipped; repairing it back to full is a planned
    /// follow-up, not built yet.
    #[serde(default = "default_max_uses")]
    pub max_uses: Option<u32>,
    /// How many fights this item has actually been used in so far — see
    /// `run_encounter`'s post-fight decay step.
    #[serde(default)]
    pub uses: u32,
    /// Secondary rolls on top of the slot's primary stat above - see
    /// `Affix`/`roll_affixes`. Usually one entry; a rare roll (see
    /// `roll_affixes`) can add up to three more, always distinct types.
    /// Empty on items saved before this existed - same "just doesn't
    /// have the bonus yet" backward compat as an old item's missing
    /// `max_uses`/`uses` fields.
    #[serde(default)]
    pub affixes: Vec<(Affix, f64)>,
    /// Set by Krangle (see `CraftAction::Krangle`) - once true, the item
    /// is permanently excluded from every crafting action (the currency
    /// crafts, Recombine as either input, Reforge's random-equipped-item
    /// pool) but stays fully equippable, repairable, and disenchantable.
    /// `false` for every item saved before Krangle existed - same
    /// harmless "doesn't have it yet" backward compat as `affixes`.
    #[serde(default)]
    pub locked: bool,
    /// Custom name a player gave this item after Krangling it (see
    /// `AdventureManager::name_item`) - shown as `Name "Nickname"`
    /// wherever the item's name is displayed (see `display_name`).
    /// `None` means "never asked or decided yet" - the dashboard keeps
    /// prompting for a name while a locked item has this unset (see
    /// `render_nickname_prompt`); `Some("")` means they were asked and
    /// chose to skip, so it stops asking. Only ever settable on a
    /// locked item - Krangle is deliberately the only way to earn a
    /// nickname prompt.
    #[serde(default)]
    pub nickname: Option<String>,
    /// Player-set "don't disenchant this" flag - a tick-box on the bag's
    /// own item card, completely separate from `locked` (Krangle) -
    /// protects an item the player just doesn't want to accidentally
    /// destroy, whether or not it's ever been Krangled. Blocks BOTH the
    /// single-item Disenchant button/action AND Disenchant All (see
    /// `disenchant_from_inventory`/`disenchant_all_from_inventory`) -
    /// unlike Krangle's `locked`, it has no effect on crafting/equip/
    /// repair at all, purely a disenchant guard.
    #[serde(default)]
    pub disenchant_protected: bool,
    /// A `UniqueAffix` from the Celestial Shard-style "unique crafting"
    /// mechanic (see `CraftAction::CelestialShard`) - deliberately a
    /// SEPARATE slot from `affixes`, outside the normal 4-modifier cap
    /// entirely (an item can have 4 normal affixes AND this). `None` for
    /// every ordinary item, same harmless "doesn't have it yet" backward
    /// compat as `locked`/`disenchant_protected`. An item with this set
    /// can never be Krangled (see `craftable_affix_pool`'s Krangle
    /// branch) - unique and locked are mutually exclusive states.
    #[serde(default)]
    pub unique_affix: Option<UniqueAffix>,
    /// Perfect Quality - a rare drop tier, distinct from a normal max
    /// `power_roll` (see `PERFECT_QUALITY_MULT`'s doc for how the bonus
    /// is applied). `false` for every ordinary item, same harmless
    /// "doesn't have it yet" backward compat as `unique_affix`/`locked`.
    /// Deliberately never inherited by Recombine (see
    /// `apply_recombine_roll`) - the child `Item` this produces just
    /// never has this field set, so it defaults to `false` regardless of
    /// whether either parent was Perfect.
    #[serde(default)]
    pub perfect: bool,
    /// Sacred item tier (2026-08-16, a live request) - a rarer-than-Perfect
    /// drop that's exactly like Perfect Quality (`perfect` is always also
    /// `true` alongside this) PLUS one extra `Affix` rolled at its own
    /// max jitter (see `make_item_sacred`), from the full `ALL_AFFIXES`
    /// pool regardless of this item's slot eligibility. Deliberately a
    /// SEPARATE field from `affixes` - outside the normal 4-modifier cap
    /// entirely and untouchable by every crafting action (Scour/Krangle/
    /// Augment/Regal/Exalt/Reforge/Recombine all only ever read or write
    /// `affixes`, never this field) - same "implicit, like Gloves' speed
    /// stat" shape the live request asked for, and same "crafting can't
    /// see it" precedent `unique_affix` already set. `None` for every
    /// ordinary item. Same "never inherited by Recombine" treatment as
    /// `perfect` (see that field's doc) - not copied in
    /// `apply_recombine_roll`, so the child item just never has it set.
    #[serde(default)]
    pub sacred_affix: Option<(Affix, f64)>,
    /// LEGACY (2026-08-16 to 2026-08-18) sticky item-level "used" gate -
    /// permanently `true` forever from the moment Reforge's crit first
    /// fired, even after the affix it granted was later removed by
    /// Annulment (or anything else) - see
    /// `migrations::migrate_crit_flag_to_affix_tracking`'s doc for the
    /// live report this caused (a tier-374 item, genuinely crit once,
    /// permanently soft-locked out of ever crit-ing again after that one
    /// bonus affix was Annulled away) and the fix. Kept ONLY so that
    /// migration can read existing save data once; nothing else may read
    /// this. `skip_serializing` so freshly-saved JSON stops carrying the
    /// old key forward once converted.
    #[serde(default, rename = "reforge_crit_used", skip_serializing)]
    pub(crate) legacy_reforge_crit_used: bool,
    /// Same legacy/migration-only role as `legacy_reforge_crit_used`, for
    /// Recombine's own separate crit.
    #[serde(default, rename = "recombine_crit_used", skip_serializing)]
    pub(crate) legacy_recombine_crit_used: bool,
    /// Legacy pre-2026-08-18 shape of `crit_bonus_affixes` (a plain list
    /// of types, no source tag) - migration-only, see
    /// `legacy_reforge_crit_used`'s doc.
    #[serde(default, rename = "crit_bonus_affixes", skip_serializing)]
    pub(crate) legacy_crit_bonus_affixes: Vec<Affix>,
    /// WHICH `affixes` entries were granted by a Reforge or Recombine
    /// crit, rather than a normal currency craft, and which source
    /// granted each one. This is now the ONLY source of truth for the
    /// once-per-lineage crit gates too (see `reforge_crit_used`/
    /// `recombine_crit_used` below) - unlike the old sticky bools, a gate
    /// reads as "used" only while a matching entry here still points at
    /// an affix that's ACTUALLY present in `affixes`. That means removing
    /// the affix - by Annulment, or by any future crafting method that
    /// doesn't even know this field exists - automatically re-opens the
    /// gate, with zero bookkeeping required at the removal site. (Contrast
    /// `chance_all_affixes`/`apply_chancing_reroll`, which DO need to
    /// explicitly migrate an entry here when a slot's TYPE changes in
    /// place - that's a deliberate "same slot, new flavor, still spent"
    /// case, not a removal.) At most one entry per source in practice
    /// (each source's own once-per-lineage nature), but Recombine can
    /// carry across up to one from EACH parent - see `roll_recombine`.
    #[serde(default, rename = "crit_bonus_affixes_v2")]
    pub crit_bonus_affixes: Vec<(Affix, CritSource)>,
}

impl Item {
    /// The name to actually show anywhere this item appears - its
    /// rolled `name` plus a quoted custom `nickname` (see
    /// `AdventureManager::name_item`) if one was ever given and isn't
    /// just the empty "skipped" sentinel.
    pub fn display_name(&self) -> String {
        match self.nickname.as_deref() {
            Some(nickname) if !nickname.is_empty() => format!("{} \"{nickname}\"", self.name),
            _ => self.name.clone(),
        }
    }

    /// The single definition of "may this item be mutated at all?", and
    /// the reason both halves of the guard can never disagree:
    /// `Character::find_mutable_item` (the mutable route every craft takes)
    /// and `Character::check_item_mutable` (the read-only gate the
    /// veiled-insert branches take) both ask this, so a rule change lands
    /// in one place instead of two that drift.
    ///
    /// `None` means "go ahead". The order matters slightly: `locked` is
    /// reported first because it is permanent and game-imposed, so telling
    /// a player about their own removable tick-box on a Krangled item
    /// would send them to a control that cannot help them.
    pub(crate) fn mutation_block(&self) -> Option<CraftError> {
        if self.locked {
            Some(CraftError::ItemLocked)
        } else if self.disenchant_protected {
            Some(CraftError::ItemProtected)
        } else {
            None
        }
    }

    /// Whether Reforge's rare bonus-affix crit is still "spent" for this
    /// item's lineage - see `crit_bonus_affixes`'s doc for why this is a
    /// derived check (present-in-`affixes`) rather than a stored flag.
    pub(crate) fn reforge_crit_used(&self) -> bool {
        self.crit_bonus_affixes.iter().any(|&(a, src)| src == CritSource::Reforge && self.affixes.iter().any(|&(ax, _)| ax == a))
    }

    /// Same derived-from-presence check as `reforge_crit_used`, for
    /// Recombine's own separate crit.
    pub(crate) fn recombine_crit_used(&self) -> bool {
        self.crit_bonus_affixes.iter().any(|&(a, src)| src == CritSource::Recombine && self.affixes.iter().any(|&(ax, _)| ax == a))
    }

    /// Whether `affix` is one of this item's crit-granted modifiers,
    /// regardless of which source granted it - `gear_stat_line` uses this
    /// to color that bullet, `craftable_affix_pool` uses it to exclude
    /// crit-bonus affixes from the normal-affix-count precondition.
    pub fn is_crit_bonus_affix(&self, affix: Affix) -> bool {
        self.crit_bonus_affixes.iter().any(|&(a, _)| a == affix)
    }

    /// Records that Reforge's crit just fired, tagging `affix` (already
    /// pushed onto `affixes` by the caller) as the crit-granted one.
    /// Drops any stale prior Reforge-tagged entry first - only reachable
    /// when `reforge_crit_used()` was false, i.e. either this item never
    /// crit before, or it did and that affix has since been removed, in
    /// which case the old dead entry would otherwise linger here forever.
    pub(crate) fn record_reforge_crit(&mut self, affix: Affix) {
        self.crit_bonus_affixes.retain(|&(_, src)| src != CritSource::Reforge);
        self.crit_bonus_affixes.push((affix, CritSource::Reforge));
    }

    /// Same "drop stale, tag fresh" bookkeeping as `record_reforge_crit`,
    /// for Recombine's own crit.
    pub(crate) fn record_recombine_crit(&mut self, affix: Affix) {
        self.crit_bonus_affixes.retain(|&(_, src)| src != CritSource::Recombine);
        self.crit_bonus_affixes.push((affix, CritSource::Recombine));
    }

    /// Where this item's fixed `power_roll` landed within
    /// `POWER_ROLL_RANGE`, as a 0-100% - purely cosmetic/informational
    /// (the roll itself never changes after the item is created, reforge
    /// included - see `power_roll`'s doc). Shown on the web dashboard
    /// right above Tier.
    pub fn quality_percent(&self) -> f64 {
        let span = POWER_ROLL_RANGE.end - POWER_ROLL_RANGE.start;
        ((self.power_roll - POWER_ROLL_RANGE.start) / span * 100.0).clamp(0.0, 100.0)
    }

    /// Whether `CraftAction::Polishing` could actually improve this item
    /// right now (2026-08-17, a live report: sand was being charged even
    /// when nothing was left to raise) - see `Character::polish`'s doc for
    /// the two paths this mirrors: a non-Perfect item can still improve via
    /// `power_roll` alone even with every affix maxed, but a Perfect item
    /// only ever improves through `polish_eligible_affixes`. Used both to
    /// refuse the craft server-side (`CraftError::NothingToPolish`) and to
    /// grey out the dashboard's Polishing button client-side (see
    /// `craft_item_options`' `data-polish-room` attribute).
    pub fn has_polish_room(&self) -> bool {
        if !self.perfect && self.power_roll < POWER_ROLL_RANGE.end - MAX_JITTER_TOLERANCE {
            return true;
        }
        !polish_eligible_affixes(self).is_empty()
    }

    /// Whether this item is at or above `tier`/`min_percent` on the
    /// Quality% < Perfect < Sacred ordering `AutoDisenchantTier` uses (see
    /// `Character::receive_item_with_auto_disenchant`) - `min_percent`
    /// only matters when `tier` is `Quality`; Perfect and Sacred items are
    /// always safe regardless of the picked percent, since they're
    /// categorically above the percent scale (same `sacred_affix.is_some()`
    /// `>` `perfect` `>` `quality_percent()` precedence the dashboard's own
    /// tier badge already uses).
    pub fn meets_auto_disenchant_floor(&self, tier: AutoDisenchantTier, min_percent: u32) -> bool {
        match tier {
            AutoDisenchantTier::Sacred => self.sacred_affix.is_some(),
            AutoDisenchantTier::Perfect => self.perfect,
            AutoDisenchantTier::Quality => self.perfect || self.quality_percent() >= min_percent as f64,
        }
    }

    /// Bumps this (already-Krangled) item's tier up to `level` if it's
    /// currently behind, rescaling `power` (via `compute_power`, the same
    /// formula a fresh item or a reforge uses) and every existing affix
    /// value (proportional to the tier ratio - exact, since
    /// `affix_base_value` is purely linear in tier with no constant
    /// term) to match. No-op, including no rescale, if the item's
    /// already at or above `level` - this only ever pulls a lagging item
    /// UP, never down. Used by `grow_krangled_items` (every level-up)
    /// and by Krangle's own lock step (see `Character::apply_craft_affix`) -
    /// a live report caught tier syncing to level while power/modifiers
    /// stayed frozen at their old values, making the tier number purely
    /// cosmetic; this is the fix for both call sites at once.
    /// Reapplies `PERFECT_QUALITY_MULT` to `power` when `self.perfect` -
    /// `compute_power` alone recomputes fresh at the new tier with no
    /// knowledge of Perfect Quality (same gap `reforge_equipped_item`'s
    /// `was_perfect` block already has to work around), so every tier
    /// sync was silently shaving the 20% Perfect bonus back off `power`
    /// specifically while the ratio-scaled affixes kept theirs -
    /// `quality_percent`/the dashboard's power display would drift back
    /// toward a non-Perfect item's numbers a little more with every
    /// Krangle level-up or crafted tier bump.
    pub(crate) fn sync_tier_to(&mut self, level: u32) {
        if self.tier >= level {
            return;
        }
        let old_tier = self.tier.max(1);
        self.tier = level;
        self.power = compute_power(self.slot, self.tier, self.power_roll);
        if self.perfect {
            self.power *= PERFECT_QUALITY_MULT;
        }
        let ratio = self.tier as f64 / old_tier as f64;
        for (_, value) in self.affixes.iter_mut() {
            *value *= ratio;
        }
        // Sacred's implicit affix scales the same proportional way as
        // every normal affix above - it's still tier-linear underneath
        // (see `make_item_sacred`), it just lives in a separate field.
        if let Some((_, value)) = self.sacred_affix.as_mut() {
            *value *= ratio;
        }
    }

    /// Only meaningful for Helm (stacking dps buff - see
    /// `Character::helm_skill`) and Boots (self-heal) — higher tier gear
    /// procs more often, floored so it never becomes a second normal
    /// attack cadence. Helm's curve is exactly half of Boots' (same
    /// shape, halved base/floor/per-tier slope) - the raw numbers on
    /// helms were cut 50% (cooldown AND magnitude - see
    /// `generate_item_at_tier`'s Helm base) when the skill became a
    /// stacking buff instead of a one-off burst, so it ramps up faster.
    pub fn cooldown_ms(&self) -> u32 {
        let curves = cooldown_curves();
        let curve = if self.slot == EquipSlot::Helm { &curves.helm } else { &curves.default };
        (curve.base_ms - self.tier as f64 * curve.per_tier_ms).max(curve.floor_ms).round() as u32
    }

    pub fn is_indestructible(&self) -> bool {
        self.max_uses.is_none()
    }

    /// 1.0 (full strength) down to 0.0 as `uses` approaches `max_uses` -
    /// the shared wear curve behind both `effective_power` and
    /// `effective_affix_total`, so a beat-up item's secondary stats fade
    /// out exactly in step with its primary one.
    pub(crate) fn decay_fraction(&self) -> f64 {
        match self.max_uses {
            None => 1.0,
            Some(0) => 0.0,
            Some(max_uses) => (1.0 - (self.uses as f64 / max_uses as f64)).max(0.0),
        }
    }

    /// Current real strength after wear - linearly ramps from full
    /// `power` down to 0 as `uses` approaches `max_uses` ("becoming
    /// weaker through its use"), indestructible gear stays at full power
    /// forever. This is what combat math and skill procs actually read;
    /// `power` alone is only the item's original/display strength.
    pub fn effective_power(&self) -> f64 {
        self.power * self.decay_fraction()
    }

    /// Same wear-decayed treatment as `effective_power`, but summed
    /// across every rolled affix of one `Affix` type (almost always 0 or
    /// 1 matches, occasionally more on a multi-affix roll - see
    /// `roll_affixes`) PLUS `sacred_affix` if it matches `affix` too -
    /// this is the ONE shared function every `Character::combat_*`
    /// aggregate getter (via `sum_affix`) routes through, so folding
    /// Sacred's implicit in here (rather than patching each getter
    /// individually) is what makes it actually affect combat instead of
    /// being a display-only implicit. Decays with wear the same as
    /// everything else here - same "implicit, like gloves' speed stat"
    /// treatment the live request asked for, and `power` (the OTHER
    /// implicit) already decays too.
    pub fn effective_affix_total(&self, affix: Affix) -> f64 {
        let decay = self.decay_fraction();
        let normal: f64 = self.affixes.iter().filter(|(a, _)| *a == affix).map(|(_, v)| v * decay).sum();
        let sacred = self.sacred_affix.filter(|(a, _)| *a == affix).map(|(_, v)| v * decay).unwrap_or(0.0);
        normal + sacred
    }

    /// How many times `affix` is rolled on this item - normal `affixes`
    /// (0 or 1: `roll_affixes`' `weighted_affix_pick` removes each type
    /// from the pool as it's picked, so a plain roll never duplicates one
    /// type on one item) plus 1 more if `sacred_affix` independently
    /// landed on the same type too (it's drawn from the full pool
    /// regardless of what's already rolled - see `make_item_sacred`'s own
    /// doc - so it CAN double up). Same "sacred counts as one more"
    /// precedent `disenchant_multiplier` already established just below,
    /// just counting instances instead of dust-tier steps. No decay
    /// weighting here, unlike `effective_affix_total` above - this is a
    /// presence count, not a value, and an item doesn't stop HAVING an
    /// affix as its durability wears down.
    pub fn affix_instance_count(&self, affix: Affix) -> u32 {
        let normal = self.affixes.iter().filter(|(a, _)| *a == affix).count() as u32;
        let sacred = if self.sacred_affix.map(|(a, _)| a) == Some(affix) { 1 } else { 0 };
        normal + sacred
    }

    /// Dust-value multiplier from this item's affix count - the item's
    /// primary slot stat (dps/hp/etc.) counts as its baseline modifier,
    /// so a totally plain item (0 affixes) is 1x, and EVERY rolled affix
    /// adds another flat 5x (additive, not compounding): 1x/5x/10x/15x/
    /// 20x for 0/1/2/3/4 affixes. `sacred_affix` counts as one more affix
    /// toward this same step (2026-08-16 fix, a live report: "Sacred
    /// items are returning less dust than Perfect items") - it was
    /// previously invisible here entirely, since it lives in its own
    /// field, not `affixes`, so a Sacred item disenchanted for EXACTLY
    /// what a Perfect item with the same normal affix count would,
    /// completely ignoring the extra maxed implicit that makes Sacred
    /// strictly the better item. Perfect Quality items (2026-08-15, a
    /// live request) get a further flat 5x on top of that (compounding,
    /// unlike the affix steps above - "a 5x dust multiplier for
    /// disenchanting" perfect items, stacked with whatever the affix
    /// count already earned, not a replacement for it) - Sacred is
    /// always also `perfect`, so it gets this too, ON TOP of the extra
    /// step its implicit already earned above. Used by
    /// `Character::disenchant_from_inventory` and the web dashboard's
    /// disenchant confirmation preview, so they can never drift out of
    /// sync with each other.
    pub fn disenchant_multiplier(&self) -> u32 {
        let affix_count = self.affixes.len() + if self.sacred_affix.is_some() { 1 } else { 0 };
        let base = (5 * affix_count as u32).max(1);
        if self.perfect { base * 5 } else { base }
    }

    /// 0-100 - what !character/the web dashboard both show.
    pub fn durability_percent(&self) -> Option<u32> {
        match self.max_uses {
            None => None,
            Some(0) => Some(0),
            Some(max) => Some((((max.saturating_sub(self.uses)) as f64 / max as f64) * 100.0).round() as u32),
        }
    }

    /// Dust cost to fully repair - scales with how much durability is
    /// actually missing, not a flat per-tier fee: a full repair (0%
    /// durability) costs `tier` dust, needing back only 20% costs 20% of
    /// that, always rounded up. Only meaningful when `needs_repair()` is
    /// true (0 otherwise, since there'd be nothing missing to charge for).
    pub fn repair_cost(&self) -> u64 {
        let missing_fraction = match self.max_uses {
            Some(max) if max > 0 => (self.uses as f64 / max as f64).min(1.0),
            _ => 0.0,
        };
        (self.tier as f64 * missing_fraction).ceil() as u64
    }

    /// Worn (not indestructible, and has actually taken at least one use)
    /// - indestructible and full-durability items have nothing to repair.
    pub fn needs_repair(&self) -> bool {
        self.max_uses.is_some() && self.uses > 0
    }

    /// Fully spent (0% durability, not indestructible) - see
    /// `Character::all_gear_worn_out`.
    pub fn is_fully_worn(&self) -> bool {
        matches!(self.max_uses, Some(max) if self.uses >= max)
    }


    /// Joined "+X% foo, +Y% bar" line for every affix this item rolled
    /// (see `roll_affixes`) - empty string for a pre-affix-system item
    /// (`affixes` defaults to empty on load, see `Item::affixes`).
    pub fn affix_label(&self) -> String {
        self.affixes.iter().map(|(a, v)| affix_display(*a, *v)).collect::<Vec<_>>().join(", ")
    }
}

/// What a won encounter's loot roll actually did - fed to main.rs's
/// already-delayed chat announcement (see `EncounterResult`) rather than
/// announced separately, so it lands at the same "fight's over" moment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LootDrop {
    pub display_name: String,
    pub item_name: String,
    pub slot: EquipSlot,
    pub outcome: ReceiveOutcome,
    /// The dropped item's own stats (2026-08-17, a live request: "fix
    /// loot log to record the stats from the item") - the `/fights` loot
    /// log used to show only the item's name/slot/outcome, nothing about
    /// what it actually rolled. Captured at drop time, before the item is
    /// handed off to `Character::receive_item`/`receive_item_with_auto_disenchant`
    /// (which consume it by value).
    #[serde(default)]
    pub tier: u32,
    #[serde(default)]
    pub affixes: Vec<(Affix, f64)>,
}

/// What happened to a newly-acquired item - see `Character::receive_item`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiveOutcome {
    /// The slot was empty, so it went straight on.
    Equipped,
    /// The slot was already occupied, so it's sitting in the bag instead.
    AddedToBag,
    /// The slot was occupied AND the bag was already full (150/150) - lost.
    BagFull,
    /// Auto-disenchant (2026-08-16, a live request) intercepted it before
    /// it could be equipped OR bagged - see
    /// `Character::receive_item_with_auto_disenchant`. Carries the dust
    /// granted, same as a manual disenchant would.
    AutoDisenchanted { dust: u32 },
}

/// The "safe" floor for auto-disenchant (2026-08-16, a live request: "a
/// checkbox with a dropdown... whatever is selected is safe and anything
/// above it in quality/rarity is safe") - orders as `Quality(min_percent)`
/// `<` `Perfect` `<` `Sacred`, matching `Item::meets_auto_disenchant_floor`'s
/// own precedence and the dashboard's existing tier-badge logic. Stored on
/// `Character` alongside a separate `auto_disenchant_min_percent: u32`
/// field (only meaningful when this is `Quality`) rather than carrying the
/// percent as enum data, so a plain `<select>` can drive the tier and a
/// plain `<input type="number">` can drive the percent independently on
/// the dashboard form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoDisenchantTier {
    #[default]
    Quality,
    Perfect,
    Sacred,
}

/// Why a repair attempt (see `Character::repair_equipped`/
/// `repair_inventory_item`) didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum RepairError {
    /// No such item equipped/in the bag.
    NoItem,
    /// Indestructible, or already at full durability.
    NotNeeded,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
}

/// Why a `Character::recombine`/`AdventureManager::recombine_gear`
/// attempt didn't go through.
#[derive(Debug, Clone, Copy)]
pub enum RecombineError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// One or both item ids don't currently exist on this character
    /// (already consumed, wrong owner, or never did).
    ItemNotFound,
    /// One or both items are locked (Krangled - see `Item::locked`),
    /// which excludes them from every crafting action.
    ItemLocked,
    /// The same item id was picked for both sides.
    SameItem,
    /// The two items aren't the same `EquipSlot`.
    SlotMismatch,
    /// Not enough dust — carries the cost that was needed.
    InsufficientDust(u64),
    /// Both items carry a unique affix and they're not the same one -
    /// uniques can't blend into each other on Recombine.
    IncompatibleUniqueAffixes,
    /// One or both items have the player's "Keep" tick-box set
    /// (`Item::disenchant_protected`) - see `CraftError::ItemProtected`
    /// for why this is separate from `ItemLocked`.
    ItemProtected,
}

/// Lets Recombine reuse the ONE mutation rule (`Item::mutation_block`)
/// instead of restating it in `RecombineError`'s own vocabulary - without
/// this, recombine's gate and every other craft's gate could disagree
/// about what "protected" means, which is exactly the drift the shared
/// rule exists to prevent. Only the two refusal kinds a mutation check can
/// produce are mapped; anything else means the caller passed an error this
/// conversion was never meant to see, and `ItemNotFound` is the honest
/// answer for "no item to speak of".
impl From<CraftError> for RecombineError {
    fn from(err: CraftError) -> Self {
        match err {
            CraftError::ItemLocked => RecombineError::ItemLocked,
            CraftError::ItemProtected => RecombineError::ItemProtected,
            _ => RecombineError::ItemNotFound,
        }
    }
}

/// Everything a recombine roll decides before either source item is
/// actually touched - see `Character::roll_recombine`/
/// `apply_recombine_roll`. Also what a veiled recombine's candidates
/// (see `PendingVeil`) are made of, so the exact roll the player sees
/// and picks is the exact one later committed - never re-rolled.
#[derive(Debug, Clone)]
pub struct RecombineRoll {
    pub slot: EquipSlot,
    pub new_tier: u32,
    pub was_indestructible: bool,
    /// Either source's `UniqueAffix`, if either had one - "always stays
    /// with the item" per the request, exactly the same carry-over
    /// treatment as `was_indestructible` right above (a unique affix is
    /// an intrinsic property, not a normal modifier competing for one of
    /// the 4 `affixes` slots, so it isn't part of that pool's
    /// guaranteed/optional/truncation logic at all). Stale-doc fix
    /// (2026-08-19): this comment used to claim BOTH sources carrying a
    /// DIFFERENT `UniqueAffix` was "currently impossible, with only one
    /// variant existing" and that item_a's would arbitrarily win - both
    /// claims were already wrong by the time `SplitPersonality` shipped
    /// (2026-08-17), and stayed wrong since: `Character::roll_recombine`
    /// actually REJECTS that combination outright
    /// (`RecombineError::IncompatibleUniqueAffixes`), it never reaches
    /// this field at all in that case. `unique_affix` here is only ever
    /// `item_a.unique_affix.or(item_b.unique_affix)` - well-defined
    /// (at most one source can be `Some` by the time this runs) regardless
    /// of how many `UniqueAffix` variants exist.
    pub unique_affix: Option<UniqueAffix>,
    pub affixes: Vec<(Affix, f64)>,
    pub bonus_affix: Option<Affix>,
    /// Which source item's `power_roll` (see that field's doc) the
    /// result inherits (see `roll_recombine`) - the higher of the two
    /// when veiled/guaranteed (certainty is the whole point of paying to
    /// veil), a straight 50/50 coin flip otherwise. Never an average or a
    /// fresh roll, same "carry an existing roll forward instead of
    /// re-rolling it" principle as reforge, just with two candidates
    /// instead of one.
    pub power_roll: f64,
    /// See `Item::crit_bonus_affixes` - carried over from either source
    /// (filtered down to whichever types actually made it into `affixes`
    /// above, same "must still be present" rule `Item::reforge_crit_used`/
    /// `recombine_crit_used` derive from), plus this roll's own
    /// `bonus_affix` if it fired. `reforge_crit_used()`/
    /// `recombine_crit_used()` below read this the same way `Item`'s own
    /// methods do - Recombine never grants a fresh Reforge-crit itself,
    /// only inherits whichever of that type survives the merge.
    pub crit_bonus_affixes: Vec<(Affix, CritSource)>,
}

impl RecombineRoll {
    pub(crate) fn reforge_crit_used(&self) -> bool {
        self.crit_bonus_affixes.iter().any(|&(a, src)| src == CritSource::Reforge && self.affixes.iter().any(|&(ax, _)| ax == a))
    }
}

/// Result of a successful recombination - see `Character::recombine`.
#[derive(Debug, Clone)]
pub struct RecombineOutcome {
    /// The freshly generated result item's id - lets a caller (e.g. the
    /// web dashboard's item picker, see `Character::last_crafted_item_id`)
    /// select it in a follow-up UI without needing to guess which of the
    /// character's items is the new one.
    pub item_id: String,
    pub item_name: String,
    pub slot: EquipSlot,
    pub new_tier: u32,
    /// Set on the rare (5%) recomb "crit" that adds a bonus modifier
    /// the retention rolls alone didn't grant - see `GearCritEvent`.
    pub bonus_affix: Option<Affix>,
}

/// What `AdventureManager::recombine_gear` handed back - `Applied` when
/// it went through immediately (the non-veiled, default path);
/// `PendingChoice` when it was veiled instead and is now waiting on
/// `AdventureManager::choose_veil_outcome` (see `PendingVeil`).
#[derive(Debug, Clone)]
pub enum RecombineResult {
    Applied(RecombineOutcome),
    PendingChoice,
}

/// A piece of gear that hit the end of its lifespan and now sits at 0
/// effective power (not destroyed/unequipped - repairing it back to full
/// is a planned follow-up, not built yet) - see
/// `run_encounter`'s post-fight decay step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenItem {
    pub display_name: String,
    pub item_name: String,
    pub slot: EquipSlot,
}

/// Result of a successful "Reforge Gear" redemption — see
/// `AdventureManager::reforge_random_gear`.
#[derive(Debug, Clone)]
pub struct ReforgeOutcome {
    pub item_name: String,
    pub slot: EquipSlot,
    pub old_tier: u32,
    pub new_tier: u32,
    /// Set on the rare (1%) reforge "crit" that rolls the new item a
    /// bonus modifier it wouldn't otherwise have gotten - see
    /// `reforge_equipped_item`/`GearCritEvent`.
    pub bonus_affix: Option<Affix>,
}

/// Result of a successful `CraftAction::DivineDust` apply/reroll - see
/// `Character::apply_divine_dust`. `became_sacred: true` (with
/// `old_affix: None`) is the "item wasn't sacred yet" path; `false` (with
/// `old_affix: Some(...)`) is a reroll of an already-sacred item's affix.
/// `new_affix`/`new_value` are set either way - always exactly one sacred
/// affix on the item afterward.
#[derive(Debug, Clone)]
pub struct DivineDustOutcome {
    pub item_name: String,
    pub slot: EquipSlot,
    pub tier: u32,
    pub became_sacred: bool,
    pub old_affix: Option<Affix>,
    pub new_affix: Affix,
    pub new_value: f64,
}

/// Result of a successful single-item disenchant — see
/// `Character::disenchant_from_inventory`. `dust_max` is what the same
/// item would've granted on the best possible roll (`6 * tier *
/// disenchant_multiplier()`, since the roll is 1-6) - `dust` is always
/// somewhere in `1..=dust_max` at the same step size, so `dust as f64 /
/// dust_max as f64` is exactly the roll's own percentile.
#[derive(Debug, Clone)]
pub struct DisenchantOutcome {
    pub item_name: String,
    pub dust: u32,
    pub dust_max: u32,
    /// Divine Dust granted alongside the dust roll above - always 0 for a
    /// non-Sacred item (see `roll_divine_dust_disenchant`).
    pub divine_dust: u64,
}

/// Which gear action a `GearCritEvent` came from - main.rs's subscriber
/// picks its wording off this.
#[derive(Debug, Clone, Copy)]
pub enum GearCritSource {
    Reforge,
    Recombine,
}

/// A rare bonus-modifier roll worth announcing in chat - fired by
/// `AdventureManager::reforge_random_gear`/`reforge_random_gear_for_dust`
/// (1% chance, see `reforge_equipped_item`) and `recombine_gear` (5%
/// chance, see `Character::recombine`). One shared event type/channel
/// for both since they're the same kind of moment - "gear rolled a
/// bonus it wasn't guaranteed to get" - just from two different actions.
#[derive(Debug, Clone)]
pub struct GearCritEvent {
    pub display_name: String,
    pub source: GearCritSource,
    pub item_name: String,
    pub slot: EquipSlot,
    pub tier: u32,
    pub affix: Affix,
}

pub const EQUIP_SLOTS: [EquipSlot; 5] = [EquipSlot::Weapon, EquipSlot::Helm, EquipSlot::Body, EquipSlot::Gloves, EquipSlot::Boots];

/// Random item for `slot`, scaled off the current world stage - higher
/// stages roll higher tiers, same "current progression, not raw level"
/// philosophy as `boss_stats_for`. Tier bands pick from small themed name
/// pools purely for flavor in the loot announcement.
pub(crate) fn generate_item(slot: EquipSlot, stage: u32, rng: &mut impl Rng) -> Item {
    let tier = (1 + stage / 5).max(1);
    generate_item_at_tier(slot, tier, rng)
}

/// Base per-tier magnitude for a slot's primary stat, before `power_roll`
/// and `tier` are applied - factored out of `generate_item_at_tier_with_roll`
/// so the one-time power_roll backfill migration (see
/// `AdventureManager::new`) can reverse-engineer an existing item's real
/// original roll from its already-persisted `power`, instead of
/// defaulting every pre-existing item to the same flat value.
/// Code default for `base_power_for_slot`, sparsely overridable via
/// `adventure-item-balance.toml`'s `[slot_base_power]` (see `slot_power`).
pub(crate) fn default_base_power_for_slot(slot: EquipSlot) -> f64 {
    match slot {
        // 3x'd (2026-08-16, a live request: "increase the implicit
        // damage contribution of all weapons by 3x across the board") -
        // weapon `power` is the only one of these 5 that feeds directly
        // into `Character::combat_atk` as a flat bonus "same for every
        // [combat] function" (see that fn's doc), i.e. the weapon's own
        // implicit damage contribution independent of its affix rolls -
        // scales every weapon uniformly regardless of tier/roll, same as
        // every other slot's own coefficient here already does.
        EquipSlot::Weapon => 12.0,
        EquipSlot::Helm => 4.0,
        EquipSlot::Body => 12.0,
        // 0.045 uncapped (see default_slot_power_cap's doc) turned out
        // way too strong once actually played with - a live report:
        // "at tier 95 I have 616% speed thats too much [...] reduce it
        // by a factor of about 5". Cut to 0.009 (2026-08-16) - still
        // linear/uncapped, just a 5x shallower slope, landing that same
        // tier-95 Perfect glove around 123% instead of 616%.
        EquipSlot::Gloves => 0.009,
        EquipSlot::Boots => 13.0,
    }
}

/// Code default for `compute_power`'s per-slot cap - `None` for slots
/// with no cap. Gloves USED TO cap here at a flat 0.55 (reached around
/// tier 12-13), on the theory that its speed fraction needed protecting
/// from blowing up `attack_interval_ms` - but that fn already divides by
/// `(1.0 + gear_bonus)` and floors the result at 50ms (see its own doc),
/// which is itself an asymptotic curve: ever-larger gear_bonus yields
/// ever-smaller marginal speedup, safely bounded by the 50ms floor no
/// matter how large gloves' `power` gets. The flat cap here was
/// redundant with that existing protection AND actively harmful - it
/// made gloves' speed stat dead-stop scaling entirely once reached
/// (unlike every other slot's power, which keeps growing with tier
/// forever) - removed 2026-08-16 per a live report. Gloves now scales
/// uncapped like every other slot; `attack_interval_ms`'s own division +
/// floor is the only (and sufficient) safety net.
pub(crate) fn default_slot_power_cap(_slot: EquipSlot) -> Option<f64> {
    None
}

/// Resolved (base_power, power_cap) for every `EquipSlot`, computed once
/// and cached - code defaults with `adventure-item-balance.toml`'s
/// `[slot_base_power]`/`[slot_power_cap]` overrides (if any) applied on
/// top.
pub(crate) static SLOT_POWER: std::sync::OnceLock<HashMap<EquipSlot, (f64, Option<f64>)>> = std::sync::OnceLock::new();

pub(crate) fn slot_power(slot: EquipSlot) -> (f64, Option<f64>) {
    SLOT_POWER.get_or_init(|| {
        let file = load_item_balance_file();
        let mut resolved: HashMap<EquipSlot, (f64, Option<f64>)> =
            EQUIP_SLOTS.iter().map(|&s| (s, (default_base_power_for_slot(s), default_slot_power_cap(s)))).collect();
        for (key, value) in file.slot_base_power {
            match EquipSlot::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&key)) {
                Ok(slot) => {
                    resolved.get_mut(&slot).expect("EQUIP_SLOTS covers every EquipSlot variant").0 = value;
                    tracing::info!("{ITEM_BALANCE_PATH}: slot_base_power.{key} overridden to {value}");
                }
                Err(_) => tracing::warn!("{ITEM_BALANCE_PATH}: unknown slot_base_power key '{key}', ignoring"),
            }
        }
        for (key, value) in file.slot_power_cap {
            match EquipSlot::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&key)) {
                Ok(slot) => {
                    resolved.get_mut(&slot).expect("EQUIP_SLOTS covers every EquipSlot variant").1 = Some(value);
                    tracing::info!("{ITEM_BALANCE_PATH}: slot_power_cap.{key} overridden to {value}");
                }
                Err(_) => tracing::warn!("{ITEM_BALANCE_PATH}: unknown slot_power_cap key '{key}', ignoring"),
            }
        }
        resolved
    })[&slot]
}

pub fn base_power_for_slot(slot: EquipSlot) -> f64 {
    slot_power(slot).0
}

/// An item's base stat for a given `tier`/`power_roll` combination -
/// what a fresh item generates with, what a reforge/Krangle-tier-growth
/// recomputes to when either changes, and what the one-time Krangled-item
/// accuracy pass (see `AdventureManager::new`) checks existing items
/// against. Single source of truth so all three stay consistent.
pub(crate) fn compute_power(slot: EquipSlot, tier: u32, power_roll: f64) -> f64 {
    let raw = base_power_for_slot(slot) * tier as f64 * power_roll;
    match slot_power(slot).1 {
        Some(cap) => raw.min(cap),
        None => raw,
    }
}

pub(crate) struct CooldownCurve {
    base_ms: f64,
    per_tier_ms: f64,
    floor_ms: f64,
}

/// `Item::cooldown_ms`'s two code-default curves - Helm's own (halved
/// base/floor/per-tier slope vs. every other slot - see `cooldown_ms`'s
/// doc for why) and the shared "default" curve every non-Helm slot uses.
pub(crate) struct ResolvedCooldownCurves {
    helm: CooldownCurve,
    default: CooldownCurve,
}

pub(crate) static COOLDOWN_CURVES: std::sync::OnceLock<ResolvedCooldownCurves> = std::sync::OnceLock::new();

pub(crate) fn cooldown_curves() -> &'static ResolvedCooldownCurves {
    COOLDOWN_CURVES.get_or_init(|| {
        let file = load_item_balance_file();
        let mut helm = CooldownCurve { base_ms: 4500.0, per_tier_ms: 175.0, floor_ms: 1750.0 };
        let mut default = CooldownCurve { base_ms: 9000.0, per_tier_ms: 350.0, floor_ms: 3500.0 };
        for (key, ov) in &file.cooldown {
            let curve = match key.as_str() {
                "helm" => &mut helm,
                "default" => &mut default,
                _ => {
                    tracing::warn!("{ITEM_BALANCE_PATH}: unknown cooldown key '{key}', ignoring");
                    continue;
                }
            };
            if let Some(v) = ov.base_ms {
                curve.base_ms = v;
            }
            if let Some(v) = ov.per_tier_ms {
                curve.per_tier_ms = v;
            }
            if let Some(v) = ov.floor_ms {
                curve.floor_ms = v;
            }
            // Same "section header alone shouldn't log as an override"
            // reasoning as affix_balance's identical guard.
            if ov.base_ms.is_some() || ov.per_tier_ms.is_some() || ov.floor_ms.is_some() {
                tracing::info!("{ITEM_BALANCE_PATH}: cooldown.{key} overridden");
            }
        }
        ResolvedCooldownCurves { helm, default }
    })
}

/// Same item generation, but at an explicit tier instead of one derived
/// from the world stage — used by the "reforge" channel points reward,
/// which deliberately jumps a specific item well above its current tier
/// rather than rolling at whatever the world's current progression is.
/// Rolls a brand-new `power_roll` (see `Item::power_roll`/
/// `POWER_ROLL_RANGE`) - only appropriate for an item that doesn't exist
/// yet. An actual reforge of an EXISTING item goes through
/// `generate_item_at_tier_with_roll` instead, carrying the old item's
/// roll forward so gaining tiers can't roll it into a weaker item.
pub(crate) fn generate_item_at_tier(slot: EquipSlot, tier: u32, rng: &mut impl Rng) -> Item {
    let power_roll = rng.gen_range(POWER_ROLL_RANGE);
    generate_item_at_tier_with_roll(slot, tier, power_roll, rng)
}

/// Same as `generate_item_at_tier`, but with an explicit `power_roll`
/// instead of drawing a fresh one - see that field's doc for why
/// `reforge_equipped_item` needs this instead of the plain version.
/// Chance a freshly-generated item rolls indestructible (never decays,
/// never auto-replaced - `max_uses: None`) instead of a finite lifespan.
/// Named 2026-08-18 for the wiki's constant audit - was a bare `0.01`.
pub(crate) const INDESTRUCTIBLE_ITEM_CHANCE: f64 = 0.01;
/// Lower bound of a non-indestructible item's `max_uses` roll (see
/// `INDESTRUCTIBLE_ITEM_CHANCE`) - uniform across
/// `ITEM_DURABILITY_MIN_USES..=ITEM_DURABILITY_MAX_USES`, averaging
/// exactly 10. Named 2026-08-18 for the wiki's constant audit - was a
/// bare `7`.
pub(crate) const ITEM_DURABILITY_MIN_USES: u32 = 7;
/// Upper bound of the same roll - see `ITEM_DURABILITY_MIN_USES`. Named
/// 2026-08-18 for the wiki's constant audit - was a bare `13`.
pub(crate) const ITEM_DURABILITY_MAX_USES: u32 = 13;
pub(crate) fn generate_item_at_tier_with_roll(slot: EquipSlot, tier: u32, power_roll: f64, rng: &mut impl Rng) -> Item {
    let noun_pool: &[&str] = match slot {
        EquipSlot::Weapon => &["Sword", "Axe", "Blade", "Hammer", "Dagger"],
        EquipSlot::Helm => &["Helm", "Crown", "Circlet", "Hood"],
        EquipSlot::Body => &["Armor", "Plate", "Robe", "Cuirass"],
        EquipSlot::Gloves => &["Gloves", "Gauntlets", "Wraps"],
        EquipSlot::Boots => &["Boots", "Greaves", "Treads"],
    };
    let adjective_pool: &[&str] = match tier {
        1..=2 => &["Rusty", "Worn", "Crude"],
        3..=4 => &["Iron", "Steel", "Sturdy"],
        5..=6 => &["Mythril", "Ember", "Storm"],
        7..=8 => &["Runic", "Void", "Radiant"],
        _ => &["Ascended", "Celestial", "Eternal"],
    };
    let name = format!("{} {}", adjective_pool[rng.gen_range(0..adjective_pool.len())], noun_pool[rng.gen_range(0..noun_pool.len())]);
    let power = compute_power(slot, tier, power_roll);

    // 1% indestructible (never decays, never auto-replaced); otherwise a
    // lifespan drawn uniformly from 7-13 fights of use, averaging exactly
    // the requested 10.
    let max_uses = if rng.gen_bool(INDESTRUCTIBLE_ITEM_CHANCE) { None } else { Some(rng.gen_range(ITEM_DURABILITY_MIN_USES..=ITEM_DURABILITY_MAX_USES)) };
    let id = format!("{:016x}", rng.gen::<u64>());
    let affixes = roll_affixes(slot, tier, rng);

    Item {
        id,
        name,
        slot,
        tier,
        power,
        power_roll,
        max_uses,
        uses: 0,
        affixes,
        locked: false,
        nickname: None,
        disenchant_protected: false,
        unique_affix: None,
        perfect: false,
        sacred_affix: None,
        legacy_reforge_crit_used: false,
        legacy_recombine_crit_used: false,
        legacy_crit_bonus_affixes: Vec::new(),
        crit_bonus_affixes: Vec::new(),
    }
}

// Golden-output regression test for the item-balance refactor (see the
// refactor plan) - the codebase has no other automated tests, so this is
// the concrete safety net for the `adventure-item-balance.toml`
// sparse-override machinery and the `AffixDef`/`affix_def` transcription
// it depends on. Deterministic (fixed seed, fixed power_roll) generated
// items for a spread of slot/tier combos, asserted against values
// captured BEFORE the balance-externalization refactor landed - any
// unintended coefficient drift (a transcription slip in `affix_def`, a
// stray override left in the shipped `adventure-item-balance.toml`) fails
// this test. Run with the repo's shipped (fully-commented, i.e. no active
// overrides) `adventure-item-balance.toml` in place - that's the
// configuration these values were captured against.
#[cfg(test)]
mod golden_item_baseline {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    pub(crate) fn assert_close(actual: f64, expected: f64, context: &str) {
        assert!((actual - expected).abs() < 1e-6, "{context}: expected {expected}, got {actual}");
    }

    #[test]
    pub(crate) fn generated_items_match_pre_refactor_baseline() {
        // (tier, slot, expected power, expected single affix + value) -
        // captured 2026-08-16 immediately before Phase 2 of the item
        // balance refactor, with power_roll pinned to 1.0 and a fixed
        // RNG seed so every draw (name/max_uses/id/affix roll) is
        // reproducible. Body/Gloves/Boots' expected affix updated
        // 2026-08-19 (was Affix::Intervene) - the elemental-damage-on-
        // all-slots widen (see Affix::ColdDamage's own doc) made their
        // eligible pool identical to Weapon/Helm's, so the same seed now
        // draws the same affix (ColdDamage) on every slot - an EXPECTED
        // fallout of the widen, not a regression (see the two dilution
        // notes in docs/, this file's own git history for the previous
        // values, and the pool composition this seed now walks).
        let cases: &[(u32, EquipSlot, f64, Affix, f64)] = &[
            (1, EquipSlot::Weapon, 12.0, Affix::Evasion, 0.016620838),
            (1, EquipSlot::Helm, 4.0, Affix::ColdDamage, 0.024857448),
            (1, EquipSlot::Body, 12.0, Affix::ColdDamage, 0.024857448),
            (1, EquipSlot::Gloves, 0.009, Affix::ColdDamage, 0.024857448),
            (1, EquipSlot::Boots, 13.0, Affix::ColdDamage, 0.024857448),
            (5, EquipSlot::Weapon, 60.0, Affix::Evasion, 0.083104192),
            (5, EquipSlot::Helm, 20.0, Affix::ColdDamage, 0.124287242),
            (5, EquipSlot::Body, 60.0, Affix::ColdDamage, 0.124287242),
            (5, EquipSlot::Gloves, 0.045, Affix::ColdDamage, 0.124287242),
            (5, EquipSlot::Boots, 65.0, Affix::ColdDamage, 0.124287242),
            (10, EquipSlot::Weapon, 120.0, Affix::Evasion, 0.166208384),
            (10, EquipSlot::Helm, 40.0, Affix::ColdDamage, 0.248574483),
            (10, EquipSlot::Body, 120.0, Affix::ColdDamage, 0.248574483),
            (10, EquipSlot::Gloves, 0.09, Affix::ColdDamage, 0.248574483),
            (10, EquipSlot::Boots, 130.0, Affix::ColdDamage, 0.248574483),
            (20, EquipSlot::Weapon, 240.0, Affix::Evasion, 0.332416767),
            (20, EquipSlot::Helm, 80.0, Affix::ColdDamage, 0.497148967),
            (20, EquipSlot::Body, 240.0, Affix::ColdDamage, 0.497148967),
            // Gloves' power is uncapped as of the 2026-08-16 speed-scaling
            // fix, AND the base coefficient was cut 5x (0.045 -> 0.009)
            // the same day after the uncapped version tested too strong
            // in practice (0.009 * 20 * 1.0 = 0.18, no longer clamped to
            // 0.55 and no longer using the original 0.045 coefficient
            // either) - this case exercises both changes together.
            (20, EquipSlot::Gloves, 0.18, Affix::ColdDamage, 0.497148967),
            (20, EquipSlot::Boots, 260.0, Affix::ColdDamage, 0.497148967),
        ];

        for &(tier, slot, expected_power, expected_affix, expected_value) in cases {
            let mut rng = StdRng::seed_from_u64(42);
            let item = generate_item_at_tier_with_roll(slot, tier, 1.0, &mut rng);
            let context = format!("slot={slot:?} tier={tier}");
            assert_close(item.power, expected_power, &format!("{context} power"));
            assert_eq!(item.affixes.len(), 1, "{context}: expected exactly one rolled affix");
            let (affix, value) = item.affixes[0];
            assert_eq!(affix, expected_affix, "{context}: rolled affix type");
            assert_close(value, expected_value, &format!("{context} affix value"));
        }
    }
}

#[cfg(test)]
mod reforge_crit_chance_tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
    }

    #[test]
    fn matches_the_two_worked_examples_from_the_live_request() {
        // "perfect = 2.2% chance, 50% qual = 1.5% chance"
        assert_close(reforge_crit_chance(50.0, false), 0.015);
        assert_close(reforge_crit_chance(100.0, true), 0.022);
    }

    #[test]
    fn floors_at_1_percent_for_zero_quality() {
        assert_close(reforge_crit_chance(0.0, false), 0.01);
        // Zero quality is a purely theoretical floor (POWER_ROLL_RANGE
        // never actually rolls exactly its own start), but Perfect always
        // pins quality to 100% regardless of the `quality_percent` passed
        // in being wrong/stale, so this isn't really reachable with
        // perfect=true in practice - not asserted here, just documenting
        // why there's no "0% perfect" case.
    }

    #[test]
    fn scales_linearly_with_quality_for_a_non_perfect_item() {
        assert_close(reforge_crit_chance(0.0, false), 0.01);
        assert_close(reforge_crit_chance(25.0, false), 0.0125);
        assert_close(reforge_crit_chance(100.0, false), 0.02);
    }

    #[test]
    fn perfect_always_applies_the_quality_mult_on_top_of_100_percent() {
        // Even if some caller ever passed a non-100 quality_percent
        // alongside perfect=true (shouldn't happen - Perfect always pins
        // power_roll to POWER_ROLL_RANGE.end - but the formula itself
        // should still behave sanely), the mult applies to whatever
        // quality_percent it's given, not a hardcoded 100.
        assert_close(reforge_crit_chance(50.0, true), 0.01 * (1.0 + 0.5 * PERFECT_QUALITY_MULT));
    }
}

#[cfg(test)]
mod sacred_item_tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn sacred_item_is_perfect_plus_one_maxed_implicit() {
        let mut rng = StdRng::seed_from_u64(7);
        let base = generate_item_at_tier_with_roll(EquipSlot::Weapon, 10, 1.0, &mut rng);
        let normal_affix_count = base.affixes.len();

        let mut sacred_rng = StdRng::seed_from_u64(99);
        let sacred = make_item_sacred(base, &mut sacred_rng);

        // Sacred is always also Perfect (make_item_perfect applied first).
        assert!(sacred.perfect, "Sacred item must also be marked perfect");
        assert_eq!(sacred.power_roll, POWER_ROLL_RANGE.end, "Sacred's primary stat rolls max, same as plain Perfect");

        // The implicit exists, and does NOT touch the normal, countable
        // affix pool - it must stay exactly whatever roll_affixes gave
        // the base item, untouched by make_item_sacred.
        let (sacred_affix, sacred_value) = sacred.sacred_affix.expect("make_item_sacred must set sacred_affix");
        assert_eq!(sacred.affixes.len(), normal_affix_count, "sacred_affix must never be added to/counted in the normal affixes Vec");

        // Rolled at ITS OWN max jitter (1.15, the affix band - not
        // POWER_ROLL_RANGE.end/1.2, the primary-stat band) times
        // PERFECT_QUALITY_MULT, same two-step shape make_item_perfect
        // applies to the item's own primary stat.
        let expected_value = affix_base_value(sacred_affix, sacred.tier) * 1.15 * PERFECT_QUALITY_MULT;
        assert!((sacred_value - expected_value).abs() < 1e-9, "sacred_affix value: expected {expected_value}, got {sacred_value}");
    }

    /// Regression test for a live report: "Sacred items are returning
    /// less dust than Perfect items". `disenchant_multiplier` used to
    /// only ever look at `affixes.len()`, so `sacred_affix` was
    /// completely invisible to it - a Sacred item disenchanted for
    /// EXACTLY what a same-normal-affix-count Perfect item would, never
    /// more, despite carrying a strictly-better maxed implicit on top.
    #[test]
    fn sacred_disenchants_for_more_than_an_equal_affix_count_perfect_item() {
        let mut rng = StdRng::seed_from_u64(42);
        for normal_affix_count in 0..=4 {
            let mut base = generate_item_at_tier_with_roll(EquipSlot::Weapon, 10, 1.0, &mut rng);
            base.affixes = (0..normal_affix_count).map(|i| (ALL_AFFIXES[i], 1.0)).collect();
            let perfect = make_item_perfect(base.clone());
            let sacred = make_item_sacred(base, &mut rng);

            assert!(
                sacred.disenchant_multiplier() > perfect.disenchant_multiplier(),
                "at {normal_affix_count} normal affixes: Sacred ({}x) must disenchant for more than an equal-affix-count Perfect item ({}x)",
                sacred.disenchant_multiplier(),
                perfect.disenchant_multiplier()
            );
            // Exact expected relationship: Sacred's implicit counts as
            // one more affix toward the same step Perfect's own affixes
            // already climb - so Sacred(N) == Perfect(N+1) exactly.
            let mut one_more = generate_item_at_tier_with_roll(EquipSlot::Weapon, 10, 1.0, &mut rng);
            one_more.affixes = (0..normal_affix_count + 1).map(|i| (ALL_AFFIXES[i % ALL_AFFIXES.len()], 1.0)).collect();
            let perfect_plus_one = make_item_perfect(one_more);
            assert_eq!(
                sacred.disenchant_multiplier(),
                perfect_plus_one.disenchant_multiplier(),
                "Sacred with {normal_affix_count} normal affixes should match a Perfect item with {} normal affixes exactly",
                normal_affix_count + 1
            );
        }
    }

    /// The whole point of the implicit is that it actually affects
    /// combat, not just the item card - `effective_affix_total` is the
    /// ONE shared function every `Character::combat_*` getter (via
    /// `sum_affix`) routes through, so this is the direct regression
    /// test for the gap where Sacred's implicit was rolled/displayed
    /// correctly but silently contributed nothing to actual stats.
    #[test]
    fn sacred_affix_counts_toward_effective_affix_total() {
        let mut rng = StdRng::seed_from_u64(21);
        let base = generate_item_at_tier_with_roll(EquipSlot::Body, 15, 1.0, &mut rng);
        let mut item = make_item_sacred(base, &mut rng);
        // Full durability (decay_fraction == 1.0) so the math below is exact.
        item.max_uses = None;
        let (sacred_affix, sacred_value) = item.sacred_affix.expect("sacred_affix must be set");

        let normal_matching: f64 = item.affixes.iter().filter(|(a, _)| *a == sacred_affix).map(|(_, v)| *v).sum();
        let expected_total = normal_matching + sacred_value;
        assert!(
            (item.effective_affix_total(sacred_affix) - expected_total).abs() < 1e-9,
            "effective_affix_total for the sacred affix's type must include the implicit's own value on top of any normal roll of the same type"
        );
        // A non-matching affix (guaranteed distinct from the one that got
        // sacred-rolled, since Affix has 17 variants) must be completely
        // unaffected - sacred_affix should never leak into an unrelated
        // stat's total.
        let unrelated = ALL_AFFIXES.iter().copied().find(|&a| a != sacred_affix).unwrap();
        let normal_unrelated: f64 = item.affixes.iter().filter(|(a, _)| *a == unrelated).map(|(_, v)| *v).sum();
        assert!((item.effective_affix_total(unrelated) - normal_unrelated).abs() < 1e-9, "an unrelated affix's total must be untouched by sacred_affix");
    }

    #[test]
    fn sacred_affix_survives_and_scales_with_tier_sync() {
        let mut rng = StdRng::seed_from_u64(11);
        let base = generate_item_at_tier_with_roll(EquipSlot::Helm, 5, 1.0, &mut rng);
        let mut item = make_item_sacred(base, &mut rng);
        let (affix, value_at_5) = item.sacred_affix.expect("sacred_affix must be set");

        item.locked = true; // sync_tier_to is only ever called on Krangled/crafted items in practice, but doesn't gate on it itself
        item.sync_tier_to(10); // double the tier

        let (affix_after, value_after) = item.sacred_affix.expect("sacred_affix must survive a tier sync");
        assert_eq!(affix_after, affix, "sync_tier_to must never change WHICH affix is sacred, only its value");
        assert!((value_after - value_at_5 * 2.0).abs() < 1e-9, "sacred_affix value must scale proportionally with tier, same as normal affixes: expected {}, got {value_after}", value_at_5 * 2.0);
    }

    #[test]
    fn sacred_affix_not_inherited_by_recombine() {
        // Mirrors Item::perfect's own documented "never inherited by
        // Recombine" behavior exactly - apply_recombine_roll builds its
        // result via generate_item_at_tier_with_roll (which always sets
        // sacred_affix: None) and never copies the field from either
        // source, so a brand-new item from that path can never carry one
        // regardless of what fed into the recombine.
        let mut rng = StdRng::seed_from_u64(3);
        let fresh = generate_item_at_tier_with_roll(EquipSlot::Boots, 8, 1.0, &mut rng);
        assert!(fresh.sacred_affix.is_none(), "a freshly generated item must never start Sacred");
    }
}

/// How much more of EVERY stat (the primary `power` stat AND every
/// rolled `affixes` value) a Perfect Quality item has over the same item
/// at a normal max roll - "20% more total stats than a 100% quality
/// item" per the request. Applied by `make_item_perfect`, not baked into
/// `power_roll`/`quality_percent`'s existing 0-100% scale - stretching
/// `power_roll` itself past `POWER_ROLL_RANGE` would make Perfect items
/// indistinguishable from a lucky max roll in the numeric Quality%
/// display, when the whole point is a visibly distinct gold "Perfect
/// Quality" tier (see the web dashboard's item-card rendering).
pub const PERFECT_QUALITY_MULT: f64 = 1.20;

/// Reforge's rare bonus-affix crit chance (2026-08-16, a live request:
/// "increase this chance by the quality on the item") - a 1% floor at 0%
/// quality, scaling up to 2% at a naturally-rolled 100% quality, with
/// Perfect Quality items getting `PERFECT_QUALITY_MULT` applied on top of
/// that 100% (landing at 2.2%). Matches the live request's own two worked
/// examples exactly: 50% quality -> 1%*(1+0.5) = 1.5%; Perfect -> 1%*(1+
/// 1.0*1.2) = 2.2%. Shared by both reforge paths (`Character::reforge_item`
/// and `AdventureManager::reforge_equipped_item`), which used to each
/// hardcode their own identical flat `0.01` independently.
pub fn reforge_crit_chance(quality_percent: f64, perfect: bool) -> f64 {
    let quality_fraction = (quality_percent / 100.0) * if perfect { PERFECT_QUALITY_MULT } else { 1.0 };
    (0.01 * (1.0 + quality_fraction)).min(1.0)
}

/// Upgrades an already-generated item to Perfect Quality in place: pins
/// its `power_roll` to `POWER_ROLL_RANGE`'s own max (the actual "100%
/// quality" reference point `quality_percent` displays), then applies
/// `PERFECT_QUALITY_MULT` on top of both `power` and every affix value -
/// so a Perfect item is always EXACTLY 20% ahead of the best a normal
/// roll could ever produce, not just "usually better". Deliberately a
/// separate post-processing step (not a `generate_item_at_tier_with_roll`
/// parameter) so ordinary generation stays untouched and this can be
/// applied selectively to exactly one drop per the stage-90 boss-kill
/// rule (see `run_encounter`).
pub(crate) fn make_item_perfect(mut item: Item) -> Item {
    item.power_roll = POWER_ROLL_RANGE.end;
    item.power = compute_power(item.slot, item.tier, item.power_roll) * PERFECT_QUALITY_MULT;
    for (_, value) in item.affixes.iter_mut() {
        *value *= PERFECT_QUALITY_MULT;
    }
    item.perfect = true;
    item
}

/// Upgrades an already-Perfect-eligible item all the way to Sacred
/// (2026-08-16, a live request: "exactly like perfect quality items
/// except they have 1 random perfectly rolled affix... implicit... same
/// as speed for gloves") - applies `make_item_perfect` first (Sacred is
/// always also Perfect), then rolls one extra `Affix` from the FULL
/// `ALL_AFFIXES` pool (deliberately ignoring `is_eligible_for_slot` per
/// the live request - a Sacred Boots item can roll ColdDamage even
/// though a normal Boots roll never could) into `Item::sacred_affix`,
/// at that affix's own max jitter (1.15, the same ceiling
/// `craft_affix_value_range`/`roll_affixes` use for a normal affix roll -
/// NOT `POWER_ROLL_RANGE.end`/1.2, which is the PRIMARY stat's distinct
/// jitter band) with `PERFECT_QUALITY_MULT` on top, mirroring the exact
/// two-step "max roll, then the Perfect multiplier" shape
/// `make_item_perfect` already applies to the item's own primary stat.
pub(crate) fn make_item_sacred(item: Item, rng: &mut impl Rng) -> Item {
    let mut item = make_item_perfect(item);
    let affix = ALL_AFFIXES[rng.gen_range(0..ALL_AFFIXES.len())];
    let value = affix_base_value(affix, item.tier) * 1.15 * PERFECT_QUALITY_MULT;
    item.sacred_affix = Some((affix, value));
    item
}

