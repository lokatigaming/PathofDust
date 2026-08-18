// Central config, loaded from a .env file (same file/values as the Node
// version can be reused directly — this just adds a few new keys for the
// song request and chat overlay servers). Optional integrations
// (Patreon, StreamElements, song requests) are `Option` — leaving their
// keys unset disables that feature gracefully instead of erroring, same
// behavior as the Node bot.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PatreonConfig {
    pub client_id: String,
    pub client_secret: String,
    pub poll_interval_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub twitch_client_id: String,
    pub twitch_client_secret: String,
    pub twitch_channel: String,

    /// Where lokati.net's site folder lives, so commands-data.json can be
    /// written there for the public /commands.html page. None disables
    /// that regeneration step (still works fine locally without it).
    pub public_site_dir: Option<PathBuf>,

    pub patreon: Option<PatreonConfig>,

    pub announcement_interval_ms: u64,
    pub announcement_min_messages: u32,

    pub alert_server_port: u16,

    /// StreamElements account JWT for real-time tip events. See
    /// streamelements.rs for where to get this.
    pub streamelements_jwt: Option<String>,

    /// Base URL of the Cloudflare Worker relaying PayPal webhook tips (see
    /// paypal.rs and cloudflare-paypal-relay/). Both this and the token
    /// must be set for PayPal tip alerts to be enabled.
    pub paypal_relay_url: Option<String>,
    /// Shared bearer token the relay Worker also has, so its
    /// /pending-tips endpoint can't be scraped by anyone who finds the URL.
    pub paypal_relay_token: Option<String>,
    pub paypal_poll_interval_ms: u64,

    /// One or more YouTube Data API v3 keys, used to resolve/search songs
    /// for !songrequest — tried in rotation on a 429 (a single free-tier
    /// key is capped at 100 search.list calls/day, so more keys extends
    /// the effective daily budget). Set YOUTUBE_API_KEYS as a comma-
    /// separated list for multiple, or the single-key YOUTUBE_API_KEY
    /// still works too. Get a free one at
    /// https://console.cloud.google.com/apis/credentials (enable "YouTube
    /// Data API v3"). Song requests are disabled entirely if empty.
    pub youtube_api_keys: Vec<String>,
    pub song_request_server_port: u16,
    pub song_request_max_duration_secs: u64,
    /// Number of unique chatters needed for !voteskip/!vs to skip the
    /// current song. Votes reset every time the song changes.
    pub song_request_voteskip_threshold: u32,
    /// Number of unique chatters needed for !votepause to pause the current
    /// song, and separately for !votestart to resume it.
    pub song_request_votepause_threshold: u32,
    pub song_request_voteresume_threshold: u32,
    /// Once !votepause pauses the song, !votestart can't actually resume it
    /// until this many seconds have passed — even if its vote already hit
    /// threshold, the resume just waits out the rest of the cooldown.
    pub song_request_resume_cooldown_secs: u64,
    /// Number of unique chatters needed for !votevolume to set a
    /// particular volume level (always clamped to 50-75, see
    /// `song_requests::{MIN_VOTE_VOLUME, MAX_VOTE_VOLUME}`).
    pub song_request_votevolume_threshold: u32,

    /// League poe.ninja economy lookups (!essenceprofit) are scoped to —
    /// update this in .env when a new league launches, no redeploy needed.
    pub poe_ninja_league: String,

    pub chat_overlay_server_port: u16,

    /// The adventure game overlay (public_adventure_overlay/overlay.html)
    /// — see adventure.rs/adventure_overlay_server.rs.
    pub adventure_overlay_server_port: u16,

    /// The viewer-facing adventure character web dashboard — see
    /// adventure_web.rs. Separate from the OBS-only overlay above; this
    /// one's meant to be reachable by chat, not just the streamer's PC.
    pub adventure_web_port: u16,
    /// Base URL this dashboard is actually reachable at (scheme + host,
    /// no trailing slash) — used to build the exact Twitch OAuth
    /// redirect_uri (`{this}/auth/callback`), which must be registered
    /// verbatim under this app's Redirect URIs at
    /// https://dev.twitch.tv/console/apps (the existing app,
    /// TWITCH_CLIENT_ID, is reused — Twitch apps support multiple
    /// registered redirect URIs, so this doesn't disturb the bot's own
    /// http://localhost:3000/callback setup URI). Defaults to localhost,
    /// which only the streamer's own PC can reach — set this to a real
    /// public URL (behind a tunnel/reverse proxy/port-forward you set up
    /// separately) before expecting viewers to actually be able to log in.
    pub adventure_web_public_url: String,

    /// Base URL of the standalone `game` process's `/api/*` seam
    /// (architecture refactor Stage 4, see REFACTOR_PLAN.md §4) - the bot
    /// no longer runs the adventure game in-process at all, this is
    /// where it reaches it instead. Same host/port `game`'s own
    /// ADVENTURE_WEB_PORT binds by default (the seam is nested onto that
    /// same Axum server, not a separate port - see §3's own ratified
    /// text), so the default here matches that port's own default.
    pub adventure_api_base_url: String,
    /// Shared secret presented on every `/api/*` call above (see
    /// game/src/adventure_web/api.rs's `require_shared_secret`) -
    /// REQUIRED, unlike most other secrets in this file: the adventure
    /// game is always-on (no config gate, same as it always has been),
    /// so without this the bot has no way to reach it at all. Must
    /// match the `game` process's own ADVENTURE_API_SECRET exactly (see
    /// REFACTOR_PLAN.md §4d for the full credential-handling story -
    /// where it lives, why it's safe, what happens on mismatch).
    pub adventure_api_secret: String,

    /// Shared secret for pushing personal-playlist data to the Apps
    /// Script backend (see personal_playlists.rs) — must match the
    /// PLAYLIST_SYNC_SECRET script property on that project. None
    /// disables the sync; the bot's own local playlist data and
    /// !playlist <username> keep working regardless, only the public
    /// site falls behind.
    pub playlist_sync_secret: Option<String>,

    /// Cost of the self-service "Set Entrance Theme Song" channel points
    /// reward. Only takes effect the first time the reward is created
    /// (see channel_points.rs) — changing this later doesn't retroactively
    /// update an already-created reward's cost; delete
    /// channel-points-theme-reward.json (and the reward itself, in the
    /// Twitch dashboard) to force it to be recreated with the new cost.
    pub channel_points_theme_reward_cost: u32,

    /// Cost of the self-service "Interrupt the Music" channel points
    /// reward — inserts a song immediately and casts a !voteskip vote
    /// against whatever it interrupted, in one redemption. Only takes
    /// effect the first time the reward is created (see channel_points.rs)
    /// — changing this later doesn't retroactively update an
    /// already-created reward's cost; delete
    /// channel-points-interrupt-reward.json (and the reward itself, in the
    /// Twitch dashboard) to force it to be recreated with the new cost.
    pub channel_points_interrupt_reward_cost: u32,

    /// Cost of the self-service "Reforge Gear" channel points reward —
    /// reforges one random equipped adventure item into a fresh,
    /// higher-tier version (once per redeemer per hour — see
    /// adventure.rs's REFORGE_COOLDOWN). Only takes effect the first time
    /// the reward is created (see channel_points.rs) — changing this
    /// later doesn't retroactively update an already-created reward's
    /// cost; delete channel-points-reforge-reward.json (and the reward
    /// itself, in the Twitch dashboard) to force it to be recreated with
    /// the new cost.
    pub channel_points_reforge_reward_cost: u32,

    /// Cost of the self-service "Repair All Gear" channel points reward —
    /// fully repairs every equipped AND bagged adventure item and
    /// automatically clears retreat status if they were sitting out with
    /// worn-out gear (see adventure.rs's `repair_all_gear_free`). Only
    /// takes effect the first time the reward is created (see
    /// channel_points.rs) — changing this later doesn't retroactively
    /// update an already-created reward's cost; delete
    /// channel-points-repair-reward.json (and the reward itself, in the
    /// Twitch dashboard) to force it to be recreated with the new cost.
    pub channel_points_repair_reward_cost: u32,

    /// Cost of the self-service "Force Boss Fight" channel points reward
    /// — triggers the next boss encounter immediately instead of waiting
    /// for the 10-minute timer, up to `FORCE_BOSS_MAX_PER_CYCLE` (2)
    /// uses per cycle (see adventure.rs's `try_force_encounter`). Same
    /// "only takes effect on first creation" caveat as the other
    /// rewards above — delete channel-points-force-boss-reward.json (and
    /// the reward itself, in the Twitch dashboard) to force a recreate.
    pub channel_points_force_boss_reward_cost: u32,

    /// Free API key from https://www.last.fm/api/account/create — powers
    /// !playrandom's genre matching (see playrandom.rs). None disables
    /// !playrandom entirely (it replies that it's not enabled), everything
    /// else keeps working regardless.
    pub lastfm_api_key: Option<String>,

    /// obs-websocket server URL — same PC as the bot, so the default
    /// (OBS's own default port) works without any setup beyond enabling
    /// the server in OBS (Tools -> WebSocket Server Settings).
    pub obs_websocket_url: String,
    /// Only required if OBS's WebSocket server has a password set (the
    /// default since OBS 28) — must match exactly.
    pub obs_websocket_password: Option<String>,
    /// Exact name of the song overlay's Browser Source in OBS, as it
    /// appears in the Sources list — required for !votevolume/!modvolume
    /// to control the *actual* OBS fader (which applies after any
    /// Compressor/Limiter filters, unlike the old YouTube-player-internal
    /// volume). None disables that OBS control; the vote/mod-volume
    /// commands then just report they're not configured.
    pub obs_song_source_name: Option<String>,
}

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn env_var_or(key: &str, default: &str) -> String {
    env_var(key).unwrap_or_else(|| default.to_string())
}

fn env_u64_or(key: &str, default: u64) -> u64 {
    env_var(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_u32_or(key: &str, default: u32) -> u32 {
    env_var(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_u16_or(key: &str, default: u16) -> u16 {
    env_var(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

impl Config {
    /// Loads .env (if present) and reads all config from environment
    /// variables. Panics with a clear message if a truly required value
    /// (Twitch credentials) is missing — matches the Node bot's
    /// fail-fast-on-missing-required-config behavior.
    pub fn load() -> anyhow::Result<Self> {
        // Ok(()) if .env doesn't exist is fine — same as dotenv/config in
        // the Node version, which silently no-ops if the file is missing.
        let _ = dotenvy::dotenv();

        let twitch_client_id = env_var("TWITCH_CLIENT_ID")
            .ok_or_else(|| anyhow::anyhow!("Missing TWITCH_CLIENT_ID in .env"))?;
        let twitch_client_secret = env_var("TWITCH_CLIENT_SECRET")
            .ok_or_else(|| anyhow::anyhow!("Missing TWITCH_CLIENT_SECRET in .env"))?;
        let twitch_channel = env_var("TWITCH_CHANNEL")
            .ok_or_else(|| anyhow::anyhow!("Missing TWITCH_CHANNEL in .env"))?;
        // Stage 4 cutover (REFACTOR_PLAN.md §4) - required, not optional,
        // since the bot has no other way to reach the always-on adventure
        // game anymore. See this field's own doc for the credential story.
        let adventure_api_secret = env_var("ADVENTURE_API_SECRET")
            .ok_or_else(|| anyhow::anyhow!("Missing ADVENTURE_API_SECRET in .env - required now that the bot talks to the game process over HTTP instead of in-process (see REFACTOR_PLAN.md §4d)"))?;

        let patreon = match (env_var("PATREON_CLIENT_ID"), env_var("PATREON_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(PatreonConfig {
                client_id,
                client_secret,
                poll_interval_ms: env_u64_or("PATREON_POLL_INTERVAL_MS", 60_000),
            }),
            _ => None,
        };

        Ok(Self {
            twitch_client_id,
            twitch_client_secret,
            twitch_channel,
            public_site_dir: env_var("PUBLIC_SITE_DIR").map(PathBuf::from),
            patreon,
            announcement_interval_ms: env_u64_or("ANNOUNCEMENT_INTERVAL_MS", 20 * 60 * 1000),
            announcement_min_messages: env_u32_or("ANNOUNCEMENT_MIN_MESSAGES", 10),
            alert_server_port: env_u16_or("ALERT_SERVER_PORT", 4001),
            streamelements_jwt: env_var("STREAMELEMENTS_JWT"),
            paypal_relay_url: env_var("PAYPAL_RELAY_URL"),
            paypal_relay_token: env_var("PAYPAL_RELAY_TOKEN"),
            paypal_poll_interval_ms: env_u64_or("PAYPAL_POLL_INTERVAL_MS", 15_000),
            youtube_api_keys: env_var("YOUTUBE_API_KEYS")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .or_else(|| env_var("YOUTUBE_API_KEY").map(|k| vec![k]))
                .unwrap_or_default(),
            song_request_server_port: env_u16_or("SONG_REQUEST_SERVER_PORT", 4002),
            song_request_max_duration_secs: env_u64_or("SONG_REQUEST_MAX_DURATION_SECONDS", 600),
            song_request_voteskip_threshold: env_u32_or("SONG_REQUEST_VOTESKIP_THRESHOLD", 3),
            song_request_votepause_threshold: env_u32_or("SONG_REQUEST_VOTEPAUSE_THRESHOLD", 3),
            song_request_voteresume_threshold: env_u32_or("SONG_REQUEST_VOTERESUME_THRESHOLD", 3),
            song_request_resume_cooldown_secs: env_u64_or("SONG_REQUEST_RESUME_COOLDOWN_SECONDS", 30),
            song_request_votevolume_threshold: env_u32_or("SONG_REQUEST_VOTEVOLUME_THRESHOLD", 3),
            poe_ninja_league: env_var_or("POE_NINJA_LEAGUE", "Allflame"),
            chat_overlay_server_port: env_u16_or("CHAT_OVERLAY_SERVER_PORT", 4003),
            adventure_overlay_server_port: env_u16_or("ADVENTURE_OVERLAY_SERVER_PORT", 4004),
            adventure_web_port: env_u16_or("ADVENTURE_WEB_PORT", 4005),
            adventure_web_public_url: env_var_or("ADVENTURE_WEB_PUBLIC_URL", "http://localhost:4005"),
            adventure_api_base_url: env_var_or("ADVENTURE_API_BASE_URL", "http://127.0.0.1:4005"),
            adventure_api_secret,
            playlist_sync_secret: env_var("PLAYLIST_SYNC_SECRET"),
            channel_points_theme_reward_cost: env_u32_or("CHANNEL_POINTS_THEME_REWARD_COST", 5000),
            channel_points_interrupt_reward_cost: env_u32_or("CHANNEL_POINTS_INTERRUPT_REWARD_COST", 5000),
            channel_points_reforge_reward_cost: env_u32_or("CHANNEL_POINTS_REFORGE_REWARD_COST", 1000),
            channel_points_repair_reward_cost: env_u32_or("CHANNEL_POINTS_REPAIR_REWARD_COST", 100),
            channel_points_force_boss_reward_cost: env_u32_or("CHANNEL_POINTS_FORCE_BOSS_REWARD_COST", 5000),
            lastfm_api_key: env_var("LASTFM_API_KEY"),
            obs_websocket_url: env_var_or("OBS_WEBSOCKET_URL", "ws://127.0.0.1:4455"),
            obs_websocket_password: env_var("OBS_WEBSOCKET_PASSWORD"),
            obs_song_source_name: env_var("OBS_SONG_SOURCE_NAME"),
        })
    }
}
