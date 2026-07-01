//! CELL-A PLATEAU MECHANISM — is the estimator's sparse-rate non-substitutability
//! CATEGORICAL (Mechanism 1: unbounded excursions, no threshold refuses them) or
//! OPERATIONAL (Mechanism 2: finite-but-large excursions, the 4 are just the
//! strongest, impractical reluctance would eventually refuse them)? Two reads.
//!
//! ===========================================================================
//! WHY (the item-3 fork). The strength sweep found Cell A (estimator-jumpy @ sparse)
//! FLAT at 4 upward-fires across tm 8→128 (16×): the slow estimator is non-
//! substitutable at sparse rate. But the plateau has two mechanisms with DIFFERENT
//! item-3 consequences:
//!   M1 (threshold-outrun): the fast Ewma30 presents excursions UNBOUNDED in
//!      magnitude (a single share in a near-empty window moves the estimate
//!      arbitrarily far — as the window empties, one arrival's weight → the whole
//!      estimate). No FINITE threshold refuses them ⇒ count stays 4 at tm=256,512,∞
//!      ⇒ the boundary CATEGORICALLY cannot cover sparse ⇒ persistence-discount
//!      (item 3) must work by NOT-PRESENTING extreme excursions (smoothing-like),
//!      NOT by being a better boundary.
//!   M2 (count-saturation): the 4 are finite-but-strong (reluctance refuses all
//!      weaker; these 4 sit above tm=128 but below tm=∞) ⇒ count CRACKS at tm=256/512
//!      ⇒ the boundary CAN cover sparse at impractical strength ⇒ persistence-discount
//!      COULD be a boundary-route (smarter boundary), broader item-3 setup.
//!
//! TWO READS (both, because they confirm each other):
//!   (a) EXTENSION: tm ∈ {8,128,256,512,1024}. Stays 4 ⇒ M1; cracks ⇒ M2.
//!   (b) TRACE: for the upward fires, log delta (evidence) and the tm=128 threshold,
//!       and the ratio delta/threshold. delta/thr UNBOUNDED (many×, growing) ⇒ M1
//!       (excursions categorically past any threshold). delta/thr FINITE (just above 1,
//!       characterizable) ⇒ M2 (a bit more reluctance refuses them). Also log the
//!       belief swing (h_estimate vs operating point) — M1 predicts large multiples.
//!   Both agree (flat + unbounded) ⇒ M1 confirmed two ways. The mechanism story
//!   PREDICTS M1 (single-share-in-thin-window is mathematically unbounded).
//!
//! Usage: cargo run --release --bin cellA-mechanism
//! Env: VARDIFF_CAM_TRIALS (default 300).
//! ===========================================================================
//!
//! ===========================================================================
//! RESULT — the plateau is M3 (WRONG-KNOB), source-confirmed, and it RESOLVES the
//! M1/M2 conflict rather than leaving it unresolved. The two diagnostics looked
//! contradictory — EXTENSION flat at 4 through tm 8→1024 (looked like M1), TRACE
//! delta/threshold only 1.0–1.8× with modest 1.1–1.4× belief swings (refuted M1's
//! unbounded excursions, looked like M2) — but jointly they triangulate a third
//! mechanism neither M1 nor M2: TM IS NOT IN THE GATE FOR THESE FIRES.
//!
//! THE SOURCE-READ (boundary.rs:538–546, a lookup not a sim). The tighten multiplier
//! scales the threshold on EXACTLY ONE branch:
//!     would_tighten = realized_share_per_min > target   // miner OVER-performing
//!     threshold = if would_tighten { base * tighten_multiplier } else { base }
//! tm is absent from the ease branch. The trace's fires face threshold ≈ 42–118%
//! (= the un-multiplied ease `base` fraction, sensitivity/n + uncertainty floor);
//! the tighten branch at tm=128 would be ×128 ≈ 5,000–15,000%, NOWHERE in the trace.
//! So these fires are on the EASE branch — tm is not in their gate. CONFIRMED TWO
//! WAYS: (a) invariance-to-1024× — a tm-gated fire CANNOT be tm-invariant, so the
//! invariance itself proves tm is absent from the gate; (b) threshold-magnitude —
//! 42–118% is the un-multiplied ease threshold, not ×128. M1 dead (swings modest,
//! not unbounded), M2 dead (invariance — M2 predicts the count drops as tm rises).
//!
//! WHY (the mechanism, and it is DIRECTION-SPECIFIC — do NOT overgeneralize to "tm
//! never gates"). `would_tighten` keys on realized-vs-target SHARES. During an
//! over-difficulty escape the miner UNDER-performs (difficulty too high → too few
//! shares → realized < target) → would_tighten = FALSE → ease branch. So a
//! belief-spike fire that RAISES difficulty (self-deepening) is gated by the EASE
//! threshold, because the SHARE SIGN reads "under-performing." The reluctant-tighten
//! asymmetry is keyed on a sign that reads "ease" during the very over-difficulty it
//! would need to protect against — so tm CANNOT reach these fires at any strength.
//! NOT "reluctance insufficient" (M1); NOT "tm never gates" — the tighten branch IS
//! reached and IS protective when the miner OVER-performs (realized > target: a
//! genuine hashrate increase or under-difficulty wobble, where reluctance correctly
//! refuses to tighten on a transient). So: reluctance protects the SAFE direction
//! (over-performance → would-tighten → refused) and is STRUCTURALLY ABSENT in the
//! DANGEROUS direction (under-performance during over-difficulty → ease branch).
//! This is a DEEPER statement of the arc's asymmetry theme: e^(−e) makes
//! over-difficulty the dangerous direction, and the boundary's reluctance mechanism
//! is keyed on the wrong SIGN to help there.
//!
//! ITEM-1 COMPLETION. The validity axis is DANGEROUS-DIRECTION PROTECTION (suppress
//! self-deepening in over-difficulty) — the earlier reframe — and M3 gives it the
//! mechanism: that protection is provided by the BOUNDARY ASYMMETRY at DENSE rate and
//! NECESSARILY by the ESTIMATOR at SPARSE rate, because the boundary's reluctance is
//! keyed on the realized-vs-target sign that reads "ease" during a sparse escape and
//! so is structurally unable to provide it there. Eager-ease-as-boundary-reluctance
//! is therefore NOT the universal validity mechanism — it is the DENSE-rate mechanism;
//! the estimator (not-presenting the belief-spike) carries the sparse-rate
//! dangerous-direction protection. Two handles: distinct mechanisms, asymmetrically
//! substitutable (estimator non-substitutable sparse, boundary substitutable dense),
//! AND the boundary handle is structurally regime-limited (keyed on realized-vs-target).
//!
//! ITEM-3 HANDOFF (precise, falsifiable). The sparse-rate primary function (suppress
//! self-deepening fires during escape) CANNOT be done by ANY realized-vs-target-keyed
//! reluctance, because that sign reads "ease" during over-difficulty. So persistence-
//! discount can serve as a sparse-rate mechanism IFF it is keyed on something OTHER
//! than the realized-vs-target share sign (e.g. belief-trajectory or depth). If
//! persistence-discount is share-sign-keyed, it is on the wrong branch during escapes
//! and will NOT help at sparse rate. The specific check for item 3: WHAT GATES
//! persistence-discount — share-sign (→ won't help) or else (→ could).
//!
//! CARRY-FORWARD (the framing lesson). The M1/M2 "conflict" RESOLVED rather than
//! staying sub-resolution, because one diagnostic (invariance-to-1024×) had a LOGICAL
//! implication the other was consistent with: a tm-gated fire cannot be tm-invariant,
//! so the invariance WAS data — it told us tm is absent from the gate — and the
//! source-read named the branch. Reading what the conflict IMPLIED beat both the
//! prettier story (M1, refuted) and the flagged-unresolved fallback. A conflict
//! between diagnostics can be a SIGNATURE, not noise. And the discriminating quantity
//! was WHICH BRANCH gates the fires (a source-read), not the fire MAGNITUDE (the wrong
//! quantity, which is what made M1-vs-M2 look unresolvable) — the same right-quantity
//! discipline, now on the mechanism instrument itself.
//!
//! STATUS: M3 source-confirmed. Item 1 answered (validity = dangerous-direction
//! protection; eager-ease-reluctance is the dense mechanism, estimator the sparse one;
//! boundary structurally regime-limited by realized-vs-target keying). Item 3 set up
//! falsifiably (persistence-discount helps sparse iff not share-sign-keyed).
//! Sim-validated; the asymmetry is a STRUCTURAL/source fact (the keying), not a
//! sample-resolution claim.
//! ===========================================================================

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::schedule::HashrateSchedule;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const DECLINE_PPH: f32 = 40.0;
const DROP: f32 = 0.7;
const SPARSE_SPM: f32 = 2.0;
const TMS: &[f64] = &[8.0, 128.0, 256.0, 512.0, 1024.0]; // extension past 128

fn variant(tau: u64, tm: f64, s: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}/tm{tm}"), move |clock| {
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

/// build a real-decline escape and return its tick records + d_end/trial_end.
fn escape(a: &AlgorithmSpec, spm: f32, seed: u64)
    -> (vardiff_sim::trial::Trial, HashrateSchedule, u64, u64) {
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
    (t, sched, d_end, trial_end)
}

/// count upward gated tighten-fires in one escape.
fn count_upward(a: &AlgorithmSpec, spm: f32, seed: u64) -> u32 {
    let (t, sched, d_end, trial_end) = escape(a, spm, seed);
    let mut tf = 0u32;
    for tk in &t.ticks {
        if tk.t_secs <= d_end || tk.t_secs > trial_end { continue; }
        let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
        let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
        if !tk.fired || e <= 5.0 { continue; }
        if let Some(newh) = tk.new_hashrate {
            if (newh as f64) > tk.current_hashrate_before as f64 { tf += 1; }
        }
    }
    tf
}

fn main() {
    let trials: usize = env::var("VARDIFF_CAM_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed = DEFAULT_BASELINE_SEED ^ 0xCE11_A;

    // --- READ (a): EXTENSION tm 8→1024 at sparse, fast estimator ---
    println!("# CELL-A PLATEAU MECHANISM — M1 (categorical/unbounded) vs M2 (operational/finite)\n");
    println!("## READ (a) EXTENSION — median upward-fires vs boundary reluctance tm, Ewma30 @ sparse spm{}.", SPARSE_SPM as u32);
    print!("| tm |"); for &tm in TMS { print!(" {} |", tm as u32); } println!();
    print!("| --- |"); for _ in TMS { print!(" --- |"); } println!();
    print!("| upward-fires |");
    let mut last = 0.0;
    for &tm in TMS {
        let a = variant(30, tm, 0.3);
        let counts: Vec<f64> = (0..trials).map(|i| count_upward(&a, SPARSE_SPM, seed.wrapping_add(i as u64)) as f64).collect();
        let m = median(counts);
        print!(" {:.0} |", m);
        last = m;
    }
    println!();
    let first_a = { let a = variant(30, 8.0, 0.3);
        median((0..trials).map(|i| count_upward(&a, SPARSE_SPM, seed.wrapping_add(i as u64)) as f64).collect()) };
    println!("\n  EXTENSION verdict: tm8={:.0} → tm1024={:.0}. {}", first_a, last,
        if last >= first_a - 0.5 { "FLAT through 1024× ⇒ M1 (unbounded excursions, no finite threshold refuses) — categorical." }
        else if last < 1.0 { "CRACKED to ~0 ⇒ M2 (finite excursions, impractical reluctance refuses them) — operational." }
        else { "PARTIAL drop ⇒ between M1/M2; some refusable, a core persists — read the trace for the core's magnitude." });

    // --- READ (b): TRACE the upward fires' evidence magnitude at tm=128 ---
    // find a representative seed whose escape has upward fires, log delta/threshold/belief-swing.
    println!("\n## READ (b) TRACE — upward fires' evidence magnitude at tm=128 (delta vs threshold; belief swing).");
    println!("M1 ⇒ delta/threshold UNBOUNDED (many×, the excursion is categorically past any threshold) + belief swung large multiples.");
    println!("M2 ⇒ delta/threshold FINITE (just above 1, a bit more reluctance refuses) + belief swing bounded.\n");
    let a = variant(30, 128.0, 0.3);
    println!("| seed off | t(min) | e% | delta | threshold | delta/thr | belief swing (new/before) |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    let mut shown = 0;
    let mut ratios: Vec<f64> = Vec::new();
    for soff in 0..trials as u64 {
        if shown >= 12 { break; }
        let (t, sched, d_end, trial_end) = escape(&a, SPARSE_SPM, seed.wrapping_add(soff));
        for tk in &t.ticks {
            if tk.t_secs <= d_end || tk.t_secs > trial_end { continue; }
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            if !tk.fired || e <= 5.0 { continue; }
            let newh = match tk.new_hashrate { Some(h) => h as f64, None => continue };
            if newh <= tk.current_hashrate_before as f64 { continue; }
            let (d, thr) = (tk.delta.unwrap_or(f64::NAN), tk.threshold.unwrap_or(f64::NAN));
            let ratio = d / thr;
            ratios.push(ratio);
            if shown < 12 {
                println!("| {} | {} | {:+.0} | {:.0} | {:.0} | {:.1}× | {:.2} |",
                    soff, tk.t_secs/60, e, d, thr, ratio, newh / tk.current_hashrate_before as f64);
                shown += 1;
            }
        }
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med_ratio = if ratios.is_empty() { f64::NAN } else { ratios[ratios.len()/2] };
    let max_ratio = ratios.iter().cloned().fold(f64::MIN, f64::max);
    println!("\n  TRACE verdict: median delta/threshold = {:.1}×, max = {:.1}× (over {} upward fires at tm=128).", med_ratio, max_ratio, ratios.len());
    println!("  {}", if med_ratio > 3.0 {
        "delta is MANY× the (already 128×-reluctant) threshold ⇒ M1: the excursions are categorically past any threshold — UNBOUNDED.\n  ⇒ estimator-smoothing is the IRREPLACEABLE sparse primary BY NECESSITY (no boundary refuses unbounded excursions); persistence-\n  discount (item 3) must suppress excursion PRESENTATION (smoothing-like), NOT be a better boundary."
    } else {
        "delta is only modestly above the 128×-reluctant threshold ⇒ M2: finite-but-large; more reluctance would refuse ⇒ non-\n  substitutable at PRACTICAL strength only; persistence-discount COULD be a boundary-route. State the claim operationally, not categorically."
    });
}
