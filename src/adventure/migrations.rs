use super::*;

/// Applies `f` to every item a character has - both equipped slots and
/// the bag - the "every item this character owns" iteration every
/// gear-value migration needs (see `run_item_migrations`'s callers).
pub(crate) fn for_each_item_mut(character: &mut Character, mut f: impl FnMut(&mut Item)) {
    for slot in EQUIP_SLOTS {
        if let Some(item) = character.equipped_mut(slot) {
            f(item);
        }
    }
    for item in character.inventory.iter_mut() {
        f(item);
    }
}

/// 2026-08 helm rebalance retrofit - halves an existing helm's `power` to
/// match the halved `generate_item_at_tier` base. Slot-filtered inside the
/// fn (rather than only visiting the Helm slot) since `for_each_item_mut`
/// visits every slot/bag item uniformly - matches the original hand-written
/// block's behavior exactly (only ever touched Helm-slotted items, whether
/// equipped or in the bag).
pub(crate) fn migrate_helm_rebalance_v2(item: &mut Item) {
    if item.slot == EquipSlot::Helm {
        item.power *= 0.5;
    }
}

/// `power_roll` backfill - reverses an item's real original roll from its
/// already-persisted `power` (see the original block's doc for why this
/// was needed rather than a flat default).
pub(crate) fn migrate_power_roll_backfill(item: &mut Item) {
    let base = base_power_for_slot(item.slot);
    if base > 0.0 && item.tier > 0 {
        let recovered = item.power / (base * item.tier as f64);
        item.power_roll = recovered.clamp(POWER_ROLL_RANGE.start, POWER_ROLL_RANGE.end - f64::EPSILON);
    }
}

/// Krangle accuracy pass - raises a Krangled item's `power`/affix values up
/// to what its current tier implies, `.max()`-only so a genuinely good
/// historical roll is left alone.
pub(crate) fn migrate_krangle_accuracy(item: &mut Item) {
    if !item.locked {
        return;
    }
    item.power = item.power.max(compute_power(item.slot, item.tier, item.power_roll));
    for (affix, value) in item.affixes.iter_mut() {
        *value = value.max(affix_base_value(*affix, item.tier));
    }
}

/// Item accuracy pass #2 - same `.max()`-only floor as the Krangle pass,
/// just for every item regardless of `locked` (catches the reforge bug
/// the Krangle-only pass didn't).
pub(crate) fn migrate_item_accuracy(item: &mut Item) {
    item.power = item.power.max(compute_power(item.slot, item.tier, item.power_roll));
    for (affix, value) in item.affixes.iter_mut() {
        *value = value.max(affix_base_value(*affix, item.tier));
    }
}

/// Crit nerf retrofit - halves the stored value of any already-rolled
/// CritChance/CritMultiplier affix, exactly preserving each item's
/// original jitter (see the original block's doc for why halving the
/// stored value, not recomputing from `affix_base_value`, is correct).
pub(crate) fn migrate_crit_value_nerf(item: &mut Item) {
    for (affix, value) in item.affixes.iter_mut() {
        if matches!(affix, Affix::CritChance | Affix::CritMultiplier) {
            *value *= 0.5;
        }
    }
}

/// Gloves speed rebalance retrofit (2026-08-16, two live requests back
/// to back: first "make sure all existing gloves are updated" after
/// uncapping Gloves' speed scaling, then - once that uncapped version
/// turned out way too strong in practice ("at tier 95 I have 616% speed
/// thats too much... reduce it by a factor of about 5") - the
/// `default_base_power_for_slot` coefficient itself dropped 5x, 0.045 to
/// 0.009. Either change alone only affects NEWLY generated items -
/// every already-persisted glove's `power` is still frozen at whatever
/// formula was live when it was last rolled/tier-synced (the original
/// hard-capped one, or briefly the uncapped-but-too-strong one, depending
/// on exactly when). An UNCONDITIONAL recompute, deliberately NOT
/// `.max()`-gated like every other accuracy-pass migration here - this
/// one has to be able to LOWER a value too (the coefficient more than
/// halved even off the ORIGINAL pre-uncap numbers at most tiers), so
/// `.max()` would wrongly keep an old, too-high value instead of
/// correcting it. Reapplies `PERFECT_QUALITY_MULT` when `item.perfect` -
/// same trap `sync_tier_to`'s own doc warns about: a bare `compute_power`
/// recompute has no idea a Perfect/Sacred glove's `power` is supposed to
/// carry that 20% on top, so skipping this would silently shave it back
/// off. Slot-filtered (only Gloves), same pattern as
/// `migrate_helm_rebalance_v2`. Idempotent by construction (a pure
/// recompute of the same inputs always lands on the same output), so
/// it's harmless even if it ends up applied more than once through
/// whatever sequence of restarts this shipped across.
pub(crate) fn migrate_gloves_speed_rebalance(item: &mut Item) {
    if item.slot != EquipSlot::Gloves {
        return;
    }
    let mut recomputed = compute_power(item.slot, item.tier, item.power_roll);
    if item.perfect {
        recomputed *= PERFECT_QUALITY_MULT;
    }
    item.power = recomputed;
}

/// Reforge/Recombine crit lineage-tracking retrofit (2026-08-16, a live
/// report: an item had been reforged/recombined enough times to end up
/// with far more affixes than intended - see the (now legacy)
/// `Item::legacy_reforge_crit_used`'s doc for the full bug and the
/// designed 4-base+1-each-from-Reforge/Recombine/Krangle=7 ceiling this
/// establishes going forward). `legacy_reforge_crit_used`/
/// `legacy_recombine_crit_used` default to `false` on every existing
/// item (plain serde default), but an item that ALREADY has more than 4
/// affixes is unambiguous proof at least one of the 3 bonus sources
/// already fired on it at some point - marks BOTH legacy crit flags used
/// defensively (the affix count alone can't tell us which specific
/// source(s) contributed the extras, or how many times), so an
/// already-over-cap item can't keep compounding further via either crit
/// path from here on. Deliberately does NOT remove any of the item's
/// existing extra affixes - only stops future growth, same "never take
/// away what a player already has, only stop it from getting worse"
/// principle as every other accuracy-pass migration here.
///
/// Superseded by `migrate_crit_flag_to_affix_tracking` below, which reads
/// these same legacy flags (including whatever this migration itself
/// wrote) to seed the new affix-attached tracking - kept declared here
/// unchanged (rather than removed) since its own marker already ran in
/// production; every migration in `ITEM_MIGRATIONS` stays a permanent
/// historical record, never removed once shipped.
pub(crate) fn migrate_crit_lineage_backfill(item: &mut Item) {
    if item.affixes.len() > 4 {
        item.legacy_reforge_crit_used = true;
        item.legacy_recombine_crit_used = true;
    }
}

/// Retrofits the old sticky-bool crit-lineage tracking onto the new
/// affix-attached tracking (`Item::crit_bonus_affixes`) - 2026-08-18, a
/// live report: a tier-374 item had genuinely landed Reforge's crit
/// once, then had that exact bonus affix Annulled away later, and was
/// left permanently unable to crit again forever after - because the
/// old bool was sticky-forever, independent of whether the affix it
/// granted still existed on the item at all. See
/// `Item::crit_bonus_affixes`'s own doc for the new design (the "used"
/// gate is now derived by checking whether a crit-tagged affix is STILL
/// actually present, so removing it - by Annulment, or by anything else,
/// including crafting methods that don't even know this tracking exists
/// - naturally re-opens the gate with zero extra bookkeeping anywhere
/// else).
///
/// For each of the two legacy flags that was `true`: looks for a type in
/// `legacy_crit_bonus_affixes` that's both still present in `affixes`
/// and not already claimed by the other source, and tags it. Falls back
/// to defensively tagging one of the item's affixes beyond the normal
/// 4-affix cap (same "an over-cap item is unambiguous proof some bonus
/// source fired, even if we can't tell which affix it was" reasoning
/// `migrate_crit_lineage_backfill` already used) if no informative match
/// exists, so an over-cap legacy item stays defensively locked instead
/// of silently getting a free extra crit shot. If neither applies - the
/// flag was true but the item is at/under the normal cap with no
/// evidence of which affix it was - the gate is simply left open, which
/// is the exact fix for the reported bug: there's nothing left on the
/// item to lock it shut on.
pub(crate) fn migrate_crit_flag_to_affix_tracking(item: &mut Item) {
    if item.legacy_reforge_crit_used {
        assign_legacy_crit_source(item, CritSource::Reforge);
    }
    if item.legacy_recombine_crit_used {
        assign_legacy_crit_source(item, CritSource::Recombine);
    }
}

fn assign_legacy_crit_source(item: &mut Item, source: CritSource) {
    if item.crit_bonus_affixes.iter().any(|&(_, s)| s == source) {
        return; // already tagged - idempotent re-run safety
    }
    let claimed: Vec<Affix> = item.crit_bonus_affixes.iter().map(|&(a, _)| a).collect();
    let present: Vec<Affix> = item.affixes.iter().map(|&(a, _)| a).collect();
    if let Some(&affix) = item.legacy_crit_bonus_affixes.iter().find(|a| present.contains(a) && !claimed.contains(a)) {
        item.crit_bonus_affixes.push((affix, source));
        return;
    }
    if item.affixes.len() > 4 {
        if let Some(&(affix, _)) = item.affixes.iter().skip(4).find(|(a, _)| !claimed.contains(a)) {
            item.crit_bonus_affixes.push((affix, source));
        }
    }
}

/// One-time item-value corrections, oldest first. **Order matters**:
/// `migrate_krangle_accuracy`/`migrate_item_accuracy`'s `.max()` floors
/// and `migrate_gloves_speed_rebalance`'s unconditional recompute are all
/// computed from `item.power_roll` (via `compute_power`), which
/// `migrate_power_roll_backfill` is what makes trustworthy in the first
/// place - it must run first. `migrate_crit_lineage_backfill` reads
/// `item.affixes.len()` directly (not tier/power_roll-derived), so its
/// own position relative to the others doesn't matter.
/// `migrate_crit_flag_to_affix_tracking` reads `legacy_reforge_crit_used`/
/// `legacy_recombine_crit_used` - including whatever
/// `migrate_crit_lineage_backfill` itself wrote there - so it must run
/// AFTER that one; both are independent of every tier/power_roll-based
/// migration above them either way. Add a new balance-patch migration
/// here as one line: (marker filename, the mutation), inserted at
/// whatever sequence position it needs relative to the existing ones.
pub(crate) const ITEM_MIGRATIONS: &[(&str, fn(&mut Item))] = &[
    ("adventure-helm-rebalance-v2-marker.json", migrate_helm_rebalance_v2),
    ("adventure-power-roll-backfill-marker.json", migrate_power_roll_backfill),
    ("adventure-krangle-accuracy-marker.json", migrate_krangle_accuracy),
    ("adventure-item-accuracy-marker.json", migrate_item_accuracy),
    ("adventure-crit-value-nerf-marker.json", migrate_crit_value_nerf),
    ("adventure-gloves-speed-rebalance-marker.json", migrate_gloves_speed_rebalance),
    ("adventure-crit-lineage-backfill-marker.json", migrate_crit_lineage_backfill),
    ("adventure-crit-flag-to-affix-tracking-marker.json", migrate_crit_flag_to_affix_tracking),
];

/// Runs each pending entry of `ITEM_MIGRATIONS` in array order, over
/// every item every character owns. Saves `characters.json` AND that
/// migration's own marker before moving to the next entry - NOT batched
/// into one save at the end, because that would be unsafe: if the
/// process died after a batched save but before every marker finished
/// writing, a migration whose marker didn't make it in time would look
/// "still pending" on restart and get re-applied on top of
/// already-mutated data (e.g. `migrate_crit_value_nerf` would silently
/// halve crit values a second time). This preserves the exact
/// crash-safety property the original hand-written blocks already had -
/// each was its own save-then-mark-done pair - just without the
/// copy-pasted guard/save/error-log boilerplate around it.
pub(crate) fn run_item_migrations(characters_path: &PathBuf, characters: &mut HashMap<String, Character>) {
    for (marker, f) in ITEM_MIGRATIONS.iter().copied() {
        if crate::state::load_json::<bool>(marker).is_some() {
            continue;
        }
        for character in characters.values_mut() {
            for_each_item_mut(character, f);
        }
        if let Err(err) = crate::state::save_json(characters_path, characters) {
            tracing::error!("Failed to persist item migration '{marker}' to {}: {err}", characters_path.display());
        }
        if let Err(err) = crate::state::save_json(marker, &true) {
            tracing::error!("Failed to persist item migration marker to {marker}: {err}");
        }
    }
}

/// Repairs Monk allocations after the 2026-08-18 tier swap that exchanged
/// `hundredfists` (Specialization -> Modifier) and `onehundredhands`
/// (Modifier -> Specialization, renamed "Flow like Water").
///
/// Two distinct cases, because a player can hold either or both:
///
/// 1. `hundredfists` is now a Modifier, whose `max_rank` is 3 rather than
///    a Specialization's 4. A stored rank of 4 would survive as-is (no
///    caller clamps stored ranks - see `passive_node_rank`) and read as
///    `magnitude_at_rank(4)` = +8 max stacks, i.e. 13 instead of the
///    documented 11. Clamped down to 3.
/// 2. Only when `onehundredhands` is genuinely unallocated does the old
///    `hundredfists` rank MOVE across to it. That is the "they invested in
///    the old spec slot and their Chakras hang off it" case, where leaving
///    it behind would strand every point spent below. When BOTH are
///    allocated the move is deliberately skipped and both are kept: the
///    two nodes now sit at different tiers under different parents, so
///    they are separate legitimate investments, and merging them would
///    silently destroy the smaller one.
///
/// Never removes a node outright - the worst case is the single over-cap
/// 4th point in case 1, which the new tier simply cannot hold.
pub(crate) fn migrate_flowlikewater_swap(character: &mut Character) {
    for tree in [&mut character.passive_allocations, &mut character.secondary_passive_allocations] {
        // Both trees, since Split Personality can run Monk as a secondary.
        let hundredfists = tree.get("hundredfists").copied().unwrap_or(0);
        if hundredfists == 0 {
            continue;
        }
        if tree.get("onehundredhands").copied().unwrap_or(0) == 0 {
            tree.remove("hundredfists");
            tree.insert("onehundredhands".to_string(), hundredfists.min(4));
        } else {
            tree.insert("hundredfists".to_string(), hundredfists.min(3));
        }
    }
}

/// Character-level counterpart to `ITEM_MIGRATIONS` - same
/// (marker filename, mutation) shape, for one-time corrections that touch
/// a character's own fields rather than their gear.
pub(crate) const CHARACTER_MIGRATIONS: &[(&str, fn(&mut Character))] = &[("adventure-flowlikewater-swap-marker.json", migrate_flowlikewater_swap)];

/// Runs each pending entry of `CHARACTER_MIGRATIONS` over every character -
/// same save-then-mark-done-per-migration crash-safety contract
/// `run_item_migrations` documents (deliberately NOT batched into one save
/// at the end, so a crash between the save and the marker write can't make
/// an already-applied migration look pending and re-run on top of mutated
/// data).
pub(crate) fn run_character_migrations(characters_path: &PathBuf, characters: &mut HashMap<String, Character>) {
    for (marker, f) in CHARACTER_MIGRATIONS.iter().copied() {
        if crate::state::load_json::<bool>(marker).is_some() {
            continue;
        }
        for character in characters.values_mut() {
            f(character);
        }
        if let Err(err) = crate::state::save_json(characters_path, characters) {
            tracing::error!("Failed to persist character migration '{marker}' to {}: {err}", characters_path.display());
        }
        if let Err(err) = crate::state::save_json(marker, &true) {
            tracing::error!("Failed to persist character migration marker to {marker}: {err}");
        }
    }
}

#[cfg(test)]
mod gloves_speed_rebalance_tests {
    use super::*;

    fn gloves_at(tier: u32, power: f64, perfect: bool) -> Item {
        Item {
            id: "x".into(),
            name: "x".into(),
            slot: EquipSlot::Gloves,
            tier,
            power,
            power_roll: 1.0,
            max_uses: None,
            uses: 0,
            affixes: vec![],
            locked: false,
            nickname: None,
            disenchant_protected: false,
            unique_affix: None,
            perfect,
            sacred_affix: None,
            legacy_reforge_crit_used: false,
            legacy_recombine_crit_used: false,
            legacy_crit_bonus_affixes: vec![],
            crit_bonus_affixes: vec![],
        }
    }

    #[test]
    fn lowers_a_stale_over_strong_glove_to_the_new_value() {
        // The reported case: a tier-95 Perfect glove at the OLD 0.045
        // coefficient with max power_roll (1.2) read 0.045*95*1.2 = 5.13,
        // *PERFECT_QUALITY_MULT (1.2) = 6.156 -> "616% speed". At the new
        // 0.009 coefficient it must land exactly 5x lower: 0.009*95*1.2 =
        // 1.026, *1.2 = 1.2312 -> ~123%. This is the core regression test
        // for the whole point of this migration - unlike every other
        // accuracy-pass migration, it MUST be able to lower a value.
        let mut item = gloves_at(95, 6.156, true);
        item.power_roll = 1.2;
        migrate_gloves_speed_rebalance(&mut item);
        assert!((item.power - 1.2312).abs() < 1e-6, "expected ~1.2312 (123%), got {}", item.power);
        assert!(item.power < 6.156 / 4.0, "must actually drop by roughly the requested factor of 5, not stay near the old value");
    }

    #[test]
    fn recomputes_even_a_never_capped_low_tier_value() {
        // Unlike the earlier uncap-only migration, this one is NOT
        // `.max()`-gated - even a tier-5 glove that was never near the
        // old hard cap still used the old 0.045 coefficient (0.225) and
        // must be pulled down to the new 0.009 one (0.045).
        let mut item = gloves_at(5, 0.225, false);
        migrate_gloves_speed_rebalance(&mut item);
        assert!((item.power - 0.045).abs() < 1e-9, "expected 0.045, got {}", item.power);
    }

    #[test]
    fn reapplies_perfect_quality_mult_for_a_perfect_glove() {
        let mut item = gloves_at(20, 0.66, true); // stale value, whatever formula it came from
        migrate_gloves_speed_rebalance(&mut item);
        let expected = 0.009 * 20.0 * 1.0 * PERFECT_QUALITY_MULT;
        assert!((item.power - expected).abs() < 1e-9, "expected {expected}, got {}", item.power);
    }

    #[test]
    fn ignores_every_non_gloves_slot() {
        let mut item = gloves_at(20, 0.55, false);
        item.slot = EquipSlot::Boots;
        migrate_gloves_speed_rebalance(&mut item);
        assert!((item.power - 0.55).abs() < 1e-9, "non-Gloves items must be completely untouched");
    }

    #[test]
    fn is_idempotent() {
        // Safe to apply more than once (see the migration's own doc) -
        // running it a second time on an already-correct value changes
        // nothing.
        let mut item = gloves_at(30, 0.0, false); // deliberately wrong starting value
        migrate_gloves_speed_rebalance(&mut item);
        let once = item.power;
        migrate_gloves_speed_rebalance(&mut item);
        assert!((item.power - once).abs() < 1e-12, "a second application must be a no-op");
    }
}

#[cfg(test)]
mod crit_lineage_backfill_tests {
    use super::*;

    fn item_with_n_affixes(n: usize) -> Item {
        Item {
            id: "x".into(),
            name: "x".into(),
            slot: EquipSlot::Weapon,
            tier: 10,
            power: 100.0,
            power_roll: 1.0,
            max_uses: None,
            uses: 0,
            affixes: (0..n).map(|i| (ALL_AFFIXES[i % ALL_AFFIXES.len()], 1.0)).collect(),
            locked: false,
            nickname: None,
            disenchant_protected: false,
            unique_affix: None,
            perfect: false,
            sacred_affix: None,
            legacy_reforge_crit_used: false,
            legacy_recombine_crit_used: false,
            legacy_crit_bonus_affixes: vec![],
            crit_bonus_affixes: vec![],
        }
    }

    #[test]
    fn marks_both_flags_used_for_an_over_cap_item() {
        let mut item = item_with_n_affixes(7); // lokati's exact reported count
        migrate_crit_lineage_backfill(&mut item);
        assert!(item.legacy_reforge_crit_used, "an over-cap item must be defensively marked as having used its reforge crit");
        assert!(item.legacy_recombine_crit_used, "an over-cap item must be defensively marked as having used its recombine crit");
    }

    #[test]
    fn leaves_a_normal_4_affix_item_alone() {
        // 4 affixes is fully explainable by normal currency crafting
        // alone (Transmute/Augment/Regal/Exalt) - no evidence either
        // crit source ever fired, so this must NOT be touched.
        let mut item = item_with_n_affixes(4);
        migrate_crit_lineage_backfill(&mut item);
        assert!(!item.legacy_reforge_crit_used);
        assert!(!item.legacy_recombine_crit_used);
    }

    #[test]
    fn leaves_a_5_affix_item_flagged_but_removes_nothing() {
        let mut item = item_with_n_affixes(5);
        migrate_crit_lineage_backfill(&mut item);
        assert!(item.legacy_reforge_crit_used);
        assert!(item.legacy_recombine_crit_used);
        assert_eq!(item.affixes.len(), 5, "the migration must never remove any of the player's existing affixes, only stop further growth");
    }
}

#[cfg(test)]
mod crit_flag_to_affix_tracking_tests {
    use super::*;

    fn item_with(affixes: Vec<(Affix, f64)>) -> Item {
        Item {
            id: "x".into(),
            name: "x".into(),
            slot: EquipSlot::Boots,
            tier: 374,
            power: 100.0,
            power_roll: 1.0,
            max_uses: None,
            uses: 0,
            affixes,
            locked: false,
            nickname: None,
            disenchant_protected: false,
            unique_affix: None,
            perfect: true,
            sacred_affix: None,
            legacy_reforge_crit_used: false,
            legacy_recombine_crit_used: false,
            legacy_crit_bonus_affixes: vec![],
            crit_bonus_affixes: vec![],
        }
    }

    #[test]
    fn maps_a_present_legacy_bonus_affix_to_the_new_tracking() {
        // The exact reported shape: reforge_crit_used=true,
        // crit_bonus_affixes=["splash"], and "splash" IS still present.
        let mut item = item_with(vec![(Affix::FlatLife, 1.0), (Affix::Splash, 1.0)]);
        item.legacy_reforge_crit_used = true;
        item.legacy_crit_bonus_affixes = vec![Affix::Splash];
        migrate_crit_flag_to_affix_tracking(&mut item);
        assert!(item.reforge_crit_used(), "a present, informative legacy entry must translate into a locked gate");
        assert!(item.is_crit_bonus_affix(Affix::Splash));
    }

    #[test]
    fn frees_the_gate_when_the_legacy_bonus_affix_is_already_gone() {
        // The exact reported bug: reforge_crit_used=true, crit_bonus_affixes
        // names "splash", but "splash" is NOT in the current affix list
        // (Annulled away at some point before this migration ever ran).
        let mut item = item_with(vec![(Affix::FlatLife, 1.0), (Affix::CritMultiplier, 1.0)]);
        item.legacy_reforge_crit_used = true;
        item.legacy_crit_bonus_affixes = vec![Affix::Splash];
        migrate_crit_flag_to_affix_tracking(&mut item);
        assert!(!item.reforge_crit_used(), "with no trace of the crit-granted affix left on the item, the gate must come back open");
    }

    #[test]
    fn defensively_locks_an_over_cap_item_with_no_informative_entry() {
        // No legacy_crit_bonus_affixes evidence at all (e.g. this item was
        // only ever touched by the defensive migrate_crit_lineage_backfill,
        // which sets the bools but never populates that list) - falls back
        // to tagging one of the affixes past the normal 4-cap.
        let mut item = item_with((0..6).map(|i| (ALL_AFFIXES[i % ALL_AFFIXES.len()], 1.0)).collect());
        item.legacy_reforge_crit_used = true;
        item.legacy_recombine_crit_used = true;
        migrate_crit_flag_to_affix_tracking(&mut item);
        assert!(item.reforge_crit_used(), "an over-cap item with no other evidence must stay defensively locked");
        assert!(item.recombine_crit_used());
    }

    #[test]
    fn leaves_the_gate_open_when_the_flag_was_never_set() {
        let mut item = item_with(vec![(Affix::FlatLife, 1.0)]);
        migrate_crit_flag_to_affix_tracking(&mut item);
        assert!(!item.reforge_crit_used());
        assert!(!item.recombine_crit_used());
        assert!(item.crit_bonus_affixes.is_empty());
    }

    #[test]
    fn is_idempotent() {
        let mut item = item_with(vec![(Affix::FlatLife, 1.0), (Affix::Splash, 1.0)]);
        item.legacy_reforge_crit_used = true;
        item.legacy_crit_bonus_affixes = vec![Affix::Splash];
        migrate_crit_flag_to_affix_tracking(&mut item);
        let once = item.crit_bonus_affixes.clone();
        migrate_crit_flag_to_affix_tracking(&mut item);
        assert_eq!(item.crit_bonus_affixes, once, "a second application must not add a duplicate entry");
    }
}

#[cfg(test)]
mod flowlikewater_swap_tests {
    use super::*;

    fn monk_with(primary: &[(&str, u32)], secondary: &[(&str, u32)]) -> Character {
        let mut c = Character::new("test".to_string());
        c.archetype = Archetype::Monk;
        c.passive_allocations = primary.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        c.secondary_passive_allocations = secondary.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        c
    }

    #[test]
    fn moves_hundredfists_rank_onto_flow_like_water() {
        // The real live case: yo_pony had hundredfists=4 with Chakras
        // hanging off it. After the swap those Chakras parent to
        // onehundredhands, so the rank has to come with them or every
        // point below is stranded.
        let mut c = monk_with(&[("hundredfists", 4), ("chakraoflight", 3), ("chakraoflife", 1)], &[]);
        migrate_flowlikewater_swap(&mut c);
        assert_eq!(c.passive_allocations.get("onehundredhands").copied(), Some(4));
        assert!(!c.passive_allocations.contains_key("hundredfists"), "the old key must be removed, not left duplicating the points");
        assert_eq!(c.passive_allocations.get("chakraoflight").copied(), Some(3), "points below the swapped node must be untouched");
        assert_eq!(c.passive_allocations.get("chakraoflife").copied(), Some(1));
    }

    #[test]
    fn keeps_both_and_clamps_when_both_are_allocated() {
        // The real live case as of deploy: yo_pony respec'd into BOTH
        // (onehundredhands=3, hundredfists=4). Post-swap these are two
        // different tiers under two different parents, so both are
        // legitimate - merging them would silently destroy one. The only
        // correction owed is the over-cap 4th point, which a Modifier
        // (max_rank 3) cannot hold.
        let mut c = monk_with(&[("onehundredhands", 3), ("hundredfists", 4), ("chakraofmany", 1)], &[]);
        migrate_flowlikewater_swap(&mut c);
        assert_eq!(c.passive_allocations.get("onehundredhands").copied(), Some(3), "an existing Flow like Water investment must be left exactly as-is");
        assert_eq!(c.passive_allocations.get("hundredfists").copied(), Some(3), "Hundred Fists must survive, clamped to its new Modifier max_rank");
        assert_eq!(c.passive_allocations.get("chakraofmany").copied(), Some(1), "points below must never be silently dropped");
    }

    #[test]
    fn never_deletes_hundredfists_outright() {
        // Regression guard for the bug caught at deploy time: an earlier
        // version removed hundredfists unconditionally and only re-inserted
        // it when onehundredhands was 0, so a player holding both lost
        // every point in it.
        for existing in [0u32, 1, 2, 3] {
            let mut c = monk_with(&[("onehundredhands", existing), ("hundredfists", 3)], &[]);
            migrate_flowlikewater_swap(&mut c);
            let moved = c.passive_allocations.get("onehundredhands").copied().unwrap_or(0);
            let kept = c.passive_allocations.get("hundredfists").copied().unwrap_or(0);
            assert!(moved > 0 || kept > 0, "with onehundredhands={existing}, the 3 points in hundredfists vanished entirely");
        }
    }

    #[test]
    fn migrates_the_secondary_tree_too() {
        // Split Personality can run Monk as a secondary archetype.
        let mut c = monk_with(&[], &[("hundredfists", 2)]);
        migrate_flowlikewater_swap(&mut c);
        assert_eq!(c.secondary_passive_allocations.get("onehundredhands").copied(), Some(2));
        assert!(!c.secondary_passive_allocations.contains_key("hundredfists"));
    }

    #[test]
    fn is_a_no_op_for_a_character_who_never_invested() {
        let mut c = monk_with(&[("pressurepoint", 4)], &[]);
        migrate_flowlikewater_swap(&mut c);
        assert!(!c.passive_allocations.contains_key("onehundredhands"), "must not conjure an allocation out of nothing");
        assert_eq!(c.passive_allocations.get("pressurepoint").copied(), Some(4));
    }

    #[test]
    fn no_node_in_any_tree_points_at_a_missing_parent() {
        // Cheap global guard against a typo'd parent key in any archetype -
        // the swap re-parented three Chakras by hand, and a mistyped key
        // would otherwise only surface as a silently unreachable node.
        for archetype in ALL_ARCHETYPES {
            let nodes = archetype.passive_nodes();
            for node in nodes.iter() {
                if let Some(parent_key) = node.parent {
                    assert!(
                        nodes.iter().any(|n| n.key == parent_key),
                        "{archetype:?} node {} points at missing parent {parent_key}",
                        node.key
                    );
                }
            }
        }
    }

    #[test]
    fn the_swapped_monk_nodes_all_sit_under_a_rank_4_capable_parent() {
        // A Modifier carries `unlock_at: Some(4)`, so its parent has to be
        // able to REACH rank 4 - i.e. a Specialization (max_rank 4), not a
        // Skill (max_rank 3). This is scoped to the nodes the 2026-08-18
        // swap actually moved rather than asserted tree-wide, because the
        // tree already contains pre-existing Modifiers parented to Skills
        // (Monk's windwalker/unbrokenchain/risingstorm hang off the
        // flowingstrikes Skill and are therefore permanently unreachable -
        // a real pre-existing bug, deliberately not "fixed" by this test).
        let nodes = Archetype::Monk.passive_nodes();
        let tier_of = |key: &str| nodes.iter().find(|n| n.key == key).unwrap_or_else(|| panic!("missing node {key}")).tier;
        let max_rank_of = |key: &str| nodes.iter().find(|n| n.key == key).unwrap_or_else(|| panic!("missing node {key}")).max_rank;

        // Flow like Water took the spec slot; Hundred Fists took the modifier slot.
        assert_eq!(tier_of("onehundredhands"), crate::passive_tree::PassiveTier::Specialization);
        assert_eq!(tier_of("hundredfists"), crate::passive_tree::PassiveTier::Modifier);

        for (child, expected_parent) in [
            ("hundredfists", "pressurepoint"),
            ("chakraofmany", "onehundredhands"),
            ("chakraoflight", "onehundredhands"),
            ("chakraoflife", "onehundredhands"),
        ] {
            let node = nodes.iter().find(|n| n.key == child).unwrap();
            assert_eq!(node.parent, Some(expected_parent), "{child} should hang off {expected_parent}");
            assert_eq!(max_rank_of(expected_parent), 4, "{child}'s parent {expected_parent} must be able to reach rank 4 or {child} can never unlock");
        }
    }
}

