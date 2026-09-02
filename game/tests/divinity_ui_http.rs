//! Divinity Stage 4 (2026-08-24) - the end-to-end check that a player can
//! actually reach the feature, over genuine HTTP against a disposable
//! instance (same shape as `divine_dust_ui_http.rs`/
//! `unique_shard_picker_http.rs`).
//!
//! Stages 1-3 built and unit-tested the whole mechanic, and every one of
//! those tests passed while Divinity was completely unreachable from the
//! browser - there was no button and no popup. Structural tests cannot
//! see that gap; only a real GET of the real page and a real POST through
//! the real `Form<CraftForm>` extractor can.
//!
//! The POST half deliberately derives its field set FROM THE RENDERED
//! PAGE rather than from a hand-written list (see the "form/struct drift
//! guard" in `admin_tunables_splash_http.rs`, and `CraftForm::item_a`'s
//! own doc for the live 422 that convention exists to prevent). A
//! hand-written superset body can only ever catch a field the test forgot
//! to add - never a field the page stopped rendering while the struct
//! still required it, which is the direction that reaches production.
//!
//! One `#[tokio::test]`, deliberately - `adventure::set_data_dir` is a
//! process-wide `OnceLock`, so a second test function in this file would
//! race this one for who calls it first.

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, CraftAction};

#[tokio::test]
async fn a_player_holding_a_shard_can_see_and_run_divinity() {
    // Integration tests run with their PACKAGE dir as CWD, but the
    // template loader resolves "templates/" against CWD and that
    // directory belongs to the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("divinity_ui_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const TEST_LOGIN: &str = "divinity-tester";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"DivinityTester","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    // Must run before anything that could touch `data_path` - even
    // constructing a `Character` reaches it transitively via item
    // generation. See `divine_dust_ui_http.rs` for the full note.
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("DivinityTester".to_string());
    // `Character::add_craft_token` is crate-private; `craft_tokens` is
    // `pub`, so an external test crate sets it directly.
    character.craft_tokens.push((CraftAction::UniqueShard, 1));
    // Divinity is BAG-only by ruling - `Character::new`'s starter kit is
    // entirely EQUIPPED and leaves `inventory` empty, which is exactly
    // the `DivinityError::EmptyBag` case. Move three starter pieces into
    // the bag (rather than hand-constructing `Item`s, whose fields are
    // private-constructor-only from an external test crate) and leave two
    // equipped, so the run also has real equipped gear to NOT touch.
    for slot in [&mut character.weapon, &mut character.helm, &mut character.gloves] {
        let item = slot.take().expect("starter kit must equip this slot");
        character.inventory.push(item);
    }
    let equipped_body_id = character.body.as_ref().expect("starter kit must equip a body").id.clone();
    let bag_ids: Vec<String> = character.inventory.iter().map(|i| i.id.clone()).collect();
    assert_eq!(bag_ids.len(), 3, "fixture assumption: exactly three bag items");

    let mut characters = HashMap::new();
    characters.insert(TEST_LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = game::adventure_web::start_adventure_web_server(
        0,
        manager.clone(),
        sessions_path,
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

    // --- Step 1: the button must actually be on the page. ---
    let resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /inventory failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("failed to read /inventory body");

    assert!(body.contains("value=\"divinity\""), "the Divinity pseudo-action button must be rendered for a shard holder:\n{body}");
    assert!(body.contains("Divinity (3 items, 1 Unique Shard)"), "the button must state the real ELIGIBLE count and the shard price");
    // This asserts the ATTRIBUTE is emitted, and NOTHING MORE. It passed
    // continuously from 2026-08-24 to 2026-09-02 while Divinity's confirm
    // dialog never once appeared: base.html's listener was bound to the
    // wrong form the whole time, so the server-side attribute this line
    // checks was correct and inert. Read it as "the message text exists",
    // never as "the confirmation works" - whether anything is listening
    // is checked by `craft_confirm_ui_http.rs`, which owns that question
    // for all six confirmed actions.
    assert!(body.contains("data-confirm-msg=\""), "the whole-bag confirm message must be present - the default per-item confirm names an item this action ignores");

    // --- Step 2: POST exactly the fields the crafting form renders. ---
    // Anchored on `item_a`'s `<select>` rather than on `action="/craft"`,
    // because the Divine Dust recipe row is a SECOND form posting to the
    // same path and renders first.
    let form_html = {
        let picker = body.find("<select name=\"item_a\"").expect("the crafting form's item_a picker must be on the page");
        let start = body[..picker].rfind("<form").expect("the item_a picker must live inside a form");
        let end = start + body[start..].find("</form>").expect("the crafting form must be closed");
        &body[start..end]
    };
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.contains(&"action"), "sanity: the scrape must have found the action buttons, got {rendered:?}");
    assert!(rendered.contains(&"item_a"), "sanity: the scrape must have found the item picker, got {rendered:?}");

    // One value per rendered field. `action` carries the request itself;
    // the rest just have to EXTRACT, which is the whole point of the
    // exercise - a field the page renders that `CraftForm` cannot parse
    // (or one it requires that the page no longer renders) 422s here.
    let exact: Vec<(&str, &str)> = rendered
        .iter()
        .map(|name| match *name {
            "action" => ("action", "divinity"),
            "item_a" | "item_b" => (*name, bag_ids[0].as_str()),
            // `times` is a u32 - an empty string would 422 on parse, not
            // on the drift this guard is actually looking for.
            "times" => ("times", "1"),
            other => (other, "1"),
        })
        .collect();

    let resp = client.post(format!("{base}/craft")).header(reqwest::header::COOKIE, cookie).form(&exact).send().await.expect("POST /craft failed at the transport level");
    assert_ne!(
        resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "posting exactly the {} fields the crafting form renders must extract cleanly. A 422 here means `CraftForm` requires a field the form no longer renders, or renders one it cannot parse",
        rendered.len()
    );
    assert!(resp.status().is_redirection(), "a completed Divinity run must redirect to its popup, got {}", resp.status());
    let location = resp.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("Location must be ASCII").to_string();
    assert!(location.starts_with("/inventory?divinity_run=1"), "must redirect to the Divinity popup (not the generic craft-error one), got {location}");

    // --- Step 3: the run really happened. ---
    let after = read_character();
    assert_eq!(after.craft_token_count(CraftAction::UniqueShard), 0, "the Unique Shard must be consumed by a run that did real work");
    assert_eq!(after.inventory.len(), 3, "Divinity never adds or destroys items - it only crafts the ones already in the bag");
    assert!(
        after.inventory.iter().all(|i| i.nickname.as_deref() == Some("From Divinity")),
        "every item that reaches Krangle is named on the spot, so the dashboard's nickname prompt has nothing to ask"
    );
    assert_eq!(after.body.as_ref().expect("equipped body must survive").id, equipped_body_id, "equipped gear is never touched by Divinity");

    // --- Step 4: the popup must RENDER. `divinity_run` was declared as
    // an `IndexParams` field a commit before anything read it; this is
    // what proves it is now dispatched rather than silently ignored. ---
    let popup_resp = client.get(format!("{base}{location}")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET of the redirect target failed");
    assert_eq!(popup_resp.status(), reqwest::StatusCode::OK);
    let popup_body = popup_resp.text().await.expect("failed to read popup body");
    assert!(popup_body.contains("id=\"divinity-modal\""), "the Divinity result popup must render:\n{popup_body}");
    assert!(popup_body.contains("Your bag has been remade."), "the popup must carry its own copy, not another action's");
    assert!(popup_body.contains("craft steps in total, no dust spent."), "the popup must carry the run summary from divinity_summary_text");

    // --- Step 5: the second use, with no shard left, must refuse for
    // free through the ordinary craft-error popup. ---
    let resp = client.post(format!("{base}/craft")).header(reqwest::header::COOKIE, cookie).form(&exact).send().await.expect("second POST /craft failed");
    let location = resp.headers().get(reqwest::header::LOCATION).expect("a redirect must carry a Location").to_str().expect("Location must be ASCII").to_string();
    assert!(location.starts_with("/inventory?craft_failed="), "a shardless run must refuse via the craft-error popup, got {location}");
    assert!(location.contains("Unique+Shard") || location.contains("Unique%20Shard"), "the refusal must name the missing Unique Shard, got {location}");

    // And with no shard held, the button is gone entirely - the same
    // hidden-until-earned shape the other token buttons use.
    let resp = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /inventory failed");
    let body = resp.text().await.expect("failed to read /inventory body");
    assert!(!body.contains("value=\"divinity\""), "with no shard held the Divinity button must not render at all");

    std::fs::remove_dir_all(&scratch).ok();
}
