use error::VardiffError;
use mining_sv2::Target;
use std::fmt::Debug;

pub mod classic;
pub mod error;
#[cfg(test)]
pub mod test;

/// Trait defining the interface for a Vardiff implementation.
pub trait Vardiff: Debug + Send {
    /// Gets the current estimated hashrate (H/s).
    fn hashrate(&self) -> f32;

    /// Gets the target shares per minute.
    fn shares_per_minute(&self) -> f32;

    /// Gets the timestamp of the last update.
    fn last_update_timestamp(&self) -> u64;

    /// Gets the share count since the last update.
    fn shares_since_last_update(&self) -> u32;

    /// Gets the current mining target.
    fn target(&self) -> Target;

    /// Sets the estimated hashrate and updates the target.
    fn set_hashrate(&mut self, estimated_downstream_hash_rate: f32) -> Result<(), VardiffError>;

    /// Sets timestamp since last update.
    fn set_timestamp_of_last_update(&mut self, timestamp: u64);

    /// Increments the share count.
    fn increment_shares_since_last_update(&mut self);

    /// Resets share count and timestamp for a new cycle.
    fn reset_counter(&mut self) -> Result<(), VardiffError>;

    /// Checks performance and potentially adjusts difficulty, returning the new
    /// hashrate if an update occurred.
    fn try_vardiff(&mut self) -> Result<Option<f32>, VardiffError>;

    /// Gets the minimum allowed hashrate (H/s).
    fn min_allowed_hashrate(&self) -> f32;
}
