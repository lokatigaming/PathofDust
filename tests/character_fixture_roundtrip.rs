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
use std::path::PathBuf;
use twitch_bot_rs::adventure::{AdventureManager, Character, CraftAction};

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

/// Unified Unique Shards (2026-08-19) - real regression coverage for
/// `migrate_celestial_shard_into_unique_shard` against real (pseudonymized)
/// production data, exercised through the actual startup path
/// (`AdventureManager::new` -> `run_character_migrations`), not the pure
/// migration function called directly - this is what this whole fixture
/// harness exists for (see this file's own doc: "reused... at every
/// future stage that touches Character/Item's shape or the migration
/// runner"). Confirmed (at the time this test was written) that the real
/// fixture actually contains both `celestialshard` and `uniqueshard`
/// `craft_tokens` entries, so this exercises the real merge shape, not a
/// synthetic one.
#[tokio::test]
async fn celestial_shard_into_unique_shard_migration_merges_real_fixture_data_on_load() {
    let original = load_fixture();
    let expected_unique_totals: HashMap<String, u32> =
        original.iter().map(|(login, c)| (login.clone(), c.craft_token_count(CraftAction::CelestialShard) + c.craft_token_count(CraftAction::UniqueShard))).collect();
    assert!(
        expected_unique_totals.values().any(|&n| n > 0),
        "fixture assumption: at least one real character must hold a CelestialShard and/or UniqueShard token, or this test proves nothing"
    );

    let scratch = std::env::temp_dir().join(format!("celestial_into_unique_migration_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let characters_path = scratch.join("adventure-characters.json");
    std::fs::copy(FIXTURE_PATH, &characters_path).expect("failed to seed the scratch characters file from the pseudonymized fixture");

    // `set_data_dir` must run before `AdventureManager::new` (which reads
    // migration marker files via `data_path`) - this test file's other 4
    // tests never touch `data_path` at all (pure serde only), so this is
    // the only caller in this binary's whole process, same "one process,
    // one OnceLock, one caller" reasoning `http_golden_responses.rs` and
    // `divine_dust_ui_http.rs` both already document.
    assert!(twitch_bot_rs::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let manager = AdventureManager::new(characters_path, PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    for (login, expected_unique) in &expected_unique_totals {
        let character = manager.character(login).await.unwrap_or_else(|| panic!("migration must not lose character {login:?}"));
        assert_eq!(character.craft_token_count(CraftAction::CelestialShard), 0, "{login}: CelestialShard must be fully drained by the merge");
        assert_eq!(
            character.craft_token_count(CraftAction::UniqueShard),
            *expected_unique,
            "{login}: UniqueShard must hold the merged 1:1 total (old CelestialShard + old UniqueShard)"
        );
    }

    std::fs::remove_dir_all(&scratch).ok();
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
