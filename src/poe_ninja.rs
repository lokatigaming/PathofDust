// Live poe.ninja economy lookups for !essenceprofit. Deliberately no
// caching/persistence at all — every command use polls poe.ninja fresh,
// per explicit request, since a stale price would actively mislead a
// profit estimate. Endpoints found by inspecting real browser network
// requests (poe.ninja's own API docs and third-party write-ups found
// online were stale/incorrect at the time this was written).

use serde::Deserialize;

const EXCHANGE_OVERVIEW_URL: &str = "https://poe.ninja/poe1/api/economy/exchange/current/overview";
const EXCHANGE_DETAILS_URL: &str = "https://poe.ninja/poe1/api/economy/exchange/current/details";

const ESSENCES_PER_MAP: f64 = 50.0;
const MAPS_PER_HOUR: f64 = 20.0;

// !ritualprofit — 5 Cloister scarabs + 4 Ritual Vessels invested per map,
// returning 65 Stacked Decks (sold at market price) plus each vessel's
// own average net profit (already net of what the vessel itself cost, so
// that's added straight to the total rather than separately subtracting
// vessel cost too — see the chat command's own comment for why).
const CLOISTER_SCARABS_PER_MAP: f64 = 5.0;
const RITUAL_VESSELS_PER_MAP: f64 = 4.0;
const STACKED_DECKS_PER_MAP: f64 = 65.0;
const RITUAL_VESSEL_NET_PROFIT: f64 = 20.0;

#[derive(Debug, thiserror::Error)]
pub enum PoeNinjaError {
    #[error("poe.ninja request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("No \"Deafening\" essence prices found on poe.ninja for league {0}")]
    NoEssencesFound(String),
    #[error("Could not find the Divine Orb exchange rate on poe.ninja for league {0}")]
    DivineRateNotFound(String),
    #[error("Could not find \"{0}\" on poe.ninja for league {1}")]
    ItemNotFound(String, String),
}

#[derive(Deserialize)]
struct ExchangeOverview {
    lines: Vec<ExchangeLine>,
}

#[derive(Deserialize)]
struct ExchangeLine {
    id: String,
    #[serde(rename = "primaryValue")]
    primary_value: f64,
}

#[derive(Deserialize)]
struct ExchangeDetails {
    pairs: Vec<ExchangePair>,
}

#[derive(Deserialize)]
struct ExchangePair {
    id: String,
    rate: f64,
}

/// One specific item's chaos price from an exchange overview — used for
/// the Cloister scarab, Ritual Vessel, and Stacked Deck, each a single
/// exact id rather than an averaged-across-variants group like essences.
async fn fetch_price_by_id(
    http: &reqwest::Client,
    league: &str,
    item_type: &str,
    id: &str,
) -> Result<f64, PoeNinjaError> {
    let resp = http
        .get(EXCHANGE_OVERVIEW_URL)
        .query(&[("league", league), ("type", item_type)])
        .send()
        .await?
        .error_for_status()?;
    let data: ExchangeOverview = resp.json().await?;

    data.lines
        .into_iter()
        .find(|l| l.id == id)
        .map(|l| l.primary_value)
        .ok_or_else(|| PoeNinjaError::ItemNotFound(id.to_string(), league.to_string()))
}

pub struct RitualScarabProfitResult {
    pub cloister_price: f64,
    pub ritual_vessel_price: f64,
    pub stacked_deck_price: f64,
    pub chaos_per_divine: f64,
    pub chaos_per_map: f64,
    pub chaos_per_hour: f64,
    pub divine_per_hour: f64,
}

/// https://poe.ninja/poe1/economy/{league}/scarabs?name=cloister,
/// .../fragments?name=Ritual+Vessel, and .../currency?name=Stacked+Deck —
/// 5 Cloister scarabs + 4 Ritual Vessels invested per map, returning 65
/// Stacked Decks at market price plus each vessel's own ~20c average net
/// profit (already net of the vessel's own cost — confirmed with the
/// streamer directly, since "cost" and "profit over cost" mentioned
/// together read ambiguously otherwise). 20 maps/hour, same as essences.
pub async fn ritual_scarab_profit_per_hour(http: &reqwest::Client, league: &str) -> Result<RitualScarabProfitResult, PoeNinjaError> {
    let cloister_price = fetch_price_by_id(http, league, "Scarab", "divination-scarab-of-the-cloister").await?;
    let ritual_vessel_price = fetch_price_by_id(http, league, "Fragment", "ritual-vessel").await?;
    let stacked_deck_price = fetch_price_by_id(http, league, "Currency", "stacked-deck").await?;
    let chaos_per_divine = fetch_chaos_per_divine(http, league).await?;

    let chaos_per_map = (STACKED_DECKS_PER_MAP * stacked_deck_price) + (RITUAL_VESSELS_PER_MAP * RITUAL_VESSEL_NET_PROFIT)
        - (CLOISTER_SCARABS_PER_MAP * cloister_price);
    let chaos_per_hour = chaos_per_map * MAPS_PER_HOUR;
    let divine_per_hour = chaos_per_hour / chaos_per_divine;

    Ok(RitualScarabProfitResult {
        cloister_price,
        ritual_vessel_price,
        stacked_deck_price,
        chaos_per_divine,
        chaos_per_map,
        chaos_per_hour,
        divine_per_hour,
    })
}

pub struct EssenceProfitResult {
    pub avg_chaos_per_essence: f64,
    pub essence_type_count: usize,
    pub chaos_per_divine: f64,
    pub chaos_per_hour: f64,
    pub divine_per_hour: f64,
}

pub struct EssencePrice {
    /// Display name only, e.g. "Doubt" — always a Deafening essence, so
    /// that tier prefix isn't repeated per-entry.
    pub name: String,
    pub chaos_price: f64,
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// https://poe.ninja/poe1/economy/{league}/essences?name=deaf — every
/// "Deafening Essence of X" variant. IDs look like
/// "deafening-essence-of-doubt"; not traded via the item-listing search
/// vessels use (essences go through the bulk Currency Exchange instead),
/// so poe.ninja's own aggregated exchange price is the source here rather
/// than a live trade-listing lookup - confirmed with the streamer this is
/// the right call.
pub async fn fetch_deafening_essence_price_list(http: &reqwest::Client, league: &str) -> Result<Vec<EssencePrice>, PoeNinjaError> {
    let resp = http
        .get(EXCHANGE_OVERVIEW_URL)
        .query(&[("league", league), ("type", "Essence")])
        .send()
        .await?
        .error_for_status()?;
    let data: ExchangeOverview = resp.json().await?;

    Ok(data
        .lines
        .into_iter()
        .filter(|l| l.id.starts_with("deafening-essence-of-"))
        .map(|l| EssencePrice { name: title_case(&l.id.trim_start_matches("deafening-essence-of-").replace('-', " ")), chaos_price: l.primary_value })
        .collect())
}

/// https://poe.ninja/poe1/economy/{league}/currency/divine-orb — chaos
/// orbs per Divine Orb right now.
pub async fn fetch_chaos_per_divine(http: &reqwest::Client, league: &str) -> Result<f64, PoeNinjaError> {
    let resp = http
        .get(EXCHANGE_DETAILS_URL)
        .query(&[("league", league), ("type", "Currency"), ("id", "divine-orb")])
        .send()
        .await?
        .error_for_status()?;
    let data: ExchangeDetails = resp.json().await?;

    data.pairs
        .into_iter()
        .find(|p| p.id == "chaos")
        .map(|p| p.rate)
        .ok_or_else(|| PoeNinjaError::DivineRateNotFound(league.to_string()))
}

/// Assumes 50 essences drop per map on average, 20 maps per hour — fixed
/// assumptions per how this was asked for, not something chat can tune.
pub async fn essence_profit_per_hour(http: &reqwest::Client, league: &str) -> Result<EssenceProfitResult, PoeNinjaError> {
    let prices = fetch_deafening_essence_price_list(http, league).await?;
    if prices.is_empty() {
        return Err(PoeNinjaError::NoEssencesFound(league.to_string()));
    }
    let avg_chaos_per_essence = prices.iter().map(|p| p.chaos_price).sum::<f64>() / prices.len() as f64;

    let chaos_per_divine = fetch_chaos_per_divine(http, league).await?;

    let chaos_per_hour = avg_chaos_per_essence * ESSENCES_PER_MAP * MAPS_PER_HOUR;
    let divine_per_hour = chaos_per_hour / chaos_per_divine;

    Ok(EssenceProfitResult {
        avg_chaos_per_essence,
        essence_type_count: prices.len(),
        chaos_per_divine,
        chaos_per_hour,
        divine_per_hour,
    })
}
