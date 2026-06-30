use bitcoin::Target;
use error::VardiffError;
use std::fmt::Debug;
use std::sync::Arc;

pub mod classic;
pub mod clock;
pub mod composed;
pub mod error;
pub mod pid_tuned;
pub mod pow2_pid;
#[cfg(test)]
pub mod test;

pub use clock::{Clock, MockClock, SystemClock};

/// Default minimum hashrate (H/s) used by [`default`] when no value is
/// supplied.
pub const DEFAULT_MIN_HASHRATE: f32 = 1.0;

/// Trait defining the interface for a Vardiff implementation.
pub trait Vardiff: Debug + Send + Sync {
    /// Gets the timestamp of the last update.
    fn last_update_timestamp(&self) -> u64;

    /// Gets the share count since the last update.
    fn shares_since_last_update(&self) -> u32;

    /// Sets timestamp since last update.
    fn set_timestamp_of_last_update(&mut self, timestamp: u64);

    /// Increments the share count.
    fn increment_shares_since_last_update(&mut self);

    /// Adds `n` shares to the counter in a single operation.
    ///
    /// Default implementation calls [`Self::increment_shares_since_last_update`]
    /// `n` times, which is correct but `O(n)`. Implementors may override with
    /// a saturating bulk add for `O(1)` performance at large `n`. The
    /// simulation framework uses this method to bulk-add the Poisson-sampled
    /// share count per tick, which can reach the millions during cold-start
    /// scenarios — calling `increment` that many times would dominate
    /// simulation runtime.
    fn add_shares(&mut self, n: u32) {
        for _ in 0..n {
            self.increment_shares_since_last_update();
        }
    }

    /// Resets share count and timestamp for a new cycle.
    fn reset_counter(&mut self) -> Result<(), VardiffError>;

    /// Checks performance and potentially adjusts difficulty, returning the new
    /// hashrate if an update occurred.
    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError>;

    /// Gets the minimum allowed hashrate (H/s).
    fn min_allowed_hashrate(&self) -> f32;
}

/// Constructs the recommended production vardiff: the decline-safety champion.
///
/// Returns a [`Box<dyn Vardiff>`] wrapping:
/// `EwmaEstimator(360s) + AdaptiveSignPersist(spm_threshold=6) +
/// AcceleratingPartialRetarget(base=0.2, max=0.6, acc=0.05)`.
/// Uses [`DEFAULT_MIN_HASHRATE`] as the minimum hashrate floor and a
/// [`SystemClock`] for time.
///
/// The adaptive boundary uses PoissonCI for miners below SPM 6 (preventing
/// overshoot on sparse data) and a sign-persistence CUSUM for miners at SPM 6+,
/// which protects the dangerous (tightening) direction: tightening requires 8×
/// the evidence of loosening (dangerous-direction protection, not a lost-work
/// cost), and a sign-persistence discount relaxes the threshold only after
/// consecutive same-direction ticks. Selected by a decline-safety minimax (a hard
/// no-spiral constraint), not a scalar fitness score.
///
/// This is the recommended entry point for new production code. For
/// custom min-hashrate floors, use [`default_with_min`]. For a custom
/// [`Clock`] implementation (typically [`MockClock`] in tests), use
/// [`default_with_clock`].
pub fn default() -> Box<dyn Vardiff> {
    default_with_min(DEFAULT_MIN_HASHRATE)
}

/// Constructs the recommended production vardiff with a specific minimum
/// hashrate floor.
///
/// Equivalent to [`default`] but lets callers set the
/// `min_allowed_hashrate` floor. See [`default`] for the underlying
/// composition.
pub fn default_with_min(min_allowed_hashrate: f32) -> Box<dyn Vardiff> {
    default_with_clock(min_allowed_hashrate, Arc::new(SystemClock))
}

/// Constructs the recommended production vardiff with a specific minimum
/// hashrate floor and a custom [`Clock`] implementation.
///
/// Primarily intended for simulation and testing, where a
/// [`MockClock`] lets the algorithm run against controlled time. See
/// [`default`] for the underlying composition.
pub fn default_with_clock(min_allowed_hashrate: f32, clock: Arc<dyn Clock>) -> Box<dyn Vardiff> {
    Box::new(
        classic::VardiffState::new_with_clock(min_allowed_hashrate, clock)
            .expect("VardiffState construction should not fail"),
    )
}
