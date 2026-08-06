use bitcoin::Target;
use error::VardiffError;
use std::fmt::Debug;

pub mod classic;
pub mod error;
#[cfg(test)]
pub mod test;

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

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
/// Returns a [`Box<dyn Vardiff>`] wrapping the adaptive EWMA algorithm:
/// - EWMA estimator with tau=360s for smoothed hashrate estimation
/// - Adaptive boundary: PoissonCI below SPM 6, sign-persistence CUSUM at SPM 6+
///   (tightening requires 8× the evidence of loosening — dangerous-direction
///   protection, not a lost-work cost)
/// - Accelerating partial retarget (eta 0.2 → 0.6 over consecutive fires)
pub fn default() -> Box<dyn Vardiff> {
    default_with_min(DEFAULT_MIN_HASHRATE)
}

/// Constructs the recommended production vardiff with a specific minimum
/// hashrate floor.
pub fn default_with_min(min_allowed_hashrate: f32) -> Box<dyn Vardiff> {
    Box::new(
        classic::VardiffState::new_with_min(min_allowed_hashrate)
            .expect("VardiffState construction should not fail"),
    )
}
