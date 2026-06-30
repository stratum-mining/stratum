//! WHICH-BOUNDARY — is the active boundary at sparse rate (spm<6) PoissonCI or
//! SignPersistenceCusum? Direct probe: AdaptiveSignPersist switches on configured
//! spm (boundary.rs:1023, spm<spm_threshold ⇒ PoissonCI fallback). If spm2 uses
//! PoissonCI, then M3 (committed 9518f3bf, "ease-branch keying of SignPersistenceCusum")
//! AND the guard-spiral "persistence-discount" attribution are MIS-ATTRIBUTED — both
//! reference a boundary not active at sparse rate. Behavioral results survive; the
//! MECHANISM naming is wrong. Verify before correcting the record.
//!
//! Method: call AdaptiveSignPersist's threshold at spm2 and spm6 and compare to a
//! bare PoissonCI threshold at the same inputs. Equal at spm2 ⇒ PoissonCI active
//! there (M3 mis-attributed). Differs at spm6 ⇒ SignPersistenceCusum active there.

use channels_sv2::vardiff::composed::{
    AdaptiveSignPersist, Boundary, PoissonCI, SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::composed::estimator::EstimatorSnapshot;

fn snap(realized: f64) -> EstimatorSnapshot {
    EstimatorSnapshot { h_estimate: 1.0e15, realized_share_per_min: realized, n_shares: realized as u32, dt_secs: 60, uncertainty: None }
}

fn main() {
    // champion's high boundary params (s=1.5, tm=8) + the spm6 guard.
    let adaptive = AdaptiveSignPersist::sign_persist(
        SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6), 6,
    );
    let poisson = PoissonCI::default_parametric();

    println!("# WHICH-BOUNDARY — active boundary vs configured spm (switch at spm_threshold=6)\n");
    println!("| spm | dt | AdaptiveSignPersist θ | bare PoissonCI θ | equal? ⇒ active boundary |");
    println!("| --- | --- | --- | --- | --- |");
    for &spm in &[2.0f32, 4.0, 6.0, 12.0, 30.0] {
        for &dt in &[60u64, 180, 420] {
            let a = adaptive.threshold(dt, spm, &snap(spm as f64 * 0.5)); // under-performing (escape-like)
            let p = poisson.threshold(dt, spm, &snap(spm as f64 * 0.5));
            let eq = (a - p).abs() < 1e-9;
            println!("| {} | {} | {:.1} | {:.1} | {} |", spm as u32, dt, a, p,
                if eq { "EQUAL ⇒ PoissonCI active" } else { "differs ⇒ SignPersist active" });
        }
    }
    println!("\nREAD: if spm<6 rows are EQUAL to bare PoissonCI ⇒ the sparse boundary IS PoissonCI (symmetric, no tighten_multiplier),");
    println!("so M3's 'ease-branch keying of SignPersistenceCusum' is mis-attributed (that boundary isn't active at sparse rate) — the");
    println!("real reason tm-sweep does nothing at sparse is tm lives in SignPersistenceCusum, switched OFF below spm6. And the guard-");
    println!("spiral 'persistence-discount anti-spiral' is PoissonCI's dt-accumulation (θ falls as dt grows: (z√λ̄+0.5)/λ̄→0),");
    println!("NOT a persistence-discount. Behavioral findings survive; mechanism attributions need correction.");
}

// ===========================================================================
// RESULT — CONFIRMED, and the corrected mechanism is VERIFIED (Possibility 1),
// not re-read off threshold-falling. Two findings:
//
// (1) THE MISATTRIBUTION (this probe). spm2/4 thresholds are EXACTLY bare PoissonCI
//     (212.2/118.5/77.4 — matching the Cell-A trace's 42–118%); spm6+ differ. The
//     AdaptiveSignPersist switch (spm_threshold=6, boundary.rs:1023) means SPARSE rate
//     runs PoissonCI (symmetric, NO tighten_multiplier), NOT SignPersistenceCusum. So:
//       - M3 (committed 9518f3bf) named SignPersistenceCusum's ease-branch keying for
//         why tm-sweep does nothing at sparse. WRONG why: tm lives in SignPersistence-
//         Cusum, SWITCHED OFF below spm6. Right behavioral conclusion (reluctance can't
//         cover sparse) — and STRONGER (it's switched off BY DESIGN; the spm6 guard
//         exists because SignPersistenceCusum "collapses at low SPM", boundary.rs:1083).
//       - guard-spiral (committed 43fa3216) named "persistence-discount anti-spiral".
//         WRONG: guard regime is spm<6 → PoissonCI → no persistence-discount. The
//         falling threshold is PoissonCI's dt-accumulation ((z√λ̄+0.5)/λ̄ falls as dt
//         grows between fires — 212→118→77 above). Behavioral result (self-corrects,
//         no cliff) survives; the mechanism NAME was wrong.
//
// (2) THE CORRECTED MECHANISM — VERIFIED FROM SOURCE (composed.rs), not threshold-
//     falling. Possibility 1, the cleaner picture:
//       - FIRE DIRECTION is set by the ESTIMATOR, not the boundary. delta =
//         |h_estimate/hashrate − 1|·100 (magnitude, direction-blind, composed.rs:128);
//         PoissonCI threshold is magnitude, ignores snap (boundary.rs:160); fire iff
//         delta≥threshold; the update moves the operating point TOWARD h_estimate. So
//         direction = sign(h_estimate − hashrate) — the estimator's belief vs the
//         operating point. During over-difficulty the belief sits below the stale-high
//         operating point → fires go EASE. A self-deepening TIGHTEN fire happens only
//         when a noise up-spike flips belief above the operating point.
//       - PoissonCI (sparse) is a SYMMETRIC MAGNITUDE TRIGGER: it does NOT refuse
//         tighten-fires by direction (no would_tighten branch). Its dt-accumulation
//         TIMES fires; it provides NO dangerous-direction protection. So "PoissonCI
//         provides sparse protection" is REFUTED by source — it triggers, it doesn't
//         protect.
//       - SPARSE dangerous-direction protection = the ESTIMATOR'S, entirely, two ways:
//         it sets the fire DIRECTION (belief-vs-operating-point) AND (slow Ewma360)
//         doesn't present the up-spikes that would flip belief above the operating
//         point. SignPersistenceCusum's tm (dense) refuses weak-evidence tighten-fires
//         by direction — the DENSE protector.
//     ⇒ TWO protection mechanisms (estimator/sparse, boundary-reluctance/dense) + a
//        sparse symmetric TRIGGER (PoissonCI). NOT "three mechanisms." Item 1's
//        asymmetric-substitutability SHARPENS: at sparse rate the boundary side is
//        just a trigger with no protective content; all sparse protection is the
//        estimator's.
//     ⇒ ITEM 3 dissolved for the sparse question: persistence-discount is a DENSE
//        boundary refinement (switched off sparse), and the sparse boundary gives no
//        direction protection regardless — so "is persistence-discount share-sign-keyed"
//        is moot for sparse; the sparse protector is the estimator, full stop.
//
// CARRY-FORWARD: the catch (M3 named a switched-off boundary) was found by the same
// is-this-real discipline applied to OWN committed work, via which boundary is active
// (this probe). The CORRECTION's positive claim was then held to the SAME standard:
// not re-read off threshold-falling (the signal that misled M3), but verified from the
// fire-direction source (delta magnitude + update-toward-belief ⇒ direction is the
// estimator's). Don't let a correction repeat the error it corrects.
// ===========================================================================
