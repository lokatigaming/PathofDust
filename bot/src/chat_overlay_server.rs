// HTTP + WebSocket server for the transparent, draggable Twitch chat
// overlay (public_chat_overlay/overlay.html). Every incoming chat message
// is tokenized against the merged Twitch/BTTV/FFZ emote map (fetched once
// at startup via emotes.rs) into text/emote segments *server-side*, so the
// browser page never has to know about emote APIs at all — it just renders
// whatever segments arrive over the WebSocket.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

use crate::emotes::EmoteMap;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Segment {
    Text { value: String },
    Emote { code: String, url: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatEvent {
    pub sender: String,
    pub segments: Vec<Segment>,
}

pub struct ChatOverlayServer {
    emotes: HashMap<String, String>,
    tx: broadcast::Sender<ChatEvent>,
}

impl ChatOverlayServer {
    pub fn broadcast_message(&self, sender: &str, text: &str) {
        let segments = tokenize(text, &self.emotes);
        let _ = self.tx.send(ChatEvent { sender: sender.to_string(), segments });
    }

    fn subscribe(&self) -> broadcast::Receiver<ChatEvent> {
        self.tx.subscribe()
    }
}

fn tokenize(text: &str, emotes: &HashMap<String, String>) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_text = String::new();

    for word in text.split(' ') {
        if word.is_empty() {
            continue;
        }
        if let Some(url) = emotes.get(word) {
            if !current_text.is_empty() {
                segments.push(Segment::Text { value: std::mem::take(&mut current_text) });
            }
            segments.push(Segment::Emote { code: word.to_string(), url: url.clone() });
        } else {
            if !current_text.is_empty() {
                current_text.push(' ');
            }
            current_text.push_str(word);
        }
    }
    if !current_text.is_empty() {
        segments.push(Segment::Text { value: current_text });
    }

    segments
}

#[derive(Clone)]
struct AppState {
    server: Arc<ChatOverlayServer>,
    public_dir: PathBuf,
}

pub async fn start_chat_overlay_server(port: u16, public_dir: PathBuf, emotes: EmoteMap) -> anyhow::Result<Arc<ChatOverlayServer>> {
    let (tx, _rx) = broadcast::channel(256);
    let server = Arc::new(ChatOverlayServer { emotes: emotes.emotes, tx });

    let state = AppState { server: server.clone(), public_dir: public_dir.clone() };

    let app = axum::Router::new()
        .route("/ws", get(ws_handler))
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Chat overlay server crashed: {err}");
        }
    });

    tracing::info!("Chat overlay running — point an OBS Browser Source at http://localhost:{port}/ (enable \"Control audio/video via OBS\" off, transparent background is built in).");
    Ok(server)
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    match tokio::fs::read_to_string(state.public_dir.join("overlay.html")).await {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.server))
}

async fn handle_socket(socket: WebSocket, server: Arc<ChatOverlayServer>) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = server.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let Ok(json) = serde_json::to_string(&event) else { continue };
            if sink.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // The overlay page is push-only (display + drag position is all local
    // browser state, nothing it needs to tell the server) — just drain
    // incoming frames so the connection's close/ping frames are handled,
    // and let a closed socket end the send loop above too.
    let mut recv_task = tokio::spawn(async move { while stream.next().await.is_some() {} });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
