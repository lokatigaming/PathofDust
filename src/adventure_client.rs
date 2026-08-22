// Bot-side HTTP client for the Stage 3 API seam (REFACTOR_PLAN.md §4) -
// the mirror image of game/src/adventure_web/api.rs's handlers. NOT
// wired into commands.rs's real dispatch or main.rs's redemption
// handlers yet - "build the seam alongside the existing in-process
// path... only the new one exercised by tests, no cutover until Stage
// 4" (the stage's own explicit scoping). Exists purely so
// tests/api_seam.rs can drive a real disposable `game` instance over
// genuine HTTP instead of re-deriving each endpoint's request/response
// shape inline in the test itself.
//
// Every method here mirrors one row of §4a's table 1:1 - same request
// fields, same "already-formatted reply string" response shape. None of
// them retry or time out specially; that policy (§4c: fixed fallback
// reply on game-down, fire-and-forget for activity XP, refund-on-down
// for redemptions) is the CALLER's job to apply once this is actually
// wired in at Stage 4, not this module's.

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

const API_SECRET_HEADER: &str = "x-adventure-api-secret";

#[derive(Clone)]
pub struct AdventureApiClient {
    http: reqwest::Client,
    /// e.g. "http://127.0.0.1:4005" - no trailing slash.
    base_url: String,
    shared_secret: String,
}

#[derive(Deserialize)]
struct ReplyBody {
    reply: Option<String>,
}

/// Mirrors `game`'s `RedemptionResponse` - the FULFILLED/CANCELED
/// Twitch-side status update is still the caller's job (needs `helix`,
/// a bot-only concern the game process has no access to).
#[derive(Deserialize, Debug)]
pub struct RedemptionResponse {
    pub fulfilled: bool,
    pub chat_message: Option<String>,
}

impl AdventureApiClient {
    pub fn new(base_url: impl Into<String>, shared_secret: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), base_url: base_url.into(), shared_secret: shared_secret.into() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn post_reply<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> anyhow::Result<Option<String>> {
        let resp = self.http.post(self.url(path)).header(API_SECRET_HEADER, &self.shared_secret).json(body).send().await?.error_for_status()?;
        Ok(resp.json::<ReplyBody>().await?.reply)
    }

    async fn get_reply(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<Option<String>> {
        let resp = self.http.get(self.url(path)).header(API_SECRET_HEADER, &self.shared_secret).query(query).send().await?.error_for_status()?;
        Ok(resp.json::<ReplyBody>().await?.reply)
    }

    pub async fn join(&self, user: &str) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/join", &serde_json::json!({ "user": user })).await
    }

    pub async fn character(&self, user: &str) -> anyhow::Result<Option<String>> {
        self.get_reply("/api/commands/character", &[("user", user)]).await
    }

    pub async fn party(&self) -> anyhow::Result<Option<String>> {
        self.get_reply("/api/commands/party", &[]).await
    }

    pub async fn next_encounter(&self, forced: Option<&str>) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/next_encounter", &serde_json::json!({ "forced": forced })).await
    }

    pub async fn event_intro(&self, args: &[String]) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/event_intro", &serde_json::json!({ "args": args })).await
    }

    pub async fn rampage(&self, user: &str, is_mod_or_broadcaster: bool) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/rampage", &serde_json::json!({ "user": user, "is_mod_or_broadcaster": is_mod_or_broadcaster })).await
    }

    pub async fn clear_battlefield(&self) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/clear_battlefield", &serde_json::json!({})).await
    }

    pub async fn give_loot(&self) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/give_loot", &serde_json::json!({})).await
    }

    pub async fn gift_dust(&self, target: &str, amount: u64) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/gift_dust", &serde_json::json!({ "target": target, "amount": amount })).await
    }

    pub async fn pin_fight(&self) -> anyhow::Result<Option<String>> {
        self.post_reply("/api/commands/pin_fight", &serde_json::json!({})).await
    }

    async fn post_redemption<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> anyhow::Result<RedemptionResponse> {
        let resp = self.http.post(self.url(path)).header(API_SECRET_HEADER, &self.shared_secret).json(body).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn redeem_reforge(&self, user_name: &str) -> anyhow::Result<RedemptionResponse> {
        self.post_redemption("/api/redemptions/reforge", &serde_json::json!({ "user_name": user_name })).await
    }

    pub async fn redeem_repair(&self, user_name: &str) -> anyhow::Result<RedemptionResponse> {
        self.post_redemption("/api/redemptions/repair", &serde_json::json!({ "user_name": user_name })).await
    }

    pub async fn redeem_force_boss(&self, user_name: &str, announce: bool) -> anyhow::Result<RedemptionResponse> {
        self.post_redemption("/api/redemptions/force_boss", &serde_json::json!({ "user_name": user_name, "announce": announce })).await
    }

    /// Fire-and-forget per §4c - a caller wiring this in for real must
    /// `tokio::spawn` the call rather than awaiting it inline in the chat
    /// message loop; this method itself still returns a real `Result`
    /// so logging a failure remains possible, it just must never block.
    pub async fn activity_xp(&self, username: &str) -> anyhow::Result<()> {
        self.http
            .post(self.url("/api/activity_xp"))
            .header(API_SECRET_HEADER, &self.shared_secret)
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// POST the bot's published-constants payload (see
    /// src/published_constants.rs for why this replaced the old direct
    /// file write). Any non-2xx comes back as `Err`, so the caller's
    /// bounded retry can tell "game down or too old" apart from success.
    /// No retry HERE, per this module's standing rule - retry/backoff
    /// policy is the caller's job.
    pub async fn publish_published_constants<T: Serialize + ?Sized>(&self, payload: &T) -> anyhow::Result<()> {
        self.http
            .post(self.url("/api/published-constants"))
            .header(API_SECRET_HEADER, &self.shared_secret)
            .json(payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// A minimal hand-rolled SSE client for `/api/announcements/stream` -
    /// no SSE-client crate pulled in (this whole module is test-only for
    /// now, see this file's own top doc, and the wire format is trivial:
    /// every event here is a single `data: <text>` line, never multi-field
    /// or multi-line). Reconnection/backoff (mentioned as the eventual
    /// bot-down/game-restart behavior in REFACTOR_PLAN.md §4b) is
    /// deliberately NOT this method's job - it yields one open
    /// connection's worth of messages and ends when that connection ends;
    /// a real caller wraps it in its own reconnect loop.
    pub async fn announcements(&self) -> anyhow::Result<impl Stream<Item = String>> {
        let resp = self.http.get(self.url("/api/announcements/stream")).header(API_SECRET_HEADER, &self.shared_secret).send().await?.error_for_status()?;
        let mut pending = String::new();
        let stream = resp
            .bytes_stream()
            .map(move |chunk| {
                let mut out = Vec::new();
                if let Ok(bytes) = chunk {
                    pending.push_str(&String::from_utf8_lossy(&bytes));
                    // SSE frames are separated by a blank line; drain
                    // every COMPLETE frame out of `pending`, leaving any
                    // trailing partial frame for the next chunk.
                    while let Some(frame_end) = pending.find("\n\n") {
                        let frame = pending[..frame_end].to_string();
                        pending.drain(..frame_end + 2);
                        for line in frame.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                out.push(data.to_string());
                            }
                        }
                    }
                }
                futures_util::stream::iter(out)
            })
            .flatten();
        Ok(stream)
    }
}
