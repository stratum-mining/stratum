//! DEPLOYMENT-COUPLING SWEEP — does the *deployment* optimum's window slide with
//! rate, or is the fixed champion (τ=360) already ~right at every rate?
//!
//! ===========================================================================
//! WHY (the question that gates everything downstream of the share-indexed study,
//! a83be52a). That study established — on the OVER-DIFFICULTY axis — that the
//! per-rate optimum slides GENTLER than 1/r, so the optimal rate-coupling is
//! intermediate (window ∝ 1/r^p, p<1), with fixed (p=0) and constant-share (p=1)
//! both wrong endpoints. BUT that is the over-difficulty axis. The window you
//! actually DEPLOY is not the over-difficulty optimum — it is the GENTLEST-
//! ADMISSIBLE window: the champion was selected by CONSTRAINT-THEN-ORDER (gate is
//! binary admissibility; wobble orders within it; NO scalar weighting — §9.3,
//! constraint-not-cost). The deployment optimum's rate-coupling is a DIFFERENT
//! curve, and it was never measured. This binary measures it.
//!
//! THE DECISION THIS FORCES:
//!   (A) τ_deploy(r) ≈ 360 flat across rate  ⇒ the champion is already ~right at
//!       every rate; deployment rate-awareness buys ~nothing; the p<1 finding is an
//!       OVER-DIFFICULTY-AXIS curiosity that does not change what ships. STOP.
//!   (B) τ_deploy(r) SLIDES beyond the bars ⇒ a fixed window leaves gentleness on
//!       the table at the non-binding rates; rate-awareness has a real DEPLOYMENT
//!       gain; the SIGN of the slide says which way to couple (see PREDICTIONS).
//!       Only then is the expensive p-pinning (wall 2: sub-tick estimator) worth it.
//!
//! WHY NO NEW RIG, AND WHY THIS ESCAPES THE SUB-TICK WALL (the structural point
//! that makes this the cheap, high-information move). The deployment optimum is the
//! GENTLEST-admissible window = the LONGEST τ that still clears the decline-safety
//! gate (because wobble decreases monotonically toward longer τ — VERIFIED below,
//! not assumed; and the gate bounds τ from ABOVE — sleepy lag → settled over-
//! difficulty). So τ_deploy lives at the LONG-τ end of the axis, which is fully
//! REPRESENTABLE. Only the over-difficulty optimum lived at the SHORT-τ, sub-tick
//! end (the wall that blocked pinning p). Opposite ends of the same axis — this
//! question is answerable on the existing 60s-tick rig precisely because its
//! optimum is long, not short.
//!
//! METHOD — commensurate WITH the committed §8.3 numbers BY CONSTRUCTION (reuses
//! the decline profile + both metrics verbatim from tau-family-safety.rs (gate) and
//! tau-family.rs (wobble), so τ_deploy is the champion's OWN selection criterion,
//! per rate, not a new objective):
//!   · GATE (admissibility, binary): worst-over-severity settled-e% ≤ 5%. settled-e
//!     = last e in the post-decline recovery window. Longer τ → more settled over-
//!     difficulty (sleepy lag) → the gate bounds τ from ABOVE. (tau-family-safety.)
//!   · WOBBLE (ordering within the admissible set): |settled-e| at the worst-AREA
//!     severity — the −6…−8%@360 vs −14…−23%@τ*=30 quantity. Shorter τ → twitchier
//!     → more wobble; wobble wants τ LONG. (tau-family.)
//!   · τ_deploy(spm) = the GENTLEST-ADMISSIBLE window = the LONGEST τ on the grid
//!     whose gate passes. (Constraint-then-order; NO scalar — the gate is the
//!     ceiling, wobble breaks ties by preferring long, so the longest admissible
//!     window IS the constraint-then-order optimum, given wobble↓ in τ.)
//!
//! SCOPE — spm ≥ 6 (single control regime). The spm_threshold=6 guard switch makes
//! spm<6 vs ≥6 different controllers; a coupling SLOPE is a within-regime question
//! (same scoping as tau-family / tau-optimum-fit). Guard regime {2,4} run but
//! reported SEPARATELY, never stitched into the slope.
//!
//! τ GRID — REPRESENTABLE ONLY (≥ 60s tick); NO sub-tick. The deployment optimum is
//! at the long end (gate boundary), so this grid brackets it from BOTH sides as
//! long as the gate binds at < 900s. If τ_deploy rails at the grid's LONG edge
//! (900), the gate has even more headroom and the slide is even stronger — recorded
//! as an off-grid UPPER bound (the benign direction; unlike the over-difficulty
//! study's short-edge railing, a long-edge rail does not hide a sub-tick optimum).
//!
//! PRE-REGISTERED PREDICTIONS (locked before numbers):
//!   P1 (the gate binds from above — a construction check): for each spm, gate-pass
//!      holds at SHORT τ and FAILS at long τ (sleepy lag → settled over-difficulty
//!      > 5%), so there is a well-defined longest-admissible τ_deploy. FAILS IF the
//!      gate is non-monotone in τ (would mean settled over-difficulty is not a
//!      clean sleepy-lag effect — needs explaining, not a τ_deploy).
//!   P2 (wobble↓ in τ — the ordering check that makes gentlest = longest): wobble
//!      decreases monotonically toward longer τ across the representable grid, so
//!      "gentlest-admissible" = "longest-admissible." FAILS IF wobble bottoms out
//!      mid-grid and rises again (then gentlest-admissible is an interior wobble-min
//!      under the gate, not the boundary — reported as such, not forced to the edge).
//!   P3 (THE QUESTION — deployment coupling): τ_deploy(spm) is reported across the
//!      boundary regime. Outcome (A) flat ≈360 within bars ⇒ champion already right,
//!      deployment rate-awareness ~no gain. Outcome (B) slides beyond bars ⇒ real
//!      deployment gain. The SIGN matters and is NOT pre-judged:
//!        · slides SHORTER at high rate (τ_deploy↓ as spm↑) ⇒ same DIRECTION as
//!          share-indexing — the gate, like over-difficulty, tightens the admissible
//!          window at high rate; deployment coupling is toward-shorter (some p>0).
//!        · slides LONGER at high rate (τ_deploy↑ as spm↑) ⇒ OPPOSITE to share-
//!          indexing — dense shares give the gate more headroom, so you can afford a
//!          SLEEPIER (lower-wobble) window at high rate; deployment coupling is
//!          NEGATIVE-p. This would mean share-indexing couples the WRONG WAY for
//!          deployment, a strong and surprising result. Pre-registered so a flat or
//!          opposite slide is read as the finding, not as noise.
//!   P4 (the champion's own placement — control): report the champion τ=360's gate
//!      margin per spm (how far below 5%). §8.3 records worst +2.7% (margin ~2.3pp)
//!      at the binding rate. If 360 sits with LARGE margin at high spm and ~0 margin
//!      at the binding spm, that IS outcome (B)-longer: 360 = minimax (gentlest-
//!      admissible at the WORST rate), over-conservative elsewhere.
//!
//! Usage: cargo run --release --bin deploy-coupling
//! Env: VARDIFF_DC_TRIALS (default 120 base, CI-scaled), VARDIFF_DC_THREADS.
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

const GATE_PCT: f64 = 5.0;
const SENS: f64 = 1.5;
const SPMS_BOUNDARY: &[f32] = &[6.0, 8.0, 12.0, 20.0, 30.0];
const SPMS_GUARD: &[f32] = &[2.0, 4.0];
// REPRESENTABLE τ only (≥ 60s tick); the deployment optimum is at the LONG end, so
// this brackets it as long as the gate binds below 900.
const TAUS: &[u64] = &[60, 90, 120, 150, 240, 300, 360, 480, 600, 720, 900];
const RATES_PPH: &[f32] = &[1.0, 2.0, 5.0, 10.0, 20.0, 40.0];

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

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn mean_ci(v: &[f64]) -> (f64, f64) {
    let n = v.len();
    if n == 0 { return (f64::NAN, f64::NAN); }
    let mean = v.iter().sum::<f64>() / n as f64;
    if n < 2 { return (mean, f64::NAN); }
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    (mean, 1.96 * (var / n as f64).sqrt())
}

/// (signed settled-e%, over-difficulty area) for one (τ,rate,spm) trial-set.
/// VERBATIM decline profile + metrics from tau-family-safety.rs / tau-family.rs:
/// settled-e = last e in the post-decline recovery window (gate + wobble source);
/// area = escape-phase ∫max(e,0)dt. Median over trials.
fn cell(tau: u64, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> (f64, f64) {
    let a = cfg(tau, SENS);
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
    let trial_end = d_end + observe * 60;
    let (mut settleds, mut areas) = (Vec::with_capacity(trials), Vec::with_capacity(trials));
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut se, mut area, mut last_t) = (0.0f64, 0.0f64, d_start);
        for tk in &t.ticks {
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
            if tk.t_secs > d_end && tk.t_secs <= trial_end { se = e; }
        }
        settleds.push(se);
        areas.push(area);
    }
    (median(settleds), median(areas))
}

/// For (τ, spm): the GATE quantity (worst-over-severity settled-e, with a CI from
/// the worst cell's trials) and the WOBBLE (|settled-e| at the worst-AREA severity,
/// matching tau-family's ordering metric). One pass over severities computes both.
fn tau_spm(tau: u64, spm: f32, base: usize, seed: u64) -> (f64, f64, f64) {
    let a = cfg(tau, SENS);
    let ct = (base as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
    // gate = worst (max) settled-e over severities; wobble = |settled-e| at the
    // worst-AREA severity. Track both selections in one severity loop.
    let (mut worst_se, mut worst_area, mut wob_at_worst_area) = (f64::MIN, f64::MIN, f64::NAN);
    // recompute a small CI on the gate quantity by re-running the worst severity's
    // settled samples; cheap second pass only at the selected severity.
    let mut worst_se_rate = RATES_PPH[0];
    for (k, &r) in RATES_PPH.iter().enumerate() {
        let (se, area) = cell(tau, r, spm, ct, seed.wrapping_add((k as u64) << 40));
        if se > worst_se { worst_se = se; worst_se_rate = r; }
        if area > worst_area { worst_area = area; wob_at_worst_area = se.abs(); }
    }
    // CI on the gate quantity at its worst severity (per-trial settled-e samples).
    let gate_ci = {
        let mature = 60u64;
        let rate = worst_se_rate / 100.0 / 60.0;
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
        let d_end = (mature + dm) * 60;
        let trial_end = d_end + observe * 60;
        let mut ses = Vec::with_capacity(ct);
        for i in 0..ct {
            let clock = Arc::new(MockClock::new(0));
            let v = (a.factory)(clock.clone());
            let t = run_trial_observed(v, clock, config.clone(), &sched,
                seed.wrapping_add(0x6A7E).wrapping_add(i as u64));
            let mut se = 0.0f64;
            for tk in &t.ticks {
                if tk.t_secs > d_end && tk.t_secs <= trial_end {
                    let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                    se = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
                }
            }
            ses.push(se);
        }
        mean_ci(&ses).1
    };
    (worst_se, gate_ci, wob_at_worst_area)
}

struct Row { tau: u64, gate_se: f64, gate_ci: f64, wobble: f64 }

fn main() {
    let base: usize = env::var("VARDIFF_DC_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    let seed = DEFAULT_BASELINE_SEED ^ 0xDEC0_0DE;
    let nth: usize = env::var("VARDIFF_DC_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    let all_spms: Vec<f32> = SPMS_BOUNDARY.iter().chain(SPMS_GUARD).copied().collect();
    let jobs: Vec<(f32, usize)> = all_spms.iter().flat_map(|&s| (0..TAUS.len()).map(move |ti| (s, ti))).collect();
    eprintln!("deploy-coupling: {} spm × {} τ = {} cells, base {} trials, {} threads, gate={}%.",
        all_spms.len(), TAUS.len(), jobs.len(), base, nth, GATE_PCT);

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(f32, u64, f64, f64, f64)>> = Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..nth {
            sc.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (spm, ti) = jobs[j];
                let (g, gci, w) = tau_spm(TAUS[ti], spm, base, seed.wrapping_add((j as u64) << 8));
                out.lock().unwrap().push((spm, TAUS[ti], g, gci, w));
                eprintln!("  spm{} τ{} done", spm, TAUS[ti]);
            });
        }
    });
    let raw = out.into_inner().unwrap();

    let rows_for = |spm: f32| -> Vec<Row> {
        let mut rows: Vec<Row> = raw.iter().filter(|(s, ..)| *s == spm)
            .map(|(_, t, g, gci, w)| Row { tau: *t, gate_se: *g, gate_ci: *gci, wobble: *w }).collect();
        rows.sort_by_key(|r| r.tau);
        rows
    };

    // τ_deploy = longest τ whose gate (worst settled-e) ≤ 5%. Report it, its wobble,
    // the champion 360's wobble + margin, wobble-monotonicity, and gate-monotonicity.
    println!("\n## DEPLOYMENT-COUPLING — gentlest-admissible window τ_deploy(spm) = longest τ with worst-settled-e ≤ {}%.", GATE_PCT);
    println!("Gate bounds τ from ABOVE (sleepy lag → settled over-difficulty); wobble wants τ long; so τ_deploy = the gate boundary.");
    println!("THE QUESTION: does τ_deploy slide with rate (deployment gain) or sit flat ≈360 (champion already right)? Sign of slide = which way to couple.\n");

    for (label, spms, is_boundary) in [
        ("BOUNDARY regime (spm≥6) — the within-regime coupling slope", SPMS_BOUNDARY, true),
        ("GUARD regime (spm<6) — separate controller, NOT stitched", SPMS_GUARD, false),
    ] {
        println!("### {label}");
        println!("| spm | τ_deploy (longest gate-pass) | wobble@τ_deploy | champ360 gate-se% (margin) | wobble@360 | gate monotone↑? | wobble monotone↓? | τ_deploy railed? |");
        println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
        let mut deploys: Vec<(f32, u64, bool)> = Vec::new();
        for &spm in spms {
            let rows = rows_for(spm);
            // longest τ with gate ≤ 5%, CONSERVATIVE: require the UPPER CI edge ≤ 5%
            // (gate_se + ci), so a τ counts admissible only if it clears the gate
            // beyond noise — the same "slope must exceed bars" discipline applied to
            // the admissibility decision itself.
            let pass = |r: &Row| r.gate_se + r.gate_ci.max(0.0) <= GATE_PCT;
            let tau_deploy = rows.iter().filter(|r| pass(r)).map(|r| r.tau).max();
            let r360 = rows.iter().find(|r| r.tau == 360);
            // gate monotone increasing in τ? (P1)
            let gate_mono = rows.windows(2).all(|w| w[1].gate_se >= w[0].gate_se - 1.0);
            // wobble monotone decreasing toward long τ? (P2)
            let wob_mono = rows.windows(2).all(|w| w[1].wobble <= w[0].wobble + 1.0);
            let (td_str, wob_td, railed) = match tau_deploy {
                Some(t) => {
                    let wob = rows.iter().find(|r| r.tau == t).map(|r| r.wobble).unwrap_or(f64::NAN);
                    let railed = t == *TAUS.last().unwrap();
                    (format!("{t}"), wob, railed)
                }
                None => ("NONE(<60)".to_string(), f64::NAN, false),
            };
            let (g360, m360, w360) = match r360 {
                Some(r) => (format!("{:+.1}±{:.1}", r.gate_se, r.gate_ci), GATE_PCT - r.gate_se, r.wobble),
                None => ("—".to_string(), f64::NAN, f64::NAN),
            };
            println!("| {} | {} | {:.0} | {} ({:+.1}pp) | {:.0} | {} | {} | {} |",
                spm as u32, td_str, wob_td, g360, m360, w360,
                if gate_mono {"yes"} else {"**NO**"},
                if wob_mono {"yes"} else {"**NO**"},
                if railed {"**rail@900**"} else {"no"});
            if let Some(t) = tau_deploy { deploys.push((spm, t, railed)); }
        }

        if is_boundary && deploys.len() >= 2 {
            let (lo_spm, lo_t, _) = deploys.first().unwrap();
            let (hi_spm, hi_t, hi_railed) = deploys.last().unwrap();
            println!("\n**Boundary-regime deployment-coupling verdict (spm{} vs spm{}):**", *lo_spm as u32, *hi_spm as u32);
            let flat = lo_t == hi_t;
            if flat {
                println!("  τ_deploy FLAT at {} across the regime ⇒ outcome (A): the champion-class fixed window is ~right at every", lo_t);
                println!("  rate; deployment rate-awareness buys ~nothing; the p<1 finding is an OVER-DIFFICULTY-AXIS curiosity. STOP — do");
                println!("  NOT pursue the expensive sub-tick p-pinning (wall 2).");
            } else if hi_t > lo_t {
                println!("  τ_deploy SLIDES LONGER at high rate ({}→{}) ⇒ outcome (B)-OPPOSITE: dense shares give the gate headroom, so", lo_t, hi_t);
                println!("  the gentlest-admissible window is SLEEPIER at high rate — the OPPOSITE of share-indexing (which shortens it).");
                println!("  Deployment coupling is NEGATIVE-p: share-indexing couples the WRONG WAY for deployment. {}",
                    if *hi_railed {"(τ_deploy railed at 900 — slide is an off-grid UPPER bound, even stronger.)"} else {""});
            } else {
                println!("  τ_deploy SLIDES SHORTER at high rate ({}→{}) ⇒ outcome (B)-SAME-direction: the gate, like over-difficulty,", lo_t, hi_t);
                println!("  tightens the admissible window at high rate; deployment coupling is toward-shorter (some p>0), same SIGN as");
                println!("  share-indexing — then the over-difficulty p<1 finding plausibly transfers and sub-tick p-pinning is worth pursuing.");
            }
            println!("  (Whether the slide EXCEEDS its bars — real vs flat-within-noise — read the gate-se CIs; a one-grid-step move is not a slope.)");
        }
        println!();
    }
    println!("READ: τ_deploy flat ≈360 ⇒ champion already right, stop. τ_deploy slides ⇒ deployment gain; LONGER-at-high-rate = couple");
    println!("opposite to share-indexing (gate headroom), SHORTER-at-high-rate = same way. P1 gate-monotone and P2 wobble-monotone must");
    println!("hold for 'gentlest-admissible = longest-admissible' to be the right reading; a **NO** in either column means re-read that spm.");
}
