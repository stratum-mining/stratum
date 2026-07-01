//! UpdateRule — Stage 3 of the three-stage vardiff pipeline.
//!
//! The UpdateRule decides, when the algorithm has chosen to fire, by how
//! much it moves the target. It is the piece of the algorithm that lives
//! in control theory: design pressure is convergence speed (large updates,
//! faster adjustment) vs stability of the steady state (small updates,
//! less jitter).

use super::estimator::EstimatorSnapshot;
use std::fmt::Debug;
use std::sync::atomic::{AtomicI8, AtomicU32, Ordering};

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
    /// Compute the new target hashrate after the Boundary decided to fire.
    ///
    /// `threshold` is the boundary's threshold that delta exceeded. The
    /// margin `(delta - threshold)` indicates how decisive the fire was.
    /// Update rules can use this to modulate aggressiveness: move more
    /// when the evidence is overwhelming, less when marginal.
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        delta: f64,
        threshold: f64,
        shares_per_minute: f32,
    ) -> f32;

    /// A short, drift-proof code derived from the update rule's actual
    /// parameters. See [`super::Estimator::code`] for the rationale.
    fn code(&self) -> String;
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
        _threshold: f64,
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

    fn code(&self) -> String {
        "FullClamp".to_string()
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
    /// `FullRemedy` uses a tighter `η = 0.2` instead, chosen via the
    /// `sweep-eta` and joint `sweep-eta-z` Pareto sweeps as the
    /// convergence-vs-overshoot balance point — see the algorithm
    /// registry in `DESIGN.md`.
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
        _threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        current_hashrate + self.eta * (snap.h_estimate - current_hashrate)
    }

    fn code(&self) -> String {
        format!("Partial-e{}", self.eta)
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
        _threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        // Trust the estimator's belief. The min_allowed_hashrate floor
        // is applied at the Composed-adapter level, so we don't clamp
        // here even when h_estimate is below it.
        snap.h_estimate.max(0.0)
    }

    fn code(&self) -> String {
        "FullNoClamp".to_string()
    }
}

/// Margin-aware partial retarget: scales η by how decisive the fire was.
///
/// ```text
/// margin = delta - threshold
/// scale = clamp(margin / reference_margin, min_scale, max_scale)
/// η_effective = η_base × scale
/// new_hashrate = current + η_effective × (h_estimate - current)
/// ```
///
/// When the fire is overwhelmingly decisive (cold start: delta=10000%,
/// threshold=118%), the scale maxes out and the algorithm moves
/// aggressively toward the estimate. When the fire is marginal (stable
/// load noise: delta barely crosses threshold), the scale is small and
/// the algorithm makes a conservative adjustment.
///
/// This unifies fast cold-start convergence with stable-load damping
/// in a single update rule — no separate "Phase 1" logic needed.
#[derive(Debug, Clone, Copy)]
pub struct AdaptivePartialRetarget {
    /// Base damping factor. The floor when margin = reference_margin.
    pub eta_base: f32,
    /// The margin value at which η_effective = η_base (the "normal" case).
    pub reference_margin: f64,
    /// Minimum scale factor (caps how conservative marginal fires are).
    pub min_scale: f32,
    /// Maximum scale factor (caps how aggressive decisive fires are).
    pub max_scale: f32,
}

impl AdaptivePartialRetarget {
    pub fn new(eta_base: f32, reference_margin: f64) -> Self {
        Self {
            eta_base,
            reference_margin,
            min_scale: 1.0,
            max_scale: 3.0,
        }
    }

    pub fn with_scales(
        eta_base: f32,
        reference_margin: f64,
        min_scale: f32,
        max_scale: f32,
    ) -> Self {
        Self {
            eta_base,
            reference_margin,
            min_scale,
            max_scale,
        }
    }
}

impl UpdateRule for AdaptivePartialRetarget {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        delta: f64,
        threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        let margin = (delta - threshold).max(0.0);
        let scale = if self.reference_margin > 0.0 {
            (margin / self.reference_margin) as f32
        } else {
            1.0
        };
        let scale = scale.clamp(self.min_scale, self.max_scale);
        let eta = (self.eta_base * scale).min(1.0);
        current_hashrate + eta * (snap.h_estimate - current_hashrate)
    }

    fn code(&self) -> String {
        format!("AdaptPartial-e{}-m{}", self.eta_base, self.reference_margin)
    }
}

/// Accelerating partial retarget: tracks consecutive same-direction fires
/// and accelerates η accordingly.
///
/// ```text
/// direction = sign(h_estimate - current_hashrate)
/// if direction == last_direction:
///     consecutive_same_direction += 1
/// else:
///     consecutive_same_direction = 1
///     last_direction = direction
/// η_effective = min(eta_base + acceleration × (consecutive - 1), eta_max)
/// new_hashrate = current + η_effective × (h_estimate - current)
/// ```
///
/// The intuition is that a single fire near the boundary is uncertain —
/// use the conservative `eta_base`. But when the estimator keeps pulling
/// in the same direction across multiple fires, confidence grows and the
/// controller should converge faster. The acceleration ramps η up to
/// `eta_max` over consecutive same-direction fires.
///
/// This variant is a drop-in replacement for [`PartialRetarget`] in
/// scenarios where slow convergence at `η = 0.2` is acceptable for
/// stability but cold-start or step-change convergence needs a boost
/// without switching to [`AdaptivePartialRetarget`]'s margin-based
/// scaling.
pub struct AcceleratingPartialRetarget {
    /// Starting damping factor (e.g., 0.2).
    pub eta_base: f32,
    /// Maximum damping factor cap (e.g., 0.6).
    pub eta_max: f32,
    /// η increase per consecutive same-direction fire (e.g., 0.1).
    pub acceleration: f32,
    /// Number of consecutive fires in the same direction.
    consecutive_same_direction: AtomicU32,
    /// Last observed direction: +1 (tighten), -1 (loosen), 0 (initial).
    last_direction: AtomicI8,
}

impl Debug for AcceleratingPartialRetarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcceleratingPartialRetarget")
            .field("eta_base", &self.eta_base)
            .field("eta_max", &self.eta_max)
            .field("acceleration", &self.acceleration)
            .field(
                "consecutive_same_direction",
                &self.consecutive_same_direction.load(Ordering::Relaxed),
            )
            .field(
                "last_direction",
                &self.last_direction.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl AcceleratingPartialRetarget {
    pub fn new(eta_base: f32, eta_max: f32, acceleration: f32) -> Self {
        Self {
            eta_base,
            eta_max,
            acceleration,
            consecutive_same_direction: AtomicU32::new(0),
            last_direction: AtomicI8::new(0),
        }
    }
}

impl UpdateRule for AcceleratingPartialRetarget {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        _delta: f64,
        _threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        // Determine direction: +1 if estimate pulls up (tighten), -1 if down.
        let direction: i8 = if snap.h_estimate > current_hashrate {
            1
        } else {
            -1
        };

        let last = self.last_direction.load(Ordering::Relaxed);
        let consecutive = if direction == last {
            let c = self
                .consecutive_same_direction
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            c
        } else {
            self.last_direction.store(direction, Ordering::Relaxed);
            self.consecutive_same_direction.store(1, Ordering::Relaxed);
            1
        };

        let eta_effective =
            (self.eta_base + self.acceleration * (consecutive - 1) as f32).min(self.eta_max);

        current_hashrate + eta_effective * (snap.h_estimate - current_hashrate)
    }

    fn code(&self) -> String {
        format!(
            "Accel-{}-{}-{}",
            self.eta_base, self.eta_max, self.acceleration
        )
    }
}

/// Like [`AcceleratingPartialRetarget`] but only accelerates after the
/// first direction reversal. During cold-start ramp (all fires in one
/// direction, no reversal yet), η stays at `eta_base`. Once the algorithm
/// has reversed direction at least once (proving it has crossed the target
/// and is now tracking), acceleration kicks in normally.
///
/// This prevents cold-start overshoot: the initial ramp uses conservative
/// η=0.2 even on consecutive same-direction fires, then after the first
/// overshoot/reversal, subsequent step-changes get the acceleration benefit.
pub struct GuardedAccelRetarget {
    pub eta_base: f32,
    pub eta_max: f32,
    pub acceleration: f32,
    consecutive_same_direction: AtomicU32,
    last_direction: AtomicI8,
    /// Set to true after the first direction reversal.
    has_reversed: std::sync::atomic::AtomicBool,
}

impl Debug for GuardedAccelRetarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardedAccelRetarget")
            .field("eta_base", &self.eta_base)
            .field("eta_max", &self.eta_max)
            .field("acceleration", &self.acceleration)
            .field(
                "consecutive_same_direction",
                &self.consecutive_same_direction.load(Ordering::Relaxed),
            )
            .field("has_reversed", &self.has_reversed.load(Ordering::Relaxed))
            .finish()
    }
}

impl GuardedAccelRetarget {
    pub fn new(eta_base: f32, eta_max: f32, acceleration: f32) -> Self {
        Self {
            eta_base,
            eta_max,
            acceleration,
            consecutive_same_direction: AtomicU32::new(0),
            last_direction: AtomicI8::new(0),
            has_reversed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl UpdateRule for GuardedAccelRetarget {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        _delta: f64,
        _threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        let direction: i8 = if snap.h_estimate > current_hashrate {
            1
        } else {
            -1
        };

        let last = self.last_direction.load(Ordering::Relaxed);
        let consecutive = if direction == last {
            self.consecutive_same_direction
                .fetch_add(1, Ordering::Relaxed)
                + 1
        } else {
            if last != 0 {
                self.has_reversed.store(true, Ordering::Relaxed);
            }
            self.last_direction.store(direction, Ordering::Relaxed);
            self.consecutive_same_direction.store(1, Ordering::Relaxed);
            1
        };

        let eta_effective = if self.has_reversed.load(Ordering::Relaxed) {
            (self.eta_base + self.acceleration * (consecutive - 1) as f32).min(self.eta_max)
        } else {
            self.eta_base
        };

        current_hashrate + eta_effective * (snap.h_estimate - current_hashrate)
    }

    fn code(&self) -> String {
        format!(
            "GuardAccel-{}-{}-{}",
            self.eta_base, self.eta_max, self.acceleration
        )
    }
}

/// ckpool-style full retarget: `optimal = dsps × target_period`.
///
/// ckpool computes the optimal difficulty as `dsps × 3.33` (shares per
/// second × target period in seconds), then clamps to [mindiff, maxdiff].
/// The result is a hashrate that would produce exactly one share per
/// `target_period` seconds at the observed rate.
///
/// Additionally implements ckpool's **oscillation guard**: if the
/// algorithm wants to *decrease* difficulty but this is the very first
/// evaluation after a previous fire (shares_since_fire ≤ threshold),
/// suppress the decrease. This prevents a scenario where a miner returns
/// from idle, submits one batch of shares at the old rate, and
/// immediately gets difficulty reduced.
///
/// In the three-stage framework, the oscillation guard is approximated
/// by checking `snap.n_shares` against a threshold. When few shares have
/// been observed and the direction is down, the update rule returns the
/// current hashrate unchanged (the Composed adapter interprets `Some(current)`
/// as a no-op since it equals the existing target).
///
/// ## Difference from FullRetargetNoClamp
///
/// `FullRetargetNoClamp` trusts `h_estimate` directly. `CkpoolRetarget`
/// also trusts the estimator but adds the oscillation guard — a
/// directional asymmetry where decreases require more evidence than
/// increases.
#[derive(Debug, Clone, Copy)]
pub struct CkpoolRetarget {
    /// Minimum shares required before allowing a difficulty decrease.
    /// ckpool uses 1 (suppresses decrease on the very first share after
    /// a change). In the tick-based framework, this is scaled to the
    /// minimum tick's worth of data.
    pub min_shares_for_decrease: u32,
}

impl CkpoolRetarget {
    /// Construct with ckpool's default: suppress decrease when only 1
    /// share (or tick's worth) has been observed since last fire.
    pub fn ckpool_defaults() -> Self {
        Self {
            min_shares_for_decrease: 1,
        }
    }

    /// Construct with custom minimum shares threshold.
    pub fn new(min_shares_for_decrease: u32) -> Self {
        Self {
            min_shares_for_decrease,
        }
    }
}

impl UpdateRule for CkpoolRetarget {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        _delta: f64,
        _threshold: f64,
        _shares_per_minute: f32,
    ) -> f32 {
        let target = snap.h_estimate;

        // Oscillation guard: suppress difficulty decrease when insufficient
        // evidence has accumulated (ckpool's `if (optimal < client->diff &&
        // client->ssdc == 1) return` logic).
        if target < current_hashrate && snap.n_shares <= self.min_shares_for_decrease {
            return current_hashrate;
        }

        target
    }

    fn code(&self) -> String {
        format!("CkpoolRetgt-m{}", self.min_shares_for_decrease)
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
            uncertainty: None,
        }
    }

    #[test]
    fn zero_shares_short_dt_divides_by_one_point_five() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 20), 1.0e15, 100.0, 50.0, 12.0);
        assert!((h / (1.0e15 / 1.5) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_shares_medium_dt_divides_by_two() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 45), 1.0e15, 100.0, 50.0, 12.0);
        assert!((h / (1.0e15 / 2.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_shares_long_dt_divides_by_three() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(0.0, 0.0, 120), 1.0e15, 100.0, 50.0, 12.0);
        assert!((h / (1.0e15 / 3.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn large_delta_long_dt_multiplies_by_three() {
        let u = FullRetargetWithClamp::classic();
        // δ = 1500% > 1000% triggers the clamp; dt = 60 → ×3.
        let h = u.next_hashrate(&snap(1.0e20, 1200.0, 60), 1.0e10, 1500.0, 50.0, 12.0);
        assert!((h / 3.0e10 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn large_delta_short_dt_multiplies_by_ten() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(1.0e20, 1200.0, 30), 1.0e10, 1500.0, 50.0, 12.0);
        assert!((h / 1.0e11 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normal_delta_returns_h_estimate() {
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(2.0e15, 24.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        assert_eq!(h, 2.0e15);
    }

    #[test]
    fn exactly_one_thousand_delta_does_not_clamp() {
        // The classic code uses `> 1000.0`, not `>=`. Verify the boundary.
        let u = FullRetargetWithClamp::classic();
        let h = u.next_hashrate(&snap(1.1e16, 24.0, 60), 1.0e15, 1000.0, 50.0, 12.0);
        assert_eq!(h, 1.1e16); // h_estimate, not clamped
    }

    // ---- PartialRetarget ----

    #[test]
    fn partial_retarget_eta_half_moves_halfway() {
        let u = PartialRetarget::default_ewma();
        // current = 1e15, h_estimate = 2e15. η = 0.5.
        // new = 1e15 + 0.5 × (2e15 − 1e15) = 1.5e15.
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        assert!((h - 1.5e15).abs() / h < 1e-5, "got {}", h);
    }

    #[test]
    fn partial_retarget_eta_one_is_full_retarget() {
        let u = PartialRetarget::new(1.0);
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        assert!((h - 2.0e15).abs() / h < 1e-5);
    }

    #[test]
    fn partial_retarget_eta_zero_keeps_current() {
        let u = PartialRetarget::new(0.0);
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        assert_eq!(h, 1.0e15);
    }

    #[test]
    fn partial_retarget_no_zero_shares_branch() {
        // Unlike FullRetargetWithClamp, PartialRetarget doesn't special-
        // case `realized == 0`. h_estimate is trusted directly.
        let u = PartialRetarget::default_ewma();
        let h = u.next_hashrate(&snap(0.5e15, 0.0, 60), 1.0e15, 50.0, 50.0, 12.0);
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
            let h = u.next_hashrate(&snap(estimate, 100.0, 60), 1.0e15, 50.0, 50.0, 12.0);
            assert_eq!(h, estimate, "h_estimate={} should pass through", estimate);
        }
    }

    #[test]
    fn full_retarget_no_clamp_ignores_zero_shares_and_large_delta() {
        // Distinct from FullRetargetWithClamp: zero-shares doesn't
        // trigger down-clamp; large delta doesn't trigger up-clamp.
        let u = FullRetargetNoClamp;
        let h_zero = u.next_hashrate(&snap(0.5e15, 0.0, 60), 1.0e15, 50.0, 50.0, 12.0);
        assert_eq!(h_zero, 0.5e15);
        let h_large = u.next_hashrate(&snap(1.5e20, 1e7, 60), 1.0e10, 9999.0, 50.0, 12.0);
        assert_eq!(h_large, 1.5e20);
    }

    #[test]
    fn full_retarget_no_clamp_floors_negative_h_estimate_at_zero() {
        // A pathological estimator producing a negative h_estimate is
        // floored at 0.0. The min_allowed_hashrate floor in the
        // Composed adapter handles the production lower bound.
        let u = FullRetargetNoClamp;
        let h = u.next_hashrate(&snap(-1.0e10, 100.0, 60), 1.0e15, 50.0, 50.0, 12.0);
        assert_eq!(h, 0.0);
    }

    // ---- AcceleratingPartialRetarget ----

    #[test]
    fn accelerating_first_fire_uses_eta_base() {
        let u = AcceleratingPartialRetarget::new(0.2, 0.6, 0.1);
        // current = 1e15, h_estimate = 2e15, direction = +1 (first fire).
        // η_effective = 0.2 + 0.1 × (1 - 1) = 0.2.
        // new = 1e15 + 0.2 × (2e15 − 1e15) = 1.2e15.
        let h = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        assert!(
            (h - 1.2e15).abs() / 1.2e15 < 1e-5,
            "first fire: expected 1.2e15, got {}",
            h
        );
    }

    #[test]
    fn accelerating_second_same_direction_uses_eta_base_plus_acceleration() {
        let u = AcceleratingPartialRetarget::new(0.2, 0.6, 0.1);
        // First fire: direction = +1.
        let h1 = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        // Second fire: same direction (+1). consecutive = 2.
        // η_effective = 0.2 + 0.1 × (2 - 1) = 0.3.
        // new = h1 + 0.3 × (2.0e15 − h1).
        let h2 = u.next_hashrate(&snap(2.0e15, 12.0, 60), h1, 100.0, 50.0, 12.0);
        let expected = h1 + 0.3 * (2.0e15 - h1);
        assert!(
            (h2 - expected).abs() / expected < 1e-5,
            "second same-dir fire: expected {}, got {}",
            expected,
            h2
        );
    }

    #[test]
    fn accelerating_direction_reversal_resets_to_eta_base() {
        let u = AcceleratingPartialRetarget::new(0.2, 0.6, 0.1);
        // Fire twice in +1 direction to build up consecutive count.
        let h1 = u.next_hashrate(&snap(2.0e15, 12.0, 60), 1.0e15, 100.0, 50.0, 12.0);
        let _h2 = u.next_hashrate(&snap(2.0e15, 12.0, 60), h1, 100.0, 50.0, 12.0);
        // Now fire in -1 direction (h_estimate < current). Resets to 1.
        // η_effective = 0.2 + 0.1 × (1 - 1) = 0.2.
        let current = 2.0e15;
        let estimate = 1.0e15;
        let h3 = u.next_hashrate(&snap(estimate, 12.0, 60), current, 100.0, 50.0, 12.0);
        let expected = current + 0.2 * (estimate - current);
        assert!(
            (h3 - expected).abs() / expected < 1e-5,
            "direction reversal: expected {}, got {}",
            expected,
            h3
        );
    }

    #[test]
    fn accelerating_caps_at_eta_max() {
        let u = AcceleratingPartialRetarget::new(0.2, 0.6, 0.1);
        // Fire 10 times in the same direction to exceed eta_max.
        let mut current = 1.0e15;
        for _ in 0..10 {
            current = u.next_hashrate(&snap(2.0e15, 12.0, 60), current, 100.0, 50.0, 12.0);
        }
        // By the 10th fire: consecutive = 10, η = 0.2 + 0.1 × 9 = 1.1 → capped at 0.6.
        // Verify the 11th fire uses η_max = 0.6.
        let h_before = current;
        let h_after = u.next_hashrate(&snap(2.0e15, 12.0, 60), h_before, 100.0, 50.0, 12.0);
        let expected = h_before + 0.6 * (2.0e15 - h_before);
        assert!(
            (h_after - expected).abs() / expected < 1e-5,
            "eta_max cap: expected {}, got {}",
            expected,
            h_after
        );
    }
}
