// Local identity (2026-08-27) - the game's own session minter, ADDED
// alongside the Twitch OAuth flow rather than replacing it. Per
// docs/external_integration_removal_scope.md Part 2, Twitch is only a
// *session minter*, never an identity system: `Session`, the
// `adv_session` cookie, `current_session`, `Character` and
// `adventure-characters.json` are all already provider-agnostic, so a
// second minter needs none of them to change. Nothing in this module
// touches `login`/`callback`/`handle_callback` - existing Twitch
// sessions keep working untouched.
//
// Deliberately minimal and temporary: open registration, one password,
// no reset, no email, no 2FA, no recovery, no profile. An external
// identity provider replaces this later by calling `mint_session` -
// that one function is the whole seam.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Form, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse};
use serde::{Deserialize, Serialize};

use super::{
    escape_html, now_secs, random_token, render_page, save_sessions, set_cookie_header, AppState, Session, ADMIN_TUNABLES_LOGIN, BUNDLE_OPERATOR_LOGIN,
    FIGHTS_PAGE_LOGIN, SESSION_TTL,
};

/// Sits next to `adventure-sessions.json` in the deployment root -
/// CWD-relative, deliberately NOT `data_path`-wrapped, exactly like the
/// sessions file it shadows (see `AppState::sessions_path`). Derived
/// from that path rather than taking a second parameter on
/// `start_adventure_web_server`, so every existing caller and test keeps
/// its current signature and its own scratch directory.
pub(super) fn accounts_path(sessions_path: &Path) -> PathBuf {
    sessions_path.with_file_name("adventure-accounts.json")
}

/// One local account. Identity is the MAP KEY (the lowercased username),
/// matching `AdventureManager`'s own `username.to_lowercase()` character
/// key - `username` here is the as-typed form and is display only, the
/// same split `Character::display_name` already documents.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Account {
    username: String,
    password_hash: String,
    /// Seconds since UNIX_EPOCH, like `Session::created_at`.
    created_at: u64,
}

const USERNAME_MIN_LEN: usize = 3;
const USERNAME_MAX_LEN: usize = 25;
const PASSWORD_MIN_LEN: usize = 8;

/// Operator and system names nobody may register. The three operator
/// gates (`ADMIN_TUNABLES_LOGIN` and friends) compare a bare login
/// string, so a registration matching one of those would hand out
/// `/admin/tunables`; the rest just read as staff.
///
/// `lokati_gaming` is listed here PERMANENTLY and unconditionally, not
/// because it is the operator login (it is only the default now that
/// `OPERATOR_LOGIN` exists - see adventure_web.rs) but because it is the
/// owner's public handle. Today it is also protected by the live-character
/// and minted-session checks in `do_register`, but both of those only hold
/// because World 1 data exists: World 2 starts with fresh characters and
/// invalidated sessions, at which point nothing else would stop a player
/// claiming it. Do not make this entry conditional on `OPERATOR_LOGIN`.
const RESERVED_USERNAMES: &[&str] = &[
    "lokati_gaming",
    "admin",
    "administrator",
    "moderator",
    "mod",
    "operator",
    "staff",
    "support",
    "owner",
    "system",
    "root",
    "server",
    "game",
    "bot",
    "null",
    "undefined",
    "anonymous",
    "guest",
];

/// **The seam.** The single place local identity turns a name into a
/// live session: inserts the same `Session { login, display_name,
/// created_at }` record the Twitch callback inserts, into the same map,
/// persisted to the same file, and hands back the opaque token for the
/// `adv_session` cookie. A future external identity provider mints a
/// session by calling this and nothing else.
pub(super) async fn mint_session(state: &AppState, login: &str, display_name: &str) -> String {
    let token = random_token();
    let mut sessions = state.sessions.lock().await;
    sessions.insert(token.clone(), Session { login: login.to_string(), display_name: display_name.to_string(), created_at: now_secs() });
    save_sessions(state, &sessions);
    token
}

/// `#[serde(default)]` on every field: an absent field must not 422 the
/// whole POST (see CLAUDE.md's form-drift rule) - the handler reports a
/// missing value as a normal validation error instead.
#[derive(Deserialize)]
pub(super) struct CredentialsForm {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| anyhow::anyhow!("failed to hash password: {err}"))
}

fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// Every reason a username can be refused, in the order they are checked.
/// The collision arms are the security-critical ones: characters are
/// keyed by lowercased login, so registering a name that matches an
/// existing character key would hand that character to a stranger.
/// The opt-in that lets a FRESH deployment's operator register their own
/// account (2026-08-31). `OPERATOR_LOGIN` is reserved by
/// `username_rejection` below, which on a brand-new deployment means the
/// operator cannot create the account the gates point at - standing up
/// the Linux staging instance needed a temporary source patch to get past
/// it, and World 2 launch day would hit the same wall.
///
/// The variable carries the LOGIN, not a boolean: `OPERATOR_BOOTSTRAP`
/// must equal the current `OPERATOR_LOGIN` exactly. So it permits exactly
/// one name, its own value says which, and a variable left set after
/// `OPERATOR_LOGIN` moves permits nothing at all. Set it, register,
/// remove it, restart.
///
/// Deliberately NOT "allow it while the account store is empty": on a
/// public launch that leaves a window in which any player could claim the
/// operator name first, which is the exact grief vector the reservation
/// exists to prevent. This window is never open unattended.
///
/// Read fresh on every attempt rather than through a `LazyLock` (the
/// shape `operator_login_from_env` uses) so removing the variable takes
/// effect without depending on whether some earlier request already
/// forced the cell.
fn operator_bootstrap_login() -> Option<String> {
    std::env::var("OPERATOR_BOOTSTRAP").ok().map(|v| v.trim().to_ascii_lowercase()).filter(|v| !v.is_empty())
}

fn username_rejection(key: &str, accounts: &HashMap<String, Account>) -> Option<&'static str> {
    if key.len() < USERNAME_MIN_LEN || key.len() > USERNAME_MAX_LEN {
        return Some("Usernames must be 3-25 characters long.");
    }
    if !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        // Matches the login shape the rest of the site already assumes
        // (the `[a-z0-9_]` note at adventure_web.rs's overlay tray) - the
        // key ends up in URLs (`/characters/:login`), in fight records
        // and in a JS string literal.
        return Some("Usernames may only contain lowercase letters, numbers and underscores.");
    }
    let bootstrapping = operator_bootstrap_login().is_some_and(|b| b == key);
    // The PERMANENT list. `OPERATOR_BOOTSTRAP` does NOT pierce this - see
    // `RESERVED_USERNAMES`' own doc for why `lokati_gaming` in particular
    // is unconditional. An operator who set the variable to one of these
    // is told exactly that, rather than getting the bare refusal and
    // having to read the source to find out why bootstrap did nothing.
    if RESERVED_USERNAMES.contains(&key) {
        return Some(if bootstrapping {
            "That username is permanently reserved and OPERATOR_BOOTSTRAP cannot release it. Point OPERATOR_LOGIN at the operator's own account name, set OPERATOR_BOOTSTRAP to that same name, and register that instead."
        } else {
            "That username is reserved."
        });
    }
    // The three operator gates. Normally a hard refusal - they compare a
    // bare login string, so a registration matching one would hand out
    // `/admin/tunables`. `OPERATOR_BOOTSTRAP` is the operator's own
    // deliberate opt-in to let exactly this login through once; every
    // other check below and in `do_register` still runs.
    if !bootstrapping && [ADMIN_TUNABLES_LOGIN.as_str(), FIGHTS_PAGE_LOGIN.as_str(), BUNDLE_OPERATOR_LOGIN.as_str()].iter().any(|r| r.eq_ignore_ascii_case(key)) {
        return Some("That username is reserved.");
    }
    if bootstrapping {
        tracing::warn!("OPERATOR_BOOTSTRAP is set to {key:?} - the operator reservation on that login is being bypassed for this registration. Remove the variable and restart once the account exists.");
    }
    if accounts.contains_key(key) {
        return Some("That username is already taken.");
    }
    None
}

/// The shared register/login form. The `name="..."` attributes here are
/// exactly what `CredentialsForm` consumes - the HTTP test scrapes them
/// off the rendered page rather than hard-coding a list, so drift in
/// either direction fails the suite instead of shipping a 422.
fn render_form(action: &str, heading: &str, blurb: &str, submit: &str, other_link: &str, error: Option<&str>) -> String {
    let error_html = error.map_or(String::new(), |msg| format!("<p class=\"muted\">{}</p>", escape_html(msg)));
    format!(
        "<div class=\"card\"><h1>{heading}</h1>\
          <p>{blurb}</p>\
          {error_html}\
          <form method=\"post\" action=\"{action}\">\
            <p><label for=\"username\">Username</label><br>\
              <input type=\"text\" id=\"username\" name=\"username\" autocomplete=\"username\" maxlength=\"{USERNAME_MAX_LEN}\"></p>\
            <p><label for=\"password\">Password</label><br>\
              <input type=\"password\" id=\"password\" name=\"password\" autocomplete=\"current-password\"></p>\
            <p><button class=\"btn\" type=\"submit\">{submit}</button></p>\
          </form>\
          <p class=\"muted\">{other_link}</p></div>"
    )
}

fn register_page_html(error: Option<&str>) -> String {
    render_form(
        "/account/register",
        "Create an account",
        "Pick a name and a password. This is the name your character will be known by.",
        "Register",
        "Already have an account? <a href=\"/account/login\">Log in</a>.",
        error,
    )
}

fn login_page_html(error: Option<&str>) -> String {
    render_form(
        "/account/login",
        "Log in",
        "Log in with the account you registered here.",
        "Log in",
        "No account yet? <a href=\"/account/register\">Register</a>.",
        error,
    )
}

pub(super) async fn register_page() -> Html<String> {
    Html(render_page(&register_page_html(None)))
}

pub(super) async fn login_page() -> Html<String> {
    Html(render_page(&login_page_html(None)))
}

/// Open registration, with the one guard that matters: a username that
/// collides with an existing character key, an existing session login or
/// an existing account is refused, case-insensitively. Without it anyone
/// could register a current player's name and take over their character,
/// because identity lives entirely in the lowercased map key (see
/// docs/external_integration_removal_scope.md 2.5).
pub(super) async fn do_register(State(state): State<AppState>, Form(form): Form<CredentialsForm>) -> axum::response::Response {
    let typed = form.username.trim().to_string();
    let key = typed.to_lowercase();

    if form.password.len() < PASSWORD_MIN_LEN {
        return reject_register("Passwords must be at least 8 characters long.");
    }

    let mut accounts = state.accounts.lock().await;
    if let Some(reason) = username_rejection(&key, &accounts) {
        return reject_register(reason);
    }
    // A live character under this key means a real player owns the name.
    if state.adventure.character(&key).await.is_some() {
        return reject_register("That username is already taken.");
    }
    // ...and so does a session minted for it by any provider, including a
    // Twitch login whose owner has not joined the adventure yet.
    {
        let sessions = state.sessions.lock().await;
        if sessions.values().any(|s| s.login.eq_ignore_ascii_case(&key)) {
            return reject_register("That username is already taken.");
        }
    }

    let password_hash = match hash_password(&form.password) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Local account registration failed: {err}");
            return reject_register("Something went wrong creating that account. Try again.");
        }
    };
    accounts.insert(key.clone(), Account { username: typed.clone(), password_hash, created_at: now_secs() });
    if let Err(err) = crate::state::save_json(&state.accounts_path, &*accounts) {
        tracing::error!("Failed to persist local accounts to {}: {err}", state.accounts_path.display());
    }
    drop(accounts);

    tracing::info!("Adventure dashboard: local account {key} registered.");
    let token = mint_session(&state, &key, &typed).await;
    redirect_with_session(&token)
}

pub(super) async fn do_login(State(state): State<AppState>, Form(form): Form<CredentialsForm>) -> axum::response::Response {
    let key = form.username.trim().to_lowercase();
    let account = state.accounts.lock().await.get(&key).cloned();
    // One message for both "no such account" and "wrong password" -
    // nothing here should confirm which names exist.
    let Some(account) = account.filter(|a| verify_password(&form.password, &a.password_hash)) else {
        tracing::warn!("Adventure dashboard: failed local login for {key:?}.");
        return (StatusCode::UNAUTHORIZED, Html(render_page(&login_page_html(Some("Incorrect username or password."))))).into_response();
    };

    tracing::info!("Adventure dashboard: {key} logged in locally.");
    let token = mint_session(&state, &key, &account.username).await;
    redirect_with_session(&token)
}

fn reject_register(reason: &str) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Html(render_page(&register_page_html(Some(reason))))).into_response()
}

/// Byte-identical to what `callback` does with a freshly minted token -
/// same cookie helper, same 30-day `SESSION_TTL`, same redirect home.
fn redirect_with_session(token: &str) -> axum::response::Response {
    (StatusCode::FOUND, [set_cookie_header(token, SESSION_TTL.as_secs()), (header::LOCATION, "/".to_string())], "").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hashed_password_verifies_and_a_wrong_one_does_not() {
        let hash = hash_password("correct horse battery").expect("hashing must succeed");
        assert!(hash.starts_with("$argon2id$"), "must be argon2id, got: {hash}");
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong horse battery", &hash));
        assert!(!verify_password("correct horse battery", "not-a-hash"));
    }

    #[test]
    fn operator_and_system_names_are_reserved() {
        let empty = HashMap::new();
        assert!(username_rejection("lokati_gaming", &empty).is_some(), "the operator login must be reserved");
        assert!(username_rejection("admin", &empty).is_some());
        assert!(username_rejection("moderator", &empty).is_some());
        assert!(username_rejection("ordinary_player", &empty).is_none());
    }

    #[test]
    fn usernames_are_length_and_charset_checked() {
        let empty = HashMap::new();
        assert!(username_rejection("ab", &empty).is_some(), "too short");
        assert!(username_rejection(&"a".repeat(26), &empty).is_some(), "too long");
        assert!(username_rejection("has space", &empty).is_some());
        assert!(username_rejection("has-dash", &empty).is_some());
        assert!(username_rejection("ok_name_9", &empty).is_none());
    }
}
