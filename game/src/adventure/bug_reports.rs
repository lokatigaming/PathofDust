//! Player-submitted bug reports (2026-09-02).
//!
//! Ported from the bot's `src/bug_reports.rs`, which backed the chat
//! command `!bugreport <message>`. That command went away with Twitch, and
//! with it the only way a player could tell the owner something was
//! broken. This is the same store behind a web form instead of a chat
//! line: same `BugReport` shape, same file-backed manager, same
//! per-reporter cooldown.
//!
//! Deliberately no email, no external service, no new dependency. A report
//! is a JSON line in `adventure-bugreports.json`, written beside every
//! other piece of game state through `data_path`, and read back on an
//! operator-only page. Nothing here reaches the network.
//!
//! WHAT REPLACED THE CHAT GATE. On Twitch the submitter's identity came
//! from the chat message itself. Here it comes from `current_session`, so
//! the web form is login-only: it names the reporter without asking them
//! to type who they are, and being logged in IS the spam control, on top
//! of `PER_USER_COOLDOWN` below.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Where reports live, under `data_path` like the rest of the game's
/// state files.
pub const BUG_REPORTS_PATH: &str = "adventure-bugreports.json";

/// Longest report accepted. Long enough for a real description with
/// reproduction steps; short enough that the file cannot be used as
/// storage. Enforced server-side, not just as a `maxlength` on the
/// textarea - that attribute is a courtesy to the browser, not a limit.
pub const MAX_REPORT_LEN: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReport {
    pub id: u64,
    /// The reporter's login, straight from their session - never typed by
    /// the submitter, so it cannot be spoofed through the form.
    pub user: String,
    pub text: String,
    /// Unix seconds (wall-clock, not `Instant`, so it survives
    /// serialization and stays meaningful across restarts).
    pub at_unix_secs: u64,
}

/// One submission per reporter per this window - generous enough for
/// someone to actually write out a real report, tight enough that the
/// form cannot be used to flood the file. Carried over unchanged from the
/// chat command.
pub const PER_USER_COOLDOWN: Duration = Duration::from_secs(60);

pub enum SubmitOutcome {
    Recorded { id: u64 },
    OnCooldown { remaining_secs: u64 },
    Empty,
    TooLong { limit: usize },
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
        // Validated before the cooldown is spent: someone who submits an
        // empty textarea by accident should be able to fix it and send
        // straight away, not sit out a minute for a report that was never
        // recorded.
        let text = text.trim();
        if text.is_empty() {
            return SubmitOutcome::Empty;
        }
        if text.chars().count() > MAX_REPORT_LEN {
            return SubmitOutcome::TooLong { limit: MAX_REPORT_LEN };
        }

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

    /// Most recent first - the order `/admin/bugs` lists them in.
    pub async fn recent(&self, n: usize) -> Vec<BugReport> {
        let reports = self.reports.lock().await;
        reports.iter().rev().take(n).cloned().collect()
    }
}
