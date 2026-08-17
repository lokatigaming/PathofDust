// Emote lookup for the chat overlay — fetches Twitch (Helix), BTTV, and FFZ
// emote sets once at startup (global + channel-specific) and merges them
// into a single name->URL map the overlay's frontend JS uses to replace
// emote codes with <img> tags. Twitch/BTTV/FFZ names can collide; first
// match wins in the order they're merged below (Twitch takes priority
// since it's the platform-native set).

use std::collections::HashMap;

use crate::twitch::auth::AuthClient;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmoteMap {
    /// emote code -> image URL
    pub emotes: HashMap<String, String>,
}

pub async fn fetch_all(auth: Arc<AuthClient>, broadcaster_id: &str, channel_login: &str) -> EmoteMap {
    let http = reqwest::Client::new();
    let mut emotes = HashMap::new();

    // BTTV and FFZ global sets first (lowest priority — overwritten by
    // channel-specific and Twitch emotes below if names collide).
    if let Some(global) = fetch_bttv_global(&http).await {
        emotes.extend(global);
    }
    if let Some(global) = fetch_ffz_global(&http).await {
        emotes.extend(global);
    }
    if let Some(channel) = fetch_bttv_channel(&http, broadcaster_id).await {
        emotes.extend(channel);
    }
    if let Some(channel) = fetch_ffz_channel(&http, channel_login).await {
        emotes.extend(channel);
    }
    if let Some(twitch_global) = fetch_twitch_global(&http, &auth).await {
        emotes.extend(twitch_global);
    }
    if let Some(twitch_channel) = fetch_twitch_channel(&http, &auth, broadcaster_id).await {
        emotes.extend(twitch_channel);
    }

    tracing::info!("Loaded {} emotes for the chat overlay.", emotes.len());
    EmoteMap { emotes }
}

fn bttv_emote_url(id: &str) -> String {
    format!("https://cdn.betterttv.net/emote/{id}/2x")
}

async fn fetch_bttv_global(http: &reqwest::Client) -> Option<HashMap<String, String>> {
    let data: Vec<serde_json::Value> =
        http.get("https://api.betterttv.net/3/cached/emotes/global").send().await.ok()?.json().await.ok()?;
    Some(parse_bttv_emote_list(&data))
}

async fn fetch_bttv_channel(http: &reqwest::Client, broadcaster_id: &str) -> Option<HashMap<String, String>> {
    let url = format!("https://api.betterttv.net/3/cached/users/twitch/{broadcaster_id}");
    let resp = http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        // No BTTV customization for this channel — normal, not an error.
        return None;
    }
    let data: serde_json::Value = resp.json().await.ok()?;
    let mut map = HashMap::new();
    for key in ["channelEmotes", "sharedEmotes"] {
        if let Some(list) = data.get(key).and_then(|v| v.as_array()) {
            map.extend(parse_bttv_emote_list(list));
        }
    }
    Some(map)
}

fn parse_bttv_emote_list(list: &[serde_json::Value]) -> HashMap<String, String> {
    list.iter()
        .filter_map(|e| {
            let code = e.get("code")?.as_str()?.to_string();
            let id = e.get("id")?.as_str()?;
            Some((code, bttv_emote_url(id)))
        })
        .collect()
}

async fn fetch_ffz_global(http: &reqwest::Client) -> Option<HashMap<String, String>> {
    let data: serde_json::Value =
        http.get("https://api.frankerfacez.com/v1/set/global").send().await.ok()?.json().await.ok()?;
    Some(parse_ffz_sets(&data))
}

async fn fetch_ffz_channel(http: &reqwest::Client, channel_login: &str) -> Option<HashMap<String, String>> {
    let url = format!("https://api.frankerfacez.com/v1/room/{channel_login}");
    let resp = http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        // 404 is normal for channels with no FFZ customization.
        return None;
    }
    let data: serde_json::Value = resp.json().await.ok()?;
    Some(parse_ffz_sets(&data))
}

fn parse_ffz_sets(data: &serde_json::Value) -> HashMap<String, String> {
    let Some(sets) = data.get("sets").and_then(|v| v.as_object()) else { return HashMap::new() };
    let mut map = HashMap::new();
    for set in sets.values() {
        let Some(emoticons) = set.get("emoticons").and_then(|v| v.as_array()) else { continue };
        for emote in emoticons {
            let Some(name) = emote.get("name").and_then(|v| v.as_str()) else { continue };
            // FFZ gives a map of scale -> URL under "urls"; prefer the 2x
            // size if present, else whatever's available.
            let Some(urls) = emote.get("urls").and_then(|v| v.as_object()) else { continue };
            let url = urls.get("2").or_else(|| urls.get("1")).and_then(|v| v.as_str());
            if let Some(url) = url {
                let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_string() };
                map.insert(name.to_string(), url);
            }
        }
    }
    map
}

async fn fetch_twitch_global(http: &reqwest::Client, auth: &Arc<AuthClient>) -> Option<HashMap<String, String>> {
    let access_token = auth.get_valid_access_token().await.ok()?;
    let resp = http
        .get("https://api.twitch.tv/helix/chat/emotes/global")
        .bearer_auth(access_token)
        .header("Client-Id", auth.client_id().await)
        .send()
        .await
        .ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    Some(parse_twitch_emote_list(&data))
}

async fn fetch_twitch_channel(http: &reqwest::Client, auth: &Arc<AuthClient>, broadcaster_id: &str) -> Option<HashMap<String, String>> {
    let access_token = auth.get_valid_access_token().await.ok()?;
    let resp = http
        .get(format!("https://api.twitch.tv/helix/chat/emotes?broadcaster_id={broadcaster_id}"))
        .bearer_auth(access_token)
        .header("Client-Id", auth.client_id().await)
        .send()
        .await
        .ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    Some(parse_twitch_emote_list(&data))
}

fn parse_twitch_emote_list(data: &serde_json::Value) -> HashMap<String, String> {
    let Some(items) = data.get("data").and_then(|v| v.as_array()) else { return HashMap::new() };
    items
        .iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?.to_string();
            let url = e.get("images")?.get("url_2x").or_else(|| e.get("images")?.get("url_1x"))?.as_str()?.to_string();
            Some((name, url))
        })
        .collect()
}
