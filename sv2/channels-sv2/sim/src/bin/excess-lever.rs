//! THE r* LEVER (structural section): detection EXCESS vs share rate r*.
//!
//! The headline is "performance is information-floor-limited; the only lever
//! that moves the floor is raising r*." The HONEST visual of that is not the
//! composite-cost-vs-r* overlay (non-monotone, the metric collapses at the
//! band edges) and not the stable-safe-window result (a robustness claim, not
//! a lever claim). It is detection EXCESS vs r*:
//!
//!   EXCESS(r*) = P[fire within W | −10% drop] − P[fire within W | no drop]
//!
//! the false-alarm-CORRECTED small-drop detection (detection-control.rs). At
//! production r* the floor is so coarse (Theorem 2) that a −10% drop is
//! statistically invisible within a monitoring window: EXCESS ≈ 0, pinned
//! flat — no controller can do better, the floor is the limit. As r* rises,
//! the per-window share count grows, the floor recedes, and the same drop
//! becomes detectable: EXCESS climbs away from 0. THAT is "raising r* buys
//! agility," shown by a quantity that is floor-limited where the paper says
//! it is and opens up exactly where the paper says it does — monotone in the
//! right direction, and clean (it is the EXCESS measurement already trusted).
//!
//! Plotted for the CHAMPION (Ewma360/s1.5) at a tight monitoring window
//! (15min), swept over r* on a log axis. A faint band over the field shows it
//! is not champion-specific — the floor binds everyone at low r*.
//!
//! Usage: cargo run --release --bin excess-lever
//! Env: VARDIFF_EL_TRIALS (default 1500, CI-scaled at low spm), VARDIFF_EL_OUT.

use std::env;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const WINDOW_SECS: u64 = 15 * 60; // tight monitoring window — where the floor bites hardest
const SETTLE_MIN: u64 = 60; // matured counter before the event
const DROP_PCT: i32 = -10; // the small drop the floor hides at low r*

// r* axis (log-spaced), spanning production (4–6) to high-r* (60).
const RATES: &[f32] = &[4.0, 6.0, 8.0, 12.0, 20.0, 30.0, 45.0, 60.0];

fn champion() -> AlgorithmSpec {
    AlgorithmSpec::new("champion(Ewma360/s1.5)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(360),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0,
            clock,
        )))
    })
}

/// Fraction of trials that fire within `window` after the event at `spm`,
/// for `delta_pct` (0 = no-drop control). Matches detection-control.rs.
fn frac_fired(a: &AlgorithmSpec, trials: usize, spm: f32, delta_pct: i32, seed: u64) -> f64 {
    let (cfg, sched) = Scenario::SettledStep { settle_minutes: SETTLE_MIN, delta_pct }.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..cfg };
    let event_at = SETTLE_MIN * 60;
    let mut hits = 0u64;
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        if t.ticks.iter().any(|tk| tk.fired && tk.t_secs > event_at && tk.t_secs <= event_at + WINDOW_SECS) {
            hits += 1;
        }
    }
    hits as f64 / trials as f64
}

/// EXCESS at a rate, with CI-scaled trials (the floor regime is sparse).
fn excess(a: &AlgorithmSpec, spm: f32, base: usize, seed: u64) -> (f64, f64, f64) {
    let trials = (base as f64 * (12.0 / spm as f64).clamp(1.0, 3.0)).round() as usize;
    let det = frac_fired(a, trials, spm, DROP_PCT, seed);
    let ctrl = frac_fired(a, trials, spm, 0, seed ^ 0x9E37);
    (det - ctrl, det, ctrl)
}

fn main() -> std::io::Result<()> {
    let base: usize = env::var("VARDIFF_EL_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(1500);
    let out = env::var("VARDIFF_EL_OUT").unwrap_or_else(|_| "docs/excess_lever.svg".to_string());
    let seed = DEFAULT_BASELINE_SEED;

    // Champion curve + a small field for the "binds everyone" band.
    let field: Vec<(String, AlgorithmSpec)> = {
        let mk = |tau: u64, s: f64, n: &str| (n.to_string(), AlgorithmSpec::new(format!("Ewma{tau}/s{s}"), move |clock| {
            VardiffBox(Box::new(Composed::new(
                EwmaEstimator::new(tau),
                AdaptiveSignPersist::sign_persist(SignPersistenceCusumBoundary::new(s, 0.05, 8.0, 0.06, 0.6), 6),
                AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
            )))
        }));
        vec![mk(150, 0.3, "f1"), mk(240, 1.0, "f2"), mk(480, 1.0, "f3"), mk(720, 2.0, "f4")]
    };

    eprintln!("excess-lever: champion + {} field, {} rates, base {} trials", field.len(), RATES.len(), base);

    // (which, rate_idx): which=usize::MAX → champion, else field index.
    let champ = champion();
    let jobs: Vec<(usize, usize)> = std::iter::once(usize::MAX)
        .chain(0..field.len())
        .flat_map(|w| (0..RATES.len()).map(move |ri| (w, ri)))
        .collect();
    let next = AtomicUsize::new(0);
    let champ_ex: Mutex<Vec<f64>> = Mutex::new(vec![f64::NAN; RATES.len()]);
    let field_ex: Mutex<Vec<Vec<f64>>> = Mutex::new(vec![vec![f64::NAN; RATES.len()]; field.len()]);
    let nth = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..nth {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (w, ri) = jobs[j];
                let spm = RATES[ri];
                if w == usize::MAX {
                    let (e, _, _) = excess(&champ, spm, base, seed);
                    champ_ex.lock().unwrap()[ri] = e;
                } else {
                    let (e, _, _) = excess(&field[w].1, spm, base, seed);
                    field_ex.lock().unwrap()[w][ri] = e;
                }
            });
        }
    });
    let champ_ex = champ_ex.into_inner().unwrap();
    let field_ex = field_ex.into_inner().unwrap();

    println!("\n## Detection EXCESS vs r* (the lever). window={}min, {}% drop, settle={}min.", WINDOW_SECS/60, DROP_PCT, SETTLE_MIN);
    println!("EXCESS≈0 = floor-limited (drop invisible in-window); EXCESS↑ = floor recedes, agility achievable.\n");
    println!("| r* (spm) | champion EXCESS | field min | field max |");
    println!("| --- | --- | --- | --- |");
    for (ri, &r) in RATES.iter().enumerate() {
        let fmin = field_ex.iter().map(|c| c[ri]).fold(f64::MAX, f64::min);
        let fmax = field_ex.iter().map(|c| c[ri]).fold(f64::MIN, f64::max);
        println!("| {} | {:+.3} | {:+.3} | {:+.3} |", r as u32, champ_ex[ri], fmin, fmax);
    }
    let lo = champ_ex[0];
    let hi = champ_ex[RATES.len()-1];
    println!("\nChampion EXCESS: {:+.3} at {}spm (production, floor-limited) → {:+.3} at {}spm (floor recedes).",
        lo, RATES[0] as u32, hi, RATES[RATES.len()-1] as u32);
    println!("The lever: raising r* by {}× lifts detectability of a {}% drop from ~floor to {:+.2}.",
        (RATES[RATES.len()-1]/RATES[0]) as u32, DROP_PCT, hi);

    fs::write(&out, render(&champ_ex, &field_ex))?;
    eprintln!("Wrote {}", out);
    Ok(())
}

fn render(champ: &[f64], field: &[Vec<f64>]) -> String {
    let (ml, mr, mt, mb) = (72.0, 60.0, 78.0, 58.0);
    let (w, h) = (820i64, 460i64);
    let pw = w as f64 - ml - mr;
    let ph = h as f64 - mt - mb;
    let (x_lo, x_hi) = (RATES[0] as f64 * 0.9, RATES[RATES.len()-1] as f64 * 1.06);
    let all: Vec<f64> = champ.iter().cloned().chain(field.iter().flat_map(|r| r.iter().cloned())).collect();
    let y_hi = all.iter().cloned().fold(f64::MIN, f64::max).max(0.05) * 1.15;
    let y_lo = 0.0f64.min(all.iter().cloned().fold(f64::MAX, f64::min));
    let lx = |x: f64| ml + pw * (x.ln() - x_lo.ln()) / (x_hi.ln() - x_lo.ln());
    let ly = |y: f64| mt + ph * (1.0 - (y - y_lo) / (y_hi - y_lo));

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="{cx}" y="28" text-anchor="middle" font-size="16" font-weight="bold">The lever: raising r* lifts a small drop above the information floor</text>
<text x="{cx}" y="46" text-anchor="middle" font-size="12" fill="#555">False-alarm-corrected detection EXCESS of a −10% drop (15-min window) vs share rate.</text>
<text x="{cx}" y="62" text-anchor="middle" font-size="12" fill="#555">At production the drop is at the floor (60-min window: EXCESS=0.00, invisible); the lever is r*.</text>
"##,
        cx = w / 2
    ));
    // y grid
    let mut yg = (y_lo*20.0).floor()/20.0;
    while yg <= y_hi + 1e-9 {
        let y = ly(yg);
        s.push_str(&format!(
            r##"<line x1="{ml:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#ececec" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="#aaa">{:+.2}</text>
"##,
            ml + pw, ml - 8.0, y + 4.0, yg
        ));
        yg += 0.05;
    }
    // zero line emphasized (the floor level)
    let yz = ly(0.0);
    s.push_str(&format!(
        r##"<line x1="{ml:.1}" y1="{yz:.1}" x2="{:.1}" y2="{yz:.1}" stroke="#bbb" stroke-width="1.4"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#999">floor (drop invisible in-window)</text>
"##,
        ml + pw, ml + pw, yz - 5.0
    ));
    // x grid (rates)
    for &xg in RATES {
        let x = lx(xg as f64);
        s.push_str(&format!(
            r##"<line x1="{x:.1}" y1="{mt:.1}" x2="{x:.1}" y2="{:.1}" stroke="#ececec" stroke-width="1"/>
<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#999">{}</text>
"##,
            mt + ph, mt + ph + 18.0, xg as u32
        ));
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666">share rate r* (shares/min, log)</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666" transform="rotate(-90 {:.1} {:.1})">detection EXCESS (false-alarm-corrected)</text>
"##,
        ml + pw / 2.0, mt + ph + 42.0, ml - 50.0, mt + ph / 2.0, ml - 50.0, mt + ph / 2.0
    ));
    // production band shading (4–6 spm = floor-limited regime)
    let (px0, px1) = (lx(4.0), lx(6.0));
    s.push_str(&format!(
        r##"<rect x="{px0:.1}" y="{mt:.1}" width="{:.1}" height="{ph:.1}" fill="#000" fill-opacity="0.03"/>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#888">production</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#888">(floor-limited)</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="9" fill="#999">60-min: 0.00</text>
"##,
        px1 - px0,
        (px0 + px1) / 2.0, mt + 14.0,
        (px0 + px1) / 2.0, mt + 27.0,
        (px0 + px1) / 2.0, mt + 41.0
    ));
    // field lines (faint, the "binds everyone" band)
    for row in field {
        let mut d = String::new();
        for (ri, &v) in row.iter().enumerate() {
            d.push_str(&format!("{}{:.1},{:.1} ", if ri==0 {"M"} else {"L"}, lx(RATES[ri] as f64), ly(v)));
        }
        s.push_str(&format!(r##"<path d="{}" fill="none" stroke="#9aa0a6" stroke-width="1" stroke-opacity="0.4"/>
"##, d.trim()));
    }
    // champion line (bold)
    let mut d = String::new();
    for (ri, &v) in champ.iter().enumerate() {
        d.push_str(&format!("{}{:.1},{:.1} ", if ri==0 {"M"} else {"L"}, lx(RATES[ri] as f64), ly(v)));
    }
    s.push_str(&format!(r##"<path d="{}" fill="none" stroke="#2e8b2e" stroke-width="3.0"/>
"##, d.trim()));
    for (ri, &v) in champ.iter().enumerate() {
        s.push_str(&format!(r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="#2e8b2e"/>
"##, lx(RATES[ri] as f64), ly(v)));
    }
    // legend
    let lgx = ml + pw - 150.0; let lgy = mt + 12.0;
    s.push_str(&format!(
        r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#2e8b2e" stroke-width="3"/>
<text x="{:.1}" y="{:.1}" font-size="12">champion (Ewma360/s1.5)</text>
<line x1="{lgx:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#9aa0a6" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" font-size="12" fill="#777">field</text>
"##,
        lgx + 26.0, lgx + 32.0, lgy + 4.0,
        lgy + 20.0, lgx + 26.0, lgy + 20.0, lgx + 32.0, lgy + 24.0
    ));
    s.push_str("</svg>\n");
    s
}
