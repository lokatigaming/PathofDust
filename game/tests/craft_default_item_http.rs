//! Item 42b - the crafting card's main picker must open on an item that
//! can actually be crafted on. Real GET /inventory through a disposable
//! `adventure_web` server (shape copied from `unique_shard_picker_http.rs`),
//! on a FICTIONAL heretic-shaped character: locked weapon first, an
//! already-unique helm second, an eligible body third. The default must
//! land on the body, not on either item whose Unique Shard options would
//! all be greyed.
//!
//! One `#[tokio::test]` - `set_data_dir` is a process-wide `OnceLock`.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, ALL_UNIQUE_AFFIXES};

/// Every `<option value="{id}" ...>` opening tag on the page for `id`.
fn option_tags<'a>(body: &'a str, id: &str) -> Vec<&'a str> {
    let needle = format!("<option value=\"{id}\"");
    body.match_indices(&needle).map(|(at, _)| &body[at..at + body[at..].find('>').expect("option tag must close")]).collect()
}

#[tokio::test]
async fn crafting_picker_defaults_past_locked_and_unique_items() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("craft_default_item_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "craft-default-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"CraftDefaultTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("CraftDefaultTester".to_string());
    character.inventory.clear();
    character.weapon.as_mut().expect("starter kit must equip a weapon").locked = true;
    character.helm.as_mut().expect("starter kit must equip a helm").unique_affix = Some(ALL_UNIQUE_AFFIXES[0]);
    let weapon_id = character.weapon.as_ref().unwrap().id.clone();
    let helm_id = character.helm.as_ref().unwrap().id.clone();
    let body = character.body.as_ref().expect("starter kit must equip a body");
    assert!(!body.locked && !body.disenchant_protected && body.unique_affix.is_none(), "fixture assumption: the starter body is craftable");
    let body_id = body.id.clone();
    // The remembered item is the locked weapon, as on the live account.
    character.last_crafted_item_id = Some(weapon_id.clone());

    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    let resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, "adv_session=test-token").send().await.expect("GET /inventory failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let page = resp.text().await.expect("failed to read /inventory body");

    let body_tags = option_tags(&page, &body_id);
    assert!(!body_tags.is_empty(), "the body must be in the crafting picker:\n{page}");
    assert!(body_tags.iter().any(|t| t.ends_with(" selected")), "the picker must default to the eligible body: {body_tags:?}");
    for (what, id) in [("locked weapon", &weapon_id), ("already-unique helm", &helm_id)] {
        let tags = option_tags(&page, id);
        assert!(!tags.is_empty(), "the {what} must still be listed - the option list is untouched");
        assert!(tags.iter().all(|t| !t.ends_with(" selected")), "the {what} must not be the default: {tags:?}");
    }

    std::fs::remove_dir_all(&scratch).ok();
}
