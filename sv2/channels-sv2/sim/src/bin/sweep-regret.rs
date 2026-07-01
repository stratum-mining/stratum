//! Regret/effort-scored parameter sweep — the §10 replacement for the
//! maximin-over-`EqualWeightFitness` sweeps (`sweep-balanced` et al.).
//!
//! Scores each candidate by a single **cost to minimize** built from the
//! `LogErrorRegret` decomposition plus an explicit slow-degradation
//! detection term (§9.4 — detection is NOT recoverable from regret):
//!
//! ```text
//! cost =  w_over·regret_over + w_under·regret_under          (tracking)
//!      +  rho·( rho_up·effort_up + rho_down·effort_down )    (control effort)
//!      +  w_det·( 1 − detection )                            (missed slow drop)
//! ```
//!
//! - regret_* are LINEAR ⟨|e|⟩ (the chosen loss shape: keeps small
//!   persistent drops visible), `e = ln(H_est/H_true)`, averaged over a
//!   transient+steady scenario mix (cold-start excluded — one-time,
//!   dominates, washes out differences).
//! - effort_* = Σ(Δln D)² over fires, split by direction.
//! - detection = P[fire within 60min | aged-counter −10% drop], the
//!   operationally critical failing-ASIC case.
//!
//! ## Weights (the values call — see docs/THEORY.md §10)
//!
//! Directional asymmetries are well-grounded (commit `a1d3fa7b`
//! tighten_multiplier=3.0; the §5.1/§9.2 death-spiral physics):
//!   w_over:w_under = 3:1,  rho_up:rho_down = 3:1.
//! The cross-block weights are judgment: rho (effort vs regret) = 0.5,
//! w_det (miss penalty) = 0.5. All are env-overridable so the ranking's
//! sensitivity to them can be checked.
//!
//! ## Usage
//! ```text
//! cargo run --release --bin sweep-regret
//! VARDIFF_SWEEP_TRIALS=300 cargo run --release --bin sweep-regret
//! VARDIFF_W_OVER=3 VARDIFF_RHO=0.5 VARDIFF_W_DET=0.5 cargo run --release --bin sweep-regret
//! ```
//! Env: VARDIFF_SWEEP_TRIALS (default 300), VARDIFF_SWEEP_SEED,
//!      VARDIFF_W_OVER/W_UNDER/RHO_UP/RHO_DOWN/RHO/W_DET (weight overrides),
//!      VARDIFF_SWEEP_OUT_DIR (default "."), writes `regret_sweep.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, Composed,
    EwmaEstimator, PoissonCI,
};
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};

/// Weights (all relative to the "cheap" sides = 1.0). See module docs.
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
        let g = |k: &str, d: f64| {
            env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d)
        };
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

/// Aggregated cost components for one algorithm (means over the mix).
struct Profile {
    name: String,
    regret_over: f64,
    regret_under: f64,
    effort_up: f64,
    effort_down: f64,
    detection: f64,
    cost: f64,
}

fn main() -> std::io::Result<()> {
    let trial_count: usize = env::var("VARDIFF_SWEEP_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| {
            s.strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .or_else(|| s.parse().ok())
        })
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_SWEEP_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let w = Weights::from_env();

    // Search space, centered on the maximin-leader family (same family the
    // old sweep-balanced explored, so results are comparable).
    let taus = [60u64, 75, 90, 105, 120];
    let sensitivities = [1.0f64, 1.5, 2.0];
    let tightens = [2.0f64, 3.0];
    let transitions = [8u32, 10, 12];
    let eta_bases = [0.2f32, 0.3];
    let eta_maxes = [0.4f32, 0.6];
    let acceleration = 0.2f32;

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();
    // Reference anchors.
    algorithms.push(AlgorithmSpec::classic_vardiff_state());
    algorithms.push(AlgorithmSpec::balanced());
    algorithms.push(AlgorithmSpec::react_priority());

    for &tau in &taus {
        for &sens in &sensitivities {
            for &tighten in &tightens {
                for &trans in &transitions {
                    for &eb in &eta_bases {
                        for &em in &eta_maxes {
                            if em < eb {
                                continue;
                            }
                            let name = vardiff_sim::naming::triple_name(
                                &EwmaEstimator::new(tau),
                                &AdaptivePoissonCusum::with_params(
                                    PoissonCI::default_parametric(),
                                    AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                                    trans,
                                ),
                                &AcceleratingPartialRetarget::new(eb, em, acceleration),
                            );
                            algorithms.push(AlgorithmSpec::new(name, move |clock| {
                                let boundary = AdaptivePoissonCusum::with_params(
                                    PoissonCI::default_parametric(),
                                    AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                                    trans,
                                );
                                let inner = Composed::new(
                                    EwmaEstimator::new(tau),
                                    boundary,
                                    AcceleratingPartialRetarget::new(eb, em, acceleration),
                                    1.0,
                                    clock,
                                );
                                VardiffBox(Box::new(inner))
                            }));
                        }
                    }
                }
            }
        }
    }

    // Scenarios: Stable + ±10/±50 steps feed regret/effort; the
    // SettledStep{60,−10} cell feeds detection (settled_reaction_rate).
    let scenarios = vec![
        Scenario::Stable,
        Scenario::Step { delta_pct: -50 },
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: 10 },
        Scenario::Step { delta_pct: 50 },
        Scenario::SettledStep {
            settle_minutes: 60,
            delta_pct: -10,
        },
    ];
    let share_rates = vec![6.0f32, 8.0, 12.0, 20.0, 30.0];

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios: scenarios.clone(),
        trial_count,
        base_seed,
    };

    eprintln!(
        "Sweeping {} algorithms × {} cells × {} trials = {} total",
        grid.algorithms.len(),
        share_rates.len() * scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
    );
    let started = Instant::now();
    let results = grid.run();
    eprintln!("Sweep complete in {:.1}s", started.elapsed().as_secs_f64());

    // Aggregate the per-cell LogErrorRegret values (already computed by the
    // registry during the grid run) over the regret/effort scenario mix,
    // and pull detection from the SettledStep cell.
    let mut profiles: Vec<Profile> = Vec::new();
    for (name, cells) in &results {
        let mut ro = 0.0;
        let mut ru = 0.0;
        let mut eu = 0.0;
        let mut ed = 0.0;
        let mut n = 0u32;
        let mut det_sum = 0.0;
        let mut det_n = 0u32;
        for cell in cells {
            let is_mix = matches!(
                cell.scenario,
                Scenario::Stable
                    | Scenario::Step {
                        delta_pct: -50 | -10 | 10 | 50
                    }
            );
            if is_mix {
                ro += cell.get("regret_over").unwrap_or(0.0);
                ru += cell.get("regret_under").unwrap_or(0.0);
                eu += cell.get("effort_up").unwrap_or(0.0);
                ed += cell.get("effort_down").unwrap_or(0.0);
                n += 1;
            }
            if matches!(
                cell.scenario,
                Scenario::SettledStep {
                    settle_minutes: 60,
                    delta_pct: -10
                }
            ) {
                if let Some(rate) = cell.get("settled_reaction_rate") {
                    det_sum += rate;
                    det_n += 1;
                }
            }
        }
        if n == 0 {
            continue;
        }
        let (regret_over, regret_under, effort_up, effort_down) = (
            ro / n as f64,
            ru / n as f64,
            eu / n as f64,
            ed / n as f64,
        );
        let detection = if det_n > 0 {
            det_sum / det_n as f64
        } else {
            0.0
        };
        let cost = w.w_over * regret_over
            + w.w_under * regret_under
            + w.rho * (w.rho_up * effort_up + w.rho_down * effort_down)
            + w.w_det * (1.0 - detection);
        profiles.push(Profile {
            name: name.clone(),
            regret_over,
            regret_under,
            effort_up,
            effort_down,
            detection,
            cost,
        });
    }

    // Rank by cost ascending (lower = better).
    profiles.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());

    let mut md = String::new();
    md.push_str("# Regret/effort sweep\n\n");
    md.push_str(&format!(
        "Cost = {:.1}·regret_over + {:.1}·regret_under + {:.2}·({:.1}·effort_up + {:.1}·effort_down) + {:.2}·(1−detection). Lower is better.\n\n",
        w.w_over, w.w_under, w.rho, w.rho_up, w.rho_down, w.w_det
    ));
    md.push_str(&format!(
        "{} trials/cell, base_seed {:#x}. regret = linear ⟨|e|⟩; detection = P[fire≤60min | aged −10%].\n\n",
        trial_count, base_seed
    ));
    md.push_str("| rank | algorithm | **cost** | reg_over | reg_under | eff_up | eff_down | det% |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for (i, p) in profiles.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | **{:.4}** | {:.4} | {:.4} | {:.4} | {:.4} | {:.0}% |\n",
            i + 1,
            p.name,
            p.cost,
            p.regret_over,
            p.regret_under,
            p.effort_up,
            p.effort_down,
            p.detection * 100.0,
        ));
    }
    md.push('\n');

    let out_path = out_dir.join("regret_sweep.md");
    fs::write(&out_path, &md)?;
    eprintln!("Wrote {}", out_path.display());

    // Also echo the top 15 to stdout for quick inspection.
    println!("\n## Regret/effort sweep — top 15 (lower cost = better)\n");
    println!("| rank | algorithm | cost | reg_over | reg_under | eff_up | eff_down | det% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for (i, p) in profiles.iter().take(15).enumerate() {
        println!(
            "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.0}% |",
            i + 1,
            p.name,
            p.cost,
            p.regret_over,
            p.regret_under,
            p.effort_up,
            p.effort_down,
            p.detection * 100.0,
        );
    }
    Ok(())
}
