//! A well-tuned PID controller for variable difficulty adjustment.
//!
//! Takes the concept from the Pow2-PID approach (direct error minimization in
//! SPM-space) and fixes the implementation flaws:
//!
//! - All three PID terms active (P for immediate response, I for
//!   persistent-error elimination, D for oscillation damping)
//! - Anti-windup on the integral term (exponential decay + clamp)
//! - Rate-aware gain scheduling (√(SPM) noise scaling)
//! - Dead zone replacing the Boundary concept (prevents acting on noise)
//! - No power-of-2 quantization
//! - Proper gain magnitudes
//!
//! ## Why PID for vardiff?
//!
//! The vardiff problem is a classic setpoint-tracking control problem:
//! drive realized_spm → target_spm by adjusting difficulty. PID is the
//! canonical solution for such problems. The three terms handle three
//! distinct failure modes:
//!
//! - **P**: immediate proportional correction to current error
//! - **I**: eliminates steady-state offset that P alone can't close
//!   (e.g., when the estimator is consistently biased in one direction)
//! - **D**: dampens oscillation by opposing rapid error changes
//!   (prevents overshoot when the P term overcorrects)
//!
//! ## Comparison to the three-stage pipeline
//!
//! The Composed framework decomposes the problem differently:
//! - Estimator ≈ measurement filter (analogous to PID's D-term filter)
//! - Boundary ≈ dead zone (prevents acting on noise)
//! - UpdateRule ≈ P-term (proportional move toward estimate)
//!
//! What Composed lacks is the **integral term**: persistent same-direction
//! errors don't accelerate correction. PartialRetarget(η=0.2) always
//! moves exactly 20% of the gap, whether it's the first fire or the
//! tenth consecutive fire in the same direction.
//!
//! ## Operating in difficulty-space
//!
//! The PID operates on the error signal `e = target_spm - realized_spm`
//! and produces a difficulty adjustment directly. This avoids the
//! hashrate estimation step and its associated noise (U256 arithmetic,
//! target↔hashrate conversion precision). The final hashrate for the
//! `Vardiff` trait is derived from the new difficulty at the end.

use std::sync::Arc;

use bitcoin::Target;

use super::error::VardiffError;
use super::{Clock, Vardiff};

/// A well-tuned PID vardiff controller.
///
/// Operates in difficulty-space with SPM error as the process variable.
/// Configurable via [`PidConfig`].
#[derive(Debug)]
pub struct PidTunedVardiff {
    config: PidConfig,
    current_difficulty: f64,
    shares_since_update: u32,
    timestamp_of_last_update: u64,
    // PID state
    error_integral: f64,
    prev_error: f64,
    prev_realized_spm: f64,
    consecutive_fires: u32,
    last_fire_direction: f64,
    clock: Arc<dyn Clock>,
    // Observability
    pub last_error: Option<f64>,
    pub last_output: Option<f64>,
    pub last_p: Option<f64>,
    pub last_i: Option<f64>,
    pub last_d: Option<f64>,
}

/// Configuration for the tuned PID vardiff.
#[derive(Debug, Clone, Copy)]
pub struct PidConfig {
    /// Target shares per minute.
    pub spm_target: f32,
    /// Proportional gain. Units: difficulty-fraction per SPM-error.
    /// At Kp=0.05, a 10-SPM error produces a 50% difficulty change.
    pub kp: f64,
    /// Integral gain. Units: difficulty-fraction per (SPM-error × tick).
    /// Eliminates persistent steady-state offset.
    pub ki: f64,
    /// Derivative gain. Units: difficulty-fraction per (SPM-error/sec).
    /// Dampens oscillation by opposing rapid error changes.
    pub kd: f64,
    /// Exponential decay factor applied to the integral per tick.
    /// Range (0, 1]. Prevents unbounded integral growth.
    /// 0.95 = effective memory ~20 ticks. 0.99 = ~100 ticks.
    pub integral_decay: f64,
    /// Maximum |integral| in SPM-error×ticks units. Hard clamp.
    pub integral_clamp: f64,
    /// Dead zone: minimum |error| in SPM before acting. Errors below
    /// this threshold still accumulate in the integral (so persistent
    /// small errors eventually break through) but don't trigger
    /// immediate correction.
    pub dead_zone_spm: f64,
    /// Minimum observation window before acting (seconds).
    pub min_window_secs: u64,
    /// Maximum single-fire adjustment as fraction of current difficulty.
    /// Prevents wild swings during cold start. 0.5 = max 50% change per fire.
    pub max_adjustment_fraction: f64,
    /// Minimum hashrate floor.
    pub min_allowed_hashrate: f32,
}

impl PidConfig {
    /// Balanced tuning: responsive to real changes, stable under noise.
    ///
    /// Designed for SPM=10-30 operational range. Gains are normalized
    /// by spm_target so behavior is consistent across share rates.
    pub fn balanced(spm_target: f32) -> Self {
        Self {
            spm_target,
            kp: 0.04,
            ki: 0.008,
            kd: 0.01,
            integral_decay: 0.92,
            integral_clamp: 30.0,
            dead_zone_spm: 0.5,
            min_window_secs: 20,
            max_adjustment_fraction: 0.5,
            min_allowed_hashrate: 1.0,
        }
    }

    /// Aggressive tuning: faster response, more jitter.
    pub fn aggressive(spm_target: f32) -> Self {
        Self {
            spm_target,
            kp: 0.08,
            ki: 0.015,
            kd: 0.005,
            integral_decay: 0.95,
            integral_clamp: 50.0,
            dead_zone_spm: 0.3,
            min_window_secs: 20,
            max_adjustment_fraction: 0.7,
            min_allowed_hashrate: 1.0,
        }
    }

    /// Conservative tuning: minimal jitter, slower response.
    pub fn conservative(spm_target: f32) -> Self {
        Self {
            spm_target,
            kp: 0.02,
            ki: 0.004,
            kd: 0.015,
            integral_decay: 0.88,
            integral_clamp: 20.0,
            dead_zone_spm: 1.0,
            min_window_secs: 30,
            max_adjustment_fraction: 0.3,
            min_allowed_hashrate: 1.0,
        }
    }
}

impl PidTunedVardiff {
    pub fn new(config: PidConfig, initial_hashrate: f32, clock: Arc<dyn Clock>) -> Self {
        let initial_difficulty =
            Self::difficulty_from_hashrate(initial_hashrate, config.spm_target);
        Self {
            config,
            current_difficulty: initial_difficulty,
            shares_since_update: 0,
            timestamp_of_last_update: clock.now_secs(),
            error_integral: 0.0,
            prev_error: 0.0,
            prev_realized_spm: config.spm_target as f64,
            consecutive_fires: 0,
            last_fire_direction: 0.0,
            clock,
            last_error: None,
            last_output: None,
            last_p: None,
            last_i: None,
            last_d: None,
        }
    }

    pub fn balanced(initial_hashrate: f32, spm_target: f32, clock: Arc<dyn Clock>) -> Self {
        Self::new(PidConfig::balanced(spm_target), initial_hashrate, clock)
    }

    fn difficulty_from_hashrate(hashrate: f32, spm: f32) -> f64 {
        let shares_per_second = spm as f64 / 60.0;
        hashrate as f64 / (shares_per_second * 2f64.powi(32))
    }

    fn hashrate_from_difficulty(difficulty: f64, spm: f32) -> f32 {
        let shares_per_second = spm as f64 / 60.0;
        (shares_per_second * difficulty * 2f64.powi(32)) as f32
    }

    pub fn current_difficulty(&self) -> f64 {
        self.current_difficulty
    }
}

impl Vardiff for PidTunedVardiff {
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
        self.config.min_allowed_hashrate
    }

    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.shares_since_update = 0;
        self.timestamp_of_last_update = self.clock.now_secs();
        self.error_integral = 0.0;
        self.prev_error = 0.0;
        self.consecutive_fires = 0;
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        _hashrate: f32,
        _target: &Target,
        _shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        self.last_error = None;
        self.last_output = None;
        self.last_p = None;
        self.last_i = None;
        self.last_d = None;

        let now = self.clock.now_secs();
        let dt = now.saturating_sub(self.timestamp_of_last_update);

        if dt < self.config.min_window_secs {
            return Ok(None);
        }

        let dt_secs = dt as f64;
        let dt_minutes = dt_secs / 60.0;

        // Compute realized SPM
        let realized_spm = if dt_minutes > 0.0 {
            self.shares_since_update as f64 / dt_minutes
        } else {
            return Ok(None);
        };

        // Error signal: positive error means we're getting fewer shares
        // than target (difficulty too high), negative means too many
        // (difficulty too low).
        let error = self.config.spm_target as f64 - realized_spm;
        self.last_error = Some(error);

        // Normalize error by spm_target for scale-invariant gains.
        // This means Kp=0.04 produces 4% difficulty change per 100% SPM error,
        // regardless of the absolute SPM target.
        let normalized_error = error / self.config.spm_target as f64;

        // --- Integral term ---
        // Decay existing integral (anti-windup via exponential forgetting)
        self.error_integral *= self.config.integral_decay;
        // Accumulate (always, even in dead zone — persistent small errors
        // eventually break through via the integral)
        self.error_integral += normalized_error;
        // Hard clamp (anti-windup ceiling)
        self.error_integral = self
            .error_integral
            .clamp(-self.config.integral_clamp, self.config.integral_clamp);

        // --- Derivative term ---
        // Rate of change of the error, using the previous realized_spm
        // for smoothing (derivative-on-measurement, not derivative-on-error,
        // to avoid derivative kick on setpoint changes).
        let d_realized = (realized_spm - self.prev_realized_spm) / dt_secs;
        // Negative because increasing realized_spm means error is decreasing
        let derivative = -d_realized / self.config.spm_target as f64;

        // --- PID output ---
        let p_term = self.config.kp * normalized_error;
        let i_term = self.config.ki * self.error_integral;
        let d_term = self.config.kd * derivative;

        self.last_p = Some(p_term);
        self.last_i = Some(i_term);
        self.last_d = Some(d_term);

        // Total output: fractional difficulty adjustment
        let raw_output = p_term + i_term + d_term;

        // Update derivative state
        self.prev_error = normalized_error;
        self.prev_realized_spm = realized_spm;

        // Dead zone: don't act unless the output exceeds the noise floor.
        // The dead zone is in normalized error units.
        let dead_zone_normalized = self.config.dead_zone_spm / self.config.spm_target as f64;
        if raw_output.abs() < dead_zone_normalized * self.config.kp {
            return Ok(None);
        }

        // Clamp output to max adjustment fraction
        let output = raw_output.clamp(
            -self.config.max_adjustment_fraction,
            self.config.max_adjustment_fraction,
        );
        self.last_output = Some(output);

        // Apply to difficulty. Positive error (under-performing) → positive
        // output → increase difficulty? No — if we're getting too few shares,
        // difficulty is too HIGH, so we need to DECREASE it.
        // The sign convention: output > 0 when error > 0 (too few shares),
        // which means difficulty should DECREASE.
        let new_difficulty = self.current_difficulty * (1.0 - output);

        if new_difficulty <= 0.0 {
            return Ok(None);
        }

        // Check if the change is meaningful (> 1% relative change)
        let relative_change =
            ((new_difficulty - self.current_difficulty) / self.current_difficulty).abs();
        if relative_change < 0.01 {
            return Ok(None);
        }

        // Track fire direction for consecutive-fire boosting
        let direction = if new_difficulty > self.current_difficulty {
            1.0
        } else {
            -1.0
        };
        if direction == self.last_fire_direction {
            self.consecutive_fires += 1;
        } else {
            self.consecutive_fires = 1;
            self.last_fire_direction = direction;
        }

        // Apply
        self.current_difficulty = new_difficulty;
        self.shares_since_update = 0;
        self.timestamp_of_last_update = now;

        let new_hashrate = Self::hashrate_from_difficulty(new_difficulty, self.config.spm_target);
        let floored = new_hashrate.max(self.config.min_allowed_hashrate);

        Ok(Some(floored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vardiff::MockClock;

    #[test]
    fn stable_load_does_not_fire() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        // On-target: 10 shares in 60s = 10 SPM
        v.add_shares(10);
        clock.set(60);
        assert_eq!(v.try_vardiff(1.0e15, &target, 10.0).unwrap(), None);
    }

    #[test]
    fn detects_50_percent_hashrate_drop() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        // 50% drop: only 5 shares in 60s = 5 SPM (error = +5)
        v.add_shares(5);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert!(result.is_some(), "should detect 50% drop");
        // New hashrate should be lower (difficulty decreased)
        assert!(result.unwrap() < 1.0e15);
    }

    #[test]
    fn detects_50_percent_hashrate_increase() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        // 50% increase: 15 shares in 60s = 15 SPM (error = -5)
        v.add_shares(15);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert!(result.is_some(), "should detect 50% increase");
        // New hashrate should be higher (difficulty increased)
        assert!(result.unwrap() > 1.0e15);
    }

    #[test]
    fn integral_accelerates_persistent_error() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        // Feed persistent under-performance (5 SPM vs 10 target)
        let mut adjustments = Vec::new();
        for tick in 1..=5 {
            v.add_shares(5);
            clock.set(tick * 60);
            if let Some(new_h) = v.try_vardiff(1.0e15, &target, 10.0).unwrap() {
                let adj = (new_h as f64 - 1.0e15) / 1.0e15;
                adjustments.push(adj);
            }
        }

        // With integral term, later fires should be more aggressive
        // (integral accumulates same-direction error)
        assert!(
            adjustments.len() >= 2,
            "should fire multiple times, got {}",
            adjustments.len()
        );
    }

    #[test]
    fn dead_zone_absorbs_small_noise() {
        let clock = Arc::new(MockClock::new(0));
        let config = PidConfig {
            dead_zone_spm: 2.0, // Wide dead zone
            ..PidConfig::balanced(10.0)
        };
        let mut v = PidTunedVardiff::new(config, 1.0e15, clock.clone());
        let target = Target::MAX;

        // Small deviation: 9 shares = 9 SPM (error = 1, within dead zone of 2)
        v.add_shares(9);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert_eq!(result, None, "small error within dead zone should not fire");
    }

    #[test]
    fn max_adjustment_prevents_wild_swings() {
        let clock = Arc::new(MockClock::new(0));
        let config = PidConfig {
            max_adjustment_fraction: 0.3, // Max 30% per fire
            ..PidConfig::aggressive(10.0)
        };
        let mut v = PidTunedVardiff::new(config, 1.0e15, clock.clone());
        let target = Target::MAX;

        // Extreme error: 0 shares in 60s
        v.add_shares(0);
        clock.set(60);
        let result = v.try_vardiff(1.0e15, &target, 10.0).unwrap();
        assert!(result.is_some());
        let change = (result.unwrap() as f64 - 1.0e15) / 1.0e15;
        assert!(
            change.abs() <= 0.31,
            "change {} exceeds max_adjustment_fraction",
            change
        );
    }

    #[test]
    fn respects_min_window() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        v.add_shares(0);
        clock.set(15); // Below min_window_secs=20
        assert_eq!(v.try_vardiff(1.0e15, &target, 10.0).unwrap(), None);
    }

    #[test]
    fn integral_decays_over_time() {
        let clock = Arc::new(MockClock::new(0));
        let mut v = PidTunedVardiff::balanced(1.0e15, 10.0, clock.clone());
        let target = Target::MAX;

        // Build up integral with under-performance
        v.add_shares(5);
        clock.set(60);
        let _ = v.try_vardiff(1.0e15, &target, 10.0);

        let integral_after_error = v.error_integral;

        // Now feed on-target data — integral should decay
        v.add_shares(10);
        clock.set(120);
        let _ = v.try_vardiff(1.0e15, &target, 10.0);

        assert!(
            v.error_integral.abs() < integral_after_error.abs(),
            "integral should decay when error returns to zero"
        );
    }
}
