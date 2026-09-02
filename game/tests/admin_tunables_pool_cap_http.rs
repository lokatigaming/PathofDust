//! The Enemy HP Pool Cap field on `/admin/tunables`, over real HTTP
//! (2026-08-30).
//!
//! This dial was `pacing::ENEMY_HP_POOL_HARD_CAP`, a compile-time
//! constant, until the measurement in anomaly ledger #67 established it
//! as the root cause of the pacing saturation: at 1e15 it binds on every
//! boss fight, cutting Controller A's honest request of ~186 down to
//! 13.35 and delivering 2.69 s fights against a 30-45 s target.
//!
//! **The default is deliberately unchanged at 1e15, so shipping the
//! tunable changes nothing about live play.** That safety property is
//! proven arithmetically in `pacing::tests` (see
//! `generation_is_bit_identical_at_the_default_cap`); what THIS file
//! proves is the operator half - that the control renders with its unit
//! and its bounds, that a raise actually reaches the tunables the game
//! reads and survives a restart, and that an out-of-range value cannot
//! get through by bypassing the browser.
//!
//! Same disposable-instance setup as `admin_tunables_rampage_http.rs`,
//! and the same house rule about the POST body: the field set is derived
//! from the rendered page, never hand-written.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Must match `pacing::ENEMY_HP_POOL_HARD_CAP` / `_CAP_MIN` / `_CAP_MAX`,
/// which are `pub(crate)`. Spelled out here on purpose: if someone edits
/// the constants without meaning to change live behaviour, this file
/// fails and says so.
const SHIPPED_DEFAULT: f64 = 1.0e15;
const CAP_MIN: f64 = 1.0e15;
const CAP_MAX: f64 = 5.0e16;

#[tokio::test]
async fn the_enemy_hp_pool_cap_renders_with_bounds_and_round_trips_through_a_real_form_post() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_pool_cap_http_{}", std::process::id()));
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
        manager.clone(),
        sessions_path,
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

    // --- THE SAFETY PROPERTY, as the operator sees it -------------------
    // A fresh instance must be running exactly the value production has
    // been running. If this fails, deploying the branch changed the game.
    assert_eq!(
        manager.live_tunables().enemy_hp_pool_hard_cap,
        SHIPPED_DEFAULT,
        "fresh tunables must equal the compile-time constant this replaced - anything else is a live behaviour change on deploy"
    );

    let admin_page = page(client.clone(), base.clone()).await;

    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };

    // --- the control carries its unit and its range ---------------------
    // The house standard for a numeric tunable is a typed input with
    // min/max (which is what actually reports an out-of-range value to
    // the operator, in the browser, before the POST is made) plus a hint
    // stating the unit. A bare unlabelled number field is banned.
    let row = {
        let start = form_html.find("for=\"enemy_hp_pool_hard_cap\"").expect("the Enemy HP Pool Cap control must render inside the save form");
        let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
        &form_html[start..end]
    };
    assert!(row.contains("type=\"number\""), "must be a typed numeric input, not free text: {row}");
    assert!(row.contains(&format!("min=\"{CAP_MIN}\"")), "the input must carry its lower bound so the browser rejects a low value visibly: {row}");
    assert!(row.contains(&format!("max=\"{CAP_MAX}\"")), "the input must carry its upper bound so the browser rejects a high value visibly: {row}");
    assert!(row.contains("required"), "an empty submission must be rejected rather than read as 0: {row}");
    assert!(row.to_lowercase().contains("hit points"), "the hint must state the UNIT - a bare number field is banned: {row}");
    assert!(row.contains(&format!("value=\"{SHIPPED_DEFAULT}\"")), "the field must render the live value back: {row}");

    // The field set comes off the rendered page, per the house rule - a
    // hand-written body can only catch drift in one direction, and it is
    // the other direction that shipped a dead Save button in production.
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.contains(&"enemy_hp_pool_hard_cap"), "the cap must be inside the save form, not merely on the page");

    // --- a raise reaches the tunables the game actually reads -----------
    let raised = "14000000000000000"; // 1.4e16 - the pool the 37.5s midpoint needs
    let body: Vec<(&str, &str)> = rendered.iter().map(|name| if *name == "enemy_hp_pool_hard_cap" { (*name, raised) } else { (*name, "1") }).collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    assert_eq!(manager.live_tunables().enemy_hp_pool_hard_cap, 1.4e16, "a save must move the value the generation path reads");

    let admin_page = page(client.clone(), base.clone()).await;
    assert!(admin_page.contains(&format!("value=\"{}\"", 1.4e16)), "the raised value must render back, or the operator cannot see the state they set");

    // A restart must not silently revert a difficulty dial.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the cap must persist to the live tunables file");
    assert!(on_disk.contains("enemy_hp_pool_hard_cap = 14000000000000000"), "the cap must survive a restart - got: {on_disk}");

    // --- out of range cannot get through by bypassing the browser -------
    // The form's min/max is what an operator sees. A hand-crafted POST has
    // no such gate. Until 2026-08-31 the handler CLAMPED such a POST and
    // answered `303 ?saved=1` - the guard held, but the page reported a
    // success while changing the operator's number (ledger #69). It now
    // REJECTS: 400, the field and its range named, and - the part that
    // matters most - the live value is left exactly where it was.
    let before = manager.live_tunables().enemy_hp_pool_hard_cap;
    assert_eq!(before, 1.4e16, "sanity: the raise above is what must survive every rejected POST below");
    for attempt in ["1000", "0", "-5", "999999999999999999999"] {
        let body: Vec<(&str, &str)> =
            rendered.iter().map(|name| if *name == "enemy_hp_pool_hard_cap" { (*name, attempt) } else { (*name, "1") }).collect();
        let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
        assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "the out-of-range POST {attempt} must be REJECTED, not clamped and reported as saved");
        let body = saved.text().await.expect("body");
        assert!(body.contains("NOT SAVED"), "the refusal must say plainly that nothing was written: {attempt}");
        assert!(body.contains("enemy_hp_pool_hard_cap"), "the refusal must name the offending field: {attempt}");
        assert!(
            body.contains(&format!("{}", CAP_MIN)) || body.contains("1000000000000000"),
            "the refusal must name the accepted range so the operator knows what to type: {attempt}"
        );
        assert_eq!(
            manager.live_tunables().enemy_hp_pool_hard_cap,
            before,
            "a rejected POST must leave the live value untouched - {attempt} must neither be stored nor clamped into place"
        );
    }

    // The clamp itself is NOT deleted, and is not dead code either:
    // `pacing::sanitize_pool_cap` is called on every generation read of the
    // cap (`capped_hp_mult_for_pool`), and is covered directly by
    // `pacing::tests::sanitize_pool_cap_*`. What changed here is only that
    // an operator is now TOLD when a bound fires, not that the bound went
    // away.
    assert!(CAP_MAX > CAP_MIN, "sanity: the range this test asserts against is the real one");

    // --- a body that omits the field entirely preserves live behaviour --
    // The both-directions form-field trap (see CLAUDE.md): an older
    // client, or any test still posting a pre-existing field set, must
    // neither 422 nor collapse the cap to `f64::default()` == 0.0.
    let without: Vec<(&str, &str)> = rendered.iter().filter(|name| **name != "enemy_hp_pool_hard_cap").map(|name| (*name, "1")).collect();
    assert_eq!(without.len(), rendered.len() - 1, "sanity: exactly the cap was dropped from the body");
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "a body omitting the cap must still extract - got {}. A 422 here means the field is required rather than defaulted", saved.status());
    assert_eq!(
        manager.live_tunables().enemy_hp_pool_hard_cap,
        SHIPPED_DEFAULT,
        "an omitted field must fall back to the SHIPPED CONSTANT, not to 0.0 - a serde default of 0.0 would be clamped up to the floor and quietly discard an operator's raise"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
