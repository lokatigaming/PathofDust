// Viewer-facing web dashboard for the chat adventure game (see
// adventure.rs) — unlike public_adventure_overlay (OBS-only, no auth,
// pushed state), this is a real multi-user site: each viewer logs in with
// their OWN Twitch account (OAuth authorization code flow, no scopes
// requested — all that's needed is confirming who they are via
// GET /users) and sees/manages their own character, matched by their
// lowercased Twitch login (same id AdventureManager already keys
// characters by).
//
// Sessions are an opaque random token in an HttpOnly cookie, mapped to
// (login, display_name) in an in-memory store - same "restart just clears
// it, nobody re-persists this" tradeoff as every other cooldown/session
// map in adventure.rs. Nothing here touches combat/loot logic directly;
// it only reads/joins through AdventureManager's existing public API.
//
// Reuses the bot's own TWITCH_CLIENT_ID/SECRET (see config.rs) rather
// than needing a second registered app - Twitch apps support multiple
// redirect URIs, so this just needs its own
// (`{ADVENTURE_WEB_PUBLIC_URL}/auth/callback`) added alongside the
// existing one-time-setup URI in the app's dashboard settings.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Json, Redirect};
use axum::routing::{get, post};
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::adventure::{
    affix_display, affix_name, affix_quality_percent, craft_affix_value_range, recent_fights, recent_summary_fights, summarize_fight, AdventureManager, Affix, Archetype,
    AutoDisenchantTier, Character, CombatEvent, CraftAction, CraftError, CraftOutcome, CraftResult, EncounterKind, EquipSlot, FightSummarySnapshot, Item, LastFightSnapshot,
    LiveTunables, PassiveError, PassivePreview, PendingVeil,
    PendingVeilAction, RecombineError, RecombineOutcome, RecombineResult, ReforgeOutcome, SetSecondaryArchetypeError, StatBreakdown, VeilCandidate, VeilChosenOutcome,
    ALL_ARCHETYPES, ALL_SPRITES, ARCHETYPE_CHANGE_COST, COARSE_FIGHTS_CAPACITY, INVENTORY_CAPACITY, LIFE_LEECH_CAP_PER_SEC, MODEL_CHANGES_FREE_FOR_ALL, MODEL_CHANGE_COST,
    NICKNAME_MAX_LEN, PASSIVE_RESPEC_COST, RETREAT_REPAIR_DURATION, SUMMARY_FIGHTS_CAPACITY, VEIL_EXTRA_COST, WEB_REFORGE_DUST_COST, WINGS_COST,
};
use crate::passive_tree::{PassiveNode, PassiveTier};

const SESSION_COOKIE: &str = "adv_session";
/// How long a login lasts before needing to sign in again.
const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How long an in-flight OAuth attempt's CSRF `state` token stays valid -
/// just long enough to actually complete the Twitch login page.
const OAUTH_STATE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Session {
    login: String,
    display_name: String,
    /// Seconds since UNIX_EPOCH, not `Instant` - this map is persisted to
    /// disk (see `sessions_path`) so a login survives a deploy restart,
    /// and `Instant` has no meaning across a process boundary.
    created_at: u64,
}

#[derive(Clone)]
struct AppState {
    adventure: Arc<AdventureManager>,
    client_id: String,
    client_secret: String,
    /// e.g. "https://adventure.example.com" - no trailing slash.
    public_url: String,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    sessions_path: PathBuf,
    oauth_states: Arc<Mutex<HashMap<String, Instant>>>,
    http: reqwest::Client,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Sessions survive a restart (see `Session::created_at`'s doc) - every
/// insert/remove re-persists the whole map so a deploy never silently
/// forces everyone to relog, the original complaint this was built for.
fn save_sessions(state: &AppState, sessions: &HashMap<String, Session>) {
    if let Err(err) = crate::state::save_json(&state.sessions_path, sessions) {
        tracing::error!("Failed to persist sessions to {}: {err}", state.sessions_path.display());
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn redirect_uri(public_url: &str) -> String {
    format!("{public_url}/auth/callback")
}

/// Nicer display label for a sprite name, e.g. `"melee-knight-red"` ->
/// `"Knight Red"` - drops the melee/ranged/support prefix (the picker
/// doesn't care about a model's original role bucket, it's purely
/// cosmetic) and title-cases the rest.
fn sprite_label(name: &str) -> String {
    name.splitn(2, '-')
        .nth(1)
        .unwrap_or(name)
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn start_adventure_web_server(
    port: u16,
    public_url: String,
    client_id: String,
    client_secret: String,
    adventure: Arc<AdventureManager>,
    sessions_path: PathBuf,
) -> anyhow::Result<()> {
    let sessions: HashMap<String, Session> = crate::state::load_json(&sessions_path).unwrap_or_default();
    let state = AppState {
        adventure,
        client_id,
        client_secret,
        public_url,
        sessions: Arc::new(Mutex::new(sessions)),
        sessions_path,
        oauth_states: Arc::new(Mutex::new(HashMap::new())),
        http: reqwest::Client::new(),
    };

    let app = axum::Router::new()
        .route("/", get(index))
        .route("/inventory", get(inventory_page))
        .route("/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/logout", get(logout))
        .route("/join", post(do_join))
        .route("/equip", post(do_equip))
        .route("/unequip", post(do_unequip))
        .route("/disenchant", post(do_disenchant))
        .route("/disenchant-all", post(do_disenchant_all))
        .route("/toggle-disenchant-protect", post(do_toggle_disenchant_protect))
        .route("/reforge", post(do_reforge))
        .route("/repair-equipped", post(do_repair_equipped))
        .route("/repair-item", post(do_repair_item))
        .route("/repair-all", post(do_repair_all))
        .route("/change-archetype", post(do_change_archetype))
        .route("/change-model", post(do_change_model))
        .route("/purchase-wings", post(do_purchase_wings))
        .route("/toggle-flying", post(do_toggle_flying))
        .route("/toggle-auto-repair", post(do_toggle_auto_repair))
        .route("/set-auto-disenchant", post(do_set_auto_disenchant))
        .route("/name-item", post(do_name_item))
        .route("/craft", post(do_craft))
        .route("/craft/choose-veil", post(do_choose_veil))
        .route("/passives", get(passives_page))
        .route("/passives/allocate", post(do_allocate_passive))
        .route("/passives/save", post(do_save_passives))
        .route("/passives/reset", post(do_reset_passive_preview))
        .route("/passives/respec", post(do_respec_passives))
        .route("/passives/set-secondary", post(do_set_secondary_archetype))
        .route("/patch-notes", get(patch_notes))
        .route("/wiki", get(wiki_page))
        .route("/characters", get(character_list))
        .route("/characters/:login", get(character_detail))
        .route("/characters/:login/passives", get(character_passives_readonly))
        .route("/fights", get(fights_page))
        .route("/fights.json", get(fights_json))
        .route("/admin/tunables", get(admin_tunables_page))
        .route("/admin/tunables/save", post(do_save_tunables))
        .route("/overlay", get(overlay_page))
        .route("/ws", get(overlay_ws_handler))
        .with_state(state)
        // Same sprite art the OBS overlay uses (public_adventure_overlay/
        // sprites/*.png) - served here too so the dashboard can show a
        // character's sprite without needing it reachable via the
        // OBS-only overlay server (port 4004, not publicly exposed).
        .nest_service("/sprites", tower_http::services::ServeDir::new("public_adventure_overlay/sprites"))
        // Skill-effect gifs (Flicker Strike's dash, Dragon's Breath) -
        // the other asset folder `/overlay` (below) needs, same
        // "duplicate it here rather than expose port 4004" reasoning as
        // `/sprites` above.
        .nest_service("/skill-effects", tower_http::services::ServeDir::new("public_adventure_overlay/skill-effects"));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Adventure web dashboard server crashed: {err}");
        }
    });

    tracing::info!("Adventure character dashboard running on port {port}.");
    Ok(())
}

/// Pulls the session token out of the `Cookie` header and resolves it
/// against the store - `None` for no cookie, an unrecognized/expired one.
/// Expired sessions are lazily dropped here rather than on a timer.
async fn current_session(headers: &HeaderMap, state: &AppState) -> Option<(String, String)> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    let token = cookie_header.split(';').map(|p| p.trim()).find_map(|p| p.strip_prefix(&format!("{SESSION_COOKIE}=")))?;

    let mut sessions = state.sessions.lock().await;
    let session = sessions.get(token)?;
    if now_secs().saturating_sub(session.created_at) > SESSION_TTL.as_secs() {
        sessions.remove(token);
        save_sessions(state, &sessions);
        return None;
    }
    Some((session.login.clone(), session.display_name.clone()))
}

fn set_cookie_header(token: &str, max_age_secs: u64) -> (header::HeaderName, String) {
    (header::SET_COOKIE, format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}"))
}

fn clear_cookie_header() -> (header::HeaderName, String) {
    (header::SET_COOKIE, format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Abbreviates a large character-sheet number with a K/M/B/T suffix
/// (1,000 -> "1K", 1,500 -> "1.5K", 1,000,000 -> "1M", etc.) - dust, XP,
/// HP, DPS/HPS, win/loss counts and item power all grow large enough over
/// a long-lived character (and after the recent tier-growth-on-craft
/// change - see `Item::sync_tier_to`) that the raw digit string stops
/// being readable at a glance. Anything under 1000 is shown as a plain
/// whole number, unchanged. One decimal place, trimmed when it'd just be
/// ".0" (1,500 -> "1.5K", but 2,000 -> "2K" not "2.0K").
pub fn format_number(n: f64) -> String {
    let sign = if n < 0.0 { "-" } else { "" };
    let abs = n.abs();
    let (scaled, suffix) = if abs >= 1e12 {
        (abs / 1e12, "T")
    } else if abs >= 1e9 {
        (abs / 1e9, "B")
    } else if abs >= 1e6 {
        (abs / 1e6, "M")
    } else if abs >= 1e3 {
        (abs / 1e3, "K")
    } else {
        return format!("{sign}{abs:.0}");
    };
    let rounded = (scaled * 10.0).round() / 10.0;
    let text = if rounded.fract() == 0.0 { format!("{rounded:.0}") } else { format!("{rounded:.1}") };
    format!("{sign}{text}{suffix}")
}

/// Combat logging (2026-08-15, a live request) - a plain UTC timestamp
/// for `LastFightSnapshot::started_at_unix_ms` on the streamer-only
/// `/fights` page, so "when did this fight happen" is finally answerable
/// (see that field's own doc for why it didn't used to be). Falls back
/// to the raw epoch seconds if `chrono` somehow can't parse them (should
/// never happen for a value this codebase generated itself).
fn format_unix_secs(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_else(|| format!("epoch {secs}"))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct IndexParams {
    reforged: Option<String>,
    item: Option<String>,
    slot: Option<String>,
    old_tier: Option<u32>,
    new_tier: Option<u32>,
    /// Set by `do_craft`/`do_choose_veil` after ANY of the unified
    /// Crafting card's actions actually applies - see `render_craft_popup`.
    /// Separate marker from `reforged` since reforge keeps its own
    /// existing popup/icon; both share the `item`/`slot` fields above,
    /// `crafted` additionally uses `tier`/`change` below.
    crafted: Option<String>,
    tier: Option<u32>,
    change: Option<String>,
    /// Set by `do_craft` when a crafting-card action (currency or
    /// Recombine) came back `Err` - see `render_craft_error_popup`. This
    /// used to just silently redirect back with no feedback at all,
    /// which read as "the game did nothing" (a live report against
    /// Recombine specifically - same silent-failure shape regardless of
    /// which precondition actually tripped, e.g. picking the same item
    /// twice, an empty second selection, a locked item, or - for a
    /// veiled/free-token roll - any of those surfacing from deep inside
    /// `roll_recombine`'s `?` instead of the outer cost check).
    craft_failed: Option<String>,
    /// Set by `do_disenchant` after a single-item disenchant actually
    /// grants dust - see `render_disenchant_popup`. Reuses `item` above;
    /// `dust`/`dust_max` are this popup's own.
    disenchanted: Option<String>,
    dust: Option<u32>,
    dust_max: Option<u32>,
}

async fn index(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<IndexParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, display_name)) => {
            let character = state.adventure.character(&login).await;
            let (used_this_hour, next_reset_ms) = state.adventure.reforge_status(&login).await;
            let popup = if params.reforged.is_some() { render_reforge_popup(&params) } else { String::new() };
            format!("{popup}{}", render_dashboard(&login, &display_name, character.as_ref(), used_this_hour, next_reset_ms))
        }
    };
    Html(render_page(&body))
}

/// `/inventory` - Bag and Crafting, split out of the main dashboard (see
/// `render_dashboard`) into their own page so the dashboard itself stays
/// focused on equipped gear/stats, per a live request that the dashboard
/// was getting cluttered. Handles the SAME `crafted`/`craft_failed`
/// popups `index` used to (see `do_craft`/`do_choose_veil`, which now
/// redirect here instead of `/`) - `reforged` stays index's alone, since
/// Reforge itself stayed on the main dashboard (it acts on equipped gear
/// directly, not the bag).
async fn inventory_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<IndexParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, display_name)) => {
            let character = state.adventure.character(&login).await;
            let pending_veil = state.adventure.pending_veil(&login).await;
            let popup = if params.crafted.is_some() {
                render_craft_popup(&params)
            } else if params.craft_failed.is_some() {
                render_craft_error_popup(&params)
            } else if params.disenchanted.is_some() {
                render_disenchant_popup(&params)
            } else {
                String::new()
            };
            format!("{popup}{}", render_inventory_page(&display_name, character.as_ref(), pending_veil.as_ref()))
        }
    };
    Html(render_page(&body))
}

/// Shown once, right after the web "Reforge Now" button (see do_reforge)
/// redirects back here with the result in the query string - a
/// POST-redirect-GET flow has nowhere else to carry "what just happened"
/// across the redirect. `history.replaceState` on dismiss strips the
/// query string so refreshing the page doesn't re-show it.
fn render_reforge_popup(params: &IndexParams) -> String {
    let item = escape_html(params.item.as_deref().unwrap_or("something"));
    let slot = escape_html(params.slot.as_deref().unwrap_or(""));
    let old_tier = params.old_tier.unwrap_or(0);
    let new_tier = params.new_tier.unwrap_or(0);
    format!(
        "<div class=\"modal-backdrop\" id=\"reforge-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">🔥</div>\
            <h2>Reforged!</h2>\
            <p>Your {slot} is now a <strong>{item}</strong></p>\
            <p class=\"modal-tier\">Tier {old_tier} → Tier {new_tier}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('reforge-modal').remove(); history.replaceState(null, '', '/');\">Nice!</button>\
          </div>\
        </div>"
    )
}

/// Shown once after ANY of the unified Crafting card's actions (see
/// `render_crafting_card`) actually applies - the six currency crafts
/// AND Recombine, whether committed immediately or via a veiled choice
/// (see `do_craft`/`do_choose_veil`, which build the redirect URL this
/// reads). Same POST-redirect-GET query-string-carry-through and
/// self-dismissing/URL-cleaning pattern as `render_reforge_popup` - kept
/// as its own popup/marker (`crafted`, not `reforged`) since reforge
/// already has a working one of its own.
fn render_craft_popup(params: &IndexParams) -> String {
    let item = escape_html(params.item.as_deref().unwrap_or("something"));
    let slot = escape_html(params.slot.as_deref().unwrap_or(""));
    let tier = params.tier.unwrap_or(0);
    let change = escape_html(params.change.as_deref().unwrap_or(""));
    format!(
        "<div class=\"modal-backdrop\" id=\"craft-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">✨</div>\
            <h2>Crafted!</h2>\
            <p>Your {slot} is now a <strong>{item}</strong> (Tier {tier})</p>\
            <p class=\"modal-tier\">{change}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('craft-modal').remove(); history.replaceState(null, '', '/'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
          </div>\
        </div>"
    )
}

/// Shown once when a crafting-card action (currency or Recombine) came
/// back `Err` from `do_craft` - see `IndexParams::craft_failed`. Same
/// popup pattern as `render_craft_popup`/`render_reforge_popup`, just
/// reporting why nothing happened instead of what changed.
/// Shown once after a single-item disenchant (see `do_disenchant`) - same
/// POST-redirect-GET query-string-carry-through and self-dismissing/
/// URL-cleaning pattern as the other three popups here. Disenchant All
/// stays silent (it's a bulk cleanup action, not a single roll worth
/// showing) - this is only wired to the single-item button.
fn render_disenchant_popup(params: &IndexParams) -> String {
    let item = escape_html(params.item.as_deref().unwrap_or("something"));
    let dust = params.dust.unwrap_or(0);
    let dust_max = params.dust_max.unwrap_or(0).max(1);
    let percent = ((dust as f64 / dust_max as f64) * 100.0).round() as u32;
    format!(
        "<div class=\"modal-backdrop\" id=\"disenchant-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">\u{1f4a8}</div>\
            <h2>Disenchanted!</h2>\
            <p>Your <strong>{item}</strong> broke down into <strong>{dust} Dust</strong></p>\
            <p class=\"modal-tier\">{percent}% of the {dust_max} possible</p>\
            <button class=\"btn\" onclick=\"document.getElementById('disenchant-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
          </div>\
        </div>"
    )
}

fn render_craft_error_popup(params: &IndexParams) -> String {
    let reason = escape_html(params.craft_failed.as_deref().unwrap_or("Something went wrong."));
    format!(
        "<div class=\"modal-backdrop\" id=\"craft-error-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">⚠️</div>\
            <h2>Nothing Happened</h2>\
            <p>{reason}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('craft-error-modal').remove(); history.replaceState(null, '', '/'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">OK</button>\
          </div>\
        </div>"
    )
}

async fn login(State(state): State<AppState>) -> Redirect {
    let csrf = random_token();
    {
        let mut states = state.oauth_states.lock().await;
        // Lazily prune expired attempts while we're in here anyway -
        // this map only ever grows from actual login attempts, so it
        // stays small regardless.
        states.retain(|_, &mut created| created.elapsed() < OAUTH_STATE_TTL);
        states.insert(csrf.clone(), Instant::now());
    }

    let mut url = url::Url::parse("https://id.twitch.tv/oauth2/authorize").expect("static URL");
    url.query_pairs_mut()
        .append_pair("client_id", &state.client_id)
        .append_pair("redirect_uri", &redirect_uri(&state.public_url))
        .append_pair("response_type", "code")
        // No scopes requested - GET /users (called below) returns the
        // token's own owner with a bare, scopeless user token, and that's
        // all this needs: confirming who's logging in.
        .append_pair("scope", "")
        .append_pair("state", &csrf);

    Redirect::to(url.as_str())
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct HelixUser {
    login: String,
    display_name: String,
}

#[derive(Deserialize)]
struct HelixUsersResponse {
    data: Vec<HelixUser>,
}

async fn callback(State(state): State<AppState>, Query(params): Query<CallbackParams>) -> impl IntoResponse {
    match handle_callback(&state, params).await {
        Ok((token, login)) => {
            tracing::info!("Adventure dashboard: {login} logged in.");
            let cookie = set_cookie_header(&token, SESSION_TTL.as_secs());
            (StatusCode::FOUND, [cookie, (header::LOCATION, "/".to_string())], "").into_response()
        }
        Err(err) => {
            tracing::warn!("Adventure dashboard login failed: {err}");
            Html(render_page(&format!(
                "<div class=\"card\"><h2>Login failed</h2><p>{}</p><p><a class=\"btn\" href=\"/login\">Try again</a></p></div>",
                escape_html(&err.to_string())
            )))
            .into_response()
        }
    }
}

async fn handle_callback(state: &AppState, params: CallbackParams) -> anyhow::Result<(String, String)> {
    if let Some(desc) = params.error_description {
        anyhow::bail!("{desc}");
    }
    let code = params.code.ok_or_else(|| anyhow::anyhow!("Twitch didn't return an authorization code."))?;
    let csrf = params.state.ok_or_else(|| anyhow::anyhow!("Missing state parameter."))?;

    {
        let mut states = state.oauth_states.lock().await;
        // Single-use: removed the moment it's checked, whether or not it
        // turns out valid, so a replayed callback URL can never succeed twice.
        let Some(created) = states.remove(&csrf) else {
            anyhow::bail!("Login link expired or already used — try logging in again.");
        };
        if created.elapsed() > OAUTH_STATE_TTL {
            anyhow::bail!("Login link expired — try logging in again.");
        }
    }

    let resp = state
        .http
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[
            ("client_id", state.client_id.as_str()),
            ("client_secret", state.client_secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", &redirect_uri(&state.public_url)),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Twitch rejected the login (token exchange failed).");
    }
    let token: TokenResponse = resp.json().await?;

    let resp = state
        .http
        .get("https://api.twitch.tv/helix/users")
        .bearer_auth(&token.access_token)
        .header("Client-Id", &state.client_id)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Couldn't confirm your Twitch identity.");
    }
    let users: HelixUsersResponse = resp.json().await?;
    let user = users.data.into_iter().next().ok_or_else(|| anyhow::anyhow!("Twitch returned no user for that login."))?;

    let session_token = random_token();
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_token.clone(), Session { login: user.login.clone(), display_name: user.display_name, created_at: now_secs() });
        save_sessions(state, &sessions);
    }

    Ok((session_token, user.login))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = cookie_header.split(';').map(|p| p.trim()).find_map(|p| p.strip_prefix(&format!("{SESSION_COOKIE}="))) {
            let mut sessions = state.sessions.lock().await;
            sessions.remove(token);
            save_sessions(&state, &sessions);
        }
    }
    (StatusCode::FOUND, [clear_cookie_header(), (header::LOCATION, "/".to_string())], "")
}

async fn do_join(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, display_name)) = current_session(&headers, &state).await {
        let _ = state.adventure.join(&login, &display_name).await;
    }
    Redirect::to("/")
}

#[derive(Deserialize)]
struct ItemIdForm {
    item_id: String,
}

#[derive(Deserialize)]
struct SlotForm {
    slot: EquipSlot,
}

#[derive(Deserialize)]
struct ArchetypeForm {
    archetype: Archetype,
}

#[derive(Deserialize)]
struct ModelForm {
    model: String,
}

#[derive(Deserialize)]
struct NameItemForm {
    item_id: String,
    #[serde(default)]
    nickname: String,
}

#[derive(Deserialize)]
struct CraftForm {
    item_a: String,
    #[serde(default)]
    item_b: String,
    action: String,
    /// A checkbox only shows up in the form body at all when checked -
    /// `#[serde(default)]` is what lets an unchecked box (the field
    /// simply absent) deserialize as `None` instead of a hard 422.
    #[serde(default)]
    veiled: Option<String>,
    /// x5/x10 batch repeat count for Polishing/Reforge only (see the
    /// hidden `times` input in the Polish/Reforge section of
    /// `render_crafting_card`) - every other action ignores this
    /// entirely, even if a stale value is still sitting in the field
    /// from a prior checkbox selection.
    #[serde(default)]
    times: Option<u32>,
    /// Hideout Warrior only - whether its run includes the final Krangle
    /// step (see `HIDEOUT_WARRIOR_STEPS`/`do_hideout_warrior`). Checked by
    /// default in the form itself, so an absent field only happens when
    /// the player explicitly unchecked it.
    #[serde(default)]
    hideout_krangle: Option<String>,
}

#[derive(Deserialize)]
struct VeilChoiceForm {
    index: usize,
}

async fn do_equip(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ItemIdForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.equip_item(&login, &form.item_id).await;
    }
    Redirect::to("/")
}

async fn do_unequip(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SlotForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.unequip_item(&login, form.slot).await;
    }
    Redirect::to("/")
}

async fn do_disenchant(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ItemIdForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Some(outcome) = state.adventure.disenchant_item(&login, &form.item_id).await {
            // Same POST-redirect-GET query-string-carry-through as
            // render_reforge_popup/render_craft_popup.
            let url = format!(
                "/inventory?disenchanted=1&item={}&dust={}&dust_max={}",
                urlencoding::encode(&outcome.item_name),
                outcome.dust,
                outcome.dust_max,
            );
            return Redirect::to(&url);
        }
    }
    Redirect::to("/inventory")
}

/// Silent (no popup), same as every other web-only action here - the
/// now-empty(er) Bag and updated dust total on the next page load are
/// confirmation enough. Skips Krangled and disenchant-protected items
/// entirely - see `Character::disenchant_all_from_inventory`.
async fn do_disenchant_all(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.disenchant_all(&login).await;
    }
    Redirect::to("/inventory")
}

/// Silent (no popup) - the tick-box's own checked state on the next page
/// load is confirmation enough, same as every other toggle here.
async fn do_toggle_disenchant_protect(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ItemIdForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.toggle_disenchant_protect(&login, &form.item_id).await;
    }
    Redirect::to("/inventory")
}

/// Web-dashboard alternative to redeeming the "Reforge Gear" channel
/// points reward — draws from the SAME once-per-hour allowance (see
/// AdventureManager::try_claim_reforge_cooldown), it doesn't stack an
/// extra reforge on top of the redemption. Can't actually charge Twitch
/// channel points from a web click (no API for that outside a real
/// reward redemption), so it charges dust instead - see
/// WEB_REFORGE_DUST_COST/reforge_random_gear_for_dust. Silent either way
/// (no chat announcement), same as the other web-only actions (equip/
/// unequip/disenchant) - the redemption path is still what gets announced.
async fn do_reforge(State(state): State<AppState>, headers: HeaderMap) -> Redirect {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if state.adventure.try_claim_reforge_cooldown(&login).await.is_ok() {
            match state.adventure.reforge_random_gear_for_dust(&login).await {
                Some(outcome) => {
                    // Result carried across this redirect via the query
                    // string (see IndexParams/render_reforge_popup) - a
                    // POST-redirect-GET flow has no other way to hand
                    // "what just happened" to the page it lands on.
                    let url = format!(
                        "/?reforged=1&item={}&slot={:?}&old_tier={}&new_tier={}",
                        urlencoding::encode(&outcome.item_name),
                        outcome.slot,
                        outcome.old_tier,
                        outcome.new_tier,
                    );
                    return Redirect::to(&url);
                }
                None => state.adventure.release_reforge_cooldown(&login).await,
            }
        }
    }
    Redirect::to("/")
}

async fn do_repair_equipped(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SlotForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.repair_equipped_item(&login, form.slot).await;
    }
    Redirect::to("/")
}

async fn do_repair_item(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ItemIdForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.repair_inventory_item(&login, &form.item_id).await;
    }
    Redirect::to("/inventory")
}

async fn do_repair_all(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.repair_all_gear_for_dust(&login).await;
    }
    Redirect::to("/")
}

/// Silent either way (no popup), same as every other web-only action
/// here except reforge - the updated badge/Combat Stats card on the next
/// page load is confirmation enough (or, on failure, the still-Commoner
/// badge / unspent dust is).
async fn do_change_archetype(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ArchetypeForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.change_archetype(&login, form.archetype).await;
    }
    Redirect::to("/")
}

/// `/passives` - the character's own passive skill tree page (see
/// `render_passive_tree_page`). Same "logged out -> render_logged_out"
/// shape as every other page here.
#[derive(Deserialize, Default)]
#[serde(default)]
struct PassivesParams {
    /// Set by `do_allocate_passive`/`do_save_passives`/`do_respec_passives`
    /// when a passive-tree action came back `Err` - see
    /// `render_passive_error_popup`. Used to fix a live "I have points
    /// available but nothing happens when I click" report: every passive
    /// web handler used to discard the `Result` entirely, so a rejected
    /// click (over budget, missing prerequisite, or the real
    /// `InsufficientPoints` budget bug that turned out to be causing
    /// it) was indistinguishable from the page doing nothing. Same
    /// pattern `IndexParams::craft_failed` already established for the
    /// Crafting card's identically-shaped silent-failure report.
    passive_failed: Option<String>,
}

async fn passives_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<PassivesParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, display_name)) => {
            let character = state.adventure.character(&login).await;
            let preview = state.adventure.pending_passive_preview(&login).await;
            let popup = if params.passive_failed.is_some() { render_passive_error_popup(&params) } else { String::new() };
            format!("{popup}{}", render_passive_tree_page(&display_name, character.as_ref(), preview.as_ref()))
        }
    };
    Html(render_page(&body))
}

/// Shown once when a passive-tree action came back `Err` - see
/// `PassivesParams::passive_failed`. Same popup pattern as
/// `render_craft_error_popup`, just reporting why nothing happened on
/// `/passives` instead of `/inventory`.
fn render_passive_error_popup(params: &PassivesParams) -> String {
    let reason = escape_html(params.passive_failed.as_deref().unwrap_or("Something went wrong."));
    format!(
        "<div class=\"modal-backdrop\" id=\"passive-error-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">⚠️</div>\
            <h2>Nothing Happened</h2>\
            <p>{reason}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('passive-error-modal').remove(); history.replaceState(null, '', '/passives');\">OK</button>\
          </div>\
        </div>"
    )
}

/// Player-facing reason a passive-tree action didn't go through - see
/// `PassivesParams::passive_failed`/`render_passive_error_popup`.
fn passive_error_text(err: PassiveError) -> String {
    match err {
        PassiveError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        PassiveError::NodeNotFound => "That passive node doesn't exist for your class.".to_string(),
        PassiveError::ParentNotInvested => "You need to invest in this node's prerequisite first.".to_string(),
        PassiveError::MaxRankReached => "That node is already at its max rank.".to_string(),
        PassiveError::InsufficientPoints => "Not enough passive points available for that.".to_string(),
        PassiveError::InsufficientDust(cost) => format!("Not enough dust — this needs {cost}."),
    }
}

/// Query string for `render_passive_error_popup`.
fn passive_error_popup_url(reason: &str) -> String {
    format!("/passives?passive_failed={}", urlencoding::encode(reason))
}

#[derive(Deserialize)]
struct PassiveAllocateForm {
    node_key: String,
    delta: i32,
    /// Which tree this click targets (see `PassivePreview`) - `false`
    /// (the default, so the primary tree's existing forms don't need
    /// changing) targets the primary archetype's tree; `true` targets
    /// Split Personality's secondary tree.
    #[serde(default)]
    secondary: bool,
}

/// Every click on a node's +/- button lands here - mutates the PREVIEW
/// only (see `AdventureManager::preview_allocate_passive`), never the
/// real saved tree. An `Err` shows its own popup (see
/// `render_passive_error_popup`) instead of the old silent no-op
/// redirect.
async fn do_allocate_passive(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<PassiveAllocateForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.preview_allocate_passive(&login, &form.node_key, form.delta, form.secondary).await {
            return Redirect::to(&passive_error_popup_url(&passive_error_text(err)));
        }
    }
    Redirect::to("/passives")
}

async fn do_save_passives(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.save_passive_tree(&login).await {
            return Redirect::to(&passive_error_popup_url(&passive_error_text(err)));
        }
    }
    Redirect::to("/passives")
}

async fn do_reset_passive_preview(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.discard_passive_preview(&login).await;
    }
    Redirect::to("/passives")
}

async fn do_respec_passives(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.respec_passive_tree(&login).await {
            return Redirect::to(&passive_error_popup_url(&passive_error_text(err)));
        }
    }
    Redirect::to("/passives")
}

#[derive(Deserialize)]
struct SetSecondaryArchetypeForm {
    archetype: Archetype,
}

/// Split Personality's 2nd-class dropdown submit - see
/// `AdventureManager::set_secondary_archetype`.
async fn do_set_secondary_archetype(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SetSecondaryArchetypeForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.set_secondary_archetype(&login, form.archetype).await {
            let reason = match err {
                SetSecondaryArchetypeError::NotJoined => "You haven't joined the adventure yet.",
                SetSecondaryArchetypeError::InvalidChoice => "Commoner has no passive tree to pick as a 2nd class.",
                SetSecondaryArchetypeError::NotEquipped => "You need Split Personality equipped to pick a 2nd class.",
                SetSecondaryArchetypeError::SameAsPrimary => "Your 2nd class can't be the same as your primary class.",
            };
            return Redirect::to(&passive_error_popup_url(reason));
        }
    }
    Redirect::to("/passives")
}

/// Silent (no popup) same as every other web-only action here - the
/// updated avatar/sprite on the next page load (and live on the OBS
/// overlay - see `AdventureManager::change_model`) is confirmation enough.
async fn do_change_model(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<ModelForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.change_model(&login, form.model).await;
    }
    Redirect::to("/")
}

/// Silent (no popup) same as every other web-only action here - a
/// successful purchase shows up as the new "Wings of Flight" section
/// replacing the purchase button on the next page load (or, on failure,
/// the unspent dust / still-missing section is the tell).
async fn do_purchase_wings(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.purchase_wings(&login).await;
    }
    Redirect::to("/")
}

/// Silent (no popup) - the toggle button's own label (see
/// `render_wings_card`) flips to reflect the new state on the next page
/// load, and live on the OBS overlay via `broadcast_state`.
async fn do_toggle_flying(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let _ = state.adventure.toggle_flying(&login).await;
    }
    Redirect::to("/")
}

/// Silent (no popup) - the tick-box's own checked state on the next page
/// load is confirmation enough, same as every other toggle here.
async fn do_toggle_auto_repair(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.toggle_auto_repair(&login).await;
    }
    Redirect::to("/")
}

#[derive(Deserialize)]
struct AutoDisenchantForm {
    /// Present (any value) when the checkbox is checked, absent entirely
    /// from the POST body when it's unchecked - standard HTML checkbox
    /// behavior, not something a bool field can parse directly.
    #[serde(default)]
    enabled: Option<String>,
    tier: String,
    min_percent: u32,
}

/// One self-submitting form (checkbox + dropdown + number, see
/// `render_auto_disenchant_settings`) sets all 3 fields together in a
/// single POST, rather than 3 separate toggle-one-value endpoints.
async fn do_set_auto_disenchant(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<AutoDisenchantForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let tier = match form.tier.as_str() {
            "perfect" => AutoDisenchantTier::Perfect,
            "sacred" => AutoDisenchantTier::Sacred,
            _ => AutoDisenchantTier::Quality,
        };
        state.adventure.set_auto_disenchant(&login, form.enabled.is_some(), tier, form.min_percent).await;
    }
    Redirect::to("/inventory")
}

/// Names (or, submitted blank, permanently declines to name) a Krangled
/// item - see `render_nickname_prompt`. Free, silent (no popup) same as
/// every other web-only action here.
async fn do_name_item(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<NameItemForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        state.adventure.name_item(&login, &form.item_id, &form.nickname).await;
    }
    Redirect::to("/inventory")
}

fn parse_craft_action(s: &str) -> Option<CraftAction> {
    match s {
        "transmute" => Some(CraftAction::Transmute),
        "scour" => Some(CraftAction::Scour),
        "augment" => Some(CraftAction::Augment),
        "regal" => Some(CraftAction::Regal),
        "exalt" => Some(CraftAction::Exalt),
        "krangle" => Some(CraftAction::Krangle),
        "annulment orb" => Some(CraftAction::Annulment),
        "chancing" => Some(CraftAction::Chancing),
        "celestial shard" => Some(CraftAction::CelestialShard),
        "unique shard" => Some(CraftAction::UniqueShard),
        "polishing" => Some(CraftAction::Polishing),
        "reforge" => Some(CraftAction::Reforge),
        _ => None,
    }
}

/// Query string for `render_craft_popup` - same "carry the result across
/// a POST-redirect-GET" trick `do_reforge` uses for its own popup.
fn craft_popup_url(item_name: &str, slot: EquipSlot, tier: u32, change: &str) -> String {
    format!(
        "/inventory?crafted=1&item={}&slot={:?}&tier={}&change={}",
        urlencoding::encode(item_name),
        slot,
        tier,
        urlencoding::encode(change),
    )
}

/// One-line "what changed" for a currency craft's popup - Scour reports
/// how many modifiers it stripped; every affix-adding action reports the
/// one it added (plus a permanent-lock note for Krangle specifically).
fn craft_outcome_change_text(outcome: &CraftOutcome) -> String {
    if outcome.action == CraftAction::Scour {
        format!("Removed {} modifier{}", outcome.affixes_removed, if outcome.affixes_removed == 1 { "" } else { "s" })
    } else if outcome.action == CraftAction::Polishing {
        let affix_text = if outcome.polished_affixes.is_empty() {
            String::new()
        } else {
            let raised: Vec<String> = outcome.polished_affixes.iter().map(|(a, v)| format!("raised {}", affix_display(*a, *v))).collect();
            format!(" — {}", raised.join(", "))
        };
        match outcome.new_quality_percent {
            Some(q) => format!("Quality raised to {q:.0}%{affix_text}"),
            None => format!("Polished{affix_text}"),
        }
    } else if outcome.action == CraftAction::Annulment {
        match (outcome.affix_removed, outcome.affix_removed_value) {
            (Some(a), Some(v)) => format!("Removed {}", affix_display(a, v)),
            _ => "Removed a modifier".to_string(),
        }
    } else if outcome.action == CraftAction::Chancing {
        // Real before→after now that Chancing rerolls TYPES, not just
        // values (2026-08-17) - `chancing_previous`/`polished_affixes` are
        // parallel, same length, same index (see `CraftOutcome::chancing_previous`'s doc).
        let list: Vec<String> = outcome
            .chancing_previous
            .iter()
            .zip(outcome.polished_affixes.iter())
            .map(|(old, (new_a, new_v))| format!("{} → {}", affix_name(*old), affix_display(*new_a, *new_v)))
            .collect();
        if list.is_empty() { "Rerolled".to_string() } else { format!("Rerolled: {}", list.join(", ")) }
    } else {
        let affix_text = match (outcome.affix_added, outcome.affix_value) {
            (Some(a), Some(v)) => {
                let (lo, hi) = craft_affix_value_range(outcome.tier, a, outcome.perfect);
                format!("{} (range: {} – {})", affix_display(a, v), affix_display(a, lo), affix_display(a, hi))
            }
            _ => "a new modifier".to_string(),
        };
        if outcome.now_locked {
            format!("Added {affix_text} — permanently locked 🔒")
        } else {
            format!("Added {affix_text}")
        }
    }
}

/// One-line "what changed" for a crafting-panel Reforge's popup - reuses
/// the same "old_tier → new_tier" shape `render_reforge_popup` already
/// shows for the channel-points-style Reforge Now button, plus a note
/// for the rare bonus-affix crit (see `ReforgeOutcome::bonus_affix`).
fn reforge_outcome_change_text(outcome: &ReforgeOutcome) -> String {
    let tier_text = format!("Tier {} → Tier {}", outcome.old_tier, outcome.new_tier);
    match outcome.bonus_affix {
        Some(affix) => format!("{tier_text} — bonus modifier: {}", affix_name(affix)),
        None => tier_text,
    }
}

/// One-line "what changed" for a recombine's popup - just the bonus
/// modifier when the rare recomb crit landed (see `RecombineOutcome`,
/// which doesn't carry the full surviving-affix list, only the crit) -
/// no value attached, same "name only" convention the chat announcement
/// for this same crit already uses (see `AdventureManager::announce_gear_crit`).
fn recombine_outcome_change_text(outcome: &RecombineOutcome) -> String {
    match outcome.bonus_affix {
        Some(affix) => format!("Forged — bonus modifier: {}", affix_name(affix)),
        None => "Forged from two items".to_string(),
    }
}

/// Player-facing reason a Recombine attempt didn't go through - see
/// `IndexParams::craft_failed`/`render_craft_error_popup`.
fn recombine_error_text(err: RecombineError) -> String {
    match err {
        RecombineError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        RecombineError::ItemNotFound => "Pick two items to recombine — one of your selections was empty or no longer exists.".to_string(),
        RecombineError::ItemLocked => "A Krangled (locked) item can't be recombined.".to_string(),
        RecombineError::SameItem => "You picked the same item for both sides — choose two different items.".to_string(),
        RecombineError::SlotMismatch => "Both items need to be the same gear slot (e.g. two helms, two gloves).".to_string(),
        RecombineError::InsufficientDust(cost) => format!("Not enough dust — this needs {cost}."),
        RecombineError::IncompatibleUniqueAffixes => "Both items have a unique affix and they're not the same one — those can't be combined.".to_string(),
    }
}

/// Player-facing reason a currency craft attempt didn't go through - see
/// `IndexParams::craft_failed`/`render_craft_error_popup`.
fn craft_error_text(err: CraftError) -> String {
    match err {
        CraftError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        CraftError::ItemNotFound => "Pick an item first — your selection was empty or no longer exists.".to_string(),
        CraftError::ItemLocked => "A Krangled (locked) item can't be crafted on.".to_string(),
        CraftError::PreconditionNotMet => "That item doesn't have the right number of modifiers for this action.".to_string(),
        CraftError::NothingToRemove => "That item has no modifiers to remove.".to_string(),
        CraftError::NothingToReroll => "That item has no modifiers to reroll.".to_string(),
        CraftError::InsufficientDust(cost) => {
            if cost == u64::MAX {
                "You need an actual Celestial Shard for that — it can't be bought with dust.".to_string()
            } else {
                format!("Not enough dust — this needs {cost}.")
            }
        }
        CraftError::NoCandidatesLeft => "That item already has every possible modifier — nothing left to add.".to_string(),
        CraftError::AlreadyUnique => "That item already has a unique affix — only one per item.".to_string(),
        CraftError::CannotKrangleUnique => "An item with a unique affix can't be Krangled.".to_string(),
        CraftError::InsufficientSand(cost) => format!("Not enough sand — this needs {cost}."),
        CraftError::NothingToPolish => "That item's already maxed out — nothing left for Polishing to improve.".to_string(),
    }
}

/// Query string for `render_craft_error_popup`.
fn craft_error_popup_url(reason: &str) -> String {
    format!("/inventory?craft_failed={}", urlencoding::encode(reason))
}

/// Handles every button in the unified Crafting card - the six currency
/// actions AND Recombine all post here (see `render_crafting_card`),
/// with `action` telling this handler which one was actually clicked
/// (HTML's standard "multiple submit buttons, one form" pattern). Shows
/// the "what changed" popup (see `render_craft_popup`) whenever the
/// craft actually applied immediately; a veiled craft instead lands on
/// the "choose your outcome" card with no popup yet - that comes from
/// `do_choose_veil` once a candidate's actually picked. An `Err` shows
/// its OWN popup (see `render_craft_error_popup`) instead of the old
/// silent no-op redirect - a live report against Recombine ("the game
/// did nothing") turned out to be exactly that: any precondition failure
/// (same item picked twice, empty second selection, locked item, etc.)
/// previously gave zero feedback. A non-veiled recomb/currency crit still
/// gets its own chat announcement from the manager's broadcast channel
/// (see main.rs), not from anything this handler does directly.
async fn do_craft(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<CraftForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let veiled = form.veiled.is_some();
        if form.action == "recombine" {
            match state.adventure.recombine_gear(&login, &form.item_a, &form.item_b, veiled).await {
                Ok(RecombineResult::Applied(outcome)) => {
                    let change = recombine_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
                }
                Ok(RecombineResult::PendingChoice) => {}
                Err(err) => return Redirect::to(&craft_error_popup_url(&recombine_error_text(err))),
            }
        } else if form.action == "hideout warrior" {
            return do_hideout_warrior(&state, &login, &form.item_a, form.hideout_krangle.is_some()).await;
        } else if let Some(action) = parse_craft_action(&form.action) {
            // Only Polishing/Reforge get the x5/x10 batch treatment (see
            // the dedicated section in render_crafting_card) - every
            // other action ignores `times` even if the hidden input still
            // carries a stale value from a prior checkbox selection.
            let times = if matches!(action, CraftAction::Polishing | CraftAction::Reforge) {
                form.times.unwrap_or(1).clamp(1, 10)
            } else {
                1
            };
            if times > 1 {
                return do_craft_batch(&state, &login, &form.item_a, action, times).await;
            }
            match state.adventure.craft_item(&login, &form.item_a, action, veiled).await {
                Ok(CraftResult::Applied(outcome)) => {
                    let change = craft_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
                }
                Ok(CraftResult::Reforged(outcome)) => {
                    let change = reforge_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
                }
                Ok(CraftResult::PendingChoice) => {}
                Err(err) => return Redirect::to(&craft_error_popup_url(&craft_error_text(err))),
            }
        }
    }
    Redirect::to("/inventory")
}

/// Repeats a Polishing or Reforge craft against the same item `times` in
/// a row in one click (the x5/x10 checkboxes) - never veiled (neither
/// action is veilable in the first place). Stops as soon as one
/// iteration errors (out of sand/dust, item vanished, etc.) rather than
/// silently eating the failure, and the popup reports how many of the
/// requested repeats actually landed vs. how many were asked for.
async fn do_craft_batch(state: &AppState, login: &str, item_id: &str, action: CraftAction, times: u32) -> Redirect {
    let mut completed = 0u32;
    let mut last_applied: Option<CraftOutcome> = None;
    let mut last_reforged: Option<ReforgeOutcome> = None;
    // Reforge's own popup text (`reforge_outcome_change_text`) shows a
    // single craft's own old_tier -> new_tier - called on just the LAST
    // iteration's outcome, that would show only that one iteration's
    // narrow before/after (e.g. "139 -> 140" for the 10th reforge of a
    // x10 batch) instead of the true whole-batch range a player actually
    // cares about (e.g. "130 -> 140") - a live report against the first
    // version of this feature. Tracked separately here instead.
    let mut reforge_first_old_tier: Option<u32> = None;
    let mut reforge_bonus_affix: Option<Affix> = None;
    let mut error: Option<CraftError> = None;
    for _ in 0..times {
        match state.adventure.craft_item(login, item_id, action, false).await {
            Ok(CraftResult::Applied(outcome)) => {
                completed += 1;
                last_applied = Some(outcome);
            }
            Ok(CraftResult::Reforged(outcome)) => {
                completed += 1;
                if reforge_first_old_tier.is_none() {
                    reforge_first_old_tier = Some(outcome.old_tier);
                }
                if outcome.bonus_affix.is_some() {
                    reforge_bonus_affix = outcome.bonus_affix;
                }
                last_reforged = Some(outcome);
            }
            Ok(CraftResult::PendingChoice) => break,
            Err(err) => {
                error = Some(err);
                break;
            }
        }
    }
    if completed == 0 {
        let reason = error.map(craft_error_text).unwrap_or_else(|| "Nothing happened.".to_string());
        return Redirect::to(&craft_error_popup_url(&reason));
    }
    let prefix = if completed < times { format!("(x{completed} of {times} — ran out) ") } else { format!("(x{completed}) ") };
    if let Some(outcome) = last_applied {
        let change = format!("{prefix}{}", craft_outcome_change_text(&outcome));
        return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
    }
    if let Some(outcome) = last_reforged {
        let old_tier = reforge_first_old_tier.unwrap_or(outcome.old_tier);
        let tier_text = format!("Tier {old_tier} → Tier {}", outcome.new_tier);
        let change_body = match reforge_bonus_affix {
            Some(affix) => format!("{tier_text} — bonus modifier: {}", affix_name(affix)),
            None => tier_text,
        };
        let change = format!("{prefix}{change_body}");
        return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
    }
    Redirect::to("/inventory")
}

/// The fixed 5-step chain Hideout Warrior runs, in order - the only
/// sequence of existing `CraftAction::required_affix_count` preconditions
/// that takes a bare item all the way through to Krangled.
const HIDEOUT_WARRIOR_STEPS: [CraftAction; 5] = [CraftAction::Transmute, CraftAction::Augment, CraftAction::Regal, CraftAction::Exalt, CraftAction::Krangle];

/// Hideout Warrior (2026-08-17, a live request) - runs every step of
/// `HIDEOUT_WARRIOR_STEPS` against one item in a single click, always
/// non-veiled and always paying each step's real dust cost in full (never
/// a banked token - see `AdventureManager::craft_item_ex`'s
/// `allow_token_use` param). A step whose precondition doesn't currently
/// match the item (`PreconditionNotMet`/`ItemLocked`/`NoCandidatesLeft`)
/// just isn't eligible right now and is skipped, not treated as a
/// failure - this IS "run all 5 basic crafts if the item is eligible,"
/// no separate eligibility check needed since the existing preconditions
/// already are that check. Running out of dust mid-chain
/// (`InsufficientDust`) stops the whole run early; whatever already
/// landed stays applied (not transactional across the 5 steps, same as
/// `do_craft_batch`'s existing x5/x10 behavior). `include_krangle` (a
/// checkbox next to the button, checked by default - 2026-08-17, a live
/// request to make the permanent lock optional) drops the final Krangle
/// step from the chain entirely when false, so the run stops after Exalt
/// and the item stays unlocked.
async fn do_hideout_warrior(state: &AppState, login: &str, item_id: &str, include_krangle: bool) -> Redirect {
    let steps = if include_krangle { &HIDEOUT_WARRIOR_STEPS[..] } else { &HIDEOUT_WARRIOR_STEPS[..4] };
    let mut completed: Vec<CraftOutcome> = Vec::new();
    let mut hard_error: Option<CraftError> = None;
    for action in steps {
        match state.adventure.craft_item_ex(login, item_id, *action, false, false).await {
            Ok(CraftResult::Applied(outcome)) => completed.push(outcome),
            Ok(CraftResult::PendingChoice) | Ok(CraftResult::Reforged(_)) => {}
            Err(err @ (CraftError::NotJoined | CraftError::ItemNotFound | CraftError::InsufficientDust(_))) => {
                hard_error = Some(err);
                break;
            }
            // ItemLocked / PreconditionNotMet / NoCandidatesLeft - this
            // step just wasn't eligible right now, move on to the next.
            Err(_) => {}
        }
    }
    if completed.is_empty() {
        let reason = hard_error.map(craft_error_text).unwrap_or_else(|| "None of Hideout Warrior's steps were eligible on that item.".to_string());
        return Redirect::to(&craft_error_popup_url(&reason));
    }
    let labels: Vec<&str> = completed.iter().map(|o| o.action.label()).collect();
    let prefix = if completed.len() < steps.len() {
        format!("(x{} of {} — {}) ", completed.len(), steps.len(), labels.join(", "))
    } else {
        format!("(all {}: {}) ", steps.len(), labels.join(", "))
    };
    let last = completed.last().expect("completed checked non-empty above");
    let change = format!("{prefix}{}", craft_outcome_change_text(last));
    Redirect::to(&craft_popup_url(&last.item_name, last.slot, last.tier, &change))
}

/// Applies whichever of the 3 rolled candidates the player picked on a
/// veiled craft (see `render_veil_choice_card`/
/// `AdventureManager::choose_veil_outcome`) and shows the same "what
/// changed" popup a non-veiled craft gets from `do_craft`.
async fn do_choose_veil(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<VeilChoiceForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        match state.adventure.choose_veil_outcome(&login, form.index).await {
            Some(VeilChosenOutcome::Currency(outcome)) => {
                let change = craft_outcome_change_text(&outcome);
                return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
            }
            Some(VeilChosenOutcome::Recombine(outcome)) => {
                let change = recombine_outcome_change_text(&outcome);
                return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
            }
            // A veiled Chancing step was applied but more affix slots
            // remain this pass - a fresh PendingVeil for the next slot is
            // already inserted, so redirect back silently (same panel,
            // next slot) instead of showing a popup. See
            // `VeilChosenOutcome::ChancingContinues`'s own doc.
            Some(VeilChosenOutcome::ChancingContinues) | None => {}
        }
    }
    Redirect::to("/inventory")
}

#[derive(Deserialize, Default)]
struct PatchNoteSection {
    heading: String,
    items: Vec<String>,
}

#[derive(Deserialize, Default)]
struct PatchNoteEntry {
    date: String,
    sections: Vec<PatchNoteSection>,
}

/// Public - no login needed, same as the point of a changelog. Reads
/// `patch-notes.json` fresh on every request (a handful of dated
/// entries, negligible cost) rather than caching, so editing the file
/// takes effect immediately without a bot restart.
async fn patch_notes(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let entries: Vec<PatchNoteEntry> = crate::state::load_json("patch-notes.json").unwrap_or_default();
    let character = match current_session(&headers, &state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    };
    Html(render_page(&render_patch_notes(&entries, character.as_ref())))
}

/// `/overlay` - public, no login needed (same reasoning as patch-notes/
/// wiki above), serves the EXACT SAME `overlay.html` OBS points a
/// Browser Source at (port 4004, not publicly exposed - see
/// `start_adventure_web_server`'s own doc on `/sprites`/`/skill-effects`)
/// so anyone can watch the live fight animate in a normal browser tab
/// without needing the stream itself - a live request. Served raw, NOT
/// wrapped in `render_page`'s own header chrome, since this page is a
/// full-bleed canvas animation with its own embedded layout that assumes
/// it owns the whole viewport, same as OBS's Browser Source does.
/// `overlay.html`'s own asset paths (`sprites/...`, `skill-effects/...`)
/// and its WebSocket (`${location.host}/ws`) are all relative/host-
/// relative, so serving the identical file from this ALREADY-public
/// dashboard host just works, with zero new tunnel/DNS/infra changes -
/// `/ws` below reuses `adventure_overlay_server::handle_socket` directly,
/// sharing the same `Arc<AdventureManager>` broadcast this dashboard
/// already holds.
/// Same Twitch login `FIGHTS_PAGE_LOGIN` gates its own streamer-only page
/// with - reused here as the channel Twitch's chat embed points at
/// (they're the same string only because the streamer's own login IS the
/// channel name, not because these two constants mean the same thing).
const TWITCH_CHANNEL: &str = "lokati_gaming";

async fn overlay_page(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match tokio::fs::read_to_string("public_adventure_overlay/overlay.html").await {
        Ok(contents) => {
            // overlay.html's own `html, body { background: transparent; }`
            // is deliberate for OBS - the Browser Source composites this
            // page over the actual stream, so the game has to show
            // through. A bare browser tab has no stream behind it, so
            // that same transparency just renders as blinding white - a
            // live report: "so people [aren't] flashbanged" opening
            // `/overlay` here. Patched in ONLY for this web route, not
            // OBS's own copy (adventure_overlay_server.rs's `serve_index`
            // serves the file completely untouched) - a late `<style>`
            // override right before `</head>` wins via `!important`
            // regardless of the file's own rule order, so the actual
            // `overlay.html` file on disk never needs editing.
            let dark_bg_override = "<style>html,body{background:#1a1a1a!important;}</style></head>";
            let mut patched = contents.replacen("</head>", dark_bg_override, 1);
            // Live chat embed pinned to the right side (moved from an
            // initial top-strip placement per a live follow-up request) -
            // gated to logged-in dashboard visitors only (not the
            // anonymous "no login needed" default this whole page
            // otherwise has, see this fn's own doc) - an actual Twitch
            // account is needed to chat there anyway, so a logged-out
            // visitor gets the plain overlay instead of a chat box they
            // can only read, not use. Twitch's own embed requires a
            // `parent` query param matching the EXACT hostname the page
            // loads from (enforced by Twitch, not this app) - resolved
            // client-side via `location.hostname` rather than hardcoded,
            // so this works unchanged whether it's reached via
            // adventure.lokati.net or localhost.
            if current_session(&headers, &state).await.is_some() {
                const CHAT_WIDTH_PX: u32 = 340;
                // /* */ block comment, NOT //, inside the JS below -
                // deliberately, after a confirmed live bug: this whole
                // string is built with Rust's backslash-newline
                // continuations (each line ends in `\`), which strip the
                // actual newline character between lines. A `//` line
                // comment has no closing delimiter, so with the newlines
                // gone it silently swallowed every line after it -
                // including the entire resize IIFE below - into one giant
                // comment that never executed at all (the chat panel
                // itself still rendered fine, since that's plain HTML,
                // unaffected - only the JS silently no-opped). `/* */`
                // can't make that mistake regardless of how the lines get
                // joined.
                let chat_html = format!(
                    "<div id=\"overlay-chat\" style=\"position:fixed;top:0;right:0;width:{CHAT_WIDTH_PX}px;height:100%;z-index:9999;background:#0e0e10;border-left:2px solid #7c5cff;\">\
                      <iframe id=\"overlay-chat-frame\" frameborder=\"0\" style=\"width:100%;height:100%;\" title=\"Twitch chat\"></iframe>\
                    </div>\
                    <script>\
                      document.getElementById('overlay-chat-frame').src = 'https://www.twitch.tv/embed/{TWITCH_CHANNEL}/chat?parent=' + location.hostname + '&darkpopout';\
                      /* overlay.html's OWN resize() (see its own <script>, near the top) sizes the game canvas to the FULL window width - it has no idea this chat panel is reserving {CHAT_WIDTH_PX}px on the right, so without this the game keeps drawing (and getting covered) behind the chat panel. Re-shrinking the canvas here, both immediately (correcting whatever the page's own resize() just set on load) and on every future resize (this listener is attached AFTER the page's own, so it always runs second/last and wins) - every draw call in the game already positions everything relative to canvas.width/height, so shrinking it is enough to make the whole scene fit the remaining space with no other changes needed. */\
                      (function() {{\
                        var stage = document.getElementById('stage');\
                        function shrinkForChat() {{\
                          if (!stage) return;\
                          stage.width = Math.max(0, window.innerWidth - {CHAT_WIDTH_PX});\
                          stage.height = window.innerHeight;\
                        }}\
                        shrinkForChat();\
                        window.addEventListener('resize', shrinkForChat);\
                      }})();\
                    </script></body>"
                );
                patched = patched.replacen("</body>", &chat_html, 1);
            }
            // Same no-store reasoning as adventure_overlay_server.rs's
            // own `serve_index` - this page is actively iterated on, and
            // a stale cached copy has bitten OBS's own CEF cache before.
            ([(header::CACHE_CONTROL, "no-store")], Html(patched)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `/ws` - see `overlay_page`'s doc. No session/auth check (matches the
/// OBS-only overlay server's own "push-only, no login" shape) - this
/// feed is the same fight/roster snapshot data an anonymous OBS Browser
/// Source already gets, nothing account-specific.
async fn overlay_ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| crate::adventure_overlay_server::handle_socket(socket, state.adventure.clone()))
}

/// Public - no login needed, same "pure reference content, same for
/// everyone" reasoning as patch-notes above (the two deliberate
/// exceptions to this dashboard's usual login gate). First section:
/// what each world boss actually does, so chat knows what it's walking
/// into before a fight - see `render_wiki_bosses`. Content is hand-
/// written prose baked into the binary (unlike patch-notes' JSON file),
/// since unlike a changelog this doesn't change fight-to-fight - editing
/// it is a real code change/redeploy, which is fine for how rarely boss
/// mechanics themselves change. Still resolves the session (if any) now,
/// purely so `top_nav`'s stat summary can show for a logged-in visitor -
/// the page itself stays fully viewable logged out.
async fn wiki_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let character = match current_session(&headers, &state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    };
    Html(render_page(&render_wiki(character.as_ref())))
}

fn render_wiki(character: Option<&Character>) -> String {
    format!("{}{}{}{}", top_nav(character), render_wiki_bosses(), render_wiki_crafting(), render_wiki_passives())
}

/// Public, read-only reference for every class's full passive tree - the
/// interactive `/passives` page (`render_passive_tree_page`) needs a
/// logged-in character (it's driven by THEIR current ranks/unlock state),
/// so it can't double as a "just let me look at the whole tree" page for
/// a logged-out visitor or someone comparing classes before picking one.
/// This instead renders the SAME node-graph component (`compute_passive_layout`/
/// `root_node_html`, the exact boxes-and-SVG-connectors tree `/passives`
/// itself draws) per class, one collapsible section each - not a
/// hand-duplicated text summary, so the two views can never drift apart.
fn render_wiki_passives() -> String {
    let sections: String = ALL_ARCHETYPES.iter().map(|&archetype| render_wiki_archetype_graph(archetype)).collect();
    format!(
        "<div class=\"card\">\
          <h2>Class Passive Trees</h2>\
          <p>Every class has its own tree: 1 root passive (always active, scales with level), 3 Skills you can rank up freely, \
          9 Specializations (3 per Skill, gated behind their Skill), and 27 Modifiers (3 per Specialization, gated behind pushing \
          that Specialization to 4/4 - the 4th point \"specializes\" it without adding another point of its own stat, but unlocks \
          its 3 Modifiers below). Shown fully unlocked below so every node is visible at once - a node flagged \
          <span class=\"muted\">(inactive)</span> doesn't do anything yet, though points spent there are banked, not wasted, and \
          will start working the day that mechanic ships.</p>\
        </div>{sections}"
    )
}

/// One class's full tree, drawn with the exact same graph renderer the
/// interactive `/passives` page uses - see `render_wiki_passives`'s doc.
/// Since this has no real character behind it, `allocations` is a
/// synthetic "every node maxed" map (every skill/spec/modifier's own
/// `max_rank`) rather than anyone's real ranks - the ONLY purpose here is
/// to make `compute_passive_layout` treat every Specialization as
/// unlocked (so every Modifier row actually gets drawn instead of hidden
/// behind its normal in-game gate), and it doubles as a natural "here's
/// what a fully-invested tree looks like" reference view. Read-only, same
/// "no forms, plain rank text" shape as `render_passive_tree_readonly`'s
/// own node_html.
fn render_wiki_archetype_graph(archetype: Archetype) -> String {
    let (icon, role) = passive_archetype_icon_role(archetype);
    let role_class = archetype.css_class();
    let nodes = archetype.passive_nodes();
    let allocations: HashMap<String, u32> = nodes.iter().map(|n| (n.key.to_string(), n.max_rank)).collect();

    let layout = compute_passive_layout(archetype, &allocations);
    let PassiveLayout { positioned, root_x, svg_lines, stage_w, stage_h } = layout;

    let node_html = |p: &PosNode| -> String {
        let n = &p.node;
        let rank = n.max_rank;
        let not_yet = matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented);
        let tip = if not_yet {
            format!("{} Not yet active - allocating still banks the point for when this mechanic ships.", escape_html(n.description))
        } else {
            escape_html(n.description)
        };
        let (kind_class, kind_label) = match n.tier {
            PassiveTier::Skill => ("node-skill", "Tier 1"),
            PassiveTier::Specialization => ("node-spec", "Specialization"),
            PassiveTier::Modifier => ("node-mod", ""),
        };
        let state_class = if n.max_rank == 4 { " node--specialized" } else { " node--maxed" };
        let dots: String = (0..n.max_rank)
            .map(|i| {
                let gold = n.max_rank == 4 && i == 3;
                format!("<span class=\"dot filled{}\"></span>", if gold { " dot-spec" } else { "" })
            })
            .collect();
        let kind_label_html = if kind_label.is_empty() { String::new() } else { format!("<div class=\"node-kind\">{kind_label}</div>") };
        format!(
            "<div class=\"node {kind_class}{state_class}\" style=\"left:{x}px;top:{y}px;width:{w}px;\" data-tip=\"{tip}\">\
              {kind_label_html}\
              <div class=\"node-name\">{name}{flag}</div>\
              <div class=\"dots\">{dots}</div>\
              <div class=\"node-buttons\"><span class=\"node-rank\">{rank}/{max_rank}</span></div>\
            </div>",
            x = p.x,
            y = p.y,
            w = p.w,
            name = escape_html(n.name),
            flag = if not_yet { " <span class=\"muted\">(inactive)</span>" } else { "" },
            max_rank = n.max_rank,
        )
    };

    let root_desc = escape_html(&archetype.description(1));
    let root_html = root_node_html(archetype, root_x, &root_desc);
    let nodes_html: String = positioned.iter().map(node_html).collect();

    format!(
        "<details class=\"card bag-row wiki-archetype\">\
          <summary>{icon} {archetype:?} <span class=\"role-badge role-{role_class}\">{role}</span></summary>\
          <p class=\"wiki-root-desc\"><strong>Root passive (always active):</strong> {root_desc} <span class=\"muted\">- shown at level 1; grows with level.</span></p>\
          <div class=\"ptree-page\">\
            <div class=\"tree-wrap\"><div style=\"width:{stage_w}px;height:{stage_h}px;position:relative;\">\
              <svg class=\"connectors\" width=\"{stage_w}\" height=\"{stage_h}\">{svg_lines}</svg>\
              {root_html}{nodes_html}\
            </div></div>\
          </div>\
        </details>",
    )
}

/// Hand-written, deliberately non-technical - a curious viewer or a
/// player deciding whether to !join should be able to read this without
/// knowing anything about how the game's actually coded. Numbers here
/// (thresholds, add counts, percentages) are pulled directly from
/// adventure.rs's own constants (TWO_BOSS_STAGE/THREE_BOSS_STAGE,
/// LICH_MAX_ADDS, the Fire Demon/Dragon aura magnitudes, Cthulhu's -90%)
/// - keep this in sync if any of those ever change.
fn render_wiki_bosses() -> String {
    "<div class=\"card\"><h1>Wiki</h1><p class=\"muted\"><a href=\"/\">&larr; Back to your character</a></p></div>\
    <div class=\"card\">\
      <h2>Bosses</h2>\
      <p>Every world boss fight pits the party against one or more of five named bosses. Each one plays completely differently - here's what to watch for.</p>\
      <h3 id=\"dragon\">🐲 The Dragon</h3>\
      <p>Hovers above the battlefield instead of standing on the ground, sweeping from one side to the other. Every 5 seconds it rears back and breathes fire across the <strong>whole party at once</strong> - unlike a normal attack, which only hits one hero, nobody is safe from this one. It also radiates a passive aura for the entire fight that slows everyone's attack speed. Two looks can show up in a Dragon fight - the classic purple dragon, or the rarer, fully-animated Bahamut - purely cosmetic, same fight either way.</p>\
      <h3 id=\"cthulhu\">🐙 Cthulhu</h3>\
      <p>Every 5 seconds, a purple bubble locks onto whoever's currently dealt the most damage this fight and permanently crushes THEIR damage output by 90% for the rest of it. The instant someone else takes the damage lead, the bubble follows them instead - no single hero can carry a Cthulhu fight alone; the whole party has to pull their weight.</p>\
      <h3 id=\"lich\">💀 The Lich</h3>\
      <p>Raises 5 skeletons to fight alongside it every 2 seconds, up to 20 in a single battle. Each one is individually weak - a fraction of the Lich's own health and attack - but they pile up fast if the fight drags on, so speed matters against this one.</p>\
      <h3 id=\"fire-demon\">🔥 Fire Demon</h3>\
      <p>No flashy abilities - just relentless heat. Its aura cuts the entire party's healing power in half for the whole fight, so healers get a lot less mileage out of every cast.</p>\
      <h3 id=\"gelatinous-cube\">🧊 The Gelatinous Cube</h3>\
      <p>Crawls across the field absorbing party members into its body - every few seconds it rotates a fresh batch of heroes into itself (scaled to how many are still standing), and while trapped they can't act, though allies can still damage or heal them normally. It also lashes out with a wide splash attack hitting several heroes at once, and every hit it lands stacks a shred on that hero's defenses, up to a steep cap - the longer a fight against the Cube drags on, the softer the party gets.</p>\
      <h3>Multiple bosses at once</h3>\
      <p>As the world stage climbs, more bosses join the fight at once - it's not a fixed number for a given stage, either: every fight rolls its own boss count with some real variance, trending upward the higher the stage, so two fights at the same stage can genuinely differ in size. It's always a different mix - the game never repeats the last fight's boss back-to-back, and won't repeat a kind WITHIN one fight either, unless there are more boss slots than distinct kinds left to fill them (only relevant at the very highest stages, since there are only five named kinds total).</p>\
      <p class=\"muted\">These five only show up in real world-boss fights. The smaller, more frequent filler encounters between them use weaker, unnamed enemies instead.</p>\
    </div>"
        .to_string()
}

/// Second wiki section - the crafting/item system, ported from the
/// standalone "Crafting Codex" reference doc into the in-game wiki so
/// players don't need a separate link to find it. Same "hand-written
/// prose baked into the binary" reasoning as `render_wiki_bosses` - the
/// numbers here (dust costs, crit chances, disenchant multipliers) are
/// transcribed from the actual constants/formulas in `item.rs`/
/// `character.rs`/`manager.rs` and need a code change to stay in sync,
/// same maintenance contract as the boss section above.
fn render_wiki_crafting() -> String {
    "<div class=\"card wiki-wide\">\
      <h2>Crafting</h2>\
      <p>Every currency, every action, and the one rule that governs them all: no item can ever carry more than seven modifiers, however you get there.</p>\
      <h3 id=\"currencies\">Currencies</h3>\
      <div class=\"wiki-currency-grid\">\
        <div class=\"wiki-currency-card\"><h4>Dust</h4><p>Earned from wins and boss kills. Pays for every currency-crafting action, Reforge, and Recombine.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Sand</h4><p>Earned from wins and disenchanting. Spent exclusively on Polishing.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Craft Tokens</h4><p>One kind per action (Transmute, Scour, Augment, Regal, Exalt, Krangle). Spending a token skips that action's dust cost entirely.</p></div>\
        <div class=\"wiki-currency-card\"><h4>Celestial Shard</h4><p>A rare token, separate from every other currency. The only way to grant a Unique Affix.</p></div>\
      </div>\
      <h3 id=\"ceiling\">The Modifier Ceiling</h3>\
      <p>The single rule worth memorizing before anything else here: <strong>four base modifiers, plus at most one each from three independent bonus sources.</strong> Seven, and never more &mdash; on any single item, for its entire life, no matter how many times it gets reforged or recombined.</p>\
      <div class=\"wiki-ceiling\">\
        <div class=\"wiki-ceiling-row\">\
          <span class=\"wiki-slot wiki-slot--base\">1</span><span class=\"wiki-slot wiki-slot--base\">2</span><span class=\"wiki-slot wiki-slot--base\">3</span><span class=\"wiki-slot wiki-slot--base\">4</span>\
          <span class=\"wiki-slot-plus\">+</span>\
          <span class=\"wiki-slot wiki-slot--bonus\" title=\"Reforge crit\">R</span><span class=\"wiki-slot wiki-slot--bonus\" title=\"Recombine crit\">C</span><span class=\"wiki-slot wiki-slot--bonus\" title=\"Krangle\">K</span>\
        </div>\
        <ul>\
          <li><strong>1&ndash;4</strong> &mdash; Transmute / Augment / Regal / Exalt, ordinary currency crafting.</li>\
          <li><strong>R</strong> &mdash; Reforge's rare crit, can land once, ever, on this item's whole lineage.</li>\
          <li><strong>C</strong> &mdash; Recombine's own separate crit, same one-time rule, tracked independently of Reforge's.</li>\
          <li><strong>K</strong> &mdash; Krangle, guarantees a bonus modifier, but permanently locks the item afterward.</li>\
        </ul>\
        <p class=\"wiki-ceiling-cap\">Hard ceiling: <strong>7</strong> modifiers total.</p>\
      </div>\
      <p>The R and C slots each carry a memory: once either crit has landed on an item, it can never land again &mdash; even if that item is later merged into something new by Recombine. A merged item inherits \"already used\" from either parent, so pairing a just-crit'd item with a fresh one can't reset its odds.</p>\
      <p>A Unique Affix (from a Celestial Shard) and Sacred's implicit affix live entirely outside this pool &mdash; they never count toward the seven, and no crafting action can touch them.</p>\
      <h3 id=\"currency-crafting\">Currency Crafting</h3>\
      <p>Six actions, each gated by exactly how many modifiers the target item currently has. Every one of them also bumps the item's tier as a side effect &mdash; <code>+3</code> below tier 25, <code>+2</code> below tier 50, <code>+1</code> beyond that.</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Action</th><th>Effect</th><th>Requires</th><th>Dust</th><th>Veilable</th></tr></thead>\
        <tbody>\
          <tr><td>Transmute</td><td>Adds a random modifier to a bare item.</td><td>0 modifiers</td><td>250</td><td>Yes</td></tr>\
          <tr><td>Augment</td><td>Adds a 2nd modifier.</td><td>1 modifier</td><td>500</td><td>Yes</td></tr>\
          <tr><td>Regal</td><td>Adds a 3rd modifier.</td><td>2 modifiers</td><td>750</td><td>Yes</td></tr>\
          <tr><td>Exalt</td><td>Adds a 4th modifier.</td><td>3 modifiers</td><td>1,250</td><td>Yes</td></tr>\
          <tr><td>Scour</td><td>Strips every modifier back to none.</td><td>1+ modifiers</td><td>250</td><td>No</td></tr>\
          <tr><td>Krangle</td><td>Adds one final modifier beyond the normal 4, then permanently locks the item &mdash; no further crafting of any kind, ever.</td><td>Any unlocked item</td><td>2,500</td><td>Yes</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Every action above also costs <code>3&times;tier</code> extra dust on top of its base price &mdash; unless you spend that action's own Craft Token instead, which skips the dust entirely. A locked (Krangled) item is permanently excluded from all six.</p>\
      <h3 id=\"reforge\">Reforge</h3>\
      <p>Raises an item's tier and rescales everything it already has to match &mdash; the closest thing to a straightforward power-up. Comes in two forms.</p>\
      <p><strong>Reforge Now</strong> &mdash; a free action (via the dashboard button or its matching channel-points redemption), or <code>1,000</code> dust on demand. Always targets a random unlocked equipped item, replacing it with a freshly reforged version at the same slot.</p>\
      <p><strong>Crafting-Panel Reforge</strong> &mdash; costs <code>30&times;tier</code> dust and lets you pick the exact item, equipped or bagged, rather than leaving it to chance. No cooldown, unlike the free button.</p>\
      <p><strong>Tier jump:</strong> +2 to +4 below tier 50, +1 to +2 at tier 50&ndash;99, +1 at tier 100+.</p>\
      <p>Every reforge also has a chance to add a bonus modifier on top of the tier increase &mdash; the \"R\" slot from the ceiling above. The chance scales with the item's own quality:</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Item quality</th><th>Crit chance</th></tr></thead>\
        <tbody>\
          <tr><td>0% quality</td><td>1.0%</td></tr>\
          <tr><td>50% quality</td><td>1.5%</td></tr>\
          <tr><td>100% quality</td><td>2.0%</td></tr>\
          <tr><td><span class=\"gear-unique\">Perfect</span></td><td>2.2%</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Once it fires, that's it for this item's lineage &mdash; permanently. Higher-quality gear is both better to begin with <em>and</em> slightly likelier to pick up that bonus modifier, for as long as it hasn't already.</p>\
      <h3 id=\"recombine\">Recombine</h3>\
      <p>Forges two items of the same slot into one, consuming both. The result inherits the better parts of each &mdash; not everything.</p>\
      <ul>\
        <li><strong>Tier:</strong> the average of the two sources, rounded down, plus one.</li>\
        <li><strong>Modifiers:</strong> any modifier type present on <em>both</em> sources is guaranteed to carry over (keeping the stronger of the two values). Each source's own unique modifiers get a coin-flip chance each, capped so the transferred pool never exceeds four.</li>\
        <li><strong>Power roll:</strong> a 50/50 coin flip between the two sources' rolls.</li>\
        <li><strong>Unique Affix &amp; durability:</strong> either source's Unique Affix carries over; the result is indestructible if either source was.</li>\
        <li><strong>Perfect &amp; Sacred never carry over.</strong> The result is always an ordinary item, regardless of what went in.</li>\
      </ul>\
      <p>Costs <code>500</code> dust, or one of your free recombines if you have one banked. A rare <strong>5% crit</strong> can add a bonus modifier on top &mdash; the \"C\" slot from the ceiling, tracked and gated exactly like Reforge's own crit, independently.</p>\
      <p class=\"muted\">Veiled Recombine guarantees the higher of the two power rolls and the certain transfer of every shared modifier &mdash; see Veiling below for the cost.</p>\
      <h3 id=\"polishing\">Polishing</h3>\
      <p>The only action priced in Sand instead of Dust, and the only one that improves rolls you already have rather than adding new ones.</p>\
      <p>On an ordinary item, Polishing nudges the primary stat's roll upward by a fixed step, and does the same to one random modifier that still has room to climb. On a <span class=\"gear-unique\">Perfect</span> item &mdash; whose primary stat is already maxed &mdash; it instead nudges up to two modifiers at once. An affix already sitting at its own cap is skipped automatically.</p>\
      <p class=\"muted\">Cost scales with how much room is left to improve: <code>ceil(quality% &divide; 10)</code> sand on a normal item (1 to 10), or a flat <code>12</code> sand on a Perfect one. Bypasses tokens and veiling entirely.</p>\
      <h3 id=\"celestial-shard\">Celestial Shard</h3>\
      <p>The only way to grant a Unique Affix &mdash; a permanent, implicit bonus that lives entirely outside the normal modifier pool.</p>\
      <p>Consumes a Celestial Shard token (never dust) to grant <strong>Celestial Conversion</strong>, whose effect depends on the wielder's role:</p>\
      <ul>\
        <li><strong>Healers</strong> convert 10% of every heal into bonus damage against a random enemy.</li>\
        <li><strong>Every other archetype</strong> instead lands a follow-up hit on whatever they just struck, for 10% of that hit's damage &mdash; a real second hit that can trigger Leech, elemental procs, and anything else an on-hit effect would.</li>\
      </ul>\
      <p class=\"muted\">Mutually exclusive with Krangle: an item carrying a Unique Affix can never be Krangled, and vice versa.</p>\
      <h3 id=\"item-tiers\">Item Tiers</h3>\
      <p>Beyond the normal 0&ndash;100% quality roll, two rarer tiers exist &mdash; both earned from boss kills, never crafted.</p>\
      <div class=\"wiki-tier-grid\">\
        <div class=\"wiki-tier-card\"><h4>Normal</h4><p>Rolls somewhere in the standard quality band. What every drop starts as.</p></div>\
        <div class=\"wiki-tier-card\"><h4><span class=\"gear-unique\">Perfect Quality</span></h4><p>Primary stat pinned to its maximum roll, plus a flat 20% bonus to that stat <em>and</em> every modifier. Guaranteed on stage-100+ boss kills &mdash; once per kill, and once more the first time each character personally takes part in one. Once Sacred also starts dropping (stage 300+), that per-kill guarantee only fires half as often.</p></div>\
        <div class=\"wiki-tier-card\"><h4><span class=\"gear-sacred\">Sacred</span></h4><p>Everything Perfect Quality gets, plus one further modifier &mdash; drawn from the full affix pool regardless of slot, rolled at its own maximum, shown as its own implicit line. Outside the 4-modifier pool entirely; no crafting action can ever touch it. Same drop mechanism as Perfect, gated to stage 300+.</p></div>\
      </div>\
      <h3 id=\"veiling\">Veiling</h3>\
      <p>Most crafts are a blind gamble &mdash; you commit, then see what you got. Veiling flips that: pay extra to see the exact result first, and choose whether to keep it.</p>\
      <p>Available on Transmute, Augment, Regal, Exalt, Krangle, and Recombine. <strong>Not</strong> available on Scour (nothing to choose &mdash; it's fully deterministic), Celestial Shard (always the same grant), Polishing (bypasses this system entirely), or Reforge.</p>\
      <p class=\"muted\">A veiled Recombine costs an extra <code>500</code> dust flat, plus <code>500</code> per modifier in the guaranteed-transfer pool.</p>\
      <h3 id=\"disenchanting\">Disenchanting</h3>\
      <p>Turns an unwanted item straight into Dust, valued by how much was invested in it.</p>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Normal modifiers</th><th>Ordinary</th><th>Perfect</th><th>Sacred</th></tr></thead>\
        <tbody>\
          <tr><td>0</td><td>1&times;</td><td>5&times;</td><td>25&times;</td></tr>\
          <tr><td>1</td><td>5&times;</td><td>25&times;</td><td>50&times;</td></tr>\
          <tr><td>2</td><td>10&times;</td><td>50&times;</td><td>75&times;</td></tr>\
          <tr><td>3</td><td>15&times;</td><td>75&times;</td><td>100&times;</td></tr>\
          <tr><td>4</td><td>20&times;</td><td>100&times;</td><td>125&times;</td></tr>\
        </tbody>\
      </table></div>\
      <p class=\"muted\">Sacred's bonus implicit counts as one more modifier toward this same table &mdash; a Sacred item with <em>N</em> normal modifiers is always worth exactly what a Perfect item with <em>N+1</em> would be.</p>\
      <p>Toggle <strong>Keep</strong> on any item to protect it from both single and bulk disenchanting &mdash; independent of Krangle's lock, and reversible any time. A Krangled item can still be disenchanted; locking only ever blocks further crafting.</p>\
      <p><strong>Auto-disenchant</strong> can be set to a threshold &mdash; Quality, Perfect, or Sacred &mdash; so any new drop at or below that tier is converted to Dust automatically the moment it's picked up, instead of filling the bag.</p>\
      <h3 id=\"quick-reference\">Quick Reference</h3>\
      <div class=\"wiki-table-wrap\"><table>\
        <thead><tr><th>Action</th><th>Currency</th><th>Base cost</th><th>Notable rule</th></tr></thead>\
        <tbody>\
          <tr><td>Transmute</td><td>Dust</td><td>250 + 3/tier</td><td>Bare items only</td></tr>\
          <tr><td>Augment</td><td>Dust</td><td>500 + 3/tier</td><td>1 modifier only</td></tr>\
          <tr><td>Regal</td><td>Dust</td><td>750 + 3/tier</td><td>2 modifiers only</td></tr>\
          <tr><td>Exalt</td><td>Dust</td><td>1,250 + 3/tier</td><td>3 modifiers only</td></tr>\
          <tr><td>Scour</td><td>Dust</td><td>250 + 3/tier</td><td>Not veilable</td></tr>\
          <tr><td>Krangle</td><td>Dust</td><td>2,500 + 3/tier</td><td>Locks the item forever</td></tr>\
          <tr><td>Reforge (free)</td><td>&mdash;</td><td>0</td><td>Random equipped item</td></tr>\
          <tr><td>Reforge (on demand)</td><td>Dust</td><td>1,000</td><td>Random equipped item</td></tr>\
          <tr><td>Reforge (crafting panel)</td><td>Dust</td><td>30/tier</td><td>Choose the exact item</td></tr>\
          <tr><td>Recombine</td><td>Dust</td><td>500</td><td>+500 veil, +500/modifier</td></tr>\
          <tr><td>Polishing</td><td>Sand</td><td>1&ndash;10, or 12 flat if Perfect</td><td>Not veilable</td></tr>\
          <tr><td>Celestial Shard</td><td>Shard token</td><td>1 token</td><td>Excludes Krangle</td></tr>\
        </tbody>\
      </table></div>\
    </div>"
        .to_string()
}

/// Browse every character that's ever `!join`ed - login-gated same as
/// everything else here (patch-notes is the one deliberate exception),
/// since it's still someone's own dashboard session, just looking at
/// other players instead of themselves.
async fn character_list(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, _)) => {
            let characters = state.adventure.all_characters().await;
            let viewer = state.adventure.character(&login).await;
            render_character_list(&characters, viewer.as_ref())
        }
    };
    Html(render_page(&body))
}

/// One character's read-only detail view (see `render_character_detail`) -
/// same Combat Stats/Gear/Bag info the owner's own dashboard shows, minus
/// every action button (equip/craft/reforge/etc. only ever apply to your
/// OWN character - see every other `do_*` handler's `current_session`
/// check). `login` is whatever `/characters` linked to, already
/// lowercased (see `AdventureManager::all_characters`) - the VIEWED
/// character, not necessarily the logged-in viewer's own, which is
/// fetched separately below purely for `top_nav`'s own stat summary.
async fn character_detail(State(state): State<AppState>, headers: HeaderMap, Path(login): Path<String>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((viewer_login, _)) => {
            let viewer = state.adventure.character(&viewer_login).await;
            match state.adventure.character(&login).await {
                Some(c) => render_character_detail(&login, &c, viewer.as_ref()),
                None => format!(
                    "{}<div class=\"card\"><h1>Not Found</h1><p>No such character.</p><p class=\"muted\"><a href=\"/characters\">&larr; Back to the character list</a></p></div>",
                    top_nav(viewer.as_ref())
                ),
            }
        }
    };
    Html(render_page(&body))
}

/// `/characters/{login}/passives` - read-only view of another player's
/// passive tree (see `render_passive_tree_readonly`). Same shape as
/// `character_detail` above: logged out -> render_logged_out, unknown
/// login -> generic "Not Found" (not gated to your own character - anyone
/// logged in can look up anyone's tree, same as the gear/bag detail page).
async fn character_passives_readonly(State(state): State<AppState>, headers: HeaderMap, Path(login): Path<String>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((viewer_login, _)) => {
            let viewer = state.adventure.character(&viewer_login).await;
            match state.adventure.character(&login).await {
                Some(c) => render_passive_tree_readonly(&login, &c, viewer.as_ref()),
                None => format!(
                    "{}<div class=\"card\"><h1>Not Found</h1><p>No such character.</p><p class=\"muted\"><a href=\"/characters\">&larr; Back to the character list</a></p></div>",
                    top_nav(viewer.as_ref())
                ),
            }
        }
    };
    Html(render_page(&body))
}

/// The streamer's own login still gets the full, unfiltered fight-history
/// list (see `fights_for_viewer`) - everything today's `/admin/tunables`-
/// style balance tuning is built around. Everyone else (2026-08-17, opened
/// up from a single-account-only page) sees the same page, scoped to just
/// the fights they personally took part in.
const FIGHTS_PAGE_LOGIN: &str = "lokati_gaming";

#[derive(Deserialize, Default)]
#[serde(default)]
struct FightsPageParams {
    limit: Option<usize>,
}

/// Shared by `fights_page` (HTML) and `fights_json` (the companion app's
/// preferred format) so both read exactly the same rules. The streamer's
/// login gets `recent_fights(limit)` unfiltered, same as always; anyone
/// else gets the full currently-stored history fetched first (cheap at
/// today's `COARSE_FIGHTS_CAPACITY`-fight retention), filtered to fights
/// they actually appear in (`units[].id`, the same lowercase login used
/// everywhere else - NOT `participants`, which holds display names and
/// would mismatch case), THEN capped at `limit` - fetching-then-filtering
/// (rather than filtering a `recent_fights(limit)` slice) is what actually
/// answers "the last N fights THEY were in," not "however many of the
/// last N fights overall happened to include them."
fn fights_for_viewer(login: &str, requested_limit: usize) -> Vec<LastFightSnapshot> {
    let limit = requested_limit.clamp(1, COARSE_FIGHTS_CAPACITY);
    if login == FIGHTS_PAGE_LOGIN {
        recent_fights(limit)
    } else {
        recent_fights(COARSE_FIGHTS_CAPACITY).into_iter().filter(|snap| snap.result.units.iter().any(|u| !u.is_boss && u.id == login)).take(limit).collect()
    }
}

/// Same shape as `fights_for_viewer` above, but reads the summary tier
/// (2026-08-18, the `/fights.json` size/latency fix) instead of the
/// coarse tier - what `fights_json` actually serves now, since a
/// consumer like the companion app only wants the per-player aggregates,
/// not the full event log. `SUMMARY_FIGHTS_CAPACITY` is far larger than
/// `COARSE_FIGHTS_CAPACITY` (summaries are a few KB each), so unlike
/// `fights_for_viewer`, reading the whole tier before filtering here
/// isn't the meaningful cost `fights_for_viewer`'s own doc flags for the
/// coarse tier.
fn fight_summaries_for_viewer(login: &str, requested_limit: usize) -> Vec<FightSummarySnapshot> {
    let limit = requested_limit.clamp(1, SUMMARY_FIGHTS_CAPACITY);
    if login == FIGHTS_PAGE_LOGIN {
        recent_summary_fights(limit)
    } else {
        recent_summary_fights(SUMMARY_FIGHTS_CAPACITY).into_iter().filter(|s| s.players.iter().any(|p| p.id == login)).take(limit).collect()
    }
}

async fn fights_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<FightsPageParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, _)) => {
            let viewer = state.adventure.character(&login).await;
            let is_streamer = login == FIGHTS_PAGE_LOGIN;
            let limit = params.limit.unwrap_or(FIGHTS_PAGE_DISPLAY_LIMIT);
            // recent_fights()'s per-fight-file read is real, synchronous
            // disk I/O (see its doc) - offloaded so it can't stall this
            // worker thread's own tokio runtime.
            let fights = tokio::task::spawn_blocking(move || fights_for_viewer(&login, limit)).await.unwrap_or_default();
            tokio::task::spawn_blocking(move || render_fights_page(viewer.as_ref(), &fights, is_streamer))
                .await
                .unwrap_or_else(|err| {
                    tracing::error!("fights_page render task panicked: {err}");
                    "<div class=\"card\"><h1>Error</h1></div>".to_string()
                })
        }
    };
    Html(render_page(&body))
}

/// JSON twin of `/fights` (2026-08-17, a live request) - the desktop
/// companion app would rather parse this than scrape the HTML page's
/// markup. Same session cookie, same `fights_for_viewer` rules (streamer
/// unfiltered, everyone else scoped to their own fights), same `?limit=`.
/// 401 with an empty array when not logged in, same "don't hint at
/// what's behind the gate" spirit `fights_page` itself now only applies
/// to logged-out visitors (every logged-in viewer can reach this now).
async fn fights_json(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<FightsPageParams>) -> impl IntoResponse {
    let Some((login, _)) = current_session(&headers, &state).await else {
        return (StatusCode::UNAUTHORIZED, Json(Vec::<FightSummarySnapshot>::new())).into_response();
    };
    let limit = params.limit.unwrap_or(FIGHTS_PAGE_DISPLAY_LIMIT);
    let summaries = tokio::task::spawn_blocking(move || fight_summaries_for_viewer(&login, limit)).await.unwrap_or_default();
    Json(summaries).into_response()
}

/// Gates `/admin/tunables` the same way `FIGHTS_PAGE_LOGIN` gates
/// `/fights` (`current_session` + a plain login-equality check, same
/// generic "Not Found" fallback so a restricted page's existence isn't
/// hinted at) - same login as `FIGHTS_PAGE_LOGIN`, kept as its own
/// constant rather than reused directly so this page's access isn't
/// accidentally coupled to a future change made for that unrelated page.
const ADMIN_TUNABLES_LOGIN: &str = "lokati_gaming";

#[derive(Deserialize)]
struct AdminTunablesParams {
    saved: Option<String>,
}

async fn admin_tunables_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<AdminTunablesParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        Some((login, _)) if login == ADMIN_TUNABLES_LOGIN => {
            let viewer = state.adventure.character(&login).await;
            let current_boss_power_mult = state.adventure.current_boss_power_mult().await;
            render_tunables_page(viewer.as_ref(), &state.adventure.live_tunables(), current_boss_power_mult, params.saved.is_some())
        }
        _ => "<div class=\"card\"><h1>Not Found</h1></div>".to_string(),
    };
    Html(render_page(&body))
}

#[derive(Deserialize)]
struct TunablesForm {
    loot_mult: f64,
    sand_mult: f64,
    wings_drop_chance: f64,
    celestial_shard_drop_chance: f64,
    boss_health: f64,
    boss_power: f64,
    dynamic_scaling_mult: f64,
    boss_count_tier_stages: u32,
    boss_count_cap_mult: f64,
    late_content_stage: u32,
    /// A checkbox only shows up in the form body at all when checked -
    /// same `#[serde(default)]`-as-absent convention every other checkbox
    /// on this dashboard already uses (see `CraftForm::veiled`).
    #[serde(default)]
    permanent_rampage: Option<String>,
    /// Manual override for `WorldState::boss_power_mult` (see
    /// `AdventureManager::set_boss_power_mult`) - a separate, optional
    /// field from everything else in this form: it edits live WORLD
    /// state, not a `LiveTunables` field, so it's applied through its own
    /// call in `do_save_tunables` rather than folding into the
    /// `LiveTunables` struct built there. Plain `String` (not `Option<f64>`)
    /// because a blank text input still submits as an empty string, not an
    /// absent field - `Option<f64>` would fail to deserialize that. Blank
    /// (the normal case - this isn't something you'd touch on every save)
    /// means "leave it alone," parsed by hand in `do_save_tunables`.
    #[serde(default)]
    boss_power_mult_override: String,
}

/// Same admin gate as the GET page above - a direct POST from someone
/// other than `ADMIN_TUNABLES_LOGIN` is silently ignored (no error, no
/// hint), same "don't reveal a restricted action exists" reasoning.
/// Values are clamped to sane ranges (never negative, drop chances kept
/// within 0-1) before being written - this is a trusted single-admin tool,
/// not a public form, so the clamping is a sanity backstop against a typo
/// rather than a security boundary.
async fn do_save_tunables(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<TunablesForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if login == ADMIN_TUNABLES_LOGIN {
            let tunables = LiveTunables {
                loot_mult: form.loot_mult.max(0.0),
                sand_mult: form.sand_mult.max(0.0),
                wings_drop_chance: form.wings_drop_chance.clamp(0.0, 1.0),
                celestial_shard_drop_chance: form.celestial_shard_drop_chance.clamp(0.0, 1.0),
                boss_health: form.boss_health.max(0.0),
                boss_power: form.boss_power.max(0.0),
                dynamic_scaling_mult: form.dynamic_scaling_mult.max(0.0),
                boss_count_tier_stages: form.boss_count_tier_stages.max(1),
                boss_count_cap_mult: form.boss_count_cap_mult.max(0.0),
                late_content_stage: form.late_content_stage,
                permanent_rampage: form.permanent_rampage.is_some(),
            };
            if let Err(err) = state.adventure.save_live_tunables(tunables) {
                tracing::error!("Failed to persist live tunables: {err}");
            }
            // Separate from the LiveTunables save above - this edits live
            // WORLD state (WorldState::boss_power_mult), not a tunables
            // field. Blank input (the normal case) leaves it untouched.
            if let Ok(value) = form.boss_power_mult_override.trim().parse::<f64>() {
                state.adventure.set_boss_power_mult(value).await;
            }
        }
    }
    Redirect::to("/admin/tunables?saved=1")
}

// ---- Rendering ----

fn render_logged_out() -> String {
    "<div class=\"card\"><h1>Adventure Character Dashboard</h1>\
      <p>Log in with your Twitch account to view and manage your adventure character.</p>\
      <p><a class=\"btn\" href=\"/login\">Login with Twitch</a></p>\
      <p class=\"muted\"><a href=\"/patch-notes\">Patch Notes</a></p></div>"
        .to_string()
}

fn render_patch_notes(entries: &[PatchNoteEntry], character: Option<&Character>) -> String {
    let header = format!("{}<div class=\"card\"><h1>Patch Notes</h1></div>", top_nav(character));
    if entries.is_empty() {
        return format!("{header}<div class=\"card\"><p class=\"muted\">Nothing here yet — check back soon.</p></div>");
    }
    let dated_entries: String = entries
        .iter()
        .map(|entry| {
            let sections: String = entry
                .sections
                .iter()
                .map(|section| {
                    let items: String = section.items.iter().map(|item| format!("<li>{}</li>", escape_html(item))).collect();
                    format!("<h3>{}</h3><ul>{items}</ul>", escape_html(&section.heading))
                })
                .collect();
            format!("<div class=\"card\"><h2>{}</h2>{sections}</div>", escape_html(&entry.date))
        })
        .collect();
    format!("{header}{dated_entries}")
}

/// Aggregate secondary stats across all 5 equipped items PLUS the
/// archetype's own bonus (see Character::combat_*/Affix/Archetype) - the
/// breakdown a viewer can't get from !character alone, since that
/// reply's already long. Shared between the owner's own dashboard (see
/// `render_dashboard`) and the read-only character-list detail view (see
/// `render_character_detail`) - same numbers either way, nothing here
/// depends on who's looking. Every attack action now splits between
/// damage and healing by combat_heal_power (see its "healing is
/// strictly converted damage" doc) - DPS and HPS are both always shown,
/// and BOTH can be nonzero for any archetype now, not just a dedicated
/// healer's; 0 is still a perfectly normal value for either (no
/// heal-power investment at all means 0 HPS, 100%+ heal power means 0
/// DPS - nothing left to attack with). Block/Evasion/Crit Chance/
/// Splash/Healing Power/Intervene are all plain 0%+ magnitudes - real,
/// unambiguous floors, so a plain unsigned number says everything for
/// those. The other three here (damage reduction, increased damage,
/// crit damage) are deltas from a baseline and CAN swing negative for
/// real (see combat_damage_reduction's/combat_increased_damage's docs) -
/// never shown with a raw +/- sign, always a magnitude paired with a
/// label naming the actual direction. Every one of those three ALSO
/// explicitly says "Dealt" or "Taken" - damage reduction is about what
/// you TAKE, increased damage and crit damage are about what you DEAL,
/// and those are two entirely different numbers that happen to share
/// the word "damage", so leaving either word out reads as ambiguous
/// rather than just terse.
/// Multi-line hover breakdown for one of the three 75%-capped defensive
/// stats - every contributing source (gear + archetype), the raw total,
/// and, when it's actually over the cap, how much spilled over and where
/// it went (see `Character::defensive_overflow`'s doc) - per the request
/// to see "all sources... and the total amount exceeding the cap" on
/// mouseover, not just the already-capped headline number. `\n`-joined,
/// rendered via the `.stat-value[data-tip]` CSS rule's `white-space:
/// pre-line` (not `<br>` - `content: attr(...)` can't render markup,
/// only literal newlines already present in the attribute text).
fn stat_breakdown_tip(breakdown: &StatBreakdown) -> String {
    let mut lines: Vec<String> = breakdown.sources.iter().map(|(label, v)| format!("{label}: {:+.0}%", v * 100.0)).collect();
    if lines.is_empty() {
        lines.push("No sources.".to_string());
    }
    lines.push(format!("Total: {:.0}%", breakdown.raw * 100.0));
    if breakdown.overflow > 0.0 {
        lines.push(format!("{:.0}% over the 75% cap \u{2192} converted to bonus damage", breakdown.overflow * 100.0));
    }
    escape_html(&lines.join("\n"))
}

fn render_combat_stats_card(c: &Character) -> String {
    // Only shown at all when nonzero (Slayer or a lucky Leech affix roll) -
    // 0% Leech on the other ten archetypes would just be dead space on
    // every other character's card.
    let leech = c.combat_life_leech();
    let leech_stat = if leech > 0.0 {
        format!(
            "<div class=\"stat\"><div class=\"stat-label\" data-tip=\"Fraction of a hit's actual damage healed back to you, capped at {cap:.0}% of your max hp per second.\">Life Leech</div><div class=\"stat-value\">{leech_pct:.2}%</div></div>",
            cap = LIFE_LEECH_CAP_PER_SEC * 100.0,
            leech_pct = leech * 100.0,
        )
    } else {
        String::new()
    };
    // Same "only shown when nonzero" convention as Life Leech - most
    // characters won't have rolled any Lingering Effect gear, and 0%/0
    // dmg would just be dead space on every other card.
    let lingering_pct = c.combat_lingering_effect_pct();
    let lingering_stat = if lingering_pct > 0.0 {
        format!(
            "<div class=\"stat\"><div class=\"stat-label\" data-tip=\"Symmetric: each landed hit applies an unavoidable damage-over-time debuff to the enemy struck, and each landed heal applies an equivalent heal-over-time to the ally healed - both worth this % of that action's own amount, paid out over 4 seconds. The value shown is an estimate of total Lingering Effect output (damage + healing combined) over a 10-second window, based on your current total output.\">Lingering Effect</div><div class=\"stat-value\">{lingering_pct:.2}% &middot; ~{lingering_10s}/10s</div></div>",
            lingering_pct = lingering_pct * 100.0,
            lingering_10s = format_number(c.combat_lingering_effect_10s_estimate()),
        )
    } else {
        String::new()
    };
    let (dr_label, dr_value) =
        if c.combat_damage_reduction() >= 0.0 { ("Reduced Dmg Taken", c.combat_damage_reduction() * 100.0) } else { ("Increased Dmg Taken", -c.combat_damage_reduction() * 100.0) };
    let (dmg_label, dmg_value) =
        if c.combat_increased_damage() >= 0.0 {
            ("Increased Dmg Dealt", c.combat_increased_damage() * 100.0)
        } else {
            ("Reduced Dmg Dealt", -c.combat_increased_damage() * 100.0)
        };
    let crit_dmg_delta = (c.combat_crit_multiplier() - 1.0) * 100.0;
    let (crit_dmg_label, crit_dmg_value) = if crit_dmg_delta >= 0.0 { ("Increased Crit Dmg Dealt", crit_dmg_delta) } else { ("Reduced Crit Dmg Dealt", -crit_dmg_delta) };
    let dr_tip = stat_breakdown_tip(&c.damage_reduction_breakdown());
    let block_tip = stat_breakdown_tip(&c.block_breakdown());
    let evasion_tip = stat_breakdown_tip(&c.evasion_breakdown());
    let intervene_tip = stat_breakdown_tip(&c.intervene_breakdown());
    let dmg_tip = stat_breakdown_tip(&c.increased_damage_breakdown());
    // Past 100% Healing Power, the extra no longer inflates each heal's
    // own size (see `Character::combat_hps`'s doc) - it shortens the
    // action interval instead (`Character::attack_interval_ms`), so the
    // tooltip surfaces that actual cadence rather than leaving a
    // healer to wonder why HPS keeps climbing with no bigger heals to
    // show for it in the combat log. Only shown once it's actually
    // doing anything (heal power > 100%) - clutter for everyone else.
    let hps_tip = if c.combat_heal_power() > 1.0 {
        escape_html(&format!(
            "Average healing you do per second across a fight. Past 100% Healing Power, extra no longer makes each heal bigger - it makes you heal more often instead: you're currently acting every {interval}ms.",
            interval = c.combat_action_interval_ms(),
        ))
    } else {
        "Average healing you do per second across a fight.".to_string()
    };
    format!(
        "<div class=\"card\"><h2>Combat Stats</h2>\
          <div class=\"stat-row\">\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Total hit points before you go down.\">Health</div><div class=\"stat-value\">{max_hp}</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Average damage you deal per second across a fight.\">DPS</div><div class=\"stat-value\">{dps}</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"{hps_tip}\">HPS</div><div class=\"stat-value\">{hps}</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"How much incoming damage is reduced (or increased) before it lands, after block/evasion.\">{dr_label}</div><div class=\"stat-value\" data-tip=\"{dr_tip}\">{dr_value:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Chance an incoming hit is blocked, halving its damage.\">Block</div><div class=\"stat-value\" data-tip=\"{block_tip}\">{block:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Chance an incoming hit is avoided entirely.\">Evasion</div><div class=\"stat-value\" data-tip=\"{evasion_tip}\">{evasion:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Chance a hit lands as a critical strike. Past 100% guarantees extra crit stacks (double/triple/etc.) instead of capping.\">Crit Chance</div><div class=\"stat-value\">{crit_chance:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"How much bonus damage a critical hit deals on top of a normal hit.\">{crit_dmg_label}</div><div class=\"stat-value\">{crit_dmg_value:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"How much your outgoing damage is boosted (or reduced) overall.\">{dmg_label}</div><div class=\"stat-value\" data-tip=\"{dmg_tip}\">{dmg_value:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Fraction of a hit or heal that also splashes onto other targets. Past 100% grants 2 extra splash targets instead of being wasted.\">Splash</div><div class=\"stat-value\">{splash:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"How much your healing output is boosted. A Heal-role character starts at 100%; others start at 0%.\">Healing Power</div><div class=\"stat-value\">{heal_power:.0}%</div></div>\
            <div class=\"stat\"><div class=\"stat-label\" data-tip=\"Redirects a share of boss hits meant for other players onto you instead. Capped at 50% of your own investment - past that, the excess becomes bonus damage instead. Your party's total Intervene pool also separately caps at 50% redirected.\">Intervene</div><div class=\"stat-value\" data-tip=\"{intervene_tip}\">{intervene:.0}%</div></div>\
            {leech_stat}\
            {lingering_stat}\
          </div>\
        </div>",
        // Full max hp (base 20 + level*5, plus equipped body armor's
        // effective_power, times the archetype's max_hp_pct multiplier -
        // see Character::combat_max_hp) - scales with level the same way
        // every other combat stat here does, not the plain unscaled
        // Character::hp() base.
        max_hp = format_number(c.combat_max_hp() as f64),
        dps = format_number(c.combat_dps()),
        hps = format_number(c.combat_hps()),
        // .max(0.0) here is just a "-0%" float-rounding guard (these are
        // already floored at 0 mechanically) - cosmetic, not a real
        // negative-value case the way the four dynamic-label stats above are.
        block = (c.combat_block_chance() * 100.0).max(0.0),
        evasion = (c.combat_evasion() * 100.0).max(0.0),
        // Crit Chance is deliberately shown uncapped past 100% now (see
        // Character::combat_crit_chance's doc) - e.g. "250%" tells a
        // player they get a guaranteed double crit plus a 50% chance of
        // a third, not a display bug.
        crit_chance = (c.combat_crit_chance() * 100.0).max(0.0),
        splash = (c.combat_splash() * 100.0).max(0.0),
        // Healing Power is a plain 0%+ magnitude now, not a delta off a
        // universal 100% baseline (see Character::combat_heal_power's
        // doc - the baseline itself now varies: 0% off a Melee/Ranged
        // archetype, 100% off a Heal one) - same sign-free plain-number
        // treatment as Block/Evasion/Crit Chance/Splash above.
        heal_power = (c.combat_heal_power() * 100.0).max(0.0),
        intervene = (c.combat_intervene() * 100.0).max(0.0),
    )
}

/// Card grid for `/characters` - every character that's ever `!join`ed,
/// sorted by level desc (wins desc as a tiebreaker) so the roster reads
/// like a leaderboard. Each card links to `/characters/{login}` (see
/// `render_character_detail`). Empty state can't really happen on a live
/// server, but costs nothing to handle.
fn render_character_list(characters: &[(String, Character)], viewer: Option<&Character>) -> String {
    let header = format!("{}<div class=\"card\"><h1>Adventure Roster</h1></div>", top_nav(viewer));
    if characters.is_empty() {
        return format!("{header}<div class=\"card\"><p class=\"muted\">Nobody's joined the adventure yet.</p></div>");
    }
    let mut sorted: Vec<&(String, Character)> = characters.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| b.level.cmp(&a.level).then(b.wins.cmp(&a.wins)));
    let cards: String = sorted
        .iter()
        .map(|(login, c)| {
            let sprite = c.effective_sprite(login);
            let games = c.wins + c.losses;
            let winrate = if games > 0 { format!("{:.0}%", c.wins as f64 / games as f64 * 100.0) } else { "—".to_string() };
            format!(
                "<a class=\"roster-card\" href=\"/characters/{login_enc}\">\
                  <img class=\"roster-sprite\" src=\"/sprites/{sprite}.png\" onerror=\"this.onerror=null;this.src='/sprites/{sprite}.gif'\" alt=\"\">\
                  <div class=\"roster-name\">{name}</div>\
                  <div class=\"roster-meta\">Level {level} {archetype:?}</div>\
                  <div class=\"roster-meta\">{wins}W / {losses}L ({winrate})</div>\
                </a>",
                login_enc = urlencoding::encode(login),
                name = escape_html(&c.display_name),
                level = c.level,
                archetype = c.archetype,
                wins = format_number(c.wins as f64),
                losses = format_number(c.losses as f64),
            )
        })
        .collect();
    format!("{header}<div class=\"roster-grid\">{cards}</div>")
}

/// The inner block every item card renders identically - name, quality
/// line, primary stat, unique-affix line, tier, secondary-affix line, and
/// durability. All 4 item-card renderers (`render_gear_slot`/`_readonly`,
/// `render_inventory_item`/`_readonly`) built this exact same block via
/// their own copy-pasted `format!` before this was factored out - they
/// differ only in their wrapping label/action-button markup, which each
/// keeps for itself.
fn item_card_body_html(item: &Item) -> String {
    format!(
        "<div class=\"{name_class}\">{}</div>\
          {quality}\
          <div class=\"gear-primary\">{}</div>\
          {sacred}\
          {unique}\
          <div class=\"gear-tier\">Tier {}</div>\
          <div class=\"gear-stat\">{}</div>\
          {durability}\
          {locked_tag}",
        escape_html(&item.display_name()),
        escape_html(&gear_primary_stat(item)),
        item.tier,
        gear_stat_line(item),
        name_class = item_name_class(item),
        quality = quality_line_html(item),
        sacred = sacred_affix_html(item),
        unique = unique_affix_html(item),
        durability = durability_html(item),
        locked_tag = locked_tag_html(item),
    )
}

/// Same gear-slot layout `render_gear_slot` uses on the owner's own
/// dashboard, minus the form/buttons - viewing someone ELSE'S character
/// (see `render_character_detail`) is read-only, equip/repair/etc. only
/// ever apply to your own.
fn render_gear_slot_readonly(item: Option<&Item>, label: &str) -> String {
    match item {
        None => format!("<div class=\"gear-slot empty\"><div class=\"gear-slot-label\">{label}</div><div class=\"gear-empty\">— empty —</div></div>"),
        Some(item) => {
            let body = item_card_body_html(item);
            format!("<div class=\"gear-slot\"><div class=\"gear-slot-label\">{label}</div>{body}</div>")
        }
    }
}

/// Read-only counterpart to `render_inventory_item` - same fields, no
/// Equip/Disenchant/Repair forms.
fn render_inventory_item_readonly(item: &Item) -> String {
    let slot = item.slot;
    let body = item_card_body_html(item);
    format!("<div class=\"gear-slot\"><div class=\"gear-slot-label\">{slot:?}</div>{body}</div>")
}

/// `/characters/{login}` - a read-only mirror of the owner's own
/// dashboard (profile header, Combat Stats via the SAME shared
/// `render_combat_stats_card` the owner's page uses, Gear, Bag), just
/// with every action button stripped out.
fn render_character_detail(login: &str, c: &Character, viewer: Option<&Character>) -> String {
    let nav = top_nav(viewer);
    let name = escape_html(&c.display_name);
    let sprite = c.effective_sprite(login);
    let xp_pct = if c.xp_needed() > 0 { (c.xp as f64 / c.xp_needed() as f64 * 100.0).clamp(0.0, 100.0) } else { 100.0 };
    let games = c.wins + c.losses;
    let winrate = if games > 0 { format!("{:.0}%", c.wins as f64 / games as f64 * 100.0) } else { "—".to_string() };
    let combat_stats_html = render_combat_stats_card(c);
    let gear_html = [(EquipSlot::Weapon, "Weapon"), (EquipSlot::Helm, "Helm"), (EquipSlot::Body, "Body"), (EquipSlot::Gloves, "Gloves"), (EquipSlot::Boots, "Boots")]
        .into_iter()
        .map(|(slot, label)| render_gear_slot_readonly(c.equipped(slot).as_ref(), label))
        .collect::<String>();
    let bag_count = c.inventory.len();
    let inventory_html = if c.inventory.is_empty() { "<p class=\"muted\">Empty.</p>".to_string() } else { render_inventory_by_slot(&c.inventory, render_inventory_item_readonly) };
    let passives_link = if c.archetype == Archetype::Commoner {
        String::new()
    } else {
        format!("<a class=\"passives-link-btn\" href=\"/characters/{login}/passives\">🌳 View Passive Tree</a>")
    };
    format!(
        "{nav}\
        <div class=\"card\">\
          <p class=\"muted\"><a href=\"/characters\">&larr; Back to the roster</a></p>\
          <div class=\"profile-row\">\
            <img class=\"sprite-avatar\" src=\"/sprites/{sprite}.png\" onerror=\"this.onerror=null;this.src='/sprites/{sprite}.gif'\" alt=\"{name}'s sprite\">\
            <div class=\"profile-info\">\
              <div class=\"header-row\"><h1>{name}</h1><span class=\"role-badge role-{role_class}\">{archetype:?}</span>{passives_link}</div>\
              <div class=\"stat-row\">\
                <div class=\"stat\"><div class=\"stat-label\">Level</div><div class=\"stat-value\">{level}</div></div>\
                <div class=\"stat\"><div class=\"stat-label\">Record</div><div class=\"stat-value\">{wins}W / {losses}L</div></div>\
                <div class=\"stat\"><div class=\"stat-label\">Win rate</div><div class=\"stat-value\">{winrate}</div></div>\
                <div class=\"stat\"><div class=\"stat-label\">Dust</div><div class=\"stat-value\">{dust}</div></div>\
                <div class=\"stat\"><div class=\"stat-label\">Sand</div><div class=\"stat-value\">{sand}</div></div>\
              </div>\
            </div>\
          </div>\
          <div class=\"xp-label\">XP: {xp} / {xp_needed}</div>\
          <div class=\"xp-bar\"><div class=\"xp-fill\" style=\"width:{xp_pct:.0}%\"></div></div>\
        </div>\
        {combat_stats_html}\
        <div class=\"card\"><h2>Gear</h2><div class=\"gear-grid\">{gear_html}</div></div>\
        <div class=\"card\"><h2>Bag ({bag_count})</h2>{inventory_html}</div>",
        role_class = c.archetype.css_class(),
        archetype = c.archetype,
        level = c.level,
        wins = format_number(c.wins as f64),
        losses = format_number(c.losses as f64),
        dust = format_number(c.dust as f64),
        sand = format_number(c.sand as f64),
        xp = format_number(c.xp as f64),
        xp_needed = format_number(c.xp_needed() as f64),
    )
}

/// Real fixed display limit for `/fights` (see `render_fights_page`'s
/// doc) - fixes a stale bug: this page's own doc previously claimed
/// "the last up-to-10 encounters" while `recent_fights()` had no
/// `.take()` at all and actually rendered/re-summarized the WHOLE
/// coarse-tier history (up to `COARSE_FIGHTS_CAPACITY`, 100) on every
/// request.
const FIGHTS_PAGE_DISPLAY_LIMIT: usize = 10;

/// `/fights` - breakdown of recent encounters (2026-08-17, opened up from
/// streamer-only to every player - see `fights_for_viewer`), newest first:
/// outcome, boss stats (real boss fights only), the same top-3 DPS/tanks/
/// heals leaderboard the chat report uses (see `summarize_fight`), and
/// loot/broken/retreated. `fights` is already resolved (fetched + filtered
/// + limited) by the caller via `fights_for_viewer` - this function only
/// renders. Everything needed to start tuning balance without having to
/// catch a fight live or dig through raw JSON by hand, for the streamer;
/// a personal fight log for everyone else.
fn render_fights_page(viewer: Option<&Character>, fights: &[LastFightSnapshot], is_streamer: bool) -> String {
    let header = format!("{}<div class=\"card\"><h1>Fight History</h1></div>", top_nav(viewer));
    if fights.is_empty() {
        let msg = if is_streamer { "No fights logged yet." } else { "You haven't been in any recently logged fights yet." };
        return format!("{header}<div class=\"card\"><p class=\"muted\">{msg}</p></div>");
    }
    let cards: String = fights
        .iter()
        .map(|snapshot| {
            let r = &snapshot.result;
            let outcome = if r.won { "Won" } else { "Lost" };
            let title = match r.kind {
                EncounterKind::Boss => format!("Boss — Stage {} — {outcome}", r.stage),
                EncounterKind::Basic => format!(
                    "Basic — Stage {} — {outcome} — {} ({})",
                    r.stage,
                    r.enemy_name.as_deref().unwrap_or("enemies"),
                    r.enemy_count.map(|n| format!("{n}")).unwrap_or_default()
                ),
            };

            let boss_stats_html: String = snapshot
                .boss_stats
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let sprite = r.boss_sprites.get(i).map(String::as_str).unwrap_or("?");
                    format!(
                        "<li>#{n} ({sprite}): {hp} HP, {atk} ATK/{interval}ms, DR {dr:.0}%, Block {block:.0}%, Evasion {ev:.0}%, +{dmg:.0}% dmg, Crit {crit:.0}% x{critmult:.2}, Splash {splash:.0}%</li>",
                        n = i + 1,
                        hp = b.hp,
                        atk = b.atk,
                        interval = b.attack_interval_ms,
                        dr = b.damage_reduction * 100.0,
                        block = b.block_chance * 100.0,
                        ev = b.evasion * 100.0,
                        dmg = b.increased_damage * 100.0,
                        crit = b.crit_chance * 100.0,
                        critmult = b.crit_multiplier,
                        splash = b.splash * 100.0,
                    )
                })
                .collect();
            let boss_stats_section =
                if boss_stats_html.is_empty() { String::new() } else { format!("<h3>Boss Stats</h3><ul>{boss_stats_html}</ul>") };

            let summary = summarize_fight(&r.units, &r.events);
            let ranked = |entries: &[(String, u64)]| {
                if entries.is_empty() {
                    return "—".to_string();
                }
                let names = entries.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(", ");
                let total: u64 = entries.iter().map(|(_, amt)| amt).sum();
                format!("{names} ({total})")
            };
            let first_down = summary.first_to_die.as_deref().unwrap_or("—").to_string();

            // Combat logging (2026-08-15, a live request: "a robust log
            // system... live buff/debuff stack counts along with skills
            // used"). Skill casts - grouped by (unit, skill) with a
            // count rather than one row per instance, since a single
            // fight can carry dozens of the same cast (Bloodpact/
            // FlickerStrike especially) - sorted by count desc, capped
            // so a big roster's fight doesn't blow the card out.
            let mut skill_counts: HashMap<(String, String), u32> = HashMap::new();
            let mut skill_display_name: HashMap<String, String> = HashMap::new();
            for event in &r.events {
                if let CombatEvent::SkillCast { unit, skill, .. } = event {
                    let display = r.units.iter().find(|u| &u.id == unit).map(|u| u.display_name.clone()).unwrap_or_else(|| unit.clone());
                    skill_display_name.entry(unit.clone()).or_insert(display);
                    *skill_counts.entry((unit.clone(), skill.clone())).or_insert(0) += 1;
                }
            }
            let mut skill_rows: Vec<((String, String), u32)> = skill_counts.into_iter().collect();
            skill_rows.sort_by(|a, b| b.1.cmp(&a.1));
            const SKILL_ROWS_CAP: usize = 25;
            let skill_html = if skill_rows.is_empty() {
                "<li class=\"muted\">None logged</li>".to_string()
            } else {
                let rows: String = skill_rows
                    .iter()
                    .take(SKILL_ROWS_CAP)
                    .map(|((unit, skill), count)| {
                        let name = skill_display_name.get(unit).map(String::as_str).unwrap_or(unit.as_str());
                        format!("<li>{} — {} ×{count}</li>", escape_html(name), escape_html(skill))
                    })
                    .collect();
                let overflow = if skill_rows.len() > SKILL_ROWS_CAP {
                    format!("<li class=\"muted\">...and {} more</li>", skill_rows.len() - SKILL_ROWS_CAP)
                } else {
                    String::new()
                };
                format!("{rows}{overflow}")
            };

            // Buff/debuff stack activity - every (unit, buff name) pair
            // that showed up in at least one `BuffSnapshot`, with how
            // many snapshots recorded it, its average stack across those
            // snapshots, and its peak. NOT time-weighted (a snapshot only
            // fires on an actual hit/heal, not on a fixed clock - see
            // `CombatEvent::BuffSnapshot`'s own doc for why it's sparse
            // by design), so "average" here means "average of the
            // recorded samples", the most honest claim the data actually
            // supports. Sorted by occurrence count desc, capped same as
            // the skill list above.
            let mut buff_samples: HashMap<(String, String), Vec<f64>> = HashMap::new();
            for event in &r.events {
                if let CombatEvent::BuffSnapshot { unit, buffs, .. } = event {
                    let display = r.units.iter().find(|u| &u.id == unit).map(|u| u.display_name.clone()).unwrap_or_else(|| unit.clone());
                    for (buff_name, value) in buffs {
                        buff_samples.entry((display.clone(), buff_name.clone())).or_default().push(*value);
                    }
                }
            }
            let mut buff_rows: Vec<((String, String), Vec<f64>)> = buff_samples.into_iter().collect();
            buff_rows.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            const BUFF_ROWS_CAP: usize = 25;
            let buff_html = if buff_rows.is_empty() {
                "<tr><td colspan=\"5\" class=\"muted\">None logged</td></tr>".to_string()
            } else {
                let rows: String = buff_rows
                    .iter()
                    .take(BUFF_ROWS_CAP)
                    .map(|((name, buff), samples)| {
                        let n = samples.len();
                        let avg = samples.iter().sum::<f64>() / n as f64;
                        let peak = samples.iter().cloned().fold(0.0_f64, f64::max);
                        format!(
                            "<tr><td>{}</td><td>{}</td><td>{n}</td><td>{avg:.2}</td><td>{peak:.2}</td></tr>",
                            escape_html(name),
                            escape_html(buff)
                        )
                    })
                    .collect();
                let overflow = if buff_rows.len() > BUFF_ROWS_CAP {
                    format!("<tr><td colspan=\"5\" class=\"muted\">...and {} more</td></tr>", buff_rows.len() - BUFF_ROWS_CAP)
                } else {
                    String::new()
                };
                format!("{rows}{overflow}")
            };

            let started_at = if snapshot.started_at_unix_ms > 0 {
                format_unix_secs((snapshot.started_at_unix_ms / 1000) as i64)
            } else {
                "unknown time (logged before this was tracked)".to_string()
            };

            let loot_html: String = if r.loot.is_empty() {
                "<li class=\"muted\">None</li>".to_string()
            } else {
                r.loot
                    .iter()
                    .map(|l| {
                        let stats = if l.affixes.is_empty() {
                            String::new()
                        } else {
                            let list = l.affixes.iter().map(|(a, v)| affix_display(*a, *v)).collect::<Vec<_>>().join(", ");
                            format!(" [{list}]")
                        };
                        format!(
                            "<li>{} — {} T{} ({:?}, {:?}){stats}</li>",
                            escape_html(&l.display_name),
                            escape_html(&l.item_name),
                            l.tier,
                            l.slot,
                            l.outcome
                        )
                    })
                    .collect()
            };
            let broken_html: String = if r.broken.is_empty() {
                "<li class=\"muted\">None</li>".to_string()
            } else {
                r.broken.iter().map(|b| format!("<li>{} — {}</li>", escape_html(&b.display_name), escape_html(&b.item_name))).collect()
            };

            format!(
                "<div class=\"card\">\
                  <h2>{title}</h2>\
                  <p class=\"muted\">{started_at} · {participants} participants · {events} events · {display_ms}ms display</p>\
                  {boss_stats_section}\
                  <h3>Battle Report</h3>\
                  <ul>\
                    <li>🗡️ Top DPS: {dps}</li>\
                    <li>🛡️ Top Tanks: {tanks}</li>\
                    <li>💚 Top Heals: {heals}</li>\
                    <li>💀 First down: {first_down}</li>\
                  </ul>\
                  <h3>Skills Cast</h3><ul>{skill_html}</ul>\
                  <h3>Buff/Debuff Stack Activity</h3>\
                  <div style=\"overflow-x:auto\">\
                  <table class=\"buff-activity-table\">\
                    <thead><tr><th>Unit</th><th>Buff/Debuff</th><th>Samples</th><th>Avg Stack</th><th>Peak Stack</th></tr></thead>\
                    <tbody>{buff_html}</tbody>\
                  </table>\
                  </div>\
                  <h3>Loot</h3><ul>{loot_html}</ul>\
                  <h3>Broken Gear</h3><ul>{broken_html}</ul>\
                </div>",
                participants = r.participants.len(),
                events = r.events.len(),
                display_ms = r.display_duration_ms,
                dps = ranked(&summary.top_damage_dealt),
                tanks = ranked(&summary.top_damage_taken),
                heals = ranked(&summary.top_healing_done),
            )
        })
        .collect();
    format!("{header}{cards}")
}

/// Admin-only live-tunables form (see `ADMIN_TUNABLES_LOGIN`/`LiveTunables`)
/// - one number input per field, pre-filled with the current live value,
/// grouped into the same two sections `LiveTunables`'s own doc describes.
/// A save POSTs everything at once (not per-field) and redirects back here
/// with `?saved=1` for the confirmation banner - same query-param-flash
/// pattern `IndexParams`'s fields already use elsewhere on this dashboard.
fn render_tunables_page(viewer: Option<&Character>, t: &LiveTunables, current_boss_power_mult: f64, saved: bool) -> String {
    let nav = top_nav(viewer);
    let banner = if saved {
        "<p class=\"muted\">✅ Saved — takes effect on the very next encounter, no restart needed.</p>"
    } else {
        ""
    };
    format!(
        "{nav}\
        <div class=\"card\">\
          <h1>⚙️ Live Tunables</h1>\
          <p class=\"muted\">Changes apply immediately to the next fight — no rebuild, no restart required.</p>\
          {banner}\
          <form method=\"post\" action=\"/admin/tunables/save\">\
            <h2>Drop Rates</h2>\
            <div class=\"tunable-row\">\
              <label for=\"loot_mult\">Loot Multiplier</label>\
              <input type=\"number\" step=\"any\" id=\"loot_mult\" name=\"loot_mult\" value=\"{loot_mult}\">\
              <p class=\"tunable-hint\">Scales dust, item drops, and craft-token drops together (boss and basic-encounter wins alike).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"sand_mult\">Sand Multiplier</label>\
              <input type=\"number\" step=\"any\" id=\"sand_mult\" name=\"sand_mult\" value=\"{sand_mult}\">\
              <p class=\"tunable-hint\">Scales sand from boss wins, basic wins, and disenchanting — separate from Loot Multiplier.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"wings_drop_chance\">Wings of Flight Drop Chance</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"wings_drop_chance\" name=\"wings_drop_chance\" value=\"{wings_drop_chance}\">\
              <p class=\"tunable-hint\">0 to 1 (e.g. 0.0001 = 0.01%). Rolled on every real item drop.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"celestial_shard_drop_chance\">Unique Shard Drop Rate</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"celestial_shard_drop_chance\" name=\"celestial_shard_drop_chance\" value=\"{celestial_shard_drop_chance}\">\
              <p class=\"tunable-hint\">0 to 1 (e.g. 0.001 = 0.1%). One shared rate, rolled independently for the Celestial Shard and Unique Shard tokens on every real item drop - rolls for every archetype.</p>\
            </div>\
            <h2>Boss Difficulty</h2>\
            <div class=\"tunable-row\">\
              <label for=\"boss_health\">Boss Health</label>\
              <input type=\"number\" step=\"any\" id=\"boss_health\" name=\"boss_health\" value=\"{boss_health}\">\
              <p class=\"tunable-hint\">Single multiplier on boss HP (consolidated 2026-08-16 from 4 separate dials — 1.0 = base design.)</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"boss_power\">Boss Power</label>\
              <input type=\"number\" step=\"any\" id=\"boss_power\" name=\"boss_power\" value=\"{boss_power}\">\
              <p class=\"tunable-hint\">Boss Health's own counterpart for boss ATK. 1.0 = base design.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"dynamic_scaling_mult\">Dynamic Scaling Modifier</label>\
              <input type=\"number\" step=\"any\" id=\"dynamic_scaling_mult\" name=\"dynamic_scaling_mult\" value=\"{dynamic_scaling_mult}\">\
              <p class=\"tunable-hint\">How hard the automatic win/loss rubber-band reacts to streaks. 1.0 = normal, 0 = disabled (boss power never moves from wins/losses), &gt;1 = swings harder/faster.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"boss_power_mult_override\">Current Dynamic Boss Power: {current_boss_power_mult:.3}x</label>\
              <input type=\"text\" id=\"boss_power_mult_override\" name=\"boss_power_mult_override\" placeholder=\"leave blank to keep as-is\">\
              <p class=\"tunable-hint\">Manual override for the live value above — e.g. after a bad losing streak leaves it stuck low. Leave blank to change nothing.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"boss_count_tier_stages\">Boss Count Tier Size (stages)</label>\
              <input type=\"number\" step=\"1\" min=\"1\" id=\"boss_count_tier_stages\" name=\"boss_count_tier_stages\" value=\"{boss_count_tier_stages}\">\
              <p class=\"tunable-hint\">Boss count = 1 + a random 1-or-2 roll per completed tier of this many stages (jitter, re-rolled every fight), capped below. E.g. 100 means stage 400 is 4 tiers.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"boss_count_cap_mult\">Boss Count Cap Multiplier</label>\
              <input type=\"number\" step=\"0.1\" min=\"0\" id=\"boss_count_cap_mult\" name=\"boss_count_cap_mult\" value=\"{boss_count_cap_mult}\">\
              <p class=\"tunable-hint\">Hard ceiling on total bosses: floor(tiers × this). E.g. stage 400 (4 tiers) × 1.5 caps at 6 bosses even though the jitter alone could roll up to 8 — the jitter is what makes any two fights at the same stage different, this is what keeps it from spiraling. Only 5 named boss kinds exist, so past 5 the extra slots duplicate (preferring variety first — see BossKind::random_excluding_multiple).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"late_content_stage\">Late-Content Stage Threshold</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"late_content_stage\" name=\"late_content_stage\" value=\"{late_content_stage}\">\
              <p class=\"tunable-hint\">Gates the guaranteed-Perfect-item milestones (per-character and per-kill) — unrelated to Boss Health/Power above.</p>\
            </div>\
            <h2>Rampage</h2>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"permanent_rampage\" value=\"1\"{permanent_rampage_checked}> Permanent Rampage</label>\
            <p class=\"tunable-hint\">Unlike !rampage (a one-time 50-fight burst), this never runs out — boss fights back-to-back with instant revives between them, until unchecked here.</p>\
            <button class=\"btn\" type=\"submit\">Save</button>\
          </form>\
        </div>",
        loot_mult = t.loot_mult,
        sand_mult = t.sand_mult,
        wings_drop_chance = t.wings_drop_chance,
        celestial_shard_drop_chance = t.celestial_shard_drop_chance,
        boss_health = t.boss_health,
        boss_power = t.boss_power,
        dynamic_scaling_mult = t.dynamic_scaling_mult,
        current_boss_power_mult = current_boss_power_mult,
        boss_count_tier_stages = t.boss_count_tier_stages,
        boss_count_cap_mult = t.boss_count_cap_mult,
        late_content_stage = t.late_content_stage,
        permanent_rampage_checked = if t.permanent_rampage { " checked" } else { "" },
    )
}

/// Prominent top-of-page nav bar linking to the roster - shown on every
/// logged-in dashboard state (joined, not-yet-joined) so browsing other
/// players' characters doesn't require hunting for a small muted link.
/// A live request added two more things here: a link straight back to
/// "/" (every OTHER page here already had a way OFF the dashboard, but
/// nothing to get back to it short of the browser's own back button or
/// re-typing the URL) and a link to `/overlay` (a live fight, watchable
/// without the stream itself) - both belong on every page the same way
/// the other 4 links already do, not just the dashboard. `character`
/// (`None` for the not-yet-joined dashboard/inventory/passives states,
/// `Some` everywhere else including the two public no-login pages -
/// wiki/patch-notes callers just always pass whatever the current
/// session resolved, if any) drives a compact stat summary appended
/// after the links - another live request, so a player's own level/
/// archetype/dust/sand stay visible while browsing pages that otherwise
/// show none of that (Bag & Crafting, Passives, Wiki, Character List).
fn top_nav(character: Option<&Character>) -> String {
    let stats = character.map_or(String::new(), |c| {
        format!(
            "<span class=\"top-nav-stats\">Lv {level} {archetype:?} · 💰 {dust} · \u{1FAB5} {sand}</span>",
            level = c.level,
            archetype = c.archetype,
            dust = format_number(c.dust as f64),
            sand = format_number(c.sand as f64),
        )
    });
    format!(
        "<div class=\"top-nav\">\
          <a class=\"top-nav-link\" href=\"/\">🏠 Character Sheet</a>\
          <a class=\"top-nav-link\" href=\"/inventory\">🎒 Bag &amp; Crafting</a>\
          <a class=\"top-nav-link\" href=\"/passives\">🌳 Passives</a>\
          <a class=\"top-nav-link\" href=\"/characters\">🏆 Character List</a>\
          <a class=\"top-nav-link\" href=\"/fights\">📜 Fight History</a>\
          <a class=\"top-nav-link\" href=\"/wiki\">📖 Wiki</a>\
          <a class=\"top-nav-link\" href=\"/overlay\" target=\"_blank\" rel=\"noopener\">📺 Watch Overlay</a>\
          {stats}\
        </div>"
    )
}

fn render_dashboard(
    login: &str,
    display_name: &str,
    character: Option<&Character>,
    reforge_used_this_hour: bool,
    reforge_next_reset_ms: u64,
) -> String {
    let name = escape_html(display_name);
    let nav = top_nav(character);
    let Some(c) = character else {
        return format!(
            "{nav}\
              <div class=\"card\"><h1>Welcome, {name}!</h1>\
              <p>You haven't joined the adventure yet.</p>\
              <form method=\"post\" action=\"/join\"><button class=\"btn\" type=\"submit\">Join the Adventure</button></form>\
              <p class=\"muted\"><a href=\"/patch-notes\">Patch Notes</a> &middot; <a href=\"/logout\">Log out</a></p></div>"
        );
    };

    let xp_pct = if c.xp_needed() > 0 { (c.xp as f64 / c.xp_needed() as f64 * 100.0).clamp(0.0, 100.0) } else { 100.0 };
    let games = c.wins + c.losses;
    let winrate = if games > 0 { format!("{:.0}%", c.wins as f64 / games as f64 * 100.0) } else { "—".to_string() };
    let sprite = c.effective_sprite(login);

    // Retreated now covers two distinct cases: gear actually worn out
    // (the original path, with a real free-repair countdown), and a mod
    // forcing everyone off via !clearbattlefield regardless of gear
    // state - the latter has nothing to repair, so the gear-focused
    // messaging/countdown would be actively misleading for it.
    let retreat_banner = match c.retreated_since {
        None => String::new(),
        Some(since) if c.all_gear_worn_out() => {
            let auto_repair_at = since + RETREAT_REPAIR_DURATION;
            let ms = auto_repair_at.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            format!(
                "<div class=\"card countdown-card retreat-card\" data-reset-ms=\"{ms}\">\
                  <h2>🏳️ Retreated</h2>\
                  <p>Every piece of your equipped gear is at 0% durability, so you're sitting out every encounter until you're back.</p>\
                  <p>Repair gear below (costs dust) to jump back in immediately — or wait for a free gear repair in <span class=\"countdown-timer\">--:--</span>, which puts you right back on the battlefield too, no !join needed.</p>\
                </div>"
            )
        }
        Some(_) => "<div class=\"card retreat-card\">\
              <h2>🏳️ Off the battlefield</h2>\
              <p>You're currently sitting out (a mod cleared the battlefield) — your gear's fine, just type !join in chat to rejoin!</p>\
            </div>"
            .to_string(),
    };

    let gear_html = [(EquipSlot::Weapon, "Weapon"), (EquipSlot::Helm, "Helm"), (EquipSlot::Body, "Body"), (EquipSlot::Gloves, "Gloves"), (EquipSlot::Boots, "Boots")]
        .into_iter()
        .map(|(slot, label)| render_gear_slot(c, slot, label))
        .collect::<String>();

    let reforge_status_html = if reforge_used_this_hour {
        "<span class=\"reforge-pill reforge-used\">⏳ Used this hour</span>".to_string()
    } else {
        let disabled = if c.dust < WEB_REFORGE_DUST_COST { " disabled" } else { "" };
        format!(
            "<span class=\"reforge-pill reforge-ready\">✅ Available</span>\
              <form method=\"post\" action=\"/reforge\">\
                <button class=\"btn\" type=\"submit\"{disabled}>Reforge Now ({WEB_REFORGE_DUST_COST}d)</button>\
              </form>"
        )
    };

    let repair_all_cost = c.repair_all_cost();
    let repair_all_html = if repair_all_cost == 0 {
        String::new()
    } else {
        let disabled = if c.dust < repair_all_cost { " disabled" } else { "" };
        format!(
            "<form method=\"post\" action=\"/repair-all\">\
              <button class=\"btn-sm btn-repair\" type=\"submit\"{disabled}>Repair All ({repair_all_cost}d)</button>\
            </form>"
        )
    };
    let auto_repair_checked = if c.auto_repair { " checked" } else { "" };
    let auto_repair_html = format!(
        "<form method=\"post\" action=\"/toggle-auto-repair\" class=\"protect-toggle\">\
          <label><input type=\"checkbox\" name=\"auto_repair\"{auto_repair_checked} onchange=\"this.form.submit()\"> \u{1f527} Auto-repair gear with dust after every boss fight</label>\
        </form>"
    );

    let combat_stats_html = render_combat_stats_card(c);

    let archetype_picker_html = render_archetype_picker(c);
    let model_picker_html = render_model_picker(c, login);
    let wings_card_html = render_wings_card(c);

    format!(
        "{nav}\
        {retreat_banner}\
        <div class=\"dashboard-grid\">\
          <div class=\"dashboard-col\">\
            <div class=\"card\">\
              <div class=\"profile-row\">\
                <img class=\"sprite-avatar\" src=\"/sprites/{sprite}.png\" onerror=\"this.onerror=null;this.src='/sprites/{sprite}.gif'\" alt=\"{name}'s sprite\">\
                <div class=\"profile-info\">\
                  <div class=\"header-row\"><h1>{name}</h1><span class=\"role-badge role-{role_class}\">{archetype:?}</span></div>\
                  <div class=\"stat-row\">\
                    <div class=\"stat\"><div class=\"stat-label\">Level</div><div class=\"stat-value\">{level}</div></div>\
                    <div class=\"stat\"><div class=\"stat-label\">Record</div><div class=\"stat-value\">{wins}W / {losses}L</div></div>\
                    <div class=\"stat\"><div class=\"stat-label\">Win rate</div><div class=\"stat-value\">{winrate}</div></div>\
                    <div class=\"stat\"><div class=\"stat-label\">Dust</div><div class=\"stat-value\">{dust}</div></div>\
                    <div class=\"stat\"><div class=\"stat-label\">Sand</div><div class=\"stat-value\">{sand}</div></div>\
                  </div>\
                </div>\
              </div>\
              <div class=\"xp-label\">XP: {xp} / {xp_needed}</div>\
              <div class=\"xp-bar\"><div class=\"xp-fill\" style=\"width:{xp_pct:.0}%\"></div></div>\
            </div>\
            {archetype_picker_html}\
            {combat_stats_html}\
            {model_picker_html}\
            {wings_card_html}\
          </div>\
          <div class=\"dashboard-col\">\
            <div class=\"card\">\
              <div class=\"header-row\"><h2>Gear</h2>{repair_all_html}</div>\
              <div class=\"gear-grid\">{gear_html}</div>\
              {auto_repair_html}\
              <p class=\"muted\"><a href=\"/inventory\">Bag &amp; Crafting &rarr;</a></p>\
            </div>\
            <div class=\"card countdown-card\" data-reset-ms=\"{reforge_next_reset_ms}\">\
              <h2>Reforge Gear</h2>\
              <p class=\"muted\">Upgrades one random equipped item 2-4 tiers — shares its once-per-hour limit with the channel points redemption.</p>\
              {reforge_status_html}\
              <p class=\"reforge-countdown\">Resets in <span class=\"countdown-timer\">--:--</span></p>\
            </div>\
          </div>\
        </div>\
        <p class=\"muted\"><a href=\"/patch-notes\">Patch Notes</a> &middot; <a href=\"/logout\">Log out</a></p>",
        role_class = c.archetype.css_class(),
        archetype = c.archetype,
        level = c.level,
        wins = format_number(c.wins as f64),
        losses = format_number(c.losses as f64),
        xp = format_number(c.xp as f64),
        xp_needed = format_number(c.xp_needed() as f64),
        dust = format_number(c.dust as f64),
        sand = format_number(c.sand as f64),
    )
}

/// `/inventory` page body - Bag and Crafting, split out of the main
/// dashboard (see `render_dashboard`'s doc). Shows the "haven't joined"
/// prompt if `character` is `None`, same as the dashboard does, since
/// this page is reachable straight from `top_nav`'s links regardless of
/// join state.
fn render_inventory_page(display_name: &str, character: Option<&Character>, pending_veil: Option<&PendingVeil>) -> String {
    let name = escape_html(display_name);
    let nav = top_nav(character);
    let Some(c) = character else {
        return format!(
            "{nav}\
              <div class=\"card\"><h1>Welcome, {name}!</h1>\
              <p>You haven't joined the adventure yet.</p>\
              <form method=\"post\" action=\"/join\"><button class=\"btn\" type=\"submit\">Join the Adventure</button></form>\
              <p class=\"muted\"><a href=\"/\">&larr; Back to your character</a></p></div>"
        );
    };

    let gear_html = [(EquipSlot::Weapon, "Weapon"), (EquipSlot::Helm, "Helm"), (EquipSlot::Body, "Body"), (EquipSlot::Gloves, "Gloves"), (EquipSlot::Boots, "Boots")]
        .into_iter()
        .map(|(slot, label)| render_gear_slot(c, slot, label))
        .collect::<String>();

    let bag_count = c.inventory.len();
    let inventory_html = if c.inventory.is_empty() {
        "<p class=\"muted\">Empty — gear you find lands here to equip yourself.</p>".to_string()
    } else {
        render_inventory_by_slot(&c.inventory, |item| render_inventory_item(item, c.dust))
    };

    // Only shown when there's actually something it would touch -
    // Krangled and disenchant-protected items don't count (see
    // Character::disenchant_all_from_inventory), so an all-precious bag
    // doesn't get a button that would just do nothing.
    let disenchantable_count = c.inventory.iter().filter(|i| !i.locked && !i.disenchant_protected).count();
    let disenchant_all_html = if disenchantable_count == 0 {
        String::new()
    } else {
        format!(
            "<form method=\"post\" action=\"/disenchant-all\" onsubmit=\"return confirm('Disenchant all {disenchantable_count} eligible item(s) for dust? Krangled and \\'Keep\\'-marked items are skipped. This can\\'t be undone.');\">\
              <button class=\"btn-sm btn-danger\" type=\"submit\">Disenchant All ({disenchantable_count})</button>\
            </form>"
        )
    };

    // Free craft-action tokens (see Character::craft_tokens) - "shown in
    // the player's inventory" per the original request, so it lives
    // right above the Bag rows rather than as its own separate card.
    let craft_tokens_html = {
        let held: String = c
            .craft_tokens
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(action, n)| {
                // Celestial/Unique Shard's token name doesn't self-describe
                // what it grants (unlike Scour/Krangle/etc, which are
                // self-explanatory) - spell out the unique affix it grants
                // right in the pill so a held token isn't a mystery, plus
                // the full tooltip every craft button already gets.
                let grants = match action {
                    CraftAction::CelestialShard => " \u{2192} Celestial Conversion",
                    CraftAction::UniqueShard => " \u{2192} Split Personality",
                    _ => "",
                };
                format!("<span class=\"token-pill\" data-tip=\"{tip}\">🎫 {label}{grants} ×{n}</span>", tip = escape_html(craft_action_tip(*action)), label = action.label())
            })
            .collect();
        if held.is_empty() { String::new() } else { format!("<div class=\"token-row\">{held}</div>") }
    };

    let nickname_prompt_html = render_nickname_prompt(c);
    let crafting_card_html = match pending_veil {
        Some(pending) => render_veil_choice_card(pending),
        None => render_crafting_card(c),
    };

    // Combat Stats (2026-08-17, a live request) - same shared card the
    // main dashboard uses (see render_combat_stats_card), so a player
    // crafting/reforging can see whether it actually moved a number
    // without tabbing back to "/".
    let combat_stats_html = render_combat_stats_card(c);

    // Top row: Equipped Items (left) / Crafting (right), same
    // dashboard-grid 2-column layout the main dashboard itself uses -
    // per the request's mockup. Bag rows (one collapsible row per equip
    // slot, not the old per-slot columns) sit full-width below both.
    format!(
        "{nav}\
        <p class=\"muted\"><a href=\"/\">&larr; Back to your character</a></p>\
        <div class=\"dashboard-grid\">\
          <div class=\"dashboard-col\">\
            <div class=\"card\"><h2>Equipped Items</h2><div class=\"gear-grid\">{gear_html}</div></div>\
            {combat_stats_html}\
          </div>\
          <div class=\"dashboard-col\">\
            {nickname_prompt_html}\
            {crafting_card_html}\
          </div>\
        </div>\
        <div class=\"card bag-card\"><div class=\"header-row\"><h2>Bag ({bag_count}/{cap})</h2>{disenchant_all_html}</div>{auto_disenchant_html}{craft_tokens_html}{inventory_html}</div>",
        cap = INVENTORY_CAPACITY,
        auto_disenchant_html = render_auto_disenchant_settings(c),
    )
}

/// Checkbox + dropdown + number, all in ONE self-submitting form so any
/// single change (ticking the box, picking a different tier, retyping the
/// percent) posts all 3 current values together - see
/// `AdventureManager::set_auto_disenchant`. "Whatever's selected is safe,
/// and anything above it in quality/rarity is safe" (2026-08-16, a live
/// request): picking Sacred means ONLY Sacred is safe (even Perfect items
/// get scrapped); picking Perfect means Perfect and Sacred are both safe;
/// picking Quality % means that percent (or Perfect/Sacred) is safe - see
/// `Item::meets_auto_disenchant_floor`. The percent input stays visible
/// regardless of which tier is selected (rather than hiding/showing via
/// JS) so its last value is never lost switching tiers and back.
fn render_auto_disenchant_settings(c: &Character) -> String {
    let checked = if c.auto_disenchant_enabled { " checked" } else { "" };
    let opt = |tier: AutoDisenchantTier, value: &str, label: &str| {
        let selected = if c.auto_disenchant_tier == tier { " selected" } else { "" };
        format!("<option value=\"{value}\"{selected}>{label}</option>")
    };
    format!(
        "<form method=\"post\" action=\"/set-auto-disenchant\" class=\"protect-toggle auto-disenchant-row\">\
          <label><input type=\"checkbox\" name=\"enabled\" value=\"on\"{checked} onchange=\"this.form.submit()\"> \u{1f5d1}\u{fe0f} Auto-disenchant new items below:</label>\
          <select name=\"tier\" onchange=\"this.form.submit()\">{quality_opt}{perfect_opt}{sacred_opt}</select>\
          <input type=\"number\" name=\"min_percent\" min=\"1\" max=\"100\" value=\"{min_percent}\" onchange=\"this.form.submit()\" title=\"Quality % floor - only used when 'Quality %' is selected above\">\
        </form>",
        quality_opt = opt(AutoDisenchantTier::Quality, "quality", "Quality % (below)"),
        perfect_opt = opt(AutoDisenchantTier::Perfect, "perfect", "Perfect"),
        sacred_opt = opt(AutoDisenchantTier::Sacred, "sacred", "Sacred"),
        min_percent = c.auto_disenchant_min_percent,
    )
}

/// Archetype picker card - a `<select>` of every archetype (see
/// `ALL_ARCHETYPES`), each option labeled with its own bonus, defaulted
/// to whichever one the character currently has (see Combat Stats,
/// above this card - own it there, don't duplicate it here) so its
/// advantages are right there in the closed dropdown without a separate
/// summary line above it. Free while still `Commoner` (the unspecialized
/// starting state, not itself a selectable option); `ARCHETYPE_CHANGE_COST`
/// dust every time after, button disabled if they can't afford it (same
/// disabled-button pattern as the reforge/repair buttons above).
fn render_archetype_picker(c: &Character) -> String {
    let free_changes_note = if c.free_archetype_changes > 0 {
        format!("<p class=\"muted\">You have {} free change{} banked!</p>", c.free_archetype_changes, if c.free_archetype_changes == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    let options: String = ALL_ARCHETYPES
        .iter()
        .map(|a| {
            let selected = if *a == c.archetype { " selected" } else { "" };
            format!(
                "<option value=\"{value}\"{selected}>{a:?} — {desc}</option>",
                value = format!("{a:?}").to_lowercase(),
                desc = escape_html(&a.description(c.level)),
            )
        })
        .collect();
    let free = c.free_archetype_changes > 0;
    let (button_label, disabled) =
        if free { ("Choose (Free)".to_string(), "") } else { (format!("Change ({ARCHETYPE_CHANGE_COST} dust)"), if c.dust < ARCHETYPE_CHANGE_COST { " disabled" } else { "" }) };
    format!(
        "<div class=\"card\">\
          <div class=\"header-row\"><h2>Archetype</h2><span class=\"role-badge role-{class}\">{archetype:?}</span></div>\
          {free_changes_note}\
          <form method=\"post\" action=\"/change-archetype\">\
            <select name=\"archetype\">{options}</select>\
            <button class=\"btn\" type=\"submit\"{disabled}>{button_label}</button>\
          </form>\
        </div>",
        class = c.archetype.css_class(),
        archetype = c.archetype,
    )
}

/// `/passives` - the character's own passive skill tree (see
/// `passive_tree.rs`) for their CURRENT archetype only - unlike the
/// design artifact's own mockup (all 11 classes side by side for
/// comparison), a real character only ever has one live tree at a time.
/// Node buttons post to `/passives/allocate`, mutating a PREVIEW (see
/// `AdventureManager::preview_allocate_passive`) - nothing is saved until
/// Save Changes, same "compare freely" idea `PendingVeil` established for
/// crafting. `preview` is `None` when there's nothing unsaved, in which
/// case the page just shows `Character::passive_allocations` directly.
/// (icon, role label) per archetype for the passive tree's class-strip -
/// same icon/role choices as the design artifact's own `CLASSES` map, no
/// `Archetype::css_class`-style enum method exists for these since
/// they're purely this one page's presentation, not combat data.
fn passive_archetype_icon_role(a: Archetype) -> (&'static str, &'static str) {
    let role = match a.combat_function() {
        crate::adventure::CombatFunction::Melee => "Melee",
        crate::adventure::CombatFunction::Ranged => "Ranged",
        crate::adventure::CombatFunction::Heal => "Support",
    };
    let icon = match a {
        Archetype::Slayer => "🩸",
        Archetype::Warrior => "🛡️",
        Archetype::Berserker => "🪓",
        Archetype::Rogue => "🗡️",
        Archetype::Monk => "🥋",
        Archetype::Paladin => "⚜️",
        Archetype::Ranger => "🏹",
        Archetype::Mage => "🔮",
        Archetype::Warlock => "💀",
        Archetype::Cleric => "✨",
        Archetype::Druid => "🍃",
        Archetype::Commoner => "❔",
    };
    (icon, role)
}

/// One positioned node in the passive-tree layout (see
/// `compute_passive_layout`) - shared between the interactive owner's-own
/// `/passives` page and the read-only `/characters/{login}/passives` view,
/// so the two can never visually drift apart.
struct PosNode {
    node: PassiveNode,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    locked: bool,
}

/// Result of `compute_passive_layout` - everything needed to draw the tree
/// (node boxes + SVG connector lines + stage size) for a given character's
/// CURRENT ranks. Deliberately carries no rendering/interactivity - that's
/// the caller's job (`render_passive_tree_page`'s node_html builds allocate
/// forms; the read-only view just prints rank text).
struct PassiveLayout {
    positioned: Vec<PosNode>,
    root_x: f64,
    svg_lines: String,
    stage_w: f64,
    stage_h: f64,
}

/// Shared layout math for the passive tree - a LITERAL port of the design
/// artifact's own layout() - same 4 rows (root class passive -> 3 skills ->
/// up to 9 specializations, always leaves -> up to 27 modifiers shown only
/// under whichever specialization(s) are currently unlocked), same
/// width/height/row-Y/gap constants, same "position is the node's CENTER,
/// CSS transform: translate(-50%,-50%) does the rest" scheme - not a
/// reinterpretation. Positions are recomputed fresh every request straight
/// off the node list + `allocations`, so the boxes and the SVG connector
/// lines between them can never drift out of sync with each other, same
/// reasoning the artifact's own JS layout() used. `allocations` is the
/// PREVIEW map for the owner's own interactive page, or simply
/// `&c.passive_allocations` (no such thing as a preview) for the read-only
/// view of someone else's character. Takes `Archetype` directly rather
/// than `&Character` - `archetype.passive_nodes()` is the only thing this
/// (and `root_node_html` below) ever reads off a character, so the wiki's
/// reference tree (`render_wiki_archetype_graph`, no real character
/// backing it) can call this with just an `Archetype` and a synthetic
/// all-maxed allocations map, instead of needing a throwaway `Character`.
fn compute_passive_layout(archetype: Archetype, allocations: &HashMap<String, u32>) -> PassiveLayout {
    let rank_of = |key: &str| allocations.get(key).copied().unwrap_or(0);
    let nodes = archetype.passive_nodes();

    const ROOT_W: f64 = 220.0;
    const SKILL_W: f64 = 140.0;
    const SPEC_W: f64 = 112.0;
    const MOD_W: f64 = 80.0;
    const ROOT_H: f64 = 96.0;
    const SKILL_H: f64 = 74.0;
    const SPEC_H: f64 = 80.0;
    const MOD_H: f64 = 62.0;
    const ROW_ROOT_Y: f64 = 58.0;
    const ROW_SKILL_Y: f64 = 178.0;
    const ROW_SPEC_Y: f64 = 296.0;
    const ROW_MOD_Y: f64 = 402.0;
    const GAP_X: f64 = 14.0;
    const MARGIN: f64 = 30.0;

    let skills: Vec<PassiveNode> = nodes.iter().copied().filter(|n| matches!(n.tier, PassiveTier::Skill)).collect();
    let mut cursor_x = MARGIN;
    let mut positioned: Vec<PosNode> = Vec::new();
    let mut svg_lines = String::new();
    let mut line = |x1: f64, y1: f64, x2: f64, y2: f64, color: &str| {
        svg_lines.push_str(&format!("<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{color}\" stroke-width=\"2\"></line>"));
    };

    for skill in &skills {
        let skill_rank = rank_of(skill.key);
        let specs: Vec<PassiveNode> =
            nodes.iter().copied().filter(|n| matches!(n.tier, PassiveTier::Specialization) && n.parent == Some(skill.key)).collect();
        let mut spec_xs: Vec<f64> = Vec::new();
        for spec in &specs {
            // A live bug report: "if you max 2 2nd tier talents, the 3rd
            // tiers stack on each other" - each spec's own Modifier
            // children are centered under ITS x (see the mods loop
            // below), but this cursor advance used to only reserve
            // `SPEC_W` per spec regardless of how wide that spec's own
            // mod row actually is - a spec with 2+ modifier children is
            // routinely WIDER than the SPEC_W+GAP_X gap between two
            // adjacent specs, so once a spec was unlocked its mod row
            // could visually overlap a neighbor. Only reserving the wider
            // footprint for a spec that's ACTUALLY unlocked right now
            // (not every spec unconditionally, worst-case) - a first
            // attempt at this reserved extra room for every spec
            // regardless of unlock state, which kept the whole tree
            // needlessly wide (and scrolling horizontally) for anyone who
            // hasn't unlocked much of anything, which was worse than the
            // bug it fixed. This still fully fixes the reported overlap
            // (the exact case that broke: 2 neighboring specs unlocked at
            // once) - the only remaining trade-off is a spec's siblings
            // can shift slightly the moment IT unlocks, rather than
            // everything being permanently pre-spaced.
            let currently_unlocked = rank_of(spec.key) >= spec.unlock_at.unwrap_or(4);
            let mod_span = if currently_unlocked {
                let mods_count =
                    nodes.iter().filter(|n| matches!(n.tier, PassiveTier::Modifier) && n.parent == Some(spec.key)).count() as f64;
                if mods_count > 0.0 { mods_count * MOD_W + (mods_count - 1.0) * GAP_X } else { 0.0 }
            } else {
                0.0
            };
            let reserved_w = mod_span.max(SPEC_W);
            let x = cursor_x + reserved_w / 2.0;
            cursor_x += reserved_w + GAP_X;
            spec_xs.push(x);
            positioned.push(PosNode { node: *spec, x, y: ROW_SPEC_Y, w: SPEC_W, h: SPEC_H, locked: skill_rank == 0 });
        }
        let skill_x = if spec_xs.is_empty() { cursor_x } else { spec_xs.iter().sum::<f64>() / spec_xs.len() as f64 };
        positioned.push(PosNode { node: *skill, x: skill_x, y: ROW_SKILL_Y, w: SKILL_W, h: SKILL_H, locked: false });

        if !spec_xs.is_empty() {
            let mid_y = (ROW_SKILL_Y + SKILL_H / 2.0 + ROW_SPEC_Y - SPEC_H / 2.0) / 2.0;
            line(skill_x, ROW_SKILL_Y + SKILL_H / 2.0, skill_x, mid_y, "#7a6ba8");
            if spec_xs.len() > 1 {
                line(spec_xs[0], mid_y, spec_xs[spec_xs.len() - 1], mid_y, "#7a6ba8");
            }
            for &sx in &spec_xs {
                line(sx, mid_y, sx, ROW_SPEC_Y - SPEC_H / 2.0, "#7a6ba8");
            }
        }

        for (spec, &spec_x) in specs.iter().zip(spec_xs.iter()) {
            let unlock_at = spec.unlock_at.unwrap_or(4);
            if rank_of(spec.key) < unlock_at {
                continue;
            }
            let mods: Vec<PassiveNode> =
                nodes.iter().copied().filter(|n| matches!(n.tier, PassiveTier::Modifier) && n.parent == Some(spec.key)).collect();
            if mods.is_empty() {
                continue;
            }
            let span = mods.len() as f64 * MOD_W + (mods.len() as f64 - 1.0) * GAP_X;
            let start_x = spec_x - span / 2.0 + MOD_W / 2.0;
            let mut mod_xs: Vec<f64> = Vec::new();
            for (i, m) in mods.iter().enumerate() {
                let mx = start_x + i as f64 * (MOD_W + GAP_X);
                mod_xs.push(mx);
                positioned.push(PosNode { node: *m, x: mx, y: ROW_MOD_Y, w: MOD_W, h: MOD_H, locked: false });
            }
            let mid_y = (ROW_SPEC_Y + SPEC_H / 2.0 + ROW_MOD_Y - MOD_H / 2.0) / 2.0;
            line(spec_x, ROW_SPEC_Y + SPEC_H / 2.0, spec_x, mid_y, "#7a6ba8");
            if mod_xs.len() > 1 {
                line(mod_xs[0], mid_y, mod_xs[mod_xs.len() - 1], mid_y, "#7a6ba8");
            }
            for &mx in &mod_xs {
                line(mx, mid_y, mx, ROW_MOD_Y - MOD_H / 2.0, "#7a6ba8");
            }
        }
    }

    // Root box (Class Passive - always active) + its own connector down
    // to the skill row - present in the artifact, was missing here.
    let root_x = if skills.is_empty() {
        MARGIN + ROOT_W / 2.0
    } else {
        positioned.iter().filter(|p| matches!(p.node.tier, PassiveTier::Skill)).map(|p| p.x).sum::<f64>() / skills.len() as f64
    };
    {
        let mid_y = (ROW_ROOT_Y + ROOT_H / 2.0 + ROW_SKILL_Y - SKILL_H / 2.0) / 2.0;
        let skill_xs: Vec<f64> = positioned.iter().filter(|p| matches!(p.node.tier, PassiveTier::Skill)).map(|p| p.x).collect();
        line(root_x, ROW_ROOT_Y + ROOT_H / 2.0, root_x, mid_y, "#b3495a");
        if skill_xs.len() > 1 {
            line(skill_xs[0], mid_y, skill_xs[skill_xs.len() - 1], mid_y, "#b3495a");
        }
        for &sx in &skill_xs {
            line(sx, mid_y, sx, ROW_SKILL_Y - SKILL_H / 2.0, "#b3495a");
        }
    }

    let stage_w = positioned.iter().map(|p| p.x + p.w / 2.0).fold(root_x + ROOT_W / 2.0, f64::max) + MARGIN;
    let stage_h = positioned.iter().map(|p| p.y + p.h / 2.0).fold(0.0_f64, f64::max) + MARGIN;

    PassiveLayout { positioned, root_x, svg_lines, stage_w, stage_h }
}

/// The root "class passive" box - same position constants as
/// `compute_passive_layout`'s root row (kept in sync by construction: both
/// only ever place it at `root_x` from that same layout call). Shared
/// between the interactive and read-only passive tree renders since
/// neither one ever makes the root box clickable. Takes `Archetype`
/// directly, same reasoning as `compute_passive_layout`.
fn root_node_html(archetype: Archetype, root_x: f64, root_desc: &str) -> String {
    const ROOT_W: f64 = 220.0;
    const ROW_ROOT_Y: f64 = 58.0;
    format!(
        "<div class=\"node node-root\" style=\"left:{root_x}px;top:{ROW_ROOT_Y}px;width:{ROOT_W}px;\" data-tip=\"{root_desc}\">\
          <div class=\"node-kind\">Class Passive &middot; Always Active</div>\
          <div class=\"node-name\">{archetype_upper}</div>\
          <div class=\"node-desc\">{root_desc}</div>\
        </div>",
        archetype_upper = format!("{:?}", archetype).to_uppercase(),
    )
}

/// One tree's full node-graph markup (SVG connectors + root box + every
/// node box, sized/positioned by `compute_passive_layout`) - shared by
/// the interactive `/passives` page's primary AND secondary tree, and by
/// the read-only `/characters/{login}/passives` view's primary AND
/// secondary tree (four call sites, one function). `interactive` picks
/// between a real allocate form per node (own +/- buttons, POSTing
/// `secondary` alongside `node_key` so the server knows which tree/map
/// the click targets - see `PassiveAllocateForm`) and a plain read-only
/// rank display; `available` is only consulted when `interactive` is
/// true (gates the + button) and is the character's WHOLE remaining
/// budget across both trees combined, not a per-tree allowance - there's
/// one shared pool, not two.
fn render_ptree_body(archetype: Archetype, level: u32, allocations: &HashMap<String, u32>, interactive: bool, secondary: bool, available: u32) -> String {
    let rank_of = |key: &str| allocations.get(key).copied().unwrap_or(0);
    let layout = compute_passive_layout(archetype, allocations);
    let PassiveLayout { positioned, root_x, svg_lines, stage_w, stage_h } = layout;

    let node_html = |p: &PosNode| -> String {
        let n = &p.node;
        let rank = rank_of(n.key);
        let not_yet = matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented);
        let tip = if not_yet {
            format!("{} Not yet active - allocating still banks the point for when this mechanic ships.", escape_html(n.description))
        } else {
            escape_html(n.description)
        };
        let (kind_class, kind_label) = match n.tier {
            PassiveTier::Skill => ("node-skill", "Tier 1"),
            PassiveTier::Specialization => ("node-spec", "Specialization"),
            PassiveTier::Modifier => ("node-mod", ""),
        };
        let state_class = if p.locked {
            " node--locked"
        } else if n.max_rank == 4 && rank == 4 {
            " node--specialized"
        } else if rank == n.max_rank {
            " node--maxed"
        } else if rank > 0 {
            " node--invested"
        } else {
            ""
        };
        let dots: String = (0..n.max_rank)
            .map(|i| {
                let filled = i < rank;
                let gold = filled && n.max_rank == 4 && i == 3;
                format!("<span class=\"dot{}{}\"></span>", if filled { " filled" } else { "" }, if gold { " dot-spec" } else { "" })
            })
            .collect();
        let kind_label_html = if kind_label.is_empty() { String::new() } else { format!("<div class=\"node-kind\">{kind_label}</div>") };
        let controls = if interactive {
            let can_add = !p.locked && rank < n.max_rank && available > 0;
            let can_remove = !p.locked && rank > 0;
            format!(
                "<form method=\"post\" action=\"/passives/allocate\" class=\"node-buttons\">\
                  <input type=\"hidden\" name=\"node_key\" value=\"{key}\">\
                  <input type=\"hidden\" name=\"secondary\" value=\"{secondary}\">\
                  <button class=\"btn-sm\" type=\"submit\" name=\"delta\" value=\"-1\"{remove_disabled}>-</button>\
                  <span class=\"node-rank\">{rank}/{max_rank}</span>\
                  <button class=\"btn-sm\" type=\"submit\" name=\"delta\" value=\"1\"{add_disabled}>+</button>\
                </form>",
                key = n.key,
                remove_disabled = if can_remove { "" } else { " disabled" },
                max_rank = n.max_rank,
                add_disabled = if can_add { "" } else { " disabled" },
            )
        } else {
            format!("<div class=\"node-buttons\"><span class=\"node-rank\">{rank}/{max_rank}</span></div>", max_rank = n.max_rank)
        };
        format!(
            "<div class=\"node {kind_class}{state_class}\" style=\"left:{x}px;top:{y}px;width:{w}px;\" data-tip=\"{tip}\">\
              {kind_label_html}\
              <div class=\"node-name\">{name}{flag}</div>\
              <div class=\"dots\">{dots}</div>\
              {controls}\
            </div>",
            x = p.x,
            y = p.y,
            w = p.w,
            name = escape_html(n.name),
            flag = if not_yet { " <span class=\"muted\">(inactive)</span>" } else { "" },
        )
    };

    let root_desc = escape_html(&archetype.description(level));
    let root_html = root_node_html(archetype, root_x, &root_desc);
    let nodes_html: String = positioned.iter().map(node_html).collect();

    format!(
        "<div class=\"tree-wrap\"><div style=\"width:{stage_w}px;height:{stage_h}px;position:relative;\">\
          <svg class=\"connectors\" width=\"{stage_w}\" height=\"{stage_h}\">{svg_lines}</svg>\
          {root_html}{nodes_html}\
        </div></div>",
    )
}

/// The shared "Layer 1-4 / legend" strip shown above every tree's own
/// node-graph - identical text regardless of which tree or which
/// interactive/read-only mode, factored out once both `/passives` and
/// `/characters/{login}/passives` needed to repeat it per tree (primary
/// + Split Personality's secondary) rather than just once per page.
fn render_ptree_meta_legend() -> String {
    "<div class=\"tree-meta\">\
      <span class=\"tier-count\">Layer 1 &middot; class passive</span>\
      <span class=\"tier-count\">Layer 2 &middot; 3 skills</span>\
      <span class=\"tier-count\">Layer 3 &middot; 9 specializations</span>\
      <span class=\"tier-count\">Layer 4 &middot; 3 modifiers, per specialization pushed to 4/4</span>\
    </div>\
    <div class=\"legend\">\
      <div class=\"legend-item\"><span class=\"legend-dots\"><span class=\"legend-dot on\"></span><span class=\"legend-dot on\"></span><span class=\"legend-dot on\"></span></span> Maxed (3/3)</div>\
      <div class=\"legend-item\"><span class=\"legend-dots\"><span class=\"legend-dot on\"></span><span class=\"legend-dot\"></span><span class=\"legend-dot\"></span></span> Partially invested</div>\
      <div class=\"legend-item\"><span class=\"legend-dots\"><span class=\"legend-dot\"></span><span class=\"legend-dot\"></span><span class=\"legend-dot\"></span></span> Not invested</div>\
      <div class=\"legend-item\"><span class=\"legend-dots\"><span class=\"legend-dot on\"></span><span class=\"legend-dot on\"></span><span class=\"legend-dot on\"></span><span class=\"legend-dot spec\"></span></span> 4th (gold) point &mdash; specializes, reveals 3 modifiers below it</div>\
    </div>"
        .to_string()
}

fn render_passive_tree_page(display_name: &str, character: Option<&Character>, preview: Option<&PassivePreview>) -> String {
    let name = escape_html(display_name);
    let nav = top_nav(character);
    let Some(c) = character else {
        return format!("{nav}<div class=\"card\"><h1>Welcome, {name}!</h1><p>You haven't joined the adventure yet.</p></div>");
    };
    if c.archetype == Archetype::Commoner {
        return format!(
            "{nav}<div class=\"card\"><h2>Passives</h2><p class=\"muted\">Pick an Archetype on your <a href=\"/\">dashboard</a> first - Commoner has no passive tree.</p></div>"
        );
    }

    let saved_primary = &c.passive_allocations;
    let saved_secondary = &c.secondary_passive_allocations;
    let dirty = preview.is_some_and(|p| p.primary != *saved_primary || p.secondary != *saved_secondary);
    let primary_allocations: &HashMap<String, u32> = preview.map(|p| &p.primary).unwrap_or(saved_primary);
    let secondary_allocations: &HashMap<String, u32> = preview.map(|p| &p.secondary).unwrap_or(saved_secondary);

    let total_points = c.total_passive_points();
    let spent: u32 =
        primary_allocations.values().sum::<u32>() + if c.effective_secondary_archetype().is_some() { secondary_allocations.values().sum() } else { 0 };
    let available = total_points.saturating_sub(spent);

    let (archetype_icon, archetype_role) = passive_archetype_icon_role(c.archetype);
    let primary_meta_legend = render_ptree_meta_legend();
    let primary_body = render_ptree_body(c.archetype, c.level, primary_allocations, true, false, available);

    let (respec_label, respec_disabled) = if c.free_passive_respecs > 0 {
        ("Respec (Free)".to_string(), "")
    } else {
        (format!("Respec ({PASSIVE_RESPEC_COST}d)"), if c.dust < PASSIVE_RESPEC_COST { " disabled" } else { "" })
    };
    let free_respec_note = if c.free_passive_respecs > 0 {
        format!("<p class=\"muted\">You have {} free respec{} banked.</p>", c.free_passive_respecs, if c.free_passive_respecs == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    let preview_note = if dirty { "<p class=\"preview-note dirty\">Unsaved changes.</p>" } else { "<p class=\"preview-note\">No unsaved changes.</p>" };

    // Split Personality's 2nd-class section (2026-08-17) - the picker
    // stays available the whole time the unique is equipped (so it can be
    // changed freely, per the live decision that this is always free),
    // pre-selected to whatever's currently active; the tree itself only
    // renders once a choice is actually active. Entirely absent when the
    // unique isn't equipped at all - same "hidden, not just disabled"
    // reasoning as the crafting card's own unique-only buttons.
    let secondary_section = if c.effective_split_personality_item().is_some() {
        let current_secondary = c.effective_secondary_archetype();
        let options: String = ALL_ARCHETYPES
            .iter()
            .filter(|&&a| a != c.archetype)
            .map(|&a| {
                let selected = if Some(a) == current_secondary { " selected" } else { "" };
                format!("<option value=\"{value}\"{selected}>{a:?}</option>", value = format!("{a:?}").to_lowercase())
            })
            .collect();
        let button_label = if current_secondary.is_some() { "Change" } else { "Choose" };
        let picker = format!(
            "<form method=\"post\" action=\"/passives/set-secondary\" class=\"secondary-picker\">\
              <label for=\"secondary-archetype-select\">2nd Class (Split Personality)</label>\
              <select id=\"secondary-archetype-select\" name=\"archetype\">{options}</select>\
              <button class=\"btn-sm\" type=\"submit\">{button_label}</button>\
            </form>",
        );
        let tree_section = if let Some(secondary_archetype) = current_secondary {
            let (sec_icon, sec_role) = passive_archetype_icon_role(secondary_archetype);
            let meta_legend = render_ptree_meta_legend();
            let body = render_ptree_body(secondary_archetype, c.level, secondary_allocations, true, true, available);
            format!(
                "<div class=\"current-row\">\
                  <div class=\"class-strip\"><span class=\"icon\">{sec_icon}</span><h2>{secondary_archetype:?}</h2><span class=\"role-badge\">{sec_role}</span></div>\
                </div>\
                {meta_legend}{body}"
            )
        } else {
            String::new()
        };
        format!(
            "<div class=\"ptree-secondary\">\
              <div class=\"masthead\">\
                <h1>2nd Class</h1>\
                <p class=\"subhead\">Split Personality lets you invest your same shared points into a 2nd class's tree too, spent from the SAME pool above - shown here below your primary tree.</p>\
              </div>\
              {picker}\
              {tree_section}\
            </div>",
        )
    } else {
        String::new()
    };

    format!(
        "{nav}\
        <div class=\"ptree-page\">\
          <div class=\"masthead\">\
            <div class=\"eyebrow\">Live &middot; {archetype:?}</div>\
            <h1>Passives</h1>\
            <p class=\"subhead\">1 point from the start, +1 every 5 levels. Points spent on a node marked (inactive) are banked, not wasted - they'll activate once that mechanic ships.</p>\
          </div>\
          <div class=\"current-row\">\
            <div class=\"class-strip\">\
              <span class=\"icon\">{archetype_icon}</span>\
              <h2>{archetype:?}</h2>\
              <span class=\"role-badge\">{archetype_role}</span>\
            </div>\
            <div class=\"side-chips\">\
              <div class=\"points-chip\">{archetype_icon} <span>Skill Points</span> &middot; <strong>{available}/{total_points} unspent</strong></div>\
              <div class=\"preview-row\">\
                <form method=\"post\" action=\"/passives/save\"><button class=\"btn-save\" type=\"submit\"{save_disabled}>Save Changes</button></form>\
                <form method=\"post\" action=\"/passives/reset\"><button class=\"btn-respec\" type=\"submit\"{reset_disabled}>Reset Preview</button></form>\
              </div>\
              {preview_note}\
              <form method=\"post\" action=\"/passives/respec\"><button class=\"btn-respec\" type=\"submit\"{respec_disabled}>{respec_label}</button></form>\
              <p class=\"points-formula\">1 point from the start, +1 every 5 levels &mdash; level {level} gives you {total_points} to spend.</p>\
              {free_respec_note}\
            </div>\
          </div>\
          {primary_meta_legend}\
          {primary_body}\
          {secondary_section}\
          <footer>\
            Root passive numbers mirror <code>Archetype::bonus()</code>'s real formula for {archetype:?} (hover \
            the root node for the exact math). Clicking a node's buttons only edits your PREVIEW - nothing is \
            spent until Save Changes, and Reset Preview reverts to your last save for free.\
          </footer>\
        </div>",
        archetype = c.archetype,
        level = c.level,
        save_disabled = if dirty { "" } else { " disabled" },
        reset_disabled = if dirty { "" } else { " disabled" },
    )
}

/// `/characters/{login}/passives` - read-only view of another player's
/// passive tree (see `character_passives_readonly`). Same layout as the
/// owner's own interactive `/passives` page (`render_passive_tree_page`,
/// shares its `compute_passive_layout`/`root_node_html`), but nodes are
/// plain rank displays instead of allocate forms, and there's no
/// save/reset/respec strip - viewing someone else's build is not the
/// same "session" as editing your own passive PREVIEW (see
/// `AdventureManager::preview_allocate_passive`'s doc), so there is
/// nothing here to preview or save.
fn render_passive_tree_readonly(login: &str, c: &Character, viewer: Option<&Character>) -> String {
    let name = escape_html(&c.display_name);
    let nav = top_nav(viewer);
    let back_link = format!("<p class=\"muted\"><a href=\"/characters/{login}\">&larr; Back to {name}</a></p>");
    if c.archetype == Archetype::Commoner {
        return format!("{nav}<div class=\"card\">{back_link}<h2>Passives</h2><p class=\"muted\">{name} hasn't picked an Archetype yet - Commoner has no passive tree.</p></div>");
    }

    let allocations = &c.passive_allocations;
    let total_points = c.total_passive_points();
    let spent: u32 =
        allocations.values().sum::<u32>() + if c.effective_secondary_archetype().is_some() { c.secondary_passive_allocations.values().sum() } else { 0 };

    let (archetype_icon, archetype_role) = passive_archetype_icon_role(c.archetype);
    let meta_legend = render_ptree_meta_legend();
    let primary_body = render_ptree_body(c.archetype, c.level, allocations, false, false, 0);

    // Split Personality's 2nd tree, read-only - same "only if there's
    // actually one active" gate as the interactive page, just with no
    // picker form (viewing someone else's build isn't your session to
    // edit - see this function's own doc).
    let secondary_section = if let Some(secondary_archetype) = c.effective_secondary_archetype() {
        let (sec_icon, sec_role) = passive_archetype_icon_role(secondary_archetype);
        let sec_meta_legend = render_ptree_meta_legend();
        let sec_body = render_ptree_body(secondary_archetype, c.level, &c.secondary_passive_allocations, false, true, 0);
        format!(
            "<div class=\"ptree-secondary\">\
              <div class=\"masthead\"><h1>2nd Class</h1><p class=\"subhead\">{name} has Split Personality equipped, investing the same shared points into a 2nd class's tree too.</p></div>\
              <div class=\"current-row\">\
                <div class=\"class-strip\"><span class=\"icon\">{sec_icon}</span><h2>{secondary_archetype:?}</h2><span class=\"role-badge\">{sec_role}</span></div>\
              </div>\
              {sec_meta_legend}{sec_body}\
            </div>",
        )
    } else {
        String::new()
    };

    format!(
        "{nav}\
        <div class=\"ptree-page\">\
          {back_link}\
          <div class=\"masthead\">\
            <div class=\"eyebrow\">{archetype:?} &middot; Level {level}</div>\
            <h1>{name}'s Passives</h1>\
            <p class=\"subhead\">Read-only - this is {name}'s saved build. Points spent on a node marked (inactive) are banked, not wasted - they'll activate once that mechanic ships.</p>\
          </div>\
          <div class=\"current-row\">\
            <div class=\"class-strip\">\
              <span class=\"icon\">{archetype_icon}</span>\
              <h2>{archetype:?}</h2>\
              <span class=\"role-badge\">{archetype_role}</span>\
            </div>\
            <div class=\"side-chips\">\
              <div class=\"points-chip\">{archetype_icon} <span>Skill Points</span> &middot; <strong>{spent}/{total_points} spent</strong></div>\
            </div>\
          </div>\
          {meta_legend}\
          {primary_body}\
          {secondary_section}\
          <footer>Root passive numbers mirror <code>Archetype::bonus()</code>'s real formula for {archetype:?} (hover the root node for the exact math).</footer>\
        </div>",
        archetype = c.archetype,
        level = c.level,
    )
}

/// Character model/sprite picker - a thumbnail grid (radio input per
/// `ALL_SPRITES` entry, styled as a clickable card via `.model-option`/
/// the page's `<script>`, since a plain `<select>` couldn't show a
/// preview image per option) rather than the archetype picker's plain
/// dropdown. Free while `c.model` is still `None` (never explicitly
/// chosen); `MODEL_CHANGE_COST` dust every time after - same
/// free-once-then-paid shape as `render_archetype_picker`. Submitting
/// without changing the selection is harmless (server-side `change_model`
/// still charges for it if not free, same as re-picking the same
/// archetype would) - the picker doesn't try to detect/block a no-op pick.
fn render_model_picker(c: &Character, login: &str) -> String {
    let current_sprite = c.effective_sprite(login);
    let current_line = if MODEL_CHANGES_FREE_FOR_ALL {
        // See MODEL_CHANGES_FREE_FOR_ALL's doc - no token/dust accounting
        // to report while this is on, so just say what it is.
        "<p class=\"muted\">Sprite changes are free for everyone right now while we settle on the new set — try as many as you like!</p>".to_string()
    } else {
        match (c.model.is_none(), c.free_model_changes) {
            (true, _) => "<p class=\"muted\">Never picked one — you're on a random default. Pick one below, free!</p>".to_string(),
            (false, 0) => format!("<p class=\"muted\">Current model: {}</p>", escape_html(&sprite_label(&current_sprite))),
            (false, n) => format!(
                "<p class=\"muted\">Current model: {} — you have {n} free change{} banked!</p>",
                escape_html(&sprite_label(&current_sprite)),
                if n == 1 { "" } else { "s" }
            ),
        }
    };
    let options: String = ALL_SPRITES
        .iter()
        .map(|&sprite| {
            let checked = if sprite == current_sprite { " checked" } else { "" };
            format!(
                "<label class=\"model-option\">\
                  <input type=\"radio\" name=\"model\" value=\"{sprite}\"{checked}>\
                  <span class=\"model-thumb\"><img src=\"/sprites/{sprite}.png\" alt=\"{label}\"></span>\
                </label>",
                label = escape_html(&sprite_label(sprite)),
            )
        })
        .collect();
    // Self-service custom drop-in sprites (see `CUSTOM_SPRITE_DIR`'s doc,
    // 2026-08-16) - scanned live off disk on every page render, so a PNG
    // dropped into the folder shows up here immediately, no code change/
    // recompile/restart needed. Only rendered as its own section when
    // there's actually something there, so an empty/missing folder
    // doesn't leave a bare empty heading on the page.
    let custom_names = custom_sprite_names(login);
    let custom_section = if custom_names.is_empty() {
        String::new()
    } else {
        let custom_options: String = custom_names
            .iter()
            .map(|name| {
                let value = format!("custom/{name}");
                let checked = if value == current_sprite { " checked" } else { "" };
                format!(
                    "<label class=\"model-option\">\
                      <input type=\"radio\" name=\"model\" value=\"{value}\"{checked}>\
                      <span class=\"model-thumb\"><img src=\"/sprites/{value}.png\" onerror=\"this.onerror=null;this.src='/sprites/{value}.gif'\" alt=\"{label}\"></span>\
                    </label>",
                    label = escape_html(name),
                )
            })
            .collect();
        format!("<h3 class=\"model-custom-heading\">Custom Sprites</h3><div class=\"model-grid\">{custom_options}</div>")
    };
    let free = MODEL_CHANGES_FREE_FOR_ALL || c.free_model_changes > 0;
    let (button_label, disabled) =
        if free { ("Choose (Free)".to_string(), "") } else { (format!("Change ({MODEL_CHANGE_COST} dust)"), if c.dust < MODEL_CHANGE_COST { " disabled" } else { "" }) };
    format!(
        "<div class=\"card\"><h2>Character Model</h2>\
          {current_line}\
          <details class=\"model-picker-details\">\
            <summary>Change sprite</summary>\
            <form method=\"post\" action=\"/change-model\">\
              <div class=\"model-grid\">{options}</div>\
              {custom_section}\
              <button class=\"btn\" type=\"submit\"{disabled}>{button_label}</button>\
            </form>\
          </details>\
        </div>"
    )
}

/// See `render_model_picker`/`CUSTOM_SPRITE_DIR`'s doc - every `.png` or
/// `.gif` in the self-service drop-in folder THIS `viewer_login` is
/// actually allowed to pick (2026-08-16: name-gated per
/// `custom_sprite_is_owned_by`'s doc - either the file's named after
/// them (optionally with a numbered suffix, "kibukah"/"kibukah2"/...),
/// or it's in the reserved public pool), sorted,
/// deduplicated, base filename only (no extension - the overlay itself
/// figures out which extension actually loads, see overlay.html's
/// `getOrLoadSprite`, so the picker doesn't need to care or distinguish
/// them here). Empty (not an error) if the folder doesn't exist yet or
/// nothing's been dropped in - same "no crash, just nothing to show"
/// tradeoff a fresh install already has for `commands.json`/etc. via
/// `crate::state::load_json`.
fn custom_sprite_names(viewer_login: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(crate::adventure::CUSTOM_SPRITE_DIR) else { return Vec::new() };
    let viewer_id = viewer_login.to_lowercase();
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let is_image = path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("gif"));
            if !is_image {
                return None;
            }
            let name = path.file_stem().and_then(|s| s.to_str())?.to_string();
            crate::adventure::custom_sprite_is_owned_by(&viewer_id, &name).then_some(name)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// "Wings of Flight" cosmetic MTX card - a purchase button (or, once
/// `owns_wings`, a flying on/off toggle instead - see
/// `AdventureManager::purchase_wings`/`toggle_flying`). Purely cosmetic,
/// no combat effect; see `CharacterView::flying` for how the overlay
/// actually renders it.
fn render_wings_card(c: &Character) -> String {
    if c.owns_wings {
        let (label, active_class) = if c.flying { ("Flying: ON (click to land)", " wings-active") } else { ("Flying: OFF (click to take off)", "") };
        format!(
            "<div class=\"card\"><h2>\u{1f54a}\u{fe0f} Wings of Flight</h2>\
              <p class=\"muted\">You own this rare cosmetic — toggle it any time, no cost. While flying, you hover above the crowd instead of walking/jumping.</p>\
              <form method=\"post\" action=\"/toggle-flying\">\
                <button class=\"btn{active_class}\" type=\"submit\">{label}</button>\
              </form>\
            </div>"
        )
    } else {
        let disabled = if c.dust < WINGS_COST { " disabled" } else { "" };
        format!(
            "<div class=\"card\"><h2>\u{1f54a}\u{fe0f} Wings of Flight</h2>\
              <p class=\"muted\">An extremely rare cosmetic — hover above the crowd instead of walking/jumping. Purchase outright, or hope for the ~0.01% chance it drops alongside any item you earn.</p>\
              <form method=\"post\" action=\"/purchase-wings\">\
                <button class=\"btn\" type=\"submit\"{disabled}>Purchase ({WINGS_COST} dust)</button>\
              </form>\
            </div>"
        )
    }
}

/// Every item the character has anywhere, equipped slots first (in a
/// stable order) then the bag - what the unified Crafting card's item
/// pickers draw from, now that one dropdown covers every slot at once
/// (Recombine is the only action that cares about slot match, and it
/// validates that server-side same as always).
/// Prompt card for naming a Krangled item - shows once per item that's
/// `locked` (see `Item::locked`) and has never been asked yet (`nickname`
/// is `None`, not just empty - see `Item::nickname`'s doc). Only ever
/// shows ONE such item at a time (the first found, equipped slots
/// first) even if several are pending, so a Krangle-happy player isn't
/// stacked with a wall of prompts at once - the rest just wait their
/// turn on the next page load after this one's answered. `String::new()`
/// (no card) once nothing's pending.
fn render_nickname_prompt(c: &Character) -> String {
    let Some(item) = all_items(c).into_iter().find(|i| i.locked && i.nickname.is_none()) else {
        return String::new();
    };
    format!(
        "<div class=\"card\"><h2>Name Your Krangled Item</h2>\
          <p class=\"muted\">You Krangled a {name} — give it a custom name if you'd like! It'll show as {name} \"Your Name\" everywhere. Leave it blank to skip (you won't be asked again for this item).</p>\
          <form method=\"post\" action=\"/name-item\">\
            <input type=\"hidden\" name=\"item_id\" value=\"{id}\">\
            <input type=\"text\" name=\"nickname\" maxlength=\"{NICKNAME_MAX_LEN}\" placeholder=\"e.g. Excalibur\">\
            <button class=\"btn\" type=\"submit\">Save</button>\
          </form>\
        </div>",
        name = escape_html(&item.name),
        id = item.id,
    )
}

fn all_items(c: &Character) -> Vec<&Item> {
    let mut items: Vec<&Item> = Vec::new();
    for slot in [EquipSlot::Weapon, EquipSlot::Helm, EquipSlot::Body, EquipSlot::Gloves, EquipSlot::Boots] {
        if let Some(item) = c.equipped(slot) {
            items.push(item);
        }
    }
    items.extend(c.inventory.iter());
    items
}

/// Options for one crafting-form `<select>` - every item the character
/// owns, labeled with name/slot/tier/modifier-count so otherwise-
/// identical-looking pieces stay distinguishable across every slot at
/// once. A locked (Krangled) item is still listed - so a player can see
/// it's still there - but marked 🔒 since every crafting action against
/// it will fail. `with_none` prepends a selected "None" option (empty
/// value) - used for the second (Recombine-only) picker, so it defaults
/// to not picking a second item rather than silently pre-selecting one
/// (see `RecombineError::SameItem`/`ItemNotFound`, either harmless if
/// submitted without touching it - Recombine just won't have found a
/// valid pair, and every other action ignores this field entirely).
/// `selected_id` pre-selects one option (see `Character::last_crafted_item_id`'s
/// doc) instead of leaving the browser's own "first option wins" default
/// in charge - so the picker keeps pointing at whatever item was just
/// worked on, across both the post-craft redirect AND a plain page
/// refresh, until a different craft (or a different item's craft)
/// changes it. `None` (either no last-crafted item yet, or that id no
/// longer exists among `items` - e.g. Scour'd... no, still exists;
/// really just consumed by a Recombine) falls back to the previous
/// behavior: `with_none`'s explicit "None" option if there is one,
/// otherwise whatever the browser defaults an unselected `<select>` to
/// (its first option).
/// One `<option>` for `craft_item_options` - `show_slot` controls whether
/// the slot name is folded into the visible text (needed in the
/// "Equipped" group, which mixes every slot together; redundant - and
/// omitted - inside a per-slot `<optgroup>`, where it's already implied by
/// the group's own label). Quality/Perfect/Sacred (`Item::quality_percent`/
/// `perfect`/`sacred_affix`, same `Quality% < Perfect < Sacred` precedence
/// `quality_line_html`'s own tag already uses) is always shown - the whole
/// point of this pass (a live request: "each item should list the quality
/// %/perfect/sacred/etc").
fn craft_item_option_html(item: &Item, show_slot: bool, selected_id: Option<&str>) -> String {
    let lock = if item.locked { " \u{1F512}" } else { "" };
    let unique_mark = if item.unique_affix.is_some() { " \u{2726}" } else { "" };
    let mods = item.affixes.len();
    let selected = if selected_id == Some(item.id.as_str()) { " selected" } else { "" };
    let quality = item.quality_percent();
    let quality_tag = if item.sacred_affix.is_some() {
        " Sacred".to_string()
    } else if item.perfect {
        " Perfect".to_string()
    } else {
        format!(" Q{quality:.0}%")
    };
    let slot_prefix = if show_slot { format!("{:?}, ", item.slot) } else { String::new() };
    format!(
        "<option value=\"{id}\" data-affixes=\"{mods}\" data-tier=\"{tier}\" data-quality=\"{quality:.0}\" data-perfect=\"{perfect}\" data-polish-room=\"{polish_room}\"{selected}>{name} ({slot_prefix}T{tier}, {mods} mod{plural}{quality_tag}){lock}{unique_mark}</option>",
        id = item.id,
        name = escape_html(&item.display_name()),
        tier = item.tier,
        mods = mods,
        plural = if mods == 1 { "" } else { "s" },
        perfect = if item.perfect { "1" } else { "0" },
        polish_room = if item.has_polish_room() { "1" } else { "0" },
    )
}

/// Options for one crafting-form `<select>`, grouped for scannability (a
/// live request - the flat unsorted list made otherwise-identical-looking
/// pieces hard to tell apart once a character owned more than a handful):
/// every currently-equipped item first under an "Equipped" `<optgroup>`
/// (order: Weapon, Helm, Body, Gloves, Boots), THEN every remaining item
/// grouped into its own per-slot `<optgroup>` in that same order. `c` is
/// only used to determine which of `items` are equipped right now (via
/// `Character::equipped`, matched by id) - `render_equip_picker` passes
/// bag-only candidates, so its "Equipped" group is always empty (and thus
/// omitted) and every item lands in its single (matching) slot group.
/// A locked (Krangled) item is still listed - so a player can see it's
/// still there - but marked 🔒 since every crafting action against it will
/// fail. `with_none` prepends a selected "None" option (empty value) -
/// used for the second (Recombine-only) picker, so it defaults to not
/// picking a second item rather than silently pre-selecting one (see
/// `RecombineError::SameItem`/`ItemNotFound`, either harmless if submitted
/// without touching it - Recombine just won't have found a valid pair, and
/// every other action ignores this field entirely). `selected_id`
/// pre-selects one option (see `Character::last_crafted_item_id`'s doc)
/// instead of leaving the browser's own "first option wins" default in
/// charge - so the picker keeps pointing at whatever item was just worked
/// on, across both the post-craft redirect AND a plain page refresh, until
/// a different craft (or a different item's craft) changes it. `None`
/// (either no last-crafted item yet, or that id no longer exists among
/// `items`) falls back to the previous behavior: `with_none`'s explicit
/// "None" option if there is one, otherwise whatever the browser defaults
/// an unselected `<select>` to (its first option).
fn craft_item_options(c: &Character, items: &[&Item], with_none: bool, selected_id: Option<&str>) -> String {
    let none_selected = with_none && selected_id.is_none();
    let none_option = if with_none { format!("<option value=\"\"{}>None</option>", if none_selected { " selected" } else { "" }) } else { String::new() };

    let is_equipped = |item: &Item| c.equipped(item.slot).as_ref().is_some_and(|e| e.id == item.id);

    let equipped: Vec<&Item> = items.iter().copied().filter(|i| is_equipped(i)).collect();
    let equipped_group = if equipped.is_empty() {
        String::new()
    } else {
        let opts: String = equipped.iter().map(|i| craft_item_option_html(i, true, selected_id)).collect();
        format!("<optgroup label=\"Equipped\">{opts}</optgroup>")
    };

    let slot_groups: String = [EquipSlot::Weapon, EquipSlot::Helm, EquipSlot::Body, EquipSlot::Gloves, EquipSlot::Boots]
        .into_iter()
        .map(|slot| {
            let group_items: Vec<&Item> = items.iter().copied().filter(|i| i.slot == slot && !is_equipped(i)).collect();
            if group_items.is_empty() {
                return String::new();
            }
            let opts: String = group_items.iter().map(|i| craft_item_option_html(i, false, selected_id)).collect();
            format!("<optgroup label=\"{slot:?}\">{opts}</optgroup>")
        })
        .collect();

    format!("{none_option}{equipped_group}{slot_groups}")
}

/// Unified Crafting card - one item picker (see `craft_item_options`)
/// feeding all 7 crafting actions (the 6 currencies + Recombine), each
/// its own submit button under one form (`POST /craft`, dispatching on
/// `action` server-side). No card at all if the character owns no items
/// yet (can't happen post-starter-kit, but a defensive empty check
/// costs nothing).
/// Detailed per-action explainer shown as a hover tooltip on that
/// action's button (see `render_crafting_card`) - replaces what used to
/// be one bulky paragraph covering every action at once. Recombine gets
/// the most detail since it's the most-confusing action by far (per a
/// live request after it caused real confusion).
fn craft_action_tip(action: CraftAction) -> &'static str {
    match action {
        CraftAction::Transmute => "Adds one random modifier to a bare item (0 modifiers only).",
        CraftAction::Scour => "Strips every modifier from an item, leaving just its base stat. Needs at least 1 modifier to do anything.",
        CraftAction::Augment => "Adds a second modifier to a 1-modifier item.",
        CraftAction::Regal => "Adds a third modifier to a 2-modifier item.",
        CraftAction::Exalt => "Adds a fourth modifier to a 3-modifier item.",
        CraftAction::Krangle => {
            "Adds one final modifier to ANY unlocked item, any modifier count, then permanently locks it \u{2014} no further crafting of any kind, but it stays equippable, repairable, and disenchantable."
        }
        CraftAction::Annulment => {
            "Removes one modifier from the item. Unveiled: a random one goes. Veiled: rolls up to 2 of the item's existing modifiers as candidates and you pick which one leaves."
        }
        CraftAction::Chancing => {
            "A real chance-orb reroll: every existing modifier gets a brand-new TYPE (not just a new value for its old one), each at a fresh roll range. Unveiled: all of them reroll at once. Veiled: walks them one at a time, showing 3 candidate replacements for each before you commit and move to the next. Works on a Reforge/Recombine crit-bonus modifier too \u{2014} that slot stays marked special under whatever new type it lands on."
        }
        CraftAction::CelestialShard => {
            "Consumes a Celestial Shard to grant a unique affix, shown above the item's tier \u{2014} outside the normal 4-modifier cap and unaffected by any other crafting. An item can only ever have one; a Krangled item can't receive one, and a unique item can't be Krangled. Needs an actual Celestial Shard \u{2014} can't be bought with dust."
        }
        CraftAction::UniqueShard => {
            "Consumes a Unique Shard to grant Split Personality, shown above the item's tier \u{2014} same rules as any other unique affix (one per item, mutually exclusive with Krangle). Lets you invest passive points into a 2nd class on /passives. Needs an actual Unique Shard \u{2014} can't be bought with dust."
        }
        CraftAction::Polishing => {
            "Costs sand, not dust \u{2014} 1 per 10% quality (12 for a Perfect item). Raises the item's own quality by 5% and bumps one random modifier's roll by 5% of its range, both capped at the max. On an already-Perfect item (nothing left to raise on quality), instead bumps up to 2 random modifiers' rolls by 5%."
        }
        CraftAction::Reforge => {
            "Rerolls this item to a new (usually higher) tier, same as the Reforge Gear channel points reward, but costs dust instead \u{2014} 30 per tier of the item \u{2014} and targets this specific item, with a small chance at a bonus modifier."
        }
    }
}

const RECOMBINE_TIP: &str = "Forges item A and item B (same slot) into one new item, consuming both. New tier = the two items' average tier, rounded down, +1. Each source's own modifiers each independently have a 50% chance to carry over (max 4 total on the result); any modifier TYPE both items already share is guaranteed to carry over instead, keeping whichever of the two rolled values is higher. The result's quality is a coin flip between the two source items' own quality rolls. Free by default. Checking Veil (+dust) guarantees EVERY modifier carries over (same 4 cap) and keeps the BETTER of the two quality rolls instead of a coin flip, for 500 dust per combined modifier on top of the veil surcharge.";

const VEIL_TIP: &str = "Turns this craft's randomness into a choice: pay extra dust up front and get 3 independently-rolled outcomes to pick from, instead of one outcome applied automatically. A banked free token always veils at no extra cost. Scour has nothing to pick between, so veiling it does nothing.";

const HIDEOUT_WARRIOR_TIP: &str = "Runs Transmute \u{2192} Augment \u{2192} Regal \u{2192} Exalt \u{2192} Krangle on this item in order, skipping any step that isn't eligible right now, paying each step's normal dust cost as it goes \u{2014} always in full, never a banked token. Stops early if you run out of dust; whatever already landed stays. Never veiled, regardless of the checkbox above. Reaching the Krangle step permanently locks the item, same as using Krangle directly \u{2014} uncheck \"Include Krangle\" to stop after Exalt and leave the item unlocked.";

fn render_crafting_card(c: &Character) -> String {
    let items = all_items(c);
    if items.is_empty() {
        return String::new();
    }
    let options_a = craft_item_options(c, &items, false, c.last_crafted_item_id.as_deref());
    let options_b = craft_item_options(c, &items, true, None);
    let action_btn = |action: CraftAction| {
        let tip = craft_action_tip(action);
        // A banked free token (see Character::craft_tokens) covers this
        // action entirely for free, shown as "Free — N tokens" instead
        // of the dust cost, never disabled by dust, and never carries the
        // data-base/data-veil-extra attributes the client-side cost-
        // preview script (see the <script> block below) looks for - a
        // free button's label never needs to change when Veil is
        // toggled, since a token always veils at no extra cost regardless
        // (see AdventureManager::craft_item).
        // Krangle (permanently locks the item) and Scour (strips every
        // modifier) are the two genuinely destructive/irreversible
        // actions here - flagged for the confirm-before-submit script
        // below (2026-08-15, a live request: "add a confirmation box for
        // krangle & scour showing the item you are asking to be
        // scoured/krangle"). Every other action either only adds
        // something or (Polishing/Reforge) has its own separate risk
        // profile the request didn't ask to gate the same way.
        let confirm_attr =
            if matches!(action, CraftAction::Krangle | CraftAction::Scour | CraftAction::Annulment | CraftAction::Chancing) { " data-confirm=\"1\"" } else { "" };
        let tokens = c.craft_token_count(action);
        if tokens > 0 {
            return format!(
                "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"{value}\" data-tip=\"{tip}\"{confirm_attr}>{label} (Free — {tokens} token{s})</button>",
                value = action.label().to_lowercase(),
                label = action.label(),
                s = if tokens == 1 { "" } else { "s" },
            );
        }
        let base = action.base_cost();
        let disabled = if c.dust < base { " disabled" } else { "" };
        // Scour has nothing to pick between when veiled (is_veilable() is
        // false for it) - omitting data-veil-extra is what tells the
        // preview script to leave its cost alone regardless of the
        // checkbox.
        let veil_attr = if action.is_veilable() { format!(" data-veil-extra=\"{VEIL_EXTRA_COST}\"") } else { String::new() };
        format!(
            "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"{value}\" data-base=\"{base}\" data-label=\"{label}\" data-tip=\"{tip}\"{veil_attr}{disabled}{confirm_attr}>{label} ({base}d)</button>",
            value = action.label().to_lowercase(),
            label = action.label(),
        )
    };
    // A basic (non-veiled) recombine is free for everyone now - a banked
    // token (Character::free_recombines) only still matters for making a
    // VEILED recombine free too (see AdventureManager::recombine_gear).
    // data-base is 0 both unveiled AND as the veiled floor - unlike the
    // currency actions, Recombine's veiled cost is VEIL_EXTRA_COST + the
    // pool surcharge and NOTHING else (see recombine_gear's cost calc -
    // RECOMBINE_DUST_COST doesn't factor in at all anymore, a real live
    // bug fixed there: a 3-modifier veiled recombine was charging 2500
    // instead of the intended 2000).
    let (recombine_cost_label, recombine_disabled, recombine_attrs) = if c.free_recombines > 0 {
        (
            format!("Free — {} token{}", c.free_recombines, if c.free_recombines == 1 { "" } else { "s" }),
            if items.len() < 2 { " disabled" } else { "" },
            String::new(),
        )
    } else {
        (
            "Free".to_string(),
            if items.len() < 2 { " disabled" } else { "" },
            format!(" data-base=\"0\" data-label=\"Recombine\" data-veil-extra=\"{VEIL_EXTRA_COST}\" data-recombine=\"1\""),
        )
    };
    let dust = format_number(c.dust as f64);
    let sand = format_number(c.sand as f64);
    // Hidden entirely (not just disabled) until the player actually has
    // one - unlike the 6 normal actions, showing a permanently-disabled
    // "Celestial Shard (18446744073709551615d)" button (its real
    // base_cost, deliberately unaffordable in dust - see that method's
    // doc) would just be confusing clutter for a resource nobody starts
    // with. It naturally reveals itself the moment one is actually earned.
    let celestial_btn =
        if c.craft_token_count(CraftAction::CelestialShard) > 0 { action_btn(CraftAction::CelestialShard) } else { String::new() };
    // Same hidden-until-earned shape as celestial_btn above, for the
    // Unique Shard token.
    let unique_shard_btn = if c.craft_token_count(CraftAction::UniqueShard) > 0 { action_btn(CraftAction::UniqueShard) } else { String::new() };
    // Polishing (sand) and Reforge (30*tier dust) both price off the
    // SELECTED item rather than a flat action-wide cost, so unlike
    // action_btn's other 6 buttons their price text is entirely
    // client-side (see updateCraftCosts' own data-polish/data-reforge
    // handling below) - unaffordable-on-load can't be precomputed
    // server-side without knowing which item the browser defaults to,
    // so data-sand/data-dust just carry the player's current balance for
    // the same script to compare against once it knows the real cost.
    let polish_tip = craft_action_tip(CraftAction::Polishing);
    let reforge_tip = craft_action_tip(CraftAction::Reforge);
    let polish_btn = format!(
        "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"polishing\" data-polish=\"1\" data-sand=\"{}\" data-tip=\"{polish_tip}\">Polishing</button>",
        c.sand,
    );
    let reforge_btn = format!(
        "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"reforge\" data-reforge=\"1\" data-dust=\"{}\" data-tip=\"{reforge_tip}\">Reforge</button>",
        c.dust,
    );
    format!(
        "<div class=\"card\" id=\"crafting-card\">\
          <div class=\"header-row\"><h2>Crafting</h2><span class=\"dust-available\">💰 {dust} dust · \u{1FAB5} {sand} sand</span></div>\
          <p class=\"muted\">Pick your item(s) below, then hover any button for exactly what it does.</p>\
          <form method=\"post\" action=\"/craft\">\
            <select name=\"item_a\">{options_a}</select>\
            <select name=\"item_b\">{options_b}</select>\
            <label class=\"veil-check\" data-tip=\"{VEIL_TIP}\"><input type=\"checkbox\" name=\"veiled\" value=\"1\"> Veil this craft (+{VEIL_EXTRA_COST} dust; Recombine also +500 per combined modifier)</label>\
            <div class=\"craft-actions\">\
              {transmute}{augment}{regal}{exalt}{krangle}{annulment}{chancing}\
            </div>\
            <div class=\"polish-reforge-actions\">\
              <span class=\"muted\">Polish / Reforge:</span>\
              <label class=\"batch-check\"><input type=\"checkbox\" data-times=\"5\"> x5</label>\
              <label class=\"batch-check\"><input type=\"checkbox\" data-times=\"10\"> x10</label>\
              <input type=\"hidden\" name=\"times\" value=\"1\">\
              {polish_btn}{reforge_btn}\
            </div>\
            <div class=\"craft-actions\">\
              {scour}{celestial_btn}{unique_shard_btn}\
              <button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"recombine\" data-tip=\"{RECOMBINE_TIP}\"{recombine_attrs}{recombine_disabled}>Recombine ({recombine_cost_label})</button>\
              <button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"hideout warrior\" data-confirm=\"1\" data-tip=\"{HIDEOUT_WARRIOR_TIP}\">Hideout Warrior</button>\
              <label class=\"veil-check\" data-tip=\"Leave checked to end on Krangle (permanently locks the item). Uncheck to stop after Exalt and leave it unlocked.\"><input type=\"checkbox\" name=\"hideout_krangle\" value=\"1\" checked> Include Krangle</label>\
            </div>\
          </form>\
        </div>",
        transmute = action_btn(CraftAction::Transmute),
        scour = action_btn(CraftAction::Scour),
        augment = action_btn(CraftAction::Augment),
        regal = action_btn(CraftAction::Regal),
        exalt = action_btn(CraftAction::Exalt),
        krangle = action_btn(CraftAction::Krangle),
        annulment = action_btn(CraftAction::Annulment),
        chancing = action_btn(CraftAction::Chancing),
    )
}

/// Replaces the Crafting card while `username` has a veiled craft
/// awaiting a choice (see `PendingVeil`/`AdventureManager::pending_veil`) -
/// each of the up-to-3 rolled candidates gets its own button
/// (`POST /craft/choose-veil` with a hidden `index`), labeled with
/// exactly what picking it would do. No item pickers shown in this
/// state - the source item(s) were already decided when the veiled
/// craft was started.
fn render_veil_choice_card(pending: &PendingVeil) -> String {
    let title = match &pending.action {
        PendingVeilAction::Currency { action, .. } => format!("{} — choose your outcome", action.label()),
        PendingVeilAction::Recombine { .. } => "Recombine — choose your outcome".to_string(),
    };
    let options: String = pending
        .candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let desc = match candidate {
                VeilCandidate::Currency(outcome) => match (outcome.affix_added, outcome.affix_value, outcome.affix_removed, outcome.affix_removed_value) {
                    // Chancing's replacement shape - both set at once (see
                    // `AdventureManager::chancing_step_candidates`).
                    (Some(new_affix), Some(new_value), Some(old_affix), _) => {
                        format!("{} → {}", affix_name(old_affix), affix_display(new_affix, new_value))
                    }
                    (Some(affix), Some(value), _, _) => {
                        let line = affix_display(affix, value);
                        if outcome.now_locked {
                            format!("{line} (locks the item)")
                        } else {
                            line
                        }
                    }
                    (_, _, Some(affix), Some(value)) => format!("Remove {}", affix_display(affix, value)),
                    _ => "No change".to_string(),
                },
                VeilCandidate::Recombine(roll) => {
                    let affixes =
                        if roll.affixes.is_empty() { "no modifiers".to_string() } else { roll.affixes.iter().map(|(a, v)| affix_display(*a, *v)).collect::<Vec<_>>().join(", ") };
                    let crit = if roll.bonus_affix.is_some() { " · CRIT!" } else { "" };
                    format!("Tier {} — {affixes}{crit}", roll.new_tier)
                }
            };
            format!(
                "<form method=\"post\" action=\"/craft/choose-veil\" class=\"veil-option\">\
                  <input type=\"hidden\" name=\"index\" value=\"{i}\">\
                  <button class=\"btn\" type=\"submit\">Option {n}: {desc}</button>\
                </form>",
                n = i + 1,
            )
        })
        .collect();
    // Almost always exactly 3 (every existing action) - but a veiled
    // Annulment rolls up to 2 (only 1 on a 1-modifier item, see
    // `AdventureManager::craft_item_ex`'s Annulment branch), so the intro
    // text can't hardcode "3" anymore.
    let n = pending.candidates.len();
    let is_chancing = matches!(&pending.action, PendingVeilAction::Currency { action, .. } if *action == CraftAction::Chancing);
    let intro = if is_chancing {
        // Chancing rerolls one affix slot at a time, reusing this SAME
        // panel per step (see `AdventureManager::choose_veil_outcome`'s
        // Chancing arm) - a progress note instead of the generic "rolled
        // up front" line, which wouldn't be true here (only THIS slot's
        // candidates are rolled; later slots aren't rolled until picked).
        let left = pending.chancing_remaining.len();
        if left > 0 {
            format!("{left} more modifier{} left to reroll after this one — whatever you pick here is committed immediately.", if left == 1 { "" } else { "s" })
        } else {
            "Last modifier to reroll this pass — whatever you pick here is committed immediately.".to_string()
        }
    } else {
        format!(
            "Veiled crafts roll {n} possible result{} up front{}",
            if n == 1 { "" } else { "s" },
            if n > 1 { " — pick the one you want, the rest are discarded." } else { "." }
        )
    };
    format!(
        "<div class=\"card\" id=\"veil-choice\"><h2>{title}</h2>\
          <p class=\"muted\">{intro}</p>\
          <div class=\"veil-options\">{options}</div>\
        </div>"
    )
}

fn durability_html(item: &Item) -> String {
    match item.durability_percent() {
        None => "<span class=\"indestructible\">Indestructible</span>".to_string(),
        Some(pct) => {
            let color_class = if pct > 50 { "good" } else if pct > 20 { "warn" } else { "critical" };
            let note = if pct == 0 { "<div class=\"needs-repair\">Needs repair — 0 bonus</div>" } else { "" };
            format!(
                "<div class=\"durability-bar\"><div class=\"durability-fill {color_class}\" style=\"width:{pct}%\"></div></div><span class=\"durability-pct\">{pct}%</span>{note}"
            )
        }
    }
}

/// The `class` attribute for an item's name line - dark red for a
/// Krangled (locked) item, gold/icy-blue for Sacred/Unique otherwise,
/// plain if none apply. Locked takes visual priority over EVERYTHING
/// else (2026-08-16 fix, a live request: "krangled items should turn
/// red even if they are sacred or perfect") - Krangle has no
/// precondition on `sacred_affix`/`perfect` at all, so a Sacred or
/// Perfect item being permanently locked out of further crafting is a
/// completely normal, common combination, and that permanently-locked
/// state is the single most important thing for a player to notice at a
/// glance - more so than which rarity tier it happened to land on.
/// Unique still can't co-occur with locked (Krangle refuses a unique
/// item and vice versa), so its own position in this chain is moot in
/// practice, but kept below Sacred for the same "can't actually happen,
/// defensive ordering only" reasoning as before.
fn item_name_class(item: &Item) -> &'static str {
    if item.locked {
        "gear-name gear-name-locked"
    } else if item.sacred_affix.is_some() {
        "gear-name gear-name-sacred"
    } else if item.unique_affix.is_some() {
        "gear-name gear-name-unique"
    } else {
        "gear-name"
    }
}

/// The unique-affix line shown ABOVE an item's tier - the "implicit"
/// treatment per the request. Empty string (nothing rendered) for an
/// item with no `unique_affix`.
fn unique_affix_html(item: &Item) -> String {
    match item.unique_affix {
        Some(unique) => format!("<div class=\"gear-unique\">✦ {}: {}</div>", escape_html(unique.name()), escape_html(&unique.description())),
        None => String::new(),
    }
}

/// Sacred's own implicit-affix callout (2026-08-16, a live request) -
/// same "implicit, above Tier" placement as `unique_affix_html`, just a
/// distinct color/copy so it doesn't read as an ordinary `UniqueAffix`
/// (Celestial Shard etc). Empty string for a non-Sacred item.
fn sacred_affix_html(item: &Item) -> String {
    match item.sacred_affix {
        Some((affix, value)) => format!("<div class=\"gear-sacred\">✦ Sacred: {}</div>", escape_html(&affix_display(affix, value))),
        None => String::new(),
    }
}

/// Krangled items' own tag (2026-08-16, a live request), shown at the
/// VERY BOTTOM of the card - below durability, not up near the name/tier
/// like `sacred_affix_html`/`unique_affix_html`, since this isn't a
/// property of the item's power the way those are; it's a standing
/// warning about what you CAN'T do with it anymore. Empty string for an
/// unlocked item.
fn locked_tag_html(item: &Item) -> String {
    if item.locked {
        "<div class=\"gear-locked-tag\">\u{1F512} Unmodifiable</div>".to_string()
    } else {
        String::new()
    }
}

/// Explains Perfect Quality (see `Item::perfect`'s doc) on hover - per
/// the request that this live on a tooltip over the label itself.
const PERFECT_QUALITY_TIP: &str =
    "Perfect Quality: 20% more of every stat (primary stat AND every modifier) than the same item at 100% Quality. This can't be transferred by Recombine - a recombined item never comes out Perfect, even if one of its two sources was.";

/// Sacred's own tooltip (2026-08-16) - shown instead of Perfect
/// Quality's on a Sacred item (Sacred is always also `perfect`, but gets
/// its own label/tip since it's strictly more than plain Perfect).
const SACRED_TIP: &str =
    "Sacred: Perfect Quality (20% more of every stat) PLUS one extra affix rolled at its own maximum, shown above as an implicit - it doesn't count toward the normal 4-modifier cap and can never be changed by crafting (Scour/Krangle/Augment/Regal/Exalt/Reforge/Recombine). Can't be transferred by Recombine, same as Perfect Quality itself.";

/// The item card's quality line - the normal muted "Quality {n}%" for an
/// ordinary item, a highlighted gold "Perfect Quality" tag for one made
/// via `make_item_perfect`, or Sacred's own distinctly-colored tag (takes
/// priority over Perfect Quality's, since Sacred is always also
/// `perfect`) for one made via `make_item_sacred`.
fn quality_line_html(item: &Item) -> String {
    if item.sacred_affix.is_some() {
        format!("<div class=\"gear-quality gear-quality--sacred\" data-tip=\"{}\">Sacred</div>", escape_html(SACRED_TIP))
    } else if item.perfect {
        format!("<div class=\"gear-quality gear-quality--perfect\" data-tip=\"{}\">Perfect Quality</div>", escape_html(PERFECT_QUALITY_TIP))
    } else {
        format!("<div class=\"gear-quality\">Quality {:.0}%</div>", item.quality_percent())
    }
}

/// Short-form dropdown (same option label as `craft_item_options`) +
/// Equip button for every bagged item matching `slot` - lets the Gear
/// section swap gear directly instead of needing a trip to the Bag &
/// Crafting page (see `render_gear_slot`), per the request that the
/// dashboard keep re-equipping possible without leaving it. Empty
/// string (no form at all) when nothing in the bag matches this slot.
fn render_equip_picker(character: &Character, slot: EquipSlot) -> String {
    let candidates: Vec<&Item> = character.inventory.iter().filter(|i| i.slot == slot).collect();
    if candidates.is_empty() {
        return String::new();
    }
    let options = craft_item_options(character, &candidates, false, None);
    format!(
        "<form method=\"post\" action=\"/equip\" class=\"equip-picker\">\
          <select name=\"item_id\">{options}</select>\
          <button class=\"btn-sm\" type=\"submit\">Equip</button>\
        </form>"
    )
}

fn render_gear_slot(character: &Character, slot: EquipSlot, label: &str) -> String {
    let item = character.equipped(slot);
    let equip_picker_html = render_equip_picker(character, slot);
    match item {
        None => format!(
            "<div class=\"gear-slot empty\"><div class=\"gear-slot-label\">{label}</div><div class=\"gear-empty\">— empty —</div>{equip_picker_html}</div>"
        ),
        Some(item) => {
            let slot_value = format!("{:?}", slot).to_lowercase();
            let repair_html = render_repair_form("/repair-equipped", "slot", &slot_value, item, character.dust);
            let body = item_card_body_html(item);
            format!(
                "<div class=\"gear-slot\"><div class=\"gear-slot-label\">{label}</div>\
                  {body}\
                  <div class=\"slot-actions\">\
                    <form method=\"post\" action=\"/unequip\">\
                      <input type=\"hidden\" name=\"slot\" value=\"{slot_value}\">\
                      <button class=\"btn-sm\" type=\"submit\">Unequip</button>\
                    </form>\
                    {repair_html}\
                  </div>\
                  {equip_picker_html}\
                </div>"
            )
        }
    }
}

/// Splits a Bag's flat item list into 5 independently-collapsible ROWS,
/// one per equip slot (Helm/Weapon/Gloves/Body/Boots), each row laying
/// its items out left-to-right - per the request's mockup, replacing
/// the earlier per-slot-COLUMN layout. Shared by both the owner's own
/// dashboard (`render_inventory_item`, bound to their dust for the
/// repair-cost preview) and the read-only `/characters/{login}` page
/// (`render_inventory_item_readonly`) via the `render_item` callback, so
/// the row/collapse layout itself only lives in one place. Each row
/// defaults open (`<details open>`) - collapsible for tidying away a
/// slot you don't care about, not hidden by default the way the sprite
/// picker is, since browsing your own bag is the whole point of this card.
fn render_inventory_by_slot(items: &[Item], render_item: impl Fn(&Item) -> String) -> String {
    let rows: String = [(EquipSlot::Helm, "Helms"), (EquipSlot::Weapon, "Weapons"), (EquipSlot::Gloves, "Gloves"), (EquipSlot::Body, "Body"), (EquipSlot::Boots, "Boots")]
        .into_iter()
        .map(|(slot, label)| {
            let slot_items: Vec<&Item> = items.iter().filter(|i| i.slot == slot).collect();
            let count = slot_items.len();
            let body: String = if slot_items.is_empty() {
                "<p class=\"muted\">Empty.</p>".to_string()
            } else {
                slot_items.into_iter().map(|item| render_item(item)).collect()
            };
            format!("<details class=\"bag-row\" open><summary>{label} ({count})</summary><div class=\"bag-row-items\">{body}</div></details>")
        })
        .collect();
    format!("<div class=\"bag-rows\">{rows}</div>")
}

fn render_inventory_item(item: &Item, dust: u64) -> String {
    let repair_html = render_repair_form("/repair-item", "item_id", &item.id, item, dust);
    // Protected items don't even get a Disenchant button - the tick-box
    // below is the only way to get one back (see
    // Character::disenchant_from_inventory's matching server-side guard,
    // in case of a stale page/direct POST). Krangled items already never
    // needed this - they've always kept their Disenchant button (see
    // `locked`'s own doc, "still... disenchantable") - the tick-box is a
    // second, independent protection, not a Krangle-only thing.
    let disenchant_html = if item.disenchant_protected {
        String::new()
    } else {
        format!(
            "<form method=\"post\" action=\"/disenchant\" onsubmit=\"return confirm('Disenchant {name_js} for {dust_min}-{dust_max} Thaumatergic Dust? This can\\'t be undone.');\">\
              <input type=\"hidden\" name=\"item_id\" value=\"{id}\">\
              <button class=\"btn-sm btn-danger\" type=\"submit\">Disenchant</button>\
            </form>",
            id = item.id,
            name_js = escape_html(&item.display_name()).replace('\'', ""),
            dust_min = item.tier * item.disenchant_multiplier(),
            dust_max = item.tier * 6 * item.disenchant_multiplier(),
        )
    };
    let protect_checked = if item.disenchant_protected { " checked" } else { "" };
    let slot = item.slot;
    let id = item.id.clone();
    let body = item_card_body_html(item);
    format!(
        "<div class=\"gear-slot\"><div class=\"gear-slot-label\">{slot:?}</div>\
          {body}\
          <div class=\"slot-actions\">\
            <form method=\"post\" action=\"/equip\">\
              <input type=\"hidden\" name=\"item_id\" value=\"{id}\">\
              <button class=\"btn-sm\" type=\"submit\">Equip</button>\
            </form>\
            {disenchant_html}\
            {repair_html}\
          </div>\
          <form method=\"post\" action=\"/toggle-disenchant-protect\" class=\"protect-toggle\">\
            <input type=\"hidden\" name=\"item_id\" value=\"{id}\">\
            <label><input type=\"checkbox\" name=\"protect\" autocomplete=\"off\"{protect_checked} onchange=\"this.form.submit()\"> \u{1f512} Keep (no disenchant)</label>\
          </form>\
        </div>"
    )
}

/// Repair button+form for `item`, posted to `action` with a hidden
/// `field_name=field_value` (either the equip slot or the bag item's id) -
/// empty string (no button) if the item's indestructible or already at
/// full durability. Disabled (still visible, greyed out) if the
/// character can't currently afford the 1-dust-per-tier cost.
fn render_repair_form(action: &str, field_name: &str, field_value: &str, item: &Item, dust: u64) -> String {
    if !item.needs_repair() {
        return String::new();
    }
    let cost = item.repair_cost();
    let disabled = if dust < cost { " disabled" } else { "" };
    format!(
        "<form method=\"post\" action=\"{action}\">\
          <input type=\"hidden\" name=\"{field_name}\" value=\"{field_value}\">\
          <button class=\"btn-sm btn-repair\" type=\"submit\"{disabled}>Repair ({cost}d)</button>\
        </form>"
    )
}

/// The slot's fixed primary stat (from `power`/`power_roll`/`tier`) - shown
/// on its own line between Quality and Tier, separate from `gear_stat_line`'s
/// affixes, so the one guaranteed base stat every item of that slot has
/// reads as visually distinct from the modifiers crafting can add.
fn gear_primary_stat(item: &Item) -> String {
    match item.slot {
        EquipSlot::Weapon => format!("+{} dps", format_number(item.effective_power())),
        EquipSlot::Helm => format!("+{} dps / {:.1}s (stacking)", format_number(item.effective_power()), item.cooldown_ms() as f64 / 1000.0),
        EquipSlot::Body => format!("+{} hp", format_number(item.effective_power())),
        // Gloves' effective_power is a plain 0-1 fraction (a % speed
        // bonus), not a magnitude that grows unbounded with tier the way
        // the other 4 slots' power does - format_number would just read
        // "+50" instead of "+50%" for a perfectly normal roll, so this one
        // stays a plain percentage.
        EquipSlot::Gloves => format!("+{:.0}% speed", item.effective_power() * 100.0),
        EquipSlot::Boots => format!("+{} hp / {:.1}s", format_number(item.effective_power()), item.cooldown_ms() as f64 / 1000.0),
    }
}

/// Just the crafted-on modifiers (see `Affix`) - `gear_primary_stat`'s
/// fixed base stat is shown separately, above Tier. One bullet per
/// modifier (was a single comma-joined line) so a multi-modifier item
/// actually scans easily instead of running together into one dense
/// sentence, with a hover tooltip showing where that specific modifier's
/// own roll landed in its jitter range (see `affix_quality_percent`) -
/// separate from the item's own overall Quality%. Returns raw HTML (each
/// modifier's own text IS escaped individually) - callers must NOT run
/// this back through `escape_html`, unlike every other gear-display
/// field here. Empty string, not an empty `<ul>`, when there are no
/// modifiers at all.
fn gear_stat_line(item: &Item) -> String {
    if item.affixes.is_empty() {
        return String::new();
    }
    let items: String = item
        .affixes
        .iter()
        .map(|(a, v)| {
            let roll_pct = affix_quality_percent(*a, *v, item.tier, item.perfect);
            // Crit-granted modifiers (a rare Reforge/Recombine bonus, see
            // Item::crit_bonus_affixes) get their own bullet color
            // (2026-08-17, a live request) so they stand out from the
            // item's normal, guaranteed modifiers at a glance.
            let class = if item.crit_bonus_affixes.contains(a) { "mod-roll mod-roll-crit" } else { "mod-roll" };
            format!(
                "<li class=\"{class}\" data-tip=\"Roll: {roll_pct:.0}%\">{}</li>",
                escape_html(&affix_display(*a, *v))
            )
        })
        .collect();
    format!("<ul>{items}</ul>")
}

fn render_page(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Adventure Character Dashboard</title>
<style>
  :root {{ color-scheme: dark; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0; padding: 24px 16px 60px; min-height: 100vh;
    background: radial-gradient(ellipse at top, #1c1830 0%, #0c0a16 65%);
    color: #eae6f5; font-family: "Segoe UI", Arial, sans-serif;
    display: flex; flex-direction: column; align-items: center; gap: 16px;
  }}
  .card {{
    width: 100%; max-width: 640px; background: rgba(30, 25, 50, 0.75);
    border: 1px solid rgba(160, 140, 255, 0.25); border-radius: 12px;
    padding: 24px; backdrop-filter: blur(6px);
  }}
  .dashboard-grid {{
    display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 16px; width: 100%; max-width: 1320px; align-items: start;
  }}
  .dashboard-col {{ display: flex; flex-direction: column; gap: 16px; min-width: 0; }}
  .dashboard-col .card {{ max-width: none; }}
  /* Bag card sits below the dashboard-grid, not inside it, so it'd
     otherwise fall back to the plain .card 640px cap - widened to match
     the grid above it so the row layout actually has room to lay items
     out side-by-side instead of cramming into a narrow column. */
  .bag-card {{ max-width: 1320px; }}
  @media (max-width: 860px) {{
    .dashboard-grid {{ grid-template-columns: 1fr; }}
  }}
  h1 {{ margin: 0 0 8px; font-size: 1.6rem; }}
  h2 {{ margin: 0 0 12px; font-size: 1.1rem; color: #c9bdf0; }}
  h3 {{ margin: 16px 0 6px; font-size: 0.95rem; color: #b0a2d9; }}
  h3:first-of-type {{ margin-top: 4px; }}
  .card ul {{ margin: 0 0 4px; padding-left: 20px; }}
  .card li {{ margin: 4px 0; font-size: 0.9rem; line-height: 1.4; }}
  .profile-row {{ display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }}
  .sprite-avatar {{
    width: 88px; height: 88px; object-fit: contain; flex-shrink: 0;
    background: rgba(255,255,255,0.04); border: 1px solid rgba(160,140,255,0.2);
    border-radius: 10px; padding: 6px;
  }}
  .profile-info {{ flex: 1; min-width: 0; }}
  .header-row {{ display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; }}
  .dust-available {{ font-size: 0.95rem; font-weight: 600; color: #ffd76b; }}
  .role-badge {{ padding: 4px 12px; border-radius: 999px; font-size: 0.8rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }}
  .passives-link-btn {{
    display: inline-block; padding: 6px 14px; border-radius: 999px; text-decoration: none;
    font-size: 0.8rem; font-weight: 700; color: #eae6f5;
    background: linear-gradient(135deg, rgba(124,92,255,0.35), rgba(160,140,255,0.15));
    border: 1px solid rgba(160, 140, 255, 0.4);
  }}
  .passives-link-btn:hover {{ background: linear-gradient(135deg, rgba(124,92,255,0.5), rgba(160,140,255,0.25)); border-color: rgba(160,140,255,0.6); }}
  .role-melee {{ background: #4a1f24; color: #ff8a8a; }}
  .role-ranged {{ background: #1f3a24; color: #8affa0; }}
  .role-support {{ background: #1f2c4a; color: #8ab4ff; }}
  .stat-row {{ display: flex; gap: 24px; margin: 16px 0; flex-wrap: wrap; }}
  .stat-label {{ font-size: 0.75rem; color: #a394c7; text-transform: uppercase; letter-spacing: 0.04em; }}
  .stat-value {{ font-size: 1.4rem; font-weight: 600; }}
  .xp-label {{ font-size: 0.8rem; color: #a394c7; margin-bottom: 4px; }}
  .xp-bar {{ width: 100%; height: 10px; background: rgba(255,255,255,0.08); border-radius: 999px; overflow: hidden; }}
  .xp-fill {{ height: 100%; background: linear-gradient(90deg, #7c5cff, #b98bff); }}
  .gear-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px; }}
  .bag-rows {{ display: flex; flex-direction: column; gap: 10px; }}
  .bag-row {{ background: rgba(255,255,255,0.02); border: 1px solid rgba(160,140,255,0.15); border-radius: 8px; padding: 10px; }}
  .bag-row summary {{
    cursor: pointer; list-style: none; font-weight: 600; font-size: 0.85rem;
    color: #d9d2f0; padding: 4px 2px;
  }}
  .bag-row summary::-webkit-details-marker {{ display: none; }}
  .bag-row[open] summary {{ margin-bottom: 8px; }}
  .bag-row-items {{ display: flex; flex-wrap: wrap; gap: 10px; }}
  .bag-row-items .gear-slot {{ flex: 1 1 180px; max-width: 220px; }}
  .bag-row .muted {{ padding: 0 2px; }}
  .model-picker-details summary {{
    cursor: pointer; list-style: none; display: inline-block;
    padding: 8px 14px; margin-top: 4px; border-radius: 8px; font-size: 0.85rem; font-weight: 600;
    background: rgba(124,92,255,0.15); color: #d9d2f0; border: 1px solid rgba(160,140,255,0.25);
  }}
  .model-picker-details summary::-webkit-details-marker {{ display: none; }}
  .model-picker-details summary:hover {{ background: rgba(124,92,255,0.28); }}
  .model-picker-details[open] summary {{ margin-bottom: 4px; }}
  .model-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(112px, 1fr)); gap: 10px; margin: 12px 0; }}
  .model-option {{
    display: flex; flex-direction: column; align-items: center; gap: 4px;
    padding: 8px; border: 1px solid rgba(160,140,255,0.15); border-radius: 8px;
    cursor: pointer; font-size: 0.7rem; text-align: center; background: rgba(255,255,255,0.03);
  }}
  .model-option:hover {{ border-color: rgba(160,140,255,0.4); }}
  .model-option:has(input:checked) {{ border-color: #7c5cff; background: rgba(124,92,255,0.18); }}
  .model-option input {{ margin: 0; }}
  /* 96px, not the original 56px - a two-subject composition (e.g. the
     wolf+ranger duo) reads as an indistinct blob at 56px even though
     the underlying crop itself is clean; this is purely a legibility
     fix, not a re-crop. */
  .model-thumb {{ width: 96px; height: 96px; overflow: hidden; display: flex; align-items: center; justify-content: center; }}
  .model-option img {{ width: 96px; height: 96px; object-fit: contain; }}
  .top-nav {{ width: 100%; max-width: 1320px; display: flex; flex-wrap: wrap; gap: 10px; }}
  .top-nav-link {{
    display: block; text-align: center; flex: 1 1 220px; box-sizing: border-box;
    padding: 12px 16px; border-radius: 12px; text-decoration: none;
    font-size: 1.05rem; font-weight: 700; letter-spacing: 0.02em;
    color: #eae6f5; background: linear-gradient(135deg, rgba(124,92,255,0.35), rgba(160,140,255,0.15));
    border: 1px solid rgba(160, 140, 255, 0.4);
  }}
  .top-nav-link:hover {{ background: linear-gradient(135deg, rgba(124,92,255,0.5), rgba(160,140,255,0.25)); border-color: rgba(160,140,255,0.6); }}
  /* The player's own stat summary (see top_nav's doc) sits apart from
     the link pills - not a link at all, so it doesn't stretch to fill
     the row like they do (flex: 0 0 auto overrides their shared
     flex: 1 1 220px) and margin-left: auto pushes it to the right edge
     of the nav bar instead. */
  .top-nav-stats {{
    flex: 0 0 auto; margin-left: auto; padding: 12px 16px; display: flex; align-items: center;
    font-size: 0.95rem; font-weight: 600; color: #cfc9e8; white-space: nowrap;
  }}
  .buff-activity-table {{ width: 100%; border-collapse: collapse; font-size: 0.92rem; }}
  .buff-activity-table th, .buff-activity-table td {{ text-align: left; padding: 6px 10px; border-bottom: 1px solid rgba(160,140,255,0.15); }}
  .buff-activity-table th {{ color: #b8b0d8; font-weight: 700; text-transform: uppercase; font-size: 0.75rem; letter-spacing: 0.03em; }}
  .roster-grid {{ display: flex; flex-wrap: wrap; gap: 12px; margin: 12px 0; }}
  .roster-card {{ flex: 1 1 140px; max-width: 200px; }}
  .roster-card {{
    display: flex; flex-direction: column; align-items: center; gap: 4px; text-align: center;
    padding: 12px; border: 1px solid rgba(160,140,255,0.15); border-radius: 8px;
    background: rgba(255,255,255,0.03); text-decoration: none; color: inherit;
  }}
  .roster-card:hover {{ border-color: rgba(160,140,255,0.4); background: rgba(124,92,255,0.1); }}
  .roster-sprite {{ width: 72px; height: 72px; object-fit: contain; }}
  .roster-name {{ font-weight: 600; }}
  .roster-meta {{ font-size: 0.75rem; color: #a394c7; }}
  .gear-slot {{ background: rgba(255,255,255,0.04); border: 1px solid rgba(160,140,255,0.15); border-radius: 8px; padding: 12px; }}
  .gear-slot.empty {{ opacity: 0.5; }}
  .gear-slot-label {{ font-size: 0.7rem; color: #a394c7; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 6px; }}
  .gear-name {{ font-weight: 600; margin-bottom: 2px; }}
  /* Golden - a unique item (Item::unique_affix, e.g. Celestial Shard). */
  .gear-name-unique {{ color: #ffd76b; text-shadow: 0 0 6px rgba(255, 215, 107, 0.4); }}
  /* Dark red - a Krangled (locked) item. Checked FIRST in
     item_name_class (2026-08-16 fix) - locked overrides Sacred/Perfect's
     own coloring, since Krangle has no precondition against either and
     "this is permanently locked" is the more important thing to notice. */
  .gear-name-locked {{ color: #b23a3a; }}
  /* Icy blue-white - a Sacred item (Item::sacred_affix), deliberately
     distinct from unique/Perfect's gold so the three tiers never read
     as the same thing at a glance. Only applies when NOT locked (see
     item_name_class's doc). */
  .gear-name-sacred {{ color: #d6f0ff; text-shadow: 0 0 8px rgba(120, 200, 255, 0.6); }}
  /* Krangled items' bottom-of-card tag (2026-08-16, a live request) -
     same dark-red family as .gear-name-locked, styled as a quiet
     standing warning rather than a loud callout like .gear-unique/
     .gear-sacred above (those advertise a benefit; this warns about a
     permanent restriction, so it reads as muted/serious instead). */
  .gear-locked-tag {{
    font-size: 0.72rem; color: #d98d86; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em;
    margin-top: 6px; opacity: 0.85;
  }}
  /* The unique affix line itself, shown above Tier - the "implicit"
     treatment, styled like a small callout so it reads as a fixed,
     built-in property rather than just another modifier. */
  .gear-unique {{
    font-size: 0.78rem; color: #ffd76b; font-weight: 600;
    background: rgba(255, 215, 107, 0.1); border: 1px solid rgba(255, 215, 107, 0.35);
    border-radius: 6px; padding: 4px 7px; margin-bottom: 6px; line-height: 1.35;
  }}
  /* Sacred's own implicit callout - same shape as .gear-unique above,
     just in the icy blue-white theme instead of gold. */
  .gear-sacred {{
    font-size: 0.78rem; color: #d6f0ff; font-weight: 600;
    background: rgba(120, 200, 255, 0.12); border: 1px solid rgba(120, 200, 255, 0.4);
    border-radius: 6px; padding: 4px 7px; margin-bottom: 6px; line-height: 1.35;
  }}
  .gear-quality {{ font-size: 0.75rem; color: #8f7fc4; margin-bottom: 2px; }}
  /* Perfect Quality (see quality_line_html's doc) - gold, bold, and
     cursor:help like every other [data-tip] trigger so its tooltip
     (the shared #js-tooltip system) is discoverable the same way a
     unique implicit's callout already is. */
  .gear-quality--perfect {{ color: #ffd76b; font-weight: 700; cursor: help; width: fit-content; }}
  /* Sacred's own quality-line tag - same shape as --perfect, icy
     blue-white theme, shown instead of --perfect on a Sacred item. */
  .gear-quality--sacred {{ color: #d6f0ff; font-weight: 700; cursor: help; width: fit-content; }}
  .gear-primary {{ font-size: 0.85rem; font-weight: 600; color: #d9d2f0; margin-bottom: 4px; }}
  .gear-tier {{ font-size: 0.75rem; color: #8f7fc4; margin-bottom: 6px; }}
  .gear-stat {{ font-size: 0.85rem; color: #b8adda; margin-bottom: 8px; }}
  .gear-stat ul {{ margin: 0; padding-left: 1.1em; }}
  .gear-stat li {{ margin: 1px 0; }}
  /* All `[data-tip]` tooltips (this rule, stat labels/values, craft
     buttons, veil-check, and the passives tree's nodes below) render
     through ONE shared JS-positioned tooltip (`#js-tooltip`, styled near
     the end of this block, driven by the script at the bottom of the
     page) instead of a per-element `::after` popup - a pure-CSS `::after`
     gets silently clipped by ANY ancestor with `overflow` set to
     anything but `visible` (the passives tree's horizontally-scrollable
     `.tree-wrap` was the reproducible case a live report caught, but the
     same clipping risk existed for every tooltip on the site, in any
     card that ever gained a scroll/overflow rule) - no `z-index` value
     can fix that, since ancestor overflow-clipping happens independently
     of stacking order. A single `position: fixed` element appended
     directly under `<body>` escapes every ancestor's overflow box by
     construction. These base rules just keep the trigger's own cursor/
     sizing; `data-roll` was renamed to `data-tip` on the mod-roll list
     item so every tooltip trigger shares one attribute the script can
     query generically.
  */
  .mod-roll {{ cursor: help; width: fit-content; }}
  /* Crit-granted modifier (Reforge/Recombine's rare bonus affix, see
     Item::crit_bonus_affixes) - green to stand apart from the item's
     normal, guaranteed modifiers at a glance. */
  .mod-roll-crit {{ color: #6ee08a; font-weight: 600; }}
  .stat-label[data-tip] {{ cursor: help; width: fit-content; }}
  .stat-value[data-tip] {{ cursor: help; width: fit-content; }}
  .veil-check[data-tip] {{ cursor: help; width: fit-content; }}
  #js-tooltip {{
    position: fixed; z-index: 9999; pointer-events: none;
    width: max-content; max-width: 300px;
    background: #1c1830; border: 1px solid rgba(160, 140, 255, 0.4);
    color: #eae6f5; font-size: 0.72rem; font-weight: 400; line-height: 1.45;
    text-align: left; white-space: normal;
    padding: 8px 10px; border-radius: 8px; box-shadow: 0 10px 26px rgba(0, 0, 0, 0.45);
    opacity: 0; transition: opacity 0.1s ease;
  }}
  #js-tooltip.visible {{ opacity: 1; }}
  /* ---- Passives page: literal port of the design artifact's own CSS,
     scoped under .ptree-page (same ".dashboard-col .card"-style scoping
     already used above) so it can reuse the artifact's exact class names
     (.node, .btn-save, .role-badge, etc.) without colliding with this
     file's other, differently-styled uses of those same names elsewhere
     on the site. Values copied 1:1 from the artifact, not re-derived. */
  .ptree-page {{ max-width: 1320px; margin: 0 auto; width: 100%; }}
  .ptree-page .masthead {{ margin-bottom: 6px; }}
  .ptree-page .eyebrow {{ font-size: 0.72rem; letter-spacing: 0.14em; text-transform: uppercase; color: #ff8a8a; font-weight: 600; }}
  .ptree-page h1 {{ margin: 4px 0 0; font-size: 2.1rem; font-weight: 700; letter-spacing: 0.01em; }}
  .ptree-page .subhead {{ margin: 6px 0 0; color: #a394c7; font-size: 0.95rem; max-width: 78ch; }}
  .ptree-page .current-row {{ display: flex; align-items: flex-end; justify-content: space-between; gap: 20px; flex-wrap: wrap; margin: 20px 0 6px; }}
  .ptree-page .class-strip {{ display: flex; align-items: center; gap: 10px; }}
  .ptree-page .class-strip .icon {{ font-size: 1.5rem; }}
  .ptree-page .class-strip h2 {{ margin: 0; font-size: 1.4rem; font-weight: 700; }}
  .ptree-page .role-badge {{ font-size: 0.66rem; letter-spacing: 0.06em; text-transform: uppercase; padding: 3px 10px; border-radius: 999px; border: 1px solid rgba(160, 140, 255, 0.18); color: #a394c7; background: none; }}
  .ptree-page .side-chips {{ display: flex; flex-direction: column; align-items: flex-end; gap: 8px; }}
  .ptree-page .points-chip {{
    display: flex; align-items: center; gap: 8px;
    background: rgba(255, 255, 255, 0.04); border: 1px solid rgba(160, 140, 255, 0.18);
    border-radius: 999px; padding: 8px 16px; font-size: 0.85rem; color: #a394c7; white-space: nowrap;
  }}
  .ptree-page .points-chip strong {{ color: #ff8a8a; font-weight: 700; font-variant-numeric: tabular-nums; }}
  .ptree-page .btn-respec {{
    font: inherit; font-size: 0.78rem; color: #a394c7;
    background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(160, 140, 255, 0.18);
    border-radius: 999px; padding: 6px 14px; cursor: pointer;
  }}
  .ptree-page .btn-respec:hover {{ border-color: rgba(160, 140, 255, 0.5); color: #eae6f5; }}
  .ptree-page .btn-respec:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .ptree-page .points-formula {{ font-size: 0.72rem; color: #766a99; margin: 2px 0 0; }}
  .ptree-page .preview-row {{ display: flex; gap: 8px; }}
  .ptree-page .btn-save {{
    font: inherit; font-size: 0.78rem; font-weight: 600; color: #0c0a16;
    background: #ff8a8a; border: 1px solid #ff8a8a; border-radius: 999px; padding: 6px 14px; cursor: pointer;
  }}
  .ptree-page .btn-save:hover:not(:disabled) {{ background: #ffa3a3; border-color: #ffa3a3; }}
  .ptree-page .btn-save:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .ptree-page .preview-note {{ font-size: 0.72rem; color: #766a99; margin: 0; }}
  .ptree-page .preview-note.dirty {{ color: #ffd76b; }}
  .ptree-page .tree-meta {{ display: flex; gap: 10px; flex-wrap: wrap; margin: 16px 0 10px; }}
  .ptree-page .tier-count {{
    font-size: 0.72rem; color: #766a99; letter-spacing: 0.04em;
    background: rgba(255,255,255,0.03); border: 1px solid rgba(160, 140, 255, 0.18);
    border-radius: 999px; padding: 4px 12px;
  }}
  .ptree-page .legend {{
    display: flex; gap: 16px; flex-wrap: wrap; align-items: center; margin: 4px 0 20px;
    padding: 12px 18px; background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(160, 140, 255, 0.18); border-radius: 10px;
    font-size: 0.75rem; color: #a394c7;
  }}
  .ptree-page .legend-item {{ display: flex; align-items: center; gap: 7px; }}
  .ptree-page .legend-dots {{ display: flex; gap: 3px; }}
  .ptree-page .legend-dot {{ width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid #766a99; }}
  .ptree-page .legend-dot.on {{ background: #ff3b4e; border-color: #ff3b4e; }}
  .ptree-page .legend-dot.spec {{ background: #ffd76b; border-color: #ffd76b; }}
  .ptree-page .tree-wrap {{ overflow-x: auto; overflow-y: visible; padding-bottom: 12px; display: flex; justify-content: center; }}
  .ptree-page #stage {{ position: relative; }}
  .ptree-page .connectors {{ position: absolute; top: 0; left: 0; overflow: visible; shape-rendering: crispEdges; }}
  .ptree-page .node {{
    position: absolute; transform: translate(-50%, -50%); z-index: 1;
    border-radius: 10px; padding: 7px 9px;
    background: #1c1830; border: 1.5px solid rgba(160, 140, 255, 0.18);
    text-align: center; cursor: help;
  }}
  .ptree-page .node:hover {{ z-index: 20; }}
  .ptree-page .node-root {{
    background: linear-gradient(160deg, rgba(255, 59, 78, 0.16), rgba(28, 24, 48, 0.9));
    border: 1.5px solid rgba(255, 59, 78, 0.5); padding: 14px 18px; cursor: default;
  }}
  .ptree-page .node-root .node-name {{ font-size: 1.02rem; letter-spacing: 0.04em; }}
  .ptree-page .node-root .node-desc {{ color: #a394c7; font-size: 0.72rem; line-height: 1.4; margin-top: 4px; }}
  .ptree-page .node-kind {{ font-size: 0.56rem; letter-spacing: 0.06em; text-transform: uppercase; color: #766a99; margin-bottom: 2px; }}
  .ptree-page .node-spec .node-kind, .ptree-page .node-mod .node-kind {{ display: none; }}
  .ptree-page .node-name {{ font-size: 0.78rem; font-weight: 700; line-height: 1.22; color: #eae6f5; }}
  .ptree-page .node-skill .node-name {{ font-size: 0.84rem; }}
  .ptree-page .node-mod .node-name {{ font-size: 0.72rem; }}
  .ptree-page .dots {{ display: flex; justify-content: center; gap: 4px; margin-top: 6px; }}
  .ptree-page .dot {{ width: 11px; height: 11px; border-radius: 50%; border: 1.5px solid #766a99; background: transparent; display: inline-block; }}
  .ptree-page .dot.filled {{ background: #ff3b4e; border-color: #ff3b4e; box-shadow: 0 0 6px rgba(255, 59, 78, 0.7); }}
  .ptree-page .dot.filled.dot-spec {{ background: #ffd76b; border-color: #ffd76b; box-shadow: 0 0 6px rgba(255, 215, 107, 0.7); }}
  .ptree-page .node-buttons {{ display: flex; align-items: center; justify-content: center; gap: 6px; margin-top: 6px; }}
  .ptree-page .node-rank {{ font-variant-numeric: tabular-nums; color: #a394c7; font-size: 0.78rem; }}
  .ptree-page .node--maxed {{ border-color: rgba(255, 59, 78, 0.55); box-shadow: 0 0 0 1px rgba(255, 59, 78, 0.12), 0 8px 22px rgba(255, 59, 78, 0.12); }}
  .ptree-page .node--maxed .node-name {{ color: #ff8a8a; }}
  .ptree-page .node--specialized {{ border-color: rgba(255, 215, 107, 0.6); box-shadow: 0 0 0 1px rgba(255, 215, 107, 0.15), 0 8px 22px rgba(255, 215, 107, 0.12); }}
  .ptree-page .node--specialized .node-name {{ color: #ffd76b; }}
  .ptree-page .node--invested {{ border-color: rgba(160, 140, 255, 0.45); }}
  .ptree-page .node--locked {{ opacity: 0.45; cursor: not-allowed; }}
  .ptree-page .node--locked .node-name {{ color: #a394c7; }}
  .ptree-page .node--locked .node-buttons button {{ pointer-events: none; }}
  .ptree-page footer {{ margin-top: 26px; font-size: 0.78rem; color: #766a99; max-width: 80ch; line-height: 1.6; }}
  .ptree-page footer strong {{ color: #a394c7; }}
  /* Split Personality's 2nd-class section (see `render_ptree_body`'s
     doc) - nested inside `.ptree-page` so it already inherits every
     `.ptree-page X` rule above for free (masthead/current-row/tree-wrap/
     node styling); this just adds a visual break from the primary tree
     above it and lays out the picker form. */
  .ptree-secondary {{ margin-top: 36px; padding-top: 28px; border-top: 1px dashed rgba(160, 140, 255, 0.25); }}
  .secondary-picker {{ display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin: 14px 0 6px; }}
  .secondary-picker label {{ font-size: 0.85rem; color: #c9bdf0; }}
  .secondary-picker select {{
    background: rgba(255,255,255,0.04); color: #eae6f5; border: 1px solid rgba(160,140,255,0.3);
    border-radius: 8px; padding: 6px 10px; font-size: 0.85rem; font-family: inherit;
  }}
  .passive-actions {{ display: flex; gap: 10px; flex-wrap: wrap; margin: 12px 0 4px; }}
  .btn-save {{
    font: inherit; font-size: 0.85rem; font-weight: 600; color: #fff;
    background: #7c5cff; border: none; border-radius: 8px; padding: 8px 16px; cursor: pointer;
  }}
  .btn-save:hover {{ background: #6a4ae8; }}
  .btn-save:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .btn-respec {{
    font: inherit; font-size: 0.85rem; color: #a394c7;
    background: rgba(255, 255, 255, 0.05); border: 1px solid rgba(160, 140, 255, 0.25);
    border-radius: 8px; padding: 8px 16px; cursor: pointer;
  }}
  .btn-respec:hover {{ border-color: rgba(160, 140, 255, 0.5); color: #eae6f5; }}
  .btn-respec:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .preview-note {{ font-size: 0.8rem; color: #766a99; margin: 4px 0; }}
  .preview-note.dirty {{ color: #ffd76b; }}
  .gear-empty {{ font-size: 0.85rem; color: #766a99; }}
  .durability-bar {{ width: 100%; height: 6px; background: rgba(255,255,255,0.08); border-radius: 999px; overflow: hidden; display: inline-block; vertical-align: middle; width: 70%; }}
  .durability-fill {{ height: 100%; }}
  .durability-fill.good {{ background: #5fd66a; }}
  .durability-fill.warn {{ background: #e0c23f; }}
  .durability-fill.critical {{ background: #ff4444; }}
  .durability-pct {{ font-size: 0.75rem; color: #a394c7; margin-left: 6px; }}
  .indestructible {{ font-size: 0.8rem; color: #ffd76a; font-weight: 600; }}
  .needs-repair {{ font-size: 0.75rem; color: #ff8a8a; margin-top: 4px; }}
  .btn {{
    display: inline-block; padding: 10px 20px; background: #7c5cff; color: #fff;
    text-decoration: none; border: none; border-radius: 8px; font-size: 0.95rem;
    font-weight: 600; cursor: pointer;
  }}
  .btn:hover {{ background: #6a4ae8; }}
  .btn:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .btn.wings-active {{ background: linear-gradient(135deg, #4fd1c5, #7c5cff); }}
  .btn.wings-active:hover {{ background: linear-gradient(135deg, #3fb8ac, #6a4ae8); }}
  select, input[type="text"], input[type="number"] {{
    display: block; width: 100%; margin: 10px 0; padding: 10px 12px;
    background: rgba(255,255,255,0.06); color: #eae6f5; border: 1px solid rgba(160,140,255,0.25);
    border-radius: 8px; font-size: 0.9rem; font-family: inherit;
  }}
  select option {{ background: #1e1932; color: #eae6f5; }}
  /* Crafting/equip-picker dropdown groups (live request: "soften the
     colors to muted greys... we dont want to flashbang anyone") -
     `craft_item_options` is the only place `<optgroup>` is used anywhere
     on the site, and it had zero custom color before this: an unstyled
     optgroup renders with stark default browser/OS colors (often a plain
     bright system list), clashing hard against the page's dark theme -
     that's almost certainly the actual flashbang, not the option color
     above (which every other dropdown already shares uneventfully).
     Muted greys, not the page's usual purple tint, per the exact request. */
  select optgroup {{ background: #242429; color: #8f8f99; font-weight: 600; font-style: normal; }}
  #crafting-card select option, .equip-picker select option {{ background: #2b2b31; color: #b8b8c0; }}
  input[type="text"]::placeholder {{ color: #766a99; }}
  .tunable-row {{ margin-bottom: 14px; }}
  .tunable-row label {{ display: block; font-size: 0.85rem; color: #c9bdf0; margin-bottom: 2px; }}
  .tunable-row input {{ margin: 4px 0 0; }}
  .tunable-hint {{ font-size: 0.75rem; color: #766a99; margin: 2px 0 0; }}
  /* Crafting wiki section (see `render_wiki_crafting`) - own `.wiki-*`
     namespace, same convention as the passive-tree wiki block below.
     `.wiki-wide` opts this one card out of the page's default 640px
     `.card` cap (see `.dashboard-col .card`/`.bag-card` for the same
     precedent) since it's mostly tables, which cramp badly at 640px. */
  .wiki-wide {{ max-width: 900px; }}
  .wiki-wide h3 {{ border-top: 1px solid rgba(160,140,255,0.15); padding-top: 16px; margin-top: 24px; }}
  .wiki-wide h3:first-of-type {{ border-top: none; padding-top: 4px; margin-top: 4px; }}
  .wiki-wide code {{ background: rgba(255,255,255,0.06); border-radius: 4px; padding: 1px 5px; font-size: 0.88em; }}
  .wiki-currency-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin: 12px 0; }}
  .wiki-currency-card {{ background: rgba(255,255,255,0.03); border: 1px solid rgba(160,140,255,0.15); border-radius: 10px; padding: 12px 14px; }}
  .wiki-currency-card h4 {{ margin: 0 0 4px; font-size: 0.9rem; color: #d9d2f0; }}
  .wiki-currency-card p {{ margin: 0; font-size: 0.82rem; color: #a394c7; }}
  .wiki-ceiling {{ background: rgba(255,255,255,0.03); border: 1px solid rgba(160,140,255,0.15); border-radius: 10px; padding: 16px 18px; margin: 14px 0; }}
  .wiki-ceiling-row {{ display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }}
  .wiki-slot {{ width: 34px; height: 34px; border-radius: 8px; display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: 0.85rem; flex: none; }}
  .wiki-slot--base {{ background: #7c5cff; color: #fff; }}
  .wiki-slot--bonus {{ border: 2px dashed rgba(160,140,255,0.35); color: #a394c7; }}
  .wiki-slot-plus {{ color: #a394c7; font-size: 1rem; padding: 0 2px; }}
  .wiki-ceiling ul {{ margin: 14px 0 0; font-size: 0.85rem; color: #c9bdf0; }}
  .wiki-ceiling-cap {{ text-align: center; margin: 14px 0 0; font-size: 0.85rem; color: #a394c7; }}
  .wiki-ceiling-cap strong {{ color: #eae6f5; font-size: 1.05rem; }}
  .wiki-table-wrap {{ overflow-x: auto; border: 1px solid rgba(160,140,255,0.15); border-radius: 10px; margin: 12px 0; }}
  .wiki-table-wrap table {{ border-collapse: collapse; width: 100%; min-width: 480px; }}
  .wiki-table-wrap th, .wiki-table-wrap td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid rgba(160,140,255,0.12); font-size: 0.85rem; }}
  .wiki-table-wrap th {{ font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.05em; color: #a394c7; background: rgba(255,255,255,0.03); }}
  .wiki-table-wrap tbody tr:last-child td {{ border-bottom: none; }}
  .wiki-tier-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 12px 0; }}
  .wiki-tier-card {{ background: rgba(255,255,255,0.03); border: 1px solid rgba(160,140,255,0.15); border-radius: 10px; padding: 14px 16px; }}
  .wiki-tier-card h4 {{ margin: 0 0 6px; }}
  .wiki-tier-card p {{ margin: 0; font-size: 0.85rem; color: #a394c7; }}
  /* Wide enough for the passive-tree graph (see `render_wiki_archetype_graph`)
     - same reasoning as `.wiki-wide`/`.bag-card` opting out of the plain
     `.card` 640px cap. `.tree-wrap`'s own `overflow-x: auto` still covers
     any class whose tree is wider than even this on a small screen. */
  .wiki-archetype {{ max-width: 1320px; }}
  .wiki-archetype .ptree-page {{ max-width: none; margin: 12px 0 0; }}
  .wiki-archetype summary {{ font-size: 1rem; display: flex; align-items: center; gap: 10px; }}
  .wiki-root-desc {{ font-size: 0.85rem; color: #c9bdf0; margin: 4px 0 16px; }}
  .wiki-skill {{ border-top: 1px solid rgba(160,140,255,0.15); padding-top: 12px; margin-top: 12px; }}
  .wiki-skill h3 {{ margin: 0 0 4px; font-size: 0.95rem; }}
  .wiki-skill > p {{ margin: 0 0 10px; font-size: 0.88rem; }}
  .wiki-spec {{ margin: 10px 0 10px 18px; padding-left: 12px; border-left: 2px solid rgba(160,140,255,0.2); }}
  .wiki-spec h4 {{ margin: 0 0 4px; font-size: 0.85rem; color: #d9d2f0; }}
  .wiki-spec > p {{ margin: 0 0 6px; font-size: 0.82rem; }}
  .wiki-mod-list {{ margin: 0; padding-left: 18px; font-size: 0.8rem; color: #c9bdf0; }}
  .wiki-mod-list li {{ margin-bottom: 4px; }}
  .slot-actions {{ display: flex; gap: 8px; margin-top: 10px; flex-wrap: wrap; }}
  .protect-toggle {{ margin-top: 8px; font-size: 0.78rem; color: #c9bdf0; }}
  .protect-toggle label {{ display: flex; align-items: center; gap: 6px; cursor: pointer; }}
  .auto-disenchant-row {{ display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 12px; }}
  .auto-disenchant-row select, .auto-disenchant-row input[type=\"number\"] {{ display: inline-block; width: auto; margin: 0; padding: 6px 10px; }}
  .veil-check {{ display: block; margin: 4px 0 12px; font-size: 0.85rem; color: #c9bdf0; }}
  .craft-actions {{ display: flex; gap: 8px; flex-wrap: wrap; }}
  .polish-reforge-actions {{ display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 10px; padding-top: 10px; border-top: 1px solid #3a3255; }}
  .polish-reforge-actions + .craft-actions {{ margin-top: 10px; padding-top: 10px; border-top: 1px solid #3a3255; }}
  .polish-reforge-actions .muted {{ font-size: 0.82rem; }}
  .batch-check {{ display: inline-flex; align-items: center; gap: 4px; font-size: 0.82rem; color: #c9bdf0; cursor: pointer; }}
  .veil-options {{ display: flex; flex-direction: column; gap: 10px; margin-top: 8px; }}
  .veil-option {{ margin: 0; }}
  .veil-option .btn {{ width: 100%; text-align: center; }}
  .btn-sm {{
    padding: 6px 12px; background: #7c5cff; color: #fff; border: none;
    border-radius: 6px; font-size: 0.8rem; font-weight: 600; cursor: pointer;
  }}
  .btn-sm:hover {{ background: #6a4ae8; }}
  .btn-sm.btn-danger {{ background: rgba(255,68,68,0.15); color: #ff8a8a; }}
  .btn-sm.btn-danger:hover {{ background: rgba(255,68,68,0.3); }}
  .btn-sm.btn-repair {{ background: rgba(122,196,255,0.15); color: #7ac4ff; }}
  .btn-sm.btn-repair:hover {{ background: rgba(122,196,255,0.3); }}
  .btn-sm:disabled {{ opacity: 0.4; cursor: not-allowed; }}
  .btn-sm:disabled:hover {{ background: rgba(122,196,255,0.15); }}
  .muted {{ color: #a394c7; font-size: 0.85rem; }}
  .muted a {{ color: #a394c7; }}
  a {{ color: #b98bff; }}
  .reforge-pill {{ display: inline-block; padding: 4px 12px; border-radius: 999px; font-size: 0.85rem; font-weight: 600; margin-bottom: 10px; }}
  .token-row {{ display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 14px; }}
  .token-pill {{
    display: inline-block; padding: 4px 10px; border-radius: 999px; font-size: 0.8rem; font-weight: 600;
    background: rgba(255, 215, 106, 0.15); color: #ffd76a; border: 1px solid rgba(255, 215, 106, 0.3);
  }}
  .countdown-card form {{ margin-top: 4px; }}
  .retreat-card {{ border-color: rgba(255, 138, 138, 0.5); }}
  .retreat-card h2 {{ color: #ff8a8a; }}
  .reforge-ready {{ background: rgba(95,214,106,0.15); color: #5fd66a; }}
  .reforge-used {{ background: rgba(255,255,255,0.08); color: #a394c7; }}
  .reforge-countdown {{ margin: 10px 0 0; font-size: 0.85rem; color: #a394c7; }}
  #reforge-timer {{ font-variant-numeric: tabular-nums; font-weight: 600; color: #eae6f5; }}
  .modal-backdrop {{
    position: fixed; inset: 0; background: rgba(8, 6, 16, 0.75); backdrop-filter: blur(3px);
    display: flex; align-items: center; justify-content: center; z-index: 100; padding: 16px;
  }}
  .modal {{
    width: 100%; max-width: 380px; background: linear-gradient(160deg, #241f3d, #17132a);
    border: 1px solid rgba(255, 215, 106, 0.4); border-radius: 14px; padding: 28px 24px;
    text-align: center; box-shadow: 0 0 40px rgba(255, 140, 60, 0.25);
  }}
  .modal-icon {{ font-size: 2.5rem; margin-bottom: 8px; }}
  .modal h2 {{ margin: 0 0 12px; color: #ffd76a; }}
  .modal p {{ margin: 6px 0; }}
  .modal-tier {{ font-size: 1.1rem; font-weight: 700; color: #eae6f5; margin: 12px 0 20px; }}
</style>
</head>
<body>
<div id="js-tooltip"></div>
{body}
<script>
  (function() {{
    // Every action on this dashboard is a plain <form method="post">
    // submit followed by a server-side redirect - a real page
    // navigation, which the browser resets to scroll 0 on by default
    // (2026-08-17, a live report: "the page keeps scrolling back to the
    // top on every action"). Save the scroll position right before the
    // browser navigates away (sessionStorage survives the navigation, a
    // JS variable wouldn't), then restore it as soon as the new page's
    // script runs.
    document.addEventListener('submit', function() {{
      sessionStorage.setItem('scrollY', String(window.scrollY));
    }}, true);
    var savedY = sessionStorage.getItem('scrollY');
    if (savedY !== null) {{
      sessionStorage.removeItem('scrollY');
      window.scrollTo(0, parseInt(savedY, 10));
    }}
  }})();
  (function() {{
    // Generic ticking countdown for every .countdown-card on the page
    // (Reforge Gear's hourly reset, the Retreated banner's auto-repair
    // timer) - each just needs a data-reset-ms and a .countdown-timer
    // span inside it.
    var cards = document.querySelectorAll('.countdown-card');
    if (!cards.length) return;
    var timers = Array.prototype.map.call(cards, function(card) {{
      return {{ resetMs: parseInt(card.dataset.resetMs, 10), el: card.querySelector('.countdown-timer') }};
    }}).filter(function(t) {{ return t.el; }});
    function tick() {{
      var now = Date.now();
      timers.forEach(function(t) {{
        var remaining = Math.max(0, t.resetMs - now);
        if (remaining <= 0) {{
          // Whatever this timer was counting down to just happened - the
          // rest of the page (server-rendered at load) is now stale, so
          // prompt a refresh rather than silently showing a wrong status.
          t.el.textContent = '00:00 — refresh the page';
          return;
        }}
        var mins = Math.floor(remaining / 60000);
        var secs = Math.floor((remaining % 60000) / 1000);
        t.el.textContent = (mins < 10 ? '0' : '') + mins + ':' + (secs < 10 ? '0' : '') + secs;
      }});
    }}
    tick();
    setInterval(tick, 1000);
  }})();
  (function() {{
    // After submitting a craft that came back as a veil choice (see
    // render_veil_choice_card), the POST-redirect-GET flow reloads the
    // whole page from scratch with no memory of what was just clicked -
    // this is what puts the "choose your outcome" card in front of the
    // player instead of leaving them to scroll down and find it
    // themselves. Runs on every load the card happens to be present for,
    // which is exactly the "still waiting on a choice" state either way.
    var veilCard = document.getElementById('veil-choice') || document.getElementById('chancing-wizard');
    if (veilCard) {{
      veilCard.scrollIntoView({{ behavior: 'smooth', block: 'start' }});
    }}
  }})();
  (function() {{
    // Live cost preview for the Crafting card (see render_crafting_card) -
    // every non-free action button carries data-base/data-label, and, if
    // veiling actually changes its price, data-veil-extra too (Scour and
    // token-covered buttons omit it, so they're left alone below).
    // Recombine additionally carries data-recombine, since its veiled
    // price also depends on the combined modifier count of whichever two
    // items are currently picked (each <option> carries data-affixes -
    // see craft_item_options) - server-side is still the source of truth,
    // this is purely a preview so the cost shown matches what submitting
    // the form will actually charge. The 6 non-Recombine buttons also add
    // a flat per-tier surcharge (TIER_CRAFT_DUST_COST in adventure.rs -
    // kept as a literal here, not interpolated, since this script block
    // has no access to that Rust constant by name) off item_a's own tier
    // (each <option> carries data-tier) - Recombine doesn't pay this at
    // all (see AdventureManager::craft_item vs recombine_gear - two
    // separate cost paths), so it's excluded via the same data-recombine
    // check the pool surcharge already uses.
    var TIER_CRAFT_DUST_COST = 3;
    var veilCheckbox = document.querySelector('input[name="veiled"]');
    var itemASelect = document.querySelector('select[name="item_a"]');
    var itemBSelect = document.querySelector('select[name="item_b"]');
    var craftButtons = document.querySelectorAll('.craft-actions button[data-base]');
    // Polishing/Reforge each price off item_a alone, in their own
    // currency, entirely outside the base/tier/veil formula above - see
    // render_crafting_card's own doc on why their cost text has to be
    // computed here instead of server-side.
    var polishBtn = document.querySelector('.craft-actions button[data-polish], .polish-reforge-actions button[data-polish]');
    var reforgeBtn = document.querySelector('.craft-actions button[data-reforge], .polish-reforge-actions button[data-reforge]');
    // x5/x10 batch checkboxes for Polish/Reforge only (see the hidden
    // `times` input do_craft reads server-side) - mutually exclusive
    // (checking one clears the other), unchecked-both means times=1, the
    // normal single-craft behavior every other button already has.
    var timesInput = document.querySelector('input[name="times"]');
    var batchChecks = document.querySelectorAll('.polish-reforge-actions input[data-times]');
    var currentTimes = function() {{
      for (var i = 0; i < batchChecks.length; i++) {{
        if (batchChecks[i].checked) return parseInt(batchChecks[i].getAttribute('data-times'), 10);
      }}
      return 1;
    }};
    var updateSpecialCosts = function() {{
      var times = currentTimes();
      if (timesInput) timesInput.value = times;
      var suffix = times > 1 ? ' x' + times : '';
      if (polishBtn) {{
        var quality = itemASelect && itemASelect.selectedOptions.length ? parseFloat(itemASelect.selectedOptions[0].getAttribute('data-quality') || '0') : 0;
        var perfect = itemASelect && itemASelect.selectedOptions.length && itemASelect.selectedOptions[0].getAttribute('data-perfect') === '1';
        // Item::has_polish_room, mirrored server-side too (see
        // Character::polish's own early NothingToPolish check) - an
        // already-maxed item gets a disabled, clearly-labeled button
        // instead of silently charging sand for nothing.
        var polishRoom = !itemASelect || !itemASelect.selectedOptions.length || itemASelect.selectedOptions[0].getAttribute('data-polish-room') !== '0';
        // Only an approximation past the first use when NOT perfect -
        // polishing raises quality, which raises the next use's own cost,
        // and this preview can't simulate that chain without duplicating
        // Character::polish's own logic client-side. Good enough for a
        // preview; the server is still the real source of truth and
        // charges each iteration's actual cost as it goes.
        var sandCost = (perfect ? 12 : Math.ceil(quality / 10)) * times;
        var sandHave = parseInt(polishBtn.getAttribute('data-sand') || '0', 10);
        if (!polishRoom) {{
          polishBtn.textContent = 'Polishing (maxed)';
          polishBtn.disabled = true;
        }} else {{
          polishBtn.textContent = 'Polishing' + suffix + ' (' + sandCost + 's)';
          polishBtn.disabled = sandHave < sandCost;
        }}
      }}
      if (reforgeBtn) {{
        var tier = itemASelect && itemASelect.selectedOptions.length ? parseInt(itemASelect.selectedOptions[0].getAttribute('data-tier') || '0', 10) : 0;
        var dustCost = 30 * tier * times;
        var dustHave = parseInt(reforgeBtn.getAttribute('data-dust') || '0', 10);
        reforgeBtn.textContent = 'Reforge' + suffix + ' (' + dustCost + 'd)';
        reforgeBtn.disabled = dustHave < dustCost;
      }}
    }};
    if (itemASelect) itemASelect.addEventListener('change', updateSpecialCosts);
    for (var bi = 0; bi < batchChecks.length; bi++) {{
      (function(cb) {{
        cb.addEventListener('change', function() {{
          if (cb.checked) {{
            for (var j = 0; j < batchChecks.length; j++) {{
              if (batchChecks[j] !== cb) batchChecks[j].checked = false;
            }}
          }}
          updateSpecialCosts();
        }});
      }})(batchChecks[bi]);
    }}
    updateSpecialCosts();
    if (veilCheckbox && craftButtons.length) {{
      var affixCountOf = function(select) {{
        if (!select || !select.selectedOptions.length) return 0;
        var n = parseInt(select.selectedOptions[0].getAttribute('data-affixes') || '0', 10);
        return isNaN(n) ? 0 : n;
      }};
      var tierOf = function(select) {{
        if (!select || !select.selectedOptions.length) return 0;
        var n = parseInt(select.selectedOptions[0].getAttribute('data-tier') || '0', 10);
        return isNaN(n) ? 0 : n;
      }};
      var updateCraftCosts = function() {{
        var veiled = veilCheckbox.checked;
        var pool = affixCountOf(itemASelect) + affixCountOf(itemBSelect);
        var tierCost = TIER_CRAFT_DUST_COST * tierOf(itemASelect);
        craftButtons.forEach(function(btn) {{
          var base = parseInt(btn.getAttribute('data-base'), 10);
          var extra = btn.getAttribute('data-veil-extra');
          var isRecombine = btn.hasAttribute('data-recombine');
          var cost = isRecombine ? base : base + tierCost;
          if (veiled && extra !== null) {{
            cost += parseInt(extra, 10);
            if (isRecombine) {{
              cost += 500 * pool;
            }}
          }}
          var costText = cost > 0 ? (cost + 'd') : 'Free';
          btn.textContent = btn.getAttribute('data-label') + ' (' + costText + ')';
        }});
      }};
      veilCheckbox.addEventListener('change', updateCraftCosts);
      if (itemASelect) itemASelect.addEventListener('change', updateCraftCosts);
      if (itemBSelect) itemBSelect.addEventListener('change', updateCraftCosts);
      updateCraftCosts();
    }}
  }})();

  (function() {{
    // Confirmation prompt for Krangle & Scour (see action_btn's
    // data-confirm attribute) - both are irreversible, so this shows
    // exactly which item is about to be affected before the form
    // actually submits, instead of a silent one-click commit.
    var craftForm = document.querySelector('.craft-actions')?.closest('form');
    if (craftForm) {{
      craftForm.addEventListener('submit', function(e) {{
        var btn = e.submitter;
        if (!btn || !btn.hasAttribute('data-confirm')) return;
        var itemSelect = craftForm.querySelector('select[name="item_a"]');
        var itemLabel = itemSelect && itemSelect.selectedOptions.length ? itemSelect.selectedOptions[0].textContent : 'this item';
        var actionLabel = btn.getAttribute('data-label') || btn.textContent;
        if (!window.confirm(actionLabel + ' ' + itemLabel + '? This cannot be undone.')) {{
          e.preventDefault();
        }}
      }});
    }}
  }})();

  (function() {{
    // Same fix as the OBS overlay's sprite normalization (see
    // overlay.html's computeContentScale) - sprites are sliced by hand
    // from packed sheets, so different sheets carry different amounts of
    // transparent padding around the actual character art. Even fit
    // into the SAME 96x96 box (see .model-thumb/.model-option img),
    // a tightly-cropped sprite reads visibly bigger than a padded one.
    // Zooms each thumbnail in proportionally (never out - a zoom below
    // 1x would just shrink an already-fine crop for no reason) to
    // compensate, clipped to the 96x96 window by .model-thumb's own
    // overflow:hidden so the zoomed image never spills into a neighbor.
    var thumbs = document.querySelectorAll('.model-option img');
    if (!thumbs.length) return;
    function normalize(img) {{
      try {{
        var w = img.naturalWidth, h = img.naturalHeight;
        if (!w || !h) return;
        var cvs = document.createElement('canvas');
        cvs.width = w;
        cvs.height = h;
        var cctx = cvs.getContext('2d');
        cctx.drawImage(img, 0, 0);
        var data = cctx.getImageData(0, 0, w, h).data;
        var minX = -1, maxX = -1, minY = -1, maxY = -1;
        var THRESHOLD = 10;
        for (var y = 0; y < h; y++) {{
          var rowStart = y * w * 4;
          for (var x = 0; x < w; x++) {{
            if (data[rowStart + x * 4 + 3] > THRESHOLD) {{
              if (minX === -1 || x < minX) minX = x;
              if (x > maxX) maxX = x;
              if (minY === -1) minY = y;
              maxY = y;
            }}
          }}
        }}
        if (minX === -1 || maxX < minX || minY === -1 || maxY < minY) return;
        var fillW = (maxX - minX + 1) / w;
        var fillH = (maxY - minY + 1) / h;
        var fill = Math.max(fillW, fillH);
        // Negligible padding (already fills the frame) or a scan result
        // too small to trust (likely a decode glitch, not real content) -
        // leave it alone rather than risk a wrong or wild zoom.
        if (fill <= 0.05 || fill >= 0.95) return;
        var zoom = Math.min(1 / fill, 1.8);
        img.style.transform = 'scale(' + zoom.toFixed(3) + ')';
      }} catch (err) {{
        // Same-origin sprites shouldn't ever taint the canvas, but a
        // decode hiccup should never break the picker over a cosmetic fix.
      }}
    }}
    thumbs.forEach(function(img) {{
      if (img.complete && img.naturalWidth > 0) normalize(img);
      else img.addEventListener('load', function() {{ normalize(img); }});
    }});
  }})();

  (function() {{
    // Every [data-tip] element on the page (stat labels/values, craft
    // buttons, veil-check, mod-roll, passives tree nodes) shares this one
    // tooltip instead of each rendering its own CSS ::after popup - see
    // the #js-tooltip rule's doc for why (ancestor overflow-clipping a
    // live report caught on the passives tree, but really any card on
    // the site). Appended directly under <body> (not wherever the
    // trigger happens to be nested) and positioned with `fixed`
    // coordinates computed from the trigger's own bounding box, so no
    // ancestor's overflow/scroll box can ever clip it, and it's clamped
    // to the viewport so it can't run off-screen either.
    var tip = document.getElementById('js-tooltip');
    var triggers = document.querySelectorAll('[data-tip]');
    if (!tip || !triggers.length) return;
    var MARGIN = 8;
    function position(el) {{
      var rect = el.getBoundingClientRect();
      var tipRect = tip.getBoundingClientRect();
      var top = rect.top - tipRect.height - MARGIN;
      if (top < MARGIN) {{
        // Not enough room above the trigger (e.g. the top row of the
        // passives tree) - flip to below it instead.
        top = rect.bottom + MARGIN;
      }}
      top = Math.min(top, window.innerHeight - tipRect.height - MARGIN);
      var left = rect.left + rect.width / 2 - tipRect.width / 2;
      left = Math.max(MARGIN, Math.min(left, window.innerWidth - tipRect.width - MARGIN));
      tip.style.top = top + 'px';
      tip.style.left = left + 'px';
    }}
    function show(el) {{
      var text = el.getAttribute('data-tip');
      if (!text) return;
      tip.textContent = text;
      tip.classList.add('visible');
      position(el);
    }}
    function hide() {{
      tip.classList.remove('visible');
    }}
    triggers.forEach(function(el) {{
      el.addEventListener('mouseenter', function() {{ show(el); }});
      el.addEventListener('mouseleave', hide);
      el.addEventListener('focus', function() {{ show(el); }});
      el.addEventListener('blur', hide);
    }});
  }})();
</script>
</body>
</html>"#
    )
}
