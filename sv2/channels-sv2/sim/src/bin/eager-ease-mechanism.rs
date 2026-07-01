//! EAGER-EASE MECHANISM TEST — is the reluctant-tighten asymmetry a NOISE-SAFETY
//! property (refuses wrong-direction fires on sparse-stream up-spikes during an
//! escape), or pure wobble-efficiency? And is noise-safety SUBSTITUTABLE between the
//! boundary asymmetry and estimator smoothing? Measures the MECHANISM (tighten-fire
//! count during escape), not just the gate outcome — because the effect lives at the
//! collapsed rate where depth-magnitude has been sub-resolution all arc, but
//! event-counts are resolvable there (cf. the worst-cell trace: 7 fires legible at
//! 196 events).
//!
//! ===========================================================================
//! THE FORK (settles two analytic reframes at once). Validity of a vardiff was
//! framed as "eager-ease (A>1) necessary." Reframe 1: validity is the dangerous-
//! direction ESCAPE RATE clearing a spiral floor; eager-ease is the wobble-efficient
//! corner, symmetric-high-gain is a valid-but-wobbly counterexample (escapes by
//! catching the excursion shallow before the collapse starves it). Reframe 2 (the
//! sharper one): symmetric-high-gain escapes on a CLEAN STEP but FAILS on a REAL
//! noisy DECLINE, because at the collapsed (sparse, Poisson-thin) rate the stream
//! up-fluctuates even while truly dropping, and a SYMMETRIC low-threshold boundary
//! FIRE-TIGHTENS on those up-spikes mid-escape — raising difficulty, deepening
//! over-difficulty, collapsing the stream further. The reluctant-tighten half
//! structurally REFUSES those fires. So reluctant-tighten = noise-safety (invisible
//! on a clean step, decisive on a noisy decline); eager = wobble-efficiency. The two
//! handles split BY FUNCTION.
//!
//! SUBSTITUTABILITY (the third finding the both-jumpy cell tests). Source
//! (boundary.rs:540, composed.rs:262): the asymmetry is reluctant-TIGHTEN
//! (tighten_multiplier=8 RAISES the tighten-branch threshold; ease gets base). Two
//! independent noise-safety mechanisms in the champion: (a) the reluctant-tighten
//! BOUNDARY refuses wrong-direction fires regardless of estimator speed; (b) the SLOW
//! estimator (Ewma360) smooths up-spikes so belief doesn't track them regardless of
//! boundary threshold. PREDICTION (corrected from "estimator-jumpy worse" after
//! reading source): each SINGLE removal stays protected by the OTHER mechanism; only
//! BOTH-removed shows tighten-fires-that-deepen. ⇒ the two are SUBSTITUTES, champion
//! noise-safety is OVER-DETERMINED.
//!
//! CONSTRUCTIONS (each isolates one variable; source-confirmed knobs):
//!   champion       — Ewma360 + reluctant-tighten boundary (tm=8, s=1.5). both present.
//!   boundary-jumpy — Ewma360 + SYMMETRIC low-threshold boundary (tm=1, s=0.3).
//!                    asymmetry removed, slow estimator intact.
//!   estimator-jumpy— FAST estimator (Ewma30) + champion boundary (tm=8, s=1.5).
//!                    estimator sped up, asymmetry intact.
//!   both-jumpy     — Ewma30 + symmetric low-threshold (tm=1, s=0.3). both removed.
//!
//! MECHANISM INSTRUMENT (the faithful, sparse-resolvable measurement). A TIGHTEN-FIRE
//! = a fire where new_hashrate > current_hashrate_before (belief raised → harder
//! difficulty → over-difficulty DEEPENS) — the wrong-direction self-deepening fire.
//! Count tighten-fires DURING THE ESCAPE WINDOW, and the sign of e's step after each.
//! champion should show ~0 (reluctant half refuses); both-jumpy should show several
//! that step e UP. This is an EVENT COUNT — resolvable at the collapsed rate where a
//! depth-magnitude comparison (±7% σ) is not.
//!
//! STIMULI: CLEAN STEP (instantaneous drop — no noise to thrash on; reframe-1 regime)
//! and REAL RAMPED DECLINE (the noisy escape the mechanism needs — reframe-2 regime).
//! Measuring on clean steps alone would systematically confirm reframe-1 by hiding
//! the stimulus that breaks it (the slow-decline-envelope error again).
//!
//! GATE OUTCOME (coarser, may be sub-res): also report peak + sustained escape depth
//! per construction; pre-register that the depth comparison may be UNRESOLVABLE at
//! the collapsed rate (σ~6-7%), in which case the verdict rides the MECHANISM count.
//!
//! PRE-REGISTERED READINGS:
//!   - all constructions clean tighten-fire counts on both stimuli ⇒ reframe-1:
//!     eager-ease pure efficiency, R_escape the validity axis. Build the map on R_escape.
//!   - symmetric constructions show tighten-fires-that-deepen on REAL DECLINE but not
//!     clean step ⇒ reframe-2: reluctant-tighten is noise-safety. Then WHICH fail says
//!     the substitutability story: only both-jumpy fails ⇒ SUBSTITUTES (over-determined);
//!     a single-removal fails ⇒ that mechanism was load-bearing alone.
//!   - sticks on clean step ⇒ arc's original frame (collapse beats gain), noise moot.
//!   - real-decline depth UNRESOLVABLE but tighten-fire count resolvable ⇒ verdict on
//!     the mechanism (event-level), depth-gate noted sub-res (the standing wall).
//!
//! Usage: cargo run --release --bin eager-ease-mechanism
//! Env: VARDIFF_EEM_TRIALS (default 200), VARDIFF_EEM_THREADS.
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

const SPMS: &[f32] = &[2.0, 6.0]; // sparse (mechanism lives here) + boundary edge
const DECLINE_PPH: f32 = 40.0;
const DROP: f32 = 0.7;

/// Construct one of the four variants. tau = estimator window; tm = tighten
/// multiplier (8=reluctant champion, 1=symmetric); s = base sensitivity (1.5=champion,
/// 0.3=jumpy/low-threshold). AdaptiveSignPersist wrapper + AccelPartialRetarget held
/// fixed so ONLY the estimator window and boundary (tm, s) vary.
fn variant(name: &str, tau: u64, tm: f64, s: f64) -> AlgorithmSpec {
    let nm = name.to_string();
    AlgorithmSpec::new(nm, move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(s, 0.05, tm, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
        )))
    })
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

struct Trial {
    peak_e: f64,
    sustained_e: f64,    // mean e over breached escape ticks
    tighten_fires: u32,  // belief-raising fires WHILE ALREADY OVER-DIFFICULT (e>5%) = the
                         // self-deepening pathology. GATED on e-already-breached so a
                         // belief-raising fire from NEAR-EQUILIBRIUM (noise-fire at the
                         // window's leading edge, before over-difficulty develops) is NOT
                         // counted — that's generic jumpiness, not escape self-deepening.
    ease_fires: u32,     // belief-lowering fires while breached (correct escape direction)
    tighten_stepped_up: u32, // of gated tighten-fires, how many DEEPENED e within a few-tick
                             // window (not next-tick: at the collapsed rate the e-response
                             // lags through empty ticks, so next-tick alone under-counts).
}

/// One trial. `clean` = instantaneous drop (no ramp); else ramped decline.
/// Escape window = from drop-end to trial-end (true H fixed at floor).
///
/// GATE QUANTITY IS THE TRUE OVER-DIFFICULTY, NOT BELIEF (verified vs trial.rs).
/// e = ln(current_hashrate_before / true_h)·100. `current_hashrate_before` is the
/// OPERATING POINT (trial.rs:303,335 — the hashrate difficulty is set from, updated
/// ONLY on fires; the belief `h_estimate` is a SEPARATE field, trial.rs:326). So e
/// is a STEP FUNCTION (operating point) over the DETERMINISTIC true schedule — it
/// has ZERO estimator tick-jitter and is construction-independent in noise. A fast
/// estimator causes more FIRES (the mechanism), not e-jitter (would-be gate
/// contamination) — so gating on e>5% counts "fired upward while GENUINELY over-
/// difficult," identical condition across all four constructions. (Had we gated on
/// h_estimate/belief, fast-estimator cells would inflate — we don't.) Corollary: e
/// steps IMMEDIATELY on an upward fire (operating point rises next tick), so the
/// lagged deepening window is unnecessary; tf_up≈tf by construction, verdict rides tf.
fn run(a: &AlgorithmSpec, spm: f32, clean: bool, seed: u64) -> Trial {
    let mature = 60u64;
    let observe = 200u64;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let floor_h = TRUE_HASHRATE * (1.0 - DROP);
    let dm: u64;
    if clean {
        phases.push(Phase::Hold { secs: 60, h: floor_h }); // one-tick instantaneous drop
        dm = 1;
    } else {
        let rate = DECLINE_PPH / 100.0 / 60.0;
        let mut m_used = 0u64;
        for m in 0..600u64 {
            let frac = (rate * (m as f32 + 1.0)).min(DROP);
            phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
            m_used = m + 1;
            if frac >= DROP { break; }
        }
        dm = m_used;
    }
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "x".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let clock = Arc::new(MockClock::new(0));
    let v = (a.factory)(clock.clone());
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let t = run_trial_observed(v, clock, config, &sched, seed);

    // collect the escape trace: per escape tick, (e, fired, belief_raised).
    let mut esc: Vec<(f64, bool, bool)> = Vec::new(); // (e, is_tighten_fire, _)
    let (mut peak, mut breached_sum, mut breached_n) = (f64::MIN, 0.0f64, 0u32);
    let mut ef = 0u32;
    for tk in &t.ticks {
        if tk.t_secs <= d_end || tk.t_secs > trial_end { continue; }
        let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
        let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
        if e > peak { peak = e; }
        if e > 5.0 { breached_sum += e; breached_n += 1; }
        let mut is_tighten = false;
        if tk.fired {
            if let Some(newh) = tk.new_hashrate {
                let raised = (newh as f64) > tk.current_hashrate_before as f64;
                if raised {
                    // GATE: count as the pathology only if ALREADY over-difficult (e>5%)
                    // when it fires — a belief-raising fire from near-equilibrium is a
                    // noise-fire, not escape self-deepening.
                    if e > 5.0 { is_tighten = true; } // gated tighten-fire
                } else if e > 5.0 {
                    ef += 1; // ease-fire while breached (correct escape direction)
                }
            }
        }
        esc.push((e, is_tighten, false));
    }
    // count gated tighten-fires and, for each, whether e DEEPENED within a few-tick
    // window after it (lagged, because at the collapsed rate the e-response lands a
    // few empty ticks later when a share finally arrives).
    const DEEPEN_WIN: usize = 5; // ticks (minutes) to allow a share to arrive + register
    let (mut tf, mut tf_up) = (0u32, 0u32);
    for i in 0..esc.len() {
        if esc[i].1 {
            tf += 1;
            let e_at = esc[i].0;
            let hi = (i + DEEPEN_WIN).min(esc.len() - 1);
            let e_max_after = esc[(i + 1).min(esc.len() - 1)..=hi].iter().map(|x| x.0).fold(f64::MIN, f64::max);
            if e_max_after > e_at { tf_up += 1; } // deepened within the lagged window
        }
    }
    Trial {
        peak_e: peak,
        sustained_e: if breached_n > 0 { breached_sum / breached_n as f64 } else { f64::NAN },
        tighten_fires: tf, ease_fires: ef, tighten_stepped_up: tf_up,
    }
}

fn main() {
    let trials: usize = env::var("VARDIFF_EEM_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
    let seed = DEFAULT_BASELINE_SEED ^ 0xEA6E_A5E;
    let nth: usize = env::var("VARDIFF_EEM_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // (label, tau, tighten_mult, sensitivity)
    let variants: Vec<(&str, u64, f64, f64)> = vec![
        ("champion (Ewma360, reluctant tm8)", 360, 8.0, 1.5),
        ("boundary-jumpy (Ewma360, symm tm1, s0.3)", 360, 1.0, 0.3),
        ("estimator-jumpy (Ewma30, reluctant tm8)", 30, 8.0, 1.5),
        ("both-jumpy (Ewma30, symm tm1, s0.3)", 30, 1.0, 0.3),
    ];
    let stimuli = [("clean-step", true), ("real-decline", false)];

    let jobs: Vec<(usize, usize, usize)> = (0..variants.len()).flat_map(|vi|
        (0..SPMS.len()).flat_map(move |si| (0..stimuli.len()).map(move |ti| (vi, si, ti)))).collect();
    let next = AtomicUsize::new(0);
    // (vi, si, ti, med_peak, med_sustained, med_tf, med_ef, med_tf_up, frac_any_tf)
    let out: Mutex<Vec<(usize, usize, usize, f64, f64, f64, f64, f64, f64)>> = Mutex::new(Vec::new());
    eprintln!("eager-ease-mechanism: {} jobs × {} trials, {} threads.", jobs.len(), trials, nth);
    std::thread::scope(|sc| {
        for _ in 0..nth {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (vi, si, ti) = jobs[j];
                let (_, tau, tm, s) = variants[vi];
                let a = variant(variants[vi].0, tau, tm, s);
                let (spm, clean) = (SPMS[si], stimuli[ti].1);
                let (mut pk, mut su, mut tf, mut ef, mut tfu) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
                let mut any_tf = 0u32;
                for i in 0..trials {
                    let r = run(&a, spm, clean, seed.wrapping_add((j as u64) << 24).wrapping_add(i as u64));
                    pk.push(r.peak_e); if r.sustained_e.is_finite() { su.push(r.sustained_e); }
                    tf.push(r.tighten_fires as f64); ef.push(r.ease_fires as f64);
                    tfu.push(r.tighten_stepped_up as f64);
                    if r.tighten_fires > 0 { any_tf += 1; }
                }
                out.lock().unwrap().push((vi, si, ti, median(pk), median(su), median(tf), median(ef),
                    median(tfu), any_tf as f64 / trials as f64));
                eprintln!("  {} spm{} {} done", variants[vi].0, SPMS[si], stimuli[ti].0);
            });
        }
    });
    let mut raw = out.into_inner().unwrap();
    raw.sort_by_key(|(vi, si, ti, ..)| (*vi, *si, *ti));

    println!("\n## EAGER-EASE MECHANISM — tighten-fires (wrong-direction, self-deepening) during the escape, per construction.");
    println!("TIGHTEN-FIRE = fire raising belief (new>before) ⇒ harder difficulty ⇒ deepens over-difficulty. The reluctant-tighten");
    println!("half should REFUSE these. Mechanism (counts) is resolvable at the collapsed rate where depth-magnitude (σ~6-7%) is not.\n");
    println!("| construction | spm | stimulus | med peak% | med sust% | TIGHTEN-fires | (stepped e UP) | ease-fires | % trials w/ ≥1 tighten |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for (vi, si, ti, pk, su, tf, ef, tfu, frac) in &raw {
        println!("| {} | {} | {} | {:+.0} | {} | {:.0} | {:.0} | {:.0} | {:.0}% |",
            variants[*vi].0, SPMS[*si] as u32, stimuli[*ti].0, pk,
            if su.is_finite() { format!("{:+.0}", su) } else { "—".into() },
            tf, tfu, ef, frac * 100.0);
    }
    println!("\nREAD (per pre-registration):");
    println!("  champion ~0 tighten-fires on BOTH stimuli (reluctant half refuses) = baseline.");
    println!("  If symmetric constructions show tighten-fires-that-step-e-UP on REAL-DECLINE but ~0 on CLEAN-STEP ⇒ reframe-2:");
    println!("    reluctant-tighten is NOISE-SAFETY (invisible on clean step, fires on noisy decline). Then which constructions show it:");
    println!("    ONLY both-jumpy ⇒ SUBSTITUTES (boundary-asym OR estimator-smoothing each suffice alone; champion over-determined).");
    println!("    a single-removal too (boundary-jumpy or estimator-jumpy) ⇒ that mechanism was load-bearing alone, not substitutable.");
    println!("  If ALL constructions ~0 tighten-fires even on real-decline ⇒ reframe-1: eager-ease is pure efficiency, build map on R_escape.");
    println!("  Depth (peak/sust) is the coarser instrument; if symmetric-vs-champion depth difference is within σ~6-7% it's sub-res —");
    println!("  ride the tighten-fire COUNT (event-level, faithful at sparse rate), same as worst-cell recovery rode fire-count not peak.");
}
