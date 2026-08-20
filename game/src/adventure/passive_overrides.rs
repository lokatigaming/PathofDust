//! Live-tunable passive-tree VALUES (2026-08-19) - a sparse override
//! store letting the owner retune any passive node's numbers from
//! `/admin/passives` with no rebuild and no restart, the same way
//! `LiveTunables` already works for drop rates and boss difficulty. See
//! `docs/passive_tunables_spec.md`.
//!
//! **Values only.** Node STRUCTURE - keys, max ranks, parents, unlock
//! gating, which nodes exist at all - stays code-defined in
//! `passive_tree.rs` and is deliberately not reachable from here. An
//! override can change what a node is worth; it can never change the
//! shape of the tree.
//!
//! **Sparse by construction.** An absent node key, or an absent rank
//! within a present key, falls straight through to the node's own
//! compiled-in value. A missing or unparseable file therefore means
//! "everything is at its code default" - byte-identical behavior to
//! before this module existed, which is what makes the whole feature
//! safe to ship dark.
//!
//! **Why per-rank values rather than overriding the two formula
//! coefficients.** `PassiveEffect`'s own magnitude formula is strictly
//! linear (`at_rank_1 + per_additional_rank * (rank - 1)`), but 27
//! nodes across the tree are implemented in `combat.rs` as non-linear
//! per-rank tables (typically "does nothing at rank 1, then two
//! different values at ranks 2 and 3"). Overriding the coefficients
//! could never express those. A per-rank array expresses both shapes,
//! so no node is permanently locked out of tuning.
//!
//! **Three values per node, always.** Every node in the tree has at
//! most three DISTINCT magnitudes: Skills and Modifiers cap at rank 3,
//! and a Specialization's 4th point is unlock-only (see
//! `PassiveNode::magnitude_at_rank`, which floors its effective rank at
//! 3). So an override array is indexed by effective rank - index 0 is
//! rank 1 - and never needs a 4th entry.

use super::data_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Same naming/location convention as `TUNABLES_PATH` and
/// `ITEM_BALANCE_PATH` - resolved through `data_path`, so a disposable
/// test instance pointed at a scratch directory never reads or writes
/// the live file.
pub const PASSIVE_OVERRIDES_PATH: &str = "adventure-passive-overrides.toml";

/// Node key -> per-rank value overrides. Serialized as a single
/// `[nodes]` TOML table of arrays:
///
/// ```toml
/// [nodes]
/// bulwark = [0.08, 0.14, 0.20]
/// juggernaut = [0.10]           # only rank 1 overridden; 2 and 3 keep their defaults
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PassiveOverrides {
    #[serde(default)]
    pub nodes: HashMap<String, Vec<f64>>,
}

impl PassiveOverrides {
    /// The overridden magnitude for `key` at `effective_rank`, or `None`
    /// to mean "use the node's own compiled-in value".
    ///
    /// `effective_rank` is 1-indexed and is the rank AFTER
    /// `magnitude_at_rank`'s Specialization flooring - callers must not
    /// pass a raw rank 4. Rank 0 is never overridable: an unallocated
    /// node is worth nothing by definition, and letting an override
    /// change that would let the store grant a passive the player never
    /// invested in.
    ///
    /// Pure and side-effect-free on purpose: the whole lookup is
    /// testable against a hand-built `PassiveOverrides` without touching
    /// the process-global store (see `passive_override_for`).
    pub fn value_for(&self, key: &str, effective_rank: u32) -> Option<f64> {
        if effective_rank == 0 {
            return None;
        }
        let per_rank = self.nodes.get(key)?;
        per_rank.get(effective_rank as usize - 1).copied()
    }

    /// Whether `key` has any override at all - what the admin page's
    /// "differs from default" marker keys off.
    pub fn has_override(&self, key: &str) -> bool {
        self.nodes.get(key).is_some_and(|v| !v.is_empty())
    }

    /// Drops every override for `key`, returning it to its compiled-in
    /// values - the admin page's per-node Revert.
    pub fn revert(&mut self, key: &str) {
        self.nodes.remove(key);
    }
}

/// The process-wide live store.
///
/// Global rather than a field on `AdventureManager` because the read
/// path genuinely has no manager to reach: `PassiveNode::magnitude_at_rank`
/// is an inherent method on a `'static` node definition, and
/// `Character::passive_node_magnitude` is a plain synchronous method
/// called from the web layer's stat display as well as from combat.
/// Threading a store parameter through those would mean changing every
/// caller of every `combat_*` getter.
///
/// `std::sync::RwLock` (not `tokio`'s) deliberately, matching
/// `AdventureManager::live_tunables`' own reasoning: every use is a
/// quick read-and-copy, never held across an `.await`. `LazyLock` gives
/// first-use initialization without the load-once limitation an
/// `OnceLock` would impose - unlike the `ITEM_BALANCE_PATH` caches,
/// this one has to be replaceable at runtime, since applying without a
/// restart is the entire point.
static PASSIVE_OVERRIDES: LazyLock<RwLock<PassiveOverrides>> = LazyLock::new(|| RwLock::new(load_passive_overrides_file()));

/// Fail-soft load, same spirit as `load_live_tunables` and
/// `load_item_balance_file` - missing or unparseable means "no
/// overrides", logged, never a boot failure. A malformed file must
/// never take the game down or, worse, silently apply half a table.
fn load_passive_overrides_file() -> PassiveOverrides {
    match std::fs::read_to_string(data_path(PASSIVE_OVERRIDES_PATH)) {
        Ok(contents) => match toml::from_str::<PassiveOverrides>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::warn!("{PASSIVE_OVERRIDES_PATH} failed to parse, using built-in passive values: {err}");
                PassiveOverrides::default()
            }
        },
        Err(_) => PassiveOverrides::default(),
    }
}

/// **The hook.** Called by `PassiveNode::magnitude_at_rank` for every
/// numeric read of every node in the tree, so this is the one place an
/// override can enter the game - stat pooling, every `Special`
/// mechanic in `combat.rs`, and the dashboard's own stat display all
/// inherit it without knowing it exists.
///
/// Deliberately does NOT special-case `NotYetImplemented` nodes: an
/// override on one is harmless (nothing consumes their magnitude), and
/// a carve-out here would be a second rule to keep in sync. The admin
/// page simply doesn't offer them.
pub fn passive_override_for(key: &str, effective_rank: u32) -> Option<f64> {
    PASSIVE_OVERRIDES.read().expect("passive overrides lock poisoned").value_for(key, effective_rank)
}

/// A snapshot of the current overrides, for the admin page to render.
pub fn passive_overrides() -> PassiveOverrides {
    PASSIVE_OVERRIDES.read().expect("passive overrides lock poisoned").clone()
}

/// Admin-page save: persists first, then swaps the live copy in, so a
/// write failure leaves the running game on the values that are still
/// on disk rather than diverging from them. Same order and reasoning as
/// `AdventureManager::save_live_tunables`.
pub fn save_passive_overrides(overrides: PassiveOverrides) -> std::io::Result<()> {
    let contents = toml::to_string_pretty(&overrides).map_err(std::io::Error::other)?;
    std::fs::write(data_path(PASSIVE_OVERRIDES_PATH), contents)?;
    *PASSIVE_OVERRIDES.write().expect("passive overrides lock poisoned") = overrides;
    Ok(())
}

/// Nodes whose numbers are NOT yet reachable by an override, because
/// `combat.rs` reads only their rank and hardcodes their values rather
/// than reading `passive_node_magnitude`.
///
/// Setting an override on one of these would silently do nothing, so
/// `/admin/passives` renders them as pending instead of offering an
/// input. **This list shrinks to empty as the per-class migration
/// batches land** - each batch deletes its own entries here, and the
/// admin page needs no other change to start offering them.
///
/// Derived from a full audit of every `passive_node_rank` /
/// `passive_node_magnitude` call site across `combat.rs`,
/// `character.rs`, `manager.rs` and the web layer (2026-08-19). See
/// `docs/passive_tunables_spec.md` for the audit table and the
/// per-class batch order.
pub const PENDING_MIGRATION_NODES: &[&str] = &[
    // Berserker (10)
    "bloodrush",
    "bloodscent",
    "crush",
    "deathwish",
    "frenzy",
    "gloriousdeath",
    "lastlaugh",
    "neverending",
    "reckless",
    "shatter",
    // Cleric (4)
    "chainoflight",
    "compassion",
    "guardianspirit",
    "sanctifiedtouch",
    // Druid (1)
    "unitedpack",
    // Mage (7)
    "absolutezero",
    "arcaneinstability",
    "empoweredbolt",
    "eternalmoment",
    "infiniteloop",
    "perpetualmotion",
    "stormcaller",
    // Monk (7)
    "clarity",
    "eternalflow",
    "onehundredhands",
    "relentlessassault",
    "sharedstrength",
    "stillwater",
    "widecircle",
    // Paladin (2)
    "finaljudgment",
    "risingblaze",
    // Ranger (6)
    "finalblow",
    "piercingshots",
    "stormofarrows",
    "widerburst",
    "widerpack",
    "windborn",
    // Rogue (8)
    "assassinate",
    "doubletap",
    "markedfordeath",
    "opportunist",
    "quickdraw",
    "surgicalstrike",
    "vitalstrike",
    "windrunner",
    // Slayer (6)
    "arterialspray",
    "blooddebt",
    "bloodsac",
    "lastrites",
    "plaguebearer",
    "undying",
    // Warlock (4)
    "chaostheory",
    "covenant",
    "plagueoflocusts",
    "ravage",
    // Warrior (5)
    "payback",
    "secondwind",
    "stonewall",
    "undyingwill",
    "unstoppable",
];

/// Nodes whose magnitude is a COUNT (extra targets, extra hits, banked
/// charges, revives) rather than a rate or a percentage. The admin page
/// restricts these to whole-number input, per an explicit owner ruling:
/// it simply won't accept 2.5 extra targets, rather than accepting it
/// and rounding behind the player's back.
///
/// **Deliberately empty until the migration batches populate it.**
///
/// The tempting shortcut is to seed this from the 36 nodes whose
/// declaration is `1.0 / 1.0` (magnitude == rank exactly), since that
/// is why `combat.rs` reads their rank directly. That inference is
/// WRONG for 12 of them: `lastlaugh`, `compassion`, `quickdraw`,
/// `markedfordeath`, `absolutezero`, `arcaneinstability`, `clarity`,
/// `covenant`, `empoweredbolt`, `finalblow`, `ravage` and
/// `surgicalstrike` all declare `1.0 / 1.0` but are implemented as
/// BOOLEAN THRESHOLDS (`rank >= 2` flips a flag, `rank >= 3` flips
/// another), not as counts of anything. Seeding from the declaration
/// would mark a third of the list wrong.
///
/// Every entry here is therefore added by the batch that migrates its
/// node, at the point where the node's actual `combat.rs` code has been
/// read and its shape is known rather than guessed. Nothing is lost by
/// starting empty: every candidate is currently in
/// `PENDING_MIGRATION_NODES`, so none of them is tunable yet anyway.
pub const INTEGER_COUNT_NODES: &[&str] = &[];

/// Whether an override on `key` would actually reach the game today.
/// `false` means the node still hardcodes its numbers in `combat.rs`
/// and is waiting on its migration batch.
pub fn node_is_tunable(key: &str) -> bool {
    !PENDING_MIGRATION_NODES.contains(&key)
}

/// Whether `key`'s magnitude is a count, and so must be tuned in whole
/// numbers - see `INTEGER_COUNT_NODES`.
pub fn node_is_integer_count(key: &str) -> bool {
    INTEGER_COUNT_NODES.contains(&key)
}

/// Stage 1 of the live-tunable passive values build
/// (docs/passive_tunables_spec.md) - the override store and the hook.
///
/// Every behavioral test here goes through
/// `PassiveNode::magnitude_at_rank_with` and hand-built
/// `PassiveOverrides` rather than the process-global store. That is
/// deliberate: this crate runs its whole suite in ONE process across
/// many threads, so a test that WROTE the global would change what
/// every other test in the binary sees. The one test that reads the
/// global only asserts it is empty - which is the property that
/// actually matters for shipping.
#[cfg(test)]
mod passive_override_tests {
    use super::*;
    use crate::adventure::Archetype;
    use crate::passive_tree::PassiveNode;

    fn node(archetype: Archetype, key: &str) -> &'static PassiveNode {
        archetype.passive_nodes().iter().find(|n| n.key == key).unwrap_or_else(|| panic!("no node {key:?}"))
    }

    fn overrides(entries: &[(&str, &[f64])]) -> PassiveOverrides {
        PassiveOverrides { nodes: entries.iter().map(|(k, v)| (k.to_string(), v.to_vec())).collect() }
    }

    // ---- the lookup itself -----------------------------------------

    #[test]
    fn an_absent_node_falls_through_to_its_compiled_in_value() {
        assert_eq!(PassiveOverrides::default().value_for("bulwark", 1), None);
    }

    #[test]
    fn a_present_node_returns_its_per_rank_value() {
        let o = overrides(&[("bulwark", &[0.11, 0.22, 0.33])]);
        assert_eq!(o.value_for("bulwark", 1), Some(0.11));
        assert_eq!(o.value_for("bulwark", 2), Some(0.22));
        assert_eq!(o.value_for("bulwark", 3), Some(0.33));
    }

    #[test]
    fn rank_zero_is_never_overridable() {
        // An unallocated node is worth nothing by definition. Letting an
        // override change that would hand a player a passive they never
        // invested in.
        let o = overrides(&[("bulwark", &[0.11, 0.22, 0.33])]);
        assert_eq!(o.value_for("bulwark", 0), None);
        assert_eq!(node(Archetype::Warrior, "bulwark").magnitude_at_rank_with(0, &o), 0.0);
    }

    #[test]
    fn a_short_array_overrides_only_the_ranks_it_covers() {
        // Sparse in both directions - by node AND by rank within a node.
        let o = overrides(&[("bulwark", &[0.11])]);
        let bulwark = node(Archetype::Warrior, "bulwark");
        assert_eq!(bulwark.magnitude_at_rank_with(1, &o), 0.11, "rank 1 is overridden");
        assert_eq!(
            bulwark.magnitude_at_rank_with(2, &o),
            bulwark.magnitude_at_rank_with(2, &PassiveOverrides::default()),
            "rank 2 must keep its compiled-in value"
        );
    }

    #[test]
    fn an_empty_array_is_not_treated_as_an_override() {
        let o = overrides(&[("bulwark", &[])]);
        assert_eq!(o.value_for("bulwark", 1), None);
        assert!(!o.has_override("bulwark"));
    }

    // ---- the hook ---------------------------------------------------

    #[test]
    fn the_hook_replaces_the_compiled_in_value() {
        let bulwark = node(Archetype::Warrior, "bulwark");
        let default_r2 = bulwark.magnitude_at_rank_with(2, &PassiveOverrides::default());
        let o = overrides(&[("bulwark", &[0.5, 0.6, 0.7])]);
        assert_eq!(bulwark.magnitude_at_rank_with(2, &o), 0.6);
        assert_ne!(default_r2, 0.6, "sanity: the override must actually differ from the default");
    }

    #[test]
    fn an_override_on_one_node_never_leaks_to_another() {
        let o = overrides(&[("bulwark", &[0.9, 0.9, 0.9])]);
        let juggernaut = node(Archetype::Warrior, "juggernaut");
        assert_eq!(juggernaut.magnitude_at_rank_with(2, &o), juggernaut.magnitude_at_rank_with(2, &PassiveOverrides::default()));
    }

    #[test]
    fn a_specialization_is_keyed_by_effective_rank_so_rank_4_reads_the_rank_3_value() {
        // A Specialization's 4th point is unlock-only - `effective_rank`
        // floors it at 3. An override array therefore never needs a 4th
        // entry, and rank 4 must read exactly what rank 3 reads.
        let unbreakable = node(Archetype::Warrior, "unbreakable");
        assert_eq!(unbreakable.effective_rank(4), 3);
        let o = overrides(&[("unbreakable", &[0.1, 0.2, 0.3])]);
        assert_eq!(unbreakable.magnitude_at_rank_with(3, &o), 0.3);
        assert_eq!(unbreakable.magnitude_at_rank_with(4, &o), 0.3, "the 4th point buys child unlock, not more magnitude");
    }

    #[test]
    fn every_tier_of_node_can_be_overridden() {
        for key in ["bulwark", "unbreakable", "fortress"] {
            let n = node(Archetype::Warrior, key);
            let o = overrides(&[(key, &[7.0, 8.0, 9.0])]);
            assert_eq!(n.magnitude_at_rank_with(3, &o), 9.0, "{key} must be overridable");
        }
    }

    // ---- persistence ------------------------------------------------

    #[test]
    fn overrides_round_trip_through_toml() {
        let o = overrides(&[("bulwark", &[0.08, 0.14, 0.20]), ("juggernaut", &[0.1])]);
        let text = toml::to_string_pretty(&o).expect("must serialize");
        let back: PassiveOverrides = toml::from_str(&text).expect("must deserialize");
        assert_eq!(back, o);
    }

    #[test]
    fn an_empty_store_serializes_and_reloads_as_empty() {
        let text = toml::to_string_pretty(&PassiveOverrides::default()).expect("must serialize");
        let back: PassiveOverrides = toml::from_str(&text).expect("must deserialize");
        assert!(back.nodes.is_empty());
    }

    #[test]
    fn a_file_with_an_unknown_node_key_still_parses() {
        // Fail-soft: a key left behind by a renamed node must not stop
        // the rest of the file from loading. It simply never matches.
        let text = "[nodes]\na_node_that_was_deleted = [1.0]\nbulwark = [0.5]\n";
        let parsed: PassiveOverrides = toml::from_str(text).expect("an unknown key must not fail the parse");
        assert_eq!(parsed.value_for("bulwark", 1), Some(0.5));
        assert_eq!(parsed.value_for("a_node_that_was_deleted", 1), Some(1.0), "stored, but no node will ever ask for it");
    }

    #[test]
    fn revert_returns_a_node_to_its_compiled_in_values() {
        let mut o = overrides(&[("bulwark", &[0.9, 0.9, 0.9])]);
        assert!(o.has_override("bulwark"));
        o.revert("bulwark");
        assert!(!o.has_override("bulwark"));
        let bulwark = node(Archetype::Warrior, "bulwark");
        assert_eq!(bulwark.magnitude_at_rank_with(2, &o), bulwark.magnitude_at_rank_with(2, &PassiveOverrides::default()));
    }

    // ---- shipping safety --------------------------------------------

    #[test]
    fn with_no_overrides_file_every_node_in_every_tree_is_byte_identical_to_before() {
        // The property that makes Stage 1 safe to ship dark. Reads the
        // real global on purpose - it is the thing being asserted - but
        // only reads it.
        assert!(passive_overrides().nodes.is_empty(), "no test may leave overrides in the global store");
        for &archetype in crate::adventure::ALL_ARCHETYPES.iter() {
            for n in archetype.passive_nodes() {
                for rank in 0..=n.max_rank {
                    assert_eq!(
                        n.magnitude_at_rank(rank),
                        n.magnitude_at_rank_with(rank, &PassiveOverrides::default()),
                        "{archetype:?} {} rank {rank} diverged with no overrides loaded",
                        n.key
                    );
                }
            }
        }
    }

    #[test]
    fn every_pending_migration_key_still_exists_in_the_tree() {
        // Guards the hand-maintained list against a node rename - a
        // stale entry would silently mark a tunable node as pending, or
        // leave a genuinely untunable one being offered.
        for key in PENDING_MIGRATION_NODES {
            let found = crate::adventure::ALL_ARCHETYPES.iter().any(|a| a.passive_nodes().iter().any(|n| n.key == *key));
            assert!(found, "{key:?} is listed as pending migration but no longer exists in any archetype's tree");
        }
    }

    #[test]
    fn every_integer_count_key_still_exists_in_the_tree() {
        for key in INTEGER_COUNT_NODES {
            let found = crate::adventure::ALL_ARCHETYPES.iter().any(|a| a.passive_nodes().iter().any(|n| n.key == *key));
            assert!(found, "{key:?} is listed as an integer count but no longer exists in any archetype's tree");
        }
    }

    #[test]
    fn the_pending_list_matches_the_audit_and_has_no_duplicates() {
        // The audit found exactly 60 nodes whose values live in
        // combat.rs rather than in their own declaration. If this
        // changes without a migration batch landing, the list has
        // drifted and the admin page is now lying about what is tunable.
        assert_eq!(PENDING_MIGRATION_NODES.len(), 60);
        let unique: std::collections::HashSet<&str> = PENDING_MIGRATION_NODES.iter().copied().collect();
        assert_eq!(unique.len(), 60, "the pending list must not contain duplicates");
    }

    #[test]
    fn tunability_is_reported_correctly_for_both_kinds_of_node() {
        assert!(!node_is_tunable("frenzy"), "frenzy hardcodes its numbers in combat.rs");
        assert!(node_is_tunable("bulwark"), "bulwark reads its value through the accessor");
    }

    // ---- #39: the 2026-08-16 Druid rework's stale reads --------------

    /// The bug this guards, stated concretely: the 2026-08-16 rework
    /// repurposed several Druid nodes, and `combat.rs` kept reading
    /// their magnitudes for the mechanics they USED to have. The worst
    /// case summed Werebear's Thick Hide cycle (4000 - a duration in
    /// MILLISECONDS) with Wild Roar's charge count (3) and fed 4003.0
    /// into the damage-reduction pipeline as a fraction, pinning
    /// whichever party member was currently lowest-HP at the 95%
    /// mitigation cap.
    ///
    /// The shared shape is a magnitude whose UNIT changed - milliseconds
    /// or a count now being read as a 0..1 rate - so this asserts on the
    /// scale of the values themselves. A future rework that repurposes
    /// another of these nodes trips it without anyone needing to
    /// remember why.
    #[test]
    fn the_reworked_druid_nodes_still_have_the_units_their_readers_expect() {
        let empty = PassiveOverrides::default();
        // Werebear and Unyielding Roots are millisecond CYCLES.
        for key in ["symbiosis", "unyieldingroots"] {
            let mag = node(Archetype::Druid, key).magnitude_at_rank_with(3, &empty);
            assert!(mag >= 1000.0, "{key} is a millisecond cycle; a magnitude of {mag} means it was repurposed again - re-audit its read sites (#39)");
        }
        // Wild Roar, Nature's Embrace, Verdant Burst and Rooted Network
        // are COUNTS. None of these is a rate, so none may be summed
        // into a `*_pct` field.
        for key in ["livingbond", "naturesembrace", "verdantburst", "rootednetwork"] {
            let mag = node(Archetype::Druid, key).magnitude_at_rank_with(3, &empty);
            assert_eq!(mag, mag.round(), "{key} is a count; a fractional magnitude of {mag} means it was repurposed again (#39)");
            assert!(mag >= 1.0, "{key} is a count and should be at least 1 at rank 3, got {mag}");
        }
    }

    /// The dead reads, asserted by absence. Each of these named a
    /// mechanic the 2026-08-16 rework deleted, and each kept running for
    /// four days afterwards.
    #[test]
    fn the_stale_druid_rework_reads_stay_removed() {
        let combat = include_str!("combat.rs");
        assert!(!combat.contains("own_symbiosis_dr_pct"), "the stale Symbiosis damage-reduction field is back (#39)");
        assert!(!combat.contains("symbiosis_dr_bonus"), "the stale Symbiosis damage-reduction plumbing is back (#39)");
        assert!(
            !combat.contains("\"Symbiosis\""),
            "the stale \"Symbiosis\" reduction-source label is back - it named a deleted mechanic, and is why the live source could not be found from the fight log (#39)"
        );
        // The repurposed nodes must not be summed into a rate field again.
        for stale in ["passive_node_magnitude(\"livingbond\")", "passive_node_magnitude(\"naturesembrace\")", "passive_node_magnitude(\"verdantburst\")"] {
            assert!(!combat.contains(stale), "{stale} is read as a magnitude again - its unit is a count, not a rate (#39)");
        }
        // Rooted Network now extends Thick Hide's CLEANSE count, not the
        // lowest-HP-ally protect count it fed before the rework.
        assert!(
            !combat.contains("passive_node_rank(\"rootednetwork\")"),
            "rootednetwork is feeding the protect-count again - the rework moved it to Thick Hide's cleanse count (#39)"
        );
    }
}
