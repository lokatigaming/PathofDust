//! The Permanent Rampage checkbox on `/admin/tunables`, over real HTTP
//! (2026-08-28).
//!
//! This control already existed - `LiveTunables::permanent_rampage`,
//! shipped 2026-08-16 - and Stage 2 of the standalone plan added no
//! rampage code at all once that was established. What it had no test
//! for was the round trip a browser actually performs, which for a
//! CHECKBOX is a different shape from every numeric field beside it: an
//! unchecked box posts NOTHING, so "turn it off" is proven by a body
//! that omits the field entirely, not by one that sends `0`.
//!
//! Same disposable-instance setup as `admin_tunables_splash_http.rs`,
//! and the same rule about the POST body: the field set is derived from
//! the rendered page, never hand-written.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

#[tokio::test]
async fn the_permanent_rampage_checkbox_round_trips_through_a_real_form_post() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_rampage_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        Some("test-client-id".to_string()),
        Some("test-client-secret".to_string()),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let page = |client: reqwest::Client, base: String| async move {
        client
            .get(format!("{base}/admin/tunables"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .send()
            .await
            .expect("GET /admin/tunables failed")
            .text()
            .await
            .expect("body must read")
    };

    assert!(!manager.live_tunables().permanent_rampage, "sanity: fresh tunables start at LiveTunables::default(), rampage off");
    let admin_page = page(client.clone(), base.clone()).await;
    assert!(admin_page.contains("name=\"permanent_rampage\""), "the Permanent Rampage checkbox must render on the page");
    assert!(!admin_page.contains("name=\"permanent_rampage\" value=\"1\" checked"), "an off toggle must not render as checked");

    // The field set comes off the rendered page, per the house rule - a
    // hand-written body can only catch drift in one direction, and it is
    // the other direction that shipped a dead Save button in production.
    // `<form>` boundaries matter here too: the operator control added
    // 2026-08-28 is a SEPARATE form deliberately placed after this one
    // closes (nested forms are invalid HTML, and this scrape stops at the
    // first `</form>`), so if it ever migrates inside, this assertion is
    // what notices.
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };
    assert!(!form_html.contains("/admin/ops/next-encounter"), "the operator control must stay OUTSIDE the tunables save form - nested forms are invalid and would drag its fields into every save");

    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.contains(&"permanent_rampage"), "the checkbox must be inside the save form, not merely on the page");

    // --- checked: the box is present in the body ------------------------
    let with_box: Vec<(&str, &str)> = rendered.iter().map(|name| (*name, "1")).collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&with_box).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    assert!(manager.live_tunables().permanent_rampage, "a save carrying permanent_rampage must turn it ON in the live tunables the game reads");

    let admin_page = page(client.clone(), base.clone()).await;
    assert!(admin_page.contains("name=\"permanent_rampage\" value=\"1\" checked"), "an on toggle must render back as checked, or the operator cannot see the state they set");

    // The toggle is the persisted half of rampage state (unlike the
    // bot's finite countdown, which is in-memory + its own file), so a
    // restart must not silently revert it.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the toggle must persist to the live tunables file");
    assert!(on_disk.contains("permanent_rampage = true"), "the toggle must survive a restart - got: {on_disk}");

    // --- unchecked: the box is ABSENT from the body ---------------------
    // This is the half a numeric field can never exercise. An unchecked
    // HTML checkbox sends no key at all, so `Option<String>` + `is_some()`
    // is the only thing standing between "turn rampage off" and "422, and
    // nothing changed".
    let without_box: Vec<(&str, &str)> = rendered.iter().filter(|name| **name != "permanent_rampage").map(|name| (*name, "1")).collect();
    assert_eq!(without_box.len(), rendered.len() - 1, "sanity: exactly the checkbox was dropped from the body");
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without_box).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "a body omitting the unchecked box must still extract - got {}. A 422 here means permanent_rampage is required rather than optional", saved.status());
    assert!(!manager.live_tunables().permanent_rampage, "a save with the box unchecked must turn rampage OFF, not leave it on");

    let _ = std::fs::remove_dir_all(&scratch);
}
