//! Stage 0.5 harness #2 (2026-08-18, architecture refactor) - the real-
//! save-file round-trip fixture. Reused at every future stage that
//! touches `Character`/`Item`'s shape or the migration runner: point
//! this same fixture at the OLD code path and the NEW code path after a
//! refactor stage moves/renames something, and assert the two
//! deserializations produce identical data. Right now (Stage 0.5, before
//! any code has moved) there's only one path to check against, so this
//! establishes the BASELINE the future comparison will need - the
//! fixture round-trips cleanly, with the expected roster size, through
//! the exact type the real production code uses.
//!
//! Fixture: `tests/fixtures/characters_pseudonymized.json`, produced by
//! `cargo run --bin pseudonymize_characters` from the real, gitignored
//! `adventure-characters.json` - every login/display name (and the one
//! other identity-carrying field found so far, a self-uploaded custom
//! sprite's `custom/<login>` model string) replaced by a deterministic
//! placeholder. See that binary's own doc for the full scrubbing/
//! verification story. Real player data never enters git history; only
//! this fixture and the script that produced it are committed.

use std::collections::HashMap;
use twitch_bot_rs::adventure::Character;

const FIXTURE_PATH: &str = "tests/fixtures/characters_pseudonymized.json";

fn load_fixture() -> HashMap<String, Character> {
    let raw = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_else(|e| panic!("failed to read {FIXTURE_PATH}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("failed to parse {FIXTURE_PATH} as HashMap<String, Character>: {e}"))
}

#[test]
fn fixture_deserializes_as_the_real_production_type() {
    let characters = load_fixture();
    assert!(!characters.is_empty(), "the fixture must actually contain characters - an empty file would make every other test in this suite meaningless");
}

#[test]
fn fixture_round_trips_through_serialize_deserialize_unchanged() {
    // The harness future stages will actually use: load under the OLD
    // code path, load under the NEW code path, assert equality. With
    // only one code path today, this instead proves the round trip
    // itself is lossless - serialize what we just loaded, parse it
    // again, and the two in-memory maps must agree on every key and on
    // every character's full JSON representation (avoids requiring
    // `Character: PartialEq`, which it doesn't derive - see
    // `golden_corpus.rs` for the same reasoning applied to the combat
    // wire types).
    let original = load_fixture();
    let json = serde_json::to_string(&original).expect("a freshly loaded fixture must always re-serialize");
    let round_tripped: HashMap<String, Character> = serde_json::from_str(&json).expect("a freshly serialized fixture must always re-parse");

    assert_eq!(original.len(), round_tripped.len(), "round-trip must not gain or lose any character");
    for (login, character) in &original {
        let other = round_tripped.get(login).unwrap_or_else(|| panic!("round-trip lost character {login:?} entirely"));
        let a = serde_json::to_value(character).unwrap();
        let b = serde_json::to_value(other).unwrap();
        assert_eq!(a, b, "round-trip changed character {login:?}'s data");
    }
}

#[test]
fn fixture_roster_size_matches_the_real_file_at_capture_time() {
    // Pinned to the count `pseudonymize_characters` reported at Stage 0
    // execution time (2026-08-18) - a real roster-size DROP here would
    // mean either the fixture was hand-edited or a future refresh lost
    // characters; a real roster-size GROWTH is fine (a refresh against a
    // bigger real file) and just needs this constant bumped to match,
    // same spirit as any other "update the golden value" fixture
    // maintenance.
    let characters = load_fixture();
    assert_eq!(characters.len(), 52, "if this changed because you refreshed the fixture from a bigger real file, update this constant to match - if not, investigate");
}

#[test]
fn no_placeholder_login_or_display_name_looks_like_a_real_username() {
    // Defense in depth, mirroring `pseudonymize_characters`' own final
    // scan (see its doc) - every key/display_name here must follow the
    // `player<N>`/`Player<N>` placeholder shape, never anything that
    // could be a real Twitch login that slipped through un-scrubbed.
    let characters = load_fixture();
    for (login, character) in &characters {
        assert!(login.starts_with("player") && login["player".len()..].chars().all(|c| c.is_ascii_digit()), "login {login:?} doesn't look like a generated placeholder");
        assert!(
            character.display_name.starts_with("Player") && character.display_name["Player".len()..].chars().all(|c| c.is_ascii_digit()),
            "display_name {:?} (for {login:?}) doesn't look like a generated placeholder",
            character.display_name
        );
    }
}
