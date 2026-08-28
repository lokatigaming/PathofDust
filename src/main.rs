use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing_subscriber::prelude::*;

use twitch_bot_rs::adventure_client::{AdventureApiClient, RedemptionResponse};
use twitch_bot_rs::alerts;
use twitch_bot_rs::announcements::Announcements;
use twitch_bot_rs::bug_reports;
use twitch_bot_rs::channel_points;
use twitch_bot_rs::chat_overlay_server;
use twitch_bot_rs::commands::{self, Services};
use twitch_bot_rs::config::Config;
use twitch_bot_rs::emotes;
use twitch_bot_rs::entrance_themes::EntranceThemeManager;
use twitch_bot_rs::essence_pricing;
use twitch_bot_rs::obs_websocket::ObsClient;
use twitch_bot_rs::paypal;
use twitch_bot_rs::personal_playlists::PersonalPlaylistManager;
use twitch_bot_rs::playrandom::PlayRandomManager;
use twitch_bot_rs::song_overlay_server;
use twitch_bot_rs::song_requests::{SongInsertOutcome, SongRequestManager};
use twitch_bot_rs::streamelements::{self, Tip};
use twitch_bot_rs::twitch::auth::AuthClient;
use twitch_bot_rs::twitch::eventsub::{self, TwitchEvent};
use twitch_bot_rs::twitch::helix::HelixClient;
use twitch_bot_rs::twitch::chat;
use twitch_bot_rs::vessel_pricing;

// Shared by both the EventSub listener and chat's USERNOTICE-based sub
// detection, so an event is announced identically (chat message + alert
// broadcast) no matter which path it came in on.
async fn announce_twitch_event(event: TwitchEvent, chat_client: &chat::ChatClient, alerts: &alerts::AlertServer) {
    match event {
        TwitchEvent::Follow { user_name } => {
            tracing::info!("EventSub: follow — {user_name}");
            chat_client
                .say(format!(
                    "Welcome {user_name}, Thanks for the Follow! Check out our site https://lokati.net/ lokatiSalute"
                ))
                .await;
            alerts.broadcast(serde_json::json!({ "type": "follow", "name": user_name }));
        }
        TwitchEvent::Subscription { user_name } => {
            tracing::info!("Subscription — {user_name}");
            chat_client.say(format!("{user_name} just subscribed! Thank you!")).await;
            alerts.broadcast(serde_json::json!({ "type": "subscription", "name": user_name }));
        }
        TwitchEvent::SubscriptionGift { gifter_name, amount } => {
            tracing::info!("EventSub: subscription gift — {gifter_name} x{amount}");
            chat_client.say(format!("{gifter_name} gifted {amount} sub(s)! Thank you!")).await;
            alerts.broadcast(serde_json::json!({ "type": "subscriptionGift", "name": gifter_name, "amount": amount }));
        }
        TwitchEvent::Cheer { user_name, bits } => {
            tracing::info!("EventSub: cheer — {user_name} x{bits} bits");
            chat_client.say(format!("{user_name} cheered {bits} bits! Thanks for the support!")).await;
            alerts.broadcast(serde_json::json!({ "type": "cheer", "name": user_name, "bits": bits }));
        }
        TwitchEvent::Raid { from_broadcaster_name, viewers } => {
            tracing::info!("EventSub: raid — {from_broadcaster_name} x{viewers}");
            chat_client
                .say(format!("Thanks for the raid, {from_broadcaster_name}, bringing {viewers} viewers!"))
                .await;
            alerts.broadcast(serde_json::json!({ "type": "raid", "name": from_broadcaster_name, "viewers": viewers }));
        }
        TwitchEvent::ChannelPointsRedemption { .. } => {
            // Handled directly in main()'s eventsub callback instead (it
            // needs song_requests/entrance_themes, which this shared
            // function doesn't have) — reaching this arm would mean that
            // interception broke.
            tracing::warn!("announce_twitch_event got a ChannelPointsRedemption — should have been intercepted earlier.");
        }
    }
}

/// Someone redeemed the self-service "Set Entrance Theme Song" channel
/// points reward — resolves what they typed the same way !settheme does,
/// then either sets their theme + marks the redemption FULFILLED, or
/// refunds their points (CANCELED) and tells them why in chat so they can
/// redeem again with a working link/search. On success, also inserts the
/// song into playback right now (same mechanism as !songinsert/!si) so
/// spending the points gets an immediate payoff instead of only mattering
/// the next time they happen to chat.
#[allow(clippy::too_many_arguments)]
async fn handle_theme_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    user_input: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &chat::ChatClient,
    entrance_themes: &Arc<EntranceThemeManager>,
    song_requests: &Option<Arc<SongRequestManager>>,
    // Twitch's own redemption UI (pending/fulfilled/canceled) already
    // tells the redeemer what happened - a chat announcement on top is
    // redundant noise for a LIVE redemption (see the dispatch site below,
    // which passes `false`). The one exception is `reconcile_missed_
    // redemptions` replaying a backlog from while the bot was down
    // (passes `true`) - Twitch's UI moment for those already came and
    // went with nobody able to react to it, so chat is the only way
    // anyone finds out it actually went through, late.
    announce: bool,
) {
    let Some(song_requests) = song_requests else {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        return;
    };

    let query = user_input.trim();
    if query.is_empty() {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        if announce {
            chat_client
                .say(format!(
                    "{user_name}, your entrance theme redemption needs a YouTube link or search term entered — refunded, redeem again with one included."
                ))
                .await;
        }
        return;
    }

    match song_requests.resolve_song_preview(query).await {
        Ok(song) => {
            let youtube_url = format!("https://youtu.be/{}", song.video_id);
            entrance_themes.set_theme(&user_name, youtube_url, song.title.clone()).await;
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;

            // Re-resolving the same query here is a free cache hit (same
            // cache resolve_song_preview just populated) — not a second
            // YouTube API call.
            match song_requests.insert_song(query, &user_name).await {
                Ok(SongInsertOutcome::Inserted { song: inserted }) => {
                    let sr = song_requests.clone();
                    let video_id = inserted.video_id.clone();
                    let timeout = Duration::from_secs(inserted.duration_secs + 30);
                    tokio::spawn(async move {
                        tokio::time::sleep(timeout).await;
                        sr.clear_active_insert_if_stuck(&video_id);
                    });
                    if announce {
                        chat_client
                            .say(format!("{user_name} set their entrance theme to \"{}\" — playing now!", song.title))
                            .await;
                    }
                }
                Ok(SongInsertOutcome::AlreadyInserting) => {
                    if announce {
                        chat_client
                            .say(format!(
                                "{user_name} set their entrance theme to \"{}\" — another inserted song is playing right now, so it'll play next time they chat instead.",
                                song.title
                            ))
                            .await;
                    }
                }
                Err(err) => {
                    tracing::warn!("Theme redemption for {user_name}: theme set but immediate insert failed: {err}");
                    if announce {
                        chat_client.say(format!("{user_name} set their entrance theme to \"{}\"!", song.title)).await;
                    }
                }
            }
        }
        Err(err) => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
            if announce {
                chat_client
                    .say(format!(
                        "{user_name}, couldn't set that as your entrance theme ({err}) — refunded, redeem again with a different link/search."
                    ))
                    .await;
            }
        }
    }
}

/// Someone redeemed "Interrupt the Music" — unlike chat's !voteskip, a
/// paid 5k-point redemption doesn't need to build up votes toward a
/// threshold: it's a guaranteed, instant pass. Resolves what they typed
/// first (same as !songinsert), and only once that's confirmed valid does
/// it actually force the current song out — mirrors the mod-only !skip
/// command's own logic (cut an active insert if one's playing, else
/// advance the queue for real) — then inserts their pick to play right
/// now. Validating the query before touching playback means a bad
/// link/search just gets refunded with nothing skipped, instead of
/// nuking the current song for a request that then fails.
async fn handle_interrupt_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    user_input: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &chat::ChatClient,
    song_requests: &Option<Arc<SongRequestManager>>,
    // See `handle_theme_redemption`'s matching parameter's doc - `false`
    // for a live redemption (Twitch's own UI is confirmation enough),
    // `true` only when `reconcile_missed_redemptions` is replaying a
    // backlog from while the bot was down.
    announce: bool,
) {
    let Some(song_requests) = song_requests else {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        return;
    };

    let query = user_input.trim();
    if query.is_empty() {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        if announce {
            chat_client
                .say(format!(
                    "{user_name}, Interrupt the Music needs a YouTube link or search term entered — refunded, redeem again with one included."
                ))
                .await;
        }
        return;
    }

    // Shared with !voteskip's own cooldown (see song_requests.rs) — just a
    // peek here, checked before resolving the song so a viewer on cooldown
    // doesn't burn a YouTube API call for a redemption that's about to be
    // refunded anyway. Not started until the redemption is confirmed to
    // actually go through (below), so a refunded attempt never costs them
    // any cooldown.
    if let Some(remaining_secs) = song_requests.skip_cooldown_remaining(&user_name) {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        // Cooldown refusals are always announced, live or not (per a
        // live request) - unlike every other silenced-live message here,
        // a redeemer who gets no feedback at all has no way to tell "on
        // cooldown, refunded" apart from "Twitch just ate my points".
        chat_client
            .say(format!(
                "{user_name}, Interrupt the Music is on cooldown for you for another {remaining_secs}s — refunded."
            ))
            .await;
        return;
    }

    if let Err(err) = song_requests.resolve_song_preview(query).await {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        if announce {
            chat_client
                .say(format!("{user_name}, couldn't interrupt with that ({err}) — refunded, redeem again with a different link/search."))
                .await;
        }
        return;
    }

    // The song resolved fine, so this redemption is genuinely going
    // through — starts the cooldown now, right before actually forcing
    // the skip.
    song_requests.start_skip_cooldown(&user_name);

    // Guaranteed pass: cut an already-playing insert if there is one,
    // otherwise advance the real queue — same as !skip, just without
    // needing a mod to run it.
    let skip_msg = if song_requests.skip_insert() {
        "skipped it".to_string()
    } else {
        match song_requests.advance() {
            Some(song) => format!("skipped it — next up was \"{}\"", song.title),
            None => "skipped it — the queue is now empty".to_string(),
        }
    };

    // Re-resolving here is a free cache hit (same cache the preview above
    // just populated), not a second YouTube API call.
    match song_requests.insert_song(query, &user_name).await {
        Ok(SongInsertOutcome::Inserted { song: inserted }) => {
            let sr = song_requests.clone();
            let video_id = inserted.video_id.clone();
            let timeout = Duration::from_secs(inserted.duration_secs + 30);
            tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                sr.clear_active_insert_if_stuck(&video_id);
            });
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
            if announce {
                chat_client
                    .say(format!("{user_name} interrupted the music with \"{}\" — {skip_msg}!", inserted.title))
                    .await;
            }
        }
        Ok(SongInsertOutcome::AlreadyInserting) => {
            // Vanishingly rare race (another insert started in the instant
            // between skip_insert/advance above and here) — the skip still
            // genuinely happened, so this still counts as fulfilled.
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
            if announce {
                chat_client
                    .say(format!(
                        "{user_name} {skip_msg} — but another inserted song started playing right before theirs could, so it'll play next instead."
                    ))
                    .await;
            }
        }
        Err(err) => {
            tracing::warn!("Interrupt redemption for {user_name}: skip happened but insert failed: {err}");
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
            if announce {
                chat_client.say(format!("{user_name} {skip_msg}, but their song failed to play: {err}")).await;
            }
        }
    }
}

/// Someone redeemed "Reforge Gear" — no text input needed, unlike the two
/// above. Own 1-hour-per-redeemer cooldown (see adventure.rs's
/// REFORGE_COOLDOWN), claimed atomically FIRST (before touching the
/// roster) so two redemptions arriving milliseconds apart can't both slip
/// through — the second one always sees the first's freshly-claimed slot.
/// Given back if it turns out there was nothing to reforge, so a refunded
/// attempt never costs them the hour.
/// Unlike Theme/Interrupt/Force Boss, this one is deliberately ALWAYS
/// chat-announced (live or replayed from `reconcile_missed_redemptions`
/// alike, no `announce` gate) - a real request to keep it that way, it
/// doubles as a fun public "look what I just got" moment that Twitch's
/// own redemption UI alone doesn't convey.
/// What a redemption handler actually does once it has an answer -
/// separated from `handle_reforge_redemption` itself (Stage 5,
/// REFACTOR_PLAN.md §4c/§5, 2026-08-19) so the game-down DECISION is
/// unit-testable without a real `HelixClient`/`ChatClient` (neither has
/// a test-friendly constructor - see Stage 4's own "honest gap" note).
struct RedemptionAction {
    status: &'static str,
    chat_message: Option<String>,
}

/// Game down (§4c) - REFUND silently, matching Repair's own
/// already-silent tone (the ratified policy table lumps these two
/// together under one "REFUND silently" row) - this is a DIFFERENT
/// refund reason than the cooldown/no-gear refunds the game itself
/// already handles (see api.rs's own `redeem_reforge`), which still
/// chat-announce normally when the game is actually reachable.
fn reforge_redemption_action(result: anyhow::Result<RedemptionResponse>, user_name: &str) -> RedemptionAction {
    match result {
        Ok(resp) => RedemptionAction { status: if resp.fulfilled { "FULFILLED" } else { "CANCELED" }, chat_message: resp.chat_message },
        Err(err) => {
            tracing::warn!("reforge redemption for {user_name} failed (game down?): {err}");
            RedemptionAction { status: "CANCELED", chat_message: None }
        }
    }
}

async fn handle_reforge_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &chat::ChatClient,
    adventure: &AdventureApiClient,
) {
    let action = reforge_redemption_action(adventure.redeem_reforge(&user_name).await, &user_name);
    let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, action.status).await;
    if let Some(msg) = action.chat_message {
        chat_client.say(msg).await;
    }
}

/// Someone redeemed "Repair All Gear" — no text input, no cooldown
/// (unlike Reforge Gear). Fully repairs every equipped AND bagged item
/// for free and, since a repaired character is no longer worn out,
/// automatically clears retreat status too — no separate !join needed
/// (see adventure.rs's `repair_all_gear_free`). Silent in chat either
/// way now (per request) - the redemption's own FULFILLED/CANCELED
/// status on Twitch's side is confirmation enough.
/// Game down (§4c) - REFUND, same as today's up-and-running behavior:
/// this redemption is already silent in chat either way (see this fn's
/// own doc), so there's no separate "down" tone to apply - the fixed
/// `chat_message: None` here is just making that explicit.
fn repair_redemption_action(result: anyhow::Result<RedemptionResponse>, user_name: &str) -> RedemptionAction {
    match result {
        Ok(resp) => RedemptionAction { status: if resp.fulfilled { "FULFILLED" } else { "CANCELED" }, chat_message: resp.chat_message },
        Err(err) => {
            tracing::warn!("repair redemption for {user_name} failed (game down?): {err}");
            RedemptionAction { status: "CANCELED", chat_message: None }
        }
    }
}

async fn handle_repair_redemption(redemption_id: String, reward_id: String, user_name: String, helix: &HelixClient, broadcaster_id: &str, adventure: &AdventureApiClient) {
    let action = repair_redemption_action(adventure.redeem_repair(&user_name).await, &user_name);
    let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, action.status).await;
}

/// Someone redeemed "Force Boss Fight" — no text input, no per-user
/// cooldown, but a shared cycle-wide budget (see adventure.rs's
/// FORCE_BOSS_MAX_PER_CYCLE/try_force_encounter) that resets every time
/// the natural 10-minute timer fires, not per-redeemer. Chat-announced
/// either way (unlike Reforge/Repair's silent redemption-status-only
/// confirmation) since this affects the WHOLE party, not just the
/// redeemer's own gear - everyone should see why a boss fight just
/// started early.
async fn handle_force_boss_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &chat::ChatClient,
    adventure: &AdventureApiClient,
    // See `handle_theme_redemption`'s matching parameter's doc - `false`
    // for a live redemption (Twitch's own UI is confirmation enough),
    // `true` only when `reconcile_missed_redemptions` is replaying a
    // backlog from while the bot was down.
    announce: bool,
) {
    let action = force_boss_redemption_action(adventure.redeem_force_boss(&user_name, announce).await, &user_name, announce);
    let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, action.status).await;
    if let Some(msg) = action.chat_message {
        chat_client.say(msg).await;
    }
}

/// Game down (§4c) - REFUND + a chat line explaining why, same as this
/// redemption's already-always-chat-announced tone - still gated on
/// `announce` so a replayed backlog stays quiet.
fn force_boss_redemption_action(result: anyhow::Result<RedemptionResponse>, user_name: &str, announce: bool) -> RedemptionAction {
    match result {
        Ok(resp) => RedemptionAction { status: if resp.fulfilled { "FULFILLED" } else { "CANCELED" }, chat_message: resp.chat_message },
        Err(err) => {
            tracing::warn!("force-boss redemption for {user_name} failed (game down?): {err}");
            let chat_message = announce.then(|| format!("{user_name}, Force Boss Fight couldn't be processed right now — refunded. Try again in a moment!"));
            RedemptionAction { status: "CANCELED", chat_message }
        }
    }
}

/// Startup-only backlog catch-up (see its call site's doc) - for each of
/// the 5 channel points rewards that actually got created, asks Twitch
/// for whatever it's still holding UNFULFILLED and runs every one
/// through the SAME handler a live EventSub event would use, so a
/// redemption made during downtime gets exactly the same real
/// fulfillment/refund treatment, just late. Errors talking to Twitch are
/// logged and treated as "nothing to catch up on" for that reward
/// (rather than failing startup entirely) - the live listener starting
/// right after this is still the primary path either way.
#[allow(clippy::too_many_arguments)]
async fn reconcile_missed_redemptions(
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &Arc<chat::ChatClient>,
    entrance_themes: &Arc<EntranceThemeManager>,
    song_requests: &Option<Arc<SongRequestManager>>,
    adventure: &AdventureApiClient,
    theme_reward_id: Option<&str>,
    interrupt_reward_id: Option<&str>,
    reforge_reward_id: Option<&str>,
    repair_reward_id: Option<&str>,
    force_boss_reward_id: Option<&str>,
) {
    async fn fetch(helix: &HelixClient, broadcaster_id: &str, reward_id: Option<&str>, label: &str) -> Vec<twitch_bot_rs::twitch::helix::PendingRedemption> {
        let Some(reward_id) = reward_id else { return Vec::new() };
        match helix.get_unfulfilled_redemptions(broadcaster_id, reward_id).await {
            Ok(pending) => {
                if !pending.is_empty() {
                    tracing::info!("Reconciling {} missed \"{label}\" redemption(s) from while the bot was down.", pending.len());
                }
                pending
            }
            Err(err) => {
                tracing::error!("Failed to check for missed \"{label}\" redemptions: {err}");
                Vec::new()
            }
        }
    }

    for r in fetch(helix, broadcaster_id, theme_reward_id, "Set Entrance Theme Song").await {
        handle_theme_redemption(r.id, theme_reward_id.unwrap().to_string(), r.user_name, r.user_input, helix, broadcaster_id, chat_client, entrance_themes, song_requests, true).await;
    }
    for r in fetch(helix, broadcaster_id, interrupt_reward_id, "Interrupt the Music").await {
        handle_interrupt_redemption(r.id, interrupt_reward_id.unwrap().to_string(), r.user_name, r.user_input, helix, broadcaster_id, chat_client, song_requests, true).await;
    }
    for r in fetch(helix, broadcaster_id, reforge_reward_id, "Reforge Gear").await {
        handle_reforge_redemption(r.id, reforge_reward_id.unwrap().to_string(), r.user_name, helix, broadcaster_id, chat_client, adventure).await;
    }
    for r in fetch(helix, broadcaster_id, repair_reward_id, "Repair All Gear").await {
        handle_repair_redemption(r.id, repair_reward_id.unwrap().to_string(), r.user_name, helix, broadcaster_id, adventure).await;
    }
    for r in fetch(helix, broadcaster_id, force_boss_reward_id, "Force Boss Fight").await {
        handle_force_boss_redemption(r.id, force_boss_reward_id.unwrap().to_string(), r.user_name, helix, broadcaster_id, chat_client, adventure, true).await;
    }
}

/// Not `#[tokio::main]` anymore (2026-08-16) - that macro has no attribute
/// for stack size, and Tokio's own worker-thread default (2MiB) has gotten
/// tight for this codebase's combat simulation: `CombatSimUnit` has grown
/// to dozens of fields across many passive-tree/boss-ability passes, and a
/// single fight can now involve 30+ party members and, at high enough world
/// stages, more than 5 simultaneous bosses - boss count is no longer fixed
/// per stage at all (see `manager.rs`'s `boss_count_for_stage`), so this
/// margin needs to keep comfortably covering whatever that formula's cap
/// allows as stages climb, not just a flat "5." Bumped
/// well past the default as a safety margin - a repeat
/// `STATUS_STACK_OVERFLOW` crash (`0xC00000FD`) with no catchable panic or
/// backtrace at all (a hard OS-level fault, unlike a normal Rust panic) is
/// exactly what a hot function's stack usage creeping past the default
/// limit looks like from the outside.
fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    // Nothing from a plain stdout logger survives when this runs headless
    // under the Windows Scheduled Task (no console attached to capture
    // it) — every past "why did the bot die" investigation had to be done
    // live, in the moment, by re-running in the foreground. Writing to a
    // daily-rolling file too means a crash/restart can actually be
    // diagnosed after the fact. `_log_guard` has to stay alive for the
    // whole program — dropping it stops the background flush thread and
    // buffered log lines are lost. (Briefly disabled 2026-08-17 after
    // logs/ grew to several GB - re-enabled with a one-time cleanup of the
    // old files, see that same commit.)
    std::fs::create_dir_all("logs")?;
    let file_appender = tracing_appender::rolling::daily("logs", "bot.log");
    let (non_blocking, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!("PANIC: {panic_info}");
    }));

    let config = Config::load()?;

    let auth = AuthClient::new(
        config.twitch_client_id.clone(),
        config.twitch_client_secret.clone(),
        PathBuf::from("tokens.json"),
    )?;

    let helix = HelixClient::new(auth.clone());
    let broadcaster_id = helix
        .get_user_id_by_login(&config.twitch_channel)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Could not find Twitch user \"{}\" — check TWITCH_CHANNEL in .env", config.twitch_channel))?;

    let announcements = Arc::new(Announcements::new());

    let alerts = alerts::start_alert_server(config.alert_server_port, PathBuf::from("public")).await?;

    let static_commands = commands::StaticCommands::load(PathBuf::from("commands.json"), config.public_site_dir.clone()).await;
    let bug_reports = bug_reports::BugReportManager::new(PathBuf::from("bugreports.json"));

    let song_requests = if !config.youtube_api_keys.is_empty() {
        let manager = SongRequestManager::new(
            config.youtube_api_keys.clone(),
            config.song_request_max_duration_secs,
            config.song_request_voteskip_threshold,
            config.song_request_votepause_threshold,
            config.song_request_voteresume_threshold,
            config.song_request_resume_cooldown_secs,
            config.song_request_votevolume_threshold,
            PathBuf::from("song-queue.json"),
            PathBuf::from("search-cache.json"),
        );
        song_overlay_server::start_song_overlay_server(
            config.song_request_server_port,
            PathBuf::from("public_song_overlay"),
            manager.clone(),
        )
        .await?;
        Some(manager)
    } else {
        tracing::info!("YOUTUBE_API_KEY(S) not set — song requests are disabled.");
        None
    };

    let emote_map = emotes::fetch_all(auth.clone(), &broadcaster_id, &config.twitch_channel).await;
    let chat_overlay = chat_overlay_server::start_chat_overlay_server(
        config.chat_overlay_server_port,
        PathBuf::from("public_chat_overlay"),
        emote_map,
    )
    .await?;

    let (chat_client, mut chat_rx, mut twitch_event_rx) = chat::connect(
        config.twitch_client_id.clone(),
        config.twitch_client_secret.clone(),
        config.twitch_channel.clone(),
        auth.clone(),
    )
    .await?;

    let streamelements_watcher = if let Some(jwt) = &config.streamelements_jwt {
        match streamelements::start_streamelements_watcher(jwt.clone(), PathBuf::from("tips-history.json"), {
            let chat_client = chat_client.clone();
            let alerts = alerts.clone();
            move |tip: Tip| {
                let chat_client = chat_client.clone();
                let alerts = alerts.clone();
                tokio::spawn(async move {
                    tracing::info!("EventSub: tip — {} x{} {}", tip.name, tip.amount, tip.currency);
                    let amount_text = format!("{} {}", tip.currency, tip.amount).trim().to_string();
                    chat_client
                        .say(format!("{} just tipped {amount_text}! Thank you so much for the support!", tip.name))
                        .await;
                    alerts.broadcast(serde_json::json!({
                        "type": "tip",
                        "name": tip.name,
                        "amount": tip.amount,
                        "currency": tip.currency,
                        "message": tip.message,
                    }));
                });
            }
        })
        .await
        {
            Ok(watcher) => {
                tracing::info!("StreamElements watcher started — watching for tips.");
                Some(watcher)
            }
            Err(err) => {
                tracing::error!("Failed to start StreamElements watcher: {err}");
                None
            }
        }
    } else {
        tracing::info!("STREAMELEMENTS_JWT not set — tip alerts are disabled.");
        None
    };

    if let (Some(relay_url), Some(relay_token)) = (&config.paypal_relay_url, &config.paypal_relay_token) {
        paypal::start_paypal_watcher(
            relay_url.clone(),
            relay_token.clone(),
            config.paypal_poll_interval_ms,
            PathBuf::from("paypal-tips-history.json"),
            {
                let chat_client = chat_client.clone();
                let alerts = alerts.clone();
                move |tip: Tip| {
                    let chat_client = chat_client.clone();
                    let alerts = alerts.clone();
                    tokio::spawn(async move {
                        tracing::info!("PayPal: tip — {} x{} {}", tip.name, tip.amount, tip.currency);
                        let amount_text = format!("{} {}", tip.currency, tip.amount).trim().to_string();
                        chat_client
                            .say(format!("{} just tipped {amount_text}! Thank you so much for the support!", tip.name))
                            .await;
                        alerts.broadcast(serde_json::json!({
                            "type": "tip",
                            "name": tip.name,
                            "amount": tip.amount,
                            "currency": tip.currency,
                            "message": tip.message,
                        }));
                    });
                }
            },
        );
        tracing::info!("PayPal watcher started — polling the relay for tips.");
    } else {
        tracing::info!("PAYPAL_RELAY_URL/PAYPAL_RELAY_TOKEN not set — PayPal tip alerts are disabled.");
    }

    let entrance_themes = EntranceThemeManager::new(
        PathBuf::from("entrance-themes.json"),
        PathBuf::from("daily-greeted.json"),
        config.public_site_dir.clone(),
    );

    // A theme that had to wait (another theme, or a mod's !songinsert,
    // already had the active-insert slot) starts here, not from the
    // per-message trigger in the chat loop below — this is what actually
    // plays a queued theme once the slot frees up.
    if let Some(manager) = &song_requests {
        entrance_themes.clone().spawn_theme_queue_watcher(manager.clone());
    }

    // The "Welcome in, X!" announcement fires from here rather than
    // inline with the trigger, since a theme queued behind another one
    // might not actually start until well after the chat message that
    // triggered it.
    {
        let chat_client = chat_client.clone();
        let mut theme_started_rx = entrance_themes.subscribe_theme_started();
        tokio::spawn(async move {
            while let Ok(event) = theme_started_rx.recv().await {
                chat_client
                    .say(format!("Welcome in, {}! Playing their entrance theme: {}", event.username, event.title))
                    .await;
            }
        });
    }

    // Chat adventure game prototype — see adventure.rs. Always on (no
    // config gate — unlike song requests it needs no external API key).
    // Stage 4 cutover (REFACTOR_PLAN.md, 2026-08-19) - the bot no longer
    // runs the adventure game in-process (no `AdventureManager::new`, no
    // spawn_*_loop, no adventure_web/adventure_overlay servers started
    // here) - it's a thin HTTP client of the standalone `game` process
    // now. The one-time "Wings of Flight" giveaway that used to live
    // right here moved to game/src/main.rs's own startup for the same
    // reason the Celestial-Shard/launch-giveaway logic moved into
    // manager.rs at Stage 3: it's real game-state mutation, and this
    // process no longer has the state to mutate.
    let adventure = Arc::new(AdventureApiClient::new(config.adventure_api_base_url.clone(), config.adventure_api_secret.clone()));

    // Bot->game published constants (2026-08-22, build-time decoupling -
    // see src/published_constants.rs). Used to be a direct file write at
    // the very top of startup, before Config even loaded; it needs the
    // API client now, so it moved down here with it. Bounded retry, and a
    // down/old game never blocks or fails startup - the wiki just keeps
    // rendering "varies" until a successful publish lands.
    twitch_bot_rs::published_constants::publish_to_game(&adventure).await;

    // Self-service theme redemptions need entrance_themes and
    // song_requests, both already available here — created (once ever;
    // subsequent runs just reuse the persisted id) before the EventSub
    // listener starts so its subscription can be scoped to this specific
    // reward. None if creation fails (most likely tokens.json predates
    // the channel:manage:redemptions scope) — redemptions are just
    // skipped entirely in that case, everything else still runs.
    let theme_reward_id = if song_requests.is_some() {
        channel_points::ensure_theme_reward(
            &helix,
            &broadcaster_id,
            config.channel_points_theme_reward_cost,
            PathBuf::from("channel-points-theme-reward.json"),
        )
        .await
    } else {
        None
    };

    // Same idea as the theme reward above, just for "Interrupt the Music"
    // (see channel_points.rs's ensure_interrupt_reward and
    // handle_interrupt_redemption below) — also gated on song_requests
    // being configured, since both its effects (insert + vote skip) need it.
    let interrupt_reward_id = if song_requests.is_some() {
        channel_points::ensure_interrupt_reward(
            &helix,
            &broadcaster_id,
            config.channel_points_interrupt_reward_cost,
            PathBuf::from("channel-points-interrupt-reward.json"),
        )
        .await
    } else {
        None
    };

    // Same idea again, for "Reforge Gear" (see channel_points.rs's
    // ensure_reforge_reward and handle_reforge_redemption below) — always
    // created (unlike the two above, it has no song_requests dependency,
    // the adventure game is always on).
    let reforge_reward_id =
        channel_points::ensure_reforge_reward(&helix, &broadcaster_id, config.channel_points_reforge_reward_cost, PathBuf::from("channel-points-reforge-reward.json")).await;

    // Same idea again, for "Repair All Gear" (see channel_points.rs's
    // ensure_repair_reward and handle_repair_redemption below) — also
    // always created, same as Reforge Gear.
    let repair_reward_id =
        channel_points::ensure_repair_reward(&helix, &broadcaster_id, config.channel_points_repair_reward_cost, PathBuf::from("channel-points-repair-reward.json")).await;

    // Same idea again, for "Force Boss Fight" (see channel_points.rs's
    // ensure_force_boss_reward and handle_force_boss_redemption below) —
    // also always created, same as Reforge Gear/Repair All Gear.
    let force_boss_reward_id = channel_points::ensure_force_boss_reward(
        &helix,
        &broadcaster_id,
        config.channel_points_force_boss_reward_cost,
        PathBuf::from("channel-points-force-boss-reward.json"),
    )
    .await;

    // Startup reconciliation: Twitch keeps every redemption on its own
    // servers regardless of whether this bot was connected when it
    // happened - a redemption made while the bot was down/restarting
    // doesn't vanish, it just sits UNFULFILLED until something processes
    // it. Catches up on any backlog for all 5 rewards BEFORE the live
    // EventSub listener starts below, feeding each one through the exact
    // same handler a live event would use - so a viewer's spent points
    // never just go unprocessed because of bad timing around a deploy.
    reconcile_missed_redemptions(
        &helix,
        &broadcaster_id,
        &chat_client,
        &entrance_themes,
        &song_requests,
        &adventure,
        theme_reward_id.as_deref(),
        interrupt_reward_id.as_deref(),
        reforge_reward_id.as_deref(),
        repair_reward_id.as_deref(),
        force_boss_reward_id.as_deref(),
    )
    .await;

    eventsub::start_eventsub_listener(
        auth.clone(),
        broadcaster_id.clone(),
        theme_reward_id.clone(),
        interrupt_reward_id.clone(),
        reforge_reward_id.clone(),
        repair_reward_id.clone(),
        force_boss_reward_id.clone(),
        {
        let chat_client = chat_client.clone();
        let alerts = alerts.clone();
        let helix = helix.clone();
        let broadcaster_id = broadcaster_id.clone();
        let entrance_themes = entrance_themes.clone();
        let song_requests = song_requests.clone();
        let adventure = adventure.clone();
        move |event: TwitchEvent| {
            let chat_client = chat_client.clone();
            let alerts = alerts.clone();
            let helix = helix.clone();
            let broadcaster_id = broadcaster_id.clone();
            let entrance_themes = entrance_themes.clone();
            let song_requests = song_requests.clone();
            let theme_reward_id = theme_reward_id.clone();
            let interrupt_reward_id = interrupt_reward_id.clone();
            let reforge_reward_id = reforge_reward_id.clone();
            let repair_reward_id = repair_reward_id.clone();
            let force_boss_reward_id = force_boss_reward_id.clone();
            let adventure = adventure.clone();
            tokio::spawn(async move {
                if let TwitchEvent::ChannelPointsRedemption { redemption_id, reward_id, user_name, user_input } = event {
                    // Release 2 observability - the raw redemption itself
                    // had no log trace before this; each handler only
                    // logged its OWN downstream outcome, so a parser
                    // auditing redemption volume/attribution had nothing
                    // to start from.
                    let reward_title = if interrupt_reward_id.as_deref() == Some(reward_id.as_str()) {
                        channel_points::INTERRUPT_REWARD_TITLE
                    } else if theme_reward_id.as_deref() == Some(reward_id.as_str()) {
                        channel_points::THEME_REWARD_TITLE
                    } else if reforge_reward_id.as_deref() == Some(reward_id.as_str()) {
                        channel_points::REFORGE_REWARD_TITLE
                    } else if repair_reward_id.as_deref() == Some(reward_id.as_str()) {
                        channel_points::REPAIR_REWARD_TITLE
                    } else if force_boss_reward_id.as_deref() == Some(reward_id.as_str()) {
                        channel_points::FORCE_BOSS_REWARD_TITLE
                    } else {
                        "unrecognized"
                    };
                    tracing::info!(%reward_title, %reward_id, %user_name, %redemption_id, "Redemption");
                    if interrupt_reward_id.as_deref() == Some(reward_id.as_str()) {
                        handle_interrupt_redemption(
                            redemption_id,
                            reward_id,
                            user_name,
                            user_input,
                            &helix,
                            &broadcaster_id,
                            &chat_client,
                            &song_requests,
                            false,
                        )
                        .await;
                        return;
                    }
                    if theme_reward_id.as_deref() == Some(reward_id.as_str()) {
                        handle_theme_redemption(
                            redemption_id,
                            reward_id,
                            user_name,
                            user_input,
                            &helix,
                            &broadcaster_id,
                            &chat_client,
                            &entrance_themes,
                            &song_requests,
                            false,
                        )
                        .await;
                        return;
                    }
                    if reforge_reward_id.as_deref() == Some(reward_id.as_str()) {
                        handle_reforge_redemption(redemption_id, reward_id, user_name, &helix, &broadcaster_id, &chat_client, &adventure).await;
                        return;
                    }
                    if repair_reward_id.as_deref() == Some(reward_id.as_str()) {
                        handle_repair_redemption(redemption_id, reward_id, user_name, &helix, &broadcaster_id, &adventure).await;
                        return;
                    }
                    if force_boss_reward_id.as_deref() == Some(reward_id.as_str()) {
                        handle_force_boss_redemption(redemption_id, reward_id, user_name, &helix, &broadcaster_id, &chat_client, &adventure, false).await;
                        return;
                    }
                    tracing::warn!("ChannelPointsRedemption for an unrecognized reward_id {reward_id} — ignoring.");
                    return;
                }
                announce_twitch_event(event, &chat_client, &alerts).await;
            });
        }
    })
    .await;
    tracing::info!("EventSub listener started — watching for follows, gift subs, cheers, and raids.");

    // Regular subs/resubs arrive via chat's USERNOTICE handling instead of
    // EventSub (see chat.rs) — a real subscription was confirmed to post a
    // native chat notification but never reach the EventSub WebSocket, so
    // the already-reliable chat connection is now the sole source for
    // these, routed through the same announce_twitch_event used for
    // EventSub-sourced alerts to keep the chat message + alert-box output
    // identical either way.
    {
        let chat_client = chat_client.clone();
        let alerts = alerts.clone();
        tokio::spawn(async move {
            while let Some(event) = twitch_event_rx.recv().await {
                announce_twitch_event(event, &chat_client, &alerts).await;
            }
        });
    }

    // Video unavailable, region-locked, embedding disabled, etc. — the
    // overlay's YouTube player reports these to song_overlay_server, which
    // skips the song and emits a PlaybackErrorEvent here so chat knows why
    // it changed instead of the switch happening with no explanation.
    if let Some(manager) = &song_requests {
        let chat_client = chat_client.clone();
        let mut playback_error_rx = manager.subscribe_playback_errors();
        tokio::spawn(async move {
            while let Ok(event) = playback_error_rx.recv().await {
                tracing::warn!("Song skipped due to playback error: \"{}\" — {}", event.title, event.reason);
                chat_client.say(format!("Skipped \"{}\" — {}.", event.title, event.reason)).await;
            }
        });
    }

    let personal_playlists =
        PersonalPlaylistManager::new(PathBuf::from("personal-playlists.json"), config.playlist_sync_secret.clone());

    // Cheap idempotent cleanup, run every startup — catches anything saved
    // before the 10-minute playlist cap existed (a one-time backlog right
    // now) and self-heals if bad data ever slips in some other way later.
    // No-ops (and doesn't re-sync) once there's nothing left to remove.
    {
        let personal_playlists = personal_playlists.clone();
        tokio::spawn(async move {
            let removed = personal_playlists.cull_long_songs().await;
            if removed > 0 {
                tracing::info!("Removed {removed} playlist song(s) over the 10-minute cap.");
            }
        });
    }

    // Continuous mode's actual top-ups happen here, watching the live
    // queue in the background — !playrandom on/off just flips a flag this
    // watcher checks. None (LASTFM_API_KEY unset) disables !playrandom
    // entirely; the command replies that it's not enabled.
    let play_random = config.lastfm_api_key.as_ref().map(|key| PlayRandomManager::new(key.clone()));
    if let (Some(play_random), Some(manager)) = (&play_random, &song_requests) {
        play_random.clone().spawn_continuous_watcher(manager.clone());
    }

    // Hourly Blood-filled Vessel price snapshots for lokati.net/vessel-pricing.html
    // — see vessel_pricing.rs. No-ops if PLAYLIST_SYNC_SECRET isn't set.
    vessel_pricing::spawn_hourly_snapshotter(reqwest::Client::new(), config.poe_ninja_league.clone(), config.playlist_sync_secret.clone());

    // Hourly Deafening Essence price snapshots for lokati.net/essence-pricing.html
    // — see essence_pricing.rs. No-ops if PLAYLIST_SYNC_SECRET isn't set.
    essence_pricing::spawn_hourly_snapshotter(reqwest::Client::new(), config.poe_ninja_league.clone(), config.playlist_sync_secret.clone());

    // Connects (and keeps reconnecting) in the background regardless of
    // whether OBS is even running yet — !votevolume/!modvolume just get
    // a "couldn't reach OBS" error on a request made while it's down.
    // None (OBS_SONG_SOURCE_NAME unset) disables OBS volume control
    // entirely rather than trying to connect at all.
    let obs_song_volume = config.obs_song_source_name.as_ref().map(|source_name| {
        let obs = ObsClient::new(config.obs_websocket_url.clone(), config.obs_websocket_password.clone());
        (obs, source_name.clone())
    });

    // Stage 4 cutover (REFACTOR_PLAN.md §4b, 2026-08-19) - the standalone
    // `game` process now owns starting itself (its own binary spawns the
    // encounter/basic-encounter/rampage loops and the adventure_web/
    // adventure_overlay servers - see game/src/main.rs), and every
    // formerly-in-process broadcast subscriber below (encounter-result,
    // gear-crit, rampage-complete, unique-shard-win) collapses into ONE
    // relay loop reading `GET /api/announcements/stream` and passing each
    // already-formatted string straight to `chat_client.say()` - no
    // re-formatting here, per the seam's own "game owns all player-facing
    // text" design principle. Reconnects with backoff on any stream error
    // (including "game process restarted" or "game was never up yet at
    // boot") - announcements simply pause during the gap and resume once
    // reconnected, matching §4b's "drop gracefully" policy for the
    // reverse direction (bot down) with a symmetric, self-healing
    // treatment for THIS direction instead of ending the loop for good.
    {
        let chat_client = chat_client.clone();
        let adventure = adventure.clone();
        tokio::spawn(async move {
            loop {
                match adventure.announcements().await {
                    Ok(mut stream) => {
                        use futures_util::StreamExt;
                        while let Some(msg) = stream.next().await {
                            chat_client.say(msg).await;
                        }
                        tracing::warn!("Adventure announcements stream ended (game restarting?) - reconnecting in 5s.");
                    }
                    Err(err) => {
                        tracing::warn!("Failed to open the adventure announcements stream (game down?): {err} - retrying in 5s.");
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
    let services = Arc::new(Services {
        helix,
        broadcaster_id,
        alerts: Some(alerts.clone()),
        streamelements: streamelements_watcher,
        announcements: announcements.clone(),
        static_commands,
        song_requests,
        // A bare `Client::new()` has NO request timeout — a stalled
        // poe.ninja/build_feed response used to hang this forever, and
        // since !essence/!ritualscarab await it directly inside the
        // single sequential chat-command loop (see chat_rx.recv() below),
        // one stuck request froze EVERY chat command for EVERY user until
        // the process was restarted (confirmed live 2026-08-13).
        poe_ninja_http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("reqwest client build"),
        poe_ninja_league: config.poe_ninja_league.clone(),
        entrance_themes,
        personal_playlists,
        play_random,
        obs_song_volume,
        adventure,
        bug_reports,
    });

    let messages_since_announcement = Arc::new(AtomicU32::new(0));

    {
        let messages_since_announcement = messages_since_announcement.clone();
        let chat_client = chat_client.clone();
        let services = services.clone();
        let chat_overlay = chat_overlay.clone();
        tokio::spawn(async move {
            while let Some(msg) = chat_rx.recv().await {
                messages_since_announcement.fetch_add(1, Ordering::Relaxed);
                chat_overlay.broadcast_message(&msg.sender, &msg.text);

                // Checked for *every* message (not just non-command ones)
                // since "first message of the day" should count
                // regardless of whether that first message happens to be
                // a command. Only queues/starts the theme — the "Welcome
                // in" announcement fires separately once it actually
                // starts (see the theme_started_rx task above), since a
                // queued theme might not start right away.
                services.entrance_themes.maybe_play_entrance_theme(&msg.sender, &services.song_requests).await;

                // Same "every message counts" reasoning as the entrance
                // theme check above — passive XP shouldn't care whether
                // this particular message happened to be a command.
                // Fire-and-forget (§4c) - spawned rather than awaited so
                // a slow/down game process can never stall this loop for
                // every other chat message. The level-up announcement
                // itself now comes from the game side over the SSE relay
                // above (see `AdventureManager::grant_activity_xp`'s own
                // doc) - nothing to format or say here anymore.
                {
                    let adventure = services.adventure.clone();
                    let sender = msg.sender.clone();
                    tokio::spawn(async move {
                        if let Err(err) = adventure.activity_xp(&sender).await {
                            tracing::debug!("activity_xp call failed for {sender} (game down?): {err}");
                        }
                    });
                }

                let Some(rest) = msg.text.strip_prefix('!') else { continue };
                let mut parts = rest.trim().split_whitespace();
                let Some(name) = parts.next() else { continue };
                let name = name.to_lowercase();
                let args: Vec<String> = parts.map(String::from).collect();

                // This whole loop is single-threaded/sequential (one
                // `chat_rx.recv()` at a time) - a command handler that
                // ever hangs (e.g. the untimed poe_ninja_http client that
                // caused a real live outage on 2026-08-13) freezes EVERY
                // subsequent chat command for EVERY user with nothing
                // else in the log to show it, since most command
                // handlers don't log on success. These two lines exist
                // purely so a future "commands aren't responding" report
                // can be diagnosed from the log instead of by re-deriving
                // hypotheses from the source - "received" with no
                // matching "completed" pinpoints exactly which command
                // wedged the queue.
                tracing::info!("chat command: !{name} from {} (args={args:?})", msg.sender);
                let reply = commands::handle_command(
                    &name,
                    &msg.sender,
                    &args,
                    msg.is_mod_or_broadcaster,
                    msg.is_broadcaster,
                    &services,
                )
                .await;
                tracing::info!("chat command: !{name} from {} completed", msg.sender);

                match reply {
                    commands::Reply::None => {}
                    commands::Reply::One(text) => chat_client.say(text).await,
                    commands::Reply::Many(lines) => {
                        for line in lines {
                            chat_client.say(line).await;
                        }
                    }
                }
            }
        });
    }

    // Always scheduled, regardless of whether the "Announcements" sheet
    // has anything in it right now — announcements.next() just returns
    // None on an empty list, a no-op cycle. Not gating this on a
    // startup-time emptiness check means adding rows to a previously-empty
    // sheet takes effect on the next firing too, no restart needed either.
    {
        let min_messages = config.announcement_min_messages;
        let interval_ms = config.announcement_interval_ms;
        tracing::info!(
            "Periodic announcement enabled: every {}m, min {} chat messages since last time — reading from the Announcements sheet live.",
            interval_ms / 60_000,
            min_messages,
        );

        let messages_since_announcement = messages_since_announcement.clone();
        let chat_client = chat_client.clone();
        let announcements = announcements.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
            interval.tick().await;
            loop {
                interval.tick().await;
                if messages_since_announcement.load(Ordering::Relaxed) >= min_messages {
                    if let Some(text) = announcements.next().await {
                        messages_since_announcement.store(0, Ordering::Relaxed);
                        chat_client.say(text).await;
                    }
                }
            }
        });
    }

    tracing::info!("Bot is running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down...");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stage 5 (REFACTOR_PLAN.md §4c/§5, 2026-08-19) - every row of the
    /// redemption half of §4c's failure-isolation table, unit-tested
    /// directly against the pure decision functions rather than the
    /// handlers themselves (which need a real `HelixClient`/`ChatClient` -
    /// see Stage 4's own "honest gap" note on why those can't be
    /// constructed in a test without live Twitch).
    #[test]
    fn reforge_up_and_fulfilled_passes_the_status_and_message_through() {
        let action = reforge_redemption_action(Ok(RedemptionResponse { fulfilled: true, chat_message: Some("reforged!".to_string()) }), "viewer1");
        assert_eq!(action.status, "FULFILLED");
        assert_eq!(action.chat_message, Some("reforged!".to_string()));
    }

    #[test]
    fn reforge_up_but_declined_still_passes_the_game_side_message_through() {
        // e.g. on cooldown / no gear equipped - the game already formatted
        // a real refund message for this, unrelated to game-down at all.
        let action = reforge_redemption_action(Ok(RedemptionResponse { fulfilled: false, chat_message: Some("on cooldown".to_string()) }), "viewer1");
        assert_eq!(action.status, "CANCELED");
        assert_eq!(action.chat_message, Some("on cooldown".to_string()));
    }

    #[test]
    fn reforge_down_refunds_silently() {
        let action = reforge_redemption_action(Err(anyhow::anyhow!("connection refused")), "viewer1");
        assert_eq!(action.status, "CANCELED");
        assert_eq!(action.chat_message, None, "§4c: Reforge/Repair on game-down must refund SILENTLY");
    }

    #[test]
    fn repair_up_and_fulfilled_is_silent_either_way() {
        let action = repair_redemption_action(Ok(RedemptionResponse { fulfilled: true, chat_message: None }), "viewer1");
        assert_eq!(action.status, "FULFILLED");
        assert_eq!(action.chat_message, None);
    }

    #[test]
    fn repair_down_refunds_silently() {
        let action = repair_redemption_action(Err(anyhow::anyhow!("connection refused")), "viewer1");
        assert_eq!(action.status, "CANCELED");
        assert_eq!(action.chat_message, None, "§4c: Reforge/Repair on game-down must refund SILENTLY");
    }

    #[test]
    fn force_boss_up_and_fulfilled_passes_the_announcement_through() {
        let action = force_boss_redemption_action(Ok(RedemptionResponse { fulfilled: true, chat_message: Some("boss summoned!".to_string()) }), "viewer1", true);
        assert_eq!(action.status, "FULFILLED");
        assert_eq!(action.chat_message, Some("boss summoned!".to_string()));
    }

    #[test]
    fn force_boss_down_refunds_and_announces_when_live() {
        let action = force_boss_redemption_action(Err(anyhow::anyhow!("connection refused")), "viewer1", true);
        assert_eq!(action.status, "CANCELED", "§4c: Force Boss Fight on game-down must still refund");
        assert!(
            action.chat_message.as_deref().is_some_and(|m| m.contains("viewer1") && m.contains("refunded")),
            "§4c: Force Boss Fight on game-down must chat-announce why when live, got {:?}",
            action.chat_message
        );
    }

    #[test]
    fn force_boss_down_stays_quiet_when_replaying_a_backlog() {
        // `announce: false` - reconcile_missed_redemptions replaying while
        // the bot was down; matches every other redemption's "don't spam
        // chat for a backlog nobody's watching live" convention.
        let action = force_boss_redemption_action(Err(anyhow::anyhow!("connection refused")), "viewer1", false);
        assert_eq!(action.status, "CANCELED");
        assert_eq!(action.chat_message, None);
    }
}
