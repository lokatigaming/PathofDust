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

/// Same as `save_json`, but compact: no indentation, no spaces.
///
/// Only for files that are written by the machine and read by the
/// machine. The pretty variant above stays the default precisely
/// because most of what this bot persists (tokens, commands, character
/// state) is small enough that indentation costs nothing and is worth
/// being able to read in an editor when something goes wrong.
///
/// Fight archives are the opposite case. Measured 2026-08-20 on a real
/// detail-tier boss fight: 34.31 MB on disk re-serialized compact to
/// 22.94 MB, so **33.1% of every archive byte was indentation** - and
/// the detail tier is the single largest thing this game writes. Nobody
/// reads a 34 MB fight log in an editor; every consumer parses it, and
/// `serde_json` parses both forms identically, so existing pretty files
/// on disk keep loading with no migration.
pub fn save_json_compact<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let contents = serde_json::to_string(value)?;
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod compact_save_tests {
    use super::*;

    /// Guards the one property the fight archive depends on: that this
    /// writes no indentation. A future refactor pointing `write_and_prune`
    /// back at `save_json` would silently re-add 33.1% to every archive
    /// file, and nothing else in the system would notice or complain.
    #[test]
    fn compact_save_writes_no_indentation_and_round_trips() {
        let value = serde_json::json!({
            "events": [{ "kind": "attack", "atMs": 0 }, { "kind": "defeat", "atMs": 12 }],
            "stage": 2056,
        });
        let path = std::env::temp_dir().join("pod-compact-save-test.json");

        save_json_compact(&path, &value).expect("compact save must succeed");
        let written = std::fs::read_to_string(&path).expect("must be readable");
        let parsed: serde_json::Value = load_json(&path).expect("must parse back");
        let _ = std::fs::remove_file(&path);

        assert!(!written.contains('\n'), "compact output must not be line-broken: {written}");
        assert!(!written.contains(": "), "compact output must not pad after colons: {written}");
        assert_eq!(parsed, value, "compact output must round-trip unchanged");

        // The saving this exists for, in miniature.
        let pretty = serde_json::to_string_pretty(&value).expect("must serialize");
        assert!(written.len() < pretty.len(), "compact must be smaller than pretty");
    }
}
