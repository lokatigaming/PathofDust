// Architecture refactor Stage 1 (2026-08-18) - `adventure`/`passive_tree`/
// `state` now physically live in the `game` library crate (see its own
// `lib.rs`). Re-exported here under their ORIGINAL names so every
// existing `crate::adventure::X`/`crate::passive_tree::X`/`crate::state::X`
// reference throughout this crate - main.rs, commands.rs,
// adventure_web.rs, and (deliberately left completely untouched, per
// CLAUDE.md's multi-session coordination rules) wiki.rs - keeps resolving
// with zero changes. A mechanical move, not a behavior change.
pub use game::adventure;
pub use game::passive_tree;
pub use game::state;
pub mod adventure_overlay_server;
pub mod adventure_web;
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
