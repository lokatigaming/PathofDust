//! Live bugfix regression (2026-08-19) - the Divine Dust craft recipe's
//! own `<form>` (`render_divine_dust_recipe_row`) submits no `item_a`
//! field at all (it's a currency-only recipe, no item involved). When
//! `CraftForm::item_a` was a required `String`, Axum's own `Form`
//! extractor rejected that submission with a 422 ("missing field
//! `item_a`") BEFORE `do_craft` ever ran - completely unusable in
//! production despite every existing structural test passing, because
//! none of them POST a real, item-less form through the real Axum
//! extractor (the sibling `divine_dust_ui_http.rs` only GETs and checks
//! rendered markup). This file closes that gap: real `reqwest` POSTs
//! against a real, disposable `adventure_web` server, exactly
//! reproducing what a browser actually submits (only the fields the real
//! `<form>` HTML actually contains - no `item_a` key at all for the
//! recipe, matching the live bug precisely).
//!
//! Also click-tests the sibling "Apply Divine Dust" action (sacralize on
//! a non-Sacred item, then reroll on the now-Sacred one) in case it had
//! its own form/extractor mismatch - it didn't (its shared form already
//! carries `item_a` via the item picker), but per the craft-multiplier
//! lesson this needs a real live POST to actually close, not an
//! assumption.
//!
//! One `#[tokio::test]` per the same `set_data_dir`-is-a-process-wide-
//! `OnceLock` reasoning `divine_dust_ui_http.rs` already documents - a
//! second test fn in this file would race this one.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character};

#[tokio::test]
async fn craft_recipe_and_apply_divine_dust_both_work_over_real_http() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("divine_dust_craft_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "dust-craft-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"DustCraftTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("DustCraftTester".to_string());
    character.dust = 100_000;
    character.sand = 1_000;
    character.divine_dust = 100;
    // `Character::new`'s own starter kit auto-equips a real, generated,
    // non-Sacred weapon - reused here as the apply-button's own target
    // rather than hand-constructing an `Item` (most of its fields are
    // private-constructor-only from an external test crate).
    let weapon_id = character.weapon.as_ref().expect("starter kit must equip a weapon").id.clone();
    let weapon_tier = character.weapon.as_ref().unwrap().tier;
    let apply_cost = 2 * weapon_tier as u64;
    assert!(character.weapon.as_ref().unwrap().sacred_affix.is_none(), "fixture assumption: the starter weapon must not already be Sacred");

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

    // Reads the persisted character straight off disk - the same
    // mechanism `AdventureManager` itself uses, so this is a genuine
    // end-to-end check of what actually landed, not a re-derivation.
    let read_character = || -> Character {
        let raw = std::fs::read_to_string(&characters_path).expect("failed to read characters file");
        let map: HashMap<String, Character> = serde_json::from_str(&raw).expect("failed to parse characters file");
        map.get(TEST_LOGIN).expect("test character must still exist").clone()
    };

    // --- THE LIVE BUG: the craft recipe's own form has no item_a field ---
    // Deliberately mirrors render_divine_dust_recipe_row's exact <form>
    // fields (action + times only) - reqwest's .form() only sends what's
    // listed here, so this is byte-for-byte what a real browser submits.
    let before = read_character();
    let resp = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("action", "divine dust craft"), ("times", "1")])
        .send()
        .await
        .expect("POST /craft (divine dust craft, x1) failed at the transport level");
    assert_ne!(
        resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "the live bug: Axum's Form<CraftForm> extractor must not reject an item-less submission with 422 missing field `item_a`"
    );
    assert!(resp.status().is_redirection(), "a successful craft must redirect, got {}", resp.status());

    let after_x1 = read_character();
    assert_eq!(after_x1.divine_dust, before.divine_dust + 1, "x1 recipe must grant exactly the configured output (default 1)");
    assert_eq!(after_x1.dust, before.dust - 1000, "x1 recipe must deduct exactly its dust cost (default 1000)");
    assert_eq!(after_x1.sand, before.sand - 10, "x1 recipe must deduct exactly its sand cost (default 10)");

    // --- Same recipe, x10 batch - the OTHER form the owner asked to click-test ---
    let resp = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("action", "divine dust craft"), ("times", "10")])
        .send()
        .await
        .expect("POST /craft (divine dust craft, x10) failed at the transport level");
    assert_ne!(resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY, "x10 must not 422 either - same item-less form, just a different times value");
    assert!(resp.status().is_redirection(), "a successful x10 craft must redirect, got {}", resp.status());

    let after_x10 = read_character();
    assert_eq!(after_x10.divine_dust, after_x1.divine_dust + 10, "x10 must grant exactly 10x the recipe's output");
    assert_eq!(after_x10.dust, after_x1.dust - 10_000, "x10 must deduct exactly 10x the dust cost");
    assert_eq!(after_x10.sand, after_x1.sand - 100, "x10 must deduct exactly 10x the sand cost");

    // --- Sibling click-test: Apply Divine Dust, sacralize path (not yet Sacred) ---
    let before_apply = after_x10.divine_dust;
    let resp = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("action", "divine dust"), ("item_a", weapon_id.as_str())])
        .send()
        .await
        .expect("POST /craft (divine dust apply, sacralize) failed at the transport level");
    assert_ne!(resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY, "the apply/reroll action's own form already carries item_a - no 422 expected here");
    assert!(resp.status().is_redirection(), "a successful apply must redirect, got {}", resp.status());

    let after_sacralize = read_character();
    assert_eq!(after_sacralize.divine_dust, before_apply - apply_cost, "applying Divine Dust must cost exactly 2x the item's own tier ({weapon_tier})");
    let sacred_after_first = after_sacralize.weapon.as_ref().unwrap().sacred_affix;
    assert!(sacred_after_first.is_some(), "sacralize must set sacred_affix");
    assert!(after_sacralize.weapon.as_ref().unwrap().perfect, "sacralizing a non-Perfect item must also make it Perfect (docs/divine_dust_spec.md's own decisions log)");

    // --- Sibling click-test: Apply Divine Dust, reroll path (already Sacred now) ---
    let before_reroll = after_sacralize.divine_dust;
    let resp = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("action", "divine dust"), ("item_a", weapon_id.as_str())])
        .send()
        .await
        .expect("POST /craft (divine dust apply, reroll) failed at the transport level");
    assert!(resp.status().is_redirection(), "a successful reroll must redirect, got {}", resp.status());

    let after_reroll = read_character();
    assert_eq!(after_reroll.divine_dust, before_reroll - apply_cost, "rerolling must cost the same 2x-tier formula as sacralizing");
    assert!(after_reroll.weapon.as_ref().unwrap().sacred_affix.is_some(), "reroll must leave the item Sacred");

    std::fs::remove_dir_all(&scratch).ok();
}
