// The `game` binary (2026-08-18, architecture refactor Stage 2) - the
// standalone-game addendum's actual deliverable: starts, fights,
// persists, and serves the adventure game's full web UI (dashboard,
// wiki, overlay, /ws) with NO Twitch bot process running at all. This is
// a SECOND, independent entry point over the exact same library code
// `twitch-bot-rs`'s own main.rs already calls in-process - "dual-mode
// transitional state" (REFACTOR_PLAN.md's own Stage 2 wording): this
// binary doesn't replace anything yet, it just proves the standalone
// path works. Reads the SAME `.env` file the bot does (same repo-root
// working directory, same TWITCH_CLIENT_ID/SECRET/ADVENTURE_WEB_PORT/
// ADVENTURE_OVERLAY_SERVER_PORT keys, same defaults) - running this
// ALONGSIDE a bot that's also starting its own in-process copy would
// collide on those same ports; the intended smoke-test shape is "game
// alone, bot not running" (see REFACTOR_PLAN.md's Stage 2 criterion),
// not side-by-side yet.
//
// Every real, on-disk file this touches (adventure-characters.json and
// the rest of §6's list) is the SAME data the bot's in-process instance
// reads - by design: this is meant to be the same live game, reachable
// through a new door, not a toy copy. Set GAME_DATA_DIR to point this
// instance at a different directory instead (see `adventure::set_data_dir`) -
// mainly useful for local testing, not needed for a real standalone run.

use game::adventure::{AdventureManager, PublishedConstants, PUBLISHED_CONSTANTS_PATH};
use std::path::PathBuf;

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn env_var_or(key: &str, default: &str) -> String {
    env_var(key).unwrap_or_else(|| default.to_string())
}

fn env_u16_or(key: &str, default: u16) -> u16 {
    env_var(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!("PANIC: {panic_info}");
    }));

    let _ = dotenvy::dotenv();

    // Optional - see this file's own top-of-file doc. Absent (the normal
    // case for a real standalone run) leaves data_path at its default,
    // i.e. every persisted file resolves exactly where the in-process
    // bot's own copy already reads/writes it - the same live game.
    if let Some(dir) = env_var("GAME_DATA_DIR") {
        if game::adventure::set_data_dir(PathBuf::from(dir)) {
            tracing::info!("GAME_DATA_DIR set - persistence redirected away from the default location.");
        }
    }

    let twitch_client_id =
        env_var("TWITCH_CLIENT_ID").ok_or_else(|| anyhow::anyhow!("Missing TWITCH_CLIENT_ID in .env - needed for the web dashboard's own Twitch login"))?;
    let twitch_client_secret = env_var("TWITCH_CLIENT_SECRET").ok_or_else(|| anyhow::anyhow!("Missing TWITCH_CLIENT_SECRET in .env"))?;
    let adventure_web_port = env_u16_or("ADVENTURE_WEB_PORT", 4005);
    let adventure_web_public_url = env_var_or("ADVENTURE_WEB_PUBLIC_URL", "http://localhost:4005");
    let adventure_overlay_server_port = env_u16_or("ADVENTURE_OVERLAY_SERVER_PORT", 4004);
    // Stage 3 API seam (REFACTOR_PLAN.md §4) - same env var, same
    // None-disables-the-mount default, as the bot's own config.rs reads;
    // must match whatever the bot process is configured with once Stage
    // 4's cutover actually uses this.
    let adventure_api_secret = env_var("ADVENTURE_API_SECRET");

    // Bot->game published constants (see PublishedConstants' own doc) -
    // a standalone game instance has no bot to publish these, so wiki.rs
    // reads whatever was last published (possibly stale, possibly never
    // written at all - both handled gracefully, see wiki_placeholder_map's
    // own "varies" fallback). Deliberately NOT written here - this
    // binary doesn't own that data, the bot does.
    if crate::env_var("GAME_SUPPRESS_MISSING_PUBLISHED_CONSTANTS_WARNING").is_none()
        && game::state::load_json::<PublishedConstants>(PUBLISHED_CONSTANTS_PATH).is_none()
    {
        tracing::warn!(
            "{PUBLISHED_CONSTANTS_PATH} not found - the wiki's chat-cooldown/vote-volume placeholders will render \"varies\" until the bot has started at least once."
        );
    }

    let adventure = AdventureManager::new(
        PathBuf::from("adventure-characters.json"),
        PathBuf::from("adventure-world.json"),
        PathBuf::from("adventure-reforge-cooldown.json"),
    );

    adventure.clone().spawn_encounter_loop();
    adventure.clone().spawn_basic_encounter_loop();
    adventure.clone().spawn_rampage_loop();

    game::adventure_overlay_server::start_adventure_overlay_server(adventure_overlay_server_port, PathBuf::from("public_adventure_overlay"), adventure.clone())
        .await?;
    game::adventure_web::start_adventure_web_server(
        adventure_web_port,
        adventure_web_public_url,
        twitch_client_id,
        twitch_client_secret,
        adventure.clone(),
        PathBuf::from("adventure-sessions.json"),
        adventure_api_secret,
    )
    .await?;

    tracing::info!("game standalone binary running - adventure_web on port {adventure_web_port}, adventure_overlay_server on port {adventure_overlay_server_port}. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down.");
    Ok(())
}
