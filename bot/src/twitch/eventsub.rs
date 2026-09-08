// Twitch EventSub over WebSocket — ports the @twurple/eventsub-ws half of
// bot.js. Hand-rolled against Twitch's documented WebSocket envelope
// (verified directly against dev.twitch.tv before writing this, not
// guessed) since there's no equivalent to @twurple/eventsub-ws in the
// Rust ecosystem yet.
//
// Reconnect strategy: on any disconnect (clean session_reconnect message or
// a dropped connection), just reconnect fresh to the default URL and
// re-create every subscription against the new session id, rather than
// relying on Twitch's session migration behavior on the reconnect_url —
// more code, but unambiguously correct either way.

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

use super::auth::AuthClient;

// keepalive_timeout_seconds=30 (Twitch's default is 10s) asks Twitch to
// send a session_keepalive at least that often — paired with the
// read-timeout in run_session below, which is how a silently-dead
// connection (network blip, no clean close) actually gets detected and
// reconnected instead of hanging forever in read.next().await.
const WS_URL: &str = "wss://eventsub.wss.twitch.tv/ws?keepalive_timeout_seconds=30";

// Generous margin over the requested keepalive interval — if nothing at
// all (not even a keepalive) arrives in this window, the connection is
// treated as dead and reconnected, rather than trusting TCP to notice.
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const HELIX_BASE: &str = "https://api.twitch.tv/helix";

#[derive(Debug, Clone)]
pub enum TwitchEvent {
    Follow { user_name: String },
    Subscription { user_name: String },
    SubscriptionGift { gifter_name: String, amount: u32 },
    Cheer { user_name: String, bits: u32 },
    Raid { from_broadcaster_name: String, viewers: u32 },
    /// Someone redeemed the "Set Entrance Theme Song" channel points
    /// reward — user_input is whatever they typed (their YouTube
    /// link/search), only present at all if the reward requires text
    /// input, which it's created with (see channel_points.rs).
    ChannelPointsRedemption { redemption_id: String, reward_id: String, user_name: String, user_input: String },
}

#[derive(Deserialize)]
struct Envelope {
    metadata: Metadata,
    payload: serde_json::Value,
}

#[derive(Deserialize)]
struct Metadata {
    message_type: String,
    #[serde(default)]
    subscription_type: Option<String>,
}

async fn create_subscription(
    http: &reqwest::Client,
    auth: &AuthClient,
    session_id: &str,
    sub_type: &str,
    version: &str,
    condition: serde_json::Value,
) -> anyhow::Result<()> {
    let access_token = auth.get_valid_access_token().await?;
    let resp = http
        .post(format!("{HELIX_BASE}/eventsub/subscriptions"))
        .bearer_auth(access_token)
        .header("Client-Id", auth.client_id().await)
        .json(&json!({
            "type": sub_type,
            "version": version,
            "condition": condition,
            "transport": { "method": "websocket", "session_id": session_id },
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to create {sub_type} subscription: {body}");
    }
    Ok(())
}

/// Twitch doesn't delete a WebSocket-transport subscription the instant its
/// connection dies — it lingers in `websocket_disconnected` status for a
/// while. If a reconnect tries to create a fresh subscription of the same
/// type+condition before that cleanup happens, Twitch rejects it as a
/// conflict, which used to abort the entire subscribe_all chain (since
/// each subscription type was created sequentially with `?`) and leave
/// *zero* working alerts until the zombie eventually expired on its own —
/// the "sometimes works, sometimes doesn't" bug. Deleting every existing
/// subscription first, every time, means there's never a stale one left
/// around to conflict with.
async fn cleanup_existing_subscriptions(http: &reqwest::Client, auth: &AuthClient) -> anyhow::Result<()> {
    let access_token = auth.get_valid_access_token().await?;
    let client_id = auth.client_id().await;
    let mut cursor: Option<String> = None;

    loop {
        let mut req = http.get(format!("{HELIX_BASE}/eventsub/subscriptions")).bearer_auth(&access_token).header(
            "Client-Id",
            &client_id,
        );
        if let Some(c) = &cursor {
            req = req.query(&[("after", c.as_str())]);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list existing EventSub subscriptions: {body}");
        }

        let data: serde_json::Value = resp.json().await?;
        let items = data.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
        if items.is_empty() {
            break;
        }

        for item in &items {
            let Some(id) = item.get("id").and_then(|v| v.as_str()) else { continue };
            let del = http
                .delete(format!("{HELIX_BASE}/eventsub/subscriptions"))
                .bearer_auth(&access_token)
                .header("Client-Id", &client_id)
                .query(&[("id", id)])
                .send()
                .await;
            if let Err(err) = del {
                tracing::warn!("Failed to delete stale EventSub subscription {id}: {err}");
            }
        }

        cursor = data.get("pagination").and_then(|p| p.get("cursor")).and_then(|v| v.as_str()).map(String::from);
        if cursor.is_none() {
            break;
        }
    }

    Ok(())
}

async fn subscribe_all(
    http: &reqwest::Client,
    auth: &AuthClient,
    session_id: &str,
    broadcaster_id: &str,
    theme_reward_id: Option<&str>,
    interrupt_reward_id: Option<&str>,
) -> anyhow::Result<()> {
    if let Err(err) = cleanup_existing_subscriptions(http, auth).await {
        tracing::warn!("EventSub: failed to clean up stale subscriptions before resubscribing: {err}");
    }

    create_subscription(
        http,
        auth,
        session_id,
        "channel.follow",
        "2",
        json!({ "broadcaster_user_id": broadcaster_id, "moderator_user_id": broadcaster_id }),
    )
    .await?;

    create_subscription(
        http,
        auth,
        session_id,
        "channel.subscription.gift",
        "1",
        json!({ "broadcaster_user_id": broadcaster_id }),
    )
    .await?;

    create_subscription(
        http,
        auth,
        session_id,
        "channel.cheer",
        "1",
        json!({ "broadcaster_user_id": broadcaster_id }),
    )
    .await?;

    create_subscription(
        http,
        auth,
        session_id,
        "channel.raid",
        "1",
        json!({ "to_broadcaster_user_id": broadcaster_id }),
    )
    .await?;

    // Only subscribed once the "Set Entrance Theme Song" reward has
    // actually been created (see channel_points.rs's ensure_theme_reward)
    // — condition scoped to that specific reward id so this doesn't fire
    // for whatever other channel points rewards the streamer already has.
    if let Some(reward_id) = theme_reward_id {
        create_subscription(
            http,
            auth,
            session_id,
            "channel.channel_points_custom_reward_redemption.add",
            "1",
            json!({ "broadcaster_user_id": broadcaster_id, "reward_id": reward_id }),
        )
        .await?;
    }

    // Same idea, scoped to "Interrupt the Music" instead (see
    // channel_points.rs's ensure_interrupt_reward) — a second, independent
    // condition since a redemption subscription can only ever be scoped to
    // one reward_id at a time.
    if let Some(reward_id) = interrupt_reward_id {
        create_subscription(
            http,
            auth,
            session_id,
            "channel.channel_points_custom_reward_redemption.add",
            "1",
            json!({ "broadcaster_user_id": broadcaster_id, "reward_id": reward_id }),
        )
        .await?;
    }

    Ok(())
}

fn parse_notification(subscription_type: &str, event: &serde_json::Value) -> Option<TwitchEvent> {
    let get_str = |key: &str| event.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let get_u32 = |key: &str| event.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let get_bool = |key: &str| event.get(key).and_then(|v| v.as_bool()).unwrap_or(false);

    match subscription_type {
        "channel.follow" => Some(TwitchEvent::Follow { user_name: get_str("user_name") }),
        "channel.subscription.gift" => {
            let gifter_name = if get_bool("is_anonymous") { "Anonymous".to_string() } else { get_str("user_name") };
            Some(TwitchEvent::SubscriptionGift { gifter_name, amount: get_u32("total") })
        }
        "channel.cheer" => {
            let user_name = if get_bool("is_anonymous") { "Anonymous".to_string() } else { get_str("user_name") };
            Some(TwitchEvent::Cheer { user_name, bits: get_u32("bits") })
        }
        "channel.raid" => Some(TwitchEvent::Raid {
            from_broadcaster_name: get_str("from_broadcaster_user_name"),
            viewers: get_u32("viewers"),
        }),
        "channel.channel_points_custom_reward_redemption.add" => Some(TwitchEvent::ChannelPointsRedemption {
            redemption_id: get_str("id"),
            reward_id: event.get("reward").and_then(|r| r.get("id")).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            user_name: get_str("user_name"),
            user_input: get_str("user_input"),
        }),
        _ => None,
    }
}

pub async fn start_eventsub_listener(
    auth: Arc<AuthClient>,
    broadcaster_id: String,
    theme_reward_id: Option<String>,
    interrupt_reward_id: Option<String>,
    on_event: impl Fn(TwitchEvent) + Send + Sync + 'static,
) {
    let http = reqwest::Client::new();
    let on_event = Arc::new(on_event);

    tokio::spawn(async move {
        loop {
            match run_session(
                &http,
                &auth,
                &broadcaster_id,
                theme_reward_id.as_deref(),
                interrupt_reward_id.as_deref(),
                &on_event,
            )
            .await
            {
                Ok(()) => tracing::warn!("EventSub session ended cleanly, reconnecting..."),
                Err(err) => tracing::error!("EventSub session error: {err}, reconnecting in 5s..."),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn run_session(
    http: &reqwest::Client,
    auth: &Arc<AuthClient>,
    broadcaster_id: &str,
    theme_reward_id: Option<&str>,
    interrupt_reward_id: Option<&str>,
    on_event: &Arc<impl Fn(TwitchEvent) + Send + Sync + 'static>,
) -> anyhow::Result<()> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(WS_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    let mut subscribed = false;

    loop {
        let msg = match tokio::time::timeout(READ_TIMEOUT, read.next()).await {
            Ok(Some(msg)) => msg?,
            Ok(None) => break, // connection closed cleanly
            Err(_) => anyhow::bail!("No message (not even a keepalive) in {READ_TIMEOUT:?} — connection presumed dead"),
        };
        let Message::Text(text) = msg else { continue };

        let envelope: Envelope = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!("Failed to parse EventSub message: {err}");
                continue;
            }
        };

        match envelope.metadata.message_type.as_str() {
            "session_welcome" => {
                let session_id = envelope
                    .payload
                    .get("session")
                    .and_then(|s| s.get("id"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("session_welcome missing session id"))?;

                subscribe_all(
                    http,
                    auth,
                    session_id,
                    broadcaster_id,
                    theme_reward_id,
                    interrupt_reward_id,
                )
                .await?;
                subscribed = true;
                let mut redemptions = Vec::new();
                if theme_reward_id.is_some() {
                    redemptions.push("entrance-theme redemptions");
                }
                if interrupt_reward_id.is_some() {
                    redemptions.push("Interrupt the Music redemptions");
                }
                tracing::info!(
                    "EventSub: subscribed to follows, gift subs, cheers, raids{} (regular subs come from chat).",
                    if redemptions.is_empty() { String::new() } else { format!(", and {}", redemptions.join(" and ")) }
                );
            }
            "session_keepalive" => {
                // Just a heartbeat — no action needed.
            }
            "notification" => {
                // Every notification gets logged, whether or not it ends
                // up producing an alert — previously anything that
                // didn't map to a TwitchEvent (missing fields, a
                // subscription_type we don't handle, an intentionally
                // skipped gift-flagged channel.subscribe, or anything
                // else) was silently dropped with zero trace, making a
                // "the bot missed an event" report impossible to
                // actually diagnose after the fact.
                let Some(sub_type) = &envelope.metadata.subscription_type else {
                    tracing::warn!("EventSub: notification missing subscription_type: {:?}", envelope.payload);
                    continue;
                };
                let Some(event) = envelope.payload.get("event") else {
                    tracing::warn!("EventSub: notification ({sub_type}) missing \"event\" field: {:?}", envelope.payload);
                    continue;
                };
                match parse_notification(sub_type, event) {
                    Some(parsed) => {
                        tracing::info!("EventSub: notification received — {sub_type}");
                        on_event(parsed);
                    }
                    None => {
                        tracing::info!("EventSub: notification ({sub_type}) didn't map to an announced event: {event}");
                    }
                }
            }
            "session_reconnect" => {
                tracing::info!("EventSub: server requested reconnect.");
                break;
            }
            "revocation" => {
                tracing::warn!("EventSub: a subscription was revoked: {:?}", envelope.payload);
            }
            other => {
                tracing::debug!("EventSub: unhandled message type {other}");
            }
        }
    }

    // Close cleanly if we still have the sink (best-effort; connection may
    // already be gone if we got here via an error).
    let _ = write.send(Message::Close(None)).await;

    if !subscribed {
        anyhow::bail!("Connection closed before receiving session_welcome");
    }
    Ok(())
}
