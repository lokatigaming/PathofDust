//! End-to-end proof that `FightSummarySnapshot::bundle_seq`, once served
//! over `/fights.json`, is exactly the key `/fights/:seq/members/:member`
//! resolves - the whole point of stamping it (see manager.rs's
//! `save_last_fight` and the `bundle_seq` field doc).
//!
//! Same disposable-instance setup as `replay_bundle_tiers_http.rs`: an
//! OS-assigned ephemeral port, a scratch data directory, a seeded fake
//! session - nothing here can reach the live game's files or ports.
//!
//! Own file (not added to `replay_bundle_tiers_http.rs`) for the same
//! reason that file gives: `adventure::set_data_dir` is a process-wide
//! `OnceLock`, and Rust gives each `tests/*.rs` file its own process, so
//! this stays isolated from every other integration test.

use std::path::PathBuf;
use game::adventure::AdventureManager;

const STREAMER_LOGIN: &str = "lokati_gaming";

/// A minimal bundle with just the public `core` member, distinguishable
/// from any other fixture by its `stage`.
fn bundle_json() -> String {
    r#"{"members":{"core":{"stage":4242,"participants":["Kazesosa"]}}}"#.to_string()
}

/// The summary tier's on-disk shape, `bundleSeq` pointing at the bundle
/// seeded above - exactly what `save_last_fight` would have written had
/// the bundle write for this fight succeeded.
fn summary_json(bundle_seq: u64) -> String {
    format!(
        r#"{{"kind":"boss","stage":4242,"won":true,"startedAtUnixMs":1755690000000,"displayDurationMs":6000,"realDurationMs":2800,"bundleSeq":{bundle_seq},"participants":1,"players":[],"firstToDie":null,"loot":[],"broken":[]}}"#
    )
}

#[tokio::test]
async fn a_summary_bundle_seq_served_over_http_resolves_to_its_own_bundle() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("summary_bundle_seq_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(r#"{{"streamer-token":{{"login":"{STREAMER_LOGIN}","display_name":"Lokati","created_at":{now}}}}}"#),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(
        game::adventure::set_data_dir(scratch.clone()),
        "set_data_dir must succeed - only caller in this process"
    );

    let bundle_dir = scratch.join("adventure-fights-bundle");
    std::fs::create_dir_all(&bundle_dir).expect("failed to create the bundle tier dir");
    std::fs::write(bundle_dir.join("fight-0000000007.json"), bundle_json()).expect("failed to seed a bundle");

    let summary_dir = scratch.join("adventure-fights-summary");
    std::fs::create_dir_all(&summary_dir).expect("failed to create the summary tier dir");
    std::fs::write(summary_dir.join("fight-0000000001.json"), summary_json(7)).expect("failed to seed a summary");

    let manager = AdventureManager::new(
        PathBuf::from("adventure-characters.json"),
        PathBuf::from("adventure-world.json"),
        PathBuf::from("adventure-reforge-cooldown.json"),
    );

    let bound = game::adventure_web::start_adventure_web_server(
        0,
        "http://localhost".to_string(),
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        sessions_path,
        None,
    )
    .await
    .expect("disposable adventure_web server must start");

    let base = format!("http://127.0.0.1:{}", bound.port());
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("failed to build reqwest client");

    let fights: serde_json::Value = client
        .get(format!("{base}/fights.json"))
        .header(reqwest::header::COOKIE, "adv_session=streamer-token")
        .send()
        .await
        .expect("GET /fights.json failed")
        .json()
        .await
        .expect("/fights.json must return valid JSON");

    let served = fights.as_array().expect("a JSON array of summaries");
    assert_eq!(served.len(), 1, "exactly the one seeded summary must be served: {fights}");
    let bundle_seq = served[0]["bundleSeq"].as_u64().expect(&format!("the served summary must carry a numeric bundleSeq: {fights}"));
    assert_eq!(bundle_seq, 7, "the served bundleSeq must be exactly the key seeded onto the summary, not any other tier's counter");

    let member_body = client
        .get(format!("{base}/fights/{bundle_seq}/members/core"))
        .send()
        .await
        .expect("GET member route failed")
        .text()
        .await
        .expect("body");
    assert!(
        member_body.contains("\"stage\":4242"),
        "the bundleSeq served on the summary must resolve, through the real route, to THIS fight's own bundle - got {member_body}",
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
