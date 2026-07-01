//! Decline-safety gate: the single source of truth for the gate threshold, the
//! sustained-decline scenario, and the worst-settled-over-difficulty readout the
//! gate bounds.
//!
//! Two consumers share this module so neither restates the gate as a literal or
//! re-defines the scenario:
//!   - `bin/slow-decline.rs` — the full diagnostic sweep (per-cell table across
//!     the rate × spm grid, all configs).
//!   - the champion margin guard ([`champion_clears_decline_gate`] below) — the
//!     CI assertion that the SHIPPED champion clears the gate. This is the
//!     claim-level guard for the champion's selection criterion; its failure is
//!     legible ("champion's decline margin regressed past the gate") in a way a
//!     full-grid baseline diff is not.
//!
//! Why this is a *separate* guard from the grid baseline (`baseline_champion`):
//! the decline scenario is NOT in `default_cells()` (that grid is
//! cold-start/stable/step only), so the grid baseline does not cover the
//! decline-safety margin at all. The margin is the champion's actual selection
//! criterion (information-floor.md §9.2), so it earns its own dedicated guard.

use std::sync::Arc;

use channels_sv2::vardiff::MockClock;

use crate::baseline::{Scenario, TRUE_HASHRATE};
use crate::grid::AlgorithmSpec;
use crate::trial::{run_trial_observed, TrialConfig};

/// Tick interval for decline trials (seconds).
pub const DECLINE_TICK_SECS: u64 = 60;
/// Minutes the counter matures on-target before the decline begins.
pub const DECLINE_MATURE_MIN: u64 = 60;
/// Post-decline observe window (minutes) — long enough that a 2-spm reaction
/// completes, so "settled" is steady state, not residual lag.
pub const DECLINE_OBSERVE_MIN: u64 = 120;
/// Cap a slow decline at 4h of decline phase.
pub const DECLINE_MAX_MIN: u64 = 240;
/// Decline until this fraction of hashrate is lost (or the cap).
pub const DECLINE_TARGET_DROP: f32 = 0.50;

/// **The decline-safety gate.** Settled over-difficulty `e` (percent) after the
/// post-decline recovery window must stay at or below this bound; a config that
/// exceeds it is a *runaway* (the death-spiral failure: over-difficulty → fewer
/// shares → less evidence → slower ease → more over-difficulty) and is rejected
/// as decline-UNSAFE regardless of its scalar fitness.
///
/// Provenance: `docs/information-floor.md` §9.2 and `docs/SLOW_DECLINE_TEST.md`.
/// The shipped champion sits at +2.7% worst-settled (clears it); sleepy
/// long-window configs fail here. This constant is the canonical home of the
/// "5%" gate — the rig and the CI guard both read it, so the number is stated
/// exactly once in the codebase rather than copied into a literal that can drift
/// from the doc.
pub const DECLINE_SAFETY_GATE_PCT: f64 = 5.0;

/// A sustained decline scenario as a `Custom` phase list: mature on-target, then
/// drop at `rate_pct_per_hr` in fine 1-min `Hold` steps until [`DECLINE_TARGET_DROP`]
/// (capped at [`DECLINE_MAX_MIN`]), then hold at the floor.
///
/// Returns the scenario and `(decline_start, decline_end, trial_end)` for the
/// readout — `trial_end` is after the post-decline observe window, where the
/// *settled* error is sampled (so a deep-but-recovering lag is not misread as a
/// runaway).
pub fn decline_scenario(rate_pct_per_hr: f32) -> (Scenario, u64, u64, u64) {
    use crate::baseline::Phase;

    let rate = rate_pct_per_hr / 100.0; // fraction/hr
    let decline_min = ((DECLINE_TARGET_DROP / rate) * 60.0).min(DECLINE_MAX_MIN as f32) as u64;
    let mut phases = vec![Phase::Hold {
        secs: DECLINE_MATURE_MIN * 60,
        h: TRUE_HASHRATE,
    }];
    for m in 0..decline_min {
        let frac = (rate / 60.0) * (m as f32 + 1.0);
        let h = TRUE_HASHRATE * (1.0 - frac).max(1.0 - DECLINE_TARGET_DROP);
        phases.push(Phase::Hold { secs: 60, h });
    }
    let floor =
        TRUE_HASHRATE * (1.0 - (rate / 60.0 * decline_min as f32).min(DECLINE_TARGET_DROP));
    phases.push(Phase::Hold {
        secs: DECLINE_OBSERVE_MIN * 60,
        h: floor,
    });
    let start = DECLINE_MATURE_MIN * 60;
    let end = start + decline_min * 60;
    let trial_end = end + DECLINE_OBSERVE_MIN * 60;
    (
        Scenario::Custom {
            name: format!("decline_{}pph", rate_pct_per_hr as u32),
            phases,
            initial_estimate: None,
        },
        start,
        end,
        trial_end,
    )
}

/// Median of a vector (the rig's per-cell aggregator — robust to the heavy tail
/// of the sparse-rate settled-e distribution).
fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Worst settled over-difficulty (percent) the `make_spec` algorithm reaches
/// across the decline grid (`rates` × `spms`). This is the exact quantity the
/// gate bounds — the per-cell median settled-`e` at the end of the observe
/// window, maxed over cells. Sparse cells are oversampled (estimator variance
/// ∝ 1/(r*·τ)) so the sub-guard medians are well-resolved, matching the rig.
///
/// Deterministic given `base_seed`. Reused by the CI margin guard; the diagnostic
/// rig (`bin/slow-decline.rs`) computes this alongside its other metrics in one
/// pass, but the gate only needs this number.
pub fn worst_settled_e_pct(
    make_spec: impl Fn() -> AlgorithmSpec,
    rates: &[f32],
    spms: &[f32],
    base_trials: usize,
    base_seed: u64,
) -> f64 {
    let mut worst = f64::MIN;
    for &rate in rates {
        let (scen, _d_start, d_end, trial_end) = decline_scenario(rate);
        for &spm in spms {
            let (config_proto, schedule) = scen.build(spm);
            let config = TrialConfig {
                tick_interval_secs: DECLINE_TICK_SECS,
                ..config_proto
            };
            // CI-match trials to estimator variance: a 4-spm cell needs ~15× a
            // 60-spm cell for the same CI. Oversample the sparse cells.
            let scale = (60.0 / spm as f64).max(1.0);
            let cell_trials = ((base_trials as f64) * scale).round() as usize;

            let mut settled = Vec::with_capacity(cell_trials);
            for i in 0..cell_trials {
                let clock = Arc::new(MockClock::new(0));
                let v = (make_spec().factory)(clock.clone());
                let t = run_trial_observed(
                    v,
                    clock,
                    config.clone(),
                    &schedule,
                    base_seed.wrapping_add(i as u64),
                );
                let mut settled_e = 0.0f64;
                for tk in &t.ticks {
                    if tk.t_secs > d_end && tk.t_secs <= trial_end {
                        let h_true =
                            schedule.at(tk.t_secs.saturating_sub(DECLINE_TICK_SECS / 2)) as f64;
                        settled_e = (tk.current_hashrate_before as f64 / h_true).ln();
                    }
                }
                settled.push(settled_e * 100.0);
            }
            worst = worst.max(median(settled));
        }
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use channels_sv2::vardiff::composed::champion_composed;
    use crate::grid::VardiffBox;

    /// The decline grid the gate is asserted over. Matches the rig's grid,
    /// including the sub-guard {2,4} spm cells (below spm_threshold=6) — the
    /// worst-case safety regime where the PoissonCI guard is load-bearing.
    const GATE_RATES: [f32; 6] = [1.0, 2.0, 5.0, 10.0, 20.0, 40.0];
    const GATE_SPMS: [f32; 7] = [2.0, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];

    /// A spec that builds the SHIPPED champion directly (NOT a re-spelled copy
    /// of its params) — so this guard tracks `champion_composed`, the actual
    /// production constructor. If the shipped champion's params change, this
    /// rebuilds with them and the gate re-asserts against the new behavior.
    fn shipped_champion_spec() -> AlgorithmSpec {
        AlgorithmSpec::new("shipped_champion", |clock| {
            VardiffBox(Box::new(champion_composed(1.0, clock)))
        })
    }

    /// **B4 — the load-bearing champion guard.** Re-derives the shipped
    /// champion's worst settled over-difficulty across the decline grid and
    /// asserts it clears [`DECLINE_SAFETY_GATE_PCT`]. This is the champion's
    /// selection criterion (information-floor.md §9.2); the grid baseline does
    /// NOT cover it (decline is not in `default_cells()`).
    ///
    /// `#[ignore]` (slow, ~seconds): run by the `--ignored` CI job alongside the
    /// classic/champion grid regressions, so all three guards share one surface.
    ///
    /// Noise band: the assertion is against the 5% gate, not the champion's
    /// nominal +2.7%, so it fails only on a *regression past admissibility* —
    /// not on run-to-run sim noise around 2.7%. The headroom (gate − champion ≈
    /// 2.3pp) IS the band; it is wide relative to the per-cell median's
    /// run-to-run variation at the oversampled trial counts here.
    #[test]
    #[ignore = "slow decline-margin guard; run with `cargo test --release -- --ignored`"]
    fn champion_clears_decline_gate() {
        let base_seed = crate::baseline::DEFAULT_BASELINE_SEED;
        let worst = worst_settled_e_pct(
            shipped_champion_spec,
            &GATE_RATES,
            &GATE_SPMS,
            300,
            base_seed,
        );
        assert!(
            worst <= DECLINE_SAFETY_GATE_PCT,
            "shipped champion's worst settled over-difficulty {:+.2}% exceeds the \
             decline-safety gate of {:.1}% — the champion's selection criterion \
             REGRESSED to decline-UNSAFE. (Re-derived over the {}×{} decline grid; \
             see decline_safety.rs / information-floor.md §9.2.)",
            worst,
            DECLINE_SAFETY_GATE_PCT,
            GATE_RATES.len(),
            GATE_SPMS.len(),
        );
    }
}
