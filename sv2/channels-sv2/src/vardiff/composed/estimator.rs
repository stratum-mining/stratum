//! Estimator — Stage 1 of the three-stage vardiff pipeline.
//!
//! The Estimator carries observation history between ticks and exposes a
//! snapshot of its current belief about the miner's hashrate. It is the
//! piece of the algorithm that lives in statistical estimation theory:
//! design pressure is responsiveness (small effective window, high
//! variance) vs stability (large effective window, low variance but
//! potentially stale data).
//!
//! See `sim/docs/DESIGN.md` for the pipeline architecture.

use crate::target::hash_rate_from_target;
use bitcoin::Target;
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

/// Uncertainty quantification from the Estimator. Enables the Boundary
/// to adapt its threshold based on how confident the estimate is, rather
/// than inferring confidence from proxy signals (dt_secs, SPM).
#[derive(Debug, Clone, Copy)]
pub struct Uncertainty {
    /// Standard deviation of the ratio estimate (h_estimate / current_h).
    /// Dimensionless. Smaller = more confident.
    pub ratio_std: f64,
    /// Effective independent sample count informing this estimate.
    /// Analogous to 1/(1-discount) for Bayesian, tau/tick for EWMA.
    pub effective_n: f64,
}

/// The Estimator's belief at a tick — the primary handoff struct from
/// Stage 1 to all downstream stages. Carries both a point estimate and
/// optional uncertainty quantification.
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
    /// Optional uncertainty quantification. `None` for estimators that
    /// don't track uncertainty (CumulativeCounter, EWMA, SlidingWindow).
    /// `Some` for estimators that maintain a probabilistic belief
    /// (BayesianEstimator). Downstream stages (especially Boundary) can
    /// use this to auto-calibrate their thresholds.
    pub uncertainty: Option<Uncertainty>,
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
/// [`SpmRatioEstimator`] (EWMA with pure SPM-ratio conversion),
/// [`SlidingWindowEstimator`] (Sliding-Window).
pub trait Estimator: Debug + Send + Sync {
    /// Record `n_shares` new arrivals.
    ///
    /// **Calling convention**: callers may invoke this either once per
    /// tick with the full batch count (`observe(12)`) or once per
    /// individual share arrival (`observe(1)` × 12). Implementations
    /// MUST produce correct results under both patterns. Estimators
    /// that apply per-call temporal decay (e.g., EWMA) must accumulate
    /// shares internally and defer the decay to `snapshot()`.
    fn observe(&mut self, n_shares: u32);

    /// Notification that the algorithm fired and changed the target.
    /// `new_hashrate` is the target being set; `old_hashrate` is what
    /// it was before the fire.
    ///
    /// **Why this method has to exist (the invariant):** the only observable
    /// is `r_obs = r*·H/Ĥ`, so any estimator whose state is expressed
    /// *relative to the current difficulty* must be told when the controller
    /// moves `Ĥ`, or it will read its own retarget as a change in `H`. That
    /// clause is the discriminator: every difficulty-relative representation
    /// pays a rescale here (EWMA rescales its shares-per-tick rate, Ckpool its
    /// dsps, the Kalman/Bayesian filters their ratio state) — only a
    /// difficulty-*weighted* hashrate-space estimator (weight each share by its
    /// `job_id_to_target` snapshot) would be retarget-invariant and could
    /// no-op. The tree deliberately takes the former (see DESIGN.md).
    ///
    /// Estimators choose how to respond:
    /// - Absolute-rate estimators (CumulativeCounter, EWMA, SlidingWindow):
    ///   full reset / rate rescale — old data is invalid under the new target.
    /// - Ratio-based estimators (Bayesian, Kalman): rescale state to center
    ///   on ratio=1.0 relative to the new target, preserving accumulated
    ///   confidence.
    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32);

    /// Compute a snapshot of the estimator's current belief.
    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot;

    /// Return the raw share count, for the `Vardiff::shares_since_last_update`
    /// trait method. Estimators that don't carry an integer count (e.g.,
    /// EWMA) may return a derived value such as `ceil(weighted_sum)`.
    fn shares_count(&self) -> u32;

    /// A short, drift-proof code derived from the estimator's actual
    /// parameters. Used (with the [`Boundary`](super::Boundary) and
    /// [`UpdateRule`](super::UpdateRule) codes) to build the composed
    /// algorithm's display name and filename. Because each `code()` is
    /// built from the concrete type's live fields, the name can never
    /// drift away from what the algorithm actually does.
    fn code(&self) -> String;
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

    fn on_fire(&mut self, _new_hashrate: f32, _old_hashrate: f32) {
        self.shares = 0;
    }

    fn shares_count(&self) -> u32 {
        self.shares
    }

    fn code(&self) -> String {
        "Cumul".to_string()
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
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares,
            dt_secs,
            uncertainty: None,
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
/// `realized_share_per_min` returned from `snapshot` is the rate
/// estimate converted to shares-per-minute: `rate_per_tick × (60 /
/// tick_secs)`.
///
/// ## Calling convention safety
///
/// `observe()` simply accumulates shares into a pending counter.
/// The EWMA decay is applied once per `snapshot()` call (which the
/// `Composed` adapter calls once per vardiff tick). This makes the
/// estimator immune to whether the caller invokes `observe(12)` once
/// or `observe(1)` twelve times — both produce identical results.
///
/// After snapshot computes the blended rate, `on_fire()` (or the next
/// `snapshot()` via atomic CAS) advances the tick state. The atomics
/// satisfy the `Send + Sync` trait bounds without actual concurrent
/// access — `Vardiff` objects are always behind `&mut self`.
#[derive(Debug)]
pub struct EwmaEstimator {
    /// Time constant in seconds. Common values: 30, 60, 120, 300.
    pub tau_secs: u64,
    /// Tick interval in seconds. Must match the trial driver's tick
    /// cadence for the EWMA's effective τ to be correct.
    pub tick_secs: u64,
    /// Current rate estimate in shares-per-tick (after last snapshot).
    /// Stored as bits via AtomicU64 for Send+Sync.
    rate_bits: std::sync::atomic::AtomicU64,
    /// Shares accumulated since last snapshot (pending for next decay step).
    pending_shares: std::sync::atomic::AtomicU32,
    /// Number of ticks (snapshots) since the last reset.
    n_ticks: std::sync::atomic::AtomicU32,
}

impl EwmaEstimator {
    /// Constructs an EWMA with the given time constant. `tick_secs`
    /// defaults to 60 (matching the trial driver's default tick
    /// interval).
    pub fn new(tau_secs: u64) -> Self {
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::AtomicU64;
        Self {
            tau_secs,
            tick_secs: 60,
            rate_bits: AtomicU64::new(0.0f64.to_bits()),
            pending_shares: AtomicU32::new(0),
            n_ticks: AtomicU32::new(0),
        }
    }

    /// Constructs an EWMA already seeded to a known share rate — the
    /// cold-start hint from the channel's declared `nominal_hash_rate`.
    ///
    /// The default `new()` starts cold (`rate=0`, `n_ticks=0`) and must climb
    /// from the floor on real shares — the ~65-min cold-start ramp (§8.5). At
    /// channel open the miner *declares* a nominal hashrate, and the channel's
    /// difficulty is set from it (`D = nominal/r*`); if the declaration is
    /// accurate the miner therefore produces ≈ `r*` shares/min at that
    /// difficulty. Seeding the EWMA's rate to `seed_spm·tick/60` makes the very
    /// first `snapshot` believe ≈ the declared nominal, collapsing the ramp.
    ///
    /// SAFETY / why this is bounded, not blind trust:
    ///   - The seed is a TIGHTEN-from-the-floor (the cold-start belief is below
    ///     truth, §8.5), i.e. the dangerous over-difficulty direction. It is
    ///     safe ONLY because (a) the channel clamps difficulty by the miner's
    ///     own declared `max_target` (an inflated seed cannot push past the
    ///     floor the miner accepted), and (b) `prior_ticks` is SMALL, so the
    ///     first window of real shares overwrites the seed via the EWMA — the
    ///     seed is a prior, not a commitment.
    ///   - A larger `prior_ticks` trusts the hint longer (faster ramp collapse,
    ///     slower correction of a bad hint); a smaller one is more cautious.
    ///     `prior_ticks = 1` gives the seed roughly one tick's worth of weight.
    ///
    /// `seed_spm` is the share rate the declaration implies at the open-time
    /// difficulty — i.e. the target `r*` (`shares_per_minute`) when the
    /// declared nominal was used to set that difficulty. Callers that cannot
    /// establish a plausible seed should use [`Self::new`] (cold).
    pub fn new_seeded(tau_secs: u64, seed_spm: f64, prior_ticks: u32) -> Self {
        use std::sync::atomic::AtomicU32;
        use std::sync::atomic::AtomicU64;
        let tick_secs = 60u64;
        // rate is in shares-per-tick; seed_spm is shares-per-minute.
        let seed_rate = seed_spm.max(0.0) * (tick_secs as f64 / 60.0);
        Self {
            tau_secs,
            tick_secs,
            rate_bits: AtomicU64::new(seed_rate.to_bits()),
            pending_shares: AtomicU32::new(0),
            // n_ticks>0 marks the EWMA as already-initialized, so the first
            // real observation BLENDS with the seed (alpha·seed + (1-alpha)·obs)
            // rather than replacing it — but with small prior_ticks the seed
            // decays out within ~τ as designed.
            n_ticks: AtomicU32::new(prior_ticks),
        }
    }

    /// Decay factor per tick: `exp(-tick_secs / tau_secs)`.
    fn alpha(&self) -> f64 {
        (-(self.tick_secs as f64) / (self.tau_secs as f64)).exp()
    }

    fn get_rate(&self) -> f64 {
        f64::from_bits(self.rate_bits.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn set_rate(&self, rate: f64) {
        self.rate_bits
            .store(rate.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// Returns the current smoothed rate (shares per tick). For testing.
    pub fn rate_per_tick(&self) -> f64 {
        self.get_rate()
    }
}

impl Estimator for EwmaEstimator {
    fn observe(&mut self, n_shares: u32) {
        *self.pending_shares.get_mut() = self.pending_shares.get_mut().saturating_add(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        let pending = *self.pending_shares.get_mut();
        let n_ticks = *self.n_ticks.get_mut();

        // Flush pending shares into the EWMA before rescaling.
        if pending > 0 {
            let n = pending as f64;
            let rate = if n_ticks == 0 {
                n
            } else {
                let alpha = self.alpha();
                alpha * self.get_rate() + (1.0 - alpha) * n
            };
            self.set_rate(rate);
            *self.n_ticks.get_mut() = n_ticks + 1;
            *self.pending_shares.get_mut() = 0;
        }

        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
            return;
        }

        // Rescale: if target moved by R = new/old, future shares at
        // the same true hashrate arrive at rate = old_rate / R.
        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.set_rate(self.get_rate() / ratio);
        } else {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
        }
    }

    fn shares_count(&self) -> u32 {
        self.pending_shares
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn code(&self) -> String {
        format!("Ewma{}s", self.tau_secs)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        // Apply EWMA decay with the pending shares as this tick's observation.
        let pending = self
            .pending_shares
            .load(std::sync::atomic::Ordering::Relaxed);
        let n_ticks = self.n_ticks.load(std::sync::atomic::Ordering::Relaxed);
        let n = pending as f64;

        let rate = if n_ticks == 0 {
            n
        } else {
            let alpha = self.alpha();
            alpha * self.get_rate() + (1.0 - alpha) * n
        };

        // Advance state: flush pending into rate, increment tick count.
        self.set_rate(rate);
        self.n_ticks
            .store(n_ticks + 1, std::sync::atomic::Ordering::Relaxed);
        self.pending_shares
            .store(0, std::sync::atomic::Ordering::Relaxed);

        let realized_share_per_min = rate * (60.0 / self.tick_secs as f64);
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };
        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            // RAW per-tick share count (the `pending` observed this window,
            // captured BEFORE the flush to 0 above). This is the un-smoothed
            // count whose variance Theorem 2 bounds (`Var ≥ 1/(r*τ)`) — the
            // quantity the lever/band-scaling claim is actually about, upstream
            // of the EWMA smoothing that `realized_share_per_min` has been
            // through. `shares_count()` here would read 0 (post-flush), so use
            // `pending` directly.
            n_shares: pending,
            dt_secs,
            uncertainty: None,
        }
    }
}

/// SHARE-INDEXED estimator — a rate-aware window, the contestant in the
/// fixed-vs-rate-aware study (`sim/src/bin/tau-share-indexed.rs`,
/// pre-registered). Identical to [`EwmaEstimator`] EXCEPT the decay weight is
/// per-SHARE, not per-tick: `alpha = exp(-pending_shares / n_span)` instead of
/// `exp(-tick_secs / tau_secs)`.
///
/// WHY (the theory, §5.2a / Theorem 2): a constant-shares window holds per-window
/// precision `1/√(r*·τ_eff)` CONSTANT across rate by construction — its effective
/// time-window `n_span/r*` shrinks as `r*` rises (jumpier high, sleepier low),
/// tracking the per-rate optimum that a fixed-time window rides off at the band
/// edges. No `r*` is ever computed (which would put the noisy rate estimate in
/// the feedback path); the rate-awareness is implicit in counting shares.
///
/// `n_span` is GROUNDED, not chosen: `n_span = r_ref · τ_eff` so that this and
/// the fixed champion COINCIDE at the reference rate `r_ref` (same shares-in-
/// window ⇒ same precision and lag there) — making the comparison a clean test
/// of *rate-awareness* rather than of window-size. At `r_ref=12 spm`, champion
/// `τ=360s=6min` ⇒ `n_span = 12·6 = 72` shares. The coincidence is EXACT, not
/// approximate: `exp(-pending/n_span) = exp(-tick/τ)` precisely when
/// `pending = r_ref·tick` and `n_span = r_ref·τ`. (`r_ref` is itself a grounding
/// choice and a sweepable axis, like the gate's k and δ_clock.)
///
/// Emergent rate-awareness, as a property of the rule: on an empty tick (no
/// shares — low rate) `alpha = exp(0) = 1`, so the estimate is HELD (no shares =
/// no information), where the time-EWMA would decay toward zero on the same empty
/// tick. The window stretches itself when shares are sparse.
#[derive(Debug)]
pub struct ShareIndexedEstimator {
    /// Window span in SHARES (the share-indexed analogue of `tau_secs`).
    pub n_span: f64,
    /// Tick interval in seconds (for the shares-per-min conversion at snapshot;
    /// the decay does NOT use it — that is the whole point).
    pub tick_secs: u64,
    /// Current rate estimate in shares-per-tick. Bits via AtomicU64 for Send+Sync.
    rate_bits: std::sync::atomic::AtomicU64,
    /// Shares accumulated since last snapshot.
    pending_shares: std::sync::atomic::AtomicU32,
    /// Snapshots since last reset (0 ⇒ first observation sets the rate directly).
    n_ticks: std::sync::atomic::AtomicU32,
}

impl ShareIndexedEstimator {
    /// Construct with an explicit share-window span.
    pub fn new(n_span: f64) -> Self {
        use std::sync::atomic::{AtomicU32, AtomicU64};
        Self {
            n_span: n_span.max(1.0),
            tick_secs: 60,
            rate_bits: AtomicU64::new(0.0f64.to_bits()),
            pending_shares: AtomicU32::new(0),
            n_ticks: AtomicU32::new(0),
        }
    }

    /// Construct by the GROUNDING rule: coincide with a time-EWMA of `tau_secs`
    /// at reference rate `r_ref_spm`. `n_span = r_ref · (tau_secs/60)` shares.
    /// This is how the study builds it (champion τ=360, r_ref=12 ⇒ n_span=72).
    pub fn new_grounded(tau_secs: u64, r_ref_spm: f64) -> Self {
        Self::new(r_ref_spm * (tau_secs as f64 / 60.0))
    }

    fn get_rate(&self) -> f64 {
        f64::from_bits(self.rate_bits.load(std::sync::atomic::Ordering::Relaxed))
    }
    fn set_rate(&self, rate: f64) {
        self.rate_bits
            .store(rate.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
    /// Per-SHARE decay factor for a window observing `pending` shares this tick:
    /// `exp(-pending / n_span)`. (The rate-aware analogue of `EwmaEstimator::alpha`.)
    fn alpha_for(&self, pending: u32) -> f64 {
        (-(pending as f64) / self.n_span).exp()
    }
    /// Current smoothed rate (shares per tick). For testing.
    pub fn rate_per_tick(&self) -> f64 {
        self.get_rate()
    }
}

impl Estimator for ShareIndexedEstimator {
    fn observe(&mut self, n_shares: u32) {
        *self.pending_shares.get_mut() = self.pending_shares.get_mut().saturating_add(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        let pending = *self.pending_shares.get_mut();
        let n_ticks = *self.n_ticks.get_mut();
        // Flush pending into the rate with the PER-SHARE decay weight.
        if pending > 0 {
            let n = pending as f64;
            let rate = if n_ticks == 0 {
                n
            } else {
                let alpha = self.alpha_for(pending);
                alpha * self.get_rate() + (1.0 - alpha) * n
            };
            self.set_rate(rate);
            *self.n_ticks.get_mut() = n_ticks + 1;
            *self.pending_shares.get_mut() = 0;
        }
        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
            return;
        }
        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.set_rate(self.get_rate() / ratio);
        } else {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
        }
    }

    fn shares_count(&self) -> u32 {
        self.pending_shares
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn code(&self) -> String {
        format!("ShareIdx{}", self.n_span as u64)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let pending = self
            .pending_shares
            .load(std::sync::atomic::Ordering::Relaxed);
        let n_ticks = self.n_ticks.load(std::sync::atomic::Ordering::Relaxed);
        let n = pending as f64;
        // PER-SHARE decay: alpha = exp(-pending/n_span). On an empty tick
        // (pending=0) alpha=1 ⇒ rate HELD (no shares = no information), the
        // emergent window-stretch at low rate.
        let rate = if n_ticks == 0 {
            n
        } else {
            let alpha = self.alpha_for(pending);
            alpha * self.get_rate() + (1.0 - alpha) * n
        };
        self.set_rate(rate);
        self.n_ticks
            .store(n_ticks + 1, std::sync::atomic::Ordering::Relaxed);
        self.pending_shares
            .store(0, std::sync::atomic::Ordering::Relaxed);

        // rate→h_estimate path IDENTICAL to EwmaEstimator (only the decay differs).
        let realized_share_per_min = rate * (60.0 / self.tick_secs as f64);
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };
        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: pending,
            dt_secs,
            uncertainty: None,
        }
    }
}

/// SPM-ratio estimator: EWMA-smoothed share rate, converted to a
/// hashrate estimate purely via linear SPM-ratio scaling.
///
/// This is mathematically equivalent to `EwmaEstimator`'s linear
/// fallback path (`current_hashrate * realized_spm / shares_per_minute`),
/// but uses it as the PRIMARY conversion — never attempting the
/// `hash_rate_from_target` U256 arithmetic path. This avoids any
/// precision issues from U256 truncation and makes the conversion
/// trivially auditable.
///
/// ## EWMA smoothing
///
/// Identical to [`EwmaEstimator`]:
///
/// ```text
/// alpha = exp(-tick_secs / tau_secs)
/// rate_per_tick <- alpha * rate_per_tick + (1 - alpha) * n
/// ```
///
/// ## Snapshot conversion
///
/// ```text
/// realized_spm = rate_per_tick * (60 / tick_secs)
/// h_estimate = current_hashrate * (realized_spm / shares_per_minute)
/// ```
///
/// No U256 arithmetic. No fallback branches.
///
/// ## When to use
///
/// Prefer this over `EwmaEstimator` when:
/// - The deployment has hit U256 precision edge cases (very low/high SPM)
/// - Simplicity and auditability are priorities
/// - The linear-scaling assumption holds (target was set for
///   `current_hashrate` at the configured `shares_per_minute`)
#[derive(Debug)]
pub struct SpmRatioEstimator {
    /// Time constant in seconds. Common values: 30, 60, 120, 300.
    pub tau_secs: u64,
    /// Tick interval in seconds. Must match the trial driver's tick
    /// cadence for the EWMA's effective tau to be correct.
    pub tick_secs: u64,
    /// Current rate estimate in shares-per-tick.
    rate_per_tick: f64,
    /// Number of observations since the last reset. The first
    /// observation initializes `rate_per_tick` directly (no decay yet).
    n_observations: u32,
}

impl SpmRatioEstimator {
    /// Constructs an SPM-ratio estimator with the given time constant.
    /// `tick_secs` defaults to 60 (matching the trial driver's default
    /// tick interval).
    pub fn new(tau_secs: u64) -> Self {
        Self {
            tau_secs,
            tick_secs: 60,
            rate_per_tick: 0.0,
            n_observations: 0,
        }
    }

    /// Decay factor per tick: `exp(-tick_secs / tau_secs)`.
    fn alpha(&self) -> f64 {
        (-(self.tick_secs as f64) / (self.tau_secs as f64)).exp()
    }

    /// Returns the current smoothed rate (shares per tick). For testing.
    pub fn rate_per_tick(&self) -> f64 {
        self.rate_per_tick
    }
}

impl Estimator for SpmRatioEstimator {
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

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.rate_per_tick = 0.0;
            self.n_observations = 0;
            return;
        }

        // Rescale the smoothed rate by the retarget ratio, identical to
        // EwmaEstimator: future shares at the same true hashrate arrive
        // at rate = old_rate / R under the new (harder/easier) target.
        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.rate_per_tick /= ratio;
        } else {
            self.rate_per_tick = 0.0;
            self.n_observations = 0;
        }
    }

    fn shares_count(&self) -> u32 {
        self.rate_per_tick.round().max(0.0) as u32
    }

    fn code(&self) -> String {
        "SpmRatio".to_string()
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let realized_share_per_min = self.rate_per_tick * (60.0 / self.tick_secs as f64);

        // Pure linear scaling — no U256 arithmetic.
        let h_estimate =
            ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute;

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares_count(),
            dt_secs,
            uncertainty: None,
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

    fn on_fire(&mut self, _new_hashrate: f32, _old_hashrate: f32) {
        self.buffer.clear();
    }

    fn shares_count(&self) -> u32 {
        // Saturating sum — at typical n_ticks (≤ 200) and per-tick
        // counts (≤ a few thousand under realistic Poisson) overflow
        // is unreachable. The saturate is defense in depth.
        self.buffer
            .iter()
            .fold(0u32, |acc, &n| acc.saturating_add(n))
    }

    fn code(&self) -> String {
        format!("Slide{}t", self.n_ticks)
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
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            // Report the buffer total as n_shares — that's what the
            // estimator's snapshot represents.
            n_shares: total_shares.min(u32::MAX as u64) as u32,
            dt_secs,
            uncertainty: None,
        }
    }
}

/// Bayesian Gamma-Poisson estimator with exponential discounting.
///
/// Models the per-tick share rate as a Poisson process with unknown rate λ,
/// and maintains a Gamma posterior over a normalized ratio:
///
/// ```text
/// ratio = true_rate / expected_rate
/// ratio ~ Gamma(alpha, beta)
/// ```
///
/// On each `observe(n_shares)` call (one per tick), the estimator:
/// 1. Discounts the prior: `alpha *= discount`, `beta *= discount`
/// 2. Updates with the observation: `alpha += n_shares`, `beta += 1.0`
///
/// The `+= 1.0` for beta represents "one tick's worth of exposure" — the
/// posterior tracks *shares per tick* normalized by the expected count at
/// snapshot time.
///
/// ## Why Gamma-Poisson?
///
/// This is the conjugate Bayesian model for Poisson data: Gamma prior +
/// Poisson likelihood → Gamma posterior. The update is exact (no approximation)
/// and closed-form (no sampling required). The posterior automatically
/// provides uncertainty quantification (variance shrinks as data accumulates).
///
/// ## Discount factor
///
/// `discount ∈ (0, 1]` controls memory decay per tick:
/// - 0.99: effective memory ~100 ticks (slow, steady)
/// - 0.95: effective memory ~20 ticks (moderate)
/// - 0.90: effective memory ~10 ticks (fast, jittery)
///
/// The effective memory length is approximately `1 / (1 - discount)` ticks.
///
/// ## Relationship to EWMA
///
/// Both have exponential memory. Key difference: EWMA tracks a point estimate
/// of shares-per-tick. Bayesian tracks the full Gamma posterior, which gives
/// both a point estimate (posterior mean) AND an uncertainty measure (posterior
/// variance). This uncertainty can inform downstream decisions (e.g., the
/// boundary can require higher confidence before firing when uncertainty is high).
///
/// ## Break detection / mode-switching
///
/// This estimator does NOT include break detection. That responsibility lives
/// in the Boundary axis. Keeping estimation pure lets us compose the Bayesian
/// estimator with different boundary strategies (PoissonCI, posterior-predictive,
/// etc.) independently.
#[derive(Debug, Clone)]
pub struct BayesianEstimator {
    /// Gamma posterior shape parameter. Roughly "total discounted share count."
    pub alpha: f64,
    /// Gamma posterior rate parameter. Roughly "total discounted tick count."
    pub beta: f64,
    /// Exponential discount factor applied before each tick update. Range (0, 1].
    pub discount: f64,
    /// Initial pseudo-observation strength.
    pub prior_shares: f64,
    /// Tick interval in seconds.
    pub tick_secs: u64,
    /// Total shares observed since last reset (for reporting).
    shares_since_reset: u32,
    /// Number of ticks since last reset.
    n_ticks_since_reset: u32,
}

impl BayesianEstimator {
    /// Constructs a Bayesian estimator.
    ///
    /// `discount` in (0, 1]: exponential forgetting per tick.
    /// `prior_shares`: initial pseudo-count strength. Higher = more
    /// confident initial estimate. Typically 2.0–10.0.
    pub fn new(discount: f64, prior_shares: f64) -> Self {
        let prior_shares = prior_shares.max(0.01);
        Self {
            alpha: prior_shares,
            beta: prior_shares, // ratio_mean = alpha/beta = 1.0 initially
            discount: discount.clamp(0.001, 1.0),
            prior_shares,
            tick_secs: 60,
            shares_since_reset: 0,
            n_ticks_since_reset: 0,
        }
    }

    /// Posterior mean of the rate ratio.
    pub fn ratio_mean(&self) -> f64 {
        if self.beta <= 0.0 {
            return 1.0;
        }
        (self.alpha / self.beta).clamp(1.0e-6, 1.0e6)
    }

    /// Posterior variance of the rate ratio.
    pub fn ratio_variance(&self) -> f64 {
        if self.beta <= 0.0 {
            return 1.0e6;
        }
        (self.alpha / (self.beta * self.beta)).max(0.0)
    }

    /// Posterior standard deviation of the rate ratio.
    pub fn ratio_std(&self) -> f64 {
        self.ratio_variance().sqrt()
    }
}

impl Estimator for BayesianEstimator {
    fn observe(&mut self, n_shares: u32) {
        // Each observe = one tick of data.
        // We accumulate shares and tick count; the actual Bayesian update
        // happens at snapshot time when we have access to SPM (the expected
        // rate). This deferred approach is needed because observe() doesn't
        // receive context about the expected share rate.
        self.shares_since_reset = self.shares_since_reset.saturating_add(n_shares);
        self.n_ticks_since_reset = self.n_ticks_since_reset.saturating_add(1);
    }

    fn on_fire(&mut self, _new_hashrate: f32, _old_hashrate: f32) {
        // After a fire, the new target ≈ old_target × ratio_mean, so the
        // true ratio relative to the new target should be ≈ 1.0.
        //
        // Preserve posterior mass proportional to accumulated data — this
        // is the key difference from full-reset estimators. We shift the
        // center to ratio=1.0 but keep confidence proportional to
        // min(accumulated_info, cap).
        //
        // The cap prevents over-confidence: even if we've seen 100 ticks,
        // after a target change we should be somewhat uncertain about
        // whether the new target is exactly right.
        let info_accumulated = self.alpha.min(self.beta);
        let preserved = info_accumulated
            .min(self.prior_shares * 4.0)
            .max(self.prior_shares);
        self.alpha = preserved;
        self.beta = preserved; // ratio_mean = 1.0

        self.shares_since_reset = 0;
        self.n_ticks_since_reset = 0;
    }

    fn shares_count(&self) -> u32 {
        self.shares_since_reset
    }

    fn code(&self) -> String {
        format!("Bayes-d{}-p{}", self.discount, self.prior_shares)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        // Compute the posterior after incorporating ALL ticks since last reset.
        // Each tick contributes (observed_shares_that_tick, expected_shares_per_tick)
        // to the Gamma-Poisson update. Since we only have the total, we treat
        // the whole window as one observation:
        //   alpha_new = discount^n * alpha_0 + total_observed
        //   beta_new = discount^n * beta_0 + total_expected
        //
        // This is mathematically equivalent to per-tick updates when the
        // observation is concentrated in one window (which it is, since
        // Composed resets on fire).
        let n_ticks = self.n_ticks_since_reset as f64;
        let total_observed = self.shares_since_reset as f64;

        // Expected shares for the entire window at current difficulty
        let expected_per_tick = (ctx.shares_per_minute as f64) * (self.tick_secs as f64 / 60.0);
        let total_expected = expected_per_tick * n_ticks;

        // Discount factor over the full window
        let discount_factor = self.discount.powf(n_ticks);

        let alpha = discount_factor * self.alpha + total_observed;
        let beta = discount_factor * self.beta + total_expected.max(1.0e-12);

        // Posterior mean ratio
        let ratio_mean = if beta > 0.0 {
            (alpha / beta).clamp(1.0e-6, 1.0e6)
        } else {
            1.0
        };

        // Convert ratio to hashrate estimate
        let h_estimate = (ctx.current_hashrate as f64 * ratio_mean) as f32;

        let realized_share_per_min = if dt_secs > 0 {
            total_observed / (dt_secs as f64 / 60.0)
        } else {
            0.0
        };

        // Posterior standard deviation: sqrt(alpha / beta^2)
        let ratio_std = if beta > 0.0 {
            (alpha / (beta * beta)).sqrt()
        } else {
            1.0
        };
        let effective_n = beta; // beta ≈ total discounted exposure

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares_since_reset,
            dt_secs,
            uncertainty: Some(Uncertainty {
                ratio_std,
                effective_n,
            }),
        }
    }
}

/// Kalman filter estimator for hashrate tracking.
///
/// Models the hashrate ratio as a scalar state evolving with process noise:
///
/// ```text
/// State model:     ratio(t+1) = ratio(t) + w,   w ~ N(0, Q)
/// Observation:     shares(t) ~ Poisson(SPM * ratio(t) * tick/60)
///                  ≈ N(SPM * ratio * tick/60, SPM * ratio * tick/60)  [Gaussian approx]
/// ```
///
/// The Kalman filter produces both a point estimate (state mean) and
/// uncertainty (state covariance P) at each tick — exactly what the
/// pipeline's `Uncertainty` handoff requires.
///
/// ## Process noise Q
///
/// Q controls how much the filter expects hashrate to change between ticks:
/// - Small Q (e.g., 0.0001): assumes hashrate is nearly constant — smooth
///   estimate, slow to react to real changes
/// - Large Q (e.g., 0.01): assumes hashrate can change rapidly — responsive
///   but noisy
///
/// Analogous to EWMA's τ: `Q ≈ 1/τ²` roughly (larger Q = shorter memory).
///
/// ## Gaussian approximation
///
/// Valid for SPM ≥ 12 (expected shares ≥ 12 per tick). At SPM=6 the Poisson
/// is skewed; the filter still works but uncertainty estimates are less
/// accurate. The approximation `Var[shares] ≈ E[shares]` is used for the
/// measurement noise R.
///
/// ## on_fire behavior
///
/// Rescales the state estimate and covariance by the retarget ratio. Unlike
/// EWMA (which rescales its rate), the Kalman rescales the ratio state
/// itself: `new_ratio = old_ratio / retarget_factor` and P is preserved.
/// This means the filter retains confidence across fires.
#[derive(Debug, Clone)]
pub struct KalmanEstimator {
    /// State estimate: ratio = true_h / target_h.
    ratio_state: f64,
    /// State covariance (uncertainty squared).
    p: f64,
    /// Process noise variance per tick. Controls responsiveness.
    pub q: f64,
    /// Tick interval in seconds.
    pub tick_secs: u64,
    /// Shares accumulated since last fire (for reporting).
    shares_since_fire: u32,
    /// Ticks since last fire.
    n_ticks: u32,
}

impl KalmanEstimator {
    /// Constructs a Kalman estimator with the given process noise.
    ///
    /// `q` is process noise variance per tick. Good values:
    /// - 0.001: moderate responsiveness (similar to EWMA τ=120s)
    /// - 0.005: fast response (similar to EWMA τ=60s)
    /// - 0.0002: very smooth (similar to EWMA τ=300s)
    pub fn new(q: f64) -> Self {
        Self {
            ratio_state: 1.0,
            p: 1.0, // Start uncertain
            q: q.max(1.0e-12),
            tick_secs: 60,
            shares_since_fire: 0,
            n_ticks: 0,
        }
    }
}

impl Estimator for KalmanEstimator {
    fn observe(&mut self, n_shares: u32) {
        self.shares_since_fire = self.shares_since_fire.saturating_add(n_shares);
        self.n_ticks = self.n_ticks.saturating_add(1);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        if old_hashrate > 0.0 && new_hashrate > 0.0 {
            let retarget_ratio = new_hashrate as f64 / old_hashrate as f64;
            // After retarget, the new ratio relative to new target should be
            // old_ratio / retarget_ratio (since new_target = old_target * retarget_ratio)
            self.ratio_state /= retarget_ratio;
            // Covariance P is preserved — our confidence about the ratio
            // doesn't change just because the reference point shifted.
        } else {
            self.ratio_state = 1.0;
            self.p = 1.0;
        }
        self.shares_since_fire = 0;
        self.n_ticks = 0;
    }

    fn shares_count(&self) -> u32 {
        self.shares_since_fire
    }

    fn code(&self) -> String {
        format!("Kalman-q{}", self.q)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        // Run the Kalman predict+update for all accumulated ticks.
        // We do this at snapshot time (like Bayesian) because observe()
        // doesn't have SPM context.
        let mut ratio = self.ratio_state;
        let mut p = self.p;

        if self.n_ticks > 0 && dt_secs > 0 {
            let expected_shares_per_tick =
                (ctx.shares_per_minute as f64) * (self.tick_secs as f64 / 60.0);

            // Per-tick Kalman update (batch: apply n_ticks of predict+update)
            // For efficiency, we do the batch as if all shares arrived uniformly
            let shares_per_tick = self.shares_since_fire as f64 / self.n_ticks as f64;

            for _ in 0..self.n_ticks {
                // Predict step: state unchanged, covariance grows
                p += self.q;

                // Update step (Gaussian-approximated Poisson observation)
                // Measurement: z = shares_this_tick
                // Expected measurement: H = expected_shares_per_tick * ratio
                // Measurement noise: R = expected_shares_per_tick * ratio (Poisson variance)
                let h_matrix = expected_shares_per_tick; // dz/d_ratio
                let predicted_shares = expected_shares_per_tick * ratio;
                let r = predicted_shares.max(1.0); // measurement noise (Poisson variance)

                // Innovation
                let innovation = shares_per_tick - predicted_shares;

                // Innovation covariance: S = H*P*H' + R
                let s = h_matrix * p * h_matrix + r;

                if s > 0.0 {
                    // Kalman gain: K = P*H' / S
                    let k = (p * h_matrix) / s;

                    // State update
                    ratio += k * innovation;

                    // Covariance update: P = (1 - K*H) * P
                    p = (1.0 - k * h_matrix) * p;
                }

                // Clamp ratio to reasonable bounds
                ratio = ratio.clamp(1.0e-6, 1.0e6);
                p = p.max(1.0e-12);
            }
        }

        let h_estimate = (ctx.current_hashrate as f64 * ratio) as f32;
        let ratio_std = p.sqrt();
        let effective_n = if p > 0.0 { 1.0 / p } else { 1.0 };

        let realized_share_per_min = if dt_secs > 0 {
            self.shares_since_fire as f64 / (dt_secs as f64 / 60.0)
        } else {
            0.0
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: self.shares_since_fire,
            dt_secs,
            uncertainty: Some(Uncertainty {
                ratio_std,
                effective_n,
            }),
        }
    }
}

/// Dual-window EWMA estimator inspired by ckpool's `decay_time` function.
///
/// ckpool updates its EMA on every share submission using `decay_time()`:
/// ```text
/// fprop = 1 - e^(-elapsed / interval)
/// f += (share_diff / elapsed) * fprop
/// f /= (1 + fprop)
/// ```
///
/// In the tick-based framework, we simulate this by running ckpool's
/// exact `decay_time()` formula once per share within `snapshot()`,
/// distributing shares uniformly across the tick interval. With N shares
/// in a 60s tick, each gets `elapsed = 60/N` seconds — faithfully
/// reproducing ckpool's per-share EMA accumulation.
///
/// This avoids the time-bias correction problem: the EMA warms up
/// organically through simulated per-share updates rather than needing
/// a post-hoc `1/bias` amplification.
///
/// Two EMAs are maintained in parallel (short=60s, long=300s). The short
/// window is selected when total shares since last fire exceed the "fast"
/// threshold (72), matching ckpool's adaptive window switching for rapid
/// ramp-up of high-hashrate miners.
#[derive(Debug)]
pub struct CkpoolEstimator {
    /// Short-window time constant (seconds). ckpool uses 60.
    pub tau_short: u64,
    /// Long-window time constant (seconds). ckpool uses 300.
    pub tau_long: u64,
    /// Tick interval in seconds (must match trial driver).
    pub tick_secs: u64,
    /// Override for the fast-threshold. When `Some(n)`, the short window
    /// activates after `n` shares since last fire. When `None`, the
    /// threshold is derived as `tau_long * 0.8 / target_period_secs`.
    fast_threshold_override: Option<u32>,
    /// Target share period in seconds (ckpool uses 3.33).
    pub target_period_secs: f64,
    /// Short-window EMA: difficulty-weighted shares per second.
    dsps_short_bits: std::sync::atomic::AtomicU64,
    /// Long-window EMA: difficulty-weighted shares per second.
    dsps_long_bits: std::sync::atomic::AtomicU64,
    /// Shares accumulated since last snapshot (pending).
    pending_shares: std::sync::atomic::AtomicU32,
    /// Total shares since last fire (for adaptive window switching).
    shares_since_fire: std::sync::atomic::AtomicU32,
}

impl CkpoolEstimator {
    /// Construct with ckpool's default parameters: τ_short=60s, τ_long=300s,
    /// target period=3.33s (one share every 3.33 seconds → drr target 0.3).
    pub fn new(tau_short: u64, tau_long: u64) -> Self {
        use std::sync::atomic::{AtomicU32, AtomicU64};
        Self {
            tau_short,
            tau_long,
            tick_secs: 60,
            fast_threshold_override: None,
            target_period_secs: 3.33,
            dsps_short_bits: AtomicU64::new(0.0f64.to_bits()),
            dsps_long_bits: AtomicU64::new(0.0f64.to_bits()),
            pending_shares: AtomicU32::new(0),
            shares_since_fire: AtomicU32::new(0),
        }
    }

    /// Construct with a custom fast-threshold override. When shares since
    /// last fire reach this count, the estimator switches to the short
    /// window for faster responsiveness.
    pub fn with_fast_threshold(tau_short: u64, tau_long: u64, fast_threshold: u32) -> Self {
        let mut e = Self::new(tau_short, tau_long);
        e.fast_threshold_override = Some(fast_threshold);
        e
    }

    /// ckpool defaults: 60s short, 300s long, 3.33s target period.
    pub fn ckpool_defaults() -> Self {
        Self::new(60, 300)
    }

    /// The "fast" threshold: number of shares since last fire required to
    /// switch to the short-window EMA. ckpool uses 72 (= 240s / 3.33s).
    pub fn fast_threshold(&self) -> u32 {
        self.fast_threshold_override.unwrap_or_else(|| {
            ((self.tau_long as f64 * 0.8) / self.target_period_secs).round() as u32
        })
    }

    fn get_dsps_short(&self) -> f64 {
        f64::from_bits(
            self.dsps_short_bits
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    fn set_dsps_short(&self, v: f64) {
        self.dsps_short_bits
            .store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    fn get_dsps_long(&self) -> f64 {
        f64::from_bits(
            self.dsps_long_bits
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    fn set_dsps_long(&self, v: f64) {
        self.dsps_long_bits
            .store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// ckpool's `decay_time()` — the core EMA update per share.
    ///
    /// ```text
    /// fprop = 1 - e^(-elapsed / interval)
    /// f += (fadd / elapsed) * fprop
    /// f /= (1 + fprop)
    /// ```
    ///
    /// `f` is the running dsps (difficulty-shares per second).
    /// `fadd` is the difficulty of the submitted share (1.0 in our model).
    /// `elapsed` is seconds since the last share.
    /// `interval` is the time constant (tau).
    fn decay_time(f: f64, fadd: f64, elapsed: f64, interval: f64) -> f64 {
        if elapsed <= 0.0 {
            return f;
        }
        let dexp = (elapsed / interval).min(36.0);
        let fprop = 1.0 - (-dexp).exp();
        let ftotal = 1.0 + fprop;
        (f + (fadd / elapsed) * fprop) / ftotal
    }
}

impl Estimator for CkpoolEstimator {
    fn observe(&mut self, n_shares: u32) {
        *self.pending_shares.get_mut() = self.pending_shares.get_mut().saturating_add(n_shares);
        *self.shares_since_fire.get_mut() =
            self.shares_since_fire.get_mut().saturating_add(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.set_dsps_short(0.0);
            self.set_dsps_long(0.0);
            *self.shares_since_fire.get_mut() = 0;
            return;
        }

        // Rescale dsps: under new difficulty, same true hashrate produces
        // shares at rate = old_rate × (old_h / new_h).
        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.set_dsps_short(self.get_dsps_short() / ratio);
            self.set_dsps_long(self.get_dsps_long() / ratio);
        } else {
            self.set_dsps_short(0.0);
            self.set_dsps_long(0.0);
        }
        *self.shares_since_fire.get_mut() = 0;
    }

    fn shares_count(&self) -> u32 {
        self.pending_shares
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn code(&self) -> String {
        format!("Ckpool{}-{}s", self.tau_short, self.tau_long)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let pending = self
            .pending_shares
            .load(std::sync::atomic::Ordering::Relaxed);
        let shares_since_fire = self
            .shares_since_fire
            .load(std::sync::atomic::Ordering::Relaxed);

        // Simulate per-share decay_time() calls. Each share is treated
        // as arriving at uniform intervals within the tick: elapsed =
        // tick_secs / n_shares per share submission.
        let mut dsps_s = self.get_dsps_short();
        let mut dsps_l = self.get_dsps_long();

        if pending > 0 {
            let inter_share_secs = self.tick_secs as f64 / pending as f64;
            for _ in 0..pending {
                dsps_s = Self::decay_time(dsps_s, 1.0, inter_share_secs, self.tau_short as f64);
                dsps_l = Self::decay_time(dsps_l, 1.0, inter_share_secs, self.tau_long as f64);
            }
        } else {
            // No shares this tick: decay with full tick interval, fadd=0.
            // decay_time with fadd=0 simplifies to: f / (1 + fprop)
            let dexp_s = (self.tick_secs as f64 / self.tau_short as f64).min(36.0);
            let fprop_s = 1.0 - (-dexp_s).exp();
            dsps_s /= 1.0 + fprop_s;

            let dexp_l = (self.tick_secs as f64 / self.tau_long as f64).min(36.0);
            let fprop_l = 1.0 - (-dexp_l).exp();
            dsps_l /= 1.0 + fprop_l;
        }

        // Advance state.
        self.set_dsps_short(dsps_s);
        self.set_dsps_long(dsps_l);
        self.pending_shares
            .store(0, std::sync::atomic::Ordering::Relaxed);

        // Select which EMA to use based on share flood detection.
        let total_shares = shares_since_fire.saturating_add(pending);
        let dsps = if total_shares >= self.fast_threshold() {
            dsps_s
        } else {
            dsps_l
        };

        // Convert dsps (shares/sec) to shares/min, then to hashrate.
        let realized_share_per_min = dsps * 60.0;

        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: total_shares,
            dt_secs,
            uncertainty: None,
        }
    }
}

/// EWMA estimator with ckpool's time-bias warmup correction.
///
/// Identical to [`EwmaEstimator`] except `snapshot()` divides the raw
/// EWMA rate by `1 - e^(-dt_secs/tau_secs)` before converting to
/// hashrate. This compensates for the EMA being naturally suppressed
/// when the counter is young (few ticks since last fire), making the
/// estimator more accurate immediately after retarget events.
///
/// The time-bias idea comes from ckpool's `time_bias()` function in
/// `stratifier.c`. Empirically, ckpool's counter-age ratio advantage
/// (reacting faster with a mature counter) traces to this correction.
///
/// Use in place of `EwmaEstimator` to test whether time-bias correction
/// improves counter-age sensitivity and post-fire accuracy without
/// sacrificing steady-state performance.
#[derive(Debug)]
pub struct TimeBiasEwmaEstimator {
    /// Time constant in seconds.
    pub tau_secs: u64,
    /// Tick interval in seconds.
    pub tick_secs: u64,
    /// Current rate estimate (shares per tick), stored as bits.
    rate_bits: std::sync::atomic::AtomicU64,
    /// Pending shares since last snapshot.
    pending_shares: std::sync::atomic::AtomicU32,
    /// Ticks since last fire/reset.
    n_ticks: std::sync::atomic::AtomicU32,
}

impl TimeBiasEwmaEstimator {
    pub fn new(tau_secs: u64) -> Self {
        use std::sync::atomic::{AtomicU32, AtomicU64};
        Self {
            tau_secs,
            tick_secs: 60,
            rate_bits: AtomicU64::new(0.0f64.to_bits()),
            pending_shares: AtomicU32::new(0),
            n_ticks: AtomicU32::new(0),
        }
    }

    fn alpha(&self) -> f64 {
        (-(self.tick_secs as f64) / (self.tau_secs as f64)).exp()
    }

    fn get_rate(&self) -> f64 {
        f64::from_bits(self.rate_bits.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn set_rate(&self, rate: f64) {
        self.rate_bits
            .store(rate.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// ckpool's time_bias: `1 - e^(-elapsed/tau)`.
    fn time_bias(&self, dt_secs: u64) -> f64 {
        let dexp = (dt_secs as f64 / self.tau_secs as f64).min(36.0);
        1.0 - (-dexp).exp()
    }
}

impl Estimator for TimeBiasEwmaEstimator {
    fn observe(&mut self, n_shares: u32) {
        *self.pending_shares.get_mut() = self.pending_shares.get_mut().saturating_add(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        let pending = *self.pending_shares.get_mut();
        let n_ticks = *self.n_ticks.get_mut();

        if pending > 0 {
            let n = pending as f64;
            let rate = if n_ticks == 0 {
                n
            } else {
                let alpha = self.alpha();
                alpha * self.get_rate() + (1.0 - alpha) * n
            };
            self.set_rate(rate);
            *self.n_ticks.get_mut() = n_ticks + 1;
            *self.pending_shares.get_mut() = 0;
        }

        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
            return;
        }

        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.set_rate(self.get_rate() / ratio);
        } else {
            self.set_rate(0.0);
            *self.n_ticks.get_mut() = 0;
        }
    }

    fn shares_count(&self) -> u32 {
        self.pending_shares
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn code(&self) -> String {
        format!("TimeBiasEwma{}s", self.tau_secs)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let pending = self
            .pending_shares
            .load(std::sync::atomic::Ordering::Relaxed);
        let n_ticks = self.n_ticks.load(std::sync::atomic::Ordering::Relaxed);
        let n = pending as f64;

        let rate = if n_ticks == 0 {
            n
        } else {
            let alpha = self.alpha();
            alpha * self.get_rate() + (1.0 - alpha) * n
        };

        // Advance state.
        self.set_rate(rate);
        self.n_ticks
            .store(n_ticks + 1, std::sync::atomic::Ordering::Relaxed);
        self.pending_shares
            .store(0, std::sync::atomic::Ordering::Relaxed);

        // Apply time-bias correction: divide by `1 - e^(-dt/tau)` to
        // compensate for EMA warmup suppression when counter is young.
        let bias = self.time_bias(dt_secs);
        let corrected_rate = if bias > 0.01 { rate / bias } else { rate };

        let realized_share_per_min = corrected_rate * (60.0 / self.tick_secs as f64);
        let h_estimate = match hash_rate_from_target(
            ctx.current_target.to_le_bytes().into(),
            realized_share_per_min,
        ) {
            Ok(h) => h as f32,
            Err(_) => ctx.current_hashrate * realized_share_per_min as f32 / ctx.shares_per_minute,
        };

        EstimatorSnapshot {
            h_estimate,
            realized_share_per_min,
            n_shares: pending,
            dt_secs,
            uncertainty: None,
        }
    }
}

/// Steady-state debias wrapper for any inner estimator `E`.
///
/// Multiplies the inner estimator's hashrate belief (`h_estimate`) by a
/// fixed `bias` factor, leaving the raw `realized_share_per_min`
/// observation untouched. The motivation: an asymmetric, tighten-reluctant
/// boundary (the SignPersist champion) equilibrates at a point where the
/// under-difficulty deviation just fails to cross the elevated tighten
/// threshold — a persistent negative `regret_under` offset (≈ −7% in the
/// trajectory plot). A `bias > 1` lifts the belief so that equilibrium
/// sits closer to truth.
///
/// **Caveat (see `docs/THEORY.md` §5/§9):** the same scaled belief is what
/// the update rule retargets toward, so raising `bias` trades
/// `regret_under` down for `regret_over` up. Under the §10 cost
/// (`w_over:w_under = 3:1`) that trade is priced against you, so this is
/// expected to *reduce the settle gap but raise the cost* — it exists to
/// quantify that tradeoff curve and confirm whether the −7% offset is a
/// defect or the deliberate cost optimum, not as an obvious win.
#[derive(Debug)]
pub struct DebiasEstimator<E: Estimator> {
    /// The wrapped estimator.
    pub inner: E,
    /// Multiplicative bias on `h_estimate` (1.0 = no change; >1 lifts the
    /// belief, pushing the steady-state equilibrium toward truth).
    pub bias: f32,
}

impl<E: Estimator> DebiasEstimator<E> {
    pub fn new(inner: E, bias: f32) -> Self {
        Self {
            inner,
            bias: bias.max(0.0),
        }
    }
}

impl<E: Estimator> Estimator for DebiasEstimator<E> {
    fn observe(&mut self, n_shares: u32) {
        self.inner.observe(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        self.inner.on_fire(new_hashrate, old_hashrate);
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        let mut snap = self.inner.snapshot(dt_secs, ctx);
        snap.h_estimate *= self.bias;
        snap
    }

    fn shares_count(&self) -> u32 {
        self.inner.shares_count()
    }

    fn code(&self) -> String {
        format!("{}+bias{}", self.inner.code(), self.bias)
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
        e.on_fire(0.0, 0.0);
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
    fn debias_scales_h_estimate_only() {
        // The debias wrapper must multiply h_estimate by `bias` while
        // leaving the raw realized rate and share count untouched.
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };

        let mut base = CumulativeCounter::new();
        base.observe(12);
        let base_snap = base.snapshot(60, &ctx);

        let mut wrapped = DebiasEstimator::new(CumulativeCounter::new(), 1.1);
        wrapped.observe(12);
        let snap = wrapped.snapshot(60, &ctx);

        assert!(
            (snap.h_estimate - base_snap.h_estimate * 1.1).abs() <= base_snap.h_estimate * 1e-6,
            "h_estimate must be scaled by bias: got {}, want {}",
            snap.h_estimate,
            base_snap.h_estimate * 1.1
        );
        // Raw observation and count pass through unchanged.
        assert!((snap.realized_share_per_min - base_snap.realized_share_per_min).abs() < 1e-9);
        assert_eq!(snap.n_shares, base_snap.n_shares);
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

    /// Simulate one tick: add shares, then call snapshot to flush the
    /// EWMA state (mirroring how `Composed` drives the estimator).
    fn ewma_tick(e: &mut EwmaEstimator, n_shares: u32) {
        e.observe(n_shares);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        e.snapshot(60, &ctx);
    }

    #[test]
    fn ewma_first_observation_sets_rate_directly() {
        let mut e = EwmaEstimator::new(60);
        ewma_tick(&mut e, 12);
        assert!((e.rate_per_tick() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn ewma_converges_to_steady_input() {
        let mut e = EwmaEstimator::new(120);
        for _ in 0..200 {
            ewma_tick(&mut e, 12);
        }
        assert!((e.rate_per_tick() - 12.0).abs() < 1e-6);
    }

    // ===================================================================
    // ShareIndexedEstimator — P1 CONSTRUCTION CHECK (Stage 1 of the
    // pre-registered study). These verify STEADY-STATE construction only:
    // (does it track the optimum / coincide with the time-EWMA at r_ref).
    // They DO NOT validate TRANSIENT behavior — the per-share window
    // CONTRACTS at a rate spike (pending jumps → alpha shrinks → jumpier),
    // which is the mechanism the sweep grades and which steady-state ticks
    // never exercise. A clean pass here means "correctly built," NOT
    // "behaves as expected under the sweep" — that is Stage 2 (transient
    // trace), deliberately separate.
    // ===================================================================
    fn si_tick(e: &mut ShareIndexedEstimator, n_shares: u32) {
        e.observe(n_shares);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        e.snapshot(60, &ctx);
    }

    #[test]
    fn share_indexed_first_observation_sets_rate_directly() {
        let mut e = ShareIndexedEstimator::new(72.0);
        si_tick(&mut e, 12);
        assert!((e.rate_per_tick() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn share_indexed_converges_to_steady_input() {
        // Tracks a steady share rate — the basic construction check.
        let mut e = ShareIndexedEstimator::new(72.0);
        for _ in 0..400 {
            si_tick(&mut e, 12);
        }
        assert!((e.rate_per_tick() - 12.0).abs() < 1e-6);
    }

    #[test]
    fn share_indexed_coincides_with_time_ewma_at_reference_rate() {
        // THE GROUNDING-CORRECTNESS CHECK (load-bearing). At r_ref the two arms
        // must be IDENTICAL: n_span = r_ref·(tau/60). With tau=360s, r_ref=12 spm,
        // tick=60s → n_span=72, and each tick observes r_ref·tick/60 = 12 shares.
        // Then share-indexed alpha = exp(-12/72) must equal EWMA alpha =
        // exp(-60/360); both are exp(-1/6). Feed BOTH 12 shares/tick from the same
        // start and they must track to floating-point agreement — confirming the
        // grounding makes them the same controller at r_ref (so any later
        // difference is rate-awareness, not a window-size confound).
        let mut si = ShareIndexedEstimator::new_grounded(360, 12.0);
        let mut ew = EwmaEstimator::new(360);
        for _ in 0..200 {
            si_tick(&mut si, 12);
            ewma_tick(&mut ew, 12);
        }
        assert!(
            (si.rate_per_tick() - ew.rate_per_tick()).abs() < 1e-9,
            "share-indexed ({}) must coincide with time-EWMA ({}) at r_ref",
            si.rate_per_tick(),
            ew.rate_per_tick()
        );
        // and the grounded n_span is exactly 72.
        assert!((si.n_span - 72.0).abs() < 1e-9);
    }

    #[test]
    fn share_indexed_holds_estimate_on_empty_tick() {
        // Emergent rate-awareness: on an empty tick (no shares) alpha=exp(0)=1,
        // so the estimate is HELD — where a time-EWMA decays toward 0 on the same
        // empty tick. Establish a rate, then feed one empty tick; share-indexed
        // must be unchanged, EWMA must have decayed.
        let mut si = ShareIndexedEstimator::new(72.0);
        let mut ew = EwmaEstimator::new(360);
        for _ in 0..50 {
            si_tick(&mut si, 12);
            ewma_tick(&mut ew, 12);
        }
        let si_before = si.rate_per_tick();
        let ew_before = ew.rate_per_tick();
        si_tick(&mut si, 0); // empty tick
        ewma_tick(&mut ew, 0);
        assert!(
            (si.rate_per_tick() - si_before).abs() < 1e-12,
            "share-indexed must HOLD on an empty tick (no shares = no info)"
        );
        assert!(
            ew.rate_per_tick() < ew_before - 1e-6,
            "time-EWMA must DECAY on an empty tick (the difference under test)"
        );
    }

    #[test]
    fn ewma_seeded_starts_at_declared_rate_not_zero() {
        // Cold start believes nothing (rate 0 → 65-min ramp). A seed from the
        // declared nominal makes the rate ≈ the seed spm from cycle zero, before
        // any shares are observed. seed_spm=12, tick=60s → seed_rate=12/tick.
        let cold = EwmaEstimator::new(360);
        assert_eq!(cold.rate_per_tick(), 0.0);
        let seeded = EwmaEstimator::new_seeded(360, 12.0, 1);
        assert!((seeded.rate_per_tick() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn ewma_seed_is_overwritten_by_shares_within_tau() {
        // The seed is a prior, not a commitment: with small prior_ticks, a
        // wrong seed (declared 2× truth) is pulled toward the true share rate as
        // real observations arrive — converging to the same steady state a cold
        // start would reach. After ~τ of on-truth shares it must sit at truth.
        let mut e = EwmaEstimator::new_seeded(120, 24.0, 1); // seeded at 2× truth
        assert!((e.rate_per_tick() - 24.0).abs() < 1e-9);
        for _ in 0..200 {
            ewma_tick(&mut e, 12); // truth is 12 spm
        }
        assert!(
            (e.rate_per_tick() - 12.0).abs() < 1e-6,
            "seed not overwritten: {}",
            e.rate_per_tick()
        );
    }

    #[test]
    fn ewma_with_long_tau_responds_slowly_to_change() {
        let mut e = EwmaEstimator::new(600);
        for _ in 0..10 {
            ewma_tick(&mut e, 10);
        }
        assert!((e.rate_per_tick() - 10.0).abs() < 1.0);
        // Now switch to 0; one tick later the rate should still be
        // mostly 10 (long memory).
        ewma_tick(&mut e, 0);
        // alpha = exp(-60/600) ≈ 0.905. new rate ≈ 0.905 × 10 + 0.095 × 0 = 9.05
        assert!(
            e.rate_per_tick() > 8.5,
            "rate fell too fast: {}",
            e.rate_per_tick()
        );
        assert!(e.rate_per_tick() < 9.5);
    }

    #[test]
    fn ewma_with_short_tau_responds_fast_to_change() {
        let mut e = EwmaEstimator::new(30);
        for _ in 0..10 {
            ewma_tick(&mut e, 10);
        }
        // Switch to 0.
        ewma_tick(&mut e, 0);
        // alpha = exp(-60/30) = exp(-2) ≈ 0.135. new ≈ 0.135 × 10 + 0.865 × 0 = 1.35
        assert!(
            e.rate_per_tick() < 2.0,
            "rate fell too slow: {}",
            e.rate_per_tick()
        );
    }

    #[test]
    fn ewma_reset_clears_state() {
        let mut e = EwmaEstimator::new(60);
        for _ in 0..5 {
            ewma_tick(&mut e, 10);
        }
        e.on_fire(0.0, 0.0);
        assert_eq!(e.shares_count(), 0);
        // After reset, first observation again initializes directly.
        ewma_tick(&mut e, 42);
        assert!((e.rate_per_tick() - 42.0).abs() < 1e-9);
    }

    #[test]
    fn ewma_observe_accumulates_without_decay() {
        // Multiple observe(1) calls within a tick should produce the
        // same result as one observe(12) call.
        let mut e1 = EwmaEstimator::new(120);
        let mut e2 = EwmaEstimator::new(120);
        // First tick: seed both
        ewma_tick(&mut e1, 10);
        ewma_tick(&mut e2, 10);
        // Second tick: e1 gets bulk, e2 gets per-share
        e1.observe(12);
        for _ in 0..12 {
            e2.observe(1);
        }
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        let snap1 = e1.snapshot(60, &ctx);
        let snap2 = e2.snapshot(60, &ctx);
        assert!(
            (snap1.realized_share_per_min - snap2.realized_share_per_min).abs() < 1e-9,
            "bulk observe and per-share observe must produce identical results: {} vs {}",
            snap1.realized_share_per_min,
            snap2.realized_share_per_min,
        );
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
        use crate::target::{hash_rate_from_target, hash_rate_to_target};

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
                hashrate,
                configured,
                realized,
                inflation,
                recovered,
                linear_prediction,
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
        e.on_fire(0.0, 0.0);
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
            ewma_tick(&mut e, 12);
        }
        // rate_per_tick ≈ 12, tick_secs = 60.
        // shares_per_min = 12 * (60/60) = 12.
        // Add one more tick's worth and snapshot to check.
        e.observe(12);
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 12.0,
        };
        let snap = e.snapshot(60, &ctx);
        assert!((snap.realized_share_per_min - 12.0).abs() < 1e-3);
    }

    // ---- SpmRatioEstimator ----

    #[test]
    fn spm_ratio_converges_to_steady_input() {
        // Feed a constant stream of 12 shares per tick; the EWMA should
        // converge to 12 regardless of tau.
        let mut e = SpmRatioEstimator::new(120);
        for _ in 0..200 {
            e.observe(12);
        }
        assert!(
            (e.rate_per_tick() - 12.0).abs() < 1e-6,
            "rate_per_tick should converge to 12.0, got {}",
            e.rate_per_tick()
        );
    }

    #[test]
    fn spm_ratio_snapshot_returns_correct_estimate() {
        // At steady state with realized_spm == shares_per_minute,
        // h_estimate should equal current_hashrate.
        let mut e = SpmRatioEstimator::new(60);
        for _ in 0..100 {
            e.observe(30);
        }
        // rate_per_tick ≈ 30, tick_secs = 60, realized_spm = 30.
        let target = Target::MAX;
        let ctx = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 30.0,
        };
        let snap = e.snapshot(60, &ctx);
        // h_estimate = 1e15 * (30 / 30) = 1e15
        let relative_error = ((snap.h_estimate as f64) - 1.0e15).abs() / 1.0e15;
        assert!(
            relative_error < 1e-6,
            "h_estimate should be ~1e15, got {} (relative err {})",
            snap.h_estimate,
            relative_error
        );
        assert!((snap.realized_share_per_min - 30.0).abs() < 1e-6);

        // Now test with realized_spm != shares_per_minute (miner is 2x faster).
        let mut e2 = SpmRatioEstimator::new(60);
        for _ in 0..100 {
            e2.observe(60); // 60 shares per tick → 60 spm
        }
        let ctx2 = EstimatorContext {
            current_hashrate: 1.0e15,
            current_target: &target,
            shares_per_minute: 30.0,
        };
        let snap2 = e2.snapshot(60, &ctx2);
        // h_estimate = 1e15 * (60 / 30) = 2e15
        let relative_error2 = ((snap2.h_estimate as f64) - 2.0e15).abs() / 2.0e15;
        assert!(
            relative_error2 < 1e-6,
            "h_estimate should be ~2e15, got {} (relative err {})",
            snap2.h_estimate,
            relative_error2
        );
    }

    #[test]
    fn spm_ratio_on_fire_rescales_correctly() {
        let mut e = SpmRatioEstimator::new(60);
        for _ in 0..50 {
            e.observe(30);
        }
        // rate_per_tick ≈ 30.0
        let rate_before = e.rate_per_tick();

        // Fire: new target is 2x harder (hashrate doubled).
        // Expected: rate_per_tick /= 2.0 (miner finds shares half as often).
        e.on_fire(2.0e15, 1.0e15);
        let expected = rate_before / 2.0;
        assert!(
            (e.rate_per_tick() - expected).abs() < 1e-9,
            "rate_per_tick should be {} after on_fire, got {}",
            expected,
            e.rate_per_tick()
        );
        // n_observations preserved (not reset).
        assert!(e.n_observations > 0);
    }

    #[test]
    fn spm_ratio_on_fire_with_zero_hashrate_resets() {
        let mut e = SpmRatioEstimator::new(60);
        for _ in 0..10 {
            e.observe(10);
        }
        // Zero old_hashrate → full reset.
        e.on_fire(1.0e15, 0.0);
        assert_eq!(e.rate_per_tick(), 0.0);
        assert_eq!(e.n_observations, 0);
    }
}
