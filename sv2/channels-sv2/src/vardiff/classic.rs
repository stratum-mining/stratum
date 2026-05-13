use crate::target::hash_rate_from_target;
use crate::vardiff::clock::{Clock, SystemClock};
use bitcoin::Target;
use std::sync::Arc;
use tracing::debug;

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

use super::{error::VardiffError, Vardiff};

/// Represents the dynamic state for a variable difficulty (Vardiff) connection.
///
/// Tracks performance and adjusts the mining target to achieve a desired share rate.
///
/// The state holds an `Arc<dyn Clock>` for reading "current time." In production
/// this is a [`SystemClock`] and behavior is identical to reading
/// `SystemTime::now()` directly. For simulation and high-throughput testing the
/// clock can be replaced with a mock via [`VardiffState::new_with_clock`].
#[derive(Debug)]
pub struct VardiffState {
    /// Count of shares received since the last difficulty adjustment.
    pub shares_since_last_update: u32,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// The lowest hashrate (H/s) the system will allow; values below this are clamped.
    pub min_allowed_hashrate: f32,
    /// Source of "current time" for elapsed-time computations.
    /// Defaults to [`SystemClock`]; replaceable via [`Self::new_with_clock`].
    ///
    /// Public so downstream consumers can continue to construct `VardiffState`
    /// via struct literal (e.g., custom impl Clock + Arc::new). The
    /// constructors are still the recommended path.
    pub clock: Arc<dyn Clock>,
}

// `Arc<dyn Clock>` does not auto-implement `UnwindSafe` / `RefUnwindSafe`
// because trait objects don't propagate auto-traits unless the bounds are
// included in the trait-object type. We assert them explicitly here:
// `VardiffState`'s scalar fields are trivially unwind-safe, and the contract
// of the `Clock` trait (a single `&self -> u64` read) does not introduce any
// interior mutability that could leave the type inconsistent after a panic.
//
// This preserves the auto-trait impls that `VardiffState` had prior to the
// `Clock` injection refactor — required for semver compatibility.
impl std::panic::UnwindSafe for VardiffState {}
impl std::panic::RefUnwindSafe for VardiffState {}

impl VardiffState {
    /// Creates a new `VardiffState` with the default minimum hashrate and the
    /// system clock.
    pub fn new() -> Result<Self, VardiffError> {
        Self::new_with_min(DEFAULT_MIN_HASHRATE)
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate and the
    /// system clock.
    ///
    /// # Arguments
    /// * `min_allowed_hashrate` - The minimum hashrate to enforce.
    pub fn new_with_min(min_allowed_hashrate: f32) -> Result<Self, VardiffError> {
        Self::new_with_clock(min_allowed_hashrate, Arc::new(SystemClock))
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate and a
    /// custom clock implementation.
    ///
    /// Primarily intended for simulation and testing, where a
    /// [`MockClock`](super::clock::MockClock) lets the algorithm run against
    /// controlled time. In production code prefer [`Self::new`] or
    /// [`Self::new_with_min`].
    ///
    /// # Arguments
    /// * `min_allowed_hashrate` - The minimum hashrate to enforce.
    /// * `clock` - The clock implementation to read current time from.
    pub fn new_with_clock(
        min_allowed_hashrate: f32,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, VardiffError> {
        let timestamp_secs = clock.now_secs();

        Ok(VardiffState {
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            min_allowed_hashrate,
            clock,
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

    /// Bulk-adds `n` shares with a single saturating add. Overrides the default
    /// trait implementation (which calls increment `n` times) for performance
    /// — the simulation framework calls this with `n` values into the millions
    /// during cold-start ticks, where the loop overhead would dominate.
    fn add_shares(&mut self, n: u32) {
        self.shares_since_last_update = self.shares_since_last_update.saturating_add(n);
    }

    /// Resets the share counter and updates the timestamp to now.
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        let timestamp_secs = self.clock.now_secs();
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
        let now = self.clock.now_secs();
        let delta_time = now.saturating_sub(self.timestamp_of_last_update);

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
        if new_hashrate < self.min_allowed_hashrate {
            debug!(
                target: "vardiff",
                "New hashrate {:.2} H/s below minimum threshold {:.2} H/s — clamping",
                new_hashrate,
                self.min_allowed_hashrate
            );
            new_hashrate = self.min_allowed_hashrate;
        }
        self.reset_counter()?;

        Ok(Some(new_hashrate))
    }
}
