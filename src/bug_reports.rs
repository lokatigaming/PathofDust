// Chat-submitted bug reports (!bugreport <message>) - persisted to disk so
// they can be reviewed/discussed in depth later, same small file-backed
// manager shape as EntranceThemeManager/PersonalPlaylistManager.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReport {
    pub id: u64,
    pub user: String,
    pub text: String,
    /// Unix seconds (wall-clock, not `Instant`, so it survives
    /// serialization and stays meaningful across restarts).
    pub at_unix_secs: u64,
}

/// One submission per user per this window - generous enough for someone
/// to actually write out a real report (not spam-tight like the 5s shared
/// builtin cooldown other commands share), tight enough that chat can't
/// flood the file.
pub(crate) const PER_USER_COOLDOWN: Duration = Duration::from_secs(60);

pub enum SubmitOutcome {
    Recorded { id: u64 },
    OnCooldown { remaining_secs: u64 },
}

pub struct BugReportManager {
    reports: Mutex<Vec<BugReport>>,
    path: PathBuf,
    user_cooldowns: Mutex<HashMap<String, Instant>>,
}

impl BugReportManager {
    pub fn new(path: PathBuf) -> Arc<Self> {
        let reports: Vec<BugReport> = crate::state::load_json(&path).unwrap_or_default();
        Arc::new(Self { reports: Mutex::new(reports), path, user_cooldowns: Mutex::new(HashMap::new()) })
    }

    pub async fn submit(&self, user: &str, text: &str) -> SubmitOutcome {
        let now = Instant::now();
        {
            let mut cooldowns = self.user_cooldowns.lock().await;
            if let Some(last) = cooldowns.get(user) {
                let elapsed = now.duration_since(*last);
                if elapsed < PER_USER_COOLDOWN {
                    return SubmitOutcome::OnCooldown { remaining_secs: (PER_USER_COOLDOWN - elapsed).as_secs() };
                }
            }
            cooldowns.insert(user.to_string(), now);
        }

        let mut reports = self.reports.lock().await;
        let id = reports.last().map(|r| r.id + 1).unwrap_or(1);
        let at_unix_secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        reports.push(BugReport { id, user: user.to_string(), text: text.to_string(), at_unix_secs });
        if let Err(err) = crate::state::save_json(&self.path, &*reports) {
            tracing::error!("Failed to save {}: {err}", self.path.display());
        }
        SubmitOutcome::Recorded { id }
    }

    /// Most recent first - what !bugreports (mod tool) shows in chat.
    pub async fn recent(&self, n: usize) -> Vec<BugReport> {
        let reports = self.reports.lock().await;
        reports.iter().rev().take(n).cloned().collect()
    }
}
