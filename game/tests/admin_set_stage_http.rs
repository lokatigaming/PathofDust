//! `/admin/tunables/save`'s world-stage override over real HTTP
//! (2026-08-23) - the operator control for a party parked on a stage it
//! cannot win.
//!
//! Everything here goes through a real `Form<TunablesForm>` extraction on
//! the SAME POST route the rest of the tunables page saves through, for
//! the reason this repo has now been bitten by twice: an in-crate call
//! into `do_save_tunables`'s Rust types sails straight past a mismatch
//! between the form's `<input name="...">` attributes and the struct
//! fields deserializing them. See `admin_tunables_splash_http.rs` for the
//! first direction of that trap (a required field with no rendered input)
//! and the dynamic-pacing note in CLAUDE.md for the second (a removed
//! input against a still-required field).
//!
//! So the POST body here is DERIVED FROM THE RENDERED PAGE - GET the
//! form, scrape its `name` attributes, post exactly those - never from a
//! hand-maintained list. A hand-written superset can only ever catch a
//! field it forgot to add; it can never catch a field the page stopped
//! rendering, which is the direction that actually shipped broken.
//!
//! Same disposable-instance setup as `admin_tunables_splash_http.rs`: an
//! OS-assigned ephemeral port and a scratch data directory, so nothing
//! here can reach the live game's files or ports.

use std::path::PathBuf;
use game::adventure::AdventureManager;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const OTHER_LOGIN: &str = "someone_else";

/// The party is standing on stage 90 having once reached 120 - the shape
/// of the production incident this control exists for.
const SEEDED_STAGE: u32 = 90;
const SEEDED_HIGH_WATER: u32 = 120;

/// Reads the scratch world file back as raw JSON. The controllers and
/// both sampling windows are private to the game crate, so the persisted
/// file is how an out-of-crate test proves a stage set left them alone -
/// and it proves the stronger thing anyway, since that file is what
/// survives a restart.
fn persisted_world(scratch: &std::path::Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(scratch.join("adventure-world.json")).expect("the world file must exist");
    serde_json::from_str(&raw).expect("the world file must be valid JSON")
}

async fn admin_page(client: &reqwest::Client, base: &str) -> String {
    client
        .get(format!("{base}/admin/tunables"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body")
}

/// Every `name="..."` on the tunables form, in render order, deduped.
/// This is the whole point of the file - see the module doc.
fn rendered_fields(html: &str) -> Vec<String> {
    let start = html.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
    let end = start + html[start..].find("</form>").expect("the tunables form must be closed");
    let form_html = &html[start..end];
    let mut names: Vec<String> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted").to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Exactly the fields the page renders, every one at a harmless "1"
/// except the stage override, which carries whatever this round is
/// testing. The two sibling override rows post blank: they share the
/// blank-means-leave-it-alone contract, and posting "1" into them would
/// move the very controllers this test asserts stay put - so this is also
/// exactly what a real save that touched only the stage row looks like.
fn body(names: &[String], stage_value: &str) -> Vec<(String, String)> {
    names
        .iter()
        .map(|name| {
            let value = match name.as_str() {
                "world_stage_override" => stage_value,
                "hp_pacing_mult_override" | "boss_power_mult_override" => "",
                _ => "1",
            };
            (name.clone(), value.to_string())
        })
        .collect()
}

async fn post(client: &reqwest::Client, base: &str, form: &[(String, String)], token: &str) -> reqwest::StatusCode {
    client
        .post(format!("{base}/admin/tunables/save"))
        .header(reqwest::header::COOKIE, format!("adv_session={token}"))
        .form(form)
        .send()
        .await
        .expect("POST failed")
        .status()
}

#[tokio::test]
async fn the_world_stage_override_sets_the_stage_over_real_http_and_touches_nothing_else() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under
    // the workspace suite), but the template loader resolves "templates/"
    // against CWD and that directory belongs to the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_set_stage_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}},"other-token":{{"login":"{OTHER_LOGIN}","display_name":"SomeoneElse","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    // Seed a world with DISTINCTIVE values on every axis the override
    // must not touch, so "unchanged" is a real assertion rather than a
    // comparison of defaults against defaults.
    std::fs::write(
        scratch.join("adventure-world.json"),
        format!(
            r#"{{"stage":{SEEDED_STAGE},"last_boss_kind":null,"boss_power_mult":0.62,"hp_pacing_mult":1.37,"recent_boss_outcomes":[true,false,true],"recent_win_dps":[11.0,22.0,33.0],"stage_high_water":{SEEDED_HIGH_WATER}}}"#
        ),
    )
    .expect("failed to seed the scratch world file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    // Sanity: the seed actually loaded. `AdventureManager::new` falls back
    // to `WorldState::default()` on an unreadable file, so without this a
    // malformed fixture would quietly turn every assertion below into a
    // test of a stage-0 world.
    assert_eq!(manager.stage_override_bounds().await, (SEEDED_STAGE, SEEDED_HIGH_WATER), "the seeded world must have loaded");

    let bound = game::adventure_web::start_adventure_web_server(
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

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // --- the control renders, and names its real bound -----------------
    let page = admin_page(&client, &base).await;
    assert!(page.contains("name=\"world_stage_override\""), "the stage override input must be on the tunables form");
    assert!(page.contains("World Stage Override"), "the row must be labelled for an operator, not just named for the parser");
    assert!(
        page.contains(&format!("stage {SEEDED_STAGE} now, max {SEEDED_HIGH_WATER}")),
        "the label must show where the party is and how high they may be set"
    );
    assert!(page.contains(&format!("Whole number, 1 to {SEEDED_HIGH_WATER}")), "the hint must name the validated range");

    // --- derive the POST body from the rendered form -------------------
    // This is the drift guard, and the new field is inside it: an
    // `<input>` the page renders but `TunablesForm` will not accept - or a
    // required struct field the page stopped rendering - 422s right here
    // instead of silently doing nothing on every real browser save.
    let names = rendered_fields(&page);
    assert!(names.iter().any(|n| n == "world_stage_override"), "the scraped field set must include the new field, got: {names:?}");

    // --- a blank submission must leave the stage exactly where it is ---
    let status = post(&client, &base, &body(&names, ""), "admin-token").await;
    assert!(
        status.is_redirection(),
        "posting exactly the {} fields the page renders must extract cleanly - got {status}. A 422 here means `TunablesForm` requires a field the form no longer renders (or renders one it does not accept)",
        names.len()
    );
    assert_eq!(
        manager.stage_override_bounds().await,
        (SEEDED_STAGE, SEEDED_HIGH_WATER),
        "a blank stage row must change nothing - this is the ordinary case on every unrelated save"
    );

    // --- a non-admin must not be able to move the stage ----------------
    let status = post(&client, &base, &body(&names, "3"), "other-token").await;
    assert!(status.is_redirection(), "the handler redirects regardless, to avoid confirming the page exists");
    assert_eq!(manager.stage_override_bounds().await, (SEEDED_STAGE, SEEDED_HIGH_WATER), "a non-admin POST must not move the stage");

    // --- every invalid value is refused, changing nothing --------------
    // Whole numbers only, at least 1, never past the high-water mark. The
    // non-numeric cases matter as much as the out-of-range one: they are
    // what a fat-fingered operator actually types. " " is blank after the
    // trim, and so is simply the no-op case again.
    for bad in ["0", "-3", "12.5", "twelve", "121", "999999", " ", "1e3"] {
        let status = post(&client, &base, &body(&names, bad), "admin-token").await;
        assert!(status.is_redirection(), "a refused stage value must not break the save - got {status} for {bad:?}");
        assert_eq!(manager.stage_override_bounds().await, (SEEDED_STAGE, SEEDED_HIGH_WATER), "{bad:?} must leave the stage untouched");
    }

    // Nothing so far should have moved the stage, so the world file still
    // holds the seeded controllers and windows.
    let before = persisted_world(&scratch);
    assert_eq!(before["stage"], SEEDED_STAGE, "the persisted stage must still be the seed");

    // --- the real thing ------------------------------------------------
    let status = post(&client, &base, &body(&names, "40"), "admin-token").await;
    assert!(status.is_redirection(), "a valid stage set must extract and redirect, got {status}");
    assert_eq!(manager.stage_override_bounds().await, (40, SEEDED_HIGH_WATER), "the stage moves to 40; the ceiling stays at the high-water mark");

    // --- and it did ONE job --------------------------------------------
    let after = persisted_world(&scratch);
    assert_eq!(after["stage"], 40, "the stage reached disk, so it survives a restart");
    assert_eq!(after["hp_pacing_mult"], before["hp_pacing_mult"], "Controller A's multiplier must be untouched by a stage set");
    assert_eq!(after["boss_power_mult"], before["boss_power_mult"], "Controller B's multiplier must be untouched by a stage set");
    assert_eq!(
        after["recent_boss_outcomes"], before["recent_boss_outcomes"],
        "Controller B's outcome window must be untouched - clearing it would drop the controller back into warmup"
    );
    assert_eq!(after["recent_win_dps"], before["recent_win_dps"], "Controller A's DPS sample window must be untouched - same reason");
    assert_eq!(after["stage_high_water"], SEEDED_HIGH_WATER, "an override must never ratchet up its own ceiling");

    // --- the page now renders the new stage, same bound -----------------
    let page = admin_page(&client, &base).await;
    assert!(page.contains(&format!("stage 40 now, max {SEEDED_HIGH_WATER}")), "the label must reflect the set stage on the next load");

    // --- walking back up to the mark is allowed ------------------------
    let status = post(&client, &base, &body(&names, &SEEDED_HIGH_WATER.to_string()), "admin-token").await;
    assert!(status.is_redirection());
    assert_eq!(
        manager.stage_override_bounds().await,
        (SEEDED_HIGH_WATER, SEEDED_HIGH_WATER),
        "the high-water mark is inclusive - an operator can put the party back where they got to"
    );

    std::fs::remove_dir_all(&scratch).ok();
}
