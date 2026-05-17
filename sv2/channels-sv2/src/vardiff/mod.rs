use bitcoin::Target;
use error::VardiffError;
use std::fmt::Debug;

pub mod classic;
pub mod clock;
pub mod composed;
pub mod error;
#[cfg(test)]
pub mod test;

pub use clock::{Clock, MockClock, SystemClock};

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
