//! Local identity (2026-08-27) over real HTTP - `/account/register`,
//! `/account/login` and the existing `/logout`, exercised against the
//! real session layer on a disposable instance (OS-assigned ephemeral
//! port, scratch data dir), the same setup `admin_passives_http.rs` and
//! `memories_http.rs` use, so nothing here can reach the live game.
//!
//! The one that matters most is the **collision guard**. Characters are
//! keyed by lowercased login in `adventure-characters.json` and identity
//! lives entirely in that map key, so a registration matching an
//! existing character key - or an existing session login, from any
//! provider - would hand a live character to a stranger. Both refusals
//! are proven here, case-insensitively.
//!
//! The register/login POST field set is **scraped off the rendered
//! form**, never hard-coded (CLAUDE.md's form-drift rule): a required
//! struct field with no rendered input, or a rendered input the struct
//! does not consume, has to fail this test rather than 422 a real
//! browser save in production.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is
//! a process-wide `OnceLock`.

use std::path::PathBuf;

use game::adventure::AdventureManager;

/// Already has a character in `adventure-characters.json`.
const EXISTING_CHARACTER: &str = "veteran_player";
/// Has a live session but no character yet - a session minted before the
/// Twitch removal, proving identity outlived its minter.
const EXISTING_SESSION_LOGIN: &str = "existing_twitch_user";
/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`, which is private.
const OPERATOR_LOGIN: &str = "lokati_gaming";

const PASSWORD: &str = "correct horse battery";

/// Pulls the `name="..."` attributes out of the form posting to
/// `action`, in document order. This is the whole point of the
/// derive-from-the-page rule - the POST below sends exactly these.
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

fn session_cookie(resp: &reqwest::Response) -> String {
    let header = resp.headers().get(reqwest::header::SET_COOKIE).expect("a minted session must set a cookie").to_str().expect("ascii cookie");
    let token = header.strip_prefix("adv_session=").expect("the cookie name must not change").split(';').next().expect("a cookie value");
    assert!(!token.is_empty(), "a minted session token must not be empty");
    format!("adv_session={token}")
}

#[tokio::test]
async fn local_accounts_mint_sessions_and_refuse_to_collide_with_existing_identities() {
    // Integration tests run with the PACKAGE dir as CWD, but the template
    // loader resolves "templates/" against the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("local_accounts_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let _ = std::fs::remove_file(scratch.join("adventure-accounts.json"));

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(r#"{{"twitch-token":{{"login":"{EXISTING_SESSION_LOGIN}","display_name":"ExistingTwitchUser","created_at":{now}}}}}"#),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    manager.join(EXISTING_CHARACTER, "VeteranPlayer").await;

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        manager.clone(),
        sessions_path.clone(),
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let accounts_path = scratch.join("adventure-accounts.json");
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // --- the field set comes from the page, not from this file --------
    let page = client.get(format!("{base}/account/register")).send().await.expect("GET failed");
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let register_html = page.text().await.expect("body");
    let register_fields = form_field_names(&register_html, "/account/register");
    assert_eq!(register_fields, vec!["username".to_string(), "password".to_string()], "the register form's rendered inputs are the contract");

    let login_html = client.get(format!("{base}/account/login")).send().await.expect("GET failed").text().await.expect("body");
    assert_eq!(form_field_names(&login_html, "/account/login"), register_fields, "both forms post the same field set");

    let post_form = |path: &'static str, username: String, password: String| {
        let client = client.clone();
        let base = base.clone();
        let fields = register_fields.clone();
        async move {
            // Built by walking the SCRAPED names, so an input the page
            // renders but this test does not know about is a hard failure
            // above rather than a silently-missing field here.
            let body: Vec<(String, String)> = fields
                .iter()
                .map(|name| match name.as_str() {
                    "username" => (name.clone(), username.clone()),
                    "password" => (name.clone(), password.clone()),
                    other => panic!("the form grew an input this test does not fill: {other}"),
                })
                .collect();
            client.post(format!("{base}{path}")).form(&body).send().await.expect("POST failed")
        }
    };

    // --- THE COLLISION GUARD -----------------------------------------
    let collide_character = post_form("/account/register", "Veteran_Player".to_string(), PASSWORD.to_string()).await;
    assert_eq!(collide_character.status(), reqwest::StatusCode::BAD_REQUEST, "a username matching an existing character key must be refused");
    assert!(collide_character.text().await.expect("body").contains("already taken"));
    assert!(!accounts_path.exists(), "a refused registration must not create an account store");

    let collide_session = post_form("/account/register", "EXISTING_TWITCH_USER".to_string(), PASSWORD.to_string()).await;
    assert_eq!(collide_session.status(), reqwest::StatusCode::BAD_REQUEST, "a username matching an existing session login must be refused");
    assert!(collide_session.text().await.expect("body").contains("already taken"));

    let reserved = post_form("/account/register", OPERATOR_LOGIN.to_uppercase(), PASSWORD.to_string()).await;
    assert_eq!(reserved.status(), reqwest::StatusCode::BAD_REQUEST, "the operator login must be reserved");
    assert!(reserved.text().await.expect("body").contains("reserved"));
    assert!(!accounts_path.exists(), "still nothing registered");

    // --- the password minimum -----------------------------------------
    let short = post_form("/account/register", "shortpw_user".to_string(), "hunter7".to_string()).await;
    assert_eq!(short.status(), reqwest::StatusCode::BAD_REQUEST, "a 7-character password must be refused (the minimum is 8)");
    let short_body = short.text().await.expect("body");
    assert!(short_body.contains("at least 8 characters"), "and must say so inline on the form, got: {short_body}");
    assert!(short_body.contains("action=\"/account/register\""), "the error is rendered on the register form itself");
    assert!(!accounts_path.exists(), "a refused registration must not create an account store");

    // --- registration -------------------------------------------------
    let registered = post_form("/account/register", "NewPlayer1".to_string(), PASSWORD.to_string()).await;
    assert_eq!(registered.status(), reqwest::StatusCode::FOUND, "a valid registration redirects home with a session");
    let _ = session_cookie(&registered);

    let stored = std::fs::read_to_string(&accounts_path).expect("the account store must be persisted");
    assert!(stored.contains("\"newplayer1\""), "accounts are keyed by the LOWERCASED username, got: {stored}");
    assert!(stored.contains("$argon2id$"), "passwords must be argon2id-hashed, got: {stored}");
    assert!(!stored.contains(PASSWORD), "the plaintext password must never be persisted");

    let duplicate = post_form("/account/register", "newplayer1".to_string(), PASSWORD.to_string()).await;
    assert_eq!(duplicate.status(), reqwest::StatusCode::BAD_REQUEST, "a second registration of the same name must be refused");

    // --- login --------------------------------------------------------
    let bad = post_form("/account/login", "newplayer1".to_string(), "wrong password".to_string()).await;
    assert_eq!(bad.status(), reqwest::StatusCode::UNAUTHORIZED, "a wrong password must not mint a session");
    assert!(bad.headers().get(reqwest::header::SET_COOKIE).is_none(), "and must not set a cookie");

    let good = post_form("/account/login", "NEWPLAYER1".to_string(), PASSWORD.to_string()).await;
    assert_eq!(good.status(), reqwest::StatusCode::FOUND, "correct credentials mint a session, case-insensitively");
    let cookie = session_cookie(&good);

    let dashboard = client.get(&base).header(reqwest::header::COOKIE, &cookie).send().await.expect("GET failed").text().await.expect("body");
    assert!(dashboard.contains("Welcome, NewPlayer1!"), "the minted session must reach the authenticated dashboard, got: {dashboard}");

    let anon = client.get(&base).send().await.expect("GET failed").text().await.expect("body");
    assert!(!anon.contains("Welcome, NewPlayer1!"), "sanity: that page is not what a logged-out visitor sees");

    // --- logout invalidates server-side -------------------------------
    let logout = client.get(format!("{base}/logout")).header(reqwest::header::COOKIE, &cookie).send().await.expect("GET failed");
    assert_eq!(logout.status(), reqwest::StatusCode::FOUND);
    let after = client.get(&base).header(reqwest::header::COOKIE, &cookie).send().await.expect("GET failed").text().await.expect("body");
    assert!(!after.contains("Welcome, NewPlayer1!"), "the token must be dead server-side, not just cleared in the browser");
    let sessions_file = std::fs::read_to_string(&sessions_path).expect("sessions file");
    assert!(!sessions_file.contains(cookie.trim_start_matches("adv_session=")), "and must be gone from the persisted store");

    // --- the Twitch path is GONE, not merely unconfigured --------------
    // Folded in from the deleted `twitch_optional_http.rs` (2026-09-02),
    // which proved these same two things about credentials being ABSENT.
    // There are no credentials any more - the routes, the handlers and
    // the `client_id`/`client_secret` parameters were deleted - so the
    // surviving assertions are that the routes 404 and that the
    // logged-out page offers no link to them.
    for path in ["/login", "/auth/callback"] {
        let resp = client.get(format!("{base}{path}")).send().await.expect("GET failed");
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND, "{path} must not exist at all, got {}", resp.status());
    }
    let logged_out = client.get(&base).send().await.expect("GET failed").text().await.expect("body");
    assert!(!logged_out.contains("Login with Twitch"), "a link to a deleted route must not be offered");
    // The exact href, not a bare "/login" substring - `/account/login`
    // ends with one and must NOT trip this.
    assert!(!logged_out.contains("href=\"/login\""), "no link to the deleted /login route may survive, got: {logged_out}");
    assert!(logged_out.contains("/account/login"), "local accounts are the only login path now, got: {logged_out}");

    // A session minted before the removal still resolves - identity never
    // depended on Twitch, only the minter did.
    let pre_existing = client.get(&base).header(reqwest::header::COOKIE, "adv_session=twitch-token").send().await.expect("GET failed").text().await.expect("body");
    assert!(pre_existing.contains("Welcome, ExistingTwitchUser!"), "a pre-existing session must keep working, got: {pre_existing}");

    std::fs::remove_file(&accounts_path).ok();
}
