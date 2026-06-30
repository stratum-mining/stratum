//! Maximin sweep for the VolatilityAdaptiveBoundary prototype.
//!
//! Hypothesis: a boundary that tightens when the share stream is calm and
//! loosens when it's genuinely volatile can beat the fixed-boundary react−10%
//! ↔ jitter tradeoff that capped the EWMA sweep at maximin ≈ 0.55.
//!
//! Sweeps alpha (EWMA memory of the volatility tracker) × vf_max (max
//! loosening) × z (PoissonCI floor) × EWMA τ, with the Accel update fixed at
//! the balanced-winner setting. Scored by maximin over the 6 equal-weight
//! axes; the EWMA balanced winner and production are included as references.
//!
//! ## Usage
//! ```text
//! VARDIFF_SWEEP_TRIALS=300 cargo run --release --bin sweep-voladapt
//! ```
//! Writes `voladapt_sweep.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};
use vardiff_sim::metrics::{DerivedMetric, EqualWeightFitness};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, Composed,
    EwmaEstimator, PoissonCI, VolatilityAdaptiveBoundary,
};

const AXES: &[(&str, &str)] = &[
    ("reaction_10", "react-10%"),
    ("reaction_50", "react-50%"),
    ("jitter", "jitter"),
    ("step_safety", "step-safe"),
    ("convergence", "conv"),
    ("overshoot", "overshoot"),
];

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

    let taus = [60u64, 90, 120];
    let alphas = [0.1f64, 0.2, 0.4];
    let vf_maxes = [2.0f64, 4.0, 8.0];
    let zs = [2.0f64, 2.576];
    let eta = (0.3f32, 0.6f32, 0.2f32);

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();

    // References.
    algorithms.push(AlgorithmSpec::classic_vardiff_state());
    {
        // EWMA balanced winner (maximin 0.551) for head-to-head.
        let name = vardiff_sim::naming::triple_name(
            &EwmaEstimator::new(90),
            &AdaptivePoissonCusum::with_params(
                PoissonCI::default_parametric(),
                AsymmetricCusumBoundary::new(1.0, 0.05, 3.0),
                8,
            ),
            &AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
        );
        algorithms.push(AlgorithmSpec::new(name, move |clock| {
            VardiffBox(Box::new(Composed::new(
                EwmaEstimator::new(90),
                AdaptivePoissonCusum::with_params(
                    PoissonCI::default_parametric(),
                    AsymmetricCusumBoundary::new(1.0, 0.05, 3.0),
                    8,
                ),
                AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
                1.0,
                clock,
            )))
        }));
    }

    for &tau in &taus {
        for &alpha in &alphas {
            for &vf_max in &vf_maxes {
                for &z in &zs {
                    let name = vardiff_sim::naming::triple_name(
                        &EwmaEstimator::new(tau),
                        &VolatilityAdaptiveBoundary::with_poisson(
                            PoissonCI::with_z(z, 0.05),
                            alpha,
                            vf_max,
                        ),
                        &AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
                    );
                    algorithms.push(AlgorithmSpec::new(name, move |clock| {
                        VardiffBox(Box::new(Composed::new(
                            EwmaEstimator::new(tau),
                            VolatilityAdaptiveBoundary::with_poisson(
                                PoissonCI::with_z(z, 0.05),
                                alpha,
                                vf_max,
                            ),
                            AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
                            1.0,
                            clock,
                        )))
                    }));
                }
            }
        }
    }

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
        "Sweeping {} algorithms × {} cells × {} trials",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
    );
    let started = Instant::now();
    let results = grid.run();
    eprintln!("Sweep complete in {:.1}s", started.elapsed().as_secs_f64());

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

    profiles.sort_by(|a, b| {
        b.maximin
            .partial_cmp(&a.maximin)
            .unwrap()
            .then(b.mean.partial_cmp(&a.mean).unwrap())
    });

    let hull: [f64; 6] = std::array::from_fn(|i| {
        profiles.iter().map(|p| p.axes[i]).fold(0.0_f64, f64::max)
    });

    let mut md = String::new();
    md.push_str("# VolatilityAdaptiveBoundary (maximin) sweep\n\n");
    md.push_str(&format!(
        "{} configs, {} trials/cell, seed `{:#x}`. Ranked by **min axis** \
         (maximin). All axes higher = better.\n\n",
        profiles.len(),
        trial_count,
        base_seed,
    ));
    md.push_str("| # | Algorithm | react-10% | react-50% | jitter | step-safe | conv | overshoot | **min** | mean |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for (rank, p) in profiles.iter().enumerate() {
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

    let path = out_dir.join("voladapt_sweep.md");
    fs::create_dir_all(&out_dir)?;
    fs::write(&path, &md)?;

    eprintln!("\nTop 10 by maximin:\n");
    eprintln!("| rank | min | mean | react-10% | algorithm |");
    eprintln!("| --- | --- | --- | --- | --- |");
    for (rank, p) in profiles.iter().enumerate().take(10) {
        eprintln!(
            "| {} | {:.3} | {:.3} | {:.3} | {} |",
            rank + 1,
            p.maximin,
            p.mean,
            p.axes[0],
            p.name
        );
    }
    eprintln!(
        "\nreact-10% best-in-class: {:.3} (EWMA/Kalman wall was 0.54)",
        hull[0]
    );
    eprintln!("Wrote {}", path.display());
    Ok(())
}
