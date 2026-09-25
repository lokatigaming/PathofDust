//! Item 31 (2026-09-25) - the Unique Shard's browse-before-commit flow.
//! Real disposable `adventure_web` server, real `reqwest` GET/POSTs
//! through the real `Form<UniqueShardForm>` extractor.
//!
//! Every POST is built from the RENDERED page (`admin_tunables_splash_http.rs`'s
//! drift-guard shape): the field NAMES come from scraping the
//! `/craft/unique-shard` form, and the `choice` VALUES from the picker's
//! own `data-us-choice`/`data-us-pick`/`data-affix` attributes - exactly
//! what base.html's script copies into that form. If the page and the
//! handler drift apart on either, a POST below is refused and fails.
//!
//! One `#[tokio::test]` - `set_data_dir` is a process-wide `OnceLock`
//! (see `unique_shard_picker_http.rs`).

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{Affix, AdventureManager, Character, CraftAction, LuckyKind, UniqueAffix};

/// The `<form ...>...</form>` whose opening tag contains `action`.
fn find_form<'a>(page: &'a str, action: &str) -> &'a str {
    let start = page.find(&format!("<form method=\"post\" action=\"{action}\"")).unwrap_or_else(|| panic!("no form posting to {action}"));
    let form = &page[start..];
    &form[..form.find("</form>").expect("every form must be closed")]
}

/// Every `name="..."` of the form's inputs, as rendered.
fn field_names(form: &str) -> Vec<String> {
    form.split("<input").skip(1).filter_map(|chunk| Some(chunk.split('>').next()?.split("name=\"").nth(1)?.split('"').next()?.to_string())).collect()
}

/// Every value of attribute `attr` inside `html`, in order.
fn attr_values(html: &str, attr: &str) -> Vec<String> {
    html.split(&format!("{attr}=\"")).skip(1).filter_map(|s| s.split('"').next().map(str::to_string)).collect()
}

/// The `data-us-item` row for `item_id`.
fn item_row<'a>(page: &'a str, item_id: &str) -> &'a str {
    let start = page.find(&format!("data-us-item=\"{item_id}\"")).unwrap_or_else(|| panic!("no picker row for {item_id}"));
    let row = &page[start..];
    &row[..row.find("</div>").expect("row must be closed")]
}

#[tokio::test]
async fn unique_shard_spends_only_at_the_final_confirm_and_the_form_matches_the_handler() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("unique_shard_flow_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    const LOGIN: &str = "shard-browser";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"shard-token":{{"login":"{LOGIN}","display_name":"Browser","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");
    // Stops the one-time token backfills re-granting what is cleared below
    // (same markers `craft_confirm_ui_http.rs` seeds).
    for marker in ["adventure-craft-token-backfill-marker.json", "adventure-craft-token-backfill-v2-marker.json"] {
        std::fs::write(scratch.join(marker), "true").expect("failed to seed a migration marker");
    }

    // Two shards: one Divine Forge on the worn weapon, one Luckstone on a
    // bagged helm. The weapon gets three known modifiers so Divine Forge
    // has something to tick.
    let characters_path = scratch.join("adventure-characters.json");
    let mut character = Character::new("Browser".to_string());
    character.craft_tokens.clear();
    character.craft_tokens.push((CraftAction::UniqueShard, 2));
    let weapon = character.weapon.as_mut().expect("starter kit must equip a weapon");
    weapon.affixes = vec![(Affix::CritChance, 1.0), (Affix::Evasion, 1.0), (Affix::BlockChance, 1.0)];
    weapon.unique_affix = None;
    let weapon_id = weapon.id.clone();
    let mut bag_helm = character.helm.take().expect("starter kit must equip a helm");
    bag_helm.unique_affix = None;
    let bag_id = bag_helm.id.clone();
    assert!(character.add_to_inventory(bag_helm), "the bag must accept the helm");
    let mut characters = HashMap::new();
    characters.insert(LOGIN.to_string(), character);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    const COOKIE: &str = "adv_session=shard-token";

    let read_character = || -> Character {
        let raw = std::fs::read_to_string(&characters_path).expect("failed to read characters file");
        let map: HashMap<String, Character> = serde_json::from_str(&raw).expect("failed to parse characters file");
        map.get(LOGIN).expect("test character must still exist").clone()
    };
    let shards = || read_character().craft_token_count(CraftAction::UniqueShard);
    let page = client.get(format!("{base}/inventory")).header(reqwest::header::COOKIE, COOKIE).send().await.expect("GET /inventory failed");
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let page = page.text().await.expect("failed to read /inventory body");
    let post = |body: Vec<(String, String)>| {
        let client = client.clone();
        let url = format!("{base}/craft/unique-shard");
        async move {
            let resp = client.post(url).header(reqwest::header::COOKIE, COOKIE).form(&body).send().await.expect("POST failed at the transport level");
            assert!(resp.status().is_redirection(), "/craft/unique-shard must extract and redirect, got {}", resp.status());
        }
    };

    // --- The page: an opener with no confirm, every kind, one final form ---
    let opener = page.split("<button").find(|b| b.contains("data-us-open")).expect("a held shard must render the picker opener");
    let opener = &opener[..opener.find('>').unwrap()];
    assert!(!opener.contains("data-confirm") && opener.contains("type=\"button\""), "the shard button only opens the picker - no submit, no confirm:\n{opener}");
    let picks = attr_values(&page, "data-us-pick");
    assert_eq!(picks, ["celestialConversion", "splitPersonality", "unyielding", "craftingExpertise", "divineForge", "luckstone"], "every top-level affix is offered");
    let form = find_form(&page, "/craft/unique-shard");
    let mut names = field_names(form);
    names.sort();
    assert_eq!(names, ["choice", "item_id"], "the form's fields must be exactly what the handler reads:\n{form}");
    assert!(form.contains("data-confirm=\"1\"") && form.contains("data-confirm-msg="), "the final submit carries the confirm:\n{form}");
    assert!(item_row(&page, &bag_id).contains("data-off=\"craftingExpertise\""), "Expertise on a bag item is greyed with a reason, not hidden");
    assert_eq!(shards(), 2, "rendering and browsing spend nothing");

    let body = |item: &str, choice: &str| names.iter().map(|n| (n.clone(), if n == "item_id" { item.to_string() } else { choice.to_string() })).collect::<Vec<_>>();

    // --- A rejected pick spends nothing: Expertise on the bag item ---
    post(body(&bag_id, "craftingExpertise")).await;
    let c = read_character();
    assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 2, "a refused request must not spend the shard");
    assert!(c.inventory.iter().find(|i| i.id == bag_id).unwrap().unique_affix.is_none());

    // --- Divine Forge from the weapon row's own modifier keys ---
    let affix_keys = attr_values(item_row(&page, &weapon_id), "data-affix");
    assert_eq!(affix_keys.len(), 3, "the weapon row lists its three modifiers for the pick-2");
    post(body(&weapon_id, &format!("divineForge:{},{}", affix_keys[0], affix_keys[2]))).await;
    let c = read_character();
    assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 1, "exactly one shard spent, at the final confirm");
    assert_eq!(c.weapon.as_ref().unwrap().unique_affix, Some(UniqueAffix::DivineForge { affixes: [Affix::CritChance, Affix::BlockChance] }));

    // --- Luckstone from the sub-panel's own rendered choice ---
    let luck = attr_values(&page, "data-us-choice").into_iter().find(|c| c.ends_with(":block")).expect("the Luckstone panel offers Block");
    post(body(&bag_id, &luck)).await;
    let c = read_character();
    assert_eq!(c.craft_token_count(CraftAction::UniqueShard), 0);
    match c.inventory.iter().find(|i| i.id == bag_id).unwrap().unique_affix {
        Some(UniqueAffix::Luckstone { kind: LuckyKind::Block, pct }) => assert!(pct > 0, "the pct rolls on apply"),
        other => panic!("expected a Block Luckstone, got {other:?}"),
    }

    std::fs::remove_dir_all(&scratch).ok();
}
