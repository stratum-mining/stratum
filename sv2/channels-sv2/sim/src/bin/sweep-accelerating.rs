//! Parameter sweep for AcceleratingPartialRetarget.
//!
//! Sweeps (acceleration, eta_max) over the standard scenario grid to find
//! the optimal tradeoff between convergence speed and jitter/stability.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-accelerating
//! ```

use std::env;

use vardiff_sim::{baseline::Scenario, grid::AlgorithmSpec, grid::Grid};

fn main() {
    let trial_count: usize = env::var("VARDIFF_SWEEP_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    let base_seed: u64 = 0xDEAD_BEEF_CAFE_F00D;

    let accelerations: &[f32] = &[0.05, 0.1, 0.15, 0.2];
    let eta_maxes: &[f32] = &[0.4, 0.5, 0.6, 0.8];

    let mut algorithms = vec![
        AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 3.0, 0.2),
    ];

    for &acc in accelerations {
        for &eta_max in eta_maxes {
            algorithms.push(AlgorithmSpec::ada_cusum_accelerating(
                120, 1.5, 0.05, 3.0, 0.2, eta_max, acc,
            ));
        }
    }

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }

    let grid = Grid {
        algorithms,
        share_rates: vec![6.0, 10.0, 12.0, 20.0, 30.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Sweeping {} algorithms × {} SPM × {} scenarios × {} trials",
        grid.algorithms.len(),
        grid.share_rates.len(),
        grid.scenarios.len(),
        grid.trial_count,
    );

    let results = grid.run_paired();

    // Print header
    println!("# AcceleratingPartialRetarget Parameter Sweep\n");
    println!("Base: EwmaEstimator(120s) + AsymmetricCusumBoundary(s=1.5, floor=0.05, tighten=3.0)");
    println!("Sweep: eta_base=0.2, acceleration ∈ {{0.05, 0.1, 0.15, 0.2}}, eta_max ∈ {{0.4, 0.5, 0.6, 0.8}}");
    println!("Trials: {}\n", trial_count);

    // Aggregate metrics across share rates for each algorithm
    println!("## Summary (averaged across SPM=6,10,12,20,30)\n");
    println!("| Algorithm | Stable Jitter | Step±50 React | Step±50 Conv | Step±10 React | Step±10 Conv | Step±5 React | ColdStart Conv |");
    println!("|-----------|---------------|---------------|--------------|---------------|--------------|--------------|----------------|");

    let mut algo_names: Vec<&String> = results.keys().collect();
    algo_names.sort();

    for algo_name in &algo_names {
        let cells = &results[*algo_name];

        let mut stable_jitter = Vec::new();
        let mut step50_react = Vec::new();
        let mut step50_conv = Vec::new();
        let mut step10_react = Vec::new();
        let mut step10_conv = Vec::new();
        let mut step5_react = Vec::new();
        let mut cold_conv = Vec::new();

        for cell in cells {
            match &cell.scenario {
                Scenario::Stable => {
                    if let Some(v) = cell.get("jitter_mean_per_min") {
                        stable_jitter.push(v);
                    }
                }
                Scenario::Step { delta_pct: 50 } | Scenario::Step { delta_pct: -50 } => {
                    if let Some(v) = cell.get("reaction_rate") {
                        step50_react.push(v);
                    }
                    step50_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                Scenario::Step { delta_pct: 10 } | Scenario::Step { delta_pct: -10 } => {
                    if let Some(v) = cell.get("reaction_rate") {
                        step10_react.push(v);
                    }
                    step10_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                Scenario::Step { delta_pct: 5 } | Scenario::Step { delta_pct: -5 } => {
                    if let Some(v) = cell.get("reaction_rate") {
                        step5_react.push(v);
                    }
                }
                Scenario::ColdStart => {
                    cold_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                _ => {}
            }
        }

        let mean = |v: &[f64]| -> f64 {
            if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
        };

        println!(
            "| {} | {:.4} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            algo_name,
            mean(&stable_jitter),
            mean(&step50_react),
            mean(&step50_conv),
            mean(&step10_react),
            mean(&step10_conv),
            mean(&step5_react),
            mean(&cold_conv),
        );
    }

    // Detailed per-SPM table for the step±50 convergence (the key metric)
    println!("\n## Step ±50% Convergence Rate by SPM\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut by_spm: Vec<f64> = Vec::new();

        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let conv_values: Vec<f64> = cells
                .iter()
                .filter(|c| {
                    (c.shares_per_minute - spm).abs() < 0.1
                        && matches!(
                            c.scenario,
                            Scenario::Step { delta_pct: 50 } | Scenario::Step { delta_pct: -50 }
                        )
                })
                .map(|c| c.get("convergence_rate").unwrap_or(0.0))
                .collect();
            let avg = if conv_values.is_empty() {
                f64::NAN
            } else {
                conv_values.iter().sum::<f64>() / conv_values.len() as f64
            };
            by_spm.push(avg);
        }

        println!(
            "| {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            algo_name, by_spm[0], by_spm[1], by_spm[2], by_spm[3], by_spm[4],
        );
    }

    // Jitter detail
    println!("\n## Stable-Load Jitter by SPM (fires/min)\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut by_spm: Vec<f64> = Vec::new();

        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let jitter: Vec<f64> = cells
                .iter()
                .filter(|c| {
                    (c.shares_per_minute - spm).abs() < 0.1
                        && matches!(c.scenario, Scenario::Stable)
                })
                .filter_map(|c| c.get("jitter_mean_per_min"))
                .collect();
            let avg = if jitter.is_empty() {
                f64::NAN
            } else {
                jitter.iter().sum::<f64>() / jitter.len() as f64
            };
            by_spm.push(avg);
        }

        println!(
            "| {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} |",
            algo_name, by_spm[0], by_spm[1], by_spm[2], by_spm[3], by_spm[4],
        );
    }
}
