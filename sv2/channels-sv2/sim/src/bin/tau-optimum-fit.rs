//! REFINE τ*(r) — settle whether τ*∝1/r, the premise share-indexing rests on.
//!
//! ===========================================================================
//! WHY (the load-bearing premise check, before grounding N_span or building the
//! share-indexed sweep). Share-indexing = a constant-SHARE window. A constant-
//! share window tracks a per-rate optimum ONLY IF that optimum scales as τ*∝1/r
//! (then τ*·r = const = N_span). The committed tau-family ARGMIN suggested the
//! shares-optimum (τ*·r/60) runs 4.5→15 across the band — NOT constant — which
//! would mean τ* slides GENTLER than 1/r and a constant-share window over-tracks.
//! BUT that argmin is on a COARSE grid (30,45,60 at the short-τ end), and
//! tau-family itself flagged the argmin as "indicative, not pinned." So
//! "τ*∝1/r is false" currently rests on reading 45-vs-30 (ONE grid step, below
//! the grid's resolution) as real curvature — refuting the premise with a grid
//! already known too coarse to pin the optimum. This binary measures it properly.
//!
//! FOUR SPECIFICATIONS (or it answers the wrong question):
//!  (1) BOUNDARY REGIME ONLY (spm ≥ 6). The spm=6 guard switch makes spm<6 vs ≥6
//!      DIFFERENT controllers, so τ*∝1/r is a SINGLE-CONTROLLER question askable
//!      only WITHIN a regime. The cross-switch non-monotonicity (8,10 guard vs
//!      4.5..15 boundary) is two regimes STITCHED, not curvature — set aside.
//!      (Guard regime spm{2,4} reported separately, never fit across the switch.)
//!  (2) SHORT-τ-WEIGHTED DENSE GRID. At high spm the optimum is at short τ (~30),
//!      where {30,45,60} is coarse RELATIVE TO THE VALUE — that's where the
//!      1/r-vs-gentler question is decided and where the old grid can't resolve.
//!      Dense spacing at the short end (24,28,32,...), coarser at the long end
//!      (where 10-unit spacing is already fine relative to a ~240 optimum).
//!  (3) VALLEY SHARPNESS → ERROR BAR PER RATE. The cost field is ~12% flat, so
//!      the τ-valley may be SHALLOW → argmin poorly determined. Each per-rate
//!      shares-optimum must carry an UNCERTAINTY: the band of τ whose cost CI
//!      overlaps the minimum's cost CI (all τ statistically indistinguishable
//!      from best). A flat valley → wide band → "4.5±5" not "4.5".
//!  (4) THE QUESTION = SLOPE vs BARS. "Shares constant (1/r)" vs "shares rising
//!      (gentler)" is a claim about a SLOPE in shares-vs-rate. Real only if it
//!      EXCEEDS the argmin uncertainty. Ask: within the boundary regime, does the
//!      shares-optimum slope exceed its error bars, or is it flat-within-bars?
//!
//! THE TWO OUTCOMES, both grounding N_span (r_ref stays DEAD either way):
//!  - FLAT-within-bars ⇒ τ*∝1/r holds ⇒ N_span = the constant shares-optimum,
//!    share-indexing is the principled-exact form.
//!  - SLOPED-beyond-bars ⇒ gentler than 1/r ⇒ constant-N_span over-tracks (too
//!    jumpy at high rate); N_span = best-fit, with a MEASURED MISS. Share-indexing
//!    is then "better-than-fixed-but-imperfect," still worth building IF the miss
//!    is small — NOT dead. (Even the bad case isn't "don't build.")
//!
//! Usage: cargo run --release --bin tau-optimum-fit
//! Env: VARDIFF_TOF_TRIALS (default 200 base, CI-scaled), VARDIFF_TOF_THREADS.
//! ===========================================================================

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const SENS: f64 = 1.5;
// (1) BOUNDARY REGIME ONLY for the fit. Guard regime {2,4} measured but fit
//     separately, never stitched.
const SPMS_BOUNDARY: &[f32] = &[6.0, 8.0, 12.0, 20.0, 30.0];
const SPMS_GUARD: &[f32] = &[2.0, 4.0];
// Decline severities collapsed by worst (matching tau-family's conservatism).
const RATES_PPH: &[f32] = &[1.0, 2.0, 5.0, 10.0, 20.0, 40.0];
// (2) SHORT-τ-WEIGHTED DENSE GRID: fine at the short end (where high-spm optima
//     live and quantization bit), coarser at the long end (fine relative to ~240).
const TAUS: &[u64] = &[
    24, 28, 32, 36, 40, 45, 50, 56, 64, 72, 80, 90, 100, 115, 130, 150, 175, 200, 240, 300, 360,
];

fn cfg(tau: u64, sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}/s{sens}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(sens, 0.05, 8.0, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0,
            clock,
        )))
    })
}

/// Over-difficulty AREA for one (τ,rate,spm) trial (escape-phase ∫max(e,0)dt).
/// Same decline profile as tau-family.rs. Returns per-TRIAL areas (not median)
/// so the caller can compute mean + CI for the valley-sharpness error bar.
fn decline_areas(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> Vec<f64> {
    let mature = 60u64;
    let rate = rate_pph / 100.0 / 60.0;
    let target = 0.50f32;
    let observe = 120u64;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..300u64 {
        let frac = (rate * (m as f32 + 1.0)).min(target);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= target { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - (rate * dm as f32).min(target));
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "decline".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let d_start = mature * 60;
    let d_end = (mature + dm) * 60;
    let mut areas = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut area, mut last_t) = (0.0f64, d_start);
        for tk in &t.ticks {
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
        }
        areas.push(area);
    }
    areas
}

/// (mean, ci95_halfwidth) of a sample.
fn mean_ci(v: &[f64]) -> (f64, f64) {
    let n = v.len();
    if n == 0 { return (f64::NAN, f64::NAN); }
    let mean = v.iter().sum::<f64>() / n as f64;
    if n < 2 { return (mean, f64::NAN); }
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    (mean, 1.96 * (var / n as f64).sqrt())
}

/// For one (τ, spm): worst-over-severity mean over-difficulty area AND the CI at
/// that worst severity (the valley-sharpness input). Worst-area matches
/// tau-family conservatism; the CI is the worst cell's CI.
fn cell(tau: u64, spm: f32, base_trials: usize, seed: u64) -> (f64, f64) {
    let a = cfg(tau, SENS);
    let (mut worst_mean, mut ci_at_worst) = (f64::MIN, f64::NAN);
    let mut k = 0u64;
    for &r in RATES_PPH {
        let ct = (base_trials as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        let (m, ci) = mean_ci(&decline_areas(&a, r, spm, ct, seed.wrapping_add(k << 40)));
        if m > worst_mean { worst_mean = m; ci_at_worst = ci; }
        k += 1;
    }
    (worst_mean, ci_at_worst)
}

struct SpmFit {
    spm: f32,
    argmin_tau: u64,
    argmin_area: f64,
    // (3) the error bar: the τ-band whose cost CI overlaps the minimum's CI —
    // all τ statistically indistinguishable from best. The optimum "could be
    // anywhere in here."
    tau_lo: u64,
    tau_hi: u64,
    // expressed in shares: the optimum and its band, ·r/60.
    shares_opt: f64,
    shares_lo: f64,
    shares_hi: f64,
}

fn fit_spm(spm: f32, grid: &[(u64, f64, f64)]) -> SpmFit {
    // grid: (tau, mean_area, ci). argmin by mean; band = τ whose (mean-ci) ≤
    // (argmin_mean + argmin_ci), i.e. CI overlaps the minimum's CI.
    let (amin_i, &(argmin_tau, argmin_area, argmin_ci)) = grid
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();
    let _ = amin_i;
    let thresh = argmin_area + argmin_ci; // top of the minimum's CI
    let indistinguishable: Vec<u64> = grid
        .iter()
        .filter(|(_, m, ci)| (m - ci) <= thresh) // their CI reaches down into the min's CI
        .map(|(t, _, _)| *t)
        .collect();
    let tau_lo = *indistinguishable.iter().min().unwrap();
    let tau_hi = *indistinguishable.iter().max().unwrap();
    let sh = |t: u64| t as f64 * spm as f64 / 60.0;
    SpmFit {
        spm,
        argmin_tau,
        argmin_area,
        tau_lo,
        tau_hi,
        shares_opt: sh(argmin_tau),
        shares_lo: sh(tau_lo),
        shares_hi: sh(tau_hi),
    }
}

fn main() {
    let base: usize = env::var("VARDIFF_TOF_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
    let seed = DEFAULT_BASELINE_SEED ^ 0x0F17_7ED;
    let nth: usize = env::var("VARDIFF_TOF_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    let all_spms: Vec<f32> = SPMS_BOUNDARY.iter().chain(SPMS_GUARD).copied().collect();
    let jobs: Vec<(f32, usize)> = all_spms.iter().flat_map(|&s| (0..TAUS.len()).map(move |ti| (s, ti))).collect();
    eprintln!("tau-optimum-fit: {} spm × {} τ = {} cells, base {} trials, {} threads",
        all_spms.len(), TAUS.len(), jobs.len(), base, nth);

    let next = AtomicUsize::new(0);
    // results[spm_key][ti] = (tau, mean, ci)
    let out: Mutex<Vec<(f32, u64, f64, f64)>> = Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..nth {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (spm, ti) = jobs[j];
                let (m, ci) = cell(TAUS[ti], spm, base, seed.wrapping_add((j as u64) << 8));
                out.lock().unwrap().push((spm, TAUS[ti], m, ci));
                eprintln!("  spm{} τ{} done", spm, TAUS[ti]);
            });
        }
    });
    let raw = out.into_inner().unwrap();

    let fit_for = |spm: f32| -> SpmFit {
        let mut grid: Vec<(u64, f64, f64)> = raw.iter().filter(|(s, ..)| *s == spm)
            .map(|(_, t, m, ci)| (*t, *m, *ci)).collect();
        grid.sort_by_key(|(t, _, _)| *t);
        fit_spm(spm, &grid)
    };

    println!("\n## τ*(r) REFINED — is the shares-optimum FLAT (τ*∝1/r) or SLOPED (gentler)? WITHIN regime, with error bars.");
    println!("argmin τ*, the indistinguishable-τ band [lo,hi] (cost CI overlaps the min's CI), and the optimum IN SHARES (τ*·r/60).\n");

    for (label, spms) in [("BOUNDARY regime (spm≥6) — the fit that decides τ*∝1/r", SPMS_BOUNDARY),
                          ("GUARD regime (spm<6) — separate controller, NOT stitched to above", SPMS_GUARD)] {
        println!("### {label}");
        println!("| spm | argmin τ* | τ-band [lo,hi] | shares-opt | shares-band [lo,hi] |");
        println!("| --- | --- | --- | --- | --- |");
        let fits: Vec<SpmFit> = spms.iter().map(|&s| fit_for(s)).collect();
        for f in &fits {
            println!("| {} | {} | [{},{}] | {:.1} | [{:.1},{:.1}] |",
                f.spm as u32, f.argmin_tau, f.tau_lo, f.tau_hi, f.shares_opt, f.shares_lo, f.shares_hi);
        }
        // (4) THE QUESTION: does the shares-opt slope across this regime exceed the bars?
        if label.starts_with("BOUNDARY") && fits.len() >= 2 {
            let lo = &fits[0];               // spm 6
            let hi = &fits[fits.len() - 1];  // spm 30
            // is hi's shares-opt band disjoint-above lo's? (slope real) or overlapping? (flat-within-bars)
            let disjoint_rising = hi.shares_lo > lo.shares_hi;
            let disjoint_falling = hi.shares_hi < lo.shares_lo;
            let verdict = if disjoint_rising {
                "SLOPED (rising) beyond bars → τ* GENTLER than 1/r → constant-N_span OVER-tracks (build as approximation, measure the miss)"
            } else if disjoint_falling {
                "SLOPED (falling) beyond bars → τ* STEEPER than 1/r (unexpected — share-indexing UNDER-tracks)"
            } else {
                "FLAT within bars → τ*∝1/r CONSISTENT → constant-N_span is the principled form, N_span = the shares-optimum"
            };
            println!("\n**Boundary-regime slope test (spm6 vs spm30 shares-band overlap):** {verdict}");
            println!("  spm6 shares-band [{:.1},{:.1}] vs spm30 shares-band [{:.1},{:.1}] — {}.",
                lo.shares_lo, lo.shares_hi, hi.shares_lo, hi.shares_hi,
                if disjoint_rising || disjoint_falling { "DISJOINT (slope real)" } else { "OVERLAP (flat-within-bars)" });
            // and report the grid-edge caveat if argmin is AT an edge (optimum may be outside the grid)
            for f in &fits {
                if f.argmin_tau == *TAUS.first().unwrap() || f.argmin_tau == *TAUS.last().unwrap() {
                    println!("  ⚠ spm{} argmin at grid EDGE (τ={}): true optimum may lie outside [{},{}] — widen grid.",
                        f.spm as u32, f.argmin_tau, TAUS.first().unwrap(), TAUS.last().unwrap());
                }
            }
        }
        println!();
    }
    println!("READ: if the boundary-regime shares-band is FLAT-within-bars across spm6→30, τ*∝1/r holds and N_span is the");
    println!("grounded constant. If the bands are DISJOINT (slope exceeds bars), τ* is gentler than 1/r and constant-shares is");
    println!("an approximation with a measured miss. The coarse-grid '4.5→15' must NOT be trusted unless the bands are disjoint.");
}
