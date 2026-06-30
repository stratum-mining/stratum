//! Composite trajectory plot — the plain-language algorithm comparison.
//!
//! The regret/effort radar answers "which algorithm wins the §10 cost",
//! but it abstracts away TIME, and ramp-up speed + detection latency ARE
//! time. This bin draws the one picture a non-expert reads instantly:
//! the algorithm's difficulty-implied hashrate estimate chasing the true
//! miner hashrate over a single continuous timeline. "The line should
//! follow the dashed target; sooner and tighter is better."
//!
//! ## The composite scenario (one timeline, three phases)
//!
//!   Phase 1  COLD START  (0 → SETTLE_AT): estimate starts 10^5× low,
//!            must ramp up to truth — shows RAMP-UP TIME.
//!   Phase 2  SETTLE      (SETTLE_AT → DROP_AT): hold at truth so the
//!            share counter ages ~60 min, the operationally common state.
//!   Phase 3  AGED DROP   (DROP_AT → end): true hashrate sags −10% on the
//!            aged counter — the failing-ASIC case. Shows DETECTION: how
//!            long (if ever) until the algorithm notices and follows down.
//!
//! Each algorithm's line is the PER-TICK MEDIAN across N trials (robust to
//! the Poisson share noise); truth is the dashed reference. The classic
//! line famously ramps slowly AND never catches the aged −10% drop within
//! the window — the two failures the new metric was built to expose, both
//! visible in one frame.
//!
//! Usage: `cargo run --release --bin trajectory-plot`
//! Env: VARDIFF_TRAJ_TRIALS (default 400), VARDIFF_TRAJ_SPM (default 12),
//!      VARDIFF_TRAJ_OUT (svg path, default trajectory_plot.svg).

use std::env;
use std::fs;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator, FullRetargetNoClamp,
    SignPersistenceCusumBoundary, StepFunction,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{COLD_START_INITIAL_HASHRATE, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::schedule::HashrateSchedule;
use vardiff_sim::trial::{run_trial_observed, Trial, TrialConfig};

// ---- Timeline geometry (simulated seconds) -------------------------------
const SETTLE_AT: u64 = 25 * 60; // end of cold-start ramp window
const DROP_AT: u64 = 85 * 60; // counter has aged ~60 min by here
const END: u64 = 145 * 60; // 60 min to observe (or miss) the drop
const TICK: u64 = 60;
const DROP_FRAC: f32 = 0.90; // −10% sag

const COLORS: &[&str] = &["#777777", "#377eb8", "#4daf4a", "#e41a1c", "#ff7f00", "#984ea3"];

/// The NEW champion: the minimax-over-r* / corrected-metric winner, confirmed
/// also as the band champion (the configs that beat it on band-cost fail the
/// decline-safety gate). Gentler, longer window than the old s0.3 champion —
/// Ewma360 / SignPersist-s1.5 / t8 / d0.06 / etaMax0.6 / accel0.05.
fn champion_new() -> AlgorithmSpec {
    AlgorithmSpec::new("new champion (Ewma360/s1.5)", |clock| {
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

/// The "oracle" reference: the SAME estimator belief as the champion
/// (EWMA τ=150), but with the control policy stripped away — it fires on
/// any deviation (threshold 0) and retargets straight to its estimate
/// (no partial retarget, no clamp, no tighten asymmetry). It therefore
/// tracks the best a τ=150 estimator can, limited only by irreducible
/// Poisson measurement noise. Plotting it decomposes the champion's
/// settle-phase offset into two parts:
///   gap(oracle → truth)    = irreducible noise — nobody can do better
///   gap(champion → oracle) = the cost of the asymmetric control policy
/// i.e. how much of the offset is physics vs. a deliberate choice.
fn oracle() -> AlgorithmSpec {
    AlgorithmSpec::new("oracle (Ewma150 MLE, no policy)", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            // Threshold 0 for all dt ⇒ fires on any nonzero deviation.
            StepFunction {
                table: vec![(u64::MAX, 0.0)],
            },
            FullRetargetNoClamp,
            1.0,
            clock,
        )))
    })
}

/// Per-fire retarget magnitude for one trial: `(t_secs, |Δln D|)` where
/// Δln D = ln(new_hashrate / current_hashrate_before). This is the
/// "violence" of each fire — a full classic retarget is a big |Δln D|, a
/// gentle partial nudge is small. Used to size the fire-raster marks.
fn fire_magnitudes(t: &Trial) -> Vec<(u64, f64)> {
    t.ticks
        .iter()
        .filter(|tk| tk.fired)
        .filter_map(|tk| {
            let old = tk.current_hashrate_before as f64;
            tk.new_hashrate.map(|nh| nh as f64).and_then(|nh| {
                if old > 0.0 && nh > 0.0 {
                    Some((tk.t_secs, (nh / old).ln().abs()))
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Median of a slice (returns lower-mid for even n). Mutates via sort of a
/// scratch copy.
fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    let mut v: Vec<f64> = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() -> std::io::Result<()> {
    let trials: usize = env::var("VARDIFF_TRAJ_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400);
    let spm: f32 = env::var("VARDIFF_TRAJ_SPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12.0);
    let out_path = env::var("VARDIFF_TRAJ_OUT").unwrap_or_else(|_| "trajectory_plot.svg".to_string());

    // Composite schedule: truth = TRUE_HASHRATE through the cold-start and
    // settle phases, then −10% at DROP_AT. (Cold start is about the
    // ESTIMATE being low, not truth — truth is flat here; the estimate
    // starts low via TrialConfig::initial_hashrate.)
    let schedule = HashrateSchedule::new(vec![
        (0, TRUE_HASHRATE),
        (DROP_AT, TRUE_HASHRATE * DROP_FRAC),
    ]);
    let config = TrialConfig {
        duration_secs: END,
        initial_hashrate: COLD_START_INITIAL_HASHRATE,
        shares_per_minute: spm,
        tick_interval_secs: TICK,
    };

    // The three the comparison asks for: original vardiff (classic baseline),
    // the OLD winner (s0.3 SignPersist champion), and the NEW winner (the
    // corrected-metric / minimax-over-r* champion Ewma360/s1.5). Ordered
    // baseline → old → new so the legend and colors read as the progression.
    // Each gets a median line AND a fire-raster lane.
    let algos = vec![
        ("classic (original vardiff)", AlgorithmSpec::classic_composed()),
        ("old champion (s0.3)", AlgorithmSpec::champion()),
        ("new champion (Ewma360/s1.5)", champion_new()),
    ];

    let n_ticks = (END / TICK) as usize;
    // Representative single trial (for the fire raster — medians smear fires
    // out, so the raster reads ONE concrete run). Same seed for every algo
    // so they see the identical share stream.
    const REP_SEED: u64 = 0x7A1;
    eprintln!(
        "trajectory-plot: {} algos (+oracle) × {} trials, spm {}, {} ticks over {} min",
        algos.len(),
        trials,
        spm,
        n_ticks,
        END / 60
    );

    // Per algo: per-tick median of current_hashrate_before across trials,
    // plus the fires (t_secs, |Δln D|) from the representative trial.
    let mut series: Vec<(String, Vec<f64>)> = Vec::new();
    let mut rasters: Vec<(String, Vec<(u64, f64)>)> = Vec::new();
    for (label, algo) in &algos {
        let mut per_tick: Vec<Vec<f64>> = vec![Vec::with_capacity(trials); n_ticks];
        for i in 0..trials {
            let clock = Arc::new(MockClock::new(0));
            let v = (algo.factory)(clock.clone());
            let t = run_trial_observed(v, clock, config.clone(), &schedule, REP_SEED.wrapping_add(i as u64));
            for (ti, tick) in t.ticks.iter().enumerate() {
                if ti < n_ticks {
                    per_tick[ti].push(tick.current_hashrate_before as f64);
                }
            }
            if i == 0 {
                rasters.push((label.to_string(), fire_magnitudes(&t)));
            }
        }
        let med: Vec<f64> = per_tick.iter().map(|c| median(c)).collect();
        series.push((label.to_string(), med));
    }

    // Oracle reference line: same EWMA(150) belief, no control policy —
    // the best a τ=150 estimator can track. Median across trials, like the
    // contenders. Kept SEPARATE so it renders as a faint reference (not a
    // contender) and gets no fire lane (it fires every tick by design).
    let oracle_spec = oracle();
    let mut oracle_pt: Vec<Vec<f64>> = vec![Vec::with_capacity(trials); n_ticks];
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (oracle_spec.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &schedule, REP_SEED.wrapping_add(i as u64));
        for (ti, tick) in t.ticks.iter().enumerate() {
            if ti < n_ticks {
                oracle_pt[ti].push(tick.current_hashrate_before as f64);
            }
        }
    }
    let oracle_line: Vec<f64> = oracle_pt.iter().map(|c| median(c)).collect();

    // Truth line, per tick (at tick end time).
    let truth: Vec<f64> = (0..n_ticks)
        .map(|ti| schedule.at((ti as u64 + 1) * TICK) as f64)
        .collect();

    let svg = render(&series, &rasters, &oracle_line, &truth, n_ticks, spm);
    fs::write(&out_path, &svg)?;
    eprintln!("Wrote {}", out_path);

    // Console: key readouts — ramp-up time to ±10% of truth, and detection
    // latency after the drop (first tick whose median crosses below the
    // halfway point between old and new truth).
    println!("\n## Trajectory readouts (median across {} trials, spm {})\n", trials, spm);
    println!("| algo | ramp-up to ±10% | follows −10% drop? | detect latency | settle gap vs truth |");
    println!("| --- | --- | --- | --- | --- |");
    let drop_tick = (DROP_AT / TICK) as usize;
    let drop_abs = TRUE_HASHRATE as f64 * (1.0 - DROP_FRAC as f64); // the −10% in absolute H/s
    // Settle-phase window: the 10 ticks just before the drop. Used to
    // quantify the steady-state offset, with the oracle as the floor.
    let settle = |line: &[f64]| {
        let w = &line[drop_tick.saturating_sub(10)..drop_tick];
        (median(w) / TRUE_HASHRATE as f64 - 1.0) * 100.0 // signed % vs truth
    };
    let oracle_gap = settle(&oracle_line);
    for (label, med) in &series {
        // ramp-up: first tick within ±10% of TRUE_HASHRATE.
        let ramp = med
            .iter()
            .position(|&h| (h - TRUE_HASHRATE as f64).abs() <= 0.10 * TRUE_HASHRATE as f64)
            .map(|ti| format!("{} min", (ti as u64 + 1) * TICK / 60))
            .unwrap_or_else(|| "never".into());
        // Detection measured against the algorithm's OWN pre-drop baseline
        // (the median over the 10 ticks just before the drop), NOT an
        // absolute line — an algorithm that already sits under truth must
        // not be credited with "instant" detection. "Followed" = the
        // estimate moves DOWN by at least half the drop magnitude relative
        // to its own baseline, within the 60-min window.
        let pre = median(&med[drop_tick.saturating_sub(10)..drop_tick]);
        let threshold = pre - 0.5 * drop_abs;
        let det = med
            .iter()
            .enumerate()
            .skip(drop_tick)
            .find(|(_, &h)| h <= threshold)
            .map(|(ti, _)| ((ti - drop_tick) as u64 * TICK / 60))
            .map(|d| ("yes".to_string(), format!("{} min", d)))
            .unwrap_or_else(|| ("NO".into(), "—".into()));
        println!(
            "| {} | {} | {} | {} | {:+.1}% |",
            label, ramp, det.0, det.1, settle(med)
        );
    }
    // Oracle ramp-up: the estimator's OWN cold-start floor (no policy). If
    // a contender's ramp ≈ this, the remaining slowness is the τ=150
    // estimator, not the control policy — so a policy-only fix (warm-up
    // mode) can't help; you'd have to shorten τ during cold start.
    let oracle_ramp = oracle_line
        .iter()
        .position(|&h| (h - TRUE_HASHRATE as f64).abs() <= 0.10 * TRUE_HASHRATE as f64)
        .map(|ti| format!("{} min", (ti as u64 + 1) * TICK / 60))
        .unwrap_or_else(|| "never".into());
    println!(
        "\nAccuracy ceiling (cost-blind, fires every tick) settle gap at τ=150: {:+.1}%. \
         NOT a target — confirm-debias proved closing a contender's gap to this raises §10 cost monotonically.",
        oracle_gap
    );
    println!(
        "Accuracy-ceiling ramp-up to ±10% (estimator-only floor at τ=150): {}. \
         A contender's ramp near this is estimator-limited, not policy-limited.",
        oracle_ramp
    );
    Ok(())
}

fn render(
    series: &[(String, Vec<f64>)],
    rasters: &[(String, Vec<(u64, f64)>)],
    oracle_line: &[f64],
    truth: &[f64],
    n_ticks: usize,
    spm: f32,
) -> String {
    let ml = 70.0;
    let mr = 230.0; // wide right for legend
    let mt = 70.0;
    let w = 1100i64;
    let pw = w as f64 - ml - mr;
    let ph = 360.0; // plot region height (fixed; raster strip sits below)

    // Raster strip geometry: one lane per contender, below the x-axis.
    let lane_h = 22.0;
    let raster_top = mt + ph + 64.0; // leave room for x-axis ticks + label
    let raster_h = lane_h * series.len() as f64;
    let h = (raster_top + raster_h + 36.0) as i64;

    // Y axis: hashrate in units of 1e15 (= TRUE_HASHRATE), range 0..1.25.
    let y_max = 1.25f64;
    let unit = TRUE_HASHRATE as f64;
    let xt = |ti: usize| ml + pw * ti as f64 / (n_ticks - 1).max(1) as f64;
    let xs = |t_secs: u64| ml + pw * (t_secs as f64 / END as f64).clamp(0.0, 1.0);
    let yv = |h_units: f64| mt + ph * (1.0 - (h_units / y_max).clamp(0.0, 1.0));

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="{cx}" y="30" text-anchor="middle" font-size="16" font-weight="bold">Estimate chasing truth — ramp-up, settle, and an aged −10% drop</text>
<text x="{cx}" y="50" text-anchor="middle" font-size="12" fill="#555">Difficulty-implied hashrate (median of trials). Dashed = truth; dotted = accuracy ceiling (cost-blind); green band = cost-optimal settle level. spm={spm}.</text>
"##,
        cx = w / 2,
    ));

    // Phase bands + labels.
    let x_settle = xt((SETTLE_AT / TICK) as usize - 1);
    let x_drop = xt((DROP_AT / TICK) as usize - 1);
    s.push_str(&format!(
        r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#000" fill-opacity="0.03"/>
<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#e41a1c" fill-opacity="0.04"/>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#777">① cold-start ramp</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#777">② settle (counter ages ~60min)</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#c0392b">③ aged −10% drop</text>
"##,
        ml, mt, x_settle - ml, ph,                                  // cold band
        x_drop, mt, ml + pw - x_drop, ph,                           // drop band
        (ml + x_settle) / 2.0, mt - 8.0,
        (x_settle + x_drop) / 2.0, mt - 8.0,
        (x_drop + ml + pw) / 2.0, mt - 8.0,
    ));

    // Y gridlines + labels (0, 0.25, .., 1.25 ×1e15).
    let mut yg = 0.0;
    while yg <= y_max + 1e-9 {
        let y = yv(yg);
        s.push_str(&format!(
            r##"<line x1="{ml:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#ddd" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="#999">{:.2}</text>
"##,
            ml + pw, ml - 8.0, y + 4.0, yg
        ));
        yg += 0.25;
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#999" transform="rotate(-90 {:.1} {:.1})">hashrate (×10¹⁵ H/s)</text>
"##,
        ml - 44.0, mt + ph / 2.0, ml - 44.0, mt + ph / 2.0
    ));

    // X axis ticks every 20 min.
    let mut xm = 0u64;
    while xm * 60 <= END {
        let ti = ((xm * 60) / TICK) as usize;
        let x = xt(ti.min(n_ticks - 1));
        s.push_str(&format!(
            r##"<line x1="{x:.1}" y1="{:.1}" x2="{x:.1}" y2="{:.1}" stroke="#ddd" stroke-width="1"/>
<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#999">{}</text>
"##,
            mt, mt + ph, mt + ph + 18.0, xm
        ));
        xm += 20;
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#999">minutes</text>
"##,
        ml + pw / 2.0, mt + ph + 42.0
    ));

    // Cost-optimal corridor: the champion's own settle level. We proved
    // (confirm-debias) that under the §10 objective the cost minimum sits
    // at the champion's bias=1.0 — i.e. THIS level, not the accuracy
    // ceiling, is optimal. Shade a thin band at the champion's back-half
    // settle median across the settle phase so the champion line visibly
    // sits IN the optimal corridor rather than "failing" the dotted ceiling.
    let drop_tick = (DROP_AT / TICK) as usize;
    if let Some((_, champ)) = series.last() {
        let w0 = drop_tick.saturating_sub(10);
        if drop_tick <= champ.len() && w0 < drop_tick {
            let lvl = median(&champ[w0..drop_tick]) / unit; // in 1e15 units
            let half = 0.01; // ±1% visual thickness
            let (x0, x1) = (xt((SETTLE_AT / TICK) as usize - 1), xt(drop_tick - 1));
            s.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#4daf4a" fill-opacity="0.18"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#3a8a3a">cost-optimal</text>
"##,
                x0,
                yv(lvl + half),
                (x1 - x0).max(0.0),
                (yv(lvl - half) - yv(lvl + half)).max(0.0),
                x1 - 4.0,
                yv(lvl + half) - 3.0,
            ));
        }
    }

    // Truth (dashed).
    let mut tp = String::new();
    for (ti, &hu) in truth.iter().enumerate() {
        tp.push_str(&format!("{}{:.1},{:.1}", if ti == 0 { "M" } else { "L" }, xt(ti), yv(hu / unit)));
        tp.push(' ');
    }
    s.push_str(&format!(
        r##"<path d="{}" fill="none" stroke="#111" stroke-width="2" stroke-dasharray="6 5" stroke-opacity="0.8"/>
"##,
        tp.trim()
    ));

    // Accuracy-ceiling line (faint, before the contenders so they draw on
    // top): the best a τ=150 estimator can track, but COST-BLIND — it
    // reaches this only by firing every tick. confirm-debias proved the
    // gap from a contender to here is NOT recoverable daylight: closing it
    // raises §10 cost monotonically. So this is an accuracy bound, not a
    // target; the cost-optimal level is the green corridor above.
    let mut op = String::new();
    for (ti, &hu) in oracle_line.iter().enumerate() {
        op.push_str(&format!("{}{:.1},{:.1}", if ti == 0 { "M" } else { "L" }, xt(ti), yv(hu / unit)));
        op.push(' ');
    }
    s.push_str(&format!(
        r##"<path d="{}" fill="none" stroke="#9467bd" stroke-width="1.5" stroke-opacity="0.55" stroke-dasharray="2 3"/>
"##,
        op.trim()
    ));

    // Algorithm lines.
    for (idx, (_label, med)) in series.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        let mut p = String::new();
        for (ti, &hu) in med.iter().enumerate() {
            p.push_str(&format!("{}{:.1},{:.1}", if ti == 0 { "M" } else { "L" }, xt(ti), yv(hu / unit)));
            p.push(' ');
        }
        s.push_str(&format!(
            r##"<path d="{}" fill="none" stroke="{color}" stroke-width="2.2" stroke-opacity="0.9"/>
"##,
            p.trim()
        ));
    }

    // ---- Fire-raster strip: one lane per contender. Each fire is a mark at
    // its time; mark HEIGHT ∝ |Δln D| (retarget violence). Spacing shows
    // "slowly", size shows "violently", marks just after ③ show reactivity
    // to the small drop (classic's lane stays empty there = the blind spot).
    let max_mag = rasters
        .iter()
        .flat_map(|(_, fs)| fs.iter().map(|&(_, m)| m))
        .fold(0.0f64, f64::max)
        .max(1e-9);
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" font-size="12" font-weight="bold" fill="#333">Fires (mark height ∝ retarget size |Δln D|): few+tall = slow &amp; violent, many+short = quick &amp; gentle</text>
"##,
        ml, raster_top - 12.0
    ));
    // Phase dividers across the raster (settle + drop boundaries).
    for &bx in &[xt((SETTLE_AT / TICK) as usize - 1), xt((DROP_AT / TICK) as usize - 1)] {
        s.push_str(&format!(
            r##"<line x1="{bx:.1}" y1="{:.1}" x2="{bx:.1}" y2="{:.1}" stroke="#ccc" stroke-width="1"/>
"##,
            raster_top, raster_top + raster_h
        ));
    }
    for (idx, (label, fires)) in rasters.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        let lane_y = raster_top + lane_h * idx as f64;
        let baseline = lane_y + lane_h - 4.0;
        // lane baseline + label
        s.push_str(&format!(
            r##"<line x1="{ml:.1}" y1="{baseline:.1}" x2="{:.1}" y2="{baseline:.1}" stroke="#eee" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="{color}">{}</text>
"##,
            ml + pw, ml - 6.0, baseline, short_label(label)
        ));
        for &(t_secs, mag) in fires {
            let x = xs(t_secs);
            let bar_h = 3.0 + (lane_h - 7.0) * (mag / max_mag);
            s.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="2.6" height="{:.1}" fill="{color}" fill-opacity="0.85"/>
"##,
                x - 1.3, baseline - bar_h, bar_h
            ));
        }
    }

    // Legend (right margin).
    let lx = ml + pw + 22.0;
    let mut ly = mt + 6.0;
    s.push_str(&format!(
        r##"<line x1="{lx:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#111" stroke-width="2" stroke-dasharray="6 5"/>
<text x="{:.1}" y="{:.1}" font-size="12">true hashrate</text>
"##,
        ly, lx + 26.0, ly, lx + 32.0, ly + 4.0
    ));
    ly += 24.0;
    for (idx, (label, _)) in series.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        s.push_str(&format!(
            r##"<line x1="{lx:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="3"/>
<text x="{:.1}" y="{:.1}" font-size="12">{}</text>
"##,
            ly, lx + 26.0, ly, lx + 32.0, ly + 4.0, label
        ));
        ly += 24.0;
    }
    // Accuracy-ceiling entry (faint dotted) — relabeled from "oracle".
    s.push_str(&format!(
        r##"<line x1="{lx:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#9467bd" stroke-width="1.5" stroke-opacity="0.7" stroke-dasharray="2 3"/>
<text x="{:.1}" y="{:.1}" font-size="12" fill="#6a4a8a">accuracy ceiling (cost-blind)</text>
"##,
        ly, lx + 26.0, ly, lx + 32.0, ly + 4.0
    ));
    ly += 24.0;
    // Cost-optimal corridor swatch.
    s.push_str(&format!(
        r##"<rect x="{lx:.1}" y="{:.1}" width="26" height="12" fill="#4daf4a" fill-opacity="0.18"/>
<text x="{:.1}" y="{:.1}" font-size="12" fill="#3a8a3a">cost-optimal settle (§10)</text>
"##,
        ly - 9.0, lx + 32.0, ly + 1.0
    ));

    s.push_str("</svg>\n");
    s
}

/// First word of an algo label, for the cramped raster lane labels.
fn short_label(label: &str) -> &str {
    label.split_whitespace().next().unwrap_or(label)
}
