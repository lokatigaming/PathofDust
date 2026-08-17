// Patreon integration — ports patreon.js. Polls for the campaign's member
// list on an interval and calls the new-patron callback for anyone not
// seen on a previous poll. First run seeds the baseline silently so
// existing patrons don't all fire alerts at once.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const TOKEN_URL: &str = "https://www.patreon.com/api/oauth2/token";
const API_BASE: &str = "https://www.patreon.com/api/oauth2/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatreonTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub obtainment_timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_url: Option<String>,
}

impl PatreonTokens {
    fn is_near_expiry(&self) -> bool {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let expires_at = self.obtainment_timestamp + self.expires_in * 1000;
        now_ms > expires_at.saturating_sub(60_000)
    }
}

#[derive(Debug, Clone)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub tier: String,
}

#[derive(Debug, Clone)]
pub struct NewPatron {
    pub name: String,
    pub tier: String,
    pub campaign_url: String,
}

struct PatreonClient {
    client_id: String,
    client_secret: String,
    tokens_path: PathBuf,
    tokens: RwLock<PatreonTokens>,
    http: reqwest::Client,
}

impl PatreonClient {
    async fn get_valid_access_token(&self) -> anyhow::Result<String> {
        {
            let tokens = self.tokens.read().await;
            if !tokens.is_near_expiry() {
                return Ok(tokens.access_token.clone());
            }
        }
        self.refresh().await
    }

    async fn refresh(&self) -> anyhow::Result<String> {
        let refresh_token = self.tokens.read().await.refresh_token.clone();

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token: String,
            refresh_token: String,
            expires_in: u64,
        }

        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Patreon token refresh failed: {body}");
        }

        let data: RefreshResponse = resp.json().await?;
        let mut tokens = self.tokens.write().await;
        tokens.access_token = data.access_token;
        tokens.refresh_token = data.refresh_token;
        tokens.expires_in = data.expires_in;
        tokens.obtainment_timestamp = chrono::Utc::now().timestamp_millis() as u64;

        crate::state::save_json(&self.tokens_path, &*tokens)?;
        Ok(tokens.access_token.clone())
    }

    async fn get(&self, path_or_url: &str) -> anyhow::Result<serde_json::Value> {
        let access_token = self.get_valid_access_token().await?;
        let url = if path_or_url.starts_with("http") {
            path_or_url.to_string()
        } else {
            format!("{API_BASE}{path_or_url}")
        };

        let resp = self.http.get(&url).bearer_auth(access_token).send().await?;
        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }

    async fn get_campaign_info(&self) -> anyhow::Result<(String, String)> {
        {
            let tokens = self.tokens.read().await;
            if let (Some(id), Some(url)) = (&tokens.campaign_id, &tokens.campaign_url) {
                return Ok((id.clone(), url.clone()));
            }
        }

        let data = self.get("/campaigns?fields[campaign]=url").await?;
        let campaign = data
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| anyhow::anyhow!("No Patreon campaign found for this account."))?;

        let id = campaign
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Campaign missing id"))?
            .to_string();
        let url = campaign
            .get("attributes")
            .and_then(|a| a.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        {
            let mut tokens = self.tokens.write().await;
            tokens.campaign_id = Some(id.clone());
            tokens.campaign_url = Some(url.clone());
            crate::state::save_json(&self.tokens_path, &*tokens)?;
        }

        Ok((id, url))
    }

    async fn fetch_all_members(&self, campaign_id: &str) -> anyhow::Result<Vec<Member>> {
        let mut members = Vec::new();
        let mut url = Some(format!(
            "/campaigns/{campaign_id}/members?include=currently_entitled_tiers&fields[member]=full_name,patron_status&fields[tier]=title&page[count]=200"
        ));

        while let Some(current_url) = url {
            let data = self.get(&current_url).await?;

            let tiers_by_id: std::collections::HashMap<String, String> = data
                .get("included")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("tier"))
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.to_string();
                    let title = item.get("attributes")?.get("title")?.as_str()?.to_string();
                    Some((id, title))
                })
                .collect();

            for member in data.get("data").and_then(|v| v.as_array()).into_iter().flatten() {
                let id = member.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let attrs = member.get("attributes");
                let name = attrs
                    .and_then(|a| a.get("full_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let tier_names: Vec<String> = member
                    .get("relationships")
                    .and_then(|r| r.get("currently_entitled_tiers"))
                    .and_then(|t| t.get("data"))
                    .and_then(|d| d.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.get("id")?.as_str())
                    .filter_map(|id| tiers_by_id.get(id).cloned())
                    .collect();

                let tier = if tier_names.is_empty() { "Free".to_string() } else { tier_names.join(", ") };

                members.push(Member { id, name, tier });
            }

            url = data
                .get("links")
                .and_then(|l| l.get("next"))
                .and_then(|v| v.as_str())
                .map(String::from);
        }

        Ok(members)
    }
}

/// All the shared state a poll needs, whether triggered by the interval
/// timer or by `force_poll` (the !checkpatreon command) — both call the
/// same `poll` function against this same struct, so there's exactly one
/// code path, not two that could drift apart.
pub struct PatreonWatcher {
    client: Arc<PatreonClient>,
    campaign_id: String,
    campaign_url: String,
    seen: RwLock<HashSet<String>>,
    seen_path: PathBuf,
    on_new_patron: Box<dyn Fn(NewPatron) + Send + Sync>,
}

impl PatreonWatcher {
    async fn poll(&self, announce: bool) -> anyhow::Result<()> {
        let members = self.client.fetch_all_members(&self.campaign_id).await?;

        if announce {
            let seen = self.seen.read().await;
            for member in &members {
                if !seen.contains(&member.id) {
                    (self.on_new_patron)(NewPatron {
                        name: member.name.clone(),
                        tier: member.tier.clone(),
                        campaign_url: self.campaign_url.clone(),
                    });
                }
            }
        }

        let new_seen: HashSet<String> = members.into_iter().map(|m| m.id).collect();
        let new_seen_vec: Vec<String> = new_seen.iter().cloned().collect();
        *self.seen.write().await = new_seen;
        crate::state::save_json(&self.seen_path, &new_seen_vec)?;

        Ok(())
    }

    /// Triggers an out-of-schedule check right now — used by the
    /// !checkpatreon mod command.
    pub async fn force_poll(&self) -> anyhow::Result<()> {
        self.poll(true).await
    }
}

pub async fn start_patreon_watcher(
    client_id: String,
    client_secret: String,
    tokens_path: PathBuf,
    seen_path: PathBuf,
    poll_interval_ms: u64,
    on_new_patron: impl Fn(NewPatron) + Send + Sync + 'static,
) -> anyhow::Result<Arc<PatreonWatcher>> {
    let tokens: PatreonTokens = crate::state::load_json(&tokens_path).ok_or_else(|| {
        anyhow::anyhow!(
            "No patreon-tokens.json found at {} — run `cargo run --bin auth_patreon` first.",
            tokens_path.display()
        )
    })?;

    let client = Arc::new(PatreonClient {
        client_id,
        client_secret,
        tokens_path,
        tokens: RwLock::new(tokens),
        http: reqwest::Client::new(),
    });

    let (campaign_id, campaign_url) = client.get_campaign_info().await?;

    let is_first_run = !seen_path.exists();
    let seen_vec: Vec<String> = crate::state::load_json(&seen_path).unwrap_or_default();

    let watcher = Arc::new(PatreonWatcher {
        client,
        campaign_id,
        campaign_url,
        seen: RwLock::new(seen_vec.into_iter().collect()),
        seen_path,
        on_new_patron: Box::new(on_new_patron),
    });

    if is_first_run {
        tracing::info!("Patreon: first run — recording current patrons as baseline, no alerts for existing patrons.");
        watcher.poll(false).await?;
    } else {
        watcher.poll(true).await?;
    }

    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(poll_interval_ms));
            interval.tick().await; // first tick fires immediately; we already polled above
            loop {
                interval.tick().await;
                if let Err(err) = watcher.poll(true).await {
                    tracing::error!("Patreon poll failed: {err}");
                }
            }
        });
    }

    Ok(watcher)
}
