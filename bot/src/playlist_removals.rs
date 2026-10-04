// lokati.net/playlist.html's trash icon. The bot has no public address,
// so — same shape as PayPal tips (see paypal.rs) — the relay Worker
// (cloudflare-paypal-relay/worker.js) checks the viewer's Twitch login,
// queues `{id, login, videoId, at}`, and this polls its
// `/pending-playlist-removals` endpoint and applies each one through
// PersonalPlaylistManager, which persists and syncs the sheet as it does
// for any change.
//
// Duplicates are allowed in a playlist, so applying one removal twice
// would delete a second copy. Each removal's id is recorded on disk
// *before* the song is removed and any id already recorded is skipped:
// the relay drains on read, but KV listing is eventually consistent and
// can hand the same key out again. A crash between recording an id and
// removing the song loses that one removal (the song stays and the
// viewer can click again) — it never applies twice.

use crate::personal_playlists::PersonalPlaylistManager;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Applied ids are kept this long — far past the relay's one-day queue
/// TTL and KV's ~60s listing staleness.
const APPLIED_KEEP_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRemoval {
    pub id: String,
    pub login: String,
    pub video_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppliedRemoval {
    id: String,
    at: u64,
}

pub struct PlaylistRemovalWatcher {
    relay_url: String,
    relay_token: String,
    http: reqwest::Client,
    playlists: Arc<PersonalPlaylistManager>,
    applied: Mutex<Vec<AppliedRemoval>>,
    applied_path: PathBuf,
}

impl PlaylistRemovalWatcher {
    pub fn new(relay_url: String, relay_token: String, playlists: Arc<PersonalPlaylistManager>, applied_path: PathBuf) -> Self {
        let loaded: Vec<AppliedRemoval> = crate::state::load_json(&applied_path).unwrap_or_default();
        Self {
            relay_url,
            relay_token,
            http: reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("reqwest client build"),
            playlists,
            applied: Mutex::new(loaded),
            applied_path,
        }
    }

    async fn poll(&self) -> anyhow::Result<()> {
        let resp = self
            .http
            .get(format!("{}/pending-playlist-removals", self.relay_url.trim_end_matches('/')))
            .bearer_auth(&self.relay_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Playlist removal relay returned {status}: {body}");
        }

        let removals: Vec<PendingRemoval> = resp.json().await?;
        for removal in &removals {
            self.apply(removal).await;
        }
        Ok(())
    }

    /// Applies one queued removal at most once, keyed on its id.
    pub async fn apply(&self, removal: &PendingRemoval) {
        let mut applied = self.applied.lock().await;
        if applied.iter().any(|a| a.id == removal.id) {
            return;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        applied.retain(|a| now.saturating_sub(a.at) < APPLIED_KEEP_SECS);
        applied.push(AppliedRemoval { id: removal.id.clone(), at: now });
        if let Err(err) = crate::state::save_json(&self.applied_path, &*applied) {
            // Not recorded, so not applied: a retry must not be able to
            // remove a second copy.
            applied.pop();
            tracing::error!("Failed to persist playlist-removals-applied.json, skipping removal {}: {err}", removal.id);
            return;
        }

        match self.playlists.remove_video(&removal.login, &removal.video_id).await {
            Some(song) => tracing::info!("Playlist: website removal — {} removed {} ({})", removal.login, removal.video_id, song.title),
            None => tracing::debug!("Playlist: website removal for {} / {} — already gone", removal.login, removal.video_id),
        }
    }
}

pub fn start_playlist_removal_watcher(watcher: PlaylistRemovalWatcher, poll_interval_ms: u64) -> Arc<PlaylistRemovalWatcher> {
    let watcher = Arc::new(watcher);
    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(poll_interval_ms));
            loop {
                interval.tick().await;
                if let Err(err) = watcher.poll().await {
                    tracing::error!("Playlist removal relay poll failed: {}", crate::redact::redact(&err.to_string()));
                }
            }
        });
    }
    watcher
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_playlists::SampleOutcome;
    use crate::song_requests::Song;
    use std::collections::HashSet;

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("playlist-removals-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn song(video_id: &str, title: &str) -> Song {
        Song { video_id: video_id.to_string(), title: title.to_string(), duration_secs: 200, requested_by: "Viewer".to_string(), thumbnail_url: String::new() }
    }

    fn removal(id: &str, login: &str, video_id: &str) -> PendingRemoval {
        PendingRemoval { id: id.to_string(), login: login.to_string(), video_id: video_id.to_string() }
    }

    async fn setup(songs: &[Song]) -> (PlaylistRemovalWatcher, Arc<PersonalPlaylistManager>, PathBuf) {
        let dir = temp_dir();
        let playlists = PersonalPlaylistManager::new(dir.join("personal-playlists.json"), None);
        for s in songs {
            playlists.add_song("Viewer", s).await;
        }
        let watcher = PlaylistRemovalWatcher::new("http://relay.test".into(), "t".into(), playlists.clone(), dir.join("applied.json"));
        (watcher, playlists, dir)
    }

    async fn video_ids(playlists: &PersonalPlaylistManager) -> Vec<String> {
        match playlists.sample("viewer", 100, &HashSet::new()).await {
            SampleOutcome::Songs { songs, .. } => {
                let mut ids: Vec<String> = songs.into_iter().map(|s| s.video_id).collect();
                ids.sort();
                ids
            }
            _ => Vec::new(),
        }
    }

    #[tokio::test]
    async fn one_removal_deletes_exactly_one_of_two_duplicates_and_syncs_once() {
        let (watcher, playlists, _dir) = setup(&[song("dup", "Twice"), song("other", "Keep"), song("dup", "Twice")]).await;
        let before = playlists.sync_calls();
        watcher.apply(&removal("r1", "viewer", "dup")).await;
        assert_eq!(video_ids(&playlists).await, ["dup", "other"]);
        assert_eq!(playlists.sync_calls() - before, 1);
    }

    #[tokio::test]
    async fn same_removal_id_delivered_twice_deletes_one() {
        let (watcher, playlists, dir) = setup(&[song("dup", "Twice"), song("dup", "Twice")]).await;
        let before = playlists.sync_calls();
        watcher.apply(&removal("r1", "viewer", "dup")).await;
        watcher.apply(&removal("r1", "viewer", "dup")).await;
        // A restarted bot reloads the applied ids from disk and still skips it.
        let restarted = PlaylistRemovalWatcher::new("http://relay.test".into(), "t".into(), playlists.clone(), dir.join("applied.json"));
        restarted.apply(&removal("r1", "viewer", "dup")).await;
        assert_eq!(video_ids(&playlists).await, ["dup"]);
        assert_eq!(playlists.sync_calls() - before, 1);
    }

    #[tokio::test]
    async fn missing_song_is_a_quiet_no_op() {
        let (watcher, playlists, _dir) = setup(&[song("a", "A")]).await;
        let before = playlists.sync_calls();
        watcher.apply(&removal("r1", "viewer", "gone")).await;
        watcher.apply(&removal("r2", "nobody", "a")).await;
        assert_eq!(video_ids(&playlists).await, ["a"]);
        assert_eq!(playlists.sync_calls() - before, 0);
    }

    #[test]
    fn pending_removal_parses_the_relay_shape() {
        let parsed: Vec<PendingRemoval> =
            serde_json::from_str(r#"[{"id":"u1","login":"viewer","videoId":"dQw4w9WgXcQ","at":1759550000000}]"#).unwrap();
        assert_eq!(parsed[0].id, "u1");
        assert_eq!(parsed[0].login, "viewer");
        assert_eq!(parsed[0].video_id, "dQw4w9WgXcQ");
    }
}
