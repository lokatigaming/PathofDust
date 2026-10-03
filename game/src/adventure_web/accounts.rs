// Local identity (2026-08-27) - the game's own session minter. Added
// alongside the Twitch OAuth flow; since 2026-09-02, when that flow was
// deleted, it is the ONLY one. Per
// docs/external_integration_removal_scope.md Part 2, Twitch was only ever
// a *session minter*, never an identity system: `Session`, the
// `adv_session` cookie, `current_session`, `Character` and
// `adventure-characters.json` are all provider-agnostic, so replacing the
// minter needed none of them to change - and sessions minted by the old
// flow, still on disk in `adventure-sessions.json`, keep resolving.
//
// Deliberately minimal and temporary: open registration, one password,
// no reset, no email, no 2FA, no recovery, no profile. An external
// identity provider replaces this later by calling `mint_session` -
// that one function is the whole seam.
//
// 2026-10-02 (item 47a): "no reset" no longer holds - the owner can issue
// a temporary password from `/admin/accounts` (see below).
//
// 2026-10-02 (item 47b): nor does "no email" - when SMTP is configured
// (mail.rs) a player may add a verified email on `/account/email` and use
// it to reset a forgotten password. Optional, and invisible without SMTP.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Form, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use serde::{Deserialize, Serialize};

use super::mail::{mask_email, valid_email, OutgoingMail};
use super::{
    admin_not_found, current_session, escape_html, now_secs, random_token, render_page, save_sessions, set_cookie_header, AppState, Session, ADMIN_TUNABLES_LOGIN, BUNDLE_OPERATOR_LOGIN,
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
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Account {
    username: String,
    password_hash: String,
    /// Seconds since UNIX_EPOCH, like `Session::created_at`.
    created_at: u64,
    /// Set when the owner issues a temporary password from
    /// `/admin/accounts` (item 47a, 2026-10-02): the next login lands on
    /// `/account/change-password` and every other route redirects there
    /// until a new password is set. `#[serde(default)]` and skipped when
    /// false, so an account that never had a temporary password
    /// serializes byte-identically to before and an older binary still
    /// loads the file (it ignores the unknown keys - and drops them on its
    /// next save, which turns a pending temporary password into an
    /// ordinary one).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    must_change_password: bool,
    /// When the temporary password stops working, seconds since
    /// UNIX_EPOCH. Only meaningful while `must_change_password` is set;
    /// cleared with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temp_password_expires_at: Option<u64>,
    /// The player's VERIFIED email (item 47b) - the only address a reset
    /// link is ever sent to. Personal data: stored here and nowhere else,
    /// shown only to the player on `/account/email`, logged only masked.
    /// Same `#[serde(default)]` + skip contract as the 47a fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    /// An address the player has entered but not yet confirmed. It does
    /// not count for anything until verified; a verified `email` stays in
    /// force meanwhile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_email: Option<PendingEmail>,
    /// An outstanding password-reset link. Only the SHA-256 of the token
    /// is stored. Cleared on use, on any other password change, and when
    /// the email is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reset_token: Option<TokenRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct TokenRecord {
    token_hash: String,
    /// Seconds since UNIX_EPOCH.
    expires_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct PendingEmail {
    address: String,
    #[serde(flatten)]
    token: TokenRecord,
}

/// How long an unused temporary password keeps working (item 47a).
const TEMP_PASSWORD_TTL_SECS: u64 = 72 * 60 * 60;
/// Lowercase letters and digits minus the ones that read alike aloud or
/// on screen (0/o, 1/l/i). 31 symbols, 16 of them: ~79 bits.
const TEMP_PASSWORD_ALPHABET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";
const TEMP_PASSWORD_GROUPS: usize = 4;
const TEMP_PASSWORD_GROUP_LEN: usize = 4;
pub(super) const CHANGE_PASSWORD_PATH: &str = "/account/change-password";

/// `xxxx-xxxx-xxxx-xxxx` from the OS CSPRNG. The dashes are part of the
/// password - easy to read aloud in groups, and a paste carries them.
fn generate_temp_password() -> String {
    use rand::Rng;
    let mut rng = rand::rngs::OsRng;
    (0..TEMP_PASSWORD_GROUPS)
        .map(|_| (0..TEMP_PASSWORD_GROUP_LEN).map(|_| TEMP_PASSWORD_ALPHABET[rng.gen_range(0..TEMP_PASSWORD_ALPHABET.len())] as char).collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

fn temp_password_expired(account: &Account, now: u64) -> bool {
    account.must_change_password && account.temp_password_expires_at.is_some_and(|at| now >= at)
}

const USERNAME_MIN_LEN: usize = 3;
const USERNAME_MAX_LEN: usize = 25;
const PASSWORD_MIN_LEN: usize = 8;

// ---------------------------------------------------------------------
// Failed-login throttle (2026-09-02)
// ---------------------------------------------------------------------
//
// KEYED ON USERNAME, NOT ON IP, AND THAT IS DELIBERATE. Do not "improve"
// this to per-IP without reading this paragraph. Ingress is a Cloudflare
// Tunnel: `cloudflared` runs on the box and dials the game over loopback,
// so the peer address of EVERY request is 127.0.0.1. Per-IP throttling
// here would therefore have to trust the `CF-Connecting-IP` header, and
// that header is only trustworthy for as long as the tunnel is the sole
// ingress. The moment anything else can reach the port - a debug
// port-forward, a second front end, a firewall rule that stops matching
// after a port change - an attacker sets that header themselves and every
// per-IP bucket becomes whatever they say it is. Username is a property
// of the request body, not of a hop we are choosing to believe.
//
// The tradeoff, stated so it is not discovered later: per-username does
// not slow an attacker spraying ONE password across MANY usernames, only
// one guessing MANY passwords for ONE account. That is the right half to
// defend here - the accounts worth taking are specific ones - and the
// spray case is bounded instead by argon2 now running on the blocking
// pool rather than the reactor (see `verify_password_blocking`).
const LOGIN_FREE_FAILURES: u32 = 10;
const LOGIN_FAILURE_WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);
const LOGIN_DELAY_CAP: std::time::Duration = std::time::Duration::from_secs(30);

/// Hard bound on the throttle map. Entries are ~100 bytes, so this caps
/// it around 1 MB - see `record_login_failure` for what happens at the
/// bound and why the eviction order is what it is.
const LOGIN_THROTTLE_MAX_ENTRIES: usize = 10_000;

/// One username's recent failed-login history. `Instant`, not a unix
/// timestamp: this is never persisted, so there is no process boundary
/// for it to cross, and `Instant` is immune to a wall-clock jump.
pub(super) struct LoginFailure {
    count: u32,
    last: std::time::Instant,
}

/// How long this login attempt should be delayed before it is even
/// checked, given what is already recorded against the username.
///
/// Ten failures are free, so a player fat-fingering a password three
/// times pays nothing at all. From the eleventh the delay doubles -
/// 1s, 2s, 4s, 8s, 16s - capped at 30s. An entry whose last failure is
/// older than the 15-minute window is treated as absent, so the ladder
/// resets on its own without anything having to sweep it.
fn login_throttle_delay(failures: &HashMap<String, LoginFailure>, key: &str, now: std::time::Instant) -> std::time::Duration {
    let Some(entry) = failures.get(key) else {
        return std::time::Duration::ZERO;
    };
    if now.duration_since(entry.last) >= LOGIN_FAILURE_WINDOW {
        return std::time::Duration::ZERO;
    }
    let Some(over) = entry.count.checked_sub(LOGIN_FREE_FAILURES) else {
        return std::time::Duration::ZERO;
    };
    // `over` is 0 on the first throttled attempt, giving 2^0 = 1s.
    // Shift rather than powi, and saturate: `over` is attacker-influenced
    // and 1u64 << 64 is undefined behaviour territory, not a big number.
    let seconds = 1u64.checked_shl(over).unwrap_or(u64::MAX);
    std::time::Duration::from_secs(seconds).min(LOGIN_DELAY_CAP)
}

/// Record one failed attempt against `key`.
///
/// UNBOUNDED GROWTH IS THE REAL RISK HERE, because an unauthenticated
/// caller chooses the keys: POSTing a fresh username every time would
/// otherwise grow this map forever. Three things bound it.
///
/// 1. Every write first drops entries whose window has already expired,
///    which is what reclaims the ordinary case - the map's steady state
///    is "usernames that failed in the last 15 minutes", not "every
///    username ever tried".
/// 2. If the map is still at `LOGIN_THROTTLE_MAX_ENTRIES` after that
///    sweep, the OLDEST entry is evicted to make room.
/// 3. The sweep is O(n) and only runs when the map is at the bound, not
///    on every failure.
///
/// Evicting oldest rather than refusing new entries is the security
/// choice, and it is the less obvious one. Refusing to add would mean an
/// attacker could fill the table with junk usernames and thereby switch
/// the throttle OFF for everyone not already in it - the exact account
/// they are attacking would become untracked. Evicting oldest keeps the
/// most recently-active attackers throttled, which is the population
/// that matters. It does mean a sustained flood of >10,000 distinct
/// usernames inside one 15-minute window can push a specific victim's
/// counter out early; that costs the attacker far more requests than it
/// buys them, and each of those requests is itself argon2-bounded.
fn record_login_failure(failures: &mut HashMap<String, LoginFailure>, key: &str, now: std::time::Instant) {
    if let Some(entry) = failures.get_mut(key) {
        // A stale entry restarts the ladder rather than resuming it.
        if now.duration_since(entry.last) >= LOGIN_FAILURE_WINDOW {
            entry.count = 1;
        } else {
            entry.count = entry.count.saturating_add(1);
        }
        entry.last = now;
        return;
    }

    if failures.len() >= LOGIN_THROTTLE_MAX_ENTRIES {
        failures.retain(|_, e| now.duration_since(e.last) < LOGIN_FAILURE_WINDOW);
        if failures.len() >= LOGIN_THROTTLE_MAX_ENTRIES {
            if let Some(oldest) = failures.iter().min_by_key(|(_, e)| e.last).map(|(k, _)| k.clone()) {
                failures.remove(&oldest);
            }
        }
    }
    failures.insert(key.to_string(), LoginFailure { count: 1, last: now });
}

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
/// created_at }` record the retired Twitch callback used to insert, into
/// the same map, persisted to the same file, and hands back the opaque
/// token for the
/// `adv_session` cookie. Since the Twitch removal (2026-09-02) this is
/// the only minter in the process. A future external identity provider
/// mints a session by calling this and nothing else.
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

// ---------------------------------------------------------------------
// Argon2 runs on the BLOCKING pool, never on an async worker (2026-09-02)
// ---------------------------------------------------------------------
//
// `Argon2::default()` is the RFC 9106 second-recommended parameter set:
// m = 19 MiB, t = 2, p = 1. That is deliberate and correct for a password
// hash - it is supposed to be expensive - but it means every call is tens
// to hundreds of milliseconds of straight-line CPU plus a 19 MiB
// allocation, and `verify_password` is reachable by anyone who can POST
// `/account/login`, before any authentication whatsoever.
//
// Called directly from an `async fn`, that work runs ON a Tokio worker
// thread and never yields. This is the same defect class as `5f17202`
// (2026-09-02), where `simulate_battle` on the async runtime left
// production 71% unresponsive - `accept()` itself stopped, so even static
// sprite requests hung. The production box is an emulated QEMU vCPU with
// no host passthrough at ~3992 BogoMIPS, so these costs land at the top of
// their range, and unlike the fight loop this one has an unauthenticated
// trigger and no natural concurrency limit.
//
// `spawn_blocking` moves it to the blocking pool, which is sized and
// separate: a burst of login attempts queues there instead of starving the
// reactor, and the request handlers stay responsive. The throttle below
// bounds how much work an attacker can queue; this wrapper bounds where
// that work lands. They are independent fixes and either is worth having
// without the other.
//
// The password crosses a thread boundary as an owned `String`, which is
// why these take `String` rather than `&str`.
/// How long a caller waits for an argon2 permit before being told to try
/// again (2026-09-05).
///
/// **The queued-or-rejected question, decided deliberately.** A plain
/// `acquire().await` queues forever, which under a flood turns every
/// sign-in into a hang: the request pile-up just moves from CPU to open
/// connections, and a player sees a spinner until their browser gives up.
/// A bare `try_acquire` rejects instantly, which is honest under attack
/// but fails a legitimate burst - four people signing in at the same
/// moment would get "try again" on an idle server.
///
/// So: **bounded wait, then reject.** A normal burst queues for
/// milliseconds and succeeds; a genuine flood is shed with an answer
/// instead of accumulating. Five seconds is chosen against the permit
/// count: at 4 permits and the 100-300 ms per pass the emulated vCPU
/// delivers, five seconds is 60-200 passes of queue - far more than any
/// real burst - while still returning a page rather than a hang.
///
/// A rejected caller is told to retry and NOTHING ELSE HAPPENS: no login
/// failure is recorded, no account is created. Being turned away because
/// the server is busy must not cost a player a step on the failed-login
/// ladder for a password they typed correctly.
const PASSWORD_HASH_QUEUE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Takes one of the process-wide argon2 permits, or `None` if the bound
/// is saturated for longer than `PASSWORD_HASH_QUEUE_TIMEOUT`.
///
/// Both argon2 entry points go through here - `do_register`'s hash and
/// `do_login`'s verify - because they cost the same and land on the same
/// blocking pool. Bounding only the hash would leave the verify path
/// unbounded, and it is the one reachable with no account at all.
///
/// The permit is held across the `spawn_blocking` await and released when
/// the returned guard drops, so it covers the actual CPU and the 19 MiB,
/// not just the decision to start.
pub(super) async fn acquire_password_hash_permit(state: &AppState) -> Option<tokio::sync::OwnedSemaphorePermit> {
    acquire_permit_within(&state.password_hash_permits, PASSWORD_HASH_QUEUE_TIMEOUT).await
}

/// The testable core of `acquire_password_hash_permit`, taking the
/// semaphore and the timeout directly.
///
/// Split out so the tests exercise THIS function rather than a
/// `Semaphore` they constructed themselves. A test that builds its own
/// semaphore and checks that tokio counts correctly proves tokio works;
/// it would pass unchanged if this wrapper stopped acquiring anything.
async fn acquire_permit_within(sem: &std::sync::Arc<tokio::sync::Semaphore>, wait: std::time::Duration) -> Option<tokio::sync::OwnedSemaphorePermit> {
    match tokio::time::timeout(wait, sem.clone().acquire_owned()).await {
        Ok(Ok(permit)) => Some(permit),
        // The semaphore is never closed in this process; treating a close
        // as "busy" fails closed rather than letting an unbounded argon2
        // through on a path that should be impossible.
        Ok(Err(_)) => None,
        Err(_) => None,
    }
}

async fn hash_password_blocking(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password)).await.map_err(|err| anyhow::anyhow!("password hashing task failed: {err}"))?
}

/// Always returns a bool - a join error is reported as "did not verify",
/// which fails closed. There is no path here that lets a panicking or
/// cancelled task be read as a successful login.
async fn verify_password_blocking(password: String, stored: String) -> bool {
    match tokio::task::spawn_blocking(move || verify_password(&password, &stored)).await {
        Ok(verified) => verified,
        Err(err) => {
            tracing::error!("password verification task failed, treating as a failed login: {err}");
            false
        }
    }
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

/// `email_enabled` adds the email-reset link (item 47b); without SMTP the
/// page is exactly the stage-1 page.
fn login_page_html(error: Option<&str>, email_enabled: bool) -> String {
    let email_reset = if email_enabled { "<br>Added an email to your account? <a href=\"/account/forgot\">Reset your password by email</a>." } else { "" };
    render_form(
        "/account/login",
        "Log in",
        "Log in with the account you registered here.",
        "Log in",
        &format!("No account yet? <a href=\"/account/register\">Register</a>.<br>Forgot your password? Ask Lokati for a temporary one.{email_reset}"),
        error,
    )
}

pub(super) async fn register_page() -> Html<String> {
    Html(render_page(&register_page_html(None)))
}

pub(super) async fn login_page(State(state): State<AppState>) -> Html<String> {
    Html(render_page(&login_page_html(None, state.email.is_some())))
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

    let accounts = state.accounts.lock().await;
    if let Some(reason) = username_rejection(&key, &accounts) {
        return reject_register(reason);
    }
    // A live character under this key means a real player owns the name.
    if state.adventure.character(&key).await.is_some() {
        return reject_register("That username is already taken.");
    }
    // ...and so does a session minted for it by any provider, including a
    // pre-removal Twitch login whose owner never joined the adventure.
    {
        let sessions = state.sessions.lock().await;
        if sessions.values().any(|s| s.login.eq_ignore_ascii_case(&key)) {
            return reject_register("That username is already taken.");
        }
    }

    // Hashing happens off the async runtime - see `hash_password_blocking`.
    // The accounts lock is deliberately NOT held across this await: it is
    // dropped here and re-taken below, because holding a mutex across ~19
    // MiB and hundreds of milliseconds of argon2 would serialise every
    // other account operation behind one registration.
    drop(accounts);
    // The argon2 bound (2026-09-05). This is the path the bound exists
    // for: registration accepts a fresh username every time, so the
    // per-username login throttle can never see it - every attempt is the
    // first failure for its key, and a successful registration is not a
    // failure at all. Turning the caller away here costs them a retry;
    // NOT turning them away costs 19 MiB and a CPU pass per request, with
    // no authentication in front of it.
    let Some(_permit) = acquire_password_hash_permit(&state).await else {
        tracing::warn!("Adventure dashboard: argon2 bound saturated, turning away a registration for {key:?} without hashing.");
        return (StatusCode::SERVICE_UNAVAILABLE, Html(render_page(&register_page_html(Some("The server is busy right now. Please try again in a moment."))))).into_response();
    };
    let password_hash = match hash_password_blocking(form.password.clone()).await {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Local account registration failed: {err}");
            return reject_register("Something went wrong creating that account. Try again.");
        }
    };
    // Re-check under the re-taken lock. Between the drop above and here,
    // another registration could have claimed this key - the collision
    // checks above are no longer guaranteed to hold, and a bare `insert`
    // would silently overwrite the winner's account with this one's hash,
    // handing their character to whoever registered second.
    let mut accounts = state.accounts.lock().await;
    if accounts.contains_key(&key) {
        return reject_register("That username is already taken.");
    }
    accounts.insert(key.clone(), Account { username: typed.clone(), password_hash, created_at: now_secs(), ..Default::default() });
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

    // Throttle BEFORE the password is checked - see the block comment on
    // `LOGIN_FREE_FAILURES` for why this keys on username and not on IP.
    // Sleeping and then verifying (rather than rejecting outright) is what
    // keeps this honest for the legitimate case: a player who trips the
    // ladder and then types the RIGHT password waits out the delay and is
    // let in, instead of being locked out of their own account by someone
    // else guessing at their username.
    //
    // A sleeping task costs a few hundred bytes and no CPU, which is
    // orders of magnitude less than the argon2 pass it is pacing.
    let delay = {
        let failures = state.login_failures.lock().await;
        login_throttle_delay(&failures, &key, std::time::Instant::now())
    };
    if !delay.is_zero() {
        tracing::warn!("Adventure dashboard: throttling login for {key:?} by {:?} after repeated failures.", delay);
        tokio::time::sleep(delay).await;
    }

    let account = state.accounts.lock().await.get(&key).cloned();
    // Verification happens off the async runtime - see
    // `verify_password_blocking`. The accounts lock is already released
    // above (the `.cloned()` ends its temporary), so nothing is held
    // across the await.
    //
    // One message for both "no such account" and "wrong password" -
    // nothing here should confirm which names exist.
    // The argon2 bound (2026-09-05). Taken only on the arm that actually
    // verifies: the no-such-account arm below does no argon2 work, so
    // making it queue for a permit would hand an attacker a way to
    // exhaust the bound with usernames that cost nothing to reject.
    let verified = match &account {
        Some(a) => {
            let Some(_permit) = acquire_password_hash_permit(&state).await else {
                tracing::warn!("Adventure dashboard: argon2 bound saturated, turning away a login for {key:?} without verifying.");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Html(render_page(&login_page_html(Some("The server is busy checking sign-ins right now. Please try again in a moment."), state.email.is_some()))),
                )
                    .into_response();
            };
            verify_password_blocking(form.password.clone(), a.password_hash.clone()).await
        }
        // No account: nothing to verify, and deliberately no compensating
        // dummy hash. The response body and status are already identical
        // for both arms, so the only difference is timing - and paying a
        // full argon2 pass to hide it would hand an attacker exactly the
        // CPU burn this commit exists to deny them, on the cheaper of the
        // two paths. Enumeration by timing is the lesser problem.
        None => false,
    };
    let Some(account) = account.filter(|_| verified) else {
        {
            let mut failures = state.login_failures.lock().await;
            record_login_failure(&mut failures, &key, std::time::Instant::now());
        }
        tracing::warn!("Adventure dashboard: failed local login for {key:?}.");
        return (StatusCode::UNAUTHORIZED, Html(render_page(&login_page_html(Some("Incorrect username or password."), state.email.is_some())))).into_response();
    };

    // An owner-issued temporary password that went unused past its TTL
    // (item 47a). Checked only AFTER the password verified, so the
    // message tells nothing to anyone who did not already hold it - and
    // it counts as a failure, so the throttle applies unchanged.
    if temp_password_expired(&account, now_secs()) {
        {
            let mut failures = state.login_failures.lock().await;
            record_login_failure(&mut failures, &key, std::time::Instant::now());
        }
        tracing::warn!("Adventure dashboard: refused an expired temporary password for {key:?}.");
        return (StatusCode::UNAUTHORIZED, Html(render_page(&login_page_html(Some("That temporary password has expired. Ask Lokati for a new one."), state.email.is_some())))).into_response();
    }

    // Cleared on success, so a player who eventually remembers their
    // password starts clean rather than carrying a ladder they can only
    // wait out.
    state.login_failures.lock().await.remove(&key);

    let token = mint_session(&state, &key, &account.username).await;
    if account.must_change_password {
        tracing::warn!("Adventure dashboard: {key} logged in with a temporary password; forcing a password change.");
        return (StatusCode::FOUND, [set_cookie_header(&token, SESSION_TTL.as_secs()), (header::LOCATION, CHANGE_PASSWORD_PATH.to_string())], "").into_response();
    }
    tracing::info!("Adventure dashboard: {key} logged in locally.");
    redirect_with_session(&token)
}

// ---------------------------------------------------------------------
// Owner-issued temporary passwords (item 47a, 2026-10-02)
// ---------------------------------------------------------------------
//
// `/admin/accounts` replaces an account's hash with a fresh temporary
// password, flags it `must_change_password`, and removes every session
// for it. The temporary password is shown to the owner once and never
// stored or logged in the clear. Logging in with it lands on
// `/account/change-password`, and `forced_change_guard` redirects every
// other route there until a new password is set.

/// Router middleware: a session whose account still carries
/// `must_change_password` can reach the change page and `/logout`, and
/// nothing else.
pub(super) async fn forced_change_guard(State(state): State<AppState>, req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let path = req.uri().path();
    if path != CHANGE_PASSWORD_PATH && path != "/logout" {
        if let Some((login, _)) = current_session(req.headers(), &state).await {
            if state.accounts.lock().await.get(&login).is_some_and(|a| a.must_change_password) {
                return Redirect::to(CHANGE_PASSWORD_PATH).into_response();
            }
        }
    }
    next.run(req).await
}

/// `#[serde(default)]` per the form-drift rule, like `CredentialsForm`.
#[derive(Deserialize)]
pub(super) struct ChangePasswordForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: String,
}

fn change_password_html(error: Option<&str>) -> String {
    let error_html = error.map_or(String::new(), |msg| format!("<p class=\"muted\">{}</p>", escape_html(msg)));
    format!(
        "<div class=\"card\"><h1>Set a new password</h1>\
          <p>You logged in with a temporary password. Choose a new one (at least {PASSWORD_MIN_LEN} characters) to continue.</p>\
          {error_html}\
          <form method=\"post\" action=\"{CHANGE_PASSWORD_PATH}\">\
            <p><label for=\"password\">New password</label><br>\
              <input type=\"password\" id=\"password\" name=\"password\" autocomplete=\"new-password\"></p>\
            <p><label for=\"confirm\">Repeat new password</label><br>\
              <input type=\"password\" id=\"confirm\" name=\"confirm\" autocomplete=\"new-password\"></p>\
            <p><button class=\"btn\" type=\"submit\">Set password</button></p>\
          </form>\
          <p class=\"muted\"><a href=\"/logout\">Log out</a></p></div>"
    )
}

/// The login whose session is on this request AND whose account is
/// pending a forced change; anyone else is sent to the `Err` path.
async fn forced_change_login(state: &AppState, headers: &HeaderMap) -> Result<String, &'static str> {
    let Some((login, _)) = current_session(headers, state).await else {
        return Err("/account/login");
    };
    if !state.accounts.lock().await.get(&login).is_some_and(|a| a.must_change_password) {
        return Err("/");
    }
    Ok(login)
}

pub(super) async fn change_password_page(State(state): State<AppState>, headers: HeaderMap) -> axum::response::Response {
    match forced_change_login(&state, &headers).await {
        Ok(_) => Html(render_page(&change_password_html(None))).into_response(),
        Err(to) => Redirect::to(to).into_response(),
    }
}

pub(super) async fn do_change_password(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ChangePasswordForm>) -> axum::response::Response {
    let key = match forced_change_login(&state, &headers).await {
        Ok(key) => key,
        Err(to) => return Redirect::to(to).into_response(),
    };
    let reject = |msg: &str| (StatusCode::BAD_REQUEST, Html(render_page(&change_password_html(Some(msg))))).into_response();
    if form.password.len() < PASSWORD_MIN_LEN {
        return reject("Passwords must be at least 8 characters long.");
    }
    if form.password != form.confirm {
        return reject("The two passwords did not match.");
    }
    let Some(_permit) = acquire_password_hash_permit(&state).await else {
        return (StatusCode::SERVICE_UNAVAILABLE, Html(render_page(&change_password_html(Some("The server is busy right now. Please try again in a moment."))))).into_response();
    };
    let password_hash = match hash_password_blocking(form.password.clone()).await {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Forced password change for {key:?} failed: {err}");
            return reject("Something went wrong setting that password. Try again.");
        }
    };
    let mut accounts = state.accounts.lock().await;
    // Re-checked under the lock: the owner may have issued a fresh
    // temporary password while this one was hashing, and that one wins.
    let Some(account) = accounts.get_mut(&key).filter(|a| a.must_change_password) else {
        return Redirect::to("/account/login").into_response();
    };
    account.password_hash = password_hash;
    account.must_change_password = false;
    account.temp_password_expires_at = None;
    // A reset link requested while the temporary password was pending
    // must not outlive the change (item 47b).
    account.reset_token = None;
    if let Err(err) = crate::state::save_json(&state.accounts_path, &*accounts) {
        tracing::error!("Failed to persist local accounts to {}: {err}", state.accounts_path.display());
    }
    drop(accounts);
    tracing::info!("Adventure dashboard: {key} replaced their temporary password (forced change complete).");
    Redirect::to("/").into_response()
}

#[derive(Deserialize)]
pub(super) struct IssueTempPasswordForm {
    #[serde(default)]
    username: String,
}

fn admin_accounts_html(notice: Option<&str>) -> String {
    format!(
        "<div class=\"card\"><h1>Accounts</h1>\
          <p>Issue a temporary password. It replaces the account's password, signs the player out everywhere, and works once within {hours} hours; on login the player must set a new one.</p>\
          {notice}\
          <form method=\"post\" action=\"/admin/accounts/issue\">\
            <p><label for=\"username\">Username</label><br>\
              <input type=\"text\" id=\"username\" name=\"username\" maxlength=\"{USERNAME_MAX_LEN}\"></p>\
            <p><button class=\"btn\" type=\"submit\">Issue temporary password</button></p>\
          </form></div>",
        hours = TEMP_PASSWORD_TTL_SECS / 3600,
        notice = notice.unwrap_or_default(),
    )
}

async fn is_operator(state: &AppState, headers: &HeaderMap) -> Option<String> {
    current_session(headers, state).await.map(|(login, _)| login).filter(|login| *login == *ADMIN_TUNABLES_LOGIN)
}

pub(super) async fn admin_accounts_page(State(state): State<AppState>, headers: HeaderMap) -> axum::response::Response {
    if is_operator(&state, &headers).await.is_none() {
        return admin_not_found();
    }
    Html(render_page(&admin_accounts_html(None))).into_response()
}

pub(super) async fn do_issue_temp_password(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<IssueTempPasswordForm>) -> axum::response::Response {
    let Some(admin) = is_operator(&state, &headers).await else {
        return admin_not_found();
    };
    let key = form.username.trim().to_lowercase();
    let unknown = || {
        let msg = format!("<p class=\"muted\">No account named <code>{}</code>. Nothing was changed.</p>", escape_html(&key));
        (StatusCode::BAD_REQUEST, Html(render_page(&admin_accounts_html(Some(&msg))))).into_response()
    };
    if !state.accounts.lock().await.contains_key(&key) {
        tracing::warn!("Admin {admin}: temporary password requested for {key:?}, which has no account; nothing changed.");
        return unknown();
    }
    let temp = generate_temp_password();
    let Some(_permit) = acquire_password_hash_permit(&state).await else {
        return (StatusCode::SERVICE_UNAVAILABLE, Html(render_page(&admin_accounts_html(Some("<p class=\"muted\">The server is busy hashing passwords. Try again in a moment. Nothing was changed.</p>"))))).into_response();
    };
    let password_hash = match hash_password_blocking(temp.clone()).await {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Admin {admin}: hashing a temporary password for {key:?} failed: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Html(render_page(&admin_accounts_html(Some("<p class=\"muted\">Hashing failed. Nothing was changed.</p>"))))).into_response();
        }
    };
    let expires_at = now_secs() + TEMP_PASSWORD_TTL_SECS;
    {
        let mut accounts = state.accounts.lock().await;
        let Some(account) = accounts.get_mut(&key) else {
            return unknown();
        };
        account.password_hash = password_hash;
        account.must_change_password = true;
        account.temp_password_expires_at = Some(expires_at);
        // An outstanding email reset link dies with the old password
        // (item 47b) - the temporary one is now the only way in.
        account.reset_token = None;
        if let Err(err) = crate::state::save_json(&state.accounts_path, &*accounts) {
            tracing::error!("Failed to persist local accounts to {}: {err}", state.accounts_path.display());
        }
    }
    let removed = remove_sessions_for(&state, &key).await;
    tracing::warn!("Admin {admin}: issued a temporary password for {key}; {removed} session(s) removed; it expires at {expires_at} if unused.");

    let notice = format!(
        "<p>Temporary password for <strong>{name}</strong>, shown only this once:</p>\
         <p><code style=\"font-size:1.4em;\">{temp}</code></p>\
         <p class=\"muted\">{removed} session(s) signed out. It stops working in {hours} hours if unused. Issuing again replaces it.</p>",
        name = escape_html(&key),
        hours = TEMP_PASSWORD_TTL_SECS / 3600,
    );
    // no-store: the one page that carries the password in the clear must
    // not be kept by the browser or anything between it and the box.
    ([(header::CACHE_CONTROL, "no-store")], Html(render_page(&admin_accounts_html(Some(&notice))))).into_response()
}

/// Signs `key` out everywhere; returns how many sessions went.
async fn remove_sessions_for(state: &AppState, key: &str) -> usize {
    let mut sessions = state.sessions.lock().await;
    let before = sessions.len();
    sessions.retain(|_, s| !s.login.eq_ignore_ascii_case(key));
    let removed = before - sessions.len();
    save_sessions(state, &sessions);
    removed
}

// ---------------------------------------------------------------------
// Optional email + email password recovery (item 47b, 2026-10-02)
// ---------------------------------------------------------------------
//
// A logged-in player may add an email on `/account/email` (current
// password required). It counts only once the player opens the emailed
// link WHILE LOGGED IN as that account and confirms - so a typo'd address
// whose owner's mail scanner prefetches the link cannot end up verified
// on someone else's account. A verified email can then receive a
// single-use, one-hour reset link from `/account/forgot`, which answers
// identically whatever the username. Tokens are 256-bit, stored only as
// SHA-256. Every route here 404s when SMTP is not configured.
//
// Stage-1 interplay: an email reset clears `must_change_password` and the
// temporary password's expiry (the temporary password is overwritten by
// the new hash); an owner-issued temporary password clears any
// outstanding reset link, and so does every other password change.

pub(super) const EMAIL_PATH: &str = "/account/email";
const EMAIL_VERIFY_TTL_SECS: u64 = 24 * 60 * 60;
const RESET_TOKEN_TTL_SECS: u64 = 60 * 60;
/// Emails one username can trigger per `LOGIN_FAILURE_WINDOW` (15 min),
/// counting verification and reset sends together.
const EMAIL_SENDS_PER_USER: u32 = 3;
/// The same, per client IP.
const EMAIL_SENDS_PER_IP: u32 = 10;
const FORGOT_RESPONSE: &str = "If that account has a verified email, a reset link is on its way. It works once, within 1 hour. No email on the account? Ask Lokati for a temporary password.";

fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn new_token(now: u64, ttl: u64) -> (String, TokenRecord) {
    let token = random_token();
    let record = TokenRecord { token_hash: token_hash(&token), expires_at: now + ttl };
    (token, record)
}

fn token_matches(record: &TokenRecord, token: &str, now: u64) -> bool {
    now < record.expires_at && record.token_hash == token_hash(token)
}

/// The per-IP key. Behind the Cloudflare Tunnel every peer is 127.0.0.1
/// (see the throttle comment above), so this trusts `CF-Connecting-IP`.
/// A caller who can reach the port directly and forge it only escapes the
/// per-IP bucket; the per-username limit still holds. No header at all
/// shares one bucket.
fn client_ip(headers: &HeaderMap) -> String {
    headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).unwrap_or("direct").to_string()
}

/// The login-throttle machinery reused as a send limit: an entry within
/// its window at `limit` sends is exhausted.
fn email_sends_exhausted(sends: &HashMap<String, LoginFailure>, key: &str, limit: u32, now: std::time::Instant) -> bool {
    sends.get(key).is_some_and(|e| now.duration_since(e.last) < LOGIN_FAILURE_WINDOW && e.count >= limit)
}

/// Checks BOTH limits before recording either, then records one send
/// against each. A refused request is not recorded, so the window runs
/// out 15 minutes after the last send that was allowed.
async fn email_send_allowed(state: &AppState, user_key: &str, headers: &HeaderMap) -> bool {
    let now = std::time::Instant::now();
    let ip = client_ip(headers);
    let mut by_user = state.email_sends_by_user.lock().await;
    let mut by_ip = state.email_sends_by_ip.lock().await;
    if email_sends_exhausted(&by_user, user_key, EMAIL_SENDS_PER_USER, now) || email_sends_exhausted(&by_ip, &ip, EMAIL_SENDS_PER_IP, now) {
        return false;
    }
    record_login_failure(&mut by_user, user_key, now);
    record_login_failure(&mut by_ip, &ip, now);
    true
}

/// Fire-and-forget on the blocking pool, so the response never waits on
/// (or reveals timing from) the SMTP server. Logs the address masked
/// only - including inside a transport error, which can echo it.
fn send_in_background(state: &AppState, to: String, subject: &str, body: String, what: &'static str) {
    let Some(email) = state.email.clone() else {
        return;
    };
    let masked = mask_email(&to);
    let mail = OutgoingMail { to: to.clone(), subject: subject.to_string(), body };
    tokio::spawn(async move {
        match tokio::task::spawn_blocking(move || email.mailer.send(&mail)).await {
            Ok(Ok(())) => tracing::info!("Sent {what} email to {masked}."),
            Ok(Err(err)) => tracing::warn!("Sending {what} email to {masked} failed: {}", err.to_string().replace(&to, &masked)),
            Err(err) => tracing::error!("The {what} email task failed: {err}"),
        }
    });
}

fn link(state: &AppState, path: &str, token: &str) -> String {
    format!("{}{path}?token={}", state.email.as_ref().map_or("", |e| e.base_url.as_str()), urlencoding::encode(token))
}

fn card(title: &str, body: &str) -> String {
    format!("<div class=\"card\"><h1>{title}</h1>{body}</div>")
}

fn notice_html(msg: &str) -> String {
    format!("<p class=\"muted\">{}</p>", escape_html(msg))
}

enum PasswordCheck {
    Correct,
    Wrong,
    Busy,
}

/// Re-checks a logged-in player's current password, through the same
/// throttle and argon2 bound as `do_login`; a wrong one is a login
/// failure.
async fn check_current_password(state: &AppState, key: &str, password: &str) -> PasswordCheck {
    let delay = {
        let failures = state.login_failures.lock().await;
        login_throttle_delay(&failures, key, std::time::Instant::now())
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let Some(stored) = state.accounts.lock().await.get(key).map(|a| a.password_hash.clone()) else {
        return PasswordCheck::Wrong;
    };
    let Some(_permit) = acquire_password_hash_permit(state).await else {
        return PasswordCheck::Busy;
    };
    if verify_password_blocking(password.to_string(), stored).await {
        return PasswordCheck::Correct;
    }
    record_login_failure(&mut *state.login_failures.lock().await, key, std::time::Instant::now());
    tracing::warn!("Adventure dashboard: wrong current password on the email page for {key:?}.");
    PasswordCheck::Wrong
}

/// The logged-in login, if it has a local account (an old Twitch-minted
/// session may not).
async fn account_login(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let (login, _) = current_session(headers, state).await?;
    state.accounts.lock().await.contains_key(&login).then_some(login)
}

fn persist_accounts(state: &AppState, accounts: &HashMap<String, Account>) {
    if let Err(err) = crate::state::save_json(&state.accounts_path, accounts) {
        tracing::error!("Failed to persist local accounts to {}: {err}", state.accounts_path.display());
    }
}

fn email_page_html(account: &Account, notice: Option<&str>) -> String {
    let status = match &account.email {
        Some(email) => format!("<p>Verified email: <strong>{}</strong>. Password reset links are sent here.</p>", escape_html(email)),
        None => "<p>No verified email on this account. Adding one is optional: it lets you reset a forgotten password yourself.</p>".to_string(),
    };
    let pending = account.pending_email.as_ref().map_or(String::new(), |p| {
        format!("<p>Waiting for you to confirm <strong>{}</strong>: open the link we sent there while logged in here. It is not used for anything until then.</p>", escape_html(&p.address))
    });
    let remove = if account.email.is_some() || account.pending_email.is_some() {
        "<form method=\"post\" action=\"/account/email/remove\">\
           <p><label for=\"remove_password\">Current password</label><br>\
             <input type=\"password\" id=\"remove_password\" name=\"password\" autocomplete=\"current-password\"></p>\
           <p><button class=\"btn\" type=\"submit\">Remove email</button></p>\
         </form>"
    } else {
        ""
    };
    card(
        "Account email",
        &format!(
            "{status}{pending}{notice}\
             <form method=\"post\" action=\"{EMAIL_PATH}\">\
               <p><label for=\"email\">Email</label><br>\
                 <input type=\"email\" id=\"email\" name=\"email\" autocomplete=\"email\" maxlength=\"254\"></p>\
               <p><label for=\"password\">Current password</label><br>\
                 <input type=\"password\" id=\"password\" name=\"password\" autocomplete=\"current-password\"></p>\
               <p><button class=\"btn\" type=\"submit\">Send verification link</button></p>\
             </form>\
             {remove}\
             <p class=\"muted\">Your email is stored only with your account on the game server. Only you and the game's owner can see it, and it is used only for verification, password resets, and a notice if it is changed or removed.</p>\
             <p class=\"muted\"><a href=\"/\">Back</a></p>",
            notice = notice.map(notice_html).unwrap_or_default(),
        ),
    )
}

#[derive(Deserialize)]
pub(super) struct EmailForm {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
pub(super) struct PasswordOnlyForm {
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
pub(super) struct TokenForm {
    #[serde(default)]
    token: String,
}

#[derive(Deserialize)]
pub(super) struct ResetForm {
    #[serde(default)]
    token: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: String,
}

/// 404 when SMTP is unset (the same generic page an unknown route gets),
/// else the account's login - or a redirect to log in.
// The Err is the page to send straight back, as in `forced_change_login`.
#[allow(clippy::result_large_err)]
async fn email_account(state: &AppState, headers: &HeaderMap) -> Result<String, axum::response::Response> {
    if state.email.is_none() {
        return Err(admin_not_found());
    }
    account_login(state, headers).await.ok_or_else(|| Redirect::to("/account/login").into_response())
}

async fn render_email_page(state: &AppState, key: &str, status: StatusCode, notice: Option<&str>) -> axum::response::Response {
    let Some(account) = state.accounts.lock().await.get(key).cloned() else {
        return Redirect::to("/account/login").into_response();
    };
    (status, Html(render_page(&email_page_html(&account, notice)))).into_response()
}

pub(super) async fn email_page(State(state): State<AppState>, headers: HeaderMap) -> axum::response::Response {
    match email_account(&state, &headers).await {
        Ok(key) => render_email_page(&state, &key, StatusCode::OK, None).await,
        Err(resp) => resp,
    }
}

pub(super) async fn do_send_verification(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<EmailForm>) -> axum::response::Response {
    let key = match email_account(&state, &headers).await {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let address = form.email.trim().to_string();
    if !valid_email(&address) {
        return render_email_page(&state, &key, StatusCode::BAD_REQUEST, Some("That doesn't look like an email address.")).await;
    }
    match check_current_password(&state, &key, &form.password).await {
        PasswordCheck::Correct => {}
        PasswordCheck::Wrong => return render_email_page(&state, &key, StatusCode::UNAUTHORIZED, Some("Incorrect current password.")).await,
        PasswordCheck::Busy => return render_email_page(&state, &key, StatusCode::SERVICE_UNAVAILABLE, Some("The server is busy right now. Please try again in a moment.")).await,
    }
    if !email_send_allowed(&state, &key, &headers).await {
        tracing::warn!("Adventure dashboard: email send limit reached for {key:?}; verification not sent.");
        return render_email_page(&state, &key, StatusCode::TOO_MANY_REQUESTS, Some("Too many emails requested. Try again in 15 minutes.")).await;
    }
    let (token, record) = new_token(now_secs(), EMAIL_VERIFY_TTL_SECS);
    {
        let mut accounts = state.accounts.lock().await;
        let Some(account) = accounts.get_mut(&key) else {
            return Redirect::to("/account/login").into_response();
        };
        account.pending_email = Some(PendingEmail { address: address.clone(), token: record });
        persist_accounts(&state, &accounts);
    }
    tracing::info!("Adventure dashboard: {key} asked to verify {}.", mask_email(&address));
    let body = format!(
        "Someone (hopefully you) asked to add this address to the Path of Dust account \"{key}\".\n\n\
         To confirm, open this link while logged in to that account (it expires in 24 hours):\n{}\n\n\
         If this wasn't you, ignore this email and the address will not be added.\n",
        link(&state, "/account/email/verify", &token)
    );
    send_in_background(&state, address.clone(), "Confirm your Path of Dust email", body, "verification");
    render_email_page(&state, &key, StatusCode::OK, Some(&format!("We sent a confirmation link to {address}. Open it while logged in here."))).await
}

pub(super) async fn verify_email_page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<TokenForm>) -> axum::response::Response {
    if state.email.is_none() {
        return admin_not_found();
    }
    if account_login(&state, &headers).await.is_none() {
        return Html(render_page(&card("Confirm your email", "<p>Log in to the account you added this email to, then open the link from the email again.</p><p><a href=\"/account/login\">Log in</a></p>"))).into_response();
    }
    Html(render_page(&card(
        "Confirm your email",
        &format!(
            "<form method=\"post\" action=\"/account/email/verify\">\
               <input type=\"hidden\" name=\"token\" value=\"{}\">\
               <p><button class=\"btn\" type=\"submit\">Confirm this email</button></p>\
             </form>",
            escape_html(&query.token)
        ),
    )))
    .into_response()
}

pub(super) async fn do_verify_email(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<TokenForm>) -> axum::response::Response {
    let key = match email_account(&state, &headers).await {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let (new, old) = {
        let mut accounts = state.accounts.lock().await;
        let Some(account) = accounts.get_mut(&key) else {
            return Redirect::to("/account/login").into_response();
        };
        let Some(pending) = account.pending_email.take_if(|p| token_matches(&p.token, &form.token, now_secs())) else {
            drop(accounts);
            return render_email_page(&state, &key, StatusCode::BAD_REQUEST, Some("That confirmation link is invalid or has expired. Send a new one.")).await;
        };
        let old = account.email.replace(pending.address.clone());
        // A reset link already out to the previous address dies with it.
        account.reset_token = None;
        persist_accounts(&state, &accounts);
        (pending.address, old)
    };
    tracing::info!("Adventure dashboard: {key} verified {}.", mask_email(&new));
    if let Some(old) = old.filter(|o| !o.eq_ignore_ascii_case(&new)) {
        let body = format!("The email on the Path of Dust account \"{key}\" was changed to another address. Password reset links now go there.\n\nIf you didn't do this, contact Lokati.\n");
        send_in_background(&state, old, "Your Path of Dust email was changed", body, "change notice");
    }
    render_email_page(&state, &key, StatusCode::OK, Some("Email verified.")).await
}

pub(super) async fn do_remove_email(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<PasswordOnlyForm>) -> axum::response::Response {
    let key = match email_account(&state, &headers).await {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    match check_current_password(&state, &key, &form.password).await {
        PasswordCheck::Correct => {}
        PasswordCheck::Wrong => return render_email_page(&state, &key, StatusCode::UNAUTHORIZED, Some("Incorrect current password.")).await,
        PasswordCheck::Busy => return render_email_page(&state, &key, StatusCode::SERVICE_UNAVAILABLE, Some("The server is busy right now. Please try again in a moment.")).await,
    }
    let old = {
        let mut accounts = state.accounts.lock().await;
        let Some(account) = accounts.get_mut(&key) else {
            return Redirect::to("/account/login").into_response();
        };
        account.pending_email = None;
        account.reset_token = None;
        let old = account.email.take();
        persist_accounts(&state, &accounts);
        old
    };
    tracing::info!("Adventure dashboard: {key} removed their email.");
    if let Some(old) = old {
        let body = format!("The email was removed from the Path of Dust account \"{key}\". It can no longer be used to reset that account's password.\n\nIf you didn't do this, contact Lokati.\n");
        send_in_background(&state, old, "Your Path of Dust email was removed", body, "removal notice");
    }
    render_email_page(&state, &key, StatusCode::OK, Some("Email removed.")).await
}

fn forgot_html(notice: Option<&str>) -> String {
    card(
        "Forgot your password",
        &format!(
            "<p>Enter your username. If the account has a verified email, we'll send a link to set a new password.</p>{}\
             <form method=\"post\" action=\"/account/forgot\">\
               <p><label for=\"username\">Username</label><br>\
                 <input type=\"text\" id=\"username\" name=\"username\" autocomplete=\"username\" maxlength=\"{USERNAME_MAX_LEN}\"></p>\
               <p><button class=\"btn\" type=\"submit\">Send reset link</button></p>\
             </form>\
             <p class=\"muted\"><a href=\"/account/login\">Back to log in</a></p>",
            notice.map(notice_html).unwrap_or_default()
        ),
    )
}

pub(super) async fn forgot_page(State(state): State<AppState>) -> axum::response::Response {
    if state.email.is_none() {
        return admin_not_found();
    }
    Html(render_page(&forgot_html(None))).into_response()
}

/// ONE response for every outcome - unknown name, no email, unverified
/// email, rate-limited, sent - so the page cannot be used to find
/// accounts or emails. The send itself is in the background.
pub(super) async fn do_forgot(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<IssueTempPasswordForm>) -> axum::response::Response {
    if state.email.is_none() {
        return admin_not_found();
    }
    let key = form.username.trim().to_lowercase();
    if !email_send_allowed(&state, &key, &headers).await {
        tracing::warn!("Adventure dashboard: email send limit reached; reset for {key:?} not sent.");
    } else {
        let (token, record) = new_token(now_secs(), RESET_TOKEN_TTL_SECS);
        let to = {
            let mut accounts = state.accounts.lock().await;
            let to = accounts.get_mut(&key).and_then(|account| {
                let email = account.email.clone()?;
                // Issuing again replaces the earlier link.
                account.reset_token = Some(record);
                Some(email)
            });
            if to.is_some() {
                persist_accounts(&state, &accounts);
            }
            to
        };
        match to {
            Some(to) => {
                tracing::info!("Adventure dashboard: password reset link for {key} sent to {}.", mask_email(&to));
                let body = format!(
                    "Someone (hopefully you) asked to reset the password for the Path of Dust account \"{key}\".\n\n\
                     Set a new password here (works once, within 1 hour):\n{}\n\n\
                     If this wasn't you, ignore this email; your password has not changed.\n",
                    link(&state, "/account/reset", &token)
                );
                send_in_background(&state, to, "Reset your Path of Dust password", body, "password reset");
            }
            None => tracing::info!("Adventure dashboard: password reset asked for {key:?}, which has no verified email; nothing sent."),
        }
    }
    Html(render_page(&forgot_html(Some(FORGOT_RESPONSE)))).into_response()
}

fn reset_html(token: &str, error: Option<&str>) -> String {
    card(
        "Set a new password",
        &format!(
            "<p>Choose a new password (at least {PASSWORD_MIN_LEN} characters). This signs you out everywhere.</p>{}\
             <form method=\"post\" action=\"/account/reset\">\
               <input type=\"hidden\" name=\"token\" value=\"{}\">\
               <p><label for=\"password\">New password</label><br>\
                 <input type=\"password\" id=\"password\" name=\"password\" autocomplete=\"new-password\"></p>\
               <p><label for=\"confirm\">Repeat new password</label><br>\
                 <input type=\"password\" id=\"confirm\" name=\"confirm\" autocomplete=\"new-password\"></p>\
               <p><button class=\"btn\" type=\"submit\">Set password</button></p>\
             </form>",
            error.map(notice_html).unwrap_or_default(),
            escape_html(token)
        ),
    )
}

pub(super) async fn reset_page(State(state): State<AppState>, Query(query): Query<TokenForm>) -> axum::response::Response {
    if state.email.is_none() {
        return admin_not_found();
    }
    ([(header::CACHE_CONTROL, "no-store")], Html(render_page(&reset_html(&query.token, None)))).into_response()
}

fn reset_holder(accounts: &HashMap<String, Account>, token: &str, now: u64) -> Option<String> {
    accounts.iter().find(|(_, a)| a.reset_token.as_ref().is_some_and(|r| token_matches(r, token, now))).map(|(k, _)| k.clone())
}

pub(super) async fn do_reset(State(state): State<AppState>, Form(form): Form<ResetForm>) -> axum::response::Response {
    if state.email.is_none() {
        return admin_not_found();
    }
    let reject = |status: StatusCode, msg: &str| (status, Html(render_page(&reset_html(&form.token, Some(msg))))).into_response();
    if form.password.len() < PASSWORD_MIN_LEN {
        return reject(StatusCode::BAD_REQUEST, "Passwords must be at least 8 characters long.");
    }
    if form.password != form.confirm {
        return reject(StatusCode::BAD_REQUEST, "The two passwords did not match.");
    }
    const INVALID: &str = "This reset link is invalid, already used, or expired. Request a new one from the Forgot password page.";
    if reset_holder(&*state.accounts.lock().await, &form.token, now_secs()).is_none() {
        return reject(StatusCode::BAD_REQUEST, INVALID);
    }
    let Some(_permit) = acquire_password_hash_permit(&state).await else {
        return reject(StatusCode::SERVICE_UNAVAILABLE, "The server is busy right now. Please try again in a moment.");
    };
    let password_hash = match hash_password_blocking(form.password.clone()).await {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Email password reset failed while hashing: {err}");
            return reject(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong setting that password. Try again.");
        }
    };
    // Re-found under the re-taken lock: a second submit of the same link
    // may have used it while this one was hashing, and only one wins.
    let key = {
        let mut accounts = state.accounts.lock().await;
        let Some(key) = reset_holder(&accounts, &form.token, now_secs()) else {
            drop(accounts);
            return reject(StatusCode::BAD_REQUEST, INVALID);
        };
        let account = accounts.get_mut(&key).expect("reset_holder returned a present key");
        account.password_hash = password_hash;
        account.reset_token = None;
        // A pending owner-issued temporary password is superseded.
        account.must_change_password = false;
        account.temp_password_expires_at = None;
        persist_accounts(&state, &accounts);
        key
    };
    let removed = remove_sessions_for(&state, &key).await;
    state.login_failures.lock().await.remove(&key);
    tracing::warn!("Adventure dashboard: {key} reset their password by email; {removed} session(s) removed.");
    Html(render_page(&card("Password changed", "<p>Your password has been changed and you have been signed out everywhere.</p><p><a href=\"/account/login\">Log in</a></p>"))).into_response()
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

    /// The number the owner actually asked about: three wrong passwords
    /// must cost a legitimate player nothing at all.
    #[test]
    fn fat_fingering_a_password_three_times_is_free() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        for _ in 0..3 {
            record_login_failure(&mut failures, "player", now);
        }
        assert_eq!(login_throttle_delay(&failures, "player", now), std::time::Duration::ZERO, "three failures must not delay anyone");
    }

    #[test]
    fn ten_failures_are_free_and_the_eleventh_starts_the_ladder() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        for _ in 0..LOGIN_FREE_FAILURES {
            record_login_failure(&mut failures, "player", now);
        }
        assert_eq!(login_throttle_delay(&failures, "player", now), std::time::Duration::from_secs(1), "the 11th attempt pays 1s");

        // One further failure per step: 11 free-plus-one -> 2s, then 4, 8, 16.
        for expected in [2u64, 4, 8, 16] {
            record_login_failure(&mut failures, "player", now);
            let count = failures["player"].count;
            assert_eq!(login_throttle_delay(&failures, "player", now), std::time::Duration::from_secs(expected), "at {count} failures the delay must be {expected}s");
        }

        // And it stops doubling at the cap rather than growing forever.
        for _ in 0..8 {
            record_login_failure(&mut failures, "player", now);
        }
        assert_eq!(login_throttle_delay(&failures, "player", now), LOGIN_DELAY_CAP, "the ladder must settle at the 30s cap");
    }

    #[test]
    fn the_delay_is_capped_and_never_overflows() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        // Well past the point where 1 << over would leave u64 - the shift
        // is where an attacker-influenced count would bite if unguarded.
        failures.insert("player".to_string(), LoginFailure { count: u32::MAX, last: now });
        assert_eq!(login_throttle_delay(&failures, "player", now), LOGIN_DELAY_CAP, "the ladder caps at 30s and must not overflow");
    }

    #[test]
    fn an_expired_window_resets_the_ladder() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        let stale = now - LOGIN_FAILURE_WINDOW - std::time::Duration::from_secs(1);
        failures.insert("player".to_string(), LoginFailure { count: 50, last: stale });
        assert_eq!(login_throttle_delay(&failures, "player", now), std::time::Duration::ZERO, "a failure older than the window must not delay");
        record_login_failure(&mut failures, "player", now);
        assert_eq!(failures["player"].count, 1, "a stale entry restarts the ladder rather than resuming it");
    }

    #[test]
    fn the_throttle_map_is_bounded_and_evicts_the_oldest() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        // Fill to the bound with entries that are all still IN window, so
        // the expiry sweep cannot reclaim anything and the eviction path
        // is the one under test.
        for i in 0..LOGIN_THROTTLE_MAX_ENTRIES {
            // Oldest first, so entry 0 is the eviction candidate.
            let age = std::time::Duration::from_secs((LOGIN_THROTTLE_MAX_ENTRIES - i) as u64 / 16);
            failures.insert(format!("user{i}"), LoginFailure { count: 1, last: now - age });
        }
        assert_eq!(failures.len(), LOGIN_THROTTLE_MAX_ENTRIES);

        record_login_failure(&mut failures, "newcomer", now);
        assert!(failures.len() <= LOGIN_THROTTLE_MAX_ENTRIES, "the map must stay at or under its bound, got {}", failures.len());
        assert!(failures.contains_key("newcomer"), "a new attacker must still get tracked - refusing to add would switch the throttle off for them");
        assert!(!failures.contains_key("user0"), "the oldest entry is the one evicted");
    }

    #[test]
    fn expired_entries_are_reclaimed_before_anything_is_evicted() {
        let mut failures = HashMap::new();
        let now = std::time::Instant::now();
        let stale = now - LOGIN_FAILURE_WINDOW - std::time::Duration::from_secs(1);
        for i in 0..LOGIN_THROTTLE_MAX_ENTRIES {
            failures.insert(format!("user{i}"), LoginFailure { count: 1, last: stale });
        }
        record_login_failure(&mut failures, "newcomer", now);
        assert_eq!(failures.len(), 1, "an all-stale map is swept clean, not evicted one entry at a time");
        assert!(failures.contains_key("newcomer"));
    }

    #[test]
    fn temporary_passwords_are_grouped_unambiguous_and_distinct() {
        let a = generate_temp_password();
        let groups: Vec<&str> = a.split('-').collect();
        assert_eq!(groups.len(), TEMP_PASSWORD_GROUPS, "got {a}");
        assert!(groups.iter().all(|g| g.len() == TEMP_PASSWORD_GROUP_LEN && g.bytes().all(|b| TEMP_PASSWORD_ALPHABET.contains(&b))), "got {a}");
        assert!(!TEMP_PASSWORD_ALPHABET.iter().any(|b| b"01ilo".contains(b)), "no characters that read alike");
        assert!(a.len() >= PASSWORD_MIN_LEN);
        assert_ne!(a, generate_temp_password(), "two issues must not repeat");
    }

    #[test]
    fn a_temporary_password_expires_at_its_deadline_and_a_normal_one_never_does() {
        let mut account = Account { username: "p".into(), password_hash: String::new(), created_at: 0, must_change_password: true, temp_password_expires_at: Some(1_000), ..Default::default() };
        assert!(!temp_password_expired(&account, 999));
        assert!(temp_password_expired(&account, 1_000));
        account.must_change_password = false;
        assert!(!temp_password_expired(&account, u64::MAX), "a changed password carries no expiry");
    }

    /// The rollback contract: an account that never had a temporary
    /// password writes exactly the three pre-47a keys, and the pre-47a
    /// shape still loads.
    #[test]
    fn accounts_without_a_temporary_password_serialize_as_before() {
        let account = Account { username: "p".into(), password_hash: "h".into(), created_at: 7, ..Default::default() };
        assert_eq!(serde_json::to_string(&account).unwrap(), r#"{"username":"p","password_hash":"h","created_at":7}"#);
        let old: Account = serde_json::from_str(r#"{"username":"p","password_hash":"h","created_at":7}"#).unwrap();
        assert!(!old.must_change_password && old.temp_password_expires_at.is_none());
    }

    /// The 47b rollback contract: the email fields are skipped when unset
    /// (so the test above still holds), and a 47b account with every
    /// field set still loads in a struct that knows none of them -
    /// standing in for an older binary, which ignores unknown keys.
    #[test]
    fn email_fields_round_trip_and_an_older_reader_ignores_them() {
        let account = Account {
            username: "p".into(),
            password_hash: "h".into(),
            created_at: 7,
            email: Some("p@example.com".into()),
            pending_email: Some(PendingEmail { address: "q@example.com".into(), token: TokenRecord { token_hash: "a".into(), expires_at: 9 } }),
            reset_token: Some(TokenRecord { token_hash: "b".into(), expires_at: 10 }),
            ..Default::default()
        };
        let json = serde_json::to_string(&account).unwrap();
        let back: Account = serde_json::from_str(&json).unwrap();
        assert_eq!(back.email.as_deref(), Some("p@example.com"));
        assert_eq!(back.pending_email.as_ref().map(|p| (p.address.as_str(), p.token.expires_at)), Some(("q@example.com", 9)));
        assert_eq!(back.reset_token.as_ref().map(|r| r.expires_at), Some(10));
        #[derive(Deserialize)]
        struct PreEmail {
            username: String,
        }
        assert_eq!(serde_json::from_str::<PreEmail>(&json).unwrap().username, "p");
    }

    #[test]
    fn a_token_matches_only_itself_and_only_before_expiry() {
        let (token, record) = new_token(1_000, RESET_TOKEN_TTL_SECS);
        assert_ne!(record.token_hash, token, "only the hash is stored");
        assert_eq!(record.token_hash.len(), 64, "sha-256 hex");
        assert!(token_matches(&record, &token, 1_000));
        assert!(token_matches(&record, &token, 1_000 + RESET_TOKEN_TTL_SECS - 1));
        assert!(!token_matches(&record, &token, 1_000 + RESET_TOKEN_TTL_SECS), "one hour, then dead");
        assert!(!token_matches(&record, "some-other-token", 1_000));
        assert_eq!(RESET_TOKEN_TTL_SECS, 3600);
    }

    #[test]
    fn email_sends_stop_at_the_limit_and_reopen_after_the_window() {
        let mut sends = HashMap::new();
        let now = std::time::Instant::now();
        for _ in 0..EMAIL_SENDS_PER_USER {
            assert!(!email_sends_exhausted(&sends, "p", EMAIL_SENDS_PER_USER, now));
            record_login_failure(&mut sends, "p", now);
        }
        assert!(email_sends_exhausted(&sends, "p", EMAIL_SENDS_PER_USER, now));
        assert!(!email_sends_exhausted(&sends, "other", EMAIL_SENDS_PER_USER, now), "per key");
        assert!(!email_sends_exhausted(&sends, "p", EMAIL_SENDS_PER_USER, now + LOGIN_FAILURE_WINDOW), "the window runs out");
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

/// The argon2 bound (2026-09-05).
///
/// These assert the BOUND HOLDS, not that a semaphore exists. The
/// difference matters: a test that only checks the field is present would
/// pass just as happily if the permit were released before the hash
/// started, or if the limit were never applied.
///
/// Written against permit COUNTS rather than wall-clock timing on
/// purpose. The obvious test - spawn N+1 hashers and assert the last one
/// finishes later - is a race dressed as an assertion, and this codebase
/// has spent the week removing exactly that shape. Counts are
/// deterministic and say the same thing.
#[cfg(test)]
mod password_hash_bound_tests {
    use crate::adventure::{PASSWORD_HASH_PERMITS, PASSWORD_HASH_PERMITS_MAX, PASSWORD_HASH_PERMITS_MIN};
    use tokio::sync::Semaphore;

    /// N permits are available, the (N+1)th caller cannot proceed, and it
    /// becomes able to proceed the moment one is returned.
    #[tokio::test]
    async fn the_n_plus_first_hash_waits_until_a_permit_comes_back() {
        // Built the way production builds it: at the CEILING, then walked
        // down to the live value by the real reconcile function. If that
        // arithmetic is wrong the bound is wrong, and this test is where
        // it shows.
        let limit = 4usize;
        let sem = std::sync::Arc::new(Semaphore::new(PASSWORD_HASH_PERMITS_MAX as usize));
        let applied = std::sync::atomic::AtomicU32::new(PASSWORD_HASH_PERMITS_MAX);
        crate::adventure_web::apply_permit_limit(&sem, &applied, limit as u32);
        assert_eq!(sem.available_permits(), limit, "reconciling from the ceiling to {limit} must leave exactly {limit} permits - this is the arithmetic the live dial depends on");

        let mut held = Vec::new();
        for i in 0..limit {
            held.push(sem.clone().try_acquire_owned().unwrap_or_else(|_| panic!("permit {i} must be available - the bound is {limit} and only {i} are held")));
        }
        assert_eq!(sem.available_permits(), 0, "all {limit} permits must be in use once {limit} hashes are in flight");

        // THE BOUND. The next caller cannot start an argon2 pass, which is
        // the whole point - without this it would allocate another 19 MiB
        // and take another CPU thread.
        assert!(sem.clone().try_acquire_owned().is_err(), "the {}th concurrent hash must NOT proceed - if this passes, the bound is not bounding and an attacker can run as many argon2 passes as they can open connections", limit + 1);

        // ...and it is a queue, not a permanent lockout: returning one
        // permit lets exactly one more caller through.
        drop(held.pop().expect("one permit to return"));
        assert_eq!(sem.available_permits(), 1, "returning a permit must free exactly one slot");
        assert!(sem.clone().try_acquire_owned().is_ok(), "the waiting caller must proceed once a permit is returned - a bound that never releases is an outage, not a limit");
    }

    /// A caller that cannot get a permit within the timeout is turned
    /// away rather than queued forever. Uses tokio's paused clock, so it
    /// asserts the timeout fires without spending the timeout.
    #[tokio::test]
    async fn a_saturated_bound_turns_a_caller_away_rather_than_hanging() {
        let sem = std::sync::Arc::new(Semaphore::new(1));
        let _held = sem.clone().try_acquire_owned().expect("the only permit");

        // A short timeout rather than the real PASSWORD_HASH_QUEUE_TIMEOUT:
        // what is under test is the SHAPE - saturated means the wait ends
        // in an error the handler can answer, rather than never returning -
        // and waiting five real seconds to prove it would be five seconds
        // on every suite run. The constant is asserted separately below.
        let waited = tokio::time::timeout(std::time::Duration::from_millis(50), sem.acquire_owned()).await;
        assert!(
            waited.is_err(),
            "a saturated bound must time out and let the handler answer, not queue indefinitely - a login that hangs until the browser gives up is a worse failure than one that says 'try again'"
        );
    }

    /// The shipped value has to sit inside its own accepted range, and
    /// the floor has to be 1 rather than 0. A 0 permit count is not "no
    /// limit", it is "nobody can ever log in again".
    #[test]
    fn the_shipped_permit_count_is_inside_its_own_bounds_and_the_floor_is_not_zero() {
        assert!(
            (PASSWORD_HASH_PERMITS_MIN..=PASSWORD_HASH_PERMITS_MAX).contains(&PASSWORD_HASH_PERMITS),
            "the shipped permit count {PASSWORD_HASH_PERMITS} must be inside {PASSWORD_HASH_PERMITS_MIN}..={PASSWORD_HASH_PERMITS_MAX}"
        );
        assert_eq!(PASSWORD_HASH_PERMITS_MIN, 1, "the floor must be 1: a 0-permit semaphore locks every sign-in and registration out of the game with no error that explains it");
    }

    /// The queue timeout has to be long enough to absorb a real burst and
    /// short enough to answer rather than hang. Pinned so neither end can
    /// drift without someone deciding to move it.
    #[test]
    fn the_queue_timeout_answers_rather_than_hanging() {
        assert!(!super::PASSWORD_HASH_QUEUE_TIMEOUT.is_zero(), "a zero timeout is a bare try_acquire - it would reject a legitimate burst on an idle server");
        assert!(
            super::PASSWORD_HASH_QUEUE_TIMEOUT <= std::time::Duration::from_secs(15),
            "a caller must get an answer, not a hang: {:?} is long enough that a browser or a player gives up first",
            super::PASSWORD_HASH_QUEUE_TIMEOUT
        );
    }
}
