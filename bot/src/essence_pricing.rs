// Deafening Essence price tracking - snapshotted hourly and pushed to the
// same Apps Script backend the vessel pricing history uses, into an
// "EssencePricingHistory" sheet - lokati.net/essence-pricing.html polls
// that back out to show current prices plus history over time.
//
// Unlike Blood-filled Vessels, essences aren't traded via the item-listing
// search (no player lists an individual essence for sale that way) - they
// go through the bulk Currency Exchange instead, so poe.ninja's own
// aggregated exchange price is the right source here, not a live trade
// search. Much simpler than vessel_pricing.rs as a result: one poe.ninja
// call gets every essence type's price at once, no rate-limit tuning, no
// outlier/squatter filtering needed.

use crate::{build_feed, poe_ninja};
use std::time::Duration;

/// Same Apps Script Web App the build feed, playlists, and vessel pricing
/// already run through - see Code.gs's `doPost` for the
/// `syncEssencePricing` handler this posts to.
const APPS_SCRIPT_EXEC_URL: &str =
    "https://script.google.com/macros/s/AKfycbyPx7hialiC21-BbxqHGqVoPqVLczlKk4bx5hZhKZyHwnfumhtE2stPvqUoShYlOI4W/exec";

/// How often to snapshot prices for the history page - same cadence as
/// vessel pricing, for consistency between the two tracking pages.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Fire-and-forget push of one snapshot to the Apps Script backend - logs
/// on failure but never blocks/panics the caller, same as every other
/// sheet-sync in this codebase.
async fn sync_to_sheet(league: &str, prices: &[poe_ninja::EssencePrice], sync_secret: &str) {
    let essences_json: Vec<serde_json::Value> =
        prices.iter().map(|p| serde_json::json!({ "name": p.name, "priceChaos": p.chaos_price })).collect();
    let payload = serde_json::json!({
        "league": league,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "essences": essences_json,
    });

    let http = reqwest::Client::new();
    let result = http
        .post(APPS_SCRIPT_EXEC_URL)
        .query(&[("action", "syncEssencePricing"), ("secret", sync_secret)])
        .json(&payload)
        .send()
        .await;
    match result {
        Ok(resp) if resp.status().is_success() => tracing::info!("essence_pricing: synced hourly snapshot to sheet."),
        Ok(resp) => tracing::warn!("essence_pricing: sheet sync failed: HTTP {}", resp.status()),
        Err(err) => tracing::warn!("essence_pricing: sheet sync failed: {err}"),
    }
}

async fn snapshot_once(poe_ninja_http: &reqwest::Client, fallback_league: &str, sync_secret: &str) {
    let league = build_feed::current_league(poe_ninja_http).await.unwrap_or_else(|| fallback_league.to_string());
    match poe_ninja::fetch_deafening_essence_price_list(poe_ninja_http, &league).await {
        Ok(prices) => sync_to_sheet(&league, &prices, sync_secret).await,
        Err(err) => tracing::warn!("essence_pricing: poe.ninja fetch failed: {err}"),
    }
}

/// Spawns a background task that snapshots essence prices once
/// immediately and then every SNAPSHOT_INTERVAL, pushing each to the
/// price-history sheet lokati.net/essence-pricing.html reads from. A
/// no-op if no sync secret is configured (reuses PLAYLIST_SYNC_SECRET -
/// same bot, same site, no real benefit to a second secret).
pub fn spawn_hourly_snapshotter(poe_ninja_http: reqwest::Client, fallback_league: String, sync_secret: Option<String>) {
    let Some(sync_secret) = sync_secret else {
        tracing::info!("essence_pricing: no sync secret configured - hourly price history disabled.");
        return;
    };
    tokio::spawn(async move {
        snapshot_once(&poe_ninja_http, &fallback_league, &sync_secret).await;

        let mut interval = tokio::time::interval(SNAPSHOT_INTERVAL);
        interval.tick().await; // immediate first tick - already covered by the call above
        loop {
            interval.tick().await;
            snapshot_once(&poe_ninja_http, &fallback_league, &sync_secret).await;
        }
    });
}
