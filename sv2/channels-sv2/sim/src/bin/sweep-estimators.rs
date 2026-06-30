//! Maximin-scored sweep over ESTIMATOR FAMILIES to attack the react−10% wall.
//!
//! The balanced sweep (sweep-balanced) topped out at maximin ≈ 0.55 with
//! react−10% as the binding axis, using EWMA estimators. react−10% is hard
//! because detecting a small mean shift against Poisson noise needs either
//! more data (slower) or a looser boundary (more jitter). Uncertainty-aware
//! estimators (Kalman, Bayesian) may break that tradeoff: they can react
//! fast when confident and hold steady when not.
//!
//! This sweep pairs Kalman(q) and Bayesian(discount, prior) estimators with
//! the boundary/update that won the EWMA sweep (AdaptPC-spm8 with a sensitive
//! CUSUM + Accel update), and scores by maximin over the 6 equal-weight axes.
//!
//! ## Usage
//! ```text
//! cargo run --release --bin sweep-estimators
//! VARDIFF_SWEEP_TRIALS=300 cargo run --release --bin sweep-estimators
//! ```
//! Writes `estimator_sweep.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};
use vardiff_sim::metrics::{DerivedMetric, EqualWeightFitness};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, BayesianEstimator,
    Composed, EwmaEstimator, KalmanEstimator, PoissonCI,
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

    // Winning boundary/update family from sweep-balanced: AdaptPC-spm8 with a
    // sensitive CUSUM (s1, t3) and Accel(0.3, 0.6, 0.2). We vary the ESTIMATOR
    // while holding these, plus a couple of boundary sensitivities since the
    // estimator's noise profile may shift the optimal sensitivity.
    let spm_threshold = 8u32;
    let sensitivities = [1.0f64, 1.5];
    let tighten = 3.0f64;
    let eta = (0.3f32, 0.6f32, 0.2f32);

    // Kalman process-noise q: 0.0002 (≈τ300s, smooth) → 0.01 (fast).
    let kalman_qs = [0.0005f64, 0.001, 0.002, 0.005, 0.01];
    // Bayesian discount (forgetting) × prior strength.
    let bayes_discounts = [0.90f64, 0.95, 0.98];
    let bayes_priors = [2.0f64, 5.0];

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();

    // References: production + the EWMA balanced winner.
    algorithms.push(AlgorithmSpec::classic_vardiff_state());
    algorithms.push(AlgorithmSpec::full_remedy());
    {
        // The sweep-balanced winner, rebuilt here for head-to-head.
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

    // Kalman family.
    for &q in &kalman_qs {
        for &sens in &sensitivities {
            let name = vardiff_sim::naming::triple_name(
                &KalmanEstimator::new(q),
                &AdaptivePoissonCusum::with_params(
                    PoissonCI::default_parametric(),
                    AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                    spm_threshold,
                ),
                &AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
            );
            algorithms.push(AlgorithmSpec::new(name, move |clock| {
                VardiffBox(Box::new(Composed::new(
                    KalmanEstimator::new(q),
                    AdaptivePoissonCusum::with_params(
                        PoissonCI::default_parametric(),
                        AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                        spm_threshold,
                    ),
                    AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
                    1.0,
                    clock,
                )))
            }));
        }
    }

    // Bayesian family.
    for &disc in &bayes_discounts {
        for &prior in &bayes_priors {
            for &sens in &sensitivities {
                let name = vardiff_sim::naming::triple_name(
                    &BayesianEstimator::new(disc, prior),
                    &AdaptivePoissonCusum::with_params(
                        PoissonCI::default_parametric(),
                        AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                        spm_threshold,
                    ),
                    &AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
                );
                algorithms.push(AlgorithmSpec::new(name, move |clock| {
                    VardiffBox(Box::new(Composed::new(
                        BayesianEstimator::new(disc, prior),
                        AdaptivePoissonCusum::with_params(
                            PoissonCI::default_parametric(),
                            AsymmetricCusumBoundary::new(sens, 0.05, tighten),
                            spm_threshold,
                        ),
                        AcceleratingPartialRetarget::new(eta.0, eta.1, eta.2),
                        1.0,
                        clock,
                    )))
                }));
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
    md.push_str("# Estimator-family (maximin) sweep\n\n");
    md.push_str(&format!(
        "{} configs, {} trials/cell, seed `{:#x}`. Kalman & Bayesian \
         estimators vs the EWMA balanced winner. Ranked by **min axis** \
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

    let path = out_dir.join("estimator_sweep.md");
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
        "\nreact-10% best-in-class: {:.3} (EWMA sweep wall was 0.674)",
        hull[0]
    );
    eprintln!("Wrote {}", path.display());
    Ok(())
}
