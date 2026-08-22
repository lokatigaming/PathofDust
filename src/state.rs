// Small generic JSON file load/save helpers - the bot persists several
// independent bits of state this way (tokens.json, commands.json,
// tips-history.json, patreon-seen.json, personal-playlists.json,
// bugreports.json, playrandom-state.json, ...), porting the same pattern
// rather than introducing a database for what's fundamentally a handful
// of small local files.
//
// Local copy (2026-08-22, bot/game build-time decoupling): these lived in
// game/src/state.rs and reached this crate through lib.rs's
// `pub use game::state` re-export. Each side owns its own files - none of
// them cross the bot↔game seam - so there is no wire-compat risk and a
// shared crate would only have re-coupled the builds for no benefit.
// Bodies match game/src/state.rs's load_json/save_json exactly; a future
// divergence between the two copies should be a deliberate fork, never
// silent drift.

use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Option<T> {
    let path = path.as_ref();
    if !path.exists() {
        return None;
    }
    let contents = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&contents) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::error!("Failed to parse {}: {err}", path.display());
            None
        }
    }
}

/// Same contract as its game-side counterpart: pretty-printed, so anything
/// small enough to be hand-edited when something goes wrong stays readable
/// in an editor (see game/src/state.rs for why compact output is reserved
/// for machine-only fight archives).
pub fn save_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let contents = serde_json::to_string_pretty(value)?;
    std::fs::write(path, contents)?;
    Ok(())
}