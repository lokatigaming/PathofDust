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
                    Html(render_page(&login_page_html(Some("The server is busy checking sign-ins right now. Please try again in a moment.")))),
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
        return (StatusCode::UNAUTHORIZED, Html(render_page(&login_page_html(Some("Incorrect username or password."))))).into_response();
    };

    // Cleared on success, so a player who eventually remembers their
    // password starts clean rather than carrying a ladder they can only
    // wait out.
    state.login_failures.lock().await.remove(&key);

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
