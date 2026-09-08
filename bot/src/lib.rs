// Bot/game decoupling, finished 2026-09-02 (`chore/bot-decoupling`).
// This crate is a Twitch bot and nothing else. The build-time half went
// first (2026-08-22, REFACTOR_PLAN.md S3-S5): the path dependency on the
// `game` crate, the five `pub use game::...` re-exports and the twelve
// seam integration tests all left, so a change touching only game/**
// never rebuilds the bot. The runtime half is gone now too - the game's
// `/api/*` seam was deleted with Twitch itself, so `adventure_client`
// (the HTTP client), `published_constants` (its one remaining POST), the
// adventure chat commands, the three adventure channel-point redemptions,
// chat activity XP and the SSE announcements relay were all removed here.
// Nothing in this crate speaks to the game process any more; the two
// share no port, no file and no secret.
//
// The bot's own generic JSON helpers live in `state` - local copies of
// the two-function pair game/src/state.rs carries, since each side owns
// its own files and a shared crate would have re-coupled the builds for
// no benefit.
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
pub mod paypal;
pub mod personal_playlists;
pub mod playrandom;
pub mod poe_ninja;
pub mod song_overlay_server;
pub mod song_requests;
pub mod state;
pub mod streamelements;
pub mod twitch;
pub mod vessel_pricing;
