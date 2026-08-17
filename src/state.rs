// Small generic JSON file load/save helpers — the Node bot persists several
// independent bits of state this way (tokens.json, commands.json,
// tips-history.json, patreon-seen.json), and this ports that same pattern
// rather than introducing a database for what's fundamentally a handful of
// small local files.

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

pub fn save_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let contents = serde_json::to_string_pretty(value)?;
    std::fs::write(path, contents)?;
    Ok(())
}
