//! Principled regret/effort radar — the §10 replacement for the
//! six-axis `EqualWeightFitness` radar.
//!
//! Five axes, each a genuinely independent operational cost (not a
//! correlated projection), each higher-is-better, **normalized against
//! the ClassicComposed reference** (ClassicComposed lands at the 0.5
//! mid-ring; outward = beats it, inward = worse). ClassicComposed is
//! the introspectable three-stage decomposition that is fire-for-fire
//! equivalent to the production monolith, so it is the same behavioral
//! baseline while also exposing per-tick state. No arbitrary `/0.30`-style
//! ceilings, and — unlike hull-normalization — stable run-to-run and
//! meaningful for a single algorithm without needing a strawman in the
//! candidate set:
//!
//!   - tracking   : −regret_lin  (linear log-error ⟨|e|⟩; §10 chose
//!                  LINEAR so small persistent drops stay visible)
//!   - gentleness : −effort      (Σ(Δln D)² over fires)
//!   - detection  : P[fire within 60min | aged-counter −10% drop]
//!                  (the SEPARATE axis §9.4 proved is not derivable
//!                  from e(t))
//!   - over-safety: −regret_over (over-difficulty time; death-spiral side)
//!   - tighten-care: −effort_up  (costly tightening fires specifically)
//!
//! Raw costs are averaged across SPM and the relevant scenario classes,
//! then each axis is mapped cost→score by `1 − raw/max_raw` so the
//! worst candidate sits near 0 and the best near 1 on that axis.
//!
//! Usage: `cargo run --release --bin regret-radar`
//! Env: VARDIFF_RR_TRIALS (default 1000), VARDIFF_RR_OUT (svg path).

use std::env;
use std::f64::consts::PI;
use std::fs;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptivePoissonCusum, AsymmetricCusumBoundary, Composed,
    EwmaEstimator, PoissonCI,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{
    Cell, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT, MIN_SETTLED_WINDOW_SECS,
    QUIET_WINDOW_SECS, REACT_WINDOW_SECS, SETTLE_BUFFER_SECS, STEP_EVENT_AT_SECS,
};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, Trial};
use vardiff_sim::{
    convergence_time_distribution, jitter_distribution, reaction_time_distribution,
    settled_accuracy_distribution, LogErrorRegret, Metric,
};

const AXES: &[&str] = &[
    "Tracking",
    "Gentleness",
    "Detection",
    "Over-diff safety",
    "Tighten care",
];

/// Axis labels for the familiar-metrics radar (raw measurements, all
/// lower-is-better, mapped to a log-ratio score vs the reference so
/// "outward = better, mid-ring = ties classic" holds — see `log_score`
/// in main for why this panel uses a log scale rather than the left
/// panel's ref/(ref+val)).
// Order groups the two SPEED axes (Convergence, Reaction) adjacent and
// the two STABILITY axes (Settled accuracy, Jitter) adjacent, so each
// related pair forms a contiguous edge of the polygon.
const FAM_AXES: &[&str] = &[
    "Convergence",
    "Reaction (−50%)",
    "Settled accuracy",
    "Jitter",
];

const COLORS: &[&str] = &["#e41a1c", "#377eb8", "#4daf4a", "#984ea3", "#ff7f00"];

/// Raw per-algorithm costs before hull-normalization.
struct RawCost {
    name: String,
    regret_lin: f64,
    effort: f64,
    detection: f64, // already a benefit in [0,1]; higher = better
    regret_over: f64,
    effort_up: f64,
}

/// Familiar, human-anchored measurements plotted as a second panel so
/// the abstract regret/effort axes can be cross-read against quantities
/// people already have intuition for. Raw units (not normalized):
struct Familiar {
    name: String,
    convergence_min: f64,   // cold-start p50 time to converge (minutes)
    settled_accuracy: f64,  // stable p50 |H_est/H_true − 1| (fraction)
    reaction_min: f64,      // −50% step p50 reaction time (minutes)
    jitter_per_min: f64,    // stable post-convergence fires/min
}

fn run_cell(algo: &AlgorithmSpec, scen: &Scenario, spm: f32, trials: usize, seed: u64) -> Vec<Trial> {
    let (config, schedule) = scen.build(spm);
    let mut out = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (algo.factory)(clock.clone());
        out.push(run_trial_observed(v, clock, config.clone(), &schedule, seed.wrapping_add(i as u64)));
    }
    out
}

/// The interim AsymCusum champion (pre-SignPersist) — kept as a plotted
/// comparison so the radar shows what the sign-persistence discount bought
/// over the AsymCusum-only config. `Ewma150 / AsymCusum-s0.2-t6 /
/// Accel-0.2-0.8-0.05`.
fn champion_asymcusum() -> AlgorithmSpec {
    let name = vardiff_sim::naming::triple_name(
        &EwmaEstimator::new(150),
        &AdaptivePoissonCusum::with_params(
            PoissonCI::default_parametric(),
            AsymmetricCusumBoundary::new(0.2, 0.05, 6.0),
            5,
        ),
        &AcceleratingPartialRetarget::new(0.2, 0.8, 0.05),
    );
    AlgorithmSpec::new(name, |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptivePoissonCusum::with_params(
                PoissonCI::default_parametric(),
                AsymmetricCusumBoundary::new(0.2, 0.05, 6.0),
                5,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.8, 0.05),
            1.0,
            clock,
        )))
    })
}

fn main() -> std::io::Result<()> {
    let trials: usize = env::var("VARDIFF_RR_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TRIAL_COUNT);
    let out_path = env::var("VARDIFF_RR_OUT").unwrap_or_else(|_| "regret_radar.svg".to_string());

    // raws[0] MUST be the reference algorithm: every cost axis is
    // normalized against it (it lands at the 0.5 mid-ring). We anchor on
    // ClassicComposed — the introspectable three-stage build that is
    // fire-for-fire equivalent to the production monolith, so it is the
    // same behavioral baseline we want to beat. With reference-relative
    // scoring no strawman is needed for spread — contenders are read
    // directly as "beats / loses to classic" per axis. (To audit a
    // known-bad algorithm, append parametric() here; it'll plot near
    // center.)
    let algos = vec![
        ("classic_composed", AlgorithmSpec::classic_composed()),
        ("balanced", AlgorithmSpec::balanced()),
        ("react_priority", AlgorithmSpec::react_priority()),
        ("champion (SignPersist)", AlgorithmSpec::champion()),
        ("interim (AsymCusum)", champion_asymcusum()),
    ];
    let spms: [f32; 4] = [6.0, 12.0, 20.0, 30.0];
    let metric = LogErrorRegret;

    eprintln!("regret-radar: {} algos × {} SPM, {} trials/cell", algos.len(), spms.len(), trials);

    let mut raws: Vec<RawCost> = Vec::new();
    let mut fams: Vec<Familiar> = Vec::new();
    for (label, algo) in &algos {
        // Average regret/effort over a transient+steady mix (cold-start
        // excluded — one-time, dominates and washes out differences).
        let mut regret_lin = 0.0;
        let mut regret_over = 0.0;
        let mut effort = 0.0;
        let mut effort_up = 0.0;
        let mut n = 0.0;
        for &spm in &spms {
            let mut scens = vec![Scenario::Stable];
            for &d in &[-50i32, -10, 10, 50] {
                scens.push(Scenario::Step { delta_pct: d });
            }
            for (si, scen) in scens.iter().enumerate() {
                let seed = DEFAULT_BASELINE_SEED.wrapping_add((si as u64) << 12).wrapping_add(spm as u64);
                let ts = run_cell(algo, scen, spm, trials, seed);
                let cell = Cell { shares_per_minute: spm, scenario: scen.clone() };
                let mv = metric.compute(&ts, &cell);
                regret_lin += mv.get("regret_lin").unwrap_or(0.0);
                regret_over += mv.get("regret_over").unwrap_or(0.0);
                effort += mv.get("effort").unwrap_or(0.0);
                effort_up += mv.get("effort_up").unwrap_or(0.0);
                n += 1.0;
            }
        }
        // Detection: aged-counter −10% drop, P[fire within 60min], avg over SPM.
        let mut det = 0.0;
        let mut dn = 0.0;
        for &spm in &spms {
            let scen = Scenario::SettledStep { settle_minutes: 60, delta_pct: -10 };
            let seed = DEFAULT_BASELINE_SEED.wrapping_add(0xD17).wrapping_add(spm as u64);
            let ts = run_cell(algo, &scen, spm, trials, seed);
            let event_at = 60 * 60u64;
            let window_end = event_at + 60 * 60;
            let hit = ts.iter().filter(|t| {
                t.ticks.iter().any(|tk| tk.fired && tk.t_secs > event_at && tk.t_secs <= window_end)
            }).count();
            det += hit as f64 / ts.len() as f64;
            dn += 1.0;
        }
        raws.push(RawCost {
            name: label.to_string(),
            regret_lin: regret_lin / n,
            effort: effort / n,
            detection: det / dn,
            regret_over: regret_over / n,
            effort_up: effort_up / n,
        });

        // Familiar metrics (raw units), each from its canonical scenario,
        // averaged over SPM. These mirror the existing baseline tables so
        // the regret radar can be cross-read against intuitive numbers.
        let (mut conv, mut acc, mut react, mut jit, mut fc) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &spm in &spms {
            // cold-start convergence p50 (seconds → minutes)
            let cs = run_cell(algo, &Scenario::ColdStart, spm, trials,
                DEFAULT_BASELINE_SEED.wrapping_add(0xC04).wrapping_add(spm as u64));
            let (_, cdist) = convergence_time_distribution(&cs, QUIET_WINDOW_SECS);
            conv += cdist.p50().unwrap_or(0.0) / 60.0;
            // stable: settled accuracy p50 + jitter mean/min
            let st = run_cell(algo, &Scenario::Stable, spm, trials,
                DEFAULT_BASELINE_SEED.wrapping_add(0x57A).wrapping_add(spm as u64));
            acc += settled_accuracy_distribution(&st).p50().unwrap_or(0.0);
            jit += jitter_distribution(&st, QUIET_WINDOW_SECS, SETTLE_BUFFER_SECS, MIN_SETTLED_WINDOW_SECS)
                .mean().unwrap_or(0.0);
            // −50% step reaction p50 (seconds → minutes)
            let rs = run_cell(algo, &Scenario::Step { delta_pct: -50 }, spm, trials,
                DEFAULT_BASELINE_SEED.wrapping_add(0x4EA).wrapping_add(spm as u64));
            let (_, rdist) = reaction_time_distribution(&rs, STEP_EVENT_AT_SECS, REACT_WINDOW_SECS);
            react += rdist.p50().unwrap_or(0.0) / 60.0;
            fc += 1.0;
        }
        fams.push(Familiar {
            name: label.to_string(),
            convergence_min: conv / fc,
            settled_accuracy: acc / fc,
            reaction_min: react / fc,
            jitter_per_min: jit / fc,
        });
    }

    // Normalize against the CLASSICCOMPOSED REFERENCE (raws[0]), not the
    // candidate-set hull. Rationale: hull-normalization makes the chart
    // relative to whoever you happened to include (unstable run to run;
    // needs a strawman to show spread). ClassicComposed is a stable,
    // principled, non-arbitrary reference — the production-equivalent
    // algorithm we want to beat. For a cost axis, score = ref/(ref+cost):
    // the reference lands at exactly 0.5 (mid-ring), beating it pushes
    // outward (→1), losing pulls inward (→0), always bounded in (0,1).
    // Detection is an absolute probability — plot it RAW (normalizing a
    // probability by the set max would falsely promote the best to 1.0).
    let reference = &raws[0];
    let cost_score = |reff: f64, cost: f64| reff / (reff + cost).max(1e-12);

    let profiles: Vec<(String, [f64; 5])> = raws
        .iter()
        .map(|r| {
            (
                r.name.clone(),
                [
                    cost_score(reference.regret_lin, r.regret_lin),
                    cost_score(reference.effort, r.effort),
                    r.detection, // raw probability in [0,1]
                    cost_score(reference.regret_over, r.regret_over),
                    cost_score(reference.effort_up, r.effort_up),
                ],
            )
        })
        .collect();

    // Markdown table (raw + normalized) to stdout.
    println!("\n## Regret/effort radar — raw costs (lower better, except Detection)\n");
    println!("| algo | regret_lin | effort | detection | regret_over | effort_up |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for r in &raws {
        println!(
            "| {} | {:.4} | {:.4} | {:.0}% | {:.4} | {:.4} |",
            r.name, r.regret_lin, r.effort, r.detection * 100.0, r.regret_over, r.effort_up
        );
    }
    println!("\n## Axes vs ClassicComposed reference (0.5=ties classic, >0.5 beats it; Detection raw)\n");
    print!("| algo |");
    for a in AXES {
        print!(" {} |", a);
    }
    println!();
    print!("| --- |");
    for _ in AXES {
        print!(" --- |");
    }
    println!();
    for (name, ax) in &profiles {
        print!("| {} |", name);
        for v in ax {
            print!(" {:.2} |", v);
        }
        println!();
    }

    // Familiar-metrics table (raw units) — the intuition anchor.
    println!("\n## Familiar metrics (raw units, for cross-reference)\n");
    println!("| algo | convergence p50 | reaction p50 (−50%) | settled acc p50 | jitter (fires/min) |");
    println!("| --- | --- | --- | --- | --- |");
    for f in &fams {
        println!(
            "| {} | {:.1} min | {:.1} min | {:.1}% | {:.3} |",
            f.name,
            f.convergence_min,
            f.reaction_min,
            f.settled_accuracy * 100.0,
            f.jitter_per_min
        );
    }

    // Familiar metrics → radar profiles, normalized vs the reference
    // (raws[0]/fams[0] = ClassicComposed). These keep "0.5 = ties classic,
    // outward = better", but use a PER-AXIS LOG-RATIO score:
    //   score = 0.5 + k_axis · log2(ref/val),  clamped to [0,1].
    //
    // Rationale — on these raw axes ClassicComposed is a degenerate outlier
    // in its own favor (it barely fires, so jitter ≈ 0.018 and settled
    // accuracy ≈ 1%, ~7× better than any contender; meanwhile it is ~2×
    // SLOWER on convergence/reaction). The left panel's ref/(ref+val) would
    // crush every contender into a tiny band on the stability axes —
    // truthful but useless for reading contender-vs-contender separation —
    // and a single global log `k` can't serve axes whose spreads range from
    // ~1.3× (speed) to ~8× (accuracy): pick k for accuracy and the speed
    // axes flatten; pick k for speed and accuracy clips.
    //
    // So each axis gets its OWN `k_axis`, set from the data: the largest
    // |log2(ref/val)| on that axis maps to `EDGE` from the mid-ring. Every
    // axis then uses its full dynamic range (max separation, no clipping),
    // and ClassicComposed still sits at exactly 0.5 everywhere (ref/ref=1).
    // This panel is the "raw intuition" cross-reference, so a per-axis scale
    // is appropriate; the left panel remains the single-scale quantitative
    // one.
    let fref = &fams[0];
    const EDGE: f64 = 0.46; // most-extreme algo lands near rim/center, unclipped
    // axis i → extractor on a `Familiar`
    let axis_val: [fn(&Familiar) -> f64; 4] = [
        |f| f.convergence_min,
        |f| f.reaction_min,
        |f| f.settled_accuracy,
        |f| f.jitter_per_min,
    ];
    let axis_ref = [
        fref.convergence_min,
        fref.reaction_min,
        fref.settled_accuracy,
        fref.jitter_per_min,
    ];
    // Per-axis k from the max |log2(ref/val)| across all algos.
    let mut k_axis = [0.0f64; 4];
    for a in 0..4 {
        let max_dev = fams
            .iter()
            .map(|f| {
                let (r, v) = (axis_ref[a], axis_val[a](f));
                if r > 0.0 && v > 0.0 {
                    (r / v).log2().abs()
                } else {
                    0.0
                }
            })
            .fold(0.0f64, f64::max);
        k_axis[a] = if max_dev > 1e-9 { EDGE / max_dev } else { 0.0 };
    }
    let fam_profiles: Vec<(String, [f64; 4])> = fams
        .iter()
        .map(|f| {
            let mut ax = [0.5f64; 4];
            for a in 0..4 {
                let (r, v) = (axis_ref[a], axis_val[a](f));
                if r > 0.0 && v > 0.0 {
                    ax[a] = (0.5 + k_axis[a] * (r / v).log2()).clamp(0.0, 1.0);
                }
            }
            (f.name.clone(), ax)
        })
        .collect();

    let svg = render(&profiles, &fam_profiles);
    fs::write(&out_path, svg)?;
    eprintln!("\nWrote {}", out_path);
    Ok(())
}

fn render(profiles: &[(String, [f64; 5])], fam_profiles: &[(String, [f64; 4])]) -> String {
    let width = 1240;
    let height = 720 + 22 * profiles.len();
    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="320" y="30" text-anchor="middle" font-size="15" font-weight="bold">Regret/effort (derived)</text>
<text x="910" y="30" text-anchor="middle" font-size="15" font-weight="bold">Familiar metrics (raw)</text>
<text x="620" y="50" text-anchor="middle" font-size="12" fill="#555">Both: mid-ring = ties ClassicComposed, outward = better. Axes ↑ = better. (Left: ref/(ref+cost); Right: per-axis log-ratio.)</text>
"##
    ));

    // Left radar: the derived regret/effort axes.
    let left: Vec<(&str, &[f64])> = profiles.iter().map(|(n, a)| (n.as_str(), &a[..])).collect();
    draw_radar(&mut s, 320.0, 360.0, 215.0, AXES, &left);
    // Right radar: the familiar measurements, same normalization.
    let right: Vec<(&str, &[f64])> =
        fam_profiles.iter().map(|(n, a)| (n.as_str(), &a[..])).collect();
    draw_radar(&mut s, 910.0, 360.0, 215.0, FAM_AXES, &right);

    // Shared legend across the bottom.
    let ly0 = 660.0;
    for (idx, (name, _)) in profiles.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        let y = ly0 + idx as f64 * 22.0;
        s.push_str(&format!(
            r##"<rect x="40" y="{:.0}" width="14" height="14" fill="{color}" rx="2"/>
<text x="62" y="{:.0}" font-size="13">{}</text>
"##,
            y - 11.0, y, name
        ));
    }
    s.push_str("</svg>\n");
    s
}

/// Draws one radar centered at `(cx, cy)`: 5 rings (the 3rd bold = the
/// ClassicComposed reference), one labeled axis per entry in
/// `axes`, and one polygon per `(name, values)` profile in `COLORS`
/// order. Values are expected in [0,1] (mid-ring = 0.5 = ties classic).
fn draw_radar(s: &mut String, cx: f64, cy: f64, radius: f64, axes: &[&str], profiles: &[(&str, &[f64])]) {
    let n = axes.len();
    let angle = |i: usize| -PI / 2.0 + 2.0 * PI * i as f64 / n as f64;
    // rings
    for ring in 1..=5 {
        let r = radius * ring as f64 / 5.0;
        let mut pts = String::new();
        for i in 0..n {
            let a = angle(i);
            pts.push_str(&format!("{:.1},{:.1} ", cx + r * a.cos(), cy + r * a.sin()));
        }
        let (stroke, op, w) = if ring == 3 {
            ("#333", "0.55", "1.5")
        } else {
            ("#666", "0.15", "0.5")
        };
        s.push_str(&format!(
            r##"<polygon points="{}" fill="none" stroke="{stroke}" stroke-opacity="{op}" stroke-width="{w}"/>
"##,
            pts.trim()
        ));
    }
    // axes + labels
    for (i, axis) in axes.iter().enumerate() {
        let a = angle(i);
        let (xe, ye) = (cx + radius * a.cos(), cy + radius * a.sin());
        s.push_str(&format!(
            r##"<line x1="{cx}" y1="{cy}" x2="{xe:.1}" y2="{ye:.1}" stroke="#aaa" stroke-width="0.5"/>
"##
        ));
        let (lx, ly) = (cx + (radius + 22.0) * a.cos(), cy + (radius + 22.0) * a.sin());
        let anchor = if a.cos().abs() < 0.1 { "middle" } else if a.cos() > 0.0 { "start" } else { "end" };
        s.push_str(&format!(
            r##"<text x="{lx:.1}" y="{:.1}" text-anchor="{anchor}" font-size="12" font-weight="500">{} ↑</text>
"##,
            ly + 4.0, axis
        ));
    }
    // polygons
    for (idx, (_name, ax)) in profiles.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        let mut pts = String::new();
        for (i, &val) in ax.iter().enumerate() {
            let a = angle(i);
            let r = radius * val.clamp(0.0, 1.0);
            pts.push_str(&format!("{:.1},{:.1} ", cx + r * a.cos(), cy + r * a.sin()));
        }
        s.push_str(&format!(
            r##"<polygon points="{}" fill="{color}" fill-opacity="0.08" stroke="{color}" stroke-width="2" stroke-opacity="0.85"/>
"##,
            pts.trim()
        ));
    }
}
