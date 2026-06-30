//! Parameter sweep for AsymmetricPoissonCI.
//!
//! Sweeps: tighten_multiplier for the PoissonCI boundary at low SPM.
//! Tests whether asymmetric cost awareness improves behavior at SPM 4-10
//! where PoissonCI is the active boundary.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};
use vardiff_sim::metrics;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AsymmetricCusumBoundary, AsymmetricPoissonCI, Boundary, Composed,
    EstimatorSnapshot, EwmaEstimator, PoissonCI,
};

/// Adaptive boundary using AsymmetricPoissonCI at low SPM, AsymmetricCUSUM at high SPM.
#[derive(Debug, Clone)]
struct AdaptiveAsymPoissonCusum {
    poisson: AsymmetricPoissonCI,
    cusum: AsymmetricCusumBoundary,
    spm_threshold: u32,
}

impl Boundary for AdaptiveAsymPoissonCusum {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32, snap: &EstimatorSnapshot) -> f64 {
        if (shares_per_minute as u32) < self.spm_threshold {
            self.poisson.threshold(dt_secs, shares_per_minute, snap)
        } else {
            self.cusum.threshold(dt_secs, shares_per_minute, snap)
        }
    }

    fn code(&self) -> String {
        format!(
            "AdaptAsymPC-spm{}[{}|{}]",
            self.spm_threshold,
            self.poisson.code(),
            self.cusum.code()
        )
    }
}

fn main() -> std::io::Result<()> {
    let trial_count: usize = env::var("VARDIFF_SWEEP_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
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

    let tighten_multipliers = vec![1.0, 1.25, 1.5, 2.0, 2.5, 3.0];
    let eta_base = 0.2f32;
    let eta_max = 0.4f32;
    let acceleration = 0.2f32;
    let spm_threshold = 10u32;

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();

    // Reference: current production (symmetric PoissonCI at low SPM)
    algorithms.push(AlgorithmSpec::adaptive_boundary(spm_threshold));
    algorithms.push(AlgorithmSpec::full_remedy());

    // Sweep: AsymmetricPoissonCI with varying tighten multipliers
    for &tighten in &tighten_multipliers {
        let name = format!("AsymPoisson-t{}", (tighten * 100.0) as u32);
        algorithms.push(AlgorithmSpec::new(name, move |clock| {
            let boundary = AdaptiveAsymPoissonCusum {
                poisson: AsymmetricPoissonCI::new(2.576, 0.05, tighten),
                cusum: AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
                spm_threshold,
            };
            let inner = Composed::new(
                EwmaEstimator::new(120),
                boundary,
                AcceleratingPartialRetarget::new(eta_base, eta_max, acceleration),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        }));
    }

    // Focus on low SPM where PoissonCI is active
    let share_rates = vec![4.0, 6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 30.0];

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }
    for &d in &[-50i32, -25, 25, 50] {
        scenarios.push(Scenario::SettledStep {
            settle_minutes: 60,
            delta_pct: d,
        });
    }

    let grid = Grid {
        algorithms,
        share_rates,
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "AsymmetricPoissonCI sweep: {} algorithms × {} cells × {} trials = {} total",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    let elapsed = started.elapsed();
    eprintln!("Sweep complete in {:.1}s", elapsed.as_secs_f64());

    fs::create_dir_all(&out_dir)?;

    // Compute fitness metrics
    let derived_registry = metrics::derived_registry();
    let comp_fitness = derived_registry
        .iter()
        .find(|d| d.id() == "comprehensive_fitness")
        .expect("ComprehensiveFitness not in registry");
    let op_fitness = derived_registry
        .iter()
        .find(|d| d.id() == "operational_fitness")
        .expect("OperationalFitness not in registry");

    // For each algorithm, compute fitness per SPM
    let mut algo_fitness: Vec<(String, Vec<(u32, f64)>, Vec<(u32, f64)>)> = Vec::new();
    for (name, cells) in &results {
        let comp = comp_fitness.compute(cells);
        let ops = op_fitness.compute(cells);
        let mut per_spm_comp: Vec<(u32, f64)> = Vec::new();
        let mut per_spm_ops: Vec<(u32, f64)> = Vec::new();
        for (spm, mv) in &comp {
            if let Some(v) = mv.get("score") {
                per_spm_comp.push((*spm as u32, v));
            }
        }
        for (spm, mv) in &ops {
            if let Some(v) = mv.get("score") {
                per_spm_ops.push((*spm as u32, v));
            }
        }
        per_spm_comp.sort_by_key(|(spm, _)| *spm);
        per_spm_ops.sort_by_key(|(spm, _)| *spm);
        algo_fitness.push((name.clone(), per_spm_comp, per_spm_ops));
    }

    // Sort by mean comprehensive fitness
    algo_fitness.sort_by(|a, b| {
        let avg_a: f64 = if a.1.is_empty() {
            0.0
        } else {
            a.1.iter().map(|(_, v)| v).sum::<f64>() / a.1.len() as f64
        };
        let avg_b: f64 = if b.1.is_empty() {
            0.0
        } else {
            b.1.iter().map(|(_, v)| v).sum::<f64>() / b.1.len() as f64
        };
        avg_b.partial_cmp(&avg_a).unwrap()
    });

    // Print report
    let share_rates_u32: Vec<u32> = vec![4, 6, 8, 10, 12, 15, 20, 30];
    let mut report = String::new();
    report.push_str("# AsymmetricPoissonCI Sweep Results\n\n");
    report.push_str("## Comprehensive Fitness\n\n");
    report.push_str("| Algorithm | Avg |");
    for spm in &share_rates_u32 {
        report.push_str(&format!(" {} |", spm));
    }
    report.push_str("\n| --- | --- |");
    for _ in &share_rates_u32 {
        report.push_str(" --- |");
    }
    report.push('\n');

    for (name, per_spm_comp, _) in &algo_fitness {
        let avg: f64 = if per_spm_comp.is_empty() {
            0.0
        } else {
            per_spm_comp.iter().map(|(_, v)| v).sum::<f64>() / per_spm_comp.len() as f64
        };
        report.push_str(&format!("| {} | {:.3} |", name, avg));
        for &target_spm in &share_rates_u32 {
            match per_spm_comp.iter().find(|(s, _)| *s == target_spm) {
                Some((_, v)) => report.push_str(&format!(" {:.3} |", v)),
                None => report.push_str(" - |"),
            }
        }
        report.push('\n');
    }

    report.push_str("\n## Operational Fitness\n\n");
    report.push_str("| Algorithm | Avg |");
    for spm in &share_rates_u32 {
        report.push_str(&format!(" {} |", spm));
    }
    report.push_str("\n| --- | --- |");
    for _ in &share_rates_u32 {
        report.push_str(" --- |");
    }
    report.push('\n');

    for (name, _, per_spm_ops) in &algo_fitness {
        let avg: f64 = if per_spm_ops.is_empty() {
            0.0
        } else {
            per_spm_ops.iter().map(|(_, v)| v).sum::<f64>() / per_spm_ops.len() as f64
        };
        report.push_str(&format!("| {} | {:.3} |", name, avg));
        for &target_spm in &share_rates_u32 {
            match per_spm_ops.iter().find(|(s, _)| *s == target_spm) {
                Some((_, v)) => report.push_str(&format!(" {:.3} |", v)),
                None => report.push_str(" - |"),
            }
        }
        report.push('\n');
    }

    let out_path = out_dir.join("asymmetric_poisson_sweep.md");
    fs::write(&out_path, &report)?;
    eprintln!("Report written to {}", out_path.display());
    print!("{}", report);

    Ok(())
}
