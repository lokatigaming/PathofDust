//! The two crafting-cost dials on `/admin/tunables`, and the price the
//! crafting panel quotes for them, over real HTTP (2026-09-02).
//!
//! Both were compile-time constants until an owner ruling cut every base
//! crafting cost by 10x and turned the flat `3 x tier` surcharge into
//! `3 x tier^1.1`. **Unlike the pool-cap conversion, the DEFAULTS here do
//! change live play** - that is the point of the change - so the
//! arithmetic itself is pinned in `craft::cost_curve_tests`, and what THIS
//! file proves is the two halves that only exist over HTTP:
//!
//! 1. the operator half - the controls render with their units and their
//!    bounds, a save reaches the tunables the game reads and survives a
//!    restart, an out-of-range value cannot get through by bypassing the
//!    browser, and an omitted field falls back to the shipped constant
//!    rather than to 0.0;
//! 2. the PREVIEW-vs-CHARGE half - the price the crafting panel puts on a
//!    button is exactly the dust `craft_item_ex` then deducts. The panel's
//!    per-tier arithmetic lives in JavaScript (`templates/base.html`),
//!    which until this change carried its own hardcoded copy of the
//!    multiplier; a preview that can drift from the charge is the defect
//!    this half exists to make impossible to reintroduce silently.
//!
//! One `#[tokio::test]`, deliberately: `adventure::set_data_dir` is a
//! process-wide `OnceLock`, so a second test function in this file would
//! race this one. Same disposable-instance setup as
//! `admin_tunables_pool_cap_http.rs`, and the same house rule about the
//! POST body: the field set is derived from the rendered page, never
//! hand-written.

use game::adventure::{AdventureManager, Character};
use std::collections::HashMap;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";
const PLAYER_LOGIN: &str = "craft-price-tester";

/// Must match `craft::CRAFT_BASE_COST_MULT` / `CRAFT_TIER_EXPONENT` and
/// their bounds. Spelled out here on purpose: if someone edits the
/// constants without meaning to change live prices, this file fails and
/// says so.
const SHIPPED_MULT: f64 = 0.1;
const MULT_MIN: f64 = 0.0;
const MULT_MAX: f64 = 10.0;
const SHIPPED_EXPONENT: f64 = 1.1;
const EXPONENT_MIN: f64 = 1.0;
const EXPONENT_MAX: f64 = 1.5;
/// Must match `craft::TIER_CRAFT_DUST_COST`.
const TIER_MULT: f64 = 3.0;
/// Must match `craft_action_def(Scour).default_cost`. Scour is the
/// action used for the end-to-end price check below: unlike Transmute it
/// has no exact-affix-count precondition, and every starter-kit item
/// already carries modifiers for it to strip.
const SCOUR_BASE: u64 = 250;

#[tokio::test]
async fn the_craft_cost_dials_round_trip_and_the_quoted_price_is_the_charged_price() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_craft_cost_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"admin-token":{{"login":"{ADMIN_LOGIN}","display_name":"Lokati","created_at":{now}}},"player-token":{{"login":"{PLAYER_LOGIN}","display_name":"CraftPriceTester","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    // Before anything that could touch `data_path` - even constructing a
    // `Character` reaches it transitively via item generation.
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    // `Character::new` hands out one free token of EVERY action, and a
    // token craft is free by design - so the panel would render "Free"
    // instead of a price and nothing would be charged. Zeroing them is
    // what puts this character on the dust path the dials actually price.
    let mut character = Character::new("CraftPriceTester".to_string());
    character.dust = 1_000_000;
    character.craft_tokens = Vec::new();
    let mut characters = HashMap::new();
    characters.insert(PLAYER_LOGIN.to_string(), character);
    std::fs::write(scratch.join("adventure-characters.json"), serde_json::to_string(&characters).expect("must serialize"))
        .expect("failed to seed the scratch characters file");

    // `AdventureManager::new` runs two one-time craft-token BACKFILLS,
    // each gated on its own marker file - in a fresh scratch dir both
    // fire and hand the seeded character a free token of every action
    // straight back. Pre-writing the markers is what makes "no tokens"
    // stick, and is the same trick as letting the real server skip a
    // grant it has already made.
    for marker in ["adventure-craft-token-backfill-marker.json", "adventure-craft-token-backfill-v2-marker.json"] {
        std::fs::write(scratch.join(marker), "true").expect("failed to seed a backfill marker");
    }

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path).await.expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let get = |client: reqwest::Client, url: String, token: &'static str| async move {
        client
            .get(url)
            .header(reqwest::header::COOKIE, format!("adv_session={token}"))
            .send()
            .await
            .expect("GET failed")
            .text()
            .await
            .expect("body must read")
    };

    // --- a fresh instance ships the constants ---------------------------
    let t = manager.live_tunables();
    assert_eq!(t.craft_base_cost_mult, SHIPPED_MULT, "a fresh install must price at the shipped multiplier");
    assert_eq!(t.craft_tier_exponent, SHIPPED_EXPONENT, "a fresh install must price at the shipped exponent");

    let admin_page = get(client.clone(), format!("{base}/admin/tunables"), "admin-token").await;
    let form_html = {
        let start = admin_page.find("action=\"/admin/tunables/save\"").expect("the tunables form must be on the page");
        let end = start + admin_page[start..].find("</form>").expect("the tunables form must be closed");
        admin_page[start..end].to_string()
    };

    // --- both controls carry their unit and their range ------------------
    for (field, min, max, unit_word) in [
        ("craft_base_cost_mult", MULT_MIN, MULT_MAX, "flat dust fee"),
        ("craft_tier_exponent", EXPONENT_MIN, EXPONENT_MAX, "dust per tier"),
    ] {
        let row = {
            let start = form_html.find(&format!("for=\"{field}\"")).unwrap_or_else(|| panic!("the {field} control must render inside the save form"));
            let end = start + form_html[start..].find("</div>").expect("the tunable row must be closed");
            &form_html[start..end]
        };
        assert!(row.contains("type=\"number\""), "{field} must be a typed numeric input, not free text: {row}");
        assert!(row.contains(&format!("min=\"{}\"", trimmed(min))), "{field} must carry its lower bound so the browser rejects a low value visibly: {row}");
        assert!(row.contains(&format!("max=\"{}\"", trimmed(max))), "{field} must carry its upper bound so the browser rejects a high value visibly: {row}");
        assert!(row.contains("required"), "{field}: an empty submission must be rejected rather than read as 0: {row}");
        assert!(row.contains(unit_word), "the {field} hint must state what the number MEANS - a bare number field is banned: {row}");
    }

    // The field set comes off the rendered page, per the house rule - a
    // hand-written body can only catch drift in one direction, and it is
    // the other direction that shipped a dead Save button in production.
    // Values come off the page too, so every OTHER field is posted at
    // whatever the page is currently showing: a body that is a real
    // browser save in every respect except the two fields under test.
    let rendered = rendered_fields(&form_html);
    for field in ["craft_base_cost_mult", "craft_tier_exponent"] {
        assert!(rendered.iter().any(|(n, _)| n == field), "{field} must be inside the save form, not merely on the page");
    }
    assert_eq!(
        rendered.iter().find(|(n, _)| n == "craft_base_cost_mult").map(|(_, v)| v.as_str()),
        Some("0.1"),
        "the field must render the live value back"
    );

    let post = |body: Vec<(String, String)>, base: String, client: reqwest::Client| async move {
        client
            .post(format!("{base}/admin/tunables/save"))
            .header(reqwest::header::COOKIE, "adv_session=admin-token")
            .form(&body)
            .send()
            .await
            .expect("POST failed")
    };

    // --- a save reaches the tunables the game actually reads --------------
    let body = body_with(&rendered, &[("craft_base_cost_mult", "0.5"), ("craft_tier_exponent", "1.25")]);
    let saved = post(body, base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "posting exactly the {} fields the page renders must extract cleanly - got {}", rendered.len(), saved.status());
    let t = manager.live_tunables();
    assert_eq!(t.craft_base_cost_mult, 0.5, "a save must move the value the cost formula reads");
    assert_eq!(t.craft_tier_exponent, 1.25, "a save must move the exponent the cost formula reads");

    // A restart must not silently revert a pricing dial.
    let on_disk = std::fs::read_to_string(scratch.join("adventure-live-tunables.toml")).expect("the dials must persist to the live tunables file");
    assert!(on_disk.contains("craft_base_cost_mult = 0.5"), "the multiplier must survive a restart - got: {on_disk}");
    assert!(on_disk.contains("craft_tier_exponent = 1.25"), "the exponent must survive a restart - got: {on_disk}");

    // --- out of range cannot get through by bypassing the browser ---------
    // The form's min/max is what an operator sees; a hand-crafted POST has
    // no such gate. It must be REJECTED with the live value left exactly
    // where it was - not clamped and reported as saved (ledger #69).
    for (field, attempt) in [
        ("craft_base_cost_mult", "-1"),
        ("craft_base_cost_mult", "1000"),
        // Sub-1 is the one an operator might genuinely try: it looks like
        // "make high tiers cheaper" and is refused on purpose, because it
        // makes crafting relatively cheaper the further a player gets.
        ("craft_tier_exponent", "0"),
        ("craft_tier_exponent", "0.9"),
        ("craft_tier_exponent", "3"),
    ] {
        let before = manager.live_tunables();
        let saved = post(body_with(&rendered, &[(field, attempt)]), base.clone(), client.clone()).await;
        assert_eq!(saved.status(), reqwest::StatusCode::BAD_REQUEST, "the out-of-range POST {field}={attempt} must be REJECTED, not clamped and reported as saved");
        let text = saved.text().await.expect("body");
        assert!(text.contains("NOT SAVED"), "the refusal must say plainly that nothing was written: {field}={attempt}");
        assert!(text.contains(field), "the refusal must name the offending field: {field}={attempt}");
        let after = manager.live_tunables();
        assert_eq!(after.craft_base_cost_mult, before.craft_base_cost_mult, "a rejected POST must leave the multiplier untouched: {field}={attempt}");
        assert_eq!(after.craft_tier_exponent, before.craft_tier_exponent, "a rejected POST must leave the exponent untouched: {field}={attempt}");
    }

    // --- a body that omits the fields entirely preserves live behaviour ---
    // The both-directions form-field trap (see CLAUDE.md). `#[serde(default)]`
    // on an `f64` is 0.0, which for the multiplier would silently make every
    // craft's base fee free and for the exponent is below its own floor.
    let without: Vec<(String, String)> =
        rendered.iter().filter(|(n, _)| n != "craft_base_cost_mult" && n != "craft_tier_exponent").map(|(n, v)| (n.clone(), v.clone())).collect();
    assert_eq!(without.len(), rendered.len() - 2, "sanity: exactly the two dials were dropped from the body");
    let saved = post(without, base.clone(), client.clone()).await;
    assert!(saved.status().is_redirection(), "a body omitting the dials must still extract - got {}. A 422 here means the fields are required rather than defaulted", saved.status());
    let t = manager.live_tunables();
    assert_eq!(t.craft_base_cost_mult, SHIPPED_MULT, "an omitted multiplier must fall back to the SHIPPED CONSTANT, never to 0.0 - 0.0 would make every base fee free");
    assert_eq!(t.craft_tier_exponent, SHIPPED_EXPONENT, "an omitted exponent must fall back to the SHIPPED CONSTANT, never to 0.0");

    // --- the quoted price IS the charged price ----------------------------
    // Back at the shipped defaults (the omitted-field save above restored
    // them), read the panel exactly as the browser's own preview script
    // does - `data-base` off the button, the per-tier surcharge off
    // `data-tier-mult`/`data-tier-exp` and the selected item's `data-tier` -
    // then actually submit the craft and check the dust that left the
    // character.
    let panel = get(client.clone(), format!("{base}/inventory"), "player-token").await;
    let btn = {
        let start = panel.find("value=\"scour\"").expect("the Scour button must render for a token-less character");
        let open = panel[..start].rfind("<button").expect("the button tag must open");
        let end = start + panel[start..].find('>').expect("the button tag must close");
        panel[open..end].to_string()
    };
    let quoted_base: u64 = attr(&btn, "data-base").unwrap_or_else(|| panic!("the button must carry its server-computed base fee: {btn}")).parse().expect("data-base must be an integer");
    let tier_mult: f64 = attr(&btn, "data-tier-mult").expect("the button must carry the per-tier multiplier as a parameter, not leave it hardcoded in the script").parse().expect("numeric");
    let tier_exp: f64 = attr(&btn, "data-tier-exp").expect("the button must carry the per-tier exponent as a parameter").parse().expect("numeric");
    assert_eq!(quoted_base, 25, "Scour's flat fee at the shipped 0.1 multiplier is 250 -> 25");
    assert_eq!(tier_mult, TIER_MULT);
    assert_eq!(tier_exp, SHIPPED_EXPONENT);

    // Scour needs an item with at least 1 modifier - the same
    // `data-affixes` attribute the preview script reads is what picks one.
    let (item_id, item_tier) = pick_modded_item(&panel).expect("the starter kit must contain at least one item with modifiers for Scour");

    // The browser's own arithmetic, transcribed: ceil per term, then sum.
    let quoted = quoted_base + (tier_mult * (item_tier as f64).powf(tier_exp)).ceil() as u64;
    assert_eq!(
        quoted,
        SCOUR_BASE / 10 + (TIER_MULT * (item_tier as f64).powf(SHIPPED_EXPONENT)).ceil() as u64,
        "sanity: the preview is the shipped formula"
    );

    let dust_before = dust_of(&scratch);
    let crafted = client
        .post(format!("{base}/craft"))
        .header(reqwest::header::COOKIE, "adv_session=player-token")
        .form(&[("action", "scour"), ("item_a", item_id.as_str()), ("item_b", "")])
        .send()
        .await
        .expect("POST /craft failed");
    assert!(crafted.status().is_redirection(), "the craft must go through - got {}", crafted.status());
    let dust_after = dust_of(&scratch);
    assert_eq!(
        dust_before - dust_after,
        quoted,
        "the crafting panel quoted {quoted} dust for a tier-{item_tier} Scour and the server charged {}. A preview that can drift from the charge is exactly the defect the data-tier-* attributes exist to prevent",
        dust_before - dust_after
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

/// The first `<option>` in the crafting panel with at least one
/// modifier, as `(item id, tier)` - Scour's own precondition.
fn pick_modded_item(panel: &str) -> Option<(String, u32)> {
    for piece in panel.split("<option ").skip(1) {
        let tag_end = piece.find('>')?;
        let tag = &piece[..tag_end];
        if attr(tag, "data-affixes").as_deref().is_none_or(|n| n == "0") {
            continue;
        }
        let id = attr(tag, "value")?;
        let tier: u32 = attr(tag, "data-tier")?.parse().ok()?;
        return Some((id, tier));
    }
    None
}

/// The seeded character's dust, straight off the persisted file -
/// `AdventureManager::characters` is private to the `adventure` module and
/// this is a separate crate, the same way `divine_dust_craft_http.rs`
/// reads its own results back.
fn dust_of(scratch: &std::path::Path) -> u64 {
    let raw = std::fs::read_to_string(scratch.join("adventure-characters.json")).expect("the characters file must exist");
    let characters: HashMap<String, Character> = serde_json::from_str(&raw).expect("the characters file must deserialize");
    characters.get(PLAYER_LOGIN).expect("the seeded character must still be there").dust
}
