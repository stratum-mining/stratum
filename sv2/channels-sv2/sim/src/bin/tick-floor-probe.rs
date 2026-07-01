//! TICK-FLOOR DEGENERACY PROBE — is the τ*-railing a real sub-tick optimum, or
//! the estimator degenerating below the tick floor (argmin landing on noise)?
//!
//! The fit scan railed spm12/20/30 argmins at τ=24 < tick=60s. At τ<tick the
//! time-EWMA has alpha=exp(-60/τ) → near-pure-last-observation (alpha≈0.08 at
//! τ=24): not a short window, a DEGENERATE one. "τ* wants sub-tick" ASSUMES the
//! argmin is a real optimum cut off from below; if the estimator is degenerate
//! there, the argmin is noise past a resolution edge. The discriminator is the
//! CURVE SHAPE near the floor (smooth decline → real optimum below; erratic /
//! CI-dominated → degeneracy), NOT the argmin point.
//!
//! AND the load-bearing version: the SHARE-INDEXED arm runs on the SAME 60s tick.
//! At high rate pending_shares/tick is large, so its window also shortens toward
//! the floor. If the tick degenerates the time-EWMA at high rate, it likely
//! degenerates share-indexed too → the high-rate regime (share-indexing's payoff
//! regime) is sub-resolution for BOTH arms — the sim-tick echo of the hardware
//! sample-resolution wall.
//!
//! So this prints, for the railing rates (spm 12,20,30), the over-difficulty AREA
//! + CI across τ near the floor {24,30,45,60,90,150} for the TIME-EWMA, and the
//! same for the SHARE-INDEXED arm at matched windows — so the curve shape (smooth
//! vs erratic) is visible for both. Verdict per the shape, not the argmin.
//!
//! Usage: cargo run --release --bin tick-floor-probe
//! Env: VARDIFF_TFP_TRIALS (default 300 base, CI-scaled), VARDIFF_TFP_THREADS.

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    ShareIndexedEstimator, SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const SENS: f64 = 1.5;
const SPMS: &[f32] = &[12.0, 20.0, 30.0]; // the railing rates
const RATES_PPH: &[f32] = &[1.0, 2.0, 5.0, 10.0, 20.0, 40.0];
const TICK: u64 = 60; // the floor in question
// τ values straddling the tick floor: below (24,30,45), at (60), above (90,150).
const TAUS: &[u64] = &[24, 30, 45, 60, 90, 150];

/// Time-EWMA config at window τ.
fn ewma_cfg(tau: u64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(SENS, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0, clock,
        )))
    })
}

/// Share-indexed config with window of `n_span` shares — the matched contestant.
/// At window τ (secs) and rate r (spm) the EQUIVALENT share count is n_span=τ·r/60,
/// so to put the share-indexed arm at the "same window as a τ-EWMA at this rate"
/// we pass n_span = τ·spm/60. (This is the per-rate matching used to compare the
/// two arms' floor behavior at the same effective window.)
fn share_cfg(n_span: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("ShareIdx{}", n_span as u64), move |clock| {
        VardiffBox(Box::new(Composed::new(
            ShareIndexedEstimator::new(n_span),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(SENS, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0, clock,
        )))
    })
}

fn decline_areas(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> Vec<f64> {
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
    let config = TrialConfig { tick_interval_secs: TICK, ..proto };
    let d_start = mature * 60;
    let d_end = (mature + dm) * 60;
    let mut areas = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut area, mut last_t) = (0.0f64, d_start);
        for tk in &t.ticks {
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
        }
        areas.push(area);
    }
    areas
}

fn mean_ci(v: &[f64]) -> (f64, f64) {
    let n = v.len();
    if n == 0 { return (f64::NAN, f64::NAN); }
    let mean = v.iter().sum::<f64>() / n as f64;
    if n < 2 { return (mean, f64::NAN); }
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    (mean, 1.96 * (var / n as f64).sqrt())
}

/// worst-over-severity (mean, ci) at this (arm, spm, τ).
fn cell(arm_share: bool, tau: u64, spm: f32, base: usize, seed: u64) -> (f64, f64) {
    // share-indexed matched to this τ at this rate: n_span = τ·spm/60.
    let a = if arm_share { share_cfg(tau as f64 * spm as f64 / 60.0) } else { ewma_cfg(tau) };
    let (mut wm, mut wci) = (f64::MIN, f64::NAN);
    let mut k = 0u64;
    for &r in RATES_PPH {
        let ct = (base as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        let (m, ci) = mean_ci(&decline_areas(&a, r, spm, ct, seed.wrapping_add(k << 40)));
        if m > wm { wm = m; wci = ci; }
        k += 1;
    }
    (wm, wci)
}

fn main() {
    let base: usize = env::var("VARDIFF_TFP_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed = DEFAULT_BASELINE_SEED ^ 0x71CF_100;
    let nth: usize = env::var("VARDIFF_TFP_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // jobs: (arm_share, spm_i, tau_i)
    let jobs: Vec<(bool, usize, usize)> = [false, true].iter().flat_map(|&sh|
        (0..SPMS.len()).flat_map(move |si| (0..TAUS.len()).map(move |ti| (sh, si, ti)))).collect();
    eprintln!("tick-floor-probe: {} cells (2 arms × {} spm × {} τ), base {} trials, {} threads. tick={}s.",
        jobs.len(), SPMS.len(), TAUS.len(), base, nth, TICK);

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(bool, f32, u64, f64, f64)>> = Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..nth {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (sh, si, ti) = jobs[j];
                let (m, ci) = cell(sh, TAUS[ti], SPMS[si], base, seed.wrapping_add((j as u64) << 8));
                out.lock().unwrap().push((sh, SPMS[si], TAUS[ti], m, ci));
                eprintln!("  {}{} τ{} done", if sh {"share@spm"} else {"ewma@spm"}, SPMS[si], TAUS[ti]);
            });
        }
    });
    let raw = out.into_inner().unwrap();

    println!("\n## TICK-FLOOR PROBE — over-difficulty AREA (mean ± ci) vs τ near the {}s tick floor, BOTH arms.", TICK);
    println!("Q: is the curve SMOOTH-monotone toward short τ (real optimum below the floor) or ERRATIC/CI-dominated (degeneracy)?");
    println!("τ<{} is below the tick floor (time-EWMA alpha=exp(-{}/τ); at τ=24 alpha≈0.08 = near-pure-last-obs).\n", TICK, TICK);

    for &sh in &[false, true] {
        println!("### {} arm", if sh { "SHARE-INDEXED (matched n_span=τ·spm/60)" } else { "TIME-EWMA" });
        print!("| spm \\ τ |");
        for &t in TAUS { print!(" {}{} |", t, if t < TICK {"*"} else {""}); }
        println!(" shape? |");
        print!("| --- |"); for _ in TAUS { print!(" --- |"); } println!(" --- |");
        for &spm in SPMS {
            print!("| {} |", spm as u32);
            let mut means = vec![];
            for &t in TAUS {
                if let Some((_, _, _, m, ci)) = raw.iter().find(|(s, sp, tt, ..)| *s == sh && *sp == spm && *tt == t) {
                    print!(" {:.0}±{:.0} |", m, ci);
                    means.push(*m);
                } else { print!(" — |"); means.push(f64::NAN); }
            }
            // crude shape read: is the sub-floor segment (τ<60: indices 0,1,2 = 24,30,45)
            // monotone-decreasing toward short τ (smooth slide), or non-monotone (erratic)?
            let sub: Vec<f64> = TAUS.iter().zip(means.iter()).filter(|(t, _)| **t < TICK).map(|(_, m)| *m).collect();
            let monotone_down_toward_short = sub.windows(2).all(|w| w[0] <= w[1] + 1.0); // area rises as τ rises (opt below)
            let monotone_up_toward_short = sub.windows(2).all(|w| w[0] >= w[1] - 1.0);
            let shape = if monotone_down_toward_short { "smooth↓ (opt below floor — real)" }
                        else if monotone_up_toward_short { "smooth↑ (opt ABOVE floor — railing was noise)" }
                        else { "ERRATIC (degenerate below floor)" };
            println!(" {} |", shape);
        }
        println!();
    }
    println!("READ: SMOOTH↓ toward short τ in BOTH arms ⇒ real sub-tick optimum (widen grid, but it's below tick resolution).");
    println!("ERRATIC below 60 ⇒ estimator DEGENERATE at τ<tick ⇒ the railing was the scan running off the resolution edge,");
    println!("NOT a τ* finding. If BOTH arms degenerate at high rate, the high-rate regime is sub-resolution on the {}s tick —", TICK);
    println!("the sim-tick echo of the hardware sample-resolution wall. The spm6 disjoint-band slope (τ*∝1/r refuted) stands regardless.");
}
