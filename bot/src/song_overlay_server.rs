// HTTP + WebSocket server for the song request system's two OBS-facing
// pages: the browser-source overlay (public_song_overlay/overlay.html,
// embeds the actual YouTube video) and the control dock
// (public_song_overlay/dock.html, an OBS Custom Browser Dock for play/
// pause/restart/mute/skip and queue management). Both connect to the same
// `/ws` endpoint and see the same message stream — the overlay is the only
// side that actually holds a YouTube player, so transport controls
// (`ControlAction`) issued by the dock are just relayed to the overlay to
// carry out, which then reports the resulting state back over the same
// socket so every connected client (including other docks/overlays) stays
// in sync.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use crate::song_requests::{ControlAction, QueueState, SongRequestManager};

#[derive(Clone)]
struct AppState {
    manager: Arc<SongRequestManager>,
    public_dir: PathBuf,
}

pub async fn start_song_overlay_server(port: u16, public_dir: PathBuf, manager: Arc<SongRequestManager>) -> anyhow::Result<()> {
    let state = AppState { manager, public_dir: public_dir.clone() };

    let app = axum::Router::new()
        .route("/ws", get(ws_handler))
        .route("/", get(serve_overlay))
        .route("/dock", get(serve_dock))
        .route("/duck", post(duck_handler))
        .route("/unduck", post(unduck_handler))
        .route("/mute", post(mute_handler))
        .route("/unmute", post(unmute_handler))
        .route("/alert-pause", post(alert_pause_handler))
        .route("/alert-resume", post(alert_resume_handler))
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!("Song overlay server crashed: {err}");
        }
    });

    tracing::info!("Song request overlay running — point an OBS Browser Source at http://localhost:{port}/");
    tracing::info!("Song request control dock running — add it as an OBS Custom Browser Dock at http://localhost:{port}/dock");
    Ok(())
}

async fn serve_overlay(State(state): State<AppState>) -> impl IntoResponse {
    serve_file(&state.public_dir, "overlay.html").await
}

async fn serve_dock(State(state): State<AppState>) -> impl IntoResponse {
    serve_file(&state.public_dir, "dock.html").await
}

async fn serve_file(dir: &PathBuf, name: &str) -> impl IntoResponse {
    match tokio::fs::read_to_string(dir.join(name)).await {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

/// Called by alert-box.html (a separate OBS browser source, on the alert
/// server's own port) at the exact moment it actually displays an alert —
/// which already correctly accounts for its own batching delay (several
/// same-type alerts landing close together get held and combined for up
/// to 1.5s before showing). Triggering the duck from raw-event arrival
/// instead (the previous approach) meant the music dipped well before
/// the alert actually appeared whenever a batch delay was in play.
async fn duck_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::DuckVolume);
    axum::http::StatusCode::OK
}

/// Called the instant the alert sound that triggered /duck actually
/// finishes (its real `ended`/error event) — restores the music volume.
async fn unduck_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::UnduckVolume);
    axum::http::StatusCode::OK
}

/// Called by alert-box.html's raid video (raid.mp4) alert — muted for
/// exactly as long as that video is actually playing (its own
/// `videoEl.onended`/error handlers call /unmute), not a fixed timer.
async fn mute_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::Mute);
    axum::http::StatusCode::OK
}

/// Called by alert-box.html's follow alert (follow.mp3) instead of
/// /duck — pauses the music outright rather than just lowering it.
async fn alert_pause_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::PauseForAlert);
    axum::http::StatusCode::OK
}

async fn alert_resume_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::ResumeForAlert);
    axum::http::StatusCode::OK
}

async fn unmute_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.manager.send_command(ControlAction::Unmute);
    axum::http::StatusCode::OK
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.manager))
}

#[derive(Serialize)]
struct StateEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    state: &'a QueueState,
}

fn state_message(state: &QueueState) -> Option<String> {
    serde_json::to_string(&StateEnvelope { kind: "state", state }).ok()
}

fn command_message(action: ControlAction) -> Option<String> {
    // Unit variants (play/pause/restart/mute/unmute) serialize as a bare
    // lowercase string under ControlAction's derived Serialize, which the
    // overlay's switch-on-a-string handles directly. SetVolume carries a
    // payload, so it's flattened into its own field instead of nesting an
    // object inside `action`, keeping the overlay's parsing simple.
    let json = match action {
        ControlAction::SetVolume(percent) => {
            serde_json::json!({ "type": "command", "action": "setVolume", "volume": percent })
        }
        ControlAction::InsertSong(video_id) => {
            serde_json::json!({ "type": "command", "action": "insertSong", "videoId": video_id })
        }
        other => serde_json::json!({ "type": "command", "action": other }),
    };
    serde_json::to_string(&json).ok()
}

async fn handle_socket(socket: WebSocket, manager: Arc<SongRequestManager>) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // Send the current state immediately so a freshly (re)connected overlay
    // or dock doesn't wait for the next change to know what's going on.
    if let Some(initial) = state_message(&manager.snapshot()) {
        let _ = out_tx.send(Message::Text(initial));
    }

    let mut sink_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut state_forward_task = {
        let out_tx = out_tx.clone();
        let mut rx = manager.subscribe();
        tokio::spawn(async move {
            while let Ok(state) = rx.recv().await {
                let Some(json) = state_message(&state) else { continue };
                if out_tx.send(Message::Text(json)).is_err() {
                    break;
                }
            }
        })
    };

    let mut command_forward_task = {
        let out_tx = out_tx.clone();
        let mut rx = manager.subscribe_commands();
        tokio::spawn(async move {
            while let Ok(action) = rx.recv().await {
                let Some(json) = command_message(action) else { continue };
                if out_tx.send(Message::Text(json)).is_err() {
                    break;
                }
            }
        })
    };

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let Message::Text(text) = msg else { continue };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(kind) = value.get("type").and_then(|v| v.as_str()) else { continue };

            // The insert handshake used to be invisible from the log: the
            // only trace an `insertEnded` never arrived was
            // clear_active_insert_if_stuck's timeout warning firing much
            // later, which says nothing about what the overlay *did*
            // send. Logging the message type makes the next occurrence
            // diagnosable from the log alone. Volume is bounded by real
            // playback events — a handful per song.
            //
            // THE THREE FIELDS, added 2026-09-13 because the type alone
            // was one field short. On 2026-09-12 an entrance theme looped
            // and the log showed four bare `playerState` and one `muted`
            // between the insert starting and the backstop firing — which
            // could not distinguish "the embed looped" from "the page
            // reloaded mid-insert", and those want different fixes. The
            // `playing` boolean separates them. Still one line per
            // message and still no payload dump: three scalars that are
            // already parsed out below anyway, never the message body.
            tracing::info!("song overlay ws: received {kind}{}", ws_message_detail(&value));

            match kind {
                // From the overlay:
                "ended" => {
                    manager.advance();
                }
                "playerState" => {
                    if let Some(playing) = value.get("playing").and_then(|v| v.as_bool()) {
                        manager.report_player_state(playing);
                    }
                }
                "muted" => {
                    if let Some(muted) = value.get("muted").and_then(|v| v.as_bool()) {
                        manager.report_muted(muted);
                    }
                }
                "insertEnded" => {
                    manager.clear_active_insert();
                }
                // The YouTube IFrame player errored out on the main
                // queue's now-playing video (removed, region-locked,
                // embedding disabled, etc.) — skip it and announce why.
                "playbackError" => {
                    let reason =
                        value.get("reason").and_then(|v| v.as_str()).unwrap_or("an unknown playback error").to_string();
                    manager.report_playback_error(reason);
                }
                // Same, but the failed video was an active !songinsert/
                // entrance-theme insert, not the main queue.
                "insertPlaybackError" => {
                    let reason =
                        value.get("reason").and_then(|v| v.as_str()).unwrap_or("an unknown playback error").to_string();
                    manager.report_insert_playback_error(reason);
                }
                // From the dock:
                "skip" => {
                    manager.advance();
                }
                "remove" => {
                    if let Some(video_id) = value.get("videoId").and_then(|v| v.as_str()) {
                        manager.remove_from_queue(video_id);
                    }
                }
                "clear" => {
                    manager.clear_queue();
                }
                "toggleShowOnStream" => {
                    manager.toggle_show_on_stream();
                }
                "add" => {
                    if let Some(query) = value.get("query").and_then(|v| v.as_str()).map(String::from) {
                        let manager = manager.clone();
                        tokio::spawn(async move {
                            if let Err(err) = manager.request(&query, "Streamer").await {
                                tracing::warn!("Dock song add failed for \"{query}\": {err}");
                            }
                        });
                    }
                }
                "command" => {
                    if let Some(action) = value.get("action").and_then(|v| serde_json::from_value::<ControlAction>(v.clone()).ok()) {
                        manager.send_command(action);
                    }
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut sink_task => {}
        _ = &mut state_forward_task => {}
        _ = &mut command_forward_task => {}
        _ = &mut recv_task => {}
    }
    sink_task.abort();
    state_forward_task.abort();
    command_forward_task.abort();
    recv_task.abort();
}

/// The scalars worth having beside a WS message type in the log, as a
/// suffix like ` playing=true videoId=abc`, or empty when the message
/// carries none of them.
///
/// Deliberately a fixed set of three rather than the whole payload: the
/// log is the thing 7b's rate limiter exists to bound, and a dump would
/// grow with whatever the overlay starts sending next. These three are
/// the ones a stuck insert is diagnosed from — `playing` above all, which
/// is what separates a looping embed from a reloaded page.
fn ws_message_detail(value: &serde_json::Value) -> String {
    let mut detail = String::new();
    if let Some(playing) = value.get("playing").and_then(|v| v.as_bool()) {
        detail.push_str(&format!(" playing={playing}"));
    }
    if let Some(muted) = value.get("muted").and_then(|v| v.as_bool()) {
        detail.push_str(&format!(" muted={muted}"));
    }
    if let Some(video_id) = value.get("videoId").and_then(|v| v.as_str()) {
        detail.push_str(&format!(" videoId={video_id}"));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_state_message_logs_its_playing_flag() {
        let value = serde_json::json!({ "type": "playerState", "playing": false });
        assert_eq!(ws_message_detail(&value), " playing=false");
    }

    /// The 2026-09-12 log showed a `playerState` and a `muted` arriving in
    /// the same millisecond and could not say what either carried. Both
    /// fields now appear when both are present.
    #[test]
    fn both_booleans_appear_when_both_are_present() {
        let value = serde_json::json!({ "type": "muted", "playing": true, "muted": false });
        assert_eq!(ws_message_detail(&value), " playing=true muted=false");
    }

    #[test]
    fn a_playback_error_logs_the_video_it_failed_on() {
        let value = serde_json::json!({ "type": "insertPlaybackError", "videoId": "EdvgC3C4Zgc", "reason": "x" });
        assert_eq!(ws_message_detail(&value), " videoId=EdvgC3C4Zgc");
    }

    /// The common case — `ended`, `insertEnded`, `skip` carry nothing —
    /// must stay exactly as terse as it was before these fields existed.
    #[test]
    fn a_message_with_no_scalars_adds_nothing() {
        assert_eq!(ws_message_detail(&serde_json::json!({ "type": "insertEnded" })), "");
        assert_eq!(ws_message_detail(&serde_json::json!({ "type": "ended" })), "");
    }

    /// Never a payload dump: a field that is not one of the three is not
    /// logged, however interesting it looks. `reason` above is the live
    /// example — it can carry arbitrary text from the embed.
    #[test]
    fn no_other_field_reaches_the_log() {
        // `playing` is in the fixture on purpose. Without a field that IS
        // logged, this test never reaches the code that builds the
        // suffix, and a mutation that appended the whole payload sailed
        // past it — it only failed the two tests that assert an exact
        // string. A guarantee nothing exercises is not a guarantee.
        let value = serde_json::json!({
            "type": "playbackError",
            "playing": true,
            "reason": "a very long embed error",
            "query": "secret"
        });
        assert_eq!(ws_message_detail(&value), " playing=true", "only playing, muted and videoId are logged");
    }
}
