//! Statistic trait — Axis 2 of the four-axis vardiff decomposition.
//!
//! The Statistic computes a scalar δ from the Estimator's snapshot that
//! summarizes "how surprised should I be?". It is the piece of the
//! algorithm that lives in hypothesis testing: design pressure is
//! power (small probability of missing a real change) vs simplicity
//! (easy thresholds to derive, easy to reason about).
//!
//! ## Units convention
//!
//! δ is reported in *percentage points* (a value of `60.0` means 60%),
//! not as a fraction. This matches the convention used inside
//! `VardiffState::try_vardiff` so the bit-equivalence with the classic
//! algorithm is preserved exactly. The [`Boundary`](super::boundary) axis
//! follows the same convention.
//!
//! [`Boundary`]: super::boundary

use super::estimator::EstimatorSnapshot;
use std::fmt::Debug;

/// Axis 2: how the algorithm computes a scalar δ from the Estimator's
/// snapshot, summarizing "how surprised should I be?".
///
/// **Theory**: hypothesis testing.
///
/// **Design pressure**: power (small probability of missing a real change)
/// vs simplicity (easy threshold derivation).
///
/// Implementations: [`AbsoluteRatio`] (shared by every algorithm in
/// the current registry). A `LogLikelihoodRatio` variant for CUSUM-style
/// algorithms is plausible but not yet implemented.
pub trait Statistic: Debug + Send + Sync {
    fn delta(&self, snap: &EstimatorSnapshot, current_hashrate: f32, shares_per_minute: f32)
        -> f64;
}

/// δ = `|H̃ - H_current| / H_current * 100`.
///
/// Matches the `hashrate_delta_percentage` formula in
/// `VardiffState::try_vardiff` byte-for-byte: the computation is performed
/// in `f32` and only promoted to `f64` on return, so `f32`-precision
/// rounding is preserved exactly. This is one of the four trait impls that,
/// composed together, reproduce `VardiffState` fire-for-fire.
#[derive(Debug, Default, Clone, Copy)]
pub struct AbsoluteRatio;

impl Statistic for AbsoluteRatio {
    fn delta(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        _shares_per_minute: f32,
    ) -> f64 {
        let delta_h: f32 = snap.h_estimate - current_hashrate;
        let pct: f32 = (delta_h.abs() / current_hashrate) * 100.0;
        pct as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(h_estimate: f32) -> EstimatorSnapshot {
        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min: 0.0,
            n_shares: 0,
            dt_secs: 60,
        }
    }

    #[test]
    fn equal_hashrates_yield_zero_delta() {
        let d = AbsoluteRatio.delta(&snap(1.0e15), 1.0e15, 12.0);
        assert_eq!(d, 0.0);
    }

    #[test]
    fn fifty_percent_overestimate_yields_fifty_pct() {
        let d = AbsoluteRatio.delta(&snap(1.5e15), 1.0e15, 12.0);
        assert!((d - 50.0).abs() < 1e-3, "got {}", d);
    }

    #[test]
    fn fifty_percent_underestimate_yields_fifty_pct() {
        let d = AbsoluteRatio.delta(&snap(0.5e15), 1.0e15, 12.0);
        assert!((d - 50.0).abs() < 1e-3, "got {}", d);
    }

    #[test]
    fn tenfold_overestimate_yields_nine_hundred_pct() {
        // 10× hashrate → |10H − H|/H × 100 = 900%
        let d = AbsoluteRatio.delta(&snap(1.0e16), 1.0e15, 12.0);
        assert!((d - 900.0).abs() < 1.0, "got {}", d);
    }
}
