//! EAGER-EASE STRENGTH SWEEP — are the two noise-safety mechanisms REGIME-LOCKED
//! COMPLEMENTS or points on a TRADEOFF SURFACE? The on/off decomposition showed
//! non-overlapping coverage AT THE CHAMPION'S SETTINGS; it cannot tell "each covers
//! a regime the other CAN'T" (regime-locked) from "the champion is one point on a
//! tradeoff surface where more of one substitutes for less of the other" (substitutes).
//! This sweep discriminates by varying STRENGTH, not just on/off.
//!
//! ===========================================================================
//! THE DISTINCTION (load-bearing for item 3). The decomposition confirmed both
//! mechanisms INDEPENDENTLY do noise-safety work (boundary-jumpy fires 100% weak-
//! refusable = reluctance-specific; estimator-jumpy fires 100% strong-unrefusable =
//! estimator-specific). But it tested CHAMPION-STRENGTH vs JUMPY (on/off). "Each
//! covers a regime the other CAN'T" is a claim about CAPABILITY (regime-locked); the
//! on/off test only shows the champion's two settings happen to be non-overlapping.
//! estimator-jumpy fails sparse with STRONG fires "because the FAST estimator presents
//! excursions too strong for tm=8 to refuse" — but maybe tm=16/32/∞ WOULD refuse them
//! (then the boundary CAN cover sparse if reluctant enough → SUBSTITUTES). Likewise
//! boundary-jumpy fails dense because tm=1 fires on what Ewma360 presents — maybe
//! Ewma720/1440 smooths it away (then the estimator CAN cover dense if slow enough →
//! SUBSTITUTES). Regime-LOCKED ⟺ NO strength of the other mechanism compensates.
//!
//! THE TWO FAILURE CELLS, each given MORE of the OTHER mechanism:
//!   (A) estimator-jumpy @ SPARSE (Ewma30, sparse rate) — sweep boundary reluctance
//!       tm ∈ {8,16,32,64,128}. If tighten-fires → 0 as tm rises ⇒ a reluctant-enough
//!       boundary DOES cover sparse rate ⇒ TRADEOFF (boundary substitutes for estimator).
//!       If they persist at tm=128 ⇒ the boundary CANNOT refuse the fast estimator's
//!       sparse excursions at any reluctance ⇒ REGIME-LOCKED.
//!   (B) boundary-jumpy @ DENSE (symmetric tm=1, dense rate) — sweep estimator window
//!       τ ∈ {360,720,1440,2880}. If tighten-fires → 0 as τ rises ⇒ a slow-enough
//!       estimator DOES cover dense rate ⇒ TRADEOFF (estimator substitutes for boundary).
//!       If they persist at τ=2880 ⇒ smoothing CANNOT suppress what the symmetric
//!       boundary fires on at any window ⇒ REGIME-LOCKED.
//!
//! ITEM-3 CONSEQUENCE (why the framing waits on this):
//!   TRADEOFF SURFACE ⇒ noise-safety is ONE requirement (suppress wrong-direction
//!     firing) met by trading mechanism strengths; the champion is one point; the
//!     guard's persistence-discount is ANOTHER point on the surface (closer to
//!     SUBSTITUTES) — NOT a third necessary mechanism.
//!   REGIME-LOCKED ⇒ each mechanism does something the other categorically can't at
//!     any strength; you need the set; persistence-discount is plausibly a THIRD
//!     NECESSARY regime-specific mechanism.
//! The decomposition can't say which; this sweep can.
//!
//! PRE-REGISTERED: for each failure cell, tighten-fires vs the OTHER mechanism's
//! strength. Monotone → 0 ⇒ that mechanism substitutes (tradeoff); plateau > 0 at
//! max strength ⇒ regime-locked. Report BOTH cells — the answer could be mixed (one
//! substitutes, one locked), which is itself informative (asymmetric tradeoff).
//! Sparse-corner caveat: at very sparse rate few fires to begin with; read the
//! direction (falling vs flat), and flag if the cell is too quiet to discriminate.
//!
//! Usage: cargo run --release --bin eager-ease-strength
//! Env: VARDIFF_EES_TRIALS (default 300), VARDIFF_EES_THREADS.
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

const DECLINE_PPH: f32 = 40.0;
const DROP: f32 = 0.7;
const SPARSE_SPM: f32 = 2.0; // cell A rate (estimator-jumpy's failure regime)
const DENSE_SPM: f32 = 6.0;  // cell B rate (boundary-jumpy's failure regime)
// Cell A: fast estimator fixed, sweep boundary reluctance UP.
const TMS: &[f64] = &[8.0, 16.0, 32.0, 64.0, 128.0];
// Cell B: symmetric boundary fixed, sweep estimator window UP (slower).
const TAUS: &[u64] = &[360, 720, 1440, 2880];

fn variant(tau: u64, tm: f64, s: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}/tm{tm}/s{s}"), move |clock| {
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

/// upward (self-deepening) gated tighten-fires during a real-decline escape — same
/// gate as eager-ease-mechanism (upward fire while genuinely over-difficult, e from
/// operating-point-vs-true, noise-free). Returns (tighten_fires, ease_fires).
fn run(a: &AlgorithmSpec, spm: f32, seed: u64) -> (u32, u32) {
    let mature = 60u64;
    let observe = 200u64;
    let rate = DECLINE_PPH / 100.0 / 60.0;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..600u64 {
        let frac = (rate * (m as f32 + 1.0)).min(DROP);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= DROP { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - DROP);
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "x".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let clock = Arc::new(MockClock::new(0));
    let v = (a.factory)(clock.clone());
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let t = run_trial_observed(v, clock, config, &sched, seed);
    let (mut tf, mut ef) = (0u32, 0u32);
    for tk in &t.ticks {
        if tk.t_secs <= d_end || tk.t_secs > trial_end { continue; }
        let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
        let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
        if !tk.fired || e <= 5.0 { continue; }
        if let Some(newh) = tk.new_hashrate {
            if (newh as f64) > tk.current_hashrate_before as f64 { tf += 1; } else { ef += 1; }
        }
    }
    (tf, ef)
}

fn main() {
    let trials: usize = env::var("VARDIFF_EES_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed = DEFAULT_BASELINE_SEED ^ 0x57_4E_46;
    let nth: usize = env::var("VARDIFF_EES_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // Cell A jobs: (0, i) over TMS; Cell B jobs: (1, i) over TAUS.
    let jobs: Vec<(u8, usize)> = (0..TMS.len()).map(|i| (0u8, i))
        .chain((0..TAUS.len()).map(|i| (1u8, i))).collect();
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(u8, usize, f64, f64)>> = Mutex::new(Vec::new());
    eprintln!("eager-ease-strength: cell A {} tm × sparse + cell B {} tau × dense, {} trials, {} threads.",
        TMS.len(), TAUS.len(), trials, nth);
    std::thread::scope(|sc| {
        for _ in 0..nth {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (cell, i) = jobs[j];
                // Cell A: fast estimator (Ewma30) + jumpy sensitivity (s0.3) + sweep tm UP at SPARSE.
                // Cell B: symmetric boundary (tm1) + jumpy sensitivity (s0.3) + sweep tau UP at DENSE.
                let (a, spm) = if cell == 0 {
                    (variant(30, TMS[i], 0.3), SPARSE_SPM)
                } else {
                    (variant(TAUS[i], 1.0, 0.3), DENSE_SPM)
                };
                let (mut tfs, mut efs) = (Vec::new(), Vec::new());
                for k in 0..trials {
                    let (tf, ef) = run(&a, spm, seed.wrapping_add((j as u64) << 24).wrapping_add(k as u64));
                    tfs.push(tf as f64); efs.push(ef as f64);
                }
                out.lock().unwrap().push((cell, i, median(tfs), median(efs)));
                eprintln!("  cell{} i{} done", cell, i);
            });
        }
    });
    let mut raw = out.into_inner().unwrap();
    raw.sort_by_key(|(c, i, ..)| (*c, *i));

    println!("\n## STRENGTH SWEEP — does MORE of the OTHER mechanism drive the failure cell's tighten-fires to ZERO?");
    println!("CELL A: estimator-jumpy (Ewma30) at SPARSE spm{}, sweeping BOUNDARY RELUCTANCE tm UP. → 0 ⇒ boundary substitutes (tradeoff).", SPARSE_SPM as u32);
    println!("| tighten metric | {} |", TMS.iter().map(|t| format!("tm{}", *t as u32)).collect::<Vec<_>>().join(" | "));
    print!("| upward-fires (sparse) |");
    for i in 0..TMS.len() { print!(" {:.0} |", raw.iter().find(|(c, j, ..)| *c==0 && *j==i).unwrap().2); }
    println!();
    print!("| (ease-fires, context) |");
    for i in 0..TMS.len() { print!(" {:.0} |", raw.iter().find(|(c, j, ..)| *c==0 && *j==i).unwrap().3); }
    println!();

    println!("\nCELL B: boundary-jumpy (symmetric tm1) at DENSE spm{}, sweeping ESTIMATOR WINDOW tau UP (slower). → 0 ⇒ estimator substitutes.", DENSE_SPM as u32);
    println!("| tighten metric | {} |", TAUS.iter().map(|t| format!("τ{}", t)).collect::<Vec<_>>().join(" | "));
    print!("| upward-fires (dense) |");
    for i in 0..TAUS.len() { print!(" {:.0} |", raw.iter().find(|(c, j, ..)| *c==1 && *j==i).unwrap().2); }
    println!();
    print!("| (ease-fires, context) |");
    for i in 0..TAUS.len() { print!(" {:.0} |", raw.iter().find(|(c, j, ..)| *c==1 && *j==i).unwrap().3); }
    println!();

    let a_first = raw.iter().find(|(c, j, ..)| *c==0 && *j==0).unwrap().2;
    let a_last = raw.iter().find(|(c, j, ..)| *c==0 && *j==TMS.len()-1).unwrap().2;
    let b_first = raw.iter().find(|(c, j, ..)| *c==1 && *j==0).unwrap().2;
    let b_last = raw.iter().find(|(c, j, ..)| *c==1 && *j==TAUS.len()-1).unwrap().2;
    println!("\n**READ:**");
    println!("  Cell A: tighten-fires {:.0} (tm8) → {:.0} (tm128). {}", a_first, a_last,
        if a_last < 1.0 { "→0: a reluctant-enough boundary COVERS sparse ⇒ boundary SUBSTITUTES for estimator (tradeoff)." }
        else if a_last < a_first * 0.5 { "FALLING but >0: partial substitution; reluctance helps but doesn't fully cover sparse at tested strength." }
        else { "PLATEAU >0: no reluctance refuses the fast estimator's sparse excursions ⇒ REGIME-LOCKED (boundary can't cover sparse)." });
    println!("  Cell B: tighten-fires {:.0} (τ360) → {:.0} (τ2880). {}", b_first, b_last,
        if b_last < 1.0 { "→0: a slow-enough estimator COVERS dense ⇒ estimator SUBSTITUTES for boundary (tradeoff)." }
        else if b_last < b_first * 0.5 { "FALLING but >0: partial substitution; smoothing helps but doesn't fully cover dense at tested strength." }
        else { "PLATEAU >0: no smoothing suppresses what the symmetric boundary fires on ⇒ REGIME-LOCKED (estimator can't cover dense)." });
    println!("\n  BOTH →0 ⇒ TRADEOFF SURFACE (substitutes; champion is one point; persistence-discount is another point, NOT a 3rd necessary");
    println!("  mechanism). BOTH plateau ⇒ REGIME-LOCKED complements (need the set; persistence-discount plausibly a 3rd necessary mechanism).");
    println!("  MIXED (one →0, one plateau) ⇒ ASYMMETRIC: one mechanism substitutes, the other is regime-locked — itself a finding.");
    println!("  (If a cell's tm8/τ360 baseline is already near 0, it's too quiet to discriminate — note sub-resolution, don't over-read.)");
}
