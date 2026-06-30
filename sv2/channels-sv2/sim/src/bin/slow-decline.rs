//! Slow-decline safety test (see docs/SLOW_DECLINE_TEST.md).
//!
//! A sustained hashrate decline drives e=ln(Ĥ/H) POSITIVE (over-difficulty),
//! the costly §6 side. Shares arrive slow; the correct response is to EASE.
//! The death-spiral risk is self-reinforcing starvation: over-difficulty →
//! fewer shares → less evidence → slower ease → more over-difficulty. The
//! champion's AdaptiveSignPersist switches to the conservative low-SPM
//! PoissonCI guard below spm_threshold=6, which could freeze the ease
//! exactly when the decline drags effective rate down — the hypothesis.
//!
//! Per (rate × spm × algo) cell we report, over the decline phase:
//!   - tighten_fires : count of s>0 fires  (HARD GATE: must be 0)
//!   - max_e         : worst over-difficulty reached (runaway if unbounded)
//!   - end_e         : e at end of decline (did it turn over or keep climbing)
//!   - mean_e        : time-avg over-difficulty = regret_over (graded lag)
//!   - eased         : count of s<0 fires (the corrective action)
//!
//! Usage: cargo run --release --bin slow-decline
//! Env: VARDIFF_SD_TRIALS (default 300), VARDIFF_SD_THREADS, VARDIFF_SWEEP_SEED.
//!
//! ===========================================================================
//! CKPOOL-INVESTIGATION GATE TEST — PRE-REGISTRATION (locked before the run; see
//! plan jazzy-wobbling-dove.md). CKPOOL_INVESTIGATION.md recommends EWMA(120) +
//! AdaptivePoissonCusum(10) + AccelRetarget(0.2,0.4,0.2), selected by SCALAR FITNESS
//! on a -50% drop. We run EWMA(120) through THIS gate (slow/moderate decline, the
//! binding direction) which selected Ewma360. Baselines (METRIC §9.2): champion
//! worst settled +2.7%; Ewma240 +3.5%; gate 5%. Three configs at tau=120 (NOT a
//! tau-sweep): testA (champion boundary), isoB (+APC10, champion update), verbB
//! (+APC10, ckpool update). What's on the stand is OUR theory (360-vs-120; §8.3
//! invariance), ckpool is the probe.
//!
//! MARGIN M is PER-CELL = the rig's worst-settled measurement resolution at that
//! cell (sub-guard 2-4 spm widest: sigma~45% at 2 spm). Set from the reported
//! scatter, NOT guessed; the sub-guard cells carry their own (wider) M.
//!
//! VALIDITY GATE (gates reading anything): the in-run `corrected` (Ewma360/s1.5)
//! PER-CELL settled-e profile must reproduce within M at the read cells (esp.
//! sub-guard), not merely its +2.7% max. If not, the rig drifted — STOP.
//!
//! TEST A (Ewma120 under champion boundary) — three pre-registered buckets:
//!   (i)   worse than +3.5% -> tau-prediction confirmed; if past +5% sub-guard,
//!         EWMA(120) gate-unsafe under our own boundary.
//!   (ii)  between +2.7% and +3.5% -> WRONG IN MAGNITUDE (flank gentler than the
//!         240->360 gradient implied) — partial falsifier, report as such.
//!   (iii) at or below +2.7% -> WRONG IN DIRECTION — shorter window safer
//!         contradicts the valley's left flank; pressures the tau-valley itself.
//!
//! FOUR per-cell comparisons, each isolating ONE difference:
//!   - sub-guard (spm<6, all run identical PoissonCI guard):
//!       A vs isoB       = M-CALIBRATION (same boundary+update -> noise floor;
//!                          disagree>M => M too tight, revisit before reading on)
//!       isoB vs verbB   = UPDATE-RULE effect UNDER THE GUARD
//!   - differing-boundary (spm>=6, aggressive arms differ; esp. 6-10):
//!       isoB vs A       = §8.3 INVARIANCE verdict (<=M broaden §8.3 across boundary
//!                          type; >M narrow to sensitivity-only, prediction was
//!                          extrapolated). verbB does NOT feed this (confounded).
//!       isoB vs verbB   = UPDATE-RULE effect UNDER THE AGGRESSIVE ARM
//!   (update-rule effect differing guard-vs-aggressive => regime-dependent: a finer
//!    finding. update rule is plausibly LIVE on a decline — it governs ease-down.)
//!
//! Conditional follow-up only: a tau-sweep {120..360} to locate a crossing, IFF a
//! gate crossing is found at 120. Do not map the flank before a crossing is known.
//! ===========================================================================

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AdaptiveSignPersist, AsymmetricCusumBoundary,
    Composed, EwmaEstimator, PoissonCI, SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::DEFAULT_BASELINE_SEED;
use vardiff_sim::decline_safety::{
    decline_scenario, DECLINE_OBSERVE_MIN as OBSERVE_MIN, DECLINE_SAFETY_GATE_PCT,
    DECLINE_TICK_SECS as TICK,
};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

/// A SignPersist-family config by (tau, sens) — the two axes the minimax-over-
/// r* band sweep stresses. tighten=8, d0.06, etaMax0.6, accel0.05 fixed.
fn band_cfg(name: &'static str, tau: u64, sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(name, move |clock| {
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

/// The single-rate corrected champion (funnel winner at spm 4-6): Ewma360/s1.5.
fn corrected() -> AlgorithmSpec { band_cfg("corrected(Ewma360/s1.5)", 360, 1.5) }

// ---- ckpool-investigation gate test (see docs/CKPOOL_INVESTIGATION.md ~313-323
// and plan jazzy-wobbling-dove.md). The CKPOOL writeup recommends EWMA(120) +
// AdaptivePoissonCusum(10) + AccelRetarget(0.2,0.4,0.2), selected by SCALAR FITNESS
// on a -50% drop. This runs EWMA(120) through the SAME decline-safety gate that
// selected Ewma360, to test (1) the tau-valley prediction [120 is left of the 360
// floor -> worse settled-over-difficulty than Ewma240's +3.5%, plausibly past +5%
// sub-guard], (2) whether METRIC §8.3's sensitivity-invariance EXTRAPOLATES across
// boundary TYPE [isolated-B vs testA, boundary-only difference], and (3) the
// update-rule effect on a decline [verbatim-B vs isolated-B, update-only difference].
// Three configs at the single window tau=120; NOT a tau-sweep.

/// Test A: EWMA(120) under the CHAMPION boundary + CHAMPION update rule. Differs
/// from `corrected` in tau ONLY (120 vs 360) -> the clean tau-valley test, within
/// the regime §8.3's invariance was measured (SignPersistenceCusum).
fn test_a_ewma120() -> AlgorithmSpec { band_cfg("testA(Ewma120/s1.5)", 120, 1.5) }

/// isolated-B: EWMA(120) + AdaptivePoissonCusum(10) + CHAMPION update (0.2,0.6,0.05).
/// Differs from Test A in BOUNDARY ONLY -> the clean §8.3 invariance-across-boundary-
/// type test. (NOT the ckpool config verbatim — that also changes the update rule.)
fn isolated_b() -> AlgorithmSpec {
    AlgorithmSpec::new("isoB(Ewma120/APC10/upd.6.05)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(120),
            AdaptivePoissonCusum::new(10),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), // champion update
            1.0,
            clock,
        )))
    })
}

/// verbatim-B: the ckpool recommendation AS SHIPPED — EWMA(120) +
/// AdaptivePoissonCusum(10) + AccelRetarget(0.2,0.4,0.2). Differs from isolated-B in
/// UPDATE RULE ONLY -> isolates the accelerating-retarget effect on a decline.
fn verbatim_b() -> AlgorithmSpec {
    AlgorithmSpec::new("verbB(Ewma120/APC10/upd.4.2)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(120),
            AdaptivePoissonCusum::new(10),
            AcceleratingPartialRetarget::new(0.2, 0.4, 0.2), // ckpool update
            1.0,
            clock,
        )))
    })
}

/// The minimax-over-r* band cluster (λ-robust over λ∈{0.5,1,2}, all ~1.12
/// worst-rate ratio). The band-optimal cost drifts INTO the sleepy corner as
/// the firing penalty rises, so — exactly as at single-rate — the decline
/// gate must break the tie. Safety is UNINHERITED for every one of these.
fn band_480_s15() -> AlgorithmSpec { band_cfg("band(Ewma480/s1.5)", 480, 1.5) }
fn band_480_s2() -> AlgorithmSpec { band_cfg("band(Ewma480/s2)", 480, 2.0) }
fn band_720_s2() -> AlgorithmSpec { band_cfg("band(Ewma720/s2)", 720, 2.0) }

/// The interim champion (AsymCusum boundary, no sign-persistence) — the
/// control that isolates whether the sign-persistence discount specifically
/// helps or hurts on a decline.
fn interim() -> AlgorithmSpec {
    AlgorithmSpec::new("interim(AsymCusum)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptivePoissonCusum::with_params(
                PoissonCI::default_parametric(),
                AsymmetricCusumBoundary::new(0.2, 0.05, 6.0),
                5,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.8, 0.05),
            1.0,
            clock,
        )))
    })
}

struct Row {
    rate: f32,
    spm: f32,
    algo: String,
    trials: usize,
    tighten_fires: f64, // mean per trial
    eased: f64,
    max_e_pct: f64,     // worst over-difficulty %, median over trials
    end_e_pct: f64,     // e at decline end (instantaneous trough), median
    settled_e_pct: f64, // e at the END of the post-decline observe window
    settled_mid_e_pct: f64, // e at the window MIDPOINT — if > settled_e, the
                        // offset is still-relaxing lag; if ≈, it's steady bias
    mean_e_pct: f64,    // time-avg over-difficulty during decline
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let trials: usize = env::var("VARDIFF_SD_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_SD_THREADS")
        .ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    let rates = [1.0f32, 2.0, 5.0, 10.0, 20.0, 40.0];
    // Extend BELOW the spm6 guard: {2,4} are the only regime where the
    // PoissonCI guard is load-bearing, and the big sweep never went under 6
    // — so the guard's placement there is untested. This is the worst-case
    // safety regime and the point of this extension.
    let spms = [2.0f32, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];
    let algos: Vec<(String, Box<dyn Fn() -> AlgorithmSpec + Send + Sync>)> = vec![
        // VALIDATION SET (must reproduce the original selection or the gate is
        // wrong): `corrected`=Ewma360/s1.5 must PASS; the `dom*` long-window
        // configs that originally FAILED the cross-rate sub-guard must FAIL.
        ("dom720s03".into(), Box::new(|| band_cfg("dom(Ewma720/s0.3)", 720, 0.3))),
        ("dom720s06".into(), Box::new(|| band_cfg("dom(Ewma720/s0.6)", 720, 0.6))),
        ("dom480s1".into(), Box::new(|| band_cfg("dom(Ewma480/s1)", 480, 1.0))),
        ("dom480s06".into(), Box::new(|| band_cfg("dom(Ewma480/s0.6)", 480, 0.6))),
        ("dom480s03".into(), Box::new(|| band_cfg("dom(Ewma480/s0.3)", 480, 0.3))),
        ("corrected".into(), Box::new(corrected)),     // single-rate champion (must PASS)
        // RECALIBRATION λ-WINNERS (the configs the recalibrated cost picks at
        // various λ): does the sub-guard gate reject the long-window ones the
        // way it did originally (→ champion holds), or do they now pass (→
        // champion moves, owe a full re-sweep + uninherited hardware re-clear)?
        ("rcl720s2".into(),  Box::new(|| band_cfg("rcl(Ewma720/s2)",  720, 2.0))),
        ("rcl240s15".into(), Box::new(|| band_cfg("rcl(Ewma240/s1.5)",240, 1.5))),
        ("rcl150s1".into(),  Box::new(|| band_cfg("rcl(Ewma150/s1)",  150, 1.0))),
        ("rcl150s03".into(), Box::new(|| band_cfg("rcl(Ewma150/s0.3)",150, 0.3))),
        ("champion".into(), Box::new(AlgorithmSpec::champion)), // old s0.3, for contrast
        ("classic".into(), Box::new(AlgorithmSpec::classic_composed)),
        // CKPOOL-INVESTIGATION GATE TEST (3 configs at tau=120; see fns above +
        // plan). corrected (above) is the champion control / +2.7% validity gate.
        ("testA".into(), Box::new(test_a_ewma120)),   // Ewma120 + champion boundary (tau-only vs corrected)
        ("isoB".into(), Box::new(isolated_b)),         // + AdaptivePoissonCusum(10), champion update (boundary-only vs testA)
        ("verbB".into(), Box::new(verbatim_b)),        // + ckpool update 0.2,0.4,0.2 (update-only vs isoB)
    ];
    let _ = (interim, band_720_s2, band_480_s2, band_480_s15);

    // Per-cell trial count CI-matched to a reference of 60 spm: estimator
    // variance ∝ 1/(r*·τ), so a 4-spm cell needs ~15× the trials of a
    // 60-spm cell for the same confidence interval. Oversample the sparse
    // cells rather than raising trials everywhere.
    let trials_for = |spm: f32| -> usize {
        let scale = (60.0 / spm as f64).max(1.0); // ∝ 1/spm
        ((trials as f64) * scale).round() as usize
    };

    // Flatten the work into (rate, spm, algo_idx) jobs.
    let n_algos = algos.len();
    let jobs: Vec<(f32, f32, usize)> = rates
        .iter()
        .flat_map(|&r| spms.iter().flat_map(move |&s| (0..n_algos).map(move |a| (r, s, a))))
        .collect();
    eprintln!(
        "slow-decline: {} cells, base {} trials (sparse cells oversampled up to {}×), {} threads",
        jobs.len(), trials, (60.0 / spms[0] as f64).round() as u64, n_threads
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
                let (rate, spm, ai) = jobs[j];
                let cell_trials = trials_for(spm);
                let (scen, d_start, d_end, trial_end) = decline_scenario(rate);
                let (config_proto, schedule) = scen.build(spm);
                let config = TrialConfig { tick_interval_secs: TICK, ..config_proto };
                // wrong_dir = tighten (s>0) WHILE still over-difficulty (e>0):
                //   the literal death-spiral step. A tighten while e<0 is a
                //   correct staircase-overshoot correction and is NOT counted.
                let (mut wrong_dir, mut eased) = (0.0f64, 0.0f64);
                let (mut maxe, mut ende, mut meane, mut settled, mut settled_mid) =
                    (vec![], vec![], vec![], vec![], vec![]);
                for i in 0..cell_trials {
                    let clock = Arc::new(MockClock::new(0));
                    let v = (algos[ai].1)().factory.clone()(clock.clone());
                    let t = run_trial_observed(v, clock, config.clone(), &schedule, base_seed.wrapping_add(i as u64));
                    let (mut mx, mut last, mut sum, mut n) = (f64::MIN, 0.0f64, 0.0f64, 0u32);
                    let mut settled_e = 0.0f64; // last e in the post-decline observe window
                    let mut settled_mid_e = 0.0f64; // e at the window midpoint
                    let mid_t = d_end + (trial_end - d_end) / 2;
                    for tk in &t.ticks {
                        let h_true = schedule.at(tk.t_secs.saturating_sub(TICK / 2)) as f64;
                        let e = (tk.current_hashrate_before as f64 / h_true).ln();
                        // post-decline window: track recovered (settled) error.
                        // mid vs end distinguishes residual lag (mid>end, still
                        // relaxing) from a true steady bias (mid≈end).
                        if tk.t_secs > d_end && tk.t_secs <= mid_t {
                            settled_mid_e = e;
                        }
                        if tk.t_secs > d_end && tk.t_secs <= trial_end {
                            settled_e = e;
                        }
                        // decline window: the safety metrics.
                        if tk.t_secs <= d_start || tk.t_secs > d_end {
                            continue;
                        }
                        mx = mx.max(e);
                        last = e;
                        sum += e.max(0.0); // over-difficulty contribution
                        n += 1;
                        if tk.fired {
                            if let Some(nh) = tk.new_hashrate {
                                let s = (nh as f64 / tk.current_hashrate_before as f64).ln();
                                if s < 0.0 {
                                    eased += 1.0;
                                } else if e > 0.02 {
                                    // tighten while genuinely over-difficulty (>2%,
                                    // not the noisy e≈0 staircase crossing) = the
                                    // death-spiral step.
                                    wrong_dir += 1.0;
                                }
                            }
                        }
                    }
                    if n > 0 {
                        maxe.push(mx * 100.0);
                        ende.push(last * 100.0);
                        meane.push(sum / n as f64 * 100.0);
                        settled.push(settled_e * 100.0);
                        settled_mid.push(settled_mid_e * 100.0);
                    }
                }
                let tn = cell_trials as f64;
                out.lock().unwrap().push(Row {
                    rate, spm, algo: algos[ai].0.clone(), trials: cell_trials,
                    tighten_fires: wrong_dir / tn, eased: eased / tn,
                    settled_mid_e_pct: median(settled_mid),
                    max_e_pct: median(maxe), end_e_pct: median(ende),
                    settled_e_pct: median(settled), mean_e_pct: median(meane),
                });
            });
        }
    });

    let mut rows = out.into_inner().unwrap();
    rows.sort_by(|a, b| (a.rate, a.spm as u32, a.algo.clone()).partial_cmp(&(b.rate, b.spm as u32, b.algo.clone())).unwrap());

    println!("\n## Slow-decline safety. e>0 = over-difficulty (costly). spm<6 is the guard-only regime.");
    println!("Runaway test = SETTLED e (after a {}-min recovery window), not the instantaneous trough.\n", OBSERVE_MIN);
    println!("| rate %/hr | spm | algo | trials | wrong-dir | eases | max e% | settled e% | mean e% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let mut fails = 0;
    for r in &rows {
        // A genuine runaway = settled (recovered) e still materially over-difficulty.
        let runaway = r.settled_e_pct > DECLINE_SAFETY_GATE_PCT;
        let flag = if runaway { " ⚠RUNAWAY" } else if r.tighten_fires > 0.05 { " ·wrongdir" } else { "" };
        if runaway { fails += 1; }
        println!(
            "| {} | {} | {} | {} | {:.2} | {:.1} | {:+.1} | {:+.1}{} | {:.2} |",
            r.rate as u32, r.spm as u32, r.algo, r.trials, r.tighten_fires, r.eased,
            r.max_e_pct, r.settled_e_pct, flag, r.mean_e_pct
        );
    }
    println!("\nRunaway cells (settled e > {}% over-difficulty): {}.", DECLINE_SAFETY_GATE_PCT, fails);
    // Lag-vs-bias diagnostic for the sub-guard cells: if mid-window e is
    // materially ABOVE end-window e, the offset is still relaxing (residual
    // decline lag, benign); if mid ≈ end, it is a true steady-state offset.
    println!("\nSub-guard (spm<6) lag-vs-bias — dom720s03 (sleepiest dominator, most at-risk), e at observe-window mid vs end:");
    for r in rows.iter().filter(|r| r.spm < 6.0 && r.algo == "dom720s03") {
        let relaxing = r.settled_mid_e_pct - r.settled_e_pct;
        let tag = if relaxing > 2.0 { "LAG (still relaxing)" } else { "steady" };
        println!(
            "  {}%/hr {}spm: mid {:+.1}% → end {:+.1}%  (Δ {:+.1} = {})",
            r.rate as u32, r.spm as u32, r.settled_mid_e_pct, r.settled_e_pct, relaxing, tag
        );
    }
    println!("Per-algo summary (worst over rates/spm):");
    for algo in ["corrected", "testA", "isoB", "verbB", "rcl720s2", "rcl240s15", "rcl150s1", "rcl150s03", "dom720s03", "dom720s06", "dom480s1", "dom480s06", "dom480s03", "champion", "classic"] {
        let sub: Vec<&Row> = rows.iter().filter(|r| r.algo == algo).collect();
        let worst_max = sub.iter().map(|r| r.max_e_pct).fold(f64::MIN, f64::max);
        let worst_settled = sub.iter().map(|r| r.settled_e_pct).fold(f64::MIN, f64::max);
        let worst_wd = sub.iter().map(|r| r.tighten_fires).fold(f64::MIN, f64::max);
        // Worst sub-guard cell (spm<6) specifically — the untested regime.
        let worst_guard = sub.iter().filter(|r| r.spm < 6.0).map(|r| r.settled_e_pct).fold(f64::MIN, f64::max);
        println!(
            "  {}: worst max_e {:+.1}%, worst SETTLED e {:+.1}%, worst sub-guard settled {:+.1}%, worst wrong-dir/run {:.2}",
            algo, worst_max, worst_settled, worst_guard, worst_wd
        );
    }

    // ====================================================================
    // CKPOOL-INVESTIGATION GATE TEST — per-cell readout for the four
    // pre-registered comparisons (see the module header). settled-e% per cell
    // for corrected / testA / isoB / verbB, with the four isolated diffs.
    // M is NOT auto-derived (the rig reports medians, not CIs here) — read it
    // off the spread; the table prints the raw per-cell settled-e so the
    // per-cell M call is made on the visible numbers, not a baked threshold.
    // ====================================================================
    println!("\n## CKPOOL GATE TEST — settled-e% per cell (the four pre-registered comparisons).");
    println!("Guard regime: spm<6 (all four run the IDENTICAL PoissonCI guard). Differing-boundary: spm>=6.");
    println!("Validity gate: `corrected` must reproduce its known per-cell profile (worst +2.7%) — check first.\n");
    let get = |algo: &str, rate: f32, spm: f32| -> Option<f64> {
        rows.iter().find(|r| r.algo == algo && r.rate == rate && r.spm == spm).map(|r| r.settled_e_pct)
    };
    println!("| rate | spm | regime | corrected | testA | isoB | verbB | A−corr(τ) | isoB−A(bndry/INVAR) | verbB−isoB(update) |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for &rate in &rates {
        for &spm in &spms {
            let (c, a, ib, vb) = (get("corrected", rate, spm), get("testA", rate, spm),
                                  get("isoB", rate, spm), get("verbB", rate, spm));
            if let (Some(c), Some(a), Some(ib), Some(vb)) = (c, a, ib, vb) {
                let regime = if spm < 6.0 { "guard" } else { "diff-bnd" };
                // sub-guard: isoB−A is the M-calibration check (identical boundary);
                // diff-bnd: isoB−A is the INVARIANCE verdict.
                println!(
                    "| {} | {} | {} | {:+.1} | {:+.1} | {:+.1} | {:+.1} | {:+.1} | {:+.1} | {:+.1} |",
                    rate as u32, spm as u32, regime, c, a, ib, vb,
                    a - c, ib - a, vb - ib
                );
            }
        }
    }
    println!("\nREAD (per pre-registration): testA vs corrected = τ-valley (buckets: testA worst-settled vs +3.5%/+5%).");
    println!("  sub-guard isoB−A ≈ 0 (within M) = M-calibration OK (identical boundary+update there).");
    println!("  sub-guard verbB−isoB = update-rule effect UNDER THE GUARD; diff-bnd verbB−isoB = under the AGGRESSIVE arm.");
    println!("  diff-bnd isoB−A = §8.3 INVARIANCE verdict (≈0 broaden across boundary type; large = narrow to sensitivity-only).");
    println!("  (verbB does NOT feed the invariance verdict — it differs from A on two axes. M is read per-cell off the spread.)");
}
