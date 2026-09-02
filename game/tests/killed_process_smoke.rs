//! A genuinely killed process, not a synthetic stand-in: spawns the REAL
//! compiled `game` binary as a child process, sends it a hard kill
//! (`TerminateProcess` on Windows via `Child::kill()` - not a graceful
//! shutdown, the actual worst case a crash/OOM/manual kill would look
//! like), and confirms the wire-level failure is clean. Also checks the
//! reverse: a fresh process on the same port, same data dir, recovers
//! and serves normally again.
//!
//! Re-pointed at the PUBLIC dashboard root (2026-09-02, Twitch removal).
//! It used to prove liveness through `GET /api/commands/party` with a
//! shared-secret header; that seam is deleted, and the liveness/recovery
//! property it was really testing never had anything to do with it. `GET
//! /` is unauthenticated and renders the logged-out page, so the
//! assertion is now "the process serves a real rendered page", which is
//! strictly stronger than the old status-only check.
//!
//! Lives in `game/tests/`, not the bot crate's `tests/`, specifically so
//! `env!("CARGO_BIN_EXE_game")` resolves - that env var is only set for
//! integration tests in the SAME package as the binary it names, and
//! guarantees the binary is built (and the path is real) before this
//! test runs, rather than assuming a stale/manually-built artifact.
//!
//! Fixed, dedicated ports (not the game's own real defaults, not
//! ephemeral - `adventure_web.rs`'s own startup log line prints the
//! REQUESTED port, not the OS-assigned one, so port 0 wouldn't be
//! learnable here anyway) - a stray leftover process from a previously
//! killed test run holding this exact port is the one known flakiness
//! risk, same class as any other process-spawning test.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

const TEST_PORT: u16 = 24005;
const TEST_OVERLAY_PORT: u16 = 24004;

fn spawn_game(scratch: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_game"))
        // Also redirects the game's own `logs/` dir (a bare CWD-relative
        // path, not routed through GAME_DATA_DIR - see main.rs's Stage 5
        // file-logging addition) into the scratch dir instead of the
        // real repo's `game/logs/` (cargo test's CWD is the package
        // root - the same lesson golden_corpus.rs/render.rs already hit).
        .current_dir(scratch)
        .env("GAME_DATA_DIR", scratch)
        .env("ADVENTURE_WEB_PORT", TEST_PORT.to_string())
        .env("ADVENTURE_OVERLAY_SERVER_PORT", TEST_OVERLAY_PORT.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the real game binary via CARGO_BIN_EXE_game - was it built?")
}

/// The spawned binary is a release build, so `render.rs`'s `#[cfg(test)]`
/// absolute-path escape hatch does not apply to it - it resolves a bare
/// `templates/` against its own CWD, which here is the scratch dir.
/// Copied rather than symlinked: a symlink needs privileges on Windows.
/// Lifted verbatim from `game_data_dir_paths.rs`, which hit this first.
///
/// Newly required (2026-09-02): while this test proved liveness through
/// `/api/commands/party` it never rendered a template, so a scratch dir
/// without `templates/` went unnoticed. `GET /` does render one.
fn copy_templates_into(cwd: &std::path::Path) {
    let source = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../templates"));
    let dest = cwd.join("templates");
    std::fs::create_dir_all(&dest).expect("failed to create the scratch templates dir");
    for entry in std::fs::read_dir(source).expect("failed to read the workspace templates dir").flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), dest.join(entry.file_name())).expect("failed to copy a template into the scratch dir");
        }
    }
}

async fn wait_until_ready(client: &reqwest::Client, base: &str) {
    for _ in 0..50 {
        if client.get(format!("{base}/")).timeout(Duration::from_millis(500)).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("game process never became reachable at {base} within 5s");
}

#[tokio::test]
async fn a_killed_game_process_stops_answering_and_a_fresh_one_recovers() {
    let scratch = std::env::temp_dir().join(format!("killed_process_smoke_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("failed to create scratch dir");
    copy_templates_into(&scratch);

    let base = format!("http://127.0.0.1:{TEST_PORT}");
    let client = reqwest::Client::new();

    let mut child = spawn_game(&scratch);
    wait_until_ready(&client, &base).await;

    let resp = client
        .get(&base)
        .send()
        .await
        .expect("request to the live process failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "the live process must answer normally before it's killed");
    assert!(
        resp.text().await.expect("body").contains("Adventure Character Dashboard"),
        "and must actually RENDER, not merely accept the connection"
    );

    // The actual "deliberately-killed game process."
    child.kill().expect("failed to kill the game process");
    child.wait().expect("failed to reap the killed process");

    // A short window for the OS to actually release the port - right
    // after kill() a connection can still transiently succeed on some
    // platforms while the socket finishes tearing down.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let dead_result = client
        .get(&base)
        .timeout(Duration::from_secs(5))
        .send()
        .await;
    assert!(dead_result.is_err(), "a request to a killed process must fail, not succeed - got {dead_result:?}");

    // Recovery: a FRESH process on the exact same port, same
    // GAME_DATA_DIR, picks up right where the old one left off.
    let mut child2 = spawn_game(&scratch);
    wait_until_ready(&client, &base).await;
    let resp = client
        .get(&base)
        .send()
        .await
        .expect("request to the restarted process failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "a fresh process on the same port must serve normally again");
    assert!(
        resp.text().await.expect("body").contains("Adventure Character Dashboard"),
        "and must render the same page the pre-kill process did"
    );

    child2.kill().ok();
    child2.wait().ok();
    std::fs::remove_dir_all(&scratch).ok();
}
