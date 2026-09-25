//! Item 30 - the panel Reforge label must show the price the server
//! charges, Crafting Expertise's 10% discount included.
//!
//! The label used to be computed in `base.html` from the rule's tier
//! parameters alone, while `craft_item_ex` discounted the Expertise item:
//! a player saw one number and paid another. Both now read
//! `panel_reforge_dust_cost`; the page carries it per item as the
//! option's `data-reforge-cost`.
//!
//! No hand-written expectation: the price is scraped from the REAL
//! rendered /inventory page, then the real charge is made and the dust
//! actually taken is compared against it - for an Expertise item and a
//! plain item at the same tier.
//!
//! One `#[tokio::test]` per file, deliberately - `adventure::set_data_dir`
//! is a process-wide `OnceLock`. See `divinity_ui_http.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, CraftAction, UniqueAffix};

/// The `data-reforge-cost` on item picker A's `<option>` for `item_id`.
/// Picker A is the first `<select>` carrying the option, and the one the
/// Reforge button prices off.
fn rendered_reforge_cost(body: &str, item_id: &str) -> u64 {
    let open = body.find(&format!("<option value=\"{item_id}\"")).unwrap_or_else(|| panic!("no picker option for {item_id}"));
    let tag = &body[open..open + body[open..].find('>').expect("option tag must close")];
    let attr = "data-reforge-cost=\"";
    let at = tag.find(attr).unwrap_or_else(|| panic!("option for {item_id} carries no data-reforge-cost: {tag}")) + attr.len();
    tag[at..at + tag[at..].find('"').unwrap()].parse().expect("data-reforge-cost must be a number")
}

#[tokio::test]
async fn reforge_label_matches_the_charge_with_and_without_crafting_expertise() {
    // Integration tests run with their PACKAGE dir as CWD, but the
    // template loader resolves "templates/" against CWD and that
    // directory belongs to the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("reforge_expertise_price_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "reforge-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"ReforgeTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    // No banked tokens (and the backfills that would re-grant them marked
    // done), so Reforge is paid in dust - see `craft_confirm_ui_http.rs`.
    for marker in ["adventure-craft-token-backfill-marker.json", "adventure-craft-token-backfill-v2-marker.json"] {
        std::fs::write(scratch.join(marker), "true").expect("failed to seed a migration marker");
    }
    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("ReforgeTester".to_string());
    character.craft_tokens.clear();
    character.dust = 10_000_000;
    // Same tier on both, so the discount is visible as a difference
    // between the two prices and the test cannot pass on two equal ones.
    let weapon = character.weapon.as_mut().expect("starter kit equips a weapon");
    weapon.tier = 10;
    weapon.unique_affix = Some(UniqueAffix::CraftingExpertise);
    let expert_id = weapon.id.clone();
    let helm = character.helm.as_mut().expect("starter kit equips a helm");
    helm.tier = 10;
    helm.unique_affix = None;
    let plain_id = helm.id.clone();

    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path)
        .await
        .expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, "adv_session=test-token").send().await.expect("GET /inventory failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("failed to read /inventory body");
    assert!(body.contains("data-reforge=\"1\""), "the Reforge button must be on the page");

    // Both scraped before either charge: a reforge rerolls the item's tier.
    let expert_shown = rendered_reforge_cost(&body, &expert_id);
    let plain_shown = rendered_reforge_cost(&body, &plain_id);
    assert!(expert_shown < plain_shown, "same tier, so the Expertise item must show the cheaper price: {expert_shown} vs {plain_shown}");

    for (id, shown) in [(&expert_id, expert_shown), (&plain_id, plain_shown)] {
        let before = manager.character(TEST_LOGIN).await.unwrap().dust;
        manager.craft_item_ex(TEST_LOGIN, id, CraftAction::Reforge, false, true).await.expect("affordable");
        let charged = before - manager.character(TEST_LOGIN).await.unwrap().dust;
        assert_eq!(charged, shown, "item {id}: the page showed {shown}d but the server charged {charged}d");
    }

    // The label must READ the option's price, not recompute it.
    let base_html = std::fs::read_to_string("templates/base.html").expect("templates/base.html must be readable from the workspace root");
    assert!(base_html.contains("getAttribute('data-reforge-cost')"), "base.html's Reforge label must read the option's server-computed data-reforge-cost");

    std::fs::remove_dir_all(&scratch).ok();
}
