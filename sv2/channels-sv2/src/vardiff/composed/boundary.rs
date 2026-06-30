//! Boundary — Stage 2 of the three-stage vardiff pipeline.
//!
//! The Boundary computes the threshold θ that the deviation δ must
//! exceed for the algorithm to fire. It is the piece of the algorithm
//! that lives in decision theory: design pressure is Type I vs Type II
//! error rates, plus *rate-awareness* — the noise floor of δ depends on
//! share rate, so the threshold should too.
//!
//! The Boundary receives the full `EstimatorSnapshot` including optional
//! uncertainty, enabling uncertainty-aware implementations that adapt
//! their threshold based on estimator confidence.

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU64, Ordering};

use super::estimator::EstimatorSnapshot;

/// The decision threshold θ that the deviation δ must exceed to fire.
/// Returns `f64` in percentage points (e.g., `60.0` means δ must be at
/// least 60%).
///
/// Implementations: [`StepFunction`] (classic), [`PoissonCI`]
/// (rate-aware), [`CredibleIntervalBoundary`] (uncertainty-aware),
/// [`CusumBoundary`] (sequential-testing).
pub trait Boundary: Debug + Send + Sync {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64;

    /// A short, drift-proof code derived from the boundary's actual
    /// parameters. See [`super::Estimator::code`] for the rationale.
    fn code(&self) -> String;
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
    fn threshold(&self, dt_secs: u64, _shares_per_minute: f32, _snap: &EstimatorSnapshot) -> f64 {
        for &(threshold_dt, value) in &self.table {
            if dt_secs < threshold_dt {
                return value;
            }
        }
        // Unreachable when the table includes the required u64::MAX entry,
        // but defensive fallback rather than panic.
        self.table.last().map(|(_, v)| *v).unwrap_or(100.0)
    }

    fn code(&self) -> String {
        "Step".to_string()
    }
}

/// Parametric boundary: threshold derived from the Poisson confidence
/// interval on the realized share count under the null hypothesis (no
/// genuine change in miner hashrate).
///
/// Formula (in *percentage points*, matching the deviation
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
/// This boundary is the only stage where `Parametric` differs from
/// `ClassicComposed`; Estimator and UpdateRule are unchanged.
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
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, _snap: &EstimatorSnapshot) -> f64 {
        // Expected share count under H₀ over the window.
        let lambda_bar = (shares_per_minute as f64 / 60.0) * dt_secs as f64;
        if lambda_bar <= 0.0 {
            // Pathological — fall back to a very strict threshold so
            // the algorithm only fires on overwhelming evidence.
            return 100.0;
        }
        let bound_fraction = (self.z * lambda_bar.sqrt() + 0.5) / lambda_bar + self.margin;
        // The deviation δ is in percentage points;
        // convert the fractional bound to match.
        bound_fraction * 100.0
    }

    fn code(&self) -> String {
        format!("Poisson-z{:.2}", self.z)
    }
}

/// Credible-interval boundary: fires when the estimator's posterior
/// credible interval excludes ratio = 1.0, meaning the algorithm is
/// confident that the miner's hashrate differs from the current target.
///
/// This boundary uses the estimator's reported `uncertainty.ratio_std`
/// directly, rather than inferring noise from dt_secs or SPM. This
/// makes it self-calibrating: a confident estimate (many ticks of data,
/// small ratio_std) triggers on small deviations; an uncertain estimate
/// (fresh after fire, large ratio_std) requires large deviations.
///
/// ## Formula
///
/// The deviation δ = |ratio - 1| × 100 (percentage points).
/// The boundary computes:
///
/// ```text
/// θ = z × ratio_std × 100
/// ```
///
/// Fire iff δ ≥ θ, i.e., the deviation is at least z standard deviations
/// from ratio=1.0. With z=1.96, this corresponds to a 95% credible
/// interval excluding 1.0.
///
/// ## Fallback
///
/// When `snap.uncertainty` is `None` (estimator doesn't report confidence),
/// falls back to PoissonCI behavior. This ensures the boundary works with
/// all estimators, not just Bayesian.
///
/// ## Parameters
///
/// - `z`: number of standard deviations required (1.96 = 95% CI, 2.576 = 99%)
/// - `fallback`: PoissonCI used when uncertainty is unavailable
#[derive(Debug, Clone, Copy)]
pub struct CredibleIntervalBoundary {
    /// Z-score for the credible interval (e.g., 1.96 for 95% CI).
    pub z: f64,
    /// Fallback boundary for estimators that don't report uncertainty.
    pub fallback: PoissonCI,
}

impl CredibleIntervalBoundary {
    /// 95% credible interval boundary.
    pub fn new_95() -> Self {
        Self {
            z: 1.96,
            fallback: PoissonCI::default_parametric(),
        }
    }

    /// Custom z-score credible interval boundary.
    pub fn with_z(z: f64) -> Self {
        Self {
            z,
            fallback: PoissonCI::default_parametric(),
        }
    }
}

impl Boundary for CredibleIntervalBoundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        match &snap.uncertainty {
            Some(u) if u.ratio_std > 0.0 && u.effective_n > 0.0 => {
                // Fire when |ratio - 1.0| > z × ratio_std
                // In percentage points (matching deviation convention):
                self.z * u.ratio_std * 100.0
            }
            _ => {
                // No uncertainty available — delegate to PoissonCI
                self.fallback.threshold(dt_secs, shares_per_minute, snap)
            }
        }
    }

    fn code(&self) -> String {
        format!("CredInt-z{:.2}", self.z)
    }
}

/// Sequential-testing boundary inspired by CUSUM (Cumulative Sum).
///
/// Unlike PoissonCI (which treats each tick independently), this boundary
/// accounts for the fact that evidence accumulates over ticks. A genuine
/// hashrate change produces *persistent* deviations across all subsequent
/// ticks, not just one spike. The threshold shrinks faster with dt_secs
/// than PoissonCI, making it more sensitive to sustained changes.
///
/// ## Formula
///
/// ```text
/// n_ticks = dt_secs / tick_secs
/// θ = (sensitivity / n_ticks + floor) × 100
/// ```
///
/// Where:
/// - `sensitivity`: how many ticks of sustained deviation needed to fire
///   (analogous to CUSUM's h/k ratio). Lower = more sensitive.
/// - `floor`: minimum threshold as a fraction (prevents firing on tiny
///   deviations even with many ticks). Acts as the "slack" k in CUSUM.
///
/// The key difference from PoissonCI: PoissonCI threshold decreases as
/// 1/√n (Poisson CI width). CusumBoundary threshold decreases as 1/n
/// (linear evidence accumulation). At n=1 tick they're similar; at n=10
/// ticks CUSUM is much tighter.
///
/// ## When uncertainty is available
///
/// If the estimator reports uncertainty, the floor is scaled by ratio_std
/// to avoid firing when the estimate itself is uncertain.
#[derive(Debug, Clone, Copy)]
pub struct CusumBoundary {
    /// Ticks of sustained deviation needed to fire at maximum sensitivity.
    /// Lower = fires faster. Typical range: 2.0–5.0.
    pub sensitivity: f64,
    /// Minimum fractional threshold regardless of accumulated evidence.
    /// Prevents firing on tiny deviations. Typical: 0.03–0.10 (3–10%).
    pub floor: f64,
    /// Tick interval for converting dt_secs to tick count.
    pub tick_secs: u64,
}

impl CusumBoundary {
    pub fn new(sensitivity: f64, floor: f64) -> Self {
        Self {
            sensitivity,
            floor,
            tick_secs: 60,
        }
    }
}

/// Rate-adaptive CUSUM: sensitivity scales with share rate so the
/// boundary is appropriately conservative at low SPM (where Poisson
/// noise is high) and aggressive at high SPM (where noise is low).
///
/// ```text
/// effective_sensitivity = base_sensitivity × √(SPM / reference_spm)
/// ```
///
/// At reference_spm, effective_sensitivity = base_sensitivity. At lower
/// SPM it's smaller (tighter → more conservative, less jitter). At higher
/// SPM it's larger (looser → but the EWMA estimate is better anyway).
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveCusumBoundary {
    pub base_sensitivity: f64,
    pub reference_spm: f64,
    pub floor: f64,
    pub tick_secs: u64,
}

impl AdaptiveCusumBoundary {
    pub fn new(base_sensitivity: f64, floor: f64) -> Self {
        Self {
            base_sensitivity,
            reference_spm: 30.0,
            floor,
            tick_secs: 60,
        }
    }
}

impl Boundary for AdaptiveCusumBoundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        let n_ticks = (dt_secs as f64 / self.tick_secs as f64).max(1.0);

        // Rate-adaptive sensitivity: more conservative at low SPM
        let spm_factor = ((shares_per_minute as f64) / self.reference_spm).sqrt();
        let sensitivity = self.base_sensitivity * spm_factor;

        let effective_floor = match &snap.uncertainty {
            Some(u) if u.ratio_std > 0.0 => self.floor + u.ratio_std * 0.5,
            _ => self.floor,
        };

        let threshold_fraction = (sensitivity / n_ticks) + effective_floor;
        threshold_fraction * 100.0
    }

    fn code(&self) -> String {
        format!("AdaptCusum-s{}-f{}", self.base_sensitivity, self.floor)
    }
}

/// Asymmetric rate-adaptive CUSUM: uses different thresholds for
/// tightening (miner over-performing) vs easing (miner under-performing).
///
/// **Rationale:** Making difficulty harder costs ~1 rejected share per fire
/// (in-flight work becomes invalid). Making difficulty easier costs nothing
/// (old harder work is still valid). Therefore the boundary should:
/// - Fire **quickly** to ease difficulty (miner slowing → detect fast, free action)
/// - Fire **cautiously** to tighten difficulty (miner speeding → avoid rejecting in-flight shares)
///
/// Direction is determined from the snapshot: if `realized_share_per_min > shares_per_minute`,
/// the miner is over-performing and firing would tighten (costly direction).
///
/// The `tighten_multiplier` (>1.0) makes the threshold higher when tightening,
/// requiring more evidence before making difficulty harder.
#[derive(Debug, Clone, Copy)]
pub struct AsymmetricCusumBoundary {
    /// Base sensitivity (same as AdaptiveCusumBoundary).
    pub base_sensitivity: f64,
    /// Reference SPM for rate scaling.
    pub reference_spm: f64,
    /// Minimum threshold floor.
    pub floor: f64,
    /// Multiplier applied to threshold when firing would TIGHTEN difficulty.
    /// Values > 1.0 make tightening more conservative. Typical: 1.5–3.0.
    pub tighten_multiplier: f64,
    /// Tick interval.
    pub tick_secs: u64,
}

impl AsymmetricCusumBoundary {
    /// Constructs with default reference_spm=30 and tick_secs=60.
    /// `tighten_multiplier` controls the asymmetry: 1.0 = symmetric,
    /// 2.0 = requires 2× more evidence to tighten than to ease.
    pub fn new(base_sensitivity: f64, floor: f64, tighten_multiplier: f64) -> Self {
        Self {
            base_sensitivity,
            reference_spm: 30.0,
            floor,
            tighten_multiplier: tighten_multiplier.max(1.0),
            tick_secs: 60,
        }
    }
}

impl Boundary for AsymmetricCusumBoundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        let n_ticks = (dt_secs as f64 / self.tick_secs as f64).max(1.0);

        let spm_factor = ((shares_per_minute as f64) / self.reference_spm).sqrt();
        let sensitivity = self.base_sensitivity * spm_factor;

        let effective_floor = match &snap.uncertainty {
            Some(u) if u.ratio_std > 0.0 => self.floor + u.ratio_std * 0.5,
            _ => self.floor,
        };

        let base_threshold = (sensitivity / n_ticks) + effective_floor;

        // Determine direction: is the miner over-performing (tighten) or under-performing (ease)?
        // realized_spm > configured_spm means miner is faster → fire would tighten → costly
        let would_tighten = snap.realized_share_per_min > shares_per_minute as f64;

        let threshold_fraction = if would_tighten {
            base_threshold * self.tighten_multiplier
        } else {
            base_threshold
        };

        threshold_fraction * 100.0
    }

    fn code(&self) -> String {
        format!(
            "AsymCusum-s{}-f{}-t{}",
            self.base_sensitivity, self.floor, self.tighten_multiplier
        )
    }
}

/// Sign-persistence CUSUM: an asymmetric rate-adaptive boundary that
/// additionally tracks consecutive ticks where the deviation has the same
/// sign and lowers the threshold when the sign has been consistent for
/// many ticks.
///
/// **Insight (PID integral term analogy):** A persistent sub-threshold drift
/// in one direction — even if each individual tick is below the firing
/// threshold — represents strong evidence of a genuine hashrate change.
/// This boundary captures that insight: after `n` consecutive same-sign
/// ticks the threshold is reduced by `sign_persistence_discount × (n-1)`,
/// capped at `max_sign_discount`.
///
/// ## State tracking
///
/// The `Boundary` trait takes `&self` (immutable). To track sign persistence
/// across calls, this struct uses atomic types with `Ordering::Relaxed`.
/// This is safe because vardiff evaluation is single-threaded per channel;
/// the atomics are merely an interior-mutability mechanism, not a
/// cross-thread synchronization primitive.
pub struct SignPersistenceCusumBoundary {
    /// Base sensitivity (same as AsymmetricCusumBoundary).
    pub base_sensitivity: f64,
    /// Reference SPM for rate scaling.
    pub reference_spm: f64,
    /// Minimum threshold floor.
    pub floor: f64,
    /// Multiplier applied when firing would TIGHTEN difficulty.
    pub tighten_multiplier: f64,
    /// Tick interval.
    pub tick_secs: u64,
    /// Threshold reduction per consecutive same-sign tick (fractional).
    /// E.g., 0.02 = 2% reduction per tick.
    pub sign_persistence_discount: f64,
    /// Maximum total discount (fractional cap).
    /// E.g., 0.4 = at most 40% threshold reduction.
    pub max_sign_discount: f64,
    /// Number of consecutive ticks with the same sign.
    consecutive_count: AtomicU32,
    /// Last observed sign: +1 (over-performing), -1 (under-performing), 0 (unset).
    last_sign: AtomicI8,
}

impl SignPersistenceCusumBoundary {
    /// Constructs with default reference_spm=30 and tick_secs=60.
    pub fn new(
        base_sensitivity: f64,
        floor: f64,
        tighten_multiplier: f64,
        sign_persistence_discount: f64,
        max_sign_discount: f64,
    ) -> Self {
        Self {
            base_sensitivity,
            reference_spm: 30.0,
            floor,
            tighten_multiplier: tighten_multiplier.max(1.0),
            tick_secs: 60,
            sign_persistence_discount,
            max_sign_discount,
            consecutive_count: AtomicU32::new(0),
            last_sign: AtomicI8::new(0),
        }
    }
}

impl Debug for SignPersistenceCusumBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignPersistenceCusumBoundary")
            .field("base_sensitivity", &self.base_sensitivity)
            .field("reference_spm", &self.reference_spm)
            .field("floor", &self.floor)
            .field("tighten_multiplier", &self.tighten_multiplier)
            .field("tick_secs", &self.tick_secs)
            .field("sign_persistence_discount", &self.sign_persistence_discount)
            .field("max_sign_discount", &self.max_sign_discount)
            .field(
                "consecutive_count",
                &self.consecutive_count.load(Ordering::Relaxed),
            )
            .field("last_sign", &self.last_sign.load(Ordering::Relaxed))
            .finish()
    }
}

impl Boundary for SignPersistenceCusumBoundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        // Step 1: Compute the base threshold exactly like AsymmetricCusumBoundary.
        let n_ticks = (dt_secs as f64 / self.tick_secs as f64).max(1.0);

        let spm_factor = ((shares_per_minute as f64) / self.reference_spm).sqrt();
        let sensitivity = self.base_sensitivity * spm_factor;

        let effective_floor = match &snap.uncertainty {
            Some(u) if u.ratio_std > 0.0 => self.floor + u.ratio_std * 0.5,
            _ => self.floor,
        };

        let base_threshold_fraction = (sensitivity / n_ticks) + effective_floor;

        // Apply asymmetric tighten multiplier.
        let would_tighten = snap.realized_share_per_min > shares_per_minute as f64;
        let asymmetric_threshold = if would_tighten {
            base_threshold_fraction * self.tighten_multiplier
        } else {
            base_threshold_fraction
        };

        // Step 2: Determine current sign.
        let current_sign: i8 = if snap.realized_share_per_min > shares_per_minute as f64 {
            1
        } else {
            -1
        };

        // Step 3: Update sign persistence state.
        let stored_sign = self.last_sign.load(Ordering::Relaxed);
        let consecutive = if current_sign == stored_sign {
            self.consecutive_count.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.last_sign.store(current_sign, Ordering::Relaxed);
            self.consecutive_count.store(1, Ordering::Relaxed);
            1
        };

        // Step 4: Compute discount from sign persistence.
        let discount =
            (self.sign_persistence_discount * (consecutive - 1) as f64).min(self.max_sign_discount);

        // Step 5: Apply discount and convert to percentage points.
        asymmetric_threshold * (1.0 - discount) * 100.0
    }

    fn code(&self) -> String {
        // Include params so sweep variants get distinct (drift-proof) names.
        format!(
            "SignPersist-s{}-f{}-t{}-d{}-dm{}",
            self.base_sensitivity,
            self.floor,
            self.tighten_multiplier,
            self.sign_persistence_discount,
            self.max_sign_discount
        )
    }
}

impl Boundary for CusumBoundary {
    fn threshold(&self, dt_secs: u64, _shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        let n_ticks = (dt_secs as f64 / self.tick_secs as f64).max(1.0);

        // Base floor — scaled by uncertainty if available
        let effective_floor = match &snap.uncertainty {
            Some(u) if u.ratio_std > 0.0 => self.floor + u.ratio_std * 0.5,
            _ => self.floor,
        };

        // Threshold decreases as 1/n_ticks (sequential evidence accumulation)
        let threshold_fraction = (self.sensitivity / n_ticks) + effective_floor;

        // Convert to percentage points (matching deviation convention)
        threshold_fraction * 100.0
    }

    fn code(&self) -> String {
        format!("Cusum-s{}-f{}", self.sensitivity, self.floor)
    }
}

/// Hysteresis-gate boundary inspired by ckpool's retarget decision logic.
///
/// ckpool does NOT use a continuous threshold; instead it applies two
/// sequential gates:
///
/// 1. **Data gate**: suppress evaluation entirely until either enough
///    shares have been observed OR enough time has elapsed. This prevents
///    decisions on statistically meaningless data.
///
/// 2. **Hysteresis band**: once the data gate is satisfied, check whether
///    the diff-rate-ratio (drr = realized_rate / current_diff) falls
///    within a dead band around the target. If inside the band → no fire.
///    If outside → fire.
///
/// In the three-stage framework, this maps to a boundary that returns
/// `f64::MAX` (never fire) when either gate suppresses, and a threshold
/// of 0.0 (always fire) when outside the hysteresis band. The binary
/// fire/don't-fire semantics are preserved exactly.
///
/// ## Parameters
///
/// - `min_shares`: minimum shares since last fire before evaluating
///   (ckpool: 72, derived from `window_secs / target_period`)
/// - `min_time_secs`: minimum seconds since last fire before evaluating
///   (ckpool: 240)
/// - `hysteresis_low`: lower bound multiplier on target rate (ckpool: 0.5)
///   — drr below `target × hysteresis_low` → fire (decrease difficulty)
/// - `hysteresis_high`: upper bound multiplier on target rate (ckpool: 1.33)
///   — drr above `target × hysteresis_high` → fire (increase difficulty)
///
/// The data gate uses OR semantics (fire if EITHER threshold is met),
/// matching ckpool's `if (ssdc < 72 && tdiff < 240) return`.
#[derive(Debug, Clone, Copy)]
pub struct HysteresisGate {
    /// Minimum shares before evaluating. ckpool default: 72.
    pub min_shares: u32,
    /// Minimum seconds before evaluating. ckpool default: 240.
    pub min_time_secs: u64,
    /// Lower hysteresis multiplier (fire below this). ckpool: 0.5.
    pub hysteresis_low: f64,
    /// Upper hysteresis multiplier (fire above this). ckpool: 1.33.
    pub hysteresis_high: f64,
}

impl HysteresisGate {
    /// Construct with ckpool's default parameters.
    pub fn ckpool_defaults() -> Self {
        Self {
            min_shares: 72,
            min_time_secs: 240,
            hysteresis_low: 0.5,
            hysteresis_high: 1.33,
        }
    }

    /// Construct with custom parameters.
    pub fn new(
        min_shares: u32,
        min_time_secs: u64,
        hysteresis_low: f64,
        hysteresis_high: f64,
    ) -> Self {
        Self {
            min_shares,
            min_time_secs,
            hysteresis_low,
            hysteresis_high,
        }
    }
}

impl Boundary for HysteresisGate {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        // Gate 1: insufficient data — suppress fire.
        // OR semantics: proceed if EITHER min_shares OR min_time is met.
        let shares_met = snap.n_shares >= self.min_shares;
        let time_met = dt_secs >= self.min_time_secs;
        if !shares_met && !time_met {
            return f64::MAX;
        }

        // Gate 2: hysteresis band check.
        // Compute the rate ratio: realized_spm / target_spm.
        // If within the dead band [low, high] around 1.0 → suppress.
        if shares_per_minute > 0.0 {
            let rate_ratio = snap.realized_share_per_min / shares_per_minute as f64;
            if rate_ratio >= self.hysteresis_low && rate_ratio <= self.hysteresis_high {
                return f64::MAX;
            }
        }

        // Outside the hysteresis band with sufficient data → always fire.
        0.0
    }

    fn code(&self) -> String {
        format!(
            "Hyst-{}-{}-g{}",
            self.hysteresis_low, self.hysteresis_high, self.min_shares
        )
    }
}

/// Asymmetric PoissonCI: applies a tighten multiplier when the miner is
/// over-performing (would tighten difficulty), making it harder to
/// increase difficulty than to decrease it.
///
/// Uses the same Poisson confidence interval formula as [`PoissonCI`] but
/// multiplies the threshold by `tighten_multiplier` when the direction of
/// the deviation would tighten difficulty (reject in-flight shares).
#[derive(Debug, Clone, Copy)]
pub struct AsymmetricPoissonCI {
    pub poisson: PoissonCI,
    pub tighten_multiplier: f64,
}

impl AsymmetricPoissonCI {
    pub fn new(z: f64, margin: f64, tighten_multiplier: f64) -> Self {
        Self {
            poisson: PoissonCI::with_z(z, margin),
            tighten_multiplier: tighten_multiplier.max(1.0),
        }
    }
}

impl Boundary for AsymmetricPoissonCI {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        let base = self.poisson.threshold(dt_secs, shares_per_minute, snap);
        let would_tighten = snap.realized_share_per_min > shares_per_minute as f64;
        if would_tighten {
            base * self.tighten_multiplier
        } else {
            base
        }
    }

    fn code(&self) -> String {
        format!(
            "AsymPoisson-z{:.2}-t{}",
            self.poisson.z, self.tighten_multiplier
        )
    }
}

/// Volatility-adaptive boundary: a PoissonCI floor scaled by *recently
/// observed* share-rate volatility relative to the Poisson expectation.
///
/// Motivation: the sweeps showed react−10% is bounded by a hard tradeoff —
/// a fixed boundary must choose between tight (good react−10%, bad jitter)
/// and loose (good jitter, bad react−10%). This boundary breaks the tradeoff
/// by adapting *per miner over time*: when the share stream is behaving like
/// clean Poisson noise (calm), it tightens toward the PoissonCI floor so a
/// genuine 10% drop is detected fast; when the stream is genuinely volatile
/// (the miner's rate is wandering), it loosens to avoid firing on noise.
///
/// ## State (interior mutability)
///
/// Two EWMAs over the per-tick realized rate, updated on every `threshold`
/// call (one call per tick):
/// - `mean_bits`: EWMA of realized shares-per-minute → the smoothed rate μ.
/// - `var_bits`: EWMA of the squared residual `(realized − μ)²` → observed
///   variance σ²_obs.
///
/// The *expected* per-tick Poisson variance of the share count, expressed in
/// the same (shares-per-minute)² units, is `λ̄ · (60/dt)²` where
/// `λ̄ = SPM·dt/60` is the expected count. The **excess-volatility factor**
///
/// ```text
/// vf = clamp( σ²_obs / σ²_poisson , 1.0 , vf_max )
/// ```
///
/// is ≈1 when the stream is pure Poisson and grows when the rate is
/// genuinely moving. The threshold is the PoissonCI floor times `vf`:
///
/// ```text
/// θ = poisson_ci(dt, SPM) · vf
/// ```
///
/// `alpha` is the EWMA smoothing in (0,1] (lower = longer memory); `vf_max`
/// caps how far volatility can loosen the boundary.
#[derive(Debug)]
pub struct VolatilityAdaptiveBoundary {
    /// Underlying rate-aware floor.
    pub poisson: PoissonCI,
    /// EWMA smoothing factor for the mean/variance trackers, in (0, 1].
    pub alpha: f64,
    /// Maximum volatility loosening multiplier (≥ 1.0).
    pub vf_max: f64,
    /// Tick interval in seconds (matches the trial driver cadence).
    pub tick_secs: u64,
    /// EWMA of realized shares-per-minute (μ), as f64 bits. NaN = uninit.
    mean_bits: AtomicU64,
    /// EWMA of squared residual (σ²_obs), as f64 bits. NaN = uninit.
    var_bits: AtomicU64,
}

impl Clone for VolatilityAdaptiveBoundary {
    fn clone(&self) -> Self {
        Self {
            poisson: self.poisson,
            alpha: self.alpha,
            vf_max: self.vf_max,
            tick_secs: self.tick_secs,
            mean_bits: AtomicU64::new(self.mean_bits.load(Ordering::Relaxed)),
            var_bits: AtomicU64::new(self.var_bits.load(Ordering::Relaxed)),
        }
    }
}

impl VolatilityAdaptiveBoundary {
    /// Construct with the default PoissonCI floor (z=2.576, margin=0.05).
    /// `alpha` is the EWMA smoothing (e.g. 0.2 ≈ 5-tick memory); `vf_max`
    /// caps the loosening (e.g. 4.0).
    pub fn new(alpha: f64, vf_max: f64) -> Self {
        Self {
            poisson: PoissonCI::default_parametric(),
            alpha: alpha.clamp(1e-3, 1.0),
            vf_max: vf_max.max(1.0),
            tick_secs: 60,
            mean_bits: AtomicU64::new(f64::NAN.to_bits()),
            var_bits: AtomicU64::new(f64::NAN.to_bits()),
        }
    }

    pub fn with_poisson(poisson: PoissonCI, alpha: f64, vf_max: f64) -> Self {
        let mut b = Self::new(alpha, vf_max);
        b.poisson = poisson;
        b
    }

    /// Update the EWMA mean/variance trackers with this tick's realized rate
    /// and return the current excess-volatility factor in [1, vf_max].
    fn update_volatility_factor(&self, realized_spm: f64, expected_var_spm2: f64) -> f64 {
        let prev_mean = f64::from_bits(self.mean_bits.load(Ordering::Relaxed));
        let prev_var = f64::from_bits(self.var_bits.load(Ordering::Relaxed));

        // First observation: seed mean to the realized rate, variance to the
        // Poisson expectation (factor starts neutral at ~1.0).
        let (mean, var) = if prev_mean.is_nan() {
            (realized_spm, expected_var_spm2.max(1e-9))
        } else {
            let residual = realized_spm - prev_mean;
            let mean = prev_mean + self.alpha * residual;
            let var = (1.0 - self.alpha) * prev_var + self.alpha * residual * residual;
            (mean, var)
        };
        self.mean_bits.store(mean.to_bits(), Ordering::Relaxed);
        self.var_bits.store(var.to_bits(), Ordering::Relaxed);

        if expected_var_spm2 <= 0.0 {
            return 1.0;
        }
        (var / expected_var_spm2).clamp(1.0, self.vf_max)
    }
}

impl Boundary for VolatilityAdaptiveBoundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        let base = self.poisson.threshold(dt_secs, shares_per_minute, snap);

        // Expected count and its Poisson variance, converted to (SPM)² units.
        // count ~ Poisson(λ̄), Var[count] = λ̄. realized_spm = count·60/dt, so
        // Var[realized_spm] = λ̄·(60/dt)².
        let dt = dt_secs.max(1) as f64;
        let lambda_bar = (shares_per_minute as f64 / 60.0) * dt;
        if lambda_bar <= 0.0 {
            return base;
        }
        let scale = 60.0 / dt;
        let expected_var_spm2 = lambda_bar * scale * scale;

        let vf = self.update_volatility_factor(snap.realized_share_per_min, expected_var_spm2);
        base * vf
    }

    fn code(&self) -> String {
        format!(
            "VolAdapt-z{:.2}-a{}-vf{}",
            self.poisson.z, self.alpha, self.vf_max
        )
    }
}

/// Cold-start warm-up wrapper for any inner boundary `B`.
///
/// Until the algorithm first converges, this returns threshold `0` — fire
/// on any deviation — so cold start retargets aggressively (paired with a
/// full/accelerating update rule it reaches truth in a couple of fires).
/// Once the realized share rate first lands within `converge_band` of the
/// configured target (i.e. `|realized/target − 1| ≤ converge_band`), the
/// wrapper latches "warmed up" PERMANENTLY and from then on delegates
/// entirely to the inner boundary `B`.
///
/// Rationale: the cautious, tighten-reluctant steady-state policy exists to
/// avoid the death spiral — raising difficulty into a genuinely failing
/// miner and starving it of valid work. On a FRESH connection there is no
/// in-flight work worth protecting and no established baseline to spiral
/// away from, so that caution only costs ramp-up time. The simulation's
/// oracle (the τ-matched estimator with no policy) converges in ~2 min,
/// while the cautious champion takes ~15 min — i.e. ~13 min of the ramp is
/// policy-imposed, not estimator-imposed, and recoverable here. After the
/// one-time warm-up the full steady-state safety is in force.
///
/// The latch is one-way (cold start happens once per connection); a later
/// genuine drop is handled by the inner boundary, not by re-entering
/// warm-up.
pub struct WarmupBoundary<B: Boundary> {
    /// The steady-state boundary, active after warm-up completes.
    pub inner: B,
    /// Convergence band (fractional): warm-up ends the first time
    /// `|realized/target − 1| ≤ converge_band`. E.g. 0.15 = within 15%.
    pub converge_band: f64,
    /// One-way latch: false during warm-up, true once converged.
    warmed: AtomicBool,
}

impl<B: Boundary> WarmupBoundary<B> {
    /// Wrap `inner` with a cold-start warm-up that ends once the realized
    /// rate first lands within `converge_band` of target.
    pub fn new(inner: B, converge_band: f64) -> Self {
        Self {
            inner,
            converge_band: converge_band.max(0.0),
            warmed: AtomicBool::new(false),
        }
    }
}

impl<B: Boundary + Clone> Clone for WarmupBoundary<B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            converge_band: self.converge_band,
            warmed: AtomicBool::new(self.warmed.load(Ordering::Relaxed)),
        }
    }
}

impl<B: Boundary> Debug for WarmupBoundary<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmupBoundary")
            .field("inner", &self.inner)
            .field("converge_band", &self.converge_band)
            .field("warmed", &self.warmed.load(Ordering::Relaxed))
            .finish()
    }
}

impl<B: Boundary> Boundary for WarmupBoundary<B> {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        if self.warmed.load(Ordering::Relaxed) {
            return self.inner.threshold(dt_secs, shares_per_minute, snap);
        }
        // Still warming. Check whether we've converged yet: realized rate
        // within converge_band of target. Use realized (not h_estimate) so
        // the test reflects observed performance, matching how the boundary
        // sees deviation.
        let target = shares_per_minute as f64;
        if target > 0.0 {
            let dev = (snap.realized_share_per_min / target - 1.0).abs();
            if dev <= self.converge_band {
                // Latch warmed; from the NEXT tick on we delegate. This tick
                // still fires aggressively (threshold 0) to land the final
                // convergence step.
                self.warmed.store(true, Ordering::Relaxed);
            }
        }
        // Fire on any deviation during warm-up.
        0.0
    }

    fn code(&self) -> String {
        format!("Warmup-b{}[{}]", self.converge_band, self.inner.code())
    }
}

/// Dual-mode boundary: PoissonCI (conservative) below `spm_threshold`, an
/// arbitrary aggressive boundary `B` at or above it, switched on the miner's
/// configured shares-per-minute rate.
///
/// Below `spm_threshold`, PoissonCI's wide confidence interval prevents
/// premature fires on sparse data (bitaxe / small miners) — sparse windows
/// produce noisy residual runs that destabilize sequential boundaries. At or
/// above the threshold, the data-rich boundary `B` enables fast reaction with
/// abundant evidence (large hashrate miners).
///
/// This mimics ckpool's dual-window strategy at the boundary layer:
/// conservative when data is sparse, aggressive when data is abundant. `B` is
/// generic so the same low-SPM guard protects any high-SPM boundary —
/// AsymmetricCusum (see [`AdaptivePoissonCusum`]) or SignPersistence (see
/// [`AdaptiveSignPersist`]), both of which collapse at low SPM without it.
#[derive(Debug)]
pub struct AdaptiveBoundary<B: Boundary> {
    /// Conservative boundary for the sparse-data regime.
    pub poisson: PoissonCI,
    /// Aggressive boundary for the data-rich regime.
    pub high: B,
    /// Configured SPM below which PoissonCI is used. At or above this, `high`
    /// activates.
    pub spm_threshold: u32,
}

impl<B: Boundary> AdaptiveBoundary<B> {
    /// Construct with an explicit PoissonCI floor and high-SPM boundary.
    pub fn with_high(poisson: PoissonCI, high: B, spm_threshold: u32) -> Self {
        Self {
            poisson,
            high,
            spm_threshold,
        }
    }
}

impl<B: Boundary> Boundary for AdaptiveBoundary<B> {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        if (shares_per_minute as u32) < self.spm_threshold {
            self.poisson.threshold(dt_secs, shares_per_minute, snap)
        } else {
            self.high.threshold(dt_secs, shares_per_minute, snap)
        }
    }

    fn code(&self) -> String {
        // Include the inner Poisson and high-boundary codes so two adaptive
        // boundaries differing only in inner params get distinct names. Omit
        // the Poisson code when it's the default, to keep the common case
        // readable. The high boundary's own code() carries its identity, so
        // CUSUM vs SignPersist is automatically distinguished here.
        let default_poisson = PoissonCI::default_parametric();
        let poisson_is_default =
            self.poisson.z == default_poisson.z && self.poisson.margin == default_poisson.margin;
        if poisson_is_default {
            format!("Adapt-spm{}[{}]", self.spm_threshold, self.high.code())
        } else {
            format!(
                "Adapt-spm{}[{}|{}]",
                self.spm_threshold,
                self.poisson.code(),
                self.high.code()
            )
        }
    }
}

/// Back-compat alias: the original PoissonCI ⇄ AsymmetricCusum dual-mode
/// boundary, now a specialization of [`AdaptiveBoundary`]. Existing callers
/// keep using `AdaptivePoissonCusum::new` / `::with_params`.
pub type AdaptivePoissonCusum = AdaptiveBoundary<AsymmetricCusumBoundary>;

impl AdaptiveBoundary<AsymmetricCusumBoundary> {
    /// Default CUSUM dual-mode boundary (sensitivity=1.5, tighten=3.0).
    pub fn new(spm_threshold: u32) -> Self {
        Self {
            poisson: PoissonCI::default_parametric(),
            high: AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
            spm_threshold,
        }
    }

    /// Construct with explicit PoissonCI floor and CUSUM boundary.
    pub fn with_params(
        poisson: PoissonCI,
        cusum: AsymmetricCusumBoundary,
        spm_threshold: u32,
    ) -> Self {
        Self {
            poisson,
            high: cusum,
            spm_threshold,
        }
    }
}

/// Dual-mode boundary pairing PoissonCI (low SPM) with
/// [`SignPersistenceCusumBoundary`] (high SPM). The sign-persistence boundary
/// is excellent at high SPM but collapses at low SPM (it discounts its
/// threshold on same-sign residual runs, which sparse-data Poisson noise
/// produces spuriously). This alias gives it the same low-SPM guard the CUSUM
/// dual-mode uses.
pub type AdaptiveSignPersist = AdaptiveBoundary<SignPersistenceCusumBoundary>;

impl AdaptiveBoundary<SignPersistenceCusumBoundary> {
    /// Construct with the default PoissonCI floor and an explicit
    /// sign-persistence boundary for the high-SPM regime.
    pub fn sign_persist(high: SignPersistenceCusumBoundary, spm_threshold: u32) -> Self {
        Self {
            poisson: PoissonCI::default_parametric(),
            high,
            spm_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_snap() -> EstimatorSnapshot {
        EstimatorSnapshot {
            h_estimate: 1.0e15,
            realized_share_per_min: 12.0,
            n_shares: 12,
            dt_secs: 60,
            uncertainty: None,
        }
    }

    #[test]
    fn adaptive_boundary_switches_on_spm_threshold() {
        // Below the threshold it must return the PoissonCI floor; at/above it
        // must defer to the wrapped high-SPM boundary. Use a SignPersist high
        // boundary so we exercise the generic over a non-CUSUM type.
        let b = AdaptiveSignPersist::sign_persist(
            SignPersistenceCusumBoundary::new(1.5, 0.05, 3.0, 0.15, 0.4),
            8,
        );
        let poisson = PoissonCI::default_parametric();

        // Below threshold (4 SPM) → PoissonCI floor exactly.
        let low = b.threshold(60, 4.0, &snap_with_realized(4.0));
        let expected_low = poisson.threshold(60, 4.0, &snap_with_realized(4.0));
        assert!(
            (low - expected_low).abs() < 1e-9,
            "below spm_threshold must use PoissonCI: got {low}, want {expected_low}"
        );

        // At/above threshold (20 SPM) → NOT the PoissonCI value (defers to the
        // sign-persistence boundary, which differs from the Poisson floor).
        let high = b.threshold(60, 20.0, &snap_with_realized(25.0));
        let poisson_high = poisson.threshold(60, 20.0, &snap_with_realized(25.0));
        assert!(
            (high - poisson_high).abs() > 1e-9,
            "at/above spm_threshold must use the high boundary, not PoissonCI"
        );
    }

    fn snap_with_realized(realized_spm: f64) -> EstimatorSnapshot {
        EstimatorSnapshot {
            h_estimate: 1.0e15,
            realized_share_per_min: realized_spm,
            n_shares: realized_spm.round() as u32,
            dt_secs: 60,
            uncertainty: None,
        }
    }

    #[test]
    fn warmup_fires_until_converged_then_latches_to_inner() {
        // Inner boundary with an easily-distinguished, non-zero threshold.
        let inner = AsymmetricCusumBoundary::new(1.0, 0.05, 3.0);
        let target = 12.0f32;
        let inner_theta = inner.threshold(60, target, &snap_with_realized(target as f64));
        assert!(
            inner_theta > 1e-9,
            "inner threshold should be non-zero for the test"
        );

        let w = WarmupBoundary::new(AsymmetricCusumBoundary::new(1.0, 0.05, 3.0), 0.15);

        // While far from target (realized 50% high), warm-up returns 0 (fire
        // on any deviation) and does NOT latch.
        for _ in 0..3 {
            let t = w.threshold(60, target, &snap_with_realized(18.0));
            assert_eq!(t, 0.0, "warm-up must return 0 until converged");
        }

        // A tick within the 15% band latches warmed; this tick still returns
        // 0 (fires the final convergence step).
        let at_converge = w.threshold(60, target, &snap_with_realized(12.5)); // ~4% off
        assert_eq!(at_converge, 0.0, "the converging tick still fires at 0");

        // From now on it must delegate to the inner boundary — even though
        // the realized rate is once again far off (a later disturbance must
        // NOT re-enter warm-up; the latch is one-way).
        let after = w.threshold(60, target, &snap_with_realized(18.0));
        let expect = inner.threshold(60, target, &snap_with_realized(18.0));
        assert!(
            (after - expect).abs() < 1e-9,
            "after warm-up must delegate to inner: got {after}, want {expect}"
        );
        assert!(
            after > 1e-9,
            "delegated threshold should be the non-zero inner value"
        );
    }

    #[test]
    fn voladapt_loosens_under_volatility_and_tightens_when_calm() {
        let spm = 12.0f32;

        // Calm stream: realized rate stays at the configured rate every tick.
        // The volatility factor should converge to ~1.0, so the threshold
        // matches the bare PoissonCI floor.
        let calm = VolatilityAdaptiveBoundary::new(0.3, 8.0);
        let poisson = PoissonCI::default_parametric();
        let base = poisson.threshold(60, spm, &dummy_snap());
        let mut calm_th = 0.0;
        for _ in 0..40 {
            calm_th = calm.threshold(60, spm, &snap_with_realized(12.0));
        }
        // Calm → factor clamps to its 1.0 floor → threshold ≈ PoissonCI floor.
        assert!(
            (calm_th - base).abs() < 1e-6,
            "calm threshold {calm_th} should equal PoissonCI floor {base}"
        );

        // Volatile stream: realized rate swings wildly tick to tick. Observed
        // variance >> Poisson expectation → factor > 1 → threshold loosens
        // above the bare floor.
        let volatile = VolatilityAdaptiveBoundary::new(0.3, 8.0);
        let mut vol_th = 0.0;
        for i in 0..40 {
            let realized = if i % 2 == 0 { 2.0 } else { 40.0 };
            vol_th = volatile.threshold(60, spm, &snap_with_realized(realized));
        }
        assert!(
            vol_th > base * 1.5,
            "volatile threshold {vol_th} should be well above the floor {base}"
        );
    }

    #[test]
    fn classic_table_matches_vardiff_state_cascade() {
        let b = StepFunction::classic_table();
        // Below the first boundary: 100% (only very large δ fires).
        assert_eq!(b.threshold(0, 12.0, &dummy_snap()), 100.0);
        assert_eq!(b.threshold(15, 12.0, &dummy_snap()), 100.0);
        assert_eq!(b.threshold(59, 12.0, &dummy_snap()), 100.0);
        // Each subsequent rung.
        assert_eq!(b.threshold(60, 12.0, &dummy_snap()), 60.0);
        assert_eq!(b.threshold(119, 12.0, &dummy_snap()), 60.0);
        assert_eq!(b.threshold(120, 12.0, &dummy_snap()), 50.0);
        assert_eq!(b.threshold(179, 12.0, &dummy_snap()), 50.0);
        assert_eq!(b.threshold(180, 12.0, &dummy_snap()), 45.0);
        assert_eq!(b.threshold(239, 12.0, &dummy_snap()), 45.0);
        assert_eq!(b.threshold(240, 12.0, &dummy_snap()), 30.0);
        assert_eq!(b.threshold(299, 12.0, &dummy_snap()), 30.0);
        // Floor: 15% past dt = 300s.
        assert_eq!(b.threshold(300, 12.0, &dummy_snap()), 15.0);
        assert_eq!(b.threshold(1_800, 12.0, &dummy_snap()), 15.0);
        assert_eq!(b.threshold(u64::MAX - 1, 12.0, &dummy_snap()), 15.0);
    }

    #[test]
    fn classic_threshold_is_share_rate_blind() {
        // The classic ladder ignores share rate — the same dt produces the
        // same threshold regardless of SPM. This is the property that
        // motivates the Parametric boundary (which IS rate-aware).
        let b = StepFunction::classic_table();
        for &spm in &[6.0f32, 12.0, 30.0, 60.0, 120.0] {
            assert_eq!(b.threshold(120, spm, &dummy_snap()), 50.0);
            assert_eq!(b.threshold(300, spm, &dummy_snap()), 15.0);
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
        let t12 = b.threshold(1200, 12.0, &dummy_snap());
        let t60 = b.threshold(1200, 60.0, &dummy_snap());
        let t120 = b.threshold(1200, 120.0, &dummy_snap());
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
        let t6 = b.threshold(600, 6.0, &dummy_snap());
        let t12 = b.threshold(600, 12.0, &dummy_snap());
        let t60 = b.threshold(600, 60.0, &dummy_snap());
        let t120 = b.threshold(600, 120.0, &dummy_snap());
        assert!(t6 > t12, "{} not > {}", t6, t12);
        assert!(t12 > t60);
        assert!(t60 > t120);
    }

    #[test]
    fn poisson_ci_returns_strict_threshold_on_degenerate_inputs() {
        let b = PoissonCI::default_parametric();
        assert_eq!(b.threshold(0, 12.0, &dummy_snap()), 100.0); // dt = 0 → λ̄ = 0
        assert_eq!(b.threshold(60, 0.0, &dummy_snap()), 100.0); // SPM = 0 → λ̄ = 0
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
                let d = default.threshold(dt, spm, &dummy_snap());
                let s = strict.threshold(dt, spm, &dummy_snap());
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
            assert_eq!(
                a.threshold(dt, spm, &dummy_snap()),
                b.threshold(dt, spm, &dummy_snap())
            );
        }
    }

    // ---- SignPersistenceCusumBoundary ----

    #[test]
    fn sign_persistence_first_tick_matches_asymmetric() {
        // On the first call the sign-persistence boundary should return
        // the same threshold as an equivalent AsymmetricCusumBoundary
        // (discount is 0 on the first tick: consecutive=1, discount = 0.02*(1-1) = 0).
        let asym = AsymmetricCusumBoundary::new(3.0, 0.05, 2.0);
        let sp = SignPersistenceCusumBoundary::new(3.0, 0.05, 2.0, 0.02, 0.4);

        let snap = dummy_snap(); // realized_share_per_min=12.0
        let spm = 30.0f32; // realized < configured → under-performing → ease direction

        let t_asym = asym.threshold(120, spm, &snap);
        let t_sp = sp.threshold(120, spm, &snap);
        assert!(
            (t_asym - t_sp).abs() < 1e-10,
            "first tick: asymmetric={}, sign_persistence={}",
            t_asym,
            t_sp
        );
    }

    #[test]
    fn sign_persistence_decreases_threshold_on_consecutive_same_sign() {
        let sp = SignPersistenceCusumBoundary::new(3.0, 0.05, 2.0, 0.02, 0.4);
        // Under-performing snap: realized_spm=12 < configured_spm=30
        let snap = dummy_snap();
        let spm = 30.0f32;

        let t1 = sp.threshold(120, spm, &snap);
        let t2 = sp.threshold(120, spm, &snap);
        let t3 = sp.threshold(120, spm, &snap);

        // Each consecutive same-sign tick should reduce the threshold.
        assert!(
            t2 < t1,
            "second tick {} should be less than first {}",
            t2,
            t1
        );
        assert!(
            t3 < t2,
            "third tick {} should be less than second {}",
            t3,
            t2
        );

        // Verify the discount magnitude: tick 2 discount = 0.02*(2-1)=0.02
        let expected_t2 = t1 * (1.0 - 0.02);
        assert!(
            (t2 - expected_t2).abs() < 1e-10,
            "t2={}, expected={}",
            t2,
            expected_t2
        );
    }

    #[test]
    fn sign_persistence_resets_on_sign_reversal() {
        let sp = SignPersistenceCusumBoundary::new(3.0, 0.05, 2.0, 0.02, 0.4);
        let spm = 30.0f32;

        // Under-performing: realized=12 < spm=30
        let snap_under = dummy_snap();
        let _t1 = sp.threshold(120, spm, &snap_under);
        let _t2 = sp.threshold(120, spm, &snap_under);
        let t3 = sp.threshold(120, spm, &snap_under); // consecutive=3, discount=0.04

        // Now flip sign: over-performing (realized=40 > spm=30)
        let snap_over = EstimatorSnapshot {
            h_estimate: 1.0e15,
            realized_share_per_min: 40.0,
            n_shares: 40,
            dt_secs: 60,
            uncertainty: None,
        };

        // After sign reversal, consecutive resets to 1 → discount = 0.
        // But note: direction also changes (now tightening), so threshold
        // itself may be different. The key property is that consecutive reset
        // means no discount.
        let t_reversed = sp.threshold(120, spm, &snap_over);

        // Compute what AsymmetricCusumBoundary would give for the over-performing case.
        let asym = AsymmetricCusumBoundary::new(3.0, 0.05, 2.0);
        let t_asym_over = asym.threshold(120, spm, &snap_over);

        assert!(
            (t_reversed - t_asym_over).abs() < 1e-10,
            "after reversal: got={}, expected asymmetric={}",
            t_reversed,
            t_asym_over
        );

        // Also verify t3 had an active discount (was less than the base).
        let t_asym_under = asym.threshold(120, spm, &snap_under);
        assert!(
            t3 < t_asym_under,
            "t3={} should be less than base asymmetric={}",
            t3,
            t_asym_under
        );
    }

    #[test]
    fn sign_persistence_caps_at_max_discount() {
        // With discount=0.10 per tick and max=0.4, the cap is reached at
        // consecutive=5 (discount = 0.10 * 4 = 0.40 = max).
        let sp = SignPersistenceCusumBoundary::new(3.0, 0.05, 2.0, 0.10, 0.4);
        let snap = dummy_snap(); // under-performing
        let spm = 30.0f32;

        let t1 = sp.threshold(120, spm, &snap); // consecutive=1, discount=0.00
        let _t2 = sp.threshold(120, spm, &snap); // consecutive=2, discount=0.10
        let _t3 = sp.threshold(120, spm, &snap); // consecutive=3, discount=0.20
        let _t4 = sp.threshold(120, spm, &snap); // consecutive=4, discount=0.30
        let t5 = sp.threshold(120, spm, &snap); // consecutive=5, discount=0.40 (capped)
        let t6 = sp.threshold(120, spm, &snap); // consecutive=6, discount=0.40 (still capped)

        let expected_capped = t1 * (1.0 - 0.4);
        assert!(
            (t5 - expected_capped).abs() < 1e-10,
            "t5={}, expected capped={}",
            t5,
            expected_capped
        );
        assert!(
            (t6 - expected_capped).abs() < 1e-10,
            "t6={}, expected capped={}",
            t6,
            expected_capped
        );
    }
}
