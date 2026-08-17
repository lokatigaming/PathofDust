// Twitch OAuth token storage + refresh. Persists to tokens.json in the same
// shape get-token.js / the Node bot's RefreshingAuthProvider already wrote
// (camelCase accessToken/refreshToken/scope/expiresIn/obtainmentTimestamp),
// so an existing tokens.json from the Node bot can be copied over directly
// — no need to re-run the OAuth flow when migrating.
//
// twitch-irc (the chat crate) does its own refresh cycle internally via the
// `TwitchAuthStorage` adapter below; Helix calls elsewhere in this project
// go through `AuthClient::get_valid_access_token` directly. Both read from
// and write back to the same shared `AuthClient`, so whichever side
// refreshes first keeps the other in sync.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use twitch_irc::login::{TokenStorage, UserAccessToken};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchTokens {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub scope: Vec<String>,
    pub expires_in: u64,
    pub obtainment_timestamp: u64,
}

impl TwitchTokens {
    fn expires_at_ms(&self) -> u64 {
        self.obtainment_timestamp + self.expires_in * 1000
    }

    fn is_near_expiry(&self) -> bool {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        now_ms > self.expires_at_ms().saturating_sub(60_000)
    }
}

#[derive(Debug)]
pub struct AuthClient {
    client_id: String,
    client_secret: String,
    tokens_path: PathBuf,
    tokens: RwLock<TwitchTokens>,
    http: reqwest::Client,
}

impl AuthClient {
    pub fn new(client_id: String, client_secret: String, tokens_path: PathBuf) -> anyhow::Result<Arc<Self>> {
        let tokens: TwitchTokens = crate::state::load_json(&tokens_path).ok_or_else(|| {
            anyhow::anyhow!(
                "No tokens.json found at {} — run `cargo run --bin auth` first.",
                tokens_path.display()
            )
        })?;

        Ok(Arc::new(Self {
            client_id,
            client_secret,
            tokens_path,
            tokens: RwLock::new(tokens),
            http: reqwest::Client::new(),
        }))
    }

    /// Returns a currently-valid access token, refreshing first if it's
    /// within 60 seconds of expiring (matches the Node bot's own margin).
    pub async fn get_valid_access_token(&self) -> anyhow::Result<String> {
        {
            let tokens = self.tokens.read().await;
            if !tokens.is_near_expiry() {
                return Ok(tokens.access_token.clone());
            }
        }
        self.refresh().await
    }

    pub async fn client_id(&self) -> String {
        self.client_id.clone()
    }

    async fn refresh(&self) -> anyhow::Result<String> {
        let refresh_token = self.tokens.read().await.refresh_token.clone();

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token: String,
            refresh_token: String,
            #[serde(default)]
            scope: Vec<String>,
            expires_in: u64,
        }

        let resp = self
            .http
            .post("https://id.twitch.tv/oauth2/token")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Twitch token refresh failed: {body}");
        }

        let data: RefreshResponse = resp.json().await?;
        let updated = TwitchTokens {
            access_token: data.access_token,
            refresh_token: data.refresh_token,
            scope: data.scope,
            expires_in: data.expires_in,
            obtainment_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };

        crate::state::save_json(&self.tokens_path, &updated)?;
        let access_token = updated.access_token.clone();
        *self.tokens.write().await = updated;

        Ok(access_token)
    }

    async fn set_from_user_access_token(&self, token: &UserAccessToken) {
        let expires_in = token
            .expires_at
            .map(|exp| (exp - chrono::Utc::now()).num_seconds().max(0) as u64)
            .unwrap_or(0);

        let updated = TwitchTokens {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            scope: self.tokens.read().await.scope.clone(),
            expires_in,
            obtainment_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };

        if let Err(err) = crate::state::save_json(&self.tokens_path, &updated) {
            tracing::error!("Failed to persist refreshed Twitch tokens: {err}");
        }
        *self.tokens.write().await = updated;
    }
}

/// Adapter so twitch-irc's `RefreshingLoginCredentials` can share the same
/// underlying token state as the rest of the bot.
#[derive(Debug, Clone)]
pub struct TwitchAuthStorage {
    pub auth: Arc<AuthClient>,
}

#[derive(Debug, thiserror::Error)]
#[error("twitch auth storage error: {0}")]
pub struct AuthStorageError(String);

#[async_trait::async_trait]
impl TokenStorage for TwitchAuthStorage {
    type LoadError = AuthStorageError;
    type UpdateError = AuthStorageError;

    async fn load_token(&mut self) -> Result<UserAccessToken, Self::LoadError> {
        // Proactively refresh here too, so twitch-irc always starts with a
        // fresh token instead of immediately hitting an expiry mid-connect.
        self.auth
            .get_valid_access_token()
            .await
            .map_err(|e| AuthStorageError(e.to_string()))?;

        let tokens = self.auth.tokens.read().await;
        Ok(UserAccessToken {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            created_at: chrono::DateTime::from_timestamp_millis(tokens.obtainment_timestamp as i64)
                .unwrap_or_else(chrono::Utc::now),
            expires_at: Some(
                chrono::DateTime::from_timestamp_millis(tokens.expires_at_ms() as i64)
                    .unwrap_or_else(chrono::Utc::now),
            ),
        })
    }

    async fn update_token(&mut self, token: &UserAccessToken) -> Result<(), Self::UpdateError> {
        self.auth.set_from_user_access_token(token).await;
        Ok(())
    }
}
