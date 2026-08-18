// Bot→game published constants (2026-08-18, architecture refactor Stage
// 2 - the owner's ruling on wiki.rs's crate placement: wiki.rs goes
// GAME-side, full stop, since the standalone deliverable requires the
// game to serve its full web UI - wiki included - with no bot process
// running. The 5 constants below are the ONLY thing that made that a
// real question: wiki.rs reads them directly today
// (`crate::commands::BUILTIN_COOLDOWN` and friends), and `game` can't
// depend back on `bot` (that's the whole point of the crate split).
//
// Mechanism chosen (owner left this as an implementation call - "bot
// POSTs them at startup via the API, or they migrate into shared
// config"): a small shared JSON file, NOT the API. All 5 source values
// are plain `const`s on the bot side today - fixed at compile time,
// identical for a process's entire lifetime - so there's no actual
// "live" data to stream, just a handful of numbers that change only
// when the bot itself is rebuilt. A file the bot writes once at startup
// (see `main.rs`) and the game reads on demand is the simplest
// mechanism that's actually proportionate to that, and it's the ONE
// piece of "the seam" that needs to exist before Stage 3 - everything
// ELSE Stage 3 covers (chat commands, redemptions, SSE announcements)
// is genuinely live/request-driven and belongs on the real HTTP seam
// once that's built.
//
// Deliberately NOT routed through `paths::data_path` - this is bot-
// owned data being published TO the game, not part of the game's own
// persisted state, same "shared, not game-exclusive" reasoning
// `state.rs` itself already documents. A bare CWD-relative filename is
// correct for as long as bot and game share one process/one working
// directory (true through Stage 2 - "dual-mode transitional state") -
// once Stage 4 actually separates them into two processes, this specific
// file-based mechanism may need revisiting (a real API publish becomes
// more honest once bot/game genuinely don't share a filesystem view by
// default) - flagging that now rather than assuming this file survives
// unexamined that far.
use serde::{Deserialize, Serialize};

pub const PUBLISHED_CONSTANTS_PATH: &str = "bot-published-constants.json";

/// The bot's own cooldown/volume-bound constants, as they exist at the
/// moment `main.rs` writes this file (see that call site) - a snapshot,
/// not a live feed, which is exactly right for values that are
/// themselves compile-time fixed on the bot side. Every field mirrors
/// one wiki.rs placeholder (see `wiki_placeholder_map`'s own doc for the
/// exact mapping) - kept as a flat struct (not reusing `Duration` for
/// the *_secs fields) so the JSON on disk is trivially human-readable
/// for anyone debugging why a placeholder looks wrong.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedConstants {
    pub builtin_cooldown_secs: u64,
    pub bug_report_cooldown_secs: u64,
    pub song_skip_cooldown_secs: u64,
    pub min_vote_volume: u8,
    pub max_vote_volume: u8,
}
