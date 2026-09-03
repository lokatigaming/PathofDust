//! The gear-tier weight dial and its measurement read-out on
//! `/admin/tunables`, over real HTTP (2026-09-03, Option C of the
//! undamped-power-loop pass).
//!
//! The arithmetic is pinned in `manager::gear_tier_excess_tests`. What
//! THIS file proves is the half that only exists over HTTP, and for this
//! release that half **is the deliverable**: at the shipped
//! `boss_gear_tier_weight = 0.0` the mechanism changes nothing about
//! play, so what ships is the read-out that lets the weight be chosen
//! from an observed distribution instead of guessed. A dial with no
//! visible distribution beside it is the thing this release exists to
//! avoid.
//!
//! It also covers the inverted form-field trap. Everywhere else in this
//! codebase a `#[serde(default)]` resolving to `0.0` is the defect — it
//! has shipped twice. **Here 0.0 is the correct shipped value**, so the
//! omitted-field assertion below is not "it must not be zero" but "it
//! must be the SHIPPED CONSTANT", which today happens to be zero and
//! must keep tracking the constant if that ever changes.
//!
//! One `#[tokio::test]`, deliberately: `adventure::set_data_dir` is a
//! process-wide `OnceLock`. Same disposable-instance setup as
//! `admin_tunables_craft_cost_http.rs`, and the same house rule about the
//! POST body: **the field set is derived from the rendered page, never
//! hand-written.**

use game::adventure::{AdventureManager, Character};
use std::collections::HashMap;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Must match `manager::BOSS_GEAR_TIER_WEIGHT` and its bounds. Spelled
/// out rather than imported on purpose: if someone edits the shipped
/// weight without meaning to change how every boss in the game scales,
/// this file fails and says so.
const SHIPPED_WEIGHT: f64 = 0.0;
const WEIGHT_MIN: f64 = 0.0;
const WEIGHT_MAX: f64 = 1.0;

#[tokio::test]
async fn the_gear_tier_weight_ships_at_zero_round_trips_and_renders_the_distribution_beside_itself() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_gear_tier_weight_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    // Two characters with a KNOWN excess, so the read-out's numbers can be
    // asserted rather than merely shown to exist: one whose gear sits at
    // its level (the Krangled steady state — zero excess), one carrying
    // +100 tiers over level on every slot.
    let mut characters: HashMap<String, Character> = HashMap::new();
    for (login, level, tier) in [("levelmatched", 50u32, 50u32), ("crafter", 50, 150)] {
        let mut c = Character::new(login.to_string());
        c.level = level;
        // `Character::equipped_mut` is `pub(crate)` and this is a separate
        // crate, so the starter kit's five slots are set through their
        // public fields. The four §8 slots start empty by ruling and stay
        // that way - the mean is over what is actually equipped.
        for item in [&mut c.weapon, &mut c.helm, &mut c.body, &mut c.gloves, &mut c.boots].into_iter().flatten() {
            item.tier = tier;
        }
        characters.insert(login.to_string(), c);
    }
    std::fs::write(scratch.join("adventure-characters.json"), serde_json::to_string(&characters).expect("must serialize"))
        .expect("failed to seed the scratch characters file");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // --- a fresh instance ships the constant, and it is ZERO -------------
    assert_eq!(
        manager.live_tunables().boss_gear_tier_weight,
        SHIPPED_WEIGHT,
        "a fresh install must ship the gear-tier weight at the shipped constant - 0 here is the intended no-op, not an unset field"
    );

    let admin_page = client
        .get(format!("{base}/admin/tunables"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body must read");
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        admin_page[start..end].to_string()
    };

    // --- THE DELIVERABLE: the distribution renders beside the dial -------
    // Seeded excesses are 0 and 100, so mean 50, median 50, max 100, with
    // 1 of 2 carrying any excess. Asserting the NUMBERS, not merely that
    // some text appeared - a read-out that is present but wrong is worse
    // than none, because the weight gets chosen from it.
    assert!(form_html.contains("Gear-tier excess"), "the excess distribution must render on the tunables page");
    for needle in ["mean 50.0", "median 50.0", "max 100.0", "1 of 2 carry any excess"] {
        assert!(form_html.contains(needle), "the read-out must report the live distribution - missing {needle:?}");
    }
    assert!(
        form_html.contains("measurement only and changes nothing"),
        "while the weight is 0 the read-out must say plainly that it is a measurement only, or an operator will read it as an active setting"
    );

    // --- the control carries its bounds and says what it MEANS -----------
    let row = {
        let start = form_html.find("for=\"boss_gear_tier_weight\"").expect("the boss_gear_tier_weight control must render inside the save form");
        let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
        &form_html[start..end]
    };
    assert!(row.contains("type=\"number\""), "the weight must be a typed numeric input, not free text: {row}");
    assert!(row.contains(&format!("min=\"{}\"", trimmed(WEIGHT_MIN))), "the weight must carry its lower bound: {row}");
    assert!(row.contains(&format!("max=\"{}\"", trimmed(WEIGHT_MAX))), "the weight must carry its upper bound: {row}");
    assert!(row.contains("required"), "an empty submission must be rejected rather than read as 0: {row}");
    assert!(row.contains("effective levels"), "the hint must state the dial's UNIT - a bare number field is banned: {row}");
    // The inverted-zero warning is the point of the hint here: a future
    // audit sweeping for the zero-defaulting defect must land on this and
    // be able to tell it is deliberate.
    assert!(
        row.contains("not an unset field"),
        "the hint must say explicitly that 0 is the intended setting here, because every other dial on this page treats a 0 as a bug: {row}"
    );

    // The field set comes off the rendered page, per the house rule.
    let rendered = rendered_fields(&form_html);
    assert!(rendered.iter().any(|(n, _)| n == "boss_gear_tier_weight"), "the weight must be inside the save form, not merely on the page");

    let post = |body: Vec<(String, String)>, base: String, client: reqwest::Client| async move {
        client
            .post(format!("{base}/admin/tunables/save"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .form(&body)
            .send()
            .await
            .expect("POST failed")
    };

    // --- a save reaches the tunables boss generation actually reads -------
    let saved = post(body_with(&rendered, &[("boss_gear_tier_weight", "0.5")]), base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    assert_eq!(manager.live_tunables().boss_gear_tier_weight, 0.5, "a save must move the value `effective_avg_level` reads");

    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the dial must persist to the live tunables file");
    assert!(on_disk.contains("boss_gear_tier_weight = 0.5"), "the weight must survive a restart - got: {on_disk}");

    // With the weight live, the read-out must stop claiming it changes
    // nothing and must report what it now adds (0.5 x mean 50 = 25).
    let admin_page = client
        .get(format!("{base}/admin/tunables"))
        .header(reqwest::header::COOKIE, "adv_session=admin-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body must read");
    assert!(admin_page.contains("+25.0 effective levels"), "with the weight live the read-out must state what it adds to the level bosses scale on");
    assert!(!admin_page.contains("measurement only and changes nothing"), "the measurement-only note must disappear once the weight is nonzero");

    // --- out of range cannot get through by bypassing the browser ---------
    for attempt in ["-0.1", "1.1", "50"] {
        let before = manager.live_tunables().boss_gear_tier_weight;
        let saved = post(body_with(&rendered, &[("boss_gear_tier_weight", attempt)]), base.clone(), client.clone()).await;
        assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "the out-of-range POST boss_gear_tier_weight={attempt} must be REJECTED, not clamped and reported as saved");
        let text = saved.text().await.expect("body");
        assert!(text.contains("NOT SAVED"), "the refusal must say plainly that nothing was written: {attempt}");
        assert!(text.contains("boss_gear_tier_weight"), "the refusal must name the offending field: {attempt}");
        assert_eq!(manager.live_tunables().boss_gear_tier_weight, before, "a rejected POST must leave the weight untouched: {attempt}");
    }

    // 0 is a LEGAL setting here and must be accepted - it is how an
    // operator switches the mechanism back off after trying it.
    let saved = post(body_with(&rendered, &[("boss_gear_tier_weight", "0")]), base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "0 must be ACCEPTED - unlike the crafting dials, it is this dial's shipped setting and its off switch");
    assert_eq!(manager.live_tunables().boss_gear_tier_weight, 0.0);

    // --- a body that omits the field preserves live behaviour -------------
    // The inverted trap: the assertion is "the SHIPPED CONSTANT", not
    // "not zero". Set the weight somewhere else first, so a fallback that
    // silently did nothing could not pass by accident.
    let saved = post(body_with(&rendered, &[("boss_gear_tier_weight", "0.75")]), base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection());
    assert_eq!(manager.live_tunables().boss_gear_tier_weight, 0.75, "sanity: the weight really is away from its shipped value before the omitted-field save");

    let without: Vec<(String, String)> = rendered.iter().filter(|(n, _)| n != "boss_gear_tier_weight").map(|(n, v)| (n.clone(), v.clone())).collect();
    assert_eq!(without.len(), rendered.len() - 1, "sanity: exactly the one dial was dropped from the body");
    let saved = post(without, base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "a body omitting the dial must still extract - got {}. A 422 here means the field is required rather than defaulted", saved.status());
    assert_eq!(
        manager.live_tunables().boss_gear_tier_weight,
        SHIPPED_WEIGHT,
        "an omitted weight must fall back to the SHIPPED CONSTANT - which is 0 today, but this asserts the constant, not the zero"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// `adventure_web::trim_float`'s output for a bound, which is what the
/// rendered `min`/`max` attributes carry.
fn trimmed(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_string() } else { s.to_string() }
}

/// Every `name="..."` in the rendered form, paired with the `value="..."`
/// in the same tag (falling back to "1" for a control that renders none) -
/// i.e. the body a browser would actually submit from an untouched page.
fn rendered_fields(form_html: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for piece in form_html.split("name=\"").skip(1) {
        let name = piece.split('"').next().expect("a name attribute must be quoted").to_string();
        if out.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let tag_end = piece.find('>').unwrap_or(piece.len());
        let value = attr(&piece[..tag_end], "value").unwrap_or_else(|| "1".to_string());
        out.push((name, value));
    }
    out
}

/// The rendered body with `overrides` applied - a real browser save in
/// every respect except the field under test.
fn body_with(rendered: &[(String, String)], overrides: &[(&str, &str)]) -> Vec<(String, String)> {
    rendered
        .iter()
        .map(|(n, v)| match overrides.iter().find(|(field, _)| field == n) {
            Some((_, replacement)) => (n.clone(), (*replacement).to_string()),
            None => (n.clone(), v.clone()),
        })
        .collect()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    Some(tag[start..].split('"').next()?.to_string())
}
