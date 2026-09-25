//! Item 29d (2026-09-25) - Crafting Expertise is lost when its item
//! leaves its equip slot, and `/equip` + `/unequip` refuse to do that
//! without `confirm_loss`. Real disposable `adventure_web` server, real
//! `reqwest` POSTs through the real `Form<EquipForm>`/`Form<UnequipForm>`
//! extractors.
//!
//! The confirmed POSTs are built by SCRAPING the rendered forms and
//! posting exactly their fields (`admin_tunables_splash_http.rs`'s
//! drift-guard shape), so the page and the handler cannot drift apart:
//! if the page stopped rendering `confirm_loss`, the confirmed swap
//! below would be refused and this test would fail.
//!
//! One `#[tokio::test]` - `set_data_dir` is a process-wide `OnceLock`
//! (see `unique_shard_picker_http.rs`).

use std::collections::HashMap;
use std::path::PathBuf;
use game::adventure::{AdventureManager, Character, UniqueAffix};

/// The `<form ...>...</form>` whose opening tag contains `action` and whose
/// body contains `marker`. Panics if there is none.
fn find_form<'a>(page: &'a str, action: &str, marker: &str) -> &'a str {
    let mut rest = page;
    while let Some(start) = rest.find("<form") {
        let form = &rest[start..];
        let end = form.find("</form>").expect("every form must be closed");
        let html = &form[..end];
        if html.split('>').next().is_some_and(|tag| tag.contains(action)) && html.contains(marker) {
            return html;
        }
        rest = &form[end..];
    }
    panic!("no form posting to {action} containing {marker}");
}

/// Every `name="..."`/`value="..."` pair of the form's hidden inputs,
/// exactly as rendered - the body a real browser posts.
fn hidden_fields(form: &str) -> Vec<(String, String)> {
    form.split("<input")
        .skip(1)
        .filter_map(|chunk| {
            let tag = chunk.split('>').next().unwrap_or(chunk);
            let name = tag.split("name=\"").nth(1)?.split('"').next()?;
            let value = tag.split("value=\"").nth(1)?.split('"').next()?;
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

#[tokio::test]
async fn crafting_expertise_is_stripped_on_unequip_and_the_server_refuses_it_without_confirm_loss() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("expertise_unequip_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    // Two players, one worn Expertise item each - the load-time
    // duplicate-unique cleanup would unequip a second one on the same
    // character. SWAPPER wears an Expertise helm with a plain copy in the
    // bag (picker + bag card); UNEQUIPPER wears an Expertise weapon.
    const SWAPPER: &str = "expertise-swapper";
    const UNEQUIPPER: &str = "expertise-unequipper";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"swap-token":{{"login":"{SWAPPER}","display_name":"Swapper","created_at":{now_secs}}},"unequip-token":{{"login":"{UNEQUIPPER}","display_name":"Unequipper","created_at":{now_secs}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let characters_path = scratch.join("adventure-characters.json");
    let mut swapper = Character::new("Swapper".to_string());
    let helm = swapper.helm.as_mut().expect("starter kit must equip a helm");
    helm.unique_affix = Some(UniqueAffix::CraftingExpertise);
    let helm_id = helm.id.clone();
    let mut spare_helm = helm.clone();
    spare_helm.id = format!("{helm_id}-spare");
    spare_helm.unique_affix = None;
    let spare_helm_id = spare_helm.id.clone();
    assert!(swapper.add_to_inventory(spare_helm), "the bag must accept the spare helm");
    let mut unequipper = Character::new("Unequipper".to_string());
    let weapon = unequipper.weapon.as_mut().expect("starter kit must equip a weapon");
    weapon.unique_affix = Some(UniqueAffix::CraftingExpertise);
    let weapon_id = weapon.id.clone();
    let weapon_name = weapon.display_name().replace('\'', "");

    let mut characters = HashMap::new();
    characters.insert(SWAPPER.to_string(), swapper);
    characters.insert(UNEQUIPPER.to_string(), unequipper);
    std::fs::write(&characters_path, serde_json::to_string(&characters).expect("must serialize")).expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(characters_path.clone(), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");
    const SWAP_COOKIE: &str = "adv_session=swap-token";
    const UNEQUIP_COOKIE: &str = "adv_session=unequip-token";

    let read_character = |login: &str| -> Character {
        let raw = std::fs::read_to_string(&characters_path).expect("failed to read characters file");
        let map: HashMap<String, Character> = serde_json::from_str(&raw).expect("failed to parse characters file");
        map.get(login).expect("test character must still exist").clone()
    };
    let inventory_page = |cookie: &'static str| {
        let client = client.clone();
        let url = format!("{base}/inventory");
        async move {
            let resp = client.get(url).header(reqwest::header::COOKIE, cookie).send().await.expect("GET /inventory failed");
            assert_eq!(resp.status(), reqwest::StatusCode::OK);
            resp.text().await.expect("failed to read /inventory body")
        }
    };
    let post = |cookie: &'static str, path: &'static str, body: Vec<(String, String)>| {
        let client = client.clone();
        let url = format!("{base}{path}");
        async move {
            let resp = client.post(url).header(reqwest::header::COOKIE, cookie).form(&body).send().await.expect("POST failed at the transport level");
            assert!(resp.status().is_redirection(), "{path} must extract and redirect, got {}", resp.status());
        }
    };
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());

    // --- The three forms render the confirm and the field ---
    let swap_page = inventory_page(SWAP_COOKIE).await;
    let unequip_page = inventory_page(UNEQUIP_COOKIE).await;
    let unequip_form = find_form(&unequip_page, "action=\"/unequip\"", "value=\"weapon\"");
    // The helm slot's picker is the only one (the bag holds only a helm).
    let picker_form = find_form(&swap_page, "class=\"equip-picker\"", &format!("value=\"{spare_helm_id}\""));
    let bag_equip_form = find_form(&swap_page, "action=\"/equip\"", &format!("name=\"item_id\" value=\"{spare_helm_id}\""));
    assert!(
        unequip_form.contains(&format!("return confirm('{weapon_name} will lose its Crafting Expertise unique affix for good")),
        "Unequip must confirm, naming the item:\n{unequip_form}"
    );
    for form in [unequip_form, picker_form, bag_equip_form] {
        assert!(form.contains("return confirm(") && form.contains("for good"), "every form that moves an Expertise item out must confirm:\n{form}");
        assert!(hidden_fields(form).iter().any(|(n, v)| n == "confirm_loss" && v == "true"), "the confirmed form must send confirm_loss:\n{form}");
    }

    // --- Without confirm_loss: refused, nothing changes ---
    let before = serde_json::to_string(&(read_character(SWAPPER), read_character(UNEQUIPPER))).unwrap();
    post(UNEQUIP_COOKIE, "/unequip", vec![pair("slot", "weapon")]).await;
    post(UNEQUIP_COOKIE, "/unequip", vec![pair("slot", "weapon"), pair("confirm_loss", "false")]).await;
    post(SWAP_COOKIE, "/equip", vec![pair("item_id", &spare_helm_id)]).await;
    let after = serde_json::to_string(&(read_character(SWAPPER), read_character(UNEQUIPPER))).unwrap();
    assert_eq!(after, before, "a POST without confirm_loss must change nothing");

    // --- Exactly the scraped Unequip form: proceeds and strips ---
    post(UNEQUIP_COOKIE, "/unequip", hidden_fields(unequip_form)).await;
    let c = read_character(UNEQUIPPER);
    assert!(c.weapon.is_none(), "the confirmed unequip must go through");
    let bagged = c.inventory.iter().find(|i| i.id == weapon_id).expect("the weapon must be in the bag");
    assert_eq!(bagged.unique_affix, None, "Crafting Expertise must be stripped when its item leaves the slot");

    // --- Exactly the scraped bag-card Equip form: swaps and strips the displaced helm ---
    post(SWAP_COOKIE, "/equip", hidden_fields(bag_equip_form)).await;
    let c = read_character(SWAPPER);
    assert_eq!(c.helm.as_ref().map(|i| i.id.as_str()), Some(spare_helm_id.as_str()), "the confirmed swap must go through");
    let displaced = c.inventory.iter().find(|i| i.id == helm_id).expect("the displaced helm must be in the bag");
    assert_eq!(displaced.unique_affix, None, "the displaced Expertise helm must lose the affix");

    // --- No affix: no confirm rendered, no field required ---
    let swap_page = inventory_page(SWAP_COOKIE).await;
    let plain_unequip = find_form(&swap_page, "action=\"/unequip\"", "value=\"helm\"");
    assert!(!plain_unequip.contains("confirm"), "a plain item must not get the confirm or the field:\n{plain_unequip}");
    post(SWAP_COOKIE, "/unequip", vec![pair("slot", "helm")]).await;
    assert!(read_character(SWAPPER).helm.is_none(), "unequipping a plain item needs no confirm_loss");
    post(UNEQUIP_COOKIE, "/equip", vec![pair("item_id", &weapon_id)]).await;
    assert_eq!(read_character(UNEQUIPPER).weapon.as_ref().map(|i| i.id.as_str()), Some(weapon_id.as_str()), "equipping into an empty slot needs no confirm_loss");

    std::fs::remove_dir_all(&scratch).ok();
}
