// The game's file log sink, with the retention policy it never had
// (2026-09-08). Sibling of `bot/src/logging.rs`, which landed first and
// is the template; only the differences are argued here.
//
// THE DEFECT. `main.rs` built its appender with
// `tracing_appender::rolling::daily(&logs_dir, "game.log")`, which
// rotates daily and **deletes nothing, ever** - that constructor takes no
// retention parameter at all. This has been running on the production
// Linux box since release 16, growing one file per day forever.
//
// JOURNALD DOES NOT COVER IT, AND THE UNIT FILE ALREADY SAID SO.
// `docs/linux_staging.md:137-140`, in the service definition's own
// comment:
//
//   > journald is the log of record. The binary ALSO writes logs/game.log
//   > via its tracing_appender layer (main.rs) - that is in the code, not
//   > configurable here, and lands under WorkingDirectory like every
//   > other data file.
//
// So the separation was written down at the moment the unit was authored
// and nobody carried it forward to "therefore nothing prunes it".
// journald owns STDOUT; this appender opens its own files and writes
// around the supervisor entirely.
//
// WHERE IT ACTUALLY LANDS ON THE LIVE BOX, since a policy pointed at the
// wrong directory prunes nothing and looks fine:
//
//   * the unit sets `WorkingDirectory=/var/lib/pathofdust`
//     (docs/linux_staging.md:126)
//   * it sets exactly three `Environment=` lines - `OPERATOR_LOGIN`,
//     `ADVENTURE_WEB_PORT`, `ADVENTURE_OVERLAY_SERVER_PORT` - and
//     **`GAME_DATA_DIR` is not among them** (:155-157, and :48 says so in
//     words)
//   * `data_path` therefore joins onto its EMPTY default base, so
//     `data_path("logs")` is the bare relative path `logs`
//
// => **`/var/lib/pathofdust/logs/game.log.<YYYY-MM-DD>`**
//
// Cross-checked independently of that arithmetic: the unit runs with
// `ProtectSystem=strict` and `ReadWritePaths=/var/lib/pathofdust`, so if
// the logs resolved anywhere else the process could not have written them
// at all. The files existing is itself evidence of the path.
//
// NOT COVERED BY THE BACKUP EITHER, in the safe direction:
// `backup-game-data.sh` stages an explicit `CORE_FILES` allow-list, so
// logs are neither backed up nor inflating the archives. They only grow.
// Note also that `docs/linux_deploy.md:176`'s "`/var/lib/pathofdust` went
// 40 MB -> 7.0 GB and then **plateaued**" is a statement about the FIGHT
// TIERS, which prune themselves (`fight_storage.rs` capacities). Logs sit
// in the same directory underneath that plateau and do not.

use tracing_appender::rolling::{RollingFileAppender, Rotation};

/// The log file's name prefix. Rotation appends `.YYYY-MM-DD`.
pub const LOG_FILENAME_PREFIX: &str = "game.log";

/// How many daily log files to keep.
///
/// **ITS OWN CONSTANT, NOT THE BOT'S.** Deliberately a separate number
/// that happens to equal `twitch_bot_rs::logging::MAX_LOG_FILES` rather
/// than a shared one: the two processes have different log profiles and
/// different uptime, so a future measurement must be able to move one
/// without moving the other. Sharing it would make the next person choose
/// between changing both and changing neither.
///
/// THE NUMBER, ARGUED FOR THIS PROCESS RATHER THAN INHERITED.
///
/// **The premise that the game is busier is true of requests and false of
/// logging.** Counting emission sites across each crate:
///
/// | | `info!` | `warn!` | `error!` | total |
/// |---|---|---|---|---|
/// | game | 18 | 19 | 58 | 95 |
/// | bot | 49 | 43 | 35 | 128 |
///
/// and site counts understate the gap, because what matters is which
/// sites sit in a hot path. **The game has none.** Every `info!` in the
/// crate is startup (`"loaded N characters"`, the two server-started
/// lines), a one-time migration, a balance-file override, or a rare
/// operator/player action (`!pinfight`, a Unique Shard apply, a login).
/// There is no per-fight and no per-request logging at all, and the
/// crate's dominant category is `error!`, which only fires when something
/// is already wrong. A healthy day is small, and an unhealthy day is
/// exactly the one worth keeping.
///
/// **The wall-clock window is SHORTER here than on the bot for the same
/// number, and that is the right direction.** `rolling::daily` creates a
/// file only on a day something is logged. The bot runs when the stream
/// is on, so its 30 files span more than 30 calendar days; the game runs
/// continuously under `Restart=always`, so its 30 files are 30 days
/// almost exactly. The busier, always-on process gets the tighter bound
/// from the identical constant.
///
/// **30 rather than fewer**, because the game's diagnostic unit is the
/// RELEASE and `c` deploys roughly daily: a month of files is a month of
/// releases, which matches how far back the anomaly ledger actually
/// cites. Fewer would put the common "this started a few releases ago"
/// question outside the window.
///
/// **What this does not claim.** It bounds FILES, not BYTES, and no
/// session has measured a real day's volume on the box - the emission
/// sites above are counted from source, not sampled from production. The
/// honest statement is that growth was UNBOUNDED and is now BOUNDED,
/// which is the defect. This constant is the one thing to change if a
/// day's volume ever proves the estimate wrong.
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
/// WHY THIS EXISTS AT ALL - the pruner it replaces is not ours.
/// `tracing_appender`'s own `max_log_files` ranks by
/// `metadata.created()` (btime) and only falls back to the filename date
/// when that metadata is unreadable (`rolling.rs:689` in 0.2.5). **btime
/// is the wrong primary key for a rotated log**, because it records when
/// the bytes arrived on THIS filesystem rather than which day the log is
/// of, and there are two ways for it to be wrong at once:
///
/// - **A RESTORE, which is the case that matters.** `tar -xzf` gives every
///   extracted `game.log.*` the same btime, to the nanosecond. On the
///   first start afterwards the pruner sorts equal keys, the tie order is
///   arbitrary, and it can delete the CURRENT day's log while keeping a
///   month-old one. That is the log a rollback decision is read from.
/// - **tmpfs**, where six files written in a tight loop also tie. That is
///   how this surfaced: the box's `/tmp` reproduced deterministically what
///   a restore does occasionally, and Windows - which never ties - stayed
///   green.
///
/// So btime is not demoted to a tie-break here, it is **removed**. The
/// filename already carries the only fact the policy needs, it is written
/// by the same library that reads it, and it survives every copy, archive
/// and restore. **Do not promote btime back**: the test that would catch
/// you is `identically_timestamped_logs_still_keep_today` and the
/// condition it encodes is a restore, not a filesystem quirk.
///
/// **A prefixed file with no parseable date is never deleted.** The old
/// pruner would happily remove `game.log.bak` - a hand-saved copy during
/// an incident is exactly the sort of thing that ends up in this
/// directory. If the rotation did not create it, it is not ours to
/// collect.
///
/// `max_files - 1` rather than `max_files`, matching what it replaces:
/// the caller is about to open today's file as the nth, so one slot is
/// reserved and the steady-state directory holds exactly `max_files`.
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

/// The game's daily-rolling file appender, retention included.
///
/// Takes the directory rather than resolving `data_path("logs")` itself,
/// so the retention policy is testable against a scratch directory
/// without touching the process-global `DATA_DIR` `OnceLock` - which
/// `paths.rs` documents as untestable in this crate's shared test binary
/// anyway.
pub fn daily_appender(directory: impl AsRef<std::path::Path>) -> anyhow::Result<RollingFileAppender> {
    daily_appender_with_retention(directory, MAX_LOG_FILES)
}

/// `daily_appender` with the retention limit as a parameter, so the
/// policy can be exercised at a size a test can seed by hand.
///
/// **`max_log_files` is deliberately NOT set on the builder.** Setting it
/// would re-arm the library's btime ranking on every rotation and at
/// construction - and at construction it is actively dangerous, because
/// the library prunes when `files.len() >= max_files` and reduces to
/// `max_files - 1`, so with a directory full of tie-btime restored files
/// the one it drops can be today's. Retention is ours alone.
///
/// KNOWN GAP, stated rather than left to be discovered: pruning now
/// happens at CONSTRUCTION only, not on daily rotation. The unit is
/// `Restart=always` and `c` deploys roughly daily, so in practice every
/// day brings a fresh appender - but an uptime longer than `max_files`
/// days with no restart would let the directory grow past the limit until
/// the next start. Growth is still bounded by restarts rather than
/// unbounded, which was the original defect; closing the last of it needs
/// a rotation hook the library does not expose.
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

    fn scratch(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("game_log_retention_{}_{label}_{unique}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        dir
    }

    /// Writes `count` synthetic already-rotated logs, oldest first.
    ///
    /// Ascending order ON PURPOSE. `prune_old_logs` ranks by the
    /// filesystem's creation timestamp and only falls back to the date in
    /// the filename when that metadata is unreadable, so creation ORDER is
    /// what the pruner sees and seeding backwards would test a ranking
    /// production never produces.
    ///
    /// PLATFORM NOTE, because this one runs on Linux in production and the
    /// bot's twin runs on Windows: both ranking paths order these
    /// correctly. Where `statx` reports a btime the creation timestamps
    /// ascend with the write order below; where it does not,
    /// `parse_date_from_filename` reads the `YYYY-MM-DD` suffix and orders
    /// by that instead. The seeds are written in ascending date AND
    /// ascending creation order so the two agree, which is also what a
    /// real rotation produces.
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

    #[test]
    fn constructing_the_appender_prunes_down_to_the_retention_limit() {
        let dir = scratch("prunes");
        let seeded = seed_aged_logs(&dir, 6);

        let _appender = daily_appender_with_retention(&dir, 3).expect("the appender must build against a real directory");

        // (n - 1) survive, because the appender is about to open today's
        // file as the nth - see `prune_old_logs`'s own comment.
        for stale in &seeded[..4] {
            assert!(!present(&dir, stale), "{stale} must be pruned");
        }
        assert!(present(&dir, &seeded[4]), "the two newest seeded logs must survive");
        assert!(present(&dir, &seeded[5]), "the two newest seeded logs must survive");
    }

    /// THE ONE THAT MATTERS FOR THIS PROCESS. The unit is `Restart=always`
    /// and `c` deploys roughly daily, so the game is restarted far more
    /// often than it crashes - and every start builds a fresh appender,
    /// which prunes. **A deploy that pruned the current day's log would
    /// destroy the evidence of whatever went wrong in the release before
    /// it**, which is precisely the log a rollback decision is made from.
    ///
    /// It cannot, and the reason is a property of the library rather than
    /// of this call site: `prune_old_logs` sorts ascending and deletes
    /// from the FRONT, so the newest `max_files - 1` always survive, and
    /// today's file is either the newest that exists or does not exist
    /// yet.
    #[test]
    fn todays_log_survives_a_restart_or_deploy_that_prunes() {
        let dir = scratch("today");
        seed_aged_logs(&dir, 4);

        let today = format!("{LOG_FILENAME_PREFIX}.2026-09-08");
        std::fs::write(dir.join(&today), "the release under investigation\n").expect("today's log must be writable");

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

        assert!(present(&dir, &today), "the day currently being written must never be pruned by a restart");
        let body = std::fs::read_to_string(dir.join(&today)).expect("today's log must still be readable");
        assert!(body.contains("the release under investigation"), "today's log must be appended to, not truncated");
    }

    /// The log directory on the box is inside `/var/lib/pathofdust`,
    /// alongside every other piece of game state, and it is a plausible
    /// place for an operator to leave a saved crash dump or a hand-copied
    /// file during an incident. The pruner filters on the filename prefix
    /// so it can only ever delete its own logs - pinned rather than
    /// trusted, because a pruner that ate an incident artifact would do it
    /// silently.
    #[test]
    fn nothing_outside_the_log_filename_prefix_is_ever_deleted() {
        let dir = scratch("foreign");
        seed_aged_logs(&dir, 5);
        std::fs::write(dir.join("incident-notes.txt"), "not a log\n").expect("foreign file must be writable");
        std::fs::write(dir.join("game.log.bak"), "hand-saved copy\n").expect("foreign file must be writable");

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

        assert!(present(&dir, "incident-notes.txt"), "a non-log file in the log directory must survive pruning");
    }

    /// Filenames must not move. The files already on the box are
    /// `game.log.<date>`, and `prune_old_logs` filters on that prefix - so
    /// a builder that emitted a different name would leave every existing
    /// file permanently invisible to the policy while the policy itself
    /// reported success. That is the failure that looks like a working
    /// pruner, and it is the same shape as a bug caught in the bot's
    /// backup script a session earlier.
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
        assert!(name.starts_with("game.log."), "the filename must still be `game.log.<date>`, got {name}");
        let date = &name["game.log.".len()..];
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
        let dir = std::env::temp_dir().join(format!("game_log_restore_{}_{label}_{unique}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch dir must be creatable");
        dir
    }

    /// **THE RESTORE CONDITION, MADE PERMANENT.**
    ///
    /// `tar -xzf` gives every extracted file the same creation timestamp,
    /// so after a restore the whole log directory ties on btime - and a
    /// pruner that ranks on btime is then choosing arbitrarily between a
    /// month-old log and the current day's. This seeds that state
    /// deliberately: no sleeps, no ordering, every file written in one
    /// tight loop, which on tmpfs ties to the nanosecond and on any
    /// filesystem ties often enough to matter.
    ///
    /// It is also the exact shape that made the box red while Windows
    /// stayed green - Windows never ties, so the defect was invisible
    /// here. Seeding the tie rather than hoping for it is what makes this
    /// test mean the same thing on both.
    ///
    /// **If this fails, someone has promoted btime back.** The filename
    /// carries the day; nothing else needs to.
    #[test]
    fn identically_timestamped_logs_still_keep_today() {
        let dir = scratch("tie");
        // Written NEWEST FIRST, so a pruner that fell back on directory
        // order rather than the filename would delete today's first. The
        // sibling suite seeds ascending on purpose; this one seeds the
        // adversarial order on purpose.
        let today = format!("{LOG_FILENAME_PREFIX}.2026-09-10");
        std::fs::write(dir.join(&today), "the release under investigation\n").expect("today's log must be writable");
        for day in (1..=8).rev() {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            std::fs::write(dir.join(&name), format!("restored day {day}\n")).expect("seed log must be writable");
        }

        let _appender = daily_appender_with_retention(&dir, 3).expect("the appender must build against a real directory");

        // CONTENT, not existence, and the order matters. A pruner that
        // deletes today's log does not leave a hole: the appender opens
        // the same filename immediately afterwards, so `exists()` is true
        // again a microsecond later and asserts almost nothing. The bytes
        // are the only witness that the file was not destroyed - checked
        // first, and with the message that names what actually went wrong.
        let body = std::fs::read_to_string(dir.join(&today)).expect("today's log must still be present after the prune");
        assert!(
            body.contains("the release under investigation"),
            "today's log was DELETED and reopened empty by a prune where every file shares a creation timestamp. This is the state a restore leaves behind, and this is the log a rollback is decided from"
        );
        // Two survive at max 3 - one slot is reserved for the file the
        // appender is about to open - and they must be the two NEWEST by
        // filename date, not by whatever order the tie resolved in.
        assert!(dir.join(format!("{LOG_FILENAME_PREFIX}.2026-09-08")).exists(), "the newest seeded day must survive alongside today");
        for day in 1..=7 {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            assert!(!dir.join(&name).exists(), "{name} is older than the retention window and must be pruned");
        }
    }

    /// A prefixed file the rotation never created is not ours to delete.
    /// The old library pruner would take `game.log.bak` - a hand-saved
    /// copy during an incident is exactly what ends up in a log directory,
    /// and it has no date for the policy to rank it by.
    #[test]
    fn a_prefixed_file_with_no_date_is_never_pruned() {
        let dir = scratch("undated");
        for day in 1..=6 {
            let name = format!("{LOG_FILENAME_PREFIX}.2026-09-{day:02}");
            std::fs::write(dir.join(&name), "rotated\n").expect("seed log must be writable");
        }
        std::fs::write(dir.join("game.log.bak"), "hand-saved during an incident\n").expect("foreign file must be writable");
        std::fs::write(dir.join("game.log"), "no date at all\n").expect("foreign file must be writable");

        let _appender = daily_appender_with_retention(&dir, 2).expect("the appender must build against a real directory");

        assert!(dir.join("game.log.bak").exists(), "a prefixed file with no parseable date must survive - the rotation did not create it");
        assert!(dir.join("game.log").exists(), "same for the bare prefix");
    }

    /// The ranking key itself, at the edges that decide what gets deleted.
    #[test]
    fn the_date_key_accepts_only_the_shape_rotation_writes() {
        assert_eq!(date_key_from_filename("game.log.2026-09-10"), Some(20_260_910));
        assert_eq!(date_key_from_filename("game.log.2026-01-01"), Some(20_260_101));
        assert!(
            date_key_from_filename("game.log.2026-09-09") < date_key_from_filename("game.log.2026-09-10"),
            "the key must order by day, which is the whole reason it exists"
        );
        assert!(
            date_key_from_filename("game.log.2025-12-31") < date_key_from_filename("game.log.2026-01-01"),
            "and across a year boundary, where a plain string compare of the day alone would invert"
        );
        for rejected in ["game.log.bak", "game.log", "game.log.", "game.log.2026-9-10", "game.log.2026-09-1", "other.log.2026-09-10", "game.log.20260910"] {
            assert_eq!(date_key_from_filename(rejected), None, "{rejected} must not be rankable, and therefore must not be deletable");
        }
    }
}
