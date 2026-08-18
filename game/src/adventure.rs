// Sidescroller/auto-battler viewer game. Shape, per the planning
// discussion this was scoped from: free to join via chat, one continuous
// shared world (not per-stream resets), characters persist forever, and
// progression is passive (being active in chat earns XP) rather than
// needing any combat input — the "auto" in auto-battler.
//
// Combat is a REAL per-unit simulation (see `simulate_battle`), not a
// single coin flip — every player and the boss have actual HP, roles are
// mechanically different (melee/ranged damage the boss, support heals
// the lowest-HP ally instead), and the boss targets a random alive
// player each swing. It's still resolved instantly server-side as a full
// event log rather than ticked live, though — deterministic-outcome-
// then-animate is still the right shape, there's just a lot more to
// animate now. `run_encounter` compresses/stretches that log's real
// timestamps into a watchable display window before broadcasting it, so
// the overlay's actual on-screen pacing stays reasonable regardless of
// how long or short the real fight naturally ran.
//
// Boss stats scale off the CURRENT party's size and average level, not
// just stage — scaling by stage alone would let an ever-growing roster
// eventually steamroll every fight for free, which defeats "genuinely
// challenging." These are first-pass numbers that will need real tuning
// once played with for real.
//
// Deliberately NOT yet handling: gear/loot (items are next), a catch-up
// bonus for late joiners (new characters start at level 1 like everyone
// did), or per-individual-performance win/loss tracking (everyone who
// fought gets the same win/loss/XP regardless of what they personally
// did in the fight, same as before). All fast follows.
//
// 2026-08-16: split into src/adventure/{affix,item,craft,balance,
// migrations,character,combat,manager}.rs (see the item-system refactor
// plan) - this file is now just the module root: shared imports plus
// `mod`/`pub use` wiring so every existing `twitch_bot_rs::adventure::X`
// import elsewhere in the crate keeps resolving unchanged. Each
// submodule does `use super::*;` to see these imports and every sibling's
// re-exported items - Rust resolves this parent-reexports-children/
// children-import-parent pattern across the whole crate, not
// line-by-line, so the apparent circularity isn't an issue.

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Mutex, Notify};

mod affix;
mod balance;
mod character;
mod combat;
mod craft;
mod fight_storage;
#[cfg(test)]
mod golden_corpus;
mod item;
mod manager;
mod migrations;
mod paths;
mod published_constants;
mod tunables;

pub use affix::*;
pub(crate) use balance::*;
pub use character::*;
pub use combat::*;
pub use craft::*;
pub use fight_storage::*;
pub use item::*;
pub use manager::*;
pub(crate) use migrations::*;
pub use paths::set_data_dir;
pub(crate) use paths::data_path;
pub use published_constants::{PublishedConstants, PUBLISHED_CONSTANTS_PATH};
pub use tunables::*;
