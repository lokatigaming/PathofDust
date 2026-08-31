//! Announcement feed (World 2 Stage 2, 2026-08-28) - the web home for
//! the narration that until now existed only in Twitch chat.
//!
//! **The compatibility guarantee is the most important thing in this
//! file.** This branch ADDS a sink; it reroutes nothing. Every producer
//! now goes through `AdventureManager::announce`, which appends to the
//! in-memory ring and THEN sends on `announcements_tx` exactly as the
//! direct `.send()` calls it replaced did - so
//! `GET /api/announcements/stream`, and therefore the bot and Twitch
//! chat, must still receive the same lines, in the same order, with the
//! same bytes. That is asserted here against a real fight on a real
//! disposable instance, not reasoned about.
//!
//! Harness (disposable instance, OS-assigned ephemeral port, scratch
//! data dir) copied from `api_seam.rs`, which already drives a real
//! Force Boss redemption and reads the resulting announcement back off
//! the SSE stream; the `/ws` half copies `overlay_compression.rs`.
//! Nothing here can reach the live game.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is
//! a process-wide `OnceLock`, the same constraint every other
//! `tests/*.rs` file here documents.
//!
//! The retention cap is NOT tested here: bounding the ring takes ~60
//! emissions and there is no cheap way to produce 60 real announcements
//! over HTTP (each one needs a real fight). It is covered directly
//! against `announce` in manager.rs's own `announcement_feed_ring_tests`,
//! which is the mechanism itself.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use game::adventure::AdventureManager;
use tokio_tungstenite::tungstenite::Message;

const TEST_SECRET: &str = "test-shared-secret";
const TEST_USER: &str = "feed-test-user";
const SESSION_TOKEN: &str = "feed-test-session-token";

/// Mirrors api_seam.rs's constant, spelled out for the same reason it is
/// spelled out there - api.rs's own is module-private.
const API_SECRET_HEADER: &str = "x-adventure-api-secret";

/// Minimal single-frame SSE reader - same wire assumption as the bot's
/// own hand-rolled parser (`data: <text>`, frames separated by a blank
/// line). Lifted from api_seam.rs.
struct AnnouncementsReader {
    resp: reqwest::Response,
    pending: String,
}

impl AnnouncementsReader {
    async fn next_message(&mut self) -> Option<String> {
        loop {
            while let Some(frame_end) = self.pending.find("\n\n") {
                let frame = self.pending[..frame_end].to_string();
                self.pending.drain(..frame_end + 2);
                if let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) {
                    return Some(data.to_string());
                }
            }
            let chunk = match self.resp.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) | Err(_) => return None,
            };
            self.pending.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    /// Everything the stream delivers until it goes quiet for `idle`. The
    /// producer count for one fight is not fixed (one-time launch
    /// giveaways fire on a manager's very first fight), so this drains
    /// rather than expecting an exact number.
    ///
    /// `first` is a longer, separate budget for the FIRST line: a
    /// fight's announcement is genuinely delayed server-side on purpose
    /// (`700ms + display_duration_ms`, and `display_duration_ms` has a
    /// hard 6s floor in `combat::MIN_DISPLAY_MS`), so the first message
    /// can legitimately take ~6.7s even for a tiny fight - see
    /// api_seam.rs, which documents the same wait.
    async fn drain(&mut self, first: Duration, idle: Duration) -> Vec<String> {
        let mut out = Vec::new();
        let mut budget = first;
        while let Ok(Some(msg)) = tokio::time::timeout(budget, self.next_message()).await {
            out.push(msg);
            budget = idle;
        }
        out
    }
}

/// The `type` of a `/ws` frame, or an empty string for a frame that has
/// none - keeps the match arms below readable.
fn frame_type(value: &serde_json::Value) -> &str {
    value.get("type").and_then(|v| v.as_str()).unwrap_or_default()
}

#[tokio::test]
async fn announcements_reach_the_web_feed_without_changing_what_the_sse_stream_receives() {
    // Integration tests run with their PACKAGE dir as CWD (game/), but the
    // template loader resolves "templates/" against the workspace root.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("announcement_feed_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    let characters_path = scratch.join("adventure-characters.json");
    std::fs::write(&characters_path, "{}").expect("failed to seed an empty characters file");

    // A real session, so the dashboard renders as an authenticated player
    // rather than the logged-out page.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(r#"{{"{SESSION_TOKEN}":{{"login":"{TEST_USER}","display_name":"FeedTestUser","created_at":{now}}}}}"#),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(game::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - only caller in this process");

    let manager = AdventureManager::new(characters_path, PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    manager.join(TEST_USER, "FeedTestUser").await;

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        Some("test-client-id".to_string()),
        Some("test-client-secret".to_string()),
        manager.clone(),
        sessions_path,
        Some(TEST_SECRET.to_string()),
    )
    .await
    .expect("disposable adventure_web server must start");
    let port = bound.port();
    let base = format!("http://127.0.0.1:{port}");
    let cookie = format!("adv_session={SESSION_TOKEN}");
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // Same reason api_seam.rs does this: batching defaults to 10, and this
    // test triggers exactly one fight, so without a batch size of 1 the
    // summary would sit pending behind a 5-minute time flush.
    let mut tunables = manager.live_tunables();
    tunables.fight_summary_batch_size = 1;
    manager.save_live_tunables(tunables).expect("failed to save test tunables");

    assert!(manager.recent_announcements().is_empty(), "a fresh manager's feed ring must start empty");

    // --- the feed card exists, and is honest, while empty --------------
    let empty_dashboard =
        client.get(format!("{base}/")).header(reqwest::header::COOKIE, &cookie).send().await.expect("GET / failed").text().await.expect("body");
    assert!(empty_dashboard.contains("id=\"announcement-feed\""), "the dashboard must render the feed card even with nothing in it");
    assert!(empty_dashboard.contains("class=\"announcement-empty muted\""), "an empty feed must say so rather than render a blank list");

    // --- observers, all attached BEFORE anything is announced ----------
    // A broadcast channel with zero subscribers drops silently (the
    // deliberate "drop gracefully" policy), so subscribing after the fact
    // would prove nothing.
    let sse_resp = client
        .get(format!("{base}/api/announcements/stream"))
        .header(API_SECRET_HEADER, TEST_SECRET)
        .send()
        .await
        .expect("GET /api/announcements/stream failed")
        .error_for_status()
        .expect("the SSE stream must still be served");
    let mut sse = AnnouncementsReader { resp: sse_resp, pending: String::new() };

    let (mut live_ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.expect("live ws client failed to connect");

    // A client connecting to an EMPTY ring still gets a backlog frame -
    // one mechanism, no special case for "nothing yet".
    let mut saw_empty_backlog = false;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(1000), live_ws.next()).await {
        let Message::Text(text) = msg else { continue };
        let value: serde_json::Value = serde_json::from_str(&text).expect("every /ws frame must be JSON");
        if frame_type(&value) == "announcements" {
            assert_eq!(
                value["lines"].as_array().expect("a backlog frame must carry a lines array").len(),
                0,
                "an empty ring must send an EMPTY backlog, not omit the frame"
            );
            saw_empty_backlog = true;
            break;
        }
    }
    assert!(saw_empty_backlog, "connecting to /ws must always send the announcement backlog");

    // --- trigger real announcements ------------------------------------
    // Force Boss Fight is the one redemption that runs a real fight
    // inline, and a fight resolving is the densest real producer there is
    // (batch summary and loot, plus this manager's first-ever fight's
    // one-time launch giveaways).
    let redemption = client
        .post(format!("{base}/api/redemptions/force_boss"))
        .header(API_SECRET_HEADER, TEST_SECRET)
        .json(&serde_json::json!({ "user_name": TEST_USER, "announce": true }))
        .send()
        .await
        .expect("POST /api/redemptions/force_boss failed");
    assert_eq!(redemption.status(), reqwest::StatusCode::OK, "the only joined character is eligible, so the fight must run");

    // --- THE COMPATIBILITY GUARANTEE -----------------------------------
    let over_sse = sse.drain(Duration::from_secs(15), Duration::from_secs(2)).await;
    assert!(!over_sse.is_empty(), "a resolved fight must still put lines on /api/announcements/stream - this branch must not change what chat receives");

    let ring = manager.recent_announcements();
    assert_eq!(ring, over_sse, "the tee must hold EVERY line the SSE stream received, in the same order, byte-for-byte - a mismatch here means the feed and chat have diverged");

    // --- the live /ws tee ----------------------------------------------
    let mut over_ws: Vec<String> = Vec::new();
    while over_ws.len() < over_sse.len() {
        let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(2000), live_ws.next()).await else { break };
        let Message::Text(text) = msg else { continue };
        let value: serde_json::Value = serde_json::from_str(&text).expect("every /ws frame must be JSON");
        if frame_type(&value) == "announcement" {
            over_ws.push(value["line"].as_str().expect("an announcement frame must carry a string line").to_string());
        }
    }
    assert_eq!(over_ws, over_sse, "an already-connected /ws client must receive the same lines the SSE stream did");

    // --- backlog on connect, for a client that arrived late -------------
    let (mut late_ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.expect("late ws client failed to connect");
    let mut backlog: Option<Vec<String>> = None;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(2000), late_ws.next()).await {
        let Message::Text(text) = msg else { continue };
        let value: serde_json::Value = serde_json::from_str(&text).expect("every /ws frame must be JSON");
        if frame_type(&value) == "announcements" {
            backlog = Some(value["lines"].as_array().expect("lines array").iter().map(|v| v.as_str().expect("a backlog line must be a string").to_string()).collect());
            break;
        }
    }
    assert_eq!(backlog.as_deref(), Some(&ring[..]), "a client connecting mid-session must receive the whole ring as its backlog - the one mechanism that covers first paint AND a reconnect");

    // --- the feed renders for an authenticated player -------------------
    // Server-rendered, before any script runs: the card is correct for a
    // viewer whose socket never connects at all.
    let dashboard = client.get(format!("{base}/")).header(reqwest::header::COOKIE, &cookie).send().await.expect("GET / failed").text().await.expect("body");
    assert!(dashboard.contains("id=\"announcement-feed\""), "the dashboard must render the feed card");
    // The class, not the sentence: base.html's own script carries the same
    // wording for the client-side empty state, so matching on the text
    // would pass no matter what the server rendered.
    assert!(!dashboard.contains("class=\"announcement-empty muted\""), "a populated ring must replace the empty-state line");
    for line in &ring {
        let escaped = line.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        assert!(dashboard.contains(&escaped), "the server-rendered feed must contain every ring line without any WebSocket involved; missing: {line}");
    }

    let newest = ring.last().expect("the ring is non-empty here");
    let oldest = ring.first().expect("the ring is non-empty here");
    if newest != oldest {
        let newest_at = dashboard.find(newest.as_str()).expect("newest line must be on the page");
        let oldest_at = dashboard.find(oldest.as_str()).expect("oldest line must be on the page");
        assert!(newest_at < oldest_at, "the card renders newest-first, which is also the end the /ws client prepends to");
    }
}
