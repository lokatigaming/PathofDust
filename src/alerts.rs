// Local HTTP server for the OBS "Alert Box" browser source — ports
// alert-server.js. Push-only (Server-Sent Events); the existing
// public/alert-box.html frontend is reused byte-for-byte, just served by
// axum instead of Node's raw http module. bot.js's EventSub/StreamElements
// handlers call `broadcast()` any time an alert should fire.

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tower_http::services::ServeDir;

pub struct AlertServer {
    tx: broadcast::Sender<serde_json::Value>,
}

impl AlertServer {
    pub fn broadcast(&self, event: serde_json::Value) {
        // No receivers connected right now (OBS not open, or the alert-box
        // Browser Source's SSE connection happened to be dropped/
        // reconnecting at this exact instant) means this event is lost —
        // SSE here is purely live, nothing buffers it for a client that
        // connects/reconnects a moment later. That's an inherent
        // limitation of this fire-and-forget design, not an error as far
        // as the send() call is concerned — but it's exactly the kind of
        // thing that otherwise leaves zero trace when an alert silently
        // never shows on stream, so it's worth at least logging.
        if self.tx.receiver_count() == 0 {
            tracing::warn!("Alert broadcast with no connected browser source (SSE) — this alert will not be seen: {event}");
        }
        let _ = self.tx.send(event);
    }
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<serde_json::Value>,
    public_dir: PathBuf,
}

pub async fn start_alert_server(port: u16, public_dir: PathBuf) -> anyhow::Result<Arc<AlertServer>> {
    let (tx, _rx) = broadcast::channel(100);
    let server = Arc::new(AlertServer { tx: tx.clone() });

    let state = AppState { tx, public_dir: public_dir.clone() };

    let app = axum::Router::new()
        .route("/events", get(sse_handler))
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Alert server crashed: {err}");
        }
    });

    tracing::info!("Alert box server running — point an OBS Browser Source at http://localhost:{port}/alert-box.html");

    Ok(server)
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    match tokio::fs::read_to_string(state.public_dir.join("alert-box.html")).await {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(value) => Some(Ok(Event::default().data(value.to_string()))),
        // A lagged receiver (fell behind) just skips those events instead
        // of ending the stream — same "best effort" delivery as the
        // Node version's plain res.write() to an SSE connection.
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
