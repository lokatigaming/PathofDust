//! Every `LiveTunables` field must render on exactly one admin page
//! (2026-09-03).
//!
//! **This is the load-bearing guard, and it is the ONLY thing that can catch
//! the failure it is aimed at.** Since the passive dials split onto their own
//! form, every field is required on exactly one form — so a rendered field set
//! that no longer matches its extractor still 422s loudly, the 2026-08-23
//! tripwire. But a field on *neither* page 422s nothing: both handlers merge
//! into `previous` and write the whole struct, so an unrendered dial is
//! silently preserved forever at whatever it happens to be, with nothing on
//! either page to say it exists. No HTTP status can report that. Only this
//! comparison can.
//!
//! **Both sides are derived, neither is hand-written.**
//!
//! * What exists: `serde_json::to_value(LiveTunables)`'s keys, straight from
//!   the struct's own `Serialize` derive. Add a field to `LiveTunables` and it
//!   appears here with no edit to this file.
//! * What renders: the `name="…"` attributes scraped out of the two real
//!   pages, fetched over real HTTP.
//!
//! A hand-maintained list on either side would have exactly the rot this
//! project keeps digging out: it stops covering new members and never says so.
//!
//! Same disposable-instance setup as the other admin HTTP tests: an
//! OS-assigned ephemeral port and a scratch data directory, so nothing here
//! can reach the live game's files or ports.

use game::adventure::AdventureManager;
use std::path::PathBuf;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const ADMIN_LOGIN: &str = "lokati_gaming";

/// Deliberately on no page. See `LiveTunables::dynamic_scaling_mult`'s doc:
/// retired with the dynamic-pacing release, read by no active code path, kept
/// declared only so existing `adventure-live-tunables.toml` files keep
/// deserializing. Re-rendering it would re-arm a switch taken out of service
/// after an incident.
///
/// Anything ADDED to this list is a decision someone has to defend in writing.
const DELIBERATELY_UNRENDERED: &[&str] = &["dynamic_scaling_mult"];

fn rendered_names(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in html.split("name=\"").skip(1) {
        if let Some(name) = piece.split('"').next() {
            if !out.contains(&name.to_string()) {
                out.push(name.to_string());
            }
        }
    }
    out
}

#[tokio::test]
async fn every_live_tunable_renders_on_exactly_one_admin_page() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_tunables_coverage_http_{}", std::process::id()));
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

    let fetch = |path: &'static str| {
        let client = client.clone();
        let base = base.clone();
        async move {
            client
                .get(format!("{base}{path}"))
                .header(reqwest::header::COOKIE, "adv_session=admin-token")
                .send()
                .await
                .unwrap_or_else(|e| panic!("GET {path} failed: {e}"))
                .text()
                .await
                .expect("body must read")
        }
    };

    let tunables_page = fetch("/admin/tunables").await;
    let passives_page = fetch("/admin/passives").await;

    let on_tunables = rendered_names(&tunables_page);
    let on_passives = rendered_names(&passives_page);

    // --- what exists, derived from the struct itself --------------------
    let live = manager.live_tunables();
    let all = match serde_json::to_value(&live).expect("LiveTunables must serialise") {
        serde_json::Value::Object(map) => map,
        other => panic!("LiveTunables must serialise to a JSON object, got {other:?}"),
    };
    assert!(all.len() > 60, "sanity: LiveTunables has many fields, found {}", all.len());

    // --- 1. nothing may be lost -----------------------------------------
    let mut unrendered: Vec<&String> = Vec::new();
    for key in all.keys() {
        if DELIBERATELY_UNRENDERED.contains(&key.as_str()) {
            continue;
        }
        if !on_tunables.contains(key) && !on_passives.contains(key) {
            unrendered.push(key);
        }
    }
    assert!(
        unrendered.is_empty(),
        "these LiveTunables fields render on NEITHER /admin/tunables nor /admin/passives, so nothing can ever change them and nothing announces it: {unrendered:?}\n\
         File each into a group in render_tunables_page, or onto the passives page via passive_tunables_fields_html. \
         If one is genuinely retired, say so in DELIBERATELY_UNRENDERED here and in its own doc comment."
    );

    // --- 2. and nothing may be written by two forms ----------------------
    // A field on both pages is written by two handlers, which is a lost
    // update: whichever page is saved second reverts the other's edit.
    //
    // Scoped to actual `LiveTunables` fields: both pages share nav and page
    // chrome (`<meta name="viewport">` among it), and those legitimately
    // appear twice. Deriving the filter from the struct rather than listing
    // the chrome keeps this from needing an exclusion list that would rot.
    let both: Vec<&String> = on_tunables.iter().filter(|n| on_passives.contains(n) && all.contains_key(n.as_str())).collect();
    assert!(both.is_empty(), "these fields render on BOTH admin pages, so two forms can write them and the later save silently reverts the earlier: {both:?}");

    // --- 3. the retired dial stays retired -------------------------------
    for key in DELIBERATELY_UNRENDERED {
        assert!(
            !on_tunables.contains(&key.to_string()) && !on_passives.contains(&key.to_string()),
            "{key} is retired and must not be rendered - giving it an input re-arms a dial that was deliberately taken out of service"
        );
    }

    // The mirror check - "every rendered input is a real field" - is
    // deliberately NOT here. `admin_tunables_splash_http.rs` already covers it
    // and covers it harder: it derives the POST body from the rendered page
    // and posts exactly that, so a typo'd `name=` 422s the extraction. Adding
    // a second, weaker version here would mean maintaining a growing list of
    // page chrome and non-tunable forms to exclude - the same hand-maintained
    // enumeration this whole mechanism exists to avoid.

    // --- 4. the Ungrouped catch-all is currently empty --------------------
    // Not a style check: if this fires, checks 1-3 still passed (the field IS
    // rendered) but it is rendered with no label, no hint and no range, in the
    // section that exists to make that visible. File it into a group.
    assert!(
        !tunables_page.contains("<h2>Ungrouped</h2>"),
        "the Ungrouped section is rendering, which means a LiveTunables field is not filed into any group in render_tunables_page. \
         It is visible and editable (that is the section's whole job) but it has no label, hint or range - file it."
    );

    std::fs::remove_dir_all(&scratch).ok();
}
