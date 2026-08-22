// Bot/game build-time decoupling (2026-08-22, finishing REFACTOR_PLAN.md's
// S3-S5): this crate no longer depends on the `game` crate at all, so a
// change touching only game/** builds and deploys without rebuilding or
// redeploying the bot. The runtime half shipped first - the bot holds no
// game data and speaks HTTP to the standalone game process through
// `adventure_client` with a shared secret, relaying fight announcements
// over that same seam's SSE stream. The build-time half closes here:
// the twelve seam/game integration tests now live in `game/tests` (S3),
// the last bot->game file write became POST /api/published-constants
// (S4, see `published_constants`), and the five `pub use game::...`
// re-exports below were deleted along with the path dependency itself
// (S5). The bot's own generic JSON helpers live in `state` - local
// copies of the two-function pair game/src/state.rs carries, since each
// side owns its own files and a shared crate would have re-coupled the
// builds for no benefit.
pub mod adventure_client;
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
pub mod published_constants;
pub mod song_overlay_server;
pub mod song_requests;
pub mod state;
pub mod streamelements;
pub mod twitch;
pub mod vessel_pricing;
