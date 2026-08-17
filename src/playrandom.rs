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
use std::collections::HashSet;
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

#[derive(Default, Serialize, Deserialize)]
struct PersistedState {
    enabled: bool,
}

pub struct PlayRandomManager {
    http: reqwest::Client,
    lastfm_api_key: String,
    enabled: AtomicBool,
    /// Guards against overlapping continuous-mode top-up attempts, and
    /// throttles retries after a failed one — see CONTINUOUS_RETRY_COOLDOWN.
    last_topup_attempt: Mutex<Option<Instant>>,
    topping_up: AtomicBool,
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
        })
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
            "!playrandom: genres {genres:?} -> {} resolved, {already_known_skips} already-known skip(s), {resolve_failures} resolve failure(s)",
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
        if !self.is_enabled() || song_requests.snapshot().queue.len() >= CONTINUOUS_TOPUP_THRESHOLD {
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
