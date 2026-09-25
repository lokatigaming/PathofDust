// PayPal tip alerts — the bot has no public address (it runs on a home
// PC), so PayPal can't call it directly the way StreamElements pushes tips
// over Socket.IO. Instead, a small Cloudflare Worker (see
// cloudflare-paypal-relay/worker.js, deployed separately) receives PayPal's
// webhook, verifies its signature, and queues the tip. This just polls
// that Worker's `/pending-tips` endpoint on an interval and drains
// whatever's waiting — same `Tip` shape (and alert type) as StreamElements
// tips, so both sources feed the exact same alert/chat-announcement path.

use crate::streamelements::Tip;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const HISTORY_LIMIT: usize = 20;

pub struct PaypalWatcher {
    relay_url: String,
    relay_token: String,
    http: reqwest::Client,
    history: Mutex<Vec<Tip>>,
    history_path: PathBuf,
}

impl PaypalWatcher {
    /// Most-recent-first, capped at `count`.
    pub async fn get_recent_tips(&self, count: usize) -> Vec<Tip> {
        let history = self.history.lock().await;
        history.iter().take(count).cloned().collect()
    }

    async fn poll(&self, on_tip: &(dyn Fn(Tip) + Send + Sync)) -> anyhow::Result<()> {
        let resp = self
            .http
            .get(format!("{}/pending-tips", self.relay_url.trim_end_matches('/')))
            .bearer_auth(&self.relay_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("PayPal relay returned {status}: {body}");
        }

        let tips: Vec<Tip> = resp.json().await?;

        let mut history = self.history.lock().await;
        let tips = unseen_tips(&history, tips);
        if !tips.is_empty() {
            for tip in &tips {
                history.insert(0, tip.clone());
            }
            history.truncate(HISTORY_LIMIT);
            if let Err(err) = crate::state::save_json(&self.history_path, &*history) {
                tracing::error!("Failed to persist paypal-tips-history.json: {err}");
            }
        }
        drop(history);

        for tip in tips {
            on_tip(tip);
        }
        Ok(())
    }
}

/// Drops any tip whose id is already in `history` (or earlier in the same
/// batch) — the relay dedupes lokati.net/tip's direct capture against
/// PayPal's backup webhook, and this is the last line behind it. Tips with
/// no id (older relay records) always pass.
fn unseen_tips(history: &[Tip], incoming: Vec<Tip>) -> Vec<Tip> {
    let mut seen: std::collections::HashSet<String> = history.iter().filter_map(|t| t.id.clone()).collect();
    incoming.into_iter().filter(|tip| tip.id.as_ref().is_none_or(|id| seen.insert(id.clone()))).collect()
}

pub fn start_paypal_watcher(
    relay_url: String,
    relay_token: String,
    poll_interval_ms: u64,
    history_path: PathBuf,
    on_tip: impl Fn(Tip) + Send + Sync + 'static,
) -> Arc<PaypalWatcher> {
    let loaded: Vec<Tip> = crate::state::load_json(&history_path).unwrap_or_default();

    let watcher =
        Arc::new(PaypalWatcher { relay_url, relay_token, http: reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("reqwest client build"), history: Mutex::new(loaded), history_path });

    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(poll_interval_ms));
            loop {
                interval.tick().await;
                if let Err(err) = watcher.poll(&on_tip).await {
                    tracing::error!("PayPal relay poll failed: {}", crate::redact::redact(&err.to_string()));
                }
            }
        });
    }

    watcher
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(id: Option<&str>, name: &str) -> Tip {
        Tip { id: id.map(str::to_string), name: name.to_string(), amount: 5.0, currency: "USD".to_string(), message: String::new() }
    }

    #[test]
    fn tip_deserializes_with_and_without_id() {
        let old: Tip = serde_json::from_str(r#"{"name":"A","amount":5.0,"currency":"USD","message":"hi"}"#).unwrap();
        assert_eq!(old.id, None);
        assert_eq!(old.name, "A");
        let new: Tip = serde_json::from_str(r#"{"id":"CAP1","name":"B","amount":2.5,"currency":"USD","message":""}"#).unwrap();
        assert_eq!(new.id.as_deref(), Some("CAP1"));
        let null_id: Tip = serde_json::from_str(r#"{"id":null,"name":"C","amount":1.0,"currency":"USD"}"#).unwrap();
        assert_eq!(null_id.id, None);
        // An id-less tip serializes to the pre-id shape, so saved history is unchanged.
        assert!(!serde_json::to_string(&old).unwrap().contains("\"id\""));
    }

    #[test]
    fn tip_whose_id_is_in_history_is_skipped() {
        let history = vec![tip(Some("CAP1"), "old"), tip(None, "legacy")];
        let incoming = vec![tip(Some("CAP1"), "dup"), tip(Some("CAP2"), "new"), tip(None, "no-id"), tip(Some("CAP2"), "dup-in-batch")];
        let names: Vec<String> = unseen_tips(&history, incoming).into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["new", "no-id"]);
    }
}
