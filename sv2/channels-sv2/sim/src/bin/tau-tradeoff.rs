//! THE SYNTHESIS IMAGE: the champion is the gentlest point in a doubly-walled
//! admissible island — NOT the balance of two opposing ends.
//!
//! `tau-family.rs` showed the per-rate valley SLIDE (τ*∝1/r) as a two-panel
//! figure. This binary collapses the whole mapped parameter space into ONE
//! image. The ask was "wobble and over-difficulty as two opposing ends of the
//! τ-path" — and the DATA REFUSED that framing, which is the finding, not a
//! disappointment: over-difficulty is NOT monotone-opposing, it is U-SHAPED
//! (high at BOTH the jumpy and sleepy extremes), so the picture is a safe ISLAND
//! bounded on both sides, not a tension between two ends.
//!
//! The single axis is τ (the window), read as a path:
//!   ← JUMPY / responsive (small τ)            SLEEPY / damped (large τ) →
//!
//! TWO CURVES, TWO DIFFERENT ROLES (this asymmetry is the spine — the
//! constraint-not-cost discipline showing up in the geometry):
//!   - OVER-DIFFICULTY AREA (escape-phase ∫max(e,0)dt, the DANGEROUS spiral
//!     cost) is U-SHAPED, but the two arms have DIFFERENT mechanisms (do not
//!     call both "lag" — they aren't symmetric):
//!       · sleepy arm: a too-sleepy window LAGS the decline → over-difficulty
//!         (the canonical death-spiral cause, also the taller/worse wall here:
//!         sleepy-end ≈ 5470 vs jumpy-end ≈ 5060 e-min);
//!       · jumpy arm: a too-short window is too NOISY at its WORST-CASE sparse
//!         rate (low spm) — that is what the minimax surfaces, NOT lag. [This
//!         mechanism is inferred from the minimax structure; the figure shows
//!         the shape, not the cause — flagged, not asserted as proven.]
//!     This is the axis that GATES: where worst settled-e breaches +5%, the
//!     window is INADMISSIBLE. The wall is on BOTH ends; the island is between.
//!
//! MINIMAX IS THE HEADLINE, not fine print — it reconciles this figure with the
//! committed §8.3 flag, which otherwise looks contradicted. The flag's measured
//! result: the PER-RATE over-difficulty optimum SLIDES to τ≈30 at high spm (τ=30
//! is the BEST window there). This figure shows over-difficulty HIGH at τ=30.
//! Both true: this is the MINIMAX (worst-case over spm), where τ=30 is dragged
//! up by being terrible at LOW spm even though it is optimal at high spm. So each
//! per-rate curve has a sliding single minimum (tau-family); the minimax-over-
//! rate is U-shaped and interior (here). The two figures are the two halves of
//! the share-indexing case: a FIXED window must compromise (this), the per-rate
//! optimum MOVES (family). Read τ=30's height here as "worst-case across rates,"
//! NOT as "τ=30 is bad" — that would contradict the flag.
//!   - WOBBLE (|settled-e| under-difficulty, the SAFE self-healing cost) falls
//!     toward sleepy ACROSS THE ADMISSIBLE BAND and bottoms near its sleepy edge
//!     (≈8.1 at τ=480), then turns UP toward τ=900 (≈12.1) — so NOT monotone to
//!     the sleepy end; the uptick is entirely INSIDE the sleepy wall (τ≥480
//!     inadmissible), so within the island the "falls toward sleepy" claim holds.
//!     It does NOT wall anything (no hard breach on the safe-side axis); it only
//!     ORDERS within the admissible set — "be as sleepy as the wall allows,
//!     because sleepy is lower wobble."
//!
//! So: the OVER-DIFFICULTY U defines the island (the dangerous axis gates); the
//! WOBBLE gradient places the champion at the island's SLEEPY edge (the safe
//! axis orders within). The champion (τ=360) is INTERIOR — the gentlest config
//! the wall still permits — not extremal. Same logic as the evaluation-plane
//! schematic (gentlest admissible point against a wall), now in the τ
//! parameterization with the wall on both sides.
//!
//! TIES TO THE §8.3 τ-flag (`information-floor.md`): the over-difficulty U's
//! minimum sits LEFT of the champion (a jumpier window would lower
//! over-difficulty but climb the wobble gradient) — that is the SAME fact as the
//! flag's "τ* is jumpier than 360, but wobble pulls back; 360 is the balance,
//! not a defect." This figure is the single-rate spatial version of that trade.
//! The wall is drawn WHERE THE DATA PUTS IT (measured worst settled-e per τ on
//! the same grid), NOT where it is assumed to bite.
//!
//! REDUCTION: worst-over-(rate×spm) — MINIMAX — for all three quantities,
//! because that is the actual champion-selection criterion (the wall only bites
//! in the minimax view; at a single rate nothing breaches in range). This is the
//! SELECTION-SUMMARY view, distinct from tau-family's per-rate SLIDE view; one
//! image can't cleanly carry both, and this one carries the tradeoff-vs-wall.
//!
//! The wall is drawn WHERE THE DATA PUTS IT (worst settled-e per τ, measured on
//! the same grid as the two costs) — not where it is assumed to bite. Same grid,
//! one sweep, three quantities; the figure cannot show a wall the data didn't.
//!
//! Usage: cargo run --release --bin tau-tradeoff
//! Env: VARDIFF_TT_TRIALS (default 80 base, CI-scaled), VARDIFF_TT_OUT.

use std::env;
use std::fs;
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

const GATE_PCT: f64 = 5.0; // the decline-safety wall, identical to slow-decline.rs
const SENS: f64 = 1.5; // champion sensitivity (settled-e is sens-invariant; tau-valley)
const CHAMPION_TAU: u64 = 360;

// τ axis — the path. 30s (tick floor) → 900s, the same range tau-family bracketed.
const TAUS: &[u64] = &[30, 45, 60, 90, 120, 150, 240, 300, 360, 480, 600, 720, 900];
// The authoritative grid the minimax runs over.
const SPMS: &[f32] = &[2.0, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];
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

/// THREE quantities for one (τ,rate,spm) cell:
///   over-difficulty area = ∫max(e,0)dt over the escape phase (dangerous cost),
///   wobble = |settled-e| (safe-side under-difficulty cost),
///   settled_signed = settled-e WITH sign (the GATE quantity — >+GATE breaches).
/// Same decline profile as tau-family.rs / slow-decline.rs.
fn decline_cell(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> (f64, f64, f64) {
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
    let (mut areas, mut wobs, mut setts) =
        (Vec::with_capacity(trials), Vec::with_capacity(trials), Vec::with_capacity(trials));
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut area, mut last_t, mut settled) = (0.0f64, d_start, 0.0f64);
        for tk in &t.ticks {
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
            if tk.t_secs > d_end && tk.t_secs <= trial_end { settled = e; }
        }
        areas.push(area);
        wobs.push(settled.abs());
        setts.push(settled);
    }
    (median(areas), median(wobs), median(setts))
}

/// Minimax over the (rate×spm) grid for one τ: returns
///   (worst over-difficulty area, worst wobble, worst SIGNED settled-e).
/// Each "worst" is the max over the grid — the conservative envelope, matching
/// the champion-selection criterion. The signed settled-e max is what defines
/// the wall (breach iff > +GATE_PCT).
fn minimax_for_tau(tau: u64, base_trials: usize, seed: u64) -> (f64, f64, f64) {
    let a = cfg(tau, SENS);
    let (mut worst_area, mut worst_wob, mut worst_sett) = (f64::MIN, f64::MIN, f64::MIN);
    let mut k = 0u64;
    for &spm in SPMS {
        let ct = (base_trials as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        for &r in RATES_PPH {
            let (area, wob, sett) = decline_cell(&a, r, spm, ct, seed.wrapping_add(k << 40));
            if area > worst_area { worst_area = area; }
            if wob > worst_wob { worst_wob = wob; }
            if sett > worst_sett { worst_sett = sett; }
            k += 1;
        }
    }
    (worst_area, worst_wob, worst_sett)
}

fn main() -> std::io::Result<()> {
    let base: usize = env::var("VARDIFF_TT_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    let out = env::var("VARDIFF_TT_OUT").unwrap_or_else(|_| "docs/tau_tradeoff.svg".to_string());
    let seed = DEFAULT_BASELINE_SEED ^ 0x712A_DE0F;

    let next = AtomicUsize::new(0);
    // rows[ti] = (area, wob, settled_signed)
    let rows: Mutex<Vec<(f64, f64, f64)>> = Mutex::new(vec![(f64::NAN, f64::NAN, f64::NAN); TAUS.len()]);
    let nth = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    eprintln!("tau-tradeoff: {} τ × ({}×{}) minimax cells, base {} trials, {} threads",
        TAUS.len(), SPMS.len(), RATES_PPH.len(), base, nth);
    std::thread::scope(|s| {
        for _ in 0..nth {
            s.spawn(|| loop {
                let ti = next.fetch_add(1, Ordering::Relaxed);
                if ti >= TAUS.len() { break; }
                let r = minimax_for_tau(TAUS[ti], base, seed.wrapping_add((ti as u64) << 8));
                rows.lock().unwrap()[ti] = r;
                eprintln!("  τ{} done", TAUS[ti]);
            });
        }
    });
    let rows = rows.into_inner().unwrap();

    // --- text table ---
    println!("\n## τ-TRADEOFF — MINIMAX over rate×spm: why a FIXED window must land INTERIOR (the complement to tau-family's per-rate slide)");
    println!("sens={} fixed. WALL = worst signed settled-e > {}%, on BOTH ends: sleepy=lags the decline; jumpy=too noisy at its worst-case sparse rate (jumpy mechanism inferred from minimax, not proven here).", SENS, GATE_PCT);
    println!("| τ (s) | regime | over-diff area | wobble % | worst settled-e % | admissible? |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for (ti, &t) in TAUS.iter().enumerate() {
        let (area, wob, sett) = rows[ti];
        let regime = if t < 150 { "jumpy" } else if t > 480 { "sleepy" } else { "mid" };
        let adm = if sett > GATE_PCT { "**WALL (breach)**" } else { "ok" };
        let champ = if t == CHAMPION_TAU { " ←champion" } else { "" };
        println!("| {}{} | {} | {:.0} | {:.1} | {:+.1} | {} |", t, champ, regime, area, wob, sett, adm);
    }
    // locate the admissible window edges and the champion's position in it
    let breach: Vec<u64> = TAUS.iter().zip(rows.iter()).filter(|(_, r)| r.2 > GATE_PCT).map(|(t, _)| *t).collect();
    let sleepy_safe_edge = TAUS.iter().zip(rows.iter()).filter(|(_, r)| r.2 <= GATE_PCT).map(|(t, _)| *t).max();
    println!("\nWall breaches at τ ∈ {:?}. Sleepiest admissible τ = {:?} (the gentlest safe window — champion should sit at/below it).",
        breach, sleepy_safe_edge);

    // --- SVG: ONE plane. x = τ (log, the path). Two cost curves + the wall. ---
    // Canvas widened (w 920→1140, mr 210→430) so the right-margin legend column
    // has room for the longest line without clipping past the edge — the plot
    // width (pw) is unchanged because it is w-ml-mr and both grew together.
    let (w, h) = (1140.0, 560.0);
    let (ml, mr, mt, mb) = (70.0, 430.0, 70.0, 80.0);
    let (pw, ph) = (w - ml - mr, h - mt - mb);
    let lt = (*TAUS.first().unwrap() as f64).ln();
    let ht = (*TAUS.last().unwrap() as f64).ln();
    let xof = |t: f64| ml + ((t).ln() - lt) / (ht - lt) * pw;
    // costs are on different scales; normalize each to its own max so both
    // curves are legible on one plane (this is a SHAPE figure — the crossing and
    // the wall are the message, not absolute magnitudes; axis labels say so).
    let max_area = rows.iter().map(|r| r.0).filter(|x| x.is_finite()).fold(f64::MIN, f64::max);
    let max_wob = rows.iter().map(|r| r.1).filter(|x| x.is_finite()).fold(f64::MIN, f64::max);
    let yof = |frac: f64| mt + (1.0 - frac) * ph; // frac in [0,1], 0=bottom

    let mut svg = String::new();
    svg.push_str(&format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" font-family=\"Helvetica,Arial,sans-serif\" font-size=\"13\">\n"));
    let cx_plot = ml + pw / 2.0;
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"18\" text-anchor=\"middle\" font-size=\"12\" font-weight=\"bold\" fill=\"#333\">MINIMAX over rate×spm: why a FIXED window lands interior — champion = gentlest point in a doubly-walled island</text>\n", cx_plot));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"33\" text-anchor=\"middle\" font-size=\"10\" fill=\"#999\" font-style=\"italic\">tau-tradeoff.rs (generated). τ=30 reads HIGH here = worst-case across rates, NOT \"τ=30 is bad\" — per-rate it is optimal at high spm (cf. §8.3 flag / tau-family).</text>\n", cx_plot));
    svg.push_str(&format!("<rect x=\"{ml}\" y=\"{mt}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"#bbb\"/>\n"));

    // THE WALL: shade τ regions that breach the gate. Drawn per-segment between
    // adjacent τ where the signed settled-e crosses +GATE_PCT.
    for ti in 0..TAUS.len() {
        if rows[ti].2 > GATE_PCT {
            // shade the band centered on this τ (half-step to each neighbor).
            let t = TAUS[ti] as f64;
            let lo = if ti > 0 { (t * TAUS[ti-1] as f64).sqrt() } else { t * 0.9 };
            let hi = if ti + 1 < TAUS.len() { (t * TAUS[ti+1] as f64).sqrt() } else { t * 1.1 };
            let x0 = xof(lo); let x1 = xof(hi);
            svg.push_str(&format!("<rect x=\"{x0:.1}\" y=\"{mt}\" width=\"{:.1}\" height=\"{ph}\" fill=\"#e74c3c\" fill-opacity=\"0.14\"/>\n", x1 - x0));
        }
    }
    // wall label (only if there is a breach region)
    if rows.iter().any(|r| r.2 > GATE_PCT) {
        // BILATERAL wall caption: one title centered over the plot, then a
        // per-wall mechanism label over each wall's OWN shading — the two arms of
        // the U have DIFFERENT causes and must not both read "lag". The jumpy
        // cause is INFERRED from the minimax structure (τ=30's worst case is low
        // spm, where a short window is too noisy), NOT proven by this figure.
        let cx = ml + pw / 2.0;
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#c0392b\" font-weight=\"bold\">DECLINE-SAFETY WALL — worst settled-e &gt; {}% (inadmissible), on BOTH ends</text>\n", cx, mt + 14.0, GATE_PCT as u32));
        // jumpy wall (left): label over the left shading.
        let jumpy_breach: Vec<f64> = TAUS.iter().zip(rows.iter()).filter(|(t, r)| r.2 > GATE_PCT && (**t as f64) < CHAMPION_TAU as f64).map(|(t, _)| *t as f64).collect();
        if let (Some(&lo), Some(&hi)) = (jumpy_breach.first(), jumpy_breach.last()) {
            let lcx = (xof(lo) + xof(hi)) / 2.0;
            svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#c0392b\">jumpy: too noisy at its</text>\n", lcx, mt + 30.0));
            svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#c0392b\">worst-case sparse rate*</text>\n", lcx, mt + 42.0));
        }
        // sleepy wall (right): label over the right shading.
        let sleepy_breach: Vec<f64> = TAUS.iter().zip(rows.iter()).filter(|(t, r)| r.2 > GATE_PCT && (**t as f64) > CHAMPION_TAU as f64).map(|(t, _)| *t as f64).collect();
        if let (Some(&lo), Some(&hi)) = (sleepy_breach.first(), sleepy_breach.last()) {
            let rcx = (xof(lo) + xof(hi)) / 2.0;
            svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#c0392b\">sleepy: lags the</text>\n", rcx, mt + 30.0));
            svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#c0392b\">decline</text>\n", rcx, mt + 42.0));
        }
        // footnote bottom-LEFT (anchor start), clear of the champion label which
        // sits center-bottom.
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"8.5\" fill=\"#999\" font-style=\"italic\">*jumpy mechanism inferred from minimax structure, not proven by this figure</text>\n", ml + 4.0, mt + ph - 4.0));
    }

    // the two cost curves (normalized each to own max).
    let mut over_pts = String::new();
    let mut wob_pts = String::new();
    for (ti, &t) in TAUS.iter().enumerate() {
        let (area, wob, _) = rows[ti];
        over_pts.push_str(&format!("{:.1},{:.1} ", xof(t as f64), yof(area / max_area)));
        wob_pts.push_str(&format!("{:.1},{:.1} ", xof(t as f64), yof(wob / max_wob)));
    }
    svg.push_str(&format!("<polyline points=\"{over_pts}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"2.5\"/>\n"));
    svg.push_str(&format!("<polyline points=\"{wob_pts}\" fill=\"none\" stroke=\"#2980b9\" stroke-width=\"2.5\"/>\n"));

    // champion marker (vertical at τ=360).
    let xc = xof(CHAMPION_TAU as f64);
    svg.push_str(&format!("<line x1=\"{xc:.1}\" y1=\"{mt}\" x2=\"{xc:.1}\" y2=\"{:.1}\" stroke=\"#2c3e50\" stroke-width=\"1.5\" stroke-dasharray=\"4 3\"/>\n", mt + ph));
    svg.push_str(&format!("<circle cx=\"{xc:.1}\" cy=\"{:.1}\" r=\"5\" fill=\"#2c3e50\"/>\n", mt + ph));
    // champion labels ABOVE the marker (inside the plot, top area) to avoid the
    // axis path-labels below — anchored near the champion x but clamped left so
    // they don't collide with the right wall's shading text.
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"bold\" font-size=\"12\" fill=\"#2c3e50\">champion τ=360 (gentlest safe window)</text>\n", xc - 60.0, mt + ph - 12.0));

    // x ticks + the path labels.
    for &t in TAUS {
        let x = xof(t as f64);
        svg.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\">{t}</text>\n", mt + ph + 15.0));
    }
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"bold\">estimator window τ (s, log) — the path</text>\n", ml + pw/2.0, h - 30.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"11\" fill=\"#444\">← JUMPY / responsive</text>\n", ml + 4.0, h - 12.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11\" fill=\"#444\">SLEEPY / damped →</text>\n", ml + pw - 4.0, h - 12.0));
    svg.push_str(&format!("<text x=\"22\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" transform=\"rotate(-90 22 {:.1})\">cost (each normalized to its own max — a SHAPE figure)</text>\n", mt + ph/2.0, mt + ph/2.0));

    // legend + the reading.
    let lx = ml + pw + 16.0;
    svg.push_str(&format!("<line x1=\"{lx:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#c0392b\" stroke-width=\"2.5\"/>\n", mt + 10.0, lx + 22.0, mt + 10.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\">over-difficulty area</text>\n", lx + 28.0, mt + 14.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#c0392b\">(DANGEROUS; U-shaped — GATES island)</text>\n", lx + 28.0, mt + 28.0));
    svg.push_str(&format!("<line x1=\"{lx:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#2980b9\" stroke-width=\"2.5\"/>\n", mt + 52.0, lx + 22.0, mt + 52.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\">wobble (under-diff)</text>\n", lx + 28.0, mt + 56.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#2980b9\">(SAFE; falls sleepy-ward across the band — ORDERS within)</text>\n", lx + 28.0, mt + 70.0));
    svg.push_str(&format!("<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"22\" height=\"11\" fill=\"#e74c3c\" fill-opacity=\"0.14\"/>\n", mt + 86.0));
    svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\">wall (inadmissible)</text>\n", lx + 28.0, mt + 95.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#333\">Over-difficulty (dangerous) GATES:</text>\n", mt + 122.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#333\">U-shaped, walls BOTH ends → the</text>\n", mt + 136.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#333\">admissible island. Wobble (safe)</text>\n", mt + 150.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#333\">ORDERS within it → champion at the</text>\n", mt + 164.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#333\">sleepy edge (interior, not extremal).</text>\n", mt + 178.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#555\">Over-diff min is LEFT of champion:</text>\n", mt + 198.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#555\">a jumpier τ lowers over-diff but</text>\n", mt + 211.0));
    svg.push_str(&format!("<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#555\">climbs wobble — the §8.3 flag's trade.</text>\n", mt + 224.0));

    svg.push_str("</svg>\n");
    fs::write(&out, svg)?;
    eprintln!("wrote {}", out);
    Ok(())
}
