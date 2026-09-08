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

        if !tips.is_empty() {
            let mut history = self.history.lock().await;
            for tip in &tips {
                history.insert(0, tip.clone());
            }
            history.truncate(HISTORY_LIMIT);
            if let Err(err) = crate::state::save_json(&self.history_path, &*history) {
                tracing::error!("Failed to persist paypal-tips-history.json: {err}");
            }
        }

        for tip in tips {
            on_tip(tip);
        }
        Ok(())
    }
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
        Arc::new(PaypalWatcher { relay_url, relay_token, http: reqwest::Client::new(), history: Mutex::new(loaded), history_path });

    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(poll_interval_ms));
            loop {
                interval.tick().await;
                if let Err(err) = watcher.poll(&on_tip).await {
                    tracing::error!("PayPal relay poll failed: {err}");
                }
            }
        });
    }

    watcher
}
