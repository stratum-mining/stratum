use crate::target::hash_rate_from_target;
use bitcoin::Target;
use tracing::debug;

use super::{error::VardiffError, Vardiff};

/// Default minimum hashrate (H/s) if not specified.
const DEFAULT_MIN_HASHRATE: f32 = 1.0;

/// Decline-safe adaptive EWMA vardiff algorithm (the champion).
///
/// Three inline stages per evaluation tick. The parameters here are the
/// decline-safety-selected champion: the gentlest composition that never enters
/// the over-difficulty death-spiral on a sustained decline (selected by a minimax
/// over the decline-safety gate, not by a scalar fitness score). Behaviorally
/// identical to the `champion_composed` reference in the research framework (#2154).
///
/// 1. **Estimator** — EWMA-smoothed share rate (tau=360s, tick=60s) converts
///    observed shares into a hashrate belief via `hash_rate_from_target`. The 360s
///    time constant is the floor of the τ-safety-valley: shorter windows fail the
///    slow-decline gate at sparse rates; longer ones lag without benefit.
///
/// 2. **Boundary** — Adaptive threshold selection based on the miner's
///    configured shares-per-minute:
///    - Below `spm_threshold` (6): PoissonCI — wide confidence interval
///      prevents premature fires on sparse data (small miners / bitaxe).
///    - At or above `spm_threshold`: sign-persistence CUSUM — a sequential-testing
///      boundary with two protections of the dangerous (tightening) direction:
///      an asymmetric multiplier (tightening requires `cusum_tighten_multiplier`×
///      more evidence) and a sign-persistence discount (the threshold relaxes only
///      after `consecutive` same-direction ticks accumulate, so a single lucky
///      streak cannot trip a tightening). This is dangerous-direction protection:
///      tightening into a falling miner is the move that spirals, so the boundary
///      resists it as a constraint. (NOTE: this is *not* justified by "tightening
///      rejects in-flight shares" — it does not; the pool validates each share
///      against the per-job target snapshotted at job creation. The justification
///      is decline-safety, not lost work.)
///
/// 3. **Update** — Accelerating partial retarget: `eta` starts at 0.2 and
///    ramps by `acceleration` per consecutive same-direction fire, capping at 0.6.
///    Direction reversals reset the counter to `eta_base`.
#[derive(Debug)]
pub struct VardiffState {
    /// Count of shares received since the last difficulty adjustment.
    pub shares_since_last_update: u32,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// The lowest hashrate (H/s) the system will allow; values below this are clamped.
    pub min_allowed_hashrate: f32,

    // -- EWMA estimator state --
    tick_secs: u64,
    tau_secs: u64,
    rate: f64,
    n_ticks: u32,

    // -- Adaptive boundary params --
    spm_threshold: u32,
    poisson_z: f64,
    poisson_margin: f64,
    cusum_sensitivity: f64,
    cusum_floor: f64,
    cusum_tighten_multiplier: f64,
    cusum_reference_spm: f64,
    /// Threshold reduction per consecutive same-sign tick (fractional).
    cusum_sign_persistence_discount: f64,
    /// Maximum total sign-persistence discount (fractional cap).
    cusum_max_sign_discount: f64,
    /// Sign of the last boundary observation (realized vs target): +1 / -1 / 0.
    cusum_last_sign: i8,
    /// Count of consecutive same-sign boundary observations.
    cusum_consecutive: u32,

    // -- Accelerating partial retarget state --
    eta_base: f32,
    eta_max: f32,
    acceleration: f32,
    consecutive_same_direction: u32,
    last_direction: i8,
}

impl VardiffState {
    /// Creates a new `VardiffState` with the default minimum hashrate.
    pub fn new() -> Result<Self, VardiffError> {
        Self::new_with_min(DEFAULT_MIN_HASHRATE)
    }

    /// Creates a new `VardiffState` with a specific minimum hashrate.
    pub fn new_with_min(min_allowed_hashrate: f32) -> Result<Self, VardiffError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        Ok(Self {
            shares_since_last_update: 0,
            timestamp_of_last_update: now,
            min_allowed_hashrate,
            tick_secs: 60,
            tau_secs: 360,
            rate: 0.0,
            n_ticks: 0,
            spm_threshold: 6,
            poisson_z: 2.576,
            poisson_margin: 0.05,
            cusum_sensitivity: 1.5,
            cusum_floor: 0.05,
            cusum_tighten_multiplier: 8.0,
            cusum_reference_spm: 30.0,
            cusum_sign_persistence_discount: 0.06,
            cusum_max_sign_discount: 0.6,
            cusum_last_sign: 0,
            cusum_consecutive: 0,
            eta_base: 0.2,
            eta_max: 0.6,
            acceleration: 0.05,
            consecutive_same_direction: 0,
            last_direction: 0,
        })
    }

    /// Sets the count of shares since the last update.
    pub fn set_shares_since_last_update(&mut self, shares_since_last_update: u32) {
        self.shares_since_last_update = shares_since_last_update;
    }

    /// Test-only: the sign-persistence boundary state `(last_sign, consecutive)`.
    /// Exposed so a reset-cleanliness test can assert these fields are zeroed
    /// directly (their effect on the threshold is too small to observe via fire
    /// behavior — see `reset_counter_clears_sign_persistence_state`).
    #[cfg(test)]
    pub(crate) fn cusum_sign_state(&self) -> (i8, u32) {
        (self.cusum_last_sign, self.cusum_consecutive)
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_secs()
    }

    fn ewma_alpha(&self) -> f64 {
        (-(self.tick_secs as f64) / (self.tau_secs as f64)).exp()
    }

    /// Stage 1: Flush pending shares into the EWMA and produce a hashrate estimate.
    fn estimate(&mut self, hashrate: f32, target: &Target, shares_per_minute: f32) -> (f64, f32) {
        let n = self.shares_since_last_update as f64;

        let rate = if self.n_ticks == 0 {
            n
        } else {
            let alpha = self.ewma_alpha();
            alpha * self.rate + (1.0 - alpha) * n
        };

        self.rate = rate;
        self.n_ticks += 1;
        self.shares_since_last_update = 0;

        let realized_share_per_min = rate * (60.0 / self.tick_secs as f64);

        let h_estimate =
            match hash_rate_from_target(target.to_le_bytes().into(), realized_share_per_min) {
                Ok(h) => h as f32,
                Err(_) => hashrate * realized_share_per_min as f32 / shares_per_minute,
            };

        (realized_share_per_min, h_estimate)
    }

    /// Stage 2: Compute decision threshold. Takes `&mut self` because the
    /// sign-persistence CUSUM updates its consecutive-tick state each evaluation.
    fn threshold(&mut self, dt_secs: u64, shares_per_minute: f32, realized_spm: f64) -> f64 {
        if (shares_per_minute as u32) < self.spm_threshold {
            self.poisson_threshold(dt_secs, shares_per_minute, realized_spm)
        } else {
            self.cusum_threshold(dt_secs, shares_per_minute, realized_spm)
        }
    }

    fn poisson_threshold(&self, dt_secs: u64, shares_per_minute: f32, realized_spm: f64) -> f64 {
        let lambda_bar = (shares_per_minute as f64 / 60.0) * dt_secs as f64;
        if lambda_bar <= 0.0 {
            return 100.0;
        }
        let bound_fraction =
            (self.poisson_z * lambda_bar.sqrt() + 0.5) / lambda_bar + self.poisson_margin;
        let base = bound_fraction * 100.0;

        let would_tighten = realized_spm > shares_per_minute as f64;
        if would_tighten {
            base * self.cusum_tighten_multiplier
        } else {
            base
        }
    }

    fn cusum_threshold(&mut self, dt_secs: u64, shares_per_minute: f32, realized_spm: f64) -> f64 {
        let n_ticks = (dt_secs as f64 / self.tick_secs as f64).max(1.0);

        let spm_factor = ((shares_per_minute as f64) / self.cusum_reference_spm).sqrt();
        let sensitivity = self.cusum_sensitivity * spm_factor;

        let base_threshold = (sensitivity / n_ticks) + self.cusum_floor;

        // Asymmetric tighten multiplier: tightening is the dangerous direction.
        let would_tighten = realized_spm > shares_per_minute as f64;
        let asymmetric_threshold = if would_tighten {
            base_threshold * self.cusum_tighten_multiplier
        } else {
            base_threshold
        };

        // Sign-persistence: the threshold relaxes only after consecutive
        // same-direction ticks accumulate, so a single lucky streak can't trip a
        // (tightening) fire. Discount = discount_per_tick × (consecutive − 1),
        // capped at max_sign_discount.
        let current_sign: i8 = if realized_spm > shares_per_minute as f64 {
            1
        } else {
            -1
        };
        let consecutive = if current_sign == self.cusum_last_sign {
            self.cusum_consecutive = self.cusum_consecutive.saturating_add(1);
            self.cusum_consecutive
        } else {
            self.cusum_last_sign = current_sign;
            self.cusum_consecutive = 1;
            1
        };
        let discount = (self.cusum_sign_persistence_discount * (consecutive - 1) as f64)
            .min(self.cusum_max_sign_discount);

        asymmetric_threshold * (1.0 - discount) * 100.0
    }

    /// Stage 3: Compute new hashrate with accelerating partial retarget.
    fn compute_new_hashrate(&mut self, h_estimate: f32, current_hashrate: f32) -> f32 {
        let direction: i8 = if h_estimate > current_hashrate { 1 } else { -1 };

        let consecutive = if direction == self.last_direction {
            self.consecutive_same_direction += 1;
            self.consecutive_same_direction
        } else {
            self.last_direction = direction;
            self.consecutive_same_direction = 1;
            1
        };

        let eta = (self.eta_base + self.acceleration * (consecutive - 1) as f32).min(self.eta_max);
        current_hashrate + eta * (h_estimate - current_hashrate)
    }

    /// Rescale the EWMA after a fire so the rate reflects the new difficulty.
    fn rescale_ewma(&mut self, new_hashrate: f32, old_hashrate: f32) {
        if old_hashrate <= 0.0 || new_hashrate <= 0.0 {
            self.rate = 0.0;
            self.n_ticks = 0;
            self.shares_since_last_update = 0;
            return;
        }

        let ratio = new_hashrate as f64 / old_hashrate as f64;
        if ratio > 0.0 && ratio.is_finite() {
            self.rate /= ratio;
        } else {
            self.rate = 0.0;
            self.n_ticks = 0;
        }
    }
}

impl Vardiff for VardiffState {
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    fn set_timestamp_of_last_update(&mut self, ts: u64) {
        self.timestamp_of_last_update = ts;
    }

    fn increment_shares_since_last_update(&mut self) {
        self.shares_since_last_update += 1;
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.timestamp_of_last_update = Self::now_secs();
        self.rate = 0.0;
        self.shares_since_last_update = 0;
        self.n_ticks = 0;
        self.consecutive_same_direction = 0;
        self.last_direction = 0;
        // Sign-persistence boundary state must also reset, else a stale consecutive
        // count would carry an accumulated discount into the next cycle — relaxing
        // the *tightening* threshold (the dangerous direction the champion resists).
        self.cusum_last_sign = 0;
        self.cusum_consecutive = 0;
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        let now = Self::now_secs();
        let dt = now.saturating_sub(self.timestamp_of_last_update);

        if dt <= 15 {
            return Ok(None);
        }

        // Stage 1: EWMA estimate
        let (realized_spm, h_estimate) = self.estimate(hashrate, target, shares_per_minute);

        // Deviation: |ratio - 1| × 100
        let delta = if hashrate > 0.0 {
            ((h_estimate as f64 / hashrate as f64) - 1.0).abs() * 100.0
        } else {
            0.0
        };

        // Stage 2: Boundary
        let threshold = self.threshold(dt, shares_per_minute, realized_spm);

        debug!(
            target: "vardiff",
            "dt={}s, realized_spm={:.2}, h_estimate={:.2}, delta={:.1}%, threshold={:.1}%",
            dt, realized_spm, h_estimate, delta, threshold,
        );

        if delta < threshold {
            return Ok(None);
        }

        // Stage 3: Update
        let mut new_hashrate = self.compute_new_hashrate(h_estimate, hashrate);

        if new_hashrate < self.min_allowed_hashrate {
            new_hashrate = self.min_allowed_hashrate;
        }

        debug!(
            target: "vardiff",
            "Firing: {:.2} -> {:.2} H/s (consecutive={})",
            hashrate, new_hashrate, self.consecutive_same_direction,
        );

        // Post-fire: rescale EWMA and reset timestamp
        self.rescale_ewma(new_hashrate, hashrate);
        self.timestamp_of_last_update = now;

        Ok(Some(new_hashrate))
    }
}
