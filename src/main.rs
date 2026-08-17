use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing_subscriber::prelude::*;

use rand::Rng;
use twitch_bot_rs::adventure::{
    affix_name, summarize_fight, AdventureManager, CombatEvent, CraftAction, EncounterKind, ForceBossOutcome, GearCritSource, ReceiveOutcome,
};
use twitch_bot_rs::adventure_overlay_server;
use twitch_bot_rs::adventure_web;
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
use twitch_bot_rs::patreon::{self, NewPatron};
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

/// Joins up to `max` items with ", ", appending "+N more" if there were
/// more than that - keeps a condensed encounter-summary chat line from
/// growing unbounded when a big roster all loot/break/retreat at once.
fn join_capped(items: &[String], max: usize) -> String {
    if items.len() <= max {
        items.join(", ")
    } else {
        format!("{}, +{} more", items[..max].join(", "), items.len() - max)
    }
}

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
async fn handle_reforge_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    chat_client: &chat::ChatClient,
    adventure: &Arc<AdventureManager>,
) {
    if let Err(remaining_secs) = adventure.try_claim_reforge_cooldown(&user_name).await {
        let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        let mins = (remaining_secs + 59) / 60;
        chat_client.say(format!("{user_name}, Reforge Gear is on cooldown for you for another ~{mins} min — refunded.")).await;
        return;
    }

    match adventure.reforge_random_gear(&user_name).await {
        Some(outcome) => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
            chat_client
                .say(format!(
                    "🔥 {user_name} reforged their {:?} into a {} (tier {} → {})! Check !character for the new stats.",
                    outcome.slot, outcome.item_name, outcome.old_tier, outcome.new_tier
                ))
                .await;
        }
        None => {
            adventure.release_reforge_cooldown(&user_name).await;
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
            chat_client
                .say(format!("{user_name}, you don't have any gear equipped yet to reforge — refunded. Fight some encounters first!"))
                .await;
        }
    }
}

/// Someone redeemed "Repair All Gear" — no text input, no cooldown
/// (unlike Reforge Gear). Fully repairs every equipped AND bagged item
/// for free and, since a repaired character is no longer worn out,
/// automatically clears retreat status too — no separate !join needed
/// (see adventure.rs's `repair_all_gear_free`). Silent in chat either
/// way now (per request) - the redemption's own FULFILLED/CANCELED
/// status on Twitch's side is confirmation enough.
async fn handle_repair_redemption(
    redemption_id: String,
    reward_id: String,
    user_name: String,
    helix: &HelixClient,
    broadcaster_id: &str,
    adventure: &Arc<AdventureManager>,
) {
    match adventure.repair_all_gear_free(&user_name).await {
        Some(()) => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
        }
        None => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
        }
    }
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
    adventure: &Arc<AdventureManager>,
    // See `handle_theme_redemption`'s matching parameter's doc - `false`
    // for a live redemption (Twitch's own UI is confirmation enough),
    // `true` only when `reconcile_missed_redemptions` is replaying a
    // backlog from while the bot was down.
    announce: bool,
) {
    match adventure.try_force_encounter().await {
        ForceBossOutcome::Triggered => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "FULFILLED").await;
            if announce {
                chat_client.say(format!("⚔️ {user_name} spent channel points to summon the boss early!")).await;
            }
        }
        ForceBossOutcome::NobodyJoined => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
            if announce {
                chat_client.say(format!("{user_name}, nobody's currently joined the adventure — refunded. Get some heroes in with !join first!")).await;
            }
        }
        ForceBossOutcome::CycleLimitReached => {
            let _ = helix.update_redemption_status(broadcaster_id, &reward_id, &redemption_id, "CANCELED").await;
            // Always announced, live or not - a shared per-cycle cooldown,
            // same "the redeemer needs to know why" reasoning as
            // Interrupt/Reforge's cooldown messages.
            chat_client
                .say(format!("{user_name}, Force Boss Fight has already been used its 2 times this cycle — refunded. Try again after the next scheduled fight!"))
                .await;
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
    adventure: &Arc<AdventureManager>,
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

    let patreon_watcher = if let Some(patreon_config) = &config.patreon {
        match patreon::start_patreon_watcher(
            patreon_config.client_id.clone(),
            patreon_config.client_secret.clone(),
            PathBuf::from("patreon-tokens.json"),
            PathBuf::from("patreon-seen.json"),
            patreon_config.poll_interval_ms,
            {
                let chat_client = chat_client.clone();
                move |member: NewPatron| {
                    let chat_client = chat_client.clone();
                    tokio::spawn(async move {
                        chat_client
                            .say(format!(
                                "New Patreon supporter: {} (Tier: {})! Support here: {}",
                                member.name, member.tier, member.campaign_url
                            ))
                            .await;
                    });
                }
            },
        )
        .await
        {
            Ok(watcher) => {
                tracing::info!("Patreon watcher started — polling for new patrons.");
                Some(watcher)
            }
            Err(err) => {
                tracing::error!("Failed to start Patreon watcher: {err}");
                None
            }
        }
    } else {
        None
    };

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
    // Created here (rather than down where spawn_encounter_loop/the
    // overlay server start, further below) specifically so the
    // "Reforge Gear" self-service reward's EventSub subscription/handler,
    // set up just below, can capture it.
    let adventure = AdventureManager::new(
        PathBuf::from("adventure-characters.json"),
        PathBuf::from("adventure-world.json"),
        PathBuf::from("adventure-reforge-cooldown.json"),
    );

    // One-time giveaway: hands the "Wings of Flight" cosmetic to one
    // random currently-joined character and announces it in chat - per a
    // live request, "while we're at it" alongside adding the cosmetic
    // itself. Guarded by its own marker (same fire-once shape as every
    // other one-off grant in adventure.rs) so a restart never re-rolls
    // it. Spawned rather than awaited inline - nothing else in startup
    // depends on this having finished.
    {
        const WINGS_GIVEAWAY_MARKER_PATH: &str = "adventure-wings-giveaway-marker.json";
        if twitch_bot_rs::state::load_json::<bool>(WINGS_GIVEAWAY_MARKER_PATH).is_none() {
            let adventure = adventure.clone();
            let chat_client = chat_client.clone();
            tokio::spawn(async move {
                if let Some(display_name) = adventure.grant_random_wings().await {
                    chat_client
                        .say(format!(
                            "🕊️ {display_name} has been randomly gifted the ultra-rare Wings of Flight cosmetic! Check your dashboard to toggle it on."
                        ))
                        .await;
                }
                if let Err(err) = twitch_bot_rs::state::save_json(WINGS_GIVEAWAY_MARKER_PATH, &true) {
                    tracing::error!("Failed to persist wings giveaway marker to {WINGS_GIVEAWAY_MARKER_PATH}: {err}");
                }
            });
        }
    }

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

    // `adventure` itself was created earlier (see above the theme reward
    // setup) so the reforge reward's EventSub handler could capture it.
    adventure.clone().spawn_encounter_loop();
    adventure.clone().spawn_basic_encounter_loop();
    adventure.clone().spawn_rampage_loop();
    adventure_overlay_server::start_adventure_overlay_server(
        config.adventure_overlay_server_port,
        PathBuf::from("public_adventure_overlay"),
        adventure.clone(),
    )
    .await?;
    adventure_web::start_adventure_web_server(
        config.adventure_web_port,
        config.adventure_web_public_url.clone(),
        config.twitch_client_id.clone(),
        config.twitch_client_secret.clone(),
        adventure.clone(),
        PathBuf::from("adventure-sessions.json"),
    )
    .await?;
    {
        let chat_client = chat_client.clone();
        let adventure = adventure.clone();
        let mut encounter_rx = adventure.subscribe_encounters();
        tokio::spawn(async move {
            while let Ok(result) = encounter_rx.recv().await {
                let chat_client = chat_client.clone();
                let adventure = adventure.clone();
                tokio::spawn(async move {
                    // The overlay doesn't show the outcome the instant this
                    // broadcast fires — it plays a charge-in animation
                    // first, then the real fight's (compressed) length —
                    // see public_adventure_overlay/overlay.html's CHARGE_MS
                    // (700) and displayDurationMs. Without this delay chat
                    // spoiled the result several seconds before the
                    // animation got anywhere near it. Spawned per-result
                    // (not awaited inline in the recv loop) so back-to-back
                    // encounters don't serialize their delays.
                    let delay_ms = 700 + result.display_duration_ms;
                    tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;

                    let mut msg = match (result.kind, result.won) {
                        (EncounterKind::Boss, true) => format!(
                            "⚔️ Victory! The party ({} heroes) bested the stage {} enemy! Onward to stage {}!",
                            result.participants.len(),
                            result.stage,
                            result.stage + 1
                        ),
                        (EncounterKind::Boss, false) => format!(
                            "⚔️ The party ({} heroes) was defeated by the stage {} enemy... knocked back to stage {}!",
                            result.participants.len(),
                            result.stage,
                            result.stage.saturating_sub(2).max(1)
                        ),
                        (EncounterKind::Basic, true) => format!(
                            "⚔️ The party ({} heroes) fought off {} ({})!",
                            result.participants.len(),
                            result.enemy_name.as_deref().unwrap_or("a group of enemies"),
                            result.enemy_count.map(|n| format!("{n} enemies")).unwrap_or_default()
                        ),
                        (EncounterKind::Basic, false) => format!(
                            "⚔️ The party ({} heroes) was overwhelmed by {} ({})...",
                            result.participants.len(),
                            result.enemy_name.as_deref().unwrap_or("a group of enemies"),
                            result.enemy_count.map(|n| format!("{n} enemies")).unwrap_or_default()
                        ),
                    };

                    // Boss fights only (per-fight MVP breakdown isn't
                    // interesting for a filler Basic encounter) - see
                    // summarize_fight's doc. Appended to THIS message
                    // (the fight outcome), not the loot one below - per
                    // an earlier request, one message for the fight, one
                    // for the loot, instead of everything crammed into a
                    // single message. Each category lists its top 3
                    // contributors' own individual amount (e.g.
                    // "Lokati_Gaming (120.7K), xDaido (98.2K), Zolaries
                    // (76.5K)") - per a live request to bring per-person
                    // numbers back after a brief combined-total version.
                    // Uses the same K/M/B/T abbreviation the character
                    // sheet already relies on (`adventure_web::format_number`)
                    // specifically to keep this within Twitch's ~500-char
                    // limit despite the extra detail - raw digit strings
                    // here were the actual reason this got collapsed to a
                    // single combined total in the first place.
                    if result.kind == EncounterKind::Boss {
                        let summary = summarize_fight(&result.units, &result.events);
                        let per_person = |entries: &[(String, u64)]| {
                            entries.iter().map(|(name, amt)| format!("{name} ({})", adventure_web::format_number(*amt as f64))).collect::<Vec<_>>().join(", ")
                        };
                        let mut parts = Vec::new();
                        if !summary.top_damage_dealt.is_empty() {
                            parts.push(format!("🗡️ Top DPS: {}", per_person(&summary.top_damage_dealt)));
                        }
                        if !summary.top_damage_taken.is_empty() {
                            parts.push(format!("🛡️ Top Tanks: {}", per_person(&summary.top_damage_taken)));
                        }
                        if !summary.top_healing_done.is_empty() {
                            parts.push(format!("💚 Top Heals: {}", per_person(&summary.top_healing_done)));
                        }
                        if let Some(name) = &summary.first_to_die {
                            parts.push(format!("💀 {name} first down"));
                        }
                        if !parts.is_empty() {
                            msg.push_str(&format!(" | {}", parts.join(" · ")));
                        }

                        // One-time: the top healer of the first boss
                        // fight after this launched gets a Celestial
                        // Shard - "award the top healer of the next
                        // fight" per the request. Computed from the raw
                        // events' real ids (not `summary` above, which
                        // only has display names) since granting
                        // something needs the actual character id. If
                        // nobody healed this particular fight, the
                        // marker is deliberately left unset so this
                        // retries on the next one instead of awarding
                        // nobody.
                        const CELESTIAL_SHARD_FIRST_AWARD_MARKER_PATH: &str = "adventure-celestial-shard-first-award-marker.json";
                        if twitch_bot_rs::state::load_json::<bool>(CELESTIAL_SHARD_FIRST_AWARD_MARKER_PATH).is_none() {
                            let is_player = |id: &str| result.units.iter().find(|u| u.id == id).map(|u| !u.is_boss).unwrap_or(false);
                            let mut healing: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                            for event in &result.events {
                                if let CombatEvent::Heal { healer, amount, .. } = event {
                                    if is_player(healer) {
                                        *healing.entry(healer.clone()).or_insert(0) += *amount as u64;
                                    }
                                }
                            }
                            if let Some((top_healer_id, _)) = healing.into_iter().max_by_key(|(_, v)| *v) {
                                let display = result.units.iter().find(|u| u.id == top_healer_id).map(|u| u.display_name.clone()).unwrap_or_else(|| top_healer_id.clone());
                                if adventure.grant_craft_token(&top_healer_id, CraftAction::CelestialShard, 1).await {
                                    chat_client
                                        .say(format!("✨ {display} was the top healer of that fight and has been awarded a rare Celestial Shard!"))
                                        .await;
                                }
                                if let Err(err) = twitch_bot_rs::state::save_json(CELESTIAL_SHARD_FIRST_AWARD_MARKER_PATH, &true) {
                                    tracing::error!("Failed to persist celestial shard first award marker: {err}");
                                }
                            }
                        }

                        // Reusable one-time launch giveaways (2026-08-17) -
                        // "award a shard randomly to one of the top 3
                        // healers/dps/tanks following the first boss fight
                        // after the shard is introduced. Same this event
                        // for future giveaways with new items introduced."
                        // Each entry is (marker path, the token to grant,
                        // the name shown in the chat announcement) - adding
                        // a future item's own launch giveaway is a single
                        // new line here, nothing else. Fires on the first
                        // boss fight (win or loss, same as the Celestial
                        // Shard precedent above - a wipe still has real
                        // damage/heal totals) after each entry's own marker
                        // doesn't exist yet, independently of the others.
                        const ITEM_LAUNCH_GIVEAWAYS: &[(&str, CraftAction, &str)] =
                            &[("adventure-unique-shard-first-award-marker.json", CraftAction::UniqueShard, "Unique Shard")];
                        for &(marker_path, action, item_label) in ITEM_LAUNCH_GIVEAWAYS {
                            if twitch_bot_rs::state::load_json::<bool>(marker_path).is_some() {
                                continue;
                            }
                            // Winner pool: this fight's top 3 by damage
                            // dealt, top 3 by damage taken (tanking), and
                            // top 3 by healing, unioned and deduplicated by
                            // id (someone who tops two categories only gets
                            // one entry, not better odds) - "one of the top
                            // 3 healers/dps/tanks" per the request, picked
                            // uniformly at random from that combined pool.
                            // Computed from the raw events' real ids, same
                            // reasoning as the Celestial Shard block above
                            // (`summary`'s own top-3 lists only carry
                            // display names, not the ids granting needs).
                            // Damage-taken totals use `unmitigated_damage`,
                            // matching `summarize_fight`'s own "reflect the
                            // real incoming threat, not what leaked through
                            // DR" convention.
                            let is_player = |id: &str| result.units.iter().find(|u| u.id == id).map(|u| !u.is_boss).unwrap_or(false);
                            let mut damage_dealt: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                            let mut damage_taken: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                            let mut healing: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                            for event in &result.events {
                                match event {
                                    CombatEvent::Attack { attacker, target, damage, unmitigated_damage, .. } => {
                                        if is_player(attacker) {
                                            *damage_dealt.entry(attacker.clone()).or_insert(0) += *damage as u64;
                                        }
                                        if is_player(target) {
                                            *damage_taken.entry(target.clone()).or_insert(0) += *unmitigated_damage as u64;
                                        }
                                    }
                                    CombatEvent::Heal { healer, amount, .. } => {
                                        if is_player(healer) {
                                            *healing.entry(healer.clone()).or_insert(0) += *amount as u64;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            let top3 = |totals: &std::collections::HashMap<String, u64>| -> Vec<String> {
                                let mut sorted: Vec<(&String, &u64)> = totals.iter().collect();
                                sorted.sort_by(|a, b| b.1.cmp(a.1));
                                sorted.into_iter().take(3).map(|(id, _)| id.clone()).collect()
                            };
                            // lokati_gaming (the streamer/admin account -
                            // same login `FIGHTS_PAGE_LOGIN`/
                            // `ADMIN_TUNABLES_LOGIN` gate in adventure_web.rs
                            // use) is deliberately excluded from EVERY
                            // one-time launch giveaway, even when they'd
                            // otherwise land in the top-3 pool - these are
                            // meant for viewers, not the account running
                            // the show. They can still win the SAME item
                            // through the normal ongoing random drop roll
                            // (`maybe_drop_unique_shard` et al.) - this
                            // exclusion is scoped to this giveaway pool
                            // only, per a live request.
                            const LAUNCH_GIVEAWAY_EXCLUDED_WINNER: &str = "lokati_gaming";
                            let mut winner_pool: Vec<String> = Vec::new();
                            for id in top3(&damage_dealt).into_iter().chain(top3(&damage_taken)).chain(top3(&healing)) {
                                if id != LAUNCH_GIVEAWAY_EXCLUDED_WINNER && !winner_pool.contains(&id) {
                                    winner_pool.push(id);
                                }
                            }
                            // Empty pool (nobody dealt/took/healed any
                            // recorded damage, or the only eligible
                            // performers were the excluded account -
                            // shouldn't happen in a real boss fight, but
                            // just in case) leaves the marker unset so
                            // this retries next fight, same as the
                            // Celestial Shard block above.
                            if winner_pool.is_empty() {
                                continue;
                            }
                            let winner_id = winner_pool[rand::thread_rng().gen_range(0..winner_pool.len())].clone();
                            let display = result.units.iter().find(|u| u.id == winner_id).map(|u| u.display_name.clone()).unwrap_or_else(|| winner_id.clone());
                            if adventure.grant_craft_token(&winner_id, action, 1).await {
                                chat_client
                                    .say(format!("🎁 {display} was randomly drawn from that fight's top performers and has been awarded a {item_label}!"))
                                    .await;
                            }
                            if let Err(err) = twitch_bot_rs::state::save_json(marker_path, &true) {
                                tracing::error!("Failed to persist {item_label} launch giveaway marker: {err}");
                            }
                        }
                    }
                    chat_client.say(msg).await;

                    // Loot/broken/retreated get their own separate
                    // message - see the doc above the battle-report
                    // block for why this is split out at all.
                    let mut loot_msg = String::new();

                    // Boss fights only skip the 🎁 loot list itself now
                    // (2026-08-16, a live request: "it's just extra spam"
                    // on top of the boss summary/MVP breakdown message
                    // above) - a Basic encounter's loot line is still the
                    // only loot feedback that fight gets in chat at all,
                    // so it's untouched. Worn-out/retreated below stay
                    // for both kinds regardless - only the loot list was
                    // flagged as noise.
                    if !result.loot.is_empty() && result.kind != EncounterKind::Boss {
                        // Auto-disenchant is silent by request (2026-08-17) -
                        // it's cleanup the player opted into, not a loot
                        // event worth announcing in chat.
                        let parts: Vec<String> = result
                            .loot
                            .iter()
                            .filter_map(|loot| match loot.outcome {
                                ReceiveOutcome::Equipped => {
                                    Some(format!("{} equipped a {}", loot.display_name, loot.item_name))
                                }
                                ReceiveOutcome::AddedToBag => {
                                    Some(format!("{} bagged a {}", loot.display_name, loot.item_name))
                                }
                                ReceiveOutcome::BagFull => {
                                    Some(format!("{} lost a {} (bag full)", loot.display_name, loot.item_name))
                                }
                                ReceiveOutcome::AutoDisenchanted { .. } => None,
                            })
                            .collect();
                        if !parts.is_empty() {
                            loot_msg.push_str(&format!("🎁 {}", join_capped(&parts, 3)));
                        }
                    }

                    if !result.broken.is_empty() {
                        let parts: Vec<String> = result
                            .broken
                            .iter()
                            .map(|item| format!("{}'s {}", item.display_name, item.item_name))
                            .collect();
                        if !loot_msg.is_empty() {
                            loot_msg.push_str(" · ");
                        }
                        loot_msg.push_str(&format!("💔 Worn out: {}", join_capped(&parts, 3)));
                    }

                    if !result.retreated.is_empty() {
                        if !loot_msg.is_empty() {
                            loot_msg.push_str(" · ");
                        }
                        loot_msg.push_str(&format!("🏳️ Retreated (repair on the dashboard!): {}", join_capped(&result.retreated, 3)));
                    }

                    if !loot_msg.is_empty() {
                        chat_client.say(loot_msg).await;
                    }
                });
            }
        });
    }

    // Reforge's 1% and recombine's 5% "bonus modifier" rolls (see
    // GearCritEvent) both funnel through this one subscriber - no delay
    // needed here (unlike the encounter one above), since neither action
    // is tied to any overlay animation to wait out.
    {
        let chat_client = chat_client.clone();
        let mut gear_crit_rx = adventure.subscribe_gear_crits();
        tokio::spawn(async move {
            while let Ok(event) = gear_crit_rx.recv().await {
                let verb = match event.source {
                    GearCritSource::Reforge => "reforge",
                    GearCritSource::Recombine => "recombination",
                };
                let msg = format!(
                    "🎲✨ {}'s {verb} CRIT! Their new {} ({:?}, tier {}) rolled a bonus {} modifier!",
                    event.display_name,
                    event.item_name,
                    event.slot,
                    event.tier,
                    affix_name(event.affix)
                );
                chat_client.say(msg).await;
            }
        });
    }

    // !rampage completion (2026-08-17, a live request: "there should also
    // be an announcement when a rampage is complete") - fires only when a
    // finite !rampage/vote-triggered countdown finishes naturally, not
    // when Permanent Rampage is toggled off (see `rampage_complete_tx`'s
    // own doc).
    {
        let chat_client = chat_client.clone();
        let mut rampage_complete_rx = adventure.subscribe_rampage_complete();
        tokio::spawn(async move {
            while rampage_complete_rx.recv().await.is_ok() {
                chat_client.say("🔥 Rampage complete! Things have settled back down... for now.".to_string()).await;
            }
        });
    }

    // Unique Shard wins from the normal ongoing random drop roll
    // (2026-08-17, a live request: "make sure an announcement goes out
    // anytime someone wins a unique shard as well") - separate from the
    // ITEM_LAUNCH_GIVEAWAYS one-time launch announcement above, which
    // already covers itself; this covers every win after that.
    {
        let chat_client = chat_client.clone();
        let mut unique_shard_rx = adventure.subscribe_unique_shard_wins();
        tokio::spawn(async move {
            while let Ok(event) = unique_shard_rx.recv().await {
                chat_client.say(format!("💎 {} just found a rare Unique Shard!", event.display_name)).await;
            }
        });
    }

    let services = Arc::new(Services {
        helix,
        broadcaster_id,
        patreon: patreon_watcher,
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
                if let Some(new_level) = services.adventure.grant_activity_xp(&msg.sender).await {
                    chat_client.say(format!("{} leveled up to level {new_level}!", msg.sender)).await;
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
