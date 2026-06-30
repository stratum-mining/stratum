//! High-trial confirmation run for the big-sweep champion region.
//!
//! `sweep-regret-big` (9216 algos, 500 trials) returned a top-100 that was
//! statistically FLAT — rank 1 (0.2000) to rank 100 (0.2057) is a ~3%
//! spread, almost certainly inside the 500-trial noise band. So "rank 1"
//! is not meaningfully the winner; the top ~30 are a tie. This bin
//! resolves that tie by re-running a SMALL, curated set at high trial
//! count, where the noise band shrinks below the inter-config gap.
//!
//! The set is:
//!   1. The distinct top-20 configs from the big sweep (champion cluster).
//!   2. Edge-extension probes on the two axes that were STILL pegged at a
//!      box edge in the big sweep — sensitivity (pegged low at 0.3) and
//!      tighten (pegged high at 7). If a probe beats the cluster, the real
//!      optimum is still outside the box and another widening is needed;
//!      if the probes are no better, the edge-lean was noise and the
//!      cluster is the true optimum.
//!   3. The three reference anchors (production-classic + current champs).
//!
//! Cost is IDENTICAL to sweep-regret / sweep-regret-big.
//!
//! Usage:
//! ```text
//! cargo run --release --bin confirm-champions                  # 2000 trials
//! VARDIFF_CONFIRM_TRIALS=4000 cargo run --release --bin confirm-champions
//! ```
//! Env: VARDIFF_CONFIRM_TRIALS (default 2000), VARDIFF_CONFIRM_THREADS,
//!      VARDIFF_SWEEP_SEED, VARDIFF_W_*/RHO_* weights,
//!      VARDIFF_CONFIRM_OUT_DIR (default "."), writes `confirm_champions.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, Composed,
    EwmaEstimator, PoissonCI,
};
use vardiff_sim::baseline::{Cell, Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{run_cell_with_algorithm, AlgorithmSpec, VardiffBox};

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

struct Profile {
    name: String,
    tag: &'static str,
    regret_over: f64,
    regret_under: f64,
    effort_up: f64,
    effort_down: f64,
    detection: f64,
    cost: f64,
}

#[derive(Clone, Copy)]
struct Params {
    tau: u64,
    sens: f64,
    floor: f64,
    tighten: f64,
    spm: u32,
    eta_base: f32,
    eta_max: f32,
    accel: f32,
}

fn spec_for(p: Params) -> AlgorithmSpec {
    let name = vardiff_sim::naming::triple_name(
        &EwmaEstimator::new(p.tau),
        &AdaptivePoissonCusum::with_params(
            PoissonCI::default_parametric(),
            AsymmetricCusumBoundary::new(p.sens, p.floor, p.tighten),
            p.spm,
        ),
        &AcceleratingPartialRetarget::new(p.eta_base, p.eta_max, p.accel),
    );
    AlgorithmSpec::new(name, move |clock| {
        let boundary = AdaptivePoissonCusum::with_params(
            PoissonCI::default_parametric(),
            AsymmetricCusumBoundary::new(p.sens, p.floor, p.tighten),
            p.spm,
        );
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(p.tau),
            boundary,
            AcceleratingPartialRetarget::new(p.eta_base, p.eta_max, p.accel),
            1.0,
            clock,
        )))
    })
}

/// Shorthand for a champion-cluster point (all share tau=150, floor=0.05,
/// eta_base=0.2; only spm/sens/tighten/eta_max/accel vary).
fn champ(spm: u32, sens: f64, tighten: f64, eta_max: f32, accel: f32) -> Params {
    Params {
        tau: 150,
        sens,
        floor: 0.05,
        tighten,
        spm,
        eta_base: 0.2,
        eta_max,
        accel,
    }
}

#[allow(clippy::too_many_arguments)]
fn profile_for(
    algo: &AlgorithmSpec,
    algo_idx: usize,
    tag: &'static str,
    share_rates: &[f32],
    scenarios: &[Scenario],
    trial_count: usize,
    base_seed: u64,
    w: &Weights,
) -> Option<Profile> {
    let n_spm = share_rates.len();
    let n_scen = scenarios.len();
    let (mut ro, mut ru, mut eu, mut ed) = (0.0, 0.0, 0.0, 0.0);
    let mut n = 0u32;
    let mut det_sum = 0.0;
    let mut det_n = 0u32;
    for (spm_idx, &spm) in share_rates.iter().enumerate() {
        for (scen_idx, scen) in scenarios.iter().enumerate() {
            let cell = Cell {
                shares_per_minute: spm,
                scenario: scen.clone(),
            };
            let cell_index = (algo_idx * n_spm * n_scen + spm_idx * n_scen + scen_idx) as u64;
            let r = run_cell_with_algorithm(algo, &cell, trial_count, base_seed, cell_index);
            let is_mix = matches!(
                scen,
                Scenario::Stable
                    | Scenario::Step {
                        delta_pct: -50 | -10 | 10 | 50
                    }
            );
            if is_mix {
                ro += r.get("regret_over").unwrap_or(0.0);
                ru += r.get("regret_under").unwrap_or(0.0);
                eu += r.get("effort_up").unwrap_or(0.0);
                ed += r.get("effort_down").unwrap_or(0.0);
                n += 1;
            }
            if matches!(
                scen,
                Scenario::SettledStep {
                    settle_minutes: 60,
                    delta_pct: -10
                }
            ) {
                if let Some(rate) = r.get("settled_reaction_rate") {
                    det_sum += rate;
                    det_n += 1;
                }
            }
        }
    }
    if n == 0 {
        return None;
    }
    let (regret_over, regret_under, effort_up, effort_down) =
        (ro / n as f64, ru / n as f64, eu / n as f64, ed / n as f64);
    let detection = if det_n > 0 { det_sum / det_n as f64 } else { 0.0 };
    let cost = w.w_over * regret_over
        + w.w_under * regret_under
        + w.rho * (w.rho_up * effort_up + w.rho_down * effort_down)
        + w.w_det * (1.0 - detection);
    Some(Profile {
        name: algo.name.clone(),
        tag,
        regret_over,
        regret_under,
        effort_up,
        effort_down,
        detection,
        cost,
    })
}

fn main() -> std::io::Result<()> {
    let trial_count: usize = env::var("VARDIFF_CONFIRM_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| {
            s.strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .or_else(|| s.parse().ok())
        })
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_CONFIRM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
        .max(1);
    let out_dir = env::var("VARDIFF_CONFIRM_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let w = Weights::from_env();

    // ---- The curated set. (spec, tag) — tag marks provenance in the table.
    let mut entries: Vec<(AlgorithmSpec, &'static str)> = Vec::new();

    // Anchors first (stable seed offsets).
    entries.push((AlgorithmSpec::classic_vardiff_state(), "anchor"));
    entries.push((AlgorithmSpec::balanced(), "anchor"));
    entries.push((AlgorithmSpec::react_priority(), "anchor"));

    // (1) Distinct top-20 champion-cluster configs (tau=150, floor=0.05,
    //     eta_base=0.2). (spm, sens, tighten, eta_max, accel).
    let cluster = [
        champ(5, 0.3, 7.0, 0.8, 0.05),
        champ(3, 0.3, 7.0, 0.6, 0.05),
        champ(3, 0.3, 7.0, 0.8, 0.05),
        champ(6, 0.3, 7.0, 0.6, 0.05),
        champ(6, 0.3, 7.0, 0.8, 0.05),
        champ(5, 0.3, 7.0, 0.6, 0.05),
        champ(6, 0.3, 7.0, 0.8, 0.1),
        champ(5, 0.3, 6.0, 0.8, 0.05),
        champ(4, 0.3, 7.0, 0.6, 0.05),
        champ(5, 0.3, 7.0, 0.6, 0.1),
        champ(6, 0.5, 6.0, 0.6, 0.1),
        champ(6, 0.3, 6.0, 0.8, 0.1),
        champ(4, 0.3, 7.0, 0.6, 0.1),
        champ(3, 0.3, 7.0, 0.6, 0.1),
        champ(5, 0.5, 6.0, 0.6, 0.1),
        champ(4, 0.3, 7.0, 0.8, 0.1),
        champ(3, 0.5, 7.0, 0.8, 0.1),
        champ(4, 0.5, 7.0, 0.8, 0.1),
        champ(6, 0.3, 6.0, 0.6, 0.05),
        champ(6, 0.3, 7.0, 0.6, 0.1),
    ];
    for p in cluster {
        entries.push((spec_for(p), "cluster"));
    }

    // (2) Edge-extension probes. Sensitivity pegged low (0.3) and tighten
    //     pegged high (7) in the big sweep — push both past the box on a
    //     representative champion skeleton (spm5, eta_max0.8, accel0.05).
    let probes = [
        // sensitivity below the box floor of 0.3
        champ(5, 0.2, 7.0, 0.8, 0.05),
        champ(5, 0.15, 7.0, 0.8, 0.05),
        champ(3, 0.2, 7.0, 0.6, 0.05),
        // tighten above the box ceiling of 7
        champ(5, 0.3, 8.0, 0.8, 0.05),
        champ(5, 0.3, 9.0, 0.8, 0.05),
        champ(3, 0.3, 8.0, 0.6, 0.05),
        // both edges pushed at once
        champ(5, 0.2, 8.0, 0.8, 0.05),
        champ(5, 0.15, 9.0, 0.8, 0.05),
    ];
    for p in probes {
        entries.push((spec_for(p), "probe"));
    }

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

    let n = entries.len();
    eprintln!(
        "Confirmation: {} algos × {} cells × {} trials = {} runs, {} threads",
        n,
        share_rates.len() * scenarios.len(),
        trial_count,
        n * share_rates.len() * scenarios.len() * trial_count,
        n_threads
    );

    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Profile>> = Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let (spec, tag) = &entries[i];
                if let Some(p) = profile_for(
                    spec,
                    i,
                    tag,
                    &share_rates,
                    &scenarios,
                    trial_count,
                    base_seed,
                    &w,
                ) {
                    out.lock().unwrap().push(p);
                }
            });
        }
    });
    let mut profiles = out.into_inner().unwrap();
    eprintln!("Done in {:.1}s", started.elapsed().as_secs_f64());
    profiles.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());

    let row = |i: usize, p: &Profile| {
        format!(
            "| {} | {} | {} | **{:.4}** | {:.4} | {:.4} | {:.4} | {:.4} | {:.0}% |\n",
            i + 1,
            p.tag,
            p.name,
            p.cost,
            p.regret_over,
            p.regret_under,
            p.effort_up,
            p.effort_down,
            p.detection * 100.0,
        )
    };

    let mut md = String::new();
    md.push_str("# Champion confirmation run\n\n");
    md.push_str(&format!(
        "Cost = {:.1}·regret_over + {:.1}·regret_under + {:.2}·({:.1}·effort_up + {:.1}·effort_down) + {:.2}·(1−detection). Lower is better.\n\n",
        w.w_over, w.w_under, w.rho, w.rho_up, w.rho_down, w.w_det
    ));
    md.push_str(&format!(
        "{} algos, {} trials/cell (high-trial tie-break), base_seed {:#x}.\n\n",
        n, trial_count, base_seed
    ));
    md.push_str("Tags: `cluster` = big-sweep top-20, `probe` = edge-extension (sens<0.3 or tighten>7), `anchor` = reference.\n");
    md.push_str("**Verdict rule:** if no `probe` ranks above the `cluster`, the box edge-lean was noise and the cluster is the optimum; if a `probe` wins, widen the box again.\n\n");
    md.push_str("| rank | tag | algorithm | **cost** | reg_over | reg_under | eff_up | eff_down | det% |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for (i, p) in profiles.iter().enumerate() {
        md.push_str(&row(i, p));
    }

    let out_path = out_dir.join("confirm_champions.md");
    fs::write(&out_path, &md)?;
    eprintln!("Wrote {}", out_path.display());

    println!("\n## Champion confirmation — full ranking ({} trials)\n", trial_count);
    println!("| rank | tag | algorithm | cost | reg_over | reg_under | eff_up | eff_down | det% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for (i, p) in profiles.iter().enumerate() {
        print!("{}", row(i, p));
    }
    let best_probe = profiles.iter().position(|p| p.tag == "probe");
    let best_cluster = profiles.iter().position(|p| p.tag == "cluster");
    println!(
        "\nBest cluster at rank {:?}, best probe at rank {:?} — {}",
        best_cluster.map(|i| i + 1),
        best_probe.map(|i| i + 1),
        match (best_cluster, best_probe) {
            (Some(c), Some(p)) if p < c => "PROBE WINS → widen the box again",
            (Some(_), _) => "cluster holds → edge-lean was noise, cluster is the optimum",
            _ => "inconclusive",
        }
    );
    Ok(())
}
