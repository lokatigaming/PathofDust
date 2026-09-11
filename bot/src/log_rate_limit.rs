// A per-target, per-minute cap on log events, for the one failure mode
// that can fill a disk faster than log retention can prune it.
//
// On 2026-09-04 bot.log reached 58 MB, and 232,184 of its ~238,700 lines
// fell inside the single minute 07:38 — roughly 970 reconnect cycles a
// second. The cause was not a missing backoff: twitch-irc 5.0.1 throttles
// new connections to one every 2 seconds by default
// (`connection_rate_limiter`, `new_connection_every`). It is that the
// credential fetch sits *upstream* of that throttle — in the crate's
// `connection/event_loop.rs`, `get_credentials()` is awaited at :115 and
// its error returns at :120, before the rate-limit permit is acquired at
// :124 and before the sleep at :146. A credential error therefore retries
// with no delay at all, and because a DNS failure needs no network round
// trip, it fails at CPU speed.
//
// log_rate_limit is the second of two layers against that. The first is
// `TwitchAuthStorage::load_token`, which no longer reports a transient
// refresh failure as an error, so the pool does not enter that path in
// the first place. This layer is the backstop for anything else that
// learns to spin: it bounds what any one target can write per minute
// while deliberately keeping it *visible* — the next outage should read
// as a handful of lines plus one "suppressed N" line a minute, never as
// silence.
//
// Retention (item 7) bounds file *count*, not file size, so a single
// runaway minute defeats it. This is the part that bounds size.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// Exactly the two targets that produced the 2026-09-04 flood: they
/// accounted for 232,137 of that minute's 232,184 lines (116,069 and
/// 116,068 respectively). The other 47 lines that minute came from
/// ordinary bot targets and are deliberately left alone — this is a cap
/// on a known-pathological pair, not a general throttle on logging.
const RATE_LIMITED_TARGETS: [&str; 2] =
    ["twitch_irc::client::event_loop", "twitch_irc::connection::event_loop"];

/// Generous enough that a normal reconnect (which emits a handful of
/// lines) is never touched, small enough that a spin is capped at a few
/// hundred bytes a minute instead of 58 MB.
const MAX_EVENTS_PER_WINDOW: u64 = 20;
const WINDOW: Duration = Duration::from_secs(60);

/// Our own suppression notice — deliberately not a rate-limited target,
/// so the check in `event_enabled` short-circuits before taking the lock
/// and re-entering this layer cannot deadlock.
const NOTICE_TARGET: &str = "twitch_bot_rs::log_rate_limit";

fn is_rate_limited(target: &str) -> bool {
    RATE_LIMITED_TARGETS.contains(&target)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Under the cap — let it through.
    Emit,
    /// Over the cap — drop it, and remember that we did.
    Suppress,
    /// First event of a new window, and the window that just closed had
    /// dropped this many. Let this one through and report the total.
    EmitAfterSuppressing(u64),
}

#[derive(Debug)]
struct Bucket {
    window_start: Instant,
    emitted: u64,
    suppressed: u64,
}

/// The whole decision, kept free of `tracing` so it can be tested
/// directly with a synthetic flood and a controlled clock. The layer
/// below is a thin wrapper over this.
#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: HashMap<String, Bucket>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self { buckets: HashMap::new() }
    }

    pub fn decide(&mut self, target: &str, now: Instant) -> Decision {
        let bucket = self.buckets.entry(target.to_string()).or_insert(Bucket {
            window_start: now,
            emitted: 0,
            suppressed: 0,
        });

        if now.duration_since(bucket.window_start) >= WINDOW {
            let dropped = bucket.suppressed;
            bucket.window_start = now;
            bucket.emitted = 1;
            bucket.suppressed = 0;
            return if dropped > 0 { Decision::EmitAfterSuppressing(dropped) } else { Decision::Emit };
        }

        if bucket.emitted < MAX_EVENTS_PER_WINDOW {
            bucket.emitted += 1;
            Decision::Emit
        } else {
            bucket.suppressed += 1;
            Decision::Suppress
        }
    }
}

#[derive(Debug, Default)]
pub struct LogRateLimitLayer {
    limiter: Mutex<RateLimiter>,
}

impl LogRateLimitLayer {
    pub fn new() -> Self {
        Self { limiter: Mutex::new(RateLimiter::new()) }
    }
}

impl<S: Subscriber> Layer<S> for LogRateLimitLayer {
    // Returning false here drops the event for the whole subscriber, not
    // just one layer, which is what makes this cap both the console and
    // the rolling file with one filter.
    fn event_enabled(&self, event: &Event<'_>, _ctx: Context<'_, S>) -> bool {
        let target = event.metadata().target();
        if !is_rate_limited(target) {
            return true;
        }

        // Scoped so the lock is released before the notice below is
        // emitted — that notice re-enters this method, and only the
        // short-circuit above keeps it from wanting this same lock.
        let decision = match self.limiter.lock() {
            Ok(mut limiter) => limiter.decide(target, Instant::now()),
            // A poisoned lock must never take logging down with it.
            Err(_) => return true,
        };

        match decision {
            Decision::Emit => true,
            Decision::Suppress => false,
            Decision::EmitAfterSuppressing(dropped) => {
                tracing::warn!(
                    target: NOTICE_TARGET,
                    "suppressed {dropped} further events from {target} in the previous minute"
                );
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOD_TARGET: &str = "twitch_irc::client::event_loop";

    /// The 2026-09-04 shape, scaled down: a flood inside one window must
    /// emit exactly the cap and drop the rest, and the next window must
    /// report the true dropped total in one line.
    #[test]
    fn a_synthetic_flood_is_bounded_and_the_total_is_reported() {
        let mut limiter = RateLimiter::new();
        let start = Instant::now();

        let mut emitted = 0u64;
        let mut suppressed = 0u64;
        for _ in 0..100_000 {
            match limiter.decide(FLOOD_TARGET, start) {
                Decision::Emit => emitted += 1,
                Decision::Suppress => suppressed += 1,
                Decision::EmitAfterSuppressing(_) => panic!("no window boundary was crossed"),
            }
        }

        assert_eq!(emitted, MAX_EVENTS_PER_WINDOW, "the cap is what bounds the file");
        assert_eq!(suppressed, 100_000 - MAX_EVENTS_PER_WINDOW, "everything over the cap is dropped");

        // The next window opens and must account for exactly what it ate.
        assert_eq!(
            limiter.decide(FLOOD_TARGET, start + WINDOW),
            Decision::EmitAfterSuppressing(100_000 - MAX_EVENTS_PER_WINDOW),
        );
        // ...and only once — the window has been reset.
        assert_eq!(limiter.decide(FLOOD_TARGET, start + WINDOW), Decision::Emit);
    }

    /// The cap is per target, so one spinning target must not spend
    /// another's budget.
    #[test]
    fn the_cap_is_per_target() {
        let mut limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..100 {
            limiter.decide(FLOOD_TARGET, start);
        }
        assert_eq!(limiter.decide(FLOOD_TARGET, start), Decision::Suppress);
        assert_eq!(
            limiter.decide("twitch_irc::connection::event_loop", start),
            Decision::Emit,
            "a second target starts with its own budget"
        );
    }

    /// The tests above prove the decision; this one proves the wiring.
    /// `event_enabled` returning false has to drop the event for the
    /// whole subscriber, or the layer would be a no-op that still passed
    /// every unit test above.
    #[test]
    fn the_layer_actually_keeps_suppressed_events_out_of_the_output() {
        use std::io;
        use std::sync::{Arc, Mutex as StdMutex};
        use tracing_subscriber::prelude::*;

        #[derive(Clone)]
        struct Capture(Arc<StdMutex<Vec<u8>>>);
        impl io::Write for Capture {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
            type Writer = Capture;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let buffer = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry()
            .with(LogRateLimitLayer::new())
            .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(Capture(buffer.clone())));

        tracing::subscriber::with_default(subscriber, || {
            for i in 0..5_000 {
                // The real flood was emitted by the crate under this
                // exact target; `target:` lets the test stand in for it.
                tracing::error!(target: "twitch_irc::client::event_loop", "pool connection {i} has failed");
            }
            // An unrelated target in the same burst must survive intact.
            tracing::info!(target: "twitch_bot_rs::song_requests", "unrelated line");
        });

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        let flood_lines = output.lines().filter(|l| l.contains("has failed")).count();
        assert_eq!(
            flood_lines, MAX_EVENTS_PER_WINDOW as usize,
            "5,000 events must reach the writer as exactly {MAX_EVENTS_PER_WINDOW}"
        );
        assert!(output.contains("unrelated line"), "an unrelated target must not be caught by the cap");
    }

    /// Everything outside the pathological pair — including this layer's
    /// own notice — must bypass the limiter entirely.
    #[test]
    fn only_the_two_flooding_targets_are_limited() {
        assert!(is_rate_limited("twitch_irc::client::event_loop"));
        assert!(is_rate_limited("twitch_irc::connection::event_loop"));
        assert!(!is_rate_limited("twitch_bot_rs::song_requests"));
        assert!(!is_rate_limited("twitch_irc::client"), "the filter is exact, not a prefix");
        assert!(!is_rate_limited(NOTICE_TARGET), "the notice must never limit itself into silence");
    }
}
