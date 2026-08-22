// Stage 0 fixture generator (2026-08-18, architecture refactor) - produces
// tests/fixtures/characters_pseudonymized.json from the real, production
// adventure-characters.json, with every login/display name replaced by a
// deterministic placeholder BEFORE anything touches git. Real logins never
// enter git history; only this script's output does.
//
// Deterministic: sorts the real file's logins alphabetically and assigns
// player001, player002, ... in that order, so re-running this against an
// updated real file (a "fixture refresh") reassigns placeholders in the
// same stable order rather than shuffling them fight-to-fight. Applied to
// BOTH the HashMap key (the login) and Character::display_name (the only
// other player-identifying field on Character - everything else is game
// state: level, gear, dust, passive allocations, etc.) so internal
// references stay coherent - a fixture reader can trust "player007" means
// the same person everywhere in the file.
//
// Run from the repo root: `cargo run --bin pseudonymize_characters`
// Reads: adventure-characters.json (the real file, never committed)
// Writes: tests/fixtures/characters_pseudonymized.json (committed)

use std::collections::HashMap;
use twitch_bot_rs::adventure::Character;

fn main() {
    let input_path = "adventure-characters.json";
    let output_path = "tests/fixtures/characters_pseudonymized.json";

    let raw = std::fs::read_to_string(input_path).unwrap_or_else(|e| panic!("failed to read {input_path}: {e}"));
    let real: HashMap<String, Character> = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("failed to parse {input_path} as HashMap<String, Character>: {e}"));
    let real_count = real.len();

    let mut logins: Vec<String> = real.keys().cloned().collect();
    logins.sort();
    let width = logins.len().max(1).to_string().len().max(3);

    let mut pseudonymized: HashMap<String, Character> = HashMap::with_capacity(real_count);
    for (i, login) in logins.iter().enumerate() {
        let placeholder_login = format!("player{:0width$}", i + 1, width = width);
        let placeholder_display_name = format!("Player{:0width$}", i + 1, width = width);
        let mut character = real.get(login).expect("login came from real.keys()").clone();
        character.display_name = placeholder_display_name;
        // Second identity-carrying field, easy to miss: a self-uploaded
        // custom sprite is stored as `custom/<own login>[optional digit
        // suffix]` (see `is_valid_custom_sprite`'s doc in character.rs -
        // every non-"public"-pool custom name is validated at write-time
        // to be exactly this player's own login). Strip the KNOWN real
        // login prefix (not a guessed pattern - logins can themselves end
        // in digits, e.g. "kmartbikes1", which would confuse a naive
        // "find the first digit" split) and rebuild under the placeholder
        // login, keeping whatever suffix followed it. `custom/public...`
        // and the curated built-in `ALL_SPRITES` names carry no identity
        // and are left untouched.
        if let Some(model) = character.model.as_deref() {
            if let Some(name) = model.strip_prefix("custom/") {
                if name.to_ascii_lowercase().starts_with(&login.to_ascii_lowercase()) {
                    let suffix = &name[login.len()..];
                    character.model = Some(format!("custom/{placeholder_login}{suffix}"));
                }
            }
        }
        pseudonymized.insert(placeholder_login, character);
    }

    let output_json = serde_json::to_string_pretty(&pseudonymized).expect("pseudonymized data must serialize - it's the same Character type the real file already deserialized as");

    // Defense in depth: scrubbing known identity-carrying fields (above)
    // only catches leaks we already know about - `model`'s "custom/<own
    // login>" shape was itself found this way, by accident, the first
    // time this script ran. Before writing anything to disk, scan the
    // WHOLE serialized output for every real login and display name as a
    // blunt but comprehensive final check, so a future field nobody
    // thought to scrub still gets caught here instead of silently
    // shipping into a committed fixture.
    let output_json_lower = output_json.to_ascii_lowercase();
    let mut leaks: Vec<String> = Vec::new();
    for login in &logins {
        if output_json_lower.contains(&login.to_ascii_lowercase()) {
            leaks.push(format!("login {login:?}"));
        }
    }
    for character in real.values() {
        if !character.display_name.is_empty() && output_json.contains(&character.display_name) {
            leaks.push(format!("display_name {:?}", character.display_name));
        }
    }
    if !leaks.is_empty() {
        panic!(
            "pseudonymized output still contains {} real identifier(s), refusing to write: {:?}\n\
             a field carries player identity that this script doesn't scrub yet - find it and fix pseudonymize_characters.rs before re-running",
            leaks.len(),
            leaks
        );
    }

    std::fs::create_dir_all("tests/fixtures").expect("failed to create tests/fixtures");
    std::fs::write(output_path, &output_json).unwrap_or_else(|e| panic!("failed to write {output_path}: {e}"));

    // Verify: round-trip the OUTPUT back through the exact same
    // deserialization path a test fixture consumer would use, and confirm
    // nothing was lost or corrupted in translation.
    let round_tripped: HashMap<String, Character> = serde_json::from_str(&output_json).expect("the file we just wrote must deserialize as HashMap<String, Character> - otherwise the fixture is useless");
    assert_eq!(round_tripped.len(), real_count, "pseudonymized roster size must match the real file's roster size exactly");
    assert_eq!(round_tripped.len(), pseudonymized.len(), "round-trip must not lose or duplicate any entry");

    println!("Pseudonymized {real_count} character(s) from {input_path} -> {output_path}");
    println!("Verified: round-trip deserialization succeeded, roster size preserved ({real_count} entries).");
}
