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
/// set (the production default - `main.rs` calls `set_data_dir` only when
/// `GAME_DATA_DIR` is present in the environment, and it is not set in
/// production today) falls back to an EMPTY base path, so `data_path("x")`
/// is byte-for-byte identical to the bare literal `"x"` it replaces -
/// today's exact CWD-relative resolution, unchanged.
///
/// `pub` rather than `pub(crate)` as of 2026-08-29 (Linux-readiness): the
/// `game` binary is its own crate and has two paths of its own to resolve
/// through here (`logs/` and the wings-giveaway marker). Purely additive.
/// An absolute `filename` still wins outright - `Path::join` replaces
/// rather than appends - which is what keeps every test that passes its
/// own scratch path working regardless of what `DATA_DIR` holds.
pub fn data_path(filename: &str) -> PathBuf {
    DATA_DIR.get_or_init(PathBuf::new).join(filename)
}

/// A one-time migration marker, resolved BESIDE the characters file the
/// manager was handed rather than through the global `data_path`
/// (2026-09-08).
///
/// THE DEFECT THIS FIXES. Markers resolved through `data_path` with
/// `DATA_DIR` unset land in the process's CWD. An in-crate
/// `disposable_manager` isolates its characters/world/cooldown files by
/// absolute scratch path but had no way to isolate its MARKER namespace,
/// so every such manager wrote its markers into the repo working
/// directory instead - 58 stray entries across two directories in this
/// checkout when it was found, gitignored and therefore invisible.
///
/// It is a correctness bug rather than untidiness because **markers
/// persist between runs**. Once the suite has run once on a machine,
/// every later in-crate `AdventureManager::new` boots with every startup
/// migration already marked done. A test of a startup backfill passes on
/// a clean checkout and behaves differently on a machine that has run the
/// suite before - history-dependent, not merely order-dependent - and it
/// fails in the silent direction, because the backfill is SKIPPED and
/// nothing errors.
///
/// WHY THE PARENT OF THE CHARACTERS PATH IS THE RIGHT ANCHOR. That path
/// has already been through `data_path` by the time a manager holds it,
/// so in production its parent IS the data directory and markers land
/// byte-identically where they land today. In an in-crate test it is an
/// absolute scratch path, so markers follow the characters file into the
/// scratch directory - with no `set_data_dir` call and no `OnceLock`
/// involvement, which is what keeps this usable from a test binary that
/// shares its process with the whole suite.
///
/// A path with no parent falls back to the bare name, which is exactly
/// what `data_path` with an unset `DATA_DIR` would have produced.
pub fn marker_path(characters_path: &std::path::Path, filename: &str) -> PathBuf {
    characters_path.parent().map_or_else(|| PathBuf::from(filename), |dir| dir.join(filename))
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
