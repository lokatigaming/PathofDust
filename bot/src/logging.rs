// The bot's file log sink, with the retention policy it spent its whole
// life without (2026-09-08).
//
// THE DEFECT THIS CLOSES. `main.rs` built its appender with
// `tracing_appender::rolling::daily("logs", "bot.log")`, which rotates
// daily and **deletes nothing, ever**. `logs/` reached several GB once
// already; the sink was disabled outright on 2026-08-17 and re-enabled
// after a ONE-TIME MANUAL CLEANUP, which is not a policy - it is the same
// incident waiting on the same interval.
//
// THE MITIGATION THAT NEVER ARRIVED. This was expected to resolve "at the
// Linux move where journald owns rotation". The game moved and the bot did
// not, so it never arrived here - but the deeper problem is that it would
// not have worked anyway. **journald owns a process's STDOUT, not a file
// appender that opens its own files.** `game/src/main.rs:86` builds the
// identical unpruned appender and is on Linux TODAY, writing to
// `data_path("logs")` under the systemd unit, pruned by nothing. Rotation
// by the supervisor was never going to cover a sink that writes around it.
//
// THE FIX IS NOT A PRUNER. `tracing-appender` has grown the policy since
// this code was written: `RollingFileAppender::builder()` takes
// `max_log_files`, and `Inner::prune_old_logs` runs both at construction
// and on every rotation. So this is a configuration change to the sink
// that already exists, not a second mechanism bolted beside it - which
// means it is platform-independent, needs no scheduled task, and travels
// with the crate rather than with a path.
//
// FILENAMES ARE UNCHANGED, WHICH MATTERS BECAUSE PEOPLE GREP THEM.
// `rolling::daily(dir, prefix)` is `RollingFileAppender::new(DAILY, dir,
// prefix)`, and `Inner::join_date` formats `(DAILY, Some(prefix), None)`
// as `"{prefix}.{date}"`. The builder below sets exactly that rotation and
// prefix and no suffix, so it produces `bot.log.YYYY-MM-DD` byte for byte,
// the same names the existing files on disk already carry. Existing logs
// are adopted by the policy rather than orphaned beside it.

use tracing_appender::rolling::{RollingFileAppender, Rotation};

/// The log file's name prefix. Rotation appends `.YYYY-MM-DD`.
pub const LOG_FILENAME_PREFIX: &str = "bot.log";

/// How many daily log files to keep.
///
/// THE NUMBER, ARGUED RATHER THAN PICKED.
///
/// **30, to match the daily tier of `backup-bot-data.ps1` and
/// `backup-game-data.ps1`.** One retention horizon across every thing this
/// project keeps is worth more than a separately-optimal second one: an
/// operator who knows "we keep a month" does not have to remember which
/// month applies to which artifact.
///
/// **This bounds a COUNT, and the count is per ACTIVE day, not per
/// calendar day.** `rolling::daily` creates a file only when something is
/// actually logged, so a bot that runs three days a week reaches 30 files
/// in roughly ten calendar weeks, while one that runs daily reaches it in
/// a month. That is the right direction on both ends - a low-activity
/// install keeps a longer window because its days are scarcer, and a
/// heavy one is bounded sooner because its days are denser. A
/// calendar-age rule would have given the quiet install less history for
/// the same disk.
///
/// **What this does NOT claim.** It is a bound on files, not on bytes, and
/// no session has measured a real day's log volume - the live box is not
/// this window's to read. So the honest statement of what changes is:
/// growth was previously UNBOUNDED and is now bounded, which is the actual
/// defect. If a day's volume ever turns out to make 30 files too large,
/// this constant is the one thing to change and nothing else moves.
pub const MAX_LOG_FILES: usize = 30;

/// The bot's daily-rolling file appender, retention included.
///
/// Fallible where `rolling::daily` was not: `build` validates the
/// directory. `main.rs` already returns `anyhow::Result`, so the `?` costs
/// nothing there and the failure is louder than a panic inside the logger
/// would be.
pub fn daily_appender(directory: impl AsRef<std::path::Path>) -> anyhow::Result<RollingFileAppender> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILENAME_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(directory)
        .map_err(|err| anyhow::anyhow!("could not open the rolling log appender: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Its own directory per test, same shape as `manager.rs`'s
    /// `disposable_manager` helpers - a shared scratch directory would let
    /// one test's pruning delete another's fixtures.
    fn scratch(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bot_log_retention_{}_{label}_{unique}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        dir
    }

    /// Writes `count` synthetic already-rotated logs, oldest first.
    ///
    /// Created in ascending date order ON PURPOSE. `prune_old_logs` sorts
    /// by the filesystem's creation timestamp, falling back to the date in
    /// the name only when the metadata is unreadable - and on this
    /// platform it is readable - so the creation ORDER is what the pruner
    /// actually sees, and writing them in the wrong order would test a
    /// ranking that never occurs in production.
    fn seed_aged_logs(dir: &std::path::Path, count: u32) -> Vec<String> {
        let mut names = Vec::new();
        for day in 1..=count {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-08-{day:02}");
            std::fs::write(dir.join(&name), format!("synthetic day {day}\n")).expect("seed log must be writable");
            names.push(name);
        }
        names
    }

    fn present(dir: &std::path::Path, name: &str) -> bool {
        dir.join(name).exists()
    }

    /// THE ASSERTION THE WHOLE MODULE EXISTS FOR: old files actually go.
    ///
    /// Deliberately does not use `MAX_LOG_FILES` - seeding 30+ files to
    /// test the shipped constant would prove the same thing slower, and
    /// would silently stop testing anything if someone lowered the
    /// constant below the seed count.
    #[test]
    fn constructing_the_appender_prunes_down_to_the_retention_limit() {
        let dir = scratch("prunes");
        let seeded = seed_aged_logs(&dir, 6);

        let _appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(LOG_FILENAME_PREFIX)
            .max_log_files(3)
            .build(&dir)
            .expect("the appender must build against a real directory");

        // (n - 1) survive, because the appender is about to open today's
        // file as the nth - see `prune_old_logs`'s own comment.
        assert!(!present(&dir, &seeded[0]), "the oldest seeded log must be pruned");
        assert!(!present(&dir, &seeded[1]), "the second-oldest seeded log must be pruned");
        assert!(!present(&dir, &seeded[2]), "the third-oldest seeded log must be pruned");
        assert!(!present(&dir, &seeded[3]), "the fourth-oldest seeded log must be pruned");
        assert!(present(&dir, &seeded[4]), "the two newest seeded logs must survive");
        assert!(present(&dir, &seeded[5]), "the two newest seeded logs must survive");
    }

    /// The bot restarts many times a day - the watchdog exists to make
    /// that happen - and every restart constructs a fresh appender, which
    /// prunes. If pruning could take the file the running process is
    /// about to append to, the restart after a crash would destroy the
    /// evidence of the crash: the exact log anyone would go looking for.
    ///
    /// It cannot, and the reason is a property of the library rather than
    /// of this call site. `prune_old_logs` sorts ascending by creation
    /// time and deletes from the FRONT, so the newest `max_files - 1`
    /// always survive - and today's file, which is either the newest that
    /// exists or does not exist yet, is never in the deleted prefix.
    #[test]
    fn todays_log_survives_a_restart_that_prunes() {
        let dir = scratch("today");
        seed_aged_logs(&dir, 4);

        // Created last, so it is the newest by creation time - exactly
        // what a mid-day restart finds.
        let today = format!("{LOG_FILENAME_PREFIX}.2026-09-08");
        std::fs::write(dir.join(&today), "this is the running day's log\n").expect("today's log must be writable");

        let _appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(LOG_FILENAME_PREFIX)
            .max_log_files(2)
            .build(&dir)
            .expect("the appender must build against a real directory");

        assert!(present(&dir, &today), "the day currently being written must never be pruned");
        let body = std::fs::read_to_string(dir.join(&today)).expect("today's log must still be readable");
        assert!(body.contains("this is the running day's log"), "today's log must be appended to, not truncated");
    }

    /// The pruner filters on the filename prefix, so it can only ever
    /// delete its own logs. Worth pinning rather than trusting: the log
    /// directory is a plausible place for someone to drop a hand-saved
    /// crash dump, and a pruner that ate it would do so silently.
    #[test]
    fn nothing_outside_the_log_filename_prefix_is_ever_deleted() {
        let dir = scratch("foreign");
        seed_aged_logs(&dir, 5);
        std::fs::write(dir.join("crash-dump-keep-me.txt"), "not a log\n").expect("foreign file must be writable");
        std::fs::write(dir.join("notes.md"), "not a log either\n").expect("foreign file must be writable");

        let _appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(LOG_FILENAME_PREFIX)
            .max_log_files(2)
            .build(&dir)
            .expect("the appender must build against a real directory");

        assert!(present(&dir, "crash-dump-keep-me.txt"), "a non-log file in the log directory must survive pruning");
        assert!(present(&dir, "notes.md"), "a non-log file in the log directory must survive pruning");
    }

    /// The filenames must not move. Operators grep `bot.log.<date>` and
    /// the files already on disk carry that shape, so a builder that
    /// produced anything else would orphan every existing log beside a
    /// policy that no longer recognises it - and `prune_old_logs`'s own
    /// prefix filter would then never delete them either, which is the
    /// failure that looks like success.
    #[test]
    fn the_configured_appender_writes_the_same_filename_the_old_call_did() {
        use std::io::Write;

        let dir = scratch("names");
        let mut appender = daily_appender(&dir).expect("the shipped configuration must build");
        appender.write_all(b"a line\n").expect("the appender must accept a write");
        appender.flush().expect("the appender must flush");

        let written: Vec<String> = std::fs::read_dir(&dir)
            .expect("scratch dir must be readable")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(written.len(), 1, "exactly one log file should exist, found {written:?}");
        let name = &written[0];
        assert!(name.starts_with("bot.log."), "the filename must still be `bot.log.<date>`, got {name}");
        let date = &name["bot.log.".len()..];
        assert_eq!(date.len(), 10, "the suffix must be a YYYY-MM-DD date, got {date}");
        assert_eq!(date.matches('-').count(), 2, "the suffix must be a YYYY-MM-DD date, got {date}");
    }
}
