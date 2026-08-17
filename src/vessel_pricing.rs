// Blood-filled Vessel pricing - live pathofexile.com/api/trade lookups for
// what to list our own vessels at (the item that actually varies in price
// by monster composition - not the empty "Ritual Vessel", which is a flat-
// priced consumable with no composition data). For each of the 6 total-
// monster-count bands, prices both the baseline (any/no unique monster)
// and each of 1/2/3+ unique (named) monsters combined with that same
// band - a genuine 2D cross-tab, not two independent dimensions - finding
// the cheapest currently-securable price that isn't an obvious mispriced/
// troll outlier. The goal is tying the real competitive floor at maximum
// profit, not undercutting a fluke.
//
// Deliberately hits the official trade site's own public search API (the
// same one pathofexile.com/trade itself uses), not the Public Stash Tab
// bulk-export API - that one requires a service:psapi OAuth scope GGG only
// grants to approved partner services, not ad-hoc scripts.
//
// Snapshotted hourly (see spawn_hourly_snapshotter) and pushed to the same
// Apps Script backend the build feed/playlists/announcements already use,
// into a "VesselPricingHistory" sheet - lokati.net/vessel-pricing.html
// polls that back out to show current prices plus the price history over
// time. !vesselprice in chat just links there rather than running a live
// query itself, both for speed and to avoid hammering the trade API on
// every chat invocation.

use crate::{build_feed, poe_ninja};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

const TRADE_SEARCH_URL: &str = "https://www.pathofexile.com/api/trade/search";
const TRADE_FETCH_URL: &str = "https://www.pathofexile.com/api/trade/fetch";
const VESSEL_TYPE: &str = "Blood-filled Vessel";
const OTHER_MONSTERS_STAT_ID: &str = "pseudo.pseudo_ritual_other_monsters";
// "pseudo.pseudo_ritual_unique_monsters" - no longer queried live (see
// recommend_vessel_prices); kept only as a reference for whoever builds
// the manual-check links in vessel-pricing.html.

/// Same Apps Script Web App the build feed, playlists, and announcements
/// already run through - see Code.gs's `doPost` for the `syncVesselPricing`
/// handler this posts to.
const APPS_SCRIPT_EXEC_URL: &str =
    "https://script.google.com/macros/s/AKfycbyPx7hialiC21-BbxqHGqVoPqVLczlKk4bx5hZhKZyHwnfumhtE2stPvqUoShYlOI4W/exec";

/// How often to snapshot prices for the history page.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// A browser-like User-Agent, since the trade site's own bot protection
/// treats reqwest's bare default UA differently - confirmed necessary via
/// direct testing, not a defensive guess.
const TRADE_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// (label, min, max) filtered against the "other" (non-named) monster
/// count, since that's the stat trade actually lets us filter by - a
/// vessel with a named monster or two can land slightly above where its
/// true total would place it, which doesn't matter at this band width.
/// 100+ splits into 20-wide bands (vs. 1-59/60-99 staying coarse) - real
/// price variance in that range turned out too wide for 40-wide bands to
/// track meaningfully.
const TOTAL_BANDS: &[(&str, u32, Option<u32>)] = &[
    ("1-59", 1, Some(59)),
    ("60-99", 60, Some(99)),
    ("100-119", 100, Some(119)),
    ("120-139", 120, Some(139)),
    ("140-159", 140, Some(159)),
    ("160-179", 160, Some(179)),
    ("180-199", 180, Some(199)),
    ("200-219", 200, Some(219)),
    ("220+", 220, None),
];


/// Keeps pulling batches until at least this many *valid* samples are
/// collected, rather than capping the raw listing count up front - a
/// fixed raw cap bit the cheapest bands early on, when a client-side
/// price-type filter (since removed - see extract_listing) discarded
/// most of what got fetched. MAX_RAW_IDS is the hard ceiling regardless
/// (trade search itself caps around 100 results anyway).
const MIN_VALID_SAMPLES: usize = 15;
const MAX_RAW_IDS: usize = 100;
const FETCH_BATCH: usize = 10;

/// A candidate price is trusted as the real floor only if at least this
/// many *distinct sellers* (itself included) are priced within
/// CLUSTER_TOLERANCE of it - squatter/mispriced listings are almost
/// always lone outliers with nothing priced anywhere near them, while a
/// genuine market floor has other real sellers clustered close by.
/// Counting distinct accounts rather than raw listing count matters: one
/// seller bulk-listing several vessels at the same throwaway price would
/// otherwise look like market consensus on its own. Falls back to
/// requiring less support if a band's sample is too thin to ever find a
/// 3-seller cluster.
///
/// 2.5 was too loose: a real case caught this directly - a single 19c
/// listing got accepted as "supported" purely because a genuine cluster
/// of unrelated sellers at 34-40c happened to fall inside its 2.5x
/// window (up to 47.5c), even though nothing was actually priced near
/// 19c itself. 1.5 still comfortably covers normal seller-to-seller price
/// variance (the historical median-vs-trimmed-average spread across
/// bands ran roughly 20-50%) while no longer treating "some other seller
/// happens to be within 150% of this" as agreement.
const CLUSTER_MIN_SUPPORT: usize = 3;
const CLUSTER_TOLERANCE: f64 = 1.5;

/// Retries a 429 (rate-limited) response rather than silently giving up.
/// There's no rush here (this only needs to finish once an hour), so
/// backing off generously is free.
const MAX_RETRIES: u32 = 4;
const RETRY_BACKOFF: Duration = Duration::from_secs(15);

/// One priced cell in the band x unique-tier grid. `variant` is "total"
/// for the baseline (no unique-monster filter) or "unique1"/"unique2"/
/// "unique3plus" when combined with that tier - `band` is always a
/// total-monster range label either way. `trade_url` (baseline only) is a
/// direct link to the exact live trade search this cell's price came
/// from - the search id trade hands back on every query is a stable,
/// bookmarkable link that re-runs live on each visit, so this costs
/// nothing extra to capture and lets a mod eyeball-verify a band's
/// current cheapest price against the real site.
pub struct Cell {
    pub band: &'static str,
    pub variant: &'static str,
    pub sampled: usize,
    pub recommended_chaos: Option<f64>,
    pub trade_url: Option<String>,
}

pub struct VesselPriceReport {
    pub league: String,
    pub cells: Vec<Cell>,
}

#[derive(Deserialize)]
struct SearchResponse {
    id: String,
    result: Vec<String>,
}

#[derive(Deserialize)]
struct FetchResponse {
    result: Vec<FetchEntry>,
}

#[derive(Deserialize)]
struct FetchEntry {
    listing: TradeListing,
}

#[derive(Deserialize)]
struct TradeListing {
    price: Option<TradePrice>,
    account: TradeAccount,
    indexed: String,
}

#[derive(Deserialize)]
struct TradeAccount {
    name: String,
}

#[derive(Deserialize)]
struct TradePrice {
    amount: f64,
    currency: String,
}

struct ListingSample {
    price_chaos: f64,
    account: String,
}

/// A listing has to have survived at least this long before it counts as
/// an established price rather than noise - a listing that's about to get
/// (or already got) instantly bought by someone faster than us doesn't
/// tell us what the market will actually bear; it's just whoever undercut
/// everyone for a moment. Direct fix for a real case: a lone 19c listing
/// (real floor was ~34c) got treated as legitimate purely because a
/// genuine cluster happened to fall inside its tolerance window - filtering
/// out anything that fresh would have excluded it outright regardless of
/// the tolerance math.
const MIN_LISTING_AGE_SECS: i64 = 10 * 60;

/// Whether a listing is actually purchasable isn't about its price "type"
/// text ("~price" vs "~b/o") - both show up under a status:securable
/// search (the same "Instant Buyout" mode the trade site's own UI filters
/// to), which is the real signal to gate on (see search_cell). An earlier
/// version filtered client-side on price type instead and got it exactly
/// backwards - confirmed directly against the live site with the
/// streamer, whose real search returned "~b/o" listings as the genuine
/// cheapest instant-buyout price.
fn extract_listing(entry: &FetchEntry, chaos_per_divine: Option<f64>) -> Option<ListingSample> {
    let price = entry.listing.price.as_ref()?;

    let indexed = chrono::DateTime::parse_from_rfc3339(&entry.listing.indexed).ok()?;
    let age = chrono::Utc::now().signed_duration_since(indexed);
    if age.num_seconds() < MIN_LISTING_AGE_SECS {
        return None;
    }

    let price_chaos = match price.currency.as_str() {
        "chaos" => price.amount,
        "divine" => price.amount * chaos_per_divine?,
        _ => return None, // barter/other currencies - skip rather than guess
    };
    Some(ListingSample { price_chaos, account: entry.listing.account.name.clone() })
}

/// The cheapest price that has real support nearby wins - walks the
/// sorted price list looking for the first one with at least
/// CLUSTER_MIN_SUPPORT *distinct sellers* (itself included) priced within
/// CLUSTER_TOLERANCE of it. Relaxes the support requirement in stages if
/// the sample's too thin to ever satisfy it (a band with only 1-2 total
/// listings has nothing to cluster against, so the cheapest available is
/// the best answer there is).
fn recommend_price(samples: &[ListingSample]) -> Option<(f64, usize)> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<&ListingSample> = samples.iter().collect();
    sorted.sort_by(|a, b| a.price_chaos.partial_cmp(&b.price_chaos).unwrap());

    for min_support in [CLUSTER_MIN_SUPPORT, 2, 1] {
        for (i, candidate) in sorted.iter().enumerate() {
            let price = candidate.price_chaos;
            // Sorted ascending, so once one entry exceeds the tolerance
            // window every later one does too - take_while's contiguous
            // scan is exact, not an approximation.
            let distinct_sellers: HashSet<&str> =
                sorted[i..].iter().take_while(|s| s.price_chaos <= price * CLUSTER_TOLERANCE).map(|s| s.account.as_str()).collect();
            if distinct_sellers.len() >= min_support {
                return Some((price, sorted.len()));
            }
        }
    }
    None
}

/// `filters` are combined with AND - e.g. total-monster range plus a
/// unique-monster-count range for a cross-tabbed cell, or just the
/// total-monster range alone for the baseline.
async fn search_cell(http: &reqwest::Client, league: &str, filters: &[(&str, u32, Option<u32>)]) -> Option<(String, Vec<String>)> {
    let filter_json: Vec<serde_json::Value> = filters
        .iter()
        .map(|(stat_id, min, max)| {
            let mut value = serde_json::json!({ "min": min });
            if let Some(max) = max {
                value["max"] = serde_json::json!(max);
            }
            serde_json::json!({ "id": stat_id, "value": value })
        })
        .collect();

    let body = serde_json::json!({
        "query": {
            // "securable" is what the trade site's own "Instant Buyout"
            // filter sets - confirmed by fetching a real search the
            // streamer ran through the site UI (id 8rK4Y8ZGFV) and reading
            // its stored query back. "online" (used originally) is a much
            // weaker signal - just "account shows online", not "this
            // exact listing is actually purchasable right now" - and was
            // the real cause of implausible cheap results earlier.
            "status": { "option": "securable" },
            "type": VESSEL_TYPE,
            "stats": [{ "type": "and", "filters": filter_json }]
        },
        "sort": { "price": "asc" }
    });

    let url = format!("{TRADE_SEARCH_URL}/{league}");
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(RETRY_BACKOFF).await;
        }
        let Ok(resp) = http.post(&url).json(&body).send().await else { continue };
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!("vessel_pricing: search for {filters:?} rate-limited (attempt {attempt}), retrying...");
            continue;
        }
        if !resp.status().is_success() {
            tracing::warn!("vessel_pricing: search for {filters:?} returned {}", resp.status());
            return None;
        }
        let Ok(data) = resp.json::<SearchResponse>().await else { return None };
        if data.result.is_empty() {
            return None;
        }
        return Some((data.id, data.result));
    }
    tracing::warn!("vessel_pricing: search for {filters:?} exhausted retries.");
    None
}

async fn fetch_cell_items(http: &reqwest::Client, query_id: &str, ids: &[String]) -> Option<Vec<FetchEntry>> {
    let ids_param = ids.join(",");
    let url = format!("{TRADE_FETCH_URL}/{ids_param}?query={query_id}");
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(RETRY_BACKOFF).await;
        }
        let Ok(resp) = http.get(&url).send().await else { continue };
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!("vessel_pricing: fetch rate-limited (attempt {attempt}), retrying...");
            continue;
        }
        if !resp.status().is_success() {
            tracing::warn!("vessel_pricing: fetch returned {}", resp.status());
            return None;
        }
        let Ok(data) = resp.json::<FetchResponse>().await else { return None };
        return Some(data.result);
    }
    tracing::warn!("vessel_pricing: fetch exhausted retries.");
    None
}

async fn price_cell(
    http: &reqwest::Client,
    league: &str,
    filters: &[(&str, u32, Option<u32>)],
    chaos_per_divine: Option<f64>,
) -> (usize, Option<f64>, Option<String>) {
    let Some((query_id, ids)) = search_cell(http, league, filters).await else {
        return (0, None, None);
    };
    let capped: Vec<String> = ids.into_iter().take(MAX_RAW_IDS).collect();

    let mut samples = Vec::new();
    for (i, chunk) in capped.chunks(FETCH_BATCH).enumerate() {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let Some(entries) = fetch_cell_items(http, &query_id, chunk).await else { continue };
        samples.extend(entries.iter().filter_map(|e| extract_listing(e, chaos_per_divine)));
        if samples.len() >= MIN_VALID_SAMPLES {
            break;
        }
    }

    let trade_url = Some(format!("https://www.pathofexile.com/trade/search/{league}/{query_id}"));
    match recommend_price(&samples) {
        Some((price, n)) => (n, Some(price), trade_url),
        None => (samples.len(), None, trade_url),
    }
}

/// Runs one baseline (no unique-monster filter) query per total-monster
/// band, sequentially with a delay between requests - respects the trade
/// API's rate limits rather than firing them all at once. Unique-tier
/// pricing is *not* queried live anymore (it was, and combined with the
/// larger band count this grew into far more requests than the trade API
/// would tolerate in an hour - sustained 429s on nearly every request).
/// The page links out to a manual trade search for those instead - see
/// vessel-pricing.html.
pub async fn recommend_vessel_prices(league: &str) -> VesselPriceReport {
    let http = reqwest::Client::builder().user_agent(TRADE_USER_AGENT).build().unwrap_or_default();

    // A separate plain client for poe.ninja - different site, its own
    // (more permissive) rate limits, no need for the trade-specific UA.
    let poe_ninja_http = reqwest::Client::new();
    let chaos_per_divine = poe_ninja::fetch_chaos_per_divine(&poe_ninja_http, league).await.ok();

    let mut cells = Vec::with_capacity(TOTAL_BANDS.len());
    for (i, (band_label, band_min, band_max)) in TOTAL_BANDS.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(2000)).await;
        }

        let baseline_filters = [(OTHER_MONSTERS_STAT_ID, *band_min, *band_max)];
        let (sampled, recommended_chaos, trade_url) = price_cell(&http, league, &baseline_filters, chaos_per_divine).await;
        cells.push(Cell { band: band_label, variant: "total", sampled, recommended_chaos, trade_url });
    }

    VesselPriceReport { league: league.to_string(), cells }
}

/// Fire-and-forget push of one snapshot to the Apps Script backend - logs
/// on failure but never blocks/panics the caller, same as every other
/// sheet-sync in this codebase (see personal_playlists.rs).
async fn sync_to_sheet(report: &VesselPriceReport, sync_secret: &str) {
    let cells_json: Vec<serde_json::Value> = report
        .cells
        .iter()
        .map(|c| {
            serde_json::json!({
                "type": c.variant,
                "band": c.band,
                "priceChaos": c.recommended_chaos,
                "sampled": c.sampled,
                "tradeUrl": c.trade_url,
            })
        })
        .collect();
    let payload = serde_json::json!({
        "league": report.league,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "cells": cells_json,
    });

    let http = reqwest::Client::new();
    let result = http
        .post(APPS_SCRIPT_EXEC_URL)
        .query(&[("action", "syncVesselPricing"), ("secret", sync_secret)])
        .json(&payload)
        .send()
        .await;
    match result {
        Ok(resp) if resp.status().is_success() => tracing::info!("vessel_pricing: synced hourly snapshot to sheet."),
        Ok(resp) => tracing::warn!("vessel_pricing: sheet sync failed: HTTP {}", resp.status()),
        Err(err) => tracing::warn!("vessel_pricing: sheet sync failed: {err}"),
    }
}

async fn snapshot_once(poe_ninja_http: &reqwest::Client, fallback_league: &str, sync_secret: &str) {
    let league = build_feed::current_league(poe_ninja_http).await.unwrap_or_else(|| fallback_league.to_string());
    let report = recommend_vessel_prices(&league).await;
    sync_to_sheet(&report, sync_secret).await;
}

/// Spawns a background task that snapshots vessel prices once immediately
/// and then every SNAPSHOT_INTERVAL, pushing each to the price-history
/// sheet lokati.net/vessel-pricing.html reads from. A no-op if no sync
/// secret is configured (reuses PLAYLIST_SYNC_SECRET - same bot, same
/// site, no real benefit to a second secret).
pub fn spawn_hourly_snapshotter(poe_ninja_http: reqwest::Client, fallback_league: String, sync_secret: Option<String>) {
    let Some(sync_secret) = sync_secret else {
        tracing::info!("vessel_pricing: no sync secret configured - hourly price history disabled.");
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
