//! Stage 3 API seam (2026-08-19, architecture refactor) - end-to-end
//! proof that the new `/api/*` surface (game/src/adventure_web/api.rs)
//! and its mirror bot-side client (src/adventure_client.rs) actually
//! work over real HTTP against a real, disposable `AdventureManager` -
//! not just that the two sides individually compile. Exercises a
//! representative slice of every §4a/§4c row plus one real, end-to-end
//! §4b round trip: a redemption call that triggers a genuine boss fight,
//! confirmed by reading the resulting announcement back off
//! `GET /api/announcements/stream`.
//!
//! Per the stage's own explicit scoping ("build the seam alongside the
//! existing in-process path... only the new one exercised by tests, no
//! cutover until Stage 4"), nothing here touches src/main.rs or
//! src/commands.rs - this is the ONLY thing that calls
//! `AdventureApiClient` today.
//!
//! **Single test function, deliberately** - see
//! tests/http_golden_responses.rs's own doc for why: `set_data_dir` is a
//! process-wide `OnceLock`, and every `tests/*.rs` file is its own
//! process, so the constraint is only within THIS file.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use twitch_bot_rs::adventure::AdventureManager;
use twitch_bot_rs::adventure_client::AdventureApiClient;

const TEST_SECRET: &str = "test-shared-secret";
const TEST_USER: &str = "api-seam-test-user";

#[tokio::test]
async fn api_seam_end_to_end_against_a_disposable_game_instance() {
    let scratch = std::env::temp_dir().join(format!("api_seam_harness_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    assert!(twitch_bot_rs::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    // Deliberately an EMPTY roster (no fixture copied in) - `AdventureManager::new`
    // defaults gracefully to an empty map when the characters file doesn't
    // exist yet, same as a real cold-start deploy, and an empty roster
    // keeps the one real fight this test triggers small and fast.
    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = twitch_bot_rs::adventure_web::start_adventure_web_server(
        0, // ephemeral
        "http://localhost".to_string(),
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        scratch.join("adventure-sessions.json"),
        Some(TEST_SECRET.to_string()),
    )
    .await
    .expect("disposable adventure_web server must start");

    let client = AdventureApiClient::new(format!("http://127.0.0.1:{}", bound_addr.port()), TEST_SECRET);

    // Fight-announcement batching (2026-08-19) defaults to a batch of 10 -
    // this test only ever triggers ONE fight, so without this the batch
    // would just sit pending and this test would have to wait out the
    // whole 5-minute time-flush to see any summary at all. Forcing a batch
    // size of 1 makes that one fight flush immediately, same as every
    // fight did before batching existed.
    let mut tunables = manager.live_tunables();
    tunables.fight_summary_batch_size = 1;
    manager.save_live_tunables(tunables).expect("failed to save test tunables");

    // Wrong/missing secret must be rejected - the whole point of gating
    // this router at all (§3: "Localhost-only or shared-secret header -
    // not public").
    let unauthorized = reqwest::Client::new().post(format!("http://127.0.0.1:{}/api/commands/party", bound_addr.port())).send().await.expect("request must complete");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED, "a request with no shared-secret header must be rejected");

    // §4a synchronous commands - each mirrors one row of REFACTOR_PLAN.md's
    // §4a table, same wording commands.rs's own handler produces.
    let join_reply = client.join(TEST_USER).await.expect("POST /api/commands/join failed").expect("join must always produce a reply");
    assert!(join_reply.contains("has joined the adventure"), "unexpected join reply: {join_reply}");
    assert!(manager.character(TEST_USER).await.is_some(), "join must actually create the character via the real AdventureManager, not just format a string");

    let character_reply = client.character(TEST_USER).await.expect("GET /api/commands/character failed").expect("character must always produce a reply");
    assert!(character_reply.contains("Level 1"), "fresh character should be level 1: {character_reply}");

    let party_reply = client.party().await.expect("GET /api/commands/party failed").expect("party must always produce a reply");
    assert!(party_reply.contains("stage"), "unexpected party reply: {party_reply}");

    // Non-mod !rampage vote (is_mod_or_broadcaster: false) - the one
    // endpoint whose BEHAVIOR (not just permission to call it) depends on
    // the caller's role, so the client passes that role explicitly.
    let vote_reply = client.rampage("some-other-viewer", false).await.expect("POST /api/commands/rampage failed").expect("a vote must always produce a reply");
    assert!(vote_reply.contains("Rampage vote: 1/3"), "first vote from a fresh manager should read 1/3: {vote_reply}");

    let dust_reply = client.gift_dust(TEST_USER, 50).await.expect("POST /api/commands/gift_dust failed").expect("gift_dust must always produce a reply");
    assert!(dust_reply.contains("Gave 50 dust"), "unexpected gift_dust reply: {dust_reply}");
    assert_eq!(manager.character(TEST_USER).await.unwrap().dust, 50, "gift_dust must actually grant the dust through the real AdventureManager");

    // §4c redemption endpoints. A fresh character (see `Character::new`)
    // starts fully equipped at full durability - nothing to repair yet,
    // but reforge has real starting gear to work with.
    let repair = client.redeem_repair(TEST_USER).await.expect("POST /api/redemptions/repair failed");
    assert!(!repair.fulfilled, "a fresh character's starting gear is at full durability, so this redemption must refund");

    let reforge = client.redeem_reforge(TEST_USER).await.expect("POST /api/redemptions/reforge failed");
    assert!(reforge.fulfilled, "a fresh character's starting gear means reforge must succeed, not refund");
    assert!(
        reforge.chat_message.as_deref().unwrap_or_default().contains("reforged their"),
        "unexpected reforge success message: {:?}",
        reforge.chat_message
    );

    // §4b end-to-end: subscribe to the announcements stream BEFORE
    // triggering anything (a broadcast channel with zero subscribers
    // drops silently - the deliberate "drop gracefully" policy - so
    // subscribing after the fact would just never see this message).
    let mut announcements = client.announcements().await.expect("GET /api/announcements/stream failed");

    // Force Boss Fight redemption - the one redemption that actually runs
    // a real fight inline. TEST_USER (freshly joined, not retreated) is
    // the only eligible fighter, so this must trigger rather than refund.
    let force_boss = client.redeem_force_boss(TEST_USER, true).await.expect("POST /api/redemptions/force_boss failed");
    assert!(force_boss.fulfilled, "the only joined character is eligible to fight, so Force Boss Fight must trigger, not refund");
    assert!(
        force_boss.chat_message.as_deref().unwrap_or_default().contains("spent channel points"),
        "unexpected force-boss announce message: {:?}",
        force_boss.chat_message
    );

    // The fight that redemption just triggered runs `announce_encounter_result`
    // server-side (see manager.rs), which pushes the fight's batched-summary
    // line (flushed immediately since `fight_summary_batch_size` was forced
    // to 1 above; plus, since this is the manager's very first-ever fight,
    // every one-time launch-giveaway announcement too - see that fn's own
    // doc on ordering) onto the SAME `announcements_tx` this SSE stream
    // reads from - proving the full round trip (HTTP redemption call ->
    // real state mutation -> SSE announcement) actually works, not just
    // that each half compiles. Collect a few messages rather than asserting
    // on the very first one, since which one-time giveaways fire first
    // isn't this test's concern. Timeout is generous (not just "5s felt
    // safe") because the announcement is genuinely delayed server-side, on
    // purpose - see the Stage 4 fix in
    // `run_encounter_inner`/`run_basic_encounter_inner`: `700ms +
    // display_duration_ms`, and `display_duration_ms` alone has a hard
    // floor of `combat::MIN_DISPLAY_MS` (6s) - so the very first message
    // can legitimately take ~6.7s to arrive even for this small a fight.
    let mut seen = Vec::new();
    let found_outcome = loop {
        let Ok(Some(msg)) = tokio::time::timeout(Duration::from_secs(10), announcements.next()).await else {
            break false;
        };
        let is_outcome = msg.starts_with("Last 1 fight:");
        seen.push(msg);
        if is_outcome {
            break true;
        }
    };
    assert!(found_outcome, "never saw the batched fight-summary line among the announcements received: {seen:?}");

    // Fire-and-forget activity XP (§4c) - just needs to succeed at the
    // HTTP layer; no reply body to check by design (see api.rs's own doc).
    client.activity_xp(TEST_USER).await.expect("POST /api/activity_xp failed");

    // Smoke matrix item 3 (owner-requested at Stage 4): "bot against a
    // killed game." Can't cleanly shut down the disposable server above
    // (`start_adventure_web_server` returns just a `SocketAddr`, no
    // shutdown handle) - instead, bind a fresh ephemeral listener and
    // immediately drop it without ever serving on it, guaranteeing
    // "connection refused" for anything that tries to talk to that port,
    // the exact condition a real killed `game` process leaves behind.
    // Proves `AdventureApiClient` fails CLEANLY (a real `Err`, not a
    // panic or a hang) - the actual new risk this cutover introduced,
    // since every command/redemption handler now has a real network call
    // on its formerly-infallible path. `adventure_reply`'s own unit tests
    // (src/commands.rs) cover the other half: that `Err` maps to the
    // ratified §4c fallback text once it gets back to a command handler.
    let dead_port = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("failed to bind a throwaway listener");
        listener.local_addr().expect("failed to read the throwaway listener's port").port()
        // `listener` drops here, closing the socket - the port is now
        // refusing connections, not just unresponsive.
    };
    let dead_client = AdventureApiClient::new(format!("http://127.0.0.1:{dead_port}"), TEST_SECRET);
    let join_result = tokio::time::timeout(Duration::from_secs(5), dead_client.join(TEST_USER))
        .await
        .expect("a connection-refused call must fail fast, not hang for the full timeout");
    assert!(join_result.is_err(), "a client pointed at a dead port must return Err, not Ok - got {join_result:?}");

    std::fs::remove_dir_all(&scratch).ok();
}
