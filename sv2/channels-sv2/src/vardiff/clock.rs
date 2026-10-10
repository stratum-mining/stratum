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

use super::error::VardiffError;
use std::{
    fmt::Debug,
    panic::RefUnwindSafe,
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
/// `RefUnwindSafe` is required for a semver reason rather than a functional one.
/// `Arc<dyn Clock>` is unwind-safe only if `dyn Clock` is, so without this bound
/// holding one costs [`VardiffState`] its `UnwindSafe` and `RefUnwindSafe`
/// implementations — auto traits, so losing them is a breaking change that
/// `cargo-semver-checks` reports as `auto_trait_impl_removed`. Both clocks here
/// satisfy it already: one is a unit struct, the other holds an atomic.
///
/// [`VardiffState`]: super::classic::VardiffState
pub trait Clock: Debug + Send + Sync + RefUnwindSafe {
    /// Returns the current time, in seconds.
    ///
    /// Fallible because the production implementation can fail: a system clock set before the UNIX
    /// epoch makes `duration_since` return an error. The pre-injection code propagated that to the
    /// caller, and every caller here is already returning [`VardiffError`], so it keeps
    /// propagating. Panicking instead would take down a pool process from inside the per-share
    /// path, where the neighbouring clock-rewind guard in `try_vardiff` shows the intended posture
    /// is to handle time anomalies rather than abort on them.
    fn now_secs(&self) -> Result<u64, VardiffError>;
}

/// Production clock — reads from `std::time::SystemTime::now()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> Result<u64, VardiffError> {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs())
    }
}

/// Mock clock for tests and simulation. Time advances only when explicitly
/// requested via [`MockClock::advance`] or [`MockClock::set`].
///
/// Internally backed by an [`AtomicU64`] so the clock is `Send + Sync` and
/// can be shared between the algorithm (which reads time) and the test
/// driver (which advances time) without locking. The typical pattern:
///
/// ```
/// use channels_sv2::vardiff::{classic::VardiffState, clock::MockClock};
/// use std::sync::Arc;
///
/// let clock = Arc::new(MockClock::new(0));
/// let _vardiff = VardiffState::new_with_clock(1.0, clock.clone()).unwrap();
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
    fn now_secs(&self) -> Result<u64, VardiffError> {
        Ok(self.now.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn system_clock_returns_recent_time() {
        let now = SystemClock.now_secs().expect("system clock after epoch");
        // Sanity: after 2026, seconds since epoch should be > 1.7B and < 4B.
        assert!(now > 1_700_000_000);
        assert!(now < 4_000_000_000);
    }

    #[test]
    fn mock_clock_reflects_advance() {
        let clock = MockClock::new(100);
        assert_eq!(clock.now_secs().unwrap(), 100);
        clock.advance(50);
        assert_eq!(clock.now_secs().unwrap(), 150);
        clock.set(1000);
        assert_eq!(clock.now_secs().unwrap(), 1000);
    }

    #[test]
    fn mock_clock_shared_via_arc_observes_external_updates() {
        let clock = Arc::new(MockClock::new(0));
        let clock_for_reader: Arc<dyn Clock> = clock.clone();
        clock.advance(42);
        assert_eq!(clock_for_reader.now_secs().unwrap(), 42);
    }
}
