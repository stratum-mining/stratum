//! Time abstraction for the vardiff algorithm.
//!
//! The vardiff algorithm consults "current time" to compute elapsed-time gates
//! between share-rate evaluations. In production this is `SystemTime::now()`.
//! For simulation and high-throughput testing, a mockable [`Clock`] lets the
//! algorithm run against a controlled time source so that thousands of trials
//! of simulated minutes complete in milliseconds of wall clock.
//!
//! The injection mechanism is intentionally simple: [`VardiffState`] holds an
//! `Arc<dyn Clock>` that defaults to [`SystemClock`]. Production behavior is
//! identical to the pre-injection code; test code constructs a
//! [`VardiffState`] with [`VardiffState::new_with_clock`] passing a
//! [`MockClock`] and drives time forward explicitly.
//!
//! [`VardiffState`]: super::classic::VardiffState
//! [`VardiffState::new_with_clock`]: super::classic::VardiffState::new_with_clock

use std::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
};

/// Source of "current time" for the vardiff algorithm.
///
/// Returns seconds since the UNIX epoch in production, or any
/// monotonically-advancing reference point in test contexts.
///
/// Implementations must be `Send + Sync` so they can be held by a
/// [`VardiffState`] stored in shared per-channel state across async tasks,
/// and `Debug` so [`VardiffState`] continues to derive `Debug`.
///
/// [`VardiffState`]: super::classic::VardiffState
pub trait Clock: Debug + Send + Sync {
    /// Returns the current time, in seconds.
    fn now_secs(&self) -> u64;
}

/// Production clock — reads from `std::time::SystemTime::now()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_secs()
    }
}

/// Mock clock for tests and simulation. Time advances only when explicitly
/// requested via [`MockClock::advance`] or [`MockClock::set`].
///
/// Internally backed by an [`AtomicU64`] so the clock is `Send + Sync` and
/// can be shared between the algorithm (which reads time) and the test
/// driver (which advances time) without locking. The typical pattern:
///
/// ```ignore
/// use std::sync::Arc;
/// use channels_sv2::vardiff::clock::MockClock;
///
/// let clock = Arc::new(MockClock::new(0));
/// let vardiff = VardiffState::new_with_clock(1.0, clock.clone()).unwrap();
/// // ... drive a tick ...
/// clock.advance(60); // simulated time moves forward
/// ```
#[derive(Debug, Default)]
pub struct MockClock {
    now: AtomicU64,
}

impl MockClock {
    /// Constructs a new mock clock initialized to `now_secs`.
    pub fn new(now_secs: u64) -> Self {
        Self {
            now: AtomicU64::new(now_secs),
        }
    }

    /// Advances simulated time by `secs` seconds.
    pub fn advance(&self, secs: u64) {
        self.now.fetch_add(secs, Ordering::Relaxed);
    }

    /// Sets simulated time to exactly `secs` seconds.
    pub fn set(&self, secs: u64) {
        self.now.store(secs, Ordering::Relaxed);
    }
}

impl Clock for MockClock {
    fn now_secs(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn system_clock_returns_recent_time() {
        let now = SystemClock.now_secs();
        // Sanity: after 2026, seconds since epoch should be > 1.7B and < 4B.
        assert!(now > 1_700_000_000);
        assert!(now < 4_000_000_000);
    }

    #[test]
    fn mock_clock_reflects_advance() {
        let clock = MockClock::new(100);
        assert_eq!(clock.now_secs(), 100);
        clock.advance(50);
        assert_eq!(clock.now_secs(), 150);
        clock.set(1000);
        assert_eq!(clock.now_secs(), 1000);
    }

    #[test]
    fn mock_clock_shared_via_arc_observes_external_updates() {
        let clock = Arc::new(MockClock::new(0));
        let clock_for_reader: Arc<dyn Clock> = clock.clone();
        clock.advance(42);
        assert_eq!(clock_for_reader.now_secs(), 42);
    }
}
