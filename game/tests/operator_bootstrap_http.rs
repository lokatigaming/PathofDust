//! `OPERATOR_BOOTSTRAP` (2026-08-31) over real HTTP - the fix for the
//! operator chicken-and-egg.
//!
//! `OPERATOR_LOGIN` is reserved by `username_rejection`, so on a FRESH
//! deployment the operator cannot register the very account the three
//! operator gates point at. Standing up the Linux staging instance needed
//! a temporary source patch to get past it; World 2 launch day would hit
//! the same wall. `OPERATOR_BOOTSTRAP` is the operator's own opt-in, and
//! it carries the LOGIN rather than a boolean, so it permits exactly one
//! name and a stale variable permits nothing.
//!
//! Four things are proven here, in one process:
//!
//! 1. Bootstrap set to the WRONG name releases nothing - the operator
//!    login is still refused. This is the assertion that makes the
//!    variable value-carrying rather than a boolean, and it is what makes
//!    a stale `OPERATOR_BOOTSTRAP` harmless after `OPERATOR_LOGIN` moves.
//! 2. Bootstrap set to the CURRENT `OPERATOR_LOGIN` registers it.
//! 3. `lokati_gaming` is refused THROUGHOUT, including while bootstrap
//!    names it - permanently reserved is permanent (see
//!    `RESERVED_USERNAMES`' own doc), and the refusal is EXPLANATORY so a
//!    future operator does not have to read the source to find out why
//!    bootstrap did nothing.
//! 4. Bootstrap does not disable any OTHER guard: a second registration
//!    of the same name is still refused as taken.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is a
//! process-wide `OnceLock`, `OPERATOR_LOGIN` has to be set before the
//! first request touches the `LazyLock`s that read it, and
//! `OPERATOR_BOOTSTRAP` is process-global env state that steps between
//! phases.
//!
//! The register POST field set is **scraped off the rendered form**, never
//! hard-coded (CLAUDE.md's form-drift rule).

use std::path::PathBuf;

use game::adventure::AdventureManager;

/// What `OPERATOR_LOGIN` is pointed at for this run - a plausible local
/// account, i.e. exactly what World 2's operator will be.
const CONFIGURED_OPERATOR: &str = "world2_operator";
/// The owner's public handle, permanently reserved in `accounts.rs`
/// regardless of `OPERATOR_LOGIN` or `OPERATOR_BOOTSTRAP`.
const PERMANENTLY_RESERVED: &str = "lokati_gaming";
/// A name bootstrap is pointed at while the operator login is what is
/// actually being registered - the stale-variable case.
const WRONG_NAME: &str = "someone_else";

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

#[tokio::test]
async fn operator_bootstrap_releases_only_the_current_operator_login() {
    // Must happen before anything touches the operator `LazyLock`s.
    std::env::set_var("OPERATOR_LOGIN", CONFIGURED_OPERATOR);
    std::env::remove_var("OPERATOR_BOOTSTRAP");

    // Integration tests run with the PACKAGE dir as CWD, but the template
    // loader resolves "templates/" against the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("operator_bootstrap_http_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, "{}").expect("failed to seed the scratch sessions file");
    let accounts_path = scratch.join("adventure-accounts.json");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    // A fresh deployment: no characters, no sessions, no accounts. This is
    // exactly the World 2 launch-day condition, and the reason the refusals
    // below can only come from the reservation arms.
    assert!(manager.character(CONFIGURED_OPERATOR).await.is_none(), "no {CONFIGURED_OPERATOR} character may exist for this test to mean anything");
    assert!(!accounts_path.exists(), "sanity: a fresh deployment has no account store");

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        manager.clone(),
        sessions_path,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    let page = client.get(format!("{base}/account/register")).send().await.expect("GET failed");
    let register_html = page.text().await.expect("body");
    let fields = form_field_names(&register_html, "/account/register");
    assert_eq!(fields, vec!["username".to_string(), "password".to_string()], "the register form's rendered inputs are the contract");

    let register = |username: String| {
        let client = client.clone();
        let base = base.clone();
        let fields = fields.clone();
        async move {
            let body: Vec<(String, String)> = fields
                .iter()
                .map(|name| match name.as_str() {
                    "username" => (name.clone(), username.clone()),
                    "password" => (name.clone(), PASSWORD.to_string()),
                    other => panic!("the register form grew an input this test does not know how to fill: {other}"),
                })
                .collect();
            let resp = client.post(format!("{base}/account/register")).form(&body).send().await.expect("POST failed");
            (resp.status(), resp.text().await.expect("body"))
        }
    };

    // --- 1. bootstrap UNSET: the chicken-and-egg, still there ----------
    let (_, body) = register(CONFIGURED_OPERATOR.to_string()).await;
    assert!(body.contains("That username is reserved."), "with OPERATOR_BOOTSTRAP unset the operator login must still be refused - that is the bug this variable exists to open a door in, not to remove");
    assert!(!accounts_path.exists(), "nothing may be registered yet");

    // --- 2. bootstrap set to the WRONG name: releases nothing ----------
    // This is what makes the variable value-carrying rather than a
    // boolean: left set after OPERATOR_LOGIN moves, it permits nothing.
    std::env::set_var("OPERATOR_BOOTSTRAP", WRONG_NAME);
    let (_, body) = register(CONFIGURED_OPERATOR.to_string()).await;
    assert!(body.contains("That username is reserved."), "OPERATOR_BOOTSTRAP naming a DIFFERENT login must not release the operator login - a stale variable must be inert");
    assert!(!accounts_path.exists(), "still nothing registered");

    // --- 3. bootstrap set to a PERMANENTLY reserved name ---------------
    // `lokati_gaming` is unconditional. The refusal must SAY so, and say
    // what to do instead, or a future operator on a fresh deployment has
    // to read the source to find out why bootstrap did nothing.
    std::env::set_var("OPERATOR_BOOTSTRAP", PERMANENTLY_RESERVED);
    let (_, body) = register(PERMANENTLY_RESERVED.to_string()).await;
    assert!(body.contains("permanently reserved"), "the refusal must say the name is permanently reserved, not just 'reserved': got {body}");
    assert!(body.contains("OPERATOR_BOOTSTRAP cannot release it"), "the refusal must name the variable that did not work");
    assert!(body.contains("OPERATOR_LOGIN"), "the refusal must point at the fix - OPERATOR_LOGIN should name the operator's own account");
    assert!(!accounts_path.exists(), "still nothing registered");

    // ...and it stays refused with bootstrap pointed at the operator too.
    std::env::set_var("OPERATOR_BOOTSTRAP", CONFIGURED_OPERATOR);
    let (_, body) = register(PERMANENTLY_RESERVED.to_string()).await;
    assert!(body.contains("reserved"), "{PERMANENTLY_RESERVED} is refused regardless of what bootstrap names");
    assert!(!accounts_path.exists(), "still nothing registered");

    // --- 4. bootstrap set to the CURRENT operator login: it works ------
    // Case-insensitively, the way every other name check on this form is.
    let (status, _) = register(CONFIGURED_OPERATOR.to_uppercase()).await;
    assert_eq!(status, reqwest::StatusCode::FOUND, "with OPERATOR_BOOTSTRAP naming the current OPERATOR_LOGIN, the operator must be able to register their own account");
    let stored = std::fs::read_to_string(&accounts_path).expect("the operator account must have been persisted");
    assert!(stored.contains(CONFIGURED_OPERATOR), "the account must be keyed by the lowercased operator login: {stored}");

    // --- 5. no OTHER guard was disabled --------------------------------
    let (_, body) = register(CONFIGURED_OPERATOR.to_string()).await;
    assert!(body.contains("already taken"), "bootstrap must not disable the duplicate-account check: got {body}");

    // --- 6. the door closes when the variable is removed ---------------
    std::env::remove_var("OPERATOR_BOOTSTRAP");
    let (_, body) = register(CONFIGURED_OPERATOR.to_string()).await;
    assert!(body.contains("taken") || body.contains("reserved"), "removing the variable must close the door again: got {body}");

    let _ = std::fs::remove_dir_all(&scratch);
}
