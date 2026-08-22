//! Replay-bundle tier enforcement over real HTTP.
//!
//! The manifest DECLARES a tier per member; this proves the server
//! ENFORCES it. Those are different claims, and only the second one keeps
//! `shield`/`buffSnapshot` off an unauthenticated feed - which is the whole
//! reason they were removed from the public broadcast on 2026-08-20.
//!
//! Asserted against the real session layer over a real socket, because the
//! failure being guarded against is precisely the kind that reads fine in
//! the handler: a caller resolving to a wider tier than they should, or a
//! member reachable by a path nobody remembered to gate.
//!
//! Same disposable-instance setup as `admin_passives_http.rs`: an
//! OS-assigned ephemeral port, a scratch data directory, seeded fake
//! sessions, so nothing here can reach the live game's files or ports.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is a
//! process-wide `OnceLock`. Rust gives each `tests/*.rs` file its own
//! process, so this file is isolated from the rest of the suite, but two
//! `#[tokio::test]`s inside THIS file would share it and race.

use std::path::PathBuf;
use game::adventure::AdventureManager;

/// Must match `adventure_web::BUNDLE_OPERATOR_LOGIN`, which is private.
const OPERATOR_LOGIN: &str = "lokati_gaming";
/// In the fight, so participant-tier members are theirs to read.
const PARTICIPANT_LOGIN: &str = "kazesosa";
/// Logged in, but not in this fight - the case that separates "has an
/// account" from "was there", and the one an author is most likely to get
/// wrong by treating any session as sufficient.
const OUTSIDER_LOGIN: &str = "someone_else";

const PUBLIC_MEMBERS: &[&str] = &["core", "replay", "playerVitals"];
const PARTICIPANT_MEMBERS: &[&str] = &["buffs", "dot"];
const OPERATOR_MEMBERS: &[&str] = &["rolls"];

/// A bundle with every member present, written the way the real writer
/// writes one. `participants` carries a DISPLAY name while sessions carry a
/// lowercased login, so this also exercises the case-insensitive match.
fn bundle_json() -> String {
    r#"{"manifest":{"schemaVersion":1,"minReaderVersion":1,"fightId":"0000000001","startedAtUnixMs":1755690000000,"realDurationMs":2800,"displayDurationMs":6000,"pinned":false,"members":{}},"members":{"core":{"participants":["Kazesosa"],"stage":2056},"replay":[{"seq":0,"kind":"defeat","atMs":1,"unit":"kazesosa"}],"playerVitals":[{"id":"kazesosa","hpSamples":[[0,100]]}],"buffs":[{"seq":1,"kind":"shield","atMs":2,"healer":"kazesosa","target":"kazesosa","amount":5}],"dot":[{"seq":2,"kind":"attack","atMs":3,"attacker":"kazesosa","target":"__enemy_0__","damage":1,"targetHpAfter":0,"sourceKind":"dot"}],"rolls":[{"eventId":1,"hitId":1,"atMs":0,"category":"crit","source":"Crit chance","actor":"kazesosa"}]}}"#
        .to_string()
}

#[tokio::test]
async fn bundle_members_are_served_only_to_their_own_tier() {
    // Integration tests run with their PACKAGE dir as CWD (game/, under the
    // workspace suite), but the template loader resolves "templates/" against
    // CWD and that directory belongs to the workspace root (see render.rs's
    // own CARGO_MANIFEST_DIR escape hatch for the unit-test half of this).
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).expect("failed to anchor CWD at the workspace root");
    let scratch = std::env::temp_dir().join(format!("replay_bundle_tiers_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(
        &sessions_path,
        format!(
            r#"{{"op-token":{{"login":"{OPERATOR_LOGIN}","display_name":"Lokati","created_at":{now}}},"part-token":{{"login":"{PARTICIPANT_LOGIN}","display_name":"Kazesosa","created_at":{now}}},"out-token":{{"login":"{OUTSIDER_LOGIN}","display_name":"SomeoneElse","created_at":{now}}}}}"#
        ),
    )
    .expect("failed to seed the scratch sessions file");

    assert!(
        game::adventure::set_data_dir(scratch.clone()),
        "set_data_dir must succeed - only caller in this process"
    );

    let bundle_dir = scratch.join("adventure-fights-bundle");
    std::fs::create_dir_all(&bundle_dir).expect("failed to create the bundle tier dir");
    std::fs::write(bundle_dir.join("fight-0000000001.json"), bundle_json()).expect("failed to seed a bundle");

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

    // `token: None` is a logged-out caller, which is what the public
    // overlay audience actually is.
    let fetch = |token: Option<&'static str>, member: &'static str| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let mut request = client.get(format!("{base}/fights/1/members/{member}"));
            if let Some(token) = token {
                request = request.header(reqwest::header::COOKIE, format!("adv_session={token}"));
            }
            request.send().await.expect("GET failed").status()
        }
    };

    let ok = reqwest::StatusCode::OK;
    // 404, not 403: a 403 confirms the fight exists and holds the member.
    let denied = reqwest::StatusCode::NOT_FOUND;

    // --- logged out ---------------------------------------------------
    for member in PUBLIC_MEMBERS {
        assert_eq!(fetch(None, member).await, ok, "{member} is public and must be served logged-out");
    }
    for member in PARTICIPANT_MEMBERS.iter().chain(OPERATOR_MEMBERS) {
        assert_eq!(
            fetch(None, member).await,
            denied,
            "{member} MUST NOT be reachable without a session - this is the invariant that keeps \
             shield/buffSnapshot off the public feed",
        );
    }

    // --- logged in, but not in this fight -----------------------------
    for member in PUBLIC_MEMBERS {
        assert_eq!(fetch(Some("out-token"), member).await, ok);
    }
    for member in PARTICIPANT_MEMBERS.iter().chain(OPERATOR_MEMBERS) {
        assert_eq!(
            fetch(Some("out-token"), member).await,
            denied,
            "{member}: a session alone must not grant participant tier - being in the fight is what does",
        );
    }

    // --- a participant of this fight ----------------------------------
    for member in PUBLIC_MEMBERS.iter().chain(PARTICIPANT_MEMBERS) {
        assert_eq!(
            fetch(Some("part-token"), member).await,
            ok,
            "{member}: a participant must be able to read their own fight's participant tier",
        );
    }
    for member in OPERATOR_MEMBERS {
        assert_eq!(
            fetch(Some("part-token"), member).await,
            denied,
            "{member}: full roll logs are operator-only, participant or not",
        );
    }

    // --- the operator -------------------------------------------------
    for member in PUBLIC_MEMBERS.iter().chain(PARTICIPANT_MEMBERS).chain(OPERATOR_MEMBERS) {
        assert_eq!(fetch(Some("op-token"), member).await, ok, "{member}: the operator reads everything");
    }

    // --- the content is real, not an empty 200 ------------------------
    let buffs = client
        .get(format!("{base}/fights/1/members/buffs"))
        .header(reqwest::header::COOKIE, "adv_session=part-token")
        .send()
        .await
        .expect("GET failed")
        .text()
        .await
        .expect("body");
    assert!(buffs.contains("\"kind\":\"shield\""), "a participant must get the actual buffs member, got {buffs}");

    // --- unknown member, and unknown fight ----------------------------
    assert_eq!(
        fetch(Some("op-token"), "secrets").await,
        denied,
        "an unknown member name must be refused even for the operator - fail closed, not open",
    );
    let missing = client
        .get(format!("{base}/fights/999/members/core"))
        .send()
        .await
        .expect("GET failed")
        .status();
    assert_eq!(missing, denied, "a fight that is not in the tier must 404");

    let _ = std::fs::remove_dir_all(&scratch);
}
