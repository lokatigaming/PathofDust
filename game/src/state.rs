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

/// Fail-loud counterpart to [`load_json`] for startup-critical save files
/// (characters, world state, reforge cooldowns, rampage state). Three-way
/// contract:
///
/// * file ABSENT -> `None`, exactly like [`load_json`]. Fresh installs must
///   stay legal (there is nothing to load yet), and several tests depend on
///   constructing a manager against an empty directory.
/// * file present and parses -> `Some(value)`, the normal load path.
/// * file present but unreadable or unparseable -> PANIC. This is the whole
///   point: [`load_json`] logs and swallows parse failures, so a corrupt
///   file silently became "empty default" - and the next autosave then
///   overwrote the real data with that default (this exact shape wiped every
///   character to disk within ~9 seconds on 2026-08-22, after a UTF-8 BOM
///   made `serde_json` reject the file; only a backup saved the roster).
///   Refusing to start is strictly better than starting empty.
///
/// A leading UTF-8 BOM is stripped with a WARNING naming the file -
/// `read_to_string` happily accepts U+FEFF but `serde_json::from_str`
/// rejects it, so a BOM'd file used to fall into the swallow-and-default
/// trap above even though its JSON is fine. Genuinely malformed JSON still
/// fails loudly and unconditionally.
pub fn load_json_fail_loud<T: DeserializeOwned>(path: impl AsRef<Path>) -> Option<T> {
    let path = path.as_ref();
    if !path.exists() {
        return None;
    }
    // `exists()` just passed, so a read error here means the file is
    // present but unusable (permissions, it's a directory, invalid UTF-8,
    // ...) - present-but-unusable is not "absent", so it must not fall
    // back to the caller's default.
    let contents = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "Refusing to start: {} exists but could not be read: {err}. Fix it or restore it from backup - refusing to overwrite.",
            display_absolute(path)
        )
    });
    let contents = match contents.strip_prefix('\u{feff}') {
        Some(stripped) => {
            tracing::warn!("{} starts with a UTF-8 BOM; stripping it before parsing (serde_json rejects a leading BOM)", display_absolute(path));
            stripped
        }
        None => contents.as_str(),
    };
    match serde_json::from_str(contents) {
        Ok(value) => Some(value),
        Err(err) => panic!(
            "Refusing to start: {} exists but failed to parse: {err}. Fix it or restore it from backup - refusing to overwrite good data with an empty default.",
            display_absolute(path)
        ),
    }
}

/// Best-effort absolute rendering of `path` for panic/log messages - the
/// message must name where the file actually lives even when a caller
/// passes a CWD-relative literal like "adventure-characters.json".
fn display_absolute(path: &Path) -> String {
    if path.is_absolute() {
        path.display().to_string()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path).display().to_string())
            .unwrap_or_else(|_| path.display().to_string())
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

#[cfg(test)]
mod fail_loud_load_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch_path(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("pod_fail_loud_{label}_{}_{}.json", std::process::id(), unique));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// ABSENT -> default. The fresh-install case: several tests and every
    /// brand-new deployment construct state against a directory with no
    /// save files yet, and that must stay a clean `None`, never a panic.
    #[test]
    fn absent_file_returns_none_instead_of_panicking() {
        let path = scratch_path("absent");
        assert_eq!(load_json_fail_loud::<serde_json::Value>(&path), None);
    }

    /// PRESENT + parses -> the value, the normal load path unchanged.
    #[test]
    fn present_and_valid_file_parses_normally() {
        let path = scratch_path("valid");
        std::fs::write(&path, r#"{"stage": 42}"#).expect("scratch fixture must be writable");
        let parsed: Option<serde_json::Value> = load_json_fail_loud(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(parsed, Some(serde_json::json!({ "stage": 42 })));
    }

    /// PRESENT + BOM -> warning, then loads anyway. `serde_json` rejects
    /// U+FEFF outright, and a BOM'd-but-valid file must not trip the
    /// refuse-to-start path - its JSON is fine. (The warning itself is
    /// emitted via `tracing::warn!`; this crate has no log-capture test
    /// infrastructure, so success of the parse is the asserted half.)
    #[test]
    fn bom_prefixed_file_still_parses() {
        let path = scratch_path("bom");
        std::fs::write(&path, format!("\u{feff}{}", r#"{"ok": true}"#)).expect("scratch fixture must be writable");
        let parsed: Option<serde_json::Value> = load_json_fail_loud(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(parsed, Some(serde_json::json!({ "ok": true })));
    }

    /// PRESENT + truncated JSON -> panic, carrying the SPECIFIC serde error
    /// (not a generic "bad file") so the message says what is actually wrong.
    #[test]
    #[should_panic(expected = "EOF while parsing")]
    fn truncated_json_panics_carrying_the_serde_error() {
        let path = scratch_path("truncated");
        std::fs::write(&path, r#"{"stage": 42"#).expect("scratch fixture must be writable");
        let _: Option<serde_json::Value> = load_json_fail_loud(&path);
    }

    /// PRESENT + garbage -> panic naming the ABSOLUTE path, so the message
    /// points at the exact file to fix or restore from backup.
    #[test]
    #[should_panic(expected = "pod_fail_loud_pathname")]
    fn malformed_json_panics_naming_the_absolute_path() {
        let path = scratch_path("pathname");
        std::fs::write(&path, "not json at all").expect("scratch fixture must be writable");
        let _: Option<serde_json::Value> = load_json_fail_loud(&path);
    }

    /// The guidance half of the contract, on its own so a message rewording
    /// can't silently drop it: fix it or restore from backup, refusing to
    /// overwrite.
    #[test]
    #[should_panic(expected = "restore it from backup - refusing to overwrite")]
    fn parse_failure_panic_carries_the_fix_or_restore_guidance() {
        let path = scratch_path("guidance");
        std::fs::write(&path, "[").expect("scratch fixture must be writable");
        let _: Option<serde_json::Value> = load_json_fail_loud(&path);
    }

    /// PRESENT + unreadable (a directory sitting where the file should be)
    /// -> panic, not a silent default: present-but-unusable is not "absent".
    #[test]
    #[should_panic(expected = "could not be read")]
    fn present_but_unreadable_file_panics_instead_of_defaulting() {
        let path = scratch_path("unreadable");
        std::fs::create_dir_all(&path).expect("scratch dir must be creatable");
        let _: Option<serde_json::Value> = load_json_fail_loud(&path);
    }
}
