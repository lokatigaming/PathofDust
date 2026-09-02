// The `game` binary (2026-08-18, architecture refactor Stage 2) - the
// standalone-game addendum's actual deliverable: starts, fights,
// persists, and serves the adventure game's full web UI (dashboard,
// wiki, overlay, /ws) with no other process running at all. Since World
// 2 (2026-09-02) this is the ONLY entry point: Twitch, the `/api/*` bot
// seam and the bot process itself are gone, so there is no longer a
// second in-process copy to collide with. Reads `.env` from the
// repo-root working directory for ADVENTURE_WEB_PORT /
// ADVENTURE_OVERLAY_SERVER_PORT / OPERATOR_LOGIN / GAME_DATA_DIR.
//
// ADVENTURE_WEB_PUBLIC_URL is read by nothing (2026-09-02): its sole
// consumer was the Twitch OAuth `redirect_uri`. It is being removed from
// the unit file and the drop-in at deploy time rather than left set, per
// the owner's ruling - a variable no code consumes reads as meaningful to
// whoever finds it next. Do not re-introduce it.
//
// Set GAME_DATA_DIR to point this instance at a different directory (see
// `adventure::set_data_dir`) - mainly useful for local testing, not
// needed for a real run.

use game::adventure::AdventureManager;
use std::path::PathBuf;
use tracing_subscriber::prelude::*;

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn env_u16_or(key: &str, default: u16) -> u16 {
    env_var(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Not `#[tokio::main]` (Stage 5, REFACTOR_PLAN.md, 2026-08-19) - matches
/// the bot's own `src/main.rs`, for the exact same reason: this process
/// now runs the REAL `simulate_battle`/`apply_hit` combat simulation
/// (moved here wholesale by the Stage 1/2 crate split), the same code
/// that caused repeat `STATUS_STACK_OVERFLOW` crashes bot-side badly
/// enough to need a dedicated watchdog (see repo-root watchdog.ps1's own
/// doc) before this refactor even started. The bot's own fix was never
/// mirrored here - Tokio's 2MiB default worker stack was fine while this
/// binary only served the dashboard/wiki (Stage 2's own live-verification
/// never ran a real fight), but Stage 4 made this binary the ONLY thing
/// that ever calls `run_encounter_inner` at all once a bot is pointed at
/// it, so this gap needed closing before any real-traffic bake could be
/// trusted not to reproduce the exact crash class that motivated the
/// watchdog in the first place.
fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    // Moved ABOVE the file-logging block below (2026-08-29,
    // Linux-readiness) - `logs/` now resolves through `data_path` like
    // every other written path, and `set_data_dir` has to have run before
    // the first `data_path` call or the OnceLock locks in the default.
    // Nothing between here and the old position logs, panics with a
    // message the log is expected to carry, or reads an env var: the
    // three statements are `create_dir_all`, the rolling appender, and
    // the subscriber build. The one env read in that stretch is the
    // subscriber's own `RUST_LOG`, which is now `.env`-settable where it
    // previously was not - inert today (neither the production `.env` nor
    // `.env.example` defines it) and process-env values still win, since
    // `dotenvy::dotenv` does not override what is already set.
    let _ = dotenvy::dotenv();

    // Optional - see this file's own top-of-file doc. Absent (the normal
    // case for a real standalone run) leaves data_path at its default,
    // i.e. every persisted file resolves exactly where the in-process
    // bot's own copy already reads/writes it - the same live game.
    if let Some(dir) = env_var("GAME_DATA_DIR") {
        game::adventure::set_data_dir(PathBuf::from(dir));
    }

    // File logging (Stage 5, 2026-08-19) - matches the bot's own
    // src/main.rs identically, and for the same reason: a plain stdout
    // logger writes nowhere a human can ever read once this runs headless
    // under a Scheduled Task (no attached console). `_log_guard` must
    // stay alive for the whole program - dropping it stops the
    // background flush thread and buffered lines are lost. Previously
    // this binary only ever ran in a foreground terminal during Stage
    // 1-4's own manual smoke tests, where stdout was enough - a real bake
    // period (or any unattended run) needs this the same way the bot
    // needed it.
    let logs_dir = game::adventure::data_path("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "game.log");
    let (non_blocking, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!("PANIC: {panic_info}");
    }));

    // Reported here rather than where `set_data_dir` is actually called -
    // that now runs before the subscriber exists, so a line emitted there
    // would go nowhere.
    if env_var("GAME_DATA_DIR").is_some() {
        tracing::info!("GAME_DATA_DIR set - persistence redirected away from the default location.");
    }

    let adventure_web_port = env_u16_or("ADVENTURE_WEB_PORT", 4005);
    let adventure_overlay_server_port = env_u16_or("ADVENTURE_OVERLAY_SERVER_PORT", 4004);

    let adventure = AdventureManager::new(
        PathBuf::from("adventure-characters.json"),
        PathBuf::from("adventure-world.json"),
        PathBuf::from("adventure-reforge-cooldown.json"),
    );

    adventure.clone().spawn_encounter_loop();
    adventure.clone().spawn_basic_encounter_loop();
    adventure.clone().spawn_rampage_loop();
    adventure.clone().spawn_fight_summary_flush_loop();

    // One-time giveaway: hands the "Wings of Flight" cosmetic to one
    // random currently-joined character - per a live request, "while
    // we're at it" alongside adding the cosmetic itself. Guarded by its
    // own marker (same fire-once shape as every other one-off grant) so
    // a restart never re-rolls it. Moved here from the bot's main.rs at
    // Stage 4 (architecture refactor, 2026-08-19) - real game-state
    // mutation belongs in the process that owns the state; the chat
    // announcement itself now comes from `grant_random_wings` pushing
    // straight onto `announcements_tx` (see its own doc), not from a
    // `chat_client.say` this process no longer has anyway. Spawned
    // rather than awaited inline - nothing else in startup depends on
    // this having finished.
    {
        const WINGS_GIVEAWAY_MARKER_PATH: &str = "adventure-wings-giveaway-marker.json";
        // Through `data_path` like every other marker (2026-08-29,
        // Linux-readiness) - this one was written straight from `main`
        // and so was the last game-state marker `GAME_DATA_DIR` did not
        // move. Resolved once, out here, so the load and the save can
        // never disagree.
        let wings_marker_path = game::adventure::data_path(WINGS_GIVEAWAY_MARKER_PATH);
        if game::state::load_json::<bool>(&wings_marker_path).is_none() {
            let adventure = adventure.clone();
            tokio::spawn(async move {
                adventure.grant_random_wings().await;
                if let Err(err) = game::state::save_json(&wings_marker_path, &true) {
                    tracing::error!("Failed to persist wings giveaway marker to {WINGS_GIVEAWAY_MARKER_PATH}: {err}");
                }
            });
        }
    }

    game::adventure_overlay_server::start_adventure_overlay_server(adventure_overlay_server_port, PathBuf::from("public_adventure_overlay"), adventure.clone())
        .await?;
    game::adventure_web::start_adventure_web_server(
        adventure_web_port,
        adventure.clone(),
        PathBuf::from("adventure-sessions.json"),
    )
    .await?;

    tracing::info!("game standalone binary running - adventure_web on port {adventure_web_port}, adventure_overlay_server on port {adventure_overlay_server_port}. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down.");
    Ok(())
}
