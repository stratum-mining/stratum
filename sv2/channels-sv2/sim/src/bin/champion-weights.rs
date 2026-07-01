//! Corner pressure-test + weight-sensitivity analysis.
//!
//! `confirm-champions` (2000 trials) showed the edge-extension probes
//! (sensitivity 0.2/0.15, tighten 8/9) BEAT the in-box cluster — but won
//! in a suspicious way: as sensitivity ↓ and tighten ↑, `regret_over`
//! keeps shrinking while `regret_under` climbs and detection starts to
//! erode (100% → 99%). Total cost improves only because the 3:1
//! over:under weight makes the over-reduction outweigh the under-increase.
//!
//! Hypothesis: this is NOT an interior optimum we haven't reached — it is
//! a FLAT SHOULDER sliding toward a degenerate "almost never tighten"
//! corner (sensitivity→0, tighten→∞ ⇒ sit permanently under-difficulty,
//! cheap regret, but never raise diff). Each box-widening "wins" by a
//! smaller margin until detection finally collapses.
//!
//! This bin tests that hypothesis two ways from ONE simulation pass (cost
//! is a linear combination of the stored components, so re-scoring under
//! any weights is free — no re-simulation):
//!
//!   #2 CORNER PRESSURE-TEST: push the probe set deep into the corner
//!      (sens ∈ {0.1,0.15,0.2,0.3}, tighten ∈ {6,7,8,10,12}). If cost
//!      keeps creeping down WHILE detection breaks, the shoulder is
//!      degenerate and the default-weight "winner" is an artifact.
//!
//!   #3 WEIGHT SENSITIVITY: re-score every config across a grid of
//!      w_over:w_under (1:1 … 5:1) and w_det (0.5,1,2,4). Report the
//!      winner per weight setting and whether it is a CORNER config
//!      (sens ≤ 0.15 or tighten ≥ 10) or INTERIOR. If gentler asymmetry
//!      pulls the champion back to an interior point, the corner is a
//!      weight artifact, not a real optimum.
//!
//! Usage:
//! ```text
//! cargo run --release --bin champion-weights                   # 2000 trials
//! VARDIFF_CW_TRIALS=4000 cargo run --release --bin champion-weights
//! ```
//! Env: VARDIFF_CW_TRIALS (default 2000), VARDIFF_CW_THREADS,
//!      VARDIFF_SWEEP_SEED, VARDIFF_CW_OUT_DIR (default ".").
//! Writes `champion_weights.md`. NOTE: this bin sweeps weights internally,
//! so it ignores the VARDIFF_W_* env overrides.

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

/// Raw, weight-independent cost components for one config — simulated
/// once, then scored under any number of weight settings.
#[derive(Clone)]
struct Components {
    name: String,
    sens: f64,    // for corner classification
    tighten: f64, // for corner classification
    is_anchor: bool,
    regret_over: f64,
    regret_under: f64,
    effort_up: f64,
    effort_down: f64,
    detection: f64,
}

impl Components {
    /// Same cost form as sweep-regret / sweep-regret-big.
    fn cost(&self, w_over: f64, w_under: f64, rho: f64, rho_up: f64, rho_down: f64, w_det: f64) -> f64 {
        w_over * self.regret_over
            + w_under * self.regret_under
            + rho * (rho_up * self.effort_up + rho_down * self.effort_down)
            + w_det * (1.0 - self.detection)
    }
    /// A config is "in the corner" if it sits at/over the edge the
    /// optimizer was sliding toward: very low sensitivity OR very high
    /// tighten. Anchors are never corner.
    fn is_corner(&self) -> bool {
        !self.is_anchor && (self.sens <= 0.15 || self.tighten >= 10.0)
    }
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
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(p.tau),
            AdaptivePoissonCusum::with_params(
                PoissonCI::default_parametric(),
                AsymmetricCusumBoundary::new(p.sens, p.floor, p.tighten),
                p.spm,
            ),
            AcceleratingPartialRetarget::new(p.eta_base, p.eta_max, p.accel),
            1.0,
            clock,
        )))
    })
}

/// Champion skeleton: tau=150, floor=0.05, spm=5, eta_base=0.2,
/// eta_max=0.8, accel=0.05 — vary only sensitivity and tighten (the two
/// axes in question).
fn skel(sens: f64, tighten: f64) -> Params {
    Params {
        tau: 150,
        sens,
        floor: 0.05,
        tighten,
        spm: 5,
        eta_base: 0.2,
        eta_max: 0.8,
        accel: 0.05,
    }
}

fn measure(
    algo: &AlgorithmSpec,
    idx: usize,
    sens: f64,
    tighten: f64,
    is_anchor: bool,
    share_rates: &[f32],
    scenarios: &[Scenario],
    trials: usize,
    base_seed: u64,
) -> Option<Components> {
    let (n_spm, n_scen) = (share_rates.len(), scenarios.len());
    let (mut ro, mut ru, mut eu, mut ed) = (0.0, 0.0, 0.0, 0.0);
    let mut n = 0u32;
    let (mut ds, mut dn) = (0.0, 0u32);
    for (si, &spm) in share_rates.iter().enumerate() {
        for (ci, scen) in scenarios.iter().enumerate() {
            let cell = Cell { shares_per_minute: spm, scenario: scen.clone() };
            let cell_index = (idx * n_spm * n_scen + si * n_scen + ci) as u64;
            let r = run_cell_with_algorithm(algo, &cell, trials, base_seed, cell_index);
            if matches!(scen, Scenario::Stable | Scenario::Step { delta_pct: -50 | -10 | 10 | 50 }) {
                ro += r.get("regret_over").unwrap_or(0.0);
                ru += r.get("regret_under").unwrap_or(0.0);
                eu += r.get("effort_up").unwrap_or(0.0);
                ed += r.get("effort_down").unwrap_or(0.0);
                n += 1;
            }
            if matches!(scen, Scenario::SettledStep { settle_minutes: 60, delta_pct: -10 }) {
                if let Some(rate) = r.get("settled_reaction_rate") {
                    ds += rate;
                    dn += 1;
                }
            }
        }
    }
    if n == 0 {
        return None;
    }
    Some(Components {
        name: algo.name.clone(),
        sens,
        tighten,
        is_anchor,
        regret_over: ro / n as f64,
        regret_under: ru / n as f64,
        effort_up: eu / n as f64,
        effort_down: ed / n as f64,
        detection: if dn > 0 { ds / dn as f64 } else { 0.0 },
    })
}

fn main() -> std::io::Result<()> {
    let trials: usize = env::var("VARDIFF_CW_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_CW_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
        .max(1);
    let out_dir = env::var("VARDIFF_CW_OUT_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));

    // Candidate set: a sens × tighten grid on the champion skeleton that
    // walks from a sane interior (sens 0.5, tighten 6) DEEP into the
    // corner (sens 0.1, tighten 12), plus the three anchors.
    let sens_axis = [0.1f64, 0.15, 0.2, 0.3, 0.5];
    let tighten_axis = [6.0f64, 7.0, 8.0, 10.0, 12.0];

    // (spec, sens, tighten, is_anchor)
    let mut entries: Vec<(AlgorithmSpec, f64, f64, bool)> = Vec::new();
    entries.push((AlgorithmSpec::classic_vardiff_state(), 0.0, 0.0, true));
    entries.push((AlgorithmSpec::balanced(), 0.0, 0.0, true));
    entries.push((AlgorithmSpec::react_priority(), 0.0, 0.0, true));
    for &s in &sens_axis {
        for &t in &tighten_axis {
            entries.push((spec_for(skel(s, t)), s, t, false));
        }
    }

    let scenarios = vec![
        Scenario::Stable,
        Scenario::Step { delta_pct: -50 },
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: 10 },
        Scenario::Step { delta_pct: 50 },
        Scenario::SettledStep { settle_minutes: 60, delta_pct: -10 },
    ];
    let share_rates = vec![6.0f32, 8.0, 12.0, 20.0, 30.0];

    let n = entries.len();
    eprintln!(
        "Champion-weights: {} algos × {} cells × {} trials = {} runs, {} threads",
        n, share_rates.len() * scenarios.len(), trials,
        n * share_rates.len() * scenarios.len() * trials, n_threads
    );

    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Components>> = Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let (spec, s, t, anchor) = &entries[i];
                if let Some(c) = measure(spec, i, *s, *t, *anchor, &share_rates, &scenarios, trials, base_seed) {
                    out.lock().unwrap().push(c);
                }
            });
        }
    });
    let comps = out.into_inner().unwrap();
    eprintln!("Simulated {} configs in {:.1}s", comps.len(), started.elapsed().as_secs_f64());

    // Fixed (non-swept) weight pieces — the well-grounded directional
    // asymmetries and the effort/regret tradeoff knob from §10.
    let (rho, rho_up, rho_down) = (0.5, 3.0, 1.0);

    let mut md = String::new();
    md.push_str("# Corner pressure-test + weight sensitivity\n\n");
    md.push_str(&format!("{} configs, {} trials/cell, base_seed {:#x}. Skeleton: Ewma150/floor0.05/spm5/Accel-0.2-0.8-0.05; sens × tighten varied.\n\n", n, trials, base_seed));

    // ---- #2 CORNER PRESSURE-TEST: raw components on the sens×tighten grid,
    //      at the DEFAULT weights, sorted by cost. Watch detection + regret_under.
    md.push_str("## #2 Corner pressure-test (default weights 3:1, w_det 0.5)\n\n");
    md.push_str("As sens↓ / tighten↑ (deeper corner): does cost keep dropping while detection breaks and regret_under climbs?\n\n");
    md.push_str("| rank | corner? | config | cost | reg_over | reg_under | det% |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    let mut by_default: Vec<&Components> = comps.iter().collect();
    by_default.sort_by(|a, b| a.cost(3.0, 1.0, rho, rho_up, rho_down, 0.5).partial_cmp(&b.cost(3.0, 1.0, rho, rho_up, rho_down, 0.5)).unwrap());
    for (i, c) in by_default.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.0}% |\n",
            i + 1,
            if c.is_anchor { "anchor" } else if c.is_corner() { "**CORNER**" } else { "interior" },
            c.name,
            c.cost(3.0, 1.0, rho, rho_up, rho_down, 0.5),
            c.regret_over, c.regret_under, c.detection * 100.0,
        ));
    }
    md.push('\n');

    // ---- #3 WEIGHT SENSITIVITY: winner per (w_over:w_under, w_det) cell.
    let w_over_axis = [1.0f64, 1.5, 2.0, 3.0, 4.0, 5.0];
    let w_det_axis = [0.5f64, 1.0, 2.0, 4.0];
    md.push_str("## #3 Weight sensitivity — champion vs over:under ratio and detection weight\n\n");
    md.push_str("Each cell: the winning config (lowest cost) under that weight setting, tagged CORNER / interior / anchor. ");
    md.push_str("If gentler ratios or higher w_det pull the winner OUT of the corner, the corner is a weight artifact.\n\n");
    md.push_str("| w_over:w_under \\ w_det | ");
    for wd in &w_det_axis {
        md.push_str(&format!("w_det={} | ", wd));
    }
    md.push('\n');
    md.push_str("| --- |");
    for _ in &w_det_axis {
        md.push_str(" --- |");
    }
    md.push('\n');
    // Also collect a parallel "detection-gated" winner (det ≥ 99.5%).
    let mut gated_note = String::new();
    for &wo in &w_over_axis {
        md.push_str(&format!("| {:.1}:1 | ", wo));
        for &wd in &w_det_axis {
            let winner = comps
                .iter()
                .min_by(|a, b| a.cost(wo, 1.0, rho, rho_up, rho_down, wd).partial_cmp(&b.cost(wo, 1.0, rho, rho_up, rho_down, wd)).unwrap())
                .unwrap();
            let tag = if winner.is_anchor { "anchor" } else if winner.is_corner() { "CORNER" } else { "interior" };
            let label = if winner.is_anchor {
                "anchor".to_string()
            } else {
                format!("s{}/t{}", winner.sens, winner.tighten)
            };
            md.push_str(&format!("{} ({}) | ", label, tag));

            // detection-gated winner for this same setting
            let gated = comps
                .iter()
                .filter(|c| c.detection >= 0.995)
                .min_by(|a, b| a.cost(wo, 1.0, rho, rho_up, rho_down, wd).partial_cmp(&b.cost(wo, 1.0, rho, rho_up, rho_down, wd)).unwrap());
            if let Some(g) = gated {
                if !std::ptr::eq(g, winner) {
                    gated_note.push_str(&format!(
                        "- at w_over={:.1}, w_det={}: unconstrained winner s{}/t{} (det {:.0}%) → with det≥99.5% gate, winner becomes s{}/t{} (det {:.0}%)\n",
                        wo, wd, winner.sens, winner.tighten, winner.detection * 100.0, g.sens, g.tighten, g.detection * 100.0,
                    ));
                }
            }
        }
        md.push('\n');
    }
    md.push('\n');
    if !gated_note.is_empty() {
        md.push_str("### Where a detection gate (≥99.5%) changes the winner\n\n");
        md.push_str(&gated_note);
        md.push('\n');
    } else {
        md.push_str("_A 99.5% detection gate never changed the winner — detection is not the binding constraint in this set._\n\n");
    }

    let out_path = out_dir.join("champion_weights.md");
    fs::write(&out_path, &md)?;
    eprintln!("Wrote {}", out_path.display());

    // Console summary.
    println!("\n=== #2 Corner pressure-test (default 3:1 weights), top 12 ===");
    println!("{:<4} {:<10} {:<8} {:<8} {:<8} {:<6}", "rank", "kind", "cost", "reg_ov", "reg_un", "det%");
    for (i, c) in by_default.iter().take(12).enumerate() {
        let kind = if c.is_anchor { "anchor" } else if c.is_corner() { "CORNER" } else { "interior" };
        println!(
            "{:<4} {:<10} {:<8.4} {:<8.4} {:<8.4} {:<6.0}",
            i + 1, kind, c.cost(3.0, 1.0, rho, rho_up, rho_down, 0.5), c.regret_over, c.regret_under, c.detection * 100.0
        );
    }
    println!("\n=== #3 Weight sensitivity: winner kind per w_over (w_det=0.5) ===");
    for &wo in &w_over_axis {
        let winner = comps.iter().min_by(|a, b| a.cost(wo, 1.0, rho, rho_up, rho_down, 0.5).partial_cmp(&b.cost(wo, 1.0, rho, rho_up, rho_down, 0.5)).unwrap()).unwrap();
        let kind = if winner.is_anchor { "anchor" } else if winner.is_corner() { "CORNER" } else { "interior" };
        let label = if winner.is_anchor { "anchor".into() } else { format!("s{}/t{}", winner.sens, winner.tighten) };
        println!("  w_over {:.1}:1 → {} ({}), det {:.0}%", wo, label, kind, winner.detection * 100.0);
    }
    println!("\nFull tables in {}", out_path.display());
    Ok(())
}
