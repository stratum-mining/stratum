//! Runs the canonical 5 × 10 cell grid against every shipped algorithm
//! and writes a per-algorithm `(toml, md)` pair for each.
//!
//! Algorithms covered:
//! - `VardiffState` — production reference.
//! - `ClassicComposed` — four-axis decomposition of `VardiffState`.
//! - `Parametric` — Classic with `PoissonCI` boundary.
//! - `ParametricStrict` — Parametric with z=3.0 (99.7% CI).
//! - `ClassicPartialRetarget-30` — Classic with `PartialRetarget(η=0.3)`
//!   replacing `FullRetargetWithClamp`.
//! - `EWMA-60s` — `EwmaEstimator(60s)` + `PoissonCI` +
//!   `PartialRetarget(η=0.5)`.
//! - `SlidingWindow-10t` — `SlidingWindowEstimator(10)` + `PoissonCI` +
//!   `FullRetargetNoClamp`.
//! - `FullRemedy` — `EwmaEstimator(120s)` + `PoissonCI` +
//!   `PartialRetarget(η=0.2)`. The mid-arc three-axis composition (a historical
//!   waypoint, NOT the shipped recommendation — superseded by the decline-safety
//!   champion; see `FINDINGS.md`). The three parameters are each substantiated by
//!   their own Pareto sweep (`sweep-ewma-tau`, `sweep-eta`, `sweep-z`,
//!   `sweep-eta-z` — archived on tag `archive/exploration-bins`, regenerable from
//!   there; not in the reproduction surface).
//! - `PoissonAccel`, `AdaptiveBoundary-spm10`, and several tuning families that
//!   sweep around them: `Adp10-*` (eta / max / acceleration variants), `Ckpool*`
//!   (ckpool-estimator ports), `Hyst-*` (hysteresis-boundary variants), `Ck-*`
//!   (ckpool dual-window tunings). (The exact arm set is the `algos` vec in `main`
//!   — read it there rather than trusting a count here; it grows as the search
//!   does, which is why this list names families, not a number.)
//!
//! NOTE: this comparison covers the PRE-champion exploration lineage
//! (Classic → FullRemedy → adaptive-boundary, plus the ckpool/hysteresis ports).
//! The shipped decline-safety champion (`Ewma360 / AdaptiveSignPersist(spm6) /
//! AccelRetarget`) is NOT an arm here — it is selected by the decline gate
//! (`slow-decline`, `sweep-minimax`), not by this cross-axis cost comparison. This
//! bin shows where each pre-champion axis-swap wins or loses; it does not crown the
//! champion.
//!
//! Algorithm is a first-class grid axis, so a single command produces
//! all baselines that can be diff'd against each other to see,
//! axis-by-axis, where each algorithm wins or loses.
//!
//! ## Usage
//!
//! From the sim crate root:
//!
//! ```text
//! cargo run --release --bin compare-algorithms
//! ```
//!
//! Output files (written to the current directory by default):
//! one `baseline_{AlgorithmName}.toml` / `.md` pair per algorithm.
//!
//! ## Configuration via environment
//!
//! - `VARDIFF_COMPARE_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_COMPARE_SEED` — base seed (default
//!   `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_COMPARE_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_COMPARE_PAIRED` — when set to `1`/`true`, use
//!   `Grid::run_paired` so all algorithms see the same trial inputs
//!   per cell (smaller cross-algorithm noise, but baselines emitted
//!   here are not directly comparable to the algo-indexed ones
//!   produced by `generate-baseline`).
//!
//! ## Runtime
//!
//! Every arm × 5 SPM × 10 scenarios × 1000 trials — a few dozen arms, so on the
//! order of a million trials; several minutes at release speed. Debug mode is
//! 5–10× slower. Reduce `VARDIFF_COMPARE_TRIALS` (e.g. to 50) for fast iteration.
//!
//! ## What's in the output
//!
//! Each algorithm's `.toml` carries the standard 7 metrics (convergence,
//! settled accuracy, jitter, reaction time, bias, variance, phase 1
//! overshoot) per cell where applicable. The `.md` renders each metric
//! as a table plus the decoupling-score summary at the end. For
//! observable algorithms (everything except `VardiffState`), bias /
//! variance / overshoot show real values; for `VardiffState` they're
//! omitted (the algorithm doesn't expose introspection).
//!
//! For Pareto comparison, diff the per-algorithm `.md` files this writes,
//! e.g.:
//!
//! ```text
//! diff baseline_classic.md baseline_FullRemedy.md | less
//! ```
//!
//! These comparison baselines are regenerated on demand and not tracked
//! (only `baseline_classic` and `baseline_champion` are tracked, as CI
//! regression fixtures); this binary IS their archive.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{
    fmt_duration, serialize_markdown, serialize_toml, CellResult, Scenario, DEFAULT_BASELINE_SEED,
    DEFAULT_TRIAL_COUNT,
};
use vardiff_sim::grid::{AlgorithmSpec, Grid, VardiffBox};
use vardiff_sim::metrics::{self, Direction, ScenarioFilter, SummaryFmt, SummarySpec};
use vardiff_sim::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, CkpoolEstimator,
    Composed, EwmaEstimator, HysteresisGate, PartialRetarget, PoissonCI,
};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_COMPARE_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_COMPARE_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_COMPARE_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // The canonical scenario set: cold start + stable + 8 step deltas
    // + 4 counter-age scenarios for sensitivity characterization.
    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }
    // Counter-age characterization: young (5min) vs mature (60min) counter
    // at both large (-50%) and moderate (-10%) step magnitudes.
    for &settle in &[5u64, 60] {
        for &delta in &[-50i32, -10] {
            scenarios.push(Scenario::SettledStep {
                settle_minutes: settle,
                delta_pct: delta,
            });
        }
    }

    let paired = env::var("VARDIFF_COMPARE_PAIRED")
        .map(|s| matches!(s.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);

    let grid = Grid {
        algorithms: vec![
            AlgorithmSpec::classic_vardiff_state(),
            AlgorithmSpec::classic_composed(),
            AlgorithmSpec::parametric(),
            AlgorithmSpec::parametric_strict(),
            AlgorithmSpec::classic_partial_retarget(0.3),
            AlgorithmSpec::ewma_60s(),
            AlgorithmSpec::sliding_window(10),
            AlgorithmSpec::full_remedy(),
            AlgorithmSpec::poisson_accel(),
            // --- Winner: AdaptiveBoundary-spm10 (PoissonCI < 10 SPM, CUSUM >= 10) ---
            AlgorithmSpec::adaptive_boundary(10),
            // --- Phase 2: Update rule sweep with AdaptiveBoundary-spm10 locked ---
            // Vary eta_base
            AlgorithmSpec::new("Adp10-eta15", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.15, 0.6, 0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Adp10-eta25", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.25, 0.6, 0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Adp10-eta30", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
                    1.0, clock,
                )))
            }),
            // Vary eta_max
            AlgorithmSpec::new("Adp10-max40", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Adp10-max80", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.8, 0.2),
                    1.0, clock,
                )))
            }),
            // Vary acceleration
            AlgorithmSpec::new("Adp10-acc10", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.6, 0.1),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Adp10-acc30", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.6, 0.3),
                    1.0, clock,
                )))
            }),
            // Fixed eta (no acceleration) for comparison
            AlgorithmSpec::new("Adp10-fixed20", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Adp10-fixed30", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    AdaptivePoissonCusum::new(10),
                    PartialRetarget::new(0.3),
                    1.0, clock,
                )))
            }),
            // --- Ckpool estimator + best boundary/update (equivalence check) ---
            AlgorithmSpec::new("Ckpool-Adp10-max40", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::new(60, 300),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Ckpool120-Adp10-max40", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::new(60, 120),
                    AdaptivePoissonCusum::new(10),
                    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
                    1.0, clock,
                )))
            }),
            // --- Hysteresis boundary sweep (tuned for 60s ticks) ---
            // Native ckpool: too wide
            AlgorithmSpec::new("Hyst-50-133-gate4", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.5, 1.33),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            // Narrower bands for 60s ticks
            AlgorithmSpec::new("Hyst-70-130-gate4", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.7, 1.3),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Hyst-80-120-gate4", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.8, 1.2),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Hyst-85-115-gate4", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.85, 1.15),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Hyst-90-110-gate4", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.9, 1.1),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            // Best narrow band + AcceleratingPartialRetarget
            AlgorithmSpec::new("Hyst-85-115-accel", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.85, 1.15),
                    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Hyst-80-120-accel", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(4, 60, 0.8, 1.2),
                    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
                    1.0, clock,
                )))
            }),
            // Vary data gate (lower = more responsive at low SPM)
            AlgorithmSpec::new("Hyst-85-115-gate2", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(2, 60, 0.85, 1.15),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::new("Hyst-85-115-gate6", |clock| {
                VardiffBox(Box::new(Composed::new(
                    EwmaEstimator::new(120),
                    HysteresisGate::new(6, 60, 0.85, 1.15),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            AlgorithmSpec::ewma_adaptive_cusum(120, 1.5, 0.05, 0.2),
            AlgorithmSpec::ewma_adaptive_cusum(120, 1.5, 0.05, 0.5),
            AlgorithmSpec::ckpool_remedy(),
            AlgorithmSpec::ckpool_remedy_ft(12),
            // --- Ckpool parameter sweep ---
            // Axis 1: shorter tau_long (120s vs 300s default)
            AlgorithmSpec::new("Ck-tl120-eta20", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(60, 120, 12),
                    PoissonCI::default_parametric(),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            // Axis 2: higher η (0.35) with shorter tau_long
            AlgorithmSpec::new("Ck-tl120-eta35", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(60, 120, 12),
                    PoissonCI::default_parametric(),
                    PartialRetarget::new(0.35),
                    1.0, clock,
                )))
            }),
            // Axis 3: CUSUM boundary instead of PoissonCI
            AlgorithmSpec::new("Ck-cusum-eta20", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(60, 300, 12),
                    AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
                    PartialRetarget::new(0.2),
                    1.0, clock,
                )))
            }),
            // Axis 4: CUSUM + shorter tau_long + moderate η
            AlgorithmSpec::new("Ck-tl120-cusum-eta30", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(60, 120, 12),
                    AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
                    PartialRetarget::new(0.3),
                    1.0, clock,
                )))
            }),
            // Axis 5: shorter tau_short (30s) for faster ramp-up
            AlgorithmSpec::new("Ck-ts30-tl120-eta30", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(30, 120, 8),
                    PoissonCI::default_parametric(),
                    PartialRetarget::new(0.3),
                    1.0, clock,
                )))
            }),
            // Axis 6: AcceleratingPartialRetarget (ramp η on consecutive fires)
            AlgorithmSpec::new("Ck-tl120-accel", |clock| {
                VardiffBox(Box::new(Composed::new(
                    CkpoolEstimator::with_fast_threshold(60, 120, 12),
                    PoissonCI::default_parametric(),
                    AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
                    1.0, clock,
                )))
            }),
        ],
        share_rates: vec![4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Comparing {} algorithms × {} cells × {} trials = {} total trials, base_seed = {:#x}",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        base_seed,
    );
    eprintln!("Output directory: {}", out_dir.display());
    eprintln!(
        "Seeding mode: {}",
        if paired {
            "paired (Grid::run_paired)"
        } else {
            "algo-indexed (Grid::run)"
        }
    );

    let started = Instant::now();
    let results = if paired {
        grid.run_paired()
    } else {
        grid.run()
    };
    let elapsed = started.elapsed();
    eprintln!("Sweep complete in {:.2}s", elapsed.as_secs_f64());

    fs::create_dir_all(&out_dir)?;

    // Sorted for stable output ordering across runs.
    let mut algo_names: Vec<&String> = results.keys().collect();
    algo_names.sort();

    for name in &algo_names {
        let cells = &results[*name];
        // The display name carries " / " separators and the monolith "*"
        // marker; sanitize to a filesystem-safe stem for the on-disk files.
        let file_stem = vardiff_sim::naming::sanitize_filename(name);
        let toml_path = out_dir.join(format!("baseline_{}.toml", file_stem));
        let md_path = out_dir.join(format!("baseline_{}.md", file_stem));
        fs::write(
            &toml_path,
            serialize_toml(cells, name, trial_count, base_seed),
        )?;
        fs::write(
            &md_path,
            serialize_markdown(cells, name, trial_count, base_seed),
        )?;
        eprintln!("  {}", toml_path.display());
        eprintln!("  {}", md_path.display());
    }

    // Cross-algorithm Pareto report — per metric, per share rate, side
    // by side. Winner bolded per row. Lets a reviewer see at a glance
    // which algorithm wins on each metric without diff'ing N files.
    let pareto_path = out_dir.join("pareto.md");
    let pareto_order = canonical_algorithm_order(&algo_names);
    fs::write(
        &pareto_path,
        render_pareto_report(&results, &pareto_order, trial_count, base_seed),
    )?;
    eprintln!("  {}", pareto_path.display());

    eprintln!("\nDone. Read pareto.md for the cross-algorithm summary; diff baseline_X.md against baseline_Y.md for per-algorithm deep-dives.");
    Ok(())
}

/// Canonical algorithm display order for the Pareto report. Names are the
/// derived "Estimator / Boundary / Update" triples:
/// 1. Cumul / Step / FullClamp*    — production monolith (VardiffState)
/// 2. Cumul / Step / FullClamp     — composed equivalent (ClassicComposed)
/// 3. Cumul / Poisson-z2.58 / FullClamp, Cumul / Poisson-z3.00 / FullClamp — boundary swaps
/// 4. Cumul / Step / Partial-e0.3  — update-axis swap
/// 5. Ewma60s / Poisson-z2.58 / Partial-e0.5  — multi-axis swap
/// 6. Slide10t / Poisson-z2.58 / FullNoClamp  — alt estimator
/// 7. Ewma120s / Poisson-z2.58 / Partial-e0.2 — FullRemedy, the mid-arc waypoint
///    (superseded; not the shipped recommendation — see FINDINGS.md)
///
/// Falls back to alphabetical for any algorithm not in this list,
/// appended at the end.
fn canonical_algorithm_order(algo_names: &[&String]) -> Vec<String> {
    let preferred: &[&str] = &[
        "Cumul / Step / FullClamp*",
        "Cumul / Step / FullClamp",
        "Cumul / Poisson-z2.58 / FullClamp",
        "Cumul / Poisson-z3.00 / FullClamp",
        "Cumul / Step / Partial-e0.3",
        "Ewma60s / Poisson-z2.58 / Partial-e0.5",
        "Slide10t / Poisson-z2.58 / FullNoClamp",
        "Ewma120s / Poisson-z2.58 / Partial-e0.2",
    ];
    let mut order: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for &p in preferred {
        if algo_names.iter().any(|n| n.as_str() == p) {
            order.push(p.to_string());
            seen.insert(p);
        }
    }
    // Append any extras alphabetically.
    let mut extras: Vec<&String> = algo_names
        .iter()
        .filter(|n| !seen.contains(n.as_str()))
        .copied()
        .collect();
    extras.sort();
    for e in extras {
        order.push(e.clone());
    }
    order
}

/// Renders the side-by-side cross-algorithm Pareto report. One section
/// per `SummarySpec` declared across the per-cell and derived
/// registries; each section is one row per share rate × one column per
/// algorithm, with the winner bolded.
fn render_pareto_report(
    results: &HashMap<String, Vec<CellResult>>,
    algo_order: &[String],
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str("# Cross-algorithm Pareto comparison\n\n");
    out.push_str(&format!(
        "*Generated by `cargo run --release --bin compare-algorithms`. {} trials per cell, base seed `{:#x}`.*\n\n",
        trial_count, base_seed
    ));
    out.push_str(
        "One section per headline metric. Each row is a share rate; \
         each column is one algorithm. The **bold** entry per row is the \
         winner in that metric's direction-of-improvement; entries \
         within 5% (relative) of the winner are also bolded as effective \
         ties. Algorithms are ordered: production reference → axis-swap \
         variants → the mid-arc three-axis composition (`FullRemedy`; a \
         waypoint, not the shipped algorithm).\n\n",
    );

    // Per-cell metric sections.
    for metric in metrics::registry() {
        for spec in metric.summary_specs() {
            render_pareto_section_per_cell(&mut out, &spec, results, algo_order);
        }
    }

    // Derived-metric sections.
    for derived in metrics::derived_registry() {
        for spec in derived.summary_specs() {
            render_pareto_section_derived(&mut out, derived.as_ref(), &spec, results, algo_order);
        }
    }

    out
}

/// Helper: find all share rates that any algorithm reports for this
/// metric's filter (so the column dimensions are consistent).
fn collect_share_rates(
    results: &HashMap<String, Vec<CellResult>>,
    filter: &ScenarioFilter,
) -> Vec<u32> {
    let mut rates: Vec<u32> = Vec::new();
    for cells in results.values() {
        for r in cells {
            if !cell_matches(&r.scenario, filter) {
                continue;
            }
            let spm = r.shares_per_minute as u32;
            if !rates.contains(&spm) {
                rates.push(spm);
            }
        }
    }
    rates.sort_unstable();
    rates
}

fn cell_matches(scen: &Scenario, f: &ScenarioFilter) -> bool {
    match f {
        ScenarioFilter::Any => true,
        ScenarioFilter::Stable => matches!(scen, Scenario::Stable),
        ScenarioFilter::ColdStart => matches!(scen, Scenario::ColdStart),
        ScenarioFilter::StepDelta(d) => {
            matches!(scen, Scenario::Step { delta_pct } if delta_pct == d)
        }
        ScenarioFilter::SettledStepDelta { settle_minutes: sm, delta_pct: d } => {
            matches!(scen, Scenario::SettledStep { settle_minutes, delta_pct }
                if *settle_minutes == *sm && *delta_pct == *d)
        }
    }
}

fn render_pareto_section_per_cell(
    out: &mut String,
    spec: &SummarySpec,
    results: &HashMap<String, Vec<CellResult>>,
    algo_order: &[String],
) {
    let rates = collect_share_rates(results, &spec.scenario_filter);
    if rates.is_empty() {
        return;
    }

    // Pull (spm, value) per algorithm.
    let mut per_algo: HashMap<&str, HashMap<u32, f64>> = HashMap::new();
    for algo in algo_order {
        let cells = match results.get(algo) {
            Some(c) => c,
            None => continue,
        };
        let mut by_spm: HashMap<u32, f64> = HashMap::new();
        for r in cells {
            if !cell_matches(&r.scenario, &spec.scenario_filter) {
                continue;
            }
            if let Some(v) = r.get(spec.key) {
                by_spm.insert(r.shares_per_minute as u32, v);
            }
        }
        per_algo.insert(algo.as_str(), by_spm);
    }

    if per_algo.values().all(|m| m.is_empty()) {
        return;
    }

    write_pareto_table(out, spec, &rates, &per_algo, algo_order);
}

fn render_pareto_section_derived(
    out: &mut String,
    derived: &dyn metrics::DerivedMetric,
    spec: &SummarySpec,
    results: &HashMap<String, Vec<CellResult>>,
    algo_order: &[String],
) {
    // Derived metrics: compute per algorithm, then look up (spm, key).
    let mut per_algo: HashMap<&str, HashMap<u32, f64>> = HashMap::new();
    let mut rates: Vec<u32> = Vec::new();
    for algo in algo_order {
        let cells = match results.get(algo) {
            Some(c) => c,
            None => continue,
        };
        let computed = derived.compute(cells);
        let mut by_spm: HashMap<u32, f64> = HashMap::new();
        for (spm, mv) in computed {
            if let Some(v) = mv.get(spec.key) {
                let spm_u = spm as u32;
                by_spm.insert(spm_u, v);
                if !rates.contains(&spm_u) {
                    rates.push(spm_u);
                }
            }
        }
        per_algo.insert(algo.as_str(), by_spm);
    }
    rates.sort_unstable();
    if rates.is_empty() {
        return;
    }
    write_pareto_table(out, spec, &rates, &per_algo, algo_order);
}

fn write_pareto_table(
    out: &mut String,
    spec: &SummarySpec,
    rates: &[u32],
    per_algo: &HashMap<&str, HashMap<u32, f64>>,
    algo_order: &[String],
) {
    let arrow = match spec.direction {
        Direction::HigherIsBetter => "↑",
        Direction::LowerIsBetter => "↓",
        Direction::Either => "↔",
    };
    out.push_str(&format!(
        "## {} ({} {})\n\n",
        spec.label,
        arrow,
        direction_label(spec.direction)
    ));

    // Header row: | SPM | algo1 | algo2 | ... |
    out.push_str("| SPM |");
    for algo in algo_order {
        if per_algo.contains_key(algo.as_str()) {
            out.push_str(&format!(" {} |", algo));
        }
    }
    out.push('\n');
    out.push_str("| ---");
    for algo in algo_order {
        if per_algo.contains_key(algo.as_str()) {
            out.push_str(" | ---");
        }
    }
    out.push_str(" |\n");

    // Data rows.
    for &spm in rates {
        // Collect this row's values from each algorithm.
        let row_vals: Vec<(&str, Option<f64>)> = algo_order
            .iter()
            .filter_map(|a| {
                per_algo
                    .get(a.as_str())
                    .map(|m| (a.as_str(), m.get(&spm).copied()))
            })
            .collect();

        // Determine the winner: best value given direction. Entries
        // within 5% relative of the winner are also "tied".
        let present: Vec<f64> = row_vals.iter().filter_map(|(_, v)| *v).collect();
        let winner: Option<f64> = match spec.direction {
            Direction::HigherIsBetter => present
                .iter()
                .copied()
                .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v)))),
            Direction::LowerIsBetter => present
                .iter()
                .copied()
                .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.min(v)))),
            Direction::Either => present.iter().copied().fold(None, |acc, v| {
                Some(acc.map_or(v, |a: f64| if v.abs() < a.abs() { v } else { a }))
            }),
        };

        out.push_str(&format!("| {} |", spm));
        for (_, v) in row_vals {
            let cell_str = format_pareto_cell(v, winner, spec.direction, spec.fmt);
            out.push_str(&format!(" {} |", cell_str));
        }
        out.push('\n');
    }
    out.push('\n');
}

fn direction_label(d: Direction) -> &'static str {
    match d {
        Direction::HigherIsBetter => "higher is better",
        Direction::LowerIsBetter => "lower is better",
        Direction::Either => "near zero is better",
    }
}

fn format_pareto_cell(
    value: Option<f64>,
    winner: Option<f64>,
    direction: Direction,
    fmt: SummaryFmt,
) -> String {
    let Some(v) = value else {
        return "—".to_string();
    };
    let formatted = match fmt {
        SummaryFmt::Percentage => format!("{:.1}%", v * 100.0),
        SummaryFmt::Duration => fmt_duration(Some(v)),
        SummaryFmt::Float3 => format!("{:.3}", v),
        SummaryFmt::RatePerMin => format!("{:.3}/min", v),
    };
    // Bold if value is the winner or within 5% relative of it.
    let is_winner = match (winner, direction) {
        (Some(w), Direction::HigherIsBetter) => w > 0.0 && (w - v).abs() / w.abs() < 0.05,
        (Some(w), Direction::LowerIsBetter) => {
            // Tie window depends on magnitude. Use absolute floor of
            // 0.01 plus 5% relative to handle near-zero baselines.
            let tie = (w.abs() * 0.05).max(0.01);
            (v - w).abs() < tie
        }
        (Some(w), Direction::Either) => (v.abs() - w.abs()).abs() < 0.05,
        _ => false,
    };
    if is_winner {
        format!("**{}**", formatted)
    } else {
        formatted
    }
}

fn env_or<T: std::str::FromStr>(var: &str, default: T) -> T {
    env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_or_seed(var: &str, default: u64) -> u64 {
    if let Ok(s) = env::var(var) {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return u64::from_str_radix(hex, 16).unwrap_or(default);
        }
        return s.parse().unwrap_or(default);
    }
    default
}
