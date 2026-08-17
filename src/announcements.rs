// Periodic chat announcements. Text lives in the "Announcements" tab of
// the same Google Sheet the build feed uses (one message per row) instead
// of being programmed into the bot — edit/add/remove/reorder rows there
// and it takes effect on the very next periodic firing, no code change or
// bot restart needed. Fetched fresh (via lokati-feed-cache, same as the
// build feed) on every use rather than loaded once/cached, since the
// whole point is that it's editable on the fly.

use std::sync::atomic::{AtomicUsize, Ordering};

const ANNOUNCEMENTS_URL: &str = "https://lokati.net/api/feed?announcements=1";

pub struct Announcements {
    http: reqwest::Client,
    next_index: AtomicUsize,
}

impl Announcements {
    pub fn new() -> Self {
        Self { http: reqwest::Client::new(), next_index: AtomicUsize::new(0) }
    }

    /// The current announcement list, straight from the sheet. Empty (not
    /// an error) if the sheet has nothing in it, the fetch fails, or the
    /// "Announcements" tab doesn't exist yet — callers treat all of those
    /// as "nothing to announce right now" rather than needing to handle
    /// them separately.
    pub async fn all(&self) -> Vec<String> {
        match self.http.get(ANNOUNCEMENTS_URL).send().await {
            Ok(resp) => match resp.json::<Vec<String>>().await {
                Ok(list) => list,
                Err(err) => {
                    tracing::warn!("Failed to parse announcements from {ANNOUNCEMENTS_URL}: {err}");
                    Vec::new()
                }
            },
            Err(err) => {
                tracing::warn!("Failed to fetch announcements from {ANNOUNCEMENTS_URL}: {err}");
                Vec::new()
            }
        }
    }

    /// Cycles through the current list in order, wrapping back to the
    /// start. Returns None if it's currently empty (see `all`'s doc
    /// comment for what that covers). The cycle position is just an
    /// index into whatever the list looks like *right now* — editing the
    /// sheet between calls can shift what's "next" (adding/removing rows
    /// changes indices), which is an acceptable, purely cosmetic
    /// consequence of the list being live-editable at all.
    pub async fn next(&self) -> Option<String> {
        let list = self.all().await;
        if list.is_empty() {
            return None;
        }
        let i = self.next_index.fetch_add(1, Ordering::SeqCst) % list.len();
        Some(list[i].clone())
    }
}
