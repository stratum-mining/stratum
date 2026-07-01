//! Estimator-side debias: how much of the champion's −7% settle gap can a
//! belief multiplier recover, and at what §10-cost price?
//!
//! THEORY PREDICTION (docs/THEORY.md §5/§9): the SignPersist champion
//! equilibrates with a persistent under-difficulty offset because the
//! tighten threshold is high. Scaling h_estimate up by `bias>1` lifts that
//! equilibrium toward truth (shrinks regret_under), but the SAME scaled
//! belief is the retarget target, so it raises regret_over. With
//! w_over:w_under = 3:1 the trade is priced AGAINST accuracy, so debias is
//! expected to reduce the settle gap while INCREASING the §10 cost. This
//! bin quantifies that curve and confirms whether −7% is a defect or the
//! deliberate cost optimum (bias=1.0 wins => deliberate).
//!
//! Two readouts per bias level:
//!   - §10 cost components (the regret/effort grid, champion = anchor).
//!   - settle gap: signed % vs truth on a stable cell, so we SEE the
//!     accuracy move even when the cost says don't.
//!
//! Usage: `cargo run --release --bin confirm-debias`
//! Env: VARDIFF_DB_TRIALS (default 1000), VARDIFF_DB_THREADS,
//!      VARDIFF_SWEEP_SEED, VARDIFF_DB_OUT_DIR (default ".").

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, DebiasEstimator, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Cell, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{run_cell_with_algorithm, AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::run_trial_observed;

const FLOOR: f64 = 0.05;
const ETA_BASE: f32 = 0.2;
const ETA_MAX: f32 = 0.8;
const ACCEL: f32 = 0.05;
const SENS: f64 = 0.3;
const TIGHTEN: f64 = 6.0;
const DISCOUNT: f64 = 0.06;
const MAX_DISCOUNT: f64 = 0.6;
const SPM_SWITCH: u32 = 6;
const TAU: u64 = 150;

struct Weights {
    w_over: f64,
    w_under: f64,
    rho_up: f64,
    rho_down: f64,
    rho: f64,
    w_det: f64,
}
impl Weights {
    fn from_env() -> Self {
        let g = |k: &str, d: f64| env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
        Weights {
            w_over: g("VARDIFF_W_OVER", 3.0),
            w_under: g("VARDIFF_W_UNDER", 1.0),
            rho_up: g("VARDIFF_RHO_UP", 3.0),
            rho_down: g("VARDIFF_RHO_DOWN", 1.0),
            rho: g("VARDIFF_RHO", 0.5),
            w_det: g("VARDIFF_W_DET", 0.5),
        }
    }
}

fn champ_boundary() -> AdaptiveSignPersist {
    AdaptiveSignPersist::sign_persist(
        SignPersistenceCusumBoundary::new(SENS, FLOOR, TIGHTEN, DISCOUNT, MAX_DISCOUNT),
        SPM_SWITCH,
    )
}

/// Champion with an estimator debias factor (bias=1.0 == the champion).
fn debias_spec(bias: f32) -> AlgorithmSpec {
    let name = vardiff_sim::naming::triple_name(
        &DebiasEstimator::new(EwmaEstimator::new(TAU), bias),
        &champ_boundary(),
        &AcceleratingPartialRetarget::new(ETA_BASE, ETA_MAX, ACCEL),
    );
    AlgorithmSpec::new(name, move |clock| {
        VardiffBox(Box::new(Composed::new(
            DebiasEstimator::new(EwmaEstimator::new(TAU), bias),
            champ_boundary(),
            AcceleratingPartialRetarget::new(ETA_BASE, ETA_MAX, ACCEL),
            1.0,
            clock,
        )))
    })
}

struct Row {
    bias: f32,
    cost: f64,
    regret_over: f64,
    regret_under: f64,
    detection: f64,
    settle_gap_pct: f64,
}

fn cost(w: &Weights, ro: f64, ru: f64, eu: f64, ed: f64, det: f64) -> f64 {
    w.w_over * ro + w.w_under * ru + w.rho * (w.rho_up * eu + w.rho_down * ed) + w.w_det * (1.0 - det)
}

fn settle_gap_pct(bias: f32, trials: usize, base_seed: u64) -> f64 {
    // Stable scenario at 12 spm; median estimate over the back half vs truth.
    let spm = 12.0f32;
    let scen = Scenario::Stable;
    let (config, schedule) = scen.build(spm);
    let algo = debias_spec(bias);
    let n_ticks = (config.duration_secs / config.tick_interval_secs) as usize;
    let start = n_ticks / 2;
    let mut vals: Vec<f64> = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (algo.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &schedule, base_seed.wrapping_add(i as u64));
        let back: Vec<f64> = t.ticks.iter().skip(start).map(|tk| tk.current_hashrate_before as f64).collect();
        if !back.is_empty() {
            let mut b = back.clone();
            b.sort_by(|a, c| a.partial_cmp(c).unwrap());
            vals.push(b[b.len() / 2]);
        }
    }
    vals.sort_by(|a, c| a.partial_cmp(c).unwrap());
    let med = if vals.is_empty() { TRUE_HASHRATE as f64 } else { vals[vals.len() / 2] };
    (med / TRUE_HASHRATE as f64 - 1.0) * 100.0
}

fn main() -> std::io::Result<()> {
    let trials: usize = env::var("VARDIFF_DB_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_DB_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
        .max(1);
    let out_dir = env::var("VARDIFF_DB_OUT_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let w = Weights::from_env();

    let biases = [1.0f32, 1.02, 1.05, 1.08, 1.12, 1.18, 1.25];

    let scenarios = vec![
        Scenario::Stable,
        Scenario::Step { delta_pct: -50 },
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: 10 },
        Scenario::Step { delta_pct: 50 },
        Scenario::SettledStep { settle_minutes: 60, delta_pct: -10 },
    ];
    let share_rates = vec![6.0f32, 8.0, 12.0, 20.0, 30.0];
    let n_spm = share_rates.len();
    let n_scen = scenarios.len();

    eprintln!("confirm-debias: {} bias levels x {} cells x {} trials, {} threads", biases.len(), n_spm * n_scen, trials, n_threads);
    let started = Instant::now();

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Row>> = Mutex::new(Vec::with_capacity(biases.len()));
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let bi = next.fetch_add(1, Ordering::Relaxed);
                if bi >= biases.len() {
                    break;
                }
                let bias = biases[bi];
                let algo = debias_spec(bias);
                let (mut ro, mut ru, mut eu, mut ed) = (0.0, 0.0, 0.0, 0.0);
                let mut n = 0u32;
                let (mut ds, mut dn) = (0.0, 0u32);
                for (si, &spm) in share_rates.iter().enumerate() {
                    for (ci, scen) in scenarios.iter().enumerate() {
                        let cell = Cell { shares_per_minute: spm, scenario: scen.clone() };
                        let cell_index = (bi * n_spm * n_scen + si * n_scen + ci) as u64;
                        let r = run_cell_with_algorithm(&algo, &cell, trials, base_seed, cell_index);
                        if matches!(scen, Scenario::Stable | Scenario::Step { delta_pct: -50 | -10 | 10 | 50 }) {
                            ro += r.get("regret_over").unwrap_or(0.0);
                            ru += r.get("regret_under").unwrap_or(0.0);
                            eu += r.get("effort_up").unwrap_or(0.0);
                            ed += r.get("effort_down").unwrap_or(0.0);
                            n += 1;
                        }
                        if matches!(scen, Scenario::SettledStep { settle_minutes: 60, delta_pct: -10 }) {
                            if let Some(rate) = r.get("settled_reaction_rate") {
                                ds += rate;
                                dn += 1;
                            }
                        }
                    }
                }
                let nf = n.max(1) as f64;
                let (ro, ru, eu, ed) = (ro / nf, ru / nf, eu / nf, ed / nf);
                let det = if dn > 0 { ds / dn as f64 } else { 0.0 };
                let gap = settle_gap_pct(bias, trials, base_seed ^ 0x5EeD);
                let row = Row {
                    bias,
                    cost: cost(&w, ro, ru, eu, ed, det),
                    regret_over: ro,
                    regret_under: ru,
                    detection: det,
                    settle_gap_pct: gap,
                };
                out.lock().unwrap().push(row);
            });
        }
    });
    let mut rows = out.into_inner().unwrap();
    eprintln!("Done in {:.1}s", started.elapsed().as_secs_f64());
    rows.sort_by(|a, b| a.bias.partial_cmp(&b.bias).unwrap());

    let mut md = String::new();
    md.push_str("# Estimator debias sweep\n\n");
    md.push_str(&format!("{} trials/cell, base_seed {:#x}. bias=1.0 is the champion. Cost = §10 (3:1). settle_gap = signed % vs truth, stable@12spm back-half median.\n\n", trials, base_seed));
    md.push_str("| bias | cost | reg_over | reg_under | det% | settle gap |\n| --- | --- | --- | --- | --- | --- |\n");
    for r in &rows {
        md.push_str(&format!(
            "| {:.2} | {:.4} | {:.4} | {:.4} | {:.0}% | {:+.1}% |\n",
            r.bias, r.cost, r.regret_over, r.regret_under, r.detection * 100.0, r.settle_gap_pct
        ));
    }
    let best = rows.iter().enumerate().min_by(|a, b| a.1.cost.partial_cmp(&b.1.cost).unwrap()).unwrap();
    md.push_str(&format!("\nLowest §10 cost at bias={:.2}. ", rows[best.0].bias));
    if (rows[best.0].bias - 1.0).abs() < 1e-6 {
        md.push_str("bias=1.0 wins => the −7% settle gap is the deliberate cost optimum; debias is cost-negative as theory predicted.\n");
    } else {
        md.push_str("a debias > 1.0 wins => the settle offset was leaving cost on the table after all.\n");
    }
    let out_path = out_dir.join("confirm_debias.md");
    fs::write(&out_path, &md)?;
    eprintln!("Wrote {}", out_path.display());

    println!("\n## Estimator debias sweep ({} trials) — cost vs settle-gap tradeoff\n", trials);
    println!("| bias | cost | reg_over | reg_under | det% | settle gap |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for r in &rows {
        println!(
            "| {:.2} | {:.4} | {:.4} | {:.4} | {:.0}% | {:+.1}% |",
            r.bias, r.cost, r.regret_over, r.regret_under, r.detection * 100.0, r.settle_gap_pct
        );
    }
    println!(
        "\nLowest §10 cost at bias={:.2} (cost {:.4}). {}",
        rows[best.0].bias, rows[best.0].cost,
        if (rows[best.0].bias - 1.0).abs() < 1e-6 {
            "bias=1.0 wins -> -7% gap is the deliberate optimum; debias trades accuracy for cost as predicted."
        } else {
            "a debias>1 wins -> the settle offset was leaving cost on the table."
        }
    );
    Ok(())
}
