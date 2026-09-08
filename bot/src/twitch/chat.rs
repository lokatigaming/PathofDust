// Twitch chat connection — ports the ChatClient half of bot.js, using the
// `twitch-irc` crate's built-in refreshing-token support (backed by the
// same AuthClient/tokens.json used everywhere else in this project).

use std::sync::Arc;
use tokio::sync::mpsc;
use twitch_irc::login::RefreshingLoginCredentials;
use twitch_irc::message::{ServerMessage, UserNoticeEvent};
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

use super::auth::{AuthClient, TwitchAuthStorage};
use super::eventsub::TwitchEvent;

pub type Credentials = RefreshingLoginCredentials<TwitchAuthStorage>;
pub type Inner = TwitchIRCClient<SecureTCPTransport, Credentials>;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub channel: String,
    /// Display name (proper capitalization), matching how the Node bot's
    /// twurple `user` callback param is used in user-facing text.
    pub sender: String,
    pub text: String,
    pub is_mod_or_broadcaster: bool,
    /// Broadcaster specifically, not moderators — for the handful of
    /// commands that need to be streamer-only (e.g. !forceplay).
    pub is_broadcaster: bool,
}

pub struct ChatClient {
    inner: Inner,
    channel: String,
}

impl ChatClient {
    /// Every bot-originated message is prefixed with "Sikwiq" to stay
    /// visually distinguishable from the streamer's own manual chat
    /// messages — same intentional behavior (and the same known 7TV
    /// case-sensitivity caveat) as the Node bot.
    pub async fn say(&self, text: impl Into<String>) {
        let mut full = format!("Sikwiq {}", text.into());
        // Twitch silently drops a chat message over ~500 characters
        // instead of erroring - confirmed live 2026-08-13, where a
        // verbose !character reply landed at 600+ chars and just never
        // appeared in chat, with nothing in this bot's own logs (the
        // twitch-irc send call itself reports success either way - it
        // only knows the local write succeeded, not what Twitch's server
        // did with it after). This is a backstop against every caller,
        // present and future, not just !character's own reply - better a
        // visibly-trimmed message than a perfectly-formed one nobody
        // ever sees. (The per-reply fix that went with this lived in
        // `Character::gear_summary`, deleted 2026-09-04 once Twitch's
        // removal left it with no callers - see the note where it was.)
        const TWITCH_MAX_MESSAGE_LEN: usize = 500;
        let len = full.chars().count();
        if len > TWITCH_MAX_MESSAGE_LEN {
            let truncated: String = full.chars().take(TWITCH_MAX_MESSAGE_LEN - 1).collect();
            tracing::warn!("Chat message truncated from {len} to {TWITCH_MAX_MESSAGE_LEN} chars: {truncated}");
            full = format!("{truncated}…");
        }
        // Observability request (2026-08-19, following an audit that
        // couldn't verify any real announcement's actual chat text since
        // nothing logged it) - log every bot-originated message, full
        // text, regardless of outcome. This is the single choke point
        // every chat message (commands, redemptions, and every game
        // announcement relayed off the SSE stream) already goes through,
        // so one log line here covers all of them without touching any
        // caller.
        tracing::info!(chars = full.chars().count(), "chat send: {full}");
        if let Err(err) = self.inner.say(self.channel.clone(), full).await {
            tracing::error!("Failed to send chat message: {err}");
        }
    }
}

pub async fn connect(
    client_id: String,
    client_secret: String,
    channel: String,
    auth: Arc<AuthClient>,
) -> anyhow::Result<(Arc<ChatClient>, mpsc::UnboundedReceiver<ChatMessage>, mpsc::UnboundedReceiver<TwitchEvent>)> {
    // twitch-irc requires lowercase login names for JOIN/PRIVMSG targets —
    // TWITCH_CHANNEL in .env is kept in its display-friendly casing (used
    // elsewhere for readability), so normalize it here at the one place
    // that actually needs IRC's stricter rules.
    let channel = channel.to_lowercase();
    let storage = TwitchAuthStorage { auth };
    let credentials = RefreshingLoginCredentials::init(client_id, client_secret, storage);
    let config = ClientConfig::new_simple(credentials);
    let (mut incoming_messages, inner) = Inner::new(config);

    inner.join(channel.clone())?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            match message {
                ServerMessage::Privmsg(msg) => {
                    let is_broadcaster = msg.badges.iter().any(|b| b.name == "broadcaster");
                    let is_mod_or_broadcaster =
                        is_broadcaster || msg.badges.iter().any(|b| b.name == "moderator");

                    let _ = tx.send(ChatMessage {
                        channel: msg.channel_login,
                        sender: msg.sender.name,
                        text: msg.message_text,
                        is_mod_or_broadcaster,
                        is_broadcaster,
                    });
                }
                // Twitch posts a native USERNOTICE for sub/resub over this
                // same already-connected IRC socket, independent of the
                // EventSub WebSocket — added as the sub/resub detection
                // path after a real subscription was confirmed to reach
                // chat but never trigger an EventSub notification.
                // subscribe_all no longer creates a channel.subscribe
                // EventSub subscription at all, so this is now the sole
                // source for regular (non-gift) subs — no double-announce
                // risk. Gift subs are intentionally left on EventSub
                // (channel.subscription.gift), which hasn't shown the same
                // problem.
                ServerMessage::UserNotice(msg) => {
                    if let UserNoticeEvent::SubOrResub { .. } = &msg.event {
                        tracing::info!("Chat USERNOTICE: {} — {}", msg.event_id, msg.sender.name);
                        let _ = event_tx.send(TwitchEvent::Subscription { user_name: msg.sender.name });
                    }
                }
                _ => {}
            }
        }
    });

    tracing::info!("Connected to chat in #{channel}");

    Ok((Arc::new(ChatClient { inner, channel }), rx, event_rx))
}
