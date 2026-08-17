// Per-user "personal playlists" — every successful !songrequest/!sr
// automatically saves that song to the requester's own list, persisted
// locally across restarts. Separate from the live song queue entirely:
// adding to (or editing) a personal playlist never touches what's
// actually playing or queued on stream.
//
// The local JSON file is the bot's own fast, always-available source of
// truth (used directly by !playlist <username>'s sampling). The public
// site can't read local files on this machine, though — same problem
// commands-data.json/themes-data.json turned out to have — so instead
// every change is also pushed (best-effort, non-blocking) to the same
// Apps Script backend that already serves the build feed, into a
// "PersonalPlaylists" sheet. The site pages poll that back out via
// `?playlists=1`, same "fetch it all, filter client-side" shape as the
// build feed already uses.

use crate::song_requests::Song;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Same Apps Script Web App the build feed, Patreon login, and Valdo
/// Farmer review already run through — see Code.gs's `doPost` for the
/// `syncPersonalPlaylists` handler this posts to.
const APPS_SCRIPT_EXEC_URL: &str =
    "https://script.google.com/macros/s/AKfycbyPx7hialiC21-BbxqHGqVoPqVLczlKk4bx5hZhKZyHwnfumhtE2stPvqUoShYlOI4W/exec";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPlaylist {
    /// As typed the first time this person requested a song (or ran
    /// !playlist add) — display only; matching against chat/the URL
    /// always goes through the lowercased map key instead.
    #[serde(alias = "display_name")]
    display_name: String,
    songs: Vec<Song>,
}

pub enum RemoveOutcome {
    Removed(Song),
    /// No song matched the given position number or title text.
    NotFound,
    /// Looked like a position number, but out of range for how many
    /// songs are actually saved.
    InvalidIndex { count: usize },
    /// This user has no saved playlist at all yet.
    NoPlaylist,
}

pub enum SampleOutcome {
    /// This user has never saved anything.
    NoPlaylist,
    /// They have a playlist entry but it's empty (everything removed).
    Empty { display_name: String },
    /// They have saved songs, but every one of them is already
    /// now-playing/queued/inserted right now.
    AllAlreadyQueued { display_name: String },
    Songs { display_name: String, songs: Vec<Song> },
}

/// Playlists exist to sample from live during a stream (see `sample`) —
/// a single 20-minute epic in there would eat a huge chunk of a
/// !playlist run's airtime for one pick, so anything over this just
/// isn't saved at all. Doesn't affect !songrequest/!songinsert actually
/// playing a long song live, only whether it gets remembered afterward.
const MAX_PLAYLIST_SONG_SECS: u64 = 600;

pub enum AddOutcome {
    Added,
    /// Longer than `MAX_PLAYLIST_SONG_SECS` — not saved.
    TooLong { duration_secs: u64 },
}

pub enum PlayOutcome {
    /// This user has never saved anything.
    NoPlaylist,
    /// A position number was given, but out of range for how many songs
    /// are actually saved.
    InvalidIndex { display_name: String, count: usize },
    Song { display_name: String, song: Song },
}

pub struct PersonalPlaylistManager {
    playlists: Mutex<HashMap<String, UserPlaylist>>,
    path: PathBuf,
    /// Shared secret the Apps Script side checks before accepting a sync
    /// — None disables pushing to the sheet entirely (the local JSON
    /// file, and therefore !playlist <username>, still works fine either
    /// way; only the public site falls behind).
    sync_secret: Option<String>,
    http: reqwest::Client,
}

impl PersonalPlaylistManager {
    pub fn new(path: PathBuf, sync_secret: Option<String>) -> Arc<Self> {
        let playlists: HashMap<String, UserPlaylist> = crate::state::load_json(&path).unwrap_or_default();
        let this = Arc::new(Self { playlists: Mutex::new(playlists), path, sync_secret, http: reqwest::Client::new() });
        // Pushes local state to the sheet once at startup too, so a
        // restart (or a hand-edited local JSON file) doesn't leave the
        // public site showing stale data until the next actual change.
        if let Ok(playlists) = this.playlists.try_lock() {
            this.sync_to_sheet(&playlists);
        }
        this
    }

    /// Called for every successful !songrequest/!sr — appends to that
    /// user's own saved playlist. Duplicates are allowed on purpose
    /// (requesting a favorite song again isn't a mistake to guard
    /// against). Songs over `MAX_PLAYLIST_SONG_SECS` are silently skipped
    /// — the song still played/queued live either way, this only governs
    /// whether it's remembered in the playlist afterward.
    pub async fn add_song(&self, username: &str, song: &Song) -> AddOutcome {
        if song.duration_secs > MAX_PLAYLIST_SONG_SECS {
            return AddOutcome::TooLong { duration_secs: song.duration_secs };
        }

        let key = username.to_lowercase();
        let mut playlists = self.playlists.lock().await;
        let entry = playlists
            .entry(key)
            .or_insert_with(|| UserPlaylist { display_name: username.to_string(), songs: Vec::new() });
        entry.display_name = username.to_string();
        entry.songs.push(song.clone());
        self.persist(&playlists);
        self.sync_to_sheet(&playlists);
        AddOutcome::Added
    }

    /// One-time cleanup for songs saved before `MAX_PLAYLIST_SONG_SECS`
    /// existed — removes every already-saved song over the limit across
    /// every user's playlist. Returns how many songs were removed.
    pub async fn cull_long_songs(&self) -> usize {
        let mut playlists = self.playlists.lock().await;
        let mut removed = 0;
        for entry in playlists.values_mut() {
            let before = entry.songs.len();
            entry.songs.retain(|s| s.duration_secs <= MAX_PLAYLIST_SONG_SECS);
            removed += before - entry.songs.len();
        }
        if removed > 0 {
            self.persist(&playlists);
            self.sync_to_sheet(&playlists);
        }
        removed
    }

    /// !playlist remove <position-or-text> — a bare number is treated as
    /// a 1-based position in that user's list (matching how it's
    /// numbered on the website); anything else falls back to a
    /// case-insensitive substring match against saved titles, first
    /// match wins.
    pub async fn remove(&self, username: &str, selector: &str) -> RemoveOutcome {
        let key = username.to_lowercase();
        let mut playlists = self.playlists.lock().await;
        let Some(entry) = playlists.get_mut(&key) else { return RemoveOutcome::NoPlaylist };

        let index = match selector.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= entry.songs.len() => Some(n - 1),
            Ok(_) => return RemoveOutcome::InvalidIndex { count: entry.songs.len() },
            Err(_) => {
                let needle = selector.trim().to_lowercase();
                entry.songs.iter().position(|s| s.title.to_lowercase().contains(&needle))
            }
        };

        let Some(index) = index else { return RemoveOutcome::NotFound };
        let removed = entry.songs.remove(index);
        self.persist(&playlists);
        self.sync_to_sheet(&playlists);
        RemoveOutcome::Removed(removed)
    }

    /// !playlist clear — wipes one saved playlist entirely: your own, or
    /// (mods only, enforced by the caller in commands.rs) someone else's.
    /// Returns false if they didn't have one. Deliberately no "clear
    /// everyone" form — too easy to nuke every saved playlist by
    /// accident.
    pub async fn clear_user(&self, username: &str) -> bool {
        let key = username.to_lowercase();
        let mut playlists = self.playlists.lock().await;
        let existed = playlists.remove(&key).is_some();
        if existed {
            self.persist(&playlists);
            self.sync_to_sheet(&playlists);
        }
        existed
    }

    /// !playlist <username> — a random sample of up to `count` songs from
    /// their saved playlist, to actually queue live. Random rather than
    /// always the first N so repeat invocations surface different songs
    /// instead of the same handful every time. `already_queued` (video
    /// ids currently now-playing/queued/inserted, from the caller's
    /// song_requests snapshot) is excluded from the candidate pool
    /// *before* sampling, so a song already on stream never gets queued
    /// a second time by this — especially relevant for the random pick,
    /// which would otherwise happily re-pick something already sitting
    /// in the queue from an earlier `!playlist` run.
    pub async fn sample(&self, username: &str, count: usize, already_queued: &HashSet<String>) -> SampleOutcome {
        let key = username.to_lowercase();
        let playlists = self.playlists.lock().await;
        let Some(entry) = playlists.get(&key) else { return SampleOutcome::NoPlaylist };

        if entry.songs.is_empty() {
            return SampleOutcome::Empty { display_name: entry.display_name.clone() };
        }

        let eligible: Vec<&Song> = entry.songs.iter().filter(|s| !already_queued.contains(&s.video_id)).collect();
        if eligible.is_empty() {
            return SampleOutcome::AllAlreadyQueued { display_name: entry.display_name.clone() };
        }

        let mut rng = rand::thread_rng();
        let songs: Vec<Song> = eligible.choose_multiple(&mut rng, count).map(|&s| s.clone()).collect();
        SampleOutcome::Songs { display_name: entry.display_name.clone(), songs }
    }

    /// !playlist <username> play <position> — the one song at that
    /// 1-based position (same numbering as the website and !playlist
    /// remove), deterministic rather than a random sample. Doesn't
    /// exclude already-queued songs the way `sample` does — asking for
    /// this exact position is a deliberate pick, not a random draw where
    /// a duplicate would just be noise.
    pub async fn get_song_at(&self, username: &str, position: usize) -> PlayOutcome {
        let key = username.to_lowercase();
        let playlists = self.playlists.lock().await;
        let Some(entry) = playlists.get(&key) else { return PlayOutcome::NoPlaylist };

        if position < 1 || position > entry.songs.len() {
            return PlayOutcome::InvalidIndex { display_name: entry.display_name.clone(), count: entry.songs.len() };
        }
        PlayOutcome::Song { display_name: entry.display_name.clone(), song: entry.songs[position - 1].clone() }
    }

    fn persist(&self, playlists: &HashMap<String, UserPlaylist>) {
        if let Err(err) = crate::state::save_json(&self.path, playlists) {
            tracing::error!("Failed to persist personal-playlists.json: {err}");
        }
    }

    /// Pushes the *entire* current dataset to Apps Script — full replace
    /// rather than an incremental add/remove call, so the sheet can't
    /// drift out of sync with local state even if an earlier push failed
    /// (network hiccup, Apps Script briefly down). Fire-and-forget: runs
    /// in the background and only logs on failure, since the bot's own
    /// behavior (the local file, !playlist <username> sampling) must
    /// never block on — or break because of — this succeeding.
    fn sync_to_sheet(&self, playlists: &HashMap<String, UserPlaylist>) {
        let Some(secret) = self.sync_secret.clone() else { return };
        let payload = serde_json::json!({ "playlists": playlists });
        let http = self.http.clone();
        tokio::spawn(async move {
            let result = http
                .post(APPS_SCRIPT_EXEC_URL)
                .query(&[("action", "syncPersonalPlaylists"), ("secret", secret.as_str())])
                .json(&payload)
                .send()
                .await;
            match result {
                Ok(resp) if resp.status().is_success() => {}
                Ok(resp) => tracing::warn!("Personal playlist sync to Apps Script failed: HTTP {}", resp.status()),
                Err(err) => tracing::warn!("Personal playlist sync to Apps Script failed: {err}"),
            }
        });
    }
}
