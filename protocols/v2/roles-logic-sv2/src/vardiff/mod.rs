use error::VardiffError;
use mining_sv2::Target;
use std::fmt::Debug;

pub mod classic;
pub mod error;

pub trait Vardiff: Debug + Send {
    /// Returns the estimated hash rate.
    fn hashrate(&self) -> f32;

    /// Returns the configured shares per minute.
    fn shares_per_minute(&self) -> f32;

    /// Returns the timestamp of the last update.
    fn last_update_timestamp(&self) -> u64;

    /// Returns the number of shares since the last update.
    fn shares_since_last_update(&self) -> u32;

    /// Returns a reference to the current target.
    fn target(&self) -> Target;

    /// Sets the estimated hash rate and updates the target accordingly.
    fn set_hashrate(&mut self, estimated_downstream_hash_rate: f32) -> Result<(), VardiffError>;

    /// Increments the shares since the last update.
    fn increment_shares_since_last_update(&mut self);

    /// Resets the vardiff state for a new calculation cycle.
    fn reset_counter(&mut self) -> Result<(), VardiffError>;

    /// Updates the hash rate based on recent activity and returns the new hash rate and delta.
    /// Returns `None` if an update is not yet due.
    fn update_hashrate(&mut self) -> Result<(), VardiffError>;

    fn min_allowed_hashrate(&self) -> f32;
}
