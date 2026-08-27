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
    /// Node key -> explicit per-node conversion-output cap (per invested
    /// rank), for that node's OWN contribution - consumed only by
    /// `Character::accumulate_overflow_conversion_bonus`, so it is
    /// meaningful on the `OverflowConversion` nodes (14 at this writing;
    /// the admin page offers the input on exactly those rows). Absent = follow the
    /// global `LiveTunables::overflow_conversion_cap_per_rank`, so a
    /// store without entries computes byte-identically to before.
    ///
    /// Deliberately NOT folded into `nodes`: that vec is indexed by
    /// effective rank and feeds every `magnitude_at_rank` read, while a
    /// cap is one scalar per node feeding a different call site - mixing
    /// them would make every existing override array mean two things at
    /// once. Serialized as its own optional `[conversion_caps]` TOML
    /// table; files written before this field existed parse unchanged
    /// (missing table = empty map).
    #[serde(default)]
    pub conversion_caps: HashMap<String, f64>,
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

    /// The overridden per-rank conversion-output cap for `key`, or `None`
    /// to mean "follow the global
    /// `LiveTunables::overflow_conversion_cap_per_rank`".
    pub fn conversion_cap_for(&self, key: &str) -> Option<f64> {
        self.conversion_caps.get(key).copied()
    }

    /// Whether `key` has any override at all - what the admin page's
    /// "differs from default" marker keys off. A conversion-cap-only
    /// override counts: the node IS retuned even if its magnitudes are
    /// not, so it must earn the badge and the Revert button too.
    pub fn has_override(&self, key: &str) -> bool {
        self.nodes.get(key).is_some_and(|v| !v.is_empty()) || self.conversion_caps.contains_key(key)
    }

    /// Drops every override for `key`, returning it to its compiled-in
    /// values - the admin page's per-node Revert. Both maps, since a
    /// node can be overridden on either axis.
    pub fn revert(&mut self, key: &str) {
        self.nodes.remove(key);
        self.conversion_caps.remove(key);
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

/// The live per-node conversion-cap override for `key`, if one is set -
/// same store, same HOT swap-on-save path as `passive_override_for`.
/// Read by `accumulate_overflow_conversion_bonus` on every fight; a
/// miss means "use LiveTunables::overflow_conversion_cap_per_rank".
pub fn passive_conversion_cap_override(key: &str) -> Option<f64> {
    PASSIVE_OVERRIDES.read().expect("passive overrides lock poisoned").conversion_cap_for(key)
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
/// **Stage 0 (2026-08-24):** this list grew 28 → 47 when the tunable
/// audit's CI consumption guard landed
/// (`every_special_node_whose_value_has_no_magnitude_read_is_tracked_as_not_yet_tunable`
/// below). The entries tagged `stage-0` are nodes whose ONLY consumers
/// read `passive_node_rank` (structure), so an override never reached
/// the game - found by docs/tunable_audit.md §3 and now enforced by the
/// guard rather than by re-auditing by hand.
///
/// **Drift batch (2026-08-25):** shrank 47 → 31 when the audit's Group-B
/// drift nodes migrated (16 entries deleted here; `sacrifice` was tracked
/// in PARTIALLY_TUNABLE_NODES instead, its cost half already being wired).
///
/// **Stage 3 (2026-08-27):** shrank 31 → 6. Twenty-five nodes migrated
/// onto declared values. The six left are NOT waiting on a batch, and
/// each says which of the two reasons keeps it here:
/// - **Structure-only** (`clarity`, `lastlaugh`, `neverending`,
///   `sanctifiedtouch`): every rank read on these is an UNLOCK GATE - a
///   `rank >= 2/3` that flips a flag or releases a flat base and a
///   sibling node's own (already tunable) magnitude. They own no rank-fed
///   number, so there is nothing an override could carry. Same ruling the
///   2026-08-25 drift batch made for `ravage`/`endlessthirst`/
///   `naturesblessing`, and unlock gating is deliberately unreachable
///   from this store (see the module doc). Listed here rather than in
///   `UNWIRED_NODES` because their mechanics DO run - only their declared
///   magnitude is decorative.
/// - **Needs a second value slot** (`reckless`, `deathwish`): each owns
///   TWO independent rank-fed ladders (damage DEALT and damage TAKEN,
///   which scale by different amounts per rank), and a node has exactly
///   one magnitude table. Same blocker as `sacrifice`/`bloomingfield`/
///   `reaperscall` in `PARTIALLY_TUNABLE_NODES` - see this module's
///   Stage 3 note in `docs/passive_tunables_spec.md`.
pub const PENDING_MIGRATION_NODES: &[&str] = &[
    "clarity", // monk - structure-only: `rank >= 2` flips "Serenity also triggers on block", no number of its own
    "deathwish", // berserker - needs a 2nd value slot: dealt (10/20/30%) AND taken (5/10/15%) ladders
    "lastlaugh", // berserker - structure-only: `rank >= 2` / `rank >= 3` flip two flags, no number of its own
    "neverending", // berserker - structure-only: invested-or-not gate, no number of its own
    "reckless", // berserker - needs a 2nd value slot: dealt (15/25/35%) AND taken (8/13/18%) ladders
    "sanctifiedtouch", // cleric - structure-only: rank 2/3 release a flat base plus holycrit/divineclarity's own tunable magnitudes
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
/// read and its shape is known rather than guessed - which is exactly
/// how the entries below arrived (Stage 2, 2026-08-20): each one was
/// confirmed to be a plain arithmetic count at its own call site before
/// being listed.
pub const INTEGER_COUNT_NODES: &[&str] = &[
    "infiniteloop",
    "chainoflight",
    "arterialspray",
    "blooddebt",
    "bloodsac",
    "eternalflow",
    "onehundredhands",
    "opportunist",
    "perpetualmotion",
    "plaguebearer",
    "plagueoflocusts",
    "sharedstrength",
    "stonewall",
    "stormcaller",
    "stormofarrows",
    "unitedpack",
    "unstoppable",
    "widecircle",
    "widerburst",
    "widerpack",
    "windborn",
    "windrunner",
    // Drift batch (2026-08-25): the audit's Group-B COUNT nodes, each
    // confirmed a plain arithmetic count at its own call site before the
    // `passive_node_rank` read was switched to `passive_node_count`.
    "cursedblood", // warlock - fight-start auto-curse target count
    "golemmaster", // elementalist - golem slots (spawn, slot-unlock check and admin picker all read it)
    "livingbond", // druid - Wild Roar per-fight charges
    "naturesembrace", // druid - on-death full-heal target count
    "risingphoenix", // elementalist - per-combat revive limit (the old `.min(3)` is redundant: effective_rank caps growth at 3)
    "verdantburst", // druid - Verdant Burst save charges (threshold half stays a LiveTunable)
    "virulence", // warlock - Soul Stone bank size
    // Stage 3 (2026-08-27): the batch's COUNT nodes, each confirmed a
    // plain arithmetic count at its own call site before its
    // `passive_node_rank` read was switched to `passive_node_count`.
    "assassinate", // rogue - guaranteed-crit charges per fight (0/1/2)
    "bloodrush", // berserker - Undying Fury charges (0/1/2)
    "doubletap", // rogue - Twin Strikes repeat cap (3/6/9)
    "frenzy", // berserker - extra strikes per proc (1/2/3)
    "gloriousdeath", // berserker - Glorious Death charges (1/1/2)
    "guardianspirit", // cleric - party-wide death saves per fight (0/1/2)
    "lastrites", // slayer - party-wide death save charges (1/1/1)
    "markedfordeath", // rogue - marked hits that count as crits (0/2/3)
    "quickdraw", // rogue - free Fleetfoot stacks on entering combat (0/1/2)
    "shattering", // elementalist - Water Golem icicle extra targets (1/2/3)
    "undying", // slayer - FlickerStrike death-save charges (1/1/2)
    "undyingwill", // warrior - Undying Will charges (0/1/2)
];

/// Nodes that declare a real per-rank effect which **nothing in the
/// codebase reads** - no `passive_node_magnitude`, no
/// `passive_node_rank`, no call site of any kind.
///
/// Distinct from `PENDING_MIGRATION_NODES` (whose values DO reach the
/// game, just via hardcoded constants awaiting migration) and from
/// `NotYetImplemented` (which declares no value at all). These sit in
/// between: the value exists in the declaration, and overriding it
/// would change nothing, because no mechanic consumes it yet.
///
/// Found in Stage 2 (2026-08-20) while dumping every migration
/// candidate's call sites: both were listed as pending migration, but
/// have no consumer to migrate. Listed separately so `/admin/passives`
/// can say the accurate thing rather than promising a batch that would
/// have nothing to do.
pub const UNWIRED_NODES: &[&str] = &[
    "stillwater",  // monk - "Serenity triggers guaranteed on your first evade each fight"
];

/// Nodes where ONE numeric aspect is still fed by `passive_node_rank`
/// (structure) even though the node's PRIMARY magnitude IS live-tunable
/// through this store - the "half-tunable" shape.
///
/// **Why a second table:** `node_is_tunable` is a whole-node boolean and
/// cannot express "half". Hiding the row would cut off the node's wired
/// aspect (an override on it genuinely works); offering it silently
/// would repeat the inert-input lie Stage 0 exists to kill. A key→note
/// side table is the smallest mechanism that expresses the half:
/// `/admin/passives` keeps the input AND shows exactly which secondary
/// aspect still reads node RANK until its migration batch lands.
///
/// Every entry must have BOTH kinds of reads in the sources (a wired
/// magnitude/count read AND a rank read) - enforced by
/// `partially_tunable_nodes_are_half_by_construction_and_stay_offered`
/// below. mercifultouch was audited as a candidate and DELIBERATELY
/// excluded: its rank read is only an invested-gate; its value comes
/// from `passive_node_magnitude`.
///
/// **Drift batch (2026-08-25):** three of the original seven entries left
/// this list rather than being code-changed, because nothing of their own
/// was rank-fed anymore:
/// - `ravage` - the 0.5 stack value already came off the node's magnitude
///   (Stage 2 count batch); its remaining `rank >= 3` read is the unlock
///   gate, which is structure (same shipped shape as empoweredbolt).
/// - `endlessthirst` - same: the cap bonus was already magnitude-fed; the
///   `rank >= 3` "cap removed entirely" read is an unlock rule.
/// - `naturesblessing` - its rank-2/3 reads only RELEASE sibling nodes'
///   bonuses (bloomstrike/wildinstinct, both tunable on their own); the
///   node owns no number that rank feeds.
/// The three entries kept below each have a genuine SECOND VALUE of their
/// own still fed by rank, with no second value slot to declare it in (a
/// node has exactly one magnitude table) - migrating them would be a
/// structural change and they stay listed per the audit's OQ6.
pub const PARTIALLY_TUNABLE_NODES: &[(&str, &str)] = &[
    (
        "bloomingfield",
        "the heal-power multiplier is live here, but the bounce-target count still reads node RANK in combat.rs.",
    ),
    (
        "reaperscall",
        "the chain chance is live here, but the chain max-extra-targets count still reads node RANK in combat.rs.",
    ),
    (
        "sacrifice",
        "the Bloodpact HP cost is live here, but Bloodpact's damage multiplier (1 + rank) still reads node RANK in combat.rs.",
    ),
];

/// The note `/admin/passives` should show for `key`'s untunable secondary
/// aspect - `None` when the node has no known rank-fed aspect.
pub fn node_partial_tunable_note(key: &str) -> Option<&'static str> {
    PARTIALLY_TUNABLE_NODES.iter().find(|(k, _)| *k == key).map(|(_, note)| *note)
}

/// Whether an override on `key` would actually reach the game today.
/// `false` means either the node still hardcodes its numbers in
/// `combat.rs` and is waiting on its migration batch
/// (`PENDING_MIGRATION_NODES`), or nothing reads its value at all
/// (`UNWIRED_NODES`).
pub fn node_is_tunable(key: &str) -> bool {
    !PENDING_MIGRATION_NODES.contains(&key) && !UNWIRED_NODES.contains(&key)
}

/// Why `key` cannot be tuned, for the admin page to show - `None` when
/// it can be.
pub fn node_untunable_reason(key: &str) -> Option<&'static str> {
    if PENDING_MIGRATION_NODES.contains(&key) {
        // Reworded 2026-08-27 (Stage 3): every node still on this list is
        // there because it owns no rank-fed value a single magnitude table
        // could carry - an unlock gate (structure), or a second value with
        // no slot to declare it in - not because a batch is coming for it.
        // Promising one would be the same inert-input lie this list exists
        // to kill. See `PENDING_MIGRATION_NODES`' own doc for which is which.
        Some("Pending migration — this node owns no per-rank value an override could carry: its rank feeds an unlock gate (structure), or a second value this store has no slot for. An override here would do nothing.")
    } else if UNWIRED_NODES.contains(&key) {
        Some("Declared but unread — this node has real per-rank values, but no code consumes them yet, so there is nothing an override could change.")
    } else {
        None
    }
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
        PassiveOverrides { nodes: entries.iter().map(|(k, v)| (k.to_string(), v.to_vec())).collect(), ..Default::default() }
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

    #[test]
    fn a_legacy_file_without_a_caps_table_parses_unchanged() {
        // The shape every adventure-passive-overrides.toml had before
        // per-node conversion caps shipped: it must keep loading, with
        // no cap entries invented.
        let text = "[nodes]\nbulwark = [0.08, 0.14, 0.20]\n";
        let parsed: PassiveOverrides = toml::from_str(text).expect("the legacy layout must still parse");
        assert_eq!(parsed.value_for("bulwark", 1), Some(0.08));
        assert!(parsed.conversion_caps.is_empty(), "no [conversion_caps] table must mean no caps");
    }

    #[test]
    fn conversion_caps_round_trip_and_revert_clears_both_axes() {
        let mut o = overrides(&[("bulwark", &[0.08, 0.14, 0.20])]);
        o.conversion_caps.insert("stonefist".to_string(), 0.05);
        let text = toml::to_string_pretty(&o).expect("must serialize");
        assert!(text.contains("[conversion_caps]"), "caps get their own table, got:\n{text}");
        let back: PassiveOverrides = toml::from_str(&text).expect("must deserialize");
        assert_eq!(back.conversion_cap_for("stonefist"), Some(0.05));
        // A cap-only override IS an override - the badge and Revert key
        // off this - but it must not invent magnitude values.
        assert!(back.has_override("stonefist"));
        assert_eq!(back.nodes.get("stonefist"), None);
        let mut reverted = back;
        reverted.revert("stonefist");
        assert!(!reverted.has_override("stonefist"));
        assert_eq!(reverted.conversion_cap_for("stonefist"), None);
        // ...and the untouched magnitude override survives next door.
        assert_eq!(reverted.value_for("bulwark", 1), Some(0.08));
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
        // Pinned so a silent edit cannot drift the list without the
        // suite noticing. 28 was the 2026-08-19 audit; Stage 0
        // (2026-08-24, docs/tunable_audit.md §3 + the CI consumption
        // guard below) added the 19 rank-only-consumed nodes its scan
        // found, taking it to 47; the drift batch (2026-08-25) migrated
        // 16 of those Group-B nodes back out, taking it to 31. This
        // number only moves again when a migration batch deletes its
        // entries or a NEW guard-verified drift is listed - both
        // deliberate, reviewed changes.
        assert_eq!(PENDING_MIGRATION_NODES.len(), 6, "Stage 0 grew the list to 47; the 2026-08-25 drift batch took it to 31; Stage 3 (2026-08-27) took it to 6 - see docs/passive_tunables_spec.md");
        let unique: std::collections::HashSet<&str> = PENDING_MIGRATION_NODES.iter().copied().collect();
        assert_eq!(unique.len(), 6, "the pending list must not contain duplicates");
    }

    #[test]
    fn rising_blaze_is_wired_and_tunable() {
        // Wired 2026-08-21 (Paladin Holy Fire branch fix), reworked same
        // day per owner ruling: Holy Fire already reaches every alive
        // enemy, so "+1 additional random enemy per rank" had nothing
        // left to add. Now reads as +10/20/30% to Holy Fire's damage
        // contribution per rank - a plain percentage magnitude, not a
        // count, so it stays OUT of INTEGER_COUNT_NODES.
        assert!(node_is_tunable("risingblaze"), "risingblaze now has a reader and must be offered");
        assert!(node_untunable_reason("risingblaze").is_none());
        assert!(!INTEGER_COUNT_NODES.contains(&"risingblaze"), "its magnitude is a damage-contribution PERCENTAGE, not a count");
        assert!(!UNWIRED_NODES.contains(&"risingblaze"), "it is no longer unwired");
        let n = node(Archetype::Paladin, "risingblaze");
        let empty = PassiveOverrides::default();
        for (rank, expected) in [(1u32, 0.10), (2, 0.20), (3, 0.30)] {
            assert!((n.magnitude_at_rank_with(rank, &empty) - expected).abs() < 1e-9, "rank {rank} must read {expected} (+{}% Holy Fire damage contribution)", (expected * 100.0) as u32);
        }
    }

    #[test]
    fn tunability_is_reported_correctly_for_both_kinds_of_node() {
        assert!(!node_is_tunable("clarity"), "clarity owns no rank-fed number - only an unlock gate");
        assert!(node_is_tunable("bulwark"), "bulwark reads its value through the accessor");
    }

    // ---- Stage 2 (2026-08-20): the count-node migration -------------

    #[test]
    fn every_migrated_count_node_is_now_tunable() {
        // The point of the batch. Each of these read
        // `passive_node_rank` directly before Stage 2 and now reads
        // `passive_node_count`, which routes through the override hook.
        for key in INTEGER_COUNT_NODES {
            assert!(node_is_tunable(key), "{key:?} was migrated in Stage 2 and must no longer be listed as pending");
            assert!(node_untunable_reason(key).is_none(), "{key:?} must have no untunable reason");
        }
    }

    #[test]
    fn a_migrated_count_node_reads_its_override_as_a_whole_number() {
        // `magnitude_at_rank` is f64; the count call sites need a u32.
        // Rounding (not truncation) is what makes a tuned 2.999 read as
        // 3 rather than silently becoming 2.
        let node = node(Archetype::Ranger, "widerburst");
        let o = overrides(&[("widerburst", &[2.0, 2.999, 4.0])]);
        assert_eq!(node.magnitude_at_rank_with(1, &o).round() as u32, 2);
        assert_eq!(node.magnitude_at_rank_with(2, &o).round() as u32, 3);
        assert_eq!(node.magnitude_at_rank_with(3, &o).round() as u32, 4);
    }

    #[test]
    fn a_negative_override_on_a_count_floors_at_zero_rather_than_wrapping() {
        // `as u32` on a negative f64 saturates to 0 in Rust, but the
        // explicit `.max(0.0)` in `passive_node_count` states the intent
        // rather than relying on that - a huge wrapped count would be a
        // spectacular way to break a fight.
        let node = node(Archetype::Ranger, "widerburst");
        let o = overrides(&[("widerburst", &[-5.0, -5.0, -5.0])]);
        assert_eq!(node.magnitude_at_rank_with(1, &o).round().max(0.0) as u32, 0);
    }

    #[test]
    fn every_integer_count_node_has_a_whole_number_default() {
        // The invariant that actually matters for a COUNT node: its
        // default magnitude is a whole number at every rank, so the
        // admin page's integer-only input round-trips it and
        // `passive_node_count`'s conversion never truncates anything.
        //
        // This used to assert the stronger "magnitude EQUALS the rank",
        // which held only because every node Stage 2 migrated happened
        // to declare `1.0 / 1.0`. That was a property of THAT batch, not
        // of counts in general - `infiniteloop` counts in threes (3/6/9)
        // and broke it the moment the Mage batch migrated it. Per-batch
        // neutrality proofs now live with their own batches; this guards
        // the property the list itself exists for.
        let empty = PassiveOverrides::default();
        for key in INTEGER_COUNT_NODES {
            let (_, n) = crate::adventure::ALL_ARCHETYPES
                .iter()
                .find_map(|&a| a.passive_nodes().iter().find(|n| n.key == *key).map(|n| (a, n)))
                .unwrap_or_else(|| panic!("{key:?} must exist"));
            for rank in 1..=n.max_rank {
                let mag = n.magnitude_at_rank_with(rank, &empty);
                assert_eq!(mag, mag.round(), "{key:?} rank {rank} is listed as an integer count but its default magnitude {mag} is not whole");
                assert!(mag >= 0.0, "{key:?} rank {rank} has a negative count default");
            }
        }
    }

    #[test]
    fn stage_2_count_nodes_still_equal_their_rank_exactly() {
        // Stage 2's own neutrality proof, kept now that the general
        // count invariant above no longer implies it. Every node Stage 2
        // migrated declares `1.0 / 1.0`, so magnitude == rank exactly and
        // swapping a rank read for a magnitude read could not move a
        // number. `infiniteloop` is excluded - it was migrated later, by
        // the Mage batch, and counts in threes. The Stage 3 (2026-08-27)
        // additions are excluded for the same reason: their real ladders
        // are 0/1/2, 1/1/2, 3/6/9 and the like, so magnitude deliberately
        // does NOT equal rank - `stage3_declarations_reproduce_the_old_
        // rank_fed_values_exactly` is their neutrality proof instead.
        let empty = PassiveOverrides::default();
        for key in INTEGER_COUNT_NODES.iter().filter(|k| **k != "infiniteloop" && !STAGE_3_MIGRATED.iter().any(|(_, s3, _)| s3 == *k)) {
            let (_, n) = crate::adventure::ALL_ARCHETYPES
                .iter()
                .find_map(|&a| a.passive_nodes().iter().find(|n| n.key == *key).map(|n| (a, n)))
                .unwrap_or_else(|| panic!("{key:?} must exist"));
            for rank in 1..=n.max_rank {
                let expected = n.effective_rank(rank) as f64;
                assert_eq!(n.magnitude_at_rank_with(rank, &empty), expected, "{key:?} rank {rank} must equal its effective rank exactly");
            }
        }
    }

    #[test]
    fn infinite_loop_migration_is_exactly_behavior_neutral() {
        // The site read `rank * 3`; the node now declares that 3 / 6 / 9
        // table directly.
        let node = node(Archetype::Mage, "infiniteloop");
        let empty = PassiveOverrides::default();
        for rank in 1..=3u32 {
            assert_eq!(node.magnitude_at_rank_with(rank, &empty), (rank * 3) as f64, "infiniteloop rank {rank} must reproduce rank * 3");
        }
    }

    #[test]
    fn the_mage_batch_per_rank_tables_reproduce_their_old_ladders() {
        // The three non-linear Mage nodes, each asserted against the
        // exact literals its `combat.rs` ladder used before migration.
        // These are why `SpecialPerRank` exists: not one of them can be
        // written as `at_rank_1 + per_additional_rank * (rank - 1)`.
        let empty = PassiveOverrides::default();
        for (key, expected) in [("absolutezero", [0.0, 0.50, 0.65]), ("arcaneinstability", [0.05, 0.09, 0.12]), ("empoweredbolt", [0.0, 0.0, 0.20])] {
            let n = node(Archetype::Mage, key);
            for (i, want) in expected.iter().enumerate() {
                let rank = i as u32 + 1;
                assert_eq!(n.magnitude_at_rank_with(rank, &empty), *want, "{key} rank {rank} must reproduce its old ladder value exactly");
            }
        }
    }

    #[test]
    fn a_per_rank_table_reads_zero_outside_its_range() {
        // `SpecialPerRank` indexes by effective rank; rank 0, and any
        // rank past the table's end, must read 0.0 - the same thing an
        // unallocated node reads - rather than panicking or saturating.
        let empty = PassiveOverrides::default();
        assert_eq!(node(Archetype::Mage, "absolutezero").magnitude_at_rank_with(0, &empty), 0.0);
    }

    #[test]
    fn unwired_nodes_are_not_offered_as_tunable_and_say_why() {
        // These declare real per-rank values that nothing reads, so an
        // override would silently do nothing - a different situation
        // from "pending migration", and the page must not conflate them.
        for key in UNWIRED_NODES {
            assert!(!node_is_tunable(key), "{key:?} has no consumer, so it must not be offered");
            let reason = node_untunable_reason(key).expect("an unwired node must explain itself");
            assert!(reason.contains("Declared but unread"), "{key:?} must get the unwired reason, not the migration one, got: {reason}");
        }
    }

    #[test]
    fn the_three_node_classifications_never_overlap() {
        for key in PENDING_MIGRATION_NODES {
            assert!(!UNWIRED_NODES.contains(key), "{key:?} cannot be both pending migration and unwired");
            assert!(!INTEGER_COUNT_NODES.contains(key), "{key:?} cannot be both pending migration and already migrated");
        }
        for key in UNWIRED_NODES {
            assert!(!INTEGER_COUNT_NODES.contains(key), "{key:?} cannot be both unwired and migrated");
        }
    }

    #[test]
    fn chain_of_light_at_four_four_now_matches_its_own_description() {
        // The ONE deliberate behavior change in Stage 2, on an explicit
        // owner decision. Chain of Light is a Specialization, so it can
        // hold rank 4; `combat.rs` used to read the raw rank and hand a
        // 4/4 investment a 5th bounce target. Its description says "2
        // total targets at rank 1, up to 4 at rank 3", and the tree
        // documents a Spec's 4th point as unlock-only.
        //
        // Pinned here explicitly because the golden corpus CANNOT catch
        // this: `run_scenario` never populates `passive_allocations`, so
        // every node sits at rank 0 in every fixture and no passive
        // mechanic fires at all.
        let node = node(Archetype::Cleric, "chainoflight");
        let empty = PassiveOverrides::default();
        // `prayer_bounce_targets` is `1 + count`, so these are the
        // target totals the description promises.
        assert_eq!(1 + node.magnitude_at_rank_with(1, &empty).round() as u32, 2, "rank 1 = 2 total targets");
        assert_eq!(1 + node.magnitude_at_rank_with(3, &empty).round() as u32, 4, "rank 3 = 4 total targets");
        assert_eq!(
            1 + node.magnitude_at_rank_with(4, &empty).round() as u32,
            4,
            "4/4 must now also be 4, not 5 - the 4th point unlocks modifiers, it does not add a target"
        );
    }

    #[test]
    fn final_judgment_migration_is_exactly_behavior_neutral() {
        // Stage 3, Paladin batch. `combat.rs` used to pre-compute
        // Judgment's raised threshold as three literals (0.60/0.65/0.70);
        // it now reads `JUDGMENT_BASE_THRESHOLD + magnitude`, where the
        // magnitude is the node's own declared 0.10/0.15/0.20 delta.
        //
        // Asserted with `==`, not an epsilon, on purpose: the entire
        // claim is that this yields the IDENTICAL f64, so an approximate
        // check would hide precisely the drift worth catching. All three
        // sums land bit-exactly on the old literals - verified before
        // relying on it, since float addition does not always oblige.
        let node = node(Archetype::Paladin, "finaljudgment");
        let empty = PassiveOverrides::default();
        let base = crate::adventure::JUDGMENT_BASE_THRESHOLD;
        for (rank, expected) in [(1u32, 0.60_f64), (2, 0.65), (3, 0.70)] {
            let migrated = base + node.magnitude_at_rank_with(rank, &empty);
            assert_eq!(migrated, expected, "Final Judgment rank {rank} must reproduce the old literal exactly");
        }
    }

    // ---- Drift batch (2026-08-25): tunable_audit.md §3 Groups B+C ----

    #[test]
    fn drift_batch_declarations_reproduce_the_old_rank_fed_values_exactly() {
        // Each entry pins a migrated node's declared table against the
        // exact formula its combat.rs call site used before the drift
        // batch - `==`, not an epsilon, for the same reason
        // `final_judgment_migration_is_exactly_behavior_neutral` gives:
        // an approximate check would hide exactly the drift that matters.
        let empty = PassiveOverrides::default();
        let mag = |archetype: Archetype, key: &str, rank: u32| node(archetype, key).magnitude_at_rank_with(rank, &empty);

        // Berserker deathdefiant: grace was `rank * 3000ms`; declared in seconds.
        for rank in 1..=3u32 {
            assert_eq!(mag(Archetype::Berserker, "deathdefiant", rank) * 1000.0, (rank * 3_000) as f64);
        }
        // Mage timewarp / Warlock demonicspeed: window was `5000 + 2000*rank` ms.
        for (archetype, key) in [(Archetype::Mage, "timewarp"), (Archetype::Warlock, "demonicspeed")] {
            for rank in 1..=3u32 {
                assert_eq!(mag(archetype, key, rank) * 1000.0, (5_000 + 2_000 * rank) as f64);
            }
        }
        // Paladin unwavering / Cleric unyieldingfaith: threshold ladder 0/.50/.65.
        for (archetype, key) in [(Archetype::Paladin, "unwavering"), (Archetype::Cleric, "unyieldingfaith")] {
            for (rank, expected) in [(1u32, 0.0), (2, 0.50), (3, 0.65)] {
                assert_eq!(mag(archetype, key, rank), expected);
            }
        }
        // Ranger huntersfocus: ally share was `rank / 3` - bit-exact.
        for rank in 1..=3u32 {
            assert_eq!(mag(Archetype::Ranger, "huntersfocus", rank), rank as f64 / 3.0, "huntersfocus rank {rank} must be the identical f64 the old division produced");
        }
        // Elementalist healingflames: irregular 3/6/10% (was `healing_flames_regen_pct`).
        for (rank, expected) in [(1u32, 0.03), (2, 0.06), (3, 0.10)] {
            assert_eq!(mag(Archetype::Elementalist, "healingflames", rank), expected);
        }
        // Rank 4 floors at rank 3's row (Specialization rule), matching the
        // old lookup's `r if r >= 3 => 0.10` arm.
        assert_eq!(mag(Archetype::Elementalist, "healingflames", 4), 0.10);
        // Elementalist blazing: irregular 6/9/18% (was `blazing_attack_speed_pct`).
        for (rank, expected) in [(1u32, 0.06), (2, 0.09), (3, 0.18)] {
            assert_eq!(mag(Archetype::Elementalist, "blazing", rank), expected);
        }
        // Slayer finaloffering: unlock ladder was `4 - rank` prior uses.
        for rank in 1..=3u32 {
            assert_eq!(mag(Archetype::Slayer, "finaloffering", rank), (4 - rank) as f64);
        }
        // Slayer unrelenting: `rank*1333ms` below rank 3, flat 600_000 at rank 3.
        for (rank, expected) in [(1u32, 1333.0), (2, 2666.0), (3, 600_000.0)] {
            assert_eq!(mag(Archetype::Slayer, "unrelenting", rank), expected);
        }
        // The seven COUNT nodes keep magnitude == rank exactly (their
        // declarations were already `1.0 / 1.0`), so the swap to
        // `passive_node_count` is neutral by construction.
        for (archetype, key) in [
            (Archetype::Warlock, "virulence"),
            (Archetype::Warlock, "cursedblood"),
            (Archetype::Elementalist, "golemmaster"),
            (Archetype::Elementalist, "risingphoenix"),
            (Archetype::Druid, "livingbond"),
            (Archetype::Druid, "naturesembrace"),
            (Archetype::Druid, "verdantburst"),
        ] {
            for rank in 1..=3u32 {
                assert_eq!(node(archetype, key).magnitude_at_rank_with(rank, &empty), rank as f64, "{key} must stay a whole count of {rank} at rank {rank}");
            }
        }
    }

    #[test]
    fn final_judgment_is_now_tunable() {
        assert!(node_is_tunable("finaljudgment"), "the Paladin batch migrated it");
        assert!(node_untunable_reason("finaljudgment").is_none());
        // An override must actually move the threshold - that is the
        // point of migrating it.
        let node = node(Archetype::Paladin, "finaljudgment");
        let o = overrides(&[("finaljudgment", &[0.25, 0.30, 0.35])]);
        assert_eq!(crate::adventure::JUDGMENT_BASE_THRESHOLD + node.magnitude_at_rank_with(3, &o), 0.85);
    }

    #[test]
    fn drift_batch_nodes_are_tunable_and_overrides_reach_their_values() {
        // All sixteen Group-B keys left PENDING_MIGRATION_NODES this batch:
        // each must now be offered as tunable with no untunable reason,
        // looked up against its own archetype's tree.
        for (archetype, key) in [
            (Archetype::Berserker, "deathdefiant"),
            (Archetype::Mage, "timewarp"),
            (Archetype::Warlock, "demonicspeed"),
            (Archetype::Paladin, "unwavering"),
            (Archetype::Cleric, "unyieldingfaith"),
            (Archetype::Ranger, "huntersfocus"),
            (Archetype::Elementalist, "golemmaster"),
            (Archetype::Elementalist, "healingflames"),
            (Archetype::Elementalist, "blazing"),
            (Archetype::Elementalist, "risingphoenix"),
            (Archetype::Warlock, "virulence"),
            (Archetype::Warlock, "cursedblood"),
            (Archetype::Druid, "livingbond"),
            (Archetype::Druid, "naturesembrace"),
            (Archetype::Druid, "verdantburst"),
            (Archetype::Slayer, "finaloffering"),
        ] {
            assert!(node_is_tunable(key), "{key:?} was migrated by the drift batch and must no longer be listed as pending");
            assert!(node_untunable_reason(key).is_none(), "{key:?} must have no untunable reason");
            assert!(archetype.passive_nodes().iter().any(|n| n.key == key), "{key:?} lookup must use the right archetype");
        }

        // And an override genuinely moves what combat.rs reads now.
        let empty = PassiveOverrides::default();
        // Threshold ladder: an unwavering override moves the low-HP party-DR trigger.
        let unwavering = node(Archetype::Paladin, "unwavering");
        let o = overrides(&[("unwavering", &[0.10, 0.20, 0.30])]);
        assert_eq!(unwavering.magnitude_at_rank_with(2, &o), 0.20);
        assert_ne!(unwavering.magnitude_at_rank_with(2, &o), unwavering.magnitude_at_rank_with(2, &empty));
        // Count nodes: an override raises the count past the old rank cap
        // (golemmaster slots, risingphoenix revives) - rounding keeps them
        // whole numbers, and negatives floor at 0 in `passive_node_count`.
        let golem = node(Archetype::Elementalist, "golemmaster");
        let o = overrides(&[("golemmaster", &[5.0])]);
        assert_eq!(golem.magnitude_at_rank_with(1, &o).round() as u32, 5);
        // Irregular tables tune per rank like any other node.
        let flames = node(Archetype::Elementalist, "healingflames");
        let o = overrides(&[("healingflames", &[0.04, 0.07, 0.12])]);
        assert_eq!(flames.magnitude_at_rank_with(3, &o), 0.12);
    }

    // ---- Stage 3 (2026-08-27): the rank-fed backlog lands ------------

    /// Every node Stage 3 migrated, paired with the EXACT per-rank values
    /// its `combat.rs` call site produced from the RAW RANK before the
    /// batch - read off each call site one at a time, never inferred from
    /// the node's old declaration, because several of those declarations
    /// disagreed with the game (`payback` declared 0.30/0.45/0.60 while
    /// combat.rs used 0/0.30/0.45; `crush`, `secondwind`, `vitalstrike`,
    /// `gloriousdeath`, `undying` and `doubletap` were all off in the same
    /// way). These are the GAME's numbers, which is what makes the batch
    /// behavior-neutral at defaults.
    ///
    /// One table, two tests: the first pins the migration as neutral, the
    /// second pins every entry as genuinely reachable by an override.
    const STAGE_3_MIGRATED: &[(Archetype, &str, [f64; 3])] = &[
        // Warrior
        (Archetype::Warrior, "payback", [0.0, 0.30, 0.45]),
        (Archetype::Warrior, "secondwind", [0.0, 0.50, 0.65]),
        (Archetype::Warrior, "undyingwill", [0.0, 1.0, 2.0]),
        // Berserker
        (Archetype::Berserker, "frenzy", [1.0, 2.0, 3.0]),
        (Archetype::Berserker, "bloodscent", [0.0, 0.50, 0.65]),
        (Archetype::Berserker, "bloodrush", [0.0, 1.0, 2.0]),
        (Archetype::Berserker, "shatter", [1.0, 1.0, 1.0]),
        (Archetype::Berserker, "crush", [0.0, 0.50, 0.65]),
        (Archetype::Berserker, "gloriousdeath", [1.0, 1.0, 2.0]),
        // Rogue
        (Archetype::Rogue, "assassinate", [0.0, 1.0, 2.0]),
        (Archetype::Rogue, "markedfordeath", [0.0, 2.0, 3.0]),
        (Archetype::Rogue, "vitalstrike", [0.50, 0.65, 0.80]),
        (Archetype::Rogue, "surgicalstrike", [1.0, 1.0, 2.0]),
        (Archetype::Rogue, "doubletap", [3.0, 6.0, 9.0]),
        (Archetype::Rogue, "quickdraw", [0.0, 1.0, 2.0]),
        // Monk
        (Archetype::Monk, "relentlessassault", [0.0, 0.0, 2.0]),
        (Archetype::Monk, "chakraoflife", [1.0, 2.0, 3.0]),
        (Archetype::Monk, "unyieldingspirit", [0.35, 0.45, 0.55]),
        // Ranger
        (Archetype::Ranger, "piercingshots", [0.0, 0.0, 0.10]),
        (Archetype::Ranger, "finalblow", [0.35, 0.40, 0.45]),
        // Cleric
        (Archetype::Cleric, "guardianspirit", [0.0, 1.0, 2.0]),
        (Archetype::Cleric, "compassion", [0.0, 0.0, 0.05]),
        // Slayer
        (Archetype::Slayer, "undying", [1.0, 1.0, 2.0]),
        (Archetype::Slayer, "lastrites", [1.0, 1.0, 1.0]),
        // Elementalist
        (Archetype::Elementalist, "shattering", [1.0, 2.0, 3.0]),
    ];

    #[test]
    fn stage3_declarations_reproduce_the_old_rank_fed_values_exactly() {
        // `==`, not an epsilon, for the same reason
        // `final_judgment_migration_is_exactly_behavior_neutral` gives: an
        // approximate check would hide exactly the drift worth catching.
        // The two computed formulas the batch replaced are bit-exact on
        // these literals - `0.25 + 0.10 * rank` for Last Stand's 0.35/
        // 0.45/0.55, and `rank * 1000ms` for Chakra of Life's 1/2/3s -
        // verified before relying on it, since float addition does not
        // always oblige.
        let empty = PassiveOverrides::default();
        for (archetype, key, expected) in STAGE_3_MIGRATED {
            for (i, want) in expected.iter().enumerate() {
                let rank = i as u32 + 1;
                assert_eq!(
                    node(*archetype, key).magnitude_at_rank_with(rank, &empty),
                    *want,
                    "{key:?} rank {rank} must reproduce the value its call site used before Stage 3"
                );
            }
        }
    }

    #[test]
    fn stage3_migrated_nodes_are_tunable_and_every_override_reaches_the_value() {
        // The point of the batch. Each of these was consumed ONLY through
        // `passive_node_rank`, so an override on it was inert - the drift
        // defect class the anomaly ledger records as #48. Each must now be
        // offered, and an override must genuinely move the magnitude the
        // call site reads.
        let empty = PassiveOverrides::default();
        for (archetype, key, defaults) in STAGE_3_MIGRATED {
            assert!(node_is_tunable(key), "{key:?} was migrated by Stage 3 and must no longer be listed as pending");
            assert!(node_untunable_reason(key).is_none(), "{key:?} must have no untunable reason");
            assert!(archetype.passive_nodes().iter().any(|n| n.key == *key), "{key:?} lookup must use the right archetype");
            let n = node(*archetype, key);
            // Deliberately out of band, so no tuned value can coincide
            // with the default it is meant to displace.
            let tuned: Vec<f64> = defaults.iter().map(|d| d + 7.0).collect();
            let o = overrides(&[(key, tuned.as_slice())]);
            for rank in 1..=3u32 {
                let before = n.magnitude_at_rank_with(rank, &empty);
                let after = n.magnitude_at_rank_with(rank, &o);
                assert_ne!(after, before, "{key:?} rank {rank}: the override must not be swallowed");
                assert_eq!(after, before + 7.0, "{key:?} rank {rank}: the override must reach the value the call site reads");
            }
        }
    }

    #[test]
    fn stage3_count_nodes_are_declared_as_whole_numbers() {
        // Every Stage 3 node whose magnitude is a COUNT is listed in
        // INTEGER_COUNT_NODES (the admin page then refuses fractions on
        // it), and its declared defaults really are whole numbers - the
        // check that would have caught a rate being filed as a count.
        for (archetype, key, defaults) in STAGE_3_MIGRATED {
            if !INTEGER_COUNT_NODES.contains(key) {
                continue;
            }
            for (i, d) in defaults.iter().enumerate() {
                assert_eq!(d.fract(), 0.0, "{key:?} is listed as an integer count but declares {d} at rank {}", i + 1);
            }
            assert!(archetype.passive_nodes().iter().any(|n| n.key == *key));
        }
        // And the ones that are NOT counts are exactly the rates,
        // fractions and durations - pinned by name so a later edit cannot
        // quietly file one of them as a count and start rounding it.
        for key in ["payback", "secondwind", "bloodscent", "shatter", "crush", "vitalstrike", "surgicalstrike", "relentlessassault", "chakraoflife", "unyieldingspirit", "piercingshots", "finalblow", "compassion"] {
            assert!(!INTEGER_COUNT_NODES.contains(&key), "{key:?} is a fraction/duration, not a count - rounding it would break it");
        }
    }

    #[test]
    fn the_nodes_stage3_left_pending_own_no_rank_fed_value() {
        // Stage 3 deliberately did NOT migrate these six, and the list's
        // own doc says which of the two reasons applies to each. What is
        // asserted here is the part a future session could break by
        // accident: they stay OFF the page (an inert input is the lie
        // this list exists to kill), and the reason shown is the pending
        // one rather than the unwired one.
        assert_eq!(PENDING_MIGRATION_NODES, &["clarity", "deathwish", "lastlaugh", "neverending", "reckless", "sanctifiedtouch"]);
        for key in PENDING_MIGRATION_NODES {
            assert!(!node_is_tunable(key), "{key:?} must not be offered");
            let reason = node_untunable_reason(key).expect("a pending node must explain itself");
            assert!(reason.contains("no per-rank value an override could carry"), "{key:?} must get the reworded pending reason, got: {reason}");
        }
    }

    #[test]
    fn every_unwired_key_still_exists_in_the_tree() {
        for key in UNWIRED_NODES {
            let found = crate::adventure::ALL_ARCHETYPES.iter().any(|a| a.passive_nodes().iter().any(|n| n.key == *key));
            assert!(found, "{key:?} is listed as unwired but no longer exists in any archetype's tree");
        }
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

    // ---- Stage 0 (2026-08-24): the consumption guard ----------------

    /// Source scan helper for the guards below: concatenates the four
    /// files that hold every passive accessor call site
    /// (`combat.rs`, `character.rs`, `manager.rs`, `adventure_web.rs`),
    /// drops comment lines (doc comments legitimately MENTION accessor
    /// calls when explaining them), then strips all whitespace so a
    /// wrapped multi-line call still matches its single-line pattern.
    fn flat_source() -> String {
        let mut src = String::new();
        src.push_str(include_str!("combat.rs"));
        src.push_str(include_str!("character.rs"));
        src.push_str(include_str!("manager.rs"));
        src.push_str(include_str!("../adventure_web.rs"));
        let no_comments: String = src
            .split('\n')
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    ""
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        no_comments.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// THE `/admin/passives` honesty invariant: every node that declares a
    /// real value (`Special`/`SpecialPerRank`) must have that value reach
    /// the game through `passive_node_magnitude` or `passive_node_count`
    /// somewhere - or it must be tracked in `PENDING_MIGRATION_NODES` /
    /// `UNWIRED_NODES` so the page stops offering an input that would
    /// silently do nothing.
    ///
    /// FlatStat and OverflowConversion nodes are exempt: they are consumed
    /// GENERICALLY by `Character::passive_bonus`/
    /// `passive_overflow_bonus` and so have no direct read by design, and
    /// `NotYetImplemented` declares no value at all.
    ///
    /// A Special node whose only reads are `passive_node_rank` (structure)
    /// is exactly the inert-override shape the 2026-08-24 tunable audit
    /// documented (chakraoflife/unyieldingspirit/shattering et al.). This
    /// test is what keeps that class from regrowing: the repo's older
    /// existence checks could not see it, because a key can exist in the
    /// tree and still never be consumed.
    #[test]
    fn every_special_node_whose_value_has_no_magnitude_read_is_tracked_as_not_yet_tunable() {
        let flat = flat_source();
        let mut offenders: Vec<&str> = Vec::new();
        for archetype in crate::adventure::ALL_ARCHETYPES.iter() {
            for n in archetype.passive_nodes() {
                if matches!(
                    n.effect,
                    crate::passive_tree::PassiveEffect::FlatStat { .. }
                        | crate::passive_tree::PassiveEffect::OverflowConversion { .. }
                        | crate::passive_tree::PassiveEffect::NotYetImplemented
                ) {
                    continue;
                }
                let magnitude_read =
                    flat.contains(&format!("passive_node_magnitude(\"{}\")", n.key))
                        || flat.contains(&format!("passive_node_count(\"{}\")", n.key));
                if !magnitude_read && !PENDING_MIGRATION_NODES.contains(&n.key) && !UNWIRED_NODES.contains(&n.key) {
                    offenders.push(n.key);
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these Special nodes' declared values never reach the game (no \
             passive_node_magnitude/passive_node_count read anywhere), yet they are \
             still OFFERED as tunable on /admin/passives - add them to \
             PENDING_MIGRATION_NODES until their migration batch lands: {offenders:?}"
        );
    }

    /// The half-tunable table's own invariant: every entry really is
    /// MIXED - a wired magnitude/count read somewhere (its offered aspect)
    /// AND a rank read somewhere (its noted, not-yet-tunable aspect) -
    /// and every entry STAYS OFFERED, because hiding it would cut off the
    /// working half. A node that loses its rank-fed aspect (migration
    /// landed) must leave this list; a node with no magnitude read never
    /// belongs here (it's pending/unwired territory).
    #[test]
    fn partially_tunable_nodes_are_half_by_construction_and_stay_offered() {
        assert!(
            !PARTIALLY_TUNABLE_NODES.is_empty(),
            "Stage 0 tracked the mixed-read nodes; an empty list means they were all migrated - good, delete this test's expectations then"
        );
        let flat = flat_source();
        for (key, note) in PARTIALLY_TUNABLE_NODES {
            assert!(!note.is_empty(), "{key:?} must explain which aspect is still rank-fed");
            assert!(node_is_tunable(key), "{key:?} has a live primary aspect and must keep its input on /admin/passives");
            assert!(node_untunable_reason(key).is_none(), "{key:?} must not carry an untunable reason while partially tunable");
            // The WIRED half's proof: a DIRECT magnitude/count read - or
            // membership in the generically-consumed shapes (FlatStat /
            // OverflowConversion), whose values flow through
            // passive_bonus/passive_overflow_bonus with no direct call
            // site. Both are genuine override paths; only "no consumer of
            // either kind" would mean pending/unwired instead.
            let declared = crate::adventure::ALL_ARCHETYPES
                .iter()
                .find_map(|a| a.passive_nodes().iter().find(|n| n.key == *key))
                .unwrap_or_else(|| panic!("{key:?} is listed as partially tunable but no longer exists in any archetype's tree"));
            let generically_consumed = matches!(
                declared.effect,
                crate::passive_tree::PassiveEffect::FlatStat { .. } | crate::passive_tree::PassiveEffect::OverflowConversion { .. }
            );
            let magnitude_read = flat.contains(&format!("passive_node_magnitude(\"{key}\")"))
                || flat.contains(&format!("passive_node_count(\"{key}\")"));
            assert!(
                magnitude_read || generically_consumed,
                "{key:?} claims to be half-tunable but its primary value has no consumer (no direct read, not generic-pooled) - it belongs in PENDING_MIGRATION_NODES instead"
            );
            assert!(
                flat.contains(&format!("passive_node_rank(\"{key}\")")),
                "{key:?} claims a rank-fed secondary aspect but no passive_node_rank read exists anymore - migration landed, remove it from PARTIALLY_TUNABLE_NODES"
            );
        }
    }

    #[test]
    fn partially_tunable_never_overlaps_the_hard_off_lists() {
        for (key, _) in PARTIALLY_TUNABLE_NODES {
            assert!(!PENDING_MIGRATION_NODES.contains(key), "{key:?} cannot be both partially tunable and pending - pending hides its working input");
            assert!(!UNWIRED_NODES.contains(key), "{key:?} cannot be both partially tunable and unwired");
            assert!(!INTEGER_COUNT_NODES.contains(key), "{key:?} cannot be both partially tunable and an integer-count node");
        }
    }
}
