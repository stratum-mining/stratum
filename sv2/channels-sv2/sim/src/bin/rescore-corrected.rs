//! Two-regime re-score on the CORRECTED metric (reviewer checks 1+2+3).
//!
//! The original 9,216-config search optimized against a detection term that
//! the no-drop control proved saturated at the scored 60-min window, and an
//! effort term (Σs²) that under-charges frequent tiny fires. This re-scores
//! the champion against the field on a corrected objective:
//!
//!   - detection REMOVED from the production scalar: the control showed it is
//!     information-floor-flat at 4–6 spm (Theorem 2), so it carries no
//!     ranking signal there. It is reported separately as a high-r*
//!     diagnostic (EXCESS at a 15-min window) where the champion leads.
//!   - effort gains the LINEAR Σ|Δln D| term (the stale-share / actuation
//!     cost), alongside Σ(Δln D)², closing the high-frequency/low-amplitude
//!     blind spot from the other side.
//!
//! Two regimes:
//!   production (spm 4,6): scalar = regret + ρ·(quad effort + λ·linear effort)
//!   fast       (spm 30,60): same + detection EXCESS term (it discriminates here)
//!
//! If the champion wins production, the headline holds. If a different config
//! wins, that is the real answer to "are we presenting the best algorithm".

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AdaptiveSignPersist, AsymmetricCusumBoundary,
    Composed, EwmaEstimator, PoissonCI, SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Cell, Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const ETA_BASE: f32 = 0.2;
const ETA_MAX: f32 = 0.8;
const ACCEL: f32 = 0.05;

// Weights: directional 3:1 (proved), rho effort/regret 0.5, and LAMBDA the
// new linear-effort coefficient relative to quadratic. We sweep lambda to
// show the champion's robustness to it, not just one value.
const W_OVER: f64 = 3.0;
const W_UNDER: f64 = 1.0;
const RHO_UP: f64 = 3.0;
const RHO_DOWN: f64 = 1.0;
const RHO: f64 = 0.5;

fn champion() -> AlgorithmSpec { AlgorithmSpec::champion() }
fn interim() -> AlgorithmSpec {
    AlgorithmSpec::new("interim(AsymCusum)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptivePoissonCusum::with_params(PoissonCI::default_parametric(),
                AsymmetricCusumBoundary::new(0.2, 0.05, 6.0), 5),
            AcceleratingPartialRetarget::new(ETA_BASE, ETA_MAX, ACCEL), 1.0, clock,
        )))
    })
}
// A deliberately gentler-but-rarer-firing contender: longer tau, higher
// sensitivity (fires less), to probe whether the linear-effort charge favors
// configs that fire less often than the champion.
fn rare_firer() -> AlgorithmSpec {
    AlgorithmSpec::new("rare(Ewma150/SP-s0.6-t6-d0.06)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(0.6, 0.05, 6.0, 0.06, 0.6), 6),
            AcceleratingPartialRetarget::new(ETA_BASE, ETA_MAX, ACCEL), 1.0, clock,
        )))
    })
}

/// Aggregate regret + both effort flavors over the production scenario mix,
/// from the universal trajectory. Returns (regret_over, regret_under,
/// quad_effort, lin_effort).
fn components(mk: &dyn Fn() -> AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> (f64, f64, f64, f64) {
    let scens = [
        Scenario::Stable,
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
            let t = run_trial_observed(v, clock, config.clone(),
                &sched, seed.wrapping_add(((si as u64) << 20) + i as u64));
            let mut last_t = 0u64;
            let (mut tro, mut tru, mut covered) = (0.0, 0.0, 0.0);
            for tk in &t.ticks {
                let dt = (tk.t_secs - last_t) as f64; last_t = tk.t_secs;
                let mid = tk.t_secs.saturating_sub(30);
                let h_true = sched.at(mid) as f64;
                let h_est = tk.current_hashrate_before as f64;
                if dt > 0.0 && h_true > 0.0 && h_est > 0.0 {
                    let e = (h_est / h_true).ln();
                    if e >= 0.0 { tro += dt * e.abs(); } else { tru += dt * e.abs(); }
                    covered += dt;
                }
                if tk.fired {
                    if let Some(nh) = tk.new_hashrate {
                        let old = tk.current_hashrate_before as f64;
                        if old > 0.0 && nh as f64 > 0.0 {
                            let dl = (nh as f64 / old).ln();
                            // direction-weighted both flavors
                            let dir = if dl >= 0.0 { RHO_UP } else { RHO_DOWN };
                            q += dir * dl * dl;
                            l += dir * dl.abs();
                        }
                    }
                }
            }
            if covered > 0.0 { ro += tro / covered; ru += tru / covered; }
            n += 1;
        }
    }
    let nf = n.max(1) as f64;
    (ro / nf, ru / nf, q / nf, l / nf)
}

// Champion-family spec parameterized by sensitivity (the fire-rate knob).
// All else = champion: Ewma150 / SignPersist[f0.05,t6,d0.06,dm0.6,spm6] /
// Accel-0.2-0.8-0.05. Higher sensitivity ⇒ fires less.
fn family(sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("champ-s{sens}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(sens, 0.05, 6.0, 0.06, 0.6), 6),
            AcceleratingPartialRetarget::new(ETA_BASE, ETA_MAX, ACCEL), 1.0, clock,
        )))
    })
}

fn main() {
    let trials = 1500usize; // high-trial confirmation
    // Sweep sensitivity across the champion family (champion = s0.3), plus
    // anchors. Names ordered so the sweep reads as a curve.
    let sens_grid = [0.3f64, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0];
    let mut builders: Vec<(String, Box<dyn Fn() -> AlgorithmSpec + Send + Sync>)> = Vec::new();
    for &s in &sens_grid {
        let label = if (s - 0.3).abs() < 1e-9 { format!("champ-s{s} (current)") } else { format!("champ-s{s}") };
        builders.push((label, Box::new(move || family(s))));
    }
    builders.push(("interim(AsymCusum)".into(), Box::new(interim)));
    builders.push(("classic".into(), Box::new(AlgorithmSpec::classic_composed)));
    let _ = (champion as fn() -> AlgorithmSpec, rare_firer as fn() -> AlgorithmSpec); // retain refs

    // production regime: spm 4,6. Average components over both.
    let prod_spms = [4.0f32, 6.0];

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(String, f64, f64, f64, f64)>> = Mutex::new(Vec::new());
    let n_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..n_threads {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= builders.len() { break; }
                let (name, mk) = &builders[j];
                let (mut ro, mut ru, mut q, mut l) = (0.0, 0.0, 0.0, 0.0);
                for &spm in &prod_spms {
                    let (a,b,c,d) = components(mk.as_ref(), spm, trials, DEFAULT_BASELINE_SEED ^ (spm as u64));
                    ro+=a; ru+=b; q+=c; l+=d;
                }
                let k = prod_spms.len() as f64;
                out.lock().unwrap().push((name.clone(), ro/k, ru/k, q/k, l/k));
            });
        }
    });
    let comps = out.into_inner().unwrap();

    // Score under a sweep of lambda (linear-effort coefficient). lambda=0
    // reproduces the old quadratic-only effort; raising it charges frequency.
    println!("## Production re-score (spm 4–6), corrected metric. detection OUT (floor-flat).");
    println!("cost = {W_OVER}·reg_over + {W_UNDER}·reg_under + {RHO}·(quad_eff + λ·lin_eff). Lower=better.\n");
    for lambda in [0.0f64, 0.5, 1.0, 2.0] {
        let mut ranked: Vec<(&String, f64, f64, f64, f64, f64)> = comps.iter().map(|(n,ro,ru,q,l)| {
            let cost = W_OVER*ro + W_UNDER*ru + RHO*(q + lambda*l);
            (n, cost, *ro, *ru, *q, *l)
        }).collect();
        ranked.sort_by(|a,b| a.1.partial_cmp(&b.1).unwrap());
        println!("### λ = {lambda}  (linear-effort weight)");
        println!("| rank | algo | cost | reg_over | reg_under | quad_eff | lin_eff |");
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        for (i,(n,c,ro,ru,q,l)) in ranked.iter().enumerate() {
            println!("| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} |", i+1, n, c, ro, ru, q, l);
        }
        println!();
    }
    println!("If 'champion' stays rank 1 across λ, it survives the linear-effort correction.");
    println!("If 'rare_firer' or another config overtakes it as λ rises, frequent firing was");
    println!("being under-charged and the champion was over-credited — the real finding.");
}
