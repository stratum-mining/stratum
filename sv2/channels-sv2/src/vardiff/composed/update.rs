//! UpdateRule trait — Axis 4 of the four-axis vardiff decomposition.
//!
//! The UpdateRule decides, when the algorithm has chosen to fire, by how
//! much it moves the target. It is the piece of the algorithm that lives
//! in control theory: design pressure is convergence speed (large updates,
//! faster adjustment) vs stability of the steady state (small updates,
//! less jitter).

use super::estimator::EstimatorSnapshot;
use std::fmt::Debug;

/// Axis 4: when the algorithm decides to fire, by how much does it move
/// the target?
///
/// **Theory**: control theory (actuator design, stability).
///
/// **Design pressure**: speed of convergence vs stability of the steady
/// state.
///
/// Implementations: [`FullRetargetWithClamp`] (classic),
/// [`PartialRetarget`] (EWMA / FullRemedy / ClassicPartialRetarget),
/// [`FullRetargetNoClamp`] (Sliding-Window).
pub trait UpdateRule: Debug + Send + Sync {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        delta: f64,
        shares_per_minute: f32,
    ) -> f32;
}

/// Full retarget toward the Estimator's belief `h_estimate`, with two
/// safety clamps that mirror `VardiffState::try_vardiff`:
///
/// 1. **Zero-shares branch**: if the window observed no shares the
///    estimator's `h_estimate` is unreliable (typically near zero from
///    the underlying conversion). Divide the current hashrate by a
///    `dt_secs`-dependent factor instead.
/// 2. **Phase 1 clamp**: when δ exceeds `clamp_above_delta` (in
///    percentage points) the estimator's belief is far from the current
///    target — apply a `dt_secs`-dependent multiplicative ceiling rather
///    than trusting the noisy single-window estimate. Prevents target
///    explosions during cold-start ramp.
///
/// The [`FullRetargetWithClamp::classic`] constructor reproduces
/// `VardiffState`'s constants exactly: down clamps `(1.5, 2.0, 3.0)`,
/// up clamps `(10.0, 5.0, 3.0)`, up-clamp trigger at δ > 1000%.
#[derive(Debug, Clone, Copy)]
pub struct FullRetargetWithClamp {
    /// Up-clamp multiplier for `dt_secs <= 30`.
    pub up_short: f32,
    /// Up-clamp multiplier for `30 < dt_secs < 60`.
    pub up_med: f32,
    /// Up-clamp multiplier for `dt_secs >= 60`.
    pub up_long: f32,
    /// Down-clamp divisor for `dt_secs <= 30` (when `realized == 0`).
    pub down_short: f32,
    /// Down-clamp divisor for `30 < dt_secs < 60`.
    pub down_med: f32,
    /// Down-clamp divisor for `dt_secs >= 60`.
    pub down_long: f32,
    /// δ threshold (percentage points) above which the up-clamp engages.
    /// Classic value: 1000.0 (hashrate has changed by more than 10×).
    pub clamp_above_delta: f64,
}

impl FullRetargetWithClamp {
    /// The classic constants from `VardiffState::try_vardiff`.
    pub fn classic() -> Self {
        Self {
            up_short: 10.0,
            up_med: 5.0,
            up_long: 3.0,
            down_short: 1.5,
            down_med: 2.0,
            down_long: 3.0,
            clamp_above_delta: 1000.0,
        }
    }
}

impl UpdateRule for FullRetargetWithClamp {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        delta: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        // Branch 1: zero shares → down-clamp. Mirrors the
        // `realized_share_per_min == 0.0` arm of VardiffState. The
        // f64 == 0.0 comparison is safe here because realized is computed
        // as integer_shares / float_dt, so it's exactly 0.0 iff shares == 0.
        if snap.realized_share_per_min == 0.0 {
            return match snap.dt_secs {
                d if d <= 30 => current_hashrate / self.down_short,
                d if d < 60 => current_hashrate / self.down_med,
                _ => current_hashrate / self.down_long,
            };
        }
        // Branch 2: very large δ → up-clamp (Phase 1 ramp safety).
        if delta > self.clamp_above_delta {
            return match snap.dt_secs {
                d if d <= 30 => current_hashrate * self.up_short,
                d if d < 60 => current_hashrate * self.up_med,
                _ => current_hashrate * self.up_long,
            };
        }
        // Branch 3: trust the estimator.
        snap.h_estimate
    }
}

/// Partial retarget: `new_target = current + η × (h_estimate − current)`.
///
/// `η = 1.0` reproduces full retarget without clamping. `η = 0.5`
/// (the EWMA default) moves halfway toward the estimator's belief
/// each fire — a damped controller useful when the estimator is noisy
/// and overshoots would be expensive. Compared to
/// [`FullRetargetWithClamp`], this update has no per-`dt_secs`
/// branching and no zero-shares special case: the estimator's
/// `h_estimate` is trusted directly, scaled by η.
///
/// For algorithms that pair this with an EWMA estimator, the
/// estimator's smoothing already filters out single-window noise — so
/// the Phase 1 clamp is redundant and would only slow convergence.
#[derive(Debug, Clone, Copy)]
pub struct PartialRetarget {
    /// Damping factor in `(0, 1]`. Smaller = slower convergence,
    /// more stable. Default for EWMA: `0.5`.
    pub eta: f32,
}

impl PartialRetarget {
    pub fn new(eta: f32) -> Self {
        Self { eta }
    }

    /// The default damping for the `EWMA-60s` algorithm: `η = 0.5`.
    /// `FullRemedy` uses a tighter `η = 0.3` instead — see the
    /// algorithm registry in `DESIGN.md`.
    pub fn default_ewma() -> Self {
        Self { eta: 0.5 }
    }
}

impl UpdateRule for PartialRetarget {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        _delta: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        current_hashrate + self.eta * (snap.h_estimate - current_hashrate)
    }
}

/// Full retarget without clamping: `new_hashrate = h_estimate`. The
/// estimator's belief is trusted directly, no branches for zero-shares
/// or extreme deltas.
///
/// Pairs naturally with a smoothing estimator (`EwmaEstimator`,
/// `SlidingWindowEstimator`) where the smoothing already filters
/// single-window noise — a Phase 1 clamp would only slow convergence
/// for no benefit, since the estimator never produces wild values.
///
/// For a `CumulativeCounter` estimator paired with `FullRetargetNoClamp`,
/// cold-start retargeting can swing wildly because the estimator's
/// h_estimate at the first tick (when `current_h ≪ true_h`) is
/// computed from `realized × current_h / configured_spm`, which (after
/// the U256 fix) is approximately true_h. So the algorithm would jump
/// to true_h in one fire. That's *fast* convergence but may be
/// undesirable for production reliability — hence Classic's preference
/// for `FullRetargetWithClamp` with the conservative 3× ramp.
#[derive(Debug, Clone, Copy, Default)]
pub struct FullRetargetNoClamp;

impl UpdateRule for FullRetargetNoClamp {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        _current_hashrate: f32,
        _delta: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        // Trust the estimator's belief. The min_allowed_hashrate floor
        // is applied at the Composed-adapter level, so we don't clamp
        // here even when h_estimate is below it.
        snap.h_estimate.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(h: f32, realized: f64, dt: u64) -> EstimatorSnapshot {
        EstimatorSnapshot {
            h_estimate: h,
            realized_share_per_min: realized,
            n_shares: 0,
            dt_secs: dt,
        }
    }

    #[test]
    fn zero_shares_short_dt_divides_by_one_point_five() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 20), 1.0e15, 100.0, 12.0);
        assert!((h / (1.0e15 / 1.5) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_shares_medium_dt_divides_by_two() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 45), 1.0e15, 100.0, 12.0);
        assert!((h / (1.0e15 / 2.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_shares_long_dt_divides_by_three() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 120), 1.0e15, 100.0, 12.0);
        assert!((h / (1.0e15 / 3.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn large_delta_long_dt_multiplies_by_three() {
        let u = FullRetargetWithClamp::classic();
        // δ = 1500% > 1000% triggers the clamp; dt = 60 → ×3.
        let h = u.next_hashrate(&snap(1.0e20, 1200.0, 60), 1.0e10, 1500.0, 12.0);
        assert!((h / 3.0e10 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn large_delta_short_dt_multiplies_by_ten() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(1.0e20, 1200.0, 30), 1.0e10, 1500.0, 12.0);
        assert!((h / 1.0e11 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normal_delta_returns_h_estimate() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(2.0e15, 24.0, 60), 1.0e15, 100.0, 12.0);
        assert_eq!(h, 2.0e15);
    }

    #[test]
    fn exactly_one_thousand_delta_does_not_clamp() {
        // The classic code uses `> 1000.0`, not `>=`. Verify the boundary.
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(1.1e16, 24.0, 60), 1.0e15, 1000.0, 12.0);
        assert_eq!(h, 1.1e16); // h_estimate, not clamped
    }

    // ---- PartialRetarget ----

    #[test]
    fn partial_retarget_eta_half_moves_halfway() {
        let u = PartialRetarget::default_ewma();
        // current = 1e15, h_estimate = 2e15. η = 0.5.
        // new = 1e15 + 0.5 × (2e15 − 1e15) = 1.5e15.
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 12.0);
        assert!((h - 1.5e15).abs() / h < 1e-5, "got {}", h);
    }

    #[test]
    fn partial_retarget_eta_one_is_full_retarget() {
        let u = PartialRetarget::new(1.0);
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 12.0);
        assert!((h - 2.0e15).abs() / h < 1e-5);
    }

    #[test]
    fn partial_retarget_eta_zero_keeps_current() {
        let u = PartialRetarget::new(0.0);
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 12.0);
        assert_eq!(h, 1.0e15);
    }

    #[test]
    fn partial_retarget_no_zero_shares_branch() {
        // Unlike FullRetargetWithClamp, PartialRetarget doesn't special-
        // case `realized == 0`. h_estimate is trusted directly.
        let u = PartialRetarget::default_ewma();
        let h = u.next_hashrate(&snap(0.5e15, 0.0, 60), 1.0e15, 50.0, 12.0);
        // η × (0.5e15 − 1e15) + 1e15 = 0.5 × -0.5e15 + 1e15 = 0.75e15.
        assert!((h - 0.75e15).abs() / 0.75e15 < 1e-5);
    }

    // ---- FullRetargetNoClamp ----

    #[test]
    fn full_retarget_no_clamp_returns_h_estimate_directly() {
        let u = FullRetargetNoClamp;
        // Any h_estimate value flows through unchanged.
        for &(estimate, _expected) in &[
            (1.0e10f32, 1.0e10f32),
            (1.5e15, 1.5e15),
            (3.0e20, 3.0e20), // Extreme — no clamp.
        ] {
            let h = u.next_hashrate(&snap(estimate, 100.0, 60), 1.0e15, 50.0, 12.0);
            assert_eq!(h, estimate, "h_estimate={} should pass through", estimate);
        }
    }

    #[test]
    fn full_retarget_no_clamp_ignores_zero_shares_and_large_delta() {
        // Distinct from FullRetargetWithClamp: zero-shares doesn't
        // trigger down-clamp; large delta doesn't trigger up-clamp.
        let u = FullRetargetNoClamp;
        let h_zero = u.next_hashrate(&snap(0.5e15, 0.0, 60), 1.0e15, 50.0, 12.0);
        assert_eq!(h_zero, 0.5e15);
        let h_large = u.next_hashrate(&snap(1.5e20, 1e7, 60), 1.0e10, 9999.0, 12.0);
        assert_eq!(h_large, 1.5e20);
    }

    #[test]
    fn full_retarget_no_clamp_floors_negative_h_estimate_at_zero() {
        // A pathological estimator producing a negative h_estimate is
        // floored at 0.0. The min_allowed_hashrate floor in the
        // Composed adapter handles the production lower bound.
        let u = FullRetargetNoClamp;
        let h = u.next_hashrate(&snap(-1.0e10, 100.0, 60), 1.0e15, 50.0, 12.0);
        assert_eq!(h, 0.0);
    }
}
