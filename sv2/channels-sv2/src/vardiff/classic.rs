use crate::target::hash_rate_from_target;
use bitcoin::Target;
use tracing::debug;

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

use super::{error::VardiffError, Vardiff};

/// Represents the dynamic state for a variable difficulty (Vardiff) connection.
///
/// Tracks performance and adjusts the mining target to achieve a desired share rate.
#[derive(Debug)]
pub struct VardiffState {
    /// Count of shares received since the last difficulty adjustment.
    pub shares_since_last_update: u32,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// The lowest hashrate (H/s) the system will allow; values below this are clamped.
    pub min_allowed_hashrate: f32,
}

impl VardiffState {
    /// Creates a new `VardiffState` with the default minimum hashrate.
    ///
    /// # Arguments
    /// * `estimated_hashrate` - The initial hashrate estimate.
    pub fn new() -> Result<Self, VardiffError> {
        Self::new_with_min(DEFAULT_MIN_HASHRATE)
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate.
    ///
    /// # Arguments
    /// * `min_allowed_hashrate` - The minimum hashrate to enforce. A non-positive or non-finite
    ///   value is meaningless as a floor (and would reintroduce the division-by-zero that
    ///   [`Vardiff::try_vardiff`] guards against), so it falls back to the default.
    pub fn new_with_min(min_allowed_hashrate: f32) -> Result<Self, VardiffError> {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let min_allowed_hashrate = if min_allowed_hashrate.is_finite() && min_allowed_hashrate > 0.0
        {
            min_allowed_hashrate
        } else {
            DEFAULT_MIN_HASHRATE
        };

        Ok(VardiffState {
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            min_allowed_hashrate,
        })
    }

    /// Sets the count of shares since the last update.
    pub fn set_shares_since_last_update(&mut self, shares_since_last_update: u32) {
        self.shares_since_last_update = shares_since_last_update;
    }
}

impl Vardiff for VardiffState {
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    /// Sets the timestamp of the last update.
    fn set_timestamp_of_last_update(&mut self, timestamp_of_last_update: u64) {
        self.timestamp_of_last_update = timestamp_of_last_update;
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
    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(VardiffError::TimeError)?
            .as_secs();

        let delta_time = match now.checked_sub(self.timestamp_of_last_update) {
            Some(delta_time) => delta_time,
            None => {
                // The clock stepped backwards (e.g. NTP correction), leaving the recorded
                // timestamp in the future, so elapsed time is unmeasurable. Re-anchor the
                // window to `now` and skip just this round: the `delta_time <= 15` guard
                // below returns before `reset_counter()` runs, so without re-anchoring here
                // every later round would measure against the same stale future timestamp
                // and vardiff would stall for the whole length of the backwards step.
                debug!(
                    target: "vardiff",
                    "Clock stepped backwards (recorded {}, now {}); re-anchoring vardiff window",
                    self.timestamp_of_last_update,
                    now
                );
                self.reset_counter()?;
                return Ok(None);
            }
        };

        if delta_time <= 15 {
            return Ok(None);
        }

        // `min_allowed_hashrate` is validated in `new_with_min`, but the field is `pub`,
        // so direct assignment can still plant a non-finite or non-positive floor;
        // sanitize it here as well before relying on it.
        let min_hashrate =
            if self.min_allowed_hashrate.is_finite() && self.min_allowed_hashrate > 0.0 {
                self.min_allowed_hashrate
            } else {
                DEFAULT_MIN_HASHRATE
            };

        // `hashrate` is the channel's nominal hashrate, which originates from the
        // miner-supplied `nominal_hash_rate` and is only checked for negativity upstream.
        // It is the divisor for the delta percentage below and the base every special-case
        // cap scales (`hashrate * 10.0`, `hashrate / 1.5`), so a value at or below the floor
        // just pins the result back to that floor instead of converging, and `0.0`/`NaN`
        // produce `inf`/`NaN` percentages that disable every `should_update` arm outright.
        // The output is already clamped to the floor below, so clamp the baseline to the
        // same floor. `is_finite` is still needed: `inf` survives a plain `max()` and makes
        // the percentage `inf / inf == NaN`.
        let hashrate = if hashrate.is_finite() {
            hashrate.max(min_hashrate)
        } else {
            debug!(
                target: "vardiff",
                "Prior hashrate {hashrate} unusable; using minimum {min_hashrate}",
            );
            min_hashrate
        };

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
            target
        );

        let mut new_hashrate = match hash_rate_from_target(
            target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(hashrate) => hashrate as f32,
            Err(e) => {
                debug!(
                    target: "vardiff",
                    "Target->Hashrate conversion failed: {:?}. Falling back using previous hashrate and realized_shares_per_minute", e
                );
                hashrate * realized_share_per_min as f32 / shares_per_minute
            }
        };

        let hashrate_delta = new_hashrate - hashrate;
        let hashrate_delta_percentage = (hashrate_delta.abs() / hashrate) * 100.0;

        debug!(
            target: "vardiff",
            "Calculated new hashrate: {:.2} H/s (Δ {:.2}%, previous {:.2} H/s)",
            new_hashrate,
            hashrate_delta_percentage,
            hashrate,
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
                dt if dt <= 30 => hashrate / 1.5,
                dt if dt < 60 => hashrate / 2.0,
                _ => hashrate / 3.0,
            };
        } else if hashrate_delta_percentage > 1000.0 {
            new_hashrate = match delta_time {
                dt if dt <= 30 => hashrate * 10.0,
                dt if dt < 60 => hashrate * 5.0,
                _ => hashrate * 3.0,
            };
        }
        if new_hashrate < min_hashrate {
            debug!(
                target: "vardiff",
                "New hashrate {:.2} H/s below minimum threshold {:.2} H/s — clamping",
                new_hashrate,
                min_hashrate
            );
            new_hashrate = min_hashrate;
        }
        self.reset_counter()?;

        Ok(Some(new_hashrate))
    }
}
