//! The argon2 concurrency bound on `/admin/tunables`, over real HTTP
//! (2026-09-05).
//!
//! `Argon2::default()` costs ~19 MiB and a CPU pass, and both entry
//! points are unauthenticated: `/account/login`'s verify and
//! `/account/register`'s hash. `spawn_blocking` moved that work off the
//! reactor but did not bound it - tokio's default `max_blocking_threads`
//! is 512, so concurrent argon2 could demand around 9.5 GiB. The
//! per-username login throttle cannot cover registration, because
//! registration flooding varies the username, which is the value that
//! throttle keys on.
//!
//! **The omitted-field property is why this file exists.** For most dials
//! a `#[serde(default)]` collapsing to `f64::default()` is a wrong
//! number. Here the field is a SEMAPHORE SIZE, so a 0 would not loosen
//! the bound - it would close it, and nobody could log in or register
//! again, with the page still reporting "Saved".
//!
//! Same disposable-instance setup and the same house rule about the POST
//! body as `admin_tunables_stage_gates_http.rs`: the field set is derived
//! from the rendered page, never hand-written.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Must match `adventure::PASSWORD_HASH_PERMITS*`. Spelled out here on
/// purpose: changing the shipped bound without meaning to should fail
/// here rather than in production.
const SHIPPED_PERMITS: u32 = 4;
const PERMITS_MIN: u32 = 1;
const PERMITS_MAX: u32 = 64;

const FIELD: &str = "password_hash_permits";

#[tokio::test]
async fn the_argon2_bound_renders_with_bounds_and_round_trips_through_a_real_form_post() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_password_hash_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");

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

    // A fresh instance must already be bounded. If this fails, deploying
    // the branch did not actually close the exposure.
    assert_eq!(manager.live_tunables().password_hash_permits, SHIPPED_PERMITS, "a fresh instance must ship the argon2 bound already applied");

    let admin_page = page(client.clone(), base.clone()).await;
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };

    let row = {
        let start = form_html.find(&format!("for=\"{FIELD}\"")).unwrap_or_else(|| panic!("the {FIELD} control must render inside the save form"));
        let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
        &form_html[start..end]
    };
    assert!(row.contains("type=\"number\""), "{FIELD} must be a typed numeric input, not free text: {row}");
    assert!(row.contains(&format!("min=\"{PERMITS_MIN}\"")), "{FIELD} must carry its lower bound - and it must be 1, not 0: {row}");
    assert!(row.contains(&format!("max=\"{PERMITS_MAX}\"")), "{FIELD} must carry its upper bound: {row}");
    assert!(row.contains("required"), "{FIELD}: an empty submission must be rejected rather than read as 0 - and 0 here locks everyone out: {row}");
    assert!(row.to_lowercase().contains("unit: simultaneous"), "{FIELD}: the hint must state the UNIT: {row}");
    assert!(row.contains(&format!("value=\"{SHIPPED_PERMITS}\"")), "{FIELD} must render the live value back: {row}");

    // The operator has to be told what happens to a caller at the bound,
    // because "queued" and "rejected" are different failures and the
    // choice here was deliberate.
    assert!(row.contains("try again"), "{FIELD}'s hint must say what a caller over the limit experiences - a bound whose behaviour is undocumented gets raised blindly the first time someone complains: {row}");

    // The field set comes off the rendered page, per the house rule.
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.contains(&FIELD), "{FIELD} must be inside the save form, not merely on the page");

    let filler = |name: &str| if name == "enemy_hp_pool_hard_cap" { "1000000000000000" } else { "1" };

    // --- a save reaches the tunable the semaphore is reconciled from ----
    let edited: Vec<(&str, &str)> = rendered.iter().map(|name| if *name == FIELD { (*name, "8") } else { (*name, filler(name)) }).collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&edited).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    assert_eq!(manager.live_tunables().password_hash_permits, 8, "a save must move the value the semaphore is reconciled from");

    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the bound must persist to the live tunables file");
    assert!(on_disk.contains("password_hash_permits = 8"), "the bound must survive a restart - missing it in: {on_disk}");

    // --- out of range cannot get through by bypassing the browser ------
    // 0 is REFUSED here, unlike the win-XP multiplier's deliberate 0. It
    // does not mean "no limit", it means "no logins".
    for attempt in ["0", "65"] {
        let before = manager.live_tunables();
        let body: Vec<(&str, &str)> = rendered.iter().map(|name| if *name == FIELD { (*name, attempt) } else { (*name, filler(name)) }).collect();
        let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
        assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "{FIELD}: the out-of-range POST {attempt} must be REJECTED, not clamped and reported as saved");
        let text = saved.text().await.expect("body");
        assert!(text.contains("NOT SAVED"), "{FIELD}: the refusal must say plainly that nothing was written: {attempt}");
        assert!(text.contains(FIELD), "the refusal must name the offending field: {FIELD} / {attempt}");
        assert_eq!(manager.live_tunables().password_hash_permits, before.password_hash_permits, "{FIELD}: a rejected POST must leave the live value untouched");
    }

    // --- THE ASSERTION THIS FILE EXISTS FOR ---------------------------
    // An older client, or any test still posting a pre-existing field
    // set, must neither 422 nor collapse this to 0. A 0 here does not
    // drift a number - it shuts every sign-in and registration in the
    // game, while the page reports success.
    let without: Vec<(&str, &str)> = rendered.iter().filter(|name| **name != FIELD).map(|name| (*name, filler(name))).collect();
    assert_eq!(without.len(), rendered.len() - 1, "sanity: exactly the argon2 bound was dropped from the body");
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "a body omitting the bound must still extract - got {}. A 422 here means the field is required rather than defaulted", saved.status());
    assert_eq!(
        manager.live_tunables().password_hash_permits,
        SHIPPED_PERMITS,
        "an omitted bound must fall back to the SHIPPED CONSTANT, not to 0 - a 0 is a closed semaphore, and every login and registration would fail with no error explaining why"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
