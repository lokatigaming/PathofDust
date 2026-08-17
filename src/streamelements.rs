// Real-time StreamElements tip/donation events via their Socket.IO push API
// — ports streamelements.js. Tips aren't a Twitch platform event (they go
// through PayPal via StreamElements), so this is a separate connection
// specifically for that, same as the Node version.
//
// Needs a JWT: StreamElements dashboard -> Account -> Channels -> "Show
// secrets" -> JWT Token. Set it as STREAMELEMENTS_JWT in .env.

use rust_socketio::asynchronous::{Client, ClientBuilder};
use rust_socketio::{Payload, TransportType};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

const REALTIME_URL: &str = "https://realtime.streamelements.com";
const HISTORY_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tip {
    pub name: String,
    pub amount: f64,
    pub currency: String,
    #[serde(default)]
    pub message: String,
}

/// Shared between this struct and the "event" callback registered with the
/// socket.io client, so both sides are always looking at the same state —
/// no separate/disconnected copy of the history.
pub struct StreamElementsWatcher {
    history: Arc<Mutex<Vec<Tip>>>,
    history_path: PathBuf,
    _client: Client,
}

impl StreamElementsWatcher {
    /// Most-recent-first, capped at `count`.
    pub async fn get_recent_tips(&self, count: usize) -> Vec<Tip> {
        let history = self.history.lock().await;
        history.iter().take(count).cloned().collect()
    }
}

fn parse_tip(payload: &Payload) -> Option<Tip> {
    let Payload::Text(values) = payload else { return None };
    let event = values.first()?;
    if event.get("type")?.as_str()? != "tip" {
        return None;
    }
    let data = event.get("data")?;
    Some(Tip {
        name: data
            .get("username")
            .or_else(|| data.get("displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or("Anonymous")
            .to_string(),
        amount: data.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0),
        currency: data.get("currency").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        message: data.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

pub async fn start_streamelements_watcher(
    jwt: String,
    history_path: PathBuf,
    on_tip: impl Fn(Tip) + Send + Sync + 'static,
) -> anyhow::Result<Arc<StreamElementsWatcher>> {
    let loaded: Vec<Tip> = crate::state::load_json(&history_path).unwrap_or_default();
    let history = Arc::new(Mutex::new(loaded));
    let on_tip = Arc::new(on_tip);

    let history_for_callback = history.clone();
    let history_path_for_callback = history_path.clone();

    let client = ClientBuilder::new(REALTIME_URL)
        // StreamElements' realtime server rejects the default HTTP-polling
        // handshake outright (400 "Transport unknown") — the crate doesn't
        // handle that error response gracefully and chokes trying to parse
        // it as a real packet ("Invalid packet id: 123", i.e. the '{' of
        // the JSON error body). Forcing WebSocket-only skips polling
        // entirely and connects directly, which the server does support.
        .transport_type(TransportType::Websocket)
        .on("authenticated", |payload, _| {
            Box::pin(async move {
                tracing::info!("StreamElements: authenticated ({payload:?})");
            })
        })
        .on("unauthorized", |payload, _| {
            Box::pin(async move {
                tracing::error!(
                    "StreamElements: authentication failed — check STREAMELEMENTS_JWT in .env: {payload:?}"
                );
            })
        })
        .on("event", move |payload, _| {
            let on_tip = on_tip.clone();
            let history = history_for_callback.clone();
            let history_path = history_path_for_callback.clone();
            Box::pin(async move {
                let Some(tip) = parse_tip(&payload) else { return };

                {
                    let mut history = history.lock().await;
                    history.insert(0, tip.clone());
                    history.truncate(HISTORY_LIMIT);
                    if let Err(err) = crate::state::save_json(&history_path, &*history) {
                        tracing::error!("Failed to persist tips-history.json: {err}");
                    }
                }

                on_tip(tip);
            })
        })
        .connect()
        .await?;

    client.emit("authenticate", json!({ "method": "jwt", "token": jwt })).await?;

    Ok(Arc::new(StreamElementsWatcher {
        history,
        history_path,
        _client: client,
    }))
}
