//! Power-of-2 quantized PID variable difficulty algorithm.
//!
//! Reproduces a common open-source pool vardiff pattern that uses
//! the `pid` crate with power-of-2 difficulty quantization. The algorithm:
//!
//! 1. Measures realized shares/min over a sliding window
//! 2. Feeds realized SPM into a P-only controller (Ki=Kd=0)
//!    with `Kp = -difficulty × 0.01`
//! 3. Applies: `new_diff = quantize_pow2((diff + pid_output).max(diff × 0.1))`
//! 4. Re-tunes PID gains proportionally to new difficulty on retarget
//!
//! The power-of-2 quantization creates a massive dead zone (~41% of
//! current difficulty) that makes the algorithm nearly inert for normal
//! operational variance.

use std::sync::Arc;

use bitcoin::Target;

use super::error::VardiffError;
use super::{Clock, Vardiff};

/// Power-of-2 quantized PID vardiff algorithm.
///
/// Operates in difficulty-space with power-of-2 quantization.
/// The PID is effectively P-only (Ki=Kd=0).
#[derive(Debug)]
pub struct Pow2PidVardiff {
    shares_since_update: u32,
    timestamp_of_last_update: u64,
    current_difficulty: f32,
    initial_difficulty: f32,
    spm_target: f32,
    kp_fraction: f32,
    min_allowed_hashrate: f32,
    clock: Arc<dyn Clock>,
    /// Minimum observation window (seconds) before acting.
    min_window_secs: u64,
    /// Internal: last computed delta and threshold for observability.
    pub last_realized_spm: Option<f32>,
    pub last_pid_output: Option<f32>,
}

impl Pow2PidVardiff {
    /// Constructs with the given parameters.
    ///
    /// - `spm_target`: shares per minute target (typical: 10.0)
    /// - `kp_fraction`: proportional gain as fraction of difficulty (typical: 0.01)
    /// - `initial_hashrate`: used to compute initial difficulty
    /// - `min_allowed_hashrate`: floor
    pub fn new(
        spm_target: f32,
        kp_fraction: f32,
        initial_hashrate: f32,
        min_allowed_hashrate: f32,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let initial_difficulty = Self::difficulty_from_hashrate(initial_hashrate, spm_target);
        let quantized = nearest_power_of_2(initial_difficulty);
        Self {
            shares_since_update: 0,
            timestamp_of_last_update: clock.now_secs(),
            current_difficulty: quantized,
            initial_difficulty: quantized,
            spm_target,
            kp_fraction,
            min_allowed_hashrate,
            clock,
            min_window_secs: 20,
            last_realized_spm: None,
            last_pid_output: None,
        }
    }

    /// Typical defaults: SPM=10, Kp=0.01×diff.
    pub fn default_params(initial_hashrate: f32, clock: Arc<dyn Clock>) -> Self {
        Self::new(10.0, 0.01, initial_hashrate, 1.0, clock)
    }

    fn difficulty_from_hashrate(hashrate: f32, spm: f32) -> f32 {
        let shares_per_second = spm / 60.0;
        hashrate / (shares_per_second * 2f32.powi(32))
    }

    fn hashrate_from_difficulty(difficulty: f32, spm: f32) -> f32 {
        let shares_per_second = spm / 60.0;
        shares_per_second * difficulty * 2f32.powi(32)
    }

    pub fn current_difficulty(&self) -> f32 {
        self.current_difficulty
    }
}

impl Vardiff for Pow2PidVardiff {
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_update
    }

    fn set_timestamp_of_last_update(&mut self, ts: u64) {
        self.timestamp_of_last_update = ts;
    }

    fn increment_shares_since_last_update(&mut self) {
        self.shares_since_update = self.shares_since_update.saturating_add(1);
    }

    fn add_shares(&mut self, n: u32) {
        self.shares_since_update = self.shares_since_update.saturating_add(n);
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.shares_since_update = 0;
        self.timestamp_of_last_update = self.clock.now_secs();
        self.last_realized_spm = None;
        self.last_pid_output = None;
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        _hashrate: f32,
        _target: &Target,
        _shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        self.last_realized_spm = None;
        self.last_pid_output = None;

        let now = self.clock.now_secs();
        let dt = now.saturating_sub(self.timestamp_of_last_update);

        // Requires at least 20s of data before acting
        if dt < self.min_window_secs {
            return Ok(None);
        }

        // Compute realized shares per minute over the window
        let realized_spm = self.shares_since_update as f32 / (dt as f32 / 60.0);
        self.last_realized_spm = Some(realized_spm);

        // PID computation (P-only: Kp × error, where error = setpoint - measured)
        // pid crate convention: output = Kp × (setpoint - measurement)
        // With Kp negative: output = (-diff × kp_fraction) × (spm_target - realized)
        let kp = -(self.current_difficulty * self.kp_fraction);
        let error = self.spm_target - realized_spm;
        let pid_output = kp * error;

        // Clamp output to ±10× current difficulty
        let output_limit = self.current_difficulty * 10.0;
        let pid_output = pid_output.clamp(-output_limit, output_limit);
        self.last_pid_output = Some(pid_output);

        // Apply: new_diff = quantize((current + output).max(initial × 0.1))
        let raw_diff = (self.current_difficulty + pid_output).max(self.initial_difficulty * 0.1);
        let new_difficulty = nearest_power_of_2(raw_diff);

        if new_difficulty == self.current_difficulty {
            return Ok(None);
        }

        // Retarget: update state and re-tune PID (gains scale with new difficulty)
        self.current_difficulty = new_difficulty;
        self.initial_difficulty = new_difficulty;
        self.shares_since_update = 0;
        self.timestamp_of_last_update = now;

        let new_hashrate = Self::hashrate_from_difficulty(new_difficulty, self.spm_target);
        let floored = new_hashrate.max(self.min_allowed_hashrate);

        Ok(Some(floored))
    }
}

/// Rounds to nearest power of 2.
fn nearest_power_of_2(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.000_976_562_5;
    }
    let exponent = x.log2().round() as i32;
    2f32.powi(exponent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vardiff::MockClock;

    #[test]
    fn nearest_power_of_2_rounds_correctly() {
        assert_eq!(nearest_power_of_2(1.0), 1.0);
        assert_eq!(nearest_power_of_2(3.0), 4.0);
        assert_eq!(nearest_power_of_2(12.0), 16.0);
        assert_eq!(nearest_power_of_2(0.2), 0.25);
        assert_eq!(nearest_power_of_2(1024.0), 1024.0);
        assert_eq!(nearest_power_of_2(1500.0), 2048.0);
    }

    #[test]
    fn no_retarget_when_within_dead_zone() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = Pow2PidVardiff::default_params(1.0e15, clock.clone());
        let target = Target::MAX;

        // Feed exactly on-target shares (SPM=10 over 60s = 10 shares)
        v.add_shares(10);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert_eq!(result, None, "on-target should not retarget");
    }

    #[test]
    fn no_retarget_at_2x_hashrate() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = Pow2PidVardiff::default_params(1.0e15, clock.clone());
        let target = Target::MAX;

        // 2× hashrate → 20 SPM realized
        v.add_shares(20);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert_eq!(result, None, "2x hashrate should still be in dead zone");
    }

    #[test]
    fn retargets_at_extreme_hashrate_change() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = Pow2PidVardiff::default_params(1.0e15, clock.clone());
        let target = Target::MAX;

        // 10× hashrate → 100 SPM realized
        v.add_shares(100);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert!(result.is_some(), "10x hashrate should trigger retarget");
    }

    #[test]
    fn respects_minimum_window() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = Pow2PidVardiff::default_params(1.0e15, clock.clone());
        let target = Target::MAX;

        v.add_shares(1000);
        clock.set(15); // Only 15s elapsed, below 20s minimum
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert_eq!(result, None, "should not act before min_window_secs");
    }

    #[test]
    fn quantization_is_power_of_2() {
        let clock = Arc::new(MockClock::new(0));
        let v = Pow2PidVardiff::default_params(1.0e15, clock);
        let diff = v.current_difficulty();
        // Must be a power of 2
        assert!(
            (diff.log2().fract()).abs() < 1e-5,
            "difficulty {} is not a power of 2",
            diff
        );
    }
}
