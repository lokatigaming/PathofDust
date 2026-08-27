// HTTP + WebSocket server for the chat adventure overlay
// (public_adventure_overlay/overlay.html) — pushes the current roster/
// stage snapshot on connect and again on every change (join, level-up,
// encounter), plus a one-shot "encounter" event whenever the party
// auto-battles, so the browser source can animate it. Push-only, same
// shape as chat_overlay_server.rs — the page has nothing to report back.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tokio::sync::{broadcast, mpsc};
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

/// Opt-in compression negotiation (2026-08-20, overlay-bandwidth work
/// phase 1 item 1 - see `handle_socket`'s own doc for the full "why not
/// permessage-deflate" reasoning). `?wire=deflate` on the `/ws` URL is
/// the entire negotiation - deliberately NOT a WebSocket subprotocol
/// (`Sec-WebSocket-Protocol`), since a query param needs no client-side
/// API most consumers here don't already reach for, and keeps the
/// negotiation visible in a plain URL for debugging. The exact key/value
/// (`wire=deflate`, not e.g. `compress=1`) is a hard contract with the
/// already-shipped PathOfDust_Desktop 2.6.0 client, which sends this
/// literal string - it is NOT a free choice on this side. Any other
/// value (including absent) means plain text frames, exactly today's
/// behavior - this is the hard requirement the opt-in itself depends on.
#[derive(Deserialize)]
pub(crate) struct WsParams {
    #[serde(default)]
    wire: String,
}

impl WsParams {
    pub(crate) fn wants_compression(&self) -> bool {
        self.wire == "deflate"
    }
}

async fn ws_handler(ws: WebSocketUpgrade, Query(params): Query<WsParams>, State(state): State<AppState>) -> impl IntoResponse {
    let compress = params.wants_compression();
    ws.on_upgrade(move |socket| handle_socket(socket, state.manager, compress))
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

/// World 2 Stage 2 (2026-08-28) - the whole announcement ring, sent once
/// the moment a client connects. One mechanism covers first paint, a
/// client that connected mid-session and a client that dropped and
/// reconnected, which is why there is deliberately no catch-up endpoint
/// beside it. A client REPLACES whatever it is holding with `lines`.
///
/// Not `flatten`ed like the two envelopes above, since the payload is a
/// bare list rather than a struct.
#[derive(Serialize)]
struct AnnouncementBacklogEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    lines: &'a [String],
}

/// One newly-emitted announcement, appended by the client to what the
/// backlog above gave it. Consumers that don't know this type (the OBS
/// overlay, the desktop app) already ignore unknown `type` values - see
/// `overlay.html`'s `handleOverlayMessage`.
#[derive(Serialize)]
struct AnnouncementEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    line: &'a str,
}

/// `compress: false` sends the exact same `Message::Text(json)` frame as
/// before this feature existed. `compress: true` zlib-compresses `json`
/// (RFC 1950 - a 2-byte zlib header, e.g. `0x78 ..`, then a raw deflate
/// stream, then an Adler-32 trailer) and wraps it as a binary frame
/// instead. Bridge-fix (2026-08-20): this was originally raw deflate
/// (RFC 1951, no header) matching `DecompressionStream('deflate-raw')`
/// - correct for the OBS/direct-viewer overlay.html client, but
/// PathOfDust_Desktop 2.6.0's own already-shipped client expects zlib
/// specifically (`DecompressionStream('deflate')`), throws on a raw
/// stream's missing header, and silently falls back to plain JSON -
/// the desktop's own bandwidth win was never actually happening.
/// overlay.html switched to `DecompressionStream('deflate')` in the
/// SAME release this changed, so both clients stay correct together -
/// see that file's own decompression code.
fn send_ready_message(json: String, compress: bool) -> Message {
    if !compress {
        return Message::Text(json);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    // Writing to a Vec<u8> and dropping the encoder's own error type
    // (`io::Error`) can't actually fail here - infallible in practice,
    // matching the same "the sink is in memory" reasoning `io::Write`
    // impls for `Vec` always carry.
    let _ = encoder.write_all(json.as_bytes());
    let compressed = encoder.finish().unwrap_or_default();
    Message::Binary(compressed)
}

/// `pub(crate)` (not just `fn`) - adventure_web.rs's own `/ws` route
/// reuses this exact function so the public dashboard (port 4005,
/// already tunneled to adventure.lokati.net) can serve the SAME overlay
/// page/feed without needing its own separate public port/DNS entry -
/// see adventure_web.rs's own `/overlay` route doc.
///
/// `compress` (2026-08-20, overlay-bandwidth work phase 1 item 1) - an
/// opt-in, per-client application-layer compression scheme, NOT the
/// WebSocket protocol's own permessage-deflate extension: neither axum
/// 0.7 nor the tungstenite 0.21 it wraps implement that extension at
/// all (verified directly against both crates' source - there's no
/// config hook to flip), so the same ~16x wire-byte reduction the
/// overlay-bandwidth census measured is delivered here instead, one
/// layer up. `false` (the default for any client that doesn't ask,
/// including every consumer that predates this - installed desktops,
/// OBS shells, anything else on the public socket) sends the exact
/// same `Message::Text(json)` frames as before this change, byte for
/// byte - the hard requirement this whole feature was built around.
/// `true` sends the identical JSON, raw-deflate compressed, as a
/// binary frame instead (see `compressed_message`).
pub(crate) async fn handle_socket(socket: WebSocket, manager: Arc<AdventureManager>, compress: bool) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // Send the current roster/stage immediately so a freshly (re)connected
    // overlay doesn't sit blank until the next change or encounter.
    if let Ok(json) = serde_json::to_string(&StateEnvelope { kind: "state", snapshot: &manager.snapshot().await }) {
        let _ = out_tx.send(send_ready_message(json, compress));
    }

    // ...and the announcement backlog with it, so the dashboard's feed
    // card is populated the instant the socket opens rather than after
    // the next thing the game happens to say.
    if let Ok(json) = serde_json::to_string(&AnnouncementBacklogEnvelope { kind: "announcements", lines: &manager.recent_announcements() }) {
        let _ = out_tx.send(send_ready_message(json, compress));
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
                if out_tx.send(send_ready_message(json, compress)).is_err() {
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
                if out_tx.send(send_ready_message(json, compress)).is_err() {
                    break;
                }
            }
        })
    };

    // World 2 Stage 2 (2026-08-28) - the same `subscribe_announcements()`
    // the SSE endpoint uses, teed onto this socket. A lagged reader skips
    // what it missed rather than ending the loop, matching the SSE
    // endpoint's own handling of the identical channel.
    let mut announcement_task = {
        let out_tx = out_tx.clone();
        let mut rx = manager.subscribe_announcements();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(line) => {
                        let Ok(json) = serde_json::to_string(&AnnouncementEnvelope { kind: "announcement", line: &line }) else { continue };
                        if out_tx.send(send_ready_message(json, compress)).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
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
        _ = &mut announcement_task => {}
        _ = &mut recv_task => {}
    }
    sink_task.abort();
    state_task.abort();
    encounter_task.abort();
    announcement_task.abort();
    recv_task.abort();
}

#[cfg(test)]
mod compression_opt_in_tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn absent_or_wrong_wire_value_means_no_compression() {
        assert!(!WsParams { wire: String::new() }.wants_compression());
        assert!(!WsParams { wire: "1".to_string() }.wants_compression());
        assert!(!WsParams { wire: "gzip".to_string() }.wants_compression());
        assert!(!WsParams { wire: "DEFLATE".to_string() }.wants_compression(), "exact match only - no case-insensitivity, matching the desktop client's own literal string");
        assert!(WsParams { wire: "deflate".to_string() }.wants_compression());
    }

    #[test]
    fn compress_false_sends_the_exact_same_text_frame_as_before_this_feature() {
        let json = r#"{"type":"state","stage":42}"#.to_string();
        let msg = send_ready_message(json.clone(), false);
        assert_eq!(msg, Message::Text(json), "the plain path must be byte-for-byte unchanged for a client that never opts in");
    }

    #[test]
    fn compress_true_round_trips_to_the_exact_original_json() {
        let json = r#"{"type":"encounter","units":[{"id":"a","hp":100},{"id":"b","hp":200}],"events":[]}"#.to_string();
        let msg = send_ready_message(json.clone(), true);
        let Message::Binary(compressed) = msg else { panic!("compress: true must send a Binary frame, not Text") };
        assert!(compressed.len() < json.len(), "a real JSON payload should actually shrink, not just change format");
        assert_eq!(compressed[0], 0x78, "the first compressed frame must begin with the zlib header byte (0x78) - a regression back to raw deflate must fail THIS assertion, not just decompress silently wrong on the desktop client");
        let mut decoder = flate2::read::ZlibDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).expect("must decompress as valid zlib (RFC 1950), matching DecompressionStream('deflate') on both clients");
        assert_eq!(decompressed, json, "round-tripped JSON must be byte-for-byte identical to the original");
    }

    #[test]
    fn a_realistic_repeated_payload_compresses_well_past_the_census_target() {
        // Real fight payloads are highly repetitive (the same field names,
        // similar unit shapes, over and over) - a synthetic stand-in here,
        // not a claim this matches the real 16x figure exactly (that's
        // measured against a real encounter separately, see the deploy
        // report), just a sanity check that THIS mechanism is capable of
        // real-world-shaped compression ratios, not just round-tripping
        // trivially on tiny inputs.
        let unit = r#"{"id":"unit_id","hp":123456,"maxHp":200000,"atk":9999,"role":"melee"}"#;
        let json = format!(r#"{{"type":"encounter","units":[{}]}}"#, vec![unit; 200].join(","));
        let msg = send_ready_message(json.clone(), true);
        let Message::Binary(compressed) = msg else { panic!("expected Binary") };
        let ratio = json.len() as f64 / compressed.len() as f64;
        assert!(ratio > 15.0, "expected >15x on a realistically repetitive payload, got {ratio:.1}x ({} -> {} bytes)", json.len(), compressed.len());
    }
}
