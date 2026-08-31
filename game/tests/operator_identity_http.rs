//! `OPERATOR_LOGIN` (2026-08-28) over real HTTP - the World 2 Stage 3
//! hard gate (docs/world2_build_plan.md, "HARD GATE - operator lockout").
//!
//! The three operator gates - `ADMIN_TUNABLES_LOGIN`, `FIGHTS_PAGE_LOGIN`
//! and `BUNDLE_OPERATOR_LOGIN` - were hardcoded to `lokati_gaming`, a name
//! only Twitch OAuth could ever mint a session for. This binary proves all
//! three follow the `OPERATOR_LOGIN` env key instead, so the operator can
//! be pointed at a local account BEFORE Twitch is removed.
//!
//! **The unset case is deliberately not tested here.** Six existing test
//! binaries (`admin_tunables_splash_http.rs`, `admin_passives_http.rs`,
//! `admin_tunables_rampage_http.rs`, `admin_ops_next_encounter_http.rs`,
//! `local_accounts_http.rs`, `replay_bundle_tiers_http.rs`) set no env var
//! and assert against `lokati_gaming`. They ARE the no-configuration test,
//! and they must keep passing untouched - that is the whole "deploying this
//! changes nothing" claim.
//!
//! **Why the `lokati_gaming` registration refusal here is airtight.** That
//! name is reserved permanently in `accounts.rs`, independent of
//! `OPERATOR_LOGIN`, because World 2 starts with fresh characters and
//! invalidated sessions - the live-character and minted-session guards in
//! `do_register` that protect it today both stop applying. The reserved arm
//! of `username_rejection` ORs `RESERVED_USERNAMES` with the three operator
//! constants, so a bare "it was refused" would be ambiguous. It is not
//! ambiguous here: the three assertions below prove all three constants
//! hold `CONFIGURED_OPERATOR`, not `lokati_gaming`, so `RESERVED_USERNAMES`
//! is the only arm left that can fire. The session guard is excluded
//! separately by the message - it says "already taken", never "reserved",
//! and it runs after `username_rejection` regardless.
//!
//! The register POST field set is **scraped off the rendered form**, never
//! hard-coded (CLAUDE.md's form-drift rule).
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is a
//! process-wide `OnceLock`, and `OPERATOR_LOGIN` has to be set before the
//! first request touches the `LazyLock`s that read it.

use std::path::PathBuf;

use game::adventure::AdventureManager;

/// What `OPERATOR_LOGIN` is pointed at for this run - a plausible local
/// account, i.e. exactly what World 2's operator will be.
const CONFIGURED_OPERATOR: &str = "world2_operator";
/// The pre-`OPERATOR_LOGIN` hardcoded value, and the owner's public handle.
/// Must stay in step with `adventure_web::DEFAULT_OPERATOR_LOGIN`, which is
/// private.
const OLD_OPERATOR: &str = "lokati_gaming";

const PASSWORD: &str = "correct horse battery";

/// Pulls the `name="..."` attributes out of the form posting to `action`,
/// in document order. Copied from `local_accounts_http.rs` - the POST below
/// sends exactly these and nothing else.
fn form_field_names(html: &str, action: &str) -> Vec<String> {
    let form_start = html.find(&format!("action=\"{action}\"")).unwrap_or_else(|| panic!("no form posting to {action} in: {html}"));
    let form = &html[form_start..];
    let form = &form[..form.find("</form>").expect("the form must be closed")];
    let mut names = Vec::new();
    let mut rest = form;
    while let Some(at) = rest.find("<input ") {
        rest = &rest[at..];
        let tag = &rest[..rest.find('>').expect("an input tag must be closed")];
        if let Some(name_at) = tag.find("name=\"") {
            let value = &tag[name_at + 6..];
            names.push(value[..value.find('"').expect("an unterminated name attribute")].to_string());
        }
        rest = &rest[1..];
    }
    names
}

/// A bundle with every member present, written the way the real writer
/// writes one. Copied from `replay_bundle_tiers_http.rs`; only the
/// operator-only `rolls` member is read below.
fn bundle_json() -> String {
    r#"{"manifest":{"schemaVersion":1,"minReaderVersion":1,"fightId":"0000000001","startedAtUnixMs":1755690000000,"realDurationMs":2800,"displayDurationMs":6000,"pinned":false,"members":{}},"members":{"core":{"participants":["Kazesosa"],"stage":2056},"replay":[{"seq":0,"kind":"defeat","atMs":1,"unit":"kazesosa"}],"playerVitals":[{"id":"kazesosa","hpSamples":[[0,100]]}],"buffs":[{"seq":1,"kind":"shield","atMs":2,"healer":"kazesosa","target":"kazesosa","amount":5}],"dot":[{"seq":2,"kind":"attack","atMs":3,"attacker":"kazesosa","target":"__enemy_0__","damage":1,"targetHpAfter":0,"sourceKind":"dot"}],"rolls":[{"eventId":1,"hitId":1,"atMs":0,"category":"crit","source":"Crit chance","actor":"kazesosa"}]}}"#
        .to_string()
}

#[tokio::test]
async fn operator_login_moves_all_three_gates_and_lokati_gaming_stays_reserved() {
    // Must happen before anything touches the operator `LazyLock`s.
    std::env::set_var("OPERATOR_LOGIN", CONFIGURED_OPERATOR);

    // Integration tests run with the PACKAGE dir as CWD, but the template
    // loader resolves "templates/" against the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("operator_identity_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let _ = std::fs::remove_file(scratch.join("adventure-accounts.json"));

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"op-token":{{"login":"{CONFIGURED_OPERATOR}","display_name":"World2Operator","created_at":{now}}},"old-token":{{"login":"{OLD_OPERATOR}","display_name":"Lokati","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let bundle_dir = scratch.join("adventure-fights-bundle");
    std::fs::create_dir_all(&bundle_dir).expect("failed to create the bundle tier dir");
    std::fs::write(bundle_dir.join("fight-0000000001.json"), bundle_json()).expect("failed to seed a bundle");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    // Neither operator name gets a character - the World 2 condition for
    // `OLD_OPERATOR`, and the reason the refusal below can only be the
    // permanent reserved list.
    assert!(manager.character(OLD_OPERATOR).await.is_none(), "no {OLD_OPERATOR} character may exist for this test to mean anything");

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

    let get_as = |path: &'static str, token: &'static str| {
        let client = client.clone();
        let base = base.clone();
        async move { client.get(format!("{base}{path}")).header(reqwest::header::COOKIE, format!("adv_session={token}")).send().await.expect("GET failed") }
    };

    // --- ADMIN_TUNABLES_LOGIN ----------------------------------------
    // The gate's refusal is a generic "Not Found" card - the body hides
    // that a restricted page lives here. Since 2026-08-31 it also carries
    // a real 404, so both halves are asserted: before then the status was
    // 200 for everyone and only the body discriminated.
    for path in ["/admin/tunables", "/admin/passives"] {
        let configured = get_as(path, "op-token").await;
        assert_eq!(configured.status(), reqwest::StatusCode::OK, "{path} must admit the configured operator with a 200");
        let configured = configured.text().await.expect("body");
        assert!(!configured.contains("<h1>Not Found</h1>"), "{path} must admit the configured operator");

        let old = get_as(path, "old-token").await;
        assert_eq!(old.status(), reqwest::StatusCode::NOT_FOUND, "{path} must refuse {OLD_OPERATOR} with a real 404 once OPERATOR_LOGIN points elsewhere");
        let old = old.text().await.expect("body");
        assert!(old.contains("<h1>Not Found</h1>"), "{path} must refuse {OLD_OPERATOR} once OPERATOR_LOGIN points elsewhere");
    }

    // --- FIGHTS_PAGE_LOGIN -------------------------------------------
    // The unfiltered list and the scoped one render different empty-state
    // copy, which is the cheapest honest discriminator for this gate.
    let configured_fights = get_as("/fights", "op-token").await.text().await.expect("body");
    assert!(configured_fights.contains("No fights logged yet."), "the configured operator must get the unfiltered fights list");
    let old_fights = get_as("/fights", "old-token").await.text().await.expect("body");
    assert!(old_fights.contains("You haven't been in any recently logged fights yet."), "{OLD_OPERATOR} must fall back to the scoped fights list");

    // --- BUNDLE_OPERATOR_LOGIN ---------------------------------------
    // `rolls` is the operator-only member; denial is a 404, not a 403, so
    // a refusal never confirms the member exists.
    let configured_rolls = get_as("/fights/1/members/rolls", "op-token").await;
    assert_eq!(configured_rolls.status(), reqwest::StatusCode::OK, "the configured operator must read the operator-tier bundle member");
    let old_rolls = get_as("/fights/1/members/rolls", "old-token").await;
    assert_eq!(old_rolls.status(), reqwest::StatusCode::NOT_FOUND, "{OLD_OPERATOR} must not read operator-tier bundle members any more");

    // --- the reserved-name guard --------------------------------------
    let register_html = client.get(format!("{base}/account/register")).send().await.expect("GET failed").text().await.expect("body");
    let register_fields = form_field_names(&register_html, "/account/register");
    assert_eq!(register_fields, vec!["username".to_string(), "password".to_string()], "the register form's rendered inputs are the contract");

    let register = |username: String| {
        let client = client.clone();
        let base = base.clone();
        let fields = register_fields.clone();
        async move {
            // Built by walking the SCRAPED names, so an input the page
            // renders but this test does not fill is a hard failure rather
            // than a silently-missing field.
            let body: Vec<(String, String)> = fields
                .iter()
                .map(|name| match name.as_str() {
                    "username" => (name.clone(), username.clone()),
                    "password" => (name.clone(), PASSWORD.to_string()),
                    other => panic!("the form grew an input this test does not fill: {other}"),
                })
                .collect();
            let resp = client.post(format!("{base}/account/register")).form(&body).send().await.expect("POST failed");
            (resp.status(), resp.text().await.expect("body"))
        }
    };

    // The configured operator name is reserved because it IS the operator
    // login - the guard reads the same constants the gates do, so it can
    // never drift from them.
    let (status, body) = register(CONFIGURED_OPERATOR.to_string()).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "a player must not be able to register the configured operator name");
    assert!(body.contains("reserved"), "the configured operator name must be refused as reserved, got: {body}");

    // Case-insensitively, since the guard compares that way.
    let (status, _) = register("World2_Operator".to_string()).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "the operator name must be reserved case-insensitively");

    // ...and the owner's handle stays reserved even though OPERATOR_LOGIN
    // points somewhere else and no character or session guard covers it.
    // "reserved", not "already taken" - see this file's own doc for why
    // that distinction is the proof.
    let (status, body) = register(OLD_OPERATOR.to_string()).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{OLD_OPERATOR} must stay reserved regardless of OPERATOR_LOGIN");
    assert!(body.contains("reserved"), "{OLD_OPERATOR} must be refused by the permanent reserved list, not by a session or character collision: {body}");

    // An ordinary name is still registerable - the guard did not just start
    // refusing everything.
    let (status, _) = register("ordinary_player".to_string()).await;
    assert_eq!(status, reqwest::StatusCode::FOUND, "an ordinary name must still register");

    let _ = std::fs::remove_dir_all(&scratch);
}
