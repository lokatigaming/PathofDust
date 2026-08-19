//! Memories (2026-08-19) - quickly swappable saved passive-tree builds,
//! so a character can move between roles without manually re-allocating
//! their whole tree. See `docs/memories_spec.md` for the full design and
//! the decisions log.
//!
//! A `Memory` captures a character's ENTIRE build: archetype, primary
//! passive allocations, Split Personality's secondary archetype + its own
//! allocations, and Elementalist's golem slot types. Loading one fully
//! becomes that build - free, bypassing the respec/class-change costs,
//! and allowed out of combat only.
//!
//! **The load never raw-writes allocations.** Every stored rank is
//! replayed through `passive_tree::validate_allocation_step` - the exact
//! same node-existence/`max_rank`/parent-gate rules
//! `AdventureManager::preview_allocate_passive` enforces on a live click
//! (they call the same function; see its own doc). That is what makes
//! "a Memory can never produce a tree state the normal UI couldn't have
//! built" a property of the code rather than a claim in a comment.
//!
//! **Nothing here does I/O or takes a lock** - it is all plain functions
//! over plain data, same Phase 1 precedent every other domain module in
//! this crate follows, so the whole policy surface is unit-testable
//! without a manager, a character file, or a running server. The
//! manager-side wrappers (`save_memory`/`load_memory`/`rename_memory`/
//! `delete_memory`, in manager.rs) are thin: lock, call in here,
//! persist, broadcast.

use crate::adventure::{Archetype, GolemType, PassiveError};
use crate::passive_tree::{validate_allocation_step, PassiveTier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Starting `Character::memory_slots` for everyone - deliberately a
/// per-character VALUE rather than a global constant read at every use
/// site (see that field's own doc), so a future feature can grant an
/// individual character extra slots without a migration. This constant
/// is only the DEFAULT; nothing downstream may assume slot count is 3.
/// Same "one constant, both paths" idiom as
/// `STARTING_FREE_ARCHETYPE_CHANGES` - `Character::new` and
/// `default_memory_slots` (the serde-default/migration-grant path) both
/// read this.
pub const STARTING_MEMORY_SLOTS: u32 = 3;

pub(crate) fn default_memory_slots() -> u32 {
    STARTING_MEMORY_SLOTS
}

/// Max length of a custom Memory name, in CHARACTERS (not bytes) - a
/// name is player-authored text the bot could end up echoing, so the
/// limit is counted the way a player would count it.
pub const MEMORY_NAME_MAX_LEN: usize = 150;

/// One saved passive-tree build - see the module doc. Every field is a
/// snapshot of the corresponding `Character` field at save time; loading
/// applies them back through the validator (see `replay_snapshot`).
/// `PartialEq` is derived (unlike `Character`, which deliberately isn't)
/// so a save-then-load round trip can be asserted directly rather than
/// through a `serde_json::to_value` comparison - every field here is
/// plainly comparable, with no float or generated-id noise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Player-facing name. Always written through
    /// `validate_memory_name` - never taken raw from a form.
    pub name: String,
    pub archetype: Archetype,
    pub passive_allocations: HashMap<String, u32>,
    /// The RAW `Character::secondary_archetype` at save time, NOT
    /// `effective_secondary_archetype()`. Storing the raw choice and
    /// re-deriving effectiveness at load time is what every other reader
    /// in this codebase does (see that getter's own doc) - a Memory
    /// saved while Split Personality was equipped and loaded while it
    /// isn't must resolve to "no secondary tree" live, not resurrect one.
    #[serde(default)]
    pub secondary_archetype: Option<Archetype>,
    #[serde(default)]
    pub secondary_passive_allocations: HashMap<String, u32>,
    /// Elementalist's golem slot choices - stored and restored verbatim,
    /// including for a non-Elementalist build where they're simply
    /// inert. Same non-lossy reasoning `Character::golem_slot_types`'s
    /// own doc already gives for never trimming the vec on respec: a
    /// later load back into an Elementalist Memory restores the prior
    /// choices for free.
    #[serde(default)]
    pub golem_slot_types: Vec<GolemType>,
    /// Unix seconds at save time, for the slot summary line on
    /// `/passives`. 0 on a Memory saved before this field existed (there
    /// are none in the wild - the field shipped with the feature - but
    /// `#[serde(default)]` costs nothing and keeps the whole struct
    /// additively extensible the same way `Character` is).
    #[serde(default)]
    pub saved_at: u64,
}

/// Why a Memory action didn't go through - see `memory_error_text` in
/// adventure_web.rs for the player-facing wording. Deliberately separate
/// from `PassiveError` (which is about one node's allocation) rather
/// than growing new variants onto it: these are slot/name/timing
/// failures, and nothing that handles a `PassiveError` today would know
/// what to say about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    /// Hasn't `!join`ed the adventure yet.
    NotJoined,
    /// `slot` is at or past this character's own `memory_slots`.
    SlotOutOfRange,
    /// Nothing saved in that slot to load/rename/delete.
    SlotEmpty,
    /// A fight is running right now - see
    /// `AdventureManager::fight_in_progress`. Loads are out-of-combat
    /// only, by design: no queuing, no mid-fight swaps.
    InCombat,
    /// Commoner has no passive tree, so there is no build to snapshot.
    NoBuildToSave,
    /// The custom name failed `validate_memory_name`.
    InvalidName(NameRejection),
}

/// Why `validate_memory_name` rejected a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRejection {
    /// Empty, or nothing but whitespace.
    Empty,
    /// More than `MEMORY_NAME_MAX_LEN` characters after trimming.
    TooLong,
    /// Contains a control character, or one of the zero-width/bidi
    /// characters listed in `is_forbidden_char` - see its doc.
    Unprintable,
    /// Tripped `MEMORY_NAME_BLOCKLIST`.
    Blocked,
}

/// One allocation a load couldn't apply - see `replay_snapshot` and
/// `MemoryLoadReport`. Carries enough to tell the player exactly what
/// changed and why, rather than a bare "something was dropped".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedAllocation {
    pub node_key: String,
    /// The rank the snapshot stored (all of which is refunded).
    pub rank: u32,
    /// Which tree it came from - the player has two on screen.
    pub secondary: bool,
    pub reason: DropReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The node key isn't in this archetype's tree any more (renamed,
    /// removed, or the archetype itself changed under the snapshot).
    NodeGone,
    /// The node still exists but its `max_rank` shrank below the stored
    /// rank. The WHOLE entry is dropped and refunded rather than being
    /// silently clamped down to the new cap - a partially-applied rank
    /// is a build the player never chose, and refunding lets them
    /// re-spend deliberately.
    RankCapShrank,
    /// Its parent isn't invested deeply enough to unlock it. Reached
    /// either by cascade (the parent itself was dropped) or because the
    /// snapshot stored an orphan - the live tree permits orphans today
    /// (de-allocating a parent doesn't cascade to its children), and a
    /// load deliberately does NOT reproduce them; see the decisions log.
    ParentNotInvested,
    /// Dropped by the budget trim - the snapshot spent more points than
    /// the character currently has (see `trim_to_budget`).
    OverBudget,
}

/// What a load actually did - handed back so `/passives` can tell the
/// player what changed instead of silently producing a different build
/// than the one they saved. A load with `class_changed == false` and an
/// empty `dropped` applied cleanly and needs no message at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLoadReport {
    pub name: String,
    pub archetype: Archetype,
    /// The load changed `Character::archetype`. Free, by design - see
    /// the module doc and the economy note in docs/memories_spec.md.
    pub class_changed: bool,
    /// Everything that couldn't be applied, in the order it was dropped.
    pub dropped: Vec<DroppedAllocation>,
    /// The snapshot had a secondary tree but it wasn't applied, because
    /// Split Personality isn't equipped any more (or the stored
    /// secondary is now the same as the primary, which
    /// `effective_secondary_archetype` treats as unset). Reported
    /// separately from `dropped` because it's one whole-tree condition,
    /// not a per-node staleness.
    pub secondary_skipped: bool,
    /// Points left unspent after the load - the level-drift surplus plus
    /// anything refunded above.
    pub unspent: u32,
}

impl MemoryLoadReport {
    /// Whether this load did anything the player should be told about.
    /// A clean load returns `false` and `/passives` redirects silently,
    /// same as every other action on that page.
    pub fn is_noteworthy(&self) -> bool {
        self.class_changed || !self.dropped.is_empty() || self.secondary_skipped
    }
}

// ---------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------

/// The default name for a build - "Memories of a Warrior", or
/// "Memories of a Warrior & Druid" with a Split Personality secondary.
///
/// Judgment call (see docs/memories_spec.md's decisions log): the spec
/// wrote the pattern literally as "Memories of a <Class>", but four of
/// the twelve archetypes start with a vowel sound. Written to pick
/// "a"/"an" correctly ("Memories of an Elementalist") rather than
/// reproducing the article verbatim - a default name is player-facing
/// text, and the alternative reads as a typo.
pub fn default_memory_name(primary: Archetype, secondary: Option<Archetype>) -> String {
    let primary_name = format!("{primary:?}");
    let article = indefinite_article(&primary_name);
    match secondary {
        Some(secondary) => format!("Memories of {article} {primary_name} & {secondary:?}"),
        None => format!("Memories of {article} {primary_name}"),
    }
}

/// "a" or "an" for `word`. A plain first-letter-vowel check: every
/// archetype name in this game is a straightforward English noun with no
/// silent-h or long-u trap ("Elementalist", "Archer"-shaped names take
/// "an"; nothing here is "a University"), so the simple rule is right
/// for all 12 and doesn't pretend to be a general-purpose one.
fn indefinite_article(word: &str) -> &'static str {
    match word.chars().next() {
        Some(c) if matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

/// Minimal content blocklist for player-authored Memory names.
///
/// **Judgment call, and the first content filter in this codebase.**
/// Nothing existed to reuse (verified across both crates), so this is
/// new ground rather than a call into an established helper. Two
/// deliberate choices, both erring toward over-rejection:
///
/// 1. Matched against a NORMALIZED form (lowercased, every
///    non-alphanumeric character stripped - see `normalize_for_blocklist`),
///    so separator evasion ("n-i-g...", "f u c k") trips the same entry
///    as the plain spelling.
/// 2. Matched as a SUBSTRING, not on word boundaries. That knowingly
///    rejects innocent names containing an entry as a substring (the
///    classic Scunthorpe problem). For a string the bot may echo into
///    Twitch chat, a false rejection costs a player one retry while a
///    false acceptance is a ToS violation on the streamer's channel -
///    so the asymmetry is intentional.
///
/// Kept short and in one place on purpose: this is a floor, not a
/// comprehensive moderation system, and it is meant to be extended here
/// as needed. Twitch's own moderation remains the outer layer.
const MEMORY_NAME_BLOCKLIST: &[&str] = &[
    "nigger", "nigga", "faggot", "retard", "tranny", "kike", "spic", "chink", "wetback", "coon", "beaner", "cunt", "rape", "raping", "rapist", "pedo",
    "pedophile", "childporn", "cp0rn", "hitler", "nazi", "heilhitler", "kkk", "whitepower", "gasthejews", "killyourself", "kysfag",
];

/// Lowercase `s` and drop everything that isn't a letter or digit - the
/// form `MEMORY_NAME_BLOCKLIST` is matched against. Also folds the
/// common digit-for-letter swaps, so "n1gger"/"f4ggot" normalize onto
/// their plain spellings instead of sliding past.
fn normalize_for_blocklist(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c.to_ascii_lowercase() {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            '@' => 'a',
            '$' => 's',
            other => other,
        })
        .collect()
}

/// Characters no Memory name may contain, beyond `char::is_control`.
///
/// The zero-width and bidirectional-override ranges are "printable" as
/// far as Unicode is concerned but exist precisely to make displayed
/// text differ from its underlying bytes - they can hide a blocklisted
/// word from this filter while still rendering it, reverse a name's
/// apparent direction, or make two different names look identical in the
/// slot list. None of that has a legitimate use in a build name, so the
/// whole span is refused rather than stripped (stripping would silently
/// change what the player typed).
fn is_forbidden_char(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'   // zero-width space/joiners, LRM/RLM
            | '\u{202A}'..='\u{202E}' // bidi embedding/override
            | '\u{2066}'..='\u{2069}' // bidi isolates
            | '\u{FEFF}'              // zero-width no-break space (BOM)
        )
}

/// The one entry point for turning player-typed text into a storable
/// Memory name. Trims, then enforces (in order) non-empty, length,
/// printability, and the blocklist. Returns the TRIMMED name to store -
/// callers must use the returned value, never the input.
pub fn validate_memory_name(raw: &str) -> Result<String, NameRejection> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NameRejection::Empty);
    }
    // Counted in `chars`, not bytes - `MEMORY_NAME_MAX_LEN` is a
    // player-facing "150 characters", and a name of 150 emoji is 150
    // characters to whoever typed it.
    if trimmed.chars().count() > MEMORY_NAME_MAX_LEN {
        return Err(NameRejection::TooLong);
    }
    if trimmed.chars().any(is_forbidden_char) {
        return Err(NameRejection::Unprintable);
    }
    let normalized = normalize_for_blocklist(trimmed);
    if MEMORY_NAME_BLOCKLIST.iter().any(|bad| normalized.contains(bad)) {
        return Err(NameRejection::Blocked);
    }
    Ok(trimmed.to_string())
}

// ---------------------------------------------------------------------
// Snapshot replay
// ---------------------------------------------------------------------

/// Rebuilds one tree's stored allocations by replaying them through the
/// live validator, dropping anything that can't legally be reached.
///
/// Replayed in TIER order (Skills, then Specializations, then
/// Modifiers) - the order a player would have had to click them in.
/// That ordering is what makes staleness cascade correctly for free: a
/// Skill that no longer exists is dropped first, so its Specializations
/// then fail their own parent gate, and their Modifiers fail in turn.
/// It also handles Monk's three Skill-parented Modifiers (see
/// `passive_tree.rs`'s own note on that irregularity) - whatever tier a
/// node's parent is, the parent is placed before the child either way.
///
/// Deliberately STRICTER than the live tree in exactly one respect: the
/// live UI lets a player de-allocate a parent while its children keep
/// their ranks (validation is node-local; de-allocation doesn't cascade),
/// so a saved snapshot can contain orphans. Replaying drops them and
/// refunds the points rather than reproducing them - see the decisions
/// log. Stricter is safe here; looser would not be.
///
/// Never fails. Anything unapplicable comes back in the drop list with
/// its full rank, for the caller to refund and report.
fn replay_snapshot(archetype: Archetype, snapshot: &HashMap<String, u32>, secondary: bool) -> (HashMap<String, u32>, Vec<DroppedAllocation>) {
    let nodes = archetype.passive_nodes();
    let mut applied: HashMap<String, u32> = HashMap::new();
    let mut dropped: Vec<DroppedAllocation> = Vec::new();

    // Node keys are globally unique across every archetype (enforced by
    // passive_tree.rs's own `every_node_key_is_globally_unique_across_
    // every_archetype`), so a snapshot entry naming a key that isn't in
    // THIS archetype's list genuinely doesn't exist for this build -
    // there's no risk of it being a same-named node from elsewhere.
    let mut unknown: Vec<(&String, u32)> = snapshot.iter().filter(|(key, _)| !nodes.iter().any(|n| n.key == key.as_str())).map(|(k, &r)| (k, r)).collect();
    // `HashMap` iteration order is randomized per process, so sort
    // before reporting - a load's drop list should read the same way
    // twice for the same input, both for the player and for tests.
    unknown.sort_by(|a, b| a.0.cmp(b.0));
    for (key, rank) in unknown {
        dropped.push(DroppedAllocation { node_key: key.clone(), rank, secondary, reason: DropReason::NodeGone });
    }

    for tier in [PassiveTier::Skill, PassiveTier::Specialization, PassiveTier::Modifier] {
        // Same determinism reasoning as the unknown-key sort above.
        let mut in_tier: Vec<&crate::passive_tree::PassiveNode> = nodes.iter().filter(|n| n.tier == tier).collect();
        in_tier.sort_by_key(|n| n.key);
        for node in in_tier {
            let Some(&rank) = snapshot.get(node.key) else { continue };
            if rank == 0 {
                continue;
            }
            match validate_allocation_step(nodes, &applied, node.key, rank) {
                Ok(()) => {
                    applied.insert(node.key.to_string(), rank);
                }
                Err(err) => {
                    let reason = match err {
                        PassiveError::MaxRankReached => DropReason::RankCapShrank,
                        _ => DropReason::ParentNotInvested,
                    };
                    dropped.push(DroppedAllocation { node_key: node.key.to_string(), rank, secondary, reason });
                }
            }
        }
    }

    (applied, dropped)
}

/// Drops allocations until both trees together fit inside `budget`.
///
/// Reachable whenever a snapshot legitimately spent more than the
/// character can currently afford - most realistically by being saved
/// with a high-tier Split Personality equipped (which grants
/// `1 + tier / 300` extra points, see `Character::total_passive_points`)
/// and loaded without it. The level-drift rule only covers the surplus
/// direction; this is the deficit one.
///
/// Trims in REVERSE replay order (Modifiers first, then Specializations,
/// then Skills, secondary tree before primary) so the deepest, most
/// specialized investment goes first and the build's foundation is what
/// survives - and so removing a node can never orphan one that's still
/// applied. The load always succeeds; it never writes an over-budget
/// tree and never fails outright, per the "fail gracefully" rule.
fn trim_to_budget(
    primary_archetype: Archetype,
    primary: &mut HashMap<String, u32>,
    secondary_archetype: Option<Archetype>,
    secondary: &mut HashMap<String, u32>,
    budget: u32,
    dropped: &mut Vec<DroppedAllocation>,
) {
    let spent = |p: &HashMap<String, u32>, s: &HashMap<String, u32>| -> u32 { p.values().sum::<u32>() + s.values().sum::<u32>() };

    for tier in [PassiveTier::Modifier, PassiveTier::Specialization, PassiveTier::Skill] {
        for is_secondary in [true, false] {
            let Some(archetype) = (if is_secondary { secondary_archetype } else { Some(primary_archetype) }) else { continue };
            let mut keys: Vec<&'static str> = archetype.passive_nodes().iter().filter(|n| n.tier == tier).map(|n| n.key).collect();
            keys.sort();
            for key in keys {
                if spent(primary, secondary) <= budget {
                    return;
                }
                let side = if is_secondary { &mut *secondary } else { &mut *primary };
                if let Some(rank) = side.remove(key) {
                    dropped.push(DroppedAllocation { node_key: key.to_string(), rank, secondary: is_secondary, reason: DropReason::OverBudget });
                }
            }
        }
    }
}

/// The whole of a load's tree logic, as one pure function over plain
/// data - the manager wrapper (`AdventureManager::load_memory`) does the
/// locking, persisting and broadcasting around it, and nothing else.
///
/// `active_secondary` is the secondary archetype that would be live
/// AFTER this load, as the caller resolves it (Split Personality still
/// equipped, and the stored choice not equal to the loaded primary) -
/// resolved by the caller rather than in here because it depends on the
/// character's live equipment, which this module deliberately can't see.
/// `None` means the snapshot's secondary tree is skipped wholesale, the
/// same rule `AdventureManager::save_passive_tree` already applies to a
/// preview saved after the item came off.
///
/// `budget` is `Character::total_passive_points()` evaluated for the
/// post-load state.
pub fn apply_memory(memory: &Memory, active_secondary: Option<Archetype>, previous_archetype: Archetype, budget: u32) -> (AppliedBuild, MemoryLoadReport) {
    let (mut primary, mut dropped) = replay_snapshot(memory.archetype, &memory.passive_allocations, false);

    let mut secondary_map: HashMap<String, u32> = HashMap::new();
    let secondary_skipped = memory.secondary_archetype.is_some() && active_secondary.is_none();
    if let Some(secondary_archetype) = active_secondary {
        let (applied, mut secondary_dropped) = replay_snapshot(secondary_archetype, &memory.secondary_passive_allocations, true);
        secondary_map = applied;
        dropped.append(&mut secondary_dropped);
    }

    trim_to_budget(memory.archetype, &mut primary, active_secondary, &mut secondary_map, budget, &mut dropped);

    let spent: u32 = primary.values().sum::<u32>() + secondary_map.values().sum::<u32>();
    let report = MemoryLoadReport {
        name: memory.name.clone(),
        archetype: memory.archetype,
        class_changed: memory.archetype != previous_archetype,
        dropped,
        secondary_skipped,
        // Saturating because a caller could in principle hand a budget
        // smaller than the trim could reach (every tree bottoms out at
        // zero nodes, so it can't in practice) - reporting 0 unspent
        // beats panicking on an underflow.
        unspent: budget.saturating_sub(spent),
    };

    let build = AppliedBuild {
        archetype: memory.archetype,
        passive_allocations: primary,
        // Only claim a secondary archetype when one is actually live -
        // writing the stored choice back while Split Personality is
        // unequipped would leave a stale raw field that
        // `effective_secondary_archetype` has to defuse later.
        secondary_archetype: active_secondary,
        secondary_passive_allocations: secondary_map,
        golem_slot_types: memory.golem_slot_types.clone(),
    };
    (build, report)
}

/// The validated result of `apply_memory`, ready to be written straight
/// onto a `Character`. A separate struct rather than mutating a
/// `&mut Character` in here so the whole computation stays pure and
/// testable without constructing one.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedBuild {
    pub archetype: Archetype,
    pub passive_allocations: HashMap<String, u32>,
    pub secondary_archetype: Option<Archetype>,
    pub secondary_passive_allocations: HashMap<String, u32>,
    pub golem_slot_types: Vec<GolemType>,
}

/// Stage A of the Memories build (docs/memories_spec.md) - the naming
/// policy table and the snapshot-replay rules, exercised as pure
/// functions with no manager, no character file and no server, same
/// Phase 1 precedent every other domain module here follows.
#[cfg(test)]
mod memory_tests {
    use super::*;

    // ---- naming --------------------------------------------------

    #[test]
    fn default_name_uses_the_primary_class_alone_with_no_secondary() {
        assert_eq!(default_memory_name(Archetype::Warrior, None), "Memories of a Warrior");
    }

    #[test]
    fn default_name_joins_both_classes_with_an_ampersand_for_split_personality() {
        assert_eq!(default_memory_name(Archetype::Warrior, Some(Archetype::Druid)), "Memories of a Warrior & Druid");
    }

    #[test]
    fn default_name_picks_an_for_a_vowel_initial_class() {
        // The spec wrote the pattern literally as "Memories of a
        // <Class>"; four archetypes start with a vowel and would read as
        // a typo. See `default_memory_name`'s own doc for the call.
        assert_eq!(default_memory_name(Archetype::Elementalist, None), "Memories of an Elementalist");
        assert_eq!(default_memory_name(Archetype::Elementalist, Some(Archetype::Monk)), "Memories of an Elementalist & Monk");
    }

    #[test]
    fn every_archetype_produces_a_grammatical_default_name() {
        // Guards the article rule against a future archetype being added
        // with a vowel-initial name and quietly reading wrong.
        for &archetype in crate::adventure::ALL_ARCHETYPES.iter() {
            let name = default_memory_name(archetype, None);
            let initial = format!("{archetype:?}").chars().next().unwrap().to_ascii_lowercase();
            let expected = if matches!(initial, 'a' | 'e' | 'i' | 'o' | 'u') { "an" } else { "a" };
            assert!(name.starts_with(&format!("Memories of {expected} ")), "{archetype:?} produced {name:?}, which uses the wrong indefinite article");
        }
    }

    #[test]
    fn a_valid_name_comes_back_trimmed() {
        assert_eq!(validate_memory_name("  My Tank Build  "), Ok("My Tank Build".to_string()));
    }

    #[test]
    fn an_empty_or_whitespace_only_name_is_rejected() {
        assert_eq!(validate_memory_name(""), Err(NameRejection::Empty));
        assert_eq!(validate_memory_name("   \t  "), Err(NameRejection::Empty));
    }

    #[test]
    fn name_length_is_capped_at_150_characters_counted_after_trimming() {
        let at_cap = "x".repeat(MEMORY_NAME_MAX_LEN);
        assert_eq!(validate_memory_name(&at_cap), Ok(at_cap.clone()), "exactly 150 characters must be accepted");

        let over_cap = "x".repeat(MEMORY_NAME_MAX_LEN + 1);
        assert_eq!(validate_memory_name(&over_cap), Err(NameRejection::TooLong), "151 characters must be rejected");

        // Trimming happens BEFORE the length check, so padding a
        // legal-length name with whitespace must not push it over.
        let padded = format!("   {at_cap}   ");
        assert_eq!(validate_memory_name(&padded), Ok(at_cap), "surrounding whitespace must not count toward the cap");
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        // 150 multi-byte characters is 150 characters to whoever typed
        // it, even though it is far more than 150 bytes.
        let emoji = "\u{1F600}".repeat(MEMORY_NAME_MAX_LEN);
        assert!(emoji.len() > MEMORY_NAME_MAX_LEN, "sanity: this string must be longer in bytes than in chars");
        assert!(validate_memory_name(&emoji).is_ok(), "150 multi-byte characters must be accepted - the cap is in chars");
    }

    #[test]
    fn control_characters_are_rejected() {
        assert_eq!(validate_memory_name("line\nbreak"), Err(NameRejection::Unprintable));
        assert_eq!(validate_memory_name("tab\there"), Err(NameRejection::Unprintable));
        assert_eq!(validate_memory_name("null\u{0}byte"), Err(NameRejection::Unprintable));
    }

    #[test]
    fn zero_width_and_bidi_override_characters_are_rejected() {
        // These render as nothing (or reverse what follows) while still
        // being present - exactly what a name would use to smuggle a
        // blocklisted word past the filter, or to make two slots look
        // identical. See `is_forbidden_char`'s own doc.
        for sneaky in ['\u{200B}', '\u{200D}', '\u{200E}', '\u{202E}', '\u{2066}', '\u{FEFF}'] {
            let name = format!("Tank{sneaky}Build");
            assert_eq!(validate_memory_name(&name), Err(NameRejection::Unprintable), "U+{:04X} must be rejected", sneaky as u32);
        }
    }

    #[test]
    fn ordinary_punctuation_and_accents_are_still_allowed() {
        // The printability rule must not turn into "ASCII letters only" -
        // these are all legitimate build names.
        for ok in ["Tank Build (v2)", "Crit/Block hybrid", "Bl\u{f6}ck & Tank", "\u{5c11}\u{6797}\u{5bfa}", "DPS \u{1F525}", "it\u{27}s fine"] {
            assert!(validate_memory_name(ok).is_ok(), "{ok:?} should be a legal name");
        }
    }

    #[test]
    fn blocklisted_words_are_rejected() {
        assert_eq!(validate_memory_name("retard"), Err(NameRejection::Blocked));
        assert_eq!(validate_memory_name("My Nazi Build"), Err(NameRejection::Blocked));
    }

    #[test]
    fn blocklist_matching_survives_separator_and_digit_evasion() {
        // The whole reason matching runs against a normalized form - see
        // `normalize_for_blocklist`.
        for evasion in ["r-e-t-a-r-d", "r e t a r d", "R.E.T.A.R.D", "r3tard", "n4zi", "N  A  Z  I"] {
            assert_eq!(validate_memory_name(evasion), Err(NameRejection::Blocked), "{evasion:?} must not slip past the blocklist");
        }
    }

    #[test]
    fn blocklist_matches_as_a_substring_and_knowingly_over_rejects() {
        // Documenting the accepted trade-off rather than pretending it
        // is harmless: substring matching means an innocent name
        // containing an entry is refused too (the Scunthorpe problem).
        // For a string the bot may echo into Twitch chat, a false
        // rejection costs one retry and a false acceptance is a ToS
        // violation - so this is the intended direction, and this test
        // exists so nobody "fixes" it without reading the reasoning.
        assert_eq!(validate_memory_name("Unretarded Damage"), Err(NameRejection::Blocked));
    }

    #[test]
    fn the_default_name_for_every_archetype_passes_its_own_validator() {
        // A default name must never be rejected by the filter guarding
        // custom ones - otherwise "Save Current Build" with no name
        // typed could fail outright for some class.
        for &primary in crate::adventure::ALL_ARCHETYPES.iter() {
            let solo = default_memory_name(primary, None);
            assert!(validate_memory_name(&solo).is_ok(), "{solo:?} must be a legal name");
            for &secondary in crate::adventure::ALL_ARCHETYPES.iter() {
                let paired = default_memory_name(primary, Some(secondary));
                assert!(validate_memory_name(&paired).is_ok(), "{paired:?} must be a legal name");
            }
        }
    }

    // ---- snapshot replay ------------------------------------------

    fn snapshot(entries: &[(&str, u32)]) -> HashMap<String, u32> {
        entries.iter().map(|(k, r)| (k.to_string(), *r)).collect()
    }

    fn memory_of(archetype: Archetype, primary: &[(&str, u32)]) -> Memory {
        Memory {
            name: default_memory_name(archetype, None),
            archetype,
            passive_allocations: snapshot(primary),
            secondary_archetype: None,
            secondary_passive_allocations: HashMap::new(),
            golem_slot_types: Vec::new(),
            saved_at: 0,
        }
    }

    #[test]
    fn a_legal_build_replays_exactly_with_nothing_dropped() {
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 4), ("fortress", 2)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert_eq!(build.passive_allocations, snapshot(&[("bulwark", 3), ("unbreakable", 4), ("fortress", 2)]));
        assert!(report.dropped.is_empty(), "a fully legal build must drop nothing, got {:?}", report.dropped);
        assert!(!report.class_changed);
        assert!(!report.is_noteworthy(), "a clean same-class load needs no message");
        assert_eq!(report.unspent, 20 - 9);
    }

    #[test]
    fn an_unknown_node_key_is_dropped_and_its_points_refunded() {
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("a_node_that_no_longer_exists", 2)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert_eq!(build.passive_allocations, snapshot(&[("bulwark", 3)]), "the surviving allocation must still apply");
        assert_eq!(report.dropped.len(), 1);
        assert_eq!(report.dropped[0].node_key, "a_node_that_no_longer_exists");
        assert_eq!(report.dropped[0].rank, 2);
        assert_eq!(report.dropped[0].reason, DropReason::NodeGone);
        assert_eq!(report.unspent, 20 - 3, "the 2 dropped points must come back as unspent, not vanish");
    }

    #[test]
    fn a_rank_above_the_current_cap_is_dropped_whole_rather_than_clamped() {
        // Deliberately not clamped down to the new cap: a partially
        // applied rank is a build the player never chose. See
        // `DropReason::RankCapShrank`.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 9)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert!(build.passive_allocations.is_empty(), "an over-cap rank must not be applied at a reduced value");
        assert_eq!(report.dropped.len(), 1);
        assert_eq!(report.dropped[0].reason, DropReason::RankCapShrank);
        assert_eq!(report.dropped[0].rank, 9);
    }

    #[test]
    fn a_missing_skill_cascades_through_its_specialization_to_its_modifiers() {
        // Tier-ordered replay is what makes this work with no explicit
        // cascade logic: the Skill is absent, so the Spec fails its own
        // parent gate, and the Modifier fails in turn.
        let memory = memory_of(Archetype::Warrior, &[("unbreakable", 4), ("fortress", 3)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert!(build.passive_allocations.is_empty(), "nothing under an uninvested Skill may survive, got {:?}", build.passive_allocations);
        assert_eq!(report.dropped.len(), 2);
        assert!(report.dropped.iter().all(|d| d.reason == DropReason::ParentNotInvested));
        assert_eq!(report.unspent, 20, "all 7 points come back");
    }

    #[test]
    fn an_orphaned_modifier_is_dropped_and_refunded_not_reproduced() {
        // The live tree lets a player de-allocate a parent while its
        // children keep their ranks (validation is node-local), so a
        // saved snapshot CAN contain orphans. A load deliberately does
        // not reproduce them - strict replay is never looser than the
        // UI, which is the invariant that matters. See the decisions log.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 0), ("fortress", 3)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert_eq!(build.passive_allocations, snapshot(&[("bulwark", 3)]));
        assert_eq!(report.dropped.len(), 1);
        assert_eq!(report.dropped[0].node_key, "fortress");
        assert_eq!(report.dropped[0].reason, DropReason::ParentNotInvested);
    }

    #[test]
    fn a_specialization_at_rank_3_does_not_unlock_its_modifiers() {
        // The 4th point is what unlocks children (`unlock_at: Some(4)`).
        // A snapshot claiming otherwise is not a state the UI could have
        // built, so the Modifier goes.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 3), ("fortress", 1)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert_eq!(build.passive_allocations, snapshot(&[("bulwark", 3), ("unbreakable", 3)]));
        assert_eq!(report.dropped.len(), 1);
        assert_eq!(report.dropped[0].node_key, "fortress");
        assert_eq!(report.dropped[0].reason, DropReason::ParentNotInvested);
    }

    #[test]
    fn loading_a_different_archetype_reports_a_class_change() {
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 2)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Mage, 20);

        assert_eq!(build.archetype, Archetype::Warrior);
        assert!(report.class_changed, "loading a Warrior Memory while playing Mage is a class change");
        assert!(report.is_noteworthy());
    }

    #[test]
    fn points_earned_since_the_snapshot_are_left_unspent_for_the_player() {
        // The level-drift rule: apply the snapshot exactly, never
        // auto-spend the surplus.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 12);

        assert_eq!(build.passive_allocations.values().sum::<u32>(), 3, "the snapshot's own spend must be applied verbatim");
        assert_eq!(report.unspent, 9, "the 9 extra points must be left for the player to place");
        assert!(report.dropped.is_empty());
    }

    #[test]
    fn an_over_budget_snapshot_trims_deepest_first_instead_of_failing() {
        // Reachable by saving with a high-tier Split Personality
        // equipped (which grants extra points) and loading without it.
        // The load must still succeed and must never write an
        // over-budget tree.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 4), ("fortress", 3), ("juggernaut", 3)]);
        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 10);

        let spent: u32 = build.passive_allocations.values().sum();
        assert!(spent <= 10, "the applied build must fit the budget, spent {spent} of 10");
        assert!(report.dropped.iter().any(|d| d.reason == DropReason::OverBudget), "the trim must be reported, got {:?}", report.dropped);
        // Modifiers go before Specializations, which go before Skills.
        assert!(!build.passive_allocations.contains_key("fortress"), "the deepest node must be trimmed first");
        assert!(build.passive_allocations.contains_key("bulwark"), "a foundation Skill must survive the trim");
    }

    #[test]
    fn trimming_never_leaves_an_allocation_orphaned() {
        // Trimming in reverse replay order is what guarantees this:
        // removing a node can never strand one that is still applied.
        let memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 4), ("fortress", 3), ("stonewall", 3), ("juggernaut", 3)]);
        for budget in 0..=16 {
            let (build, _) = apply_memory(&memory, None, Archetype::Warrior, budget);
            let nodes = Archetype::Warrior.passive_nodes();
            for key in build.passive_allocations.keys() {
                let node = nodes.iter().find(|n| n.key == key.as_str()).expect("an applied node must exist in the tree");
                if let Some(parent) = node.parent {
                    let parent_rank = build.passive_allocations.get(parent).copied().unwrap_or(0);
                    assert!(parent_rank >= node.unlock_at.unwrap_or(1), "at budget {budget}, {key} survived with its parent {parent} at rank {parent_rank}");
                }
            }
        }
    }

    #[test]
    fn the_replay_is_deterministic_across_runs() {
        // `HashMap` iteration order is randomized per process, so the
        // drop list is sorted before it is reported - a load must read
        // the same way twice for the same input.
        let memory = memory_of(Archetype::Warrior, &[("ghostnode_b", 1), ("ghostnode_a", 2), ("unbreakable", 4), ("fortress", 1)]);
        let (_, first) = apply_memory(&memory, None, Archetype::Warrior, 20);
        for _ in 0..8 {
            let (_, again) = apply_memory(&memory, None, Archetype::Warrior, 20);
            assert_eq!(first.dropped, again.dropped, "the drop list must be stable across runs");
        }
    }

    // ---- Split Personality ----------------------------------------

    #[test]
    fn a_split_personality_build_replays_both_trees() {
        let mut memory = memory_of(Archetype::Warrior, &[("bulwark", 3)]);
        memory.secondary_archetype = Some(Archetype::Mage);
        memory.secondary_passive_allocations = snapshot(&[("arcane", 2)]);

        let (build, report) = apply_memory(&memory, Some(Archetype::Mage), Archetype::Warrior, 20);

        assert_eq!(build.secondary_archetype, Some(Archetype::Mage));
        assert_eq!(build.secondary_passive_allocations, snapshot(&[("arcane", 2)]));
        assert!(!report.secondary_skipped);
        assert!(report.dropped.is_empty());
        assert_eq!(report.unspent, 20 - 5, "both trees draw from one shared pool");
    }

    #[test]
    fn the_secondary_tree_is_skipped_wholesale_when_split_personality_is_no_longer_equipped() {
        // Same rule `save_passive_tree` already applies to a preview
        // saved after the item came off - a load must not resurrect a
        // secondary tree that should have been refunded.
        let mut memory = memory_of(Archetype::Warrior, &[("bulwark", 3)]);
        memory.secondary_archetype = Some(Archetype::Mage);
        memory.secondary_passive_allocations = snapshot(&[("arcane", 2)]);

        let (build, report) = apply_memory(&memory, None, Archetype::Warrior, 20);

        assert_eq!(build.secondary_archetype, None, "no stale raw secondary may be written back");
        assert!(build.secondary_passive_allocations.is_empty());
        assert!(report.secondary_skipped, "the player must be told their 2nd tree was not applied");
        assert!(report.is_noteworthy());
        assert_eq!(report.unspent, 20 - 3, "the secondary tree's points are not spent");
    }

    #[test]
    fn a_stale_secondary_tree_is_reported_but_the_primary_still_loads() {
        let mut memory = memory_of(Archetype::Warrior, &[("bulwark", 3), ("unbreakable", 4)]);
        memory.secondary_archetype = Some(Archetype::Mage);
        memory.secondary_passive_allocations = snapshot(&[("arcane", 2)]);

        let (build, _) = apply_memory(&memory, None, Archetype::Warrior, 20);
        assert_eq!(build.passive_allocations, snapshot(&[("bulwark", 3), ("unbreakable", 4)]), "a skipped secondary must not disturb the primary");
    }

    #[test]
    fn stale_nodes_in_the_secondary_tree_are_reported_as_secondary() {
        let mut memory = memory_of(Archetype::Warrior, &[("bulwark", 1)]);
        memory.secondary_archetype = Some(Archetype::Mage);
        memory.secondary_passive_allocations = snapshot(&[("a_gone_mage_node", 3)]);

        let (_, report) = apply_memory(&memory, Some(Archetype::Mage), Archetype::Warrior, 20);

        assert_eq!(report.dropped.len(), 1);
        assert!(report.dropped[0].secondary, "the player has two trees on screen - the report must say which one");
    }

    // ---- Elementalist golem slots ---------------------------------

    #[test]
    fn golem_slot_types_are_restored_verbatim() {
        let mut memory = memory_of(Archetype::Elementalist, &[("golemmaster", 3)]);
        memory.golem_slot_types = vec![GolemType::Thunder, GolemType::Flame, GolemType::Water];

        let (build, report) = apply_memory(&memory, None, Archetype::Elementalist, 20);

        assert_eq!(build.golem_slot_types, vec![GolemType::Thunder, GolemType::Flame, GolemType::Water]);
        assert!(report.dropped.is_empty());
    }

    #[test]
    fn golem_slot_types_ride_along_on_a_non_elementalist_build_rather_than_being_cleared() {
        // Same non-lossy reasoning `golem_slot_types`' own doc gives for
        // never trimming on respec - inert here, but a later load back
        // into an Elementalist Memory restores the choices for free.
        let mut memory = memory_of(Archetype::Warrior, &[("bulwark", 1)]);
        memory.golem_slot_types = vec![GolemType::Thunder];

        let (build, _) = apply_memory(&memory, None, Archetype::Warrior, 20);
        assert_eq!(build.golem_slot_types, vec![GolemType::Thunder]);
    }
}
