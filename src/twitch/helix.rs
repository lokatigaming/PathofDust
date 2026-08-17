// Small Helix API client — just what the bot actually needs (get_stream
// for !uptime). EventSub subscription creation lives in eventsub.rs since
// it's only ever used from there.

use std::sync::Arc;

use super::auth::AuthClient;

const HELIX_BASE: &str = "https://api.twitch.tv/helix";

#[derive(Clone)]
pub struct HelixClient {
    auth: Arc<AuthClient>,
    http: reqwest::Client,
}

pub struct StreamInfo {
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// One redemption Twitch is still holding as UNFULFILLED - what
/// `get_unfulfilled_redemptions` returns. Same shape a live EventSub
/// `channel.channel_points_custom_reward_redemption.add` event carries,
/// since the point is to feed these through the exact same handlers.
pub struct PendingRedemption {
    pub id: String,
    pub user_name: String,
    pub user_input: String,
}

impl HelixClient {
    pub fn new(auth: Arc<AuthClient>) -> Self {
        Self { auth, http: reqwest::Client::new() }
    }

    /// Resolves a channel login (e.g. from TWITCH_CHANNEL in .env) to its
    /// numeric user id, needed for EventSub conditions and Helix calls that
    /// key on broadcaster id rather than login name.
    pub async fn get_user_id_by_login(&self, login: &str) -> anyhow::Result<Option<String>> {
        let access_token = self.auth.get_valid_access_token().await?;
        let resp = self
            .http
            .get(format!("{HELIX_BASE}/users?login={login}"))
            .bearer_auth(access_token)
            .header("Client-Id", self.auth.client_id().await)
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        let id = data
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(id)
    }

    pub async fn get_stream(&self, broadcaster_id: &str) -> anyhow::Result<Option<StreamInfo>> {
        let access_token = self.auth.get_valid_access_token().await?;
        let resp = self
            .http
            .get(format!("{HELIX_BASE}/streams?user_id={broadcaster_id}"))
            .bearer_auth(access_token)
            .header("Client-Id", self.auth.client_id().await)
            .send()
            .await?;

        let data: serde_json::Value = resp.json().await?;
        let Some(stream) = data.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) else {
            return Ok(None);
        };

        let started_at = stream
            .get("started_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok_or_else(|| anyhow::anyhow!("Missing/invalid started_at in stream response"))?;

        Ok(Some(StreamInfo { started_at }))
    }

    /// Creates a channel points custom reward. Requires `channel:manage:redemptions`.
    /// Returns the new reward's id — only rewards created this way (via
    /// this app's own API access) can later have their redemptions
    /// fulfilled/refunded programmatically via `update_redemption_status`,
    /// which is why this exists instead of asking the streamer to create
    /// the reward by hand in the dashboard.
    pub async fn create_custom_reward(
        &self,
        broadcaster_id: &str,
        title: &str,
        cost: u32,
        prompt: &str,
        requires_input: bool,
    ) -> anyhow::Result<String> {
        let access_token = self.auth.get_valid_access_token().await?;
        let resp = self
            .http
            .post(format!("{HELIX_BASE}/channel_points/custom_rewards"))
            .bearer_auth(access_token)
            .header("Client-Id", self.auth.client_id().await)
            .query(&[("broadcaster_id", broadcaster_id)])
            .json(&serde_json::json!({
                "title": title,
                "cost": cost,
                "prompt": prompt,
                "is_user_input_required": requires_input,
                // Deliberately NOT skipping the request queue — a
                // redemption needs to start UNFULFILLED so the bot can
                // explicitly fulfill or cancel (refund) it once it knows
                // whether the submitted song actually resolved. Still
                // "instant" from the viewer's perspective since the bot
                // reacts within moments, just not via Twitch's own
                // auto-fulfill flag.
                "should_redemptions_skip_request_queue": false,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create custom reward \"{title}\": {body}");
        }

        let data: serde_json::Value = resp.json().await?;
        data.get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("Create custom reward response missing id: {data}"))
    }

    /// Marks a redemption FULFILLED or CANCELED — CANCELED automatically
    /// refunds the viewer's points. Only works for redemptions of a
    /// reward this app itself created (see `create_custom_reward`).
    pub async fn update_redemption_status(
        &self,
        broadcaster_id: &str,
        reward_id: &str,
        redemption_id: &str,
        status: &str,
    ) -> anyhow::Result<()> {
        let access_token = self.auth.get_valid_access_token().await?;
        let resp = self
            .http
            .patch(format!("{HELIX_BASE}/channel_points/custom_rewards/redemptions"))
            .bearer_auth(access_token)
            .header("Client-Id", self.auth.client_id().await)
            .query(&[("broadcaster_id", broadcaster_id), ("reward_id", reward_id), ("id", redemption_id)])
            .json(&serde_json::json!({ "status": status }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to update redemption {redemption_id} to {status}: {body}");
        }
        Ok(())
    }

    /// Every redemption of `reward_id` Twitch is STILL holding as
    /// UNFULFILLED - a real record survives on Twitch's own servers
    /// regardless of whether this bot was connected when it happened, so
    /// this is what a startup reconciliation pass (see main.rs) uses to
    /// catch up on anything redeemed while the bot was down, instead of
    /// those redemptions just sitting unprocessed forever. Paginated
    /// (25/page, Twitch's default) - follows the cursor until exhausted,
    /// capped at 10 pages (250 redemptions) as a sane worst-case backstop
    /// so a pathological queue can't loop forever.
    pub async fn get_unfulfilled_redemptions(&self, broadcaster_id: &str, reward_id: &str) -> anyhow::Result<Vec<PendingRedemption>> {
        let access_token = self.auth.get_valid_access_token().await?;
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let mut query = vec![("broadcaster_id", broadcaster_id), ("reward_id", reward_id), ("status", "UNFULFILLED")];
            if let Some(c) = cursor.as_deref() {
                query.push(("after", c));
            }
            let resp = self
                .http
                .get(format!("{HELIX_BASE}/channel_points/custom_rewards/redemptions"))
                .bearer_auth(&access_token)
                .header("Client-Id", self.auth.client_id().await)
                .query(&query)
                .send()
                .await?;
            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("Failed to list unfulfilled redemptions for reward {reward_id}: {body}");
            }
            let data: serde_json::Value = resp.json().await?;
            let items = data.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
            let page_len = items.len();
            for item in items {
                let id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let user_name = item.get("user_name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let user_input = item.get("user_input").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                if !id.is_empty() {
                    out.push(PendingRedemption { id, user_name, user_input });
                }
            }
            cursor = data.get("pagination").and_then(|p| p.get("cursor")).and_then(|v| v.as_str()).map(String::from);
            if cursor.is_none() || page_len == 0 {
                break;
            }
        }
        Ok(out)
    }
}
