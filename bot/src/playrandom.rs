// !playrandom — queues (or, in continuous mode, keeps queuing) songs
// "similar" to the last 5 *distinct* genres actually played. YouTube has
// no genre metadata at all, so "similar" is approximated via Last.fm:
//
// 1. Walk the recent play history newest-first, best-effort parsing
//    "Artist - Track" out of each song's title (YouTube titles aren't
//    structured data — this is a heuristic, not guaranteed to work on
//    every title) and looking up its top genre tag via Last.fm's
//    track.getTopTags (falling back to artist.getTopTags if a track has
//    none tagged). Collect *distinct* tags as they're found, stopping
//    once 5 different ones are seen — a run of same-genre songs in a row
//    only counts once, so this reflects genre *variety* recently played,
//    not just whatever the last handful of songs happened to be.
// 2. Pull each of those 5 tags' top tracks from Last.fm (tag.getTopTracks)
//    as candidates, pool them all together, skip anything already in the
//    recent history, and resolve the rest through the normal YouTube
//    search path (song_requests.resolve_song_preview) to get something
//    actually playable — a candidate that doesn't resolve (no good
//    YouTube match, too long, etc.) is just skipped rather than failing
//    the batch.

use crate::song_requests::{Song, SongRequestManager};
use rand::seq::SliceRandom;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

const LASTFM_BASE_URL: &str = "http://ws.audioscrobbler.com/2.0/";

/// How many distinct recent genres to base candidate discovery on.
const RECENT_GENRES_WANTED: usize = 5;

/// How many candidate tracks to pull from Last.fm per genre lookup —
/// generous over-fetch since plenty won't resolve to a good YouTube match
/// or will already be in recent history.
const CANDIDATES_PER_LOOKUP: u32 = 50;

/// Continuous mode (!playrandom on) tops the queue up by this many songs
/// whenever it drops below this length.
const CONTINUOUS_TOPUP_THRESHOLD: usize = 2;
const CONTINUOUS_TOPUP_BATCH: usize = 3;

/// Floor between continuous-mode top-up attempts regardless of outcome —
/// without this, a persistent failure (Last.fm down, a genre with no
/// resolvable candidates) would retry on every single queue-state
/// broadcast, which fire far more often than just "a song changed".
const CONTINUOUS_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum PlayRandomError {
    #[error("Not enough play history yet — get a few more songs played first.")]
    NotEnoughHistory,
    #[error("Couldn't figure out a genre from the recent songs.")]
    NoGenreFound,
    #[error("Couldn't find any new songs for that genre right now — try again in a bit.")]
    NoCandidatesResolved,
}

/// Last.fm's JSON (converted from their original XML API) returns a bare
/// object instead of a 1-element array when there's exactly one result —
/// this normalizes both shapes into a Vec so serde doesn't choke on it.
fn one_or_many<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany<T> {
        One(T),
        Many(Vec<T>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(v) => vec![v],
        OneOrMany::Many(v) => v,
    })
}

#[derive(Deserialize)]
struct TopTagsResponse {
    toptags: Option<TopTags>,
}
#[derive(Deserialize)]
struct TopTags {
    #[serde(default, deserialize_with = "one_or_many")]
    tag: Vec<TagEntry>,
}
#[derive(Deserialize)]
struct TagEntry {
    name: String,
}

#[derive(Deserialize)]
struct TopTracksResponse {
    // Last.fm's real response wraps this in "tracks", not "toptracks" as
    // their own API docs claim — verified directly against a live
    // request. Getting this wrong doesn't error, since the field is
    // Option: it just silently deserializes to None, which is exactly
    // how this bug hid as "0 candidates" with no error anywhere.
    tracks: Option<TopTracks>,
}
#[derive(Deserialize)]
struct TopTracks {
    #[serde(default, deserialize_with = "one_or_many")]
    track: Vec<TrackEntry>,
}
#[derive(Deserialize)]
struct TrackEntry {
    name: String,
    artist: ArtistRef,
}
#[derive(Deserialize)]
struct ArtistRef {
    name: String,
}

/// Best-effort "Artist - Track" split out of a YouTube video title.
/// Strips common decorations first ("(Official Video)", "[HD]", "(Lyrics)",
/// etc.) then splits on the first " - ". Titles that don't look like that
/// shape at all (no " - ", or empty either side) just return None — the
/// caller skips that song for genre purposes rather than guessing wrong.
fn parse_artist_track(title: &str) -> Option<(String, String)> {
    static SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)[\(\[]\s*(official\s*(music\s*)?video|official\s*audio|lyrics?(\s*video)?|hd|4k|remaster(ed)?|explicit|audio|visualizer)\s*[\)\]]").unwrap()
    });
    let cleaned = SUFFIX_RE.replace_all(title, "");
    let (artist, track) = cleaned.trim().split_once(" - ")?;
    let artist = artist.trim();
    let track = track.trim();
    if artist.is_empty() || track.is_empty() {
        return None;
    }
    Some((artist.to_string(), track.to_string()))
}

async fn fetch_top_tags(http: &reqwest::Client, api_key: &str, artist: &str, track: &str) -> Vec<String> {
    let track_tags = http
        .get(LASTFM_BASE_URL)
        .query(&[
            ("method", "track.gettoptags"),
            ("artist", artist),
            ("track", track),
            ("api_key", api_key),
            ("format", "json"),
            ("autocorrect", "1"),
        ])
        .send()
        .await
        .ok();

    let tags: Vec<String> = match track_tags {
        Some(resp) => resp
            .json::<TopTagsResponse>()
            .await
            .ok()
            .and_then(|d| d.toptags)
            .map(|t| t.tag.into_iter().map(|t| t.name).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    if !tags.is_empty() {
        return tags;
    }

    // This specific track has no tags on Last.fm — fall back to the
    // artist's own top tags instead of giving up on it entirely.
    let artist_tags = http
        .get(LASTFM_BASE_URL)
        .query(&[("method", "artist.gettoptags"), ("artist", artist), ("api_key", api_key), ("format", "json"), ("autocorrect", "1")])
        .send()
        .await
        .ok();
    match artist_tags {
        Some(resp) => resp
            .json::<TopTagsResponse>()
            .await
            .ok()
            .and_then(|d| d.toptags)
            .map(|t| t.tag.into_iter().map(|t| t.name).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Walks `history` newest-first, deriving each song's top genre tag, and
/// collects up to `RECENT_GENRES_WANTED` *distinct* ones — a repeat of a
/// genre already collected doesn't count again, so a run of same-genre
/// songs in a row only contributes one entry. Stops as soon as enough
/// distinct genres are found, or the history runs out.
async fn derive_recent_genres(http: &reqwest::Client, api_key: &str, history: &[Song]) -> Vec<String> {
    let mut genres: Vec<String> = Vec::new();
    // history is oldest-first (pushed to the back as each song finishes)
    // — walk it newest-first so "recent" actually means recent.
    for song in history.iter().rev() {
        if genres.len() >= RECENT_GENRES_WANTED {
            break;
        }
        let Some((artist, track)) = parse_artist_track(&song.title) else {
            tracing::info!("!playrandom: couldn't parse \"Artist - Track\" out of \"{}\", skipping for genre purposes.", song.title);
            continue;
        };
        let tags = fetch_top_tags(http, api_key, &artist, &track).await;
        let Some(top_tag) = tags.into_iter().next() else {
            tracing::info!("!playrandom: no Last.fm tags found at all for \"{artist} - {track}\" (track or artist).");
            continue;
        };
        let top_tag = top_tag.to_lowercase();
        if genres.contains(&top_tag) {
            continue;
        }
        tracing::info!("!playrandom: \"{artist} - {track}\" -> genre \"{top_tag}\" ({}/{RECENT_GENRES_WANTED} distinct so far)", genres.len() + 1);
        genres.push(top_tag);
    }
    tracing::info!("!playrandom: recent distinct genres = {genres:?}");
    genres
}

async fn fetch_tag_candidates(http: &reqwest::Client, api_key: &str, tag: &str) -> Vec<(String, String)> {
    let resp = http
        .get(LASTFM_BASE_URL)
        .query(&[
            ("method", "tag.gettoptracks"),
            ("tag", tag),
            ("api_key", api_key),
            ("format", "json"),
            ("limit", &CANDIDATES_PER_LOOKUP.to_string()),
        ])
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("!playrandom: tag.gettoptracks request for \"{tag}\" failed: {err}");
            return Vec::new();
        }
    };

    let status = resp.status();
    let body = match resp.text().await {
        Ok(b) => b,
        Err(err) => {
            tracing::warn!("!playrandom: tag.gettoptracks response body for \"{tag}\" unreadable: {err}");
            return Vec::new();
        }
    };

    if !status.is_success() {
        tracing::warn!("!playrandom: tag.gettoptracks for \"{tag}\" returned HTTP {status}: {body}");
        return Vec::new();
    }

    match serde_json::from_str::<TopTracksResponse>(&body) {
        Ok(data) => {
            let candidates: Vec<(String, String)> =
                data.tracks.map(|t| t.track.into_iter().map(|tr| (tr.artist.name, tr.name)).collect()).unwrap_or_default();
            tracing::info!("!playrandom: tag \"{tag}\" -> {} candidate(s) from Last.fm", candidates.len());
            candidates
        }
        Err(err) => {
            tracing::warn!("!playrandom: couldn't parse tag.gettoptracks response for \"{tag}\": {err}. Body: {body}");
            Vec::new()
        }
    }
}

/// Where !playrandom on/off's state survives a restart - without this, the
/// in-memory-only flag silently reset to off on every deploy, which looked
/// like continuous mode randomly stopping on its own (it wasn't random -
/// it was every single bot restart).
const STATE_PATH: &str = "playrandom-state.json";

/// Appends one play-log entry and drops the oldest until the log is back
/// inside `PLAY_LOG_MAX_ENTRIES`.
///
/// Separate from the write path so the ceiling is testable without
/// touching the filesystem — the log's real path is a bare CWD-relative
/// literal, like every other bot data file, so exercising
/// `record_random_play` itself in a test would write into the repo.
///
/// Drains in one go rather than popping one at a time: a log that was
/// already over the ceiling (an older file, or a lowered constant) comes
/// straight back into range on the next write instead of taking one play
/// per excess entry to get there.
fn append_capped(log: &mut Vec<PlayLogEntry>, entry: PlayLogEntry) {
    log.push(entry);
    if log.len() > PLAY_LOG_MAX_ENTRIES {
        log.drain(..log.len() - PLAY_LOG_MAX_ENTRIES);
    }
}

/// Whether anything a person actually asked for is still waiting in the
/// queue. Continuous mode stays out of the way until it drains — see
/// `maybe_top_up`. Kept as a free function so the rule is testable
/// without a live queue or a Last.fm round trip.
pub(crate) fn requests_are_pending(queue: &[Song]) -> bool {
    queue.iter().any(|song| !song.is_random())
}

/// Videos !playrandom offered that the overlay could not actually play.
/// Persisted beside `playrandom-state.json` so a restart does not start
/// offering them again — the whole point is that each one costs a dead
/// slot on stream exactly once.
const BLOCKLIST_PATH: &str = "playrandom-blocklist.json";

/// Every random song that actually reached the stream (timestamp, video
/// id, title). A JSON array rather than one object per line because
/// backup-bot-data.ps1 parses every file it copies with ConvertFrom-Json
/// to verify the snapshot, and JSONL would fail that check.
///
/// Capped at `PLAY_LOG_MAX_ENTRIES`, oldest dropped — see that constant.
const PLAY_LOG_PATH: &str = "playrandom-log.json";

/// The play log is the only backed-up bot file that would otherwise grow
/// without bound, and it is copied into every hourly snapshot, so its
/// size is multiplied by however many snapshots retention is holding
/// (up to 54). At roughly 100 bytes an entry this ceiling puts the file
/// around a megabyte at worst, which is the point: bounded, and still
/// far more history than anyone will read.
const PLAY_LOG_MAX_ENTRIES: usize = 10_000;

/// The no-repeat window: a video in the last this-many random plays is
/// not offered again. When fewer than this have been played, the window
/// is simply all of them — which is what a capped history gives for
/// free, and is what keeps a small pool playable instead of starving.
const NO_REPEAT_WINDOW: usize = 100;

/// Where the ids behind that window live, so a restart does not forget.
const HISTORY_PATH: &str = "playrandom-history.json";

#[derive(Default, Serialize, Deserialize)]
struct PersistedState {
    enabled: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedHistory {
    /// Oldest first, capped at `NO_REPEAT_WINDOW`.
    played: VecDeque<String>,
}

#[derive(Serialize, Deserialize)]
struct PlayLogEntry {
    /// RFC 3339, so the log is readable without tooling.
    at: String,
    video_id: String,
    title: String,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedBlocklist {
    /// Video id -> why it was dropped. The reason is kept rather than a
    /// bare set because the next person to read this file will want to
    /// know whether it was a regional block that might lapse or a video
    /// that is simply gone.
    blocked: HashMap<String, BlockedEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct BlockedEntry {
    reason: String,
    title: String,
    /// RFC 3339, so the file is readable without tooling.
    at: String,
}

pub struct PlayRandomManager {
    http: reqwest::Client,
    lastfm_api_key: String,
    enabled: AtomicBool,
    /// Guards against overlapping continuous-mode top-up attempts, and
    /// throttles retries after a failed one — see CONTINUOUS_RETRY_COOLDOWN.
    last_topup_attempt: Mutex<Option<Instant>>,
    topping_up: AtomicBool,
    blocklist: Mutex<PersistedBlocklist>,
    history: Mutex<PersistedHistory>,
}

impl PlayRandomManager {
    pub fn new(lastfm_api_key: String) -> Arc<Self> {
        let persisted: PersistedState = crate::state::load_json(STATE_PATH).unwrap_or_default();
        Arc::new(Self {
            http: reqwest::Client::new(),
            lastfm_api_key,
            enabled: AtomicBool::new(persisted.enabled),
            last_topup_attempt: Mutex::new(None),
            topping_up: AtomicBool::new(false),
            blocklist: Mutex::new(crate::state::load_json(BLOCKLIST_PATH).unwrap_or_default()),
            history: Mutex::new(crate::state::load_json(HISTORY_PATH).unwrap_or_default()),
        })
    }

    /// Whether this video is inside the no-repeat window.
    fn was_recently_played(&self, video_id: &str) -> bool {
        self.history.lock().unwrap().played.iter().any(|id| id == video_id)
    }

    /// Records a random song that actually reached the stream: into the
    /// no-repeat window, and into the play log.
    ///
    /// Deliberately called when a song STARTS PLAYING, not when it is
    /// queued. A queued random song can be retired unplayed by a request
    /// (see song_requests::queue_song), and logging those would both lie
    /// about what was played and spend the no-repeat window on songs
    /// nobody heard.
    pub fn record_random_play(&self, video_id: &str, title: &str) {
        {
            let mut history = self.history.lock().unwrap();
            history.played.push_back(video_id.to_string());
            while history.played.len() > NO_REPEAT_WINDOW {
                history.played.pop_front();
            }
            if let Err(err) = crate::state::save_json(HISTORY_PATH, &*history) {
                tracing::error!("Failed to persist {HISTORY_PATH}: {err}");
            }
        }

        let mut log: Vec<PlayLogEntry> = crate::state::load_json(PLAY_LOG_PATH).unwrap_or_default();
        append_capped(
            &mut log,
            PlayLogEntry {
                at: chrono::Utc::now().to_rfc3339(),
                video_id: video_id.to_string(),
                title: title.to_string(),
            },
        );
        if let Err(err) = crate::state::save_json(PLAY_LOG_PATH, &log) {
            tracing::error!("Failed to append to {PLAY_LOG_PATH}: {err}");
        }
    }

    /// Watches what is actually on stream and logs each random song once
    /// it starts. Spawned once from main.rs.
    pub fn spawn_play_log_watcher(self: Arc<Self>, song_requests: Arc<SongRequestManager>) {
        let mut rx = song_requests.subscribe();
        tokio::spawn(async move {
            // State broadcasts fire for volume, mute and queue edits too,
            // not just song changes, so the same song arrives many times
            // over. Only a CHANGE of now_playing is a play.
            let mut last_logged: Option<String> = None;
            while let Ok(state) = rx.recv().await {
                let Some(now_playing) = state.now_playing else {
                    last_logged = None;
                    continue;
                };
                if last_logged.as_deref() == Some(now_playing.video_id.as_str()) {
                    continue;
                }
                last_logged = Some(now_playing.video_id.clone());
                if now_playing.is_random() {
                    self.record_random_play(&now_playing.video_id, &now_playing.title);
                }
            }
        });
    }

    /// Records a video !playrandom offered that could not be played, so
    /// it is never offered again. Only called for random songs — see
    /// `PlaybackErrorEvent::was_random`.
    pub fn blocklist_video(&self, video_id: &str, title: &str, reason: &str) {
        let mut blocklist = self.blocklist.lock().unwrap();
        blocklist.blocked.insert(
            video_id.to_string(),
            BlockedEntry {
                reason: reason.to_string(),
                title: title.to_string(),
                at: chrono::Utc::now().to_rfc3339(),
            },
        );
        if let Err(err) = crate::state::save_json(BLOCKLIST_PATH, &*blocklist) {
            tracing::error!("Failed to persist {BLOCKLIST_PATH}: {err}");
        }
        tracing::info!("!playrandom: blocklisted {video_id} (\"{title}\") — {reason}");
    }

    fn is_blocklisted(&self, video_id: &str) -> bool {
        self.blocklist.lock().unwrap().blocked.contains_key(video_id)
    }

    /// Subscribes to playback errors and blocklists the random ones.
    /// Spawned once from main.rs. A viewer's own failed request is left
    /// alone deliberately: they may well ask for it again, and it is
    /// their slot to waste.
    pub fn spawn_blocklist_watcher(self: Arc<Self>, song_requests: Arc<SongRequestManager>) {
        let mut rx = song_requests.subscribe_playback_errors();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if event.was_random {
                    self.blocklist_video(&event.video_id, &event.title, &event.reason);
                }
            }
        });
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if let Err(err) = crate::state::save_json(STATE_PATH, &PersistedState { enabled }) {
            tracing::error!("Failed to persist {STATE_PATH}: {err}");
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// !playrandom <1-5> — finds up to `count` new songs sharing one of
    /// the last 5 *distinct* genres actually played, resolved to
    /// actually-playable YouTube videos. Best-effort: a candidate that
    /// doesn't resolve or is already recently played is just skipped,
    /// not a failure, as long as *something* comes back.
    pub async fn find_similar_songs(
        &self,
        song_requests: &Arc<SongRequestManager>,
        count: usize,
    ) -> Result<(Vec<String>, Vec<Song>), PlayRandomError> {
        // recent_history() only holds songs that have *finished* — the
        // currently-playing one isn't added until it ends (see advance()).
        // Without including it here, !playrandom has zero genre context
        // right after the very first song of a session starts, which is
        // exactly when an empty/thin queue makes someone want to use it.
        let mut history = song_requests.recent_history();
        if let Some(now_playing) = song_requests.snapshot().now_playing {
            history.push(now_playing);
        }
        if history.is_empty() {
            return Err(PlayRandomError::NotEnoughHistory);
        }

        let genres = derive_recent_genres(&self.http, &self.lastfm_api_key, &history).await;
        if genres.is_empty() {
            return Err(PlayRandomError::NoGenreFound);
        }

        let already_known: HashSet<String> = history.iter().map(|s| s.title.to_lowercase()).collect();

        // Pool candidates from every recent genre together rather than
        // picking just one — a candidate showing up under more than one
        // of the 5 genres is deduped before resolving anything, so it
        // doesn't waste a resolve attempt or get queued twice.
        let mut seen_candidates: HashSet<(String, String)> = HashSet::new();
        let mut candidates: Vec<(String, String)> = Vec::new();
        for genre in &genres {
            for candidate in fetch_tag_candidates(&self.http, &self.lastfm_api_key, genre).await {
                let key = (candidate.0.to_lowercase(), candidate.1.to_lowercase());
                if seen_candidates.insert(key) {
                    candidates.push(candidate);
                }
            }
        }
        if candidates.is_empty() {
            tracing::warn!("!playrandom: genres {genres:?} had zero candidates from Last.fm combined — nothing to resolve.");
        }
        candidates.shuffle(&mut rand::thread_rng());

        let mut resolved = Vec::new();
        let mut already_known_skips = 0u32;
        let mut blocklisted_skips = 0u32;
        let mut recently_played_skips = 0u32;
        let mut resolve_failures = 0u32;
        for (artist, track) in candidates {
            if resolved.len() >= count {
                break;
            }
            let query = format!("{artist} {track}");
            if already_known.contains(&query.to_lowercase()) {
                already_known_skips += 1;
                continue;
            }
            match song_requests.resolve_song_preview(&query).await {
                Ok(song) if already_known.contains(&song.title.to_lowercase()) => {
                    already_known_skips += 1;
                }
                // Offered before and the overlay could not play it. The
                // id is only known after resolving, so this filters here
                // rather than above the resolve.
                Ok(song) if self.is_blocklisted(&song.video_id) => {
                    blocklisted_skips += 1;
                }
                // Inside the no-repeat window. The candidate list is
                // already shuffled and finite, so "draw again" is just
                // continuing this loop — which bounds the redraws at the
                // pool Last.fm returned rather than spinning.
                Ok(song) if self.was_recently_played(&song.video_id) => {
                    recently_played_skips += 1;
                }
                Ok(song) => {
                    tracing::info!("!playrandom: resolved candidate \"{query}\" -> \"{}\"", song.title);
                    resolved.push(song);
                }
                Err(err) => {
                    resolve_failures += 1;
                    tracing::info!("!playrandom: candidate \"{query}\" didn't resolve: {err}");
                }
            }
        }
        tracing::info!(
            "!playrandom: genres {genres:?} -> {} resolved, {already_known_skips} already-known skip(s), {blocklisted_skips} blocklisted skip(s), {recently_played_skips} no-repeat skip(s), {resolve_failures} resolve failure(s)",
            resolved.len()
        );

        if resolved.is_empty() {
            return Err(PlayRandomError::NoCandidatesResolved);
        }
        Ok((genres, resolved))
    }

    /// Tops the queue up with more similar-genre songs if continuous mode
    /// is on and it's currently running low — shared by the passive
    /// watcher (fires on every queue state change) and the "!playrandom
    /// on" command itself (fires once immediately, since toggling the
    /// flag alone doesn't produce a queue state change to react to — an
    /// already-empty queue that stays empty would otherwise never get
    /// topped up until *something else* changed it first, e.g. a manual
    /// !songrequest).
    async fn maybe_top_up(self: &Arc<Self>, song_requests: &Arc<SongRequestManager>) {
        let queue = song_requests.snapshot().queue;
        if !self.is_enabled() || queue.len() >= CONTINUOUS_TOPUP_THRESHOLD {
            return;
        }
        // Without this, clearing the random queue for a request achieves
        // nothing visible: this watcher fires on *every* queue state
        // change, sees a queue of one, and immediately refills random
        // songs behind the request that just cleared them. Requests play
        // through first; random resumes on its own the moment the last
        // one leaves the queue, with no extra trigger needed, because
        // that departure is itself a state change.
        if requests_are_pending(&queue) {
            return;
        }
        if self.topping_up.swap(true, Ordering::SeqCst) {
            return; // already topping up from an earlier trigger
        }

        {
            let mut last = self.last_topup_attempt.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < CONTINUOUS_RETRY_COOLDOWN) {
                self.topping_up.store(false, Ordering::SeqCst);
                return;
            }
            *last = Some(Instant::now());
        }

        match self.find_similar_songs(song_requests, CONTINUOUS_TOPUP_BATCH).await {
            Ok((genres, songs)) => {
                tracing::info!("!playrandom: topped up queue with {} song(s) from genres {genres:?}.", songs.len());
                for song in songs {
                    song_requests.queue_song(song);
                }
            }
            Err(err) => {
                tracing::warn!("!playrandom: continuous top-up failed: {err}");
            }
        }
        self.topping_up.store(false, Ordering::SeqCst);
    }

    /// !playrandom on — kicks off an immediate top-up check (see
    /// maybe_top_up's doc comment for why this can't just rely on the
    /// passive watcher alone) without blocking the command reply on it.
    pub fn trigger_top_up(self: &Arc<Self>, song_requests: &Arc<SongRequestManager>) {
        let this = self.clone();
        let song_requests = song_requests.clone();
        tokio::spawn(async move { this.maybe_top_up(&song_requests).await });
    }

    /// Watches the live queue and, whenever continuous mode is on and the
    /// queue's running low, tops it up with more similar-genre songs.
    /// Spawned once from main.rs, runs for the bot's whole lifetime.
    pub fn spawn_continuous_watcher(self: Arc<Self>, song_requests: Arc<SongRequestManager>) {
        let mut rx = song_requests.subscribe();
        tokio::spawn(async move {
            while rx.recv().await.is_ok() {
                self.maybe_top_up(&song_requests).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(video_id: &str, requested_by: &str) -> Song {
        Song {
            video_id: video_id.to_string(),
            title: format!("song {video_id}"),
            duration_secs: 200,
            requested_by: requested_by.to_string(),
            thumbnail_url: String::new(),
        }
    }

    /// The other half of the owner's first rule. Clearing the random
    /// queue for a request is undone immediately unless continuous mode
    /// also stands down while that request is waiting — this watcher
    /// fires on every queue state change and would otherwise refill
    /// behind it.
    #[test]
    fn continuous_mode_stands_down_while_a_request_is_queued() {
        assert!(requests_are_pending(&[song("REQ", "viewer")]));
        assert!(
            requests_are_pending(&[song("R1", ""), song("REQ", "viewer")]),
            "a request anywhere in the queue holds random off, not just at the front"
        );
    }

    fn log_entry(video_id: &str) -> PlayLogEntry {
        PlayLogEntry {
            at: "2026-09-11T00:00:00+00:00".to_string(),
            video_id: video_id.to_string(),
            title: format!("song {video_id}"),
        }
    }

    /// The play log rides in every hourly backup snapshot, so it needs a
    /// ceiling. One past the cap must leave exactly the cap, with the
    /// newest kept and the oldest gone.
    #[test]
    fn the_play_log_is_capped_at_ten_thousand_newest_kept() {
        let mut log = Vec::new();
        for i in 0..=PLAY_LOG_MAX_ENTRIES {
            append_capped(&mut log, log_entry(&format!("v{i}")));
        }

        assert_eq!(log.len(), PLAY_LOG_MAX_ENTRIES, "10,001 writes leave exactly 10,000");
        assert_eq!(log.last().unwrap().video_id, format!("v{PLAY_LOG_MAX_ENTRIES}"), "the newest play is kept");
        assert_eq!(log.first().unwrap().video_id, "v1", "and exactly one entry - the oldest - was dropped");
        assert!(!log.iter().any(|e| e.video_id == "v0"), "v0 is gone");
    }

    /// A log already over the ceiling - an older file, or a lowered
    /// constant - comes back into range on the next write, rather than
    /// taking one play per excess entry to get there.
    #[test]
    fn an_oversized_log_is_brought_back_in_one_write() {
        let mut log: Vec<PlayLogEntry> = (0..PLAY_LOG_MAX_ENTRIES + 500).map(|i| log_entry(&format!("old{i}"))).collect();

        append_capped(&mut log, log_entry("newest"));

        assert_eq!(log.len(), PLAY_LOG_MAX_ENTRIES);
        assert_eq!(log.last().unwrap().video_id, "newest");
    }

    /// Below the ceiling nothing is dropped - the common case.
    #[test]
    fn a_short_log_keeps_everything() {
        let mut log = Vec::new();
        for i in 0..50 {
            append_capped(&mut log, log_entry(&format!("v{i}")));
        }
        assert_eq!(log.len(), 50);
        assert_eq!(log.first().unwrap().video_id, "v0", "the first play ever is still there");
    }

    /// The no-repeat window keeps the last NO_REPEAT_WINDOW ids and no
    /// more, and "or all of them if fewer" falls out of that for free -
    /// a small pool still plays instead of starving.
    #[test]
    fn the_no_repeat_window_holds_the_last_hundred_and_drops_the_oldest() {
        let mut history = PersistedHistory::default();
        for i in 0..NO_REPEAT_WINDOW {
            history.played.push_back(format!("v{i}"));
        }
        assert_eq!(history.played.len(), NO_REPEAT_WINDOW);
        assert!(history.played.contains(&"v0".to_string()), "still inside the window");

        // One more play pushes the oldest out, and only the oldest.
        history.played.push_back("v100".to_string());
        while history.played.len() > NO_REPEAT_WINDOW {
            history.played.pop_front();
        }
        assert_eq!(history.played.len(), NO_REPEAT_WINDOW);
        assert!(!history.played.contains(&"v0".to_string()), "the oldest play has left the window");
        assert!(history.played.contains(&"v1".to_string()), "everything else stays");
        assert!(history.played.contains(&"v100".to_string()));
    }

    /// ...and resumes on its own once the request queue empties. No
    /// extra trigger is needed: the song leaving the queue is itself the
    /// state change the watcher reacts to.
    #[test]
    fn continuous_mode_resumes_when_the_request_queue_empties() {
        assert!(!requests_are_pending(&[]), "an empty queue is random's cue to resume");
        assert!(!requests_are_pending(&[song("R1", ""), song("R2", "")]), "random songs never hold random off");
    }
}
