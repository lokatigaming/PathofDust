// Stage 3 API seam (2026-08-19, architecture refactor) - the internal
// `/api/*` surface from REFACTOR_PLAN.md §4a/§4c, nested onto the SAME
// Axum server `start_adventure_web_server` already runs (per §3's
// ratified text: "a real API on the game's existing Axum server"), not
// a separate port. Gated by a shared-secret header rather than a
// peer-IP/localhost check, since this router shares a port with the
// public, reverse-proxied dashboard - a peer-IP check would see every
// proxied request as loopback-adjacent and couldn't tell "the bot"
// apart from "the public internet" the way it could on a truly
// separate, unproxied bind.
//
// Every handler here is a straight PORT of the matching chat-command
// body from src/commands.rs (or redemption handler from src/main.rs) -
// same formatted strings, byte-for-byte, per §4a's "already-formatted
// reply string" design principle. Permission gating (is_mod_or_broadcaster)
// deliberately stays OUT of this module - only the bot knows about
// Twitch roles, so it decides whether to call an endpoint at all; an
// endpoint that behaves differently by role (only `rampage`) takes the
// role as an explicit field instead.
//
// This is new, parallel code per the stage's own scoping rule - nothing
// in src/main.rs or src/commands.rs calls through here yet, and nothing
// will until Stage 4's actual cutover. Only tests exercise this today.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::adventure::{
    pin_most_recent_fight, AdventureManager, BossKind, ForceBossOutcome, JoinOutcome, PublishedConstants, RampageVoteOutcome, TriggerEncounterOutcome,
    PUBLISHED_CONSTANTS_PATH, RAMPAGE_VOTE_THRESHOLD,
};

const API_SECRET_HEADER: &str = "x-adventure-api-secret";

#[derive(Clone)]
struct ApiState {
    adventure: Arc<AdventureManager>,
    shared_secret: Arc<str>,
}

/// `None` skips mounting `/api/*` at all (today's production default -
/// see config.rs's `adventure_api_secret` doc) rather than mounting a
/// permanently-401 router; a caller that never set the secret gets
/// literally the same route table as before this stage existed.
///
/// Generic over the CALLER's own state type `S` (here, adventure_web's
/// private `AppState`) purely so `Router::nest` (which requires the
/// nested router's state type to match the outer router's) accepts this
/// - `with_state` below fully resolves this router's real state
/// (`ApiState`) already, so `S` is never actually read.
pub(super) fn router<S: Clone + Send + Sync + 'static>(adventure: Arc<AdventureManager>, shared_secret: Option<String>) -> Option<Router<S>> {
    let shared_secret: Arc<str> = shared_secret?.into();
    let state = ApiState { adventure, shared_secret: shared_secret.clone() };
    Some(
        Router::new()
            .route("/commands/join", post(join))
            .route("/commands/character", get(character))
            .route("/commands/party", get(party))
            .route("/commands/next_encounter", post(next_encounter))
            .route("/commands/event_intro", post(event_intro))
            .route("/commands/rampage", post(rampage))
            .route("/commands/clear_battlefield", post(clear_battlefield))
            .route("/commands/give_loot", post(give_loot))
            .route("/commands/gift_dust", post(gift_dust))
            .route("/commands/pin_fight", post(pin_fight))
            .route("/redemptions/reforge", post(redeem_reforge))
            .route("/redemptions/repair", post(redeem_repair))
            .route("/redemptions/force_boss", post(redeem_force_boss))
            .route("/activity_xp", post(activity_xp))
            .route("/published-constants", post(publish_constants))
            .route("/announcements/stream", get(announcements_stream))
            .layer(axum::middleware::from_fn_with_state(state.clone(), require_shared_secret))
            .with_state(state),
    )
}

async fn require_shared_secret(State(state): State<ApiState>, req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let presented = req.headers().get(API_SECRET_HEADER).and_then(|v| v.to_str().ok());
    if presented == Some(state.shared_secret.as_ref()) {
        next.run(req).await
    } else {
        // "Reject loudly" (REFACTOR_PLAN.md §4's credential-handling
        // note) - a mismatch here means either a misconfigured/drifted
        // secret between the two processes or something scanning the
        // public dashboard port; either way it belongs in the logs, not
        // just a silently-returned 401 nobody notices. Never log the
        // presented value itself - a wrong guess still isn't safe to
        // persist verbatim, and it adds nothing a human needs to fix this.
        tracing::warn!(path = %req.uri().path(), presented_secret = presented.is_some(), "rejected /api/* request with a missing or invalid shared secret");
        (StatusCode::UNAUTHORIZED, "missing or invalid API secret").into_response()
    }
}

/// Uniform envelope for every §4a command endpoint - `None` means "no
/// reply text at all" (e.g. `next_encounter`'s Triggered case: the real
/// fight result arrives later over `/announcements/stream`, not here).
#[derive(Serialize)]
struct ReplyResponse {
    reply: Option<String>,
}

fn reply(text: impl Into<String>) -> Json<ReplyResponse> {
    Json(ReplyResponse { reply: Some(text.into()) })
}

#[derive(Deserialize)]
struct UserBody {
    user: String,
}

async fn join(State(state): State<ApiState>, Json(body): Json<UserBody>) -> Json<ReplyResponse> {
    let user = &body.user;
    match state.adventure.join(user, user).await {
        JoinOutcome::Joined => reply(format!("{user} has joined the adventure! Chatting earns your character XP over time — check !character.")),
        JoinOutcome::AlreadyJoined { level } => reply(format!("{user}, you're already on the adventure (level {level}).")),
        JoinOutcome::Rejoined { level, gear_still_worn } => {
            if gear_still_worn {
                reply(format!("{user} (level {level}) is back on the battlefield! Your gear's still worn though — repair it on the dashboard for full stats."))
            } else {
                reply(format!("{user} (level {level}) is back on the battlefield, gear fully repaired!"))
            }
        }
    }
}

#[derive(Deserialize)]
struct UserQuery {
    user: String,
}

async fn character(State(state): State<ApiState>, Query(q): Query<UserQuery>) -> Json<ReplyResponse> {
    match state.adventure.character(&q.user).await {
        Some(c) => reply(format!(
            "{} — Level {} {:?} ({}/{} xp) — {}W/{}L — {} Thaumatergic Dust — Gear: {} — manage your character at https://adventure.lokati.net/",
            c.display_name,
            c.level,
            c.archetype,
            c.xp,
            c.xp_needed(),
            c.wins,
            c.losses,
            c.dust,
            c.gear_summary()
        )),
        None => reply(format!("{}, you haven't joined the adventure yet — use !join to start!", q.user)),
    }
}

async fn party(State(state): State<ApiState>) -> Json<ReplyResponse> {
    let (stage, active, total) = state.adventure.party_status().await;
    reply(format!(
        "The adventure is at stage {stage} with {active}/{total} hero(es) ready to fight (retreated/downed heroes don't count)! Use !join to add yours — https://adventure.lokati.net/"
    ))
}

#[derive(Deserialize)]
struct NextEncounterBody {
    forced: Option<String>,
}

async fn next_encounter(State(state): State<ApiState>, Json(body): Json<NextEncounterBody>) -> Json<ReplyResponse> {
    match state.adventure.trigger_encounter_now(body.forced.as_deref()).await {
        // The real fight result is announced separately once it resolves
        // - see `AdventureManager::announce_encounter_result` - so there's
        // nothing to say here on success, same as commands.rs's own
        // Reply::None for this case.
        TriggerEncounterOutcome::Triggered => Json(ReplyResponse { reply: None }),
        TriggerEncounterOutcome::NobodyJoined => reply("Nobody's joined the adventure yet — chat needs to !join first!"),
        TriggerEncounterOutcome::UnknownBoss => reply("Unrecognized boss - try lich, firedemon, cthulhu, dragon, bahamut, purple, or cube."),
    }
}

#[derive(Deserialize)]
struct EventIntroBody {
    args: Vec<String>,
}

async fn event_intro(State(state): State<ApiState>, Json(body): Json<EventIntroBody>) -> Json<ReplyResponse> {
    const USAGE: &str = "Usage: !event intro <boss name> - e.g. !event intro cube";
    let action = body.args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    if action != "intro" {
        return reply(USAGE);
    }
    let Some(name) = body.args.get(1) else {
        return reply(USAGE);
    };
    match state.adventure.trigger_boss_intro(name).await {
        TriggerEncounterOutcome::Triggered => {
            // `trigger_boss_intro` already validated `name`, so this
            // re-parse (just to recover the kind's display name/slug)
            // can't actually fail - matches handle_event_command's own
            // reasoning verbatim.
            let Some((kind, _)) = BossKind::parse_forced(name) else {
                return Json(ReplyResponse { reply: None });
            };
            reply(format!("🆕 NEW BOSS ALERT: {} has entered the fray! Read up before the fight: https://adventure.lokati.net/wiki#{}", kind.display_name(), kind.wiki_slug()))
        }
        TriggerEncounterOutcome::NobodyJoined => reply("Nobody's joined the adventure yet — chat needs to !join first!"),
        TriggerEncounterOutcome::UnknownBoss => reply("Unrecognized boss - try lich, firedemon, cthulhu, dragon, bahamut, purple, or cube."),
    }
}

#[derive(Deserialize)]
struct RampageBody {
    user: String,
    is_mod_or_broadcaster: bool,
}

async fn rampage(State(state): State<ApiState>, Json(body): Json<RampageBody>) -> Json<ReplyResponse> {
    if body.is_mod_or_broadcaster {
        state.adventure.start_rampage().await;
        return reply("🔥 RAMPAGE! The next 50 encounters are all boss fights, one every ~60s (or as soon as the last one finishes playing out) — everyone gets instantly revived between them too. Buckle up!");
    }
    match state.adventure.register_rampage_vote(&body.user).await {
        RampageVoteOutcome::Triggered => reply("🔥 RAMPAGE! The vote passed — the next 50 encounters are all boss fights, one every ~60s (or as soon as the last one finishes playing out) — everyone gets instantly revived between them too. Buckle up!"),
        RampageVoteOutcome::Counted(count) => reply(format!("🗳️ Rampage vote: {count}/{RAMPAGE_VOTE_THRESHOLD} — {} more and it's on!", RAMPAGE_VOTE_THRESHOLD.saturating_sub(count))),
    }
}

async fn clear_battlefield(State(state): State<ApiState>) -> Json<ReplyResponse> {
    let affected = state.adventure.clear_battlefield().await;
    if affected > 0 {
        reply(format!("🧹 Battlefield cleared — {affected} hero(es) were pulled from the field and need to !join again to rejoin!"))
    } else {
        reply("Nobody was on the battlefield to clear.")
    }
}

async fn give_loot(State(state): State<ApiState>) -> Json<ReplyResponse> {
    let count = state.adventure.grant_random_gear_to_all().await;
    if count > 0 {
        reply(format!("🎁 Dropped fresh gear into {count} heroes' bags! Equip it from the adventure dashboard."))
    } else {
        reply("Nobody's joined the adventure yet — chat needs to !join first!")
    }
}

#[derive(Deserialize)]
struct GiftDustBody {
    /// The literal string "all" (case-insensitive) or a single username -
    /// same shape `!giftdust <all|username> <amount>` always took.
    target: String,
    /// Already parsed/validated (>0) bot-side - chat-arg parsing stays a
    /// bot concern, this endpoint only ever sees a valid amount.
    amount: u64,
}

async fn gift_dust(State(state): State<ApiState>, Json(body): Json<GiftDustBody>) -> Json<ReplyResponse> {
    if body.target.eq_ignore_ascii_case("all") {
        let count = state.adventure.grant_dust_to_all(body.amount).await;
        if count > 0 {
            reply(format!("💰 Gave {} dust to all {count} heroes!", body.amount))
        } else {
            reply("Nobody's joined the adventure yet — chat needs to !join first!")
        }
    } else {
        let username = body.target.trim_start_matches('@');
        if state.adventure.grant_dust(username, body.amount).await {
            reply(format!("💰 Gave {} dust to {username}!", body.amount))
        } else {
            reply(format!("{username} hasn't joined the adventure yet."))
        }
    }
}

async fn pin_fight() -> Json<ReplyResponse> {
    match pin_most_recent_fight() {
        None => reply("Nothing to pin yet — no fight has landed since the last restart."),
        Some(pinned) => {
            let id = pinned.sequence.map(|s| s.to_string()).unwrap_or_else(|| "?".to_string());
            match (pinned.coarse_pinned, pinned.detail_pinned) {
                (true, true) => reply(format!("📌 Pinned fight #{id} (coarse + detail) — safe from pruning now.")),
                (true, false) => reply(format!("📌 Pinned fight #{id} (coarse only — detail file was already gone).")),
                (false, true) => reply(format!("📌 Pinned fight #{id} (detail only — coarse file was already gone).")),
                (false, false) => reply("Failed to pin — check the bot's logs."),
            }
        }
    }
}

/// Shared shape for all 3 redemption endpoints - the FULFILLED/CANCELED
/// Twitch-side status update stays bot-side (needs `helix`, a bot-only
/// concern), driven by `fulfilled` instead of an in-process enum match.
#[derive(Serialize)]
struct RedemptionResponse {
    fulfilled: bool,
    chat_message: Option<String>,
}

#[derive(Deserialize)]
struct UserNameBody {
    user_name: String,
}

async fn redeem_reforge(State(state): State<ApiState>, Json(body): Json<UserNameBody>) -> Json<RedemptionResponse> {
    let user_name = &body.user_name;
    if let Err(remaining_secs) = state.adventure.try_claim_reforge_cooldown(user_name).await {
        let mins = (remaining_secs + 59) / 60;
        return Json(RedemptionResponse {
            fulfilled: false,
            chat_message: Some(format!("{user_name}, Reforge Gear is on cooldown for you for another ~{mins} min — refunded.")),
        });
    }
    match state.adventure.reforge_random_gear(user_name).await {
        Some(outcome) => Json(RedemptionResponse {
            fulfilled: true,
            chat_message: Some(format!(
                "🔥 {user_name} reforged their {:?} into a {} (tier {} → {})! Check !character for the new stats.",
                outcome.slot, outcome.item_name, outcome.old_tier, outcome.new_tier
            )),
        }),
        None => {
            state.adventure.release_reforge_cooldown(user_name).await;
            Json(RedemptionResponse {
                fulfilled: false,
                chat_message: Some(format!("{user_name}, you don't have any gear equipped yet to reforge — refunded. Fight some encounters first!")),
            })
        }
    }
}

async fn redeem_repair(State(state): State<ApiState>, Json(body): Json<UserNameBody>) -> Json<RedemptionResponse> {
    let fulfilled = state.adventure.repair_all_gear_free(&body.user_name).await.is_some();
    // Always silent in chat, live or replayed - matches handle_repair_redemption's own doc.
    Json(RedemptionResponse { fulfilled, chat_message: None })
}

#[derive(Deserialize)]
struct ForceBossBody {
    user_name: String,
    /// `false` for a live redemption (Twitch's own UI is confirmation
    /// enough), `true` only when replaying a missed-redemption backlog -
    /// see `handle_force_boss_redemption`'s matching parameter's doc.
    /// The CycleLimitReached case is always announced regardless (the
    /// redeemer needs to know why), matching that fn's own behavior.
    announce: bool,
}

async fn redeem_force_boss(State(state): State<ApiState>, Json(body): Json<ForceBossBody>) -> Json<RedemptionResponse> {
    let user_name = &body.user_name;
    let (fulfilled, chat_message) = match state.adventure.try_force_encounter().await {
        ForceBossOutcome::Triggered => (true, body.announce.then(|| format!("⚔️ {user_name} spent channel points to summon the boss early!"))),
        ForceBossOutcome::NobodyJoined => (
            false,
            body.announce.then(|| format!("{user_name}, nobody's currently joined the adventure — refunded. Get some heroes in with !join first!")),
        ),
        ForceBossOutcome::CycleLimitReached => (
            false,
            Some(format!("{user_name}, Force Boss Fight has already been used its 2 times this cycle — refunded. Try again after the next scheduled fight!")),
        ),
    };
    Json(RedemptionResponse { fulfilled, chat_message })
}

#[derive(Deserialize)]
struct ActivityXpBody {
    username: String,
}

/// Fire-and-forget per §4c - the bot must not block its chat-message
/// loop waiting on this. The level-up announcement itself (if any) is
/// pushed by `grant_activity_xp` straight onto `announcements_tx`, not
/// returned here - there is no synchronous reply to give, by design.
async fn activity_xp(State(state): State<ApiState>, Json(body): Json<ActivityXpBody>) -> StatusCode {
    state.adventure.grant_activity_xp(&body.username).await;
    StatusCode::ACCEPTED
}

/// Bot → game published constants (2026-08-22, bot/game build-time
/// decoupling) - replaces the bot writing `bot-published-constants.json`
/// into the game's working directory itself, which was that repo's last
/// bot→game file write and the last build-time coupling forcing a bot
/// rebuild on every game-only release. The bot POSTs its five compile-
/// time constants here once at startup; this writes them to the SAME
/// path wiki.rs already reads (`PUBLISHED_CONSTANTS_PATH` - deliberately
/// NOT routed through `paths::data_path`, see that constant's own doc)
/// via the same pretty-printing `state::save_json` the old direct write
/// used, so the on-disk format is unchanged and a pre-decoupling bot
/// that still writes the file itself keeps working against this handler's
/// game. A file that never gets written (bot down or too old to know
/// about this route) leaves wiki.rs rendering its placeholders as
/// "varies", exactly as it always has.
async fn publish_constants(Json(published): Json<PublishedConstants>) -> StatusCode {
    match crate::state::save_json(crate::adventure::published_constants_path(), &published) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(err) => {
            tracing::error!("failed to persist posted published-constants payload to {PUBLISHED_CONSTANTS_PATH}: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn announcements_stream(State(state): State<ApiState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.adventure.subscribe_announcements();
    // A lagged reader (see `broadcast::Receiver::recv`'s docs) just skips
    // the messages it missed rather than ending the stream - matches the
    // channel's own "drop gracefully" policy (§4b, owner-confirmed): a
    // missed announcement is low-stakes and self-heals on the next one,
    // it shouldn't tear down the whole SSE connection.
    let stream = BroadcastStream::new(rx).filter_map(|item| async move { item.ok().map(|msg| Ok(Event::default().data(msg))) });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
