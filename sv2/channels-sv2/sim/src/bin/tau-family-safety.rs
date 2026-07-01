//! ADMISSIBILITY CHECK for the τ-family result: is the per-rate over-difficulty
//! optimum τ*≈30 still DECLINE-SAFE, or does the gate forbid it (making the
//! champion's 360 correctly conservative, not over-damped)?
//!
//! `tau-family.rs` found argmin-over-difficulty-area τ* sliding 240→30 as spm
//! rises (τ*∝1/r). But that minimizes ONE primitive. The champion was selected
//! on the GATE (worst settled-e ≤ 5% across the envelope) PLUS gentleness. So
//! "360 is over-damped, 30 is better" is only honest if τ=30 also PASSES the
//! decline-safety gate at the worst severity. If τ=30 fails the gate at some
//! severity the area-scan held fixed, then 360 is *appropriately conservative*
//! and the finding is "the unconstrained over-difficulty optimum is rate-
//! dependent, but the safe window is bounded longer."
//!
//! For each high-spm r*, at τ∈{30,360}, report WORST-over-severity settled-e%
//! (the gate quantity, GATE=5%) and the over-difficulty area side-by-side, so
//! the fork is decided by data: τ=30 gate-safe → "over-damped, share-indexing
//! helps"; τ=30 gate-fails → "360 correctly conservative, gate bounds the win."
//!
//! Usage: cargo run --release --bin tau-family-safety
//! Env: VARDIFF_TFS_TRIALS (default 120 base, CI-scaled).

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const GATE_PCT: f64 = 5.0;
const SENS: f64 = 1.5;
const SPMS: &[f32] = &[6.0, 8.0, 12.0, 20.0, 30.0];
const TAUS: &[u64] = &[30, 360]; // the per-rate optimum vs the champion
const RATES_PPH: &[f32] = &[1.0, 2.0, 5.0, 10.0, 20.0, 40.0];

fn cfg(tau: u64, sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}/s{sens}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(sens, 0.05, 8.0, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0,
            clock,
        )))
    })
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Returns (settled_e%, overdiff_area) for one (τ,rate,spm) cell. settled-e is
/// the gate/wobble quantity (last e in the post-decline recovery window);
/// area is the escape-phase ∫max(e,0)dt. Same profile as tau-family.rs.
fn cell(tau: u64, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> (f64, f64) {
    let a = cfg(tau, SENS);
    let mature = 60u64;
    let rate = rate_pph / 100.0 / 60.0;
    let target = 0.50f32;
    let observe = 120u64;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..300u64 {
        let frac = (rate * (m as f32 + 1.0)).min(target);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= target { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - (rate * dm as f32).min(target));
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "decline".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let d_start = mature * 60;
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let (mut settleds, mut areas) = (Vec::with_capacity(trials), Vec::with_capacity(trials));
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut se, mut area, mut last_t) = (0.0f64, 0.0f64, d_start);
        for tk in &t.ticks {
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
            if tk.t_secs > d_end && tk.t_secs <= trial_end { se = e; } // settled
        }
        settleds.push(se);
        areas.push(area);
    }
    (median(settleds), median(areas))
}

fn main() {
    let base: usize = env::var("VARDIFF_TFS_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    let seed = DEFAULT_BASELINE_SEED ^ 0x5A_FE7;

    println!("\n## τ*-ADMISSIBILITY CHECK: is the over-difficulty optimum τ=30 decline-SAFE, or is 360 correctly conservative?");
    println!("Per high-spm r*: WORST-over-severity settled-e% (GATE={}%) and over-diff area, at τ=30 (area-optimum) vs τ=360 (champion).", GATE_PCT);
    println!("Decision: τ=30 worst-settled ≤ {}% ⇒ '360 over-damped, share-indexing helps'. τ=30 > {}% ⇒ '360 correctly conservative, gate bounds the win'.\n", GATE_PCT, GATE_PCT);
    println!("| spm | τ | worst settled-e% | (gate) | worst-severity@ | over-diff area |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for &spm in SPMS {
        for &tau in TAUS {
            // worst settled over severity, and the area at that severity
            let ct = (base as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
            let (mut worst_se, mut worst_area, mut worst_rate) = (f64::MIN, 0.0f64, 0.0f32);
            for (k, &r) in RATES_PPH.iter().enumerate() {
                let (se, area) = cell(tau, r, spm, ct, seed.wrapping_add((k as u64) << 40).wrapping_add((tau as u64) << 20).wrapping_add((spm as u64) << 8));
                if se > worst_se { worst_se = se; worst_area = area; worst_rate = r; }
            }
            let gate = if worst_se <= GATE_PCT { "PASS" } else { "**FAIL**" };
            println!("| {} | {} | {:+.1} | {} | {}%/hr | {:.0} |",
                spm as u32, tau, worst_se, gate, worst_rate as u32, worst_area);
        }
    }
    println!("\nRead: if every τ=30 row PASSES, the area-optimum is admissible → champion 360 is over-damped on the gate-safe set,");
    println!("and share-indexing toward shorter windows at high rate is a real gentlest-ness win. If any τ=30 row FAILS, 360's");
    println!("length is (at least partly) the gate doing its job, and the flag must say 'over-damped only to the extent the gate permits'.");
}
