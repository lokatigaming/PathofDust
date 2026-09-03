//! The five Experience fields on `/admin/tunables`, over real HTTP
//! (2026-09-02).
//!
//! Win XP replaced a hardcoded `(5 + stage) * catchup` grant. Every knob
//! in the new curve is a `LiveTunable`, per the standing order that
//! things like this "should have been tunables already", and this file
//! proves the operator half of that: that each control renders with its
//! unit and its bounds, that a save reaches the tunables the grant
//! actually reads and survives a restart, that an out-of-range value
//! cannot get through by bypassing the browser, and - the part this
//! project has been bitten by twice - that a body which OMITS a field
//! falls back to the shipped constant rather than to 0.0.
//!
//! That last property is not cosmetic here. `win_xp_flat`,
//! `win_xp_level_pct` and `win_xp_mult` all zeroing to `f64::default()`
//! would stop every XP grant in the game while the page still reported
//! "Saved".
//!
//! Same disposable-instance setup as `admin_tunables_pool_cap_http.rs`,
//! and the same house rule about the POST body: the field set is derived
//! from the rendered page, never hand-written. A hand-maintained superset
//! can only catch drift in one direction, and it is the other direction -
//! a field the page stopped rendering - that shipped a dead Save button
//! in production once already.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Must match `adventure::WIN_XP_*`. Spelled out here on purpose: if
/// someone edits the constants without meaning to change live
/// progression, this file fails and says so.
const SHIPPED_FLAT: f64 = 12.0;
const SHIPPED_LEVEL_PCT: f64 = 1.0 / 48.0;
const SHIPPED_MULT: f64 = 1.0;
const SHIPPED_COOLDOWN_SECS: u64 = 450;
const FLAT_MAX: f64 = 10_000.0;
const LEVEL_PCT_MAX: f64 = 1.0;
const MULT_MIN: f64 = 0.0;
const MULT_MAX: f64 = 100.0;
const COOLDOWN_SECS_MAX: u64 = 3_600;
/// Must match `adventure::CATCHUP_FULL_DEFICIT*` (2026-09-03). Spelled
/// out for the same reason as the four above: the catch-up taper is a
/// live progression dial, and changing the constant without meaning to
/// must fail here rather than in the game.
const SHIPPED_FULL_DEFICIT: f64 = 0.5;
const FULL_DEFICIT_MIN: f64 = 0.01;
const FULL_DEFICIT_MAX: f64 = 1.0;

#[tokio::test]
async fn the_win_xp_dials_render_with_bounds_and_round_trip_through_a_real_form_post() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_win_xp_http_{}", std::process::id()));
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
    // A fresh instance must be running exactly the curve that was
    // approved. If this fails, deploying the branch changed the game.
    let fresh = manager.live_tunables();
    assert_eq!(fresh.win_xp_flat, SHIPPED_FLAT, "fresh tunables must carry the approved flat grant");
    assert_eq!(fresh.win_xp_level_pct, SHIPPED_LEVEL_PCT, "fresh tunables must carry the approved level fraction (1/48 = 2 levels/day at 96 wins/day)");
    assert_eq!(fresh.win_xp_mult, SHIPPED_MULT, "the global XP multiplier must ship neutral");
    assert_eq!(fresh.win_xp_cooldown_secs, SHIPPED_COOLDOWN_SECS, "the rampage guard must ship armed");
    assert!(fresh.win_xp_catchup_enabled, "catch-up on XP must ship ON - it predates this feature");
    assert_eq!(fresh.catchup_full_deficit, SHIPPED_FULL_DEFICIT, "the catch-up taper must ship at the approved half-the-leader's-level threshold");

    let admin_page = page(client.clone(), base.clone()).await;

    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        admin_page[start..end].to_string()
    };

    let row_for = |field: &str| -> String {
        let start = form_html.find(&format!("for=\"{field}\"")).unwrap_or_else(|| panic!("the {field} control must render inside the save form"));
        let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
        form_html[start..end].to_string()
    };

    // --- every control carries its unit and its range --------------------
    // The house standard for a numeric tunable is a typed input with
    // min/max (which is what actually reports an out-of-range value to the
    // operator, in the browser, before the POST is made) plus a hint
    // stating the unit. A bare unlabelled number field is banned.
    for (field, min, max, unit, value) in [
        ("win_xp_flat", "0".to_string(), FLAT_MAX.to_string(), "unit: raw xp", SHIPPED_FLAT.to_string()),
        ("win_xp_level_pct", "0".to_string(), LEVEL_PCT_MAX.to_string(), "fraction of the level", SHIPPED_LEVEL_PCT.to_string()),
        ("win_xp_mult", MULT_MIN.to_string(), MULT_MAX.to_string(), "unit: multiplier", SHIPPED_MULT.to_string()),
        ("win_xp_cooldown_secs", "0".to_string(), COOLDOWN_SECS_MAX.to_string(), "unit: seconds", SHIPPED_COOLDOWN_SECS.to_string()),
        ("catchup_full_deficit", FULL_DEFICIT_MIN.to_string(), FULL_DEFICIT_MAX.to_string(), "fraction of the leader", SHIPPED_FULL_DEFICIT.to_string()),
    ] {
        let row = row_for(field);
        assert!(row.contains("type=\"number\""), "{field} must be a typed numeric input, not free text: {row}");
        assert!(row.contains(&format!("min=\"{min}\"")), "{field} must carry its lower bound so the browser rejects a low value visibly: {row}");
        assert!(row.contains(&format!("max=\"{max}\"")), "{field} must carry its upper bound so the browser rejects a high value visibly: {row}");
        assert!(row.contains("required"), "{field}: an empty submission must be rejected rather than read as 0: {row}");
        assert!(row.to_lowercase().contains(unit), "{field}: the hint must state the UNIT - a bare number field is banned: {row}");
        assert!(row.contains(&format!("value=\"{value}\"")), "{field} must render the live value back: {row}");
    }

    // The multiplier's hint must say where in the order of operations it
    // lands and which currencies it does NOT touch. "Multiplier" is an
    // overloaded word on this page - loot_mult, sand_mult and the
    // catch-up multiplier are all in play - and an operator who guesses
    // wrong changes the wrong number.
    let mult_row = row_for("win_xp_mult");
    assert!(mult_row.contains("after catch-up"), "the multiplier hint must state that it applies AFTER catch-up: {mult_row}");
    assert!(
        mult_row.to_lowercase().contains("changes nothing about its shape"),
        "the multiplier hint must say it leaves the curve's shape alone - that is the whole point of it: {mult_row}"
    );
    assert!(
        mult_row.contains("Loot Multiplier") && mult_row.contains("Sand Multiplier"),
        "the multiplier hint must name the neighbouring multipliers it is NOT: {mult_row}"
    );

    // The catch-up switch is a checkbox, so it has no min/max/required -
    // but it must still be present, checked, and explained.
    let checkbox_at = form_html.find("name=\"win_xp_catchup_enabled\"").expect("the catch-up switch must be inside the save form");
    let checkbox = &form_html[checkbox_at..(checkbox_at + 120).min(form_html.len())];
    assert!(checkbox.contains(" checked"), "catch-up ships ON, so the box must render checked: {checkbox}");

    // The field set comes off the rendered page, per the house rule.
    let mut rendered: Vec<&str> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted");
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    for field in ["win_xp_flat", "win_xp_level_pct", "win_xp_mult", "win_xp_cooldown_secs", "win_xp_catchup_enabled", "catchup_full_deficit"] {
        assert!(rendered.contains(&field), "{field} must be inside the save form, not merely on the page");
    }

    // Every field this test is not exercising gets a filler value that is
    // legal for that field. "1" is legal for all but one: the Enemy HP
    // Pool Cap has a floor of 1e15, so a blanket "1" would be rejected
    // for a reason that has nothing to do with win XP and would mask
    // whatever this test is actually asserting.
    let filler = |name: &str| if name == "enemy_hp_pool_hard_cap" { "1000000000000000" } else { "1" };

    // --- a save reaches the tunables the grant actually reads -----------
    let edited: Vec<(&str, &str)> = rendered
        .iter()
        .map(|name| match *name {
            "win_xp_flat" => (*name, "20"),
            "win_xp_level_pct" => (*name, "0.05"),
            "win_xp_mult" => (*name, "2.5"),
            "win_xp_cooldown_secs" => (*name, "600"),
            "catchup_full_deficit" => (*name, "0.25"),
            other => (other, filler(other)),
        })
        .collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&edited).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    let live = manager.live_tunables();
    assert_eq!(live.win_xp_flat, 20.0, "a save must move the value the grant reads");
    assert_eq!(live.win_xp_level_pct, 0.05, "a save must move the level fraction");
    assert_eq!(live.win_xp_mult, 2.5, "a save must move the global multiplier");
    assert_eq!(live.win_xp_cooldown_secs, 600, "a save must move the rampage guard");
    assert_eq!(live.catchup_full_deficit, 0.25, "a save must move the catch-up taper the multiplier reads");

    let admin_page = page(client.clone(), base.clone()).await;
    assert!(admin_page.contains("value=\"20\""), "the saved flat grant must render back, or the operator cannot see the state they set");
    assert!(admin_page.contains("value=\"2.5\""), "the saved multiplier must render back");

    // A restart must not silently revert a progression dial.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the win-xp dials must persist to the live tunables file");
    for expected in ["win_xp_flat = 20.0", "win_xp_level_pct = 0.05", "win_xp_mult = 2.5", "win_xp_cooldown_secs = 600", "catchup_full_deficit = 0.25"] {
        assert!(on_disk.contains(expected), "the dials must survive a restart - missing {expected} in: {on_disk}");
    }

    // The checkbox's own round trip. It is the one field on this form
    // where ABSENT legitimately means false - a checkbox that is off
    // sends nothing at all - so it is asserted separately from the
    // omitted-field property below, and in the OPPOSITE direction: here
    // an omitted field must switch the setting off, not fall back to the
    // shipped constant. Nothing else can produce that absent state, which
    // is what makes the exception safe.
    assert!(manager.live_tunables().win_xp_catchup_enabled, "sanity: the save above carried the box, so catch-up must still be on");
    let without_box: Vec<(&str, &str)> = rendered.iter().filter(|name| **name != "win_xp_catchup_enabled").map(|name| (*name, filler(name))).collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without_box).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "unchecking the box must save - got {}", saved.status());
    assert!(!manager.live_tunables().win_xp_catchup_enabled, "a body with no checkbox must switch catch-up OFF - absent IS the protocol for a checkbox");
    let with_box: Vec<(&str, &str)> = rendered.iter().map(|name| (*name, filler(name))).collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&with_box).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "re-checking the box must save - got {}", saved.status());
    assert!(manager.live_tunables().win_xp_catchup_enabled, "a body carrying the checkbox must switch catch-up back ON");

    // --- out of range cannot get through by bypassing the browser -------
    // The form's min/max is what an operator sees. A hand-crafted POST has
    // no such gate. Rejection, not a silent clamp reported as "Saved"
    // (ledger #69): 400, the field and its range named, and the live value
    // left exactly where it was.
    for (field, attempts) in [
        ("win_xp_flat", vec!["-1", "10001"]),
        ("win_xp_level_pct", vec!["-0.5", "1.5"]),
        ("win_xp_mult", vec!["-1", "101"]),
        ("win_xp_cooldown_secs", vec!["3601"]),
        // 0 is NOT in range here, unlike on the two dials below. The
        // taper is a DIVISOR: a 0 would pay the full 3x to everyone
        // standing even one level below the leader, which is the
        // flat-global-multiplier defect this field exists to remove,
        // wearing a different hat. It is rejected, not clamped.
        ("catchup_full_deficit", vec!["0", "-0.5", "1.5"]),
    ] {
        for attempt in attempts {
            let before = manager.live_tunables();
            let body: Vec<(&str, &str)> = rendered.iter().map(|name| if *name == field { (*name, attempt) } else { (*name, filler(name)) }).collect();
            let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&body).send().await.expect("POST failed");
            assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "{field}: the out-of-range POST {attempt} must be REJECTED, not clamped and reported as saved");
            let text = saved.text().await.expect("body");
            assert!(text.contains("NOT SAVED"), "{field}: the refusal must say plainly that nothing was written: {attempt}");
            assert!(text.contains(field), "the refusal must name the offending field: {field} / {attempt}");
            let after = manager.live_tunables();
            assert_eq!(after.win_xp_flat, before.win_xp_flat, "{field}: a rejected POST must leave the live flat grant untouched");
            assert_eq!(after.win_xp_level_pct, before.win_xp_level_pct, "{field}: a rejected POST must leave the live level fraction untouched");
            assert_eq!(after.win_xp_mult, before.win_xp_mult, "{field}: a rejected POST must leave the live multiplier untouched");
            assert_eq!(after.win_xp_cooldown_secs, before.win_xp_cooldown_secs, "{field}: a rejected POST must leave the live cooldown untouched");
            assert_eq!(after.catchup_full_deficit, before.catchup_full_deficit, "{field}: a rejected POST must leave the live catch-up taper untouched");
        }
    }

    // 0 is IN range on the multiplier and on the cooldown, deliberately -
    // the multiplier is the end-of-season progression freeze and the
    // cooldown's 0 is "no throttle". Both must save rather than be
    // rejected as a suspected typo.
    let zeroes: Vec<(&str, &str)> = rendered
        .iter()
        .map(|name| match *name {
            "win_xp_mult" | "win_xp_cooldown_secs" => (*name, "0"),
            other => (other, filler(other)),
        })
        .collect();
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&zeroes).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "0 must be an accepted value on the multiplier and the cooldown - got {}", saved.status());
    assert_eq!(manager.live_tunables().win_xp_mult, 0.0, "a deliberate 0 multiplier must be stored, not rejected");
    assert_eq!(manager.live_tunables().win_xp_cooldown_secs, 0, "a deliberate 0 cooldown must be stored, not rejected");

    // --- a body that omits the fields preserves live behaviour ----------
    // The both-directions form-field trap (see CLAUDE.md): an older
    // client, or any test still posting a pre-existing field set, must
    // neither 422 nor collapse these to `f64::default()` == 0.0. On these
    // three fields a 0.0 does not merely drift a number - it stops every
    // XP grant in the game while the page reports success.
    let numeric = ["win_xp_flat", "win_xp_level_pct", "win_xp_mult", "win_xp_cooldown_secs", "catchup_full_deficit"];
    let without: Vec<(&str, &str)> = rendered.iter().filter(|name| !numeric.contains(name)).map(|name| (*name, filler(name))).collect();
    assert_eq!(without.len(), rendered.len() - numeric.len(), "sanity: exactly the five numeric win-xp fields were dropped from the body");
    let saved = client.post(format!("{base}/admin/tunables/save")).header(reqwest::header::COOKIE, "adv_session=admin-token").form(&without).send().await.expect("POST failed");
    assert!(saved.status().is_redirection(), "a body omitting the win-xp fields must still extract - got {}. A 422 here means a field is required rather than defaulted", saved.status());
    let live = manager.live_tunables();
    assert_eq!(live.win_xp_flat, SHIPPED_FLAT, "an omitted flat grant must fall back to the SHIPPED CONSTANT, not to 0.0 - a 0 here stops all XP");
    assert_eq!(live.win_xp_level_pct, SHIPPED_LEVEL_PCT, "an omitted level fraction must fall back to the SHIPPED CONSTANT, not to 0.0");
    assert_eq!(live.win_xp_mult, SHIPPED_MULT, "an omitted multiplier must fall back to the SHIPPED CONSTANT, not to 0.0 - a 0 here stops all XP");
    assert_eq!(live.win_xp_cooldown_secs, SHIPPED_COOLDOWN_SECS, "an omitted cooldown must fall back to the SHIPPED CONSTANT, not to 0 - a 0 here disarms the rampage guard");
    assert_eq!(
        live.catchup_full_deficit, SHIPPED_FULL_DEFICIT,
        "an omitted catch-up taper must fall back to the SHIPPED CONSTANT, not to 0.0 - a 0 here is a DIVISOR of zero and would pay the full 3x to the whole pack"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
