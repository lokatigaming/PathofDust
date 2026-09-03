//! The startup starter-kit backfill is bounded and cannot re-arm
//! (2026-09-03).
//!
//! WHAT THIS EXISTS TO CATCH, stated plainly, because the whole failure
//! mode was that nothing failed. `AdventureManager::new`'s starter-kit
//! backfill iterated `EQUIP_SLOTS` and claimed in a comment to be
//! "idempotent - once everyone has all 5 slots filled, this is just a
//! fast no-op scan". That was not a guard, it was an INVARIANT: it rested
//! on the data converging on a state where the `if` never matches. The
//! gear-slots release (spec §8) took `EQUIP_SLOTS` from 5 to 9, the loop
//! silently re-armed, and it granted 72 tier-1 items across 18 live
//! characters and persisted them. No compiler error. No failing test.
//!
//! Worse forward: `Character::new` leaves the four §8 slots empty by
//! owner ruling, so every future character would have had them auto-
//! filled at the next service restart, permanently defeating the ruling.
//!
//! A guard without a test that proves the guard holds is the same class
//! of promise the original comment made, so this file asserts the two
//! properties directly, on disk, through the real constructor:
//!
//!   1. On a first run (marker absent) the backfill fills the five
//!      STARTER-KIT slots and touches NO slot outside them.
//!   2. On every run after that (marker present) it is a total no-op,
//!      even for a character with an empty starter-kit slot.
//!
//! Property 1's "must stay empty" set is derived from `EQUIP_SLOTS` at
//! runtime rather than written out, so the day someone adds a tenth slot
//! it lands in that set automatically and this test fails if the loop
//! reaches it. `ORIGINAL_FIVE` below is deliberately a local copy rather
//! than an import of the production constant - restating the
//! implementation would let both drift together silently, and the point
//! is to verify the BEHAVIOUR against an independently stated
//! expectation.

use game::adventure::{AdventureManager, Character, EquipSlot, EQUIP_SLOTS};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The five slots that existed when the backfill was written, and the
/// only ones it is ever allowed to fill. Stated here independently of
/// `manager.rs`'s own frozen list on purpose - see the module doc.
const ORIGINAL_FIVE: [EquipSlot; 5] = [EquipSlot::Weapon, EquipSlot::Helm, EquipSlot::Body, EquipSlot::Gloves, EquipSlot::Boots];

const CHARACTERS_FILE: &str = "adventure-characters.json";
const MARKER_FILE: &str = "adventure-starter-kit-backfill-marker.json";
const LOGIN: &str = "backfiller";

fn read_characters(path: &Path) -> HashMap<String, Character> {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("failed to parse {} as HashMap<String, Character>: {e}", path.display()))
}

fn write_characters(path: &Path, characters: &HashMap<String, Character>) {
    let raw = serde_json::to_string_pretty(characters).expect("characters must serialize");
    std::fs::write(path, raw).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}

/// Every slot the backfill must never touch: whatever `EQUIP_SLOTS` holds
/// today, minus the five it was written for. Empty is not an acceptable
/// answer here - if this returns nothing the test is asserting nothing,
/// so the caller checks.
fn slots_outside_the_starter_kit() -> Vec<EquipSlot> {
    EQUIP_SLOTS.iter().copied().filter(|slot| !ORIGINAL_FIVE.contains(slot)).collect()
}

fn build_manager() -> std::sync::Arc<AdventureManager> {
    AdventureManager::new(PathBuf::from(CHARACTERS_FILE), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"))
}

#[test]
fn the_starter_kit_backfill_fills_only_the_original_five_and_never_runs_twice() {
    let scratch = std::env::temp_dir().join(format!("starter_kit_backfill_bound_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let characters_path = scratch.join(CHARACTERS_FILE);
    let marker_path = scratch.join(MARKER_FILE);

    let outside = slots_outside_the_starter_kit();
    assert!(
        !outside.is_empty(),
        "EQUIP_SLOTS has no slot outside the original five, so this test would assert nothing. If the extra slots were deliberately removed, delete this test with them; otherwise ORIGINAL_FIVE has been edited to match the implementation, which is exactly what it must not do"
    );

    // A character in the shape the backfill exists for: joined before the
    // starter kit, holding one item and nothing else. `Character::new`
    // already leaves the §8 slots empty, which is the owner ruling this
    // test is protecting, so those need no setup.
    let mut character = Character::new("Backfiller".to_string());
    character.helm = None;
    character.body = None;
    character.gloves = None;
    character.boots = None;
    for slot in &outside {
        assert!(character.equipped(*slot).is_none(), "sanity: Character::new must leave {slot:?} empty - that is the ruling under test, and if it stops being true this test is measuring the wrong thing");
    }
    let mut seeded = HashMap::new();
    seeded.insert(LOGIN.to_string(), character);
    write_characters(&characters_path, &seeded);

    // --- PHASE 1: marker absent, the backfill's one legitimate run ------
    assert!(!marker_path.exists(), "sanity: the scratch dir must start with no marker");
    drop(build_manager());

    let after = read_characters(&characters_path);
    let character = after.get(LOGIN).expect("the seeded character must survive startup");

    // It must still actually DO its job - a guard that works by disabling
    // the migration entirely would pass every other assertion here.
    for slot in ORIGINAL_FIVE {
        assert!(
            character.equipped(slot).is_some(),
            "the backfill must still fill the starter-kit slot {slot:?} on its one legitimate run - if this fails the migration has been disabled rather than bounded"
        );
    }

    // THE ASSERTION THIS FILE IS FOR.
    for slot in &outside {
        assert!(
            character.equipped(*slot).is_none(),
            "the startup backfill filled {slot:?}, which is not a starter-kit slot. It has re-armed over a slot added to EQUIP_SLOTS after it was written - the exact defect that granted 72 items in production on 2026-09-03. The loop must iterate its own frozen five-slot list, never EQUIP_SLOTS"
        );
    }

    assert!(marker_path.exists(), "the backfill must write its marker even when it changed nothing - a marker written only on a change leaves the guard unarmed on exactly the installs where the loop was a no-op");

    // --- PHASE 2: marker present, the backfill must be inert ------------
    // Re-open a starter-kit slot. On a first run this is precisely what
    // the backfill would fill; with the marker set it must not be touched,
    // and neither must anything else.
    let mut reopened = read_characters(&characters_path);
    let character = reopened.get_mut(LOGIN).expect("the seeded character must still be there");
    character.boots = None;
    write_characters(&characters_path, &reopened);

    drop(build_manager());

    let after = read_characters(&characters_path);
    let character = after.get(LOGIN).expect("the seeded character must survive the second startup");
    assert!(
        character.equipped(EquipSlot::Boots).is_none(),
        "the backfill ran a second time. Once its marker is on disk it must never run again, whatever the character data or EQUIP_SLOTS look like - guarding it by STATE rather than by an invariant is the entire point of the marker"
    );
    for slot in &outside {
        assert!(character.equipped(*slot).is_none(), "the second startup filled {slot:?} - the marker guard is not holding");
    }

    let _ = std::fs::remove_dir_all(&scratch);
}
