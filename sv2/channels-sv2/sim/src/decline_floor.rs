//! The dynamic decline floor `L*` — the bias–variance detectability floor a
//! sustained decline imposes, and the per-cell-optimal-τ EWMA arm that achieves
//! it (see `docs/DECLINE_COLLAPSE_TEST.md`, the prerequisite Experiment B scores
//! against).
//!
//! ## What this module is for (and what it deliberately is NOT)
//!
//! Experiment B (`bin/matched-detector`) asks whether a *slope-aware* tracker can
//! sit BELOW the bias–variance floor that bounds *uniform-window* (level-only)
//! estimators on a decline. That question is only meaningful against a correct
//! floor. This module supplies it two ways, and the distinction is load-bearing:
//!
//!   - [`l_star_regret_over`] — the ANALYTIC envelope: minimize
//!     `L(τ) = ρ·τ + z/√(r*·τ·e^{−e*})` over the window τ, solved
//!     SELF-CONSISTENTLY in `(τ*, e*)` with `e* ≈ ρ·τ*` (the lag IS the
//!     over-difficulty, and the realized share rate is suppressed by it,
//!     `r_obs = r*·e^{−e}` — THEORY §5.2). This is convex on log–log, NOT the
//!     linearized `(ρ/r*)^{1/3}` power law (which is only its small-`e*` limit).
//!     It is a derived shape, carrying an unknown O(1) constant `z` — so it is
//!     used as a CONVEXITY CROSS-CHECK, never as the literal bar.
//!
//!   - [`tau_star_secs`] + a level-only EWMA at that window — the MEASURED floor:
//!     the achieved `regret_over` of a uniform-window estimator tuned to the
//!     per-cell optimum. THIS is the authoritative bar B is scored against,
//!     because it is the best a level estimator actually does in the harness
//!     (same Poisson draws, same actuator, same clamp), with no free constant.
//!     `tau-family.rs` deliberately reads argmin POSITIONS to dodge the
//!     clamp-magnitude confound (METRIC §8.3 RESULT); the matched-detector rig
//!     reads the DEPTH at the analytic argmin — the one new number Experiment A
//!     owed — so the clamp confound is controlled by holding the actuator fixed
//!     across the raced arms, not by avoiding the depth.
//!
//! ## The decline is LINEAR in H, so the log-slope is not constant
//!
//! `decline_safety::decline_scenario` builds `H(m) = H0·(1 − (ρ/60)·m)` — linear
//! in hashrate, NOT geometric. So `d ln H/dt` STEEPENS through the decline:
//!
//! ```text
//!   b(t) = d ln H / dt = −(ρ/60) / (1 − (ρ/60)·m)      (per-minute, m = minutes
//!                                                        since decline onset)
//! ```
//!
//! It roughly DOUBLES from onset to a 50% drop. This matters for two consumers:
//! the analytic floor's `e*` uses the decline-AVERAGED log-rate, and the oracle
//! Holt arm (`bin/matched-detector`) must use the EXACT instantaneous `b(t)` from
//! [`oracle_log_slope_per_min`] — a constant `−ρ_log` oracle would under-correct
//! in the deep decline and fail the grid-wide calibration gate for a ramp-shape
//! reason that is neither rate nor a bug.

use crate::decline_safety::{DECLINE_MATURE_MIN, DECLINE_TARGET_DROP};

/// The instantaneous log-slope `d ln H/dt` (per MINUTE) of the linear decline at
/// `minutes_since_onset`, for a decline of `rate_pct_per_hr`. This is what the
/// ORACLE Holt arm injects as its trend `b` — computed from ρ and time-since-
/// onset ONLY (oracle-ρ + handed-onset), never from the estimated level, so the
/// oracle stays a clean ceiling read (pin 1).
///
/// Negative (H falling). Returns 0 before onset / after the floor is reached.
pub fn oracle_log_slope_per_min(rate_pct_per_hr: f32, minutes_since_onset: f64) -> f64 {
    let rate_per_min = (rate_pct_per_hr as f64) / 100.0 / 60.0; // fraction/min
    if rate_per_min <= 0.0 || minutes_since_onset < 0.0 {
        return 0.0;
    }
    // Fraction of H0 already lost at this minute, capped at the target drop.
    let frac_lost = (rate_per_min * minutes_since_onset).min(DECLINE_TARGET_DROP as f64);
    if frac_lost >= DECLINE_TARGET_DROP as f64 {
        return 0.0; // at the floor: H is flat again, no trend
    }
    // d/dt ln(1 − rate·m) = −rate / (1 − rate·m).
    -rate_per_min / (1.0 - rate_per_min * minutes_since_onset)
}

/// The log-hashrate `ln H(t)` of the linear decline at `minutes_since_onset`,
/// relative to onset (`ln(H/H0)`). Negative during the decline, clamped flat at
/// the target drop. The ORACLE de-lag uses the exact TWO-POINT displacement of
/// this (below), not an instantaneous-slope × horizon, because the linear ramp's
/// log-path is CURVED (the slope steepens) — a single-slope × horizon
/// over-corrects where curvature is large (the open-loop gate caught this).
fn oracle_log_h_rel(rate_pct_per_hr: f32, minutes_since_onset: f64) -> f64 {
    let rate_per_min = (rate_pct_per_hr as f64) / 100.0 / 60.0;
    if rate_per_min <= 0.0 || minutes_since_onset <= 0.0 {
        return 0.0; // before onset: H = H0
    }
    let frac_lost = (rate_per_min * minutes_since_onset).min(DECLINE_TARGET_DROP as f64);
    (1.0 - frac_lost).ln()
}

/// The EXACT log-displacement the de-lag must add: `ln H(now) − ln H(now − L)`,
/// over the EWMA mean-lag window `window_min`, for a decline of `rate_pct_per_hr`
/// at `minutes_since_onset_now`. This is the integral of the (steepening) log-
/// slope across the window — the curvature-exact replacement for `b(mid)·L`.
/// Computed from ρ + onset ONLY (oracle info; no estimated-state dependence —
/// pin 1). Returns the per-minute equivalent slope `displacement / window_min` so
/// the caller's `b·L_eff` form is preserved and the trace's `b` stays a slope.
pub fn oracle_delag_slope_per_min(
    rate_pct_per_hr: f32,
    minutes_since_onset_now: f64,
    window_min: f64,
) -> f64 {
    if window_min <= 0.0 {
        return 0.0;
    }
    let now = oracle_log_h_rel(rate_pct_per_hr, minutes_since_onset_now);
    let lagged = oracle_log_h_rel(rate_pct_per_hr, minutes_since_onset_now - window_min);
    (now - lagged) / window_min
}

/// The decline duration (minutes) for `rate_pct_per_hr`, mirroring
/// `decline_scenario` (drop `DECLINE_TARGET_DROP` at `ρ`, capped at the scenario
/// max). Used to bound the averaged log-rate the analytic floor needs.
fn decline_minutes(rate_pct_per_hr: f32) -> f64 {
    use crate::decline_safety::DECLINE_MAX_MIN;
    let rate_per_min = (rate_pct_per_hr as f64) / 100.0 / 60.0;
    if rate_per_min <= 0.0 {
        return DECLINE_MAX_MIN as f64;
    }
    ((DECLINE_TARGET_DROP as f64) / rate_per_min).min(DECLINE_MAX_MIN as f64)
}

/// Decline-averaged absolute log-slope (per minute): the mean of `|b(t)|` over
/// the decline phase. `∫ rate/(1−rate·m) dm / T = −ln(1−rate·T)/T`. For the
/// analytic floor this is the representative ρ_log the lag term `ρ_log·τ` uses
/// (the linear ramp's log-rate is not constant, so we average it over the phase
/// the floor is read on).
fn avg_log_slope_per_min(rate_pct_per_hr: f32) -> f64 {
    let rate_per_min = (rate_pct_per_hr as f64) / 100.0 / 60.0;
    let t = decline_minutes(rate_pct_per_hr);
    if rate_per_min <= 0.0 || t <= 0.0 {
        return 0.0;
    }
    let frac = (rate_per_min * t).min(DECLINE_TARGET_DROP as f64 - 1e-9);
    -(1.0 - frac).ln() / t
}

/// The set of estimator windows (SECONDS) the rig sweeps to MEASURE the floor
/// empirically per cell — the achieved-`regret_over` argmin over this range IS
/// the authoritative bias–variance floor (no `z`, same harness, same actuator).
///
/// We do NOT hand the rig an analytic `τ*`: the analytic optimum scales as
/// `z^{2/3}` (`z` unknown), so its absolute placement is untrustworthy (a `z=1`
/// solve rails to tens of minutes — see the module note). The SHAPE in `ρ/r*` is
/// `z`-independent (exponent 1/3, convex correction on top); the LEVEL and the
/// window are measured. This range brackets the rig's explored windows
/// (`tau-family.rs`): the 30 s tick floor up to a sleepy 20 min.
pub fn tau_sweep_range_secs() -> &'static [u64] {
    // Geometric-ish ladder over the admissible window range; dense enough that
    // the empirical argmin is well-resolved without an over-long sweep.
    &[30, 60, 90, 120, 180, 240, 360, 480, 720, 1200]
}

/// The ANALYTIC floor's predicted over-difficulty `e*` (the CONVEXITY
/// CROSS-CHECK), as a percent in the units `slow-decline.rs` emits.
///
/// Closed-form backbone + self-consistent convex correction, NO scan (so no
/// railing artifact):
///
/// ```text
///   linearized (e≈0):   lag* = (z/2)^{2/3} · (ρ/r*)^{1/3}        slope-1/3
///   convex (e^{−e}):     r* → r*·e^{−e*}, e* = lag*  ⇒  fixed point
///                        x = A · e^{x/3},   A = (z/2)^{2/3}·(ρ/r*)^{1/3}
/// ```
///
/// `z` is an unknown O(1) constant; it scales `A` (hence the level) but NOT the
/// exponent, so this is a SHAPE check — does the MEASURED floor depth bend up
/// super-linearly with `ρ/r*` in the sub-guard corner (the `e^{−e}` spiral in the
/// floor, (★★))? — not the literal bar. B's pass/fail uses the MEASURED EWMA
/// floor. Reported with `z = 1`.
pub fn l_star_regret_over(rate_pct_per_hr: f32, r_star: f64) -> f64 {
    analytic_e_star(rate_pct_per_hr, r_star, 1.0) * 100.0
}

/// The self-consistent over-difficulty `e*` (log units, NOT percent) at the
/// analytic optimum, for noise constant `z`. Solves `x = A·e^{x/3}` by fixed-point
/// iteration (contraction for the `A` in the rig's range; converges in a few
/// steps). `A = (z/2)^{2/3}·(ρ/r*)^{1/3}` is the linearized lag at optimum.
fn analytic_e_star(rate_pct_per_hr: f32, r_star: f64, z: f64) -> f64 {
    let rho = avg_log_slope_per_min(rate_pct_per_hr); // per minute, ≥ 0
    if rho <= 0.0 || r_star <= 0.0 {
        return 0.0;
    }
    let a = (z / 2.0).powf(2.0 / 3.0) * (rho / r_star).powf(1.0 / 3.0);
    // x = A·e^{x/3}; iterate from the linearized value x0 = A.
    let mut x = a;
    for _ in 0..64 {
        let next = a * (x / 3.0).exp();
        if (next - x).abs() < 1e-9 {
            x = next;
            break;
        }
        x = next;
    }
    x
}

/// The dimensionless collapse group of the spec's (★): fractional decline per
/// share arrival, `ρ_log / r*` (both per-minute / per-minute → dimensionless).
/// The analytic envelope is a one-group function of THIS (convex, per (★★)).
pub fn collapse_group(rate_pct_per_hr: f32, r_star: f64) -> f64 {
    let rho = avg_log_slope_per_min(rate_pct_per_hr);
    if r_star <= 0.0 {
        return 0.0;
    }
    rho / r_star
}

/// Minutes the counter matures before onset — re-exported convenience so the rig
/// and any HW-shape doc read the onset time from one place.
pub const MATURE_MIN: u64 = DECLINE_MATURE_MIN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_slope_steepens_through_linear_decline() {
        // 20%/hr: the instantaneous log-slope at onset is gentler than deep in
        // the decline (the linear-ramp steepening the module header describes).
        let b0 = oracle_log_slope_per_min(20.0, 0.0).abs();
        let b_deep = oracle_log_slope_per_min(20.0, 120.0).abs(); // 40% lost
        assert!(b0 > 0.0);
        assert!(
            b_deep > b0 * 1.5,
            "log-slope should steepen markedly: onset {b0:.5} deep {b_deep:.5}"
        );
    }

    #[test]
    fn oracle_slope_is_negative_during_decline_zero_at_floor() {
        // Falling H ⇒ negative log-slope during the decline.
        assert!(oracle_log_slope_per_min(10.0, 30.0) < 0.0);
        // At/after the 50% floor (10%/hr reaches 50% at 300 min) ⇒ flat ⇒ 0.
        assert_eq!(oracle_log_slope_per_min(10.0, 600.0), 0.0);
    }

    #[test]
    fn sweep_range_brackets_champion_and_tick_floor() {
        let r = tau_sweep_range_secs();
        assert!(r.first().copied().unwrap() <= crate::decline_safety::DECLINE_TICK_SECS); // reaches the tick floor
        assert!(r.contains(&360)); // includes the champion window
        assert!(r.last().copied().unwrap() >= 720); // reaches a sleepy window
        // strictly increasing (well-formed ladder)
        assert!(r.windows(2).all(|w| w[0] < w[1]));
    }

    /// Prints the analytic floor SHAPE across representative cells — reproducible
    /// reference for the cross-check, not an assertion. `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn probe_floor_numbers() {
        for (rho, rstar) in [
            (2.0f32, 30.0f64), (5.0, 20.0), (10.0, 12.0), (20.0, 6.0),
            (40.0, 2.0), (2.0, 2.0), (40.0, 30.0),
        ] {
            let g = collapse_group(rho, rstar);
            let l = l_star_regret_over(rho, rstar);
            let rho_log = avg_log_slope_per_min(rho);
            println!(
                "rho={rho:>4}%/hr r*={rstar:>4} | rho_log={rho_log:.5} | group={g:.4e} | L*(shape)={l:.3}%"
            );
        }
    }

    #[test]
    fn floor_is_convex_in_collapse_group() {
        // (★★): the analytic floor bends UP as ρ/r* grows (the e^{−e*} suppression
        // in the sub-guard/fast corner). Check the depth-per-group is monotone and
        // the high-group point is more than linearly above the low one.
        let g_lo = collapse_group(2.0, 30.0);
        let g_hi = collapse_group(40.0, 2.0);
        let l_lo = l_star_regret_over(2.0, 30.0);
        let l_hi = l_star_regret_over(40.0, 2.0);
        assert!(g_hi > g_lo && l_hi > l_lo);
        // convex: the cost ratio exceeds the group ratio (super-linear).
        assert!(
            (l_hi / l_lo) > (g_hi / g_lo).powf(1.0 / 3.0),
            "floor should be convex (steeper than slope-1/3) at high ρ/r*"
        );
    }
}
