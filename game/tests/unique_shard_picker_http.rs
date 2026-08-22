//! Unified Unique Shards (2026-08-19) - real end-to-end HTTP coverage for
//! `CraftAction::UniqueShard`'s new apply-time picker, through a real
//! disposable `adventure_web` server and genuine `reqwest` POSTs against
//! the actual Axum extractors - not a manager-API-level test. Per the
//! same lesson `divine_dust_craft_http.rs` documents (a real live 422 that
//! every purely-structural/manager-level test missed because none of them
//! POST through the real `Form<CraftForm>` extractor): this feature
//! touches the SAME `CraftForm`/`do_craft` handler that bug lived in, so
//! it gets the same real-HTTP treatment rather than trusting the manager-
//! level tests (`unique_shard_tests` in manager.rs) to have caught
//! everything a real browser submission could hit.
//!
//! One `#[tokio::test]` per the same `set_data_dir`-is-a-process-wide-
//! `OnceLock` reasoning `divine_dust_ui_http.rs`/`divine_dust_craft_http.rs`
//! both already document - a second test fn in this file would race this
//! one for who calls it first.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, CraftAction, UniqueAffix, ALL_UNIQUE_AFFIXES};

#[tokio::test]
async fn unique_shard_apply_and_choose_veil_both_work_over_real_http() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("unique_shard_picker_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "shard-picker-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"ShardPickerTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("ShardPickerTester".to_string());
    // `Character::add_craft_token` is crate-private; `craft_tokens` itself
    // is `pub`, so an external test crate sets it directly instead.
    character.craft_tokens.push((CraftAction::UniqueShard, 1));
    // `Character::new`'s own starter kit auto-equips a real, generated,
    // non-unique weapon - reused as the target item rather than hand-
    // constructing an `Item` (most fields are private-constructor-only
    // from an external test crate).
    let weapon_id = character.weapon.as_ref().expect("starter kit must equip a weapon").id.clone();
    assert!(character.weapon.as_ref().unwrap().unique_affix.is_none(), "fixture assumption: the starter weapon must not already be unique");

    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = game::adventure_web::start_adventure_web_server(
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

    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    let cookie = "adv_session=test-token";

    let read_character = || -> Character {
        let raw = std::fs::read_to_string(&characters_path).expect("failed to read characters file");
        let map: HashMap<String, Character> = serde_json::from_str(&raw).expect("failed to parse characters file");
        map.get(TEST_LOGIN).expect("test character must still exist").clone()
    };

    // --- Step 1: apply the Unique Shard - mirrors the real crafting
    // form's exact fields (action + item_a, via the "Apply Divine
    // Dust"-style item picker share the same <form>/item_a field every
    // other item-targeted action uses, unlike the currency-only Divine
    // Dust recipe form that caused the original 422). ---
    let resp = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("action", "unique shard"), ("item_a", weapon_id.as_str())])
        .send()
        .await
        .expect("POST /craft (unique shard) failed at the transport level");
    assert_ne!(resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY, "the real Form<CraftForm> extractor must accept this submission");
    assert!(resp.status().is_redirection(), "applying a Unique Shard must redirect (to the pending-choice view), got {}", resp.status());

    let after_apply = read_character();
    assert_eq!(after_apply.craft_token_count(CraftAction::UniqueShard), 0, "the token must be consumed at insert time, before any choice is made");
    assert!(after_apply.weapon.as_ref().unwrap().unique_affix.is_none(), "nothing is applied to the item until a choice is actually made");

    // --- Step 2: the pending-choice card must actually render, with a
    // real option for each UniqueAffix - a real GET through the real
    // page, not a direct call into render_veil_choice_card. ---
    let inventory_resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /inventory failed");
    assert_eq!(inventory_resp.status(), reqwest::StatusCode::OK);
    let body = inventory_resp.text().await.expect("failed to read /inventory body");
    assert!(body.contains("/craft/choose-veil"), "the pending-choice card must be showing, not the normal crafting card:\n{body}");
    for unique in ALL_UNIQUE_AFFIXES {
        assert!(body.contains(unique.name()), "the choice card must offer {unique:?} ({}) as an option:\n{body}", unique.name());
    }

    // --- Step 3: commit the choice via the real POST /craft/choose-veil
    // route - pick SplitPersonality specifically (not index 0), so this
    // also proves the index isn't just coincidentally always applying
    // whichever candidate happens to be first. ---
    let split_index = ALL_UNIQUE_AFFIXES.iter().position(|&a| a == UniqueAffix::SplitPersonality).expect("SplitPersonality must be in ALL_UNIQUE_AFFIXES");
    let resp = client
        .post(format!("{base}/craft/choose-veil"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("index", split_index.to_string())])
        .send()
        .await
        .expect("POST /craft/choose-veil failed at the transport level");
    assert!(resp.status().is_redirection(), "a successful choice must redirect, got {}", resp.status());

    let after_choice = read_character();
    assert_eq!(after_choice.weapon.as_ref().unwrap().unique_affix, Some(UniqueAffix::SplitPersonality), "the picked effect, and only the picked one, must land on the item");

    // The pending choice must be gone - a second GET /inventory must show
    // the normal crafting card again, not the choice card.
    let inventory_resp2 = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /inventory failed");
    let body2 = inventory_resp2.text().await.expect("failed to read /inventory body");
    assert!(!body2.contains("/craft/choose-veil"), "the pending choice must be cleared after committing:\n{body2}");

    std::fs::remove_dir_all(&scratch).ok();
}
