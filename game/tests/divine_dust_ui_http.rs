//! Divine Dust Stage 5 (2026-08-19) - real end-to-end render check for
//! the new /craft page UI, same disposable-instance-over-genuine-HTTP
//! shape as `http_golden_responses.rs` (that harness only asserts status
//! code/content-type, not markup content, so it wouldn't catch a broken
//! or missing Divine Dust element). One `#[tokio::test]`, deliberately -
//! `adventure::set_data_dir` is a process-wide `OnceLock`; Rust already
//! isolates this file into its own process, so it doesn't race any other
//! test file, but a second test function IN this file would race this
//! one for who calls `set_data_dir` first.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character};

#[tokio::test]
async fn inventory_page_renders_the_divine_dust_recipe_and_apply_button() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("divine_dust_ui_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "dust-ui-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"DustUiTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    // `set_data_dir` must run BEFORE anything that could touch
    // `data_path` (a process-wide `OnceLock` - see its own doc) - even
    // constructing a `Character` reaches it transitively (item generation
    // reads `adventure-item-balance.toml` via `load_item_balance_file`),
    // so it has to come before that too, not just before
    // `AdventureManager::new`.
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    // `AdventureManager::characters` is private to the `adventure` module
    // (not even `pub(crate)`) - this integration test is a separate
    // crate, so state is seeded the same way `http_golden_responses.rs`
    // seeds its own fixture: write the characters file directly BEFORE
    // `AdventureManager::new` loads it. `Character::new` already grants
    // the 5-item starter kit, so `render_crafting_card` takes its normal
    // (non-empty-inventory) path with no extra setup.
    let mut character = Character::new("DustUiTester".to_string());
    character.dust = 5000;
    character.sand = 100;
    character.divine_dust = 50;
    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(scratch.join("adventure-characters.json"), serde_json::to_string(&characters).expect("must serialize"))
        .expect("failed to seed the scratch characters file");

    // Unlocked world. The recipe is gated behind a one-way stage latch
    // since 2026-09-02, so a fresh stage-0 world would render the LOCKED
    // row and every assertion below about the recipe's costs and buttons
    // would fail for a reason that has nothing to do with this test.
    // `highest_stage` is deliberately absent from this JSON, exactly as it
    // is from the real production world file, so the `max(stage)` backfill
    // in `AdventureManager::new` is what unlocks it here.
    std::fs::write(scratch.join("adventure-world.json"), r#"{"stage":300,"last_boss_kind":null}"#).expect("failed to seed the scratch world file");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = game::adventure_web::start_adventure_web_server(
        0,
        manager.clone(),
        sessions_path,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let resp = client
        .get(format!("{base}/inventory"))
        .header(reqwest::header::COOKIE, "adv_session=test-token")
        .send()
        .await
        .expect("GET /inventory failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("failed to read /inventory body");

    // The always-visible craft recipe row.
    assert!(body.contains("Craft Divine Dust"), "recipe row must be present:\n{body}");
    assert!(body.contains("value=\"divine dust craft\""), "recipe form's pseudo-action must be present");
    assert!(body.contains("1000d + 10s"), "default recipe cost (1000 dust + 10 sand) must be shown");

    // The per-item apply/reroll button.
    assert!(body.contains("data-divine-dust-apply=\"1\""), "apply button must be present");
    assert!(body.contains("data-divine-dust=\"50\""), "apply button must carry the character's real Divine Dust balance");

    // The currency display itself, in at least the top nav and the
    // crafting card header (see top_nav/render_crafting_card's own
    // doc for every site this was added to).
    assert!(body.contains("✨ 50"), "Divine Dust balance (50) must be shown somewhere on the page:\n{body}");

    // Every item option now carries data-sacred (used by the client-side
    // script to switch the apply button's label to a reroll).
    assert!(body.contains("data-sacred=\""), "item options must carry data-sacred for the client-side apply/reroll label switch");

    // --- THE LOCKED HALF (2026-09-02) -----------------------------------
    // A second instance in the same data dir - different world/character
    // file NAMES, so `set_data_dir`'s process-wide `OnceLock` is untouched
    // (see this file's header) - standing at a stage the group has never
    // pushed past. What a below-threshold player actually sees has to be
    // checked in a real response, not inferred from the handler: the row
    // must be SHOWN (players want to know what they are working toward)
    // but must offer no way to submit.
    std::fs::write(scratch.join("locked-world.json"), r#"{"stage":1,"last_boss_kind":null}"#).expect("failed to seed the locked world file");
    std::fs::write(scratch.join("locked-characters.json"), serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the locked characters file");
    let locked_sessions = scratch.join("locked-sessions.json");
    std::fs::write(&locked_sessions, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"DustUiTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the locked sessions file");

    let locked_manager = AdventureManager::new(PathBuf::from("locked-characters.json"), PathBuf::from("locked-world.json"), PathBuf::from("locked-reforge-cooldown.json"));
    let locked_addr = game::adventure_web::start_adventure_web_server(
        0,
        locked_manager,
        locked_sessions,
    )
    .await
    .expect("disposable locked adventure_web instance must start");

    let locked_body = client
        .get(format!("http://127.0.0.1:{}/inventory", locked_addr.port()))
        .header(reqwest::header::COOKIE, "adv_session=test-token")
        .send()
        .await
        .expect("GET /inventory on the locked instance failed")
        .text()
        .await
        .expect("failed to read the locked /inventory body");

    assert!(locked_body.contains("unlocks at stage 300"), "a locked recipe must still SHOW itself and name the stage it unlocks at:\n{locked_body}");
    assert!(
        !locked_body.contains("value=\"divine dust craft\""),
        "a locked recipe must render no submittable form - the server refuses it anyway, but offering a button that always fails is worse than showing the requirement"
    );
    assert!(locked_body.contains("Craft Divine Dust"), "the row's own label must survive the lock, so the player can see what it is");

    std::fs::remove_dir_all(&scratch).ok();
}
