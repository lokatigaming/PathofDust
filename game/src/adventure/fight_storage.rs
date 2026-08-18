// Per-fight-file storage for the rolling fight-history log (2026-08-17,
// part of the full-detail combat log plan) - replaces the old single
// `adventure-last-fights.json` blob (a single ever-growing JSON array,
// fully read+deserialized+rewritten on EVERY fight save, and fully
// read+parsed on every `/fights` request with no offloading - a real,
// already-live scaling problem confirmed at 340MB on disk before any of
// this). Every fight is now its own small file in its own tier
// directory; saving a fight is "write one new file, delete the oldest if
// over capacity" - O(1) per fight, never a read-modify-rewrite of
// anything that already exists on disk.
//
// Two parallel tiers, identical mechanism, different capacity (both
// lowered 2026-08-17, Phase 2, once real file sizes were measured - see
// `COARSE_FIGHTS_CAPACITY`/`DETAIL_FIGHTS_CAPACITY`'s own docs):
// - COARSE: replaces the old log 1:1 (same `LastFightSnapshot` shape) -
//   what `/fights` and `recent_fights()` read.
// - DETAIL: the new full-roll-granularity log (see `RollEvent` in
//   `manager.rs`), retained for a much smaller recent window only -
//   full detail isn't needed/affordable for the whole coarse-tier
//   history, just "what just happened."

use super::*;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

pub(crate) const COARSE_FIGHTS_DIR: &str = "adventure-fights-coarse";
pub(crate) const DETAIL_FIGHTS_DIR: &str = "adventure-fights-detail";
pub(crate) const SUMMARY_FIGHTS_DIR: &str = "adventure-fights-summary";
const COARSE_SEQ_PATH: &str = "adventure-fights-coarse-seq.json";
const DETAIL_SEQ_PATH: &str = "adventure-fights-detail-seq.json";
const SUMMARY_SEQ_PATH: &str = "adventure-fights-summary-seq.json";

/// Lowered 100 -> 10 -> 5 (2026-08-17 Phase 2, then again 2026-08-18)
/// as real on-disk sizes kept outrunning the estimates: the Phase 2 cut
/// was made against 49MB detail files, but by the next day a single
/// heavily-built multi-boss late-game fight was producing ~245MB coarse
/// / ~620MB detail, so the 10/5 caps were still holding ~5.5GB between
/// them. "We don't need long logs, just efficient ones" - a live
/// decision to keep both tiers small rather than retain a long history.
/// The summary tier below is what actually serves player-facing fight
/// history, so shrinking these two costs nothing a player can see.
pub(crate) const COARSE_FIGHTS_CAPACITY: usize = 5;
/// Full roll-level detail is expensive relative to the coarse tier (see
/// `RollEvent`'s own doc) - retained for a much smaller recent window,
/// "what just happened" rather than the full coarse-tier history.
/// Always kept BELOW `COARSE_FIGHTS_CAPACITY` (15 -> 5 -> 3, lowered
/// alongside it each time) since a detail file runs several times the
/// size of the coarse file for the same fight.
pub(crate) const DETAIL_FIGHTS_CAPACITY: usize = 3;
/// A summary is just per-player aggregates + loot/broken - a few KB
/// regardless of how many events the real fight generated (2026-08-18,
/// the `/fights.json` size/latency fix) - roughly 100-1000x smaller than
/// a coarse file, so a much longer retention window than
/// `COARSE_FIGHTS_CAPACITY` costs about what 1-2 coarse files do. This
/// also gives `/fights.json`'s own `?limit=` a genuinely useful range
/// again, independent of the coarse tier's own much smaller retention.
pub const SUMMARY_FIGHTS_CAPACITY: usize = 200;

/// Bumps and persists the next sequence number for a tier, used to name
/// that fight's own file - zero-padded so plain filename sorting is
/// already chronological, and a real persisted counter (not inferred
/// from directory contents) so there's no ambiguity/collision risk even
/// across restarts or after pruning has deleted old low-numbered files.
fn next_seq(seq_path: &str) -> u64 {
    let current: u64 = crate::state::load_json(seq_path).unwrap_or(0);
    let next = current + 1;
    if let Err(err) = crate::state::save_json(seq_path, &next) {
        tracing::error!("Failed to persist fight sequence counter to {seq_path}: {err}");
    }
    next
}

fn fight_file_path(dir: &str, seq: u64) -> PathBuf {
    Path::new(dir).join(format!("fight-{seq:010}.json"))
}

/// Every `.json` file currently in `dir`, sorted ascending by filename
/// (oldest first, since filenames are zero-padded sequence numbers) - the
/// shared listing both pruning and reading build off. An empty Vec (not
/// an error) if `dir` doesn't exist yet - the normal state before this
/// tier has ever saved a fight.
fn list_fight_files(dir: &str) -> Vec<PathBuf> {
    let Ok(read_dir) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<PathBuf> =
        read_dir.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|ext| ext == "json")).collect();
    entries.sort();
    entries
}

/// Writes `value` as a brand-new fight file in `dir` (next sequence
/// number), then deletes the oldest file(s) if that pushed the tier over
/// `capacity`. This one call is the entire cost of saving a fight to a
/// tier - no read of anything that already exists.
pub(crate) fn write_and_prune<T: Serialize>(dir: &str, seq_path: &str, capacity: usize, value: &T) {
    if let Err(err) = std::fs::create_dir_all(dir) {
        tracing::error!("Failed to create fight log directory {dir}: {err}");
        return;
    }
    let seq = next_seq(seq_path);
    let path = fight_file_path(dir, seq);
    if let Err(err) = crate::state::save_json(&path, value) {
        tracing::error!("Failed to persist fight file {}: {err}", path.display());
        return;
    }
    let files = list_fight_files(dir);
    if files.len() > capacity {
        for old in &files[..files.len() - capacity] {
            if let Err(err) = std::fs::remove_file(old) {
                tracing::error!("Failed to prune old fight file {}: {err}", old.display());
            }
        }
    }
}

/// Reads the most recent `limit` fight files from `dir`, newest first -
/// only ever opens/parses the files actually needed, never the tier's
/// whole history (this is what fixes `/fights`' old whole-file blocking
/// read). A corrupt/unparseable individual file is skipped (logged, not
/// fatal) rather than failing the whole read - same fail-soft spirit as
/// every other `state::load_json` caller in this codebase.
pub(crate) fn read_recent<T: DeserializeOwned>(dir: &str, limit: usize) -> Vec<T> {
    let mut files = list_fight_files(dir);
    files.reverse(); // newest (highest sequence number) first
    files.into_iter().take(limit).filter_map(|path| crate::state::load_json(&path)).collect()
}

/// Total number of fight files currently in `dir` - used by the one-time
/// storage migration to confirm the split succeeded before touching the
/// original file.
pub(crate) fn count_fight_files(dir: &str) -> usize {
    list_fight_files(dir).len()
}

pub(crate) fn save_coarse_fight(snapshot: &LastFightSnapshot) {
    write_and_prune(COARSE_FIGHTS_DIR, COARSE_SEQ_PATH, COARSE_FIGHTS_CAPACITY, snapshot);
}

pub(crate) fn save_detail_fight(detail: &DetailFightSnapshot) {
    write_and_prune(DETAIL_FIGHTS_DIR, DETAIL_SEQ_PATH, DETAIL_FIGHTS_CAPACITY, detail);
}

/// Reads up to `limit` most recent coarse-tier fights, newest first -
/// what `/fights`/`recent_fights()` actually reads. Never more than
/// `limit` files are opened, regardless of how many exist in the tier
/// (up to `COARSE_FIGHTS_CAPACITY`) - the fix for the old single-blob
/// log's whole-file read on every request.
pub(crate) fn recent_coarse_fights(limit: usize) -> Vec<LastFightSnapshot> {
    read_recent(COARSE_FIGHTS_DIR, limit)
}

pub(crate) fn save_summary_fight(summary: &FightSummarySnapshot) {
    write_and_prune(SUMMARY_FIGHTS_DIR, SUMMARY_SEQ_PATH, SUMMARY_FIGHTS_CAPACITY, summary);
}

/// Reads up to `limit` most recent fight summaries, newest first - what
/// `/fights.json` reads instead of the full coarse-tier snapshot (see
/// `fight_summaries_for_viewer` in `adventure_web.rs`).
pub fn recent_summary_fights(limit: usize) -> Vec<FightSummarySnapshot> {
    read_recent(SUMMARY_FIGHTS_DIR, limit)
}

/// Preserved evidence for a bug report (2026-08-18, `!pinfight`) -
/// COPIES (never moves - the originals stay right where they are and
/// keep aging out on the tiers' own normal schedule) the current most
/// recent coarse-tier and detail-tier fight files here. `write_and_prune`
/// only ever looks inside `COARSE_FIGHTS_DIR`/`DETAIL_FIGHTS_DIR`
/// themselves for `.json` files to prune (see `list_fight_files`), so a
/// SEPARATE directory nothing ever prunes from is enough on its own -
/// no allowlist/exclusion logic needed anywhere else. Disk only grows
/// here when a mod deliberately chooses to pin something, not on a
/// schedule.
pub(crate) const PINNED_FIGHTS_DIR: &str = "adventure-fights-pinned";

/// What `pin_most_recent_fight` actually managed to preserve - either
/// tier can independently be empty very early after a restart, before
/// that tier has saved its first fight yet, so this is reported rather
/// than assumed.
pub struct PinnedFight {
    /// The shared per-fight sequence number (`save_last_fight` bumps
    /// both tiers' counters together for every fight, so a fight's
    /// coarse and detail files always carry the same number) - `None`
    /// only if the filename itself didn't parse as `fight-<seq>.json`,
    /// which shouldn't happen for anything `fight_file_path` wrote.
    pub sequence: Option<u64>,
    pub coarse_pinned: bool,
    pub detail_pinned: bool,
}

/// Pulls the sequence number back out of a `fight-<seq>.json` path (see
/// `fight_file_path`) - just string parsing, not a stored value, since
/// nothing before this needed to go the other direction.
fn fight_seq_from_path(path: &Path) -> Option<u64> {
    path.file_stem()?.to_str()?.strip_prefix("fight-")?.parse().ok()
}

/// Copies `source` into `PINNED_FIGHTS_DIR`, prefixed with `tier` (e.g.
/// `coarse-fight-0000000123.json`) so a mod (or a future !pinfight run)
/// can tell at a glance which tier a pinned file came from without
/// opening it - the two tiers' own sequence counters aren't shared, so
/// without the prefix a coarse and a detail file from the SAME fight
/// would otherwise land under the exact same name.
fn copy_pinned(tier: &str, source: &Path) -> bool {
    let Some(file_name) = source.file_name() else { return false };
    let dest = Path::new(PINNED_FIGHTS_DIR).join(format!("{tier}-{}", file_name.to_string_lossy()));
    match std::fs::copy(source, &dest) {
        Ok(_) => true,
        Err(err) => {
            tracing::error!("Failed to pin fight file {} -> {}: {err}", source.display(), dest.display());
            false
        }
    }
}

/// `!pinfight`'s whole implementation - see `PINNED_FIGHTS_DIR`'s doc for
/// why a plain copy into its own directory is sufficient protection from
/// pruning. `None` if NEITHER tier has any file at all yet (nothing to
/// pin - a fresh install/restart before the first fight has landed).
pub fn pin_most_recent_fight() -> Option<PinnedFight> {
    if let Err(err) = std::fs::create_dir_all(PINNED_FIGHTS_DIR) {
        tracing::error!("Failed to create pinned-fights directory {PINNED_FIGHTS_DIR}: {err}");
        return None;
    }
    let coarse = list_fight_files(COARSE_FIGHTS_DIR).pop();
    let detail = list_fight_files(DETAIL_FIGHTS_DIR).pop();
    if coarse.is_none() && detail.is_none() {
        return None;
    }
    let sequence = coarse.as_deref().and_then(fight_seq_from_path).or_else(|| detail.as_deref().and_then(fight_seq_from_path));
    let coarse_pinned = coarse.as_deref().is_some_and(|p| copy_pinned("coarse", p));
    let detail_pinned = detail.as_deref().is_some_and(|p| copy_pinned("detail", p));
    tracing::info!(
        "!pinfight: pinned fight #{} (coarse: {coarse_pinned}, detail: {detail_pinned}) to {PINNED_FIGHTS_DIR}",
        sequence.map(|s| s.to_string()).unwrap_or_else(|| "?".to_string())
    );
    Some(PinnedFight { sequence, coarse_pinned, detail_pinned })
}

/// Every file currently sitting in `PINNED_FIGHTS_DIR`, newest first -
/// what the admin page's "Pinned Fights" section shows (see
/// `render_tunables_page`) so a mod can confirm a `!pinfight` actually
/// landed without spelunking the filesystem.
pub fn list_pinned_fights() -> Vec<String> {
    let Ok(read_dir) = std::fs::read_dir(PINNED_FIGHTS_DIR) else { return Vec::new() };
    let mut names: Vec<String> = read_dir.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect();
    names.sort();
    names.reverse();
    names
}

const STORAGE_MIGRATION_MARKER_PATH: &str = "adventure-fights-storage-migration-marker.json";

/// One-time migration (2026-08-17, full-detail combat log storage
/// prerequisite) - explodes the old single-blob `adventure-last-fights.json`
/// (a single ever-growing `Vec<LastFightSnapshot>`, confirmed at 340MB on
/// disk, fully read+deserialized+rewritten on every fight save) into
/// individual coarse-tier files, one per fight, via the exact same
/// `write_and_prune` mechanism every future fight save now uses. Reads
/// the old file exactly once. Old log is newest-first (index 0 = most
/// recent); written out oldest-to-newest so the new tier's sequence
/// numbers (and therefore `read_recent`'s own newest-first ordering)
/// come out correct. On success, RENAMES (not deletes) the original to
/// `.bak` - the 340MB becomes reclaimable but nothing is destroyed
/// outright. Marker-gated, same fire-once shape as every other
/// migration in this codebase (see `migrations.rs`).
pub(crate) fn run_storage_migration() {
    if crate::state::load_json::<bool>(STORAGE_MIGRATION_MARKER_PATH).is_some() {
        return;
    }
    if let Some(old_log) = crate::state::load_json::<Vec<LastFightSnapshot>>(LAST_FIGHTS_LOG_PATH) {
        for snapshot in old_log.into_iter().rev() {
            save_coarse_fight(&snapshot);
        }
        let migrated = count_fight_files(COARSE_FIGHTS_DIR);
        tracing::info!("Fight storage migration: split {LAST_FIGHTS_LOG_PATH} into {migrated} coarse-tier files");
        let backup_path = format!("{LAST_FIGHTS_LOG_PATH}.bak");
        if let Err(err) = std::fs::rename(LAST_FIGHTS_LOG_PATH, &backup_path) {
            tracing::error!("Fight storage migration: failed to rename {LAST_FIGHTS_LOG_PATH} to {backup_path}: {err}");
        }
    }
    if let Err(err) = crate::state::save_json(STORAGE_MIGRATION_MARKER_PATH, &true) {
        tracing::error!("Failed to persist fight storage migration marker to {STORAGE_MIGRATION_MARKER_PATH}: {err}");
    }
}

#[cfg(test)]
mod pin_most_recent_fight_tests {
    use super::*;

    // Only `fight_seq_from_path` is unit-tested here - it's the one pure
    // piece of `!pinfight`'s logic. Every other fn in this module reads/
    // writes the real, fixed-path COARSE_FIGHTS_DIR/DETAIL_FIGHTS_DIR/
    // PINNED_FIGHTS_DIR constants directly (no injectable directory
    // param), same as every other fn in this file - there's no existing
    // precedent anywhere in fight_storage.rs for sandboxing that against
    // a temp dir, so this doesn't invent one just for the new code.
    // Verified live instead: post-deploy, run !pinfight for real and
    // check both the chat reply and the /admin/tunables dashboard
    // section.

    #[test]
    fn parses_the_sequence_number_out_of_a_real_fight_filename() {
        let path = Path::new("adventure-fights-coarse").join("fight-0000000123.json");
        assert_eq!(fight_seq_from_path(&path), Some(123));
    }

    #[test]
    fn rejects_anything_that_is_not_the_fight_dash_number_shape() {
        assert_eq!(fight_seq_from_path(Path::new("not-a-fight-file.json")), None);
        assert_eq!(fight_seq_from_path(Path::new("fight-not-a-number.json")), None);
        assert_eq!(fight_seq_from_path(Path::new("fight-.json")), None);
    }
}
