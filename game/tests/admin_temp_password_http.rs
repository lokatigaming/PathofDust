//! Owner-issued temporary passwords (item 47a, 2026-10-02) over real
//! HTTP, on a disposable instance (ephemeral port, scratch data dir) set
//! up the same way `local_accounts_http.rs` is.
//!
//! Proves: `/admin/accounts` is operator-only; issuing for an unknown
//! name creates nothing; issuing replaces the password, removes every
//! session for the account, and the temporary password then works while
//! the old one does not; a temporary-password login is forced onto
//! `/account/change-password` from every other route until a new
//! password is set; and an expired temporary password is refused.
//!
//! Every POST body is built from the `name="..."` attributes scraped off
//! the rendered form (CLAUDE.md's form-drift rule).
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is
//! a process-wide `OnceLock`.

use std::path::PathBuf;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use game::adventure::AdventureManager;

/// Must match `adventure_web::ADMIN_TUNABLES_LOGIN`'s default.
const OPERATOR_LOGIN: &str = "lokati_gaming";
const OPERATOR_TOKEN: &str = "operator-token";
const VICTIM: &str = "forgetful_player";
const VICTIM_OLD_TOKEN: &str = "victim-old-token";
const VICTIM_OTHER_TOKEN: &str = "victim-other-device";
const OLD_PASSWORD: &str = "the old forgotten one";
const NEW_PASSWORD: &str = "a brand new password";
const STALE: &str = "stale_player";
const STALE_TEMP: &str = "abcd-efgh-jkmn-pqrs";

fn hash(password: &str) -> String {
    Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng)).expect("hashing").to_string()
}

/// Same scraper as `local_accounts_http.rs`.
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

/// Builds the body from the scraped names; an input this test does not
/// know how to fill is a hard failure.
fn body_from(fields: &[String], values: &[(&str, &str)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|name| {
            let value = values.iter().find(|(k, _)| k == name).unwrap_or_else(|| panic!("the form grew an input this test does not fill: {name}")).1;
            (name.clone(), value.to_string())
        })
        .collect()
}

fn session_cookie(resp: &reqwest::Response) -> String {
    let header = resp.headers().get(reqwest::header::SET_COOKIE).expect("a minted session must set a cookie").to_str().expect("ascii cookie");
    format!("adv_session={}", header.strip_prefix("adv_session=").expect("cookie name").split(';').next().expect("a cookie value"))
}

fn location(resp: &reqwest::Response) -> String {
    resp.headers().get(reqwest::header::LOCATION).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
}

#[tokio::test]
async fn admin_issued_temporary_password_replaces_signs_out_forces_a_change_and_expires() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("admin_temp_password_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        serde_json::json!({
            OPERATOR_TOKEN: {"login": OPERATOR_LOGIN, "display_name": "Lokati", "created_at": now},
            VICTIM_OLD_TOKEN: {"login": VICTIM, "display_name": "Forgetful", "created_at": now},
            VICTIM_OTHER_TOKEN: {"login": VICTIM, "display_name": "Forgetful", "created_at": now},
        })
        .to_string(),
    )
    .expect("seed sessions");
    let accounts_path = scratch.join("adventure-accounts.json");
    std::fs::write(
        &accounts_path,
        serde_json::json!({
            VICTIM: {"username": "Forgetful_Player", "password_hash": hash(OLD_PASSWORD), "created_at": now},
            STALE: {"username": STALE, "password_hash": hash(STALE_TEMP), "created_at": now, "must_change_password": true, "temp_password_expires_at": now - 1},
        })
        .to_string(),
    )
    .expect("seed accounts");

    // Item 47b: this instance must run with SMTP unset - it is the
    // "email features hidden and inert, stage 1 unchanged" case.
    for name in ["SMTP_HOST", "SMTP_PORT", "SMTP_USERNAME", "SMTP_PASSWORD", "SMTP_FROM", "PUBLIC_BASE_URL"] {
        std::env::remove_var(name);
    }
    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");
    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound = game::adventure_web::start_adventure_web_server(0, manager.clone(), sessions_path.clone()).await.expect("server must start");
    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let get = |path: &str, cookie: &str| client.get(format!("{base}{path}")).header(reqwest::header::COOKIE, cookie.to_string()).send();
    let op = format!("adv_session={OPERATOR_TOKEN}");
    let victim_old = format!("adv_session={VICTIM_OLD_TOKEN}");

    // --- the gate ------------------------------------------------------
    assert_eq!(client.get(format!("{base}/admin/accounts")).send().await.unwrap().status(), 404, "anonymous must get the generic Not Found");
    assert_eq!(get("/admin/accounts", &victim_old).await.unwrap().status(), 404, "a player must get the generic Not Found");
    let refused = client.post(format!("{base}/admin/accounts/issue")).header(reqwest::header::COOKIE, &victim_old).form(&[("username", VICTIM)]).send().await.unwrap();
    assert_eq!(refused.status(), 404, "a player's POST must be refused");
    let victim_after_refusal = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&accounts_path).unwrap()).unwrap()[VICTIM].clone();
    assert!(victim_after_refusal.get("must_change_password").is_none(), "a refused issue must not touch the account");
    assert!(std::fs::read_to_string(&sessions_path).unwrap().contains(VICTIM_OLD_TOKEN), "a refused issue must not touch sessions");

    // --- forms scraped from the pages ----------------------------------
    let admin_html = get("/admin/accounts", &op).await.unwrap().text().await.unwrap();
    let issue_fields = form_field_names(&admin_html, "/admin/accounts/issue");
    assert_eq!(issue_fields, vec!["username".to_string()]);
    let login_html = client.get(format!("{base}/account/login")).send().await.unwrap().text().await.unwrap();
    assert!(login_html.contains("Forgot your password? Ask Lokati for a temporary one."));

    // --- SMTP unset: every email feature hidden and inert (item 47b) ----
    assert!(!login_html.contains("/account/forgot"), "no email-reset link without SMTP");
    assert!(!get("/", &op).await.unwrap().text().await.unwrap().contains("/account/email"), "no account-email link without SMTP");
    for path in ["/account/email", "/account/forgot", "/account/reset?token=x", "/account/email/verify?token=x"] {
        assert_eq!(get(path, &op).await.unwrap().status(), 404, "GET {path} must be inert without SMTP");
    }
    for path in ["/account/email", "/account/forgot", "/account/reset", "/account/email/verify", "/account/email/remove"] {
        let resp = client.post(format!("{base}{path}")).header(reqwest::header::COOKIE, &op).form(&[("username", VICTIM), ("email", "x@example.com"), ("password", "p")]).send().await.unwrap();
        assert_eq!(resp.status(), 404, "POST {path} must be inert without SMTP");
    }
    assert!(!std::fs::read_to_string(&accounts_path).unwrap().contains("example.com"), "nothing stored");
    let login_fields = form_field_names(&login_html, "/account/login");
    let login = |user: &'static str, password: String| {
        let body = body_from(&login_fields, &[("username", user), ("password", &password)]);
        let client = client.clone();
        let base = base.clone();
        async move { client.post(format!("{base}/account/login")).form(&body).send().await.unwrap() }
    };
    let issue = |user: &'static str| {
        let body = body_from(&issue_fields, &[("username", user)]);
        client.post(format!("{base}/admin/accounts/issue")).header(reqwest::header::COOKIE, &op).form(&body).send()
    };

    // --- unknown username: nothing created ------------------------------
    let unknown = issue("nobody_here").await.unwrap();
    assert_eq!(unknown.status(), 400);
    assert!(unknown.text().await.unwrap().contains("No account named"));
    assert!(!std::fs::read_to_string(&accounts_path).unwrap().contains("nobody_here"), "an unknown name must not create an account");

    // --- issue ------------------------------------------------------------
    let issued = issue("Forgetful_Player").await.unwrap();
    assert_eq!(issued.status(), 200);
    assert_eq!(issued.headers().get(reqwest::header::CACHE_CONTROL).unwrap(), "no-store");
    let page = issued.text().await.unwrap();
    let marker = "<code style=\"font-size:1.4em;\">";
    let at = page.find(marker).expect("the temporary password is shown") + marker.len();
    let temp = page[at..at + page[at..].find("</code>").unwrap()].to_string();
    assert_eq!(temp.len(), 19, "xxxx-xxxx-xxxx-xxxx");
    assert!(page.contains("2 session(s) signed out"));

    let sessions_file = std::fs::read_to_string(&sessions_path).unwrap();
    assert!(!sessions_file.contains(VICTIM_OLD_TOKEN) && !sessions_file.contains(VICTIM_OTHER_TOKEN), "every session for the account must be removed");
    assert!(sessions_file.contains(OPERATOR_TOKEN), "other accounts' sessions stay");
    assert_eq!(get("/", &victim_old).await.unwrap().status(), 200, "the old cookie now resolves to nobody");
    let accounts_file = std::fs::read_to_string(&accounts_path).unwrap();
    let issued_json = serde_json::from_str::<serde_json::Value>(&accounts_file).unwrap()[VICTIM].clone();
    assert_eq!(issued_json["must_change_password"], true);
    assert!(issued_json["temp_password_expires_at"].as_u64().unwrap() >= now + 72 * 3600, "72-hour expiry stored on the account");
    assert!(!accounts_file.contains(&temp), "the temporary password is never stored in the clear");

    // --- old fails, temp works, change is forced --------------------------
    assert_eq!(login("forgetful_player", OLD_PASSWORD.to_string()).await.status(), 401, "the old password must stop working");
    let temp_login = login("forgetful_player", temp.clone()).await;
    assert_eq!(temp_login.status(), 302);
    assert_eq!(location(&temp_login), "/account/change-password");
    let forced = session_cookie(&temp_login);

    for path in ["/", "/inventory", "/passives", "/patch-notes", "/admin/accounts", "/fights"] {
        let resp = get(path, &forced).await.unwrap();
        assert_eq!(resp.status(), 303, "{path} must redirect while a change is pending");
        assert_eq!(location(&resp), "/account/change-password", "{path}");
    }
    let post_join = client.post(format!("{base}/join")).header(reqwest::header::COOKIE, &forced).send().await.unwrap();
    assert_eq!(post_join.status(), 303, "POSTs are guarded too");

    let change_page = get("/account/change-password", &forced).await.unwrap();
    assert_eq!(change_page.status(), 200);
    let change_fields = form_field_names(&change_page.text().await.unwrap(), "/account/change-password");
    assert_eq!(change_fields, vec!["password".to_string(), "confirm".to_string()]);
    let change = |password: &str, confirm: &str| {
        let body = body_from(&change_fields, &[("password", password), ("confirm", confirm)]);
        client.post(format!("{base}/account/change-password")).header(reqwest::header::COOKIE, &forced).form(&body).send()
    };
    assert_eq!(change("short", "short").await.unwrap().status(), 400, "min 8 characters");
    assert_eq!(change(NEW_PASSWORD, "something else entirely").await.unwrap().status(), 400, "must match");
    assert_eq!(get("/", &forced).await.unwrap().status(), 303, "a refused change leaves the flag set");
    let changed = change(NEW_PASSWORD, NEW_PASSWORD).await.unwrap();
    assert_eq!(changed.status(), 303);
    assert_eq!(location(&changed), "/");

    assert_eq!(get("/", &forced).await.unwrap().status(), 200, "the change clears the redirect");
    let accounts_file = std::fs::read_to_string(&accounts_path).unwrap();
    let victim_json: serde_json::Value = serde_json::from_str::<serde_json::Value>(&accounts_file).unwrap()[VICTIM].clone();
    assert!(victim_json.get("must_change_password").is_none() && victim_json.get("temp_password_expires_at").is_none(), "flag and expiry cleared: {victim_json}");
    assert_eq!(login("forgetful_player", temp.clone()).await.status(), 401, "the temporary password is single-use");
    let new_login = login("forgetful_player", NEW_PASSWORD.to_string()).await;
    assert_eq!(new_login.status(), 302);
    assert_eq!(location(&new_login), "/");

    // --- expired ----------------------------------------------------------
    let expired = login(STALE, STALE_TEMP.to_string()).await;
    assert_eq!(expired.status(), 401, "a temporary password past its expiry must be refused");
    assert!(expired.text().await.unwrap().contains("expired"));
}
