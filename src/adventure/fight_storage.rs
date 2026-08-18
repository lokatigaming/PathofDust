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
pub(crate) const SUMMARY_FIGHTS_CAPACITY: usize = 200;

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
pub(crate) fn recent_summary_fights(limit: usize) -> Vec<FightSummarySnapshot> {
    read_recent(SUMMARY_FIGHTS_DIR, limit)
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
