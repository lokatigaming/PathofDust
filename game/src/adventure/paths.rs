// Configurable persistence root (2026-08-18, architecture refactor Stage
// 1 - "make configurable persistence paths an explicit design input,"
// the owner's own words) - every adventure-game file on disk (character
// saves, world state, fight-history tiers, live-tunables/item-balance
// TOML, one-time migration markers, ...) was previously a bare literal
// like "adventure-characters.json", resolved implicitly relative to
// whatever the process's current working directory happened to be. That
// made it impossible to run a second, disposable AdventureManager
// pointed at copied/fixture data without either colliding with the real
// files or hand-patching every call site - exactly the gap that blocked
// Stage 0.5's harness #3 (HTTP golden-response) and the POST-route
// baselines.
//
// Deliberately NOT added to `state.rs`: that module's `load_json`/
// `save_json` are shared with plenty of bot-side files too (tokens.json,
// commands.json, tips-history.json, ...) - auto-prepending a game-data
// root there would silently redirect THOSE too, an unrelated data domain
// that should stay under the process's own CWD regardless of where game
// data lives. This lives inside `adventure::` instead, and every
// `state::load_json`/`save_json`/direct-`std::fs` call site in this
// module tree explicitly wraps its path through `data_path` - a little
// more typing per call site, but zero risk of an unrelated module's
// files moving as a side effect.
use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the base directory every adventure-game persisted file resolves
/// relative to (see `data_path`). Call ONCE, before constructing an
/// `AdventureManager` or touching any game persistence - `AdventureManager::new`
/// itself doesn't call this for you, so the caller (main.rs today, a
/// future standalone `game` binary's own entry point, or a test harness
/// spinning up a disposable instance) controls it explicitly. Returns
/// `false` and leaves the existing value in place if already set - the
/// data directory is fixed for the process's whole lifetime, not
/// something that moves mid-run.
pub fn set_data_dir(dir: PathBuf) -> bool {
    DATA_DIR.set(dir).is_ok()
}

/// Joins `filename` onto the configured data directory. Never explicitly
/// set (every caller today - `set_data_dir` isn't wired into main.rs
/// yet, deliberately: this stage adds the mechanism, not a behavior
/// change) falls back to an EMPTY base path, so `data_path("x")` is
/// byte-for-byte identical to the bare literal `"x"` it replaces -
/// today's exact CWD-relative resolution, unchanged.
pub(crate) fn data_path(filename: &str) -> PathBuf {
    DATA_DIR.get_or_init(PathBuf::new).join(filename)
}

// No automated `#[cfg(test)]` coverage in this file, deliberately - a
// real functional check of `set_data_dir` (call it, then confirm
// `data_path` redirects) was tried here and then removed: this crate's
// test binary runs every test in ONE process across many threads, and
// `data_path`'s `OnceLock` is genuinely global to that process - item
// generation (`generate_item`/`generate_item_at_tier`, exercised by
// `golden_corpus.rs`'s tests among others) lazily loads the item-
// balance file on first use via this SAME `data_path`, so ANY test in
// the suite can race to be the "first" caller and permanently decide
// what `DATA_DIR` locks in as, regardless of execution order. A test
// asserting "my call is the first" is therefore inherently flaky, not
// just unlucky - confirmed live (failed consistently once another
// test's item generation reliably won the race). Verified functionally
// instead, once, outside the normal suite (set_data_dir to a scratch
// temp dir, wrote through data_path, confirmed the file landed there
// and not at the CWD-relative default, confirmed a second set_data_dir
// call correctly reports failure and leaves the first value in place) -
// see the Stage 1 commit message for the full verification transcript.
// The "never set resolves byte-identical to the bare literal" half
// needs no runtime test at all: `DATA_DIR.get_or_init(PathBuf::new).join(filename)`
// is `filename` exactly, by inspection.
