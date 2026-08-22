//! `/admin/passives` over real HTTP (2026-08-19) - Stage 1 of the
//! live-tunable passive values build. Same disposable-instance setup as
//! `http_golden_responses.rs` and `memories_http.rs`: an OS-assigned
//! ephemeral port, a scratch data directory, and seeded fake sessions,
//! so nothing here can reach the live game's files or ports.
//!
//! Covers the three things no in-crate test can:
//!
//! 1. **The admin gate actually holds** on the read and both writes.
//!    This page can change every player's combat numbers, so "a
//!    non-admin gets nothing" has to be proven over real HTTP against
//!    the real session layer, not asserted from reading the handler.
//! 2. **A save round-trips through the real store to the real file**,
//!    and a revert removes it.
//! 3. **An override actually reaches the game**, observed through
//!    `Character::passive_node_magnitude` - the accessor combat itself
//!    uses - rather than only through the admin page that wrote it.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir`
//! is a process-wide `OnceLock`, and the override store is a
//! process-wide `RwLock`. Rust gives each `tests/*.rs` file its own
//! process, so this file is isolated from the rest of the suite, but
//! two `#[tokio::test]`s inside THIS file would share both and race.

use std::path::PathBuf;
use game::adventure::{AdventureManager, Archetype};

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const OTHER_LOGIN: &str = "someone_else";

#[tokio::test]
async fn admin_passives_gates_writes_and_a_saved_override_reaches_the_game() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_passives_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}},"other-token":{{"login":"{OTHER_LOGIN}","display_name":"SomeoneElse","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // A real Warrior with a real allocation, so the override's effect is
    // observable through the same accessor combat reads.
    manager.join(ADMIN_LOGIN, "Lokati").await;
    manager.change_archetype(ADMIN_LOGIN, Archetype::Warrior).await.expect("class change must succeed");
    // One rank only - a fresh level-1 character has exactly one passive
    // point (`points_for_level(1)` = 1), so the override this test
    // observes is the RANK 1 value.
    manager.preview_allocate_passive(ADMIN_LOGIN, "bulwark", 1, false).await.expect("allocation must succeed");
    manager.save_passive_tree(ADMIN_LOGIN).await.expect("saving the tree must succeed");

    let baseline = manager.character(ADMIN_LOGIN).await.expect("joined").passive_node_magnitude("bulwark");
    assert!(baseline > 0.0, "sanity: an allocated node must have a non-zero magnitude, got {baseline}");

    // --- the gate ----------------------------------------------------
    let anon = client.get(format!("{base}/admin/passives")).send().await.expect("GET failed");
    assert_eq!(anon.status(), reqwest::StatusCode::OK);
    let anon_body = anon.text().await.expect("body");
    assert!(anon_body.contains("Not Found"), "a logged-out visitor must get the generic fallback");
    assert!(!anon_body.contains("Passive Values"), "and must not see the page itself");

    let other = client.get(format!("{base}/admin/passives")).header(reqwest::header::COOKIE, "adv_session=other-token").send().await.expect("GET failed");
    let other_body = other.text().await.expect("body");
    assert!(other_body.contains("Not Found"), "a logged-in NON-admin must get the generic fallback too");
    assert!(!other_body.contains("Passive Values"));

    let admin = client.get(format!("{base}/admin/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed");
    let admin_body = admin.text().await.expect("body");
    assert!(admin_body.contains("Passive Values"), "the admin must see the page");
    assert!(admin_body.contains("bulwark"), "and its nodes");
    assert!(!admin_body.contains("differs from default"), "nothing is overridden yet");

    // --- a NON-admin write must not take effect ----------------------
    let sneaky = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=other-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark"), ("r1", "0.9"), ("r2", "0.9"), ("r3", "0.9")])
        .send()
        .await
        .expect("POST failed");
    assert!(sneaky.status().is_redirection(), "the handler redirects regardless, to avoid confirming the page exists");
    assert_eq!(
        manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark"),
        baseline,
        "a non-admin POST must not change any value"
    );
    assert!(!scratch.join("adventure-passive-overrides.toml").exists(), "and must not create the overrides file");

    // --- the admin write ---------------------------------------------
    let save = client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark"), ("r1", "0.11"), ("r2", "0.22"), ("r3", "0.33")])
        .send()
        .await
        .expect("POST failed");
    assert!(save.status().is_redirection());

    // It reached the file...
    let overrides_file = scratch.join("adventure-passive-overrides.toml");
    assert!(overrides_file.exists(), "the override must be persisted so it survives a restart");
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(contents.contains("bulwark"), "the file must name the node, got:\n{contents}");

    // ...and it reached the GAME, through the accessor combat uses.
    let tuned = manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark");
    assert_eq!(tuned, 0.11, "the character sits at rank 1, so it must now read the rank 1 override");
    assert_ne!(tuned, baseline, "sanity: the override must actually differ");

    // The admin page reflects it, and so does the player-facing tree.
    let admin_body = client
        .get(format!("{base}/admin/passives?class=warrior"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(admin_body.contains("differs from default"), "the tuned node must be marked");
    assert!(admin_body.contains("/admin/passives/revert"), "and must offer a revert");

    let player_page =
        client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed").text().await.expect("body");
    // Matched on the MARKUP, not the bare class name: `base.html` now
    // carries a `.passive-tuned` CSS rule, which every rendered page
    // embeds, so a substring check for `passive-tuned` alone would pass
    // whether or not the note was ever emitted.
    assert!(
        player_page.contains("class=\"passive-tuned\""),
        "a retuned node must say so on the player's own tree rather than silently diverging"
    );
    assert!(player_page.contains("Tuned: 0.11"), "and must state the real numbers");

    // --- a bad node key is refused -----------------------------------
    client
        .post(format!("{base}/admin/passives/save"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "not_a_real_node"), ("r1", "1"), ("r2", "1"), ("r3", "1")])
        .send()
        .await
        .expect("POST failed");
    let contents = std::fs::read_to_string(&overrides_file).expect("readable");
    assert!(!contents.contains("not_a_real_node"), "a key that isn't in the class being edited must never be stored, got:\n{contents}");

    // --- revert -------------------------------------------------------
    let revert = client
        .post(format!("{base}/admin/passives/revert"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .form(&[("class", "warrior"), ("node_key", "bulwark")])
        .send()
        .await
        .expect("POST failed");
    assert!(revert.status().is_redirection());
    assert_eq!(
        manager.character(ADMIN_LOGIN).await.unwrap().passive_node_magnitude("bulwark"),
        baseline,
        "revert must return the node to its compiled-in value"
    );

    let player_page =
        client.get(format!("{base}/passives")).header(reqwest::header::COOKIE, "adv_session=admin-token").send().await.expect("GET failed").text().await.expect("body");
    assert!(!player_page.contains("class=\"passive-tuned\""), "and the tuned note must disappear with it");

    std::fs::remove_dir_all(&scratch).ok();
}
