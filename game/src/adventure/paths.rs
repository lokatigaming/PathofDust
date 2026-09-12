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

use super::stores::Store;
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

/// Resolves a STORE to its path under the configured data directory.
///
/// Takes a `Store` and nothing else (ruling 2026-09-08). It used to take a
/// `&str`, which meant any string could become a persisted path with
/// nothing forcing it to be classified; now a new store cannot be resolved
/// to a path at all until it is a variant in `stores.rs` with a scope and a
/// reason. For a path the caller already supplied, see
/// `normalize_caller_path` - which takes a `&Path` and cannot name a store.
///
/// With `DATA_DIR` never set (the production default - `main.rs` calls
/// `set_data_dir` only when `GAME_DATA_DIR` is present, and it is not set
/// in production today) the base is an EMPTY path, so this is byte-for-byte
/// the CWD-relative resolution the bare literals always had.
///
/// `pub` rather than `pub(crate)` as of 2026-08-29 (Linux-readiness): the
/// `game` binary is its own crate and has two paths of its own to resolve
/// through here (`logs/` and the wings-giveaway marker). Purely additive.
/// An absolute `filename` still wins outright - `Path::join` replaces
/// rather than appends - which is what keeps every test that passes its
/// own scratch path working regardless of what `DATA_DIR` holds.
pub fn data_path(store: Store) -> PathBuf {
    DATA_DIR.get_or_init(PathBuf::new).join(store.name())
}

/// Resolves a path the CALLER already supplied, rather than a store name.
///
/// THE SPLIT (ruling 2026-09-08). `data_path` used to take a `&str`, which
/// meant any string could become a persisted path and nothing forced it to
/// be classified. It now takes a `Store` and nothing else. This function
/// is the other half of that split, and it is deliberately NOT an escape
/// hatch: it takes a `&Path`, **never a `&str`**, so it cannot name a
/// store. After the split there is no string-taking entry point left, and
/// the distinction is enforced by the type rather than by remembering.
///
/// It exists because `AdventureManager::new` is handed its characters,
/// world and cooldown paths by its caller: production passes a bare
/// filename and this resolves it against the data directory, while a test
/// passes an absolute scratch path and `Path::join` REPLACES rather than
/// appends, leaving it untouched. **That is the mechanism the whole
/// test-isolation pattern rests on** - the same one `marker_path` uses -
/// and all 60 constructor call sites depend on it. Typing the constructor
/// would break isolation in order to install a correctness guard, which is
/// why it was ruled against.
pub fn normalize_caller_path(path: &std::path::Path) -> PathBuf {
    DATA_DIR.get_or_init(PathBuf::new).join(path)
}

/// A store belonging to ONE manager's data set, resolved beside the
/// characters file it was handed rather than through the global
/// `data_path`.
///
/// **Use this for anything an `AdventureManager` owns and writes.** A
/// store resolved through `data_path` lands in the process CWD when
/// `DATA_DIR` is unset, which is how 58 stray gitignored files ended up
/// in this checkout. `marker_path` below records that defect in full; it
/// applies to every manager-owned store, not only to markers, which is
/// why this generalisation exists rather than a second copy of it.
pub fn sibling_store_path(characters_path: &std::path::Path, store: Store) -> PathBuf {
    characters_path.parent().map_or_else(|| PathBuf::from(store.name()), |dir| dir.join(store.name()))
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
pub fn marker_path(characters_path: &std::path::Path, marker: Store) -> PathBuf {
    sibling_store_path(characters_path, marker)
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
