// Reads the "current league" the streamer's build spreadsheet says we're
// in — the same lokati.net/api/feed data the website's build pages pull
// from — so !essenceprofit automatically follows whatever league is set
// there instead of needing a second, separately maintained value that
// could drift out of sync with it.

use serde::Deserialize;
use std::collections::HashMap;

const FEED_URL: &str = "https://lokati.net/api/feed";

#[derive(Deserialize)]
struct BuildFeedEntry {
    #[serde(rename = "CurrentLeague")]
    current_league: Option<String>,
}

async fn fetch_current_league(http: &reqwest::Client) -> anyhow::Result<Option<String>> {
    let entries: Vec<BuildFeedEntry> = http.get(FEED_URL).send().await?.error_for_status()?.json().await?;

    // Mode across every row, not just the first one — a handful of builds
    // that haven't been updated to the new league yet shouldn't override
    // what the sheet is mostly saying.
    let mut counts: HashMap<String, u32> = HashMap::new();
    for entry in entries {
        if let Some(league) = entry.current_league {
            let league = league.trim().to_string();
            if !league.is_empty() {
                *counts.entry(league).or_insert(0) += 1;
            }
        }
    }

    Ok(counts.into_iter().max_by_key(|(_, count)| *count).map(|(league, _)| league))
}

/// None if the feed is unreachable, unparsable, or every row is blank —
/// callers should fall back to a configured default in that case rather
/// than failing outright.
pub async fn current_league(http: &reqwest::Client) -> Option<String> {
    match fetch_current_league(http).await {
        Ok(league) => league,
        Err(err) => {
            tracing::warn!("Failed to derive current league from build feed: {err}");
            None
        }
    }
}
