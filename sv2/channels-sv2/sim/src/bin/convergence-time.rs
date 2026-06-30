//! Measures convergence TIME (in seconds) for BestOfBest vs production.

use std::env;
use vardiff_sim::{baseline::Scenario, grid::AlgorithmSpec, grid::Grid};

fn main() {
    let trial_count: usize = env::var("TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }

    let grid = Grid {
        algorithms: vec![
            AlgorithmSpec::classic_vardiff_state(),
            AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 3.0, 0.2),
            AlgorithmSpec::best_of_best(),
        ],
        share_rates: vec![6.0, 10.0, 12.0, 20.0, 30.0],
        scenarios,
        trial_count,
        base_seed: 0xDEAD_BEEF_CAFE_F00D,
    };

    eprintln!("Running {} total trials...", grid.total_runs() * trial_count);
    let results = grid.run_paired();

    println!("# Convergence Time (seconds)\n");
    println!("## Cold Start (p50 / p90)\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    let mut algo_names: Vec<&String> = results.keys().collect();
    algo_names.sort();

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);
        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let cell = cells.iter().find(|c| {
                (c.shares_per_minute - spm).abs() < 0.1 && c.scenario == Scenario::ColdStart
            });
            if let Some(c) = cell {
                let p50 = c.get("convergence_p50_secs").unwrap_or(f64::NAN);
                let p90 = c.get("convergence_p90_secs").unwrap_or(f64::NAN);
                row.push_str(&format!("| {:.0}/{:.0} ", p50, p90));
            } else {
                row.push_str("| — ");
            }
        }
        row.push('|');
        println!("{}", row);
    }

    println!("\n## Step -50% (p50 / p90)\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);
        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let cell = cells.iter().find(|c| {
                (c.shares_per_minute - spm).abs() < 0.1
                    && c.scenario == Scenario::Step { delta_pct: -50 }
            });
            if let Some(c) = cell {
                let p50 = c.get("convergence_p50_secs").unwrap_or(f64::NAN);
                let p90 = c.get("convergence_p90_secs").unwrap_or(f64::NAN);
                row.push_str(&format!("| {:.0}/{:.0} ", p50, p90));
            } else {
                row.push_str("| — ");
            }
        }
        row.push('|');
        println!("{}", row);
    }

    println!("\n## Step +50% (p50 / p90)\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);
        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let cell = cells.iter().find(|c| {
                (c.shares_per_minute - spm).abs() < 0.1
                    && c.scenario == Scenario::Step { delta_pct: 50 }
            });
            if let Some(c) = cell {
                let p50 = c.get("convergence_p50_secs").unwrap_or(f64::NAN);
                let p90 = c.get("convergence_p90_secs").unwrap_or(f64::NAN);
                row.push_str(&format!("| {:.0}/{:.0} ", p50, p90));
            } else {
                row.push_str("| — ");
            }
        }
        row.push('|');
        println!("{}", row);
    }

    println!("\n## Step ±10% (p50 / p90)\n");
    println!("| Algorithm | SPM=6 | SPM=10 | SPM=12 | SPM=20 | SPM=30 |");
    println!("|-----------|-------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);
        for &spm in &[6.0f32, 10.0, 12.0, 20.0, 30.0] {
            let cell = cells.iter().find(|c| {
                (c.shares_per_minute - spm).abs() < 0.1
                    && matches!(c.scenario, Scenario::Step { delta_pct: -10 } | Scenario::Step { delta_pct: 10 })
            });
            if let Some(c) = cell {
                let p50 = c.get("convergence_p50_secs").unwrap_or(f64::NAN);
                let p90 = c.get("convergence_p90_secs").unwrap_or(f64::NAN);
                row.push_str(&format!("| {:.0}/{:.0} ", p50, p90));
            } else {
                row.push_str("| — ");
            }
        }
        row.push('|');
        println!("{}", row);
    }
}
