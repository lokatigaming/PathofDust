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
// Bodies match game/src/state.rs's load_json/save_json exactly - including
// the atomic `write_atomic` both save paths now go through; a future
// divergence between the two copies should be a deliberate fork, never
// silent drift.

use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    write_atomic(path.as_ref(), &contents)
}

/// See `game::state::write_atomic` for the full reasoning - this is the
/// deliberate mirror of it, kept in step per this module's header note.
/// Short version: temp file beside the target, fsync, rename over. A crash
/// mid-write can then only ever leave the old complete file or the new
/// complete file on disk, never a truncated one.
const ATOMIC_RENAME_ATTEMPTS: u32 = 5;
const ATOMIC_RENAME_RETRY: std::time::Duration = std::time::Duration::from_millis(20);

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_atomic(path: &Path, contents: &str) -> anyhow::Result<()> {
    let dir: PathBuf = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "state".to_string());
    let unique = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!("{stem}.{}.{unique}.tmp", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    })();
    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(anyhow::Error::new(err).context(format!("writing temp file {}", temp.display())));
    }

    let mut last_err = None;
    for attempt in 0..ATOMIC_RENAME_ATTEMPTS {
        match std::fs::rename(&temp, path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                if attempt + 1 < ATOMIC_RENAME_ATTEMPTS {
                    std::thread::sleep(ATOMIC_RENAME_RETRY);
                }
            }
        }
    }
    let _ = std::fs::remove_file(&temp);
    Err(anyhow::Error::new(last_err.expect("the loop runs at least once, so a failure path always set this"))
        .context(format!("renaming {} over {}", temp.display(), path.display())))
}