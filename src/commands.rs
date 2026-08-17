// Chat commands — ports commands.js. Hand-written commands (real logic)
// are a match statement below instead of a JS object-of-functions, since
// that's the more idiomatic way to do this dispatch in Rust; static
// text commands (commands.json-backed, with cooldowns/variables/aliases)
// work exactly like the Node version, including the `!command add/edit/
// delete` chat-based management and the commands-data.json regeneration
// for lokati.net/commands.html.

use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use crate::adventure::{AdventureManager, BossKind, JoinOutcome, RampageVoteOutcome, TriggerEncounterOutcome, RAMPAGE_VOTE_THRESHOLD};
use crate::announcements::Announcements;
use crate::build_feed;
use crate::bug_reports::{BugReportManager, SubmitOutcome};
use crate::entrance_themes::EntranceThemeManager;
use crate::obs_websocket::ObsClient;
use crate::patreon::PatreonWatcher;
use crate::personal_playlists::{AddOutcome, PersonalPlaylistManager, PlayOutcome, RemoveOutcome, SampleOutcome};
use crate::playrandom::PlayRandomManager;
use crate::poe_ninja;
use crate::song_requests::{
    RequestOutcome, SongInsertOutcome, SongRequestManager, VotePauseOutcome, VoteResumeOutcome, VoteSkipOutcome,
    VoteVolumeOutcome, MAX_VOTE_VOLUME, MIN_VOTE_VOLUME,
};
use crate::streamelements::StreamElementsWatcher;
use crate::twitch::helix::HelixClient;

/// !playlist <username> queues a random sample, up to this many songs,
/// from their saved playlist into the live queue — capped so one
/// person's playlist can't monopolize the queue in a single invocation
/// (they can just run it again for more).
const PLAYLIST_QUEUE_SAMPLE_SIZE: usize = 5;

const BUILTIN_COOLDOWN: Duration = Duration::from_secs(5);

pub enum Reply {
    None,
    One(String),
    Many(Vec<String>),
}

impl From<String> for Reply {
    fn from(s: String) -> Self {
        Reply::One(s)
    }
}

impl From<&str> for Reply {
    fn from(s: &str) -> Self {
        Reply::One(s.to_string())
    }
}

/// Everything a command handler might need. Optional integrations are
/// `Option` — same graceful "disabled if not configured" behavior as the
/// Node bot (e.g. !checkpatreon replies "not enabled" if `patreon` is None
/// instead of erroring).
pub struct Services {
    pub helix: HelixClient,
    pub broadcaster_id: String,
    pub patreon: Option<Arc<PatreonWatcher>>,
    pub alerts: Option<Arc<crate::alerts::AlertServer>>,
    pub streamelements: Option<Arc<StreamElementsWatcher>>,
    pub announcements: Arc<Announcements>,
    pub static_commands: Arc<StaticCommands>,
    pub song_requests: Option<Arc<SongRequestManager>>,
    pub poe_ninja_http: reqwest::Client,
    pub poe_ninja_league: String,
    pub entrance_themes: Arc<EntranceThemeManager>,
    pub personal_playlists: Arc<PersonalPlaylistManager>,
    pub play_random: Option<Arc<PlayRandomManager>>,
    /// The OBS client plus the exact Browser Source name to control —
    /// always Some together or None together (see config.rs). Used by
    /// !votevolume/!modvolume instead of the old YouTube-player-internal
    /// volume, so it actually works alongside a Compressor/Limiter on
    /// that source (OBS's fader applies after filters, unaffected by
    /// their fixed makeup gain the way the player's own volume was).
    pub obs_song_volume: Option<(Arc<ObsClient>, String)>,
    pub adventure: Arc<AdventureManager>,
    pub bug_reports: Arc<BugReportManager>,
}

// ---- Static text commands (commands.json) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticCommand {
    pub command: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub reply: String,
    #[serde(rename = "type")]
    pub kind: String, // "say" | "reply"
    pub access_level: u32,
    pub cooldown_user: u64,
    pub cooldown_global: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct PublicCommandEntry {
    command: String,
    aliases: Vec<String>,
    reply: String,
    #[serde(rename = "modOnly")]
    mod_only: bool,
}

/// Hand-written commands have real logic so they can't be pulled from
/// commands.json for the public page — kept in sync by hand here, same as
/// HAND_WRITTEN_PUBLIC_ENTRIES in the Node version. `command` and
/// `announcetest`/`alerttest`/`replaytips` are internal mod tools, skipped
/// from the public listing same as before.
fn hand_written_public_entries() -> Vec<PublicCommandEntry> {
    let entries: &[(&str, &str, bool)] = &[
        ("hug", "Sends a hug to a chatter. Usage: !hug @user (or just !hug to hug chat).", false),
        ("uptime", "Shows how long the stream has been live.", false),
        ("time", "Shows Lokati's local time (Singapore).", false),
        ("commands", "Links to this command list.", false),
        ("checkpatreon", "Forces an immediate check for new Patreon supporters.", true),
        ("handsome", "Plays a special video on stream.", false),
        ("thatsagoodbuild", "Plays a special video on stream.", false),
        ("essenceprofit", "Live poe.ninja lookup: average Deafening Essence price and estimated chaos/divine profit per hour farming them (alias !ep).", false),
        ("ritualprofit", "Live poe.ninja lookup: estimated chaos/divine profit per hour running 5 Cloister Scarabs + 4 Ritual Vessels per map for Stacked Decks (alias !rp).", false),
        ("vesselprice", "Mod tool: links to the Blood-filled Vessel pricing page — current post-at prices per monster-count band plus history (alias !vp).", true),
        ("price", "Umbrella shortcut for the pricing commands. Usage: !price ritual (same as !ritualprofit), !price essence (links to the essence pricing page), !price vessels (mod tool, same as !vesselprice).", false),
        ("songrequest", "Queues a song from YouTube. Usage: !songrequest <YouTube link or search terms> (alias !sr).", false),
        ("queue", "Lists the upcoming songs in the queue and the last 5 songs played.", false),
        ("nowplaying", "Shows the currently playing song (alias !np).", false),
        ("song", "Shows the link to the currently playing song and who requested it (alias !currentsong).", false),
        ("skip", "Mod tool: skips the currently playing song immediately, no vote needed (alias !modskip).", true),
        ("voteskip", "Votes to skip the current song — skips once enough chatters vote, or immediately if it's a song you requested yourself (alias !vs).", false),
        ("votepause", "Votes to pause the current song — pauses once enough chatters vote.", false),
        ("votestart", "Votes to resume the paused song — resumes once enough chatters vote (30s cooldown after a pause).", false),
        ("votevolume", "Votes for a music volume between 50-75%. Usage: !votevolume <50-75> (alias !vv).", false),
        ("clearqueue", "Clears the pending song queue (doesn't stop the current song).", true),
        ("forceplay", "Streamer-only: disables !voteskip for the current song so it plays through.", true),
        ("modpause", "Mod tool: pauses the current song immediately, no vote needed.", true),
        ("modstart", "Mod tool: resumes the paused song immediately, no vote or cooldown (alias !modresume).", true),
        ("modvolume", "Mod tool: sets the volume immediately, 0-100, no vote needed (alias !modvv).", true),
        ("songinsert", "Mod tool: plays a song immediately, then returns to the current song where it left off and continues the playlist. Usage: !songinsert <YouTube link or search terms> (alias !si).", true),
        ("settheme", "Mod tool: assigns a chatter's entrance theme song, played (same as !songinsert) the first time they chat each day. Usage: !settheme <username> <YouTube link or search terms>.", true),
        ("resetgreeted", "Mod tool: clears \"already greeted today\" for a user (or everyone, if no username given), so entrance themes retrigger without waiting for the next day. Usage: !resetgreeted [username].", true),
        ("theme", "Links to the page listing everyone's entrance theme song (alias !themes).", false),
        ("playlist", "Every song you !songrequest is saved to your own playlist, viewable anytime at lokati.net/playlists.html. Usage: !playlist <username> (queues 5 random songs from their saved playlist), !playlist <username> play <#> (queues one specific song by position number), !playlist add <song>, !playlist remove <position or title>, !playlist clear (wipes your own playlist; mods can also !playlist clear <username>).", false),
        ("modclear", "Mod tool: clears the pending song queue immediately (doesn't stop the current song) — alias for !clearqueue.", true),
        ("playrandom", "Usage: !playrandom <1-5> queues that many songs from the last 5 distinct genres actually played (genre via Last.fm, since YouTube has no genre data). Mod tool: !playrandom on/off toggles continuously auto-queuing similar songs whenever the queue runs low.", false),
        ("join", "Joins the shared chat adventure with your own permanent character — auto-battles alongside everyone else's every few minutes. Chatting earns your character XP over time.", false),
        ("character", "Shows your adventure character's level, xp, and win/loss record (aliases !char, !me).", false),
        ("party", "Shows the adventure's current stage and how many heroes have joined (alias !adventure).", false),
        ("nextencounter", "Mod tool: runs the next adventure encounter immediately instead of waiting for the timer. Optional boss name (!nextencounter bahamut) forces that specific boss/look instead of a random pick - lich, firedemon, cthulhu, dragon, bahamut, purple, or cube.", true),
        ("event", "Mod tool: !event intro <boss name> forces a stage-appropriate fight against that boss and announces it in chat with a link to its wiki writeup - e.g. !event intro cube.", true),
        ("giveloot", "Mod tool: grants every joined hero a random piece of gear right now (alias !gearall).", true),
        ("giftdust", "Mod tool: !giftdust <all|username> <amount> grants that much dust to everyone who's joined, or to one specific hero.", true),
        ("bugreport", "Send a bug report for Lokati to review. Usage: !bugreport <what happened>.", false),
        ("bugreports", "Mod tool: lists the 5 most recent bug reports in chat.", true),
        ("rampage", "Mods trigger it instantly; anyone else's !rampage counts as a vote — 3 distinct voters starts it too. Turns the next 50 encounters into boss fights, one every ~60s (or as soon as the last finishes playing out), with everyone instantly revived between them.", false),
    ];

    entries
        .iter()
        .map(|(command, reply, mod_only)| PublicCommandEntry {
            command: command.to_string(),
            aliases: vec![],
            reply: reply.to_string(),
            mod_only: *mod_only,
        })
        .collect()
}

pub struct StaticCommands {
    path: PathBuf,
    public_site_dir: Option<PathBuf>,
    commands: RwLock<Vec<StaticCommand>>,
    global_cooldowns: Mutex<HashMap<String, Instant>>,
    user_cooldowns: Mutex<HashMap<(String, String), Instant>>,
}

impl StaticCommands {
    /// Loads commands.json and immediately regenerates commands-data.json
    /// from it — covers the case where commands.json was hand-edited and
    /// the bot just restarted, so the public page reflects that too, not
    /// just changes made via !command.
    pub async fn load(path: PathBuf, public_site_dir: Option<PathBuf>) -> Arc<Self> {
        let commands: Vec<StaticCommand> = crate::state::load_json(&path).unwrap_or_default();
        let this = Arc::new(Self {
            path,
            public_site_dir,
            commands: RwLock::new(commands),
            global_cooldowns: Mutex::new(HashMap::new()),
            user_cooldowns: Mutex::new(HashMap::new()),
        });
        this.regenerate_public_page().await;
        this
    }

    async fn regenerate_public_page(&self) {
        let Some(dir) = &self.public_site_dir else { return };
        let commands = self.commands.read().await;

        let mut all: Vec<PublicCommandEntry> = hand_written_public_entries();
        all.extend(commands.iter().filter(|c| c.enabled).map(|c| PublicCommandEntry {
            command: c.command.clone(),
            aliases: c.aliases.clone(),
            reply: c.reply.clone(),
            mod_only: c.access_level >= 500,
        }));
        all.sort_by(|a, b| a.command.cmp(&b.command));

        if let Err(err) = crate::state::save_json(dir.join("commands-data.json"), &all) {
            tracing::error!("Failed to regenerate public commands page data: {err}");
        }
    }

    async fn persist_and_regenerate(&self) {
        if let Err(err) = crate::state::save_json(&self.path, &*self.commands.read().await) {
            tracing::error!("Failed to save commands.json: {err}");
            return;
        }
        self.regenerate_public_page().await;
    }

    async fn find(&self, name: &str) -> Option<StaticCommand> {
        let commands = self.commands.read().await;
        commands
            .iter()
            .find(|c| c.command == name || c.aliases.iter().any(|a| a == name))
            .cloned()
    }

    async fn exists(&self, name: &str) -> bool {
        self.find(name).await.is_some()
    }

    async fn upsert(&self, name: &str, reply: &str) -> bool {
        let mut commands = self.commands.write().await;
        if let Some(existing) = commands.iter_mut().find(|c| c.command == name) {
            existing.reply = reply.to_string();
            drop(commands);
            self.persist_and_regenerate().await;
            true // existed
        } else {
            commands.push(StaticCommand {
                command: name.to_string(),
                aliases: vec![],
                reply: reply.to_string(),
                kind: "say".to_string(),
                access_level: 100,
                cooldown_user: 15,
                cooldown_global: 5,
                enabled: true,
            });
            drop(commands);
            self.persist_and_regenerate().await;
            false // newly created
        }
    }

    async fn remove(&self, name: &str) -> bool {
        let mut commands = self.commands.write().await;
        let Some(idx) = commands.iter().position(|c| c.command == name) else { return false };
        commands.remove(idx);
        drop(commands);
        self.persist_and_regenerate().await;
        true
    }

    async fn is_on_cooldown(&self, cmd: &StaticCommand, user: &str) -> bool {
        let now = Instant::now();

        if cmd.cooldown_global > 0 {
            let cooldowns = self.global_cooldowns.lock().await;
            if let Some(last) = cooldowns.get(&cmd.command) {
                if now.duration_since(*last) < Duration::from_secs(cmd.cooldown_global) {
                    return true;
                }
            }
        }

        if cmd.cooldown_user > 0 {
            let key = (cmd.command.clone(), user.to_string());
            let cooldowns = self.user_cooldowns.lock().await;
            if let Some(last) = cooldowns.get(&key) {
                if now.duration_since(*last) < Duration::from_secs(cmd.cooldown_user) {
                    return true;
                }
            }
        }

        false
    }

    async fn mark_used(&self, cmd: &StaticCommand, user: &str) {
        let now = Instant::now();
        self.global_cooldowns.lock().await.insert(cmd.command.clone(), now);
        self.user_cooldowns.lock().await.insert((cmd.command.clone(), user.to_string()), now);
    }
}

static RANDOM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{random\.(-?\d+)-(-?\d+)\}").unwrap());
static USER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\$\(user\)|\$\(sender\)").unwrap());

fn apply_variables(text: &str, user: &str) -> String {
    let text = USER_RE.replace_all(text, user);
    RANDOM_RE
        .replace_all(&text, |caps: &regex::Captures| {
            let lo: i64 = caps[1].parse().unwrap_or(0);
            let hi: i64 = caps[2].parse().unwrap_or(0);
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            rand::thread_rng().gen_range(lo..=hi).to_string()
        })
        .into_owned()
}

// ---- Built-in cooldown (shared 5s across all hand-written commands, same
// single-Map behavior as the Node bot — not per-user, per-command-name) ----

static BUILTIN_COOLDOWNS: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

async fn builtin_on_cooldown(name: &str) -> bool {
    let mut cooldowns = BUILTIN_COOLDOWNS.lock().await;
    let now = Instant::now();
    if let Some(last) = cooldowns.get(name) {
        if now.duration_since(*last) < BUILTIN_COOLDOWN {
            return true;
        }
    }
    cooldowns.insert(name.to_string(), now);
    false
}

pub async fn handle_command(
    name: &str,
    user: &str,
    args: &[String],
    is_mod_or_broadcaster: bool,
    is_broadcaster: bool,
    services: &Services,
) -> Reply {
    if let Some(reply) = handle_builtin(name, user, args, is_mod_or_broadcaster, is_broadcaster, services).await {
        return reply;
    }

    let Some(cmd) = services.static_commands.find(name).await else { return Reply::None };
    if !cmd.enabled {
        return Reply::None;
    }
    if cmd.access_level >= 500 && !is_mod_or_broadcaster {
        return Reply::None;
    }
    if services.static_commands.is_on_cooldown(&cmd, user).await {
        return Reply::None;
    }

    services.static_commands.mark_used(&cmd, user).await;
    let text = apply_variables(&cmd.reply, user);
    Reply::One(if cmd.kind == "reply" { format!("@{user}, {text}") } else { text })
}

/// Shared by !essenceprofit/!ep and !price essence.
async fn essence_profit_reply(services: &Services) -> Reply {
    // Derived live from the same build-feed data the website uses, so
    // this follows whatever league the streamer has already set there —
    // POE_NINJA_LEAGUE is only a fallback for if that feed is ever
    // unreachable.
    let league = build_feed::current_league(&services.poe_ninja_http).await.unwrap_or_else(|| services.poe_ninja_league.clone());

    match poe_ninja::essence_profit_per_hour(&services.poe_ninja_http, &league).await {
        Ok(r) => format!(
            "[{league}] Deafening Essences ({} types, avg {:.2}c each) — assuming 50/map x 20 maps/hr: ~{:.0}c/hr = {:.2} div/hr (divine = {:.1}c)",
            r.essence_type_count, r.avg_chaos_per_essence, r.chaos_per_hour, r.divine_per_hour, r.chaos_per_divine
        )
        .into(),
        Err(err) => {
            tracing::error!("!essenceprofit failed: {err}");
            "Couldn't fetch poe.ninja prices right now — try again in a bit.".into()
        }
    }
}

/// Shared by !ritualprofit/!rp and !price ritual.
async fn ritual_profit_reply(services: &Services) -> Reply {
    let league = build_feed::current_league(&services.poe_ninja_http).await.unwrap_or_else(|| services.poe_ninja_league.clone());

    match poe_ninja::ritual_scarab_profit_per_hour(&services.poe_ninja_http, &league).await {
        Ok(r) => format!(
            "[{league}] 5 Cloister Scarabs ({:.2}c) + 4 Ritual Vessels ({:.2}c, ~20c net profit each) -> 65 Stacked Decks ({:.2}c) per map x 20 maps/hr: ~{:.0}c/hr = {:.2} div/hr (divine = {:.1}c)",
            r.cloister_price, r.ritual_vessel_price, r.stacked_deck_price, r.chaos_per_hour, r.divine_per_hour, r.chaos_per_divine
        )
        .into(),
        Err(err) => {
            tracing::error!("!ritualprofit failed: {err}");
            "Couldn't fetch poe.ninja prices right now — try again in a bit.".into()
        }
    }
}

/// Shared by !vesselprice/!vp and !price vessels. Current prices +
/// history live on the page instead of running a live trade-API query
/// per chat invocation — see vessel_pricing.rs's hourly snapshotter.
fn vessel_price_reply() -> Reply {
    "Blood-filled Vessel pricing (current + history): https://lokati.net/vessel-pricing.html".into()
}

/// Shared by !price essence (no standalone !essenceprice — !essenceprofit
/// already owns that name for the profit calculator). Same
/// link-to-the-page pattern as vessels, backed by essence_pricing.rs's
/// hourly snapshotter.
fn essence_price_reply() -> Reply {
    "Deafening Essence pricing (current + history): https://lokati.net/essence-pricing.html".into()
}

/// Returns Some(reply) if `name` matched a hand-written command (even if
/// the reply ends up being nothing to say), None if it's not a built-in at
/// all — so the caller falls through to static commands.json lookup.
async fn handle_builtin(
    name: &str,
    user: &str,
    args: &[String],
    is_mod_or_broadcaster: bool,
    is_broadcaster: bool,
    services: &Services,
) -> Option<Reply> {
    // `command`/`announcetest`/`checkpatreon`/`alerttest`/`replaytips`/
    // `skip`/`clearqueue`/`forceplay` are mod tools — deliberately NOT put
    // behind the shared cooldown, same as the Node version (only user-facing commands
    // share that cooldown). Song requests are also excluded — the "no
    // limits at all" queueing decision means they shouldn't be throttled
    // by an unrelated shared cooldown either.
    let is_mod_tool = matches!(
        name,
        "command"
            | "announcetest"
            | "checkpatreon"
            | "alerttest"
            | "replaytips"
            | "skip"
            | "modskip"
            | "clearqueue"
            | "modclear"
            | "forceplay"
            | "modpause"
            | "modstart"
            | "modresume"
            | "modvolume"
            | "modvv"
            | "songinsert"
            | "si"
            | "settheme"
            | "resetgreeted"
            | "vesselprice"
            | "vp"
    );

    if !is_mod_tool
        && matches!(
            name,
            "hug" | "uptime" | "time" | "commands" | "handsome" | "thatsagoodbuild" | "essenceprofit" | "ep" | "ritualprofit" | "rp" | "price" | "theme" | "themes" | "playrandom"
        )
    {
        // "ep"/"rp"/"themes" are just aliases for
        // "essenceprofit"/"ritualprofit"/"theme" — normalized to the same
        // cooldown key so alternating between either pair doesn't let
        // chat double the effective rate. "!price ritual"/"!price essence"
        // share a cooldown bucket with their original commands for the
        // same reason — "!price vessels" isn't listed here since
        // !vesselprice itself isn't (mod-only, and just a static link, not
        // a live poe.ninja hit worth rate-limiting).
        let cooldown_key = match name {
            "ep" => "essenceprofit",
            "rp" => "ritualprofit",
            "themes" => "theme",
            "price" => match args.first().map(|a| a.to_lowercase()) {
                Some(a) if a == "ritual" || a == "rituals" => "ritualprofit",
                Some(a) if a == "essence" || a == "essences" => "essenceprofit",
                _ => "price",
            },
            other => other,
        };
        if builtin_on_cooldown(cooldown_key).await {
            return Some(Reply::None);
        }
    }

    match name {
        "hug" => {
            let target = args.first().map(|s| s.trim_start_matches('@'));
            Some(match target {
                Some(t) if !t.is_empty() => format!("{user} hugs {t}!").into(),
                _ => format!("{user} sends a hug out to chat!").into(),
            })
        }

        "uptime" => match services.helix.get_stream(&services.broadcaster_id).await {
            Ok(Some(stream)) => {
                let elapsed = chrono::Utc::now() - stream.started_at;
                let total_minutes = elapsed.num_minutes().max(0);
                let hours = total_minutes / 60;
                let minutes = total_minutes % 60;
                Some(format!("Live for {hours}h {minutes}m.").into())
            }
            Ok(None) => Some("We're not live right now.".into()),
            Err(err) => {
                tracing::error!("uptime command failed: {err}");
                Some(Reply::None)
            }
        },

        // Ported from StreamElements: $(time.timezone Singapore Standard Time "MMMM Do h:mm:ss a z")
        "time" => {
            let now = chrono::Utc::now().with_timezone(&chrono_tz::Asia::Singapore);
            let day = now.format("%-d").to_string().parse::<u32>().unwrap_or(1);
            let ordinal = match (day % 10, day) {
                (1, d) if d != 11 => "st",
                (2, d) if d != 12 => "nd",
                (3, d) if d != 13 => "rd",
                _ => "th",
            };
            let fmt_str = format!("%B {day}{ordinal}, %-I:%M:%S %p %Z");
            Some(now.format(&fmt_str).to_string().into())
        }

        "commands" => Some("Full command list: https://lokati.net/commands.html".into()),

        "theme" | "themes" => Some("Everyone's entrance theme songs: https://lokati.net/themes.html".into()),

        "essenceprofit" | "ep" => Some(essence_profit_reply(services).await),

        "ritualprofit" | "rp" => Some(ritual_profit_reply(services).await),

        "vesselprice" | "vp" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            Some(vessel_price_reply())
        }

        // Umbrella shortcut alongside (not replacing) the above — same
        // underlying replies, just reachable as !price <category> too.
        "price" => {
            let Some(category) = args.first() else {
                return Some("Usage: !price <ritual|vessels|essence>".into());
            };
            match category.to_lowercase().as_str() {
                "ritual" | "rituals" => Some(ritual_profit_reply(services).await),
                "essence" | "essences" => Some(essence_price_reply()),
                "vessel" | "vessels" => {
                    if !is_mod_or_broadcaster {
                        return Some(Reply::None);
                    }
                    Some(vessel_price_reply())
                }
                _ => Some("Usage: !price <ritual|vessels|essence>".into()),
            }
        }

        // !playrandom <1-5>   — anyone; queues that many songs similar
        //                        (by Last.fm genre tag) to the last 10
        //                        actually played.
        // !playrandom on/off  — mod tool; toggles continuously topping
        //                        the queue up the same way whenever it
        //                        runs low.
        "playrandom" => {
            let Some(play_random) = &services.play_random else {
                return Some("!playrandom isn't enabled — set LASTFM_API_KEY in .env.".into());
            };
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let Some(arg) = args.first() else {
                return Some(
                    "Usage: !playrandom <1-5> to queue similar-genre songs, or (mods) !playrandom on/off for continuous autoplay.".into(),
                );
            };

            match arg.to_lowercase().as_str() {
                "on" | "off" => {
                    if !is_mod_or_broadcaster {
                        return Some(Reply::None);
                    }
                    let enabling = arg.eq_ignore_ascii_case("on");
                    play_random.set_enabled(enabling);
                    if enabling {
                        // Don't just wait for the next queue-state change to
                        // notice it's empty — check right now too.
                        play_random.trigger_top_up(song_requests);
                    }
                    Some(if enabling {
                        "Continuous similar-genre autoplay is ON — I'll keep the queue topped up based on the last 5 distinct genres played.".into()
                    } else {
                        "Continuous similar-genre autoplay is OFF.".into()
                    })
                }
                _ => {
                    let Ok(count) = arg.parse::<usize>() else {
                        return Some("Usage: !playrandom <1-5>, or (mods) !playrandom on/off.".into());
                    };
                    if !(1..=5).contains(&count) {
                        return Some("Pick a number between 1 and 5.".into());
                    }
                    match play_random.find_similar_songs(song_requests, count).await {
                        Ok((genres, songs)) => {
                            let titles: Vec<String> = songs.iter().map(|s| format!("\"{}\"", s.title)).collect();
                            let n = songs.len();
                            for song in songs {
                                song_requests.queue_song(song);
                            }
                            Some(
                                format!("Queued {n} song(s) from recent genres ({}): {}", genres.join(", "), titles.join(", "))
                                    .into(),
                            )
                        }
                        Err(err) => Some(err.to_string().into()),
                    }
                }
            }
        }

        "handsome" => {
            let Some(alerts) = &services.alerts else { return Some(Reply::None) };
            alerts.broadcast(serde_json::json!({ "type": "video", "src": "handsome.mp4" }));
            Some("😎".into())
        }

        "thatsagoodbuild" => {
            let Some(alerts) = &services.alerts else { return Some(Reply::None) };
            alerts.broadcast(serde_json::json!({ "type": "video", "src": "thatsagoodbuild.mp4" }));
            Some("👍".into())
        }

        "command" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            Some(handle_command_management(args, services).await)
        }

        "announcetest" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let list = services.announcements.all().await;
            if list.is_empty() {
                return Some("No announcements configured yet.".into());
            }
            let total = list.len();
            Some(Reply::Many(
                list.iter().enumerate().map(|(i, text)| format!("({}/{total}) {text}", i + 1)).collect(),
            ))
        }

        "checkpatreon" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(patreon) = &services.patreon else { return Some("Patreon integration is not enabled.".into()) };
            match patreon.force_poll().await {
                Ok(()) => Some("Checked Patreon for new supporters.".into()),
                Err(err) => {
                    tracing::error!("checkpatreon failed: {err}");
                    Some("Something went wrong checking Patreon — check the logs.".into())
                }
            }
        }

        "alerttest" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(alerts) = &services.alerts else { return Some("Alert box server is not running.".into()) };

            let kind = args.first().map(|s| s.to_lowercase()).unwrap_or_else(|| "follow".to_string());
            let event = match kind.as_str() {
                "follow" => serde_json::json!({ "type": "follow", "name": "TestUser" }),
                "subscription" => serde_json::json!({ "type": "subscription", "name": "TestUser" }),
                "subscriptiongift" => serde_json::json!({ "type": "subscriptionGift", "name": "TestGifter", "amount": 3 }),
                "cheer" => serde_json::json!({ "type": "cheer", "name": "TestUser", "bits": 500 }),
                "raid" => serde_json::json!({ "type": "raid", "name": "TestRaider", "viewers": 25 }),
                "tip" => serde_json::json!({ "type": "tip", "name": "TestTipper", "amount": 5, "currency": "USD", "message": "Love the stream!" }),
                other => {
                    return Some(
                        format!("Unknown alert type \"{other}\". Try: follow, subscription, subscriptionGift, cheer, raid, tip")
                            .into(),
                    )
                }
            };

            let type_name = event["type"].as_str().unwrap_or_default().to_string();
            alerts.broadcast(event);
            Some(format!("Sent a test \"{type_name}\" alert to the alert box.").into())
        }

        "replaytips" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(alerts) = &services.alerts else { return Some("Alert box server is not running.".into()) };
            let Some(se) = &services.streamelements else {
                return Some("StreamElements integration is not enabled.".into());
            };

            let recent = se.get_recent_tips(5).await;
            if recent.is_empty() {
                return Some("No tips recorded yet.".into());
            }

            let count = recent.len();
            let alerts = alerts.clone();
            tokio::spawn(async move {
                for (i, tip) in recent.into_iter().enumerate() {
                    tokio::time::sleep(Duration::from_millis(i as u64 * 2500)).await;
                    alerts.broadcast(serde_json::json!({
                        "type": "tip",
                        "name": tip.name,
                        "amount": tip.amount,
                        "currency": tip.currency,
                        "message": tip.message,
                    }));
                }
            });

            Some(format!("Replaying the last {count} tip(s)...").into())
        }

        "songrequest" | "sr" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            if args.is_empty() {
                return Some("Usage: !songrequest <YouTube link or search terms>".into());
            }
            let query = args.join(" ");
            match song_requests.request(&query, user).await {
                Ok(RequestOutcome::NowPlaying(song)) => {
                    services.personal_playlists.add_song(user, &song).await;
                    Some(format!("Now playing: {}", song.title).into())
                }
                Ok(RequestOutcome::Queued { song, position }) => {
                    services.personal_playlists.add_song(user, &song).await;
                    Some(format!("Queued \"{}\" at position {position}.", song.title).into())
                }
                Err(err) => Some(err.to_string().into()),
            }
        }

        // !playlist <username>       — queue a random sample of that
        //                               person's saved playlist live.
        // !playlist add <song>       — save a song to your own playlist
        //                               (doesn't touch the live queue).
        // !playlist remove <n|title> — remove one of your own saved
        //                               songs, by position or title.
        "playlist" => {
            let Some(first) = args.first() else {
                return Some(
                    "View saved playlists: https://lokati.net/playlists.html — !playlist <username> to queue up \
                     to 5 random songs from theirs, !playlist <username> play <#> to queue one specific song by \
                     its position number, !playlist add/remove <song> to manage your own, !playlist \
                     clear to wipe your own playlist (mods can also !playlist clear <username>)."
                        .into(),
                );
            };

            match first.to_lowercase().as_str() {
                "add" => {
                    let Some(song_requests) = &services.song_requests else {
                        return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
                    };
                    if args.len() < 2 {
                        return Some("Usage: !playlist add <YouTube link or search terms>".into());
                    }
                    let query = args[1..].join(" ");
                    match song_requests.resolve_song_preview(&query).await {
                        Ok(song) => Some(match services.personal_playlists.add_song(user, &song).await {
                            AddOutcome::Added => format!(
                                "Added \"{}\" to your playlist. View it: https://lokati.net/playlist.html?user={}",
                                song.title,
                                user.to_lowercase()
                            )
                            .into(),
                            AddOutcome::TooLong { duration_secs } => format!(
                                "\"{}\" is {duration_secs}s long, over the 600s (10 minute) limit for playlists — not added.",
                                song.title
                            )
                            .into(),
                        }),
                        Err(err) => Some(err.to_string().into()),
                    }
                }
                "remove" => {
                    if args.len() < 2 {
                        return Some("Usage: !playlist remove <position number or song title>".into());
                    }
                    let selector = args[1..].join(" ");
                    Some(match services.personal_playlists.remove(user, &selector).await {
                        RemoveOutcome::Removed(song) => format!("Removed \"{}\" from your playlist.", song.title).into(),
                        RemoveOutcome::NotFound => format!("Couldn't find \"{selector}\" in your playlist.").into(),
                        RemoveOutcome::InvalidIndex { count } => {
                            format!("You only have {count} song(s) saved — pick a number between 1 and {count}.").into()
                        }
                        RemoveOutcome::NoPlaylist => "You don't have any saved songs yet.".into(),
                    })
                }
                // !playlist clear (no username) — anyone can clear their
                // own playlist. !playlist clear <username> — mods only,
                // clears someone else's. No "clear everyone" form at all
                // (deliberately removed — too easy to nuke every saved
                // playlist by accident).
                "clear" => {
                    let target = args.get(1).map(|s| s.trim_start_matches('@').to_string()).unwrap_or_else(|| user.to_string());
                    let is_self = target.eq_ignore_ascii_case(user);

                    if !is_self && !is_mod_or_broadcaster {
                        return Some(Reply::None);
                    }

                    let cleared = services.personal_playlists.clear_user(&target).await;
                    Some(match (cleared, is_self) {
                        (true, true) => "Cleared your playlist.".into(),
                        (true, false) => format!("Cleared {target}'s playlist.").into(),
                        (false, true) => "You don't have a saved playlist.".into(),
                        (false, false) => format!("{target} doesn't have a saved playlist.").into(),
                    })
                }
                _ => {
                    let Some(song_requests) = &services.song_requests else {
                        return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
                    };
                    let target = first.trim_start_matches('@').to_string();

                    // !playlist <username> play <position> — a specific
                    // song by its 1-based position (same numbering as the
                    // website and !playlist remove), instead of a random
                    // sample.
                    if args.get(1).is_some_and(|a| a.eq_ignore_ascii_case("play")) {
                        let Some(pos_str) = args.get(2) else {
                            return Some("Usage: !playlist <username> play <position number>".into());
                        };
                        let Ok(position) = pos_str.parse::<usize>() else {
                            return Some("Usage: !playlist <username> play <position number> — e.g. !playlist Someone play 3".into());
                        };
                        return Some(match services.personal_playlists.get_song_at(&target, position).await {
                            PlayOutcome::NoPlaylist => format!("{target} hasn't requested any songs yet.").into(),
                            PlayOutcome::InvalidIndex { display_name, count } => {
                                format!("{display_name} only has {count} song(s) saved — pick a number between 1 and {count}.").into()
                            }
                            PlayOutcome::Song { display_name, song } => {
                                let title = song.title.clone();
                                song_requests.queue_song(song);
                                format!("Queued \"{title}\" from {display_name}'s playlist.").into()
                            }
                        });
                    }

                    // Excluded from the random pool up front (not
                    // filtered after sampling), so a song already
                    // playing/queued/inserted right now never gets
                    // sampled in the first place — repeat !playlist runs
                    // actually surface something new instead of
                    // sometimes re-picking a duplicate.
                    let snapshot = song_requests.snapshot();
                    let mut already_queued: std::collections::HashSet<String> =
                        snapshot.queue.iter().map(|s| s.video_id.clone()).collect();
                    already_queued.extend(snapshot.now_playing.iter().map(|s| s.video_id.clone()));
                    already_queued.extend(snapshot.active_insert.iter().map(|s| s.video_id.clone()));

                    match services.personal_playlists.sample(&target, PLAYLIST_QUEUE_SAMPLE_SIZE, &already_queued).await
                    {
                        SampleOutcome::NoPlaylist => Some(format!("{target} hasn't requested any songs yet.").into()),
                        SampleOutcome::Empty { display_name } => {
                            Some(format!("{display_name}'s playlist is empty.").into())
                        }
                        SampleOutcome::AllAlreadyQueued { display_name } => {
                            Some(format!("Every song in {display_name}'s playlist is already playing or queued.").into())
                        }
                        SampleOutcome::Songs { display_name, songs } => {
                            let titles: Vec<String> = songs.iter().map(|s| format!("\"{}\"", s.title)).collect();
                            for song in songs {
                                song_requests.queue_song(song);
                            }
                            Some(
                                format!(
                                    "Queued {} song(s) from {display_name}'s playlist: {}",
                                    titles.len(),
                                    titles.join(", ")
                                )
                                .into(),
                            )
                        }
                    }
                }
            }
        }

        "songinsert" | "si" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            if args.is_empty() {
                return Some("Usage: !songinsert <YouTube link or search terms>".into());
            }
            let query = args.join(" ");
            match song_requests.insert_song(&query, user).await {
                Ok(SongInsertOutcome::AlreadyInserting) => {
                    Some("An inserted song is already playing — wait for it to finish first.".into())
                }
                Ok(SongInsertOutcome::Inserted { song }) => {
                    // Safety net in case the overlay never reports back
                    // (see clear_active_insert_if_stuck's doc comment) —
                    // sized to the inserted song's own length plus a
                    // generous grace period, not the song's exact end
                    // time, since the overlay is the only thing that
                    // actually knows when playback really finishes.
                    let sr = song_requests.clone();
                    let video_id = song.video_id.clone();
                    let timeout = Duration::from_secs(song.duration_secs + 30);
                    tokio::spawn(async move {
                        tokio::time::sleep(timeout).await;
                        sr.clear_active_insert_if_stuck(&video_id);
                    });
                    Some(format!("Inserted \"{}\" — playing now, will return to the playlist after.", song.title).into())
                }
                Err(err) => Some(err.to_string().into()),
            }
        }

        "settheme" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            if args.len() < 2 {
                return Some("Usage: !settheme <username> <YouTube link or search terms>".into());
            }
            let username = args[0].trim_start_matches('@').to_string();
            let query = args[1..].join(" ");

            // Resolved now (not lazily at trigger time) so the mod gets
            // immediate confirmation of what actually got matched — and
            // since this goes through the same cache as everything else,
            // it's a free cache hit for the insert right below and
            // whenever the theme actually plays later.
            match song_requests.resolve_song_preview(&query).await {
                Ok(song) => {
                    let youtube_url = format!("https://youtu.be/{}", song.video_id);
                    services.entrance_themes.set_theme(&username, youtube_url, song.title.clone()).await;

                    // Same immediate payoff as the self-service channel
                    // points redemption (see main.rs's
                    // handle_theme_redemption) — a mod setting someone's
                    // theme plays it right now instead of only mattering
                    // the next time that person happens to chat.
                    match song_requests.insert_song(&query, &username).await {
                        Ok(SongInsertOutcome::Inserted { song: inserted }) => {
                            let sr = song_requests.clone();
                            let video_id = inserted.video_id.clone();
                            let timeout = Duration::from_secs(inserted.duration_secs + 30);
                            tokio::spawn(async move {
                                tokio::time::sleep(timeout).await;
                                sr.clear_active_insert_if_stuck(&video_id);
                            });
                            Some(format!("Set {username}'s entrance theme to \"{}\" — playing now!", song.title).into())
                        }
                        Ok(SongInsertOutcome::AlreadyInserting) => Some(
                            format!(
                                "Set {username}'s entrance theme to \"{}\" — another inserted song is playing right now, so it'll play next time they chat instead.",
                                song.title
                            )
                            .into(),
                        ),
                        Err(err) => {
                            tracing::warn!("!settheme for {username}: theme set but immediate insert failed: {err}");
                            Some(format!("Set {username}'s entrance theme to \"{}\"!", song.title).into())
                        }
                    }
                }
                Err(err) => Some(err.to_string().into()),
            }
        }

        "resetgreeted" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let username = args.first().map(|s| s.trim_start_matches('@').to_string());
            services.entrance_themes.reset_greeted(username.as_deref()).await;
            Some(match username {
                Some(name) => {
                    format!("Reset today's greeting for {name} — their entrance theme (if any) will fire on their next message.").into()
                }
                None => "Reset today's greetings for everyone — entrance themes will fire again on everyone's next message.".into(),
            })
        }

        "queue" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let state = song_requests.snapshot();

            let upcoming = if state.queue.is_empty() {
                "Up next: nothing queued.".to_string()
            } else {
                let list = state
                    .queue
                    .iter()
                    .take(5)
                    .enumerate()
                    .map(|(i, song)| format!("{}. {} ({})", i + 1, song.title, song.requested_by))
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("Up next: {list}")
            };

            let recent = song_requests.recent_songs(5);
            let recent_str = if recent.is_empty() {
                "Last played: nothing yet.".to_string()
            } else {
                let list = recent.iter().map(|song| format!("{} ({})", song.title, song.requested_by)).collect::<Vec<_>>().join(" | ");
                format!("Last played: {list}")
            };

            Some(format!("{upcoming} — {recent_str}").into())
        }

        "join" => Some(match services.adventure.join(user, user).await {
            JoinOutcome::Joined => {
                format!("{user} has joined the adventure! Chatting earns your character XP over time — check !character.").into()
            }
            JoinOutcome::AlreadyJoined { level } => format!("{user}, you're already on the adventure (level {level}).").into(),
            JoinOutcome::Rejoined { level, gear_still_worn } => if gear_still_worn {
                format!("{user} (level {level}) is back on the battlefield! Your gear's still worn though — repair it on the dashboard for full stats.").into()
            } else {
                format!("{user} (level {level}) is back on the battlefield, gear fully repaired!").into()
            },
        }),

        "character" | "char" | "me" => Some(match services.adventure.character(user).await {
            Some(c) => format!(
                "{} — Level {} {:?} ({}/{} xp) — {}W/{}L — {} Thaumatergic Dust — Gear: {} — manage your character at https://adventure.lokati.net/",
                c.display_name,
                c.level,
                c.archetype,
                c.xp,
                c.xp_needed(),
                c.wins,
                c.losses,
                c.dust,
                c.gear_summary()
            )
            .into(),
            None => format!("{user}, you haven't joined the adventure yet — use !join to start!").into(),
        }),

        "party" | "adventure" => {
            let (stage, active, total) = services.adventure.party_status().await;
            Some(
                format!(
                    "The adventure is at stage {stage} with {active}/{total} hero(es) ready to fight (retreated/downed heroes don't count)! Use !join to add yours — https://adventure.lokati.net/"
                )
                .into(),
            )
        }

        "nextencounter" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            // Optional boss name (2026-08-15) - e.g. !nextencounter
            // bahamut forces a single Dragon fight with that specific
            // look, bypassing the normal random pick/rotation entirely.
            // See BossKind::parse_forced for the full recognized list.
            let forced = args.first().map(|s| s.as_str());
            match services.adventure.trigger_encounter_now(forced).await {
                // The actual battle result gets announced separately by
                // main.rs's encounter-broadcast subscriber, so nothing
                // needed here on success.
                TriggerEncounterOutcome::Triggered => Some(Reply::None),
                TriggerEncounterOutcome::NobodyJoined => Some("Nobody's joined the adventure yet — chat needs to !join first!".into()),
                TriggerEncounterOutcome::UnknownBoss => {
                    Some("Unrecognized boss - try lich, firedemon, cthulhu, dragon, bahamut, purple, or cube.".into())
                }
            }
        }

        "event" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            Some(handle_event_command(args, services).await)
        }

        "rampage" => {
            if is_mod_or_broadcaster {
                services.adventure.start_rampage().await;
                return Some("🔥 RAMPAGE! The next 50 encounters are all boss fights, one every ~60s (or as soon as the last one finishes playing out) — everyone gets instantly revived between them too. Buckle up!".into());
            }
            // !rampage vote (2026-08-17, a live request: "if 3 or more
            // players use !rampage it will start a rampage without a mod
            // activating the command, similar to a vote") - anyone else's
            // !rampage counts as a vote instead of being a silent no-op.
            match services.adventure.register_rampage_vote(user).await {
                RampageVoteOutcome::Triggered => {
                    Some("🔥 RAMPAGE! The vote passed — the next 50 encounters are all boss fights, one every ~60s (or as soon as the last one finishes playing out) — everyone gets instantly revived between them too. Buckle up!".into())
                }
                RampageVoteOutcome::Counted(count) => {
                    Some(format!("🗳️ Rampage vote: {count}/{RAMPAGE_VOTE_THRESHOLD} — {} more and it's on!", RAMPAGE_VOTE_THRESHOLD.saturating_sub(count)).into())
                }
            }
        }

        "clearbattlefield" | "resetbattlefield" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let affected = services.adventure.clear_battlefield().await;
            Some(if affected > 0 {
                format!("🧹 Battlefield cleared — {affected} hero(es) were pulled from the field and need to !join again to rejoin!").into()
            } else {
                "Nobody was on the battlefield to clear.".into()
            })
        }

        "giveloot" | "gearall" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let count = services.adventure.grant_random_gear_to_all().await;
            Some(if count > 0 {
                format!("🎁 Dropped fresh gear into {count} heroes' bags! Equip it from the adventure dashboard.").into()
            } else {
                "Nobody's joined the adventure yet — chat needs to !join first!".into()
            })
        }

        "giftdust" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(target) = args.first() else {
                return Some("Usage: !giftdust <all|username> <amount>".into());
            };
            let Some(amount_str) = args.get(1) else {
                return Some("Usage: !giftdust <all|username> <amount>".into());
            };
            let Ok(amount) = amount_str.parse::<u64>() else {
                return Some(format!("\"{amount_str}\" isn't a valid dust amount.").into());
            };
            if amount == 0 {
                return Some("Amount has to be more than 0.".into());
            }
            if target.eq_ignore_ascii_case("all") {
                let count = services.adventure.grant_dust_to_all(amount).await;
                Some(if count > 0 {
                    format!("💰 Gave {amount} dust to all {count} heroes!").into()
                } else {
                    "Nobody's joined the adventure yet — chat needs to !join first!".into()
                })
            } else {
                let username = target.trim_start_matches('@');
                Some(if services.adventure.grant_dust(username, amount).await {
                    format!("💰 Gave {amount} dust to {username}!").into()
                } else {
                    format!("{username} hasn't joined the adventure yet.").into()
                })
            }
        }

        "nowplaying" | "np" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let state = song_requests.snapshot();
            match state.active_insert.or(state.now_playing) {
                Some(song) => Some(format!("Now playing: {} (requested by {})", song.title, song.requested_by).into()),
                None => Some("Nothing is playing right now.".into()),
            }
        }

        "song" | "currentsong" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let state = song_requests.snapshot();
            match state.active_insert.or(state.now_playing) {
                Some(song) => Some(
                    format!(
                        "{} (requested by {}) — https://www.youtube.com/watch?v={}",
                        song.title, song.requested_by, song.video_id
                    )
                    .into(),
                ),
                None => Some("Nothing is playing right now.".into()),
            }
        }

        "skip" | "modskip" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            // A theme (!songinsert/!si or an entrance theme) playing right
            // now is what's actually on stream — the main queue's
            // now_playing is just sitting there interrupted, so skipping
            // *that* wouldn't stop what's audible. Cut the insert off
            // instead when one's active, and only fall through to the
            // normal queue-advance skip otherwise.
            if song_requests.skip_insert() {
                return Some("Skipped — resuming the playlist.".into());
            }
            Some(match song_requests.advance() {
                Some(song) => format!("Skipped. Now playing: {}", song.title).into(),
                None => "Skipped. The queue is now empty.".into(),
            })
        }

        "voteskip" | "vs" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            Some(match song_requests.vote_skip(user) {
                VoteSkipOutcome::NothingPlaying => "Nothing is playing right now.".into(),
                VoteSkipOutcome::AlreadyVoted => format!("{user}, you've already voted to skip this song.").into(),
                VoteSkipOutcome::Locked => "Voteskip is disabled for this song.".into(),
                VoteSkipOutcome::OnCooldown { remaining_secs } => {
                    format!("{user}, you can !voteskip or redeem Interrupt the Music again in {remaining_secs}s.").into()
                }
                VoteSkipOutcome::Recorded { count, threshold } => {
                    format!("Vote to skip: {count}/{threshold}. Use !voteskip to vote!").into()
                }
                VoteSkipOutcome::Skipped { new_now_playing } => match new_now_playing {
                    Some(song) => format!("Vote to skip passed! Now playing: {}", song.title).into(),
                    None => "Vote to skip passed! The queue is now empty.".into(),
                },
                VoteSkipOutcome::SelfSkipped { new_now_playing } => match new_now_playing {
                    Some(song) => format!("{user} skipped their own song. Now playing: {}", song.title).into(),
                    None => format!("{user} skipped their own song. The queue is now empty.").into(),
                },
            })
        }

        "votepause" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            Some(match song_requests.vote_pause(user) {
                VotePauseOutcome::NothingPlaying => "Nothing is playing right now.".into(),
                VotePauseOutcome::AlreadyPaused => "The song is already paused.".into(),
                VotePauseOutcome::AlreadyVoted => format!("{user}, you've already voted to pause this song.").into(),
                VotePauseOutcome::Recorded { count, threshold } => {
                    format!("Vote to pause: {count}/{threshold}. Use !votepause to vote!").into()
                }
                VotePauseOutcome::Paused => "Vote to pause passed! Song paused.".into(),
            })
        }

        "votestart" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            Some(match song_requests.vote_resume(user) {
                VoteResumeOutcome::NotPaused => "The song isn't paused right now.".into(),
                VoteResumeOutcome::AlreadyVoted => format!("{user}, you've already voted to resume this song.").into(),
                VoteResumeOutcome::Recorded { count, threshold } => {
                    format!("Vote to resume: {count}/{threshold}. Use !votestart to vote!").into()
                }
                VoteResumeOutcome::Resumed => "Vote to resume passed! Song resumed.".into(),
                VoteResumeOutcome::Scheduled { remaining_secs } => {
                    let sr = song_requests.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(remaining_secs)).await;
                        sr.resume_now();
                    });
                    format!("Vote to resume passed! Resuming in {remaining_secs}s (pause cooldown).").into()
                }
                VoteResumeOutcome::AlreadyScheduled { remaining_secs } => {
                    format!("Vote to resume already passed — resuming in {remaining_secs}s.").into()
                }
            })
        }

        "votevolume" | "vv" => {
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let Some((obs, source_name)) = &services.obs_song_volume else {
                return Some("!votevolume isn't configured — set OBS_WEBSOCKET_URL/OBS_SONG_SOURCE_NAME in .env.".into());
            };
            let Some(raw) = args.first() else {
                return Some(format!("Usage: !votevolume <{MIN_VOTE_VOLUME}-{MAX_VOTE_VOLUME}>").into());
            };
            let Ok(requested) = raw.parse::<i32>() else {
                return Some(format!("\"{raw}\" isn't a number. Usage: !votevolume <{MIN_VOTE_VOLUME}-{MAX_VOTE_VOLUME}>").into());
            };
            let clamped = requested.clamp(MIN_VOTE_VOLUME as i32, MAX_VOTE_VOLUME as i32) as u8;
            let clamp_note = if clamped as i32 != requested {
                format!(" (clamped to {clamped} — valid range is {MIN_VOTE_VOLUME}-{MAX_VOTE_VOLUME})")
            } else {
                String::new()
            };
            Some(match song_requests.vote_volume(user, clamped) {
                VoteVolumeOutcome::AlreadyVoted => format!("{user}, you've already voted for {clamped}%.").into(),
                VoteVolumeOutcome::Recorded { count, threshold, percent } => format!(
                    "Vote for {percent}% volume: {count}/{threshold}{clamp_note}. Use !votevolume <{MIN_VOTE_VOLUME}-{MAX_VOTE_VOLUME}> to vote!"
                )
                .into(),
                VoteVolumeOutcome::Applied { percent } => match obs.set_input_volume(source_name, percent).await {
                    Ok(()) => format!("Vote passed! Volume set to {percent}%{clamp_note}.").into(),
                    Err(err) => {
                        tracing::error!("!votevolume: OBS set_input_volume failed: {err}");
                        format!("Vote passed ({percent}%), but couldn't reach OBS to actually apply it: {err}").into()
                    }
                },
            })
        }

        "modpause" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            Some(if song_requests.mod_pause() {
                "Song paused.".into()
            } else {
                "Nothing is playing right now.".into()
            })
        }

        "modstart" | "modresume" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            song_requests.resume_now();
            Some("Song resumed.".into())
        }

        "modvolume" | "modvv" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let Some((obs, source_name)) = &services.obs_song_volume else {
                return Some("!modvolume isn't configured — set OBS_WEBSOCKET_URL/OBS_SONG_SOURCE_NAME in .env.".into());
            };
            let Some(raw) = args.first() else {
                return Some("Usage: !modvolume <0-100>".into());
            };
            let Ok(requested) = raw.parse::<i32>() else {
                return Some(format!("\"{raw}\" isn't a number. Usage: !modvolume <0-100>").into());
            };
            let clamped = requested.clamp(0, 100) as u8;
            song_requests.mod_clear_volume_votes();
            Some(match obs.set_input_volume(source_name, clamped).await {
                Ok(()) => format!("Volume set to {clamped}%.").into(),
                Err(err) => {
                    tracing::error!("!modvolume: OBS set_input_volume failed: {err}");
                    format!("Couldn't reach OBS to set volume: {err}").into()
                }
            })
        }

        "forceplay" => {
            if !is_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            Some(if song_requests.lock_voteskip() {
                "Voteskip disabled for this song — it's playing through.".into()
            } else {
                "Nothing is playing right now.".into()
            })
        }

        "clearqueue" | "modclear" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let Some(song_requests) = &services.song_requests else {
                return Some("Song requests aren't enabled — set YOUTUBE_API_KEY in .env.".into());
            };
            let count = song_requests.clear_queue();
            Some(format!("Cleared {count} song(s) from the queue.").into())
        }

        // !bugreport <message> — anyone; free-text, persisted to
        // bugreports.json for later review/discussion. Deliberately NOT
        // on the shared 5s builtin cooldown (that's per-command-name, not
        // per-user, and would let one person's report block everyone
        // else's) — BugReportManager enforces its own per-user cooldown
        // instead, same reasoning as StaticCommands' per-user cooldown.
        "bugreport" => {
            if args.is_empty() {
                return Some(
                    "Usage: !bugreport <what happened> — e.g. !bugreport divine healing buff never procs even though heals are landing".into(),
                );
            }
            let text = args.join(" ");
            Some(match services.bug_reports.submit(user, &text).await {
                SubmitOutcome::Recorded { id } => format!("Thanks {user}, logged as bug report #{id} — Lokati will take a look!").into(),
                SubmitOutcome::OnCooldown { remaining_secs } => {
                    format!("{user}, you can submit another bug report in {remaining_secs}s.").into()
                }
            })
        }

        "bugreports" => {
            if !is_mod_or_broadcaster {
                return Some(Reply::None);
            }
            let recent = services.bug_reports.recent(5).await;
            if recent.is_empty() {
                return Some("No bug reports yet.".into());
            }
            Some(Reply::Many(recent.iter().map(|r| format!("#{} {}: {}", r.id, r.user, r.text)).collect()))
        }

        _ => None,
    }
}

/// `!event intro <boss name>` (2026-08-17, a live request: announce a new
/// boss the moment it ships, and stay reusable for every future boss too)
/// - forces a fight guaranteed to showcase the named boss at the current
/// stage's normal boss COUNT (see `AdventureManager::trigger_boss_intro`),
/// then posts a chat announcement linking to that boss's wiki writeup.
/// Only `intro` exists today, but this is a `!command`-style dispatcher
/// (sub-verb in `args[0]`) so future `!event` subcommands have somewhere
/// to land without a new top-level command each time.
async fn handle_event_command(args: &[String], services: &Services) -> Reply {
    let action = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    if action != "intro" {
        return "Usage: !event intro <boss name> - e.g. !event intro cube".into();
    }
    let Some(name) = args.get(1) else {
        return "Usage: !event intro <boss name> - e.g. !event intro cube".into();
    };
    match services.adventure.trigger_boss_intro(name).await {
        TriggerEncounterOutcome::Triggered => {
            // Re-parse just to get the kind's display name/slug -
            // trigger_boss_intro already validated the name, so this
            // can't actually fail.
            let Some((kind, _)) = BossKind::parse_forced(name) else {
                return Reply::None;
            };
            format!(
                "🆕 NEW BOSS ALERT: {} has entered the fray! Read up before the fight: https://adventure.lokati.net/wiki#{}",
                kind.display_name(),
                kind.wiki_slug(),
            )
            .into()
        }
        TriggerEncounterOutcome::NobodyJoined => "Nobody's joined the adventure yet — chat needs to !join first!".into(),
        TriggerEncounterOutcome::UnknownBoss => "Unrecognized boss - try lich, firedemon, cthulhu, dragon, bahamut, purple, or cube.".into(),
    }
}

async fn handle_command_management(args: &[String], services: &Services) -> Reply {
    let action = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    if !matches!(action.as_str(), "add" | "edit" | "delete" | "remove") {
        return "Usage: !command add !name <reply text> | !command edit !name <reply text> | !command delete !name"
            .into();
    }

    let target = args.get(1).map(|s| s.trim_start_matches('!').to_lowercase()).unwrap_or_default();
    if target.is_empty() {
        let needs_reply = !matches!(action.as_str(), "delete" | "remove");
        return format!("Usage: !command {action} !name{}", if needs_reply { " <reply text>" } else { "" }).into();
    }

    // Hand-written commands can't be managed this way — check by trying a
    // no-op dispatch would be overkill; just check against the known list.
    const BUILTIN_NAMES: &[&str] = &[
        "hug", "uptime", "time", "commands", "handsome", "thatsagoodbuild", "command", "announcetest",
        "checkpatreon", "alerttest", "replaytips", "songrequest", "sr", "queue", "nowplaying", "np", "song",
        "currentsong", "skip", "modskip", "clearqueue", "modclear", "voteskip", "vs", "votepause", "votestart", "votevolume", "vv",
        "forceplay", "modpause", "modstart", "modresume", "modvolume", "modvv", "songinsert", "si",
        "essenceprofit", "ep", "ritualprofit", "rp", "vesselprice", "vp", "price", "settheme", "resetgreeted", "theme", "themes", "playlist", "playrandom",
        "join", "character", "char", "me", "party", "adventure", "nextencounter", "event", "giveloot", "gearall", "giftdust",
        "bugreport", "bugreports", "rampage",
    ];
    if BUILTIN_NAMES.contains(&target.as_str()) {
        return format!("!{target} is a built-in command and can't be managed this way.").into();
    }

    if matches!(action.as_str(), "delete" | "remove") {
        return if services.static_commands.remove(&target).await {
            format!("Deleted !{target}.").into()
        } else {
            format!("!{target} doesn't exist.").into()
        };
    }

    let reply = args[2..].join(" ");
    if reply.trim().is_empty() {
        return format!("Give me the reply text, e.g. !command {action} !{target} some text here").into();
    }

    let existed = services.static_commands.exists(&target).await;
    if action == "edit" && !existed {
        return format!("!{target} doesn't exist yet — use !command add !{target} ... to create it.").into();
    }
    if action == "add" && existed {
        return format!("!{target} already exists — use !command edit !{target} ... to change it.").into();
    }

    let previously_existed = services.static_commands.upsert(&target, &reply).await;
    if previously_existed {
        format!("Updated !{target}.").into()
    } else {
        format!("Added !{target}.").into()
    }
}
