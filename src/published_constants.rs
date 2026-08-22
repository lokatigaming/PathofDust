// Bot → game published constants over HTTP (2026-08-22, bot/game
// build-time decoupling) - replaces this repo's LAST bot→game file
// write. Until now the bot saved `bot-published-constants.json` straight
// into the game's working directory at startup (see main.rs git history),
// which is also why every game-only release forced a bot rebuild: any
// change under game/** invalidated the binary this file was built into.
// Now the five compile-time constants are POSTed through the existing
// shared-secret /api/* seam instead, and the GAME writes its own copy of
// the same file for wiki.rs to read - identical path (the bare
// CWD-relative PUBLISHED_CONSTANTS_PATH), identical pretty-printed shape,
// so a pre-decoupling bot (still writing the file directly) stays
// compatible with a new game, and a new bot pointed at an old game just
// burns its retries and leaves the wiki rendering "varies" until both
// sides are current.
//
// The payload struct mirrors the wire format field-for-field and lives
// HERE rather than being imported from the game crate, so the S5 step of
// the decoupling plan can drop the build-time dependency without ever
// touching main.rs again.

use std::time::Duration;

use serde::Serialize;

use crate::adventure_client::AdventureApiClient;
use crate::bug_reports;
use crate::commands;
use crate::song_requests;

#[derive(Serialize)]
struct PublishedConstants {
    builtin_cooldown_secs: u64,
    bug_report_cooldown_secs: u64,
    song_skip_cooldown_secs: u64,
    min_vote_volume: u8,
    max_vote_volume: u8,
}

/// Called once per startup, after the AdventureApiClient exists (it needs
/// config.adventure_api_base_url/secret, which the old top-of-main write
/// predated - publishing a few seconds later only means the wiki's
/// placeholders read "varies" slightly longer on a cold start, same
/// fallback as when the file was never written).
///
/// Bounded retry, then give up quietly: a down or old game must never
/// block or fail bot startup over this. Three attempts with a short
/// backoff covers a game mid-restart (§13 brings the game up before the
/// bot anyway); anything longer wants human attention, which the final
/// warn-level line provides without pretending to be an error the bot
/// could recover from on its own.
pub async fn publish_to_game(adventure: &AdventureApiClient) {
    let payload = PublishedConstants {
        builtin_cooldown_secs: commands::BUILTIN_COOLDOWN.as_secs(),
        bug_report_cooldown_secs: bug_reports::PER_USER_COOLDOWN.as_secs(),
        song_skip_cooldown_secs: song_requests::SKIP_ACTION_COOLDOWN.as_secs(),
        min_vote_volume: song_requests::MIN_VOTE_VOLUME,
        max_vote_volume: song_requests::MAX_VOTE_VOLUME,
    };

    const ATTEMPTS: usize = 3;
    for attempt in 1..=ATTEMPTS {
        match adventure.publish_published_constants(&payload).await {
            Ok(()) => {
                tracing::info!(attempt, "published bot-side cooldown/volume constants to the game");
                return;
            }
            Err(err) => tracing::warn!(attempt, error = %err, "publishing bot constants to the game failed"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    tracing::warn!(
        "gave up publishing bot constants to the game after {ATTEMPTS} attempts - \
         the wiki's chat-cooldown/vote-volume placeholders will render \"varies\" \
         until a successful startup publishes them"
    );
}