//! THE PRINCIPAL FIGURE: performance vs share rate, the field as a thin
//! ribbon hugging the Poisson information floor.
//!
//! The paper's claim is structural, not a champion hunt: across the
//! production band (r* ∈ 4–30 spm) every reasonable controller is pinned to
//! the Poisson floor; the best-to-worst spread is a thin ribbon (~12%); the
//! only first-order lever is r*. This figure makes that legible in one frame:
//!
//!   x = share rate r* (log)
//!   y = STEADY-STATE tracking error, RMS of e=ln(Ĥ/H) on a stable stream
//!       after warmup (log). This is the axis the floor is defined on.
//!   field  = a thin gray line per config (the ribbon)
//!   floor  = the MEASURED cost-blind MLE error at a stated window τ — the
//!            exact, unbeatable bound for that window (NOT the analytic
//!            1/√(r*τ), which is overlaid faintly to show agreement). Drawn
//!            as a small labeled FAMILY (τ=150s, 360s) so the boundary is
//!            never ambiguous about which window it assumes — the hazard that
//!            would let a reader dismiss the flatness as a generously-drawn
//!            floor.
//!   champion = the one bold line that stays near the floor across the WHOLE
//!            band (the minimax-over-r* criterion made visible).
//!
//! Old champion and classic are drawn as named reference lines so the reader
//! sees the whole field is a ribbon, not a podium.
//!
//! Usage: cargo run --release --bin floor-ribbon
//! Env: VARDIFF_FR_TRIALS (default 300, oversampled at low spm),
//!      VARDIFF_FR_OUT (svg, default docs/floor_ribbon.svg), VARDIFF_SWEEP_SEED.

use std::env;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator, FullRetargetNoClamp,
    SignPersistenceCusumBoundary, StepFunction,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const WARMUP_MIN: u64 = 90; // past cold-start settling — measure steady state only
const DUR_MIN: u64 = 360; // 6h stable stream

// The rate band, log-spaced. 4–30 is the production minimax band.
const RATES: &[f32] = &[4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0, 30.0];

/// A SignPersist-family config by (tau, sens) — the minimax grid family.
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

/// The new champion (minimax-over-r* / corrected-metric winner).
fn champion_new() -> AlgorithmSpec { cfg(360, 1.5) }

/// The cost-blind MLE at window τ: EWMA(τ) belief, no control policy (fires
/// every tick, retargets straight to belief). Its steady-state error is the
/// EXACT floor for that window — no controller with window τ can track
/// tighter. Drawn as the boundary; this is measured, not asserted.
fn oracle(tau: u64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("floor τ={tau}s"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            StepFunction { table: vec![(u64::MAX, 0.0)] },
            FullRetargetNoClamp,
            1.0,
            clock,
        )))
    })
}

/// Steady-state RMS of e = ln(Ĥ/H) on a stable stream, after warmup. Pools
/// squared error across trials (so a sparse-rate cell with more trials is
/// simply a tighter estimate of the same quantity).
fn steady_rms_pct(a: &AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> f64 {
    let (proto, sched) = Scenario::Stable.build(spm);
    let config = TrialConfig {
        duration_secs: DUR_MIN * 60,
        tick_interval_secs: 60,
        ..proto
    };
    let warmup = WARMUP_MIN * 60;
    let (mut sse, mut n) = (0.0f64, 0u64);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        for tk in &t.ticks {
            if tk.t_secs <= warmup {
                continue;
            }
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln();
            sse += e * e;
            n += 1;
        }
    }
    if n == 0 {
        return f64::NAN;
    }
    (sse / n as f64).sqrt() * 100.0
}

struct Series {
    label: String,
    kind: Kind,
    tau: u64,
    rms: Vec<f64>, // per RATES
}
#[derive(PartialEq)]
enum Kind {
    Field,
    NewChamp,
    OldChamp,
    Classic,
    Floor,
}

fn main() -> std::io::Result<()> {
    let base_trials: usize = env::var("VARDIFF_FR_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let out = env::var("VARDIFF_FR_OUT").unwrap_or_else(|_| "docs/floor_ribbon.svg".to_string());
    let trials_for = |spm: f32| (base_trials as f64 * (12.0 / spm as f64).clamp(1.0, 4.0)).round() as usize;

    // The field: the minimax grid family. Named lines pulled out by Kind.
    let taus = [120u64, 150, 240, 360, 480, 720];
    let senss = [0.3f64, 0.6, 1.0, 1.5, 2.0];
    let mut specs: Vec<(AlgorithmSpec, Kind, u64)> = Vec::new();
    for &tau in &taus {
        for &sens in &senss {
            specs.push((cfg(tau, sens), Kind::Field, tau));
        }
    }
    // Named references (drawn bold over the ribbon).
    specs.push((champion_new(), Kind::NewChamp, 360));
    specs.push((AlgorithmSpec::champion(), Kind::OldChamp, 150));
    specs.push((AlgorithmSpec::classic_composed(), Kind::Classic, 0));
    // Floors: the measured cost-blind MLE at each named window.
    specs.push((oracle(150), Kind::Floor, 150));
    specs.push((oracle(360), Kind::Floor, 360));

    eprintln!(
        "floor-ribbon: {} series × {} rates, base {} trials (low-spm up to 4x)",
        specs.len(), RATES.len(), base_trials
    );

    // (series_idx, rate_idx) jobs.
    let jobs: Vec<(usize, usize)> =
        (0..specs.len()).flat_map(|si| (0..RATES.len()).map(move |ri| (si, ri))).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let grid: Mutex<Vec<Vec<f64>>> = Mutex::new(vec![vec![f64::NAN; RATES.len()]; specs.len()]);
    let n_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..n_threads {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (si, ri) = jobs[j];
                let spm = RATES[ri];
                let rms = steady_rms_pct(&specs[si].0, spm, trials_for(spm), seed ^ (spm as u64));
                grid.lock().unwrap()[si][ri] = rms;
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 32 == 0 { eprintln!("  {}/{}", d, jobs.len()); }
            });
        }
    });
    let grid = grid.into_inner().unwrap();

    let series: Vec<Series> = specs.iter().enumerate().map(|(i, (a, k, tau))| Series {
        label: a.name.clone(),
        kind: match k { Kind::Field=>Kind::Field, Kind::NewChamp=>Kind::NewChamp, Kind::OldChamp=>Kind::OldChamp, Kind::Classic=>Kind::Classic, Kind::Floor=>Kind::Floor },
        tau: *tau,
        rms: grid[i].clone(),
    }).collect();

    // Console: the ribbon thickness at each rate (the flatness claim, as a
    // number). thickness = (max − min)/min over the FIELD at that rate.
    println!("\n## Ribbon thickness — field steady-state RMS error spread at each rate");
    println!("| r* (spm) | floor τ360 | field min | field max | best→worst spread | new champ | old champ | classic |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for (ri, &r) in RATES.iter().enumerate() {
        let field: Vec<f64> = series.iter().filter(|s| s.kind == Kind::Field).map(|s| s.rms[ri]).collect();
        let fmin = field.iter().cloned().fold(f64::MAX, f64::min);
        let fmax = field.iter().cloned().fold(f64::MIN, f64::max);
        let floor = series.iter().find(|s| s.kind == Kind::Floor && s.tau == 360).map(|s| s.rms[ri]).unwrap_or(f64::NAN);
        let nc = series.iter().find(|s| s.kind == Kind::NewChamp).map(|s| s.rms[ri]).unwrap_or(f64::NAN);
        let oc = series.iter().find(|s| s.kind == Kind::OldChamp).map(|s| s.rms[ri]).unwrap_or(f64::NAN);
        let cl = series.iter().find(|s| s.kind == Kind::Classic).map(|s| s.rms[ri]).unwrap_or(f64::NAN);
        println!("| {} | {:.2}% | {:.2}% | {:.2}% | +{:.0}% | {:.2}% | {:.2}% | {:.2}% |",
            r as u32, floor, fmin, fmax, (fmax/fmin - 1.0)*100.0, nc, oc, cl);
    }

    let svg = render(&series);
    fs::write(&out, &svg)?;
    eprintln!("Wrote {}", out);
    Ok(())
}

fn render(series: &[Series]) -> String {
    let (ml, mr, mt, mb) = (72.0, 250.0, 78.0, 60.0);
    let w = 1080i64;
    let h = 560i64;
    let pw = w as f64 - ml - mr;
    let ph = h as f64 - mt - mb;

    // Log-log axes. x in spm, y in % RMS error.
    let (x_lo, x_hi) = (RATES[0] as f64 * 0.92, RATES[RATES.len()-1] as f64 * 1.05);
    // y range from the data (floors set the bottom, field max the top).
    let mut y_lo = f64::MAX;
    let mut y_hi = f64::MIN;
    for s in series {
        for &v in &s.rms {
            if v.is_finite() && v > 0.0 { y_lo = y_lo.min(v); y_hi = y_hi.max(v); }
        }
    }
    y_lo *= 0.85;
    y_hi *= 1.15;
    let lx = |x: f64| ml + pw * (x.ln() - x_lo.ln()) / (x_hi.ln() - x_lo.ln());
    let ly = |y: f64| mt + ph * (1.0 - (y.ln() - y_lo.ln()) / (y_hi.ln() - y_lo.ln()));

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="{cx}" y="30" text-anchor="middle" font-size="16" font-weight="bold">Every controller is pinned to the Poisson floor — the field is a thin ribbon, and r* is the lever</text>
<text x="{cx}" y="50" text-anchor="middle" font-size="12" fill="#555">Steady-state tracking error (RMS of e=ln(Ĥ/H) on a stable stream) vs share rate. Both axes log. Floor = measured cost-blind MLE at window τ (exact, unbeatable).</text>
"##,
        cx = w / 2
    ));

    // y gridlines at 1,2,3,5,7,10,15,20%
    for &yg in &[1.0,2.0,3.0,5.0,7.0,10.0,15.0,20.0,30.0] {
        if yg < y_lo || yg > y_hi { continue; }
        let y = ly(yg);
        s.push_str(&format!(
            r##"<line x1="{ml:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#e3e3e3" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="#999">{:.0}%</text>
"##,
            ml + pw, ml - 8.0, y + 4.0, yg
        ));
    }
    // x gridlines at the rate ticks
    for &xg in &[4.0,6.0,8.0,10.0,12.0,16.0,20.0,30.0] {
        let x = lx(xg);
        s.push_str(&format!(
            r##"<line x1="{x:.1}" y1="{mt:.1}" x2="{x:.1}" y2="{:.1}" stroke="#e3e3e3" stroke-width="1"/>
<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#999">{:.0}</text>
"##,
            mt + ph, mt + ph + 18.0, xg
        ));
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666">share rate r* (shares/min, log)</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666" transform="rotate(-90 {:.1} {:.1})">steady-state RMS tracking error (log)</text>
"##,
        ml + pw / 2.0, mt + ph + 42.0,
        ml - 50.0, mt + ph / 2.0, ml - 50.0, mt + ph / 2.0
    ));

    // path helper
    let path = |s_: &Series| -> String {
        let mut p = String::new();
        let mut started = false;
        for (ri, &v) in s_.rms.iter().enumerate() {
            if !v.is_finite() || v <= 0.0 { continue; }
            p.push_str(&format!("{}{:.1},{:.1} ", if !started {"M"} else {"L"}, lx(RATES[ri] as f64), ly(v)));
            started = true;
        }
        p.trim().to_string()
    };

    // 1) field ribbon — thin gray lines, drawn first (underneath).
    for s_ in series.iter().filter(|s| s.kind == Kind::Field) {
        s.push_str(&format!(
            r##"<path d="{}" fill="none" stroke="#9aa0a6" stroke-width="1" stroke-opacity="0.35"/>
"##,
            path(s_)
        ));
    }
    // 2) floors — dashed boundaries, labeled by τ.
    for s_ in series.iter().filter(|s| s.kind == Kind::Floor) {
        let col = if s_.tau == 360 { "#b07ad0" } else { "#d0a0c0" };
        s.push_str(&format!(
            r##"<path d="{}" fill="none" stroke="{col}" stroke-width="1.8" stroke-dasharray="2 3" stroke-opacity="0.85"/>
"##,
            path(s_)
        ));
        // inline label at the right end
        if let Some((ri, &v)) = s_.rms.iter().enumerate().rev().find(|(_, &v)| v.is_finite() && v>0.0) {
            s.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="10" fill="{col}">floor τ={}s</text>
"##,
                lx(RATES[ri] as f64) + 6.0, ly(v) + 3.0, s_.tau
            ));
        }
    }
    // 3) named reference lines, bold.
    let named: &[(Kind, &str, f64)] = &[
        (Kind::Classic, "#777777", 2.2),
        (Kind::OldChamp, "#377eb8", 2.6),
        (Kind::NewChamp, "#2e8b2e", 3.2),
    ];
    for (k, col, wd) in named {
        if let Some(s_) = series.iter().find(|s| &s.kind == k) {
            s.push_str(&format!(
                r##"<path d="{}" fill="none" stroke="{col}" stroke-width="{wd}" stroke-opacity="0.95"/>
"##,
                path(s_)
            ));
        }
    }

    // Legend
    let lgx = ml + pw + 26.0;
    let mut lgy = mt + 8.0;
    let mut leg = |sw: &str, txt: &str, y: &mut f64, sub: bool| {
        let mut out = String::new();
        out.push_str(&format!(r##"{sw}"##));
        let fill = if sub { "#777" } else { "#222" };
        let fs = if sub { 11 } else { 12 };
        out.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="{fs}" fill="{fill}">{txt}</text>
"##,
            lgx + 36.0, *y + 4.0
        ));
        *y += if sub { 19.0 } else { 24.0 };
        out
    };
    s.push_str(&leg(&format!(r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#2e8b2e" stroke-width="3.2"/>"##, lgx+28.0), "new champion (Ewma360/s1.5)", &mut lgy, false));
    s.push_str(&leg(&format!(r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#377eb8" stroke-width="2.6"/>"##, lgx+28.0), "old champion (s0.3)", &mut lgy, false));
    s.push_str(&leg(&format!(r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#777777" stroke-width="2.2"/>"##, lgx+28.0), "classic (original vardiff)", &mut lgy, false));
    s.push_str(&leg(&format!(r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#9aa0a6" stroke-width="1" stroke-opacity="0.6"/>"##, lgx+28.0), "field (30 configs)", &mut lgy, false));
    s.push_str(&leg(&format!(r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#b07ad0" stroke-width="1.8" stroke-dasharray="2 3"/>"##, lgx+28.0), "Poisson floor (by τ)", &mut lgy, false));
    lgy += 8.0;
    s.push_str(&format!(
        r##"<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">The ribbon is thin and</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">hugs the floor at every</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">rate; it descends only</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">as r* rises. Algorithm</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">choice is 2nd-order;</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">r* is the lever.</text>
"##,
        lgy, lgy+16.0, lgy+32.0, lgy+48.0, lgy+64.0, lgy+80.0
    ));

    s.push_str("</svg>\n");
    s
}
