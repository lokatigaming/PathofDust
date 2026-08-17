// obs-websocket v5 client — just enough to set a source's volume
// directly in OBS. This exists because of a real interaction between the
// song overlay's audio and the Compressor/Limiter filters added to it in
// OBS for loudness leveling: those filters apply a *fixed* makeup gain
// regardless of how much they compressed, so changing the YouTube
// player's own internal volume (the old !votevolume/!modvolume
// mechanism) just moves the signal below the compressor's threshold
// where it gets makeup-gained right back up — largely canceling the
// change out. OBS's own per-source volume fader applies *after* all
// filters, so controlling that instead actually works regardless of what
// the compressor does upstream.
//
// Protocol verified directly against obs-websocket's own docs (not
// guessed): connect -> server sends Hello (op 0, with an authentication
// challenge/salt if a password is set) -> client sends Identify (op 1,
// with a computed auth string) -> server sends Identified (op 2) -> from
// then on, Request (op 6) / RequestResponse (op 7) pairs correlated by a
// requestId the client makes up.

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

const OP_HELLO: u8 = 0;
const OP_IDENTIFY: u8 = 1;
const OP_IDENTIFIED: u8 = 2;
const OP_REQUEST: u8 = 6;
const OP_REQUEST_RESPONSE: u8 = 7;

/// The RPC version this client speaks — negotiated with the server during
/// Identify, matches what obs-websocket v5 currently expects.
const RPC_VERSION: u32 = 1;

struct PendingRequest {
    reply: oneshot::Sender<Result<serde_json::Value, String>>,
}

enum ObsCommand {
    SendRequest { request_type: String, request_data: serde_json::Value, reply: oneshot::Sender<Result<serde_json::Value, String>> },
}

pub struct ObsClient {
    tx: mpsc::UnboundedSender<ObsCommand>,
}

impl ObsClient {
    /// Spawns a background task that connects (and reconnects on any
    /// disconnect) to OBS's WebSocket server, and returns a handle for
    /// sending requests to it. Connection failures are logged and
    /// retried — a request made while disconnected just gets a "not
    /// connected" error back rather than the caller needing to know
    /// anything about connection state.
    pub fn new(url: String, password: Option<String>) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(url, password, rx));
        Arc::new(Self { tx })
    }

    /// Sets a source/input's volume directly in OBS (applies after all
    /// filters, including any Compressor/Limiter) — `volume_percent` is
    /// 0-100, converted to OBS's 0.0-1.0 linear multiplier.
    pub async fn set_input_volume(&self, input_name: &str, volume_percent: u8) -> Result<(), String> {
        let volume_mul = f64::from(volume_percent) / 100.0;
        let (reply, reply_rx) = oneshot::channel();
        self.tx
            .send(ObsCommand::SendRequest {
                request_type: "SetInputVolume".to_string(),
                request_data: json!({ "inputName": input_name, "inputVolumeMul": volume_mul }),
                reply,
            })
            .map_err(|_| "OBS WebSocket task isn't running".to_string())?;

        reply_rx.await.map_err(|_| "OBS WebSocket connection dropped before responding".to_string())?.map(|_| ())
    }
}

async fn run(url: String, password: Option<String>, mut rx: mpsc::UnboundedReceiver<ObsCommand>) {
    loop {
        match run_connection(&url, &password, &mut rx).await {
            Ok(()) => tracing::warn!("OBS WebSocket connection ended cleanly, reconnecting..."),
            Err(err) => tracing::error!("OBS WebSocket connection error: {err}, reconnecting in 5s..."),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[derive(Deserialize)]
struct Envelope {
    op: u8,
    d: serde_json::Value,
}

fn compute_auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let mut secret_hasher = Sha256::new();
    secret_hasher.update(password.as_bytes());
    secret_hasher.update(salt.as_bytes());
    let base64_secret = base64::engine::general_purpose::STANDARD.encode(secret_hasher.finalize());

    let mut auth_hasher = Sha256::new();
    auth_hasher.update(base64_secret.as_bytes());
    auth_hasher.update(challenge.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(auth_hasher.finalize())
}

async fn run_connection(url: &str, password: &Option<String>, rx: &mut mpsc::UnboundedReceiver<ObsCommand>) -> anyhow::Result<()> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    // ---- Hello -> Identify -> Identified handshake ----
    let hello: Envelope = match read.next().await {
        Some(Ok(Message::Text(text))) => serde_json::from_str(&text)?,
        Some(Ok(_)) => anyhow::bail!("Expected a text Hello message first"),
        Some(Err(err)) => return Err(err.into()),
        None => anyhow::bail!("Connection closed before Hello"),
    };
    if hello.op != OP_HELLO {
        anyhow::bail!("Expected Hello (op {OP_HELLO}), got op {}", hello.op);
    }

    let authentication = match hello.d.get("authentication") {
        Some(auth) => {
            let password = password
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("OBS WebSocket server requires a password but OBS_WEBSOCKET_PASSWORD isn't set"))?;
            let salt = auth.get("salt").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Hello missing salt"))?;
            let challenge =
                auth.get("challenge").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Hello missing challenge"))?;
            Some(compute_auth_string(password, salt, challenge))
        }
        None => None,
    };

    let identify = json!({
        "op": OP_IDENTIFY,
        "d": {
            "rpcVersion": RPC_VERSION,
            "authentication": authentication,
            "eventSubscriptions": 0,
        },
    });
    write.send(Message::Text(identify.to_string())).await?;

    let identified: Envelope = match read.next().await {
        Some(Ok(Message::Text(text))) => serde_json::from_str(&text)?,
        Some(Ok(_)) => anyhow::bail!("Expected a text Identified message"),
        Some(Err(err)) => return Err(err.into()),
        None => anyhow::bail!("Connection closed before Identified"),
    };
    if identified.op != OP_IDENTIFIED {
        anyhow::bail!("Identify failed — expected Identified (op {OP_IDENTIFIED}), got op {}: {:?}", identified.op, identified.d);
    }
    tracing::info!("OBS WebSocket: connected and identified.");

    // ---- Steady state: relay outgoing Requests, dispatch incoming RequestResponses ----
    let pending: std::sync::Mutex<HashMap<String, PendingRequest>> = std::sync::Mutex::new(HashMap::new());
    let next_id = AtomicU64::new(0);

    loop {
        tokio::select! {
            msg = read.next() => {
                let msg = match msg {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(_)) => continue,
                    Some(Err(err)) => return Err(err.into()),
                    None => anyhow::bail!("OBS WebSocket connection closed"),
                };
                let envelope: Envelope = match serde_json::from_str(&msg) {
                    Ok(e) => e,
                    Err(err) => { tracing::warn!("OBS WebSocket: failed to parse message: {err}"); continue; }
                };
                if envelope.op != OP_REQUEST_RESPONSE {
                    continue;
                }
                let Some(request_id) = envelope.d.get("requestId").and_then(|v| v.as_str()) else { continue };
                let Some(pending_request) = pending.lock().unwrap().remove(request_id) else { continue };

                let success = envelope.d.get("requestStatus").and_then(|s| s.get("result")).and_then(|v| v.as_bool()).unwrap_or(false);
                let result = if success {
                    Ok(envelope.d.get("responseData").cloned().unwrap_or(serde_json::Value::Null))
                } else {
                    let comment = envelope.d.get("requestStatus").and_then(|s| s.get("comment")).and_then(|v| v.as_str()).unwrap_or("request failed");
                    Err(comment.to_string())
                };
                let _ = pending_request.reply.send(result);
            }
            cmd = rx.recv() => {
                let Some(ObsCommand::SendRequest { request_type, request_data, reply }) = cmd else {
                    anyhow::bail!("OBS WebSocket command channel closed");
                };
                let request_id = next_id.fetch_add(1, Ordering::Relaxed).to_string();
                pending.lock().unwrap().insert(request_id.clone(), PendingRequest { reply });

                let request = json!({
                    "op": OP_REQUEST,
                    "d": {
                        "requestType": request_type,
                        "requestId": request_id,
                        "requestData": request_data,
                    },
                });
                if let Err(err) = write.send(Message::Text(request.to_string())).await {
                    // The write failed — the pending entry above will just
                    // never get a reply and the caller's oneshot::Receiver
                    // will see a RecvError once this whole connection
                    // task exits and drops `pending`. Bail out now so the
                    // outer loop reconnects instead of silently limping
                    // along on a broken socket.
                    return Err(err.into());
                }
            }
        }
    }
}
