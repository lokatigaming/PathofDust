// A `Transport` wrapper that bounds how fast a FAILING connect can be
// retried. The second of the two unthrottled reconnect paths in
// twitch-irc 5.0.1 — the one item 7b did not close.
//
// THE DEFECT, in the crate's own line numbers
// (`twitch-irc-5.0.1/src/connection/event_loop.rs`):
//
//     :124   let rate_limit_permit = ...acquire_owned().await;
//     :129   let connect_attempt = T::new();
//     :132   let transport = tokio::select! { ... }?;     <-- returns HERE on failure
//     :143   tokio::spawn(async move {                    <-- only reached on SUCCESS
//     :146       sleep(config.new_connection_every).await;
//     :147       drop(rate_limit_permit);
//
// The permit is acquired at :124, but the `?` at :132 returns before the
// spawn at :143 that holds it for `new_connection_every`. So on a failed
// connect the permit is dropped immediately and the 2-second throttle
// NEVER APPLIES — it only applies once a connection has succeeded. A DNS
// failure needs no network round trip, so the pool retries at CPU speed.
//
// That is not theoretical. On 2026-09-12 it wrote 5.67 GB and ~31M lines
// to bot.log in about 100 minutes — `No such host is known. (os error
// 11001)` — roughly 100x the 2026-09-04 incident, which was the OTHER
// path (credential fetch, closed by `TwitchAuthStorage::load_token`).
// Item 7b's log rate limiter bounds what either shape can WRITE; this
// bounds how fast this one can SPIN.
//
// THE SEAM. `twitch_irc::transport::Transport` is a public trait whose
// `async fn new()` is exactly the call at :129, so wrapping it needs no
// fork and no patch: the delay happens inside the crate's own connect
// attempt, before the early return that loses the permit.
//
// WHY THE CAP IS BELOW 20 SECONDS, which is not obvious and is the one
// thing to re-check if these numbers are ever tuned: at :129-:132 the
// crate races `T::new()` against `config.connect_timeout` (default 20s)
// in a `select!`. A delay at or above that timeout means the select
// always fires the timeout branch, our future is CANCELLED mid-sleep,
// and the transport is never actually attempted — the loop would still
// be bounded, but it would stop reconnecting at all rather than
// reconnecting slowly. MAX_DELAY must stay comfortably under
// `connect_timeout`, so the order's suggested 30s cap is not usable
// here; 16s is.
//
// Because that cancellation can happen at any await point, the backoff
// is grown BEFORE the sleep rather than on the error path. A cancelled
// attempt must still count as an attempt, or a connect that always times
// out would never back off at all.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use twitch_irc::transport::Transport;
use twitch_irc::SecureTCPTransport;

/// First delay after a failure. Small enough that a one-off blip costs
/// nothing noticeable, which is the common case.
const INITIAL_DELAY: Duration = Duration::from_millis(500);

/// Ceiling. See the note above: this MUST stay under the crate's
/// `connect_timeout` (default 20s) or the attempt is cancelled instead
/// of delayed.
const MAX_DELAY: Duration = Duration::from_secs(16);

/// Growth factor per consecutive failure.
const FACTOR: u32 = 2;

/// How long to wait before the next connect attempt, grown on every
/// attempt and reset by a successful one.
///
/// Split out from the transport so the policy can be tested directly: a
/// `Transport` needs real stream and sink types, so faking one to assert
/// a delay sequence would cost far more than it proves.
#[derive(Debug)]
pub struct ConnectBackoff {
    current: Mutex<Duration>,
}

impl ConnectBackoff {
    pub const fn new() -> Self {
        Self { current: Mutex::new(Duration::ZERO) }
    }

    /// The delay to apply before this attempt, growing the stored value
    /// for the next one. The FIRST call after a reset returns zero, so a
    /// healthy reconnect is never slowed down.
    pub fn next_delay(&self) -> Duration {
        let Ok(mut current) = self.current.lock() else {
            // A poisoned lock must not take reconnection down with it;
            // erring toward the cap is the safe direction, since the
            // failure this exists to bound is "too fast".
            return MAX_DELAY;
        };
        let delay = *current;
        *current = if delay.is_zero() {
            INITIAL_DELAY
        } else {
            std::cmp::min(delay.saturating_mul(FACTOR), MAX_DELAY)
        };
        delay
    }

    /// A connection succeeded — the next failure starts from zero again.
    pub fn reset(&self) {
        if let Ok(mut current) = self.current.lock() {
            *current = Duration::ZERO;
        }
    }

    #[cfg(test)]
    fn peek(&self) -> Duration {
        *self.current.lock().unwrap()
    }
}

impl Default for ConnectBackoff {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide, because the thing being bounded is process-wide: the
/// pool creates a fresh transport per attempt, so per-instance state
/// would reset on every failure and bound nothing.
static BACKOFF: ConnectBackoff = ConnectBackoff::new();

/// `SecureTCPTransport`, with a failing connect slowed down. Every
/// associated type is delegated, so this is transparent to the pool.
#[derive(Debug)]
pub struct BackoffTransport(SecureTCPTransport);

#[async_trait]
impl Transport for BackoffTransport {
    type ConnectError = <SecureTCPTransport as Transport>::ConnectError;
    type IncomingError = <SecureTCPTransport as Transport>::IncomingError;
    type OutgoingError = <SecureTCPTransport as Transport>::OutgoingError;
    type Incoming = <SecureTCPTransport as Transport>::Incoming;
    type Outgoing = <SecureTCPTransport as Transport>::Outgoing;

    async fn new() -> Result<Self, Self::ConnectError> {
        let delay = BACKOFF.next_delay();
        if !delay.is_zero() {
            tracing::warn!(
                "Twitch chat connect is failing — waiting {:.1}s before the next attempt.",
                delay.as_secs_f64()
            );
            tokio::time::sleep(delay).await;
        }

        let transport = SecureTCPTransport::new().await?;
        BACKOFF.reset();
        Ok(BackoffTransport(transport))
    }

    fn split(self) -> (Self::Incoming, Self::Outgoing) {
        self.0.split()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delay sequence: free on the first attempt, then doubling, then
    /// pinned at the cap.
    #[test]
    fn the_delay_grows_from_zero_and_stops_at_the_cap() {
        let backoff = ConnectBackoff::new();

        assert_eq!(backoff.next_delay(), Duration::ZERO, "a healthy reconnect is never delayed");
        assert_eq!(backoff.next_delay(), INITIAL_DELAY);
        assert_eq!(backoff.next_delay(), INITIAL_DELAY * 2);
        assert_eq!(backoff.next_delay(), INITIAL_DELAY * 4);

        for _ in 0..50 {
            backoff.next_delay();
        }
        assert_eq!(backoff.next_delay(), MAX_DELAY, "it pins at the cap rather than growing without bound");
        assert_eq!(backoff.peek(), MAX_DELAY, "and stays there");
    }

    /// The cap has to stay under the crate's connect_timeout or the
    /// attempt is cancelled mid-sleep instead of delayed — see the module
    /// comment. This pins the relationship so tuning one without the
    /// other fails here rather than in production.
    #[test]
    fn the_cap_stays_below_the_crates_connect_timeout() {
        // twitch-irc 5.0.1 ClientConfig::default(): connect_timeout = 20s.
        let connect_timeout = Duration::from_secs(20);
        assert!(
            MAX_DELAY < connect_timeout,
            "MAX_DELAY {MAX_DELAY:?} must stay under connect_timeout {connect_timeout:?}, or T::new() is cancelled before it ever attempts"
        );
    }

    /// The defect this exists for: a connect that always fails must not
    /// retry at CPU speed. Walks simulated time through the policy's own
    /// delays and counts how many attempts fit.
    #[test]
    fn a_connect_that_always_fails_is_bounded_to_a_handful_of_attempts_a_minute() {
        let backoff = ConnectBackoff::new();
        let window = Duration::from_secs(60);

        let mut elapsed = Duration::ZERO;
        let mut attempts = 0;
        // The ceiling is part of the assertion, not a guard rail: with no
        // backoff at all every delay is zero, `elapsed` never advances,
        // and an unbounded loop here would HANG instead of failing. A
        // test that hangs reports nothing.
        while attempts < 10_000 {
            let delay = backoff.next_delay();
            if elapsed + delay > window {
                break;
            }
            elapsed += delay;
            attempts += 1;
        }

        // Unbounded, this loop ran at roughly 970 attempts a SECOND on
        // 2026-09-04 and wrote 5.67 GB on 2026-09-12.
        assert!(attempts <= 10, "60s of solid failure must allow at most a handful of attempts, got {attempts}");
        assert!(attempts >= 4, "but it must keep trying — a bound that stops reconnecting is a different outage, got {attempts}");
    }

    /// A success wipes the accumulated delay, so an intermittent failure
    /// does not leave the next reconnect slow.
    #[test]
    fn a_successful_connect_resets_the_delay() {
        let backoff = ConnectBackoff::new();
        for _ in 0..8 {
            backoff.next_delay();
        }
        assert!(backoff.peek() > INITIAL_DELAY, "several failures have grown it");

        backoff.reset();

        assert_eq!(backoff.peek(), Duration::ZERO);
        assert_eq!(backoff.next_delay(), Duration::ZERO, "the next attempt is immediate again");
    }
}
