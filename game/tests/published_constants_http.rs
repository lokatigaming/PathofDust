//! POST /api/published-constants over real HTTP (2026-08-22, bot/game
//! build-time decoupling) - proves the replacement for the bot's old
//! direct `bot-published-constants.json` write end to end: a wrong
//! secret is rejected like any other /api route, the right secret lands
//! 204, and the file is written exactly where wiki.rs reads it, in the
//! same pretty-printed shape the old write produced.
//!
//! Own test binary, deliberately: PUBLISHED_CONSTANTS_PATH is a bare
//! CWD-relative filename by design (NOT routed through paths::data_path),
//! so this test re-anchors CWD into its scratch dir FIRST and asserts
//! against the file there. That anchor is process-wide state - same
//! reasoning as set_data_dir in every other harness here: one
//! #[tokio::test] per file, no second test fn in THIS file. (Anchoring to
//! the scratch dir rather than the workspace root also guarantees the
//! endpoint can never touch the real repo-root copy of the file.)

use std::path::PathBuf;

use game::adventure::{AdventureManager, PublishedConstants, PUBLISHED_CONSTANTS_PATH};

const TEST_SECRET: &str = "test-shared-secret";

#[tokio::test]
async fn posted_published_constants_are_persisted_where_the_wiki_reads_them() {
    let scratch = std::env::temp_dir().join(format!("published_constants_http_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    // Must run before any server work - see this file's doc.
    std::env::set_current_dir(&scratch).expect("failed to anchor CWD at the scratch dir");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));
    let bound_addr = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        scratch.join("adventure-sessions.json"),
        Some(TEST_SECRET.to_string()),
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::new();

    // --- missing/wrong secret must be rejected like any other /api route ---
    let unauthorized = client.post(format!("{base}/api/published-constants")).json(&serde_json::json!({ "builtin_cooldown_secs": 2 })).send().await.expect("request must complete");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED, "a request with no shared-secret header must be rejected");
    assert!(!std::path::Path::new(PUBLISHED_CONSTANTS_PATH).exists(), "a rejected publish must not have written anything");

    // --- the happy path: post, then read the file back from disk -------
    // Distinctive values, so a field mix-up shows up as a wrong number
    // rather than a false pass.
    let expected = PublishedConstants {
        builtin_cooldown_secs: 2,
        bug_report_cooldown_secs: 321,
        song_skip_cooldown_secs: 45,
        min_vote_volume: 10,
        max_vote_volume: 90,
    };
    let ok = client
        .post(format!("{base}/api/published-constants"))
        .header("x-adventure-api-secret", TEST_SECRET)
        .json(&expected)
        .send()
        .await
        .expect("POST /api/published-constants failed");
    assert_eq!(ok.status(), reqwest::StatusCode::NO_CONTENT, "a successful publish returns 204");

    let raw = std::fs::read_to_string(PUBLISHED_CONSTANTS_PATH).expect("the endpoint must have written the file where wiki.rs reads it");
    let parsed: PublishedConstants = serde_json::from_str(&raw).expect("the written file must parse back as PublishedConstants");
    assert_eq!(parsed.builtin_cooldown_secs, expected.builtin_cooldown_secs);
    assert_eq!(parsed.bug_report_cooldown_secs, expected.bug_report_cooldown_secs);
    assert_eq!(parsed.song_skip_cooldown_secs, expected.song_skip_cooldown_secs);
    assert_eq!(parsed.min_vote_volume, expected.min_vote_volume);
    assert_eq!(parsed.max_vote_volume, expected.max_vote_volume);

    // Format guard: state::save_json pretty-prints, and that on-disk shape
    // is part of the old-bot/new-game compatibility contract - lock it so
    // a future switch to compact output surfaces HERE first.
    assert!(raw.contains('\n'), "on-disk format must stay pretty-printed (state::save_json's shape), got: {raw}");

    // --- a type-garbage payload must be rejected, not persisted --------
    let bad = client
        .post(format!("{base}/api/published-constants"))
        .header("x-adventure-api-secret", TEST_SECRET)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(r#"{ "builtin_cooldown_secs": "not a number" }"#)
        .send()
        .await
        .expect("POST failed");
    assert!(bad.status().is_client_error(), "an unparseable payload must not be accepted, got {}", bad.status());
    let after_bad = std::fs::read_to_string(PUBLISHED_CONSTANTS_PATH).expect("the previous good file must still exist");
    assert_eq!(after_bad, raw, "a rejected payload must not have touched the previously published content");

    std::fs::remove_dir_all(&scratch).ok();
}