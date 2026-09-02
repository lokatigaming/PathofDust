// Viewer-facing web dashboard for the chat adventure game (see
// adventure.rs) — unlike public_adventure_overlay (OBS-only, no auth,
// pushed state), this is a real multi-user site: each viewer logs in
// with a local account (see accounts.rs) and sees/manages their own
// character, matched by their lowercased login (same id
// AdventureManager already keys characters by).
//
// Sessions are an opaque random token in an HttpOnly cookie, mapped to
// (login, display_name) in a store persisted to adventure-sessions.json
// so a deploy restart never forces a relog. Nothing here touches
// combat/loot logic directly; it only reads/joins through
// AdventureManager's existing public API.
//
// Twitch is GONE as of World 2 (2026-09-02): the OAuth login, the
// `/api/*` bot seam and the overlay's chat embed were all deleted, not
// merely unmounted. `accounts.rs::mint_session` is now the only session
// minter, and it is the whole seam an external identity provider
// replaces later. Identity itself never depended on Twitch - `Session`,
// the cookie, `current_session`, `Character` and
// adventure-characters.json are all provider-agnostic and always were.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Json, Redirect};
use axum::routing::{get, post};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::adventure::{
    affix_display, affix_name, affix_quality_percent, craft_affix_value_range, list_pinned_fights, recent_summary_fights, AdventureManager, Affix, Archetype,
    AutoDisenchantTier, BossKind, BugReportManager, Character, CraftAction, CraftError, CraftOutcome, CraftResult, DivineDustCraftError, DivineDustOutcome, DivinityError, DivinityReport, EncounterKind, EquipSlot, FightSummarySnapshot, GolemType, Item,
    LiveTunables, OperatorTriggerOutcome, PacingStatus, MemoryError, MemoryLoadReport, NameRejection, PassiveError, PassivePreview, PendingVeil,
    PendingVeilAction, RecombineError, RecombineOutcome, RecombineResult, ReforgeOutcome, SetGolemSlotTypeError, SetSecondaryArchetypeError, StatBreakdown, VeilCandidate,
    SubmitOutcome, VeilChosenOutcome,
    ALL_ARCHETYPES, ALL_SPRITES, ARCHETYPE_CHANGE_COST, BUG_REPORTS_PATH, INVENTORY_CAPACITY, LIFE_LEECH_CAP_PER_SEC, MAX_REPORT_LEN, MEMORY_NAME_MAX_LEN, MODEL_CHANGES_FREE_FOR_ALL, MODEL_CHANGE_COST,
    HIDEOUT_WARRIOR_STEPS, NICKNAME_MAX_LEN, PASSIVE_RESPEC_COST, RETREAT_REPAIR_DURATION, SUMMARY_FIGHTS_CAPACITY, TIER_CRAFT_DUST_COST,
    VEIL_EXTRA_COST, WEB_REFORGE_DUST_COST, WINGS_COST, scaled_base_cost,
};
use crate::adventure::default_memory_name;
use crate::adventure::passive_overrides;
use crate::passive_tree::{PassiveNode, PassiveTier};

mod accounts;
mod render;
mod wiki;

const SESSION_COOKIE: &str = "adv_session";
/// How long a login lasts before needing to sign in again.
const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

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
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    sessions_path: PathBuf,
    /// Local accounts - see `accounts.rs`. The only session minter since
    /// Twitch OAuth was removed. Persisted the same way `sessions` is, to
    /// a sibling file in the same directory.
    accounts: Arc<Mutex<HashMap<String, accounts::Account>>>,
    accounts_path: PathBuf,
    /// Player-submitted bug reports - see `adventure::bug_reports`, ported
    /// from the bot module that backed `!bugreport` before Twitch went
    /// away. Its own `Arc` with its own lock, like `AdventureManager`, so
    /// a submission never contends with the session or account maps.
    bugs: Arc<BugReportManager>,
    /// Failed-login counters, keyed by lowercased username - see
    /// `accounts::login_throttle_delay`. In memory only and deliberately
    /// NOT persisted: a restart clearing it is acceptable (deploys are
    /// rare and announced), and writing a file on every failed password
    /// would hand an unauthenticated caller a disk-write primitive.
    login_failures: Arc<Mutex<HashMap<String, accounts::LoginFailure>>>,
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
    adventure: Arc<AdventureManager>,
    sessions_path: PathBuf,
) -> anyhow::Result<std::net::SocketAddr> {
    // Resolved through `data_path` here, at the library entry point, rather
    // than by the caller (2026-08-29, Linux-readiness) - the exact shape
    // `AdventureManager::new` already uses for the three paths IT is handed.
    // Unset `GAME_DATA_DIR` makes this the identity, and an absolute path
    // (every test passes one, pointed at its own scratch dir) wins over any
    // base regardless, so no caller's behaviour moves. `accounts_path` is
    // derived from this below, so `adventure-accounts.json` follows for free.
    let sessions_path = crate::adventure::data_path(sessions_path.to_string_lossy().as_ref());
    let sessions: HashMap<String, Session> = crate::state::load_json(&sessions_path).unwrap_or_default();
    let accounts_path = accounts::accounts_path(&sessions_path);
    let accounts: HashMap<String, accounts::Account> = crate::state::load_json(&accounts_path).unwrap_or_default();
    let state = AppState {
        adventure,
        sessions: Arc::new(Mutex::new(sessions)),
        sessions_path,
        accounts: Arc::new(Mutex::new(accounts)),
        accounts_path,
        // Same `data_path` resolution as `sessions_path` above, so reports
        // land beside the rest of the game state and every test's scratch
        // dir gets its own file rather than sharing production's.
        bugs: BugReportManager::new(crate::adventure::data_path(BUG_REPORTS_PATH)),
        login_failures: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = axum::Router::<AppState>::new()
        .route("/", get(index))
        .route("/inventory", get(inventory_page))
        // Local identity (2026-08-27) - the only session minter since the
        // Twitch OAuth login was removed. See accounts.rs.
        .route("/account/register", get(accounts::register_page).post(accounts::do_register))
        .route("/account/login", get(accounts::login_page).post(accounts::do_login))
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
        .route("/passives/set-golem-type", post(do_set_golem_slot_type))
        // Memories (2026-08-19) - saved passive-tree builds, see
        // `render_memories_section` and `AdventureManager::load_memory`.
        .route("/passives/memories/save", post(do_save_memory))
        .route("/passives/memories/load", post(do_load_memory))
        .route("/passives/memories/rename", post(do_rename_memory))
        .route("/passives/memories/delete", post(do_delete_memory))
        .route("/patch-notes", get(patch_notes))
        .route("/wiki", get(wiki::wiki_page))
        .route("/wiki/passives", get(wiki::wiki_passives_page))
        .route("/wiki/:page", get(wiki::wiki_dynamic_page))
        .route("/characters", get(character_list))
        .route("/characters/:login", get(character_detail))
        .route("/characters/:login/passives", get(character_passives_readonly))
        .route("/fights", get(fights_page))
        .route("/fights.json", get(fights_json))
        .route("/admin/tunables", get(admin_tunables_page))
        .route("/admin/tunables/save", post(do_save_tunables))
        // Live-tunable passive VALUES (2026-08-19) - see
        // `render_admin_passives_page` and `adventure::passive_overrides`.
        // Same `ADMIN_TUNABLES_LOGIN` gate as the page above, applied to
        // the read and both writes.
        // The one web operator control (2026-08-28) - see
        // `do_ops_next_encounter`. Same `ADMIN_TUNABLES_LOGIN` gate as
        // the pages above; unlike them it reports every refusal.
        .route("/admin/ops/next-encounter", post(do_ops_next_encounter))
        // Player-submitted bug reports (2026-09-02). `/bugs` is the
        // player-facing form, reachable from `top_nav`; `/admin/bugs` is
        // the operator's read-back, behind the same `ADMIN_TUNABLES_LOGIN`
        // gate as every other `/admin/*` page here.
        .route("/bugs", get(bugs_page).post(do_submit_bug))
        .route("/admin/bugs", get(admin_bugs_page))
        .route("/admin/passives", get(admin_passives_page))
        .route("/admin/passives/save", post(do_save_passive_override))
        .route("/admin/passives/revert", post(do_revert_passive_override))
        .route("/fights/:seq/members/:member", get(bundle_member))
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

    // LOOPBACK ONLY, unconditionally, and deliberately not configurable
    // (2026-09-02). This was `0.0.0.0` until today.
    //
    // Nothing needs to reach this listener from off-box: ingress is a
    // Cloudflare Tunnel and `cloudflared` runs here, dialling
    // `http://localhost:4005` (docs/linux_ingress.md), so loopback is
    // functionally identical to the old bind for every real caller.
    //
    // Why it changed. The host firewall drops non-loopback traffic to
    // 4004/4005 (docs/linux_staging.md), which is correct today - but it
    // is a deny-list on a chain whose policy is `accept`, naming two
    // LITERAL ports, while the ports themselves are env-configurable
    // (`ADVENTURE_WEB_PORT`, `ADVENTURE_OVERLAY_SERVER_PORT`, both read
    // in main.rs and set in the systemd unit). Change that env var - a
    // config edit, no rebuild, no code review - and the nftables rule
    // silently stops matching what it was protecting. The dashboard goes
    // public with no error and no log line. Two layers that disagree
    // about which ports matter, only one of which tracks the change.
    //
    // The firewall REMAINS the outer layer. This is defence in depth, not
    // a replacement for it: do not remove those nftables rules on the
    // strength of this line.
    //
    // The one thing that would justify making this configurable: wanting
    // to reach the dashboard from the LAN without going through the
    // tunnel. Nothing wants that today, and a knob nobody needs is just
    // one more thing to misconfigure back to `0.0.0.0`.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    // Read BEFORE spawning/moving the listener - callers that bind to
    // port 0 (an ephemeral port, e.g. Stage 1.5's HTTP golden-response
    // harness spinning up a disposable instance) need the OS-assigned
    // port back, since nothing else reports it. A caller that already
    // knows its own fixed `port` (main.rs today) just gets the same
    // value back wrapped in a SocketAddr - harmless, and one return type
    // serves both cases instead of two divergent server-startup paths.
    let bound_addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Adventure web dashboard server crashed: {err}");
        }
    });

    tracing::info!("Adventure character dashboard running on port {port}.");
    Ok(bound_addr)
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
    /// Set alongside `disenchanted` when the item was Sacred and the
    /// Divine Dust roll hit - see `DisenchantOutcome::divine_dust`.
    /// `None`/0 for every ordinary disenchant.
    divine_dust: Option<u64>,
    /// Set by `do_craft`/`do_craft_divine_dust_batch` after the Divine
    /// Dust craft recipe actually grants at least 1 - see
    /// `render_divine_dust_craft_popup`. Own marker/amount field, distinct
    /// from `crafted`/`tier` above (the recipe has no item/slot/tier at
    /// all) - reuses `change` for the same "(x{completed} of {times} —
    /// ran out)" batch-shortfall prefix `do_craft_batch` already uses.
    divine_dust_crafted: Option<String>,
    divine_dust_amount: Option<u64>,
    /// Set by `do_craft` after a Divinity run - see
    /// `render_divinity_popup`. Own marker for the same reason
    /// `divine_dust_crafted` has one: the run has no single item, slot or
    /// tier, so `crafted`/`tier` can't describe it. Reuses `change` for the
    /// whole-run summary (`divinity_summary_text`).
    divinity_run: Option<String>,
}

async fn index(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<IndexParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, display_name)) => {
            let character = state.adventure.character(&login).await;
            let (used_this_hour, next_reset_ms) = state.adventure.reforge_status(&login).await;
            let popup = if params.reforged.is_some() { render_reforge_popup(&params) } else { String::new() };
            format!("{popup}{}", render_dashboard(&login, &display_name, character.as_ref(), used_this_hour, next_reset_ms, &state.adventure.live_tunables(), &state.adventure.recent_announcements()))
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
            let tunables = state.adventure.live_tunables();
            // World state, not character state - the Divine Dust recipe's
            // unlock is a group-wide one-way latch (see
            // `AdventureManager::divine_dust_recipe_unlocked`), so it has to
            // be resolved here where the manager is reachable and threaded
            // down into the crafting card.
            let divine_dust_unlocked = state.adventure.divine_dust_recipe_unlocked().await;
            let popup = if params.crafted.is_some() {
                render_craft_popup(&params)
            } else if params.craft_failed.is_some() {
                render_craft_error_popup(&params)
            } else if params.disenchanted.is_some() {
                render_disenchant_popup(&params)
            } else if params.divine_dust_crafted.is_some() {
                render_divine_dust_craft_popup(&params)
            } else if params.divinity_run.is_some() {
                render_divinity_popup(&params)
            } else {
                String::new()
            };
            format!("{popup}{}", render_inventory_page(&display_name, character.as_ref(), pending_veil.as_ref(), &tunables, divine_dust_unlocked))
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
            <button class=\"btn\" onclick=\"document.getElementById('craft-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
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
    // Only shown when the disenchanted item was Sacred AND the Divine
    // Dust roll actually hit - see `DisenchantOutcome::divine_dust`'s doc.
    let divine_dust = params.divine_dust.unwrap_or(0);
    let divine_dust_line = if divine_dust > 0 { format!("<p class=\"modal-tier\">✨ Also yielded {divine_dust} Divine Dust!</p>") } else { String::new() };
    format!(
        "<div class=\"modal-backdrop\" id=\"disenchant-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">\u{1f4a8}</div>\
            <h2>Disenchanted!</h2>\
            <p>Your <strong>{item}</strong> broke down into <strong>{dust} Dust</strong></p>\
            <p class=\"modal-tier\">{percent}% of the {dust_max} possible</p>\
            {divine_dust_line}\
            <button class=\"btn\" onclick=\"document.getElementById('disenchant-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
          </div>\
        </div>"
    )
}

/// Shown once after the Divine Dust craft recipe (`/craft`'s "Craft
/// Divine Dust" row) actually grants at least 1 - see `do_craft`/
/// `do_craft_divine_dust_batch`. Same popup pattern as
/// `render_disenchant_popup` above; `change` carries the x1/x10/x50
/// batch's own "(x{completed} of {times} — ran out)" shortfall prefix
/// when present, same convention `do_craft_batch` already uses.
fn render_divine_dust_craft_popup(params: &IndexParams) -> String {
    let amount = params.divine_dust_amount.unwrap_or(0);
    let change = escape_html(params.change.as_deref().unwrap_or(""));
    format!(
        "<div class=\"modal-backdrop\" id=\"divine-dust-craft-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">✨</div>\
            <h2>Crafted!</h2>\
            <p>Gained <strong>{amount} Divine Dust</strong></p>\
            <p class=\"modal-tier\">{change}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('divine-dust-craft-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
          </div>\
        </div>"
    )
}

/// Shown once after a Divinity run (see `do_craft`'s `"divinity"` branch
/// and `divinity_popup_url`). Same POST-redirect-GET popup pattern as
/// `render_divine_dust_craft_popup` above; `change` carries the whole-run
/// aggregate built by `divinity_summary_text`.
///
/// One aggregate line, no per-item log: a full bag is up to 150 items and
/// ~560 craft steps, and the run has already renamed everything it
/// Krangled to "From Divinity", so the bag itself is the detailed record
/// for anyone who wants one.
fn render_divinity_popup(params: &IndexParams) -> String {
    let change = escape_html(params.change.as_deref().unwrap_or(""));
    format!(
        "<div class=\"modal-backdrop\" id=\"divinity-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">\u{1F31F}</div>\
            <h2>Divinity</h2>\
            <p>Your bag has been remade.</p>\
            <p class=\"modal-tier\">{change}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('divinity-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">Nice!</button>\
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
            <button class=\"btn\" onclick=\"document.getElementById('craft-error-modal').remove(); history.replaceState(null, '', '/inventory'); document.getElementById('crafting-card')?.scrollIntoView({{behavior: 'smooth', block: 'start'}});\">OK</button>\
          </div>\
        </div>"
    )
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
    /// `Option`, not a bare required `String` (2026-08-19 live bugfix) -
    /// the Divine Dust craft recipe's own `<form>`
    /// (`render_divine_dust_recipe_row`) has no item involved at all and
    /// submits no `item_a` field whatsoever; a required field here made
    /// Axum's own `Form<CraftForm>` extractor reject that submission
    /// with a 422 ("missing field `item_a`") before `do_craft` ever ran,
    /// so the recipe was completely unusable in production despite
    /// passing every structural test (none of which POST a real,
    /// item-less form through the real extractor). Every action that
    /// DOES need an item validates `Some` itself in `do_craft`/
    /// `do_craft_batch` and errors cleanly via the same
    /// `craft_error_popup_url` every other craft failure already uses.
    #[serde(default)]
    item_a: Option<String>,
    #[serde(default)]
    item_b: String,
    action: String,
    /// A checkbox only shows up in the form body at all when checked -
    /// `#[serde(default)]` is what lets an unchecked box (the field
    /// simply absent) deserialize as `None` instead of a hard 422.
    #[serde(default)]
    veiled: Option<String>,
    /// x5/x10/x50 batch repeat count for Polishing/Reforge only (see the
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
                "/inventory?disenchanted=1&item={}&dust={}&dust_max={}&divine_dust={}",
                urlencoding::encode(&outcome.item_name),
                outcome.dust,
                outcome.dust_max,
                outcome.divine_dust,
            );
            return Redirect::to(&url);
        }
    }
    Redirect::to("/inventory")
}

/// Silent (no popup), same as every other web-only action here - the
/// now-empty(er) Bag and updated dust total on the next page load are
/// confirmation enough. Skips disenchant-protected items only (Krangled
/// items ARE included, 2026-08-18) - see
/// `Character::disenchant_all_from_inventory`.
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
    /// Set by `do_load_memory` when a Memory load DID apply but produced
    /// something different from what was saved - a class change, dropped
    /// stale allocations, a skipped 2nd tree. Deliberately distinct from
    /// `passive_failed`: this reports a success whose result the player
    /// should still be told about, not a rejected action. A load that
    /// applied cleanly sets nothing and redirects silently, same as
    /// every other action on this page.
    memory_note: Option<String>,
}

async fn passives_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<PassivesParams>) -> Html<String> {
    let session = current_session(&headers, &state).await;
    let body = match session {
        None => render_logged_out(),
        Some((login, display_name)) => {
            let character = state.adventure.character(&login).await;
            let preview = state.adventure.pending_passive_preview(&login).await;
            let popup = if params.passive_failed.is_some() {
                render_passive_error_popup(&params)
            } else if params.memory_note.is_some() {
                render_memory_note_popup(&params)
            } else {
                String::new()
            };
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
struct SetGolemSlotTypeForm {
    slot: usize,
    golem_type: GolemType,
}

/// Elementalist's Golem Master slot-type dropdown submit (docs/
/// elementalist_spec.md, Stage 5) - see
/// `AdventureManager::set_golem_slot_type`. Same silent-redirect-with-
/// popup-on-error shape as `do_set_secondary_archetype`.
async fn do_set_golem_slot_type(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SetGolemSlotTypeForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.set_golem_slot_type(&login, form.slot, form.golem_type).await {
            let reason = match err {
                SetGolemSlotTypeError::NotJoined => "You haven't joined the adventure yet.",
                SetGolemSlotTypeError::NotElementalist => "You need to be playing Elementalist to assign a golem type.",
                SetGolemSlotTypeError::SlotNotUnlocked => "Invest another point in Golem Master to unlock that slot.",
            };
            return Redirect::to(&passive_error_popup_url(reason));
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

// ---------------------------------------------------------------------
// Memories (2026-08-19) - saved passive-tree builds. Same
// POST-redirect-GET-with-popup-on-error shape as every other action on
// this page; `do_load_memory` adds the one thing nothing else here
// needed, a popup on SUCCESS (see `PassivesParams::memory_note`).
// ---------------------------------------------------------------------

/// Player-facing reason a Memory action didn't go through.
fn memory_error_text(err: MemoryError) -> String {
    match err {
        MemoryError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        MemoryError::SlotOutOfRange => "That Memory slot doesn't exist.".to_string(),
        MemoryError::SlotEmpty => "There's nothing saved in that Memory slot yet.".to_string(),
        MemoryError::InCombat => "You can't swap builds during a fight - try again once it's over.".to_string(),
        MemoryError::NoBuildToSave => "Pick an Archetype on your dashboard first - Commoner has no build to save.".to_string(),
        MemoryError::InvalidName(rejection) => match rejection {
            NameRejection::Empty => "Give your Memory a name first.".to_string(),
            NameRejection::TooLong => format!("That name is too long - {MEMORY_NAME_MAX_LEN} characters max."),
            NameRejection::Unprintable => "That name contains characters that aren't allowed.".to_string(),
            // Deliberately does NOT quote the offending word back at the
            // player or name which entry tripped: echoing it would put
            // exactly the string the filter exists to suppress back onto
            // the page (and into the URL).
            NameRejection::Blocked => "That name isn't allowed - please pick another.".to_string(),
        },
    }
}

/// Turns a load's `MemoryLoadReport` into the prose the note popup
/// shows. Only ever called when `is_noteworthy()` - a clean load says
/// nothing at all.
fn memory_load_note(report: &MemoryLoadReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    if report.class_changed {
        parts.push(format!("You're now playing {:?}.", report.archetype));
    }
    if report.secondary_skipped {
        parts.push("Your saved 2nd class tree wasn't applied - Split Personality isn't equipped any more.".to_string());
    }
    if !report.dropped.is_empty() {
        let total: u32 = report.dropped.iter().map(|d| d.rank).sum();
        let names: Vec<&str> = report.dropped.iter().map(|d| d.node_key.as_str()).collect();
        parts.push(format!(
            "{total} point{} refunded - {} couldn't be applied ({}).",
            if total == 1 { "" } else { "s" },
            if names.len() == 1 { "one node" } else { "some nodes" },
            names.join(", "),
        ));
    }
    parts.push(format!("You have {} unspent point{}.", report.unspent, if report.unspent == 1 { "" } else { "s" }));
    parts.join(" ")
}

/// Query string for `render_memory_note_popup`.
fn memory_note_popup_url(note: &str) -> String {
    format!("/passives?memory_note={}", urlencoding::encode(note))
}

/// Shown once after a Memory load that applied but produced something
/// different from what was saved - see `PassivesParams::memory_note`.
/// Same modal shape as `render_passive_error_popup`, worded as
/// information rather than failure.
fn render_memory_note_popup(params: &PassivesParams) -> String {
    let note = escape_html(params.memory_note.as_deref().unwrap_or_default());
    format!(
        "<div class=\"modal-backdrop\" id=\"memory-note-modal\">\
          <div class=\"modal\">\
            <div class=\"modal-icon\">\u{1F9E0}</div>\
            <h2>Memory Loaded</h2>\
            <p>{note}</p>\
            <button class=\"btn\" onclick=\"document.getElementById('memory-note-modal').remove(); history.replaceState(null, '', '/passives');\">OK</button>\
          </div>\
        </div>"
    )
}

#[derive(Deserialize)]
struct MemorySaveForm {
    slot: usize,
    /// The player's typed name. Empty means "use the default" - an empty
    /// text input is the natural way to say "I don't mind what it's
    /// called", not an error. Anything non-empty goes through
    /// `validate_memory_name` inside the manager.
    #[serde(default)]
    name: String,
}

async fn do_save_memory(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<MemorySaveForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        let name = if form.name.trim().is_empty() { None } else { Some(form.name.as_str()) };
        if let Err(err) = state.adventure.save_memory(&login, form.slot, name).await {
            return Redirect::to(&passive_error_popup_url(&memory_error_text(err)));
        }
    }
    Redirect::to("/passives")
}

#[derive(Deserialize)]
struct MemorySlotForm {
    slot: usize,
}

/// Unlike every other handler on this page, a SUCCESS here can still
/// warrant a popup: a load that changed class, dropped stale
/// allocations, or skipped a 2nd tree produced a build that differs
/// from the one saved, and silently swapping something else in would be
/// worse than saying so. A clean load redirects silently like the rest.
async fn do_load_memory(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<MemorySlotForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        match state.adventure.load_memory(&login, form.slot).await {
            Err(err) => return Redirect::to(&passive_error_popup_url(&memory_error_text(err))),
            Ok(report) if report.is_noteworthy() => return Redirect::to(&memory_note_popup_url(&memory_load_note(&report))),
            Ok(_) => {}
        }
    }
    Redirect::to("/passives")
}

#[derive(Deserialize)]
struct MemoryRenameForm {
    slot: usize,
    name: String,
}

async fn do_rename_memory(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<MemoryRenameForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.rename_memory(&login, form.slot, &form.name).await {
            return Redirect::to(&passive_error_popup_url(&memory_error_text(err)));
        }
    }
    Redirect::to("/passives")
}

async fn do_delete_memory(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<MemorySlotForm>) -> impl IntoResponse {
    if let Some((login, _)) = current_session(&headers, &state).await {
        if let Err(err) = state.adventure.delete_memory(&login, form.slot).await {
            return Redirect::to(&passive_error_popup_url(&memory_error_text(err)));
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
        "divine dust" => Some(CraftAction::DivineDust),
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

/// Query string for `render_divine_dust_craft_popup` - `change` carries
/// the same "(x{completed} of {times} — ran out)" batch-shortfall prefix
/// `do_craft_batch`'s own popups use, empty for a plain x1 craft.
fn divine_dust_craft_popup_url(amount: u64, change: &str) -> String {
    format!("/inventory?divine_dust_crafted=1&divine_dust_amount={amount}&change={}", urlencoding::encode(change))
}

/// One-line summary of a whole Divinity run - what its popup shows.
///
/// Aggregate, never per-item: a full bag is up to 150 items and ~600 craft
/// steps, and a per-item log of that is not something anyone reads. The
/// skip counts are stated separately and only when non-zero, because "19
/// already Krangled" and "19 you ticked Keep on" call for different
/// reactions from the player - and a zero of either is noise.
fn divinity_summary_text(report: &DivinityReport) -> String {
    let mut parts = vec![format!(
        "{} item{} crafted, {} Krangled",
        report.items_changed,
        if report.items_changed == 1 { "" } else { "s" },
        report.krangled
    )];
    if report.skipped_krangled > 0 {
        parts.push(format!("{} already Krangled, left alone", report.skipped_krangled));
    }
    if report.skipped_kept > 0 {
        parts.push(format!("{} marked Keep, left alone", report.skipped_kept));
    }
    if report.unchanged > 0 {
        parts.push(format!("{} had no eligible step", report.unchanged));
    }
    format!("{} \u{2014} {} craft steps in total, no dust spent.", parts.join(" \u{00B7} "), report.steps_applied)
}

fn divinity_popup_url(report: &DivinityReport) -> String {
    format!("/inventory?divinity_run=1&change={}", urlencoding::encode(&divinity_summary_text(report)))
}

/// Player-facing reason a Divinity run didn't start. Every one of these is
/// a refusal that cost nothing - the shard is only consumed once planning
/// has proved there is real work to do.
fn divinity_error_text(err: DivinityError) -> String {
    match err {
        DivinityError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        DivinityError::NoShard => "Divinity needs a Unique Shard \u{2014} you don't have one right now.".to_string(),
        DivinityError::EmptyBag => "Your bag is empty \u{2014} Divinity only works on bagged items, never equipped gear.".to_string(),
        DivinityError::NothingEligible => {
            "Every item in your bag is already Krangled or marked \u{1F512} Keep, so Divinity had nothing to work on \u{2014} your shard wasn't spent.".to_string()
        }
    }
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
    } else if let Some(unique) = outcome.unique_affix_added {
        // UniqueShard's picker (and the legacy CelestialShard path) -
        // this outcome has no `affix_added`/`affix_value` at all, so it
        // must be checked before the generic fallback below, which would
        // otherwise render the uninformative "Added a new modifier".
        format!("Granted {} — {}", unique.name(), unique.description())
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

/// One-line "what changed" for `CraftAction::DivineDust`'s popup - see
/// `DivineDustOutcome`.
fn divine_dust_outcome_change_text(outcome: &DivineDustOutcome) -> String {
    let new_line = format!("Sacred affix: {}", affix_display(outcome.new_affix, outcome.new_value));
    if outcome.became_sacred {
        format!("Became Sacred! {new_line}")
    } else {
        let old = outcome.old_affix.map(affix_name).unwrap_or("—");
        format!("{old} → {new_line}")
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
        RecombineError::ItemProtected => "That item is marked 🔒 Keep — untick Keep on its card first, since recombining consumes it.".to_string(),
    }
}

/// Player-facing reason a currency craft attempt didn't go through - see
/// `IndexParams::craft_failed`/`render_craft_error_popup`.
fn craft_error_text(err: CraftError) -> String {
    match err {
        CraftError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        CraftError::ItemNotFound => "Pick an item first — your selection was empty or no longer exists.".to_string(),
        CraftError::ItemLocked => "A Krangled (locked) item can't be crafted on.".to_string(),
        // Deliberately different wording from ItemLocked above: this one is
        // the player's own tick-box and they can undo it, so the message
        // says where to go. "Locked" would send them looking for a Krangle
        // they never did.
        CraftError::ItemProtected => "That item is marked 🔒 Keep — untick Keep on its card to craft on it.".to_string(),
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
        CraftError::InsufficientDivineDust(cost) => format!("Not enough Divine Dust — this needs {cost}."),
        CraftError::NoValidRerollTarget => "No other sacred affix is available to reroll into.".to_string(),
        CraftError::ConflictingUniqueAffix => {
            "You already have that unique effect equipped elsewhere — unequip it first, or apply the shard to an item in your bag instead.".to_string()
        }
    }
}

/// Player-facing reason the Divine Dust craft recipe didn't go through -
/// own error type/text fn from `craft_error_text` above (see
/// `DivineDustCraftError`'s own doc for why), same message wording
/// convention as `CraftError::InsufficientDust`/`InsufficientSand`.
fn divine_dust_craft_error_text(err: DivineDustCraftError) -> String {
    match err {
        DivineDustCraftError::NotJoined => "You haven't joined the adventure yet.".to_string(),
        DivineDustCraftError::InsufficientDust(cost) => format!("Not enough dust — this needs {cost}."),
        DivineDustCraftError::InsufficientSand(cost) => format!("Not enough sand — this needs {cost}."),
        DivineDustCraftError::Locked(stage) => {
            format!("The Divine Dust recipe unlocks when the group reaches stage {stage}. Once it does, it stays unlocked — a later stage loss can't take it away.")
        }
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
        // Every action below except the two that have no target item at
        // all - the Divine Dust recipe (a pure currency conversion) and
        // Divinity (which acts on the WHOLE bag) - needs a real item.
        // Validated once here rather than at each branch, since an absent
        // `item_a` (now `Option`, see the field's own doc) is the exact
        // same "nothing to act on" condition regardless of which of those
        // actions was requested.
        let item_a = if !matches!(form.action.as_str(), "divine dust craft" | "divinity") {
            match form.item_a.as_deref() {
                Some(id) => Some(id),
                None => return Redirect::to(&craft_error_popup_url("No item selected.")),
            }
        } else {
            None
        };
        if form.action == "recombine" {
            let item_a = item_a.expect("validated above - only \"divine dust craft\" skips this");
            match state.adventure.recombine_gear(&login, item_a, &form.item_b, veiled).await {
                Ok(RecombineResult::Applied(outcome)) => {
                    let change = recombine_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
                }
                Ok(RecombineResult::PendingChoice) => {}
                Err(err) => return Redirect::to(&craft_error_popup_url(&recombine_error_text(err))),
            }
        } else if form.action == "hideout warrior" {
            let item_a = item_a.expect("validated above - only the no-target actions skip this");
            return do_hideout_warrior(&state, &login, item_a, form.hideout_krangle.is_some()).await;
        } else if form.action == "divinity" {
            // Whole-bag action, no target item - a fourth string-matched
            // pseudo-action alongside "recombine"/"hideout warrior"/"divine
            // dust craft". Never batched: `times` is meaningless when the
            // unit of work is already "the entire bag", and the shard cost
            // is per USE by ruling, so a x10 would silently be ten shards.
            return match state.adventure.apply_divinity(&login).await {
                Ok(report) => Redirect::to(&divinity_popup_url(&report)),
                Err(err) => Redirect::to(&craft_error_popup_url(&divinity_error_text(err))),
            };
        } else if form.action == "divine dust craft" {
            // Currency-only recipe, no item involved at all - a third
            // string-matched pseudo-action alongside "recombine"/"hideout
            // warrior" above, rather than forcing it through
            // parse_craft_action/craft_item (which assume a target item -
            // see `DivineDustCraftError`'s own doc). Same x1/x10/x50
            // `times` field every other batchable action reads.
            let times = form.times.unwrap_or(1).clamp(1, 50);
            if times > 1 {
                return do_craft_divine_dust_batch(&state, &login, times).await;
            }
            match state.adventure.craft_divine_dust(&login).await {
                Ok(amount) => return Redirect::to(&divine_dust_craft_popup_url(amount, "")),
                Err(err) => return Redirect::to(&craft_error_popup_url(&divine_dust_craft_error_text(err))),
            }
        } else if let Some(action) = parse_craft_action(&form.action) {
            let item_a = item_a.expect("validated above - only \"divine dust craft\" skips this");
            // Only Polishing/Reforge get the x5/x10/x50 batch treatment
            // (see the dedicated section in render_crafting_card) - every
            // other action ignores `times` even if the hidden input still
            // carries a stale value from a prior checkbox selection.
            let times = if matches!(action, CraftAction::Polishing | CraftAction::Reforge) {
                form.times.unwrap_or(1).clamp(1, 50)
            } else {
                1
            };
            if times > 1 {
                return do_craft_batch(&state, &login, item_a, action, times).await;
            }
            match state.adventure.craft_item(&login, item_a, action, veiled).await {
                Ok(CraftResult::Applied(outcome)) => {
                    let change = craft_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
                }
                Ok(CraftResult::Reforged(outcome)) => {
                    let change = reforge_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
                }
                Ok(CraftResult::DivineDustApplied(outcome)) => {
                    let change = divine_dust_outcome_change_text(&outcome);
                    return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
                }
                Ok(CraftResult::PendingChoice) => {}
                Err(err) => return Redirect::to(&craft_error_popup_url(&craft_error_text(err))),
            }
        }
    }
    Redirect::to("/inventory")
}

/// Repeats a Polishing or Reforge craft against the same item `times` in
/// a row in one click (the x5/x10/x50 checkboxes) - never veiled (neither
/// action is veilable in the first place). Stops as soon as one
/// iteration errors (out of sand/dust, item vanished, etc.) rather than
/// silently eating the failure, and the popup reports how many of the
/// requested repeats actually landed vs. how many were asked for.
async fn do_craft_batch(state: &AppState, login: &str, item_id: &str, action: CraftAction, times: u32) -> Redirect {
    let mut completed = 0u32;
    let mut last_applied: Option<CraftOutcome> = None;
    let mut last_reforged: Option<ReforgeOutcome> = None;
    // DivineDust is never batch-eligible today (see the `times` gate in
    // `do_craft`, Polishing/Reforge only) - tracked anyway so this match
    // stays correct, not just exhaustive, if that ever changes.
    let mut last_divine_dust: Option<DivineDustOutcome> = None;
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
            Ok(CraftResult::DivineDustApplied(outcome)) => {
                completed += 1;
                last_divine_dust = Some(outcome);
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
    if let Some(outcome) = last_divine_dust {
        let change = format!("{prefix}{}", divine_dust_outcome_change_text(&outcome));
        return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
    }
    Redirect::to("/inventory")
}

/// Repeats the Divine Dust craft recipe `times` in a row (the x1/x10/x50
/// radios) - exact same stop-on-shortfall convention as `do_craft_batch`
/// above: stops as soon as one iteration errors (out of dust or sand),
/// whatever already landed stays applied (each iteration is its own
/// atomic dust+sand deduction, see `AdventureManager::craft_divine_dust`),
/// and the popup reports how many of the requested repeats actually
/// landed vs. how many were asked for.
async fn do_craft_divine_dust_batch(state: &AppState, login: &str, times: u32) -> Redirect {
    let mut completed = 0u32;
    let mut total: u64 = 0;
    let mut error: Option<DivineDustCraftError> = None;
    for _ in 0..times {
        match state.adventure.craft_divine_dust(login).await {
            Ok(amount) => {
                completed += 1;
                total += amount;
            }
            Err(err) => {
                error = Some(err);
                break;
            }
        }
    }
    if completed == 0 {
        let reason = error.map(divine_dust_craft_error_text).unwrap_or_else(|| "Nothing happened.".to_string());
        return Redirect::to(&craft_error_popup_url(&reason));
    }
    let prefix = if completed < times { format!("(x{completed} of {times} — ran out)") } else { format!("(x{completed})") };
    Redirect::to(&divine_dust_craft_popup_url(total, &prefix))
}


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
/// `do_craft_batch`'s existing x5/x10/x50 behavior). `include_krangle` (a
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
            // DivineDustApplied is unreachable here - DivineDust is never
            // one of HIDEOUT_WARRIOR_STEPS - but the match must stay
            // exhaustive regardless.
            Ok(CraftResult::PendingChoice) | Ok(CraftResult::Reforged(_)) | Ok(CraftResult::DivineDustApplied(_)) => {}
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
            Ok(Some(VeilChosenOutcome::Currency(outcome))) => {
                let change = craft_outcome_change_text(&outcome);
                return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.tier, &change));
            }
            Ok(Some(VeilChosenOutcome::Recombine(outcome))) => {
                let change = recombine_outcome_change_text(&outcome);
                return Redirect::to(&craft_popup_url(&outcome.item_name, outcome.slot, outcome.new_tier, &change));
            }
            // A veiled Chancing step was applied but more affix slots
            // remain this pass - a fresh PendingVeil for the next slot is
            // already inserted, so redirect back silently (same panel,
            // next slot) instead of showing a popup. See
            // `VeilChosenOutcome::ChancingContinues`'s own doc.
            Ok(Some(VeilChosenOutcome::ChancingContinues)) | Ok(None) => {}
            // The Unique Shard picker's own commit-time conflict rejection
            // (duplicate-unique-effects fix, 2026-08-21, bug #44) - same
            // "what went wrong" popup do_craft's insert-time rejections
            // already show, via the same CraftError message table.
            Err(err) => return Redirect::to(&craft_error_popup_url(&craft_error_text(err))),
        }
    }
    Redirect::to("/inventory")
}

#[derive(Deserialize, Default)]
struct PatchNoteSection {
    heading: String,
    items: Vec<String>,
    /// Optional illustrative image for a section - a path served by an
    /// already-mounted static route, NOT raw HTML - `items` stays plain
    /// text run through `escape_html` same as always. Absent
    /// (`#[serde(default)]`) on every existing entry, so old entries
    /// parse unchanged.
    #[serde(default)]
    image: Option<String>,
    /// Optional embedded interactive chart (2026-08-18, a live request -
    /// the crit-curve rebalance needed the SAME chart shown as an
    /// Artifact for review, not a hand-redrawn static approximation of
    /// it) - a path to a self-contained static HTML file served by the
    /// existing `/sprites` `ServeDir` mount, rendered as a sandboxed
    /// `<iframe>`. Only ever set by hand-edited entries in
    /// `patch-notes.json` (not user input), same trust boundary as
    /// `image` above - the src attribute is still escaped defensively.
    #[serde(default)]
    iframe: Option<String>,
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
    let entries: Vec<PatchNoteEntry> = crate::state::load_json(crate::adventure::data_path("patch-notes.json")).unwrap_or_default();
    let character = match current_session(&headers, &state).await {
        Some((login, _)) => state.adventure.character(&login).await,
        None => None,
    };
    Html(render_page(&render_patch_notes(&entries, character.as_ref())))
}

/// `?embed=app` (2026-08-18) - set by the private companion Electron app
/// when it iframes `/overlay` (see `overlay_page`'s own doc) - suppresses
/// the website-only settings tray, since the app provides its own
/// settings UI and would otherwise show two. Parsed server-side (not by
/// having the tray hide itself client-side) so a slow/failed script load
/// in the app's iframe can never leave the tray flashing visible first.
#[derive(Deserialize)]
struct OverlayPageParams {
    #[serde(default)]
    embed: Option<String>,
}

/// Compact, collapsible overlay-settings tray (2026-08-18) - injected
/// into the WEBSITE's copy of the overlay ONLY (never OBS's untouched
/// copy - see `adventure_overlay_server.rs`'s `serve_index` - and never
/// under `?embed=app`, gated by the caller). Lets a plain browser-tab
/// viewer set the same `?bgOpacity=`/`?bossSize=`/`?highlight=` params
/// `overlay.html` already reads, without hand-editing the URL - a
/// control rewrites `location.search` on `change` (not continuously)
/// and mirrors the choice into localStorage so it survives a reload/
/// revisit that carries none of these params at all. `own_login` is the
/// authenticated session's stable lowercase login (never a
/// free-text field) - `None` disables Highlight Me entirely rather than
/// letting a logged-out visitor type an arbitrary login.
fn render_overlay_settings_tray(own_login: Option<&str>) -> String {
    let own_login_js = match own_login {
        Some(login) => format!("{login:?}"),
        None => "null".to_string(),
    };
    let highlight_disabled_attr = if own_login.is_some() { "" } else { " disabled" };
    let highlight_hint = if own_login.is_some() {
        String::new()
    } else {
        "<p class=\"ov-tray-hint\">Log in to highlight your character.</p>".to_string()
    };
    let template = "\
<div id=\"overlay-settings-tray\" style=\"position:fixed;bottom:12px;left:12px;z-index:10000;font:13px 'Segoe UI',Arial,sans-serif;color:#fff;\">\
<style>\
#overlay-settings-tray button.ov-toggle{background:rgba(10,10,14,0.85);border:1px solid rgba(255,255,255,0.25);color:#fff;border-radius:6px;padding:6px 10px;cursor:pointer;font:inherit;}\
#overlay-settings-tray button.ov-toggle:focus-visible,#overlay-settings-tray input:focus-visible{outline:2px solid #7c5cff;outline-offset:2px;}\
#overlay-settings-body{background:rgba(10,10,14,0.9);border:1px solid rgba(255,255,255,0.25);border-radius:8px;padding:12px;margin-top:6px;min-width:220px;}\
#overlay-settings-body label{display:block;margin-bottom:10px;}\
#overlay-settings-body input[type=range]{width:100%;}\
.ov-switch{display:flex;align-items:center;gap:8px;cursor:pointer;}\
.ov-switch input[type=checkbox]{appearance:none;-webkit-appearance:none;width:36px;height:20px;border-radius:10px;background:rgba(255,255,255,0.25);position:relative;cursor:pointer;margin:0;flex:none;}\
.ov-switch input[type=checkbox]::after{content:'';position:absolute;top:2px;left:2px;width:16px;height:16px;border-radius:50%;background:#fff;transition:left 0.15s;}\
.ov-switch input[type=checkbox]:checked{background:#7c5cff;}\
.ov-switch input[type=checkbox]:checked::after{left:18px;}\
.ov-switch input[type=checkbox]:disabled{opacity:0.4;cursor:not-allowed;}\
.ov-tray-hint{margin:4px 0 0;opacity:0.75;font-size:12px;}\
</style>\
<button type=\"button\" class=\"ov-toggle\" id=\"overlay-settings-toggle\" aria-expanded=\"false\" aria-controls=\"overlay-settings-body\">&#9881; Overlay Settings</button>\
<div id=\"overlay-settings-body\" style=\"display:none;\">\
<label for=\"ov-bgop\">Background Opacity: <span id=\"ov-bgop-val\">100%</span>\
<input type=\"range\" id=\"ov-bgop\" min=\"0\" max=\"100\" step=\"5\" value=\"100\"></label>\
<label for=\"ov-bosssize\">Boss Size: <span id=\"ov-bosssize-val\">100%</span>\
<input type=\"range\" id=\"ov-bosssize\" min=\"5\" max=\"150\" step=\"5\" value=\"100\"></label>\
<label class=\"ov-switch\" for=\"ov-highlight\">\
<input type=\"checkbox\" id=\"ov-highlight\"__HIGHLIGHT_DISABLED__> Highlight Me</label>\
__HIGHLIGHT_HINT__\
</div>\
</div>\
<script>\
(function() {\
/* Overlay settings tray (2026-08-18) - reads/writes bgOpacity, bossSize,\
   and highlight the SAME way overlay.html's own top-of-script param\
   parsing does; a change here just rewrites location.search and lets\
   the normal page load pick the new values up, same as hand-editing\
   the URL always did. */\
var STORAGE_KEY = 'adventureOverlaySettings';\
var ownLogin = __OWN_LOGIN__;\
var qs = new URLSearchParams(location.search);\
var hasAnyTrayParam = qs.has('bgOpacity') || qs.has('bossSize') || qs.has('highlight');\
var stored = {};\
try { stored = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}'); } catch (err) {}\
/* Restore-on-fresh-load: only when NONE of the 3 tray params are\
   already in the URL - a URL that already specifies even one (a\
   shared/direct link) is respected as-is, never silently overridden by\
   a stored preference from a past visit. Computed target vs current\
   compared as strings so this can only ever navigate ONCE - the\
   reloaded page finds the same stored value, computes the same target,\
   sees no diff, and stops. */\
if (!hasAnyTrayParam && (stored.bgOpacity != null || stored.bossSize != null || stored.highlightMe)) {\
  var restore = new URLSearchParams(location.search);\
  if (stored.bgOpacity != null && stored.bgOpacity !== 100) restore.set('bgOpacity', String(stored.bgOpacity / 100));\
  if (stored.bossSize != null && stored.bossSize !== 100) restore.set('bossSize', String(stored.bossSize / 100));\
  if (stored.highlightMe && ownLogin) restore.set('highlight', ownLogin);\
  var target = restore.toString();\
  if (target !== qs.toString()) {\
    location.search = target;\
    return;\
  }\
}\
var toggle = document.getElementById('overlay-settings-toggle');\
var body = document.getElementById('overlay-settings-body');\
toggle.addEventListener('click', function() {\
  var open = body.style.display !== 'none';\
  body.style.display = open ? 'none' : 'block';\
  toggle.setAttribute('aria-expanded', String(!open));\
});\
var bgSlider = document.getElementById('ov-bgop');\
var bgLabel = document.getElementById('ov-bgop-val');\
var bossSlider = document.getElementById('ov-bosssize');\
var bossLabel = document.getElementById('ov-bosssize-val');\
var highlightBox = document.getElementById('ov-highlight');\
var initialBgOpacity = qs.has('bgOpacity') ? Math.round(parseFloat(qs.get('bgOpacity')) * 100) : 100;\
var initialBossSize = qs.has('bossSize') ? Math.round(parseFloat(qs.get('bossSize')) * 100) : 100;\
if (Number.isFinite(initialBgOpacity)) { bgSlider.value = String(initialBgOpacity); bgLabel.textContent = initialBgOpacity + '%'; }\
if (Number.isFinite(initialBossSize)) { bossSlider.value = String(initialBossSize); bossLabel.textContent = initialBossSize + '%'; }\
highlightBox.checked = !!ownLogin && qs.get('highlight') === ownLogin;\
function applyAndPersist() {\
  var bg = parseInt(bgSlider.value, 10);\
  var bs = parseInt(bossSlider.value, 10);\
  try {\
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ bgOpacity: bg, bossSize: bs, highlightMe: highlightBox.checked }));\
  } catch (err) {}\
  var target = new URLSearchParams(location.search);\
  var wasHighlightingMe = !!ownLogin && target.get('highlight') === ownLogin;\
  if (bg !== 100) target.set('bgOpacity', String(bg / 100)); else target.delete('bgOpacity');\
  if (bs !== 100) target.set('bossSize', String(bs / 100)); else target.delete('bossSize');\
  if (highlightBox.checked && ownLogin) target.set('highlight', ownLogin);\
  else if (wasHighlightingMe) target.delete('highlight');\
  var targetStr = target.toString();\
  if (targetStr !== new URLSearchParams(location.search).toString()) {\
    location.search = targetStr;\
  }\
}\
bgSlider.addEventListener('input', function() { bgLabel.textContent = bgSlider.value + '%'; });\
bgSlider.addEventListener('change', applyAndPersist);\
bossSlider.addEventListener('input', function() { bossLabel.textContent = bossSlider.value + '%'; });\
bossSlider.addEventListener('change', applyAndPersist);\
highlightBox.addEventListener('change', applyAndPersist);\
})();\
</script>";
    template
        .replace("__HIGHLIGHT_DISABLED__", highlight_disabled_attr)
        .replace("__HIGHLIGHT_HINT__", &highlight_hint)
        .replace("__OWN_LOGIN__", &own_login_js)
}

#[cfg(test)]
mod overlay_settings_tray_tests {
    use super::*;

    #[test]
    fn logged_out_disables_highlight_and_shows_the_login_hint() {
        let html = render_overlay_settings_tray(None);
        assert!(html.contains("id=\"ov-highlight\" disabled>"), "checkbox must carry a real `disabled` attribute when logged out");
        assert!(html.contains("Log in to highlight your character."));
        assert!(html.contains("var ownLogin = null;"));
        // No placeholder should ever survive substitution.
        assert!(!html.contains("__HIGHLIGHT_DISABLED__") && !html.contains("__HIGHLIGHT_HINT__") && !html.contains("__OWN_LOGIN__"));
    }

    #[test]
    fn logged_in_enables_highlight_and_omits_the_hint() {
        let html = render_overlay_settings_tray(Some("lokati_gaming"));
        assert!(html.contains("id=\"ov-highlight\">"), "checkbox must be enabled (no `disabled` attribute) when logged in");
        assert!(!html.contains("Log in to highlight your character."));
        assert!(html.contains("var ownLogin = \"lokati_gaming\";"));
    }

    #[test]
    fn own_login_is_escaped_for_safe_js_string_embedding() {
        // Twitch logins are actually restricted to [a-z0-9_] and can never
        // contain this, but `own_login` still flows into a raw JS string
        // literal via straight substitution - confirm the escaping (Rust's
        // `Debug` for `&str`) can't let a hostile value break out of the
        // quotes or inject a second statement, as defense in depth.
        let html = render_overlay_settings_tray(Some("mallory\";alert(1);//"));
        assert!(html.contains("var ownLogin = \"mallory\\\";alert(1);//\";"), "the quote must be backslash-escaped, not left to close the string early");
    }

    #[test]
    fn is_embed_app_matches_only_the_exact_value_app() {
        let is_embed_app = |embed: Option<&str>| -> bool {
            let params = OverlayPageParams { embed: embed.map(str::to_string) };
            params.embed.as_deref() == Some("app")
        };
        assert!(is_embed_app(Some("app")));
        assert!(!is_embed_app(Some("App")), "must be case-sensitive, not fuzzy-matched");
        assert!(!is_embed_app(Some("")));
        assert!(!is_embed_app(None), "the companion app's own iframe is the only ?embed=app source - absence means a plain browser visit");
    }

    #[test]
    fn tray_is_injected_immediately_after_body_open_exactly_once() {
        // Mirrors overlay_page's own injection: `format!("<body>{}", tray)`
        // then `patched.replacen("<body>", &tray_html, 1)` - confirmed here
        // against a minimal fixture rather than the full handler, which
        // needs a live AppState/disk read to construct at all.
        let fixture = "<html><head></head><body><canvas id=\"stage-back\"></canvas></body></html>";
        let tray = render_overlay_settings_tray(Some("alice"));
        let tray_html = format!("<body>{tray}");
        let patched = fixture.replacen("<body>", &tray_html, 1);
        assert_eq!(patched.matches("id=\"overlay-settings-tray\"").count(), 1);
        // The tray must land BEFORE the existing body content, and the
        // chat-panel injection point (`</body>`) must be untouched by this
        // substitution - the two injections must never collide.
        assert!(patched.find("overlay-settings-tray").unwrap() < patched.find("stage-back").unwrap());
        assert_eq!(patched.matches("</body>").count(), 1);
    }
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
async fn overlay_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<OverlayPageParams>) -> impl IntoResponse {
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
            let is_embed_app = params.embed.as_deref() == Some("app");
            let session = current_session(&headers, &state).await;
            // Settings tray (2026-08-18) - website only, never OBS's own
            // untouched copy, and never inside the companion app's
            // `?embed=app` iframe (it has its own settings UI - see
            // OverlayPageParams's own doc). Injected right after <body>
            // rather than at </body> like the chat panel below - it's
            // `position: fixed`, so DOM order doesn't matter for where it
            // renders, and doing it this way means the two injections
            // never fight over the same `</body>` anchor.
            if !is_embed_app {
                let own_login = session.as_ref().map(|(login, _)| login.as_str());
                let tray_html = format!("<body>{}", render_overlay_settings_tray(own_login));
                patched = patched.replacen("<body>", &tray_html, 1);
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
async fn overlay_ws_handler(ws: WebSocketUpgrade, Query(params): Query<crate::adventure_overlay_server::WsParams>, State(state): State<AppState>) -> impl IntoResponse {
    let compress = params.wants_compression();
    ws.on_upgrade(move |socket| crate::adventure_overlay_server::handle_socket(socket, state.adventure.clone(), compress))
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
                Some(c) => render_character_detail(&login, &c, viewer.as_ref(), &state.adventure.live_tunables()),
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

/// The login every operator gate falls back to when `OPERATOR_LOGIN` is
/// unset - the value all three constants below were hardcoded to before
/// they became configurable (2026-08-28). An unset environment therefore
/// leaves every gate behaving exactly as it did before that change.
const DEFAULT_OPERATOR_LOGIN: &str = "lokati_gaming";

/// The one `.env` key behind all three operator gates below. ONE key
/// rather than three because a typo in one of three would leave the
/// operator holding some admin surfaces and locked out of the rest - the
/// exact half-lockout World 2's Stage 3 gate exists to prevent
/// (docs/world2_build_plan.md, "HARD GATE - operator lockout"). The three
/// constants stay separate, so pointing one gate somewhere else later is
/// still a two-line change here.
///
/// Read through `dotenvy` like `GAME_DATA_DIR`
/// (see main.rs) - `main.rs`'s own `env_var` helper lives in the binary,
/// not this library, hence the local copy. Lowercased on the way in: every
/// gate compares against a session login, which is always lowercase.
///
/// Note this does NOT free `lokati_gaming` for registration when it points
/// elsewhere - that name is permanently reserved in accounts.rs, on its own,
/// independent of this key.
fn operator_login_from_env() -> String {
    std::env::var("OPERATOR_LOGIN").ok().map(|v| v.trim().to_ascii_lowercase()).filter(|v| !v.is_empty()).unwrap_or_else(|| DEFAULT_OPERATOR_LOGIN.to_string())
}

/// The streamer's own login still gets the full, unfiltered fight-history
/// list (see `fights_for_viewer`) - everything today's `/admin/tunables`-
/// style balance tuning is built around. Everyone else (2026-08-17, opened
/// up from a single-account-only page) sees the same page, scoped to just
/// the fights they personally took part in.
static FIGHTS_PAGE_LOGIN: LazyLock<String> = LazyLock::new(operator_login_from_env);

/// Operator tier for replay bundles. Its own constant rather than a reuse
/// of `FIGHTS_PAGE_LOGIN`/`ADMIN_TUNABLES_LOGIN`, following the precedent
/// those two set: same value today, but this one governs full roll logs
/// and must not silently follow a change made for an unrelated page.
static BUNDLE_OPERATOR_LOGIN: LazyLock<String> = LazyLock::new(operator_login_from_env);

/// Which tier a caller reads THIS fight at.
///
/// Participant is per-fight, not a global role: being a participant of one
/// fight grants nothing about another. Comparison is case-insensitive
/// because `participants` stores display names while a session stores the
/// lowercased Twitch login.
fn caller_tier_for(login: Option<&str>, participants: &[String]) -> crate::adventure::replay_bundle::Tier {
    match login {
        Some(login) if login.eq_ignore_ascii_case(BUNDLE_OPERATOR_LOGIN.as_str()) => crate::adventure::replay_bundle::Tier::Operator,
        Some(login) if participants.iter().any(|p| p.eq_ignore_ascii_case(login)) => {
            crate::adventure::replay_bundle::Tier::Participant
        }
        _ => crate::adventure::replay_bundle::Tier::Public,
    }
}

/// Serves one replay-bundle member.
///
/// THE serving boundary for bundle data, and the only route that reads a
/// bundle at all. Everything above `Public` in `crate::adventure::replay_bundle::MEMBER_TIERS`
/// is reachable only through here, and only after `may_read` says so -
/// the manifest DECLARES a tier, this ENFORCES it, and both read the same
/// table so they cannot disagree.
///
/// Denials are 404, not 403, matching how `/fights` and `/admin/tunables`
/// already hide restricted surfaces: a 403 would confirm that a given
/// fight exists and that it holds the member being asked for.
///
/// Note what is NOT here: no query parameter, header or cookie can widen
/// the caller's tier, and the public overlay socket has no path to this
/// code at all.
async fn bundle_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((seq, member)): Path<(u64, String)>,
) -> impl IntoResponse {
    // An unknown member name is refused before a byte is read from disk.
    if crate::adventure::replay_bundle::tier_of(&member).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let login = current_session(&headers, &state).await.map(|(login, _)| login);

    let Ok(Some(raw)) = tokio::task::spawn_blocking(move || crate::adventure::read_bundle_file(seq)).await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(bundle) = serde_json::from_str::<crate::adventure::replay_bundle::StoredBundle>(&raw) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let tier = caller_tier_for(login.as_deref(), &crate::adventure::replay_bundle::participants_of(&bundle));
    if !crate::adventure::replay_bundle::may_read(&member, tier) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match bundle.members.get(&member) {
        Some(body) => ([(header::CONTENT_TYPE, "application/json")], body.get().to_owned()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FightsPageParams {
    limit: Option<usize>,
}

/// Shared by `fights_page` (HTML) and `fights_json` (the companion app's
/// preferred format) so both read exactly the same rules - both render
/// from the summary tier (2026-08-18: `fights_page` used to read the
/// much smaller coarse tier while `fights_json` already read this same
/// summary tier, so a player could fight, then immediately see "no
/// recent fights" on the HTML page the moment the coarse tier's own
/// `COARSE_FIGHTS_CAPACITY`-fight window rolled past them, while
/// `/fights.json` still had them). The streamer's login gets
/// `recent_summary_fights(limit)` unfiltered, same as always; anyone
/// else gets the full currently-stored history fetched first (cheap -
/// summaries are a few KB each, unlike the coarse tier's full event
/// logs), filtered to fights they actually appear in (`players[].id`,
/// the same lowercase login used everywhere else - NOT `participants`,
/// which is just a count here, not display names), THEN capped at
/// `limit` - fetching-then-filtering (rather than filtering a
/// `recent_summary_fights(limit)` slice) is what actually answers "the
/// last N fights THEY were in," not "however many of the last N fights
/// overall happened to include them."
fn fight_summaries_for_viewer(login: &str, requested_limit: usize) -> Vec<FightSummarySnapshot> {
    let limit = requested_limit.clamp(1, SUMMARY_FIGHTS_CAPACITY);
    if login == *FIGHTS_PAGE_LOGIN {
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
            let is_streamer = login == *FIGHTS_PAGE_LOGIN;
            let limit = params.limit.unwrap_or(FIGHTS_PAGE_DISPLAY_LIMIT);
            // recent_summary_fights()'s per-fight-file read is real,
            // synchronous disk I/O (see its doc) - offloaded so it can't
            // stall this worker thread's own tokio runtime.
            let fights = tokio::task::spawn_blocking(move || fight_summaries_for_viewer(&login, limit)).await.unwrap_or_default();
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
/// markup. Same session cookie, same `fight_summaries_for_viewer` rules
/// (streamer unfiltered, everyone else scoped to their own fights), same
/// `?limit=`.
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
static ADMIN_TUNABLES_LOGIN: LazyLock<String> = LazyLock::new(operator_login_from_env);

/// The refusal both admin GET pages answer a non-operator with, and the
/// one all three admin POSTs answer since 2026-08-31 (ledger `#51`).
///
/// The BODY is deliberately a generic "Not Found" card rather than a
/// named 403 - a restricted page's existence isn't hinted at, and that
/// intent is correct. The STATUS was the bug: until 2026-08-31 this went
/// out as HTTP 200, so a status-code assertion on the refusal passed for
/// EVERYONE, and several deploy verifications asserted exactly that. The
/// body is byte-identical to what it always was; only the status is now
/// honest. Same shape `ops_result` has used since 2026-08-28.
fn admin_not_found() -> axum::response::Response {
    (StatusCode::NOT_FOUND, Html(render_page("<div class=\"card\"><h1>Not Found</h1></div>"))).into_response()
}

#[derive(Deserialize)]
struct AdminTunablesParams {
    saved: Option<String>,
}

// ---------------------------------------------------------------------
// `/admin/passives` (2026-08-19) - live-tunable passive VALUES. Same
// single-admin gate, same save-then-redirect shape as `/admin/tunables`
// above; the difference is that this page edits a SPARSE store, so
// every row also has to show what the compiled-in default was and offer
// a one-click revert back to it. See `adventure::passive_overrides`.
// ---------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(default)]
struct AdminPassivesParams {
    /// Which archetype's tree to edit. The tree is far too large to put
    /// all 471 nodes on one page (and a single giant form would make one
    /// bad input lose every other edit), so the page is scoped to one
    /// class at a time. Defaults to Warrior.
    class: Option<Archetype>,
    saved: Option<String>,
}

async fn admin_passives_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<AdminPassivesParams>) -> axum::response::Response {
    let session = current_session(&headers, &state).await;
    let body = match session {
        Some((login, _)) if login == *ADMIN_TUNABLES_LOGIN => {
            let viewer = state.adventure.character(&login).await;
            render_admin_passives_page(viewer.as_ref(), params.class.unwrap_or(Archetype::Warrior), params.saved.is_some(), state.adventure.live_tunables().overflow_conversion_cap_per_rank, None)
        }
        // Same generic fallback as `/admin/tunables` - a restricted
        // page's existence isn't hinted at. A real 404 since 2026-08-31.
        _ => return admin_not_found(),
    };
    Html(render_page(&body)).into_response()
}

/// One rejected or unconfirmed save, rendered back into the row it came
/// from (2026-08-27). An `error` is a value the consuming code cannot
/// use and was NOT persisted; a `warning` is a value it would accept but
/// that looks like a slip, so the row grows a "save anyway" form
/// carrying exactly what was typed - nothing is written until the
/// operator presses it. See `adventure::check_node_value`.
struct PassiveSaveFeedback {
    node_key: String,
    error: Option<String>,
    warning: Option<String>,
    /// What was typed, replayed on the confirm form so a warned save
    /// needs no retyping. `conversion_cap` is `None` for every row that
    /// doesn't render that input.
    pending: Option<(f64, f64, f64, Option<String>)>,
}

fn render_admin_passives_page(viewer: Option<&Character>, archetype: Archetype, saved: bool, global_conversion_cap: f64, feedback: Option<&PassiveSaveFeedback>) -> String {
    let nav = top_nav(viewer);
    let overrides = passive_overrides();
    let banner = if saved {
        "<p class=\"muted\">✅ Saved — live immediately, no restart. Takes effect on the very next fight.</p>"
    } else {
        ""
    };

    let class_links: String = ALL_ARCHETYPES
        .iter()
        .map(|&a| {
            let slug = format!("{a:?}").to_lowercase();
            let tuned = a.passive_nodes().iter().filter(|n| overrides.has_override(n.key)).count();
            let marker = if tuned > 0 { format!(" ({tuned})") } else { String::new() };
            let current = if a == archetype { " current" } else { "" };
            format!("<a class=\"passive-class-link{current}\" href=\"/admin/passives?class={slug}\">{a:?}{marker}</a>")
        })
        .collect();

    let rows: String = archetype
        .passive_nodes()
        .iter()
        .map(|n| {
            let key = n.key;
            let name = escape_html(n.name);
            let tier = match n.tier {
                PassiveTier::Skill => "Skill",
                PassiveTier::Specialization => "Specialization",
                PassiveTier::Modifier => "Modifier",
            };
            let not_yet = matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented);
            let pending = !crate::adventure::node_is_tunable(key);
            let overridden = overrides.has_override(key);

            let defaults: Vec<f64> = (1..=3).map(|r| n.magnitude_at_rank_with(r, &crate::adventure::PassiveOverrides::default())).collect();
            let current: Vec<f64> = (1..=3).map(|r| n.magnitude_at_rank(r)).collect();
            let default_text = defaults.iter().map(|v| trim_float(*v)).collect::<Vec<_>>().join(" / ");

            // A node whose mechanic doesn't exist yet, or whose numbers
            // still live in combat.rs, is shown but NOT offered - an
            // input that silently does nothing is worse than no input.
            // What the row's three numbers MEAN, read off the code that
            // consumes them (2026-08-27) - the page used to print them
            // bare, which is how "45" meaning 45% got typed into a
            // fraction. `unit unconfirmed` is a real, rendered answer
            // rather than a guess. See `adventure::node_unit`.
            let unit = crate::adventure::node_unit(key);
            let unit_chip = format!("<span class=\"passive-unit\">{}</span>", unit.label());

            if not_yet || pending {
                let why = if not_yet {
                    "No mechanic yet — this node declares no value, so there is nothing to tune."
                } else {
                    // `pending` is true here, so a reason always exists.
                    // The fallback keeps this total rather than panicking
                    // if the two predicates ever drift apart.
                    crate::adventure::node_untunable_reason(key).unwrap_or("Not tunable yet.")
                };
                // A `NotYetImplemented` node declares no value at all, so
                // naming a unit for it would be inventing one; every other
                // disabled row does have declared numbers and says what
                // they are.
                let head_unit = if not_yet { String::new() } else { unit_chip.clone() };
                return format!(
                    "<div class=\"passive-row disabled\">\
                       <div class=\"passive-row-head\"><strong>{name}</strong> <code>{key}</code> <span class=\"passive-tier\">{tier}</span> {head_unit}</div>\
                       <div class=\"passive-default\">Default: {default_text}</div>\
                       <p class=\"tunable-hint\">{why}</p>\
                     </div>"
                );
            }

            let step = if crate::adventure::node_is_integer_count(key) { "1" } else { "any" };
            let inputs: String = (0..3)
                .map(|i| {
                    format!(
                        "<label class=\"passive-rank\">\
                           <span>Rank {rank}</span>\
                           <input type=\"number\" step=\"{step}\" name=\"r{rank}\" value=\"{value}\">\
                         </label>",
                        rank = i + 1,
                        value = trim_float(current[i]),
                    )
                })
                .collect();
            // The unit sits with the inputs, not just in the head, so the
            // operator reads "seconds" in the same glance as the box they
            // are typing into; the range is what the save path will
            // enforce, quoted from the same `node_range_text`.
            let range_text = escape_html(&crate::adventure::node_range_text(key));
            let unit_note = format!("<span class=\"passive-unit-note\"><strong>{}</strong> — {range_text}</span>", unit.label());

            let marker = if overridden { "<span class=\"passive-tuned-badge\">differs from default</span>" } else { "" };
            // Half-tunable nodes (PARTIALLY_TUNABLE_NODES): the input below
            // genuinely works for the node's PRIMARY value, but a secondary
            // aspect still reads node RANK in combat.rs - say so instead of
            // letting the row look fully honest.
            let partial = match crate::adventure::node_partial_tunable_note(key) {
                Some(note) => format!("<p class=\"tunable-hint\">⚠ Half-tunable — this row's inputs work, but {note}</p>"),
                None => String::new(),
            };
            // Per-node conversion-output cap - offered on the 13
            // OverflowConversion rows ONLY, right beside their magnitude.
            // Blank = follow the global Conversion Output Cap / Rank on
            // /admin/tunables (shown as the placeholder so the fallback is
            // visible, never guessed).
            let is_conversion = matches!(n.effect, crate::passive_tree::PassiveEffect::OverflowConversion { .. });
            let (conversion_input, conversion_hint) = if is_conversion {
                let input = match overrides.conversion_cap_for(key) {
                    Some(cap) => format!("<label class=\"passive-rank\"><span>Cap / rank</span><input type=\"number\" step=\"any\" name=\"conversion_cap\" value=\"{}\"></label>", trim_float(cap)),
                    None => format!("<label class=\"passive-rank\"><span>Cap / rank</span><input type=\"number\" step=\"any\" name=\"conversion_cap\" placeholder=\"{}\"></label>", trim_float(global_conversion_cap)),
                };
                let hint = format!(
                    "<p class=\"tunable-hint\">Cap / rank: the ceiling on THIS node's own converted output per invested rank. Blank follows the global Conversion Output Cap / Rank ({}).</p>",
                    trim_float(global_conversion_cap)
                );
                (input, hint)
            } else {
                (String::new(), String::new())
            };
            let revert = if overridden {
                format!(
                    "<form method=\"post\" action=\"/admin/passives/revert\" class=\"passive-revert\">\
                       <input type=\"hidden\" name=\"class\" value=\"{slug}\">\
                       <input type=\"hidden\" name=\"node_key\" value=\"{key}\">\
                       <button class=\"btn-sm btn-danger\" type=\"submit\">Revert</button>\
                     </form>",
                    slug = format!("{archetype:?}").to_lowercase(),
                )
            } else {
                String::new()
            };

            // A rejected or unconfirmed save from THIS row, replayed
            // inline (2026-08-27). The confirm form is deliberately
            // rendered AFTER the edit form: `save_form_field_names` in
            // `tests/admin_passives_http.rs` derives the POST body from
            // the FIRST save form in the row, and that has to stay the
            // ordinary one.
            let slug = format!("{archetype:?}").to_lowercase();
            let (row_error, row_warning) = match feedback.filter(|f| f.node_key == key) {
                Some(f) => {
                    let err = f.error.as_deref().map(|m| format!("<p class=\"passive-error\">⛔ Not saved — {}</p>", escape_html(m))).unwrap_or_default();
                    let warn = match (f.warning.as_deref(), f.pending.as_ref()) {
                        (Some(m), Some((r1, r2, r3, cap))) => {
                            let cap_field = match cap {
                                Some(text) => format!("<input type=\"hidden\" name=\"conversion_cap\" value=\"{}\">", escape_html(text)),
                                None => String::new(),
                            };
                            format!(
                                "<div class=\"passive-warn\">\
                                   <p>⚠ Not saved yet — {msg}</p>\
                                   <form method=\"post\" action=\"/admin/passives/save\" class=\"passive-confirm\">\
                                     <input type=\"hidden\" name=\"class\" value=\"{slug}\">\
                                     <input type=\"hidden\" name=\"node_key\" value=\"{key}\">\
                                     <input type=\"hidden\" name=\"r1\" value=\"{v1}\">\
                                     <input type=\"hidden\" name=\"r2\" value=\"{v2}\">\
                                     <input type=\"hidden\" name=\"r3\" value=\"{v3}\">\
                                     {cap_field}\
                                     <input type=\"hidden\" name=\"confirm\" value=\"1\">\
                                     <button class=\"btn-sm btn-danger\" type=\"submit\">Save anyway</button>\
                                   </form>\
                                 </div>",
                                msg = escape_html(m),
                                v1 = trim_float(*r1),
                                v2 = trim_float(*r2),
                                v3 = trim_float(*r3),
                            )
                        }
                        _ => String::new(),
                    };
                    (err, warn)
                }
                None => (String::new(), String::new()),
            };

            format!(
                "<div class=\"passive-row\">\
                   <div class=\"passive-row-head\"><strong>{name}</strong> <code>{key}</code> <span class=\"passive-tier\">{tier}</span> {unit_chip} {marker}</div>\
                   {partial}\
                   {conversion_hint}\
                   <div class=\"passive-default\">Default: {default_text}</div>\
                   <div class=\"passive-controls\">\
                     <form method=\"post\" action=\"/admin/passives/save\" class=\"passive-edit\">\
                       <input type=\"hidden\" name=\"class\" value=\"{slug}\">\
                       <input type=\"hidden\" name=\"node_key\" value=\"{key}\">\
                       {inputs}\
                       {conversion_input}\
                       <button class=\"btn-sm\" type=\"submit\">Save</button>\
                     </form>\
                     {revert}\
                   </div>\
                   <p class=\"tunable-hint\">{unit_note}</p>\
                   {row_error}\
                   {row_warning}\
                 </div>",
            )
        })
        .collect();

    format!(
        "{nav}\
        <div class=\"card\">\
          <h1>🎚️ Passive Values</h1>\
          <p class=\"muted\">Retune any passive node's numbers live — no rebuild, no restart, effective on the very next fight. \
          Structure (which nodes exist, their ranks and prerequisites) stays in code and isn't editable here.</p>\
          {banner}\
          <div class=\"passive-class-nav\">{class_links}</div>\
          <h2>{archetype:?}</h2>\
          <p class=\"tunable-hint\">Every node has three meaningful ranks — a Specialization's 4th point only unlocks its modifiers, so it reuses the rank 3 value. \
          Revert removes the override entirely and returns the node to its compiled-in numbers.</p>\
          {rows}\
        </div>"
    )
}

#[derive(Deserialize)]
struct PassiveOverrideForm {
    class: Archetype,
    node_key: String,
    r1: f64,
    r2: f64,
    r3: f64,
    /// Only the `OverflowConversion` rows (14 at this writing) render this
    /// input beside their magnitude; every other row's POST omits it entirely - hence
    /// `#[serde(default)]` per the house trap rule, so old field sets
    /// keep deserializing. `Some("")` is a blank field = follow the
    /// global cap again; `Some(text)` parses as the node's own cap.
    #[serde(default)]
    conversion_cap: Option<String>,
    /// `Some("1")` only on the "Save anyway" form a WARNED save grows
    /// (2026-08-27). The ordinary edit form never renders it, so it is
    /// `#[serde(default)]` per the house trap rule - a required field
    /// with no rendered input 422s every real browser save.
    #[serde(default)]
    confirm: Option<String>,
}

#[derive(Deserialize)]
struct PassiveRevertForm {
    class: Archetype,
    node_key: String,
}

/// Per-field range validation, derived from the same consuming code the
/// row's unit label is (2026-08-27). This SUPERSEDES the earlier ruling
/// that the page should take any finite number: "known key and finite"
/// let `45` - meaning 45 percent - persist into Payback's `0..=1` HP
/// threshold as an always-true one, silently and with nothing on the
/// page to say it had.
///
/// Three outcomes, and the middle one is the point:
/// * **Reject** where the consumer settles the bound - a probability
///   the sim clamps into `0..=1`, a count cast to `u32`, a duration
///   that cannot be negative. Nothing is written; the row says which
///   field and what range it wanted.
/// * **Warn + confirm** where the value is merely unusual but the code
///   WOULD accept it (a fraction above 1 - the percent slip, but also a
///   real `+150%`). Still nothing written until "Save anyway".
/// * **Save** otherwise, redirecting exactly as before.
///
/// See `adventure::check_node_value` for where each bound comes from.
async fn do_save_passive_override(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<PassiveOverrideForm>) -> axum::response::Response {
    let slug = format!("{:?}", form.class).to_lowercase();
    let redirect = || Redirect::to(&format!("/admin/passives?class={slug}&saved=1")).into_response();
    // Ledger `#51`, fixed 2026-08-31: a non-operator used to get the same
    // `?saved=1` redirect a real save gets, so the refusal was reported as
    // a success. Same generic 404 the GET pages answer with - it still
    // doesn't confirm the route exists, it just stops claiming it saved.
    let Some((login, _)) = current_session(&headers, &state).await else {
        return admin_not_found();
    };
    if login != *ADMIN_TUNABLES_LOGIN {
        return admin_not_found();
    }
    // Reject a key that isn't in the class being edited - the page never
    // generates one, so this only fires on a hand-crafted POST, and a
    // bad key would otherwise sit in the file forever matching nothing.
    if !form.class.passive_nodes().iter().any(|n| n.key == form.node_key) {
        return redirect();
    }

    // The per-node conversion cap (OverflowConversion rows render it
    // beside the magnitude; other rows' POSTs carry no such field, which
    // lands as None here). Blank clears any explicit cap - back to
    // following the global.
    let cap_text = form.conversion_cap.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let mut checks: Vec<crate::adventure::ValueCheck> = [("Rank 1", form.r1), ("Rank 2", form.r2), ("Rank 3", form.r3)]
        .into_iter()
        .map(|(field, value)| crate::adventure::check_node_value(&form.node_key, field, value))
        .collect();
    let cap_value = match cap_text {
        None => None,
        Some(text) => match text.parse::<f64>() {
            Ok(cap) => {
                checks.push(crate::adventure::check_conversion_cap(cap));
                Some(cap)
            }
            // Was a silent `tracing::warn!` and a discarded edit - the
            // same "accepted the save, changed nothing" shape this whole
            // change exists to kill.
            Err(_) => {
                checks.push(crate::adventure::ValueCheck::Reject(format!("Cap / rank on {} is not a number — got {text:?}.", form.node_key)));
                None
            }
        },
    };

    let feedback_page = |error: Option<String>, warning: Option<String>| async {
        let viewer = state.adventure.character(&login).await;
        let feedback = PassiveSaveFeedback {
            node_key: form.node_key.clone(),
            error,
            warning,
            pending: Some((form.r1, form.r2, form.r3, cap_text.map(str::to_string))),
        };
        let body = render_admin_passives_page(viewer.as_ref(), form.class, false, state.adventure.live_tunables().overflow_conversion_cap_per_rank, Some(&feedback));
        Html(render_page(&body)).into_response()
    };

    if let Some(message) = checks.iter().find_map(|c| match c {
        crate::adventure::ValueCheck::Reject(m) => Some(m.clone()),
        _ => None,
    }) {
        return feedback_page(Some(message), None).await;
    }
    if form.confirm.as_deref() != Some("1") {
        if let Some(message) = checks.iter().find_map(|c| match c {
            crate::adventure::ValueCheck::Warn(m) => Some(m.clone()),
            _ => None,
        }) {
            return feedback_page(None, Some(message)).await;
        }
    }

    let mut overrides = passive_overrides();
    overrides.nodes.insert(form.node_key.clone(), vec![form.r1, form.r2, form.r3]);
    match cap_value {
        Some(cap) => {
            overrides.conversion_caps.insert(form.node_key.clone(), cap);
        }
        None => {
            overrides.conversion_caps.remove(&form.node_key);
        }
    }
    if let Err(err) = crate::adventure::save_passive_overrides(overrides) {
        tracing::error!("failed to persist passive overrides: {err}");
    }
    redirect()
}

async fn do_revert_passive_override(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<PassiveRevertForm>) -> axum::response::Response {
    let slug = format!("{:?}", form.class).to_lowercase();
    // Ledger `#51` - see `do_save_passive_override`.
    let Some((login, _)) = current_session(&headers, &state).await else {
        return admin_not_found();
    };
    if login != *ADMIN_TUNABLES_LOGIN {
        return admin_not_found();
    }
    let mut overrides = passive_overrides();
    overrides.revert(&form.node_key);
    if let Err(err) = crate::adventure::save_passive_overrides(overrides) {
        tracing::error!("failed to persist passive overrides: {err}");
    }
    Redirect::to(&format!("/admin/passives?class={slug}&saved=1")).into_response()
}

// ---------------------------------------------------------------------
// `/admin/ops/next-encounter` (2026-08-28) - the web equivalent of the
// bot's mod-only `!nextencounter`, built for Stage 2 of the standalone
// plan. The bot route (`api::next_encounter`) is untouched and keeps
// working; this is an ADDITION beside it, not a replacement.
//
// Gated by the same mechanism as `/admin/tunables` (`current_session` +
// a plain `ADMIN_TUNABLES_LOGIN` equality check) but deliberately NOT
// with the same response: the existing admin POSTs silently redirect a
// non-admin submission as though it worked, and a control that fires a
// fight against a live party must never do that. Every outcome here -
// refusal included - comes back as a visible page naming the exact
// condition, with a status code that matches it.
// ---------------------------------------------------------------------

#[derive(Deserialize)]
struct OpsNextEncounterForm {
    /// The boss select's value: one of `BossKind::FORCED_CHOICES`' keys,
    /// or empty for the "Random" option (the normal roll - exactly
    /// `!nextencounter` with no argument). `#[serde(default)]` per the
    /// house rule: a field an extractor REQUIRES but the page might not
    /// render is a 422 waiting to happen.
    #[serde(default)]
    boss: String,
}

/// One operator-control outcome, rendered as a page rather than a
/// redirect so the reason survives the round trip. `heading` is what
/// happened, `detail` says why - both are asserted on by
/// `admin_ops_next_encounter_http.rs`, so keep them distinct per outcome.
fn ops_result(status: StatusCode, heading: &str, detail: &str) -> axum::response::Response {
    let body = format!(
        "<div class=\"card\"><h1>{heading}</h1><p>{detail}</p><p><a href=\"/admin/tunables\">&larr; Back to tunables</a></p></div>"
    );
    (status, Html(render_page(&body))).into_response()
}

async fn do_ops_next_encounter(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<OpsNextEncounterForm>) -> axum::response::Response {
    let is_admin = matches!(current_session(&headers, &state).await, Some((login, _)) if login == *ADMIN_TUNABLES_LOGIN);
    if !is_admin {
        return ops_result(
            StatusCode::FORBIDDEN,
            "Refused - not the operator",
            "This control is operator-only and your session is not signed in as one. Nothing was triggered.",
        );
    }
    // Blank = the "Random" option = no forced boss, same as
    // `!nextencounter` with no argument.
    let picked = form.boss.trim();
    let forced = (!picked.is_empty()).then_some(picked);
    match state.adventure.operator_trigger_encounter(forced).await {
        OperatorTriggerOutcome::Triggered => {
            let what = match forced {
                Some(name) => format!("a forced {name} encounter"),
                None => "the next encounter".to_string(),
            };
            ops_result(StatusCode::OK, "Encounter triggered", &format!("Ran {what} right now. The result is announced through the usual channels."))
        }
        OperatorTriggerOutcome::Busy => ops_result(
            StatusCode::CONFLICT,
            "Refused - operator action already running",
            "Another operator trigger from this page is still running. Nothing was queued and no second fight will happen - wait for the first to finish and press it again if you still want one.",
        ),
        OperatorTriggerOutcome::FightInProgress => ops_result(
            StatusCode::CONFLICT,
            "Refused - a fight is in progress",
            "A fight is already running (the scheduled loop, a rampage, or the bot). Triggering now would have queued a bonus fight to start once that one finished, so it was refused instead. Nothing was queued.",
        ),
        OperatorTriggerOutcome::NobodyJoined => ops_result(
            StatusCode::CONFLICT,
            "Refused - nobody is eligible to fight",
            "No character is currently on the battlefield (everyone is downed, retreated, or nobody has joined). Nothing was triggered.",
        ),
        OperatorTriggerOutcome::UnknownBoss => ops_result(
            StatusCode::BAD_REQUEST,
            "Refused - unrecognized boss",
            "That boss name is not one this game knows. The select on the tunables page only ever offers names that work, so this means the request did not come from it. Nothing was triggered.",
        ),
    }
}

// ---------------------------------------------------------------------
// Bug reports (2026-09-02) - `/bugs` for players, `/admin/bugs` for the
// operator. Ported from the bot's `!bugreport` chat command, which went
// away with Twitch and took with it the only channel a player had for
// telling the owner something was broken.
//
// Reports go to a file beside the rest of the game state
// (`adventure-bugreports.json`) and are read back on an operator-gated
// page - the same shape `/admin/ops/next-encounter` established. No
// email, no external service, no new dependency: nothing here leaves the
// box.
//
// Login-required, reusing `current_session`. That is what names the
// reporter without asking them to type it (so it cannot be spoofed
// through the form) and it is the spam control, alongside
// `PER_USER_COOLDOWN`.
// ---------------------------------------------------------------------

/// How many reports `/admin/bugs` lists. Deliberately finite - the page
/// is a triage queue, not an archive; the JSON file keeps everything.
const ADMIN_BUGS_DISPLAY_LIMIT: usize = 200;

#[derive(Deserialize, Default)]
#[serde(default)]
struct BugsPageParams {
    /// Report id just filed, set by `do_submit_bug`'s redirect - renders
    /// the "report #N received" banner.
    filed: Option<u64>,
    /// A refusal to show instead, as a short key rather than prose so a
    /// crafted URL cannot put arbitrary text on the page.
    error: Option<String>,
    /// Seconds left, for `error=cooldown`.
    wait: Option<u64>,
}

#[derive(Deserialize)]
struct BugReportForm {
    /// `#[serde(default)]` per the house rule: a field the extractor
    /// requires but the page might not render is a 422 waiting to happen.
    /// An empty submission is a refusal with a message, not a 422.
    #[serde(default)]
    text: String,
}

fn render_bugs_page(character: Option<&Character>, params: &BugsPageParams) -> String {
    let banner = if let Some(id) = params.filed {
        format!("<div class=\"card\"><h2>\u{2705} Report #{id} received</h2><p>Thank you \u{2014} it is saved and the operator can read it. There is no reply channel here, so if it needs a conversation, say so in the report.</p></div>")
    } else {
        match params.error.as_deref() {
            Some("cooldown") => {
                let wait = params.wait.unwrap_or(0);
                format!("<div class=\"card\"><h2>Not sent \u{2014} too soon</h2><p>One report a minute, per person. Try again in {wait}s. Your text was not saved, so copy it before you leave this page.</p></div>")
            }
            Some("empty") => "<div class=\"card\"><h2>Not sent \u{2014} it was blank</h2><p>Nothing was saved. Describe what happened and send it again.</p></div>".to_string(),
            Some("toolong") => format!(
                "<div class=\"card\"><h2>Not sent \u{2014} too long</h2><p>The limit is {MAX_REPORT_LEN} characters. Nothing was saved, so trim it and send it again.</p></div>"
            ),
            _ => String::new(),
        }
    };
    format!(
        "{nav}\
        {banner}\
        <div class=\"card\">\
          <h1>\u{1F41E} Report a Bug</h1>\
          <p>Something behaving wrongly? Tell the owner here. Say what you did, what you expected, and what happened instead \u{2014} and name the item, passive or boss involved if there is one.</p>\
          <p class=\"muted\">Your name is taken from your login, so there is no need to sign it. One report a minute. This goes to a file the owner reads; nothing is sent anywhere else, and there is no automatic reply.</p>\
          <form method=\"post\" action=\"/bugs\">\
            <textarea name=\"text\" rows=\"10\" maxlength=\"{MAX_REPORT_LEN}\" placeholder=\"What happened?\" style=\"width:100%;box-sizing:border-box;\"></textarea>\
            <p><button class=\"btn\" type=\"submit\">Send report</button></p>\
          </form>\
        </div>",
        nav = top_nav(character),
    )
}

async fn bugs_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<BugsPageParams>) -> Html<String> {
    let body = match current_session(&headers, &state).await {
        // Login-gated the same way the rest of the dashboard is - and
        // here it is load-bearing rather than cosmetic, since the session
        // is what names the reporter and rate-limits them.
        None => render_logged_out(),
        Some((login, _)) => {
            let viewer = state.adventure.character(&login).await;
            render_bugs_page(viewer.as_ref(), &params)
        }
    };
    Html(render_page(&body))
}

async fn do_submit_bug(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<BugReportForm>) -> axum::response::Response {
    let Some((login, _)) = current_session(&headers, &state).await else {
        return Redirect::to("/account/login").into_response();
    };
    // Redirect back rather than rendering, so a refresh cannot re-file
    // the same report - the POST/redirect/GET the rest of this file uses
    // for every mutating action.
    let target = match state.bugs.submit(&login, &form.text).await {
        SubmitOutcome::Recorded { id } => format!("/bugs?filed={id}"),
        SubmitOutcome::OnCooldown { remaining_secs } => format!("/bugs?error=cooldown&wait={remaining_secs}"),
        SubmitOutcome::Empty => "/bugs?error=empty".to_string(),
        SubmitOutcome::TooLong { .. } => "/bugs?error=toolong".to_string(),
    };
    Redirect::to(&target).into_response()
}

async fn admin_bugs_page(State(state): State<AppState>, headers: HeaderMap) -> axum::response::Response {
    let is_admin = matches!(current_session(&headers, &state).await, Some((login, _)) if login == *ADMIN_TUNABLES_LOGIN);
    if !is_admin {
        // Same visible-refusal shape `do_ops_next_encounter` uses rather
        // than the silent redirect the older admin pages use: a page that
        // shows nothing is indistinguishable from a page with nothing on
        // it, and "are there no reports, or am I not signed in?" is
        // exactly the question this must not leave open.
        return ops_result(StatusCode::FORBIDDEN, "Refused - not the operator", "Bug reports are operator-only and your session is not signed in as one.");
    }
    let reports = state.bugs.recent(ADMIN_BUGS_DISPLAY_LIMIT).await;
    let rows = if reports.is_empty() {
        "<p class=\"muted\">No reports yet.</p>".to_string()
    } else {
        reports
            .iter()
            .map(|r| {
                format!(
                    "<div class=\"card\"><div class=\"header-row\"><h2>#{id}</h2><span class=\"muted\">{who} \u{00B7} {when}</span></div><pre style=\"white-space:pre-wrap;margin:0;\">{text}</pre></div>",
                    id = r.id,
                    who = escape_html(&r.user),
                    when = format_unix_secs(r.at_unix_secs as i64),
                    // Reports are player-written text rendered on the
                    // operator's own page - escaped, not trusted.
                    text = escape_html(&r.text),
                )
            })
            .collect::<String>()
    };
    let body = format!(
        "<div class=\"card\"><h1>\u{1F41E} Bug Reports</h1><p class=\"muted\">Newest first, most recent {ADMIN_BUGS_DISPLAY_LIMIT}. The full history is in {BUG_REPORTS_PATH}.</p></div>{rows}"
    );
    Html(render_page(&body)).into_response()
}

async fn admin_tunables_page(State(state): State<AppState>, headers: HeaderMap, Query(params): Query<AdminTunablesParams>) -> axum::response::Response {
    let session = current_session(&headers, &state).await;
    let body = match session {
        Some((login, _)) if login == *ADMIN_TUNABLES_LOGIN => {
            let viewer = state.adventure.character(&login).await;
            let current_pacing = state.adventure.current_pacing_status().await;
            render_tunables_page(viewer.as_ref(), &state.adventure.live_tunables(), current_pacing, params.saved.is_some(), None)
        }
        _ => return admin_not_found(),
    };
    Html(render_page(&body)).into_response()
}

/// Serde default for `TunablesForm::enemy_hp_pool_hard_cap` - the shipped
/// constant, so a POST that omits the field (an older client, or an
/// integration test posting a pre-existing field set) preserves live
/// behaviour instead of collapsing to 0.0.
fn default_enemy_hp_pool_hard_cap() -> f64 {
    crate::adventure::pacing::ENEMY_HP_POOL_HARD_CAP
}

/// Serde defaults for the two crafting-cost dials (2026-09-02). Same
/// reasoning as `default_enemy_hp_pool_hard_cap` above: `#[serde(default)]`
/// on an `f64` resolves to 0.0, which for the EXPONENT is below its 1.0
/// floor and for the MULTIPLIER would silently make every craft's base
/// fee free - so an omitted field resolves to the shipped constant
/// instead, never to a value nobody asked for.
fn default_craft_base_cost_mult() -> f64 {
    crate::adventure::CRAFT_BASE_COST_MULT
}
fn default_craft_tier_exponent() -> f64 {
    crate::adventure::CRAFT_TIER_EXPONENT
}
fn default_craft_tier_bump_mult() -> f64 {
    crate::adventure::CRAFT_TIER_BUMP_MULT
}

// Serde defaults for the five win-XP fields (2026-09-02). Each resolves
// to the SHIPPED CONSTANT, never `f64::default()` == 0.0 - this project
// has been bitten twice by a form field that silently zeroed, and on
// `win_xp_flat`/`win_xp_level_pct`/`win_xp_mult` a 0.0 is not merely
// wrong, it stops all XP in the game dead while the page still reports
// "Saved". `win_xp_catchup_enabled` is deliberately NOT in this list: it
// is a checkbox, and absent-means-unchecked is the whole protocol for a
// checkbox (see `TunablesForm::permanent_rampage`).
fn default_win_xp_flat() -> f64 {
    crate::adventure::WIN_XP_FLAT
}
fn default_win_xp_level_pct() -> f64 {
    crate::adventure::WIN_XP_LEVEL_PCT
}
fn default_win_xp_mult() -> f64 {
    crate::adventure::WIN_XP_MULT
}
fn default_win_xp_cooldown_secs() -> u64 {
    crate::adventure::WIN_XP_COOLDOWN_SECS
}

// Serde defaults for the four world-stage drop gates (2026-09-02). Same
// rule, and the same reason, as `default_enemy_hp_pool_hard_cap` above:
// `#[serde(default)]` on a `u32` resolves to 0, and 0 means "this gate is
// wide open at every stage" - the exact silent failure that would undo the
// whole feature while the page still reported a successful save. Each of
// these MUST resolve to the shipped constant.
fn default_sand_drop_stage() -> u32 {
    crate::adventure::SAND_STAGE_THRESHOLD
}
fn default_perfect_item_stage() -> u32 {
    crate::adventure::PERFECT_STAGE_THRESHOLD
}
fn default_divine_dust_drop_stage() -> u32 {
    crate::adventure::DIVINE_DUST_STAGE_THRESHOLD
}
fn default_sacred_item_stage() -> u32 {
    crate::adventure::SACRED_STAGE_THRESHOLD
}

// Serde defaults for the nine dynamic-pacing fields whose accepted range
// does NOT include zero (2026-08-31). `#[serde(default)]` on an `f64`
// resolves to 0.0, which is BELOW every one of these floors - so a body
// that omits them (an older client, or an integration test posting a
// pre-existing field set) used to have 0.0 silently clamped up to the
// floor, quietly overwriting live pacing config, and would now be
// rejected outright. Resolving to the SHIPPED CONSTANT instead is the
// same fix `default_enemy_hp_pool_hard_cap` above already applies, for
// the same reason: an omitted field must preserve sane behaviour, never
// 4xx and never collapse to a value nobody asked for.
fn default_pacing_window_fights() -> u32 {
    crate::adventure::pacing::defaults::PACING_WINDOW_FIGHTS
}
fn default_target_duration_min_s() -> f64 {
    crate::adventure::pacing::defaults::TARGET_DURATION_MIN_S
}
fn default_target_duration_max_s() -> f64 {
    crate::adventure::pacing::defaults::TARGET_DURATION_MAX_S
}
fn default_hp_multiplier_floor() -> f64 {
    crate::adventure::pacing::defaults::HP_MULTIPLIER_FLOOR
}
fn default_hp_multiplier_ceiling() -> f64 {
    crate::adventure::pacing::defaults::HP_MULTIPLIER_CEILING
}
fn default_target_win_loss_ratio() -> f64 {
    crate::adventure::pacing::defaults::TARGET_WIN_LOSS_RATIO
}
fn default_dmg_multiplier_floor() -> f64 {
    crate::adventure::pacing::defaults::DMG_MULTIPLIER_FLOOR
}
fn default_dmg_multiplier_ceiling() -> f64 {
    crate::adventure::pacing::defaults::DMG_MULTIPLIER_CEILING
}
fn default_top_layer_half_stage() -> f64 {
    crate::adventure::pacing::defaults::TOP_LAYER_HALF_STAGE
}

#[derive(Deserialize)]
struct TunablesForm {
    loot_mult: f64,
    sand_mult: f64,
    wings_drop_chance: f64,
    celestial_shard_drop_chance: f64,
    boss_health: f64,
    boss_power: f64,
    boss_count_tier_stages: u32,
    boss_count_cap_mult: f64,
    /// The four world-stage drop gates (2026-09-02). Each carries an
    /// explicit `#[serde(default = "...")]` resolving to its SHIPPED
    /// constant - see those fns' shared doc, and note that plain
    /// `#[serde(default)]` here would be a live-behaviour bug, not a
    /// stylistic choice. (`late_content_stage` was removed in the same
    /// change; the Perfect gate was its only remaining consumer and a dial
    /// that does nothing is worse than no dial at all.)
    #[serde(default = "default_sand_drop_stage")]
    sand_drop_stage: u32,
    #[serde(default = "default_perfect_item_stage")]
    perfect_item_stage: u32,
    #[serde(default = "default_divine_dust_drop_stage")]
    divine_dust_drop_stage: u32,
    #[serde(default = "default_sacred_item_stage")]
    sacred_item_stage: u32,
    /// See `LiveTunables::pierce_cap`'s doc.
    pierce_cap: f64,
    /// See `LiveTunables::pierce_h`'s doc.
    pierce_h: f64,
    /// See `LiveTunables::fight_summary_batch_size`'s doc.
    fight_summary_batch_size: u32,
    /// See `LiveTunables::thunder_redistribution_pct`'s doc.
    thunder_redistribution_pct: f64,
    /// See `LiveTunables::thunder_redistribution_window_secs`'s doc.
    thunder_redistribution_window_secs: f64,
    /// See `LiveTunables::reactive_proc_cap_ms`'s doc.
    reactive_proc_cap_ms: u32,
    /// See `LiveTunables::divine_dust_drop_chance`'s doc.
    divine_dust_drop_chance: f64,
    /// See `LiveTunables::divine_dust_disenchant_chance`'s doc.
    divine_dust_disenchant_chance: f64,
    /// See `LiveTunables::divine_dust_craft_dust_cost`'s doc.
    divine_dust_craft_dust_cost: u64,
    /// See `LiveTunables::divine_dust_craft_sand_cost`'s doc.
    divine_dust_craft_sand_cost: u64,
    /// See `LiveTunables::divine_dust_craft_output`'s doc.
    divine_dust_craft_output: u64,
    /// See `LiveTunables::craft_base_cost_mult`'s doc. `#[serde(default)]`
    /// resolves to the SHIPPED CONSTANT, never 0.0 - see
    /// `default_craft_base_cost_mult`.
    #[serde(default = "default_craft_base_cost_mult")]
    craft_base_cost_mult: f64,
    /// See `LiveTunables::craft_tier_exponent`'s doc. Same shipped-constant
    /// default - see `default_craft_tier_exponent`.
    #[serde(default = "default_craft_tier_exponent")]
    craft_tier_exponent: f64,
    /// See `LiveTunables::craft_tier_bump_mult`'s doc. Same
    /// shipped-constant default - see `default_craft_tier_bump_mult`.
    #[serde(default = "default_craft_tier_bump_mult")]
    craft_tier_bump_mult: f64,
    /// See `LiveTunables::rf_self_damage_pct_rank1`'s doc.
    rf_self_damage_pct_rank1: f64,
    /// See `LiveTunables::rf_self_damage_pct_rank2`'s doc.
    rf_self_damage_pct_rank2: f64,
    /// See `LiveTunables::rf_self_damage_pct_rank3`'s doc.
    rf_self_damage_pct_rank3: f64,
    /// See `LiveTunables::haloedsteps_per_instance_pct_rank1`'s doc.
    haloedsteps_per_instance_pct_rank1: f64,
    /// See `LiveTunables::haloedsteps_per_instance_pct_rank2`'s doc.
    haloedsteps_per_instance_pct_rank2: f64,
    /// See `LiveTunables::haloedsteps_per_instance_pct_rank3`'s doc.
    haloedsteps_per_instance_pct_rank3: f64,
    /// A checkbox only shows up in the form body at all when checked -
    /// same `#[serde(default)]`-as-absent convention every other checkbox
    /// on this dashboard already uses (see `CraftForm::veiled`).
    #[serde(default)]
    permanent_rampage: Option<String>,
    /// See `LiveTunables::win_xp_flat`'s doc. `#[serde(default = ...)]`
    /// resolves to the SHIPPED CONSTANT, not `f64::default()` - this
    /// field is consumed by the handler below, so an omitted one would
    /// otherwise zero it (CLAUDE.md's both-directions form-field trap),
    /// and a zero here stops all XP in the game.
    #[serde(default = "default_win_xp_flat")]
    win_xp_flat: f64,
    /// See `LiveTunables::win_xp_level_pct`'s doc. Shipped-constant
    /// default, same reasoning as `win_xp_flat` above.
    #[serde(default = "default_win_xp_level_pct")]
    win_xp_level_pct: f64,
    /// See `LiveTunables::win_xp_mult`'s doc. Shipped-constant default,
    /// same reasoning as `win_xp_flat` above.
    #[serde(default = "default_win_xp_mult")]
    win_xp_mult: f64,
    /// See `LiveTunables::win_xp_cooldown_secs`'s doc. Shipped-constant
    /// default, same reasoning as `win_xp_flat` above.
    #[serde(default = "default_win_xp_cooldown_secs")]
    win_xp_cooldown_secs: u64,
    /// See `LiveTunables::win_xp_catchup_enabled`'s doc - same
    /// absent-when-unchecked convention as `permanent_rampage` above, and
    /// for the same reason: a checkbox that is off sends nothing at all,
    /// so absent MUST read as false here rather than as the shipped
    /// default. That is the one field on this form where the
    /// shipped-constant rule does not apply, and it is safe because
    /// nothing else can produce the absent state.
    #[serde(default)]
    win_xp_catchup_enabled: Option<String>,
    /// See `LiveTunables::shattering_enabled`'s doc - same absent-when-
    /// unchecked convention as `permanent_rampage` above.
    #[serde(default)]
    shattering_enabled: Option<String>,
    /// See `LiveTunables::shattering_damage_pct_rank1`'s doc.
    shattering_damage_pct_rank1: f64,
    /// See `LiveTunables::shattering_damage_pct_rank2`'s doc.
    shattering_damage_pct_rank2: f64,
    /// See `LiveTunables::shattering_damage_pct_rank3`'s doc.
    shattering_damage_pct_rank3: f64,
    /// See `LiveTunables::defensive_stat_hard_cap`'s doc.
    defensive_stat_hard_cap: f64,
    /// See `LiveTunables::enemy_hp_pool_hard_cap`'s doc. `#[serde(default)]`
    /// resolves to the SHIPPED CONSTANT, not `f64::default()` - a 0.0 here
    /// would be clamped up to the floor rather than preserving live
    /// behaviour, and this field is consumed by the handler below (see
    /// CLAUDE.md on the both-directions form-field trap).
    #[serde(default = "default_enemy_hp_pool_hard_cap")]
    enemy_hp_pool_hard_cap: f64,
    /// See `LiveTunables::splash_extra_targets`'s doc.
    splash_extra_targets: u32,
    /// See `LiveTunables::splash_support_floor_targets`'s doc.
    splash_support_floor_targets: u32,
    /// See `LiveTunables::splash_overcap_bonus_targets`'s doc.
    splash_overcap_bonus_targets: u32,
    /// See `LiveTunables::splash_ladder_step_pct`'s doc.
    splash_ladder_step_pct: u32,
    /// See `LiveTunables::splash_ladder_targets_per_step`'s doc.
    splash_ladder_targets_per_step: u32,
    /// See `LiveTunables::splash_damage_pct`'s doc.
    splash_damage_pct: f64,
    /// See `LiveTunables::verdantburst_echo_threshold_pct`'s doc.
    verdantburst_echo_threshold_pct: f64,
    /// Stage 1 overflow-economy caps (2026-08-24). `#[serde(default)]` on
    /// every one of these per CLAUDE.md's BUILD & TEST rule - a POST body
    /// from an older page (or any test that forgets a field) must never
    /// 422 just because new dials shipped.
    #[serde(default)]
    overflow_conversion_cap_per_rank: f64,
    #[serde(default)]
    evasion_overflow_cap: f64,
    #[serde(default)]
    block_overflow_cap: f64,
    #[serde(default)]
    dr_overflow_cap: f64,
    #[serde(default)]
    intervene_overflow_cap: f64,
    /// See `LiveTunables::buffsnapshot_dedupe_window_ms`'s doc.
    buffsnapshot_dedupe_window_ms: u32,
    // ---- Dynamic pacing (2026-08-22) - every field #[serde(default)] so
    // the pre-existing fixed field set other tests post keeps saving fine.
    /// See `LiveTunables::dynamic_pacing_enabled`'s doc - checkbox,
    /// absent-when-unchecked convention.
    #[serde(default)]
    dynamic_pacing_enabled: Option<String>,
    /// See `LiveTunables::pacing_window_fights`'s doc.
    #[serde(default = "default_pacing_window_fights")]
    pacing_window_fights: u32,
    #[serde(default = "default_target_duration_min_s")]
    target_duration_min_s: f64,
    #[serde(default = "default_target_duration_max_s")]
    target_duration_max_s: f64,
    #[serde(default)]
    hp_max_step_per_fight: f64,
    #[serde(default = "default_hp_multiplier_floor")]
    hp_multiplier_floor: f64,
    #[serde(default = "default_hp_multiplier_ceiling")]
    hp_multiplier_ceiling: f64,
    /// See `LiveTunables::hp_relax_after_losses`'s doc.
    #[serde(default)]
    hp_relax_after_losses: u32,
    /// See `LiveTunables::hp_relax_step_per_fight`'s doc.
    #[serde(default)]
    hp_relax_step_per_fight: f64,
    #[serde(default = "default_target_win_loss_ratio")]
    target_win_loss_ratio: f64,
    #[serde(default)]
    dmg_max_step_per_fight: f64,
    #[serde(default = "default_dmg_multiplier_floor")]
    dmg_multiplier_floor: f64,
    #[serde(default = "default_dmg_multiplier_ceiling")]
    dmg_multiplier_ceiling: f64,
    /// Hand-authored baseline anchors, one CSV text input per axis
    /// (e.g. "0, 500, 1000" / "1.0, 0.92, 0.82"). Parsed by hand in
    /// `do_save_tunables`; a parse failure saves an EMPTY list, which the
    /// runtime validator reads as neutral (baseline 1.0) rather than
    /// corrupting anything.
    #[serde(default)]
    baseline_stage_anchors: String,
    #[serde(default)]
    baseline_hp_anchors: String,
    #[serde(default)]
    baseline_atk_anchors: String,
    /// See `LiveTunables::top_layer_enabled`'s doc - checkbox convention.
    #[serde(default)]
    top_layer_enabled: Option<String>,
    #[serde(default)]
    top_layer_cap_pct: f64,
    #[serde(default = "default_top_layer_half_stage")]
    top_layer_half_stage: f64,
    /// Manual override for Controller A's HP multiplier (see
    /// `AdventureManager::set_hp_pacing_mult`) - same blank-means-leave-
    /// it-alone String shape as the damage override below.
    #[serde(default)]
    hp_pacing_mult_override: String,
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

/// Every out-of-range value in one submitted `/admin/tunables` form
/// (2026-08-31, ledger `#69`).
///
/// Before this, EVERY numeric field on the page was clamped SILENTLY and
/// the page then reported `?saved=1` - the operator was told the save
/// succeeded while the number they typed was changed underneath them.
/// Rejection existed only as the browser's own `min`/`max`, which 7 of the
/// 48 clamped fields did not even carry: `loot_mult`, `sand_mult`,
/// `boss_health` and `boss_power` render no `min` at all against a server
/// floor of 0, and `hp_max_step_per_fight`, `hp_relax_step_per_fight` and
/// `dmg_max_step_per_fight` render no `max` against a server ceiling of
/// 100. A POST from anything that is not the page - curl, an older client
/// - bypassed all of it.
///
/// This is ONE pass over the whole form, not 48 separate checks. Each
/// bound is still declared exactly once, where its clamp already lived,
/// and now reports the change it would otherwise have made in silence.
/// The whole form is walked before anything is rejected, so the operator
/// learns about every typo in one round trip instead of one save per typo.
///
/// `clamping` is what keeps the clamp as defence-in-depth rather than the
/// primary bound. The pass runs TWICE: first collecting, returning values
/// exactly as typed so a rejected page can echo them back into the inputs;
/// then - only once nothing has been rejected - clamping, which is
/// provably a no-op at that point but keeps the backstop in place against
/// non-finite input and against any bound added later without a violation
/// path.
#[derive(Default)]
struct TunableViolations {
    items: Vec<String>,
    clamping: bool,
}

impl TunableViolations {
    /// A two-sided bound, e.g. a 0-1 chance.
    fn clamp(&mut self, field: &str, value: f64, min: f64, max: f64) -> f64 {
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number between {} and {}.", trim_float(min), trim_float(max)));
        } else if value < min || value > max {
            self.items.push(format!("{field} must be between {} and {} — got {}.", trim_float(min), trim_float(max), trim_float(value)));
        }
        if self.clamping && !value.is_finite() {
            min
        } else if self.clamping {
            value.clamp(min, max)
        } else {
            value
        }
    }

    /// A one-sided floor, e.g. a multiplier that may not go negative.
    fn at_least(&mut self, field: &str, value: f64, min: f64) -> f64 {
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number, {} or more.", trim_float(min)));
        } else if value < min {
            self.items.push(format!("{field} must be {} or more — got {}.", trim_float(min), trim_float(value)));
        }
        if self.clamping && !value.is_finite() {
            min
        } else if self.clamping {
            value.max(min)
        } else {
            value
        }
    }

    fn at_least_u32(&mut self, field: &str, value: u32, min: u32) -> u32 {
        if value < min {
            self.items.push(format!("{field} must be {min} or more — got {value}."));
        }
        if self.clamping {
            value.max(min)
        } else {
            value
        }
    }

    /// A two-sided whole-number bound, e.g. a world-stage drop gate. The
    /// `u32` counterpart of `clamp` - `at_least_u32` above covers the
    /// one-sided case only, and a gate needs a ceiling too: a fat-fingered
    /// stage would otherwise disable a drop forever with no complaint.
    fn range_u32(&mut self, field: &str, value: u32, min: u32, max: u32) -> u32 {
        if value < min || value > max {
            self.items.push(format!("{field} must be between {min} and {max} — got {value}."));
        }
        if self.clamping {
            value.clamp(min, max)
        } else {
            value
        }
    }

    fn at_least_u64(&mut self, field: &str, value: u64, min: u64) -> u64 {
        if value < min {
            self.items.push(format!("{field} must be {min} or more — got {value}."));
        }
        if self.clamping {
            value.max(min)
        } else {
            value
        }
    }

    /// A one-sided CEILING, for a field where 0 is a meaningful setting
    /// and only the top end needs a bound (today: `win_xp_cooldown_secs`,
    /// where 0 means "no throttle").
    fn at_most_u64(&mut self, field: &str, value: u64, max: u64) -> u64 {
        if value > max {
            self.items.push(format!("{field} must be {max} or less — got {value}."));
        }
        if self.clamping {
            value.min(max)
        } else {
            value
        }
    }

    /// The Enemy HP Pool Cap, which sanitises rather than plain-clamps -
    /// non-finite resolves to the SHIPPED DEFAULT so a NaN can never reach
    /// the division in `capped_hp_mult_for_pool`. `sanitize_pool_cap` is
    /// NOT dead once this rejects: `pacing.rs` calls it on every
    /// generation read of the cap, which is where it actually earns its
    /// keep.
    fn pool_cap(&mut self, field: &str, value: f64) -> f64 {
        let (min, max) = (crate::adventure::pacing::ENEMY_HP_POOL_CAP_MIN, crate::adventure::pacing::ENEMY_HP_POOL_CAP_MAX);
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number between {} and {}.", trim_float(min), trim_float(max)));
        } else if value < min || value > max {
            self.items.push(format!("{field} must be between {} and {} — got {}.", trim_float(min), trim_float(max), trim_float(value)));
        }
        if self.clamping {
            crate::adventure::pacing::sanitize_pool_cap(value)
        } else {
            value
        }
    }

    /// The two crafting-cost dials (2026-09-02), which sanitise rather
    /// than plain-clamp for the same reason `pool_cap` does: a non-finite
    /// reading resolves to the SHIPPED DEFAULT, so a NaN can never reach
    /// `craft::scaled_base_cost`/`tier_surcharge` and turn a price into
    /// NaN-cast-to-0. `sanitize_craft_base_cost_mult` is NOT dead once
    /// this rejects - the cost formula calls it on every craft, which is
    /// where it earns its keep.
    fn craft_base_cost_mult(&mut self, field: &str, value: f64) -> f64 {
        let (min, max) = (crate::adventure::CRAFT_BASE_COST_MULT_MIN, crate::adventure::CRAFT_BASE_COST_MULT_MAX);
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number between {} and {}.", trim_float(min), trim_float(max)));
        } else if value < min || value > max {
            self.items.push(format!("{field} must be between {} and {} — got {}.", trim_float(min), trim_float(max), trim_float(value)));
        }
        if self.clamping {
            crate::adventure::sanitize_craft_base_cost_mult(value)
        } else {
            value
        }
    }

    /// `craft_base_cost_mult`'s twin for the exponent.
    fn craft_tier_exponent(&mut self, field: &str, value: f64) -> f64 {
        let (min, max) = (crate::adventure::CRAFT_TIER_EXPONENT_MIN, crate::adventure::CRAFT_TIER_EXPONENT_MAX);
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number between {} and {}.", trim_float(min), trim_float(max)));
        } else if value < min || value > max {
            self.items.push(format!("{field} must be between {} and {} — got {}.", trim_float(min), trim_float(max), trim_float(value)));
        }
        if self.clamping {
            crate::adventure::sanitize_craft_tier_exponent(value)
        } else {
            value
        }
    }

    /// `craft_base_cost_mult`'s twin for the per-craft tier bump.
    fn craft_tier_bump_mult(&mut self, field: &str, value: f64) -> f64 {
        let (min, max) = (crate::adventure::CRAFT_TIER_BUMP_MULT_MIN, crate::adventure::CRAFT_TIER_BUMP_MULT_MAX);
        if !value.is_finite() {
            self.items.push(format!("{field} must be a number between {} and {}.", trim_float(min), trim_float(max)));
        } else if value < min || value > max {
            self.items.push(format!("{field} must be between {} and {} — got {}.", trim_float(min), trim_float(max), trim_float(value)));
        }
        if self.clamping {
            crate::adventure::sanitize_craft_tier_bump_mult(value)
        } else {
            value
        }
    }

    /// One comma-separated admin-page anchor list ("0, 500, 1000").
    /// Whitespace-tolerant. A malformed entry is now a REJECTION: it used
    /// to invalidate the whole list into an empty Vec, which the runtime
    /// validator reads as neutral - so one mistyped character silently
    /// deleted the entire hand-authored baseline floor and the page still
    /// said "Saved". That is data destruction on the pacing baseline, not
    /// a clamp. Blank stays a legitimate "no anchors".
    fn u32_list(&mut self, field: &str, raw: &str) -> Vec<u32> {
        if raw.trim().is_empty() {
            return Vec::new();
        }
        match raw.split(',').map(|piece| piece.trim().parse::<u32>()).collect::<Result<Vec<_>, _>>() {
            Ok(list) => list,
            Err(_) => {
                self.items
                    .push(format!("{field} must be a comma-separated list of whole numbers (e.g. \"0, 500, 1000\") — got \"{}\". Leave it blank to clear the list.", raw.trim()));
                Vec::new()
            }
        }
    }

    /// As `u32_list`, for the two decimal anchor axes.
    fn f64_list(&mut self, field: &str, raw: &str) -> Vec<f64> {
        if raw.trim().is_empty() {
            return Vec::new();
        }
        match raw.split(',').map(|piece| piece.trim().parse::<f64>()).collect::<Result<Vec<_>, _>>() {
            Ok(list) if list.iter().all(|v| v.is_finite()) => list,
            _ => {
                self.items
                    .push(format!("{field} must be a comma-separated list of numbers (e.g. \"1.0, 0.92, 0.82\") — got \"{}\". Leave it blank to clear the list.", raw.trim()));
                Vec::new()
            }
        }
    }
}

/// A refused `/admin/tunables` save, replayed onto the page it came from
/// (2026-08-31). Same "re-render with the values still in the boxes"
/// shape `/admin/passives` uses for a rejected node value - a 66-field
/// form that discards every other edit over one bad number is the exact
/// complaint that scoped the passives page per class in the first place.
struct TunablesRejection {
    /// Every violation in the submitted form, page order, all at once.
    violations: Vec<String>,
    /// The three CSV inputs exactly as typed. These cannot round-trip
    /// through `LiveTunables`' parsed `Vec`s, and a malformed list is
    /// precisely the case that has to come back for editing.
    stage_anchors: String,
    hp_anchors: String,
    atk_anchors: String,
}

/// Builds the `LiveTunables` a submitted form describes, recording every
/// out-of-range value into `v` as it goes. See `TunableViolations` for why
/// this runs twice.
fn tunables_from_form(form: &TunablesForm, previous: &LiveTunables, v: &mut TunableViolations) -> LiveTunables {
    {
        {
            // The retired `dynamic_scaling_mult` field is no longer on the
            // form - preserve whatever value is already live/on file so a
            // save never rewrites it to anything else.
            LiveTunables {
                loot_mult: v.at_least("loot_mult", form.loot_mult, 0.0),
                sand_mult: v.at_least("sand_mult", form.sand_mult, 0.0),
                wings_drop_chance: v.clamp("wings_drop_chance", form.wings_drop_chance, 0.0, 1.0),
                celestial_shard_drop_chance: v.clamp("celestial_shard_drop_chance", form.celestial_shard_drop_chance, 0.0, 1.0),
                boss_health: v.at_least("boss_health", form.boss_health, 0.0),
                boss_power: v.at_least("boss_power", form.boss_power, 0.0),
                dynamic_scaling_mult: previous.dynamic_scaling_mult,
                boss_count_tier_stages: v.at_least_u32("boss_count_tier_stages", form.boss_count_tier_stages, 1),
                boss_count_cap_mult: v.at_least("boss_count_cap_mult", form.boss_count_cap_mult, 0.0),
                // Defence in depth behind the form's own min/max, which is
                // what actually reports an out-of-range stage to the
                // operator in the browser; a hand-crafted POST has no such
                // gate. See `TunableViolations::range_u32`.
                sand_drop_stage: v.range_u32("sand_drop_stage", form.sand_drop_stage, crate::adventure::DROP_STAGE_MIN, crate::adventure::DROP_STAGE_MAX),
                perfect_item_stage: v.range_u32("perfect_item_stage", form.perfect_item_stage, crate::adventure::DROP_STAGE_MIN, crate::adventure::DROP_STAGE_MAX),
                divine_dust_drop_stage: v.range_u32("divine_dust_drop_stage", form.divine_dust_drop_stage, crate::adventure::DROP_STAGE_MIN, crate::adventure::DROP_STAGE_MAX),
                sacred_item_stage: v.range_u32("sacred_item_stage", form.sacred_item_stage, crate::adventure::DROP_STAGE_MIN, crate::adventure::DROP_STAGE_MAX),
                permanent_rampage: form.permanent_rampage.is_some(),
                // Defence-in-depth behind the form's own min/max, same as
                // the pool cap below: the browser is what REPORTS an
                // out-of-range value to the operator, and these re-check
                // it for a POST that never went through a browser.
                win_xp_flat: v.clamp("win_xp_flat", form.win_xp_flat, 0.0, crate::adventure::WIN_XP_FLAT_MAX),
                win_xp_level_pct: v.clamp("win_xp_level_pct", form.win_xp_level_pct, 0.0, crate::adventure::WIN_XP_LEVEL_PCT_MAX),
                win_xp_mult: v.clamp("win_xp_mult", form.win_xp_mult, crate::adventure::WIN_XP_MULT_MIN, crate::adventure::WIN_XP_MULT_MAX),
                // 0 is meaningful here - it is the deliberate "no
                // throttle, every win pays" setting - so this is a
                // ceiling check only, in the same spirit as
                // `hp_relax_after_losses` below.
                win_xp_cooldown_secs: v.at_most_u64("win_xp_cooldown_secs", form.win_xp_cooldown_secs, crate::adventure::WIN_XP_COOLDOWN_SECS_MAX),
                win_xp_catchup_enabled: form.win_xp_catchup_enabled.is_some(),
                shattering_enabled: form.shattering_enabled.is_some(),
                pierce_cap: v.clamp("pierce_cap", form.pierce_cap, 0.0, 1.0),
                pierce_h: v.at_least("pierce_h", form.pierce_h, 1.0),
                fight_summary_batch_size: v.at_least_u32("fight_summary_batch_size", form.fight_summary_batch_size, 1),
                thunder_redistribution_pct: v.clamp("thunder_redistribution_pct", form.thunder_redistribution_pct, 0.0, 1.0),
                thunder_redistribution_window_secs: v.at_least("thunder_redistribution_window_secs", form.thunder_redistribution_window_secs, 0.0),
                reactive_proc_cap_ms: form.reactive_proc_cap_ms,
                divine_dust_drop_chance: v.clamp("divine_dust_drop_chance", form.divine_dust_drop_chance, 0.0, 1.0),
                divine_dust_disenchant_chance: v.clamp("divine_dust_disenchant_chance", form.divine_dust_disenchant_chance, 0.0, 1.0),
                divine_dust_craft_dust_cost: form.divine_dust_craft_dust_cost,
                divine_dust_craft_sand_cost: form.divine_dust_craft_sand_cost,
                divine_dust_craft_output: v.at_least_u64("divine_dust_craft_output", form.divine_dust_craft_output, 1),
                // Defence-in-depth behind the rendered min/max, same shape
                // as `pool_cap` below - a hand-crafted POST is sanitised
                // rather than allowed to reach the cost formula.
                craft_base_cost_mult: v.craft_base_cost_mult("craft_base_cost_mult", form.craft_base_cost_mult),
                craft_tier_exponent: v.craft_tier_exponent("craft_tier_exponent", form.craft_tier_exponent),
                craft_tier_bump_mult: v.craft_tier_bump_mult("craft_tier_bump_mult", form.craft_tier_bump_mult),
                rf_self_damage_pct_rank1: v.clamp("rf_self_damage_pct_rank1", form.rf_self_damage_pct_rank1, 0.0, 1.0),
                rf_self_damage_pct_rank2: v.clamp("rf_self_damage_pct_rank2", form.rf_self_damage_pct_rank2, 0.0, 1.0),
                rf_self_damage_pct_rank3: v.clamp("rf_self_damage_pct_rank3", form.rf_self_damage_pct_rank3, 0.0, 1.0),
                haloedsteps_per_instance_pct_rank1: v.clamp("haloedsteps_per_instance_pct_rank1", form.haloedsteps_per_instance_pct_rank1, 0.0, 1.0),
                haloedsteps_per_instance_pct_rank2: v.clamp("haloedsteps_per_instance_pct_rank2", form.haloedsteps_per_instance_pct_rank2, 0.0, 1.0),
                haloedsteps_per_instance_pct_rank3: v.clamp("haloedsteps_per_instance_pct_rank3", form.haloedsteps_per_instance_pct_rank3, 0.0, 1.0),
                shattering_damage_pct_rank1: v.clamp("shattering_damage_pct_rank1", form.shattering_damage_pct_rank1, 0.0, 1.0),
                shattering_damage_pct_rank2: v.clamp("shattering_damage_pct_rank2", form.shattering_damage_pct_rank2, 0.0, 1.0),
                shattering_damage_pct_rank3: v.clamp("shattering_damage_pct_rank3", form.shattering_damage_pct_rank3, 0.0, 1.0),
                defensive_stat_hard_cap: v.clamp("defensive_stat_hard_cap", form.defensive_stat_hard_cap, 0.0, 1.0),
                // Defence-in-depth behind the form's own min/max (which is
                // what actually reports an out-of-range value to the
                // operator); a hand-crafted POST that bypasses the browser
                // is clamped rather than allowed to reach generation.
                enemy_hp_pool_hard_cap: v.pool_cap("enemy_hp_pool_hard_cap", form.enemy_hp_pool_hard_cap),
                splash_extra_targets: form.splash_extra_targets,
                splash_support_floor_targets: form.splash_support_floor_targets,
                splash_overcap_bonus_targets: form.splash_overcap_bonus_targets,
                splash_ladder_step_pct: form.splash_ladder_step_pct,
                splash_ladder_targets_per_step: form.splash_ladder_targets_per_step,
                splash_damage_pct: v.at_least("splash_damage_pct", form.splash_damage_pct, 0.0),
                verdantburst_echo_threshold_pct: v.at_least("verdantburst_echo_threshold_pct", form.verdantburst_echo_threshold_pct, 0.0),
                buffsnapshot_dedupe_window_ms: v.at_least_u32("buffsnapshot_dedupe_window_ms", form.buffsnapshot_dedupe_window_ms, 1),
                overflow_conversion_cap_per_rank: v.clamp("overflow_conversion_cap_per_rank", form.overflow_conversion_cap_per_rank, 0.0, 1.0),
                evasion_overflow_cap: v.clamp("evasion_overflow_cap", form.evasion_overflow_cap, 0.0, 1.0),
                block_overflow_cap: v.clamp("block_overflow_cap", form.block_overflow_cap, 0.0, 1.0),
                dr_overflow_cap: v.clamp("dr_overflow_cap", form.dr_overflow_cap, 0.0, 1.0),
                intervene_overflow_cap: v.clamp("intervene_overflow_cap", form.intervene_overflow_cap, 0.0, 1.0),
                dynamic_pacing_enabled: form.dynamic_pacing_enabled.is_some(),
                pacing_window_fights: v.at_least_u32("pacing_window_fights", form.pacing_window_fights, 1),
                target_duration_min_s: v.at_least("target_duration_min_s", form.target_duration_min_s, 0.001),
                target_duration_max_s: v.at_least("target_duration_max_s", form.target_duration_max_s, 0.001),
                hp_max_step_per_fight: v.clamp("hp_max_step_per_fight", form.hp_max_step_per_fight, 0.0, 100.0),
                hp_multiplier_floor: v.at_least("hp_multiplier_floor", form.hp_multiplier_floor, 0.001),
                hp_multiplier_ceiling: v.at_least("hp_multiplier_ceiling", form.hp_multiplier_ceiling, 0.001),
                // 0 is meaningful on BOTH of these and must survive the
                // save: on the loss count it reads as UNSET (pacing
                // substitutes the shipped default), and on the step it is
                // the deliberate off switch for relaxation. So neither is
                // floored here - only the typo backstops apply.
                hp_relax_after_losses: form.hp_relax_after_losses,
                hp_relax_step_per_fight: v.clamp("hp_relax_step_per_fight", form.hp_relax_step_per_fight, 0.0, 100.0),
                target_win_loss_ratio: v.at_least("target_win_loss_ratio", form.target_win_loss_ratio, 0.001),
                dmg_max_step_per_fight: v.clamp("dmg_max_step_per_fight", form.dmg_max_step_per_fight, 0.0, 100.0),
                dmg_multiplier_floor: v.at_least("dmg_multiplier_floor", form.dmg_multiplier_floor, 0.001),
                dmg_multiplier_ceiling: v.at_least("dmg_multiplier_ceiling", form.dmg_multiplier_ceiling, 0.001),
                baseline_stage_anchors: v.u32_list("baseline_stage_anchors", &form.baseline_stage_anchors),
                baseline_hp_anchors: v.f64_list("baseline_hp_anchors", &form.baseline_hp_anchors),
                baseline_atk_anchors: v.f64_list("baseline_atk_anchors", &form.baseline_atk_anchors),
                top_layer_enabled: form.top_layer_enabled.is_some(),
                // The hard 0.95 ceiling lives in pacing::top_layer_for_stage
                // at read time; this is just the typo backstop.
                top_layer_cap_pct: v.clamp("top_layer_cap_pct", form.top_layer_cap_pct, 0.0, 1.0),
                top_layer_half_stage: v.at_least("top_layer_half_stage", form.top_layer_half_stage, 1.0),
            }
        }
    }
}

/// Same admin gate as the GET page above - a POST from someone other than
/// `ADMIN_TUNABLES_LOGIN` gets the generic 404 (ledger `#51`).
///
/// An out-of-range value is REJECTED and reported, not clamped in silence
/// (2026-08-31, ledger `#69`). The whole form is validated in one pass, so
/// every offending field is named at once with the range it accepts, and
/// the page comes back with everything the operator typed still in the
/// boxes - the same shape `/admin/passives` uses for a rejected node
/// value. NOTHING is written when anything is rejected: this is all or
/// nothing, so a partial save can never leave the tunables in a state the
/// operator did not ask for. The clamp survives underneath as
/// defence-in-depth - see `TunableViolations`.
async fn do_save_tunables(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<TunablesForm>) -> axum::response::Response {
    // Ledger `#51` - see `do_save_passive_override`. A non-operator used
    // to get the same `?saved=1` redirect a real save gets.
    let Some((login, _)) = current_session(&headers, &state).await else {
        return admin_not_found();
    };
    if login != *ADMIN_TUNABLES_LOGIN {
        return admin_not_found();
    }

    let previous = state.adventure.live_tunables();
    // Pass one: collect. Values come back exactly as typed, so a rejected
    // page can echo them into the inputs rather than showing the operator
    // a number they never entered.
    let mut collected = TunableViolations::default();
    let candidate = tunables_from_form(&form, &previous, &mut collected);
    if !collected.items.is_empty() {
        let rejection = TunablesRejection {
            violations: collected.items,
            stage_anchors: form.baseline_stage_anchors.clone(),
            hp_anchors: form.baseline_hp_anchors.clone(),
            atk_anchors: form.baseline_atk_anchors.clone(),
        };
        let viewer = state.adventure.character(&login).await;
        let current_pacing = state.adventure.current_pacing_status().await;
        let body = render_tunables_page(viewer.as_ref(), &candidate, current_pacing, false, Some(&rejection));
        return (StatusCode::BAD_REQUEST, Html(render_page(&body))).into_response();
    }

    // Pass two: the same build with the clamps live. Provably a no-op here
    // - nothing was out of range - but it keeps the backstop in the code
    // path rather than deleting it.
    let mut clamping = TunableViolations { items: Vec::new(), clamping: true };
    let tunables = tunables_from_form(&form, &previous, &mut clamping);
    if let Err(err) = state.adventure.save_live_tunables(tunables) {
        tracing::error!("Failed to persist live tunables: {err}");
    }
    // Separate from the LiveTunables save above - these edit live WORLD
    // state (the two controllers' multipliers), not tunables fields. Blank
    // input (the normal case) leaves each untouched.
    if let Ok(value) = form.boss_power_mult_override.trim().parse::<f64>() {
        state.adventure.set_boss_power_mult(value).await;
    }
    if let Ok(value) = form.hp_pacing_mult_override.trim().parse::<f64>() {
        state.adventure.set_hp_pacing_mult(value).await;
    }
    Redirect::to("/admin/tunables?saved=1").into_response()
}

// ---- Rendering ----

/// Lost its `twitch: bool` parameter when the Twitch OAuth login was
/// removed (2026-09-02) - there is no second login path left to
/// conditionally offer, so local accounts are simply always the answer.
fn render_logged_out() -> String {
    "<div class=\"card\"><h1>Adventure Character Dashboard</h1>\
      <p>Log in to view and manage your adventure character.</p>\
      <p><a class=\"btn\" href=\"/account/login\">Log in</a> <a class=\"btn\" href=\"/account/register\">Register</a></p>\
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
                    let image = section
                        .image
                        .as_deref()
                        .map(|src| format!("<img src=\"{}\" alt=\"{}\" style=\"max-width:100%;height:auto;margin:8px 0;\">", escape_html(src), escape_html(&section.heading)))
                        .unwrap_or_default();
                    let iframe = section
                        .iframe
                        .as_deref()
                        .map(|src| {
                            format!(
                                "<iframe src=\"{}\" title=\"{}\" style=\"width:100%;max-width:800px;height:900px;border:none;border-radius:12px;margin:8px 0;display:block;\" sandbox=\"allow-scripts\" loading=\"lazy\"></iframe>",
                                escape_html(src),
                                escape_html(&section.heading)
                            )
                        })
                        .unwrap_or_default();
                    format!("<h3>{}</h3>{image}{iframe}<ul>{items}</ul>", escape_html(&section.heading))
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

fn render_combat_stats_card(c: &Character, tunables: &LiveTunables) -> String {
    // Only shown at all when nonzero (Slayer or a lucky Leech affix roll) -
    // 0% Leech on the other ten archetypes would just be dead space on
    // every other character's card.
    let leech = c.combat_life_leech(tunables);
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
    // characters won't have rolled any Echo gear, and 0% would just be
    // dead space on every other card.
    let echo_pct = c.combat_echo_pct(tunables);
    let echo_stat = if echo_pct > 0.0 {
        format!(
            "<div class=\"stat\"><div class=\"stat-label\" data-tip=\"Chance for your unified hit (damage or heal share) to fire again with fresh rolls. Past 100%, extra echoes become guaranteed: floor(value/100) guaranteed repeats plus a remainder% chance of one more - e.g. 250% is 2 guaranteed plus a 50% chance of a 3rd.\">Echo</div><div class=\"stat-value\">{echo_pct:.2}%</div></div>",
            echo_pct = echo_pct * 100.0,
        )
    } else {
        String::new()
    };
    let (dr_label, dr_value) =
        if c.combat_damage_reduction(tunables) >= 0.0 { ("Reduced Dmg Taken", c.combat_damage_reduction(tunables) * 100.0) } else { ("Increased Dmg Taken", -c.combat_damage_reduction(tunables) * 100.0) };
    let (dmg_label, dmg_value) =
        if c.combat_increased_damage(tunables) >= 0.0 {
            ("Increased Dmg Dealt", c.combat_increased_damage(tunables) * 100.0)
        } else {
            ("Reduced Dmg Dealt", -c.combat_increased_damage(tunables) * 100.0)
        };
    let crit_dmg_delta = (c.combat_crit_multiplier(tunables) - 1.0) * 100.0;
    let (crit_dmg_label, crit_dmg_value) = if crit_dmg_delta >= 0.0 { ("Increased Crit Dmg Dealt", crit_dmg_delta) } else { ("Reduced Crit Dmg Dealt", -crit_dmg_delta) };
    let dr_tip = stat_breakdown_tip(&c.damage_reduction_breakdown(tunables));
    let block_tip = stat_breakdown_tip(&c.block_breakdown(tunables));
    let evasion_tip = stat_breakdown_tip(&c.evasion_breakdown(tunables));
    let intervene_tip = stat_breakdown_tip(&c.intervene_breakdown(tunables));
    let dmg_tip = stat_breakdown_tip(&c.increased_damage_breakdown(tunables));
    // Past 100% Healing Power, the extra no longer inflates each heal's
    // own size (see `Character::combat_hps`'s doc) - it shortens the
    // action interval instead (`Character::attack_interval_ms`), so the
    // tooltip surfaces that actual cadence rather than leaving a
    // healer to wonder why HPS keeps climbing with no bigger heals to
    // show for it in the combat log. Only shown once it's actually
    // doing anything (heal power > 100%) - clutter for everyone else.
    let hps_tip = if c.combat_heal_power(tunables) > 1.0 {
        escape_html(&format!(
            "Average healing you do per second across a fight. Past 100% Healing Power, extra no longer makes each heal bigger - it makes you heal more often instead: you're currently acting every {interval}ms.",
            interval = c.combat_action_interval_ms(tunables),
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
            {echo_stat}\
          </div>\
        </div>",
        // Full max hp (base 20 + level*5, plus equipped body armor's
        // effective_power, times the archetype's max_hp_pct multiplier -
        // see Character::combat_max_hp) - scales with level the same way
        // every other combat stat here does, not the plain unscaled
        // Character::hp() base.
        max_hp = format_number(c.combat_max_hp(tunables) as f64),
        dps = format_number(c.combat_dps(tunables)),
        hps = format_number(c.combat_hps(tunables)),
        // .max(0.0) here is just a "-0%" float-rounding guard (these are
        // already floored at 0 mechanically) - cosmetic, not a real
        // negative-value case the way the four dynamic-label stats above are.
        block = (c.combat_block_chance(tunables) * 100.0).max(0.0),
        evasion = (c.combat_evasion(tunables) * 100.0).max(0.0),
        // Crit Chance is deliberately shown uncapped past 100% now (see
        // Character::combat_crit_chance's doc) - e.g. "250%" tells a
        // player they get a guaranteed double crit plus a 50% chance of
        // a third, not a display bug.
        crit_chance = (c.combat_crit_chance(tunables) * 100.0).max(0.0),
        splash = (c.combat_splash(tunables) * 100.0).max(0.0),
        // Healing Power is a plain 0%+ magnitude now, not a delta off a
        // universal 100% baseline (see Character::combat_heal_power's
        // doc - the baseline itself now varies: 0% off a Melee/Ranged
        // archetype, 100% off a Heal one) - same sign-free plain-number
        // treatment as Block/Evasion/Crit Chance/Splash above.
        heal_power = (c.combat_heal_power(tunables) * 100.0).max(0.0),
        intervene = (c.combat_intervene(tunables) * 100.0).max(0.0),
    )
}

/// One roster card's worth of context for `templates/characters.html` -
/// every field is already fully computed/escaped in Rust (game logic and
/// data stay in code per this project's own boundary; the template only
/// arranges already-final values into HTML structure).
#[derive(Serialize)]
struct RosterCardCtx {
    login_enc: String,
    sprite: String,
    name: String,
    level: u32,
    archetype: String,
    wins: String,
    losses: String,
    winrate: String,
}

/// Card grid for `/characters` - every character that's ever `!join`ed,
/// sorted by level desc (wins desc as a tiebreaker) so the roster reads
/// like a leaderboard. Each card links to `/characters/{login}` (see
/// `render_character_detail`). Empty state can't really happen on a live
/// server, but costs nothing to handle. Page STRUCTURE lives in
/// `templates/characters.html` (2026-08-18, Phase 1 pilot migration) -
/// see `render::render_template`'s doc for why `top_nav` is still a
/// Rust-rendered raw HTML string rather than a template partial.
fn render_character_list(characters: &[(String, Character)], viewer: Option<&Character>) -> String {
    let mut sorted: Vec<&(String, Character)> = characters.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| b.level.cmp(&a.level).then(b.wins.cmp(&a.wins)));
    let cards: Vec<RosterCardCtx> = sorted
        .iter()
        .map(|(login, c)| {
            let sprite = c.effective_sprite(login);
            let games = c.wins + c.losses;
            let winrate = if games > 0 { format!("{:.0}%", c.wins as f64 / games as f64 * 100.0) } else { "—".to_string() };
            RosterCardCtx {
                login_enc: urlencoding::encode(login).into_owned(),
                sprite,
                name: escape_html(&c.display_name),
                level: c.level,
                archetype: format!("{:?}", c.archetype),
                wins: format_number(c.wins as f64),
                losses: format_number(c.losses as f64),
                winrate,
            }
        })
        .collect();
    render::render_template("characters.html", minijinja::context! { top_nav => top_nav(viewer), empty => characters.is_empty(), cards => cards })
}

#[cfg(test)]
mod character_list_render_tests {
    use super::*;

    /// Characterization test - the exact byte-for-byte output of the
    /// pre-2026-08-18 `format!`-based `render_character_list`, captured
    /// before its Phase 1 template migration (see `templates/characters.html`)
    /// so the migration itself can be diff-verified against real output
    /// instead of by inspection alone. Must keep passing unchanged
    /// through the migration - any difference here is a real rendering
    /// regression.
    #[test]
    fn matches_pre_migration_baseline() {
        let mut alpha = Character::new("Alpha".to_string());
        alpha.level = 5;
        alpha.wins = 10;
        alpha.losses = 3;
        let mut bravo = Character::new("Bravo".to_string());
        bravo.level = 12;
        bravo.wins = 0;
        bravo.losses = 0;
        let characters = vec![("alpha".to_string(), alpha), ("bravo".to_string(), bravo)];
        let output = render_character_list(&characters, None);
        let expected = "<div class=\"top-nav\"><a class=\"top-nav-link\" href=\"/\">\u{1F3E0} Character Sheet</a><a class=\"top-nav-link\" href=\"/inventory\">\u{1F392} Bag &amp; Crafting</a><a class=\"top-nav-link\" href=\"/passives\">\u{1F333} Passives</a><a class=\"top-nav-link\" href=\"/characters\">\u{1F3C6} Character List</a><a class=\"top-nav-link\" href=\"/fights\">\u{1F4DC} Fight History</a><a class=\"top-nav-link\" href=\"/wiki\">\u{1F4D6} Wiki</a><a class=\"top-nav-link\" href=\"/overlay\" target=\"_blank\" rel=\"noopener\">\u{1F4FA} Watch Overlay</a><a class=\"top-nav-link\" href=\"/bugs\">\u{1F41E} Report a Bug</a></div><div class=\"card\"><h1>Adventure Roster</h1></div><div class=\"roster-grid\"><a class=\"roster-card\" href=\"/characters/bravo\"><img class=\"roster-sprite\" src=\"/sprites/sprite-26.png\" onerror=\"this.onerror=null;this.src='/sprites/sprite-26.gif'\" alt=\"\"><div class=\"roster-name\">Bravo</div><div class=\"roster-meta\">Level 12 Commoner</div><div class=\"roster-meta\">0W / 0L (\u{2014})</div></a><a class=\"roster-card\" href=\"/characters/alpha\"><img class=\"roster-sprite\" src=\"/sprites/sprite-06.png\" onerror=\"this.onerror=null;this.src='/sprites/sprite-06.gif'\" alt=\"\"><div class=\"roster-name\">Alpha</div><div class=\"roster-meta\">Level 5 Commoner</div><div class=\"roster-meta\">10W / 3L (77%)</div></a></div>";
        assert_eq!(output, expected, "render_character_list output must be byte-for-byte identical to the pre-migration baseline");
    }

    /// Same baseline-lock reasoning as `matches_pre_migration_baseline`,
    /// for the empty-roster branch (a separate early-return in the
    /// function, not just a zero-iteration loop).
    #[test]
    fn matches_pre_migration_empty_baseline() {
        let output = render_character_list(&[], None);
        let expected = "<div class=\"top-nav\"><a class=\"top-nav-link\" href=\"/\">\u{1F3E0} Character Sheet</a><a class=\"top-nav-link\" href=\"/inventory\">\u{1F392} Bag &amp; Crafting</a><a class=\"top-nav-link\" href=\"/passives\">\u{1F333} Passives</a><a class=\"top-nav-link\" href=\"/characters\">\u{1F3C6} Character List</a><a class=\"top-nav-link\" href=\"/fights\">\u{1F4DC} Fight History</a><a class=\"top-nav-link\" href=\"/wiki\">\u{1F4D6} Wiki</a><a class=\"top-nav-link\" href=\"/overlay\" target=\"_blank\" rel=\"noopener\">\u{1F4FA} Watch Overlay</a><a class=\"top-nav-link\" href=\"/bugs\">\u{1F41E} Report a Bug</a></div><div class=\"card\"><h1>Adventure Roster</h1></div><div class=\"card\"><p class=\"muted\">Nobody's joined the adventure yet.</p></div>";
        assert_eq!(output, expected, "empty-roster render_character_list output must be byte-for-byte identical to the pre-migration baseline");
    }
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
fn render_character_detail(login: &str, c: &Character, viewer: Option<&Character>, tunables: &LiveTunables) -> String {
    let nav = top_nav(viewer);
    let name = escape_html(&c.display_name);
    let sprite = c.effective_sprite(login);
    let xp_pct = if c.xp_needed() > 0 { (c.xp as f64 / c.xp_needed() as f64 * 100.0).clamp(0.0, 100.0) } else { 100.0 };
    let games = c.wins + c.losses;
    let winrate = if games > 0 { format!("{:.0}%", c.wins as f64 / games as f64 * 100.0) } else { "—".to_string() };
    let combat_stats_html = render_combat_stats_card(c, tunables);
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
                <div class=\"stat\"><div class=\"stat-label\">Divine Dust</div><div class=\"stat-value\">{divine_dust}</div></div>\
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
        divine_dust = format_number(c.divine_dust as f64),
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
/// streamer-only to every player - see `fight_summaries_for_viewer`),
/// newest first: outcome, the same top-3 DPS/tanks/heals leaderboard the
/// chat report uses (computed here directly from the summary's own
/// per-player aggregates), and loot/broken. `fights` is already resolved
/// (fetched + filtered + limited) by the caller via
/// `fight_summaries_for_viewer` - this function only renders.
///
/// Reads the SUMMARY tier (2026-08-18, wiki audit finding #4 - see
/// `fight_summaries_for_viewer`'s doc for the bug this fixes: this page
/// used to read the much smaller coarse tier while `/fights.json` always
/// read the summary tier, so a player could fight and immediately see
/// "no recent fights" here the moment the coarse tier's short window
/// rolled past them). That trades away three sections the coarse tier's
/// full event log could build that the summary tier has no data for at
/// all - per-boss combat stats, a skill-cast breakdown, and buff/debuff
/// stack-activity samples - none of which survive into `FightSummarySnapshot`.
/// Loot/broken/outcome/first-to-die/the DPS-tanks-heals leaderboard all
/// survive intact; a Basic fight's title also loses the specific enemy
/// name/count (not carried in the summary either), just "Basic — Stage
/// N — outcome" now, same as a Boss fight's own title shape.
fn render_fights_page(viewer: Option<&Character>, fights: &[FightSummarySnapshot], is_streamer: bool) -> String {
    let header = format!("{}<div class=\"card\"><h1>Fight History</h1></div>", top_nav(viewer));
    if fights.is_empty() {
        let msg = if is_streamer { "No fights logged yet." } else { "You haven't been in any recently logged fights yet." };
        return format!("{header}<div class=\"card\"><p class=\"muted\">{msg}</p></div>");
    }
    let cards: String = fights
        .iter()
        .map(|s| {
            let outcome = if s.won { "Won" } else { "Lost" };
            let title = match s.kind {
                EncounterKind::Boss => format!("Boss — Stage {} — {outcome}", s.stage),
                EncounterKind::Basic => format!("Basic — Stage {} — {outcome}", s.stage),
            };

            // Same top-3-by-amount ranked-list convention `summarize_fight`'s
            // own top_damage_dealt/top_damage_taken/top_healing_done used,
            // just derived directly from the summary tier's own per-player
            // aggregates instead of re-walking the full event log.
            let ranked = |mut entries: Vec<(&str, u64)>| {
                entries.retain(|&(_, amt)| amt > 0);
                entries.sort_by(|a, b| b.1.cmp(&a.1));
                entries.truncate(3);
                if entries.is_empty() {
                    return "—".to_string();
                }
                let names = entries.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ");
                let total: u64 = entries.iter().map(|(_, amt)| amt).sum();
                format!("{names} ({total})")
            };
            let first_down = s.first_to_die.as_deref().unwrap_or("—").to_string();

            let started_at = if s.started_at_unix_ms > 0 {
                format_unix_secs((s.started_at_unix_ms / 1000) as i64)
            } else {
                "unknown time (logged before this was tracked)".to_string()
            };

            let loot_html: String = if s.loot.is_empty() {
                "<li class=\"muted\">None</li>".to_string()
            } else {
                s.loot
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
            let broken_html: String = if s.broken.is_empty() {
                "<li class=\"muted\">None</li>".to_string()
            } else {
                s.broken.iter().map(|b| format!("<li>{} — {}</li>", escape_html(&b.display_name), escape_html(&b.item_name))).collect()
            };

            format!(
                "<div class=\"card\">\
                  <h2>{title}</h2>\
                  <p class=\"muted\">{started_at} · {participants} participants · {display_ms}ms display</p>\
                  <h3>Battle Report</h3>\
                  <ul>\
                    <li>🗡️ Top DPS: {dps}</li>\
                    <li>🛡️ Top Tanks: {tanks}</li>\
                    <li>💚 Top Heals: {heals}</li>\
                    <li>💀 First down: {first_down}</li>\
                  </ul>\
                  <h3>Loot</h3><ul>{loot_html}</ul>\
                  <h3>Broken Gear</h3><ul>{broken_html}</ul>\
                </div>",
                participants = s.participants,
                display_ms = s.display_duration_ms,
                dps = ranked(s.players.iter().map(|p| (p.display_name.as_str(), p.damage_dealt)).collect()),
                tanks = ranked(s.players.iter().map(|p| (p.display_name.as_str(), p.damage_taken)).collect()),
                heals = ranked(s.players.iter().map(|p| (p.display_name.as_str(), p.healing_done)).collect()),
            )
        })
        .collect();
    format!("{header}{cards}")
}

#[cfg(test)]
mod render_fights_page_tests {
    use super::*;
    use crate::adventure::PlayerFightStats;

    fn player_stats(name: &str, damage_dealt: u64, damage_taken: u64, healing_done: u64) -> PlayerFightStats {
        PlayerFightStats { display_name: name.to_string(), damage_dealt, damage_taken, healing_done, ..Default::default() }
    }

    /// Wiki audit finding #4: `/fights` now renders from the summary
    /// tier, same as `/fights.json` always did (see `render_fights_page`'s
    /// own doc for the "no recent fights" bug this fixes). This test
    /// exercises the new ranked-list logic (built directly from
    /// `PlayerFightStats`, not a re-walk of the full event log the
    /// summary tier no longer carries) rather than the disk-I/O path
    /// (`fight_summaries_for_viewer`/`recent_summary_fights`), which -
    /// same as the rest of fight_storage.rs - reads a fixed, real path
    /// with no injectable directory to sandbox a test against.
    #[test]
    fn renders_rankings_and_outcome_from_summary_tier_data_alone() {
        let snapshot = FightSummarySnapshot {
            kind: EncounterKind::Boss,
            stage: 5,
            won: true,
            participants: 2,
            players: vec![player_stats("Alice", 500, 0, 0), player_stats("Bob", 0, 300, 100)],
            first_to_die: Some("Bob".to_string()),
            ..Default::default()
        };
        let html = render_fights_page(None, &[snapshot], false);
        assert!(html.contains("Boss — Stage 5 — Won"), "title must reflect kind/stage/outcome from summary data alone: {html}");
        assert!(html.contains("Alice (500)"), "top DPS must be computed from PlayerFightStats::damage_dealt: {html}");
        assert!(html.contains("Bob (300)"), "top tank must be computed from PlayerFightStats::damage_taken: {html}");
        assert!(html.contains("Bob (100)"), "top heals must be computed from PlayerFightStats::healing_done: {html}");
        assert!(html.contains("First down: Bob"), "first_to_die must come straight from the summary: {html}");
    }

    #[test]
    fn a_player_with_zero_in_every_category_never_appears_in_any_ranking() {
        let snapshot = FightSummarySnapshot { players: vec![player_stats("Ghost", 0, 0, 0)], ..Default::default() };
        let html = render_fights_page(None, &[snapshot], false);
        assert!(html.contains("Top DPS: —"), "a player who dealt/took/healed nothing must not show up in any ranking: {html}");
    }

    #[test]
    fn empty_fights_shows_the_right_message_for_streamer_vs_everyone_else() {
        assert!(render_fights_page(None, &[], true).contains("No fights logged yet."));
        assert!(render_fights_page(None, &[], false).contains("You haven't been in any recently logged fights yet."));
    }
}

/// Admin-only live-tunables form (see `ADMIN_TUNABLES_LOGIN`/`LiveTunables`)
/// - one number input per field, pre-filled with the current live value,
/// grouped into the same two sections `LiveTunables`'s own doc describes.
/// A save POSTs everything at once (not per-field) and redirects back here
/// with `?saved=1` for the confirmation banner - same query-param-flash
/// pattern `IndexParams`'s fields already use elsewhere on this dashboard.
/// `rejected` replays a refused save back onto the page it came from
/// (2026-08-31) - `t` then carries what the operator TYPED rather than
/// what is live, so no other edit on this 66-field form is lost to one bad
/// number. `None` is the ordinary render.
fn render_tunables_page(viewer: Option<&Character>, t: &LiveTunables, pacing: PacingStatus, saved: bool, rejected: Option<&TunablesRejection>) -> String {
    let nav = top_nav(viewer);
    // Operator control (2026-08-28) - the select is generated from
    // `BossKind::FORCED_CHOICES` rather than hand-written, so the page
    // can never offer a name `parse_forced` would refuse. The blank
    // first option is the normal random roll.
    let boss_options = std::iter::once("<option value=\"\">Random (normal roll)</option>".to_string())
        .chain(BossKind::FORCED_CHOICES.iter().map(|(value, label)| format!("<option value=\"{value}\">{label}</option>")))
        .collect::<String>();
    // Saturation signals (owner ruling: a pinned controller must be
    // VISIBLE, not silent). A pinned flag means the party is performing
    // BELOW the stage baseline and the hand-authored floor is doing the
    // work - said explicitly right in the read-out.
    let pinned_note = |pinned: bool| {
        if pinned {
            "<p class=\"tunable-hint\" style=\"color:#e6b34d\">⚠ PINNED AT BASELINE FLOOR — the party is performing below this stage's baseline; the controller wants to go easier but the baseline is holding difficulty up. Lower the baseline anchors (or accept it) rather than expecting this controller to move.</p>"
        } else {
            ""
        }
    };
    // Three numbers per axis, in the order an operator reads them: what
    // the controller itself wants, the floor under it, and what the next
    // fight is ACTUALLY built with (the max of the two). Without the
    // third, a pinned axis showed a "current" number that generation
    // never used.
    let hp_pacing_readout = format!(
        "Controller A (HP / duration): current {:+.3}x — stage baseline floor {:+.3}x — <strong>in force {:+.3}x</strong>{}",
        pacing.hp_mult,
        pacing.hp_baseline,
        pacing.hp_effective,
        pinned_note(pacing.hp_pinned())
    );
    let dmg_pacing_readout = format!(
        "Controller B (damage / lethality): current {:+.3}x — stage baseline floor {:+.3}x — <strong>in force {:+.3}x</strong>{}",
        pacing.dmg_mult,
        pacing.dmg_baseline,
        pacing.dmg_effective,
        pinned_note(pacing.dmg_pinned())
    );
    // A rejected save echoes the three CSV inputs exactly as typed - they
    // cannot round-trip through the parsed `Vec`s, and a malformed list is
    // precisely the one that has to come back for editing.
    let baseline_stage_anchors_csv = match rejected {
        Some(r) => escape_html(&r.stage_anchors),
        None => t.baseline_stage_anchors.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "),
    };
    let baseline_hp_anchors_csv = match rejected {
        Some(r) => escape_html(&r.hp_anchors),
        None => t.baseline_hp_anchors.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "),
    };
    let baseline_atk_anchors_csv = match rejected {
        Some(r) => escape_html(&r.atk_anchors),
        None => t.baseline_atk_anchors.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "),
    };
    let banner = match rejected {
        // Every offending field at once, each with the range it accepts,
        // and an unambiguous statement that NOTHING was written - the
        // whole point of ledger #69 is that the page used to claim a
        // success it had not performed.
        Some(r) => {
            let list = r.violations.iter().map(|msg| format!("<li>{}</li>", escape_html(msg))).collect::<String>();
            format!(
                "<p class=\"tunable-hint\" style=\"color:#e6b34d\">⚠ NOT SAVED — {} rejected, and nothing on this page was written. Your other edits are still in the boxes below; fix these and save again.</p><ul style=\"color:#e6b34d\">{list}</ul>",
                if r.violations.len() == 1 { "1 value was".to_string() } else { format!("{} values were", r.violations.len()) }
            )
        }
        None if saved => "<p class=\"muted\">✅ Saved — takes effect on the very next encounter, no restart needed.</p>".to_string(),
        None => String::new(),
    };
    // !pinfight (2026-08-18) - dashboard-visible confirmation that a pin
    // actually landed, so a mod doesn't have to spelunk the filesystem to
    // check. Pure read of PINNED_FIGHTS_DIR's current contents - nothing
    // here can accidentally trigger a pin.
    let pinned_fights = list_pinned_fights();
    let pinned_fights_html = if pinned_fights.is_empty() {
        "<p class=\"muted\">Nothing pinned yet — use !pinfight in chat to preserve a fight's evidence before the normal rolling window ages it out.</p>".to_string()
    } else {
        let items: String = pinned_fights.iter().map(|f| format!("<li>{}</li>", escape_html(f))).collect();
        format!("<ul>{items}</ul>")
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
              <p class=\"tunable-hint\">0 to 1 (e.g. 0.002 = 0.2%). One roll on every real item drop, rolls for every archetype. (Celestial Shard and Unique Shard were merged into one currency 2026-08-19 - this used to be two independent rolls at half this rate each.)</p>\
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
            <h2>Dynamic Pacing</h2>\
            <p class=\"tunable-hint\">{hp_pacing_readout}</p>\
            <p class=\"tunable-hint\">{dmg_pacing_readout}</p>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"dynamic_pacing_enabled\" value=\"1\"{dynamic_pacing_enabled_checked}> Dynamic pacing enabled (master kill-switch)</label>\
            <p class=\"tunable-hint\">Unchecked = BOTH controllers completely inert (no sampling, no updates); both multipliers freeze where they sit. The stage baseline floor and the top-layer mitigation below are separate systems with their own switches.</p>\
            <div class=\"tunable-row\">\
              <label for=\"pacing_window_fights\">Pacing Window (fights)</label>\
              <input type=\"number\" step=\"1\" min=\"1\" id=\"pacing_window_fights\" name=\"pacing_window_fights\" value=\"{pacing_window_fights}\">\
              <p class=\"tunable-hint\">Rolling window for BOTH controllers (A's DPS samples, B's win/loss ratio). Both stay neutral until a full window exists.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"target_duration_min_s\">Target Fight Duration Min (s)</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"target_duration_min_s\" name=\"target_duration_min_s\" value=\"{target_duration_min_s}\">\
              <p class=\"tunable-hint\">Controller A (HP axis) targets this window of REAL fight time, aiming at the midpoint with the max below. Real clock, not the overlay's compressed playback.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"target_duration_max_s\">Target Fight Duration Max (s)</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"target_duration_max_s\" name=\"target_duration_max_s\" value=\"{target_duration_max_s}\">\
              <p class=\"tunable-hint\">Window upper bound; A scales enemy HP pools (never the per-enemy split) so expected kill time lands near (min+max)/2. Samples WINNING fights only — a wipe never feeds the measure.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_max_step_per_fight\">HP Max Step Per Fight</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"hp_max_step_per_fight\" name=\"hp_max_step_per_fight\" value=\"{hp_max_step_per_fight}\">\
              <p class=\"tunable-hint\">Max RELATIVE change of A's multiplier per winning fight (0.25 = &plusmn;25%). The oscillation damper.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_multiplier_floor\">HP Multiplier Floor</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"hp_multiplier_floor\" name=\"hp_multiplier_floor\" value=\"{hp_multiplier_floor}\">\
              <p class=\"tunable-hint\">Floor on A's own multiplier RELATIVE to the organic stage curve — NOT the absolute difficulty floor; the baseline anchors below bind first. Hard floor 0.05 / hard ceiling 1,000,000 regardless.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_multiplier_ceiling\">HP Multiplier Ceiling</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"hp_multiplier_ceiling\" name=\"hp_multiplier_ceiling\" value=\"{hp_multiplier_ceiling}\">\
              <p class=\"tunable-hint\">Ceiling on A's multiplier (hard-capped at 1,000,000 no matter what).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"enemy_hp_pool_hard_cap\">Enemy HP Pool Cap (hit points, summed over all enemies)</label>\
              <input type=\"number\" step=\"any\" min=\"{enemy_hp_pool_cap_min}\" max=\"{enemy_hp_pool_cap_max}\" required id=\"enemy_hp_pool_hard_cap\" name=\"enemy_hp_pool_hard_cap\" value=\"{enemy_hp_pool_hard_cap}\">\
              <p class=\"tunable-hint\"><strong>Unit: hit points.</strong> Range 1e15 &ndash; 5e16. Out-of-range is rejected by the form; a POST that bypasses the browser is clamped instead. Ceiling on the TOTAL scaled HP of every enemy in one encounter, applied to Controller A's multiplier before scaling &mdash; <strong>this is what decides whether A's output reaches the fight at all.</strong> Measured 2026-08-30 (anomaly ledger #67): at the 1e15 default it binds on every boss fight, cutting A's honest request of ~186 down to 13.35 and delivering 2.69s fights against the 30&ndash;45s target above. Reaching that window needs roughly 1.4e16. <strong>Raise in small watched steps, never one jump</strong> &mdash; boss HP rises with it AND every time-based boss mechanic the ~2.7s runway has been starving (boss defence ignore at 2%/s, pierce, the Gelatinous Cube's 3s shred window) starts running to completion. Both sides of the fight get harder at once.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_relax_after_losses\">HP Relax After (consecutive losses)</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"hp_relax_after_losses\" name=\"hp_relax_after_losses\" value=\"{hp_relax_after_losses}\">\
              <p class=\"tunable-hint\">Consecutive LOST boss fights before Controller A starts decaying back toward neutral. A samples wins only — correct, but it means a wipe teaches A nothing, so an overshoot has no way back without this. 0 = unset (uses the shipped default); to switch relaxation off set the step below to 0.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_relax_step_per_fight\">HP Relax Step (per lost fight)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"hp_relax_step_per_fight\" name=\"hp_relax_step_per_fight\" value=\"{hp_relax_step_per_fight}\">\
              <p class=\"tunable-hint\">How far back toward neutral A moves per lost fight once that streak is reached (0.20 = 20%). Never pushes A below neutral 1.0, and never applies while A is already at or under neutral — a losing party is never made harder by this path. <strong>0 disables relaxation entirely.</strong></p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"target_win_loss_ratio\">Target Win:Loss Ratio</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"target_win_loss_ratio\" name=\"target_win_loss_ratio\" value=\"{target_win_loss_ratio}\">\
              <p class=\"tunable-hint\">Controller B (damage axis) steers the rolling BOSS win:loss ratio here. Default 2.0 = two wins per loss — exactly neutral stage progression (+1 per win, -2 per loss), so the party only climbs by beating it. Boss outcomes only.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"dmg_max_step_per_fight\">Damage Max Step Per Fight</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"dmg_max_step_per_fight\" name=\"dmg_max_step_per_fight\" value=\"{dmg_max_step_per_fight}\">\
              <p class=\"tunable-hint\">Max RELATIVE change of B's multiplier per boss fight (0.15 = &plusmn;15%).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"dmg_multiplier_floor\">Damage Multiplier Floor</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"dmg_multiplier_floor\" name=\"dmg_multiplier_floor\" value=\"{dmg_multiplier_floor}\">\
              <p class=\"tunable-hint\">Floor on B's own multiplier relative to the organic curve (default 0.4 — real room in BOTH directions before the baseline binds). Hard floor 0.05 regardless.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"dmg_multiplier_ceiling\">Damage Multiplier Ceiling</label>\
              <input type=\"number\" step=\"any\" min=\"0.001\" id=\"dmg_multiplier_ceiling\" name=\"dmg_multiplier_ceiling\" value=\"{dmg_multiplier_ceiling}\">\
              <p class=\"tunable-hint\">Ceiling on B's multiplier (hard-capped at 1,000,000 no matter what).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"baseline_stage_anchors\">Baseline Stage Anchors (CSV)</label>\
              <input type=\"text\" id=\"baseline_stage_anchors\" name=\"baseline_stage_anchors\" value=\"{baseline_stage_anchors_csv}\">\
              <p class=\"tunable-hint\">Hand-authored difficulty floor, X axis: strictly ascending STAGE anchor points. Linearly interpolated against the value lists below, flat after the last. Malformed = neutral (organic curve is the floor).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"baseline_hp_anchors\">Baseline HP Anchors (CSV)</label>\
              <input type=\"text\" id=\"baseline_hp_anchors\" name=\"baseline_hp_anchors\" value=\"{baseline_hp_anchors_csv}\">\
              <p class=\"tunable-hint\">Minimum enemy HP per anchor, as a FRACTION of the organic stage/level/party formula (1.0 = full curve). Neither controller can ever pull effective difficulty below this. Hand-set by design — NOT derived from live player gear.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"baseline_atk_anchors\">Baseline Damage Anchors (CSV)</label>\
              <input type=\"text\" id=\"baseline_atk_anchors\" name=\"baseline_atk_anchors\" value=\"{baseline_atk_anchors_csv}\">\
              <p class=\"tunable-hint\">Same curve for enemy attack. Must have exactly as many values as stage anchors.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"hp_pacing_mult_override\">HP Controller Override: {hp_mult_override_label}</label>\
              <input type=\"text\" id=\"hp_pacing_mult_override\" name=\"hp_pacing_mult_override\" placeholder=\"leave blank to keep as-is\">\
              <p class=\"tunable-hint\">Manual override for Controller A's OWN multiplier (the read-out above shows the effective value incl. baseline). Leave blank to change nothing.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"boss_power_mult_override\">Damage Controller Override: {dmg_mult_override_label}</label>\
              <input type=\"text\" id=\"boss_power_mult_override\" name=\"boss_power_mult_override\" placeholder=\"leave blank to keep as-is\">\
              <p class=\"tunable-hint\">Manual override for Controller B's own multiplier (e.g. after a bad losing streak leaves it stuck low). Leave blank to change nothing.</p>\
            </div>\
            <h2>Top-Layer Mitigation (stage-tied)</h2>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"top_layer_enabled\" value=\"1\"{top_layer_enabled_checked}> Top-layer mitigation enabled</label>\
            <p class=\"tunable-hint\">A final ABSOLUTE damage reduction on every enemy, applied at the very END of damage resolution — after every other mitigation. NOTHING bypasses it: no armor pen, no ignore-DR, no true-damage exemption. Structurally separate from the normal DR stat and its cap. Scales with STAGE only (never with the HP controller), so gear upgrades still visibly shorten fights while HP-keyed mechanics (Shattering, Ashes to Ashes) stay sane.</p>\
            <div class=\"tunable-row\">\
              <label for=\"top_layer_cap_pct\">Top-Layer Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"top_layer_cap_pct\" name=\"top_layer_cap_pct\" value=\"{top_layer_cap_pct}\">\
              <p class=\"tunable-hint\">Asymptotic ceiling (fraction), clamped to 0.95 no matter what — an unkillable enemy is a worse failure than a long fight. Default 0.60: ~30% at stage 1500, ~41% at stage 3222.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"top_layer_half_stage\">Top-Layer Half Stage</label>\
              <input type=\"number\" step=\"any\" min=\"1\" id=\"top_layer_half_stage\" name=\"top_layer_half_stage\" value=\"{top_layer_half_stage}\">\
              <p class=\"tunable-hint\">The stage where the layer reaches HALF its cap (same asymptote shape as boss pierce). Lower = ramps in sooner.</p>\
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
            <h2>Drop Stage Gates</h2>\
            <p class=\"tunable-hint\">The world stage at which each of these four starts dropping. All four read the CURRENT stage, so a boss-loss regression below a threshold temporarily stops those drops until the group climbs back — that is intended. Polishing sand and Divine Dust are gated on FIGHT grants only: disenchanting gear still yields both at any stage.</p>\
            <div class=\"tunable-row\">\
              <label for=\"sand_drop_stage\">Polishing Sand Drop Stage (world stage)</label>\
              <input type=\"number\" step=\"1\" min=\"{drop_stage_min}\" max=\"{drop_stage_max}\" required id=\"sand_drop_stage\" name=\"sand_drop_stage\" value=\"{sand_drop_stage}\">\
              <p class=\"tunable-hint\">World stage from which a fight win grants polishing sand (boss and filler alike). 0 = always. Disenchanting is unaffected at any stage.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"perfect_item_stage\">Perfect Item Drop Stage (world stage)</label>\
              <input type=\"number\" step=\"1\" min=\"{drop_stage_min}\" max=\"{drop_stage_max}\" required id=\"perfect_item_stage\" name=\"perfect_item_stage\" value=\"{perfect_item_stage}\">\
              <p class=\"tunable-hint\">World stage from which Perfect Quality items drop — gates the per-character first-Perfect milestone and the one-guaranteed-per-boss-kill rule. Replaces the old Late-Content Stage Threshold.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_drop_stage\">Divine Dust Drop Stage (world stage)</label>\
              <input type=\"number\" step=\"1\" min=\"{drop_stage_min}\" max=\"{drop_stage_max}\" required id=\"divine_dust_drop_stage\" name=\"divine_dust_drop_stage\" value=\"{divine_dust_drop_stage}\">\
              <p class=\"tunable-hint\">World stage from which a fight win can drop Divine Dust. ALSO the stage that unlocks the /craft Divine Dust recipe — but that unlock is a one-way latch on the HIGHEST stage ever reached, so lowering this dial cannot re-lock a recipe the group already earned.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"sacred_item_stage\">Sacred Item Drop Stage (world stage)</label>\
              <input type=\"number\" step=\"1\" min=\"{drop_stage_min}\" max=\"{drop_stage_max}\" required id=\"sacred_item_stage\" name=\"sacred_item_stage\" value=\"{sacred_item_stage}\">\
              <p class=\"tunable-hint\">World stage from which Sacred items drop. Also the point where Perfect's own per-kill guarantee drops to half frequency, since that rule exists to make room for Sacred. The wiki still renders the compiled default (300) for this one, not the live value.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"pierce_cap\">Boss Pierce Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"pierce_cap\" name=\"pierce_cap\" value=\"{pierce_cap}\">\
              <p class=\"tunable-hint\">0 to 1 — the asymptotic ceiling a real boss's unavoidable/unmitigable pierce fraction climbs toward as stage grows (never actually reached). 0 = pierce disabled entirely, exactly today's pre-pierce behavior.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"pierce_h\">Boss Pierce Half-Stage</label>\
              <input type=\"number\" step=\"any\" min=\"1\" id=\"pierce_h\" name=\"pierce_h\" value=\"{pierce_h}\">\
              <p class=\"tunable-hint\">The stage at which pierce reaches HALF of the cap above. Lower = ramps up faster at earlier stages.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"fight_summary_batch_size\">Fight Summary Batch Size</label>\
              <input type=\"number\" step=\"1\" min=\"1\" id=\"fight_summary_batch_size\" name=\"fight_summary_batch_size\" value=\"{fight_summary_batch_size}\">\
              <p class=\"tunable-hint\">How many fight results (Basic and Boss alike) accumulate into one batched chat summary. 1 = post every fight individually, same as before batching existed. A partial batch always posts after ~5 minutes even if it hasn't reached this count.</p>\
            </div>\
            <h2>Elementalist</h2>\
            <div class=\"tunable-row\">\
              <label for=\"thunder_redistribution_pct\">Thunder Golem Redistribution %</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"thunder_redistribution_pct\" name=\"thunder_redistribution_pct\" value=\"{thunder_redistribution_pct}\">\
              <p class=\"tunable-hint\">0 to 1 — what fraction of a Thunder Golem incarnation's total absorbed damage gets split across the party as an unmitigated DoT when it dies. 0 disables redistribution entirely.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"thunder_redistribution_window_secs\">Thunder Golem Redistribution Window (s)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"thunder_redistribution_window_secs\" name=\"thunder_redistribution_window_secs\" value=\"{thunder_redistribution_window_secs}\">\
              <p class=\"tunable-hint\">Total seconds the 2-tick redistribution DoT is spread across (tick 1 at half this, tick 2 at the full amount).</p>\
            </div>\
            <h2>Righteous Fire</h2>\
            <div class=\"tunable-row\">\
              <label for=\"rf_self_damage_pct_rank1\">Self-Damage % (Rank 1)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"rf_self_damage_pct_rank1\" name=\"rf_self_damage_pct_rank1\" value=\"{rf_self_damage_pct_rank1}\">\
              <p class=\"tunable-hint\">0 to 1 — fraction of max HP Righteous Fire burns per second at rank 1/3, before damage reduction and shields. Decoupled from the node's own offensive damage (tune that at /admin/passives instead).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"rf_self_damage_pct_rank2\">Self-Damage % (Rank 2)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"rf_self_damage_pct_rank2\" name=\"rf_self_damage_pct_rank2\" value=\"{rf_self_damage_pct_rank2}\">\
              <p class=\"tunable-hint\">Same, rank 2/3.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"rf_self_damage_pct_rank3\">Self-Damage % (Rank 3)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"rf_self_damage_pct_rank3\" name=\"rf_self_damage_pct_rank3\" value=\"{rf_self_damage_pct_rank3}\">\
              <p class=\"tunable-hint\">Same, rank 3/3.</p>\
            </div>\
            <h2>Haloed Steps</h2>\
            <div class=\"tunable-row\">\
              <label for=\"haloedsteps_per_instance_pct_rank1\">More Damage per Divine Damage Affix (Rank 1)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"haloedsteps_per_instance_pct_rank1\" name=\"haloedsteps_per_instance_pct_rank1\" value=\"{haloedsteps_per_instance_pct_rank1}\">\
              <p class=\"tunable-hint\">0 to 1 — party more-damage % granted per equipped Divine Damage affix instance at rank 1/3, before the node's own per-rank cap (tune the cap at /admin/passives instead).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"haloedsteps_per_instance_pct_rank2\">More Damage per Divine Damage Affix (Rank 2)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"haloedsteps_per_instance_pct_rank2\" name=\"haloedsteps_per_instance_pct_rank2\" value=\"{haloedsteps_per_instance_pct_rank2}\">\
              <p class=\"tunable-hint\">Same, rank 2/3.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"haloedsteps_per_instance_pct_rank3\">More Damage per Divine Damage Affix (Rank 3)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"haloedsteps_per_instance_pct_rank3\" name=\"haloedsteps_per_instance_pct_rank3\" value=\"{haloedsteps_per_instance_pct_rank3}\">\
              <p class=\"tunable-hint\">Same, rank 3/3.</p>\
            </div>\
            <h2>Reactive Procs</h2>\
            <div class=\"tunable-row\">\
              <label for=\"reactive_proc_cap_ms\">Reactive Counter Cap (ms)</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"reactive_proc_cap_ms\" name=\"reactive_proc_cap_ms\" value=\"{reactive_proc_cap_ms}\">\
              <p class=\"tunable-hint\">Minimum time between real counter-attacks for the shared Rogue's Voidstep / Monk's Counterflow / Druid's Wild Fury group (Warrior's Retaliation is uncapped). Default 1000ms = at most 1 real trigger per second.</p>\
            </div>\
            <h2>Divine Dust</h2>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_drop_chance\">Divine Dust Fight-Drop Chance</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"divine_dust_drop_chance\" name=\"divine_dust_drop_chance\" value=\"{divine_dust_drop_chance}\">\
              <p class=\"tunable-hint\">0 to 1 — chance per fighting character, per win (boss or basic, same eligibility as sand), of gaining exactly 1 Divine Dust.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_disenchant_chance\">Divine Dust Disenchant Chance</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"divine_dust_disenchant_chance\" name=\"divine_dust_disenchant_chance\" value=\"{divine_dust_disenchant_chance}\">\
              <p class=\"tunable-hint\">0 to 1 — chance per Sacred item manually disenchanted of gaining 1 Divine Dust. Non-Sacred disenchants never grant any.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_craft_dust_cost\">Divine Dust Recipe: Dust Cost</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"divine_dust_craft_dust_cost\" name=\"divine_dust_craft_dust_cost\" value=\"{divine_dust_craft_dust_cost}\">\
              <p class=\"tunable-hint\">Dust cost of the /craft recipe (deliberately cheap relative to veteran holdings — sand is the intended pacing constraint).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_craft_sand_cost\">Divine Dust Recipe: Sand Cost</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"divine_dust_craft_sand_cost\" name=\"divine_dust_craft_sand_cost\" value=\"{divine_dust_craft_sand_cost}\">\
              <p class=\"tunable-hint\">Sand cost of the same recipe.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"divine_dust_craft_output\">Divine Dust Recipe: Output</label>\
              <input type=\"number\" step=\"1\" min=\"1\" id=\"divine_dust_craft_output\" name=\"divine_dust_craft_output\" value=\"{divine_dust_craft_output}\">\
              <p class=\"tunable-hint\">Divine Dust granted per craft, before the x1/x10/x50 batch multiplier.</p>\
            </div>\
            <h2>Crafting Costs</h2>\
            <div class=\"tunable-row\">\
              <label for=\"craft_base_cost_mult\">Craft Base Cost Multiplier (x, on the flat per-action fee)</label>\
              <input type=\"number\" step=\"any\" min=\"{craft_base_cost_mult_min}\" max=\"{craft_base_cost_mult_max}\" required id=\"craft_base_cost_mult\" name=\"craft_base_cost_mult\" value=\"{craft_base_cost_mult}\">\
              <p class=\"tunable-hint\">{craft_base_cost_mult_min} to {craft_base_cost_mult_max} — multiplies every craft action's flat dust fee (Transmute 250, Krangle 2500, …) and the veil surcharge, before the per-tier surcharge below is added. Shipped 0.1 = the 10x cost cut; 1 restores the pre-cut prices exactly; 10 is ten times those old prices. 0 makes the flat fee free but NOT the craft — the per-tier surcharge still applies. Each fee is rounded UP, so a nonzero fee can never round away to nothing.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"craft_tier_exponent\">Craft Tier Cost Exponent (per-tier surcharge = 3 x tier^exponent, dust)</label>\
              <input type=\"number\" step=\"any\" min=\"{craft_tier_exponent_min}\" max=\"{craft_tier_exponent_max}\" required id=\"craft_tier_exponent\" name=\"craft_tier_exponent\" value=\"{craft_tier_exponent}\">\
              <p class=\"tunable-hint\">{craft_tier_exponent_min} to {craft_tier_exponent_max} — 1.0 is the old flat 3 dust per tier; shipped 1.1 makes cost accelerate with tier, slowly (tier 10: 38 instead of 30; tier 100: 476 instead of 300; tier 201: 1025 instead of 603). Below 1 is refused: it would make crafting relatively cheaper the further a player progresses.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"craft_tier_bump_mult\">Craft Tier Growth Multiplier (x, on the +3/+2/+1 tiers a craft adds)</label>\
              <input type=\"number\" step=\"any\" min=\"{craft_tier_bump_mult_min}\" max=\"{craft_tier_bump_mult_max}\" required id=\"craft_tier_bump_mult\" name=\"craft_tier_bump_mult\" value=\"{craft_tier_bump_mult}\">\
              <p class=\"tunable-hint\">{craft_tier_bump_mult_min} to {craft_tier_bump_mult_max} — every successful craft raises the crafted item's tier (+3 below tier 25, +2 below 50, +1 above), which raises its power, every modifier on it, AND the per-tier surcharge on its next craft. This scales all three bands together. Shipped 1 = unchanged; <strong>0 switches per-craft tier growth off entirely</strong>, which is how to watch the exponent above in isolation. This dial and the exponent act on different things — the exponent prices tier, this decides how fast an item climbs.</p>\
            </div>\
            <h2>Experience</h2>\
            <p class=\"tunable-hint\">XP is paid on a <strong>boss-fight win only</strong> &mdash; a filler fight pays none, and a loss pays none. One win is worth <strong>Flat XP + Level % &times; that level&rsquo;s own XP cost</strong>, then &times; catch-up, then &times; the multiplier below. Because it is paid per win, XP is already exactly linear in win rate; the band that gives you is 0&times; to 1.5&times; of the 2:1 baseline, since a win rate cannot exceed 100%.</p>\
            <div class=\"tunable-row\">\
              <label for=\"win_xp_flat\">Flat XP per Win</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"{win_xp_flat_max}\" required id=\"win_xp_flat\" name=\"win_xp_flat\" value=\"{win_xp_flat}\">\
              <p class=\"tunable-hint\"><strong>Unit: raw XP.</strong> Range 0 &ndash; {win_xp_flat_max}. Fixed in XP, so its worth <em>in levels</em> shrinks as levels get more expensive &mdash; <strong>this is the dial that sets the day-one burst.</strong> At 12 and a 2:1 win rate, day one is 10 levels.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"win_xp_level_pct\">Level % per Win</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"{win_xp_level_pct_max}\" required id=\"win_xp_level_pct\" name=\"win_xp_level_pct\" value=\"{win_xp_level_pct}\">\
              <p class=\"tunable-hint\"><strong>Unit: fraction of the level&rsquo;s own XP cost</strong> (0 to 1), not raw XP. Worth a constant number of levels forever &mdash; <strong>this is the dial that sets the floor the rate settles onto.</strong> Levels per day = wins per day &times; this. At the shipped 0.0208 (1/48) and 96 wins/day that is 2 levels/day.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"win_xp_mult\">Global XP Multiplier</label>\
              <input type=\"number\" step=\"any\" min=\"{win_xp_mult_min}\" max=\"{win_xp_mult_max}\" required id=\"win_xp_mult\" name=\"win_xp_mult\" value=\"{win_xp_mult}\">\
              <p class=\"tunable-hint\"><strong>Unit: multiplier</strong> (1.0 = as designed). Range {win_xp_mult_min} &ndash; {win_xp_mult_max}. Scales <em>all</em> XP uniformly, applied last &mdash; after the two terms above are summed and after catch-up. Because it scales both terms equally it moves the whole curve up or down and <strong>changes nothing about its shape</strong>: the decay rate and the level it settles onto are untouched. <strong>0 switches XP off entirely</strong> (a deliberate end-of-season freeze, not a typo guard). Does not touch dust, sand or drop rates &mdash; those are Loot Multiplier and Sand Multiplier, separate dials on separate currencies.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"win_xp_cooldown_secs\">XP Cooldown per Character</label>\
              <input type=\"number\" step=\"1\" min=\"0\" max=\"{win_xp_cooldown_secs_max}\" required id=\"win_xp_cooldown_secs\" name=\"win_xp_cooldown_secs\" value=\"{win_xp_cooldown_secs}\">\
              <p class=\"tunable-hint\"><strong>Unit: seconds.</strong> Range 0 &ndash; {win_xp_cooldown_secs_max}. Shortest gap between two XP-paying wins for one character. <strong>This is the rampage guard.</strong> Scheduled boss fights are 600s apart so it never binds there; a rampage runs them 60s apart, and without this a rampage would be worth 10&times; the XP and would set the curve instead of the schedule. At the shipped 450 a rampage pays 1.33&times; normal rather than 10&times;. Also covers Force Boss Fight and !nextencounter. <strong>0 removes the throttle</strong> &mdash; every win pays, and a rampage becomes an XP farm.</p>\
            </div>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"win_xp_catchup_enabled\" value=\"1\"{win_xp_catchup_enabled_checked}> XP Catch-Up Enabled</label>\
            <p class=\"tunable-hint\">Keeps the catch-up multiplier (1&times; to 3&times;, by how far below the group median a character is) on the XP grant, so a newer player levels toward the pack. Unchecking makes every winner&rsquo;s XP identical regardless of level.</p>\
            <h2>Rampage</h2>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"permanent_rampage\" value=\"1\"{permanent_rampage_checked}> Permanent Rampage</label>\
            <p class=\"tunable-hint\">Unlike !rampage (a one-time 50-fight burst), this never runs out — boss fights back-to-back with instant revives between them, until unchecked here.</p>\
            <h2>Water Golem Shattering</h2>\
            <label class=\"veil-check\"><input type=\"checkbox\" name=\"shattering_enabled\" value=\"1\"{shattering_enabled_checked}> Shattering Enabled</label>\
            <p class=\"tunable-hint\">Live kill-switch, unchecked = a complete no-op pending a rework. Doesn't touch invested points or the tree node — flips back on instantly when re-checked.</p>\
            <p class=\"tunable-hint\">Full formula: targets = splash + the shattering node's own rank value (tune that at /admin/passives — splash needs no separate knob, it's already a real stat); damage = damage % below × the dead enemy's max HP × (1 − the target's damage reduction).</p>\
            <div class=\"tunable-row\">\
              <label for=\"shattering_damage_pct_rank1\">Icicle Damage % (Rank 1)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"shattering_damage_pct_rank1\" name=\"shattering_damage_pct_rank1\" value=\"{shattering_damage_pct_rank1}\">\
              <p class=\"tunable-hint\">0 to 1 — fraction of the dead enemy's max HP each icicle deals at rank 1/3, before the target's own damage reduction. Never scaled by the golem's own crit/increased-damage stack.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"shattering_damage_pct_rank2\">Icicle Damage % (Rank 2)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"shattering_damage_pct_rank2\" name=\"shattering_damage_pct_rank2\" value=\"{shattering_damage_pct_rank2}\">\
              <p class=\"tunable-hint\">Same, rank 2/3.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"shattering_damage_pct_rank3\">Icicle Damage % (Rank 3)</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"shattering_damage_pct_rank3\" name=\"shattering_damage_pct_rank3\" value=\"{shattering_damage_pct_rank3}\">\
              <p class=\"tunable-hint\">Same, rank 3/3.</p>\
            </div>\
            <h2>Defensive Stat Hard Cap</h2>\
            <p class=\"tunable-hint\">Owner doctrine: maximum damage mitigation from damage reduction, applies universally — no character, golem, or enemy may ever be immune to any damage source through DR. Does NOT cover evasion, block, or Intervene (separate mechanics, their own caps) or Thunder Golem absorption/redirect (not damage reduction at all).</p>\
            <div class=\"tunable-row\">\
              <label for=\"defensive_stat_hard_cap\">Max DR Mitigation</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"defensive_stat_hard_cap\" name=\"defensive_stat_hard_cap\" value=\"{defensive_stat_hard_cap}\">\
              <p class=\"tunable-hint\">0 to 1 — a landed hit always deals at least (1 − this) of its raw mitigable damage, however stacked a defender's DR sources get. Default 0.95.</p>\
            </div>\
            <h2>Overflow Economy (cross-class caps)</h2>\
            <p class=\"tunable-hint\">These five bound the overflow-conversion economy shared by every class — Stone Fist/Granite Skin/Overgrown Reach (Monk), Unbreakable (Warrior), Elusive/Phantom/Duskveil/Lightfoot (Rogue), Shifting Form family (Druid), Aegis Ward (Paladin) — and where Evasion/Block/DR saturate at all. Defaults are exactly today's shipped numbers; lower to nerf, raise to loosen. Read fresh from the fight's own snapshot every fight — no restart needed.</p>\
            <div class=\"tunable-row\">\
              <label for=\"overflow_conversion_cap_per_rank\">Conversion Output Cap / Rank</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"overflow_conversion_cap_per_rank\" name=\"overflow_conversion_cap_per_rank\" value=\"{overflow_conversion_cap_per_rank}\">\
              <p class=\"tunable-hint\">Hard ceiling on any ONE conversion node's own output per invested rank. Default 0.10 = +10% per point (+30% at 3/3). This is the dial for the Monk trio's free damage multiplier: at defaults the saturated trio adds +90%; at 0.05 it adds +45%.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"evasion_overflow_cap\">Evasion Overflow Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"evasion_overflow_cap\" name=\"evasion_overflow_cap\" value=\"{evasion_overflow_cap}\">\
              <p class=\"tunable-hint\">Where Evasion saturates (default 0.75); everything past it feeds every conversion channel plus Unbroken's evasion-ignore and Last Bastion's shred.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"block_overflow_cap\">Block Overflow Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"block_overflow_cap\" name=\"block_overflow_cap\" value=\"{block_overflow_cap}\">\
              <p class=\"tunable-hint\">Where Block Chance saturates (default 0.75) — feeds Unbreakable's block-to-damage conversion.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"dr_overflow_cap\">DR Overflow Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"dr_overflow_cap\" name=\"dr_overflow_cap\" value=\"{dr_overflow_cap}\">\
              <p class=\"tunable-hint\">Where Damage Reduction saturates on the positive side (default 0.75). The −75% floor is structural safety and stays fixed.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"intervene_overflow_cap\">Intervene Overflow Cap</label>\
              <input type=\"number\" step=\"any\" min=\"0\" max=\"1\" id=\"intervene_overflow_cap\" name=\"intervene_overflow_cap\" value=\"{intervene_overflow_cap}\">\
              <p class=\"tunable-hint\">Where Intervene saturates per character (default 0.50) — feeds Aegis Ward/Sanctified Armor conversions and the per-character combine ceiling.</p>\
            </div>\
            <h2>Splash</h2>\
            <p class=\"tunable-hint\">Splash % is a CHANCE (capped 100% for the roll itself), rolled once per action, all-or-nothing. ATTACK splash (a normal hit/heal's own splash) grants 0 extra targets on a miss or at 0% splash. The four SUPPORT sites (Radiant Smite heal, Relentless/Cauterizing Flames, Cleansing Flames' cleanse + buff-refresh) fall back to the floor below instead — they never do nothing. Every caller keeps its own base target count (Gelatinous Cube, the Dragon, Storm of Arrows/Wider Burst/Stormcaller, Zealotry all stay exactly as designed) — the fields below only tune the roll/floor/overcap/ladder LAYER shared by every splash site, on top of each caller's own base.</p>\
            <div class=\"tunable-row\">\
              <label for=\"splash_extra_targets\">Base Extra Targets (Player)</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"splash_extra_targets\" name=\"splash_extra_targets\" value=\"{splash_extra_targets}\">\
              <p class=\"tunable-hint\">How many extra targets a successful roll grants for the player-side base mechanics (a normal attack/heal's own splash). Boss-side bases (Cube, Dragon, default cleave) are their own separate constants, not this field.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"splash_support_floor_targets\">Support Floor Targets</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"splash_support_floor_targets\" name=\"splash_support_floor_targets\" value=\"{splash_support_floor_targets}\">\
              <p class=\"tunable-hint\">The four SUPPORT sites' floor on a missed roll or 0% splash — a zero-splash character still affects this many targets, never zero.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"splash_overcap_bonus_targets\">Overcap Bonus Targets</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"splash_overcap_bonus_targets\" name=\"splash_overcap_bonus_targets\" value=\"{splash_overcap_bonus_targets}\">\
              <p class=\"tunable-hint\">Extra targets added on top of a caller's own base once splash exceeds 100% — guaranteed, no roll.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"splash_ladder_step_pct\">Ladder Step (splash %)</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"splash_ladder_step_pct\" name=\"splash_ladder_step_pct\" value=\"{splash_ladder_step_pct}\">\
              <p class=\"tunable-hint\">Every full step of splash % beyond 100% adds another ladder rung (default 1000, i.e. every 1000% splash). 0 disables the ladder entirely.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"splash_ladder_targets_per_step\">Ladder Targets Per Step</label>\
              <input type=\"number\" step=\"1\" min=\"0\" id=\"splash_ladder_targets_per_step\" name=\"splash_ladder_targets_per_step\" value=\"{splash_ladder_targets_per_step}\">\
              <p class=\"tunable-hint\">Extra targets granted per ladder rung reached.</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"splash_damage_pct\">Splash Damage %</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"splash_damage_pct\" name=\"splash_damage_pct\" value=\"{splash_damage_pct}\">\
              <p class=\"tunable-hint\">Fraction of the primary hit/heal's own amount each splash target takes (attack splash only — the four support sites apply their own already-full-value effect regardless of this field).</p>\
            </div>\
            <div class=\"tunable-row\">\
              <label for=\"verdantburst_echo_threshold_pct\">Verdant Burst Echo Threshold</label>\
              <input type=\"number\" step=\"any\" min=\"0\" id=\"verdantburst_echo_threshold_pct\" name=\"verdantburst_echo_threshold_pct\" value=\"{verdantburst_echo_threshold_pct}\">\
              <p class=\"tunable-hint\">Druid's Verdant Burst saves a dying ally when the Druid's own Echo chance (as a fraction — 1.0 = 100%) is at or above this. Deterministic, not a roll.</p>\
            </div>\
            <h2>Live Overlay Broadcast</h2>\
            <div class=\"tunable-row\">\
              <label for=\"buffsnapshot_dedupe_window_ms\">Buff Snapshot Dedupe Window (ms)</label>\
              <input type=\"number\" step=\"1\" min=\"1\" id=\"buffsnapshot_dedupe_window_ms\" name=\"buffsnapshot_dedupe_window_ms\" value=\"{buffsnapshot_dedupe_window_ms}\">\
              <p class=\"tunable-hint\">Only the newest live buff/debuff snapshot per unit within a window this wide gets broadcast to the overlay — the desktop companion app's live Buffs & Debuffs pane only ever reads the newest one anyway. Wider cuts broadcast volume; narrower gets fresher (but noisier) updates. Default 1000ms.</p>\
            </div>\
            <button class=\"btn\" type=\"submit\">Save</button>\
          </form>\
        </div>\
        <div class=\"card\">\
          <h2>Operator Controls</h2>\
          <p class=\"muted\">The web equivalent of the mod-only <code>!nextencounter</code> — runs one encounter right now instead of waiting for the timer. Every refusal is reported back with its reason; a refused press never queues a fight to happen later.</p>\
          <form method=\"post\" action=\"/admin/ops/next-encounter\">\
            <div class=\"tunable-row\">\
              <label for=\"ops_boss\">Boss</label>\
              <select id=\"ops_boss\" name=\"boss\">{boss_options}</select>\
              <p class=\"tunable-hint\">Random rolls the normal pick. Naming one forces exactly that boss regardless of stage or rotation — the same thing <code>!nextencounter &lt;name&gt;</code> does. Refused while any fight is already in flight, so this can't stack a bonus fight onto the end of one.</p>\
            </div>\
            <button class=\"btn\" type=\"submit\">Trigger Encounter Now</button>\
          </form>\
        </div>\
        <div class=\"card\">\
          <h2>📌 Pinned Fights</h2>\
          <p class=\"muted\">Mod tool <code>!pinfight</code> copies the most recent coarse-tier and detail-tier fight files here, immune to the normal rolling-window pruning — bug-report evidence that survives past the 3-5 file window until someone deletes it by hand.</p>\
          {pinned_fights_html}\
        </div>",
        boss_options = boss_options,
        loot_mult = t.loot_mult,
        sand_mult = t.sand_mult,
        wings_drop_chance = t.wings_drop_chance,
        celestial_shard_drop_chance = t.celestial_shard_drop_chance,
        boss_health = t.boss_health,
        boss_power = t.boss_power,
        hp_pacing_readout = hp_pacing_readout,
        dmg_pacing_readout = dmg_pacing_readout,
        dynamic_pacing_enabled_checked = if t.dynamic_pacing_enabled { " checked" } else { "" },
        pacing_window_fights = t.pacing_window_fights,
        target_duration_min_s = t.target_duration_min_s,
        target_duration_max_s = t.target_duration_max_s,
        hp_max_step_per_fight = t.hp_max_step_per_fight,
        hp_multiplier_floor = t.hp_multiplier_floor,
        hp_multiplier_ceiling = t.hp_multiplier_ceiling,
        hp_relax_after_losses = t.hp_relax_after_losses,
        hp_relax_step_per_fight = t.hp_relax_step_per_fight,
        target_win_loss_ratio = t.target_win_loss_ratio,
        dmg_max_step_per_fight = t.dmg_max_step_per_fight,
        dmg_multiplier_floor = t.dmg_multiplier_floor,
        dmg_multiplier_ceiling = t.dmg_multiplier_ceiling,
        baseline_stage_anchors_csv = baseline_stage_anchors_csv,
        baseline_hp_anchors_csv = baseline_hp_anchors_csv,
        baseline_atk_anchors_csv = baseline_atk_anchors_csv,
        hp_mult_override_label = format!("{:.3}x current", pacing.hp_mult),
        dmg_mult_override_label = format!("{:.3}x current", pacing.dmg_mult),
        top_layer_enabled_checked = if t.top_layer_enabled { " checked" } else { "" },
        top_layer_cap_pct = t.top_layer_cap_pct,
        top_layer_half_stage = t.top_layer_half_stage,
        boss_count_tier_stages = t.boss_count_tier_stages,
        boss_count_cap_mult = t.boss_count_cap_mult,
        sand_drop_stage = t.sand_drop_stage,
        perfect_item_stage = t.perfect_item_stage,
        divine_dust_drop_stage = t.divine_dust_drop_stage,
        sacred_item_stage = t.sacred_item_stage,
        drop_stage_min = crate::adventure::DROP_STAGE_MIN,
        drop_stage_max = crate::adventure::DROP_STAGE_MAX,
        pierce_cap = t.pierce_cap,
        pierce_h = t.pierce_h,
        fight_summary_batch_size = t.fight_summary_batch_size,
        thunder_redistribution_pct = t.thunder_redistribution_pct,
        thunder_redistribution_window_secs = t.thunder_redistribution_window_secs,
        reactive_proc_cap_ms = t.reactive_proc_cap_ms,
        divine_dust_drop_chance = t.divine_dust_drop_chance,
        divine_dust_disenchant_chance = t.divine_dust_disenchant_chance,
        divine_dust_craft_dust_cost = t.divine_dust_craft_dust_cost,
        divine_dust_craft_sand_cost = t.divine_dust_craft_sand_cost,
        divine_dust_craft_output = t.divine_dust_craft_output,
        craft_base_cost_mult = t.craft_base_cost_mult,
        craft_base_cost_mult_min = trim_float(crate::adventure::CRAFT_BASE_COST_MULT_MIN),
        craft_base_cost_mult_max = trim_float(crate::adventure::CRAFT_BASE_COST_MULT_MAX),
        craft_tier_exponent = t.craft_tier_exponent,
        craft_tier_exponent_min = trim_float(crate::adventure::CRAFT_TIER_EXPONENT_MIN),
        craft_tier_exponent_max = trim_float(crate::adventure::CRAFT_TIER_EXPONENT_MAX),
        craft_tier_bump_mult = t.craft_tier_bump_mult,
        craft_tier_bump_mult_min = trim_float(crate::adventure::CRAFT_TIER_BUMP_MULT_MIN),
        craft_tier_bump_mult_max = trim_float(crate::adventure::CRAFT_TIER_BUMP_MULT_MAX),
        rf_self_damage_pct_rank1 = t.rf_self_damage_pct_rank1,
        rf_self_damage_pct_rank2 = t.rf_self_damage_pct_rank2,
        rf_self_damage_pct_rank3 = t.rf_self_damage_pct_rank3,
        haloedsteps_per_instance_pct_rank1 = t.haloedsteps_per_instance_pct_rank1,
        haloedsteps_per_instance_pct_rank2 = t.haloedsteps_per_instance_pct_rank2,
        haloedsteps_per_instance_pct_rank3 = t.haloedsteps_per_instance_pct_rank3,
        permanent_rampage_checked = if t.permanent_rampage { " checked" } else { "" },
        win_xp_flat = t.win_xp_flat,
        win_xp_level_pct = t.win_xp_level_pct,
        win_xp_mult = t.win_xp_mult,
        win_xp_cooldown_secs = t.win_xp_cooldown_secs,
        win_xp_catchup_enabled_checked = if t.win_xp_catchup_enabled { " checked" } else { "" },
        win_xp_flat_max = crate::adventure::WIN_XP_FLAT_MAX,
        win_xp_level_pct_max = crate::adventure::WIN_XP_LEVEL_PCT_MAX,
        win_xp_mult_min = crate::adventure::WIN_XP_MULT_MIN,
        win_xp_mult_max = crate::adventure::WIN_XP_MULT_MAX,
        win_xp_cooldown_secs_max = crate::adventure::WIN_XP_COOLDOWN_SECS_MAX,
        shattering_enabled_checked = if t.shattering_enabled { " checked" } else { "" },
        shattering_damage_pct_rank1 = t.shattering_damage_pct_rank1,
        shattering_damage_pct_rank2 = t.shattering_damage_pct_rank2,
        shattering_damage_pct_rank3 = t.shattering_damage_pct_rank3,
        defensive_stat_hard_cap = t.defensive_stat_hard_cap,
        enemy_hp_pool_hard_cap = t.enemy_hp_pool_hard_cap,
        enemy_hp_pool_cap_min = crate::adventure::pacing::ENEMY_HP_POOL_CAP_MIN,
        enemy_hp_pool_cap_max = crate::adventure::pacing::ENEMY_HP_POOL_CAP_MAX,
        splash_extra_targets = t.splash_extra_targets,
        splash_support_floor_targets = t.splash_support_floor_targets,
        splash_overcap_bonus_targets = t.splash_overcap_bonus_targets,
        splash_ladder_step_pct = t.splash_ladder_step_pct,
        splash_ladder_targets_per_step = t.splash_ladder_targets_per_step,
        splash_damage_pct = t.splash_damage_pct,
        verdantburst_echo_threshold_pct = t.verdantburst_echo_threshold_pct,
        buffsnapshot_dedupe_window_ms = t.buffsnapshot_dedupe_window_ms,
        overflow_conversion_cap_per_rank = t.overflow_conversion_cap_per_rank,
        evasion_overflow_cap = t.evasion_overflow_cap,
        block_overflow_cap = t.block_overflow_cap,
        dr_overflow_cap = t.dr_overflow_cap,
        intervene_overflow_cap = t.intervene_overflow_cap,
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
            "<span class=\"top-nav-stats\">Lv {level} {archetype:?} · 💰 {dust} · \u{1FAB5} {sand} · ✨ {divine_dust}</span>",
            level = c.level,
            archetype = c.archetype,
            dust = format_number(c.dust as f64),
            sand = format_number(c.sand as f64),
            divine_dust = format_number(c.divine_dust as f64),
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
          <a class=\"top-nav-link\" href=\"/bugs\">🐞 Report a Bug</a>\
          {stats}\
        </div>"
    )
}

/// The dashboard's announcement feed card (World 2 Stage 2, 2026-08-28):
/// the web home for the narration that used to exist only in Twitch
/// chat. `lines` is `AdventureManager::recent_announcements()`, oldest
/// first; this renders them NEWEST first, which is also the end the
/// `/ws` client prepends to (see `base.html`).
///
/// Rendered SERVER-SIDE on purpose: the card is correct before any
/// script runs and stays correct for a client whose socket never
/// connects at all. The WebSocket updates this list; it does not create
/// it.
fn render_announcement_feed(lines: &[String]) -> String {
    let cap = crate::adventure::ANNOUNCEMENT_FEED_CAP;
    let items = if lines.is_empty() {
        "<li class=\"announcement-empty muted\">Nothing yet — the feed fills up as fights resolve.</li>".to_string()
    } else {
        lines.iter().rev().map(|line| format!("<li>{}</li>", escape_html(line))).collect::<String>()
    };
    format!(
        "<div class=\"card\">\
           <div class=\"header-row\"><h2>📣 Feed</h2><span class=\"announcement-status muted\" id=\"announcement-status\"></span></div>\
           <ul class=\"announcement-feed\" id=\"announcement-feed\" data-cap=\"{cap}\">{items}</ul>\
         </div>"
    )
}

fn render_dashboard(
    login: &str,
    display_name: &str,
    character: Option<&Character>,
    reforge_used_this_hour: bool,
    reforge_next_reset_ms: u64,
    tunables: &LiveTunables,
    announcements: &[String],
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

    let announcement_feed_html = render_announcement_feed(announcements);

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

    let combat_stats_html = render_combat_stats_card(c, tunables);

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
                    <div class=\"stat\"><div class=\"stat-label\">Divine Dust</div><div class=\"stat-value\">{divine_dust}</div></div>\
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
            {announcement_feed_html}\
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
        divine_dust = format_number(c.divine_dust as f64),
    )
}

/// `/inventory` page body - Bag and Crafting, split out of the main
/// dashboard (see `render_dashboard`'s doc). Shows the "haven't joined"
/// prompt if `character` is `None`, same as the dashboard does, since
/// this page is reachable straight from `top_nav`'s links regardless of
/// join state.
fn render_inventory_page(display_name: &str, character: Option<&Character>, pending_veil: Option<&PendingVeil>, tunables: &LiveTunables, divine_dust_unlocked: bool) -> String {
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
    // disenchant-protected items don't count (see
    // Character::disenchant_all_from_inventory; Krangled items DO count,
    // 2026-08-18 - Krangle's lock only ever blocked further crafting,
    // never disenchanting), so an all-"Keep"-marked bag doesn't get a
    // button that would just do nothing.
    let disenchantable_count = c.inventory.iter().filter(|i| !i.disenchant_protected).count();
    let disenchant_all_html = if disenchantable_count == 0 {
        String::new()
    } else {
        format!(
            "<form method=\"post\" action=\"/disenchant-all\" onsubmit=\"return confirm('Disenchant all {disenchantable_count} eligible item(s) for dust? \\'Keep\\'-marked items are skipped. This can\\'t be undone.');\">\
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
                // Legacy CelestialShard's token name doesn't self-describe
                // what it grants (unlike Scour/Krangle/etc, which are
                // self-explanatory) - spell out the unique affix right in
                // the pill so a held token isn't a mystery. UniqueShard
                // (2026-08-19, Unified Unique Shards) no longer grants one
                // fixed thing - the player picks at apply time - so it
                // gets no such suffix any more, same as every other
                // action that isn't a fixed single-outcome grant.
                let grants = match action {
                    CraftAction::CelestialShard => " \u{2192} Celestial Conversion",
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
        None => render_crafting_card(c, tunables, divine_dust_unlocked),
    };

    // Combat Stats (2026-08-17, a live request) - same shared card the
    // main dashboard uses (see render_combat_stats_card), so a player
    // crafting/reforging can see whether it actually moved a number
    // without tabbing back to "/".
    let combat_stats_html = render_combat_stats_card(c, tunables);

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
        Archetype::Elementalist => "🔥",
        Archetype::Commoner => "❔",
    };
    (icon, role)
}

/// The "this node has been retuned" line appended to a node's tooltip
/// wherever a live override is in effect (2026-08-19) - `None` for a
/// node still at its compiled-in values, which is every node until an
/// admin changes one.
///
/// Exists because a node's `description` is a hardcoded prose string
/// with its numbers baked in: it cannot reflect an override, so
/// retuning a value would otherwise leave every tooltip and the wiki
/// quietly stating the old figure. Rather than templating all 471
/// description strings (a far larger content migration, deliberately
/// deferred - see docs/passive_tunables_spec.md), this states the real
/// per-rank numbers alongside the untouched prose, so a tuned node is
/// visibly tuned rather than silently divergent.
///
/// `pub(crate)` and deliberately free-standing so the wiki's own node
/// renderer (`adventure_web::wiki::render_wiki_archetype_graph`, a
/// parallel session's file this session must not edit) can adopt the
/// identical line by calling it - see the WIKI_IMPACT.md entry.
pub(crate) fn passive_override_note(node: &PassiveNode) -> Option<String> {
    let overrides = passive_overrides();
    if !overrides.has_override(node.key) {
        return None;
    }
    // Every node has at most 3 distinct magnitudes (a Specialization's
    // 4th point is unlock-only), so the line always reads as 3 values.
    let fmt = |f: fn(&PassiveNode, u32) -> f64| -> String {
        (1..=3).map(|r| trim_float(f(node, r))).collect::<Vec<_>>().join(" / ")
    };
    let tuned = fmt(|n, r| n.magnitude_at_rank(r));
    let default = fmt(|n, r| n.magnitude_at_rank_with(r, &crate::adventure::PassiveOverrides::default()));
    if tuned == default {
        // An override that happens to restate the defaults exactly is
        // not worth a line - it changes nothing the player would notice.
        return None;
    }
    Some(format!("<span class=\"passive-tuned\">Tuned: {tuned} (default {default})</span>"))
}

/// Formats a magnitude for display without trailing-zero noise - `0.5`
/// rather than `0.50000000000000001`, `3` rather than `3.0`. Values in
/// this tree are small decimals and small counts, so 4 significant
/// decimals is plenty and reads far better than `{:?}` on an `f64`.
fn trim_float(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_string() } else { s.to_string() }
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
        // Live-tunable values (2026-08-19): a node's description is a
        // hardcoded prose string and cannot know it has been retuned, so
        // an overridden node gets a generated line stating its real
        // numbers rather than silently contradicting its own text.
        // Absent unless an override exists, so an untuned tree reads
        // exactly as it did before. See `passive_override_note`.
        let tip = match passive_override_note(n) {
            Some(note) => format!("{tip} {note}"),
            None => tip,
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

/// The Memories card on `/passives` (2026-08-19) - one row per slot,
/// showing what's saved and offering Load/Rename/Delete, or a name field
/// and Save Current Build for an empty one.
///
/// Rendered for every non-Commoner character rather than hidden until
/// first use (unlike the golem/Split Personality sections, which gate on
/// something the player may not have): empty slots ARE the feature's
/// entry point, so hiding them would hide the feature.
///
/// **Escaping**: a Memory name is player-authored text. `escape_html`
/// does not escape `'`, and minijinja autoescaping is off for this
/// template (see `render::render_template`'s doc), so every name goes
/// into a DOUBLE-quoted attribute or element text only - never a
/// single-quoted attribute, and never interpolated into the inline
/// `confirm()` string, whose text is deliberately static.
fn render_memories_section(c: &Character) -> String {
    let slots = c.memories_padded();
    let rows: String = slots
        .iter()
        .enumerate()
        .map(|(slot, saved)| {
            let number = slot + 1;
            match saved {
                Some(memory) => {
                    let name = escape_html(&memory.name);
                    let spent: u32 = memory.passive_allocations.values().sum::<u32>() + memory.secondary_passive_allocations.values().sum::<u32>();
                    let class_line = match memory.secondary_archetype {
                        Some(secondary) => format!("{:?} &amp; {secondary:?}", memory.archetype),
                        None => format!("{:?}", memory.archetype),
                    };
                    format!(
                        "<div class=\"memory-slot filled\">\
                           <div class=\"memory-head\">\
                             <span class=\"memory-number\">{number}</span>\
                             <span class=\"memory-name\">{name}</span>\
                           </div>\
                           <div class=\"memory-meta\">{class_line} &middot; {spent} point{plural} spent</div>\
                           <div class=\"memory-actions\">\
                             <form method=\"post\" action=\"/passives/memories/load\">\
                               <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                               <button class=\"btn-sm\" type=\"submit\">Load</button>\
                             </form>\
                             <form method=\"post\" action=\"/passives/memories/save\" onsubmit=\"return confirm('Overwrite this Memory with your current build?');\">\
                               <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                               <input type=\"hidden\" name=\"name\" value=\"{name}\">\
                               <button class=\"btn-sm\" type=\"submit\">Overwrite</button>\
                             </form>\
                             <form method=\"post\" action=\"/passives/memories/rename\" class=\"memory-rename\">\
                               <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                               <input type=\"text\" name=\"name\" value=\"{name}\" maxlength=\"{MEMORY_NAME_MAX_LEN}\" aria-label=\"Rename Memory {number}\">\
                               <button class=\"btn-sm\" type=\"submit\">Rename</button>\
                             </form>\
                             <form method=\"post\" action=\"/passives/memories/delete\" onsubmit=\"return confirm('Delete this Memory? This cannot be undone.');\">\
                               <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                               <button class=\"btn-sm btn-danger\" type=\"submit\">Delete</button>\
                             </form>\
                           </div>\
                         </div>",
                        plural = if spent == 1 { "" } else { "s" },
                    )
                }
                None => {
                    let placeholder = escape_html(&default_memory_name(c.archetype, c.effective_secondary_archetype()));
                    format!(
                        "<div class=\"memory-slot empty\">\
                           <div class=\"memory-head\">\
                             <span class=\"memory-number\">{number}</span>\
                             <span class=\"memory-name muted\">Empty slot</span>\
                           </div>\
                           <form method=\"post\" action=\"/passives/memories/save\" class=\"memory-actions\">\
                             <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                             <input type=\"text\" name=\"name\" placeholder=\"{placeholder}\" maxlength=\"{MEMORY_NAME_MAX_LEN}\" aria-label=\"Name for Memory {number}\">\
                             <button class=\"btn-sm\" type=\"submit\">Save Current Build</button>\
                           </form>\
                         </div>"
                    )
                }
            }
        })
        .collect();

    format!(
        "<div class=\"ptree-memories\">\
          <div class=\"masthead\">\
            <h1>Memories</h1>\
            <p class=\"subhead\">Save your whole build - class, both trees, and golem types - and swap back to it later for free. \
            Loading is free and skips the usual respec and class-change costs, but only works outside a fight. \
            Points you've earned since saving are left unspent for you to place.</p>\
          </div>\
          <div class=\"memory-slots\">{rows}</div>\
        </div>"
    )
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
    let memories_section = render_memories_section(c);
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

    // Elementalist's Golem Master slot-type picker (docs/
    // elementalist_spec.md, Stage 5) - one dropdown per UNLOCKED slot
    // (Golem Master's own current rank), kept minimal and localized to
    // this page per the spec's own scoping. Entirely absent unless
    // playing Elementalist with at least 1 point in Golem Master - same
    // "hidden, not just disabled" reasoning as Split Personality's own
    // section below.
    let golem_slot_section = if c.archetype == Archetype::Elementalist {
        let unlocked_slots = c.passive_node_count("golemmaster");
        if unlocked_slots == 0 {
            String::new()
        } else {
            let all_types = [GolemType::Basic, GolemType::Thunder, GolemType::Flame, GolemType::Water];
            let pickers: String = (0..unlocked_slots)
                .map(|slot| {
                    let current = c.golem_slot_types.get(slot as usize).copied().unwrap_or_default();
                    let options: String = all_types
                        .iter()
                        .map(|&t| {
                            let value = format!("{t:?}").to_lowercase();
                            let selected = if t == current { " selected" } else { "" };
                            format!("<option value=\"{value}\"{selected}>{t:?}</option>")
                        })
                        .collect();
                    format!(
                        "<form method=\"post\" action=\"/passives/set-golem-type\" class=\"golem-slot-picker\">\
                          <input type=\"hidden\" name=\"slot\" value=\"{slot}\">\
                          <label>Golem {slot_label}</label>\
                          <select name=\"golem_type\">{options}</select>\
                          <button class=\"btn-sm\" type=\"submit\">Set</button>\
                        </form>",
                        slot_label = slot + 1,
                    )
                })
                .collect();
            format!(
                "<div class=\"golem-slots\">\
                  <div class=\"masthead\">\
                    <h1>Golem Slots</h1>\
                    <p class=\"subhead\">Golem Master grants {unlocked_slots} summon slot{plural} - assign each one a type. Basic has no bonuses of its own.</p>\
                  </div>\
                  {pickers}\
                </div>",
                plural = if unlocked_slots == 1 { "" } else { "s" },
            )
        }
    } else {
        String::new()
    };

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
            <p class=\"subhead\">1 point from the start, +1 every 4 levels. Points spent on a node marked (inactive) are banked, not wasted - they'll activate once that mechanic ships.</p>\
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
              <p class=\"points-formula\">1 point from the start, +1 every 4 levels &mdash; level {level} gives you {total_points} to spend.</p>\
              {free_respec_note}\
            </div>\
          </div>\
          {memories_section}\
          {primary_meta_legend}\
          {primary_body}\
          {golem_slot_section}\
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
    // A Krangled item and a "Keep"-ticked one both refuse every craft now
    // (2026-08-24), so both need to say so in the picker - otherwise the
    // only feedback is an error popup after the click. Same padlock, and
    // the trailing word is what separates them: Krangle's lock is
    // permanent, Keep is the player's own tick-box on the item's card.
    let lock = if item.locked {
        " \u{1F512}".to_string()
    } else if item.disenchant_protected {
        " \u{1F512} Keep".to_string()
    } else {
        String::new()
    };
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
        "<option value=\"{id}\" data-affixes=\"{mods}\" data-tier=\"{tier}\" data-quality=\"{quality:.0}\" data-perfect=\"{perfect}\" data-polish-room=\"{polish_room}\" data-sacred=\"{sacred}\"{selected}>{name} ({slot_prefix}T{tier}, {mods} mod{plural}{quality_tag}){lock}{unique_mark}</option>",
        id = item.id,
        name = escape_html(&item.display_name()),
        tier = item.tier,
        mods = mods,
        plural = if mods == 1 { "" } else { "s" },
        perfect = if item.perfect { "1" } else { "0" },
        polish_room = if item.has_polish_room() { "1" } else { "0" },
        sacred = if item.sacred_affix.is_some() { "1" } else { "0" },
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
            "Legacy currency, no longer earnable \u{2014} Celestial Shard merged into Unique Shard. Held tokens are safe and still usable, but nothing drops these any more."
        }
        CraftAction::UniqueShard => {
            "Consumes a Unique Shard to grant a unique affix, shown above the item's tier \u{2014} outside the normal 4-modifier cap and unaffected by any other crafting. Pick which effect at apply time: Celestial Conversion (bonus damage/follow-up hit) or Split Personality (invest points into a 2nd class on /passives). An item can only ever have one; a Krangled item can't receive one, and a unique item can't be Krangled. Needs an actual Unique Shard \u{2014} can't be bought with dust."
        }
        CraftAction::Polishing => {
            "Costs sand, not dust \u{2014} 1 per 10% quality (12 for a Perfect item). Raises the item's own quality by 5% and bumps one random modifier's roll by 5% of its range, both capped at the max. On an already-Perfect item (nothing left to raise on quality), instead bumps up to 2 random modifiers' rolls by 5%."
        }
        CraftAction::Reforge => {
            "Rerolls this item to a new (usually higher) tier, same as the Reforge Gear channel points reward, but costs dust instead \u{2014} 30 per tier of the item \u{2014} and targets this specific item, with a small chance at a bonus modifier."
        }
        CraftAction::DivineDust => {
            "Costs Divine Dust, not dust or sand \u{2014} 2 per tier of the item. Not yet Sacred: makes it Sacred (also Perfect, if it wasn't already) and grants one random sacred affix. Already Sacred: rerolls its sacred affix to a different one."
        }
    }
}

const RECOMBINE_TIP: &str = "Forges item A and item B (same slot) into one new item, consuming both. New tier = the two items' average tier, rounded down, +1. Each source's own modifiers each independently have a 50% chance to carry over (max 4 total on the result); any modifier TYPE both items already share is guaranteed to carry over instead, keeping whichever of the two rolled values is higher. The result's quality is a coin flip between the two source items' own quality rolls. Free by default. Checking Veil (+dust) guarantees EVERY modifier carries over (same 4 cap) and keeps the BETTER of the two quality rolls instead of a coin flip, for 500 dust per combined modifier on top of the veil surcharge.";

const VEIL_TIP: &str = "Turns this craft's randomness into a choice: pay extra dust up front and get 3 independently-rolled outcomes to pick from, instead of one outcome applied automatically. A banked free token always veils at no extra cost. Scour has nothing to pick between, so veiling it does nothing.";

const HIDEOUT_WARRIOR_TIP: &str = "Runs Transmute \u{2192} Augment \u{2192} Regal \u{2192} Exalt \u{2192} Krangle on this item in order, skipping any step that isn't eligible right now, paying each step's normal dust cost as it goes \u{2014} always in full, never a banked token. Stops early if you run out of dust; whatever already landed stays. Never veiled, regardless of the checkbox above. Reaching the Krangle step permanently locks the item, same as using Krangle directly \u{2014} uncheck \"Include Krangle\" to stop after Exalt and leave the item unlocked.";

/// Divinity (2026-08-24) - the whole-bag run. Says "ignores the item
/// pickers" explicitly because this button sits in the same `<form>` as
/// six per-item actions and is the only one there that does not act on
/// the selection.
const DIVINITY_TIP: &str = "Costs one Unique Shard and runs the whole Hideout Warrior chain \u{2014} Transmute \u{2192} Augment \u{2192} Regal \u{2192} Exalt \u{2192} Krangle \u{2014} over EVERY eligible item in your bag at once, paying no dust at all. Ignores the item pickers above: this is a whole-bag action, not a per-item one. Equipped gear is never touched. Items already Krangled or ticked \u{1F512} Keep are skipped, not refused, and everything Krangle lands on is permanently locked and auto-named \u{201C}From Divinity\u{201D}. One shard per use \u{2014} there is no x10.";

/// The Divine Dust craft recipe row (docs/divine_dust_spec.md) - a
/// separate, standalone `<form>` from the main item-crafting one below
/// (its own `times` x1/x10/x50 radio group under the SAME `name`, which
/// is safe precisely because it's a different `<form>` element - each
/// form only ever submits its own descendant inputs). Deliberately not
/// gated on the character owning any items at all: this recipe converts
/// dust+sand into Divine Dust and never touches an item, so unlike every
/// other action on this card it has nothing to be empty-inventory-gated
/// on (2026-08-19, explicit requirement - always visible).
fn render_divine_dust_recipe_row(c: &Character, tunables: &LiveTunables, unlocked: bool) -> String {
    let dust_cost = tunables.divine_dust_craft_dust_cost;
    let sand_cost = tunables.divine_dust_craft_sand_cost;
    let output = tunables.divine_dust_craft_output;
    // Locked (2026-09-02) - shown, not hidden. A recipe the group has yet
    // to unlock is information players want ("what am I working toward"),
    // and hiding it would make the row's own always-visible requirement
    // above read as a bug the first time someone below the threshold looked
    // for it. `craft_divine_dust` enforces this server-side regardless of
    // what this renders - a stale page cannot craft past the latch.
    if !unlocked {
        let stage = tunables.divine_dust_drop_stage;
        let locked_tip = format!(
            "Unlocks when the group reaches stage {stage}. Once unlocked it stays unlocked \u{2014} a later stage loss can't take it away."
        );
        return format!(
            "<div class=\"craft-actions\">\
              <span class=\"muted\" data-tip=\"{tip}\">\u{1F512} Craft Divine Dust \u{2014} unlocks at stage {stage}</span>\
            </div>",
            tip = escape_html(&locked_tip),
        );
    }
    let cost_tip = "Costs dust + sand, not Divine Dust itself \u{2014} a currency conversion, not an apply/reroll. x1/x10/x50 repeats the whole recipe that many times, stopping early (keeping whatever already landed) if you run out of either currency partway through.";
    format!(
        "<form method=\"post\" action=\"/craft\">\
          <input type=\"hidden\" name=\"action\" value=\"divine dust craft\">\
          <div class=\"craft-actions\">\
            <span class=\"muted\" data-tip=\"{cost_tip}\">Craft Divine Dust ({dust_cost}d + {sand_cost}s \u{2192} {output} \u{2728}):</span>\
            <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"1\" checked> x1</label>\
            <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"10\"> x10</label>\
            <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"50\"> x50</label>\
            <button class=\"btn-sm\" type=\"submit\"{disabled}>Craft</button>\
          </div>\
        </form>",
        disabled = if c.dust < dust_cost || c.sand < sand_cost { " disabled" } else { "" },
    )
}

fn render_crafting_card(c: &Character, tunables: &LiveTunables, divine_dust_unlocked: bool) -> String {
    let items = all_items(c);
    let divine_dust_recipe_html = render_divine_dust_recipe_row(c, tunables, divine_dust_unlocked);
    if items.is_empty() {
        return format!(
            "<div class=\"card\" id=\"crafting-card\">\
              <div class=\"header-row\"><h2>Crafting</h2><span class=\"dust-available\">💰 {dust} dust · \u{1FAB5} {sand} sand · ✨ {divine_dust} Divine Dust</span></div>\
              {divine_dust_recipe_html}\
            </div>",
            dust = format_number(c.dust as f64),
            sand = format_number(c.sand as f64),
            divine_dust = format_number(c.divine_dust as f64),
        );
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
        //
        // UniqueShard joined them 2026-09-02. It is the one action here
        // whose price cannot be re-earned by grinding dust: the shard is
        // consumed outright, binds a unique affix to the item it lands
        // on, and that item can never take a second. Every other button
        // on this row spends a currency the player can go and farm more
        // of, which is why this one was worth gating and Transmute is
        // not. The message stays the shared item-naming default rather
        // than a `data-confirm-msg` override - the whole point of the
        // 2026-08-15 request was that the dialog SAY WHICH ITEM, and a
        // static override would drop exactly that (see Divinity, which
        // overrides only because it is whole-bag and has no item to
        // name).
        let confirm_attr =
            if matches!(action, CraftAction::Krangle | CraftAction::Scour | CraftAction::Annulment | CraftAction::Chancing | CraftAction::UniqueShard) { " data-confirm=\"1\"" } else { "" };
        let tokens = c.craft_token_count(action);
        if tokens > 0 {
            return format!(
                "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"{value}\" data-tip=\"{tip}\"{confirm_attr}>{label} (Free — {tokens} token{s})</button>",
                value = action.label().to_lowercase(),
                label = action.label(),
                s = if tokens == 1 { "" } else { "s" },
            );
        }
        // Both the flat fee and the veil surcharge go through
        // `scaled_base_cost` here for exactly the same reason
        // `craft_item_ex` does: the panel's preview and the charge must
        // read the one live `craft_base_cost_mult`, never a second copy of
        // the arithmetic. The per-tier half of the price is added
        // client-side (it depends on which item is selected) - see the
        // `data-tier-mult`/`data-tier-exp` attributes below.
        let base = scaled_base_cost(action.base_cost(), tunables.craft_base_cost_mult);
        let disabled = if c.dust < base { " disabled" } else { "" };
        // The per-tier half of the price, as parameters rather than as a
        // second copy of the formula: `templates/base.html` recomputes
        // `ceil(mult x tier^exp)` off whichever item is currently
        // selected. Until 2026-09-02 that script carried its own
        // hardcoded `var TIER_CRAFT_DUST_COST = 3`, which would have gone
        // on quoting the old price after this change while the server
        // charged the new one.
        let tier_attrs = format!(" data-tier-mult=\"{TIER_CRAFT_DUST_COST}\" data-tier-exp=\"{}\"", tunables.craft_tier_exponent);
        // Scour has nothing to pick between when veiled (is_veilable() is
        // false for it) - omitting data-veil-extra is what tells the
        // preview script to leave its cost alone regardless of the
        // checkbox.
        let veil_attr = if action.is_veilable() {
            format!(" data-veil-extra=\"{}\"", scaled_base_cost(VEIL_EXTRA_COST, tunables.craft_base_cost_mult))
        } else {
            String::new()
        };
        format!(
            "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"{value}\" data-base=\"{base}\" data-label=\"{label}\" data-tip=\"{tip}\"{tier_attrs}{veil_attr}{disabled}{confirm_attr}>{label} ({base}d)</button>",
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
    // Divine Dust apply/reroll (2026-08-19) - same "price depends on the
    // selected item" shape as Polish/Reforge above (2 x item_a's own
    // tier, in Divine Dust rather than sand/dust), so its cost text is
    // also computed client-side (see updateSpecialCosts' own
    // data-divine-dust-apply handling) rather than server-side. Never
    // batched (x1/x10/x50 is the craft RECIPE's own thing, a separate
    // form below) - applying/rerolling a specific item one at a time is
    // the natural unit here.
    let divine_dust_apply_tip = craft_action_tip(CraftAction::DivineDust);
    let divine_dust_apply_btn = format!(
        "<button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"divine dust\" data-divine-dust-apply=\"1\" data-divine-dust=\"{}\" data-tip=\"{divine_dust_apply_tip}\">Apply Divine Dust</button>",
        c.divine_dust,
    );
    // Divinity (2026-08-24) - its own row, not another button in the
    // craft-actions row above, because every button there acts on the
    // item pickers and this one acts on the whole bag; sitting among them
    // it would read as "Divinity the selected item". Hidden entirely
    // until a Unique Shard is actually held, the same hidden-until-earned
    // shape celestial_btn/unique_shard_btn already use - a permanently
    // unaffordable button for a currency most players have never seen is
    // clutter, and it reveals itself the moment one drops.
    //
    // `plan_divinity` is pure reads (see its own doc), so calling it here
    // just to LABEL the button costs nothing and cannot mutate anything.
    // The label states the real eligible count rather than the bag size:
    // "Divinity (42 items)" when 19 of a 61-item bag are locked would be
    // a lie about what the shard is going to buy.
    let divinity_row = if c.craft_token_count(CraftAction::UniqueShard) > 0 {
        let plan = c.plan_divinity();
        let eligible = plan.targets.len();
        let skipped = plan.skipped_krangled + plan.skipped_kept;
        // Disabled rather than hidden when nothing is eligible: the
        // player HAS a shard, so the button existing-but-refusing plus a
        // reason is the honest state. `apply_divinity` refuses this same
        // case for free anyway (DivinityError::NothingEligible), so the
        // disable is a courtesy, not the safeguard.
        let disabled = if eligible == 0 { " disabled" } else { "" };
        let skipped_note = if skipped > 0 {
            format!(" <span class=\"muted\">{skipped} skipped (\u{1F512} Krangled or Keep)</span>")
        } else {
            String::new()
        };
        // Whole-bag, shard-priced and mostly irreversible, so it gets the
        // confirm gate - but NOT the item-named message the per-item
        // destructive actions use (see base.html's confirm block), which
        // would name whatever happened to be selected in a picker this
        // action ignores. `data-confirm-msg` overrides that text with one
        // that names the real scope instead.
        let confirm_msg = escape_html(&format!(
            "Run Divinity over all {eligible} eligible item{} in your bag? This spends 1 Unique Shard, Krangles most of them permanently, and cannot be undone.",
            if eligible == 1 { "" } else { "s" }
        ));
        format!(
            "<div class=\"craft-actions\">\
              <span class=\"muted\">Whole bag:</span>\
              <button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"divinity\" data-confirm=\"1\" data-confirm-msg=\"{confirm_msg}\" data-tip=\"{DIVINITY_TIP}\"{disabled}>Divinity ({eligible} item{plural}, 1 Unique Shard)</button>\
              {skipped_note}\
            </div>",
            plural = if eligible == 1 { "" } else { "s" },
        )
    } else {
        String::new()
    };
    format!(
        "<div class=\"card\" id=\"crafting-card\">\
          <div class=\"header-row\"><h2>Crafting</h2><span class=\"dust-available\">💰 {dust} dust · \u{1FAB5} {sand} sand · ✨ {divine_dust} Divine Dust</span></div>\
          <p class=\"muted\">Pick your item(s) below, then hover any button for exactly what it does.</p>\
          {divine_dust_recipe_html}\
          <form method=\"post\" action=\"/craft\">\
            <select name=\"item_a\">{options_a}</select>\
            <select name=\"item_b\">{options_b}</select>\
            <label class=\"veil-check\" data-tip=\"{VEIL_TIP}\"><input type=\"checkbox\" name=\"veiled\" value=\"1\"> Veil this craft (+{veil_extra} dust; Recombine instead costs {VEIL_EXTRA_COST} + 500 per combined modifier)</label>\
            <div class=\"craft-actions\">\
              {transmute}{augment}{regal}{exalt}{krangle}{annulment}{chancing}\
            </div>\
            <div class=\"polish-reforge-actions\">\
              <span class=\"muted\">Polish / Reforge:</span>\
              <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"1\" checked> x1</label>\
              <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"5\"> x5</label>\
              <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"10\"> x10</label>\
              <label class=\"batch-check\"><input type=\"radio\" name=\"times\" value=\"50\"> x50</label>\
              {polish_btn}{reforge_btn}\
            </div>\
            <div class=\"craft-actions\">\
              {scour}{celestial_btn}{unique_shard_btn}{divine_dust_apply_btn}\
              <button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"recombine\" data-tip=\"{RECOMBINE_TIP}\"{recombine_attrs}{recombine_disabled}>Recombine ({recombine_cost_label})</button>\
              <button class=\"btn-sm\" type=\"submit\" name=\"action\" value=\"hideout warrior\" data-confirm=\"1\" data-tip=\"{HIDEOUT_WARRIOR_TIP}\">Hideout Warrior</button>\
              <label class=\"veil-check\" data-tip=\"Leave checked to end on Krangle (permanently locks the item). Uncheck to stop after Exalt and leave it unlocked.\"><input type=\"checkbox\" name=\"hideout_krangle\" value=\"1\" checked> Include Krangle</label>\
            </div>\
            {divinity_row}\
          </form>\
        </div>",
        dust = format_number(c.dust as f64),
        sand = format_number(c.sand as f64),
        divine_dust = format_number(c.divine_dust as f64),
        veil_extra = scaled_base_cost(VEIL_EXTRA_COST, tunables.craft_base_cost_mult),
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
                // UniqueShard's picker candidates carry ONLY
                // `unique_affix_added` (no `affix_added`/`affix_removed`
                // at all) - checked before the affix-shaped match below,
                // which would otherwise render every candidate as the
                // same unhelpful "No change".
                VeilCandidate::Currency(outcome) if outcome.unique_affix_added.is_some() => {
                    let unique = outcome.unique_affix_added.expect("checked Some above");
                    format!("{} — {}", unique.name(), unique.description())
                }
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
            let class = if item.is_crit_bonus_affix(*a) { "mod-roll mod-roll-crit" } else { "mod-roll" };
            format!(
                "<li class=\"{class}\" data-tip=\"Roll: {roll_pct:.0}%\">{}</li>",
                escape_html(&affix_display(*a, *v))
            )
        })
        .collect();
    format!("<ul>{items}</ul>")
}

/// The shared page shell (`<head>`'s ~480-line `<style>` + `<body>`'s
/// ~313-line `<script>`, both 100% static across every page) now lives in
/// `templates/base.html` (2026-08-18, Phase 2) - see `render::render_template`'s
/// doc for the autoescape-off rationale. `body` is already-final raw HTML
/// from whichever page called this (unchanged since Phase 1's pilot -
/// this fn's own signature never changed, so nothing that calls it,
/// including `wiki.rs`, needed to change either).
fn render_page(body: &str) -> String {
    render::render_template("base.html", minijinja::context! { body => body })
}

/// Stage C of the Memories build (docs/memories_spec.md) - the
/// `/passives` card. Rendering only; the save/load rules themselves are
/// covered in `adventure::memory` and `AdventureManager`'s own tests.
///
/// The escaping tests here are the point of this module: a Memory name
/// is the first genuinely free-form player-authored string this page
/// renders, `escape_html` deliberately does not escape `'`, and
/// minijinja autoescaping is off for this template.
#[cfg(test)]
mod memories_render_tests {
    use super::*;

    fn warrior_with_points() -> Character {
        let mut c = Character::new("Tester".to_string());
        c.level = 40;
        c.archetype = Archetype::Warrior;
        c.passive_allocations.insert("bulwark".to_string(), 3);
        c.passive_allocations.insert("unbreakable".to_string(), 4);
        c
    }

    #[test]
    fn every_slot_renders_even_when_nothing_is_saved_yet() {
        // Empty slots ARE the feature's entry point, so unlike the golem
        // and Split Personality sections this one is never hidden.
        let html = render_memories_section(&warrior_with_points());
        // Matched with the trailing space so the `memory-slots`
        // CONTAINER (which has `memory-slot` as a substring) isn't
        // counted as a fourth row.
        assert_eq!(html.matches("class=\"memory-slot ").count(), 3, "a fresh character must see all 3 slots");
        assert_eq!(html.matches("Save Current Build").count(), 3);
        assert!(!html.contains("Load</button>"), "there is nothing to load yet");
    }

    #[test]
    fn an_empty_slot_offers_the_default_name_as_its_placeholder() {
        let html = render_memories_section(&warrior_with_points());
        assert!(html.contains("placeholder=\"Memories of a Warrior\""), "the suggested name must be pre-filled as a placeholder, got: {html}");
    }

    #[test]
    fn a_filled_slot_shows_its_name_class_and_spend() {
        let mut c = warrior_with_points();
        c.memories = vec![Some(c.snapshot_build("Tank Build".to_string(), 0))];

        let html = render_memories_section(&c);
        assert!(html.contains("Tank Build"));
        assert!(html.contains("Warrior"));
        assert!(html.contains("7 points spent"), "3 + 4 across the tree, got: {html}");
        assert!(html.contains("/passives/memories/load"));
        assert!(html.contains("/passives/memories/delete"));
    }

    #[test]
    fn a_split_personality_build_names_both_classes_in_its_summary() {
        let mut c = warrior_with_points();
        let mut memory = c.snapshot_build("Hybrid".to_string(), 0);
        memory.secondary_archetype = Some(Archetype::Druid);
        c.memories = vec![Some(memory)];

        let html = render_memories_section(&c);
        assert!(html.contains("Warrior &amp; Druid"), "both classes must show, got: {html}");
    }

    #[test]
    fn a_one_point_build_is_not_described_as_one_points() {
        let mut c = warrior_with_points();
        c.passive_allocations.clear();
        c.passive_allocations.insert("bulwark".to_string(), 1);
        c.memories = vec![Some(c.snapshot_build("Minimal".to_string(), 0))];

        assert!(render_memories_section(&c).contains("1 point spent"));
    }

    #[test]
    fn a_memory_name_containing_html_is_escaped_everywhere_it_is_rendered() {
        // A name reaches the page in two places - as element text and as
        // a double-quoted `value=` attribute on the rename field - and
        // both must be escaped. `<` and `"` are the two that matter:
        // unescaped `"` would break out of the attribute.
        let mut c = warrior_with_points();
        c.memories = vec![Some(c.snapshot_build("<script>alert(1)</script>\" onfocus=\"x".to_string(), 0))];

        let html = render_memories_section(&c);
        assert!(!html.contains("<script>"), "a name must never render as a live tag, got: {html}");
        assert!(html.contains("&lt;script&gt;"), "the name must appear escaped instead");
        assert!(!html.contains("\" onfocus=\""), "an unescaped quote would break out of the value attribute");
        assert!(html.contains("&quot; onfocus=&quot;"));
    }

    #[test]
    fn a_name_containing_an_apostrophe_never_reaches_the_inline_confirm_script() {
        // `escape_html` does NOT escape `'` (see its own doc, and the
        // one existing call site that strips quotes by hand). The
        // confirm() strings are therefore deliberately static - no name
        // is interpolated into them - so an apostrophe in a name can
        // never terminate the JS string literal and break the handler.
        let mut c = warrior_with_points();
        c.memories = vec![Some(c.snapshot_build("Bob's Build'); alert(1); //".to_string(), 0))];

        let html = render_memories_section(&c);
        assert!(html.contains("onsubmit=\"return confirm('Delete this Memory? This cannot be undone.');\""), "the confirm text must be exactly the static string");
        assert!(!html.contains("alert(1); //');"), "no part of a name may end up inside the confirm() argument, got: {html}");
    }

    #[test]
    fn the_memories_card_appears_on_the_passives_page_but_not_for_a_commoner() {
        let warrior = warrior_with_points();
        let page = render_passive_tree_page("Tester", Some(&warrior), None);
        assert!(page.contains("ptree-memories"), "the card must be on the page");
        assert!(page.contains("Save Current Build"));

        // Commoner has no tree at all, so `render_passive_tree_page`
        // early-returns before any of this - nothing to snapshot.
        let commoner = Character::new("Newbie".to_string());
        let commoner_page = render_passive_tree_page("Newbie", Some(&commoner), None);
        assert!(!commoner_page.contains("ptree-memories"), "a Commoner has no build to save");
    }

    #[test]
    fn the_points_per_level_copy_matches_the_real_formula() {
        // `points_for_level` moved from every 5 levels to every 4 on
        // 2026-08-16 and this copy never followed - it said "+1 every 5
        // levels" in two places until 2026-08-19. Pinned so the prose
        // and the formula can't drift apart silently again.
        let page = render_passive_tree_page("Tester", Some(&warrior_with_points()), None);
        assert!(!page.contains("every 5 levels"), "stale points-per-level copy is back on the page");
        assert_eq!(page.matches("+1 every 4 levels").count(), 2);
        // And the claim itself is true: level 40 -> 1 + 40/4 = 11.
        assert_eq!(crate::passive_tree::points_for_level(40), 11);
    }

    #[test]
    fn a_clean_load_produces_no_note_but_a_changed_one_explains_itself() {
        let clean = MemoryLoadReport {
            name: "Tank Build".to_string(),
            archetype: Archetype::Warrior,
            class_changed: false,
            dropped: Vec::new(),
            secondary_skipped: false,
            unspent: 0,
        };
        assert!(!clean.is_noteworthy(), "a clean same-class load must redirect silently");

        let changed = MemoryLoadReport { class_changed: true, unspent: 1, ..clean };
        assert!(changed.is_noteworthy());
        let note = memory_load_note(&changed);
        assert!(note.contains("You're now playing Warrior."));
        assert!(note.contains("1 unspent point."), "singular, not '1 unspent points' - got: {note}");
    }
}

/// Stage 1 of the live-tunable passive values build
/// (docs/passive_tunables_spec.md) - the `/admin/passives` page and the
/// tuned-value tooltip line.
///
/// Rendering and gating only. These deliberately do NOT write the
/// process-global override store: this crate runs its whole suite in one
/// process, so a test that saved an override would change what every
/// other test in the binary sees. The store's own behavior is covered as
/// pure functions in `adventure::passive_overrides`.
#[cfg(test)]
mod admin_passives_tests {
    use super::*;

    /// Every render test wants the compiled-in global cap as the
    /// fallback display - the LIVE global belongs to the handler.
    fn admin_page(archetype: Archetype, saved: bool) -> String {
        render_admin_passives_page(None, archetype, saved, crate::adventure::LiveTunables::default().overflow_conversion_cap_per_rank, None)
    }

    fn node(archetype: Archetype, key: &str) -> &'static PassiveNode {
        archetype.passive_nodes().iter().find(|n| n.key == key).unwrap_or_else(|| panic!("no node {key:?}"))
    }

    #[test]
    fn the_page_lists_every_node_in_the_selected_class_only() {
        let html = admin_page(Archetype::Warrior, false);
        for n in Archetype::Warrior.passive_nodes() {
            assert!(html.contains(n.key), "Warrior node {} must appear", n.key);
        }
        assert!(!html.contains(">arcane<"), "a Mage-only node must not leak onto the Warrior page");
    }

    #[test]
    fn a_tunable_node_gets_three_rank_inputs_and_shows_its_default() {
        let html = admin_page(Archetype::Warrior, false);
        let bulwark_default = (1..=3).map(|r| trim_float(node(Archetype::Warrior, "bulwark").magnitude_at_rank(r))).collect::<Vec<_>>().join(" / ");
        assert!(html.contains(&format!("Default: {bulwark_default}")), "bulwark's compiled-in values must be visible");
        assert!(html.contains("name=\"r1\"") && html.contains("name=\"r2\"") && html.contains("name=\"r3\""));
    }

    #[test]
    fn the_conversion_cap_input_appears_on_overflow_conversion_rows_only() {
        // The per-node cap belongs to the 13 OverflowConversion nodes -
        // exactly those rows carry the input, nothing else does.
        for &a in ALL_ARCHETYPES.iter() {
            let html = admin_page(a, false);
            let conversions = a.passive_nodes().iter().filter(|n| matches!(n.effect, crate::passive_tree::PassiveEffect::OverflowConversion { .. })).count();
            assert_eq!(html.matches("name=\"conversion_cap\"").count(), conversions, "{a:?} must offer the cap on each of its {conversions} conversion rows and no others");
            if conversions > 0 {
                assert!(html.contains("Blank follows the global"), "{a:?}'s conversion rows must say what blank means");
            }
        }
        // And the owner's named trio is really among them.
        for key in ["stonefist", "graniteskin", "risingdefiance"] {
            let n = node(Archetype::Monk, key);
            assert!(matches!(n.effect, crate::passive_tree::PassiveEffect::OverflowConversion { .. }), "{key} must be an OverflowConversion node");
        }
    }

    #[test]
    fn a_pending_migration_node_is_shown_but_not_editable() {
        // An input that silently does nothing is worse than no input.
        // `lastlaugh` is a Berserker node whose only rank reads are unlock
        // gates, so it owns no value an override could carry - see
        // PENDING_MIGRATION_NODES. (Was `payback` until 2026-08-27, when
        // Stage 3 migrated it and every other Warrior node off the list.)
        assert!(!crate::adventure::node_is_tunable("lastlaugh"), "sanity: lastlaugh must still be pending");
        let html = admin_page(Archetype::Berserker, false);
        assert!(html.contains("lastlaugh"), "a pending node must still be listed, so its state is visible");
        assert!(html.contains("Pending migration"), "and must say why it can't be edited");
    }

    #[test]
    fn a_not_yet_implemented_node_is_shown_but_not_editable() {
        // Searched rather than assumed - Warrior happens to have every
        // one of its 39 nodes implemented, so hardcoding a class here
        // would be testing the wrong thing (and did, first time round).
        let (archetype, inert) = ALL_ARCHETYPES
            .iter()
            .find_map(|&a| {
                a.passive_nodes()
                    .iter()
                    .find(|n| matches!(n.effect, crate::passive_tree::PassiveEffect::NotYetImplemented))
                    .map(|n| (a, n))
            })
            .expect("the tree still has at least one unimplemented node somewhere");
        let html = admin_page(archetype, false);
        assert!(html.contains(inert.key), "{:?}'s inert node {} must still be listed", archetype, inert.key);
        assert!(html.contains("No mechanic yet"), "an inert node must say why it can't be tuned");
    }

    #[test]
    fn every_class_is_reachable_from_the_class_nav() {
        let html = admin_page(Archetype::Warrior, false);
        for &a in ALL_ARCHETYPES.iter() {
            let slug = format!("{a:?}").to_lowercase();
            assert!(html.contains(&format!("/admin/passives?class={slug}")), "{a:?} must be reachable from the class nav");
        }
    }

    #[test]
    fn the_save_banner_only_appears_after_a_save() {
        assert!(!admin_page(Archetype::Warrior, false).contains("Saved"));
        assert!(admin_page(Archetype::Warrior, true).contains("Saved"));
    }

    #[test]
    fn with_no_overrides_no_node_is_marked_as_differing_from_default() {
        // The shipping-dark property at the UI layer: an untuned tree
        // shows no badges and offers no reverts anywhere.
        for &a in ALL_ARCHETYPES.iter() {
            let html = admin_page(a, false);
            assert!(!html.contains("differs from default"), "{a:?} shows a tuned badge with no overrides loaded");
            assert!(!html.contains("/admin/passives/revert"), "{a:?} offers a revert with nothing to revert");
        }
    }

    // ---- the tooltip note ------------------------------------------

    #[test]
    fn an_untuned_node_gets_no_override_note() {
        for &a in ALL_ARCHETYPES.iter() {
            for n in a.passive_nodes() {
                assert!(passive_override_note(n).is_none(), "{:?} {} produced a tuned note with no overrides loaded", a, n.key);
            }
        }
    }

    #[test]
    fn the_passives_page_shows_no_tuned_markup_when_nothing_is_overridden() {
        let mut c = Character::new("Tester".to_string());
        c.level = 40;
        c.archetype = Archetype::Warrior;
        let page = render_passive_tree_page("Tester", Some(&c), None);
        // Matched on the markup, not the bare class name - `base.html`
        // carries a `.passive-tuned` CSS rule, so once this body is
        // wrapped by `render_page` a looser check would match the
        // stylesheet and pass whether or not a note was emitted.
        assert!(!page.contains("class=\"passive-tuned\""), "an untuned tree must render no tuned markup at all");
    }

    #[test]
    fn float_display_trims_noise_without_losing_meaning() {
        assert_eq!(trim_float(0.5), "0.5");
        assert_eq!(trim_float(3.0), "3");
        assert_eq!(trim_float(0.0), "0");
        assert_eq!(trim_float(0.15000000000000002), "0.15", "float arithmetic noise must not reach the page");
        assert_eq!(trim_float(0.335), "0.335");
    }
}
