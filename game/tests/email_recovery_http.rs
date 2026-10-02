//! Optional email + email password recovery (item 47b, 2026-10-02) over
//! real HTTP, on a disposable instance set up like
//! `admin_temp_password_http.rs`. **Sends no real email**: the server is
//! started with a recording fake `Mailer`.
//!
//! Proves: adding an email needs the current password and sends a link;
//! the email counts only once confirmed by the logged-in owner; an
//! unverified email can't recover; `/account/forgot` answers identically
//! for unknown / no-email / has-email; a reset link is single-use, expires,
//! and signs the account out everywhere; the stage-1 temporary password
//! and an email reset clear each other; per-username and per-IP send
//! limits hold; removing an email notifies the old address. The
//! SMTP-unset case is covered in `admin_temp_password_http.rs`.
//!
//! Every POST body is built from the `name="..."` attributes scraped off
//! the rendered form (CLAUDE.md's form-drift rule).
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is
//! a process-wide `OnceLock`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use game::adventure::AdventureManager;
use game::adventure_web::mail::{EmailConfig, Mailer, OutgoingMail};
use sha2::{Digest, Sha256};

const OPERATOR_LOGIN: &str = "lokati_gaming";
const OPERATOR_TOKEN: &str = "operator-token";
const ALICE_TOKEN: &str = "alice-token";
const ALICE_OTHER_TOKEN: &str = "alice-other-device";
const BOB_TOKEN: &str = "bob-token";
const PASSWORD: &str = "the original password";
const NEW_PASSWORD: &str = "a fresh new password";
const ALICE_EMAIL: &str = "alice@example.com";
const EXPIRED_TOKEN: &str = "an-expired-reset-token";

#[derive(Default)]
struct Outbox(Mutex<Vec<(String, String)>>);

impl Mailer for Outbox {
    fn send(&self, mail: &OutgoingMail) -> anyhow::Result<()> {
        self.0.lock().unwrap().push((mail.to.clone(), mail.body.clone()));
        Ok(())
    }
}

impl Outbox {
    fn to(&self, addr: &str) -> Vec<String> {
        self.0.lock().unwrap().iter().filter(|(to, _)| to == addr).map(|(_, body)| body.clone()).collect()
    }

    /// Sends are fire-and-forget on the blocking pool, so wait for the
    /// expected count rather than reading straight after the response.
    async fn wait_for(&self, addr: &str, count: usize) -> Vec<String> {
        for _ in 0..100 {
            if self.to(addr).len() >= count {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let got = self.to(addr);
        assert_eq!(got.len(), count, "mails to {addr}: {got:?}");
        got
    }

    /// For asserting that nothing more arrives: give a stray send time to land first.
    async fn settled(&self, addr: &str) -> usize {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.to(addr).len()
    }
}

fn hash(password: &str) -> String {
    Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng)).expect("hashing").to_string()
}

fn sha256_hex(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Same scraper as `admin_temp_password_http.rs`.
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

/// The token out of the one link in a mail body.
fn token_in(body: &str, path: &str) -> String {
    let marker = format!("https://game.example{path}?token=");
    let at = body.find(&marker).unwrap_or_else(|| panic!("no {path} link in: {body}")) + marker.len();
    body[at..].split_whitespace().next().unwrap().to_string()
}

fn account(accounts_path: &std::path::Path, key: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(accounts_path).unwrap()).unwrap()[key].clone()
}

#[tokio::test]
async fn optional_email_verifies_recovers_once_and_never_reveals_accounts() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("email_recovery_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        serde_json::json!({
            OPERATOR_TOKEN: {"login": OPERATOR_LOGIN, "display_name": "Lokati", "created_at": now},
            ALICE_TOKEN: {"login": "alice", "display_name": "Alice", "created_at": now},
            ALICE_OTHER_TOKEN: {"login": "alice", "display_name": "Alice", "created_at": now},
            BOB_TOKEN: {"login": "bob", "display_name": "Bob", "created_at": now},
        })
        .to_string(),
    )
    .expect("seed sessions");
    let pw = hash(PASSWORD);
    let accounts_path = scratch.join("adventure-accounts.json");
    std::fs::write(
        &accounts_path,
        serde_json::json!({
            "alice": {"username": "Alice", "password_hash": pw, "created_at": now},
            "bob": {"username": "Bob", "password_hash": pw, "created_at": now},
            // A pending stage-1 temporary password AND a verified email.
            "dave": {"username": "dave", "password_hash": pw, "created_at": now, "must_change_password": true, "temp_password_expires_at": now + 3600, "email": "dave@example.com"},
            "eve": {"username": "eve", "password_hash": pw, "created_at": now, "email": "eve@example.com", "reset_token": {"token_hash": sha256_hex(EXPIRED_TOKEN), "expires_at": now - 1}},
            "frank": {"username": "frank", "password_hash": pw, "created_at": now, "email": "frank@example.com"},
            "hal": {"username": "hal", "password_hash": pw, "created_at": now, "email": "hal@example.com"},
            "ivy": {"username": "ivy", "password_hash": pw, "created_at": now, "email": "ivy@example.com"},
        })
        .to_string(),
    )
    .expect("seed accounts");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");
    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let outbox = Arc::new(Outbox::default());
    let email = EmailConfig::new(outbox.clone(), "https://game.example/");
    let bound = game::adventure_web::start_adventure_web_server_with_email(0, manager.clone(), sessions_path.clone(), Some(email)).await.expect("server must start");
    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let get = |path: &str, cookie: &str| client.get(format!("{base}{path}")).header(reqwest::header::COOKIE, cookie.to_string()).send();
    let alice = format!("adv_session={ALICE_TOKEN}");
    let bob = format!("adv_session={BOB_TOKEN}");

    // --- the pages exist and their forms are scraped --------------------
    let login_html = client.get(format!("{base}/account/login")).send().await.unwrap().text().await.unwrap();
    assert!(login_html.contains("href=\"/account/forgot\""), "the login page links the email reset when SMTP is configured");
    assert!(login_html.contains("Ask Lokati for a temporary one."), "the stage-1 fallback stays");
    let login_fields = form_field_names(&login_html, "/account/login");
    let login = |user: &'static str, password: &'static str| {
        let body = body_from(&login_fields, &[("username", user), ("password", password)]);
        let (client, base) = (client.clone(), base.clone());
        async move { client.post(format!("{base}/account/login")).form(&body).send().await.unwrap() }
    };
    assert!(get("/", &alice).await.unwrap().text().await.unwrap().contains("href=\"/account/email\""), "the dashboard links the account page");
    assert_eq!(client.get(format!("{base}/account/email")).send().await.unwrap().status(), 303, "the email page needs a login");

    let email_html = get("/account/email", &alice).await.unwrap().text().await.unwrap();
    assert!(email_html.contains("No verified email"));
    let email_fields = form_field_names(&email_html, "/account/email");
    assert_eq!(email_fields, vec!["email".to_string(), "password".to_string()]);
    assert!(!email_html.contains("action=\"/account/email/remove\""), "nothing to remove yet");
    let send_verification = |cookie: &str, addr: &str, password: &str| {
        let body = body_from(&email_fields, &[("email", addr), ("password", password)]);
        client.post(format!("{base}/account/email")).header(reqwest::header::COOKIE, cookie.to_string()).form(&body).send()
    };

    let forgot_html = client.get(format!("{base}/account/forgot")).send().await.unwrap().text().await.unwrap();
    let forgot_fields = form_field_names(&forgot_html, "/account/forgot");
    assert_eq!(forgot_fields, vec!["username".to_string()]);
    let forgot = |user: &str, ip: Option<&str>| {
        let body = body_from(&forgot_fields, &[("username", user)]);
        let mut req = client.post(format!("{base}/account/forgot")).form(&body);
        if let Some(ip) = ip {
            req = req.header("CF-Connecting-IP", ip);
        }
        async move {
            let resp = req.send().await.unwrap();
            (resp.status(), resp.text().await.unwrap())
        }
    };

    // --- add an email: current password required, link sent -------------
    assert_eq!(send_verification(&alice, "not an address", PASSWORD).await.unwrap().status(), 400);
    assert_eq!(send_verification(&alice, ALICE_EMAIL, "wrong password").await.unwrap().status(), 401, "the current password is required");
    assert_eq!(outbox.settled(ALICE_EMAIL).await, 0, "a refused request sends nothing");
    let sent = send_verification(&alice, ALICE_EMAIL, PASSWORD).await.unwrap();
    assert_eq!(sent.status(), 200);
    assert!(sent.text().await.unwrap().contains("Waiting for you to confirm"), "the pending state is shown");
    let verify_token = token_in(&outbox.wait_for(ALICE_EMAIL, 1).await[0], "/account/email/verify");
    let alice_json = account(&accounts_path, "alice");
    assert!(alice_json.get("email").is_none() && alice_json["pending_email"]["address"] == ALICE_EMAIL, "pending, not yet counted: {alice_json}");
    assert!(!std::fs::read_to_string(&accounts_path).unwrap().contains(&verify_token), "only the token's hash is stored");

    // --- an unverified email cannot recover -----------------------------
    let (status, unverified_body) = forgot("alice", None).await;
    assert_eq!(status, 200);
    assert_eq!(outbox.settled(ALICE_EMAIL).await, 1, "no reset link to an unverified address");
    assert!(account(&accounts_path, "alice").get("reset_token").is_none());

    // --- verification: only the logged-in owner can confirm -------------
    let verify_path = format!("/account/email/verify?token={verify_token}");
    let anon_verify = client.get(format!("{base}{verify_path}")).send().await.unwrap().text().await.unwrap();
    assert!(anon_verify.contains("Log in to the account") && !anon_verify.contains("action=\"/account/email/verify\""), "a logged-out click (or a mail scanner) confirms nothing");
    let verify_html = get(&verify_path, &alice).await.unwrap().text().await.unwrap();
    let verify_fields = form_field_names(&verify_html, "/account/email/verify");
    assert_eq!(verify_fields, vec!["token".to_string()]);
    let verify = |cookie: &str| {
        let body = body_from(&verify_fields, &[("token", &verify_token)]);
        client.post(format!("{base}/account/email/verify")).header(reqwest::header::COOKIE, cookie.to_string()).form(&body).send()
    };
    assert_eq!(verify(&bob).await.unwrap().status(), 400, "another account's session cannot use the link");
    let verified = verify(&alice).await.unwrap();
    assert_eq!(verified.status(), 200);
    let verified_html = verified.text().await.unwrap();
    assert!(verified_html.contains("Verified email: <strong>alice@example.com</strong>"), "the verified state is shown");
    let alice_json = account(&accounts_path, "alice");
    assert!(alice_json["email"] == ALICE_EMAIL && alice_json.get("pending_email").is_none(), "{alice_json}");
    assert_eq!(verify(&alice).await.unwrap().status(), 400, "a verification link is single-use");
    assert!(!get("/account/email", &bob).await.unwrap().text().await.unwrap().contains(ALICE_EMAIL), "nobody else sees the address");

    // --- one response for unknown / no-email / has-email ----------------
    let (s_unknown, b_unknown) = forgot("nobody_by_that_name", None).await;
    let (s_bob, b_bob) = forgot("bob", None).await;
    let (s_alice, b_alice) = forgot("Alice", None).await;
    assert_eq!((s_unknown, s_bob, s_alice), (reqwest::StatusCode::OK, reqwest::StatusCode::OK, reqwest::StatusCode::OK));
    assert_eq!(b_unknown, b_bob, "unknown vs no email");
    assert_eq!(b_bob, b_alice, "no email vs verified email");
    assert_eq!(b_alice, unverified_body, "vs unverified email");
    assert!(!b_alice.contains(ALICE_EMAIL) && !b_alice.contains("a***"), "the response never names the address");
    let mails = outbox.wait_for(ALICE_EMAIL, 2).await;
    let reset_token = token_in(&mails[1], "/account/reset");

    // --- reset: single-use, signs out everywhere -------------------------
    let reset_html = client.get(format!("{base}/account/reset?token={reset_token}")).send().await.unwrap().text().await.unwrap();
    let reset_fields = form_field_names(&reset_html, "/account/reset");
    assert_eq!(reset_fields, vec!["token".to_string(), "password".to_string(), "confirm".to_string()]);
    let reset = |token: &str, password: &str, confirm: &str| {
        let body = body_from(&reset_fields, &[("token", token), ("password", password), ("confirm", confirm)]);
        client.post(format!("{base}/account/reset")).form(&body).send()
    };
    assert_eq!(reset(&reset_token, NEW_PASSWORD, "does not match").await.unwrap().status(), 400);
    assert_eq!(reset(&reset_token, "short", "short").await.unwrap().status(), 400);
    assert_eq!(reset(&reset_token, NEW_PASSWORD, NEW_PASSWORD).await.unwrap().status(), 200);
    let sessions_file = std::fs::read_to_string(&sessions_path).unwrap();
    assert!(!sessions_file.contains(ALICE_TOKEN) && !sessions_file.contains(ALICE_OTHER_TOKEN), "every session for the account is removed");
    assert!(sessions_file.contains(BOB_TOKEN), "other accounts keep theirs");
    assert!(account(&accounts_path, "alice").get("reset_token").is_none());
    assert_eq!(reset(&reset_token, "yet another password", "yet another password").await.unwrap().status(), 400, "a reset link is single-use");
    assert_eq!(login("alice", PASSWORD).await.status(), 401, "the old password is gone");
    let alice_login = login("alice", NEW_PASSWORD).await;
    assert_eq!((alice_login.status().as_u16(), location(&alice_login)), (302, "/".to_string()));
    let alice = session_cookie(&alice_login);

    // Alice has used her 3 sends (verify + 2 resets) in this window.
    assert_eq!(send_verification(&alice, "alice2@example.com", NEW_PASSWORD).await.unwrap().status(), 429, "per-username limit on verification sends");

    // --- expired ---------------------------------------------------------
    assert_eq!(reset(EXPIRED_TOKEN, NEW_PASSWORD, NEW_PASSWORD).await.unwrap().status(), 400, "an expired link is refused");
    assert_eq!(login("eve", PASSWORD).await.status(), 302, "and changes nothing");

    // --- stage 1 interplay ------------------------------------------------
    // A pending temporary password, then an email reset: flag and expiry go.
    forgot("dave", Some("198.51.100.1")).await;
    let dave_token = token_in(&outbox.wait_for("dave@example.com", 1).await[0], "/account/reset");
    assert_eq!(reset(&dave_token, NEW_PASSWORD, NEW_PASSWORD).await.unwrap().status(), 200);
    let dave_json = account(&accounts_path, "dave");
    assert!(dave_json.get("must_change_password").is_none() && dave_json.get("temp_password_expires_at").is_none() && dave_json.get("reset_token").is_none(), "{dave_json}");
    let dave_login = login("dave", NEW_PASSWORD).await;
    assert_eq!(location(&dave_login), "/", "no forced change is left behind");

    // A reset link, then an owner-issued temporary password: the link dies.
    forgot("frank", Some("198.51.100.2")).await;
    let frank_token = token_in(&outbox.wait_for("frank@example.com", 1).await[0], "/account/reset");
    let op = format!("adv_session={OPERATOR_TOKEN}");
    let issue_fields = form_field_names(&get("/admin/accounts", &op).await.unwrap().text().await.unwrap(), "/admin/accounts/issue");
    let issued = client.post(format!("{base}/admin/accounts/issue")).header(reqwest::header::COOKIE, &op).form(&body_from(&issue_fields, &[("username", "frank")])).send().await.unwrap();
    assert_eq!(issued.status(), 200);
    assert!(account(&accounts_path, "frank").get("reset_token").is_none(), "issuing a temporary password clears the reset link");
    assert_eq!(reset(&frank_token, NEW_PASSWORD, NEW_PASSWORD).await.unwrap().status(), 400, "and the old link no longer works");

    // --- rate limits --------------------------------------------------------
    // Per username: four requests from four IPs, three mails.
    for ip in ["192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4"] {
        let (status, body) = forgot("hal", Some(ip)).await;
        assert_eq!((status, body), (reqwest::StatusCode::OK, unverified_body.clone()), "a limited request still gets the same response");
    }
    outbox.wait_for("hal@example.com", 3).await;
    assert_eq!(outbox.settled("hal@example.com").await, 3, "per-username limit on reset sends");
    // Per IP: ten requests use the IP's budget; the eleventh sends nothing,
    // and the same request from another IP goes through.
    for i in 0..10 {
        forgot(&format!("spray_{i}"), Some("203.0.113.9")).await;
    }
    forgot("ivy", Some("203.0.113.9")).await;
    assert_eq!(outbox.settled("ivy@example.com").await, 0, "per-IP limit on reset sends");
    forgot("ivy", Some("203.0.113.10")).await;
    outbox.wait_for("ivy@example.com", 1).await;

    // --- remove: password required, old address told ----------------------
    let email_html = get("/account/email", &alice).await.unwrap().text().await.unwrap();
    let remove_fields = form_field_names(&email_html, "/account/email/remove");
    assert_eq!(remove_fields, vec!["password".to_string()]);
    let remove = |password: &str| {
        let body = body_from(&remove_fields, &[("password", password)]);
        client.post(format!("{base}/account/email/remove")).header(reqwest::header::COOKIE, alice.clone()).form(&body).send()
    };
    assert_eq!(remove("wrong password").await.unwrap().status(), 401);
    assert!(account(&accounts_path, "alice").get("email").is_some());
    assert_eq!(remove(NEW_PASSWORD).await.unwrap().status(), 200);
    assert!(account(&accounts_path, "alice").get("email").is_none());
    let mails = outbox.wait_for(ALICE_EMAIL, 3).await;
    assert!(mails[2].contains("was removed"), "the old address is told: {}", mails[2]);
}
