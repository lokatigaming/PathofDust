// Channel points setup for two self-service rewards:
//   - "Set Entrance Theme Song" — viewers redeem it (entering a YouTube
//     link/search as the reward's text input) to set their own entrance
//     theme, no mod needed.
//   - "Interrupt the Music" — viewers redeem it to insert a song right
//     now AND cast a !voteskip vote against whatever it interrupted, in
//     one action.
// See eventsub.rs's TwitchEvent::ChannelPointsRedemption for the actual
// redemption handling (in main.rs, since it needs song_requests/
// entrance_themes, both constructed after this runs).

use crate::twitch::helix::HelixClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// pub, not pub(crate) (2026-08-19, Release 2 observability) - main.rs's
// redemption dispatch logs each redemption's title alongside its
// reward_id/username, reusing these instead of a second copy of the
// literal strings. `src/main.rs` is a separate BINARY crate from this
// library crate (`use twitch_bot_rs::channel_points` is a cross-crate
// access from main.rs's perspective, same as any external dependency),
// so `pub(crate)` - which only reaches within this library crate itself -
// isn't enough; confirmed the hard way (E0603) when `cargo test`
// actually compiled the bin target and `cargo build`'s own success
// turned out to be a false read - piping through `tail` swallowed
// cargo's real (nonzero) exit code behind the pipe's own.
pub const THEME_REWARD_TITLE: &str = "Set Entrance Theme Song";
const THEME_REWARD_PROMPT: &str =
    "Enter a YouTube link or song search — this becomes your entrance theme, played the next time you show up in chat!";

pub const INTERRUPT_REWARD_TITLE: &str = "Interrupt the Music";
const INTERRUPT_REWARD_PROMPT: &str =
    "Enter a YouTube link or song search — it plays immediately, interrupting the current song, AND counts as your vote to skip it!";

#[derive(Serialize, Deserialize)]
struct RewardState {
    reward_id: String,
}

/// Shared by both rewards below — creates `title` via the Helix API on
/// first run and persists the returned id at `path` so it's only ever
/// created once. A creation failure (most likely: tokens.json doesn't
/// have the `channel:manage:redemptions` scope yet — see `cargo run
/// --bin auth`) is logged and returns `None` rather than propagating, so
/// the rest of the bot still starts fine; the redemption EventSub
/// subscription is just skipped in that case.
async fn ensure_reward(helix: &HelixClient, broadcaster_id: &str, title: &str, cost: u32, prompt: &str, requires_input: bool, path: PathBuf) -> Option<String> {
    if let Some(state) = crate::state::load_json::<RewardState>(&path) {
        return Some(state.reward_id);
    }

    match helix.create_custom_reward(broadcaster_id, title, cost, prompt, requires_input).await {
        Ok(reward_id) => {
            if let Err(err) = crate::state::save_json(&path, &RewardState { reward_id: reward_id.clone() }) {
                tracing::error!("Failed to persist {}: {err}", path.display());
            }
            tracing::info!("Created \"{title}\" channel points reward ({cost} points).");
            Some(reward_id)
        }
        Err(err) => {
            tracing::error!(
                "Failed to create \"{title}\" channel points reward — its redemptions are disabled until this \
                 succeeds. Likely cause: tokens.json is missing the channel:manage:redemptions scope; run \
                 `cargo run --bin auth` to re-authorize. Error: {err}"
            );
            None
        }
    }
}

pub async fn ensure_theme_reward(helix: &HelixClient, broadcaster_id: &str, cost: u32, path: PathBuf) -> Option<String> {
    ensure_reward(helix, broadcaster_id, THEME_REWARD_TITLE, cost, THEME_REWARD_PROMPT, true, path).await
}

pub async fn ensure_interrupt_reward(helix: &HelixClient, broadcaster_id: &str, cost: u32, path: PathBuf) -> Option<String> {
    ensure_reward(helix, broadcaster_id, INTERRUPT_REWARD_TITLE, cost, INTERRUPT_REWARD_PROMPT, true, path).await
}
