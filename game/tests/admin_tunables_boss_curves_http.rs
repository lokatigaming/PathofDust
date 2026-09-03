//! The seven boss-secondary half-stage dials on `/admin/tunables`, over
//! real HTTP (2026-09-03, design §10.4).
//!
//! The curve arithmetic itself is pinned in
//! `manager::boss_secondary_curve_tests`. What THIS file proves is the
//! operator half, which only exists over HTTP: the seven controls render
//! with their bounds and a hint that says what the number MEANS, a save
//! reaches the tunables `boss_stats_for` actually reads and survives a
//! restart, an out-of-range value cannot get through by bypassing the
//! browser, and an OMITTED field falls back to the shipped constant
//! rather than to 0.0.
//!
//! That last one is the reason this file is not optional. A `0.0`
//! half-stage is not merely a wrong number: `cap * s/(s + 0)` is the cap
//! from stage 1, so a silently-zeroed dial restores the frozen boss the
//! whole change exists to remove, on every stat, while the page reports a
//! successful save.
//!
//! One `#[tokio::test]`, deliberately: `adventure::set_data_dir` is a
//! process-wide `OnceLock`. Same disposable-instance setup as
//! `admin_tunables_craft_cost_http.rs`, and the same house rule about the
//! POST body: **the field set is derived from the rendered page, never
//! hand-written.** A hand-maintained superset can only ever catch drift in
//! one direction, and it is the other direction that shipped a dead Save
//! button in production.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Must match `manager::BOSS_SECONDARY_HALF_STAGE_MIN`/`_MAX`.
const HALF_STAGE_MIN: f64 = 1.0;
const HALF_STAGE_MAX: f64 = 10_000.0;

/// The seven dials as `(field, shipped default, a word its hint must
/// carry)`. The defaults are spelled out rather than imported on purpose:
/// they are each stat's old `cap / slope`, and if someone edits a
/// constant without meaning to change how bosses scale, this file fails
/// and says so. Note `crit_chance` is 0.70/0.012, not 0.75/0.012 — the
/// flat 0.05 base sits outside the ramp (the 2026-09-03 correction to
/// design §10.1).
const DIALS: [(&str, f64, &str); 7] = [
    ("boss_dr_half_stage", 150.0, "cap 0.75"),
    ("boss_block_half_stage", 75.0, "cap 0.75"),
    ("boss_evasion_half_stage", 50.0, "cap 0.75"),
    ("boss_increased_damage_half_stage", 50.0, "cap 0.50"),
    ("boss_crit_chance_half_stage", 0.70 / 0.012, "0.05"),
    ("boss_crit_mult_half_stage", 36.0, "1.4"),
    ("boss_splash_half_stage", 60.0, "cap 0.60"),
];

#[tokio::test]
async fn the_seven_boss_curve_dials_render_their_bounds_round_trip_and_never_default_to_zero() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_boss_curves_http_{}", std::process::id()));
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

    // --- a fresh instance ships the constants ---------------------------
    let t = manager.live_tunables();
    let live = |t: &game::adventure::LiveTunables, field: &str| -> f64 {
        match field {
            "boss_dr_half_stage" => t.boss_dr_half_stage,
            "boss_block_half_stage" => t.boss_block_half_stage,
            "boss_evasion_half_stage" => t.boss_evasion_half_stage,
            "boss_increased_damage_half_stage" => t.boss_increased_damage_half_stage,
            "boss_crit_chance_half_stage" => t.boss_crit_chance_half_stage,
            "boss_crit_mult_half_stage" => t.boss_crit_mult_half_stage,
            "boss_splash_half_stage" => t.boss_splash_half_stage,
            other => panic!("unknown dial {other}"),
        }
    };
    for (field, shipped, _) in DIALS {
        assert!((live(&t, field) - shipped).abs() < 1e-9, "a fresh install must ship {field} at {shipped}, got {}", live(&t, field));
    }

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

    // --- the section states the placement rule and its own provisionality
    assert!(form_html.contains("Boss Secondary Curves"), "the seven dials must render under their own heading");
    assert!(form_html.contains("50% of its cap"), "the section must state the placement rule - a half-stage means nothing without it");
    assert!(form_html.contains("PROVISIONAL"), "the section must say plainly that the shipped defaults are not a tuned set");
    assert!(form_html.contains("Known limitation"), "the section must carry the Controller-B re-pinning limitation - it is exactly when the unfreezing matters most");

    // --- every control carries its bounds and says what it MEANS ---------
    for (field, _, hint_word) in DIALS {
        let row = {
            let start = form_html.find(&format!("for=\"{field}\"")).unwrap_or_else(|| panic!("the {field} control must render inside the save form"));
            let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
            &form_html[start..end]
        };
        assert!(row.contains("type=\"number\""), "{field} must be a typed numeric input, not free text: {row}");
        assert!(row.contains(&format!("min=\"{}\"", trimmed(HALF_STAGE_MIN))), "{field} must carry its lower bound: {row}");
        assert!(row.contains(&format!("max=\"{}\"", trimmed(HALF_STAGE_MAX))), "{field} must carry its upper bound: {row}");
        assert!(row.contains("required"), "{field}: an empty submission must be rejected rather than read as 0: {row}");
        assert!(row.contains(hint_word), "the {field} hint must state which cap the curve climbs toward - a bare number field is banned: {row}");
    }

    // The field set comes off the rendered page, per the house rule.
    let rendered = rendered_fields(&form_html);
    for (field, _, _) in DIALS {
        assert!(rendered.iter().any(|(n, _)| n == field), "{field} must be inside the save form, not merely on the page");
    }

    let post = |body: Vec<(String, String)>, base: String, client: reqwest::Client| async move {
        client
            .post(format!("{base}/admin/tunables/save"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .form(&body)
            .send()
            .await
            .expect("POST failed")
    };

    // --- a save reaches the tunables `boss_stats_for` actually reads ------
    // Seven distinct values, so a wiring mistake that crosses two fields
    // cannot pass.
    let moved: Vec<(&str, &str)> = vec![
        ("boss_dr_half_stage", "310"),
        ("boss_block_half_stage", "320"),
        ("boss_evasion_half_stage", "330"),
        ("boss_increased_damage_half_stage", "340"),
        ("boss_crit_chance_half_stage", "350"),
        ("boss_crit_mult_half_stage", "360"),
        ("boss_splash_half_stage", "370"),
    ];
    let saved = post(body_with(&rendered, &moved), base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    let t = manager.live_tunables();
    for (field, want) in &moved {
        let want: f64 = want.parse().expect("numeric");
        assert_eq!(live(&t, field), want, "a save must move the value the boss ramp reads: {field}");
    }

    // A restart must not silently revert a boss-scaling dial.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the dials must persist to the live tunables file");
    for (field, want) in &moved {
        assert!(on_disk.contains(&format!("{field} = {want}")), "{field} must survive a restart - got: {on_disk}");
    }

    // --- out of range cannot get through by bypassing the browser ---------
    // Rejected with the live value left where it was, never clamped and
    // reported as saved (ledger #69).
    for (field, _, _) in DIALS {
        for attempt in ["0", "-1", "100000"] {
            let before = manager.live_tunables();
            let saved = post(body_with(&rendered, &[(field, attempt)]), base.clone(), client.clone()).await;
            assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "the out-of-range POST {field}={attempt} must be REJECTED, not clamped and reported as saved");
            let text = saved.text().await.expect("body");
            assert!(text.contains("NOT SAVED"), "the refusal must say plainly that nothing was written: {field}={attempt}");
            assert!(text.contains(field), "the refusal must name the offending field: {field}={attempt}");
            let after = manager.live_tunables();
            for (other, _, _) in DIALS {
                assert_eq!(live(&after, other), live(&before, other), "a rejected POST must leave every dial untouched: {field}={attempt} moved {other}");
            }
        }
    }

    // --- a body that omits the seven entirely preserves live behaviour ----
    // The both-directions form-field trap (see CLAUDE.md), and the case
    // this file exists for: `#[serde(default)]` on an `f64` is 0.0, and a
    // 0.0 half-stage pins the stat at its cap from stage 1.
    let without: Vec<(String, String)> =
        rendered.iter().filter(|(n, _)| !DIALS.iter().any(|(f, _, _)| f == n)).map(|(n, v)| (n.clone(), v.clone())).collect();
    assert_eq!(without.len(), rendered.len() - DIALS.len(), "sanity: exactly the seven dials were dropped from the body");
    let saved = post(without, base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "a body omitting the dials must still extract - got {}. A 422 here means the fields are required rather than defaulted", saved.status());
    let t = manager.live_tunables();
    for (field, shipped, _) in DIALS {
        assert!(
            (live(&t, field) - shipped).abs() < 1e-9,
            "an omitted {field} must fall back to the SHIPPED CONSTANT ({shipped}), never to 0.0 - a 0 half-stage pins the stat at its cap from stage 1 and silently restores the frozen boss. Got {}",
            live(&t, field)
        );
    }

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
/// every respect except the fields under test.
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
