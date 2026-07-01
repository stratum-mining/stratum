//! `HoltEstimator` — a slope-aware (two-state) estimator for Experiment B
//! (`bin/matched-detector`, see `docs/DECLINE_COLLAPSE_TEST.md`).
//!
//! ## Why this lives in the sim crate, not production
//!
//! This is a *research* arm: the question it answers is whether a tracker that
//! models the decline's SLOPE can sit below the bias–variance floor that bounds
//! level-only estimators (`decline_floor`). It is not a shipping candidate, so it
//! stays out of `channels_sv2::vardiff::composed`. The orphan rule permits
//! `impl Estimator for HoltEstimator` here (foreign trait, local type), and
//! `Composed<HoltEstimator, B, U>` automatically gets the blanket `Vardiff` impl
//! plus the sim-side `Observable` impl — so it drops into the same trial driver,
//! same boundary, same actuator, same clamp as the champion (pin 4).
//!
//! ## The structure: champion level + a DECOUPLED multiplicative de-lag
//!
//! The level leg is an inner [`EwmaEstimator`] — the champion's estimator
//! *verbatim*. The trend `b = d ln H/dt` (per minute) rides on top as a pure
//! multiplicative forecast:
//!
//! ```text
//!   ĥ = h_level · exp(b · L_eff)          L_eff = τ (the EWMA mean lag, minutes)
//! ```
//!
//! With `b = 0` this is BIT-IDENTICAL to the champion EWMA (the [`Trend::Off`]
//! mode and a regression check both pin this). The `b·L_eff` term is exactly the
//! removable lag-bias `ρτ` of the floor (`DECLINE_COLLAPSE_TEST.md` §1(a)):
//! `h_level ≈ H(t−L_eff)`, and `ln H(t) = ln H(t−L_eff) + b·L_eff`, so the
//! forecast de-lags the level forward to *now*. This is deliberately NOT textbook
//! coupled Holt (where the trend feeds back into the level prediction) — keeping
//! the level identical to the champion is what makes a `regret_over` delta
//! attributable to the trend leg alone. Coupled Holt / Kalman-velocity is the
//! noted follow-up if the decoupled win is real.
//!
//! ## Two trend modes (the oracle/estimated gap is B's headline number)
//!
//! - [`Trend::Oracle`]: `b` is the EXACT instantaneous log-slope of the linear
//!   decline from ρ + time-since-onset ([`decline_floor::oracle_log_slope_per_min`]),
//!   evaluated at the de-lag interval's midpoint (`now − L_eff/2`) because the
//!   linear ramp's log-slope steepens through the decline. Computed from the clock
//!   and ρ ONLY — never from the estimated level — so the oracle stays a clean
//!   ceiling read (pin 1): level estimated, velocity handed.
//! - [`Trend::Estimated`]: `b` is a β-recursion on the level's own ln-increments —
//!   it must catch ramp ONSET and slope from Poisson data. Velocity is a property
//!   of TRUTH, invariant to the controller's retargets, so it is CARRIED across
//!   fires; only the artificial ℓ-jump sample at a fire is skipped (not fed into
//!   the recursion).
//!
//! The clock is held in all modes but read for the trend ONLY by the oracle; the
//! estimated mode uses it solely to timestamp diagnostic traces. This separation
//! is auditable and deliberate (no oracle info leaks into the estimated arm).

use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{Estimator, EstimatorContext, EstimatorSnapshot, EwmaEstimator};
use channels_sv2::vardiff::Clock;

use crate::decline_floor::oracle_delag_slope_per_min;

/// One per-snapshot diagnostic record — the trend state that produced a forecast.
/// Drained by the rig after a trial to build the direction-gate fire log (pin 1:
/// `b̂` on every tightening fire, paired with `r_obs/r*` to fingerprint a
/// starvation-driven ring).
#[derive(Debug, Clone, Copy)]
pub struct HoltTrace {
    pub t_secs: u64,
    /// The trend `b` (log-H per minute) used this tick. Negative on a decline.
    pub b_per_min: f64,
    /// `ln(h_level)` — the inner EWMA level before de-lag.
    pub level_ln: f64,
    /// `ln(ĥ)` — the de-lagged forecast (what the boundary/update saw).
    pub forecast_ln: f64,
}

/// A shared, per-trial sink. The rig constructs one, clones the `Arc` out before
/// moving the estimator into the trial, runs the trial, then reads the traces.
pub type HoltSink = Arc<Mutex<Vec<HoltTrace>>>;

/// The trend leg's mode.
pub enum Trend {
    /// `b = 0` — bit-identical to the inner champion EWMA. The pin-4 reference.
    Off,
    /// Exact decline slope from ρ + onset (oracle-ρ, handed-onset). `onset_secs`
    /// is when the decline begins (counter-mature boundary).
    Oracle { rate_pct_per_hr: f32, onset_secs: u64 },
    /// β-recursion on the level's ln-increments. `beta ∈ (0,1]`: higher = faster
    /// trend tracking, noisier.
    Estimated { beta: f64 },
}

/// Mutable trend-recursion state (interior-mutable; snapshot takes `&self`).
#[derive(Debug, Default)]
struct TrendState {
    b: f64,          // current trend estimate (log-H per minute)
    l_prev: f64,     // previous level ln (for the slope sample)
    initialized: bool,
    skip_next: bool, // true after a fire: skip the next (artificial) slope sample
}

/// Two-state slope-aware estimator: champion EWMA level + decoupled de-lag.
pub struct HoltEstimator {
    inner: EwmaEstimator,
    trend: Trend,
    state: Mutex<TrendState>,
    clock: Arc<dyn Clock>,
    sink: Option<HoltSink>,
}

impl std::fmt::Debug for HoltEstimator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoltEstimator")
            .field("inner", &self.inner)
            .field("mode", &self.code())
            .finish()
    }
}

impl HoltEstimator {
    /// Build with an explicit trend mode and optional diagnostic sink. `tau_secs`
    /// is the inner EWMA window (use the champion's 360 to keep the level leg
    /// identical to the champion).
    pub fn new(tau_secs: u64, trend: Trend, clock: Arc<dyn Clock>, sink: Option<HoltSink>) -> Self {
        Self {
            inner: EwmaEstimator::new(tau_secs),
            trend,
            state: Mutex::new(TrendState::default()),
            clock,
            sink,
        }
    }

    /// `L_eff` — the EWMA mean lag, in MINUTES. NOT τ: this is a DISCRETE EWMA
    /// (`rate ← α·rate + (1−α)·n`, α = exp(−tick/τ)), whose impulse-response mean
    /// lag is `α/(1−α)` TICKS, not the continuous-time τ. (E.g. τ=360,tick=60 →
    /// α=0.8465 → 5.51 min, not 6.0 — a real 9% difference the open-loop
    /// calibration gate caught as a high-rate over-correction.) This is the
    /// analytic de-lag horizon, correctly computed for the discrete filter — NOT
    /// fitted (pin 2: a calibration miss means re-spec the MODEL, which is exactly
    /// what using the discrete mean lag instead of τ does).
    fn l_eff_min(&self) -> f64 {
        let alpha = (-(self.inner.tick_secs as f64) / (self.inner.tau_secs as f64)).exp();
        let mean_lag_ticks = alpha / (1.0 - alpha);
        mean_lag_ticks * (self.inner.tick_secs as f64) / 60.0
    }

    /// The trend `b` (log-H per minute) for this snapshot, given the just-computed
    /// level ln. Advances the estimated-mode recursion as a side effect.
    fn trend_b(&self, level_ln: f64, dt_secs: u64) -> f64 {
        match &self.trend {
            Trend::Off => 0.0,
            Trend::Oracle { rate_pct_per_hr, onset_secs } => {
                // Curvature-EXACT de-lag: the displacement ln H(now) − ln H(now−L)
                // over the mean-lag window L_eff, divided by L_eff to return a
                // per-minute slope (so forecast = level + b·L_eff = level +
                // displacement). The linear ramp's log-path is CURVED, so a single
                // instantaneous slope × horizon over-corrects where curvature is
                // large (the open-loop gate caught this at high rate). Clock + ρ
                // only; no estimated-state dependence (pin 1).
                let now = self.clock.now_secs();
                let min_since_onset_now = if now > *onset_secs {
                    (now - *onset_secs) as f64 / 60.0
                } else {
                    0.0
                };
                oracle_delag_slope_per_min(*rate_pct_per_hr, min_since_onset_now, self.l_eff_min())
            }
            Trend::Estimated { beta } => {
                let mut st = self.state.lock().unwrap();
                if !st.initialized {
                    st.initialized = true;
                    st.l_prev = level_ln;
                    return 0.0;
                }
                if st.skip_next {
                    // Re-baseline across a fire's artificial ℓ-jump; CARRY b
                    // (truth velocity is continuous across a control retarget).
                    st.skip_next = false;
                    st.l_prev = level_ln;
                    return st.b;
                }
                let dt_min = (dt_secs as f64 / 60.0).max(1e-9);
                let raw_slope = (level_ln - st.l_prev) / dt_min;
                let b_new = beta * raw_slope + (1.0 - beta) * st.b;
                st.b = b_new;
                st.l_prev = level_ln;
                b_new
            }
        }
    }
}

impl Estimator for HoltEstimator {
    fn observe(&mut self, n_shares: u32) {
        self.inner.observe(n_shares);
    }

    fn on_fire(&mut self, new_hashrate: f32, old_hashrate: f32) {
        self.inner.on_fire(new_hashrate, old_hashrate);
        // A genuine reset (new/old ≤ 0) clears the trend; an ordinary retarget
        // only marks the next slope sample to be skipped (b carried).
        if let Ok(mut st) = self.state.lock() {
            if new_hashrate <= 0.0 || old_hashrate <= 0.0 {
                *st = TrendState::default();
            } else {
                st.skip_next = true;
            }
        }
    }

    fn shares_count(&self) -> u32 {
        self.inner.shares_count()
    }

    fn code(&self) -> String {
        let m = match &self.trend {
            Trend::Off => "off".to_string(),
            Trend::Oracle { .. } => "orc".to_string(),
            Trend::Estimated { beta } => format!("b{:.2}", beta),
        };
        format!("Holt{}s/{}", self.inner.tau_secs, m)
    }

    fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
        // Inner champion EWMA advances exactly once per outer snapshot.
        let snap = self.inner.snapshot(dt_secs, ctx);
        let h_level = snap.h_estimate;
        if !(h_level > 0.0) {
            return snap; // degenerate level (cold/zero) — no de-lag possible
        }
        let level_ln = (h_level as f64).ln();
        let b = self.trend_b(level_ln, dt_secs);
        let forecast_ln = level_ln + b * self.l_eff_min();
        let h_estimate = forecast_ln.exp() as f32;

        if let Some(sink) = &self.sink {
            if let Ok(mut v) = sink.lock() {
                v.push(HoltTrace {
                    t_secs: self.clock.now_secs(),
                    b_per_min: b,
                    level_ln,
                    forecast_ln,
                });
            }
        }

        EstimatorSnapshot {
            h_estimate,
            // uncertainty stays None — the champion boundary scales its threshold
            // by uncertainty.ratio_std (boundary.rs:240,348), so reporting Some
            // here would change boundary behavior and break pin 4. Holt differs
            // from the champion ONLY in the level→forecast de-lag.
            uncertainty: None,
            ..snap
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Target;
    use channels_sv2::target::hash_rate_to_target;
    use channels_sv2::vardiff::MockClock;

    /// A valid target for the snapshot conversion (exact value not asserted).
    fn target_for(h: f64, spm: f64) -> Target {
        hash_rate_to_target(h, spm).unwrap().into()
    }

    /// The EWMA level h_estimate after observing `counts` in successive ticks —
    /// the reference the de-lag rides on.
    fn ewma_level(tau: u64, counts: &[u32], tgt: &Target, h: f32, spm: f32) -> f32 {
        let mut e = EwmaEstimator::new(tau);
        let cx = EstimatorContext { current_hashrate: h, current_target: tgt, shares_per_minute: spm };
        let mut last = 0.0;
        for &n in counts {
            e.observe(n);
            last = e.snapshot(60, &cx).h_estimate;
        }
        last
    }

    fn holt_forecast(holt: &mut HoltEstimator, counts: &[u32], tgt: &Target, h: f32, spm: f32) -> f32 {
        let cx = EstimatorContext { current_hashrate: h, current_target: tgt, shares_per_minute: spm };
        let mut last = 0.0;
        for &n in counts {
            holt.observe(n);
            last = holt.snapshot(60, &cx).h_estimate;
        }
        last
    }

    #[test]
    fn off_mode_matches_inner_ewma_bit_for_bit() {
        let clock = Arc::new(MockClock::new(600));
        let (tgt, h, spm) = (target_for(1.0e15, 12.0), 1.0e15f32, 12.0f32);
        let counts = [5u32, 8, 12, 6, 10];
        let mut holt = HoltEstimator::new(360, Trend::Off, clock, None);
        let hf = holt_forecast(&mut holt, &counts, &tgt, h, spm);
        let ef = ewma_level(360, &counts, &tgt, h, spm);
        assert_eq!(hf, ef, "Off-mode Holt must equal the champion EWMA bit-for-bit");
    }

    #[test]
    fn off_mode_reports_no_uncertainty() {
        let clock = Arc::new(MockClock::new(600));
        let (tgt, h, spm) = (target_for(1.0e15, 12.0), 1.0e15f32, 12.0f32);
        let mut holt = HoltEstimator::new(360, Trend::Off, clock, None);
        let cx = EstimatorContext { current_hashrate: h, current_target: &tgt, shares_per_minute: spm };
        holt.observe(10);
        assert!(holt.snapshot(60, &cx).uncertainty.is_none(), "must stay None (pin 4)");
    }

    #[test]
    fn oracle_delags_below_level_on_a_decline() {
        // 30 min into a 20%/hr decline: oracle b<0 ⇒ forecast below the level.
        let onset = 3600u64;
        let clock = Arc::new(MockClock::new(onset + 1800));
        let (tgt, h, spm) = (target_for(1.0e15, 12.0), 1.0e15f32, 12.0f32);
        let counts = [10u32; 5];
        let mut holt = HoltEstimator::new(
            360,
            Trend::Oracle { rate_pct_per_hr: 20.0, onset_secs: onset },
            clock,
            None,
        );
        let fc = holt_forecast(&mut holt, &counts, &tgt, h, spm);
        let level = ewma_level(360, &counts, &tgt, h, spm);
        assert!(fc < level, "oracle de-lag should forecast below the level: fc {fc} level {level}");
    }

    #[test]
    fn sink_records_a_trace_per_snapshot() {
        let clock = Arc::new(MockClock::new(600));
        let sink: HoltSink = Arc::new(Mutex::new(Vec::new()));
        let (tgt, h, spm) = (target_for(1.0e15, 12.0), 1.0e15f32, 12.0f32);
        let mut holt = HoltEstimator::new(120, Trend::Estimated { beta: 0.3 }, clock, Some(sink.clone()));
        let _ = holt_forecast(&mut holt, &[8u32; 4], &tgt, h, spm);
        assert_eq!(sink.lock().unwrap().len(), 4, "one trace per snapshot");
    }
}
