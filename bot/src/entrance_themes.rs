// Per-user "entrance theme" songs — the first time someone chats each
// day, their assigned song (if any) interrupts the current playlist via
// the exact same mechanism as !songinsert (song_requests::insert_song),
// then the playlist resumes automatically right where it left off once
// the theme ends. Both the theme assignments and who's already been
// greeted today persist across restarts, so a restart mid-stream doesn't
// cause a repeat (or missed) trigger.

use crate::song_requests::{SongInsertOutcome, SongRequestManager};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Default, Serialize, Deserialize)]
struct GreetedToday {
    date: Option<NaiveDate>,
    users: HashSet<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ThemeEntry {
    /// As typed by the mod in !settheme (e.g. "HereticGamingDad") — used
    /// only for display on the public /themes.html page; matching
    /// against chat always goes through the lowercased map key instead.
    /// Empty for entries that predate this field (see Deserialize impl
    /// below) — the public page falls back to the lowercased key then.
    #[serde(default)]
    display_name: String,
    /// A youtu.be URL (not a raw video id — that wouldn't match
    /// song_requests' link-detection regex and would get treated as a
    /// text search instead).
    youtube_url: String,
    /// Resolved once at !settheme time (song_requests already resolved
    /// it to build the confirmation reply) so the public page doesn't
    /// need its own YouTube lookup. Empty for entries that predate this
    /// field — re-running !settheme backfills it.
    #[serde(default)]
    title: String,
}

/// entrance-themes.json originally stored plain `username -> youtube_url`
/// strings — this accepts either that legacy shape or the current struct,
/// so upgrading the bot doesn't silently wipe out already-configured
/// themes (deserializing straight into the new struct would just fail on
/// old files, and `load_json(..).unwrap_or_default()` would swallow that
/// into an empty map).
impl<'de> Deserialize<'de> for ThemeEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Legacy(String),
            Full {
                #[serde(default)]
                display_name: String,
                youtube_url: String,
                #[serde(default)]
                title: String,
            },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Legacy(youtube_url) => ThemeEntry { display_name: String::new(), youtube_url, title: String::new() },
            Raw::Full { display_name, youtube_url, title } => ThemeEntry { display_name, youtube_url, title },
        })
    }
}

#[derive(Serialize)]
struct PublicThemeEntry {
    username: String,
    #[serde(rename = "youtubeUrl")]
    youtube_url: String,
    title: String,
}

/// A theme waiting for whatever currently holds song_requests' single
/// "active insert" slot (another queued theme, or an unrelated
/// !songinsert) to finish, before it can start.
struct QueuedTheme {
    username: String,
    youtube_url: String,
}

/// Emitted the moment a theme *actually* starts playing — not when its
/// owner is first detected as having triggered it, since that can now be
/// well before it starts if it had to wait in `queue`. main.rs subscribes
/// to this to send the "Welcome in, X!" chat message at the right time.
#[derive(Debug, Clone)]
pub struct ThemeStartedEvent {
    pub username: String,
    pub title: String,
}

pub struct EntranceThemeManager {
    themes: Mutex<HashMap<String, ThemeEntry>>,
    themes_path: PathBuf,
    /// Where lokati.net's site folder lives, so themes-data.json can be
    /// written there for the public /themes.html page. None disables that
    /// regeneration step (still works fine locally without it) — same
    /// pattern as StaticCommands/commands-data.json.
    public_site_dir: Option<PathBuf>,
    greeted: Mutex<GreetedToday>,
    greeted_path: PathBuf,
    /// FIFO — themes announce in the order people actually showed up,
    /// not the order the currently-playing insert happens to free up.
    queue: Mutex<VecDeque<QueuedTheme>>,
    theme_started_tx: broadcast::Sender<ThemeStartedEvent>,
}

impl EntranceThemeManager {
    pub fn new(themes_path: PathBuf, greeted_path: PathBuf, public_site_dir: Option<PathBuf>) -> Arc<Self> {
        let themes: HashMap<String, ThemeEntry> = crate::state::load_json(&themes_path).unwrap_or_default();
        let greeted: GreetedToday = crate::state::load_json(&greeted_path).unwrap_or_default();
        let (theme_started_tx, _rx) = broadcast::channel(16);
        let this = Arc::new(Self {
            themes: Mutex::new(themes),
            themes_path,
            public_site_dir,
            greeted: Mutex::new(greeted),
            greeted_path,
            queue: Mutex::new(VecDeque::new()),
            theme_started_tx,
        });
        this.regenerate_public_page_sync();
        this
    }

    pub fn subscribe_theme_started(&self) -> broadcast::Receiver<ThemeStartedEvent> {
        self.theme_started_tx.subscribe()
    }

    /// Watches song_requests' queue-state broadcasts for the active-insert
    /// slot going free, and immediately starts the next queued theme (if
    /// any) when it does — called once from main.rs, kept running for the
    /// bot's whole lifetime. This is what actually plays a theme that had
    /// to wait: `maybe_play_entrance_theme` only ever queues, it never
    /// waits around itself.
    pub fn spawn_theme_queue_watcher(self: Arc<Self>, song_requests: Arc<SongRequestManager>) {
        let mut rx = song_requests.subscribe();
        tokio::spawn(async move {
            while let Ok(state) = rx.recv().await {
                if state.active_insert.is_some() {
                    continue;
                }
                self.try_advance_queue(&song_requests).await;
            }
        });
    }

    /// Starts the next queued theme, but only if the active-insert slot
    /// is actually free right now — a no-op otherwise (something else,
    /// or another theme, already has it). Safe to call speculatively
    /// from multiple places (right after queuing, and from the watcher
    /// above) since song_requests' own `insert_song` is the single point
    /// that actually enforces "only one active insert at a time".
    async fn try_advance_queue(&self, song_requests: &Arc<SongRequestManager>) {
        if song_requests.snapshot().active_insert.is_some() {
            return;
        }
        let Some(QueuedTheme { username, youtube_url }) = self.queue.lock().await.pop_front() else { return };
        self.start_theme(username, youtube_url, song_requests).await;
    }

    /// Actually starts a theme playing via song_requests' insert
    /// mechanism, and announces `ThemeStartedEvent` once it succeeds.
    async fn start_theme(&self, username: String, youtube_url: String, song_requests: &Arc<SongRequestManager>) {
        match song_requests.insert_song(&youtube_url, "Entrance Theme").await {
            Ok(SongInsertOutcome::Inserted { song }) => {
                // Same safety net !songinsert's chat command spawns — if
                // the overlay never reports the theme actually ended
                // (e.g. it wasn't fully loaded yet and dropped the
                // command, or a disconnect at the wrong moment),
                // active_insert would otherwise stay stuck forever and
                // silently block every future insert (including the
                // rest of this queue).
                let sr = song_requests.clone();
                let video_id = song.video_id.clone();
                let timeout = std::time::Duration::from_secs(song.duration_secs + 30);
                tokio::spawn(async move {
                    tokio::time::sleep(timeout).await;
                    sr.clear_active_insert_if_stuck(&video_id);
                });
                let _ = self.theme_started_tx.send(ThemeStartedEvent { username, title: song.title });
            }
            Ok(SongInsertOutcome::AlreadyInserting) => {
                // Lost a race for the active-insert slot (e.g. a mod ran
                // !songinsert in the instant between try_advance_queue
                // checking and calling insert_song) — put it back at the
                // front so the next free-slot broadcast retries it,
                // instead of losing this person's theme entirely.
                self.queue.lock().await.push_front(QueuedTheme { username, youtube_url });
            }
            Err(err) => {
                tracing::warn!("Entrance theme playback failed for {username}: {err}");
            }
        }
    }

    /// Regenerates themes-data.json from whatever's currently in `themes`
    /// — called on load (covers hand-edited entrance-themes.json) and
    /// after every !settheme. Takes a plain fn (not async) since it's
    /// only ever called from spots that already hold the lock or right
    /// after constructing the map, matching StaticCommands' pattern.
    fn regenerate_public_page(&self, themes: &HashMap<String, ThemeEntry>) {
        let Some(dir) = &self.public_site_dir else { return };

        let mut entries: Vec<PublicThemeEntry> = themes
            .iter()
            .map(|(key, entry)| PublicThemeEntry {
                username: if entry.display_name.is_empty() { key.clone() } else { entry.display_name.clone() },
                title: if entry.title.is_empty() { entry.youtube_url.clone() } else { entry.title.clone() },
                youtube_url: entry.youtube_url.clone(),
            })
            .collect();
        entries.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));

        if let Err(err) = crate::state::save_json(dir.join("themes-data.json"), &entries) {
            tracing::error!("Failed to regenerate public themes page data: {err}");
        }
    }

    fn regenerate_public_page_sync(&self) {
        // Called from `new`, before anything else can be holding the
        // lock — try_lock is just to avoid an async fn here for a
        // constructor.
        if let Ok(themes) = self.themes.try_lock() {
            self.regenerate_public_page(&themes);
        }
    }

    /// !resetgreeted mod command — clears "greeted today" for one user
    /// (or everyone, if `username` is None), e.g. for testing a theme
    /// that was assigned *after* that user already sent their first
    /// message of the day (so it wouldn't otherwise retrigger until the
    /// next calendar day).
    pub async fn reset_greeted(&self, username: Option<&str>) {
        let mut greeted = self.greeted.lock().await;
        match username {
            Some(name) => {
                greeted.users.remove(&name.to_lowercase());
            }
            None => {
                greeted.users.clear();
            }
        }
        if let Err(err) = crate::state::save_json(&self.greeted_path, &*greeted) {
            tracing::error!("Failed to persist daily-greeted.json: {err}");
        }
    }

    /// !settheme mod command — persists immediately and regenerates the
    /// public /themes.html page's data.
    pub async fn set_theme(&self, username: &str, youtube_url: String, title: String) {
        let key = username.to_lowercase();
        let mut themes = self.themes.lock().await;
        themes.insert(key, ThemeEntry { display_name: username.to_string(), youtube_url, title });
        if let Err(err) = crate::state::save_json(&self.themes_path, &*themes) {
            tracing::error!("Failed to persist entrance-themes.json: {err}");
        }
        self.regenerate_public_page(&themes);
    }

    /// Called for every chat message. If this is `username`'s first
    /// message of the current "stream day" (Singapore time, but the day
    /// boundary is 8am rather than midnight — see `stream_day` below) and
    /// they have a theme assigned, queues it — playing it immediately if
    /// nothing else currently holds the active-insert slot, or as the
    /// very next thing (ahead of the regular playlist) once whatever
    /// does finishes. Either way, the "Welcome in, X!" chat announcement
    /// is deferred until the theme actually starts (see
    /// `subscribe_theme_started`), not fired from here. Does nothing for
    /// a first-of-day chatter with no assigned theme.
    pub async fn maybe_play_entrance_theme(&self, username: &str, song_requests: &Option<Arc<SongRequestManager>>) {
        let key = username.to_lowercase();
        let today = stream_day();

        let is_first_today = {
            let mut greeted = self.greeted.lock().await;
            if greeted.date != Some(today) {
                greeted.date = Some(today);
                greeted.users.clear();
            }
            let first = greeted.users.insert(key.clone());
            if first {
                if let Err(err) = crate::state::save_json(&self.greeted_path, &*greeted) {
                    tracing::error!("Failed to persist daily-greeted.json: {err}");
                }
            }
            first
        };

        if !is_first_today {
            return;
        }

        let Some(theme_url) = self.themes.lock().await.get(&key).map(|entry| entry.youtube_url.clone()) else {
            return;
        };
        let Some(song_requests) = song_requests.as_ref() else { return };

        self.queue.lock().await.push_back(QueuedTheme { username: username.to_string(), youtube_url: theme_url });
        self.try_advance_queue(song_requests).await;
    }
}

/// The "stream day" used for entrance-theme greeting resets: Singapore
/// time, but rolling over at 8am rather than midnight, so a stream
/// running past midnight doesn't re-greet everyone partway through — the
/// day only actually turns over well after a typical stream has ended.
/// Subtracting 8 hours before taking the calendar date does this: at
/// 7:59am SGT that lands in the previous day (23:59), and at 8:00am SGT
/// it lands right at the start of today (00:00).
fn stream_day() -> NaiveDate {
    (chrono::Utc::now().with_timezone(&chrono_tz::Asia::Singapore) - chrono::Duration::hours(8)).date_naive()
}
