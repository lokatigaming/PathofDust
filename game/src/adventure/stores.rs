// The store classification (2026-09-08).
//
// WHY THIS FILE EXISTS. A season reset had no representation in code at
// all: no reset function, no wipe, nothing that decided what a season
// destroys. It was `rm` by hand against a runbook. What survived a reset
// survived because nobody typed its filename - `adventure-accounts.json`
// is not named in the cutover steps, so accounts persist by OMISSION.
// That is convention, and the runbook already has a documented instance
// of a step being skipped in practice (`OPERATOR_BOOTSTRAP` was left set
// on live production through the World 2 reset).
//
// This table is the declaration that replaces the omission. Every
// persisted entry in the data directory is named here with a scope, so
// "does this survive a season?" is answered by a value in the codebase
// rather than by what an operator remembers at 1am.
//
// THE CHECK IS ONE-DIRECTIONAL, AND THAT IS DELIBERATE (ruling
// 2026-09-08). Only `present => declared` holds against a live data
// directory. The reverse - "every declared store exists" - is FALSE
// here and asserting it would break the guard the first time anyone
// used it:
//
//   Stores are created LAZILY, on first write. 31 tolerant loads
//   (`load_json` -> `Option`, `.unwrap_or(..)`) against 14 fail-loud,
//   and every marker is among the tolerant ones - a marker file does
//   not exist until its migration fires. On a freshly reset world
//   almost NONE of the world-scoped entries below exist yet: no
//   markers, no fight directories, no reforge cooldowns, no rampage
//   state. A check demanding presence would refuse to run on exactly
//   the world it was built to protect, and its failure would look
//   identical to real corruption.
//
// The contrast worth keeping, because it is why the sprite manifest's
// shape did NOT transfer: sprites are static files in a CHECKOUT, so
// both directions are knowable at test time. Data stores are runtime
// state created on demand. Generalising the mechanism past the property
// that made it work is the mistake this comment exists to prevent
// someone repeating.
//
// The bidirectional shape still lives in the COMPILE-TIME half, where
// it is true and free - see this module's tests: every entry carries a
// scope and a reason, no name is declared twice, and every bucket is
// populated. No directory is involved in any of that.

use std::collections::BTreeSet;
use std::path::Path;

use StoreKind::{Dir, File};
use StoreScope::{Account, Config, NotAStore, World};

/// What a season reset does to a store.
///
/// FOUR buckets, not two (ruling 2026-09-08). The two-bucket framing the
/// work started from - world or account - could not express the live
/// directory: forcing `adventure-live-tunables.toml` into "account"
/// would be false, and forcing `wiki/` into either invites a reset
/// deleting content the owner edits by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoreScope {
    /// Destroyed by a season reset. This world's characters, its
    /// history, and the one-time markers recording what was applied to
    /// them.
    World,
    /// Survives a season reset. Player identity - who you are, as
    /// opposed to what you did last season.
    Account,
    /// Survives a season reset. Operator configuration and published
    /// output, not player state: tunables, balance, patch notes.
    Config,
    /// Not persisted state at all - shipped assets, live content and
    /// logs that happen to sit in the same directory.
    ///
    /// A real bucket rather than an omission: naming them explicitly is
    /// what stops a later reader classifying `wiki/` by accident, and
    /// what stops the reset guard refusing on entries that were never
    /// its business.
    NotAStore,
}

/// File or directory. The reset command needs to know which, because
/// removing a directory is a different call and a much larger mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    File,
    Dir,
}

/// One entry in the data directory.
#[derive(Debug, Clone, Copy)]
pub struct Store {
    /// The entry's name inside the data directory, exactly as it appears
    /// on disk.
    pub name: &'static str,
    pub scope: StoreScope,
    pub kind: StoreKind,
    /// Why it is classified this way, in one line.
    ///
    /// Printed by the reset command before it deletes anything - an
    /// operator must be able to see the classification it is acting on,
    /// not just the outcome afterwards.
    pub why: &'static str,
}

/// Every entry the data directory may hold.
///
/// May legitimately declare MORE than exists - see the one-directional
/// note at the top of this file. `adventure-last-fights.json` and
/// `adventure-rampage-state.json` are both absent from live production
/// today and both belong here anyway.
pub const STORES: &[Store] = &[
    // ---- World: this season's state -------------------------------
    Store { name: "adventure-characters.json", scope: World, kind: File, why: "the roster - every character in this world" },
    Store { name: "adventure-world.json", scope: World, kind: File, why: "world state: stage, boss progress, the season's own position" },
    Store { name: "adventure-sessions.json", scope: World, kind: File, why: "login sessions - invalidated at a reset by the documented cutover, so everyone logs in again against surviving accounts" },
    Store { name: "adventure-reforge-cooldown.json", scope: World, kind: File, why: "per-character crafting cooldowns - meaningless once the characters are gone" },
    Store { name: "adventure-sprite-count.json", scope: World, kind: File, why: "how many sprites existed last boot, so a grown roster grants a free model change to this world's characters" },
    // Declared and absent, both deliberately (ruling 2026-09-08). These
    // were nearly deleted as "dead consts" on the strength of their
    // files being missing. Neither is dead: absence is not evidence.
    Store { name: "adventure-last-fights.json", scope: World, kind: File, why: "RETIRED input to a completed marker-gated migration that split the old single-blob log into the four tier directories - absent because that migration consumed it, and still the only path that can upgrade a restored old-format backup" },
    // Found by measuring the `data_path` call sites rather than by
    // reading the directory: `fight_storage.rs` renames the old log to
    // this on a successful split, so ANY deployment where that migration
    // fires ends up holding a file the table did not declare - which
    // would have refused the next reset. It is absent from live today
    // only because the rename happened before, or did not survive, the
    // World 2 cutover. Exactly the class of gap the guard exists to
    // catch, caught here by the same "what would have to be true" check
    // rather than by the guard firing in production.
    Store { name: "adventure-last-fights.json.bak", scope: World, kind: File, why: "the pre-split fight log, kept as a backup by the storage migration - last season's history in archived form, so it goes with the season" },
    Store { name: "adventure-rampage-state.json", scope: World, kind: File, why: "rampage countdown, mirrored so a restart resumes it - absent because nothing has written it since `start_rampage` lost its last caller in the bot decoupling, not because nothing would" },
    // Fight history: four tiers, each a counter file plus its directory.
    Store { name: "adventure-fights-bundle-seq.json", scope: World, kind: File, why: "next sequence number for the bundle fight tier" },
    Store { name: "adventure-fights-coarse-seq.json", scope: World, kind: File, why: "next sequence number for the coarse fight tier" },
    Store { name: "adventure-fights-detail-seq.json", scope: World, kind: File, why: "next sequence number for the detail fight tier" },
    Store { name: "adventure-fights-summary-seq.json", scope: World, kind: File, why: "next sequence number for the summary fight tier" },
    Store { name: "adventure-fights-bundle", scope: World, kind: Dir, why: "this world's fight history, bundle tier" },
    Store { name: "adventure-fights-coarse", scope: World, kind: Dir, why: "this world's fight history, coarse tier" },
    Store { name: "adventure-fights-detail", scope: World, kind: Dir, why: "this world's fight history, detail tier" },
    Store { name: "adventure-fights-summary", scope: World, kind: Dir, why: "this world's fight history, summary tier" },
    // Markers. World-scoped, and it MUST be that way: a marker records
    // "this one-time change was applied to THIS world's characters". On
    // a fresh world there are no characters that missed it, so
    // re-firing is correct. Classify them Account-scoped and a fresh
    // world's characters would never receive the starter-kit and
    // craft-token grants every previous world's characters got - a
    // regression, not a tidiness question.
    //
    // CONSEQUENCE, and it is load-bearing: because markers are
    // world-scoped, a marker can never record a fact about an
    // account-scoped store. Any migration that writes to an
    // account-scoped store must be idempotent BY INSPECTION - the
    // destination field's own presence is the record - because its
    // marker will be deleted by the next reset and the migration will
    // run again.
    Store { name: "adventure-affix-tier-curve-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-celestial-shard-first-award-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-celestial-shard-into-unique-shard-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-craft-token-backfill-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-craft-token-backfill-v2-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-crit-flag-to-affix-tracking-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-crit-lineage-backfill-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-crit-reforge-equipped-backfill-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-crit-value-nerf-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-duplicate-unique-effects-cleanup-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-fights-storage-migration-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-flowlikewater-swap-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-gloves-speed-rebalance-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-helm-rebalance-v2-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-item-accuracy-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-kibukah-compensation-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-krangle-accuracy-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-lingering-effect-to-echo-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-passive-key-rename-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-pity-launch-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-power-roll-backfill-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-refund-retired-dead-nodes-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-starter-kit-backfill-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-unique-shard-first-award-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-wings-giveaway-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    Store { name: "adventure-wings-launch-grant-marker.json", scope: World, kind: File, why: "one-time migration marker - records that a backfill already ran against THIS world's characters" },
    // ---- Account: identity, survives the season -------------------
    Store { name: "adventure-accounts.json", scope: Account, kind: File, why: "logins and password hashes - who you are, as opposed to what you did last season. Survived previous resets only by NOT being named in the runbook; declared here so it survives by decision" },
    // ---- Config: operator settings and published output -----------
    Store { name: "adventure-live-tunables.toml", scope: Config, kind: File, why: "operator-set live tunables - configuration, not player state" },
    Store { name: "adventure-item-balance.toml", scope: Config, kind: File, why: "item balance table - configuration, not player state" },
    Store { name: "adventure-passive-overrides.toml", scope: Config, kind: File, why: "passive node overrides - configuration, not player state" },
    Store { name: "adventure-bugreports.json", scope: Config, kind: File, why: "player-submitted bug reports - about the game rather than about a character, and worth keeping across a season" },
    Store { name: "patch-notes.json", scope: Config, kind: File, why: "published patch notes - the game's own changelog, which outlives any one world" },
    Store { name: "bot-published-constants.json", scope: Config, kind: File, why: "constants published for the bot to read - derived output, regenerated rather than owned by a world" },
    // ---- Not a store: assets, content and logs --------------------
    Store { name: "templates", scope: NotAStore, kind: Dir, why: "shipped HTML templates - deployed assets, not persisted state" },
    Store { name: "wiki", scope: NotAStore, kind: Dir, why: "live wiki content the OWNER edits by hand - a reset must never touch it" },
    Store { name: "public_adventure_overlay", scope: NotAStore, kind: Dir, why: "shipped overlay assets, including the custom sprite drop-in directory" },
    Store { name: "logs", scope: NotAStore, kind: Dir, why: "process logs - operational output, not game state" },
];

/// The classification for one entry name, or `None` if it is not
/// declared.
///
/// `None` is the interesting answer: it is what makes the reset refuse.
pub fn scope_of(name: &str) -> Option<StoreScope> {
    STORES.iter().find(|store| store.name == name).map(|store| store.scope)
}

/// The entries a season reset destroys.
pub fn world_scoped() -> impl Iterator<Item = &'static Store> {
    STORES.iter().filter(|store| store.scope == World)
}

/// Entries present in `dir` that this table does not declare, sorted.
///
/// **The one-directional reality check.** Anything here means the reset
/// cannot know whether to destroy it, so the reset must refuse rather
/// than guess. Never reports the reverse - a declared store that is
/// simply absent is the normal state of most of this table.
///
/// A directory that cannot be read yields an error rather than an empty
/// list: "I could not look" must not be mistaken for "there was nothing
/// there", which is precisely how a fail-closed guard turns into a
/// fail-open one.
pub fn undeclared_entries(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut undeclared = BTreeSet::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if scope_of(&name).is_none() {
            undeclared.insert(name);
        }
    }
    Ok(undeclared.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compile-time half of the bidirectional check: the table
    /// agrees with itself. No directory involved, so this holds on a
    /// fresh checkout, in CI, and on a machine that has never run the
    /// game.
    #[test]
    fn every_entry_is_declared_once_and_says_why() {
        assert!(!STORES.is_empty(), "sanity: an empty table would make every other assertion here vacuous");

        let mut seen = BTreeSet::new();
        for store in STORES {
            assert!(!store.name.is_empty(), "a store with no name cannot be matched against anything on disk");
            assert!(
                !store.why.is_empty(),
                "{} has no reason - the reset command PRINTS this before deleting, so a blank one leaves an operator approving a name with no explanation",
                store.name
            );
            assert!(seen.insert(store.name), "{} is declared twice - the first classification would silently win in `scope_of`", store.name);
        }
    }

    /// Every bucket is populated. Written this way on purpose: if a
    /// bucket ever empties, that is a real change in what the game
    /// persists and should be noticed deliberately rather than by a
    /// silently-skipped loop.
    #[test]
    fn all_four_buckets_are_populated() {
        for scope in [World, Account, Config, NotAStore] {
            assert!(STORES.iter().any(|store| store.scope == scope), "{scope:?} has no entries - four buckets exist because the live directory needed four, so an empty one means the table has drifted from reality");
        }
    }

    /// The classifications this work exists to pin, by name, because
    /// each was a live decision rather than a default.
    #[test]
    fn the_decisions_that_were_actually_made_stay_made() {
        assert_eq!(scope_of("adventure-accounts.json"), Some(Account), "accounts must survive a reset BY DECISION - they previously survived only by not appearing in the runbook");
        assert_eq!(scope_of("adventure-characters.json"), Some(World), "the roster is what a season reset is for");
        assert_eq!(scope_of("wiki"), Some(NotAStore), "the owner edits wiki content by hand - a reset touching it would destroy work no backup of the game covers");
        assert_eq!(scope_of("adventure-live-tunables.toml"), Some(Config), "operator configuration is not player state and does not belong to a world");

        // Markers world-scoped: see the block comment above their rows.
        assert_eq!(scope_of("adventure-starter-kit-backfill-marker.json"), Some(World), "a marker records what was applied to THIS world's characters, so a fresh world must re-run the grant");

        // Declared despite being absent from live production.
        assert_eq!(scope_of("adventure-last-fights.json"), Some(World), "absent because a completed migration consumed it - declared anyway, because absence is not deadness");
        assert_eq!(scope_of("adventure-rampage-state.json"), Some(World), "absent because nothing has written it - declared anyway");
    }

    /// An undeclared name has no scope. This is the arm the reset guard
    /// stands on, so it is asserted directly rather than only through
    /// the guard.
    #[test]
    fn an_undeclared_name_has_no_scope() {
        assert_eq!(scope_of("adventure-something-nobody-classified.json"), None, "an unknown entry must return None - the reset refuses on exactly this");
        assert_eq!(scope_of(""), None);
    }

    /// `world_scoped` selects the destroy list and nothing else. A bug
    /// here deletes an account store, so it is checked by scope rather
    /// than by count.
    #[test]
    fn only_world_scoped_entries_are_selected_for_deletion() {
        assert!(world_scoped().all(|store| store.scope == World), "the delete list must contain nothing but World-scoped entries");
        let names: Vec<&str> = world_scoped().map(|store| store.name).collect();
        assert!(names.contains(&"adventure-characters.json"), "sanity: the roster must be in the delete list, or this test proves nothing");
        assert!(!names.contains(&"adventure-accounts.json"), "accounts must NEVER be in the delete list");
        assert!(!names.contains(&"wiki"), "wiki content must NEVER be in the delete list");
        assert!(!names.contains(&"adventure-live-tunables.toml"), "operator configuration must never be in the delete list");
    }

    /// `undeclared_entries` reports what is present and unclassified,
    /// and stays silent about what is merely absent. Both halves are
    /// asserted against a real temporary directory, because the whole
    /// point is behaviour against a filesystem.
    #[test]
    fn undeclared_entries_reports_the_present_and_ignores_the_absent() {
        let dir = std::env::temp_dir().join(format!("pod-stores-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir must be creatable");

        // A declared store, and a declared DIRECTORY, both present.
        std::fs::write(dir.join("adventure-characters.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("wiki")).unwrap();
        assert!(
            undeclared_entries(&dir).unwrap().is_empty(),
            "declared entries must not be reported - and the other 40-odd declared stores are ABSENT from this directory, which must also not be reported"
        );

        // Now something nobody classified.
        std::fs::write(dir.join("adventure-mystery.json"), "{}").unwrap();
        assert_eq!(undeclared_entries(&dir).unwrap(), vec!["adventure-mystery.json".to_string()], "an unclassified entry must be reported by name");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Failing to read the directory is an error, not an empty list.
    /// "I could not look" must never be mistaken for "there was nothing
    /// there" - that is the exact step that turns a fail-closed guard
    /// into a fail-open one.
    #[test]
    fn an_unreadable_directory_is_an_error_not_an_empty_answer() {
        let missing = std::env::temp_dir().join("pod-stores-definitely-does-not-exist-9e3f1a");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(undeclared_entries(&missing).is_err(), "a directory that cannot be read must surface as an error - an empty Vec here would read as 'nothing undeclared' and let a reset proceed blind");
    }
}
