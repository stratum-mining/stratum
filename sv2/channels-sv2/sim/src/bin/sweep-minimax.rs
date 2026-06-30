//! Minimax-over-r* champion selection (the LAST sweep; the criterion is the
//! final degree of freedom and is fixed BEFORE the curves are seen).
//!
//! r* is set once per deployment but its value is unknown, so the right
//! criterion is NOT single-rate optimality and NOT rate-elasticity (the slope
//! target only if rate were swept dynamically). For a static-but-unknown
//! parameter the target is UNIFORM near-optimality across the band: pick the
//! config whose WORST gap-to-the-per-rate-frontier over r* ∈ {4,6,12,30} is
//! smallest. A config that wins at 30 but is mediocre at 6 loses to one that
//! is good everywhere — because you might deploy at 6.
//!
//! Selection cost (per rate, corrected metric, the defensible scalar):
//!   cost(r*) = 3·reg_over + 1·reg_under + ρ·(quad_eff_dir + λ·lin_eff_dir)
//! measured AT THAT RATE over the scenario mix. The Stable scenario at each
//! rate carries that config's per-rate false-alarm firing into the effort
//! term, so "each config at its own false-alarm rate at each r*" is satisfied
//! structurally — no pinned ARL0 convention is reused across rates (the trap
//! that the gate work fell into). Detection is NOT in the scalar (an invented
//! weight we removed); it is reported as a per-rate diagnostic only.
//!
//! Frontier(r*) = min cost over the field at that rate (empirical best-in-
//! field). ratio(config,r*) = cost / frontier ≥ 1 (1.0 = on the frontier at
//! that rate). minimax(config) = max over {4,6,12,30} of ratio. Winner =
//! argmin minimax. 60 spm reported as a high-r* anchor, NOT in the minimax.
//!
//! The band-winner may be SHORTER-window / MORE-sensitive than the single-
//! rate champion (Ewma360/s1.5): a 360s window throws away little at 6 spm
//! but leaves agility on the table at 30 spm where there is 5x more info per
//! window. Re-selecting toward sensitivity is not a contradiction of the
//! "s0.3 was over-tuned" lesson — that lesson was for 6 spm; the band is
//! wider and the false-alarm cost of sensitivity drops as the floor shrinks.
//!
//! Usage: cargo run --release --bin sweep-minimax
//! Env: VARDIFF_MM_TRIALS (default 300, CI-scaled up at low spm), VARDIFF_SWEEP_SEED.

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const FLOOR: f64 = 0.05;
const W_OVER: f64 = 3.0;
const W_UNDER: f64 = 1.0;
const RHO_UP: f64 = 3.0;
const RHO_DOWN: f64 = 1.0;
const RHO: f64 = 0.5;
const LAMBDAS: [f64; 4] = [0.0, 0.5, 1.0, 2.0];

// Minimax band (the operator might set r* anywhere here). 60 is a high anchor
// reported for context but NOT included in the minimax max.
const BAND: [f32; 4] = [4.0, 6.0, 12.0, 30.0];
const ANCHOR: f32 = 60.0;

#[derive(Clone, Copy)]
struct P {
    tau: u64,
    sens: f64,
    tighten: f64,
    eta_max: f32,
}

fn spec(p: P) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{}/s{}/t{}", p.tau, p.sens, p.tighten), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(p.tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(p.sens, FLOOR, p.tighten, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, p.eta_max, 0.05),
            1.0,
            clock,
        )))
    })
}

/// Corrected-metric components at a SINGLE rate over the scenario mix.
/// Returns (reg_over, reg_under, quad_eff, lin_eff), direction-weighted.
fn components(mk: &dyn Fn() -> AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> (f64, f64, f64, f64) {
    let scens = [
        Scenario::Stable, // carries per-rate false-alarm firing into effort
        Scenario::Step { delta_pct: -50 },
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: 10 },
        Scenario::Step { delta_pct: 50 },
    ];
    let (mut ro, mut ru, mut q, mut l, mut n) = (0.0, 0.0, 0.0, 0.0, 0u32);
    for (si, scen) in scens.iter().enumerate() {
        let (cfg, sched) = scen.build(spm);
        let config = TrialConfig { tick_interval_secs: 60, ..cfg };
        for i in 0..trials {
            let clock = Arc::new(MockClock::new(0));
            let v = (mk)().factory.clone()(clock.clone());
            let t = run_trial_observed(v, clock, config.clone(), &sched,
                seed.wrapping_add(((si as u64) << 20) + ((spm as u64) << 32) + i as u64));
            let mut last_t = 0u64;
            let (mut tro, mut tru, mut cov) = (0.0, 0.0, 0.0);
            for tk in &t.ticks {
                let dt = (tk.t_secs - last_t) as f64; last_t = tk.t_secs;
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                let h_est = tk.current_hashrate_before as f64;
                if dt > 0.0 && h_true > 0.0 && h_est > 0.0 {
                    let e = (h_est / h_true).ln();
                    if e >= 0.0 { tro += dt * e.abs(); } else { tru += dt * e.abs(); }
                    cov += dt;
                }
                if tk.fired {
                    if let Some(nh) = tk.new_hashrate {
                        let old = tk.current_hashrate_before as f64;
                        if old > 0.0 && nh as f64 > 0.0 {
                            let dl = (nh as f64 / old).ln();
                            let dir = if dl >= 0.0 { RHO_UP } else { RHO_DOWN };
                            q += dir * dl * dl;
                            l += dir * dl.abs();
                        }
                    }
                }
            }
            if cov > 0.0 { ro += tro / cov; ru += tru / cov; }
            n += 1;
        }
    }
    let nf = n.max(1) as f64;
    (ro / nf, ru / nf, q / nf, l / nf)
}

fn cost(c: (f64, f64, f64, f64), lambda: f64) -> f64 {
    W_OVER * c.0 + W_UNDER * c.1 + RHO * (c.2 + lambda * c.3)
}

fn main() {
    let base_trials: usize = env::var("VARDIFF_MM_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);

    // CI-match: estimator variance ∝ 1/(r*·τ); oversample low rates so a
    // 4-spm cell's cost estimate is as tight as a 30-spm cell's.
    let trials_for = |spm: f32| -> usize { (base_trials as f64 * (12.0 / spm as f64).clamp(1.0, 4.0)).round() as usize };

    // Field: tau × sens grid (the two axes the band-criterion stresses),
    // well-established params fixed. Plus classic as an architecture anchor.
    let taus = [120u64, 150, 240, 360, 480, 720];
    let senss = [0.3f64, 0.6, 1.0, 1.5, 2.0];
    let mut field: Vec<(String, Box<dyn Fn() -> AlgorithmSpec + Send + Sync>)> = Vec::new();
    for &tau in &taus {
        for &sens in &senss {
            let p = P { tau, sens, tighten: 8.0, eta_max: 0.6 };
            field.push((spec(p).name.clone(), Box::new(move || spec(p))));
        }
    }
    field.push(("classic".into(), Box::new(AlgorithmSpec::classic_composed)));
    let n_cfg = field.len();

    let rates: Vec<f32> = BAND.iter().copied().chain([ANCHOR]).collect();
    let n_rate = rates.len();

    eprintln!(
        "sweep-minimax: {} configs × {} rates × 5 scen, base {} trials (low-spm up to 4x), criterion=minimax ratio over {:?}",
        n_cfg, n_rate, base_trials, BAND
    );

    // Jobs: (cfg_idx, rate_idx) → components.
    let jobs: Vec<(usize, usize)> =
        (0..n_cfg).flat_map(|ci| (0..n_rate).map(move |ri| (ci, ri))).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    // comps[ci][ri]
    let comps: Mutex<Vec<Vec<(f64, f64, f64, f64)>>> =
        Mutex::new(vec![vec![(0.0, 0.0, 0.0, 0.0); n_rate]; n_cfg]);
    let started = Instant::now();
    let n_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..n_threads {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (ci, ri) = jobs[j];
                let spm = rates[ri];
                let c = components(field[ci].1.as_ref(), spm, trials_for(spm), base_seed ^ (spm as u64));
                comps.lock().unwrap()[ci][ri] = c;
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 32 == 0 { eprintln!("  {}/{} ({:.0}s)", d, jobs.len(), started.elapsed().as_secs_f64()); }
            });
        }
    });
    let comps = comps.into_inner().unwrap();
    eprintln!("done in {:.0}s", started.elapsed().as_secs_f64());

    // Analysis per λ. The winner must be stable across λ (not free-tuned).
    for &lambda in &LAMBDAS {
        // Per-rate frontier (min cost over field).
        let mut frontier = vec![f64::MAX; n_rate];
        for ci in 0..n_cfg {
            for ri in 0..n_rate {
                frontier[ri] = frontier[ri].min(cost(comps[ci][ri], lambda));
            }
        }
        // Per-config: ratio at each rate, minimax over BAND (exclude anchor).
        let band_idx: Vec<usize> = (0..n_rate).filter(|&ri| rates[ri] != ANCHOR).collect();
        let mut ranked: Vec<(usize, f64, Vec<f64>)> = (0..n_cfg)
            .map(|ci| {
                let ratios: Vec<f64> = (0..n_rate).map(|ri| cost(comps[ci][ri], lambda) / frontier[ri]).collect();
                let minimax = band_idx.iter().map(|&ri| ratios[ri]).fold(f64::MIN, f64::max);
                (ci, minimax, ratios)
            })
            .collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        println!("\n## λ = {lambda}  —  minimax gap-to-frontier (ratio = cost / best-in-field at that rate)");
        print!("| rank | config | minimax |");
        for &r in &rates { print!(" r{}{} |", r as u32, if r == ANCHOR { "*" } else { "" }); }
        println!();
        print!("| --- | --- | --- |");
        for _ in &rates { print!(" --- |"); }
        println!();
        for (rank, (ci, mm, ratios)) in ranked.iter().enumerate().take(12) {
            print!("| {} | {} | {:.3} |", rank + 1, field[*ci].0, mm);
            for (ri, &x) in ratios.iter().enumerate() {
                // mark the rate that sets the minimax (the worst band rate)
                let is_worst = rates[ri] != ANCHOR && (x - mm).abs() < 1e-9;
                print!(" {:.3}{} |", x, if is_worst { "◄" } else { "" });
            }
            println!();
        }
        println!("(* = high-r* anchor, not in the minimax; ◄ = the band rate that sets this config's worst gap)");
        let win = &ranked[0];
        println!(">> λ={lambda} band-winner: {}  (worst-rate ratio {:.3})", field[win.0].0, win.1);
    }

    println!("\nThe band-winner is the config best for ANY r* you might statically set in 4–30.");
    println!("If it is stable across λ, it is the champion; re-clear its decline safety across the");
    println!("rate range (slow-decline.rs) before it ships — that safety is UNINHERITED from Ewma360.");
}
