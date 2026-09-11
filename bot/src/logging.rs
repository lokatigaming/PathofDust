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
// THE FIX IS A PRUNER AFTER ALL, AND THIS PARAGRAPH USED TO SAY
// OTHERWISE (corrected 2026-09-11).
//
// It read: "THE FIX IS NOT A PRUNER. `tracing-appender` has grown the
// policy since this code was written: `RollingFileAppender::builder()`
// takes `max_log_files`, and `Inner::prune_old_logs` runs both at
// construction and on every rotation. So this is a configuration change
// to the sink that already exists, not a second mechanism bolted beside
// it." Every sentence of that is true, and the conclusion still did not
// hold, because it assumed the library's policy ranks by the right key.
// It does not: it ranks by btime and falls back to the filename only when
// btime is unreadable, so any state that ties creation timestamps - a
// `tar -xzf` restore above all - lets it delete the current day's log and
// keep a month-old one. Release 26 removed that pruner from the game for
// exactly this reason; this ports the same fix.
//
// Kept as a correction rather than a rewrite because the original
// reasoning was sound at the time and a future reader reaching for
// `max_log_files` should find out here why it was tried and dropped,
// rather than wondering why an obvious builder method went unused.
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

/// The `YYYY-MM-DD` a rotated log's own filename carries, as a sortable
/// `YYYYMMDD`, or `None` if this name is not one of ours to rank.
///
/// Deliberately hand-rolled rather than reaching for a date crate: the
/// only thing needed is an ORDER, the format is fixed by
/// `Rotation::DAILY`, and a parser that accepts exactly that shape is
/// also the filter that decides what may be deleted.
fn date_key_from_filename(name: &str) -> Option<u64> {
    let rest = name.strip_prefix(LOG_FILENAME_PREFIX)?.strip_prefix('.')?;
    let bytes = rest.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let num = |from: usize, to: usize| rest[from..to].parse::<u64>().ok();
    Some(num(0, 4)? * 10_000 + num(5, 7)? * 100 + num(8, 10)?)
}

/// Deletes the oldest rotated logs until `max_files - 1` remain, ranking
/// **by the date in the filename and by nothing else**.
///
/// **THE TWIN OF `game::logging::prune_by_filename_date`, AND THE TWO MUST
/// NOT DRIFT.** This is a deliberate copy rather than a shared module -
/// see the note at the bottom of this doc for why - so a fix to one is a
/// fix owed to the other.
///
/// WHY THIS EXISTS - see the corrected paragraph in this file's header.
/// The policy originally leaned on the library's own `max_log_files`,
/// which was sound reasoning that release 26 overtook. `Inner::prune_old_logs`
/// (`rolling.rs:689` in 0.2.5) ranks by
///
/// ```text
/// metadata.created().ok().or_else(|| parse_date_from_filename(..))
/// ```
///
/// so btime is the PRIMARY key and the filename is only the fallback. That
/// is the wrong primary key for a rotated log, because it records when the
/// bytes arrived on this filesystem rather than which day the log is of:
///
/// - **A RESTORE, which is the case that matters.** `tar -xzf` gives every
///   extracted `bot.log.*` the same btime, to the nanosecond; on Windows
///   any copy that resets creation time does the same. On the first start
///   afterwards the pruner sorts equal keys, the tie order is arbitrary,
///   and it can delete the CURRENT day's log while keeping a month-old
///   one.
/// - **tmpfs**, where files written in a tight loop also tie. That is how
///   this surfaced on the game side: the box's `/tmp` reproduced
///   deterministically what a restore does occasionally, and Windows -
///   which never ties on a fresh write - stayed green.
///
/// So btime is **removed**, not demoted to a tie-break. The filename
/// already carries the only fact the policy needs, it is written by the
/// same library that reads it, and it survives every copy, archive and
/// restore. **Do not promote btime back**: the test that catches you is
/// `identically_timestamped_logs_still_keep_today`, and the condition it
/// encodes is a restore, not a filesystem quirk.
///
/// **A prefixed file with no parseable date is never deleted.** The old
/// pruner would happily remove `bot.log.bak` - a hand-saved copy during an
/// incident is exactly the sort of thing that ends up in this directory.
/// If the rotation did not create it, it is not ours to collect.
///
/// `max_files - 1` rather than `max_files`, matching what it replaces: the
/// caller is about to open today's file as the nth, so one slot is
/// reserved and the steady-state directory holds exactly `max_files`.
///
/// **WHY A COPY AND NOT A SHARED MODULE.** The workspace manifest states
/// the property directly - *"The crates already share no dependency, no
/// import, no env key, no port and no file"* - and a common crate for
/// twenty lines would make this the first shared file purely to avoid
/// typing it twice. The constants are already deliberately separate for
/// the same reason (`MAX_LOG_FILES` above argues its own number, and the
/// game's argues a different case for the identical value), so a shared
/// pruner would have to take the prefix and the count as parameters and
/// would still leave both crates owning their own policy. The duplication
/// is the cheaper half of that trade; the comment is what makes it safe.
fn prune_by_filename_date(directory: &std::path::Path, max_files: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut dated: Vec<(u64, std::path::PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.metadata().ok()?.is_file() {
                return None;
            }
            let key = date_key_from_filename(entry.file_name().to_str()?)?;
            Some((key, entry.path()))
        })
        .collect();
    let keep = max_files.saturating_sub(1);
    if dated.len() <= keep {
        return;
    }
    // By date, then by path, so two files claiming the same day still
    // resolve to a stable order rather than to whatever `read_dir`
    // happened to yield.
    dated.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_, path) in dated.iter().take(dated.len() - keep) {
        if let Err(err) = std::fs::remove_file(path) {
            eprintln!("Failed to remove old log file {}: {err}", path.display());
        }
    }
}

/// The bot's daily-rolling file appender, retention included.
///
/// Fallible where `rolling::daily` was not: `build` validates the
/// directory. `main.rs` already returns `anyhow::Result`, so the `?` costs
/// nothing there and the failure is louder than a panic inside the logger
/// would be.
pub fn daily_appender(directory: impl AsRef<std::path::Path>) -> anyhow::Result<RollingFileAppender> {
    daily_appender_with_retention(directory, MAX_LOG_FILES)
}

/// `daily_appender` with the retention limit as a parameter, so the policy
/// can be exercised at a size a test can seed by hand.
///
/// **`max_log_files` is deliberately NOT set on the builder.** Setting it
/// would re-arm the library's btime ranking - and at construction it is
/// actively dangerous, because the library prunes when
/// `files.len() >= max_files` and reduces to `max_files - 1`, so against a
/// directory of tie-btime restored files the one it drops can be today's.
/// Retention is ours alone.
///
/// **THE ROTATION GAP IS WORSE HERE THAN ON THE GAME, and that is the
/// honest cost of this change.** Pruning now happens at CONSTRUCTION only.
/// The game runs under `Restart=always` with roughly daily deploys, so it
/// gets a fresh appender most days; **the bot has neither** - it runs when
/// the stream is on and restarts only when someone restarts it. An uptime
/// spanning more than `MAX_LOG_FILES` active days therefore prunes nothing
/// at all until the next start.
///
/// **`tracing-appender` 0.2.5 exposes no rotation hook to close it.**
/// `prune_old_logs` is a private method on the private `Inner`, and
/// `max_log_files` is the only retention knob on the public builder - the
/// one that carries the btime ranking. Checked, not assumed. So the bound
/// is RESTART-ONLY, which is still bounded where the previous behaviour
/// was unbounded, and closing the last of it needs either an upstream
/// change or our own rotation-aware writer.
pub(crate) fn daily_appender_with_retention(
    directory: impl AsRef<std::path::Path>,
    max_files: usize,
) -> anyhow::Result<RollingFileAppender> {
    prune_by_filename_date(directory.as_ref(), max_files);
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILENAME_PREFIX)
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

        let _appender = daily_appender_with_retention(&dir, 3).expect("the appender must build against a real directory");

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

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

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

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

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

#[cfg(test)]
mod restore_condition_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bot_log_restore_{}_{label}_{unique}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        dir
    }

    /// **THE RESTORE CONDITION, MADE PERMANENT.** Twin of the game's test
    /// of the same name; see `prune_by_filename_date` for why the two
    /// files carry the same policy separately.
    ///
    /// `tar -xzf` gives every extracted file the same creation timestamp,
    /// and on Windows - where this bot actually runs - any copy that
    /// resets creation time does the same. After that the whole log
    /// directory ties on btime, and a pruner ranking on btime is choosing
    /// arbitrarily between a month-old log and the current day's. This
    /// seeds that state deliberately: no sleeps, no ordering, one tight
    /// loop.
    ///
    /// **If this fails, someone has promoted btime back.** The filename
    /// carries the day; nothing else needs to.
    #[test]
    fn identically_timestamped_logs_still_keep_today() {
        let dir = scratch("tie");
        // NEWEST FIRST, so a pruner that fell back on directory order
        // rather than the filename would delete today's first.
        let today = format!("{LOG_FILENAME_PREFIX}.2026-09-11");
        std::fs::write(dir.join(&today), "the session under investigation\n").expect("today's log must be writable");
        for day in (1..=8).rev() {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            std::fs::write(dir.join(&name), format!("restored day {day}\n")).expect("seed log must be writable");
        }

        let _appender = daily_appender_with_retention(&dir, 3).expect("the appender must build against a real directory");

        // CONTENT, not existence, and the order matters: a pruner that
        // deletes today's log does not leave a hole, because the appender
        // opens the same filename immediately afterwards - so `exists()`
        // is true again a microsecond later and asserts almost nothing.
        // The bytes are the only witness.
        let body = std::fs::read_to_string(dir.join(&today)).expect("today's log must still be present after the prune");
        assert!(
            body.contains("the session under investigation"),
            "today's log was DELETED and reopened empty by a prune where every file shares a creation timestamp. That is the state a restore leaves behind, and this is the log an incident is reconstructed from"
        );
        assert!(dir.join(format!("{LOG_FILENAME_PREFIX}.2026-09-08")).exists(), "the newest seeded day must survive alongside today");
        for day in 1..=7 {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            assert!(!dir.join(&name).exists(), "{name} is older than the retention window and must be pruned");
        }
    }

    /// A prefixed file the rotation never created is not ours to delete.
    /// The library pruner would take `bot.log.bak`.
    #[test]
    fn a_prefixed_file_with_no_date_is_never_pruned() {
        let dir = scratch("undated");
        for day in 1..=6 {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            std::fs::write(dir.join(&name), "rotated\n").expect("seed log must be writable");
        }
        std::fs::write(dir.join("bot.log.bak"), "hand-saved during an incident\n").expect("foreign file must be writable");
        std::fs::write(dir.join("bot.log"), "no date at all\n").expect("foreign file must be writable");

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

        assert!(dir.join("bot.log.bak").exists(), "a prefixed file with no parseable date must survive - the rotation did not create it");
        assert!(dir.join("bot.log").exists(), "same for the bare prefix");
    }

    /// The ranking key itself, at the edges that decide what gets deleted.
    #[test]
    fn the_date_key_accepts_only_the_shape_rotation_writes() {
        assert_eq!(date_key_from_filename("bot.log.2026-09-11"), Some(20_260_911));
        assert_eq!(date_key_from_filename("bot.log.2026-01-01"), Some(20_260_101));
        assert!(
            date_key_from_filename("bot.log.2026-09-09") < date_key_from_filename("bot.log.2026-09-10"),
            "the key must order by day, which is the whole reason it exists"
        );
        assert!(
            date_key_from_filename("bot.log.2025-12-31") < date_key_from_filename("bot.log.2026-01-01"),
            "and across a year boundary, where a plain string compare of the day alone would invert"
        );
        for rejected in ["bot.log.bak", "bot.log", "bot.log.", "bot.log.2026-9-11", "bot.log.2026-09-1", "game.log.2026-09-11", "bot.log.20260911"] {
            assert_eq!(date_key_from_filename(rejected), None, "{rejected} must not be rankable, and therefore must not be deletable");
        }
    }
}
