//! Big regret/effort-scored parameter scan — the multi-hour, multi-core
//! successor to `sweep-regret`, built to hunt the new Pareto champions
//! after the §10 metric overhaul.
//!
//! ## Why this exists
//!
//! The first `sweep-regret` (363 algos) returned a winner that pegged the
//! search-box edge on FIVE of six axes:
//!   `Ewma120s / Adapt-spm8[AsymCusum-s1-f0.05-t3] / Accel-0.2-0.6-0.2`
//!   - tau      = 120  (box MAX)
//!   - sens     = 1.0  (box MIN)
//!   - tighten  = 3.0  (box MAX)
//!   - spm      = 8    (box MIN)
//!   - eta_max  = 0.6  (box MAX)
//! That is the textbook signature of an optimum lying OUTSIDE the box. So
//! this scan (a) pushes every pegged frontier outward, and (b) unfreezes
//! the two params the first sweep held constant — the CUSUM drift fraction
//! (`floor`, was 0.05) and the retarget `acceleration` (was 0.2).
//!
//! Cost is IDENTICAL to `sweep-regret` so the two tables are directly
//! comparable:
//! ```text
//! cost = w_over·regret_over + w_under·regret_under
//!      + rho·(rho_up·effort_up + rho_down·effort_down)
//!      + w_det·(1 − detection)
//! ```
//! Defaults: w_over:w_under = 3:1, rho_up:rho_down = 3:1, rho = 0.5,
//! w_det = 0.5 (the §10 values; all env-overridable).
//!
//! ## Parallelism
//!
//! `grid.run()` is serial; this bin instead fans the algorithm list across
//! a `std::thread::scope` worker pool (no new deps — `baseline.rs` flagged
//! rayon as merely a future optimization). Each worker pulls the next
//! algorithm index from a shared atomic and runs all of its cells via the
//! same `run_cell_with_algorithm` the serial grid uses, with the SAME
//! `cell_index` seed derivation — so a parallel run is bit-identical to the
//! serial grid it replaces, only faster.
//!
//! ## Usage
//! ```text
//! cargo run --release --bin sweep-regret-big
//! VARDIFF_BIG_TRIALS=1000 cargo run --release --bin sweep-regret-big
//! VARDIFF_BIG_THREADS=12 VARDIFF_BIG_TRIALS=500 cargo run --release --bin sweep-regret-big
//! ```
//! Env: VARDIFF_BIG_TRIALS (default 500), VARDIFF_BIG_THREADS (default =
//!      available_parallelism), VARDIFF_SWEEP_SEED, the VARDIFF_W_*/RHO_*
//!      weight overrides, VARDIFF_BIG_OUT_DIR (default "."), writes
//!      `regret_sweep_big.md`.

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

/// Cost weights (all relative to the "cheap" sides = 1.0). Mirrors
/// `sweep-regret::Weights` so the two scans score identically.
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

/// Aggregated cost components for one algorithm (means over the mix).
struct Profile {
    name: String,
    regret_over: f64,
    regret_under: f64,
    effort_up: f64,
    effort_down: f64,
    detection: f64,
    cost: f64,
    is_anchor: bool,
}

/// One point in the search space. Stored so we can name + build lazily
/// inside each worker (the factory closure captures these by value).
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
        let inner = Composed::new(
            EwmaEstimator::new(p.tau),
            boundary,
            AcceleratingPartialRetarget::new(p.eta_base, p.eta_max, p.accel),
            1.0,
            clock,
        );
        VardiffBox(Box::new(inner))
    })
}

/// Run all cells for one algorithm and reduce to a `Profile`. Replicates
/// `sweep-regret`'s aggregation exactly: regret/effort averaged over the
/// Stable + ±10/±50 mix, detection from the SettledStep{60,−10} cell.
#[allow(clippy::too_many_arguments)]
fn profile_for(
    algo: &AlgorithmSpec,
    algo_idx: usize,
    share_rates: &[f32],
    scenarios: &[Scenario],
    trial_count: usize,
    base_seed: u64,
    w: &Weights,
    is_anchor: bool,
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
            // SAME seed derivation as Grid::run, so this is bit-identical
            // to a serial grid placing this algo at `algo_idx`.
            let cell_index = (algo_idx * n_spm * n_scen + spm_idx * n_scen + scen_idx) as u64;
            let result = run_cell_with_algorithm(algo, &cell, trial_count, base_seed, cell_index);

            let is_mix = matches!(
                scen,
                Scenario::Stable
                    | Scenario::Step {
                        delta_pct: -50 | -10 | 10 | 50
                    }
            );
            if is_mix {
                ro += result.get("regret_over").unwrap_or(0.0);
                ru += result.get("regret_under").unwrap_or(0.0);
                eu += result.get("effort_up").unwrap_or(0.0);
                ed += result.get("effort_down").unwrap_or(0.0);
                n += 1;
            }
            if matches!(
                scen,
                Scenario::SettledStep {
                    settle_minutes: 60,
                    delta_pct: -10
                }
            ) {
                if let Some(rate) = result.get("settled_reaction_rate") {
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
        regret_over,
        regret_under,
        effort_up,
        effort_down,
        detection,
        cost,
        is_anchor,
    })
}

fn main() -> std::io::Result<()> {
    let trial_count: usize = env::var("VARDIFF_BIG_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| {
            s.strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .or_else(|| s.parse().ok())
        })
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_BIG_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
        .max(1);
    let out_dir = env::var("VARDIFF_BIG_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let w = Weights::from_env();

    // ---- Expanded search space (see module docs for the box-edge rationale).
    // Pegged axes pushed outward; the two previously-frozen axes unfrozen.
    // Ranges re-centered after an 8-trial recon pass on a wider box: the
    // recon winners still pegged sens/spm/tighten/accel at their edges, so
    // those four are widened to BRACKET the optimum; axes whose extremes
    // never placed (tau=90, tighten=3, eta_max=0.4, high spm/sens) are
    // trimmed to keep the grid focused.
    let taus = [120u64, 150, 180, 210]; // recon winners clustered 120–150
    let sensitivities = [0.3f64, 0.5, 0.75, 1.0]; // recon pegged 0.5 min → add 0.3 below
    let floors = [0.025f64, 0.05, 0.1]; // all three placed in recon — keep
    let tightens = [4.0f64, 5.0, 6.0, 7.0]; // recon pegged 5 max → add 6,7; drop 3
    let transitions = [3u32, 4, 5, 6]; // recon pegged 5 min → add 3,4; drop 8,10
    let eta_bases = [0.2f32, 0.3];
    let eta_maxes = [0.6f32, 0.8]; // recon never chose 0.4 — drop it
    let accels = [0.05f32, 0.1, 0.2]; // recon pegged 0.1 min → add 0.05; drop 0.4

    // Reference anchors: production-classic (now == ClassicComposed) and the
    // two current Pareto champions. Placed first so seed offsets are stable.
    let mut params: Vec<(Params, bool)> = Vec::new();
    let mut specs: Vec<AlgorithmSpec> = vec![
        AlgorithmSpec::classic_vardiff_state(),
        AlgorithmSpec::balanced(),
        AlgorithmSpec::react_priority(),
    ];
    let n_anchors = specs.len();

    for &tau in &taus {
        for &sens in &sensitivities {
            for &floor in &floors {
                for &tighten in &tightens {
                    for &spm in &transitions {
                        for &eta_base in &eta_bases {
                            for &eta_max in &eta_maxes {
                                if eta_max < eta_base {
                                    continue;
                                }
                                for &accel in &accels {
                                    params.push((
                                        Params {
                                            tau,
                                            sens,
                                            floor,
                                            tighten,
                                            spm,
                                            eta_base,
                                            eta_max,
                                            accel,
                                        },
                                        false,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for (p, _) in &params {
        specs.push(spec_for(*p));
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

    let n_algos = specs.len();
    let n_cells = share_rates.len() * scenarios.len();
    let total_runs = n_algos * n_cells * trial_count;
    eprintln!(
        "Big sweep: {} algos ({} grid + {} anchors) × {} cells × {} trials = {} runs, {} threads",
        n_algos,
        n_algos - n_anchors,
        n_anchors,
        n_cells,
        trial_count,
        total_runs,
        n_threads,
    );

    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out: Mutex<Vec<Profile>> = Mutex::new(Vec::with_capacity(n_algos));

    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n_algos {
                    break;
                }
                let is_anchor = i < n_anchors;
                if let Some(p) = profile_for(
                    &specs[i],
                    i,
                    &share_rates,
                    &scenarios,
                    trial_count,
                    base_seed,
                    &w,
                    is_anchor,
                ) {
                    out.lock().unwrap().push(p);
                }
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 256 == 0 || d == n_algos {
                    let el = started.elapsed().as_secs_f64();
                    let rate = d as f64 / el;
                    let eta = (n_algos - d) as f64 / rate.max(1e-9);
                    eprintln!(
                        "  {}/{} algos ({:.0}%) | {:.0}s elapsed | ~{:.0}s remaining",
                        d,
                        n_algos,
                        100.0 * d as f64 / n_algos as f64,
                        el,
                        eta,
                    );
                }
            });
        }
    });

    let mut profiles = out.into_inner().unwrap();
    eprintln!(
        "Big sweep complete: {} profiles in {:.1}s",
        profiles.len(),
        started.elapsed().as_secs_f64()
    );

    // Rank by cost ascending (lower = better).
    profiles.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());
    let anchor_rank: Vec<(usize, &Profile)> = profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_anchor)
        .collect();

    let row = |i: usize, p: &Profile| {
        format!(
            "| {} | {}{} | **{:.4}** | {:.4} | {:.4} | {:.4} | {:.4} | {:.0}% |\n",
            i + 1,
            p.name,
            if p.is_anchor { " ⟵anchor" } else { "" },
            p.cost,
            p.regret_over,
            p.regret_under,
            p.effort_up,
            p.effort_down,
            p.detection * 100.0,
        )
    };

    let mut md = String::new();
    md.push_str("# Big regret/effort sweep\n\n");
    md.push_str(&format!(
        "Cost = {:.1}·regret_over + {:.1}·regret_under + {:.2}·({:.1}·effort_up + {:.1}·effort_down) + {:.2}·(1−detection). Lower is better.\n\n",
        w.w_over, w.w_under, w.rho, w.rho_up, w.rho_down, w.w_det
    ));
    md.push_str(&format!(
        "{} algos, {} trials/cell, base_seed {:#x}. regret = linear ⟨|e|⟩; detection = P[fire≤60min | aged −10%].\n\n",
        n_algos, trial_count, base_seed
    ));

    md.push_str("## Anchor placement\n\n");
    md.push_str("Where the reference algorithms (production-classic + the two current champions) land in the full ranking.\n\n");
    md.push_str("| overall rank | algorithm | cost | det% |\n| --- | --- | --- | --- |\n");
    for (i, p) in &anchor_rank {
        md.push_str(&format!(
            "| {} / {} | {} | {:.4} | {:.0}% |\n",
            i + 1,
            n_algos,
            p.name,
            p.cost,
            p.detection * 100.0
        ));
    }
    md.push('\n');

    md.push_str("## Full ranking\n\n");
    md.push_str("| rank | algorithm | **cost** | reg_over | reg_under | eff_up | eff_down | det% |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for (i, p) in profiles.iter().enumerate() {
        md.push_str(&row(i, p));
    }
    md.push('\n');

    let out_path = out_dir.join("regret_sweep_big.md");
    fs::write(&out_path, &md)?;
    eprintln!("Wrote {}", out_path.display());

    // Echo the top 25 + anchors to stdout for a quick glance.
    println!("\n## Big sweep — top 25 (lower cost = better)\n");
    println!("| rank | algorithm | cost | reg_over | reg_under | eff_up | eff_down | det% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for (i, p) in profiles.iter().take(25).enumerate() {
        print!("{}", row(i, p));
    }
    println!("\nAnchors landed at ranks: {}",
        anchor_rank
            .iter()
            .map(|(i, p)| format!("{} (#{}/{})", p.name, i + 1, n_algos))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
