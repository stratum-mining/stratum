//! Boundary trait — Axis 3 of the four-axis vardiff decomposition.
//!
//! The Boundary computes the threshold θ that the test statistic δ must
//! exceed for the algorithm to fire. It is the piece of the algorithm
//! that lives in decision theory: design pressure is Type I vs Type II
//! error rates, plus *rate-awareness* — the noise floor of δ depends on
//! share rate, so the threshold should too.
//!
//! ## The variance-vs-detection paradox lives here
//!
//! Classic and Parametric vardiff differ only in this axis. The classic
//! step-function threshold is share-rate-blind: it returns 15% past
//! `dt_secs ≥ 300`, regardless of how many shares per minute are flowing.
//! At high SPM the Poisson noise floor on δ is much lower than 15% and
//! the algorithm misses small real changes that it could detect. The
//! Parametric boundary fixes this by deriving θ from the Poisson CI for
//! the realized count under the null hypothesis. See `DESIGN.md` §
//! "Why these four axes" for the rate-awareness argument.

use std::fmt::Debug;

/// Axis 3: the decision threshold θ that the test statistic δ must
/// exceed to fire. Returns `f64` in *percentage points* (e.g., `60.0`
/// means δ must be at least 60%), matching the Statistic axis's
/// convention.
///
/// **Theory**: decision theory (Type I vs Type II error trade-off).
///
/// **Design pressure**: false-fire rate vs missed-detection rate, plus
/// rate-awareness — the noise floor of δ depends on share rate, so the
/// threshold should too.
///
/// Implementations: [`StepFunction`] (classic), [`PoissonCI`]
/// (Parametric / EWMA / Sliding-Window).
pub trait Boundary: Debug + Send + Sync {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32) -> f64;
}

/// A piecewise-constant threshold over `dt_secs`. Share-rate-blind.
///
/// Constructed via [`StepFunction::classic_table`] for byte-for-byte
/// equivalence with `VardiffState::try_vardiff`'s threshold cascade:
///
/// ```text
/// dt <  60s:  θ = 100%   (only very large δ fires; the >=100% short-circuit)
/// dt <  120s: θ =  60%
/// dt <  180s: θ =  50%
/// dt <  240s: θ =  45%
/// dt <  300s: θ =  30%
/// dt ≥  300s: θ =  15%
/// ```
///
/// To explore other step functions (e.g., flatter at high `dt_secs`),
/// construct `StepFunction { table: custom_table }` directly. The table
/// MUST be sorted ascending by `dt_threshold` and MUST have a final entry
/// with `dt_threshold == u64::MAX` so the function is defined for all
/// inputs.
#[derive(Debug, Clone)]
pub struct StepFunction {
    /// Sorted ascending by `dt_threshold`. For `dt_secs < dt_threshold`
    /// the function returns `value`. The last entry must have
    /// `dt_threshold == u64::MAX`.
    pub table: Vec<(u64, f64)>,
}

impl StepFunction {
    /// The classic threshold ladder from `VardiffState::try_vardiff`.
    pub fn classic_table() -> Self {
        Self {
            table: vec![
                (60, 100.0),
                (120, 60.0),
                (180, 50.0),
                (240, 45.0),
                (300, 30.0),
                (u64::MAX, 15.0),
            ],
        }
    }
}

impl Boundary for StepFunction {
    fn threshold(&self, dt_secs: u64, _shares_per_minute: f32) -> f64 {
        for &(threshold_dt, value) in &self.table {
            if dt_secs < threshold_dt {
                return value;
            }
        }
        // Unreachable when the table includes the required u64::MAX entry,
        // but defensive fallback rather than panic.
        self.table.last().map(|(_, v)| *v).unwrap_or(100.0)
    }
}

/// Parametric boundary: threshold derived from the Poisson confidence
/// interval on the realized share count under the null hypothesis (no
/// genuine change in miner hashrate).
///
/// Formula (in *percentage points*, matching the AbsoluteRatio statistic's
/// convention):
///
/// ```text
/// λ̄ = (SPM / 60) × Δt              (expected share count under H₀)
/// θ_fraction = (z·√λ̄ + 0.5) / λ̄ + margin
/// θ_pct = θ_fraction × 100
/// ```
///
/// The `z` coefficient gives the desired Type I error rate (e.g.,
/// `z = 2.576` for ~1% per-tick false-fire rate under H₀). The
/// `margin` term adds a flat slack above the Poisson floor — without
/// it the algorithm fires too readily when the statistic happens to
/// sit just above the boundary on small-variance trials. The default
/// `(z = 2.576, margin = 0.05)` is the parameterization used by the
/// `Parametric` and `FullRemedy` algorithms in the registry.
///
/// This boundary is the only axis where `Parametric` differs from
/// `ClassicComposed`; the other three axes (Estimator, Statistic,
/// Update) are unchanged.
#[derive(Debug, Clone, Copy)]
pub struct PoissonCI {
    /// Two-sided normal quantile. `2.576` ≈ 99% CI.
    pub z: f64,
    /// Additive margin in fractional form (e.g., `0.05` for +5%).
    pub margin: f64,
}

impl PoissonCI {
    /// The default Parametric parameters: `z = 2.576` (99% CI),
    /// `margin = 0.05` (+5%).
    pub fn default_parametric() -> Self {
        Self {
            z: 2.576,
            margin: 0.05,
        }
    }

    /// Construct with arbitrary `z` and `margin`. Use this to explore
    /// the Type I error frontier: e.g. `with_z(3.0, 0.05)` for a 99.7%
    /// CI ("3σ"), or `with_z(3.891, 0.05)` for 99.99%.
    ///
    /// **Why explore beyond the default?** The default `z = 2.576` gives
    /// a ~1% per-tick false-fire rate under H₀, which is the right
    /// trade-off when each tick is independent. But at very low share
    /// rates the per-tick Poisson tail is heavy enough that even
    /// 1-in-100 outliers cascade through the algorithm. See
    /// `sim/docs/FINDINGS.md` § "Parametric SPM=6 cascade" for the
    /// concrete failure mode that motivates exploring stricter z.
    pub fn with_z(z: f64, margin: f64) -> Self {
        Self { z, margin }
    }

    /// 99.7% CI ("3σ") preset: `z = 3.0`, default margin.
    /// Roughly 0.3% per-tick false-fire rate under H₀.
    pub fn strict_3sigma() -> Self {
        Self {
            z: 3.0,
            margin: 0.05,
        }
    }
}

impl Boundary for PoissonCI {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32) -> f64 {
        // Expected share count under H₀ over the window.
        let lambda_bar = (shares_per_minute as f64 / 60.0) * dt_secs as f64;
        if lambda_bar <= 0.0 {
            // Pathological — fall back to a very strict threshold so
            // the algorithm only fires on overwhelming evidence.
            return 100.0;
        }
        let bound_fraction = (self.z * lambda_bar.sqrt() + 0.5) / lambda_bar + self.margin;
        // The AbsoluteRatio statistic returns δ in percentage points;
        // convert the fractional bound to match.
        bound_fraction * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_table_matches_vardiff_state_cascade() {
        let b = StepFunction::classic_table();
        // Below the first boundary: 100% (only very large δ fires).
        assert_eq!(b.threshold(0, 12.0), 100.0);
        assert_eq!(b.threshold(15, 12.0), 100.0);
        assert_eq!(b.threshold(59, 12.0), 100.0);
        // Each subsequent rung.
        assert_eq!(b.threshold(60, 12.0), 60.0);
        assert_eq!(b.threshold(119, 12.0), 60.0);
        assert_eq!(b.threshold(120, 12.0), 50.0);
        assert_eq!(b.threshold(179, 12.0), 50.0);
        assert_eq!(b.threshold(180, 12.0), 45.0);
        assert_eq!(b.threshold(239, 12.0), 45.0);
        assert_eq!(b.threshold(240, 12.0), 30.0);
        assert_eq!(b.threshold(299, 12.0), 30.0);
        // Floor: 15% past dt = 300s.
        assert_eq!(b.threshold(300, 12.0), 15.0);
        assert_eq!(b.threshold(1_800, 12.0), 15.0);
        assert_eq!(b.threshold(u64::MAX - 1, 12.0), 15.0);
    }

    #[test]
    fn classic_threshold_is_share_rate_blind() {
        // The classic ladder ignores share rate — the same dt produces the
        // same threshold regardless of SPM. This is the property that
        // motivates the Parametric boundary (which IS rate-aware).
        let b = StepFunction::classic_table();
        for &spm in &[6.0f32, 12.0, 30.0, 60.0, 120.0] {
            assert_eq!(b.threshold(120, spm), 50.0);
            assert_eq!(b.threshold(300, spm), 15.0);
        }
    }

    // ---- PoissonCI ----

    #[test]
    fn poisson_ci_matches_reference_values() {
        // At dt=1200s, hand-computed reference values:
        //   SPM=12  → θ ≈ 0.218
        //   SPM=60  → θ ≈ 0.125
        //   SPM=120 → θ ≈ 0.103
        // (using z=2.576, margin=0.05; θ_fraction = (z·√λ̄ + 0.5)/λ̄ + margin)
        // Our boundary returns these × 100 (percentage points).
        let b = PoissonCI::default_parametric();
        let t12 = b.threshold(1200, 12.0);
        let t60 = b.threshold(1200, 60.0);
        let t120 = b.threshold(1200, 120.0);
        assert!((t12 - 21.8).abs() < 0.1, "SPM=12 got {}", t12);
        assert!((t60 - 12.5).abs() < 0.1, "SPM=60 got {}", t60);
        assert!((t120 - 10.3).abs() < 0.1, "SPM=120 got {}", t120);
    }

    #[test]
    fn poisson_ci_is_rate_aware() {
        // The defining property — unlike StepFunction. As SPM
        // increases the threshold strictly decreases (the noise floor
        // shrinks, so the algorithm can detect smaller real changes).
        let b = PoissonCI::default_parametric();
        let t6 = b.threshold(600, 6.0);
        let t12 = b.threshold(600, 12.0);
        let t60 = b.threshold(600, 60.0);
        let t120 = b.threshold(600, 120.0);
        assert!(t6 > t12, "{} not > {}", t6, t12);
        assert!(t12 > t60);
        assert!(t60 > t120);
    }

    #[test]
    fn poisson_ci_returns_strict_threshold_on_degenerate_inputs() {
        let b = PoissonCI::default_parametric();
        assert_eq!(b.threshold(0, 12.0), 100.0); // dt = 0 → λ̄ = 0
        assert_eq!(b.threshold(60, 0.0), 100.0); // SPM = 0 → λ̄ = 0
    }

    #[test]
    fn poisson_ci_strict_3sigma_returns_higher_threshold_than_default() {
        // The strict variant (z=3.0) sits above the default (z=2.576)
        // at every λ̄ — that's the whole point: trade a higher
        // missed-detection rate for a tighter false-fire rate. Holds
        // across share rates.
        let default = PoissonCI::default_parametric();
        let strict = PoissonCI::strict_3sigma();
        for &spm in &[6.0f32, 12.0, 30.0, 60.0, 120.0] {
            for &dt in &[60u64, 300, 600, 1200] {
                let d = default.threshold(dt, spm);
                let s = strict.threshold(dt, spm);
                assert!(
                    s > d,
                    "strict ({}) should be > default ({}) at dt={}, spm={}",
                    s,
                    d,
                    dt,
                    spm,
                );
            }
        }
    }

    #[test]
    fn poisson_ci_with_z_matches_strict_3sigma_preset() {
        let a = PoissonCI::strict_3sigma();
        let b = PoissonCI::with_z(3.0, 0.05);
        assert_eq!(a.z, b.z);
        assert_eq!(a.margin, b.margin);
        for &(dt, spm) in &[(60u64, 12.0f32), (600, 60.0), (1800, 120.0)] {
            assert_eq!(a.threshold(dt, spm), b.threshold(dt, spm));
        }
    }
}
