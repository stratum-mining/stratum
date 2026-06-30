//! Maximin-scored parameter sweep targeting a *balanced* vardiff algorithm.
//!
//! The radar analysis showed every shipped algorithm has at least one weak
//! axis (≤0.45), with react−10% and convergence the universal graveyards.
//! This sweep searches the maximin leader family — EwmaEstimator +
//! AdaptivePoissonCusum + AcceleratingPartialRetarget — for a triple whose
//! WORST equal-weight axis is as high as possible (maximin), i.e. no big gap.
//!
//! Axes (all higher = better), from `EqualWeightFitness`:
//!   react_10, react_50, jitter, step_safety, convergence, overshoot
//!
//! Score per algorithm = min over the 6 axes (averaged across share rates).
//! Output is ranked by that maximin score; ties broken by mean.
//!
//! ## Usage
//! ```text
//! cargo run --release --bin sweep-balanced
//! VARDIFF_SWEEP_TRIALS=300 cargo run --release --bin sweep-balanced
//! ```
//! Env: VARDIFF_SWEEP_TRIALS (default 300), VARDIFF_SWEEP_SEED,
//!      VARDIFF_SWEEP_OUT_DIR (default "."), writes `balanced_sweep.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};
use vardiff_sim::metrics::{DerivedMetric, EqualWeightFitness};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, Composed,
    EwmaEstimator, PoissonCI,
};

/// The six equal-weight axes in plot order, with display labels.
const AXES: &[(&str, &str)] = &[
    ("reaction_10", "react-10%"),
    ("reaction_50", "react-50%"),
    ("jitter", "jitter"),
    ("step_safety", "step-safe"),
    ("convergence", "conv"),
    ("overshoot", "overshoot"),
];

/// A scored algorithm: name, the 6 per-axis means, maximin, and mean.
struct Profile {
    name: String,
    axes: [f64; 6],
    maximin: f64,
    mean: f64,
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

    // Search space, centered on the maximin leader family. Lower τ and lower
    // CUSUM sensitivity push react−10% / convergence up; tighten and η_max
    // trade against jitter / overshoot.
    let taus = [60u64, 75, 90, 105, 120];
    let sensitivities = [1.0f64, 1.5, 2.0];
    let tightens = [2.0f64, 3.0];
    let transitions = [8u32, 10, 12];
    let eta_bases = [0.2f32, 0.3];
    let eta_maxes = [0.4f32, 0.6];
    let acceleration = 0.2f32;

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();

    // Reference points so the sweep table is anchored against known algos.
    algorithms.push(AlgorithmSpec::full_remedy());
    algorithms.push(AlgorithmSpec::classic_vardiff_state());
    algorithms.push(AlgorithmSpec::adaptive_boundary(10));

    for &tau in &taus {
        for &sens in &sensitivities {
            for &tighten in &tightens {
                for &trans in &transitions {
                    for &eb in &eta_bases {
                        for &em in &eta_maxes {
                            if em < eb {
                                continue; // η_max below η_base is degenerate
                            }
                            // Derive the name from the parts so it can't drift.
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

    // Scenarios required by EqualWeightFitness: ColdStart (convergence,
    // overshoot), Stable (jitter, step magnitude), Step −10% and −50%.
    let scenarios = vec![
        Scenario::ColdStart,
        Scenario::Stable,
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: -50 },
    ];

    let grid = Grid {
        algorithms,
        share_rates: vec![4.0, 6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 30.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Sweeping {} algorithms × {} cells × {} trials = {} total trials",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
    );
    let started = Instant::now();
    let results = grid.run();
    eprintln!("Sweep complete in {:.1}s", started.elapsed().as_secs_f64());

    // Score every algorithm: average each axis across share rates, then take
    // the min (maximin) and mean.
    let metric = EqualWeightFitness;
    let mut profiles: Vec<Profile> = Vec::new();
    for (name, cells) in &results {
        let computed = metric.compute(cells);
        let mut sums = [0.0f64; 6];
        let mut count = 0u32;
        for (_spm, mv) in &computed {
            for (i, (key, _)) in AXES.iter().enumerate() {
                sums[i] += mv.get(key).unwrap_or(0.0);
            }
            count += 1;
        }
        if count == 0 {
            continue;
        }
        let axes: [f64; 6] = std::array::from_fn(|i| sums[i] / count as f64);
        let maximin = axes.iter().cloned().fold(f64::INFINITY, f64::min);
        let mean = axes.iter().sum::<f64>() / 6.0;
        profiles.push(Profile {
            name: name.clone(),
            axes,
            maximin,
            mean,
        });
    }

    // Rank by maximin desc, then mean desc.
    profiles.sort_by(|a, b| {
        b.maximin
            .partial_cmp(&a.maximin)
            .unwrap()
            .then(b.mean.partial_cmp(&a.mean).unwrap())
    });

    // Best-in-class hull and per-axis leaders.
    let hull: [f64; 6] = std::array::from_fn(|i| {
        profiles.iter().map(|p| p.axes[i]).fold(0.0_f64, f64::max)
    });

    let mut md = String::new();
    md.push_str("# Balanced (maximin) vardiff sweep\n\n");
    md.push_str(&format!(
        "{} configs, {} trials/cell, seed `{:#x}`. Ranked by **min axis** \
         (maximin) — highest worst-axis first. All axes higher = better.\n\n",
        profiles.len(),
        trial_count,
        base_seed,
    ));
    md.push_str("| # | Algorithm | react-10% | react-50% | jitter | step-safe | conv | overshoot | **min** | mean |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for (rank, p) in profiles.iter().enumerate().take(30) {
        // Mark the worst axis with underscores so the gap is findable.
        let worst_i = p
            .axes
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let cell = |i: usize| {
            if i == worst_i {
                format!("_{:.3}_", p.axes[i])
            } else {
                format!("{:.3}", p.axes[i])
            }
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | **{:.3}** | {:.3} |\n",
            rank + 1,
            p.name,
            cell(0),
            cell(1),
            cell(2),
            cell(3),
            cell(4),
            cell(5),
            p.maximin,
            p.mean,
        ));
    }
    md.push_str(&format!(
        "| | **best-in-class** | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | | |\n",
        hull[0], hull[1], hull[2], hull[3], hull[4], hull[5],
    ));

    let path = out_dir.join("balanced_sweep.md");
    fs::create_dir_all(&out_dir)?;
    fs::write(&path, &md)?;

    // Echo the top 10 to stderr for immediate inspection.
    eprintln!("\nTop 10 by maximin:\n");
    eprintln!("| rank | min | mean | algorithm |");
    eprintln!("| --- | --- | --- | --- |");
    for (rank, p) in profiles.iter().enumerate().take(10) {
        eprintln!(
            "| {} | {:.3} | {:.3} | {} |",
            rank + 1,
            p.maximin,
            p.mean,
            p.name
        );
    }
    eprintln!("\nBest-in-class hull: {:?}", hull.map(|v| (v * 1000.0).round() / 1000.0));
    eprintln!("\nWrote {}", path.display());
    Ok(())
}
