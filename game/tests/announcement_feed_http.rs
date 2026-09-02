//! Announcement feed (World 2 Stage 2, 2026-08-28) - the web home for
//! the narration that used to exist only in Twitch chat.
//!
//! **The tee guarantee is the most important thing in this file.** Every
//! producer goes through `AdventureManager::announce`, which appends to
//! the in-memory ring and THEN sends on `announcements_tx` - so the
//! server-rendered feed card, an already-connected `/ws` client and a
//! late client's backlog must all carry the same lines, in the same
//! order, with the same bytes. That is asserted here against a real
//! fight on a real disposable instance, not reasoned about.
//!
//! **Rewritten 2026-09-02 (Twitch removal).** This file used to attach
//! `GET /api/announcements/stream` as a second observer and trigger the
//! fight through `POST /api/redemptions/force_boss`. Both went with the
//! `/api` seam. Neither was the subject: the trigger is now
//! `try_force_encounter` (exactly what that endpoint called) and `/ws`
//! is the live observer. Nothing about the producers or the ring moved.
//!
//! Harness (disposable instance, OS-assigned ephemeral port, scratch
//! data dir) as every other `tests/*_http.rs` file here; the `/ws` half
//! copies `overlay_compression.rs`. Nothing here can reach the live game.
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

const TEST_USER: &str = "feed-test-user";
const SESSION_TOKEN: &str = "feed-test-session-token";

/// The `type` of a `/ws` frame, or an empty string for a frame that has
/// none - keeps the match arms below readable.
fn frame_type(value: &serde_json::Value) -> &str {
    value.get("type").and_then(|v| v.as_str()).unwrap_or_default()
}

#[tokio::test]
async fn announcements_reach_the_web_feed_over_ws_and_server_rendered_html() {
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
        manager.clone(),
        sessions_path,
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

    // --- observer, attached BEFORE anything is announced ---------------
    // A broadcast channel with zero subscribers drops silently (the
    // deliberate "drop gracefully" policy), so subscribing after the fact
    // would prove nothing. `/ws` is the only live observer left: the SSE
    // stream this test also used to attach here went with the `/api`
    // seam (2026-09-02).
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
    // A resolved fight is the densest real producer there is (batch
    // summary and loot, plus this manager's first-ever fight's one-time
    // launch giveaways). Driven straight through `try_force_encounter`,
    // which is exactly what the deleted Force Boss redemption endpoint
    // called - the trigger moved, the producers did not.
    assert!(
        matches!(manager.try_force_encounter().await, game::adventure::ForceBossOutcome::Triggered),
        "the only joined character is eligible, so the fight must run"
    );

    // --- THE TEE GUARANTEE ---------------------------------------------
    // Drain the live socket until it goes quiet, then hold the ring to it.
    //
    // Two budgets, carried over from the SSE reader this replaced. The
    // FIRST announcement is genuinely delayed server-side on purpose
    // (`700ms + display_duration_ms`, and `display_duration_ms` has a hard
    // 6s floor in `combat::MIN_DISPLAY_MS`), so it can legitimately take
    // ~6.7s even for a tiny fight; everything after it arrives promptly.
    // A single flat timeout would either flake on the first line or spend
    // that budget again on every subsequent one.
    const FIRST_LINE_BUDGET: Duration = Duration::from_secs(15);
    const IDLE_BUDGET: Duration = Duration::from_secs(2);
    let mut over_ws: Vec<String> = Vec::new();
    let mut budget = FIRST_LINE_BUDGET;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(budget, live_ws.next()).await {
        let Message::Text(text) = msg else { continue };
        let value: serde_json::Value = serde_json::from_str(&text).expect("every /ws frame must be JSON");
        if frame_type(&value) == "announcement" {
            over_ws.push(value["line"].as_str().expect("an announcement frame must carry a string line").to_string());
            budget = IDLE_BUDGET;
        }
    }
    assert!(!over_ws.is_empty(), "a resolved fight must put lines on an already-connected /ws client");

    let ring = manager.recent_announcements();
    assert_eq!(
        ring, over_ws,
        "the tee must hold EVERY line the live socket received, in the same order, byte-for-byte - a mismatch means the ring and the socket have diverged"
    );

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
