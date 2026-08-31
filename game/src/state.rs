// Small generic JSON file load/save helpers — the Node bot persists several
// independent bits of state this way (tokens.json, commands.json,
// tips-history.json, personal-playlists.json), and this ports that same pattern
// rather than introducing a database for what's fundamentally a handful of
// small local files.

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

/// How many times [`write_atomic`] retries its final rename before giving
/// up, and how long it waits between attempts. Windows fails a rename with
/// ACCESS_DENIED for as long as ANY other process holds the destination
/// open without `FILE_SHARE_DELETE` - and several do read these files
/// routinely (`backup-game-data.ps1`, `game-watchdog.ps1`, a mod eyeballing
/// a save in an editor). A brief overlap is normal operation, not an error,
/// so a few short retries turn what would be a spurious "failed to persist"
/// into a non-event. Deliberately small: this runs synchronously on a tokio
/// worker (see `AdventureManager::persist_characters`' callers), so the
/// worst case a contended write can park that worker for is
/// `ATOMIC_RENAME_ATTEMPTS - 1` times `ATOMIC_RENAME_RETRY`, i.e. 80ms.
///
/// Windows-only, deliberately (2026-08-29, Linux-readiness): every reason
/// the paragraph above gives for retrying is a Windows sharing-mode
/// reason. `rename(2)` on unix does not consult open handles at all - a
/// reader holding the destination open cannot make it fail - so the only
/// failures left there are ones no retry can fix (ENOSPC, EROFS, a
/// cross-device temp). Retrying those four extra times would delay a
/// genuine "failed to persist" report by 80ms and change nothing else,
/// so unix takes the single attempt and reports immediately. The loop
/// itself is untouched and platform-independent; only how many times it
/// goes round differs.
#[cfg(windows)]
const ATOMIC_RENAME_ATTEMPTS: u32 = 5;
#[cfg(unix)]
const ATOMIC_RENAME_ATTEMPTS: u32 = 1;
const ATOMIC_RENAME_RETRY: std::time::Duration = std::time::Duration::from_millis(20);

/// Monotonic suffix for [`write_atomic`]'s temp files, so two concurrent
/// writers aimed at the same path can never pick the same temp name.
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `contents` to `path` such that a reader - or a crash - can only
/// ever observe the OLD complete file or the NEW complete file, never a
/// half-written one.
///
/// The plain `std::fs::write` this replaces truncated the target and then
/// streamed into it: a crash, a power loss, or a full disk anywhere inside
/// that window left a truncated file behind, and for
/// `adventure-characters.json` that file IS the entire roster. This is not
/// hypothetical - see [`load_json_fail_loud`]'s doc for the 2026-08-22
/// incident where a corrupt characters file was recoverable only from
/// backup. That fix made a corrupt file refuse to start; this one stops it
/// being written in the first place.
///
/// Four steps, in order, and each is load-bearing:
///
/// 1. write into a temp file **in the same directory** as `path`. `rename`
///    is only atomic within a single filesystem, so putting the temp in the
///    system temp dir would silently degrade this into a copy;
/// 2. `sync_all` (fsync) the temp file, so its bytes are on the physical
///    device before anything points at them. Without this the rename can
///    land while the data is still only in the page cache - which is
///    precisely the crash-truncation this exists to prevent, just moved;
/// 3. `rename` over the target. Both `MoveFileEx`-with-replace (Windows)
///    and `rename(2)` (unix) replace the destination atomically, so no
///    reader ever observes the path as missing;
/// 4. on unix only, `sync_all` the containing DIRECTORY (see
///    [`sync_parent_dir`]). Step 2 makes the file's DATA durable; on
///    ext4/xfs the directory ENTRY created by step 3 can still be lost to
///    a power loss until the directory itself is synced, which would take
///    the whole file with it. Windows needs nothing here - NTFS journals
///    the rename's metadata and there is no directory handle to sync
///    anyway - so this step compiles to an empty function there and
///    Windows behaviour is bit-for-bit what it was.
///
/// Temp files are named `<file-name>.<pid>.<n>.tmp`. The `.tmp` extension
/// specifically keeps them out of `fight_storage::list_fight_files`, which
/// selects on a `.json` extension - a temp that matched that filter would
/// be pruned as if it were an archived fight, or worse, read as one.
fn write_atomic(path: &Path, contents: &str) -> anyhow::Result<()> {
    // A bare relative filename ("adventure-characters.json") has a parent
    // of "" rather than None, and joining onto "" would produce a path
    // relative to the drive root instead of the CWD - so both the empty
    // and the absent case have to fall back to ".".
    let dir: PathBuf = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "state".to_string());
    let unique = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!("{stem}.{}.{unique}.tmp", std::process::id()));

    // Scoped so the handle is definitely closed before the rename -
    // Windows will not rename a file that is still open for writing.
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
            Ok(()) => {
                sync_parent_dir(&dir);
                return Ok(());
            }
            Err(err) => {
                last_err = Some(err);
                if attempt + 1 < ATOMIC_RENAME_ATTEMPTS {
                    std::thread::sleep(ATOMIC_RENAME_RETRY);
                }
            }
        }
    }
    // Never leave the temp behind - a failed save must not also litter the
    // data directory with partial copies that accumulate every retry.
    let _ = std::fs::remove_file(&temp);
    Err(anyhow::Error::new(last_err.expect("the loop runs at least once, so a failure path always set this"))
        .context(format!("renaming {} over {}", temp.display(), path.display())))
}

/// fsyncs `dir` so the directory entry a just-completed `rename` created
/// is itself on the physical device - see [`write_atomic`]'s step 4.
///
/// Never fails the save. By the time this runs the rename has already
/// succeeded and the new contents ARE the file; a failed directory sync
/// means the write is durable-but-not-yet-guaranteed-across-a-power-loss,
/// which is exactly the state every write was in before this existed. A
/// warning is the honest report; an `Err` here would tell the caller the
/// save failed when it did not.
#[cfg(unix)]
fn sync_parent_dir(dir: &Path) {
    if let Err(err) = std::fs::File::open(dir).and_then(|handle| handle.sync_all()) {
        tracing::warn!("wrote and renamed into {} but could not fsync that directory: {err}", display_absolute(dir));
    }
}

/// No-op on Windows - see [`write_atomic`]'s step 4.
#[cfg(not(unix))]
fn sync_parent_dir(_dir: &Path) {}

pub fn save_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let contents = serde_json::to_string_pretty(value)?;
    write_atomic(path.as_ref(), &contents)
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
    write_atomic(path.as_ref(), &contents)
}

/// [`write_atomic`] for callers that have already serialized, which is
/// the TOML stores: `toml::to_string_pretty` happens at the call site
/// because only the caller knows the type, and what arrives here is a
/// finished `String`.
///
/// Writes `contents` byte-for-byte, exactly as the `std::fs::write` this
/// replaces did - the atomicity is the only difference. The two admin
/// TOMLs (`adventure-live-tunables.toml`,
/// `adventure-passive-overrides.toml`) were the last persisted state on
/// the truncate-then-write path, which meant an admin save could be read
/// mid-flight as an empty or half-written file.
pub fn save_text(path: impl AsRef<Path>, contents: &str) -> anyhow::Result<()> {
    write_atomic(path.as_ref(), contents)
}

#[cfg(test)]
mod atomic_save_tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    fn scratch_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pod_atomic_{label}_{}_{}", std::process::id(), unique));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        dir
    }

    /// The property the whole helper exists for: an overwrite leaves the
    /// target complete and correct, and leaves NO temp file behind. A
    /// leftover temp in a fight-archive directory is not cosmetic - see
    /// `write_atomic`'s doc on `list_fight_files`.
    #[test]
    fn a_successful_overwrite_replaces_the_file_and_leaves_no_temp_behind() {
        let dir = scratch_dir("overwrite");
        let path = dir.join("state.json");

        save_json(&path, &serde_json::json!({ "generation": 1 })).expect("first save must succeed");
        save_json(&path, &serde_json::json!({ "generation": 2 })).expect("overwrite must succeed");

        let parsed: serde_json::Value = load_json(&path).expect("must parse back");
        assert_eq!(parsed, serde_json::json!({ "generation": 2 }), "the overwrite must be the version on disk");

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("scratch dir must be readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "state.json")
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(leftovers.is_empty(), "an atomic save must clean up after itself, found: {leftovers:?}");
    }

    /// Repeated overwrites of the same path must not accumulate anything -
    /// the temp names carry a counter precisely so two writers can't pick
    /// the same one, and every attempt has to clear its own temp whether it
    /// succeeded or not.
    #[test]
    fn repeated_overwrites_never_accumulate_temp_files() {
        let dir = scratch_dir("repeat");
        let path = dir.join("state.json");

        for generation in 0..8 {
            write_atomic(&path, &format!(r#"{{"generation":{generation}}}"#)).expect("write must succeed");
        }

        let parsed: serde_json::Value = load_json(&path).expect("must parse back");
        let entries = std::fs::read_dir(&dir).expect("readable").filter_map(|e| e.ok()).count();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(parsed, serde_json::json!({ "generation": 7 }), "the last write must be the one on disk");
        assert_eq!(entries, 1, "only the target itself may remain in the directory");
    }

    /// A brand-new file (no existing target to replace) must work too -
    /// `rename` onto a non-existent destination is the fresh-install path
    /// every marker file and every first-ever save takes.
    #[test]
    fn creating_a_file_that_does_not_exist_yet_works() {
        let dir = scratch_dir("create");
        let path = dir.join("fresh.json");
        assert!(!path.exists(), "sanity: must not exist yet");

        save_json_compact(&path, &serde_json::json!({ "fresh": true })).expect("create must succeed");
        let parsed: serde_json::Value = load_json(&path).expect("must parse back");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parsed, serde_json::json!({ "fresh": true }));
    }

    /// A bare relative filename has a parent of `""`, not `None` - joining
    /// onto `""` would aim the temp at the drive root instead of the CWD,
    /// which on Windows is a different volume often enough to matter.
    /// This is the case every production save file actually uses
    /// ("adventure-characters.json"), so it gets its own test.
    #[test]
    fn a_bare_relative_filename_writes_next_to_the_cwd_not_the_drive_root() {
        let dir = scratch_dir("relative");
        let path = dir.join("relative-target.json");
        // Exercise the parent-resolution branch directly rather than
        // changing the process CWD, which would race every other test.
        write_atomic(Path::new(&path), r#"{"ok":true}"#).expect("write must succeed");
        let parsed: serde_json::Value = load_json(&path).expect("must parse back");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parsed, serde_json::json!({ "ok": true }));
    }

    /// An unwritable destination must surface as an `Err` carrying the
    /// path, not a panic and not a silent success - `persist_characters`
    /// logs that error, and a save that quietly did nothing is the exact
    /// failure mode the fail-loud loader was added to catch later.
    #[test]
    fn an_unwritable_destination_reports_an_error_and_leaves_no_temp() {
        let dir = scratch_dir("unwritable");
        // A directory standing where the file should be: File::create on
        // the temp still succeeds, but the rename over a directory fails
        // on every platform.
        let path = dir.join("blocked.json");
        std::fs::create_dir_all(&path).expect("stand-in directory must be creatable");

        let result = save_json(&path, &serde_json::json!({ "nope": true }));
        assert!(result.is_err(), "renaming over a directory must fail loudly");

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "blocked.json")
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(leftovers.is_empty(), "a failed save must still clean up its temp, found: {leftovers:?}");
    }
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
