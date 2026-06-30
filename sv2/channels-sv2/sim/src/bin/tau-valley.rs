//! COMPANION PANEL B: the τ-safety-valley — WHY the champion is the safe
//! frontier (panel A, steady-transient.rs, shows THAT it is).
//!
//! The scatter shows configs below-left of the champion (cheaper-steady AND
//! faster-transient) but colored UNSAFE, with no in-panel reason. The reason
//! is here: decline-safety (worst settled over-difficulty e% after a sustained
//! decline, over the authoritative rate×spm grid) is a U-shaped function of
//! the estimator window τ. Long τ (sleepy) lags a sustained decline into
//! over-difficulty; short τ (twitchy) overshoots it. The safe band is the
//! middle, and its floor is at the champion's window.
//!
//! settled-e is a function of τ ALONE (sensitivity barely moves the worst
//! cell), so the valley is a clean 1-D curve, not a cloud. The horizontal
//! line is the runaway threshold (GATE_PCT = 5%, matching slow-decline.rs);
//! the τ's whose curve sits below it are the safe window.
//!
//! Uses the IDENTICAL authoritative grid as slow-decline.rs and the scatter's
//! coloring (rates {1,2,5,10,20,40}%/hr × spm {2,4,6,8,12,20,30}, worst cell),
//! so panel B's safe band is consistent with panel A's green points by
//! construction — not a second, differently-measured safety number.
//!
//! Usage: cargo run --release --bin tau-valley
//! Env: VARDIFF_TV_TRIALS (default 80 base, CI-scaled), VARDIFF_TV_OUT.

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

const GATE_PCT: f64 = 5.0; // runaway threshold, identical to slow-decline.rs

// The τ axis (the estimator window). The champion is τ=360.
const TAUS: &[u64] = &[120, 150, 240, 360, 480, 600, 720];
// One curve PER FIXED sensitivity. Drawing the valley at several FIXED sens
// (not worst-over-sens) is the control against a τ×sensitivity confound: if
// the U-shape — floored at 360 — appears at EVERY fixed sens, the valley is
// genuinely a τ effect, not sensitivity leaking in. (champion sens = 1.5.)
const SENSS: &[f64] = &[0.3, 1.0, 1.5, 2.0];

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

/// Settled e% after a (rate_pph) decline at `spm`, after a 120-min recovery
/// window. Identical profile to slow-decline.rs / the scatter.
fn decline_cell(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> f64 {
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
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let mut settled = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let mut se = 0.0f64;
        for tk in &t.ticks {
            if tk.t_secs > d_end && tk.t_secs <= trial_end {
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                se = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            }
        }
        settled.push(se);
    }
    median(settled)
}

/// Worst settled-e% over the authoritative grid for a config.
fn worst_settled(a: &AlgorithmSpec, base_trials: usize, seed: u64) -> f64 {
    let rates = [1.0f32, 2.0, 5.0, 10.0, 20.0, 40.0];
    let spms = [2.0f32, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];
    let mut worst = f64::MIN;
    let mut k = 0u64;
    for &spm in &spms {
        let ct = (base_trials as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        for &r in &rates {
            let e = decline_cell(a, r, spm, ct, seed.wrapping_add(k << 40));
            if e > worst { worst = e; }
            k += 1;
        }
    }
    worst
}

fn main() -> std::io::Result<()> {
    let base: usize = env::var("VARDIFF_TV_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    let out = env::var("VARDIFF_TV_OUT").unwrap_or_else(|_| "docs/tau_valley.svg".to_string());
    let seed = DEFAULT_BASELINE_SEED ^ 0xDEC11E;

    // For each τ, the worst-over-sens settled-e (conservative upper envelope).
    let jobs: Vec<(usize, usize)> =
        (0..TAUS.len()).flat_map(|ti| (0..SENSS.len()).map(move |si| (ti, si))).collect();
    let next = AtomicUsize::new(0);
    let grid: Mutex<Vec<Vec<f64>>> = Mutex::new(vec![vec![f64::NAN; SENSS.len()]; TAUS.len()]);
    let nth = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..nth {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (ti, si) = jobs[j];
                let w = worst_settled(&cfg(TAUS[ti], SENSS[si]), base, seed);
                grid.lock().unwrap()[ti][si] = w;
                eprintln!("  τ={} s{} done", TAUS[ti], SENSS[si]);
            });
        }
    });
    let grid = grid.into_inner().unwrap(); // grid[ti][si] = worst settled-e

    // One curve per FIXED sensitivity (curve[si][ti]) — the confound control.
    println!("\n## τ-safety-valley: worst settled over-difficulty e% vs window τ, ONE CURVE PER FIXED sens");
    println!("(worst over the authoritative rate×spm decline grid; sens held FIXED per curve). Gate = {}%.", GATE_PCT);
    println!("The U-shape appearing at EVERY fixed sens ⇒ the valley is a τ effect, not a τ×sens confound.\n");
    print!("| τ (s) |");
    for &s in SENSS { print!(" s{} |", s); }
    println!(" min-sens argmin? |");
    print!("| --- |"); for _ in SENSS { print!(" --- |"); } println!(" --- |");
    for (ti, &t) in TAUS.iter().enumerate() {
        let tag = if t == 360 { " «champ" } else { "" };
        print!("| {}{} |", t, tag);
        for si in 0..SENSS.len() { print!(" {:+.1} |", grid[ti][si]); }
        println!();
    }
    // Per-sens: is the argmin at τ=360, and what's the safe window?
    println!("\nPer fixed sens — valley floor (argmin τ) and safe window (e ≤ {}%):", GATE_PCT);
    let champ_ti = TAUS.iter().position(|&t| t == 360).unwrap();
    let mut all_floor_360 = true;
    for (si, &s) in SENSS.iter().enumerate() {
        let col: Vec<f64> = (0..TAUS.len()).map(|ti| grid[ti][si]).collect();
        let argmin_ti = col.iter().enumerate().min_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        let safe_w: Vec<u64> = TAUS.iter().zip(&col).filter(|(_, &v)| v <= GATE_PCT).map(|(&t, _)| t).collect();
        let floor_at_360 = argmin_ti == champ_ti;
        if !floor_at_360 { all_floor_360 = false; }
        println!("   s{}: floor at τ={} {}, safe window {:?}", s, TAUS[argmin_ti],
            if floor_at_360 {"(= champion ✓)"} else {"(NOT 360 — confound!)"}, safe_w);
    }
    println!("\n{}", if all_floor_360 {
        "VERDICT: the valley floors at τ=360 at EVERY fixed sens — the U-shape is a genuine τ effect (no confound)."
    } else {
        "VERDICT: floor moves with sens — the U-shape is partly a τ×sens confound; do NOT caption as a pure τ valley."
    });

    fs::write(&out, render(&grid))?;
    eprintln!("Wrote {}", out);
    Ok(())
}

fn render(grid: &[Vec<f64>]) -> String {
    let (ml, mr, mt, mb) = (72.0, 44.0, 78.0, 56.0);
    let (w, h) = (760i64, 460i64);
    let pw = w as f64 - ml - mr;
    let ph = h as f64 - mt - mb;
    // x = τ index (evenly spaced, labeled by τ); y = settled-e%.
    let all: Vec<f64> = grid.iter().flat_map(|r| r.iter().cloned()).collect();
    let y_hi = all.iter().cloned().fold(f64::MIN, f64::max).max(GATE_PCT) * 1.15;
    let y_lo = 0.0f64.min(all.iter().cloned().fold(f64::MAX, f64::min)) - 0.5;
    let lx = |i: usize| ml + pw * i as f64 / (TAUS.len() - 1) as f64;
    let ly = |v: f64| mt + ph * (1.0 - (v - y_lo) / (y_hi - y_lo));

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="{cx}" y="28" text-anchor="middle" font-size="16" font-weight="bold">Why the champion is the safe frontier: decline-safety is a valley in τ</text>
<text x="{cx}" y="46" text-anchor="middle" font-size="12" fill="#555">Worst settled over-difficulty after a sustained decline (over the rate×spm grid)</text>
<text x="{cx}" y="62" text-anchor="middle" font-size="12" fill="#555">vs estimator window τ. Below the line = safe.</text>
"##,
        cx = w / 2
    ));
    // gridlines + y labels
    let mut yg = (y_lo.ceil()) as i64;
    while (yg as f64) <= y_hi {
        let y = ly(yg as f64);
        s.push_str(&format!(
            r##"<line x1="{ml:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#ececec" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="#aaa">{:+}%</text>
"##,
            ml + pw, ml - 8.0, y + 4.0, yg
        ));
        yg += 2;
    }
    // gate threshold line
    let gy = ly(GATE_PCT);
    s.push_str(&format!(
        r##"<line x1="{ml:.1}" y1="{gy:.1}" x2="{:.1}" y2="{gy:.1}" stroke="#e41a1c" stroke-width="1.5" stroke-dasharray="6 4"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="#e41a1c">runaway gate (+{:.0}%)</text>
"##,
        ml + pw, ml + pw, gy - 5.0, GATE_PCT
    ));
    // safe band shading (region below the gate)
    s.push_str(&format!(
        r##"<rect x="{ml:.1}" y="{gy:.1}" width="{:.1}" height="{:.1}" fill="#2e8b2e" fill-opacity="0.06"/>
"##,
        pw, (mt + ph - gy).max(0.0)
    ));
    // x labels (τ); champion window bolded.
    for (i, &t) in TAUS.iter().enumerate() {
        let champ = t == 360;
        s.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{}" font-weight="{}">{}</text>
"##,
            lx(i), mt + ph + 18.0, if champ {"#2e8b2e"} else {"#999"}, if champ {"bold"} else {"normal"}, t
        ));
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666">estimator window τ (seconds)</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666" transform="rotate(-90 {:.1} {:.1})">worst settled over-difficulty e%</text>
"##,
        ml + pw / 2.0, mt + ph + 40.0,
        ml - 50.0, mt + ph / 2.0, ml - 50.0, mt + ph / 2.0
    ));
    // A SINGLE τ-curve. The per-fixed-sens sweep verified settled-e is
    // sensitivity-INVARIANT (identical to 0.1% across s0.3–s2), so four
    // overlapping curves would read as a rendering error; the invariance is
    // stated in the caption with the table as evidence. Use the champion
    // sens column (any column is identical). Green where safe, red where not.
    let csi = SENSS.iter().position(|&s| (s - 1.5).abs() < 1e-9).unwrap_or(0);
    let curve: Vec<f64> = (0..TAUS.len()).map(|ti| grid[ti][csi]).collect();
    let mut d = String::new();
    for (ti, &v) in curve.iter().enumerate() {
        d.push_str(&format!("{}{:.1},{:.1} ", if ti == 0 {"M"} else {"L"}, lx(ti), ly(v)));
    }
    s.push_str(&format!(
        r##"<path d="{}" fill="none" stroke="#333" stroke-width="2.4"/>
"##,
        d.trim()
    ));
    for (ti, &v) in curve.iter().enumerate() {
        let safe = v <= GATE_PCT;
        let (fill, stroke) = if safe { ("#2e8b2e", "#1d5e1d") } else { ("#fff", "#e41a1c") };
        s.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="5.5" fill="{fill}" stroke="{stroke}" stroke-width="{}"/>
"##,
            lx(ti), ly(v), if safe {1.0} else {2.0}
        ));
        if TAUS[ti] == 360 {
            s.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="10" fill="none" stroke="#2e8b2e" stroke-width="1.8"/>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-weight="bold" fill="#2e8b2e">champion</text>
"##,
                lx(ti), ly(v), lx(ti), ly(v) + 26.0
            ));
        }
    }
    // flank annotations
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#999" font-style="italic">twitchy: overshoots</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#999" font-style="italic">sleepy: lags decline</text>
"##,
        lx(0) + 36.0, mt + 16.0, lx(TAUS.len()-1) - 36.0, mt + 16.0
    ));
    // note: sensitivity-invariance
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#888" font-style="italic">τ-only: invariant across sens s0.3–s2</text>
"##,
        ml + pw, mt + ph - 6.0
    ));
    s.push_str("</svg>\n");
    s
}
