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

    /// The one writer for entrance-themes.json. `set_theme` and
    /// `remove_theme` both go through here rather than each saving for
    /// themselves, so the file has a single write path and the public
    /// page can never be regenerated from a map that was persisted
    /// differently.
    fn persist(&self, themes: &HashMap<String, ThemeEntry>) {
        if let Err(err) = crate::state::save_json(&self.themes_path, themes) {
            tracing::error!("Failed to persist entrance-themes.json: {err}");
        }
        self.regenerate_public_page(themes);
    }

    /// !settheme mod command — persists immediately and regenerates the
    /// public /themes.html page's data.
    pub async fn set_theme(&self, username: &str, youtube_url: String, title: String) {
        let key = username.to_lowercase();
        let mut themes = self.themes.lock().await;
        themes.insert(key, ThemeEntry { display_name: username.to_string(), youtube_url, title });
        self.persist(&themes);
    }

    /// `!theme remove <username>` mod command — drops someone's stored
    /// entrance theme. Returns false if they had none, so the caller can
    /// say so rather than claiming a removal that did not happen.
    ///
    /// Keyed by lowercased username, exactly as `set_theme` keys it, so
    /// `@Name`, `Name` and `name` all reach the same entry once the
    /// caller has stripped the `@`.
    ///
    /// Deliberately does NOT touch `daily-greeted.json` or anything about
    /// a currently-playing insert: this removes the STORED theme. If the
    /// target's theme happens to be on stream at that moment it finishes
    /// normally — the insert is already in flight and owns itself (see
    /// `start_theme`), and cutting it here would be a second, surprising
    /// effect of a command whose job is editing a file.
    pub async fn remove_theme(&self, username: &str) -> bool {
        let key = username.to_lowercase();
        let mut themes = self.themes.lock().await;
        if themes.remove(&key).is_none() {
            return false;
        }
        self.persist(&themes);
        true
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
    /// Whether this message should fire `username`'s walk-on - and, when
    /// it should, CONSUMES their daily slot in the same step.
    ///
    /// Split out of `maybe_play_entrance_theme` (2026-09-11) so the
    /// ordering is testable without a `SongRequestManager`: queueing
    /// needs one, this half is the whole decision.
    ///
    /// A COMMAND RETURNS EARLY, BEFORE THE MARKER IS TOUCHED, and that
    /// order is the entire fix. The check previously ran for every
    /// message, and the comment at the call site argued that was right -
    /// "first message of the day should count regardless of whether that
    /// first message happens to be a command". The owner has reversed it:
    /// a player whose first message of the day was `!playlist` had their
    /// walk-on consumed by it and never heard it, which reads as the
    /// theme being broken rather than spent.
    ///
    /// DEFERRED, NOT SKIPPED FOR THE DAY. Returning before the marker
    /// write is what makes that true - the slot is still unclaimed, so
    /// their first ordinary message later the same day fires it normally.
    async fn claim_walk_on(&self, username: &str, text: &str) -> bool {
        if crate::commands::is_command(text) {
            return false;
        }

        let key = username.to_lowercase();
        let today = stream_day();

        let mut greeted = self.greeted.lock().await;
        if greeted.date != Some(today) {
            greeted.date = Some(today);
            greeted.users.clear();
        }
        let first = greeted.users.insert(key);
        if first {
            if let Err(err) = crate::state::save_json(&self.greeted_path, &*greeted) {
                tracing::error!("Failed to persist daily-greeted.json: {err}");
            }
        }
        first
    }

    pub async fn maybe_play_entrance_theme(&self, username: &str, text: &str, song_requests: &Option<Arc<SongRequestManager>>) {
        if !self.claim_walk_on(username, text).await {
            return;
        }
        let key = username.to_lowercase();

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

/// A bot command must not spend a player's walk-on (owner's ruling,
/// 2026-09-11).
///
/// These drive `claim_walk_on` rather than `maybe_play_entrance_theme`
/// because the decision IS the feature: whether the daily slot gets
/// consumed, and by which message. The queueing half needs a
/// `SongRequestManager` and adds nothing to what is being asserted here.
///
/// `true` means "this message fires the walk-on, and has now spent it".
#[cfg(test)]
mod walk_on_ordering_tests {
    use super::*;

    fn manager() -> Arc<EntranceThemeManager> {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("walk_on_ordering_{}_{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        EntranceThemeManager::new(dir.join("themes.json"), dir.join("daily-greeted.json"), None)
    }

    #[tokio::test]
    async fn a_command_first_defers_the_walk_on_to_the_next_normal_message() {
        let m = manager();
        assert!(!m.claim_walk_on("lokati", "!playlist").await, "a command must not fire the walk-on");
        assert!(
            m.claim_walk_on("lokati", "hello everyone").await,
            "and must not have SPENT it either - the ruling is deferred to the next normal message, not skipped for the day, and this is the assertion that tells those two apart"
        );
        assert!(!m.claim_walk_on("lokati", "still here").await, "then it is spent, exactly once");
    }

    #[tokio::test]
    async fn a_normal_message_first_fires_once_and_only_once() {
        let m = manager();
        assert!(m.claim_walk_on("kibukah", "morning").await, "the ordinary path must be untouched by this change");
        assert!(!m.claim_walk_on("kibukah", "morning again").await, "second message of the day fires nothing");
    }

    #[tokio::test]
    async fn commands_all_day_never_fire_and_never_spend_it() {
        let m = manager();
        for text in ["!playlist", "!settheme https://example.invalid", "!playrandom", "!price essence"] {
            assert!(!m.claim_walk_on("sitch89", text).await, "{text} must not fire the walk-on");
        }
        assert!(
            m.claim_walk_on("sitch89", "finally saying something").await,
            "after a whole day of commands the slot must still be unclaimed - if any of them had consumed it this is where that shows up"
        );
    }

    #[tokio::test]
    async fn two_commands_then_a_normal_message_fires_on_the_third() {
        let m = manager();
        assert!(!m.claim_walk_on("qugetus_", "!playlist").await);
        assert!(!m.claim_walk_on("qugetus_", "!playrandom").await);
        assert!(m.claim_walk_on("qugetus_", "hi").await, "the third message is the first ordinary one, so it is the one that fires");
    }

    /// The two edge cases `parse_command`'s rule turns on, checked here
    /// too because this is where getting them wrong would be felt: a
    /// bare bang is ordinary chat and must fire, an unregistered command
    /// is still a command and must not.
    #[tokio::test]
    async fn a_bare_bang_is_chat_but_an_unknown_command_is_still_a_command() {
        let m = manager();
        assert!(!m.claim_walk_on("xborntokillx", "!notarealcommand").await, "unregistered still routes as a command, so it must not spend the walk-on");

        let m2 = manager();
        assert!(m2.claim_walk_on("xborntokillx", "!").await, "a bare ! has no command name - the dispatcher falls through and treats it as chat, so the walk-on must too");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-CALL directory. These tests write entrance-themes.json, and
    /// the suite runs them in parallel — a shared directory would have
    /// them reading each other's file back out of `new`.
    fn test_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("entrance-themes-test-{}-{unique}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn manager_in(dir: &std::path::Path) -> Arc<EntranceThemeManager> {
        // public_site_dir None disables the themes-data.json
        // regeneration, which is a display concern and needs a real site
        // folder.
        EntranceThemeManager::new(dir.join("entrance-themes.json"), dir.join("daily-greeted.json"), None)
    }

    /// Reads the file back through the SAME loader the bot boots with,
    /// rather than inspecting the in-memory map — the point of the test
    /// is that the removal survives a restart, and an in-memory check
    /// would pass even if nothing were ever written.
    fn persisted_keys(dir: &std::path::Path) -> Vec<String> {
        let themes: HashMap<String, ThemeEntry> =
            crate::state::load_json(dir.join("entrance-themes.json")).unwrap_or_default();
        let mut keys: Vec<String> = themes.keys().cloned().collect();
        keys.sort();
        keys
    }

    #[tokio::test]
    async fn removing_a_theme_persists_to_disk() {
        let dir = test_dir();
        let manager = manager_in(&dir);
        manager.set_theme("Alice", "https://youtu.be/aaaaaaaaaaa".to_string(), "A".to_string()).await;
        manager.set_theme("Bob", "https://youtu.be/bbbbbbbbbbb".to_string(), "B".to_string()).await;
        assert_eq!(persisted_keys(&dir), ["alice", "bob"]);

        assert!(manager.remove_theme("Alice").await, "removing a set theme reports success");

        assert_eq!(persisted_keys(&dir), ["bob"], "the removal is on disk, not just in the map");
    }

    /// Case folding is this manager's job and must match how `set_theme`
    /// keyed the entry in the first place, whatever case the mod typed.
    ///
    /// Stripping the leading `@` is the COMMAND layer's job, not this
    /// one's — `set_theme` would happily key an entry under `"@name"` if
    /// handed one. That the two commands strip it identically is asserted
    /// at the command level instead; see
    /// `commands::theme_remove_tests::an_at_prefix_and_a_bare_name_remove_the_same_entry`.
    #[tokio::test]
    async fn any_casing_of_a_name_reaches_the_same_entry() {
        let dir = test_dir();
        let manager = manager_in(&dir);
        manager.set_theme("MixedCase", "https://youtu.be/ccccccccccc".to_string(), "C".to_string()).await;
        assert_eq!(persisted_keys(&dir), ["mixedcase"], "set_theme keys by the lowercased name");

        assert!(manager.remove_theme("mixedcase").await, "an all-lowercase name reaches it");
        assert!(persisted_keys(&dir).is_empty());

        manager.set_theme("MixedCase", "https://youtu.be/ccccccccccc".to_string(), "C".to_string()).await;
        assert!(manager.remove_theme("MIXEDCASE").await, "and so does an all-uppercase one");
        assert!(persisted_keys(&dir).is_empty());
    }

    /// Nothing to remove must report that, so the caller can say so
    /// rather than claiming a removal that never happened.
    #[tokio::test]
    async fn removing_an_unknown_user_reports_no_theme_and_writes_nothing() {
        let dir = test_dir();
        let manager = manager_in(&dir);
        manager.set_theme("Alice", "https://youtu.be/aaaaaaaaaaa".to_string(), "A".to_string()).await;

        assert!(!manager.remove_theme("nobody").await, "an unknown user is not a removal");

        assert_eq!(persisted_keys(&dir), ["alice"], "and nothing was rewritten");
    }

    /// The removal must not disturb who has already been greeted today —
    /// that is a separate file with its own lifetime, and clearing it
    /// would re-trigger everyone's theme.
    #[tokio::test]
    async fn removing_a_theme_leaves_daily_greeted_alone() {
        let dir = test_dir();
        let manager = manager_in(&dir);
        manager.set_theme("Alice", "https://youtu.be/aaaaaaaaaaa".to_string(), "A".to_string()).await;
        // maybe_play_entrance_theme is what writes daily-greeted.json; with
        // no SongRequestManager it still records the greeting and returns.
        manager.maybe_play_entrance_theme("Alice", "hello chat", &None).await;
        let greeted_before = std::fs::read_to_string(dir.join("daily-greeted.json")).unwrap();

        assert!(manager.remove_theme("Alice").await);

        let greeted_after = std::fs::read_to_string(dir.join("daily-greeted.json")).unwrap();
        assert_eq!(greeted_before, greeted_after, "daily-greeted.json is not this command's business");
    }
}
