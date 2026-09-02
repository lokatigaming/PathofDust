//! Linux-readiness (2026-08-29) - proves the six-file `GAME_DATA_DIR`
//! gap is closed AND, far more importantly, that closing it did not move
//! a single byte for the deployment that exists today.
//!
//! Production runs with `GAME_DATA_DIR` UNSET, and its data directory IS
//! its deployment root. Every path this branch rerouted therefore has to
//! resolve, with the variable unset, to exactly the CWD-relative location
//! it resolved to before - a path that shifts on deploy silently strands
//! live character data, sessions and accounts at the old location. That
//! is the assertion this file exists for; the `GAME_DATA_DIR`-is-set half
//! is the cheaper one.
//!
//! Spawns the REAL compiled `game` binary rather than exercising
//! `data_path` directly, for two reasons. First, `data_path`'s base is a
//! process-global `OnceLock` and `game/src/adventure/paths.rs` documents
//! at length why an in-process test of it is inherently flaky - any test
//! in the shared test binary can race to be the first caller and lock in
//! the value. A child process has its own `OnceLock` and no race at all.
//! Second, two of these paths are resolved during STARTUP (the log
//! directory before the subscriber even exists, and the wings marker),
//! so only a real startup exercises them.
//!
//! Same harness shape as `killed_process_smoke.rs` - `CARGO_BIN_EXE_game`
//! for a guaranteed-built binary, fixed dedicated ports, `current_dir`
//! pointed at a scratch tree. A stray process from a previously killed
//! run holding one of these ports is the one known flakiness risk, same
//! class as that test.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;


/// Distinct from `killed_process_smoke.rs`'s pair, and from each other -
/// the two scenarios below run sequentially inside one test, but a
/// leftover listener from either must not be able to answer for the other.
const UNSET_WEB_PORT: u16 = 24105;
const UNSET_OVERLAY_PORT: u16 = 24104;
const SET_WEB_PORT: u16 = 24115;
const SET_OVERLAY_PORT: u16 = 24114;

/// Every file this branch rerouted through `data_path`, plus the one that
/// came along for free.
///
/// `adventure-accounts.json` is NOT independently resolved - `accounts.rs`
/// derives it from the sessions path with `with_file_name` - so seeing it
/// in the right directory is what proves `adventure-sessions.json`'s own
/// resolution moved with it rather than the two splitting apart.
const REROUTED_FILES: &[&str] = &[
    "logs",
    "adventure-wings-giveaway-marker.json",
    "adventure-sessions.json",
    "adventure-accounts.json",
];

/// Kills the child on the way out, INCLUDING when an assertion panics
/// and unwinds past it.
///
/// Not a nicety. A leaked `game.exe` keeps listening on this test's fixed
/// port, so the next run fails against the previous run's stale process
/// instead of its own - and, observed while building this file, it also
/// holds the test harness's inherited handles open, which hangs the whole
/// `cargo test` invocation long after the tests themselves have finished.
/// Every failure path has to reap.
struct GameProcess(Child);

impl Drop for GameProcess {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

fn spawn_game(cwd: &Path, data_dir: Option<&Path>, web_port: u16, overlay_port: u16) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_game"));
    command
        .current_dir(cwd)
        .env("ADVENTURE_WEB_PORT", web_port.to_string())
        .env("ADVENTURE_OVERLAY_SERVER_PORT", overlay_port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match data_dir {
        Some(dir) => command.env("GAME_DATA_DIR", dir),
        // Not merely "don't set it" - the parent test process inherits the
        // developer's real environment, and a `GAME_DATA_DIR` set there
        // would quietly turn the unset scenario into a second copy of the
        // set one, i.e. the exact assertion this file exists to make would
        // stop being made while still passing.
        None => command.env_remove("GAME_DATA_DIR"),
    };
    command.spawn().expect("failed to spawn the real game binary via CARGO_BIN_EXE_game - was it built?")
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

/// The spawned binary is a release build, so `render.rs`'s `#[cfg(test)]`
/// absolute-path escape hatch does not apply to it - it resolves a bare
/// `templates/` against its own CWD, which here is the scratch tree.
/// Copied rather than symlinked: a symlink needs privileges on Windows.
fn copy_templates_into(cwd: &Path) {
    let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../templates"));
    let dest = cwd.join("templates");
    std::fs::create_dir_all(&dest).expect("failed to create the scratch templates dir");
    for entry in std::fs::read_dir(source).expect("failed to read the workspace templates dir").flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), dest.join(entry.file_name())).expect("failed to copy a template into the scratch dir");
        }
    }
}

/// Drives one process to the point where every rerouted file has actually
/// been written, then returns. Each of these is deterministic on a fresh
/// data set - no fight and no roster is involved.
async fn write_every_rerouted_file(client: &reqwest::Client, base: &str) {
    // adventure-accounts.json AND adventure-sessions.json - registering
    // persists the account and then mints a session, writing both.
    let registered = client
        .post(format!("{base}/account/register"))
        .form(&[("username", "datadirprobe"), ("password", "probe-password-1")])
        .send()
        .await
        .expect("POST /account/register failed");
    assert!(registered.status().is_success() || registered.status().is_redirection(), "registration must succeed, got {}", registered.status());

    // adventure-wings-giveaway-marker.json is written by a task spawned at
    // startup - unconditionally, after `grant_random_wings` returns, which
    // on an empty roster is immediate. Give it a moment to land.
    tokio::time::sleep(Duration::from_millis(500)).await;
}

/// `patch-notes.json` is read-only, so where it resolves can only be shown
/// by which copy gets SERVED. Seeds a uniquely-dated entry at `expected`
/// and, when the two directories differ, a decoy at `cwd`.
fn seed_patch_notes(cwd: &Path, expected: &Path) {
    let entry = |date: &str| {
        serde_json::json!([{ "date": date, "sections": [{ "heading": "probe", "items": ["probe"] }] }]).to_string()
    };
    std::fs::write(expected.join("patch-notes.json"), entry("2099-01-01")).expect("failed to seed the expected patch notes");
    if cwd != expected {
        std::fs::write(cwd.join("patch-notes.json"), entry("2098-02-02")).expect("failed to seed the decoy patch notes");
    }
}

async fn assert_patch_notes_came_from_expected(client: &reqwest::Client, base: &str, decoyed: bool) {
    let body = client.get(format!("{base}/patch-notes")).send().await.expect("GET /patch-notes failed").text().await.expect("patch notes body was not text");
    assert!(body.contains("2099-01-01"), "/patch-notes must render the copy in the resolved data directory");
    if decoyed {
        assert!(!body.contains("2098-02-02"), "/patch-notes must NOT render the copy sitting in the process CWD once GAME_DATA_DIR points elsewhere");
    }
}

/// `data_dir` `None` is the production shape: unset, so everything must
/// land directly in the process's CWD, exactly as it did before this
/// branch. `Some` must move every one of them and leave the CWD clean.
async fn run_scenario(label: &str, data_dir_is_set: bool, web_port: u16, overlay_port: u16) {
    let root = std::env::temp_dir().join(format!("game_data_dir_paths_{label}_{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("failed to create scratch root");
    copy_templates_into(&root);

    let data_dir = data_dir_is_set.then(|| root.join("data"));
    if let Some(dir) = &data_dir {
        std::fs::create_dir_all(dir).expect("failed to create scratch data dir");
    }
    let expected = data_dir.clone().unwrap_or_else(|| root.clone());
    seed_patch_notes(&root, &expected);

    let base = format!("http://127.0.0.1:{web_port}");
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().expect("failed to build the test client");
    let child = GameProcess(spawn_game(&root, data_dir.as_deref(), web_port, overlay_port));
    wait_until_ready(&client, &base).await;

    assert_patch_notes_came_from_expected(&client, &base, data_dir_is_set).await;
    write_every_rerouted_file(&client, &base).await;

    // Explicit, not left to scope end - the file assertions below must run
    // against a process that has stopped writing. `Drop` is the safety net
    // for the panicking paths above, not the normal path.
    drop(child);

    for name in REROUTED_FILES {
        assert!(expected.join(name).exists(), "[{label}] {name} must resolve to {} - it did not appear there", expected.display());
        if data_dir_is_set {
            assert!(!root.join(name).exists(), "[{label}] {name} must NOT be left behind in the process CWD when GAME_DATA_DIR is set");
        }
    }

    std::fs::remove_dir_all(&root).ok();
}

/// THE test that protects live data. `GAME_DATA_DIR` unset - the exact
/// production configuration - must put every rerouted file directly in the
/// process's working directory, which is the deployment root, which is
/// where all of them already are today.
#[tokio::test]
async fn with_game_data_dir_unset_every_file_resolves_cwd_relative_exactly_as_before() {
    run_scenario("unset", false, UNSET_WEB_PORT, UNSET_OVERLAY_PORT).await;
}

/// The other half: set it and all of them move together, leaving nothing
/// behind. A file that moved while its neighbour did not would be worse
/// than neither moving - a half-moved data dir is the worst outcome of
/// all.
#[tokio::test]
async fn with_game_data_dir_set_every_file_follows_it() {
    run_scenario("set", true, SET_WEB_PORT, SET_OVERLAY_PORT).await;
}

