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

/// What `load_token` should do when the token refresh call itself failed
/// (the endpoint was unreachable, DNS was down, Twitch 5xx'd).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshFailureAction {
    /// Hand twitch-irc the token we already hold. `expired` only affects
    /// the wording of the warning — a stale token is still preferable to
    /// an error, because an error is the one thing the crate retries
    /// without any delay.
    UseCachedToken { expired: bool },
    /// Nothing to fall back on — there has never been a token. Failing
    /// here is correct and cannot spin, because a bot that was never
    /// authorised has no working state to protect.
    Fail,
}

/// Extracted so the decision can be tested directly. `load_token` itself
/// is exercised only by the live path: it needs a real `AuthClient` with
/// a real `reqwest::Client`, and putting that behind a trait purely to
/// fake it would be a refactor wearing a test's clothes.
pub fn action_on_refresh_failure(cached: &TwitchTokens, now_ms: u64) -> RefreshFailureAction {
    if cached.access_token.is_empty() {
        return RefreshFailureAction::Fail;
    }
    RefreshFailureAction::UseCachedToken { expired: now_ms > cached.expires_at_ms() }
}

#[async_trait::async_trait]
impl TokenStorage for TwitchAuthStorage {
    type LoadError = AuthStorageError;
    type UpdateError = AuthStorageError;

    async fn load_token(&mut self) -> Result<UserAccessToken, Self::LoadError> {
        // Proactively refresh here too, so twitch-irc always starts with a
        // fresh token instead of immediately hitting an expiry mid-connect.
        //
        // A failure here used to propagate straight out as Err, and that
        // is what turned a network outage into 58 MB of log in one minute
        // on 2026-09-04. twitch-irc calls this on *every* connection
        // attempt with no cache of its own, and in the crate's
        // connection/event_loop.rs the credential error returns at :120 —
        // upstream of the rate-limit permit at :124 and the sleep at :146.
        // So this one error is the only failure mode the crate does not
        // throttle, and since an unreachable endpoint fails without a
        // round trip, it span at roughly 970 attempts a second.
        //
        // The precondition needs both halves: the endpoint unreachable
        // *and* the token inside get_valid_access_token's 60s expiry
        // margin. Outside that margin no network call happens at all,
        // which is why an outage does not always produce a flood.
        //
        // So a transient refresh failure no longer becomes an Err while
        // we still hold a token. Handing back the cached one — stale or
        // not — keeps the pool out of the unthrottled path entirely. If
        // the token really is dead, Twitch rejects the login on the
        // transport-connect path instead, and that path *does* take the
        // permit, so the worst case is one attempt per 2s rather than a
        // spin. Only a genuinely empty token (never authorised) still
        // fails fast, because there is nothing to fall back to.
        if let Err(err) = self.auth.get_valid_access_token().await {
            let tokens = self.auth.tokens.read().await;
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            match action_on_refresh_failure(&tokens, now_ms) {
                RefreshFailureAction::Fail => {
                    return Err(AuthStorageError(err.to_string()));
                }
                RefreshFailureAction::UseCachedToken { expired } => {
                    tracing::warn!(
                        "Twitch token refresh failed ({err}) — reusing the cached token \
                         ({}) rather than reporting a credential failure, which would \
                         retry unthrottled.",
                        if expired { "already expired" } else { "still valid" }
                    );
                }
            }
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(access_token: &str, obtained_ms: u64, expires_in: u64) -> TwitchTokens {
        TwitchTokens {
            access_token: access_token.to_string(),
            refresh_token: "refresh".to_string(),
            scope: Vec::new(),
            expires_in,
            obtainment_timestamp: obtained_ms,
        }
    }

    /// The 2026-09-04 case: the endpoint is unreachable while the token is
    /// inside the 60s refresh margin but has not actually expired. Before
    /// this, that returned Err and twitch-irc retried it ~970 times a
    /// second because credential errors bypass the crate's own throttle.
    #[test]
    fn a_still_valid_cached_token_is_reused_instead_of_erroring() {
        let cached = tokens("live-token", 0, 3_600);
        assert_eq!(
            action_on_refresh_failure(&cached, 3_500_000),
            RefreshFailureAction::UseCachedToken { expired: false },
        );
    }

    /// Ruling: hand back the stale token anyway. Twitch then rejects the
    /// login on the transport-connect path, which *does* take the rate
    /// limit permit, so an unthrottled spin becomes a 0.5 Hz retry.
    #[test]
    fn an_expired_cached_token_is_still_reused_rather_than_spinning() {
        let cached = tokens("stale-token", 0, 3_600);
        assert_eq!(
            action_on_refresh_failure(&cached, 3_600_001),
            RefreshFailureAction::UseCachedToken { expired: true },
        );
    }

    /// Exactly at expiry is not yet expired — the boundary belongs to the
    /// valid side, matching expires_at_ms.
    #[test]
    fn the_expiry_boundary_is_not_treated_as_expired() {
        let cached = tokens("edge-token", 0, 3_600);
        assert_eq!(
            action_on_refresh_failure(&cached, 3_600_000),
            RefreshFailureAction::UseCachedToken { expired: false },
        );
    }

    /// No token has ever been obtained, so there is nothing to fall back
    /// on and failing is correct. This cannot produce the storm: without
    /// credentials the bot has no working state to protect.
    #[test]
    fn an_empty_token_still_fails() {
        let cached = tokens("", 0, 3_600);
        assert_eq!(action_on_refresh_failure(&cached, 1_000), RefreshFailureAction::Fail);
    }
}
