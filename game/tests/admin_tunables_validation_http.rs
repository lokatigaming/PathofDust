//! The `/admin/tunables` validation pass (2026-08-31, ledger #69), over
//! real HTTP.
//!
//! Every numeric field on this page used to be clamped SILENTLY and the
//! page then answered `303 ?saved=1` - the operator was told the save
//! succeeded while the number they typed was changed underneath them.
//! Rejection existed only as the browser's own `min`/`max`, which 7 of the
//! 48 clamped fields did not even carry, and which any POST that is not
//! the page bypasses entirely.
//!
//! What this file proves, all against the REAL rendered field set:
//!
//! 1. **A valid save still saves.** The regression guard for everything
//!    below - if this breaks, the page is bricked.
//! 2. **Every violation is reported AT ONCE.** Six bad fields in one POST
//!    come back as six named violations, not one. The operator learns
//!    about every typo in a single round trip.
//! 3. **The refusal names the field and its accepted range**, and says
//!    plainly that nothing was written.
//! 4. **Nothing is written.** All or nothing - a partial save can never
//!    leave the tunables somewhere the operator did not ask for.
//! 5. **The other edits survive.** The page comes back with the values
//!    that were POSTed still in the inputs, so one bad number does not
//!    discard the other 65 fields.
//! 6. **The four fields with NO HTML5 `min`** (`loot_mult`, `sand_mult`,
//!    `boss_health`, `boss_power`) and the three with no `max` against a
//!    server ceiling of 100 (`*_step_per_fight`) are covered - these were
//!    fully invisible before, the browser never fired on them either.
//! 7. **The three anchor lists REJECT rather than empty.** A malformed
//!    entry used to blank the whole hand-authored pacing baseline floor
//!    while the page said "Saved" - data destruction, not a clamp. Blank
//!    stays a legitimate "no anchors".
//!
//! House rule: the POST body is derived from the rendered page, never
//! hand-written.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
/// Must match `pacing::ENEMY_HP_POOL_CAP_MIN`, which is `pub(crate)`.
const POOL_CAP_OK: &str = "1000000000000000";

#[tokio::test]
async fn out_of_range_tunables_are_rejected_with_every_offending_field_named() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_validation_http_{}", std::process::id()));
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

    let admin_page = client
        .get(format!("{base}/admin/tunables"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET /admin/tunables failed")
        .text()
        .await
        .expect("body must read");

    // The field set comes off the rendered page, per the house rule.
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        &admin_page[start..end]
    };
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    assert!(rendered.len() > 40, "sanity: this page renders dozens of fields, scraped {}", rendered.len());

    /// A body that is valid everywhere except the overrides applied on top.
    fn body<'a>(rendered: &[&'a str], overrides: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        rendered
            .iter()
            .map(|name| {
                let base = if *name == "enemy_hp_pool_hard_cap" { POOL_CAP_OK } else { "1" };
                let value = overrides.iter().find(|(field, _)| field == name).map(|(_, v)| *v).unwrap_or(base);
                (*name, value)
            })
            .collect()
    }

    let post = |body: Vec<(&str, &str)>| {
        let client = client.clone();
        let base = base.clone();
        let owned: Vec<(String, String)> = body.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        async move {
            let resp = client
                .post(format!("{base}/admin/tunables/save"))
                .header(reqwest::header::COOKIE, "adv_session=admin-token")
                .form(&owned)
                .send()
                .await
                .expect("POST failed");
            (resp.status(), resp.text().await.expect("body"))
        }
    };

    // --- 1. a valid save still saves ------------------------------------
    let (status, _) = post(body(&rendered, &[("loot_mult", "2.5")])).await;
    assert_eq!(status, reqwest::StatusCode::SEE_OTHER, "a fully in-range save must still redirect - anything else means the page is bricked");
    assert_eq!(manager.live_tunables().loot_mult, 2.5, "and must reach the tunables the game reads");

    let saved_state = manager.live_tunables();

    // --- 2/3/4. every violation at once, named, and nothing written -----
    // Six fields, deliberately spanning every helper: a two-sided clamp, a
    // one-sided float floor, an integer floor, the pool cap's sanitiser,
    // and both no-HTML5-constraint groups.
    let bad = [
        ("loot_mult", "-3"),                    // no `min` rendered at all
        ("boss_health", "-1"),                  // no `min` rendered at all
        ("hp_max_step_per_fight", "250"),       // no `max` rendered against a server ceiling of 100
        ("dmg_max_step_per_fight", "999"),      // ditto
        ("wings_drop_chance", "5"),             // two-sided 0-1
        ("pacing_window_fights", "0"),          // integer floor of 1
    ];
    let (status, page) = post(body(&rendered, &bad)).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "an out-of-range save must be REJECTED, not clamped and reported as saved");
    assert!(page.contains("NOT SAVED"), "the refusal must say plainly that nothing was written");
    assert!(page.contains("6 values were rejected"), "every offending field must be reported AT ONCE, not one per save: {page}");
    for (field, _) in bad {
        assert!(page.contains(field), "the refusal must name the offending field {field}");
    }
    // The accepted range, not just the fact of refusal.
    assert!(page.contains("hp_max_step_per_fight must be between 0 and 100"), "the refusal must name the accepted range");
    assert!(page.contains("pacing_window_fights must be 1 or more"), "an integer floor must be reported with its bound too");

    let after = manager.live_tunables();
    assert_eq!(after.loot_mult, saved_state.loot_mult, "a rejected save must write NOTHING - not even the fields that were in range");
    assert_eq!(after.wings_drop_chance, saved_state.wings_drop_chance, "all or nothing");
    assert_eq!(after.pacing_window_fights, saved_state.pacing_window_fights, "all or nothing");

    // --- 5. the operator's other edits survive the round trip -----------
    // One bad field alongside a good edit: the good edit must come back in
    // the box, not be discarded. This is why the page re-renders from the
    // POST rather than sending the operator back to live values.
    let (status, page) = post(body(&rendered, &[("wings_drop_chance", "5"), ("sand_mult", "7.25")])).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert!(page.contains("value=\"7.25\""), "the in-range edit the operator also made must still be in its box, or one bad number discards the whole form");
    assert_ne!(manager.live_tunables().sand_mult, 7.25, "...and must NOT have been written, since the save was refused");

    // --- 6. NaN / infinity cannot slip past ------------------------------
    for hostile in ["NaN", "inf", "-inf"] {
        let (status, _) = post(body(&rendered, &[("boss_power", hostile)])).await;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "a non-finite {hostile} must be rejected, never stored");
    }

    // --- 7. the anchor lists reject rather than empty --------------------
    // This is the most dangerous item in the whole set: a malformed entry
    // used to blank the entire hand-authored pacing baseline floor while
    // the page reported success.
    let (status, _) = post(body(&rendered, &[("baseline_stage_anchors", "0, 500, 1000"), ("baseline_hp_anchors", "1.0, 0.92, 0.82")])).await;
    assert_eq!(status, reqwest::StatusCode::SEE_OTHER, "a well-formed anchor list must save");
    assert_eq!(manager.live_tunables().baseline_stage_anchors, vec![0, 500, 1000], "and must reach the tunables");
    let anchors_before = manager.live_tunables().baseline_stage_anchors.clone();

    for (field, malformed) in [("baseline_stage_anchors", "0, 5OO, 1000"), ("baseline_hp_anchors", "1.0, oops, 0.82"), ("baseline_atk_anchors", "1.0, , 0.82")] {
        let (status, page) = post(body(&rendered, &[(field, malformed)])).await;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "a malformed {field} must be REJECTED, never silently emptied");
        assert!(page.contains(field), "the refusal must name {field}");
        assert!(page.contains("comma-separated list"), "and must say what the field accepts");
        assert_eq!(
            manager.live_tunables().baseline_stage_anchors,
            anchors_before,
            "a malformed list must leave the hand-authored baseline floor intact - emptying it is data destruction, not a clamp"
        );
        // ...and what was typed comes back for editing, since a malformed
        // list cannot round-trip through the parsed Vec.
        assert!(page.contains(malformed), "the malformed text must be echoed back into its box so it can be corrected: {malformed}");
    }

    // Blank stays a legitimate "no anchors" - it is how the list is cleared.
    let (status, _) = post(body(&rendered, &[("baseline_stage_anchors", ""), ("baseline_hp_anchors", ""), ("baseline_atk_anchors", "")])).await;
    assert_eq!(status, reqwest::StatusCode::SEE_OTHER, "a blank anchor list must remain the way to clear it, not a rejection");
    assert!(manager.live_tunables().baseline_stage_anchors.is_empty(), "blank must clear the list");

    let _ = std::fs::remove_dir_all(&scratch);
}
