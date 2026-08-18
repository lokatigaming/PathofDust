// HTTP + WebSocket server for the chat adventure overlay
// (public_adventure_overlay/overlay.html) — pushes the current roster/
// stage snapshot on connect and again on every change (join, level-up,
// encounter), plus a one-shot "encounter" event whenever the party
// auto-battles, so the browser source can animate it. Push-only, same
// shape as chat_overlay_server.rs — the page has nothing to report back.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use crate::adventure::{AdventureManager, AdventureSnapshot, EncounterResult};

#[derive(Clone)]
struct AppState {
    manager: Arc<AdventureManager>,
    public_dir: PathBuf,
}

pub async fn start_adventure_overlay_server(port: u16, public_dir: PathBuf, manager: Arc<AdventureManager>) -> anyhow::Result<()> {
    let state = AppState { manager, public_dir: public_dir.clone() };

    let app = axum::Router::new()
        .route("/ws", get(ws_handler))
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state)
        // `serve_index`'s own no-store header (see its doc) only covers
        // the bare `/` route - a live report of the overlay STILL
        // showing stale sprites after a real server-side fix landed
        // traced back to this: if the OBS Browser Source URL is
        // anything other than exactly `/` (e.g. `/overlay.html`
        // directly), it hits the ServeDir fallback below instead, which
        // sets no such header and is just as vulnerable to the same CEF
        // caching quirk. Applied server-wide (sprites included - a
        // little wasted re-fetching on those is a fine trade for never
        // silently serving stale game logic again) rather than trying
        // to guess which exact URL every OBS setup actually uses.
        .layer(axum::middleware::from_fn(force_no_store));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Adventure overlay server crashed: {err}");
        }
    });

    tracing::info!("Adventure overlay running — point an OBS Browser Source at http://localhost:{port}/");
    Ok(())
}

/// Stamps every response from this server with `Cache-Control: no-store`
/// - see the `.layer` call site's doc for why this needs to cover the
/// WHOLE server, not just the one route `serve_index` already handled
/// directly.
async fn force_no_store(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(axum::http::header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
    response
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    match tokio::fs::read_to_string(state.public_dir.join("overlay.html")).await {
        // This page is actively being iterated on, and OBS's embedded
        // Chromium (CEF) has been observed serving a stale cached copy
        // even after an edit + "Refresh" — no-store forces every load to
        // actually re-fetch instead of trusting a heuristic cache.
        Ok(contents) => ([(axum::http::header::CACHE_CONTROL, "no-store")], Html(contents)).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.manager))
}

#[derive(Serialize)]
struct StateEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    snapshot: &'a AdventureSnapshot,
}

#[derive(Serialize)]
struct EncounterEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    result: &'a EncounterResult,
}

/// `pub(crate)` (not just `fn`) - adventure_web.rs's own `/ws` route
/// reuses this exact function so the public dashboard (port 4005,
/// already tunneled to adventure.lokati.net) can serve the SAME overlay
/// page/feed without needing its own separate public port/DNS entry -
/// see adventure_web.rs's own `/overlay` route doc.
pub(crate) async fn handle_socket(socket: WebSocket, manager: Arc<AdventureManager>) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // Send the current roster/stage immediately so a freshly (re)connected
    // overlay doesn't sit blank until the next change or encounter.
    if let Ok(json) = serde_json::to_string(&StateEnvelope { kind: "state", snapshot: &manager.snapshot().await }) {
        let _ = out_tx.send(Message::Text(json));
    }

    let mut sink_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut state_task = {
        let out_tx = out_tx.clone();
        let mut rx = manager.subscribe_state();
        tokio::spawn(async move {
            while let Ok(snapshot) = rx.recv().await {
                let Ok(json) = serde_json::to_string(&StateEnvelope { kind: "state", snapshot: &snapshot }) else { continue };
                if out_tx.send(Message::Text(json)).is_err() {
                    break;
                }
            }
        })
    };

    let mut encounter_task = {
        let out_tx = out_tx.clone();
        let mut rx = manager.subscribe_encounters();
        tokio::spawn(async move {
            while let Ok(result) = rx.recv().await {
                let Ok(json) = serde_json::to_string(&EncounterEnvelope { kind: "encounter", result: &result }) else { continue };
                if out_tx.send(Message::Text(json)).is_err() {
                    break;
                }
            }
        })
    };

    // Push-only page (all animation/layout is local browser state) — just
    // drain incoming frames so close/ping frames are handled, and let a
    // closed socket end the send loops above too.
    let mut recv_task = tokio::spawn(async move { while stream.next().await.is_some() {} });

    tokio::select! {
        _ = &mut sink_task => {}
        _ = &mut state_task => {}
        _ = &mut encounter_task => {}
        _ = &mut recv_task => {}
    }
    sink_task.abort();
    state_task.abort();
    encounter_task.abort();
    recv_task.abort();
}
