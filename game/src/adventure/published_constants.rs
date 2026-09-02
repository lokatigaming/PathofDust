// PERMANENTLY STALE AS OF 2026-09-02 (Twitch removal). The bot is gone
// and so is the `/api/published-constants` endpoint that wrote this file
// on its behalf, so `bot-published-constants.json` is never written
// again - it has been dropped from both backup scripts. `load_json` here
// returns `None` forever, which `wiki.rs`'s `wiki_placeholder_map`
// already handles by rendering "varies" for all five placeholders. That
// is the documented, shipped fallback, so nothing breaks.
//
// This module is RETAINED rather than deleted only because `wiki.rs`
// (owned by the wiki session - CLAUDE.md §Multi-session rule 1) still
// imports `PublishedConstants` and `published_constants_path`. It should
// be deleted together with those five placeholders; see WIKI_IMPACT.md.
//
// Historical rationale below, kept for the record:
//
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
// Originally NOT routed through `paths::data_path`, on the reasoning that
// this is bot-owned data published TO the game rather than the game's own
// persisted state, and that a bare CWD-relative filename is correct for as
// long as bot and game share one process/one working directory (true
// through Stage 2, the "dual-mode transitional state").
//
// That reasoning expired (2026-08-29, Linux-readiness). The bot no longer
// touches this file at all: since the Stage 3 seam it POSTs to
// `/api/published-constants` (src/adventure_client.rs) and the GAME writes
// the file (`adventure_web/api.rs`) and the GAME reads it back
// (`main.rs`, `adventure_web/wiki.rs`). Every hand on it is now the game's,
// so it resolves through `data_path` like the rest of the game's state -
// see `published_constants_path`. Bot and game genuinely stop sharing a
// filesystem view on Linux, which is exactly the revisit this comment
// flagged in advance.
use serde::{Deserialize, Serialize};

pub const PUBLISHED_CONSTANTS_PATH: &str = "bot-published-constants.json";

/// [`PUBLISHED_CONSTANTS_PATH`] resolved against the configured data
/// directory. Use this at every read and write; the bare const stays for
/// the log/warning messages that name the file to a human.
///
/// Both call sites MUST agree, which is why this is a function rather
/// than each site calling `data_path` for itself - the writer
/// (`adventure_web::api`) and the two readers (`main.rs`, the wiki's
/// placeholder map) landing in different directories would present as the
/// wiki quietly rendering "varies" forever.
pub fn published_constants_path() -> std::path::PathBuf {
    super::data_path(PUBLISHED_CONSTANTS_PATH)
}

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
