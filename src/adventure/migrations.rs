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
/// with far more affixes than intended - see `Item::reforge_crit_used`'s
/// doc for the full bug and the designed 4-base+1-each-from-Reforge/
/// Recombine/Krangle=7 ceiling this establishes going forward).
/// `reforge_crit_used`/`recombine_crit_used` default to `false` on every
/// existing item (plain serde default), but an item that ALREADY has
/// more than 4 affixes is unambiguous proof at least one of the 3 bonus
/// sources already fired on it at some point - marks BOTH crit flags
/// used defensively (the affix count alone can't tell us which
/// specific source(s) contributed the extras, or how many times), so an
/// already-over-cap item can't keep compounding further via either crit
/// path from here on. Deliberately does NOT remove any of the item's
/// existing extra affixes - only stops future growth, same "never take
/// away what a player already has, only stop it from getting worse"
/// principle as every other accuracy-pass migration here.
pub(crate) fn migrate_crit_lineage_backfill(item: &mut Item) {
    if item.affixes.len() > 4 {
        item.reforge_crit_used = true;
        item.recombine_crit_used = true;
    }
}

/// One-time item-value corrections, oldest first. **Order matters**:
/// `migrate_krangle_accuracy`/`migrate_item_accuracy`'s `.max()` floors
/// and `migrate_gloves_speed_rebalance`'s unconditional recompute are all
/// computed from `item.power_roll` (via `compute_power`), which
/// `migrate_power_roll_backfill` is what makes trustworthy in the first
/// place - it must run first. `migrate_crit_lineage_backfill` reads
/// `item.affixes.len()` directly (not tier/power_roll-derived), so its
/// own position relative to the others doesn't matter - listed last
/// simply because it's the newest. Add a new balance-patch migration
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

/// Moves any `hundredfists` rank onto `onehundredhands` (renamed "Flow
/// like Water") - the 2026-08-18 swap exchanged those two nodes' tiers,
/// so the three Chakra modifiers now hang off `onehundredhands` instead.
/// Without this, a Monk who had invested in the old Hundred Fists spec
/// would find their Chakras suddenly parented to a node they have no
/// points in, stranding every point spent below it. Moving the rank
/// across (rather than granting a respec) keeps their total points spent
/// identical and keeps those Chakras unlocked.
///
/// Guarded on `onehundredhands` being unallocated so it can never fuse
/// two real investments into one over-ranked node, and clamped to the
/// Specialization `max_rank` of 4 for the same reason `passive_node_rank`
/// callers can't rely on stored ranks being in range.
pub(crate) fn migrate_flowlikewater_swap(character: &mut Character) {
    for tree in [&mut character.passive_allocations, &mut character.secondary_passive_allocations] {
        // Both trees, since Split Personality can run Monk as a secondary.
        if let Some(rank) = tree.remove("hundredfists") {
            if rank > 0 && tree.get("onehundredhands").copied().unwrap_or(0) == 0 {
                tree.insert("onehundredhands".to_string(), rank.min(4));
            }
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
            reforge_crit_used: false,
            recombine_crit_used: false,
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
            reforge_crit_used: false,
            recombine_crit_used: false,
            crit_bonus_affixes: vec![],
        }
    }

    #[test]
    fn marks_both_flags_used_for_an_over_cap_item() {
        let mut item = item_with_n_affixes(7); // lokati's exact reported count
        migrate_crit_lineage_backfill(&mut item);
        assert!(item.reforge_crit_used, "an over-cap item must be defensively marked as having used its reforge crit");
        assert!(item.recombine_crit_used, "an over-cap item must be defensively marked as having used its recombine crit");
    }

    #[test]
    fn leaves_a_normal_4_affix_item_alone() {
        // 4 affixes is fully explainable by normal currency crafting
        // alone (Transmute/Augment/Regal/Exalt) - no evidence either
        // crit source ever fired, so this must NOT be touched.
        let mut item = item_with_n_affixes(4);
        migrate_crit_lineage_backfill(&mut item);
        assert!(!item.reforge_crit_used);
        assert!(!item.recombine_crit_used);
    }

    #[test]
    fn leaves_a_5_affix_item_flagged_but_removes_nothing() {
        let mut item = item_with_n_affixes(5);
        migrate_crit_lineage_backfill(&mut item);
        assert!(item.reforge_crit_used);
        assert!(item.recombine_crit_used);
        assert_eq!(item.affixes.len(), 5, "the migration must never remove any of the player's existing affixes, only stop further growth");
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
    fn does_not_fuse_two_real_investments() {
        // Nobody live has both, but if they did, silently summing them
        // into one over-ranked node would be worse than dropping the move.
        let mut c = monk_with(&[("hundredfists", 3), ("onehundredhands", 2)], &[]);
        migrate_flowlikewater_swap(&mut c);
        assert_eq!(c.passive_allocations.get("onehundredhands").copied(), Some(2), "an existing allocation must win, not be overwritten or added to");
        assert!(!c.passive_allocations.contains_key("hundredfists"));
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

