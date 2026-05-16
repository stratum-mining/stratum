//! Estimator trait — Axis 1 of the four-axis vardiff decomposition.
//!
//! The Estimator carries observation history between ticks and exposes a
//! snapshot of its current belief about the miner's hashrate. It is the
//! piece of the algorithm that lives in statistical estimation theory:
//! design pressure is responsiveness (small effective window, high
//! variance) vs stability (large effective window, low variance but
//! potentially stale data).
//!
//! See `sim/docs/DESIGN.md` § "Why these four axes" for the rationale
//! behind keeping this axis separate from Statistic, Boundary, and
//! UpdateRule.

use bitcoin::Target;
use channels_sv2::target::hash_rate_from_target;
use std::collections::VecDeque;
use std::fmt::Debug;

/// Per-tick context an [`Estimator`] needs to construct a snapshot.
///
/// Assembled by the `Composed` adapter from the inputs to `try_vardiff`
/// and passed to [`Estimator::snapshot`]. Carries the *current* target
/// and hashrate so the estimator can compute `H̃` (its own belief about
/// hashrate) from the realized share rate — e.g., via the
/// `hash_rate_from_target` conversion that `VardiffState` uses.
#[derive(Debug, Clone)]
pub struct EstimatorContext<'a> {
    pub current_hashrate: f32,
    pub current_target: &'a Target,
    pub shares_per_minute: f32,
}

/// A snapshot of the [`Estimator`]'s belief at a tick.
///
/// All inter-axis communication flows through this struct (and the
/// `Composed` adapter); there is no shared mutable state between the four
/// axes. Statistic consumes the snapshot to compute δ; UpdateRule consumes
/// it to compute the new target.
#[derive(Debug, Clone)]
pub struct EstimatorSnapshot {
    /// The estimator's belief about the miner's true hashrate (H/s).
    pub h_estimate: f32,
    /// Realized share rate over the window, in shares per minute.
    /// Zero if `dt_secs == 0` or `n_shares == 0`.
    pub realized_share_per_min: f64,
    /// Raw share count observed since the last reset.
    pub n_shares: u32,
    /// Window duration in seconds (`now - timestamp_of_last_update`).
    pub dt_secs: u64,
}

/// Axis 1: how the algorithm accumulates observation history and exposes
/// a snapshot of its current belief about the miner's hashrate.
///
/// **Theory**: statistical estimation (bias, variance, MSE).
///
/// **Design pressure**: responsiveness (small effective window, high
/// variance) vs stability (large effective window, low variance but
/// potentially stale data).
///
/// Implementations: [`CumulativeCounter`] (classic),
/// [`EwmaEstimator`] (EWMA / FullRemedy),
/// [`SlidingWindowEstimator`] (Sliding-Window).
pub trait Estimator: Debug + Send + Sync {
    /// Record `n_shares` new arrivals since the last `reset`.
    fn observe(&mut self, n_shares: u32);

    /// Reset the estimator state. Called by `Composed` on fire.
    fn reset(&mut self);

    /// Compute a snapshot of the estimator's current belief.
    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot;

    /// Return the raw share count, for the `Vardiff::shares_since_last_update`
    /// trait method. Estimators that don't carry an integer count (e.g.,
    /// EWMA) may return a derived value such as `ceil(weighted_sum)`.
    fn shares_count(&self) -> u32;
}

/// The classic estimator: a saturating cumulative share counter, with
/// `H̃` computed at snapshot time via `hash_rate_from_target` (with the
/// linear-scaling fallback used by `VardiffState`).
///
/// This is one of the four trait impls that, when composed together,
/// reproduce `VardiffState` fire-for-fire.
#[derive(Debug, Default)]
pub struct CumulativeCounter {
    shares: u32,
}

impl CumulativeCounter {
    pub fn new() -> Self {
        Self { shares: 0 }
    }
}

impl Estimator for CumulativeCounter {
    fn observe(&mut self, n_shares: u32) {
        self.shares = self.shares.saturating_add(n_shares);
    }

    fn reset(&mut self) {
        self.shares = 0;
    }

    fn shares_count(&self) -> u32 {
        self.shares
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        // dt_secs > 15 is guarded by the Composed adapter before snapshot()
        // is called, but the divide-by-zero defense is cheap and makes this
        // impl independently testable.
        let realized_share_per_min = if dt_secs == 0 {
            0.0
        } else {
            self.shares as f64 / (dt_secs as f64 / 60.0)
        };

        // Mirror VardiffState::try_vardiff: prefer hash_rate_from_target
        // for exact U256-arithmetic conversion; fall back to linear scaling
        // (mathematically equivalent when target was set for current_hashrate
        // at the configured share rate) on conversion error.
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => {
                ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute
            }
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares,
            dt_secs,
        }
    }
}

/// Exponentially-weighted moving average estimator. Tracks a smoothed
/// per-tick share count: on each `observe(n)` call (one per tick), the
/// internal rate estimate decays toward `n` with continuous-time
/// constant `tau_secs`:
///
/// ```text
/// α = exp(-tick_secs / tau_secs)
/// rate_per_tick ← α × rate_per_tick + (1 − α) × n
/// ```
///
/// ## Interpreting `tau_secs`
///
/// `tau_secs` is the continuous-time exponential time constant: an
/// impulse decays to `1/e ≈ 36.8%` of its initial value after
/// `tau_secs` seconds of elapsed time. The *half-life* (decay to 50%)
/// is `tau_secs × ln(2) ≈ 0.69 × tau_secs`.
///
/// Equivalent per-tick interpretation: at `tick_secs = tau_secs` (the
/// default, `60` and `60`), one tick decays the previous estimate by
/// `1 − e⁻¹ ≈ 63%` and weights the new observation by the same — the
/// EWMA has heavy short-term memory. To get the "lightly smoothed"
/// regime (`α ≈ 0.9`, fade over ~10 ticks) use `tau_secs ≈ 10 ×
/// tick_secs` or more.
///
/// ## Why one-observe-per-tick
///
/// The `Estimator` trait's `observe(n_shares: u32)` doesn't carry the
/// time interval between calls — so this impl assumes each `observe`
/// represents exactly one tick of duration `tick_secs`. The Composed
/// adapter's trial driver calls `add_shares` once per tick, so this
/// assumption holds for all framework-driven scenarios. Custom callers
/// driving observe directly should match the tick cadence to keep the
/// EWMA's effective time constant accurate; otherwise the discrete
/// decay rate diverges from `exp(-real_dt / tau_secs)`.
///
/// `realized_share_per_min` returned from `snapshot` is the rate
/// estimate converted to shares-per-minute: `rate_per_tick × (60 /
/// tick_secs)`.
#[derive(Debug)]
pub struct EwmaEstimator {
    /// Time constant in seconds. Common values: 30, 60, 120, 300.
    pub tau_secs: u64,
    /// Tick interval in seconds. Must match the trial driver's tick
    /// cadence for the EWMA's effective τ to be correct.
    pub tick_secs: u64,
    /// Current rate estimate in shares-per-tick.
    rate_per_tick: f64,
    /// Number of observations since the last reset. The first
    /// observation initializes `rate_per_tick` directly (no decay yet).
    n_observations: u32,
}

impl EwmaEstimator {
    /// Constructs an EWMA with the given time constant. `tick_secs`
    /// defaults to 60 (matching the trial driver's default tick
    /// interval).
    pub fn new(tau_secs: u64) -> Self {
        Self {
            tau_secs,
            tick_secs: 60,
            rate_per_tick: 0.0,
            n_observations: 0,
        }
    }

    /// Decay factor per tick: `exp(-tick_secs / tau_secs)`.
    /// Computed each call rather than cached so changes to `tau_secs`
    /// or `tick_secs` take effect immediately. The cost is negligible.
    fn alpha(&self) -> f64 {
        (-(self.tick_secs as f64) / (self.tau_secs as f64)).exp()
    }
}

impl Estimator for EwmaEstimator {
    fn observe(&mut self, n_shares: u32) {
        let n = n_shares as f64;
        if self.n_observations == 0 {
            self.rate_per_tick = n;
        } else {
            let alpha = self.alpha();
            self.rate_per_tick = alpha * self.rate_per_tick + (1.0 - alpha) * n;
        }
        self.n_observations = self.n_observations.saturating_add(1);
    }

    fn reset(&mut self) {
        self.rate_per_tick = 0.0;
        self.n_observations = 0;
    }

    fn shares_count(&self) -> u32 {
        self.rate_per_tick.round().max(0.0) as u32
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let realized_share_per_min = self.rate_per_tick * (60.0 / self.tick_secs as f64);
        // Same conversion path as CumulativeCounter: prefer
        // hash_rate_from_target, fall back to linear scaling.
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => {
                ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute
            }
        };
        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares_count(),
            dt_secs,
        }
    }
}

/// Sliding-window estimator: holds the last `n_ticks` per-tick share
/// observations in a ring buffer and computes `H̃` from the buffer
/// average.
///
/// ## Design simplification
///
/// Like `EwmaEstimator`, this impl assumes one `observe(n_shares)`
/// call per tick of fixed duration `tick_secs`. The Composed adapter's
/// trial driver satisfies this. Custom callers should match the tick
/// cadence to keep the effective window length correct.
///
/// ## Reset behavior
///
/// `reset()` clears the buffer. The Composed adapter calls `reset()`
/// on every fire, so during the cold-start Phase 1 ramp (where the
/// algorithm fires every tick) the buffer holds only a single
/// observation — the estimator behaves identically to
/// `CumulativeCounter` during Phase 1. The sliding-window benefit
/// shows up only post-Phase-1 when the buffer fills with multiple
/// at-truth ticks of data.
///
/// ## When to use
///
/// Pair with [`crate::composed::FullRetargetNoClamp`] for the
/// canonical `SlidingWindow` composition (see `DESIGN.md` §
/// "Algorithm registry"), or with [`crate::composed::PartialRetarget`]
/// for a more damped variant.
/// Always with [`crate::composed::PoissonCI`] boundary so the
/// rate-aware threshold complements the smoothing.
#[derive(Debug)]
pub struct SlidingWindowEstimator {
    /// Maximum number of per-tick observations to retain. The buffer
    /// holds up to this many entries; older entries are dropped on
    /// overflow.
    pub n_ticks: usize,
    /// Tick interval in seconds. Used to convert buffer count → time
    /// when computing realized share rate.
    pub tick_secs: u64,
    /// Ring buffer of per-tick share counts.
    buffer: VecDeque<u32>,
}

impl SlidingWindowEstimator {
    /// Constructs a sliding-window estimator with the given window
    /// length in ticks. Default `tick_secs = 60` matches the trial
    /// driver's tick cadence.
    pub fn new(n_ticks: usize) -> Self {
        Self {
            n_ticks,
            tick_secs: 60,
            buffer: VecDeque::with_capacity(n_ticks),
        }
    }
}

impl Estimator for SlidingWindowEstimator {
    fn observe(&mut self, n_shares: u32) {
        if self.buffer.len() >= self.n_ticks {
            self.buffer.pop_front();
        }
        self.buffer.push_back(n_shares);
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }

    fn shares_count(&self) -> u32 {
        // Saturating sum — at typical n_ticks (≤ 200) and per-tick
        // counts (≤ a few thousand under realistic Poisson) overflow
        // is unreachable. The saturate is defense in depth.
        self.buffer.iter().fold(0u32, |acc, &n| acc.saturating_add(n))
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let n_actual = self.buffer.len();
        let total_shares: u64 = self.buffer.iter().map(|&x| x as u64).sum();
        // The effective time window covered by the buffer.
        let window_secs = (n_actual as u64) * self.tick_secs;
        let realized_share_per_min = if window_secs > 0 {
            total_shares as f64 / (window_secs as f64 / 60.0)
        } else {
            0.0
        };

        // Same conversion path as CumulativeCounter and EwmaEstimator.
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => {
                ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute
            }
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            // Report the buffer total as n_shares — that's what the
            // estimator's snapshot represents.
            n_shares: total_shares.min(u32::MAX as u64) as u32,
            dt_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_increments_share_count() {
        let mut e = CumulativeCounter::new();
        e.observe(10);
        e.observe(5);
        assert_eq!(e.shares_count(), 15);
    }

    #[test]
    fn reset_zeroes_share_count() {
        let mut e = CumulativeCounter::new();
        e.observe(10);
        e.reset();
        assert_eq!(e.shares_count(), 0);
    }

    #[test]
    fn observe_saturates_at_u32_max() {
        let mut e = CumulativeCounter::new();
        e.observe(u32::MAX);
        e.observe(1);
        assert_eq!(e.shares_count(), u32::MAX);
    }

    #[test]
    fn snapshot_realized_rate_is_shares_per_minute() {
        let mut e = CumulativeCounter::new();
        e.observe(12); // 12 shares over 60s = 12 spm
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        let snap = e.snapshot(60, &ctx);
        assert!((snap.realized_share_per_min - 12.0).abs() < 1e-9);
        assert_eq!(snap.n_shares, 12);
        assert_eq!(snap.dt_secs, 60);
    }

    #[test]
    fn snapshot_zero_dt_yields_zero_rate() {
        let e = CumulativeCounter::new();
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        let snap = e.snapshot(0, &ctx);
        assert_eq!(snap.realized_share_per_min, 0.0);
    }

    // ---- EwmaEstimator ----

    #[test]
    fn ewma_first_observation_sets_rate_directly() {
        let mut e = EwmaEstimator::new(60);
        e.observe(12);
        // First obs initializes rate without decay.
        assert!((e.rate_per_tick - 12.0).abs() < 1e-9);
    }

    #[test]
    fn ewma_converges_to_steady_input() {
        // Feed a constant stream of 12 shares per tick; EWMA should
        // converge to 12 regardless of tau.
        let mut e = EwmaEstimator::new(120);
        for _ in 0..200 {
            e.observe(12);
        }
        assert!((e.rate_per_tick - 12.0).abs() < 1e-6);
    }

    #[test]
    fn ewma_with_long_tau_responds_slowly_to_change() {
        // Long tau → small alpha → strong memory of past values.
        let mut e = EwmaEstimator::new(600);
        for _ in 0..10 {
            e.observe(10);
        }
        // After 10 ticks the rate should be ≈ 10.
        assert!((e.rate_per_tick - 10.0).abs() < 1.0);
        // Now switch to 0; one tick later the rate should still be
        // mostly 10 (long memory).
        e.observe(0);
        // alpha = exp(-60/600) ≈ 0.905. new rate ≈ 0.905 × 10 + 0.095 × 0 = 9.05
        assert!(e.rate_per_tick > 8.5, "rate fell too fast: {}", e.rate_per_tick);
        assert!(e.rate_per_tick < 9.5);
    }

    #[test]
    fn ewma_with_short_tau_responds_fast_to_change() {
        // Short tau → large alpha decay → little memory.
        let mut e = EwmaEstimator::new(30);
        for _ in 0..10 {
            e.observe(10);
        }
        // Switch to 0.
        e.observe(0);
        // alpha = exp(-60/30) = exp(-2) ≈ 0.135. new ≈ 0.135 × 10 + 0.865 × 0 = 1.35
        assert!(e.rate_per_tick < 2.0, "rate fell too slow: {}", e.rate_per_tick);
    }

    #[test]
    fn ewma_reset_clears_state() {
        let mut e = EwmaEstimator::new(60);
        for _ in 0..5 {
            e.observe(10);
        }
        e.reset();
        assert_eq!(e.shares_count(), 0);
        // After reset, first observation again initializes directly.
        e.observe(42);
        assert!((e.rate_per_tick - 42.0).abs() < 1e-9);
    }

    // ---- U256 round-trip precision regression check ----
    //
    // The `hash_rate_to_target` / `hash_rate_from_target` round-trip
    // should be approximately linear: `recovered ≈ hashrate ×
    // realized_spm / configured_spm`. A pre-fix bug at
    // `target.rs:184` truncated the scale factor too aggressively
    // (×100 instead of ×100_000), producing up to 49% inflation at
    // realized_spm ∈ (30, 60). See FINDINGS.md §3 for the analysis.
    //
    // The fix (`× 100_000` scaling on both sides of the U256
    // arithmetic) brings round-trip precision below 1% for all
    // realistic inputs. This test pins that precision so a regression
    // (someone reverting to `× 100`) gets caught immediately.

    #[test]
    fn hash_rate_round_trip_is_precise_after_u256_fix() {
        use channels_sv2::target::{hash_rate_from_target, hash_rate_to_target};

        // (hashrate, configured_spm, realized_spm) — points that hit
        // the worst pre-fix inflation. Each is from the SPM=30
        // cold-start trace at the corresponding tick. Post-fix all
        // should round-trip within 1% of the linear prediction.
        let cases: &[(f64, f64, f64)] = &[
            // Pre-fix inflation: 1.49×. Post-fix: ~1×.
            (7.29e12, 30.0, 4031.0),
            // Pre-fix inflation: 1.09×. Post-fix: ~1×.
            (2.19e13, 30.0, 1375.0),
            // Pre-fix inflation: 1.08×. Post-fix: ~1×.
            (6.56e13, 30.0, 464.0),
            // Pre-fix inflation: 1.01×. Post-fix: ~1×.
            (1.97e14, 30.0, 169.0),
            // Always-precise cases (realized matches configured).
            (1.0e15, 30.0, 30.0),
            (1.0e15, 12.0, 60.0),
            // Additional coverage at production-typical share rates.
            (1.0e15, 6.0, 6.0),
            (1.0e15, 120.0, 120.0),
        ];

        for &(hashrate, configured, realized) in cases {
            let target = hash_rate_to_target(hashrate, configured)
                .expect("target conversion should succeed");
            let recovered = hash_rate_from_target(target.to_le_bytes().into(), realized)
                .expect("hashrate recovery should succeed");
            let linear_prediction = hashrate * realized / configured;
            let inflation = recovered / linear_prediction;
            // 2% tolerance after the U256 fix. Includes the small
            // target+1 ≈ target and 2^256 − target ≈ 2^256
            // approximations (both relative error < 1e-13) plus the
            // residual truncation in `60/realized × 100_000 as u128`.
            assert!(
                (inflation - 1.0).abs() < 0.02,
                "round-trip inflation at hashrate={}, cfg={}, realized={}: \
                 expected ~1.0, got {} (recovered={}, linear_prediction={}). \
                 This likely means the `× 100_000` U256 scale factor in \
                 target.rs was reverted to `× 100` — see FINDINGS.md §3.",
                hashrate, configured, realized,
                inflation, recovered, linear_prediction,
            );
        }
    }

    // ---- SlidingWindowEstimator ----

    #[test]
    fn sliding_window_buffer_grows_then_evicts_oldest() {
        let mut e = SlidingWindowEstimator::new(3);
        e.observe(10);
        e.observe(20);
        e.observe(30);
        // Buffer is full; next observe evicts the oldest.
        e.observe(40);
        assert_eq!(e.shares_count(), 20 + 30 + 40);
        e.observe(50);
        assert_eq!(e.shares_count(), 30 + 40 + 50);
    }

    #[test]
    fn sliding_window_reset_clears_buffer() {
        let mut e = SlidingWindowEstimator::new(5);
        for _ in 0..5 {
            e.observe(10);
        }
        assert_eq!(e.shares_count(), 50);
        e.reset();
        assert_eq!(e.shares_count(), 0);
        // Buffer is empty; first observe after reset starts fresh.
        e.observe(7);
        assert_eq!(e.shares_count(), 7);
    }

    #[test]
    fn sliding_window_short_buffer_uses_actual_length() {
        // Window n_ticks=10 but only 3 observations. realized rate
        // should be (sum / 3 ticks), not (sum / 10 ticks).
        let mut e = SlidingWindowEstimator::new(10);
        e.observe(30);
        e.observe(30);
        e.observe(30);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 30.0,
        };
        let snap = e.snapshot(180, &ctx);
        // 3 ticks × 60s = 180s window. 90 shares total. realized = 90/3 = 30 spm.
        assert!((snap.realized_share_per_min - 30.0).abs() < 1e-6);
    }

    #[test]
    fn sliding_window_full_buffer_averages_correctly() {
        // Constant 30 shares per tick across 10 ticks. realized_per_min
        // = 30. Exact.
        let mut e = SlidingWindowEstimator::new(10);
        for _ in 0..10 {
            e.observe(30);
        }
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 30.0,
        };
        let snap = e.snapshot(600, &ctx);
        assert!((snap.realized_share_per_min - 30.0).abs() < 1e-6);
    }

    #[test]
    fn sliding_window_oldest_drops_out() {
        // Fill with 60-per-tick (high rate), then add ticks at 0
        // (truth has dropped). Once the buffer fully turns over, the
        // estimate should reflect the new rate.
        let mut e = SlidingWindowEstimator::new(5);
        for _ in 0..5 {
            e.observe(60);
        }
        // Buffer = [60, 60, 60, 60, 60]; total = 300; realized = 60 spm.
        // Now feed 5 ticks of 0:
        for _ in 0..5 {
            e.observe(0);
        }
        // Buffer = [0, 0, 0, 0, 0]; realized = 0 spm.
        assert_eq!(e.shares_count(), 0);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 60.0,
        };
        let snap = e.snapshot(300, &ctx);
        assert_eq!(snap.realized_share_per_min, 0.0);
    }

    #[test]
    fn sliding_window_empty_buffer_yields_zero_rate() {
        let e = SlidingWindowEstimator::new(10);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 30.0,
        };
        let snap = e.snapshot(60, &ctx);
        assert_eq!(snap.realized_share_per_min, 0.0);
    }

    #[test]
    fn ewma_snapshot_returns_rate_in_shares_per_minute() {
        let mut e = EwmaEstimator::new(60);
        // Feed 12 shares per tick for many ticks.
        for _ in 0..50 {
            e.observe(12);
        }
        // rate_per_tick ≈ 12, tick_secs = 60.
        // shares_per_min = 12 * (60/60) = 12.
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        let snap = e.snapshot(60, &ctx);
        assert!((snap.realized_share_per_min - 12.0).abs() < 1e-3);
    }
}
