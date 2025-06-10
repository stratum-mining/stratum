use crate::utils::{hash_rate_from_target, hash_rate_to_target};
use mining_sv2::Target;
use tracing::{debug, warn};

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

use super::{error::VardiffError, Vardiff};

/// Represents the dynamic state for a variable difficulty (Vardiff) connection.
///
/// Tracks performance and adjusts the mining target to achieve a desired share rate.
#[derive(Debug)]
pub struct VardiffState {
    /// Current estimated hashrate (H/s).
    pub estimated_hashrate: f32,
    /// Target number of shares to be submitted per minute.
    pub shares_per_minute: f32,
    /// Count of shares received since the last difficulty adjustment.
    pub shares_since_last_update: u32,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// The current mining target.
    pub current_target: Target,
    /// The lowest hashrate (H/s) the system will allow; values below this are clamped.
    pub min_allowed_hashrate: f32,
}

impl VardiffState {
    /// Creates a new `VardiffState` with the default minimum hashrate.
    ///
    /// # Arguments
    /// * `shares_per_minute` - The target share submission rate.
    /// * `estimated_hashrate` - The initial hashrate estimate.
    pub fn new(shares_per_minute: f32, estimated_hashrate: f32) -> Result<Self, VardiffError> {
        Self::new_with_min(shares_per_minute, estimated_hashrate, DEFAULT_MIN_HASHRATE)
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate.
    ///
    /// # Arguments
    /// * `shares_per_minute` - The target share submission rate.
    /// * `estimated_hashrate` - The initial hashrate estimate.
    /// * `min_allowed_hashrate` - The minimum hashrate to enforce.
    pub fn new_with_min(
        shares_per_minute: f32,
        estimated_hashrate: f32,
        min_allowed_hashrate: f32,
    ) -> Result<Self, VardiffError> {
        let current_target =
            hash_rate_to_target(estimated_hashrate as f64, shares_per_minute as f64)
                .map_err(|e| VardiffError::HashrateToTargetError(e.to_string()))?
                .into();
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        Ok(VardiffState {
            estimated_hashrate,
            shares_per_minute,
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            current_target,
            min_allowed_hashrate,
        })
    }

    /// Sets the target shares per minute.
    pub fn set_shares_per_minute(&mut self, shares_per_minute: f32) {
        self.shares_per_minute = shares_per_minute;
    }

    /// Sets the timestamp of the last update.
    pub fn set_timestamp_of_last_update(&mut self, timestamp_of_last_update: u64) {
        self.timestamp_of_last_update = timestamp_of_last_update;
    }

    /// Sets the count of shares since the last update.
    pub fn set_shares_since_last_update(&mut self, shares_since_last_update: u32) {
        self.shares_since_last_update = shares_since_last_update;
    }

    /// Sets the current miner target.
    pub fn set_current_target(&mut self, current_target: Target) {
        self.current_target = current_target;
    }
}

impl Vardiff for VardiffState {
    fn hashrate(&self) -> f32 {
        self.estimated_hashrate
    }

    fn shares_per_minute(&self) -> f32 {
        self.shares_per_minute
    }

    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    fn target(&self) -> Target {
        self.current_target.clone()
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    /// Sets the hashrate and recalculates the `current_target`.
    fn set_hashrate(&mut self, estimated_hashrate: f32) -> Result<(), VardiffError> {
        self.estimated_hashrate = estimated_hashrate;
        let current_target =
            hash_rate_to_target(estimated_hashrate as f64, self.shares_per_minute as f64)
                .map_err(|e| VardiffError::HashrateToTargetError(e.to_string()))?
                .into();
        self.set_current_target(current_target);
        Ok(())
    }

    /// Increments the share counter by one.
    fn increment_shares_since_last_update(&mut self) {
        self.shares_since_last_update += 1;
    }

    /// Resets the share counter and updates the timestamp to now.
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        self.set_timestamp_of_last_update(timestamp_secs);
        self.set_shares_since_last_update(0);
        Ok(())
    }

    /// Checks channel performance and potentially updates the hashrate and target.
    ///
    /// It calculates the realized share rate since the last update. If the
    /// deviation from the target rate is significant enough (based on internal,
    /// time-sensitive thresholds), it estimates a new hashrate and applies it.
    ///
    /// It returns `Ok(Some(new_hashrate))` when an update occurs,
    /// `Ok(None)` when conditions don't warrant an update, and
    /// `Err` for actual processing errors.
    fn try_vardiff(&mut self) -> Result<Option<f32>, VardiffError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(VardiffError::TimeError)?
            .as_secs();

        let delta_time = now - self.timestamp_of_last_update;

        #[cfg(not(test))]
        if delta_time <= 15 {
            return Ok(None);
        }

        let realized_share_per_min =
            self.shares_since_last_update as f64 / (delta_time as f64 / 60.0);

        debug!(
            target: "vardiff",
            "Hashrate update check triggered:
            - Elapsed time: {}s
            - Shares since last update: {}
            - Realized shares per minute: {:.4}
            - Current miner target: {:?}",
            delta_time,
            self.shares_since_last_update,
            realized_share_per_min,
            self.current_target
        );

        let mut new_hashrate = match hash_rate_from_target(
            self.current_target.clone().into(),
            realized_share_per_min,
        ) {
            Ok(hashrate) => hashrate as f32,
            Err(e) => {
                warn!(
                    target: "vardiff",
                    "Target->Hashrate conversion failed: {:?}. Falling back using previous hashrate and realized_shares_per_minute", e
                );
                self.estimated_hashrate * realized_share_per_min as f32 / self.shares_per_minute
            }
        };

        let hashrate_delta = new_hashrate - self.estimated_hashrate;
        let hashrate_delta_percentage = (hashrate_delta.abs() / self.estimated_hashrate) * 100.0;

        debug!(
            target: "vardiff",
            "Calculated new hashrate: {:.2} H/s (Δ {:.2}%, previous {:.2} H/s)",
            new_hashrate,
            hashrate_delta_percentage,
            self.estimated_hashrate,
        );

        let should_update = match hashrate_delta_percentage {
            pct if pct >= 100.0 => true,
            pct if pct >= 60.0 && delta_time >= 60 => true,
            pct if pct >= 50.0 && delta_time >= 120 => true,
            pct if pct >= 45.0 && delta_time >= 180 => true,
            pct if pct >= 30.0 && delta_time >= 240 => true,
            pct if pct >= 15.0 && delta_time >= 300 => true,
            _ => false,
        };

        if !should_update {
            return Ok(None);
        }

        // realized_share_per_min is 0.0 when d.difficulty_mgmt.shares_since_last_update is 0
        // so it's safe to compare realized_share_per_min with == 0.0
        if realized_share_per_min == 0.0 {
            new_hashrate = match delta_time {
                dt if dt <= 30 => self.estimated_hashrate / 1.5,
                dt if dt < 60 => self.estimated_hashrate / 2.0,
                _ => self.estimated_hashrate / 3.0,
            };
        } else if hashrate_delta_percentage > 1000.0 {
            new_hashrate = match delta_time {
                dt if dt <= 30 => self.estimated_hashrate * 10.0,
                dt if dt < 60 => self.estimated_hashrate * 5.0,
                _ => self.estimated_hashrate * 3.0,
            };
        }
        if new_hashrate < self.min_allowed_hashrate {
            debug!(
                target: "vardiff",
                "New hashrate {:.2} H/s below minimum threshold {:.2} H/s — clamping",
                new_hashrate,
                self.min_allowed_hashrate
            );
            new_hashrate = self.min_allowed_hashrate;
        }
        self.set_hashrate(new_hashrate)?;
        self.reset_counter()?;

        Ok(Some(new_hashrate))
    }
}
