use super::*;

/// Applies `f` to every item a character has - both equipped slots and
/// the bag - the "every item this character owns" iteration every
/// gear-value migration needs (see `run_item_migrations`'s callers).
///
/// Deliberately unguarded (see `Character::owned_items_mut_unguarded`): a
/// migration that skipped locked or disenchant-protected items would leave
/// a permanently inconsistent save - half a roster on the new shape and
/// half on the old, with no second chance to finish the job, because every
/// migration here is marker-gated to run exactly once. That is strictly
/// worse than the drift it repairs.
pub(crate) fn for_each_item_mut(character: &mut Character, mut f: impl FnMut(&mut Item)) {
    for item in character.owned_items_mut_unguarded() {
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

/// Affix tier curve retrofit (2026-09-02, `docs/affix_curve_spec.md`
/// §4.1, owner-ruled retroactive).
///
/// Moves every stored affix value from the old linear tier term onto the
/// curve, and applies the §7 `CritMultiplier` halving to already-rolled
/// crit-damage affixes in the same pass.
///
/// # Why a pure RATIO, and not the shape §4.1 actually specifies
///
/// **Deliberate, owner-approved deviation. Do not "restore" the spec's
/// version — it would quietly take polish away from players.**
///
/// §4.1 says to rewrite each value as `per_tier * f(T) * preserved_jitter`.
/// That requires recovering `preserved_jitter` from the stored value, and
/// the obvious way to recover it (`affix_quality_percent`, `affix.rs`)
/// **clamps jitter to 0.85..1.15**. Rolled jitter does live in that band —
/// but `Character::polish` drives an affix's effective jitter as high as
/// `POWER_ROLL_RANGE.end` = **1.20**, above the roll ceiling. Reconstructing
/// through a clamp would therefore silently confiscate the polish from
/// every polished affix in the game. The owner's own live tier-7 item
/// carries a divine roll sitting at exactly 1.20 for that reason.
///
/// `value * f(T)/T` is the same arithmetic — `(per_tier * T * j) * f(T)/T`
/// is `per_tier * f(T) * j` — with nothing to reconstruct and therefore
/// nothing to lose. Jitter is preserved bit-for-bit at any magnitude,
/// polish included, and it makes §4.4's quality%-drift problem a no-op:
/// `affix_quality_percent` divides the stored value by
/// `affix_base_value(tier)`, and this scales numerator and denominator by
/// the identical factor, so every displayed quality% stays exactly where
/// it was.
///
/// # Idempotency — this migration is NOT idempotent
///
/// Unlike the `.max()`-gated accuracy passes above, running this twice
/// would apply the cut twice. It is marker-guarded like every other
/// one-off grant in `AdventureManager::new`, and that guard is the only
/// thing standing between a correct rescale and a second one. Same
/// reasoning as `migrate_crit_value_nerf`, which halves rather than
/// recomputing for the same jitter-preserving reason.
///
/// Covers `sacred_affix` as well as `affixes` — it is a second stored
/// value on the same footing, rolled through `affix_base_value` by
/// `make_item_sacred` and rescaled by `sync_tier_to` alongside the rest.
///
/// `PERFECT_QUALITY_MULT` needs no special handling here, unlike
/// `migrate_gloves_speed_rebalance`: that one RECOMPUTES from scratch and
/// so has to reapply the 20% by hand, while this one only ever multiplies
/// what is already stored, so an already-boosted value stays
/// proportionally boosted.
pub(crate) fn migrate_affix_tier_curve(item: &mut Item) {
    let scale = affix_tier_curve(item.tier) / item.tier.max(1) as f64;
    for (affix, value) in item.affixes.iter_mut() {
        *value *= scale;
        // The §7 halving, applied to already-rolled values. New rolls get
        // it from `affix_def`'s changed default; stored ones need it here,
        // or an existing crit-damage affix keeps twice the coefficient it
        // is now defined to have and reads as a permanent 100% roll.
        if matches!(affix, Affix::CritMultiplier) {
            *value *= 0.5;
        }
    }
    if let Some((affix, value)) = item.sacred_affix.as_mut() {
        *value *= scale;
        if matches!(affix, Affix::CritMultiplier) {
            *value *= 0.5;
        }
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
    // LAST on purpose. It scales every stored affix value by
    // `f(tier)/tier`, so it must run AFTER every migration above that
    // reads or rewrites an affix value against the OLD linear
    // `affix_base_value` - `migrate_item_accuracy` and
    // `migrate_krangle_accuracy` both floor stored values at
    // `affix_base_value(affix, tier)`, which now returns the CURVED value.
    // On an already-migrated data dir their markers are long since set so
    // they never run again; ordering it last is what keeps a fresh
    // install or a restored backup correct too, where every marker is
    // absent and the whole array runs in sequence.
    ("adventure-affix-tier-curve-marker.json", migrate_affix_tier_curve),
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
        if crate::state::load_json::<bool>(data_path(marker)).is_some() {
            continue;
        }
        for character in characters.values_mut() {
            for_each_item_mut(character, f);
        }
        if let Err(err) = crate::state::save_json(characters_path, characters) {
            tracing::error!("Failed to persist item migration '{marker}' to {}: {err}", characters_path.display());
        }
        if let Err(err) = crate::state::save_json(data_path(marker), &true) {
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

/// Unified Unique Shards (2026-08-19) - Celestial Shard merges into
/// Unique Shard as one currency (see `CraftAction::UniqueShard`'s own
/// doc). 1:1 merge, per the owner's ruling: adds a held `CelestialShard`
/// count straight onto `UniqueShard`'s own, then removes the
/// `CelestialShard` entry entirely - no character can hold a nonzero
/// `CelestialShard` count after this runs. No-op (and safe to re-run,
/// though the marker file already prevents that in practice) for a
/// character with no `CelestialShard` tokens at all.
///
/// Deliberately does NOT touch `Item::unique_affix` anywhere - only the
/// unspent CURRENCY migrates. An item that already carries
/// `UniqueAffix::CelestialConversion` (granted before this merge existed)
/// keeps it exactly as-is; there is nothing to remap on the item side.
pub(crate) fn migrate_celestial_shard_into_unique_shard(character: &mut Character) {
    let celestial = character.craft_token_count(CraftAction::CelestialShard);
    if celestial == 0 {
        return;
    }
    character.add_craft_token(CraftAction::UniqueShard, celestial);
    character.craft_tokens.retain(|(action, _)| *action != CraftAction::CelestialShard);
}

/// Refunds every point spent on the two retired dead nodes, `stillwater`
/// (Monk) and `sacredoverflow` (Paladin).
///
/// **NO BALANCE CONSEQUENCE - this migration cannot change any
/// character's combat output, and a reader auditing migrations for
/// balance impact can stop at this line.** Both nodes are no-ops today
/// and always have been: nothing anywhere in `game/src` ever passes
/// `"stillwater"` to a by-key lookup, and `sacredoverflow` is the tree's
/// last `PassiveEffect::NotYetImplemented`, whose `magnitude_at_rank` is
/// a literal `0.0`. Removing their allocations therefore returns points
/// and changes nothing else. (Found by the 2026-09-03
/// advertised-vs-actual sweep; retirement ruled by the owner over both
/// building the mechanics and rewording the copy.)
///
/// **Why removing the entry IS the refund.** There is no separate
/// "points available" counter to keep in sync: every site that asks how
/// many points a character has spent derives it by summing the map
/// itself (`manager.rs`'s allocate-time guard and the three
/// `adventure_web.rs` render sites all do
/// `passive_allocations.values().sum()`). So a removed entry returns its
/// points automatically, and the two numbers cannot disagree because
/// there is only one number.
///
/// **Refund, deliberately, rather than remapping onto the replacement
/// nodes.** `migrate_flowlikewater_swap` above remaps, and it is right to
/// - that was the same mechanic moving between two tiers. This is
/// different mechanics arriving. A player who chose a defensive-uptime
/// node and silently received a party-support node would have been
/// wronged in a new way by the fix for the old one. They get the points
/// back and spend them themselves.
///
/// **Both trees, and this is the part most likely to be simplified away
/// by mistake.** Split Personality can run Monk or Paladin as a
/// SECONDARY archetype, so an affected allocation can live in
/// `secondary_passive_allocations` and nowhere else. A migration that
/// touched only the primary map would silently miss exactly those
/// characters - and, being marker-guarded, would never get a second
/// chance at them. Same two-map loop as `migrate_flowlikewater_swap`.
///
/// Safe on a character with neither node allocated (both `remove`s are
/// no-ops), and safe against a tree that has already dropped the node
/// definitions - `remove` needs no node to exist.
pub(crate) fn migrate_refund_retired_dead_nodes(character: &mut Character) {
    for tree in [&mut character.passive_allocations, &mut character.secondary_passive_allocations] {
        for key in RETIRED_DEAD_NODE_KEYS {
            tree.remove(key);
        }
    }
}

/// The node keys `migrate_refund_retired_dead_nodes` clears. Named rather
/// than inlined so the test and the migration cannot drift apart.
///
/// **These keys are retired permanently and must never be reused.** The
/// replacement nodes going into the same two tree slots take NEW keys, so
/// that any allocation this migration fails to clear - a save lost
/// between the mutation and the marker write, a character restored from a
/// backup predating it, a path nobody has thought of - resolves to
/// nothing instead of silently onto a mechanic the player never chose.
/// Making the bad outcome unrepresentable beats making the migration
/// perfect.
pub(crate) const RETIRED_DEAD_NODE_KEYS: [&str; 2] = ["stillwater", "sacredoverflow"];

/// Echo replaces Lingering Effect (2026-08-21, docs/echo_spec.md) - renames
/// every existing `Affix::LingeringEffect` entry, on every item this
/// character owns (equipped + bag), to `Affix::Echo` at HALF its stored
/// value, across the board. `Affix::LingeringEffect` itself stays declared
/// permanently (see its own doc - no `#[serde(other)]`/alias exists, so
/// deleting the variant outright would break deserialization of any
/// not-yet-migrated save) - this migration, not a deleted variant, is what
/// actually clears it off live items; nothing else ever produces or reads
/// it again afterward.
///
/// A character-level migration (not an `ITEM_MIGRATIONS` entry) purely so
/// it can log per-item with the owning character's name, same discipline
/// as `migrate_duplicate_unique_effects` just below - a plain
/// `fn(&mut Item)` has no character context to log with, and none of the
/// 8 existing item-level migrations log at all. No-op (silent) for a
/// character with no `LingeringEffect`-affixed item at all.
pub(crate) fn migrate_lingering_effect_to_echo(character: &mut Character) {
    let mut converted: Vec<(String, String)> = Vec::new();
    for slot in EQUIP_SLOTS {
        if let Some(item) = character.equipped_mut(slot) {
            for (affix, value) in item.affixes.iter_mut() {
                if *affix == Affix::LingeringEffect {
                    *affix = Affix::Echo;
                    *value *= 0.5;
                    converted.push((format!("{slot:?}"), item.name.clone()));
                }
            }
        }
    }
    for item in character.inventory.iter_mut() {
        for (affix, value) in item.affixes.iter_mut() {
            if *affix == Affix::LingeringEffect {
                *affix = Affix::Echo;
                *value *= 0.5;
                converted.push(("bag".to_string(), item.name.clone()));
            }
        }
    }
    if !converted.is_empty() {
        tracing::info!(
            "lingering-effect-to-echo migration: character={} converted {} affix instance(s): {converted:?}",
            character.display_name,
            converted.len()
        );
    }
}

/// One-time cleanup for the duplicate-equipped-uniques bug (2026-08-21,
/// see docs/duplicate_unique_effects_spec.md) - equipping the SAME
/// `UniqueAffix` in two or more slots at once used to be reachable
/// through two mutation points that never re-ran equip-time's own
/// one-per-unique rule (`Character::apply_unique_affix`'s commit step
/// and the legacy `CraftAction::CelestialShard` branch - both fixed, see
/// `Character::has_conflicting_unique_affix_value`). This is the
/// retroactive repair for characters who already hit it before the fix
/// shipped. For every `UniqueAffix` currently duplicated across 2+
/// equipped slots, EVERY copy is unequipped - not "keep one, drop the
/// rest" (no silent winner-picking, the owner's own call to make) - into
/// the bag, intact: nothing destroyed, nothing refunded, no stat
/// changes. The player re-equips whichever one they want. Naturally
/// idempotent - after the first run no equipped group has 2+ items
/// sharing a unique, so a second run finds nothing to touch, on this
/// character or any other. Logs one line per AFFECTED character (only -
/// a character with nothing to clean stays silent) naming every slot and
/// item moved, for the deploy report.
pub(crate) fn migrate_duplicate_unique_effects(character: &mut Character) {
    let mut by_unique: std::collections::HashMap<UniqueAffix, Vec<EquipSlot>> = std::collections::HashMap::new();
    for slot in EQUIP_SLOTS {
        if let Some(unique) = character.equipped(slot).as_ref().and_then(|i| i.unique_affix) {
            by_unique.entry(unique).or_default().push(slot);
        }
    }
    let mut moved: Vec<(EquipSlot, String)> = Vec::new();
    for slots in by_unique.into_values().filter(|slots| slots.len() > 1) {
        for slot in slots {
            if let Some(item) = character.equipped_mut(slot).take() {
                moved.push((slot, item.name.clone()));
                character.inventory.push(item);
            }
        }
    }
    if !moved.is_empty() {
        tracing::info!("duplicate-unique-effects cleanup: character={} unequipped {} item(s): {moved:?}", character.display_name, moved.len());
    }
}

/// Character-level counterpart to `ITEM_MIGRATIONS` - same
/// (marker filename, mutation) shape, for one-time corrections that touch
/// a character's own fields rather than their gear.
pub(crate) const CHARACTER_MIGRATIONS: &[(&str, fn(&mut Character))] = &[
    ("adventure-flowlikewater-swap-marker.json", migrate_flowlikewater_swap),
    ("adventure-celestial-shard-into-unique-shard-marker.json", migrate_celestial_shard_into_unique_shard),
    ("adventure-duplicate-unique-effects-cleanup-marker.json", migrate_duplicate_unique_effects),
    ("adventure-lingering-effect-to-echo-marker.json", migrate_lingering_effect_to_echo),
    ("adventure-refund-retired-dead-nodes-marker.json", migrate_refund_retired_dead_nodes),
];

/// Runs each pending entry of `CHARACTER_MIGRATIONS` over every character -
/// same save-then-mark-done-per-migration crash-safety contract
/// `run_item_migrations` documents (deliberately NOT batched into one save
/// at the end, so a crash between the save and the marker write can't make
/// an already-applied migration look pending and re-run on top of mutated
/// data).
pub(crate) fn run_character_migrations(characters_path: &PathBuf, characters: &mut HashMap<String, Character>) {
    for (marker, f) in CHARACTER_MIGRATIONS.iter().copied() {
        if crate::state::load_json::<bool>(data_path(marker)).is_some() {
            continue;
        }
        for character in characters.values_mut() {
            f(character);
        }
        if let Err(err) = crate::state::save_json(characters_path, characters) {
            tracing::error!("Failed to persist character migration '{marker}' to {}: {err}", characters_path.display());
        }
        if let Err(err) = crate::state::save_json(data_path(marker), &true) {
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

#[cfg(test)]
mod lingering_effect_to_echo_tests {
    use super::*;

    fn item_with_affix(slot: EquipSlot, affix: Affix, value: f64, name: &str) -> Item {
        Item {
            id: name.to_string(),
            name: name.to_string(),
            slot,
            tier: 10,
            power: 100.0,
            power_roll: 1.0,
            max_uses: None,
            uses: 0,
            affixes: vec![(affix, value)],
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
    fn converts_an_equipped_item_and_halves_its_value() {
        let mut c = Character::new("test".to_string());
        *c.equipped_mut(EquipSlot::Helm) = Some(item_with_affix(EquipSlot::Helm, Affix::LingeringEffect, 0.08, "old helm"));
        migrate_lingering_effect_to_echo(&mut c);
        let item = c.equipped(EquipSlot::Helm).as_ref().unwrap();
        assert_eq!(item.affixes, vec![(Affix::Echo, 0.04)], "the affix must be renamed AND halved in one step");
    }

    #[test]
    fn converts_a_bagged_item_too() {
        let mut c = Character::new("test".to_string());
        c.inventory.push(item_with_affix(EquipSlot::Boots, Affix::LingeringEffect, 0.10, "old boots"));
        migrate_lingering_effect_to_echo(&mut c);
        assert_eq!(c.inventory[0].affixes, vec![(Affix::Echo, 0.05)]);
    }

    #[test]
    fn leaves_every_other_affix_on_the_item_untouched() {
        let mut c = Character::new("test".to_string());
        let mut item = item_with_affix(EquipSlot::Weapon, Affix::LingeringEffect, 0.06, "mixed weapon");
        item.affixes.push((Affix::CritChance, 0.05));
        *c.equipped_mut(EquipSlot::Weapon) = Some(item);
        migrate_lingering_effect_to_echo(&mut c);
        let item = c.equipped(EquipSlot::Weapon).as_ref().unwrap();
        assert_eq!(item.affixes, vec![(Affix::Echo, 0.03), (Affix::CritChance, 0.05)], "order and every other affix must survive untouched");
    }

    #[test]
    fn is_a_no_op_for_a_character_with_no_lingering_effect_affix_at_all() {
        let mut c = Character::new("test".to_string());
        *c.equipped_mut(EquipSlot::Weapon) = Some(item_with_affix(EquipSlot::Weapon, Affix::CritChance, 0.05, "plain weapon"));
        let before = c.equipped(EquipSlot::Weapon).clone();
        migrate_lingering_effect_to_echo(&mut c);
        assert_eq!(c.equipped(EquipSlot::Weapon).as_ref().map(|i| &i.affixes), before.as_ref().map(|i| &i.affixes));
    }

    #[test]
    fn is_idempotent_on_rerun() {
        // The marker file is what actually prevents a real re-run in
        // production, but the migration itself must also be safe to call
        // twice - same convention every other migration's own
        // `is_idempotent_on_rerun` test establishes: after the first run
        // there is no `LingeringEffect` entry left to convert, so a second
        // run must be a pure no-op.
        let mut c = Character::new("test".to_string());
        *c.equipped_mut(EquipSlot::Helm) = Some(item_with_affix(EquipSlot::Helm, Affix::LingeringEffect, 0.08, "old helm"));
        migrate_lingering_effect_to_echo(&mut c);
        let once = c.equipped(EquipSlot::Helm).as_ref().unwrap().affixes.clone();
        migrate_lingering_effect_to_echo(&mut c);
        assert_eq!(c.equipped(EquipSlot::Helm).as_ref().unwrap().affixes, once, "a second application must not halve the value again");
    }

    #[test]
    fn converts_multiple_items_across_slots_and_bag_in_one_pass() {
        let mut c = Character::new("test".to_string());
        *c.equipped_mut(EquipSlot::Helm) = Some(item_with_affix(EquipSlot::Helm, Affix::LingeringEffect, 0.08, "helm"));
        *c.equipped_mut(EquipSlot::Body) = Some(item_with_affix(EquipSlot::Body, Affix::LingeringEffect, 0.04, "body"));
        c.inventory.push(item_with_affix(EquipSlot::Boots, Affix::LingeringEffect, 0.02, "spare boots"));
        migrate_lingering_effect_to_echo(&mut c);
        assert_eq!(c.equipped(EquipSlot::Helm).as_ref().unwrap().affixes, vec![(Affix::Echo, 0.04)]);
        assert_eq!(c.equipped(EquipSlot::Body).as_ref().unwrap().affixes, vec![(Affix::Echo, 0.02)]);
        assert_eq!(c.inventory[0].affixes, vec![(Affix::Echo, 0.01)]);
    }
}

#[cfg(test)]
mod celestial_shard_into_unique_shard_tests {
    use super::*;

    fn character_with_tokens(tokens: &[(CraftAction, u32)]) -> Character {
        let mut c = Character::new("test".to_string());
        c.craft_tokens = tokens.to_vec();
        c
    }

    #[test]
    fn merges_celestial_count_onto_unique_and_removes_celestial_entry() {
        let mut c = character_with_tokens(&[(CraftAction::CelestialShard, 3), (CraftAction::UniqueShard, 2)]);
        migrate_celestial_shard_into_unique_shard(&mut c);
        assert_eq!(c.craft_token_count(CraftAction::CelestialShard), 0, "CelestialShard must be fully drained");
        assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 5, "1:1 merge - 3 + 2 = 5");
        assert!(!c.craft_tokens.iter().any(|(a, _)| *a == CraftAction::CelestialShard), "the CelestialShard entry itself must be removed, not just zeroed");
    }

    #[test]
    fn merges_onto_a_character_who_never_held_unique_shard_at_all() {
        let mut c = character_with_tokens(&[(CraftAction::CelestialShard, 4)]);
        migrate_celestial_shard_into_unique_shard(&mut c);
        assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 4, "a fresh UniqueShard entry must be created, not silently dropped");
        assert_eq!(c.craft_token_count(CraftAction::CelestialShard), 0);
    }

    #[test]
    fn is_a_no_op_for_a_character_who_never_held_celestial_shard() {
        let mut c = character_with_tokens(&[(CraftAction::UniqueShard, 7), (CraftAction::Transmute, 1)]);
        migrate_celestial_shard_into_unique_shard(&mut c);
        assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 7, "an already-held UniqueShard balance must be untouched when there's nothing to merge");
        assert_eq!(c.craft_token_count(CraftAction::Transmute), 1, "unrelated tokens must be untouched");
        assert!(!c.craft_tokens.iter().any(|(a, _)| *a == CraftAction::CelestialShard));
    }

    #[test]
    fn is_idempotent_on_rerun() {
        // The marker file is what actually prevents a real re-run in
        // production, but the migration itself must also be safe to call
        // twice - defense in depth, same convention
        // `migrate_crit_flag_to_affix_tracking_tests` establishes.
        let mut c = character_with_tokens(&[(CraftAction::CelestialShard, 3), (CraftAction::UniqueShard, 2)]);
        migrate_celestial_shard_into_unique_shard(&mut c);
        migrate_celestial_shard_into_unique_shard(&mut c);
        assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 5, "a second run must not double-count - there's nothing left to merge after the first run");
        assert_eq!(c.craft_token_count(CraftAction::CelestialShard), 0);
    }

    #[test]
    fn never_touches_item_unique_affix() {
        // Only the unspent CURRENCY migrates - an item that already
        // carries the granted effect keeps it exactly as-is (see the
        // migration's own doc for why there's nothing to remap there).
        let mut c = character_with_tokens(&[(CraftAction::CelestialShard, 1)]);
        let mut item = generate_item_at_tier(EquipSlot::Weapon, 10, &mut rand::thread_rng());
        item.unique_affix = Some(UniqueAffix::CelestialConversion);
        c.weapon = Some(item);
        migrate_celestial_shard_into_unique_shard(&mut c);
        assert_eq!(c.weapon.as_ref().unwrap().unique_affix, Some(UniqueAffix::CelestialConversion), "an already-granted unique affix must never be touched by the currency migration");
    }
}

#[cfg(test)]
mod duplicate_unique_effects_cleanup_tests {
    use super::*;

    fn character_with_equipped_uniques(name: &str, items: &[(EquipSlot, UniqueAffix)]) -> Character {
        let mut c = Character::new(name.to_string());
        for &(slot, unique) in items {
            let mut item = generate_item_at_tier(slot, 10, &mut rand::thread_rng());
            item.unique_affix = Some(unique);
            *c.equipped_mut(slot) = Some(item);
        }
        c
    }

    #[test]
    fn unequips_all_copies_of_a_duplicated_unique() {
        let mut c = character_with_equipped_uniques("dupe", &[(EquipSlot::Helm, UniqueAffix::SplitPersonality), (EquipSlot::Body, UniqueAffix::SplitPersonality)]);
        migrate_duplicate_unique_effects(&mut c);
        assert!(c.helm.is_none(), "both copies must be unequipped, not just the extra");
        assert!(c.body.is_none());
        assert_eq!(c.inventory.iter().filter(|i| i.unique_affix == Some(UniqueAffix::SplitPersonality)).count(), 2, "both items must land in the bag, intact");
    }

    #[test]
    fn moved_items_keep_every_field_unchanged() {
        let mut c = character_with_equipped_uniques("intact", &[(EquipSlot::Weapon, UniqueAffix::CelestialConversion), (EquipSlot::Helm, UniqueAffix::CelestialConversion)]);
        let weapon_id = c.weapon.as_ref().unwrap().id.clone();
        let weapon_tier = c.weapon.as_ref().unwrap().tier;
        migrate_duplicate_unique_effects(&mut c);
        let moved = c.inventory.iter().find(|i| i.id == weapon_id).expect("the weapon must be in the bag now");
        assert_eq!(moved.tier, weapon_tier, "nothing about the item's own stats changes - pure re-slotting");
        assert_eq!(moved.unique_affix, Some(UniqueAffix::CelestialConversion), "the unique effect itself is preserved, not stripped");
    }

    #[test]
    fn non_duplicated_uniques_are_left_alone() {
        let mut c = character_with_equipped_uniques("fine", &[(EquipSlot::Weapon, UniqueAffix::CelestialConversion), (EquipSlot::Helm, UniqueAffix::SplitPersonality)]);
        migrate_duplicate_unique_effects(&mut c);
        assert_eq!(c.weapon.as_ref().and_then(|i| i.unique_affix), Some(UniqueAffix::CelestialConversion), "a single copy of each different unique must never be touched");
        assert_eq!(c.helm.as_ref().and_then(|i| i.unique_affix), Some(UniqueAffix::SplitPersonality));
    }

    #[test]
    fn a_character_with_no_uniques_at_all_is_untouched() {
        let mut c = Character::new("plain".to_string());
        let weapon_id_before = c.weapon.as_ref().unwrap().id.clone();
        migrate_duplicate_unique_effects(&mut c);
        assert_eq!(c.weapon.as_ref().unwrap().id, weapon_id_before, "no uniques at all - nothing should move");
        assert!(c.inventory.is_empty(), "starter kit fills every slot, so the bag should stay empty");
    }

    #[test]
    fn is_idempotent_on_rerun() {
        // The marker file is what actually prevents a real re-run in
        // production, but the migration itself must also be safe to call
        // twice - same convention `celestial_shard_into_unique_shard_tests::is_idempotent_on_rerun`
        // establishes.
        let mut c = character_with_equipped_uniques("rerun", &[(EquipSlot::Weapon, UniqueAffix::CelestialConversion), (EquipSlot::Body, UniqueAffix::CelestialConversion)]);
        migrate_duplicate_unique_effects(&mut c);
        let inventory_after_first = c.inventory.len();
        migrate_duplicate_unique_effects(&mut c);
        assert_eq!(c.inventory.len(), inventory_after_first, "nothing left equipped to duplicate - a second run must be a pure no-op");
        assert!(c.weapon.is_none() && c.body.is_none());
    }

    #[test]
    fn every_slot_sharing_the_same_unique_all_get_unequipped() {
        // The real live case this migration was written for (xDaido, see
        // the fit report's scan) - every single slot carrying the same
        // unique, not just two.
        //
        // Named `all_five_slots_...` with a hardcoded `5` until 2026-09-03,
        // when §8 took the slot count to nine and it became the one test
        // in the suite that failed on the count alone. Counting
        // `EQUIP_SLOTS` instead of a literal, since what this asserts is
        // "every slot was emptied into the bag", not "five were".
        let mut c = character_with_equipped_uniques("every_slot", &EQUIP_SLOTS.map(|s| (s, UniqueAffix::CelestialConversion)));
        migrate_duplicate_unique_effects(&mut c);
        for slot in EQUIP_SLOTS {
            assert!(c.equipped(slot).is_none(), "{slot:?} must be unequipped");
        }
        assert_eq!(c.inventory.iter().filter(|i| i.unique_affix == Some(UniqueAffix::CelestialConversion)).count(), EQUIP_SLOTS.len());
    }

    /// Reproduces the exact 2026-08-21 fit-report scan figures (7
    /// characters, 18 items, split 3 splitPersonality/4 celestialConversion)
    /// against a synthetic multi-character fixture shaped identically to
    /// the real `adventure-characters.json` findings - proves the
    /// reported blast radius is a real, reproducible property of this
    /// migration's own logic, not a one-off manual read of the save file.
    #[test]
    fn reproduces_the_live_scan_figures_from_a_seven_character_fixture() {
        let mut characters: std::collections::HashMap<String, Character> = std::collections::HashMap::new();
        let cases: &[(&str, &[(EquipSlot, UniqueAffix)])] = &[
            ("zolaries", &[(EquipSlot::Helm, UniqueAffix::SplitPersonality), (EquipSlot::Body, UniqueAffix::SplitPersonality)]),
            ("qugetus_", &[(EquipSlot::Helm, UniqueAffix::SplitPersonality), (EquipSlot::Body, UniqueAffix::SplitPersonality)]),
            ("colonyna", &[(EquipSlot::Weapon, UniqueAffix::SplitPersonality), (EquipSlot::Body, UniqueAffix::SplitPersonality)]),
            ("drewm1022", &[(EquipSlot::Weapon, UniqueAffix::CelestialConversion), (EquipSlot::Body, UniqueAffix::CelestialConversion)]),
            ("xborntokillx", &[(EquipSlot::Weapon, UniqueAffix::CelestialConversion), (EquipSlot::Helm, UniqueAffix::CelestialConversion)]),
            ("gorshie", &[(EquipSlot::Body, UniqueAffix::CelestialConversion), (EquipSlot::Gloves, UniqueAffix::CelestialConversion), (EquipSlot::Boots, UniqueAffix::CelestialConversion)]),
            (
                "xdaido",
                &[
                    (EquipSlot::Weapon, UniqueAffix::CelestialConversion),
                    (EquipSlot::Helm, UniqueAffix::CelestialConversion),
                    (EquipSlot::Body, UniqueAffix::CelestialConversion),
                    (EquipSlot::Gloves, UniqueAffix::CelestialConversion),
                    (EquipSlot::Boots, UniqueAffix::CelestialConversion),
                ],
            ),
        ];
        for &(login, items) in cases {
            characters.insert(login.to_string(), character_with_equipped_uniques(login, items));
        }
        // A few unaffected characters mixed in, same as the real save -
        // proves the migration doesn't false-positive on normal builds.
        characters.insert("clean_one".to_string(), Character::new("clean_one".to_string()));
        characters.insert("clean_two".to_string(), character_with_equipped_uniques("clean_two", &[(EquipSlot::Weapon, UniqueAffix::SplitPersonality)]));

        let mut affected_characters = 0;
        let mut total_items_moved = 0;
        for character in characters.values_mut() {
            let inventory_before = character.inventory.len();
            migrate_duplicate_unique_effects(character);
            let moved = character.inventory.len() - inventory_before;
            if moved > 0 {
                affected_characters += 1;
                total_items_moved += moved;
            }
        }

        assert_eq!(affected_characters, 7, "must match the live scan's reported blast radius exactly");
        assert_eq!(total_items_moved, 18, "2+2+2+2+2+3+5 = 18, the live scan's reported item total");
    }
}


/// The affix tier curve (2026-09-02, `docs/affix_curve_spec.md` §1-§7).
///
/// The curve is arithmetic, so most of it is checked directly against the
/// spec's own ratified anchors and tables rather than against behaviour.
/// The exceptions are the three tier-GROWTH sites, which are the
/// load-bearing half of the change (§4.1) and are tested through real
/// items: a curve that is correct at roll time and leaks at growth time
/// looks completely right in every static table and is wrong within a few
/// crafts.
#[cfg(test)]
mod affix_curve_tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{RngCore, SeedableRng};

    /// Every anchor §1 ratifies, checked as an anchor rather than as a
    /// sampled value. If one of these moves the curve is a different
    /// curve and the spec no longer describes the game.
    #[test]
    fn the_curve_hits_every_ratified_anchor() {
        assert_eq!(affix_tier_curve(1), 1.0, "f(1) = 1 EXACTLY - a new character's first drop must read exactly as it always has");
        assert_eq!(affix_tier_curve(100), 10.0, "f(100) = 10 EXACTLY - tier 100 lands on what tier 10 used to deliver; this is the compression point the curve exists for");

        // Continuity in VALUE at the knee. The two halves meet with no
        // seam: sqrt(100) and 10 * 1^0.289 are both exactly 10.
        let left = (100.0_f64).sqrt();
        let right = AFFIX_CURVE_KNEE.sqrt() * (100.0_f64 / AFFIX_CURVE_KNEE).powf(AFFIX_CURVE_EXPONENT);
        assert!((left - right).abs() < 1e-12, "the two halves must be continuous in value at T=100: {left} vs {right}");

        // The exponent's own anchor: the first 1,000-tier step past the
        // knee is a doubling. `0.289` is the ratified rounding of
        // ln(2)/ln(11) and lands 0.0155% short of exact - that shortfall
        // is asserted rather than tolerated, so nobody "fixes" the
        // constant into ln2/ln11 without noticing the spec chose the
        // literal deliberately (§1, "Use 0.289").
        let doubling = affix_tier_curve(1100) / affix_tier_curve(100);
        assert!((doubling - 1.99968913).abs() < 1e-6, "11^0.289 must be 1.99968913 - got {doubling}");
        assert!(doubling < 2.0, "the ratified 0.289 lands just SHORT of a true doubling; if this passes 2.0 someone has swapped in ln2/ln11");
    }

    /// §2: `f(T) < T` for all T > 1, and `f(T)/T` strictly decreasing.
    /// The spec calls this a structural property rather than a property
    /// of the sampled range - the curve can never cross back over the old
    /// linear line at any tier, ever.
    #[test]
    fn the_curve_is_sublinear_everywhere_and_never_crosses_back() {
        assert_eq!(affix_tier_curve(1), 1.0, "T=1 is the one tier where the curve equals the old linear term");
        let mut previous_ratio = 1.0;
        for tier in [2u32, 10, 50, 100, 101, 500, 1_000, 10_000, 100_000, 1_000_000] {
            let f = affix_tier_curve(tier);
            assert!(f < tier as f64, "f({tier}) = {f} must be strictly below the old linear term {tier}");
            let ratio = f / tier as f64;
            assert!(ratio < previous_ratio, "f(T)/T must be MONOTONICALLY decreasing - it rose at T={tier} ({ratio} vs {previous_ratio}), which would mean the curve can cross back");
            previous_ratio = ratio;
        }
    }

    /// §6's computed table, spot-checked at the tiers the owner asked for.
    /// These are the numbers a player will actually see, so they are
    /// asserted as values rather than as properties.
    #[test]
    fn the_computed_affix_table_matches_the_spec() {
        // Elementals (0.0225): cold, fire, lightning, divine, chaos.
        for (tier, expected) in [(1u32, 0.0225), (7, 0.059529), (20, 0.100623), (50, 0.159099), (100, 0.225)] {
            let got = affix_base_value(Affix::ColdDamage, tier);
            assert!((got - expected).abs() < 1e-6, "ColdDamage at T={tier}: expected {expected}, got {got}");
        }
        // IncreasedLife (0.03) - the owner's "max hp" column.
        for (tier, expected) in [(1u32, 0.03), (7, 0.079373), (20, 0.134164), (50, 0.212132), (100, 0.30)] {
            let got = affix_base_value(Affix::IncreasedLife, tier);
            assert!((got - expected).abs() < 1e-6, "IncreasedLife at T={tier}: expected {expected}, got {got}");
        }
        // Spec §6's own T=1000 / T=10000 rows, which sit past the knee and
        // so exercise the power-law half.
        assert!((affix_base_value(Affix::IncreasedDamage, 1_000) - 0.583608).abs() < 1e-5);
        assert!((affix_base_value(Affix::IncreasedDamage, 10_000) - 1.135329).abs() < 1e-5);
    }

    /// §7 / R4: the halving and the curve COMPOSE. Both cuts are applied,
    /// and the table here is the one the owner ratified.
    #[test]
    fn crit_multiplier_carries_both_cuts_not_one() {
        assert_eq!(affix_balance(Affix::CritMultiplier).0, 0.025, "the affix_def default must be the halved coefficient - if this is 0.05 the halving was reverted");

        // today (0.05 * T) -> both cuts (0.025 * f(T)).
        for (tier, today, both) in [(7u32, 0.35, 0.066144), (20, 1.00, 0.111803), (50, 2.50, 0.176777), (100, 5.00, 0.25)] {
            let got = affix_base_value(Affix::CritMultiplier, tier);
            assert!((got - both).abs() < 1e-6, "CritMultiplier at T={tier}: expected {both} (was {today} before this change), got {got}");
        }

        // The relative-weight invariant §1 promises: every OTHER affix
        // keeps its exact ratio against every other affix at every tier.
        // CritMultiplier is the sole ratified exception, so its ratio to
        // DivineDamage is the one that moves - from 2.22 to 1.11.
        let ratio = affix_base_value(Affix::CritMultiplier, 500) / affix_base_value(Affix::DivineDamage, 500);
        assert!((ratio - 1.1111).abs() < 1e-3, "post-halving CritMultiplier:DivineDamage must be 1.11 at every tier, got {ratio}");
        let elsewhere = affix_base_value(Affix::IncreasedDamage, 37) / affix_base_value(Affix::Evasion, 37);
        let at_another_tier = affix_base_value(Affix::IncreasedDamage, 4_000) / affix_base_value(Affix::Evasion, 4_000);
        assert!((elsewhere - at_another_tier).abs() < 1e-12, "every non-crit affix pair must hold its ratio at EVERY tier - the curve is a pure tier term");
    }

    /// §5.3, the trap that would waste a day if missed: the curve must
    /// change only the VALUE computed from the draws, never how many
    /// draws happen, their order, or their ranges. If the draw count
    /// moves, every golden fixture diverges for a second, unrelated
    /// reason and the 17 diffs stop being readable.
    ///
    /// Asserted by counting draws off a counting Rng rather than by
    /// reading the source, so it keeps holding as the function changes.
    #[test]
    fn the_curve_does_not_change_the_rng_draw_count_in_roll_affixes() {
        /// Wraps a real seeded Rng and counts every call that reaches it.
        struct Counting<R: Rng> {
            inner: R,
            draws: std::cell::Cell<u32>,
        }
        impl<R: Rng> RngCore for Counting<R> {
            fn next_u32(&mut self) -> u32 {
                self.draws.set(self.draws.get() + 1);
                self.inner.next_u32()
            }
            fn next_u64(&mut self) -> u64 {
                self.draws.set(self.draws.get() + 1);
                self.inner.next_u64()
            }
            fn fill_bytes(&mut self, dest: &mut [u8]) {
                self.draws.set(self.draws.get() + 1);
                self.inner.fill_bytes(dest)
            }
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
                self.draws.set(self.draws.get() + 1);
                self.inner.try_fill_bytes(dest)
            }
        }

        // The count is a function of the seed (it decides how many affixes
        // roll), not of the tier - which is the whole point. Same seed at
        // wildly different tiers must draw identically.
        for seed in [1u64, 7, 99, 12345] {
            let mut counts = Vec::new();
            for tier in [1u32, 7, 100, 1_000, 50_000] {
                let mut rng = Counting { inner: StdRng::seed_from_u64(seed), draws: std::cell::Cell::new(0) };
                let rolled = roll_affixes(EquipSlot::Weapon, tier, &mut rng);
                counts.push((tier, rng.draws.get(), rolled.len()));
            }
            let (_, first_draws, first_len) = counts[0];
            for &(tier, draws, len) in &counts {
                assert_eq!(draws, first_draws, "seed {seed}: roll_affixes drew {draws} times at T={tier} but {first_draws} at T=1 - the curve must not touch the rng stream (spec 5.3)");
                assert_eq!(len, first_len, "seed {seed}: affix COUNT changed with tier at T={tier}");
            }
        }
    }

    /// §4.1's rescale, as arithmetic: the migration must preserve each
    /// affix's jitter EXACTLY, including a polished roll sitting above the
    /// 0.85..1.15 roll ceiling. This is the property the ratio was chosen
    /// for over the spec's stated `per_tier * f(T) * preserved_jitter`
    /// shape, which would clamp and silently strip the polish.
    #[test]
    fn the_rescale_preserves_jitter_exactly_including_polish_above_the_roll_ceiling() {
        let tier = 7u32;
        // Jitters spanning the rolled band plus 1.20, which only polish
        // can produce (`Character::polish` drives toward
        // POWER_ROLL_RANGE.end, above roll_affixes' own 1.15).
        for jitter in [0.85_f64, 0.94730, 1.0, 1.149206, 1.20] {
            let stored = affix_balance(Affix::ColdDamage).0 * tier as f64 * jitter;
            let mut item = test_item_with_affix(tier, Affix::ColdDamage, stored);
            migrate_affix_tier_curve(&mut item);
            let migrated = item.affixes[0].1;
            let recovered = migrated / affix_base_value(Affix::ColdDamage, tier);
            assert!(
                (recovered - jitter).abs() < 1e-5,
                "jitter {jitter} must survive the rescale to five decimals - recovered {recovered}. A clamped reconstruction would pin anything above 1.15 back to 1.15 and take the player's polish."
            );
        }
    }

    /// The other half of §4.4, asserted rather than argued: because the
    /// rescale scales the stored value and `affix_base_value` by the same
    /// factor, every displayed quality% is unchanged. No player sees their
    /// items' quality readings jump.
    #[test]
    fn the_rescale_leaves_displayed_quality_percent_untouched() {
        for tier in [1u32, 7, 20, 50, 100, 1_000] {
            for jitter in [0.85_f64, 1.0, 1.15] {
                let stored = affix_balance(Affix::Evasion).0 * tier as f64 * jitter;
                // Quality as it read BEFORE the curve: stored over the old
                // linear base.
                let before = {
                    let base = affix_balance(Affix::Evasion).0 * tier as f64;
                    let j = (stored / base).clamp(0.85, 1.15);
                    ((j - 0.85) / 0.30 * 100.0).clamp(0.0, 100.0)
                };
                let mut item = test_item_with_affix(tier, Affix::Evasion, stored);
                migrate_affix_tier_curve(&mut item);
                let after = affix_quality_percent(Affix::Evasion, item.affixes[0].1, tier, false);
                assert!((before - after).abs() < 1e-6, "T={tier} jitter={jitter}: quality% moved {before} -> {after}; the ratio rescale is supposed to make this a no-op");
            }
        }
    }

    /// The §7 halving reaches ALREADY-STORED crit-damage affixes, not just
    /// new rolls, and reaches nothing else.
    #[test]
    fn the_rescale_halves_stored_crit_multiplier_and_only_that() {
        let tier = 20u32;
        let mut item = test_item_with_affix(tier, Affix::CritMultiplier, 0.05 * tier as f64);
        item.affixes.push((Affix::CritChance, 0.01 * tier as f64));
        migrate_affix_tier_curve(&mut item);
        let expected_crit_mult = 0.05 * affix_tier_curve(tier) * 0.5;
        assert!((item.affixes[0].1 - expected_crit_mult).abs() < 1e-9, "CritMultiplier must take BOTH cuts: curve and halving");
        let expected_crit_chance = 0.01 * affix_tier_curve(tier);
        assert!((item.affixes[1].1 - expected_crit_chance).abs() < 1e-9, "CritChance takes the curve ONLY - the 2026-08-16 nerf already halved it once and must not be applied twice");
    }

    /// THE LOAD-BEARING TEST (§4.1 second half, owner: "test it directly").
    ///
    /// An item crafted up several tiers AFTER the migration must land ON
    /// the curve, not above it. With the old linear ratio at the three
    /// growth sites, a migrated item resumes growing linearly from where
    /// the rescale left it and is back above the curve within a few
    /// tiers - the change would read as a one-time cut that silently
    /// undoes itself.
    #[test]
    fn an_item_grown_after_the_rescale_lands_on_the_curve_not_above_it() {
        // A tier-7 item as it exists on live today: linear value, ordinary
        // jitter. Rescaled by the migration, then grown the way Krangle
        // and reforge grow items.
        let jitter = 1.0473_f64;
        let stored = affix_balance(Affix::IncreasedLife).0 * 7.0 * jitter;
        let mut item = test_item_with_affix(7, Affix::IncreasedLife, stored);
        migrate_affix_tier_curve(&mut item);

        for grown_to in [8u32, 15, 40, 120, 900] {
            let mut carried = item.clone();
            carried.sync_tier_to(grown_to);
            let on_curve = affix_base_value(Affix::IncreasedLife, grown_to) * jitter;
            let got = carried.affixes[0].1;
            assert!(
                (got - on_curve).abs() < 1e-6,
                "grown 7 -> {grown_to}: value {got} must equal the on-curve value {on_curve}. If this is high, a tier-growth site is still using a LINEAR new/old ratio and the curve leaks."
            );
            // And state the failure the old code would have produced, so a
            // regression reads as what it is rather than as a rounding
            // problem.
            let what_linear_would_give = stored * (affix_tier_curve(7) / 7.0) * (grown_to as f64 / 7.0);
            if grown_to > 7 {
                assert!(what_linear_would_give > got * 1.05, "sanity: at T={grown_to} the old linear ratio would have given {what_linear_would_give}, meaningfully above the on-curve {got} - this is the leak the test exists to catch");
            }
        }
    }

    /// Growth is path-independent: an item that reaches T=200 in one jump
    /// and one that gets there in five must hold the same value. This is
    /// what `f(new)/f(old)` buys and what a per-step approximation would
    /// quietly lose.
    #[test]
    fn tier_growth_is_path_independent() {
        let stored = affix_balance(Affix::Splash).0 * affix_tier_curve(3);
        let mut direct = test_item_with_affix(3, Affix::Splash, stored);
        direct.sync_tier_to(200);

        let mut stepped = test_item_with_affix(3, Affix::Splash, stored);
        for step in [11u32, 40, 99, 101, 200] {
            stepped.sync_tier_to(step);
        }
        assert!(
            (direct.affixes[0].1 - stepped.affixes[0].1).abs() < 1e-9,
            "one jump {} vs five steps {} - growth must be path-independent, including across the T=100 knee",
            direct.affixes[0].1,
            stepped.affixes[0].1
        );
    }

    /// The migration is NOT idempotent and must not be made to look like
    /// it is - it is marker-guarded, and this records the consequence of
    /// that guard failing so nobody removes it believing the pass is safe
    /// to repeat.
    #[test]
    fn the_rescale_is_deliberately_not_idempotent() {
        let mut item = test_item_with_affix(50, Affix::ColdDamage, affix_balance(Affix::ColdDamage).0 * 50.0);
        migrate_affix_tier_curve(&mut item);
        let once = item.affixes[0].1;
        migrate_affix_tier_curve(&mut item);
        assert!(item.affixes[0].1 < once, "running it twice applies the cut twice - that is expected, and the marker guard in run_item_migrations is what prevents it");
    }

    /// A bare item carrying exactly one affix, for the arithmetic tests
    /// above. Built through `generate_item_at_tier_with_roll` so it is a
    /// real `Item` rather than a hand-assembled one.
    fn test_item_with_affix(tier: u32, affix: Affix, value: f64) -> Item {
        let mut rng = StdRng::seed_from_u64(4242);
        let mut item = generate_item_at_tier_with_roll(EquipSlot::Body, tier, 1.0, &mut rng);
        item.affixes = vec![(affix, value)];
        item.sacred_affix = None;
        item.perfect = false;
        item
    }
}

/// The 2026-09-04 retirement refund. What these pin is the property the
/// owner called non-negotiable - **both** allocation maps - and the
/// property that makes the migration safe to run at all: it returns
/// points and touches nothing else.
#[cfg(test)]
mod refund_retired_dead_nodes_tests {
    use super::*;

    fn with(archetype: Archetype, primary: &[(&str, u32)], secondary: &[(&str, u32)]) -> Character {
        let mut c = Character::new("test".to_string());
        c.archetype = archetype;
        c.passive_allocations = primary.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        c.secondary_passive_allocations = secondary.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        c
    }

    /// `spent` is derived by summing the map at every call site, so
    /// removing the entry IS the refund. Asserted as the sum rather than
    /// as a missing key, because the sum is what the game actually reads.
    fn spent(c: &Character) -> u32 {
        c.passive_allocations.values().sum::<u32>() + c.secondary_passive_allocations.values().sum::<u32>()
    }

    #[test]
    fn refunds_both_retired_nodes_from_the_primary_tree() {
        let mut c = with(Archetype::Monk, &[("stillwater", 3), ("unmovable", 2)], &[]);
        assert_eq!(spent(&c), 5);
        migrate_refund_retired_dead_nodes(&mut c);
        assert!(!c.passive_allocations.contains_key("stillwater"), "the retired key must be gone, not zeroed");
        assert_eq!(c.passive_allocations.get("unmovable").copied(), Some(2), "a sibling node must be untouched");
        assert_eq!(spent(&c), 2, "the 3 points must come back");
    }

    /// **The non-negotiable one.** Split Personality can run Monk or
    /// Paladin as a SECONDARY, so an affected allocation can live only in
    /// `secondary_passive_allocations`. A migration touching just the
    /// primary map would miss exactly those characters - and, being
    /// marker-guarded, would never get a second chance at them.
    #[test]
    fn refunds_from_the_secondary_tree_too() {
        let mut c = with(Archetype::Berserker, &[("bloodlust", 3)], &[("stillwater", 2), ("sacredoverflow", 3)]);
        assert_eq!(spent(&c), 8);
        migrate_refund_retired_dead_nodes(&mut c);
        assert!(!c.secondary_passive_allocations.contains_key("stillwater"));
        assert!(!c.secondary_passive_allocations.contains_key("sacredoverflow"));
        assert_eq!(c.passive_allocations.get("bloodlust").copied(), Some(3), "the primary tree must be untouched");
        assert_eq!(spent(&c), 3, "all 5 secondary-tree points must come back");
    }

    #[test]
    fn refunds_the_paladin_node_and_leaves_its_siblings() {
        let mut c = with(Archetype::Paladin, &[("sacredoverflow", 3), ("radiantbarrier", 3), ("graceperiod", 1)], &[]);
        migrate_refund_retired_dead_nodes(&mut c);
        assert!(!c.passive_allocations.contains_key("sacredoverflow"));
        assert_eq!(c.passive_allocations.get("radiantbarrier").copied(), Some(3));
        assert_eq!(c.passive_allocations.get("graceperiod").copied(), Some(1));
        assert_eq!(spent(&c), 4);
    }

    /// Marker-guarded means it runs once, but a migration that is not
    /// idempotent is a landmine if the marker is ever lost - and this one
    /// costs nothing to make safe.
    #[test]
    fn is_a_no_op_when_neither_node_is_allocated_and_is_safe_to_re_run() {
        let mut c = with(Archetype::Monk, &[("unshakable", 3)], &[("serenity", 4)]);
        let before = spent(&c);
        migrate_refund_retired_dead_nodes(&mut c);
        migrate_refund_retired_dead_nodes(&mut c);
        assert_eq!(spent(&c), before, "a character with neither node must be left exactly as-is");
        assert_eq!(c.passive_allocations.get("unshakable").copied(), Some(3));
        assert_eq!(c.secondary_passive_allocations.get("serenity").copied(), Some(4));
    }

    /// **The claim the doc comment opens with, pinned rather than
    /// asserted in prose: the refund cannot change combat output.**
    ///
    /// Note carefully what makes that true, because the obvious test is
    /// the wrong one. `stillwater` DECLARES a magnitude
    /// (`Special { at_rank_1: 1.0, .. }`, so `magnitude_at_rank(3)` is
    /// 3.0) - it is not zero. What makes it inert is that **no call site
    /// ever passes its key**, which is a property of the CONSUMERS, not
    /// of the node. So this scans the four files where every by-key
    /// passive lookup in the game lives, the same `include_str!`
    /// technique `character.rs`'s `guard_tests` uses to pin the reach of
    /// the mutation-guard bypasses.
    ///
    /// It fails the moment anyone adds a consumer for a retired key -
    /// which is precisely when the refund would stop being
    /// balance-neutral.
    ///
    /// `passive_overrides.rs` is deliberately NOT scanned: it lists these
    /// keys in an override allow-list, which is not a combat consumer and
    /// cannot make a node do anything on its own.
    #[test]
    fn no_consumer_anywhere_reads_a_retired_node_key() {
        const CONSUMER_SOURCES: &[(&str, &str)] = &[
            ("combat.rs", include_str!("combat.rs")),
            ("character.rs", include_str!("character.rs")),
            ("manager.rs", include_str!("manager.rs")),
            ("adventure_web.rs", include_str!("../adventure_web.rs")),
        ];
        for key in RETIRED_DEAD_NODE_KEYS {
            let needle = format!("\"{key}\"");
            for (file, source) in CONSUMER_SOURCES {
                assert!(
                    !source.contains(&needle),
                    "{file} names the retired node key {needle}. A retired node must have no consumer - if one exists the node is not inert, and refunding its points silently removes something the player had. Either that consumer is new (do not add one), or a replacement node reused the key (replacements take NEW keys)."
                );
            }
        }
    }

    /// A retired key must live in **at most one** tree slot, and only as
    /// the dead node awaiting replacement.
    ///
    /// The retired definitions still exist at this commit - this branch
    /// ships the refund alone, and the replacements land later under NEW
    /// keys. What must never happen is a retired key appearing on a
    /// SECOND node, or on an archetype it never belonged to: either would
    /// mean an unrefunded allocation resolving somewhere the player never
    /// chose. Combined with `no_consumer_anywhere_reads_a_retired_node_key`
    /// above, this is the pair that makes reuse of these keys fail loudly.
    #[test]
    fn each_retired_key_appears_on_at_most_one_node_and_only_where_it_started() {
        for key in RETIRED_DEAD_NODE_KEYS {
            let home = if key == "stillwater" { Archetype::Monk } else { Archetype::Paladin };
            for archetype in ALL_ARCHETYPES {
                let hits = archetype.passive_nodes().iter().filter(|n| n.key == key).count();
                let allowed = if archetype == home { 1 } else { 0 };
                assert!(
                    hits <= allowed,
                    "node key {key:?} appears {hits}x on {archetype:?} (at most {allowed} allowed). A replacement node must take a NEW key, so an unrefunded allocation cannot resolve onto a mechanic the player never chose."
                );
            }
        }
    }
}
