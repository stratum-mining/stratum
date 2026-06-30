//! Cold-start SEED measurement (Path A of NOMINAL_HASHRATE_COLDSTART.md).
//!
//! Question: does seeding the EWMA estimator from the declared nominal at
//! channel open collapse the cold-start ramp — and by how much? This is the
//! pre-registered Path-A payoff measurement, run BEFORE the doc asserts a
//! magnitude.
//!
//! PRE-REGISTERED CAVEAT (recorded before reading the numbers): the cold
//! `EwmaEstimator` has an `n_ticks==0 → rate = pending` fast-path
//! (estimator.rs:386) that snaps belief to the FIRST observed share count in
//! one tick. If that path dominates, a cold estimator already recovers in ~1
//! tick and the seed is largely INERT — the real convergence time would then
//! be set by the boundary's evidence accumulation and the partial-retarget η,
//! NEITHER of which the seed touches. The honest outcomes are therefore:
//!   (i)  seed collapses a multi-tick ramp  → real payoff, quantify it; or
//!   (ii) cold already snaps in ~1 tick      → seed inert in sim; the doc's
//!        "~65-min ramp" claim is about the OPERATING POINT / boundary, not the
//!        estimator, and must be re-scoped.
//! Either way the binary reports the per-tick e curve and lets it decide.
//!
//! Setup mirrors a production channel OPEN: the operating point
//! (`initial_hashrate`) is set to the DECLARED nominal (accurate, or 2×/0.5×
//! wrong), exactly as the channel sets `D = nominal/r*` at open. The cold arm
//! builds the estimator with `EwmaEstimator::new(360)`; the seeded arm with
//! `new_seeded(360, r*, 1)`. Everything else (boundary, retarget) is the
//! champion, identical between arms — so any difference is the seed alone.
//!
//! Metric: per-tick e = ln(belief/truth) where belief = current_hashrate_before
//! (the operating point the controller is acting on), truth = scheduled H.
//! Reported: median |e| at each of the first ~10 ticks, plus integrated
//! regret ∫|e| over the first 65 min (the ramp window), cold vs seeded.
//!
//! Usage: cargo run --release --bin seed-rampup
//! Env: VARDIFF_SR_TRIALS (default 400), VARDIFF_SR_THREADS, VARDIFF_SWEEP_SEED.

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use channels_sv2::vardiff::composed::{champion_composed, champion_composed_seeded};
use channels_sv2::vardiff::MockClock;
use std::sync::Arc;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::VardiffBox;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const TICK: u64 = 60;
const RAMP_MIN: u64 = 65; // the cold-start ramp window the seed targets (§8.5)
const HEAD_TICKS: usize = 10; // per-tick detail for the first 10 ticks

/// Build a flat-truth open scenario: the miner runs at TRUE_HASHRATE the whole
/// time; the only thing that varies is the controller's OPENING operating
/// point, which we set to `declared` (= the nominal the channel opened on).
fn open_scenario(declared: f32) -> (Scenario, TrialConfig) {
    let phases = vec![Phase::Hold {
        secs: (RAMP_MIN + 5) * 60,
        h: TRUE_HASHRATE,
    }];
    let scen = Scenario::Custom {
        name: format!("open_decl{:.2}x", declared / TRUE_HASHRATE),
        phases,
        // initial_estimate IS the opening operating point (TrialConfig::
        // initial_hashrate). Production sets this from the declared nominal.
        initial_estimate: Some(declared),
    };
    // build() returns (config_proto, schedule); we override the tick interval.
    let (proto, _schedule) = scen.build(0.0);
    (scen, proto)
}

struct Row {
    label: String,
    spm: f32,
    declared_mult: f32,
    seeded: bool,
    head_e: Vec<f64>,    // median |e|%, ticks 1..=HEAD_TICKS
    integ_regret: f64,   // median ∫|e| dt over the ramp window (e-min)
    settled_e: f64,      // median e% at the end of the window (sign-preserving)
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

#[allow(clippy::too_many_arguments)]
fn run_cell(
    spm: f32,
    declared_mult: f32,
    seeded: bool,
    trials: usize,
    base_seed: u64,
) -> Row {
    let declared = TRUE_HASHRATE * declared_mult;
    let (scen, proto) = open_scenario(declared);
    let (_p, schedule) = scen.build(spm);
    let config = TrialConfig {
        tick_interval_secs: TICK,
        shares_per_minute: spm,
        ..proto
    };

    // Per-tick |e| samples across trials, and per-trial integrated regret.
    let mut head: Vec<Vec<f64>> = vec![Vec::new(); HEAD_TICKS];
    let mut integ: Vec<f64> = Vec::new();
    let mut settled: Vec<f64> = Vec::new();
    let ramp_end = RAMP_MIN * 60;

    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v: VardiffBox = if seeded {
            VardiffBox(Box::new(champion_composed_seeded(
                1.0,
                spm as f64,
                1,
                clock.clone(),
            )))
        } else {
            VardiffBox(Box::new(champion_composed(1.0, clock.clone())))
        };
        let t = run_trial_observed(v, clock, config.clone(), &schedule, base_seed.wrapping_add(i as u64));

        let mut last_t = 0u64;
        let mut acc = 0.0f64; // ∫|e| dt in e-seconds
        let mut last_e_in_window = 0.0f64;
        let mut hi = 0usize;
        for tk in &t.ticks {
            let h_true = schedule.at(tk.t_secs.saturating_sub(TICK / 2)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln();
            // per-tick head detail
            if hi < HEAD_TICKS {
                head[hi].push((e.abs()) * 100.0);
                hi += 1;
            }
            // integrate |e| over the ramp window
            if tk.t_secs <= ramp_end {
                let dt = (tk.t_secs - last_t) as f64;
                acc += e.abs() * (dt / 60.0); // e-minutes
                last_e_in_window = e;
            }
            last_t = tk.t_secs;
        }
        integ.push(acc);
        settled.push(last_e_in_window * 100.0);
    }

    let head_e: Vec<f64> = head.into_iter().map(median).collect();
    Row {
        label: if seeded { "seeded".into() } else { "cold".into() },
        spm,
        declared_mult,
        seeded,
        head_e,
        integ_regret: median(integ),
        settled_e: median(settled),
    }
}

fn main() {
    let trials: usize = env::var("VARDIFF_SR_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_SR_THREADS")
        .ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // Deployed rates (slot3=6, slot4=30) plus a mid anchor. Declared-nominal
    // accuracy: 1.0 = accurate open (slot reconnect with good telemetry); 2.0 /
    // 0.5 = over-/under-declared (the seed-robustness arm).
    let spms = [6.0f32, 12.0, 30.0];
    let mults = [1.0f32, 2.0, 0.5];
    let jobs: Vec<(f32, f32, bool)> = spms
        .iter()
        .flat_map(|&s| mults.iter().flat_map(move |&m| [false, true].into_iter().map(move |sd| (s, m, sd))))
        .collect();

    eprintln!(
        "seed-rampup: {} cells × {} trials, {} threads, ramp window {} min",
        jobs.len(), trials, n_threads, RAMP_MIN
    );

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Row>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() {
                    break;
                }
                let (spm, mult, seeded) = jobs[j];
                let row = run_cell(spm, mult, seeded, trials, base_seed);
                out.lock().unwrap().push(row);
            });
        }
    });

    let mut rows = out.into_inner().unwrap();
    rows.sort_by(|a, b| {
        (a.spm as u32, (a.declared_mult * 100.0) as u32, a.seeded)
            .partial_cmp(&(b.spm as u32, (b.declared_mult * 100.0) as u32, b.seeded))
            .unwrap()
    });

    println!("\n## Cold-start seed: per-tick |e|% and ramp-window integrated regret");
    println!("e = ln(operating_point / true_H). decl = declared-nominal multiple of truth (1.0 = accurate open).\n");
    println!("| spm | decl | arm | t1 | t2 | t3 | t5 | t10 | ∫|e| (e-min) | settled e% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let g = |r: &Row, i: usize| r.head_e.get(i).copied().unwrap_or(f64::NAN);
    for r in &rows {
        println!(
            "| {} | {:.1}x | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.2} | {:+.1} |",
            r.spm as u32, r.declared_mult, r.label,
            g(r, 0), g(r, 1), g(r, 2), g(r, 4), g(r, 9),
            r.integ_regret, r.settled_e
        );
    }

    // Paired cold-vs-seeded summary per (spm, decl): the headline number.
    println!("\n## Seed payoff = cold ∫|e| − seeded ∫|e| (e-min saved over the {}-min ramp). >0 ⇒ seed helps.", RAMP_MIN);
    println!("| spm | decl | cold ∫|e| | seeded ∫|e| | saved (e-min) | saved % |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for &spm in &spms {
        for &mult in &mults {
            let cold = rows.iter().find(|r| r.spm == spm && r.declared_mult == mult && !r.seeded);
            let seed = rows.iter().find(|r| r.spm == spm && r.declared_mult == mult && r.seeded);
            if let (Some(c), Some(s)) = (cold, seed) {
                let saved = c.integ_regret - s.integ_regret;
                let pct = if c.integ_regret.abs() > 1e-9 { 100.0 * saved / c.integ_regret } else { 0.0 };
                println!(
                    "| {} | {:.1}x | {:.2} | {:.2} | {:+.2} | {:+.0}% |",
                    spm as u32, mult, c.integ_regret, s.integ_regret, saved, pct
                );
            }
        }
    }
    println!("\nIf 'saved' ≈ 0 at decl=1.0x: the cold estimator's n_ticks==0 fast-path already snaps belief in ~1 tick → seed inert in sim (see header caveat ii).");
    println!("If 'saved' > 0: seed collapses a real multi-tick ramp (caveat i). Read the t1..t10 columns to see WHERE the gap lives.");
}
