//! Stage 1.5 harness #3 (2026-08-18, architecture refactor) - the HTTP
//! golden-response harness deferred at Stage 0.5 execution time (no
//! path-injection point existed yet to safely run a disposable instance -
//! see REFACTOR_PLAN.md §9's execution log). Stage 1's
//! `adventure::set_data_dir` unblocks it: spins up a REAL, disposable
//! `AdventureManager` + `adventure_web` Axum server on an OS-assigned
//! ephemeral port, seeded from the pseudonymized character fixture (never
//! the real production data), and exercises it over genuine HTTP via
//! `reqwest` - status codes, content types, and (for `/join`) an actual
//! state change confirmed through the manager directly. Reused for
//! route-table-preservation verification during later stages: a route
//! disappearing, a param renamed, or a status code changing would fail
//! here.
//!
//! **Single test function, deliberately** - `adventure::set_data_dir` is
//! a process-wide `OnceLock` (see `game::adventure::paths`' own doc for
//! why: the same lesson learned the hard way building this exact
//! mechanism in Stage 1). Rust compiles each file under `tests/` into its
//! OWN separate process, so this file doesn't race
//! `character_fixture_roundtrip.rs` or anything in the `game` crate's own
//! `#[cfg(test)]` suite - but MULTIPLE `#[tokio::test]` functions inside
//! THIS one file would still share one process/OnceLock and race each
//! other for which one gets to call `set_data_dir` first. One disposable
//! server, spun up once, every check run against it in sequence, avoids
//! that entirely.

use std::path::PathBuf;
use twitch_bot_rs::adventure::AdventureManager;

const FIXTURE_PATH: &str = "tests/fixtures/characters_pseudonymized.json";

#[tokio::test]
async fn http_golden_responses_against_a_disposable_game_instance() {
    let scratch = std::env::temp_dir().join(format!("http_golden_harness_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");

    std::fs::copy(FIXTURE_PATH, scratch.join("adventure-characters.json")).expect("failed to seed the scratch characters file from the pseudonymized fixture");

    // A fake logged-in session, written directly as JSON (adventure_web's
    // own `Session` type is private to that module, not constructible
    // from here) - `sessions_path` is a genuine per-call parameter to
    // `start_adventure_web_server`, not one of Stage 1's `data_path`-
    // wrapped constants, so pointing it straight at the scratch dir is
    // enough on its own; no `set_data_dir` interaction for this one file.
    // Sessions load once into memory at server startup, so this file
    // only needs to be correct BEFORE `start_adventure_web_server` runs -
    // "test-login" deliberately doesn't match the fixture's own
    // player001.. naming, so the later join check can tell a REAL state
    // change apart from a character that merely already existed.
    const TEST_LOGIN: &str = "test-login";
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let sessions_path = scratch.join("adventure-sessions.json");
    std::fs::write(&sessions_path, format!(r#"{{"test-token":{{"login":"{TEST_LOGIN}","display_name":"TestLogin","created_at":{now_secs}}}}}"#))
        .expect("failed to seed the scratch sessions file");

    assert!(twitch_bot_rs::adventure::set_data_dir(scratch.clone()), "set_data_dir must succeed - this is the only caller in this test binary's whole process");

    let manager = AdventureManager::new(PathBuf::from("adventure-characters.json"), PathBuf::from("adventure-world.json"), PathBuf::from("adventure-reforge-cooldown.json"));

    let bound_addr = twitch_bot_rs::adventure_web::start_adventure_web_server(
        0, // ephemeral - the OS picks a free port, returned below
        "http://localhost".to_string(),
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        manager.clone(),
        sessions_path,
    )
    .await
    .expect("disposable adventure_web server must start");

    // `bound_addr`'s host is `0.0.0.0` (bind to every interface - see
    // start_adventure_web_server's own listener) - valid to bind, not
    // valid to connect a client to; loopback reaches the same listener.
    let base = format!("http://127.0.0.1:{}", bound_addr.port());
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build reqwest client");

    // GET routes - safe, read-only, the same set captured against live
    // production at Stage 0 execution time (see REFACTOR_PLAN.md §9),
    // now reproducible against a disposable instance instead of an
    // observed live snapshot.
    for (route, expect_html) in [("/", true), ("/inventory", true), ("/passives", true), ("/fights", true)] {
        let resp = client.get(format!("{base}{route}")).send().await.unwrap_or_else(|e| panic!("GET {route} failed: {e}"));
        assert_eq!(resp.status(), reqwest::StatusCode::OK, "GET {route} must return 200");
        if expect_html {
            let content_type = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
            assert!(content_type.starts_with("text/html"), "GET {route} must render HTML, got content-type {content_type:?}");
        }
    }

    // Unlike the plain GET pages above, /fights.json gates on a real
    // session (401 without one - see fights_json's own auth check), so
    // this one needs the same seeded cookie the /join check below uses.
    let fights_json =
        client.get(format!("{base}/fights.json")).header(reqwest::header::COOKIE, "adv_session=test-token").send().await.expect("GET /fights.json failed");
    assert_eq!(fights_json.status(), reqwest::StatusCode::OK, "GET /fights.json must return 200");
    let body = fights_json.text().await.expect("failed to read /fights.json body");
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("/fights.json must be valid JSON, got {e}: {body}"));
    assert!(parsed.is_array(), "/fights.json must return a JSON array, got: {body}");

    // One representative POST/mutating route, exercised authenticated
    // (via the seeded fake session's cookie) end to end - not just an
    // HTTP status check, but a REAL state change confirmed through the
    // manager directly, proving the disposable instance's data really is
    // isolated (TEST_LOGIN doesn't exist in the pseudonymized fixture at
    // all) and really does persist through the same code path a live
    // !join would use.
    assert!(manager.character(TEST_LOGIN).await.is_none(), "sanity check: TEST_LOGIN must not already exist in the fixture, or this test isn't proving anything");

    let join_resp = client.post(format!("{base}/join")).header(reqwest::header::COOKIE, "adv_session=test-token").send().await.expect("POST /join failed");
    assert!(join_resp.status().is_redirection(), "POST /join must redirect (Redirect::to(\"/\")), got {}", join_resp.status());

    let joined = manager.character(TEST_LOGIN).await;
    assert!(joined.is_some(), "POST /join with a valid session must actually create the character via the real AdventureManager - the whole point of testing through real HTTP instead of calling the handler fn directly");

    std::fs::remove_dir_all(&scratch).ok();
}
