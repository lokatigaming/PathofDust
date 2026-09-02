//! Memories over real HTTP (2026-08-19) - the Stage C route-level pass,
//! done the way this repo already spins up a disposable game instance
//! (see `http_golden_responses.rs`, whose setup this mirrors) rather
//! than by hand against a running server: an OS-assigned ephemeral port,
//! a scratch data directory, and a seeded fake session, so nothing here
//! can reach the live game's files or ports.
//!
//! What it covers that the in-crate tests can't: that the four new
//! `/passives/memories/*` routes are actually registered, parse their
//! forms, and drive the real `AdventureManager` end to end - a route
//! typo'd into the wrong path, or a form field renamed, fails here and
//! nowhere else.
//!
//! **Single test function, deliberately** - same reason
//! `http_golden_responses.rs` gives: `adventure::set_data_dir` is a
//! process-wide `OnceLock`, so two `#[tokio::test]`s in ONE file would
//! race for which gets to set it. Rust compiles each file under `tests/`
//! into its own process, so this file doesn't race the others.

use std::path::PathBuf;
use game::adventure::{AdventureManager, Archetype};

#[tokio::test]
async fn memories_routes_drive_a_disposable_game_instance_end_to_end() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("memories_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "memtester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"MemTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = game::adventure_web::start_adventure_web_server(
        0, // ephemeral - the OS picks a free port, so this can never collide with the live game
        manager.clone(),
        sessions_path,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    let cookie = "adv_session=test-token";

    // Set up a real character with a real build, through the manager's
    // own public API (not by writing the save file by hand).
    manager.join(TEST_LOGIN, "MemTester").await;
    manager.change_archetype(TEST_LOGIN, Archetype::Warrior).await.expect("class change must succeed");
    manager.preview_allocate_passive(TEST_LOGIN, "bulwark", 1, false).await.expect("a legal allocation must succeed");
    manager.save_passive_tree(TEST_LOGIN).await.expect("saving the tree must succeed");

    // --- the page renders the card, with all three slots empty -------
    let page = client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /passives failed");
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let html = page.text().await.expect("failed to read /passives body");
    assert!(html.contains("ptree-memories"), "the Memories card must render on /passives");
    assert!(html.contains("Memories of a Warrior"), "the default name must show as the empty-slot placeholder");
    // Counted as `class="memory-slot ` so the `memory-slots` CONTAINER
    // isn't counted.
    assert_eq!(html.matches("class=\"memory-slot ").count(), 3, "all three empty slots must render");
    assert_eq!(html.matches("Save Current Build").count(), 3, "all three empty slots must offer a save");

    // --- save -------------------------------------------------------
    let save = client
        .post(format!("{base}/passives/memories/save"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("slot", "0"), ("name", "Tank Build")])
        .send()
        .await
        .expect("POST /passives/memories/save failed");
    assert!(save.status().is_redirection(), "a save must redirect, got {}", save.status());
    let character = manager.character(TEST_LOGIN).await.expect("still joined");
    assert_eq!(character.memory_slot(0).map(|m| m.name.as_str()), Some("Tank Build"), "the save must reach the real character");

    // --- a blocked name is refused, and says so ---------------------
    let blocked = client
        .post(format!("{base}/passives/memories/save"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("slot", "1"), ("name", "retard")])
        .send()
        .await
        .expect("POST with a blocked name failed");
    let location = blocked.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    assert!(location.contains("passive_failed"), "a blocked name must redirect to the error popup, got {location:?}");
    assert!(!location.contains("retard"), "the rejected word must NOT be echoed back in the URL");
    assert!(manager.character(TEST_LOGIN).await.unwrap().memory_slot(1).is_none(), "a rejected save must leave the slot empty");

    // --- rename -----------------------------------------------------
    let rename = client
        .post(format!("{base}/passives/memories/rename"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("slot", "0"), ("name", "Blocking Build")])
        .send()
        .await
        .expect("POST /passives/memories/rename failed");
    assert!(rename.status().is_redirection());
    assert_eq!(manager.character(TEST_LOGIN).await.unwrap().memory_slot(0).unwrap().name, "Blocking Build");

    // --- load, after wandering off to another class -----------------
    manager.change_archetype(TEST_LOGIN, Archetype::Mage).await.expect("class change must succeed");
    let load = client
        .post(format!("{base}/passives/memories/load"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("slot", "0")])
        .send()
        .await
        .expect("POST /passives/memories/load failed");
    assert!(load.status().is_redirection());
    let location = load.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    assert!(location.contains("memory_note"), "a load that changed class must report it, got {location:?}");

    let character = manager.character(TEST_LOGIN).await.expect("still joined");
    assert_eq!(character.archetype, Archetype::Warrior, "the load must really have changed the class back");
    assert_eq!(character.passive_allocations.get("bulwark"), Some(&1), "and restored the tree");

    // The note popup renders on the page it redirects to.
    let noted = client.get(format!("{base}{location}")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET the note URL failed");
    let noted_html = noted.text().await.expect("failed to read body");
    assert!(noted_html.contains("memory-note-modal"), "the note popup must render");
    assert!(noted_html.contains("You&#x27;re now playing Warrior.") || noted_html.contains("You're now playing Warrior."), "the note must say what changed");

    // --- the filled slot now renders with its summary ---------------
    let page = client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /passives failed");
    let html = page.text().await.expect("failed to read /passives body");
    assert!(html.contains("Blocking Build"), "the saved Memory's name must render");
    assert!(html.contains("1 point spent"), "the slot summary must render");
    assert_eq!(html.matches("Save Current Build").count(), 2, "one slot is filled now, so only two offer a fresh save");

    // --- delete -----------------------------------------------------
    let delete = client
        .post(format!("{base}/passives/memories/delete"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("slot", "0")])
        .send()
        .await
        .expect("POST /passives/memories/delete failed");
    assert!(delete.status().is_redirection());
    let character = manager.character(TEST_LOGIN).await.expect("still joined");
    assert!(character.memory_slot(0).is_none(), "the Memory is gone");
    assert_eq!(character.memory_slots, 3, "the slot itself is a grant, not a container - it must survive");

    std::fs::remove_dir_all(&scratch).ok();
}
