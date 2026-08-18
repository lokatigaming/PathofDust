// Architecture refactor Stage 1-2 (2026-08-18) - `adventure`/
// `passive_tree`/`state` (Stage 1), then `adventure_web`/
// `adventure_overlay_server` including wiki.rs (Stage 2) physically live
// in the `game` library crate now (see its own `lib.rs`). Re-exported
// here under their ORIGINAL names so every existing
// `crate::adventure::X`/`crate::adventure_web::X`-style reference
// throughout this crate - main.rs, commands.rs - keeps resolving with
// zero changes. `twitch-bot-rs` still calls `adventure_web::
// start_adventure_web_server`/`adventure_overlay_server`'s own start fn
// directly, in-process, exactly as before ("dual-mode transitional
// state," per REFACTOR_PLAN.md's Stage 2 - the standalone `game` binary
// this stage adds is a SECOND, independent way to start the same code,
// not a replacement for this one yet).
pub use game::adventure;
pub use game::adventure_overlay_server;
pub use game::adventure_web;
pub use game::passive_tree;
pub use game::state;
pub mod alerts;
pub mod announcements;
pub mod build_feed;
pub mod bug_reports;
pub mod channel_points;
pub mod chat_overlay_server;
pub mod commands;
pub mod config;
pub mod emotes;
pub mod entrance_themes;
pub mod essence_pricing;
pub mod obs_websocket;
pub mod patreon;
pub mod paypal;
pub mod personal_playlists;
pub mod playrandom;
pub mod poe_ninja;
pub mod song_overlay_server;
pub mod song_requests;
pub mod streamelements;
pub mod twitch;
pub mod vessel_pricing;
