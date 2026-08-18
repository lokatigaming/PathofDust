// Built-in song requests — replaces StreamElements' song request feature.
// Resolves a YouTube URL or search query via the YouTube Data API v3, queues
// it, and pushes the current now-playing/queue state to any connected OBS
// browser source (public/song-overlay.html) over a WebSocket. The overlay
// page itself decides when a song has finished (YouTube IFrame Player API's
// ENDED event) and asks the server to advance — the server is otherwise the
// single source of truth for queue order.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub duration_secs: u64,
    pub requested_by: String,
    pub thumbnail_url: String,
}

/// What actually gets saved to disk (song-queue.json) so the playlist
/// survives a bot restart — deliberately just the songs themselves, not
/// player_state/muted/show_on_stream, which describe the *browser's*
/// player and get freshly re-reported by the overlay once it reconnects
/// anyway. If the restored `now_playing` matches what the overlay's own
/// YouTube player already has loaded (the normal case — OBS's browser
/// source doesn't reload just because the bot restarted), playback isn't
/// interrupted at all; see overlay.html's applyState, which skips
/// reloading when the video id is unchanged.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedQueue {
    now_playing: Option<Song>,
    queue: Vec<Song>,
}

/// Persisted so a repeat request (someone re-requesting a song already
/// looked up earlier — very common in chat) resolves for free instead of
/// spending more of the scarce YouTube search quota (100/day on the free
/// tier). Two layers: a text query only ever needs to hit search.list
/// once per distinct query, and a video id only ever needs videos.list
/// once regardless of whether it arrived via a cached search or a direct
/// link/ID paste.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct SearchCache {
    /// Normalized (trimmed, lowercased) query -> resolved video id.
    queries: HashMap<String, String>,
    /// Video id -> (title, duration_secs).
    videos: HashMap<String, (String, u64)>,
}

/// Actual playback state lives in the browser (only the overlay page holds
/// the real YouTube player), so this is just the *last known* state as
/// reported back by the overlay over its WebSocket — the dock reads this to
/// decide what its play/pause and mute icons should show, but it's not the
/// source of truth for whether the video visually is or isn't playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerState {
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlAction {
    Play,
    Pause,
    Restart,
    Mute,
    Unmute,
    /// 0-100 — the overlay clamps again on its end for safety, but the
    /// only current caller (`!votevolume`) already clamps to 50-75 before
    /// this is ever sent.
    SetVolume(u8),
    /// !songinsert/!si — tells the overlay to remember exactly where it is
    /// in the current video (locally, in its own JS state — the server
    /// never learns the position), swap to this video immediately, then
    /// automatically resume the original one at that position once this
    /// one ends. The server-side queue/now_playing bookkeeping is never
    /// touched by this at all, which is what makes "just continue the
    /// playlist afterward" free — there's nothing to restore server-side.
    InsertSong(String),
    /// Fired the instant an alert's sound starts playing — tells the
    /// overlay to duck the music volume down to 20%. Stays ducked until
    /// `UnduckVolume` arrives (or a safety-net timeout, in case that
    /// never comes), rather than a fixed duration, so it naturally
    /// covers alert sounds of any length instead of guessing.
    DuckVolume,
    /// Fired the instant that same alert sound actually finishes
    /// playing (its real `ended`/error event, not an estimate) —
    /// restores the music to whatever volume it was at before ducking.
    UnduckVolume,
    /// Follow alerts specifically pause the music outright instead of
    /// ducking it — fired the instant follow.mp3 starts. The overlay
    /// remembers whether it was actually playing before pausing (so this
    /// is a no-op-on-resume if the song was already paused for an
    /// unrelated reason, e.g. !modpause) and never touches the
    /// server-side vote-pause bookkeeping (!votepause/!votestart's
    /// cooldown etc.) at all — this is a fully separate mechanism.
    PauseForAlert,
    /// Fired the instant that follow sound actually finishes playing.
    ResumeForAlert,
    /// !modskip while an insert (!songinsert/!si or an entrance theme) is
    /// actively playing — tells the overlay to cut it off immediately and
    /// resume the interrupted song right where it left off, exactly like
    /// the insert reaching its own natural end (`insertEnded`) would, just
    /// triggered manually instead of by the video actually finishing.
    SkipInsert,
}

/// Chat-facing volume vote is deliberately restricted to a "reasonable"
/// band so a vote can't blast viewers' ears or mute the stream's music
/// entirely.
pub const MIN_VOTE_VOLUME: u8 = 50;
pub const MAX_VOTE_VOLUME: u8 = 75;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    pub now_playing: Option<Song>,
    pub queue: Vec<Song>,
    pub player_state: PlayerState,
    pub muted: bool,
    /// Whether the video should be visible on stream right now (the dock's
    /// "Show on Stream" toggle) — false hides the video element on the
    /// overlay page but leaves audio playing, it doesn't pause anything.
    pub show_on_stream: bool,
    /// Set by !songinsert/!si while an inserted song is actively playing
    /// — `now_playing` above deliberately keeps showing the *interrupted*
    /// song throughout (queue bookkeeping never changes for an insert),
    /// so this is what actually reflects what's on stream right now.
    pub active_insert: Option<Song>,
}

pub enum RequestOutcome {
    NowPlaying(Song),
    Queued { song: Song, position: usize },
}

pub enum VoteSkipOutcome {
    NothingPlaying,
    AlreadyVoted,
    Locked,
    /// Shared with the "Interrupt the Music" channel points redemption —
    /// see `SKIP_ACTION_COOLDOWN`.
    OnCooldown { remaining_secs: u64 },
    Recorded { count: u32, threshold: u32 },
    Skipped { new_now_playing: Option<Song> },
    /// The voter requested the song currently playing themselves — no
    /// group vote needed, skipped immediately.
    SelfSkipped { new_now_playing: Option<Song> },
}

pub enum VotePauseOutcome {
    NothingPlaying,
    AlreadyPaused,
    AlreadyVoted,
    Recorded { count: u32, threshold: u32 },
    Paused,
}

pub enum VoteResumeOutcome {
    NotPaused,
    AlreadyVoted,
    Recorded { count: u32, threshold: u32 },
    Resumed,
    /// Threshold reached, but !votepause's post-pause cooldown hasn't
    /// elapsed yet — the resume is scheduled to fire automatically once it
    /// does, rather than happening right now.
    Scheduled { remaining_secs: u64 },
    /// Threshold was already reached by an earlier voter and a resume is
    /// already scheduled — this voter doesn't trigger a second one.
    AlreadyScheduled { remaining_secs: u64 },
}

pub enum VoteVolumeOutcome {
    AlreadyVoted,
    Recorded { count: u32, threshold: u32, percent: u8 },
    Applied { percent: u8 },
}

pub enum SongInsertOutcome {
    /// Only one insert can be active at a time — a second !songinsert
    /// while one's already playing is rejected rather than queued or
    /// stacked, to keep the resume logic (both here and in the overlay)
    /// simple: exactly one "song to return to" at a time.
    AlreadyInserting,
    Inserted { song: Song },
}

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("Couldn't find a video for \"{0}\".")]
    NotFound(String),
    #[error("That video is {duration_secs}s long, which is over the {max_secs}s limit.")]
    TooLong { duration_secs: u64, max_secs: u64 },
    #[error("YouTube lookup failed: {0}")]
    Api(#[from] anyhow::Error),
}

struct Inner {
    now_playing: Option<Song>,
    queue: VecDeque<Song>,
    player_state: PlayerState,
    muted: bool,
    show_on_stream: bool,
    /// Lowercased usernames who've voted to skip the *current* now-playing
    /// song — cleared every time the song changes (see `advance`).
    votes: HashSet<String>,
    /// Set by !forceplay — blocks !voteskip entirely for the rest of the
    /// current song (not just a one-time vote reset, since chat could just
    /// immediately re-vote past the threshold again). Cleared on `advance`.
    voteskip_locked: bool,
    /// Lowercased usernames who've voted to pause/resume — separate sets
    /// since they're independent votes, both cleared whenever a pause or
    /// resume actually happens (or the song changes, see `advance`).
    pause_votes: HashSet<String>,
    resume_votes: HashSet<String>,
    /// Set the moment !votepause's threshold is reached — starts the
    /// post-pause cooldown that !votestart can't resume through early.
    paused_at: Option<Instant>,
    /// True once a resume has been scheduled to fire automatically after
    /// the cooldown ends, so a second voter reaching threshold again
    /// doesn't queue up a duplicate resume.
    resume_scheduled: bool,
    /// Lowercased usernames who've voted for each proposed volume level —
    /// keyed by the (already-clamped) target percent, since multiple
    /// different levels can have votes in flight at once. Cleared whenever
    /// a level's votes hit threshold and get applied, or the song changes.
    volume_votes: HashMap<u8, HashSet<String>>,
    /// Lowercased username -> when they last used !voteskip or redeemed
    /// "Interrupt the Music" — shared between the two since both are ways
    /// a single viewer can force/push a skip through, so they share one
    /// cooldown bucket rather than each getting their own 10 minutes.
    /// Deliberately NOT cleared by `advance()` (unlike `votes` etc.) — it
    /// tracks the user, not the song, so it has to survive song changes to
    /// mean anything.
    skip_action_cooldowns: HashMap<String, Instant>,
    /// Set by !songinsert/!si, cleared once the overlay reports the
    /// inserted song ended and the interrupted one resumed. Purely a
    /// display concern — see `QueueState::active_insert`.
    active_insert: Option<Song>,
    /// The last `RECENT_HISTORY_LIMIT` songs that actually played through
    /// the regular queue (not inserts/themes — see `advance`) — used by
    /// !playrandom to figure out "similar genre to what's been playing".
    /// In-memory only, not persisted; losing it on a restart just means a
    /// few songs' worth of genre context, not anything worth a file for.
    history: VecDeque<Song>,
}

/// How many recently-played songs !playrandom keeps around to derive "the
/// last 5 distinct genres played" from — larger than 5 on purpose, since
/// consecutive songs are often the same genre and it needs enough raw
/// history to actually find 5 *different* ones, not just the last 5 songs.
const RECENT_HISTORY_LIMIT: usize = 30;

/// Per-user cooldown shared by !voteskip and "Interrupt the Music" — see
/// `Inner::skip_action_cooldowns`.
pub(crate) const SKIP_ACTION_COOLDOWN: Duration = Duration::from_secs(600);

/// Emitted when the overlay reports the YouTube IFrame player failed to
/// play a video (removed, region-locked, embedding disabled, etc.) — so a
/// bad link doesn't just silently stall the stream. main.rs subscribes to
/// this and announces the skip + reason in chat.
#[derive(Debug, Clone)]
pub struct PlaybackErrorEvent {
    pub title: String,
    pub reason: String,
}

pub struct SongRequestManager {
    /// Tried in rotation on a 429 (quota exceeded) — a single free-tier
    /// key is capped at 100 search.list calls/day, so more than one key
    /// multiplies the effective daily budget. `current_key_index` is where
    /// the *next* request starts trying from, updated whenever a key gets
    /// rotated past so later requests don't re-hit an already-exhausted
    /// one first every time.
    youtube_api_keys: Vec<String>,
    current_key_index: AtomicUsize,
    http: reqwest::Client,
    max_duration_secs: u64,
    voteskip_threshold: u32,
    votepause_threshold: u32,
    voteresume_threshold: u32,
    votevolume_threshold: u32,
    /// How long after a successful !votepause that !votestart is blocked
    /// from actually resuming playback, even if its vote already passed.
    resume_cooldown: Duration,
    queue_path: PathBuf,
    cache_path: PathBuf,
    cache: Mutex<SearchCache>,
    state: Mutex<Inner>,
    tx: broadcast::Sender<QueueState>,
    command_tx: broadcast::Sender<ControlAction>,
    playback_error_tx: broadcast::Sender<PlaybackErrorEvent>,
}

impl SongRequestManager {
    pub fn new(
        youtube_api_keys: Vec<String>,
        max_duration_secs: u64,
        voteskip_threshold: u32,
        votepause_threshold: u32,
        voteresume_threshold: u32,
        resume_cooldown_secs: u64,
        votevolume_threshold: u32,
        queue_path: PathBuf,
        cache_path: PathBuf,
    ) -> Arc<Self> {
        assert!(!youtube_api_keys.is_empty(), "SongRequestManager::new requires at least one YouTube API key");
        let (tx, _rx) = broadcast::channel(16);
        let (command_tx, _rx) = broadcast::channel(16);
        let (playback_error_tx, _rx) = broadcast::channel(16);
        let persisted: PersistedQueue = crate::state::load_json(&queue_path).unwrap_or_default();
        let cache: SearchCache = crate::state::load_json(&cache_path).unwrap_or_default();
        Arc::new(Self {
            youtube_api_keys,
            current_key_index: AtomicUsize::new(0),
            http: reqwest::Client::new(),
            voteskip_threshold,
            votepause_threshold,
            voteresume_threshold,
            votevolume_threshold,
            resume_cooldown: Duration::from_secs(resume_cooldown_secs),
            max_duration_secs,
            queue_path,
            cache_path,
            cache: Mutex::new(cache),
            state: Mutex::new(Inner {
                now_playing: persisted.now_playing,
                queue: persisted.queue.into(),
                player_state: PlayerState::Paused,
                muted: false,
                show_on_stream: true,
                votes: HashSet::new(),
                voteskip_locked: false,
                pause_votes: HashSet::new(),
                resume_votes: HashSet::new(),
                paused_at: None,
                resume_scheduled: false,
                volume_votes: HashMap::new(),
                skip_action_cooldowns: HashMap::new(),
                active_insert: None,
                history: VecDeque::new(),
            }),
            tx,
            command_tx,
            playback_error_tx,
        })
    }

    fn persist_queue(&self, state: &Inner) {
        let persisted = PersistedQueue { now_playing: state.now_playing.clone(), queue: state.queue.iter().cloned().collect() };
        if let Err(err) = crate::state::save_json(&self.queue_path, &persisted) {
            tracing::error!("Failed to persist song-queue.json: {err}");
        }
    }

    fn persist_cache(&self, cache: &SearchCache) {
        if let Err(err) = crate::state::save_json(&self.cache_path, cache) {
            tracing::error!("Failed to persist search-cache.json: {err}");
        }
    }

    /// Issues a YouTube Data API GET, trying configured keys in rotation
    /// on a 429 (quota exceeded) — up to once per key per call, so a
    /// single exhausted key doesn't take song requests down as long as
    /// another configured key still has quota left today.
    async fn youtube_get(&self, url: &str, params: &[(&str, &str)]) -> Result<serde_json::Value, RequestError> {
        let num_keys = self.youtube_api_keys.len();
        let start_index = self.current_key_index.load(Ordering::Relaxed) % num_keys;

        for attempt in 0..num_keys {
            let index = (start_index + attempt) % num_keys;
            let key = self.youtube_api_keys[index].as_str();

            let mut query: Vec<(&str, &str)> = params.to_vec();
            query.push(("key", key));

            let resp = self.http.get(url).query(&query).send().await.map_err(anyhow::Error::from)?;

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                self.current_key_index.store((index + 1) % num_keys, Ordering::Relaxed);
                if attempt + 1 < num_keys {
                    tracing::warn!("YouTube API key #{index} is over its daily quota, rotating to the next key.");
                    continue;
                }
                tracing::error!("All {num_keys} configured YouTube API key(s) are over their daily quota.");
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::error!("YouTube API request failed ({status}): {body}");
                return Err(RequestError::Api(anyhow::anyhow!(
                    "YouTube API returned {status} — check the bot's log for details."
                )));
            }

            self.current_key_index.store(index, Ordering::Relaxed);
            return Ok(resp.json().await.map_err(anyhow::Error::from)?);
        }

        unreachable!("loop always returns on the last attempt (success or error)")
    }

    pub fn subscribe(&self) -> broadcast::Receiver<QueueState> {
        self.tx.subscribe()
    }

    pub fn subscribe_commands(&self) -> broadcast::Receiver<ControlAction> {
        self.command_tx.subscribe()
    }

    pub fn subscribe_playback_errors(&self) -> broadcast::Receiver<PlaybackErrorEvent> {
        self.playback_error_tx.subscribe()
    }

    pub fn snapshot(&self) -> QueueState {
        let state = self.state.lock().unwrap();
        QueueState {
            now_playing: state.now_playing.clone(),
            queue: state.queue.iter().cloned().collect(),
            player_state: state.player_state,
            muted: state.muted,
            show_on_stream: state.show_on_stream,
            active_insert: state.active_insert.clone(),
        }
    }

    /// The last `n` songs that actually finished playing through the
    /// regular queue (same history !playrandom already keeps for genre
    /// matching — see `Inner::history`), most recently played first. Not
    /// its own command — this is what !queue's own reply feeds into its
    /// "Last played: ..." line (see commands.rs). Inserts/entrance themes
    /// never end up in here, same as !playrandom's genre derivation — see
    /// `advance`'s doc comment.
    pub fn recent_songs(&self, n: usize) -> Vec<Song> {
        let state = self.state.lock().unwrap();
        state.history.iter().rev().take(n).cloned().collect()
    }

    fn broadcast_state(&self) {
        // No receivers connected (OBS not open yet) — fine, same best-effort
        // behavior as the alert server's SSE broadcast.
        let _ = self.tx.send(self.snapshot());
    }

    /// Relays a transport control (play/pause/restart/mute/unmute) to the
    /// overlay page, which is the only thing that actually holds the
    /// YouTube player — this doesn't change any state itself, the overlay
    /// reports back via `report_player_state`/`report_muted` once it's
    /// actually carried the command out.
    pub fn send_command(&self, action: ControlAction) {
        let _ = self.command_tx.send(action);
    }

    pub fn report_player_state(&self, playing: bool) {
        {
            let mut state = self.state.lock().unwrap();
            state.player_state = if playing { PlayerState::Playing } else { PlayerState::Paused };
        }
        self.broadcast_state();
    }

    pub fn report_muted(&self, muted: bool) {
        {
            self.state.lock().unwrap().muted = muted;
        }
        self.broadcast_state();
    }

    pub fn toggle_show_on_stream(&self) -> bool {
        let new_val = {
            let mut state = self.state.lock().unwrap();
            state.show_on_stream = !state.show_on_stream;
            state.show_on_stream
        };
        self.broadcast_state();
        new_val
    }

    /// Removes a specific pending song (dock's per-item remove button) —
    /// does nothing to now-playing, same scope restriction as `clear_queue`.
    pub fn remove_from_queue(&self, video_id: &str) -> bool {
        let removed = {
            let mut state = self.state.lock().unwrap();
            let before = state.queue.len();
            state.queue.retain(|s| s.video_id != video_id);
            let removed = before != state.queue.len();
            if removed {
                self.persist_queue(&state);
            }
            removed
        };
        if removed {
            self.broadcast_state();
        }
        removed
    }

    /// Resolves a query (direct link/ID or text search) into a full `Song`
    /// — shared by `request` (adds it to the queue) and `insert_song`
    /// (plays it immediately without touching the queue).
    async fn resolve_song(&self, query: &str, requested_by: &str) -> Result<Song, RequestError> {
        let video_id = match extract_video_id(query) {
            Some(id) => id,
            None => self.resolve_search(query).await?.ok_or_else(|| RequestError::NotFound(query.to_string()))?,
        };

        let (title, duration_secs) =
            self.resolve_video_details(&video_id).await?.ok_or_else(|| RequestError::NotFound(query.to_string()))?;

        if duration_secs > self.max_duration_secs {
            return Err(RequestError::TooLong { duration_secs, max_secs: self.max_duration_secs });
        }

        let thumbnail_url = format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg");
        Ok(Song { video_id, title, duration_secs, requested_by: requested_by.to_string(), thumbnail_url })
    }

    /// Resolves a query into full song info (title/duration/video id)
    /// without touching the queue or now_playing at all — used by
    /// !settheme to confirm what actually got matched (and, since this
    /// goes through the same cache-checked resolve_song as everything
    /// else, to warm the cache for it) without playing anything.
    pub async fn resolve_song_preview(&self, query: &str) -> Result<Song, RequestError> {
        self.resolve_song(query, "").await
    }

    pub async fn request(&self, query: &str, requested_by: &str) -> Result<RequestOutcome, RequestError> {
        let song = self.resolve_song(query, requested_by).await?;
        Ok(self.queue_song(song))
    }

    /// Directly queues an already-resolved `Song`, skipping resolve_song
    /// entirely — used by !playlist <username> to queue songs pulled
    /// from someone's saved personal playlist without re-hitting the
    /// YouTube API (or even the cache) for details that are already
    /// known.
    pub fn queue_song(&self, song: Song) -> RequestOutcome {
        let outcome = {
            let mut state = self.state.lock().unwrap();
            let outcome = if state.now_playing.is_none() {
                state.now_playing = Some(song.clone());
                RequestOutcome::NowPlaying(song)
            } else {
                state.queue.push_back(song.clone());
                RequestOutcome::Queued { song, position: state.queue.len() }
            };
            self.persist_queue(&state);
            outcome
        };

        self.broadcast_state();
        outcome
    }

    /// !songinsert/!si — plays `query` immediately, interrupting whatever
    /// is currently on stream, then automatically resumes it (from
    /// wherever it was, not from the start) once the inserted song ends.
    /// The queue and now_playing are never touched by this at all — the
    /// overlay remembers the interrupted position entirely on its own
    /// side and resumes it directly, so there's nothing here to restore
    /// afterward.
    pub async fn insert_song(&self, query: &str, requested_by: &str) -> Result<SongInsertOutcome, RequestError> {
        // Best-effort, not a hard guarantee under a true race (two mods
        // hitting !songinsert at the exact same instant) — acceptable
        // given chat commands are processed one at a time in practice.
        {
            let state = self.state.lock().unwrap();
            if state.active_insert.is_some() {
                return Ok(SongInsertOutcome::AlreadyInserting);
            }
        }

        let song = self.resolve_song(query, requested_by).await?;

        {
            let mut state = self.state.lock().unwrap();
            state.active_insert = Some(song.clone());
        }
        self.broadcast_state();
        self.send_command(ControlAction::InsertSong(song.video_id.clone()));

        Ok(SongInsertOutcome::Inserted { song })
    }

    /// Called once the overlay reports the inserted song ended and it has
    /// (locally, on its own side) resumed the interrupted one — clears
    /// the display-only marker so !nowplaying/!song and the dock go back
    /// to reflecting the real now_playing/queue.
    pub fn clear_active_insert(&self) {
        {
            self.state.lock().unwrap().active_insert = None;
        }
        self.broadcast_state();
    }

    /// !modskip — if an insert (!songinsert/!si or an entrance theme) is
    /// actively playing, cuts it off immediately instead of skipping the
    /// main queue (which is still just sitting there interrupted, not
    /// what's actually audible right now). Returns false if there's no
    /// active insert, so the caller falls through to the normal
    /// `advance()`-based skip. Doesn't clear `active_insert` itself —
    /// the overlay reports back `insertEnded` once it's actually resumed
    /// the interrupted song, same as a natural end, so there's no window
    /// where the server thinks the insert is gone before it really is.
    pub fn skip_insert(&self) -> bool {
        let has_insert = self.state.lock().unwrap().active_insert.is_some();
        if has_insert {
            self.send_command(ControlAction::SkipInsert);
        }
        has_insert
    }

    /// Same as `report_playback_error`, but for a video that failed
    /// *during* an active !songinsert/entrance-theme insert — clears the
    /// insert (same as a normal `insertEnded` report) so the interrupted
    /// playlist song resumes, rather than advancing the main queue.
    pub fn report_insert_playback_error(&self, reason: String) {
        let title = self.state.lock().unwrap().active_insert.as_ref().map(|s| s.title.clone());
        self.clear_active_insert();
        if let Some(title) = title {
            let _ = self.playback_error_tx.send(PlaybackErrorEvent { title, reason });
        }
    }

    /// Safety net for `insert_song` — if the overlay's `insertEnded`
    /// report never arrives (stale cached page that doesn't know the
    /// command, a crash, a disconnect at the wrong moment), the flag
    /// above would otherwise stay stuck set forever, permanently
    /// blocking every future !songinsert with "already playing". Called
    /// from a delayed task (see commands.rs) sized to the inserted
    /// song's own duration; only actually clears if this is still that
    /// same insert — a legitimate `insertEnded` already clearing it, or
    /// a *later* insert already running, both leave this a no-op.
    pub fn clear_active_insert_if_stuck(&self, video_id: &str) {
        let mut state = self.state.lock().unwrap();
        if state.active_insert.as_ref().is_some_and(|s| s.video_id == video_id) {
            state.active_insert = None;
            drop(state);
            self.broadcast_state();
            tracing::warn!(
                "!songinsert for video {video_id} never got an insertEnded report — force-cleared after timeout."
            );
        }
    }

    /// Pops the next song off the queue into now-playing (or clears
    /// now-playing if the queue is empty). Used by the `!skip` mod command,
    /// `!voteskip` hitting its threshold, and the overlay reporting a video
    /// ended naturally — every path that changes the current song, so vote
    /// tallies always reset with it.
    pub fn advance(&self) -> Option<Song> {
        let next = {
            let mut state = self.state.lock().unwrap();
            if let Some(finished) = state.now_playing.take() {
                state.history.push_back(finished);
                while state.history.len() > RECENT_HISTORY_LIMIT {
                    state.history.pop_front();
                }
            }
            state.now_playing = state.queue.pop_front();
            state.votes.clear();
            state.voteskip_locked = false;
            state.pause_votes.clear();
            state.resume_votes.clear();
            state.paused_at = None;
            state.resume_scheduled = false;
            state.volume_votes.clear();
            self.persist_queue(&state);
            state.now_playing.clone()
        };
        self.broadcast_state();
        next
    }

    /// The last (up to) `RECENT_HISTORY_LIMIT` songs that actually played
    /// through — used by !playrandom to derive "the genre" that's been on.
    pub fn recent_history(&self) -> Vec<Song> {
        self.state.lock().unwrap().history.iter().cloned().collect()
    }

    /// The overlay reported its YouTube player failed on the main queue's
    /// now-playing video (not an active !songinsert/entrance-theme insert)
    /// — video removed, region-locked, embedding disabled, etc. Skips it
    /// exactly like a natural `ended` would (via `advance`), but also
    /// emits a `PlaybackErrorEvent` so chat gets told *why* the song
    /// changed instead of it just silently jumping to the next one.
    pub fn report_playback_error(&self, reason: String) {
        let title = self.state.lock().unwrap().now_playing.as_ref().map(|s| s.title.clone());
        self.advance();
        if let Some(title) = title {
            let _ = self.playback_error_tx.send(PlaybackErrorEvent { title, reason });
        }
    }

    /// How much longer a (already-lowercased) user is on the shared
    /// !voteskip/"Interrupt the Music" cooldown, or `None` if they're
    /// clear to use either right now. Doesn't start the cooldown itself —
    /// see `start_skip_cooldown`.
    fn skip_cooldown_remaining_inner(cooldowns: &HashMap<String, Instant>, user: &str) -> Option<u64> {
        let elapsed = cooldowns.get(user)?.elapsed();
        (elapsed < SKIP_ACTION_COOLDOWN).then(|| (SKIP_ACTION_COOLDOWN - elapsed).as_secs())
    }

    /// Records `user`'s vote to skip the current song (case-insensitive,
    /// one vote per user per song). Skips immediately once the configured
    /// threshold is reached, unless !forceplay has locked voting for this
    /// song — or immediately regardless of threshold if `user` requested
    /// the song currently playing themselves (no group permission needed
    /// to change your own mind about your own request). Shares its
    /// 10-minute per-user cooldown with "Interrupt the Music" — a vote or
    /// self-skip cast here starts it, same as a redemption does.
    pub fn vote_skip(&self, user: &str) -> VoteSkipOutcome {
        let lower = user.to_lowercase();
        // `None` here means "self-skip, go straight to advance()" —
        // distinct from `Some(count)` below `voteskip_threshold`.
        let count = {
            let mut state = self.state.lock().unwrap();
            if state.now_playing.is_none() {
                return VoteSkipOutcome::NothingPlaying;
            }
            if state.voteskip_locked {
                return VoteSkipOutcome::Locked;
            }
            if let Some(remaining_secs) = Self::skip_cooldown_remaining_inner(&state.skip_action_cooldowns, &lower) {
                return VoteSkipOutcome::OnCooldown { remaining_secs };
            }

            let is_own_song = state.now_playing.as_ref().is_some_and(|s| s.requested_by.to_lowercase() == lower);
            if is_own_song {
                state.skip_action_cooldowns.insert(lower, Instant::now());
                None
            } else {
                if !state.votes.insert(lower.clone()) {
                    return VoteSkipOutcome::AlreadyVoted;
                }
                state.skip_action_cooldowns.insert(lower, Instant::now());
                Some(state.votes.len() as u32)
            }
        };

        match count {
            None => VoteSkipOutcome::SelfSkipped { new_now_playing: self.advance() },
            Some(count) if count >= self.voteskip_threshold => VoteSkipOutcome::Skipped { new_now_playing: self.advance() },
            Some(count) => VoteSkipOutcome::Recorded { count, threshold: self.voteskip_threshold },
        }
    }

    /// "Interrupt the Music"'s side of the shared !voteskip cooldown — the
    /// redemption forces its skip through directly (see main.rs) rather
    /// than going through `vote_skip`, so it checks/starts the same
    /// cooldown bucket here instead. Read-only — pairs with
    /// `start_skip_cooldown`, called separately once the redemption is
    /// confirmed to actually go through, so a refunded attempt (bad
    /// link/search) never costs the viewer any cooldown.
    pub fn skip_cooldown_remaining(&self, user: &str) -> Option<u64> {
        let state = self.state.lock().unwrap();
        Self::skip_cooldown_remaining_inner(&state.skip_action_cooldowns, &user.to_lowercase())
    }

    /// Starts (or restarts) `user`'s shared !voteskip/"Interrupt the
    /// Music" cooldown — call only once the action it's gating has
    /// actually happened.
    pub fn start_skip_cooldown(&self, user: &str) {
        self.state.lock().unwrap().skip_action_cooldowns.insert(user.to_lowercase(), Instant::now());
    }

    /// !forceplay — clears any pending votes and blocks !voteskip for the
    /// rest of the current song. Returns false if nothing is playing (no
    /// song to lock).
    pub fn lock_voteskip(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.now_playing.is_none() {
            return false;
        }
        state.votes.clear();
        state.voteskip_locked = true;
        true
    }

    /// Records `user`'s vote to pause the current song. Pauses immediately
    /// once the threshold is reached, starting the post-pause cooldown that
    /// `vote_resume` has to respect.
    pub fn vote_pause(&self, user: &str) -> VotePauseOutcome {
        let mut state = self.state.lock().unwrap();
        if state.now_playing.is_none() {
            return VotePauseOutcome::NothingPlaying;
        }
        if state.player_state == PlayerState::Paused {
            return VotePauseOutcome::AlreadyPaused;
        }
        if !state.pause_votes.insert(user.to_lowercase()) {
            return VotePauseOutcome::AlreadyVoted;
        }
        let count = state.pause_votes.len() as u32;
        if count < self.votepause_threshold {
            return VotePauseOutcome::Recorded { count, threshold: self.votepause_threshold };
        }

        state.pause_votes.clear();
        state.resume_votes.clear();
        state.paused_at = Some(Instant::now());
        state.resume_scheduled = false;
        drop(state);
        self.send_command(ControlAction::Pause);
        VotePauseOutcome::Paused
    }

    /// Records `user`'s vote to resume the current song. Once the threshold
    /// is reached, resumes right away unless still inside the cooldown
    /// started by `vote_pause` — in that case the resume is scheduled to
    /// fire automatically (see `resume_now`) once the cooldown ends.
    pub fn vote_resume(&self, user: &str) -> VoteResumeOutcome {
        let mut state = self.state.lock().unwrap();
        if state.player_state != PlayerState::Paused {
            return VoteResumeOutcome::NotPaused;
        }
        if !state.resume_votes.insert(user.to_lowercase()) {
            return VoteResumeOutcome::AlreadyVoted;
        }
        let count = state.resume_votes.len() as u32;
        if count < self.voteresume_threshold {
            return VoteResumeOutcome::Recorded { count, threshold: self.voteresume_threshold };
        }

        let remaining =
            state.paused_at.map(|at| self.resume_cooldown.saturating_sub(at.elapsed())).filter(|d| !d.is_zero());

        match remaining {
            None => {
                state.resume_votes.clear();
                state.paused_at = None;
                state.resume_scheduled = false;
                drop(state);
                self.send_command(ControlAction::Play);
                VoteResumeOutcome::Resumed
            }
            Some(remaining) => {
                let remaining_secs = remaining.as_secs().max(1);
                if state.resume_scheduled {
                    return VoteResumeOutcome::AlreadyScheduled { remaining_secs };
                }
                state.resume_scheduled = true;
                VoteResumeOutcome::Scheduled { remaining_secs }
            }
        }
    }

    /// Actually resumes playback — called directly once the cooldown has
    /// already elapsed by the time a resume vote passes, or from a delayed
    /// task scheduled by `vote_resume` when it hadn't.
    pub fn resume_now(&self) {
        let mut state = self.state.lock().unwrap();
        state.resume_votes.clear();
        state.paused_at = None;
        state.resume_scheduled = false;
        drop(state);
        self.send_command(ControlAction::Play);
    }

    /// Records `user`'s vote for `percent` (already clamped to
    /// `MIN_VOTE_VOLUME..=MAX_VOTE_VOLUME` by the caller) as the volume.
    /// Different chatters can have votes in flight for different levels at
    /// once — whichever level first collects enough unique voters wins and
    /// clears every pending vote, not just its own.
    /// Just the vote tallying — actually applying the winning volume
    /// (via OBS's own source fader, so it survives any Compressor/
    /// Limiter on that source — see obs_websocket.rs) is the caller's
    /// job in commands.rs once it sees `Applied`, not this method's.
    pub fn vote_volume(&self, user: &str, percent: u8) -> VoteVolumeOutcome {
        let mut state = self.state.lock().unwrap();
        let voters = state.volume_votes.entry(percent).or_default();
        if !voters.insert(user.to_lowercase()) {
            return VoteVolumeOutcome::AlreadyVoted;
        }
        let count = voters.len() as u32;
        if count < self.votevolume_threshold {
            return VoteVolumeOutcome::Recorded { count, threshold: self.votevolume_threshold, percent };
        }

        state.volume_votes.clear();
        VoteVolumeOutcome::Applied { percent }
    }

    /// !modpause — pauses immediately, no vote needed. Still starts the
    /// same post-pause cooldown as `vote_pause` so chat can't instantly
    /// vote-resume what a mod just paused. Returns false if nothing is
    /// playing (nothing to pause).
    pub fn mod_pause(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.now_playing.is_none() {
            return false;
        }
        state.pause_votes.clear();
        state.resume_votes.clear();
        state.paused_at = Some(Instant::now());
        state.resume_scheduled = false;
        drop(state);
        self.send_command(ControlAction::Pause);
        true
    }

    /// !modvolume/!modvv — sets the volume immediately, no vote needed, and
    /// unlike `vote_volume` isn't restricted to the 50-75 safety band (the
    /// caller still clamps to a sane 0-100 to guard against garbage input).
    /// Clears any pending !votevolume votes so an old vote can't
    /// confusingly apply after a mod's already overridden it — actually
    /// applying `percent` (via OBS's fader, see `vote_volume`'s doc
    /// comment) is the caller's job in commands.rs.
    pub fn mod_clear_volume_votes(&self) {
        self.state.lock().unwrap().volume_votes.clear();
    }

    /// Clears pending queue only — does not stop whatever's currently
    /// playing (use `advance` for that, via `!skip`).
    pub fn clear_queue(&self) -> usize {
        let count = {
            let mut state = self.state.lock().unwrap();
            let count = state.queue.len();
            state.queue.clear();
            self.persist_queue(&state);
            count
        };
        self.broadcast_state();
        count
    }

    /// Cache-checked wrapper around `search` — a repeat text query (very
    /// common in chat, e.g. a popular request coming up again) resolves
    /// for free instead of spending another search.list call.
    async fn resolve_search(&self, query: &str) -> Result<Option<String>, RequestError> {
        let normalized = query.trim().to_lowercase();

        if let Some(cached) = self.cache.lock().unwrap().queries.get(&normalized).cloned() {
            return Ok(Some(cached));
        }

        let Some(video_id) = self.search(query).await? else { return Ok(None) };

        let mut cache = self.cache.lock().unwrap();
        cache.queries.insert(normalized, video_id.clone());
        self.persist_cache(&cache);
        drop(cache);

        Ok(Some(video_id))
    }

    /// Cache-checked wrapper around `fetch_video_details` — applies
    /// whether the video id came from a cached search or a direct
    /// link/ID paste, so a repeat direct link doesn't re-spend quota
    /// either.
    async fn resolve_video_details(&self, video_id: &str) -> Result<Option<(String, u64)>, RequestError> {
        if let Some(cached) = self.cache.lock().unwrap().videos.get(video_id).cloned() {
            return Ok(Some(cached));
        }

        let Some(details) = self.fetch_video_details(video_id).await? else { return Ok(None) };

        let mut cache = self.cache.lock().unwrap();
        cache.videos.insert(video_id.to_string(), details.clone());
        self.persist_cache(&cache);
        drop(cache);

        Ok(Some(details))
    }

    async fn search(&self, query: &str) -> Result<Option<String>, RequestError> {
        let data = self
            .youtube_get(
                "https://www.googleapis.com/youtube/v3/search",
                &[("part", "snippet"), ("type", "video"), ("maxResults", "1"), ("q", query)],
            )
            .await?;

        let id = data
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|item| item.get("id"))
            .and_then(|id| id.get("videoId"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(id)
    }

    async fn fetch_video_details(&self, video_id: &str) -> Result<Option<(String, u64)>, RequestError> {
        let data = self
            .youtube_get("https://www.googleapis.com/youtube/v3/videos", &[("part", "snippet,contentDetails"), ("id", video_id)])
            .await?;

        let Some(item) = data.get("items").and_then(|v| v.as_array()).and_then(|a| a.first()) else {
            return Ok(None);
        };

        let title = item
            .get("snippet")
            .and_then(|s| s.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown title")
            .to_string();

        let duration_secs = item
            .get("contentDetails")
            .and_then(|c| c.get("duration"))
            .and_then(|v| v.as_str())
            .map(parse_iso8601_duration)
            .unwrap_or(0);

        Ok(Some((title, duration_secs)))
    }
}

static VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtube\.com/(?:watch\?v=|embed/|shorts/)|youtu\.be/)([A-Za-z0-9_-]{11})").unwrap()
});

fn extract_video_id(input: &str) -> Option<String> {
    VIDEO_ID_RE.captures(input).map(|caps| caps[1].to_string())
}

static DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?$").unwrap());

/// Parses YouTube's ISO 8601 video duration format (e.g. "PT4M13S", "PT1H2M3S").
fn parse_iso8601_duration(input: &str) -> u64 {
    let Some(caps) = DURATION_RE.captures(input) else { return 0 };
    let hours: u64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let minutes: u64 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let seconds: u64 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    hours * 3600 + minutes * 60 + seconds
}
