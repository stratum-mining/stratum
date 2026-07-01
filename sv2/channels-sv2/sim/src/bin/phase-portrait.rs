//! Phase-portrait DEPTH-AXIS run (see docs/records/PHASE_PORTRAIT_SPEC.md §9).
//!
//! The first cut (now superseded below) fixed depth at the scenario's 50% and
//! swept rate only — so it could NOT see the structural-deadband's defining
//! claim, failure-as-a-HALF-PLANE-IN-DEPTH (rate drops out). This run makes
//! DEPTH a real axis: a `(depth d, rate ṙ)` grid, depth = fraction of hashrate
//! lost, swept independently of rate.
//!
//! Two regions on one plane, each measured against its OWN threshold (NOT one
//! derived from the other):
//!   - OVER-DIFFICULTY (interior) — settled `e` after the recovery window vs the
//!     5% gate (`DECLINE_SAFETY_GATE_PCT`), the exact quantity §9.2's gate bounds.
//!   - DISCONNECT (boundary) — `min_t(r_obs/r*) = min_t(e^{−e})` over the decline
//!     vs a liveness floor θ (the essays' 0.36).
//! These are two level sets of the same underlying error surface (info-floor
//! §9.2a); recording both directly lets the figure show the modal cloud sitting
//! between the two heights without deriving one region from the other.
//!
//! Archetypes:
//!   - sleepy-symmetric (champion Ewma360) → failure a CORNER (deep AND fast).
//!   - structural-deadband (pow2 PID, Kp=0.01) → failure a HALF-PLANE in depth
//!     alone (the trough/settled-e crossing is rate-independent).
//!
//! Usage: cargo run --release --bin phase-portrait
//! Env: VARDIFF_PP_TRIALS (default 60), VARDIFF_PP_THREADS, VARDIFF_SWEEP_SEED,
//!      VARDIFF_PP_THETA (liveness floor, default 0.36),
//!      VARDIFF_PP_SPM (shares/min the plane is sliced at, default 6 — the
//!      spm_threshold; the plane is window/rate-of-evidence-specific, §5 guard 2).

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bitcoin::Target;
use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::error::VardiffError;
use channels_sv2::vardiff::{Clock, MockClock, Vardiff};
use vardiff_sim::baseline::{Phase, Scenario, TRUE_HASHRATE};
use vardiff_sim::decline_safety::{
    DECLINE_MATURE_MIN, DECLINE_MAX_MIN, DECLINE_OBSERVE_MIN, DECLINE_SAFETY_GATE_PCT,
    DECLINE_TICK_SECS as TICK,
};
use vardiff_sim::grid::{AlgorithmSpec, AsObservable, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

/// Faithful port of the NOMP `node-stratum-pool` `lib/varDiff.js` (read verbatim
/// 2026-06-30; see docs/records/VARDIFF_SURVEY.md). The point of the port is the
/// DEFINING family property: retarget logic lives ONLY in the share-submit path,
/// so there is **no idle ease** — a tick with zero shares does nothing, which is
/// what reproduces "a starved miner is dropped before it can ease."
///
/// Faithful elements (NOMP varDiff.js):
///   - boxcar `RingBuffer` of inter-share gaps `sinceLast`, `avg()` over it;
///   - clock-gated `retargetTime` (no retarget until that much wall-time passed);
///   - symmetric ±`variancePercent` band on share-TIME (tMin/tMax);
///   - ease iff `avg > tMax`, `ddiff = targetTime / avg` (full retarget, η=1).
///
/// Harness adaptation (honest): the sim bulk-adds `n` shares per `dt`-second tick
/// rather than delivering individual submits. We reconstruct NOMP's per-submit
/// buffer by appending the inter-share gap `dt/n` exactly `n` times (capped to
/// buffer size) — the same per-share reconstruction the ckpool port used
/// (CKPOOL_INVESTIGATION §"per-share decay_time simulation"). CRUCIALLY: when
/// `n == 0`, nothing is appended and the submit branch never runs → no retarget.
/// That zero-share branch IS the family's defining "no idle path."
#[derive(Debug)]
struct NompVarDiff {
    target_time: f64,   // seconds per share (= 60 / spm_target)
    variance_pct: f64,  // ±band on share-time (NOMP default 30)
    retarget_time: f64, // seconds between retargets (NOMP default ~90)
    buffer: Vec<f64>,   // boxcar of inter-share gaps
    buf_cap: usize,
    last_rtc: Option<u64>, // last retarget wall-time
    shares_since: u32,
    last_update_ts: u64,
    current_hashrate: f32,
    spm_target: f32,
    min_hashrate: f32,
    clock: Arc<dyn Clock>,
}

impl NompVarDiff {
    fn new(spm_target: f32, initial_hashrate: f32, clock: Arc<dyn Clock>) -> Self {
        let target_time = 60.0 / spm_target as f64;
        let retarget_time = 90.0; // NOMP default
        // NOMP: bufferSize = retargetTime / targetTime * 4
        let buf_cap = ((retarget_time / target_time) * 4.0).round().max(1.0) as usize;
        Self {
            target_time,
            variance_pct: 30.0,
            retarget_time,
            buffer: Vec::with_capacity(buf_cap),
            buf_cap,
            last_rtc: None,
            shares_since: 0,
            last_update_ts: clock.now_secs(),
            current_hashrate: initial_hashrate,
            spm_target,
            min_hashrate: 1.0,
            clock,
        }
    }

    fn append_gap(&mut self, gap: f64) {
        if self.buffer.len() == self.buf_cap {
            self.buffer.remove(0);
        }
        self.buffer.push(gap);
    }

    fn avg(&self) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }
        self.buffer.iter().sum::<f64>() / self.buffer.len() as f64
    }
}

impl Vardiff for NompVarDiff {
    fn last_update_timestamp(&self) -> u64 {
        self.last_update_ts
    }
    fn shares_since_last_update(&self) -> u32 {
        self.shares_since
    }
    fn set_timestamp_of_last_update(&mut self, ts: u64) {
        self.last_update_ts = ts;
    }
    fn increment_shares_since_last_update(&mut self) {
        self.shares_since = self.shares_since.saturating_add(1);
    }
    fn add_shares(&mut self, n: u32) {
        self.shares_since = self.shares_since.saturating_add(n);
    }
    fn min_allowed_hashrate(&self) -> f32 {
        self.min_hashrate
    }
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.shares_since = 0;
        self.last_update_ts = self.clock.now_secs();
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        _hashrate: f32,
        _target: &Target,
        _spm: f32,
    ) -> Result<Option<f32>, VardiffError> {
        let now = self.clock.now_secs();
        let dt = now.saturating_sub(self.last_update_ts);
        let n = self.shares_since;

        // THE FAMILY PROPERTY: no shares this tick → submit handler never runs →
        // no retarget, no ease. This is the "no idle path" that drops a starved
        // miner before it can ease. Do NOT add an else-branch here.
        if n == 0 {
            return Ok(None);
        }

        // Reconstruct NOMP's per-submit buffer appends: n gaps of dt/n each.
        let gap = if n > 0 { dt as f64 / n as f64 } else { dt as f64 };
        for _ in 0..n {
            self.append_gap(gap);
        }
        self.last_update_ts = now;
        self.shares_since = 0;

        if self.last_rtc.is_none() {
            // NOMP inits lastRtc on first submit and returns (no retarget yet).
            self.last_rtc = Some(now.saturating_sub((self.retarget_time / 2.0) as u64));
            return Ok(None);
        }
        // clock-gated retargetTime (NOMP: if (ts - lastRtc) < retargetTime return)
        if (now - self.last_rtc.unwrap()) < self.retarget_time as u64 && !self.buffer.is_empty() {
            return Ok(None);
        }

        self.last_rtc = Some(now);
        let avg = self.avg();
        let variance = self.target_time * (self.variance_pct / 100.0);
        let t_min = self.target_time - variance;
        let t_max = self.target_time + variance;

        // NOMP: ease iff avg > tMax (shares slower than target → lower difficulty);
        // tighten iff avg < tMin; else no change.
        let ddiff = if avg > t_max {
            self.target_time / avg // < 1 → ease (hashrate estimate down)
        } else if avg < t_min {
            self.target_time / avg // > 1 → tighten
        } else {
            return Ok(None);
        };

        let new_hashrate = (self.current_hashrate as f64 * ddiff).max(self.min_hashrate as f64) as f32;
        self.current_hashrate = new_hashrate;
        self.buffer.clear(); // NOMP: timeBuffer.clear() after retarget
        Ok(Some(new_hashrate))
    }
}

/// Share-driven family archetype: faithful NOMP (no idle path).
fn nomp() -> AlgorithmSpec {
    AlgorithmSpec::new("nomp(share-gated)", |clock: Arc<dyn Clock>| {
        VardiffBox(Box::new(AsObservable(NompVarDiff::new(6.0, TRUE_HASHRATE, clock))))
    })
}

/// Sleepy-symmetric archetype: the shipped champion (Ewma360/s1.5).
fn sleepy() -> AlgorithmSpec {
    AlgorithmSpec::new("sleepy(Ewma360)", |clock: Arc<dyn Clock>| {
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

/// Structural-deadband archetype: the pow2-in-loop PID at the production gain
/// (Kp=0.01) — the DMND construction. Reuses the grid's existing spec.
fn deadband() -> AlgorithmSpec {
    AlgorithmSpec::pow2_pid_default()
}

/// A sustained-decline scenario parameterized by DEPTH as well as rate — the
/// depth-axis generalization of `decline_safety::decline_scenario`, kept local
/// to the bin so the gate's single-source-of-truth module is not perturbed.
///
/// Mature on-target, drop at `rate_pct_per_hr` in 1-min steps until `depth`
/// fraction is lost (capped at `DECLINE_MAX_MIN`), then hold at the floor for the
/// observe window. Returns the scenario and `(decline_start, decline_end,
/// trial_end)`. Mirrors the source scenario exactly except `depth` replaces the
/// fixed `DECLINE_TARGET_DROP`.
fn decline_scenario_depth(rate_pct_per_hr: f32, depth: f32) -> (Scenario, u64, u64, u64) {
    let rate = rate_pct_per_hr / 100.0; // fraction/hr
    let decline_min = ((depth / rate) * 60.0).min(DECLINE_MAX_MIN as f32) as u64;
    let mut phases = vec![Phase::Hold {
        secs: DECLINE_MATURE_MIN * 60,
        h: TRUE_HASHRATE,
    }];
    for m in 0..decline_min {
        let frac = (rate / 60.0) * (m as f32 + 1.0);
        let h = TRUE_HASHRATE * (1.0 - frac).max(1.0 - depth);
        phases.push(Phase::Hold { secs: 60, h });
    }
    let floor = TRUE_HASHRATE * (1.0 - (rate / 60.0 * decline_min as f32).min(depth));
    phases.push(Phase::Hold {
        secs: DECLINE_OBSERVE_MIN * 60,
        h: floor,
    });
    let start = DECLINE_MATURE_MIN * 60;
    let end = start + decline_min * 60;
    let trial_end = end + DECLINE_OBSERVE_MIN * 60;
    (
        Scenario::Custom {
            name: format!("decline_{}pph_d{}", rate_pct_per_hr as u32, (depth * 100.0) as u32),
            phases,
            initial_estimate: None,
        },
        start,
        end,
        trial_end,
    )
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Both region quantities for one `(depth, rate)` cell, each median-over-trials:
///   `.0` settled over-difficulty `e` (percent) — the OVER-DIFFICULTY region (vs
///        the 5% gate), sampled in the post-decline observe window (steady state).
///   `.1` disconnect trough `min_t(r_obs/r*)` over the decline — the DISCONNECT
///        region (vs θ).
fn cell(
    make: &dyn Fn() -> AlgorithmSpec,
    rate: f32,
    depth: f32,
    spm: f32,
    trials: usize,
    seed: u64,
) -> (f64, f64) {
    let (scen, d_start, d_end, trial_end) = decline_scenario_depth(rate, depth);
    let (cfg_proto, schedule) = scen.build(spm);
    let cfg = TrialConfig { tick_interval_secs: TICK, ..cfg_proto };
    let mut settled = Vec::with_capacity(trials);
    let mut troughs = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (make().factory)(clock.clone());
        let t = run_trial_observed(v, clock, cfg.clone(), &schedule, seed.wrapping_add(i as u64));
        let mut trough = f64::INFINITY;
        let mut settled_e = 0.0f64;
        for tk in &t.ticks {
            let h_true = schedule.at(tk.t_secs.saturating_sub(TICK / 2)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln();
            // disconnect trough: over the decline window only.
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let r_obs_ratio = (-e).exp();
                if r_obs_ratio < trough {
                    trough = r_obs_ratio;
                }
            }
            // settled over-difficulty: last sample in the observe window.
            if tk.t_secs > d_end && tk.t_secs <= trial_end {
                settled_e = e;
            }
        }
        settled.push(settled_e * 100.0);
        if trough.is_finite() {
            troughs.push(trough);
        }
    }
    (median(settled), median(troughs))
}

fn main() {
    let trials: usize = env::var("VARDIFF_PP_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    let theta: f64 = env::var("VARDIFF_PP_THETA").ok().and_then(|s| s.parse().ok()).unwrap_or(0.36);
    let spm: f32 = env::var("VARDIFF_PP_SPM").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let seed: u64 = env::var("VARDIFF_SWEEP_SEED").ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(vardiff_sim::baseline::DEFAULT_BASELINE_SEED);
    let n_threads: usize = env::var("VARDIFF_PP_THREADS").ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // The (rate × depth) plane. Depth is now a real axis: fraction of hashrate
    // lost, swept independently of rate. Rates span gentle→cliff; depths span
    // shallow→deep. Depth capped where rate×DECLINE_MAX_MIN can't reach it (the
    // slow/deep corner is rate-limited by the 4h cap — flagged in the read).
    let rates = [1.0f32, 2.0, 5.0, 10.0, 20.0, 40.0];
    let depths = [0.10f32, 0.20, 0.30, 0.40, 0.50, 0.65, 0.80];
    let arms: [(&str, fn() -> AlgorithmSpec); 3] =
        [("sleepy", sleepy), ("deadband", deadband), ("nomp", nomp)];

    let mut jobs: Vec<(usize, f32, f32)> = Vec::new();
    for a in 0..arms.len() {
        for &r in &rates {
            for &d in &depths {
                jobs.push((a, r, d));
            }
        }
    }
    eprintln!(
        "phase-portrait DEPTH-AXIS: θ={theta}, spm={spm}, gate={DECLINE_SAFETY_GATE_PCT}%, \
         {} cells × {} trials, {} threads",
        jobs.len(), trials, n_threads
    );

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, f32, f32, f64, f64)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() {
                    break;
                }
                let (ai, rate, depth) = jobs[j];
                let (settled, trough) = cell(&arms[ai].1, rate, depth, spm, trials, seed);
                out.lock().unwrap().push((ai, rate, depth, settled, trough));
            });
        }
    });

    let rows = out.into_inner().unwrap();
    let get = |ai: usize, rate: f32, depth: f32| -> (f64, f64) {
        rows.iter()
            .find(|(a, r, d, _, _)| *a == ai && *r == rate && *d == depth)
            .map(|t| (t.3, t.4))
            .unwrap_or((f64::NAN, f64::NAN))
    };

    for (ai, name) in arms.iter().enumerate() {
        // OVER-DIFFICULTY region: settled-e surface, F = fail (settled > gate).
        println!(
            "\n## {} — OVER-DIFFICULTY (interior): settled-e %% over (depth × rate); F = > {}%% gate",
            name.0, DECLINE_SAFETY_GATE_PCT
        );
        print!("  depth\\rate |");
        for r in &rates { print!("   {:>4}", *r as u32); }
        println!("   %/hr");
        for &depth in &depths {
            print!("  {:>8.2}  |", depth);
            for &rate in &rates {
                let (settled, _) = get(ai, rate, depth);
                if settled > DECLINE_SAFETY_GATE_PCT { print!("   F  "); }
                else { print!(" {:>5.1}", settled); }
            }
            println!();
        }
        // DISCONNECT region: trough surface, X = drop (trough < θ).
        println!(
            "\n## {} — DISCONNECT (boundary): min_t(r_obs/r*) over (depth × rate); X = < θ={}",
            name.0, theta
        );
        print!("  depth\\rate |");
        for r in &rates { print!("   {:>4}", *r as u32); }
        println!("   %/hr");
        for &depth in &depths {
            print!("  {:>8.2}  |", depth);
            for &rate in &rates {
                let (_, trough) = get(ai, rate, depth);
                if trough < theta { print!("   X  "); }
                else { print!("  {:.2}", trough); }
            }
            println!();
        }
    }

    println!("\nREAD:");
    println!("  sleepy   → over-difficulty F's in the deep+fast CORNER (both axes needed to fail)?");
    println!("  deadband → over-difficulty F's a HALF-PLANE in DEPTH (F across all rates past some d*)?");
    println!("  disconnect (both) → expect EMPTY (no X) at θ={theta}: the cost is interior, not boundary.");
    println!("(Plane sliced at spm={spm}; window/evidence-rate-specific — §5 guard 2. Slow+deep corner");
    println!(" is rate-limited by the {DECLINE_MAX_MIN}-min decline cap, not a controller property.)");

    // --- Render the report figure (illustrates the derived fact; see spec §10). ---
    let out_path = env::var("VARDIFF_PP_OUT")
        .unwrap_or_else(|_| "docs/figures/phase_portrait.svg".to_string());
    let svg = render_svg(&rates, &depths, &get, theta, spm);
    match std::fs::write(&out_path, svg) {
        Ok(()) => eprintln!("wrote figure: {out_path}"),
        Err(e) => eprintln!("WARN: could not write {out_path}: {e}"),
    }

    // --- The "how bad" twin: payout-variance inflation vs depth (see spec §12). ---
    let var_path = env::var("VARDIFF_PP_VAR_OUT")
        .unwrap_or_else(|_| "docs/figures/variance_inflation.svg".to_string());
    let var_svg = render_variance_svg(&depths, &get, spm);
    match std::fs::write(&var_path, var_svg) {
        Ok(()) => eprintln!("wrote figure: {var_path}"),
        Err(e) => eprintln!("WARN: could not write {var_path}: {e}"),
    }

    // --- The lay-twin (essays register; see spec §14). Built from the SAME `get`
    // source as the report variance figure and the portrait champion panel, so
    // the cross-derivation tie is STRUCTURAL — a third view of the same 1/trough.
    let lay_path = env::var("VARDIFF_PP_LAY_OUT")
        .unwrap_or_else(|_| "docs/figures/variance_inflation_lay.svg".to_string());
    // The lay function RETURNS the champion series it actually plotted (depth,
    // inflation), so the cross-check can compare what-was-drawn against an
    // independent read — not a tautology.
    let (lay_svg, lay_champion) = render_variance_lay_svg(&depths, &get);
    match std::fs::write(&lay_path, lay_svg) {
        Ok(()) => eprintln!("wrote figure: {lay_path}"),
        Err(e) => eprintln!("WARN: could not write {lay_path}: {e}"),
    }

    // CROSS-CHECK WITH TEETH: recompute the champion series INDEPENDENTLY here
    // from `get(0,…)` (arm 0 = champion) and compare against what the lay function
    // RETURNED it had plotted. If the lay render used the wrong arm (the §12 bug),
    // its returned series is `1/get(1,…)` = deadband and this fires. A tautology
    // (1/trough == 1/trough) would not — the teeth come from comparing the drawn
    // series to an independent read. Third view of the same number, free.
    let independent: Vec<f64> = depths
        .iter()
        .map(|&depth| {
            let (_, trough) = get(0, 20.0, depth); // arm 0 = champion, cap-free rate 20%/hr
            if trough.is_finite() { 1.0 / trough } else { f64::NAN }
        })
        .collect();
    let mismatch = lay_champion
        .iter()
        .zip(independent.iter())
        .filter(|(a, b)| a.is_finite() && b.is_finite() && (**a - **b).abs() > 1e-9)
        .count();
    if mismatch == 0 {
        eprintln!(
            "cross-check OK: lay-twin champion (drawn) == 1/get(0,·) (independent) — \
             same source as report fig + portrait panel; all three views agree (§12 fix holds)"
        );
    } else {
        eprintln!(
            "CROSS-CHECK FAILED: {mismatch} champion points disagree with an independent \
             1/get(0,·) read — WRONG ARM in the lay render (the §12 bug) or a regression"
        );
    }
}

/// The variance-inflation curve — the "how bad" twin of the phase portrait's
/// "where" (spec §12). Relative payout variance over a window is `∝ 1/(r_obs·τ)`
/// (Poisson share count, §1/§3); on-target that is the floor `1/(r*·τ)`, and
/// over-difficulty pushes `r_obs → r*·trough`, inflating it by exactly
/// `1/trough`. So the inflation factor at a decline's worst point is `1/trough`:
///   - DEADBAND: closed form `1/(1−d)` — the no-op identity again (it does nothing,
///     so observed rate is the raw loss; nothing measured, the curve is derived).
///   - CHAMPION: measured `1/trough` (it reacts, so it must be traced), shown at
///     the cap-free rates where depth is genuinely swept (20, 40 %/hr).
/// Same identity as the figure; this one reads it on the y-axis as a cost.
fn render_variance_svg(
    depths: &[f32],
    get: &dyn Fn(usize, f32, f32) -> (f64, f64),
    spm: f32,
) -> String {
    let (w, h) = (760.0f64, 540.0f64);
    let (ml, mt) = (78.0f64, 70.0f64);
    let (pw, ph) = (620.0f64, 392.0f64);
    let dmax = 0.85f64;
    let ymax = 5.5f64; // inflation factor axis top (deadband hits 5× at d=0.8)

    let gate_depth = 1.0 - (-DECLINE_SAFETY_GATE_PCT / 100.0).exp(); // ≈0.0488
    let xd = |d: f64| ml + pw * (d / dmax).min(1.0);
    let yv = |v: f64| mt + ph * (1.0 - ((v - 1.0) / (ymax - 1.0)).clamp(0.0, 1.0));

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         font-family=\"system-ui, sans-serif\" font-size=\"13\">\n"
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"24\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" \
         fill=\"#222\">How bad it is there: payout-variance inflation through a decline</text>\n",
        w / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"43\" text-anchor=\"middle\" font-size=\"11\" fill=\"#888\" \
         font-style=\"italic\">phase-portrait.rs (generated). Relative variance ∝ 1/(r_obs·τ); \
         inflation over the on-target floor = 1/trough. Slice at spm={spm}.</text>\n",
        w / 2.0
    ));

    // gridlines + y ticks (inflation factor).
    for v in [1.0, 1.5, 2.0, 3.0, 4.0, 5.0] {
        let y = yv(v);
        s.push_str(&format!(
            "<line x1=\"{ml:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#eee\"/>\n",
            ml + pw
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"9\" fill=\"#666\">{:.1}×</text>\n",
            ml - 6.0, y + 3.0, v
        ));
    }
    // on-target reference (1× = the floor itself, no inflation).
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"9.5\" fill=\"#1e8449\">\
         1× = on-target (at the floor, no inflation)</text>\n",
        ml + pw - 6.0, yv(1.0) - 5.0
    ));

    // modal-decline band (8–20% loss) — shaded vertical.
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{mt:.1}\" width=\"{:.1}\" height=\"{ph:.1}\" fill=\"#2c3e50\" \
         fill-opacity=\"0.06\"/>\n",
        xd(0.08), xd(0.20) - xd(0.08)
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#2c3e50\" \
         font-weight=\"bold\">modal declines</text>\n",
        (xd(0.08) + xd(0.20)) / 2.0, mt + 14.0
    ));
    // gate-depth marker.
    s.push_str(&format!(
        "<line x1=\"{0:.1}\" y1=\"{mt:.1}\" x2=\"{0:.1}\" y2=\"{1:.1}\" stroke=\"#e67e22\" \
         stroke-width=\"1.2\" stroke-dasharray=\"3 3\"/>\n",
        xd(gate_depth), mt + ph
    ));

    // DEADBAND closed-form curve: 1/(1−d).
    let mut db = String::new();
    let steps = 80;
    for k in 0..=steps {
        let d = dmax * k as f64 / steps as f64;
        db.push_str(&format!("{:.1},{:.1} ", xd(d), yv(1.0 / (1.0 - d))));
    }
    s.push_str(&format!(
        "<polyline points=\"{db}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"2.6\"/>\n"
    ));

    // CHAMPION measured: 1/trough at cap-free rates 20 and 40 %/hr.
    // arm 0 = sleepy/champion (arm 1 is the deadband) — see `arms` in main.
    for (rate, dash, label_y) in [(20.0f32, "", 0.0), (40.0f32, "5 3", 0.0)] {
        let _ = label_y;
        let mut pts = String::new();
        for &depth in depths {
            let (_, trough) = get(0, rate, depth);
            if trough.is_finite() {
                pts.push_str(&format!("{:.1},{:.1} ", xd(depth as f64), yv(1.0 / trough)));
            }
        }
        s.push_str(&format!(
            "<polyline points=\"{pts}\" fill=\"none\" stroke=\"#2980b9\" stroke-width=\"2.2\" \
             stroke-dasharray=\"{dash}\"/>\n"
        ));
    }

    // curve labels.
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11.5\" font-weight=\"bold\" \
         fill=\"#c0392b\">deadband 1/(1−d) — steep (it does nothing)</text>\n",
        xd(0.80) - 4.0, yv(1.0 / (1.0 - 0.80)) + 4.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11.5\" font-weight=\"bold\" \
         fill=\"#1f618d\">champion — flat (it reacts; ≤~1.4× even at deep/fast)</text>\n",
        xd(0.32), yv(1.85)
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#5dade2\">\
         (solid 20%/hr, dashed 40%/hr — the cap-free columns)</text>\n",
        xd(0.32), yv(1.85) + 15.0
    ));

    // frame + axes.
    s.push_str(&format!(
        "<rect x=\"{ml:.1}\" y=\"{mt:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\" fill=\"none\" \
         stroke=\"#999\"/>\n"
    ));
    for d in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.65, 0.8] {
        let x = xd(d);
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\">{:.0}%</text>\n",
            mt + ph + 14.0, d * 100.0
        ));
    }
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#444\">\
         decline depth (fraction of hashrate lost)</text>\n",
        ml + pw / 2.0, mt + ph + 32.0
    ));
    s.push_str(&format!(
        "<text x=\"22\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#444\" \
         transform=\"rotate(-90 22 {:.1})\">payout-variance inflation × (over the on-target floor)</text>\n",
        mt + ph / 2.0, mt + ph / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" fill=\"#555\">\
         Same identity as the phase portrait, read as a cost: the figure shows WHERE the failure \
         is, this shows HOW BAD it is there. Deadband variance blows up as 1/(1−d) because it never \
         retargets; the champion holds near 1× because it does.</text>\n",
        w / 2.0, h - 16.0
    ));

    s.push_str("</svg>\n");
    s
}

/// The LAY-TWIN of the variance figure (essays register; spec §14). Same
/// `1/trough` quantity as `render_variance_svg` and the portrait champion panel —
/// built from the SAME `get` source so the cross-derivation tie is structural —
/// but stripped to plain English: depth on x, two curves, shaded modal band, a
/// caption a non-technical reader can carry. NO `1/(1−d)`, NO τ, NO cap language.
/// Returns `(svg, champion_series)` where champion_series is the inflation values
/// it actually plotted (rate 20, arm 0), so main() can cross-check the drawn
/// series against an independent read — the check that has teeth (§14).
fn render_variance_lay_svg(
    depths: &[f32],
    get: &dyn Fn(usize, f32, f32) -> (f64, f64),
) -> (String, Vec<f64>) {
    let (w, h) = (720.0f64, 460.0f64);
    let (ml, mt) = (66.0f64, 64.0f64);
    let (pw, ph) = (588.0f64, 320.0f64);
    let dmax = 0.85f64;
    let ymax = 5.5f64;

    let xd = |d: f64| ml + pw * (d / dmax).min(1.0);
    let yv = |v: f64| mt + ph * (1.0 - ((v - 1.0) / (ymax - 1.0)).clamp(0.0, 1.0));

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         font-family=\"system-ui, sans-serif\" font-size=\"13\">\n"
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"26\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" \
         fill=\"#222\">How noisy your payouts get as your miner declines</text>\n",
        w / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"44\" text-anchor=\"middle\" font-size=\"11.5\" fill=\"#777\">\
         The bad controller makes payouts ~5× noisier in a deep decline; the good one stays near normal.</text>\n",
        w / 2.0
    ));

    // y gridlines.
    for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
        let y = yv(v);
        s.push_str(&format!(
            "<line x1=\"{ml:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#eee\"/>\n",
            ml + pw
        ));
        let lbl = if v == 1.0 { "normal".to_string() } else { format!("{:.0}× noisier", v) };
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"9.5\" fill=\"#888\">{lbl}</text>\n",
            ml - 6.0, y + 3.0
        ));
    }

    // shaded modal band (8–20% loss) — "what miners usually see".
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{mt:.1}\" width=\"{:.1}\" height=\"{ph:.1}\" fill=\"#2c3e50\" \
         fill-opacity=\"0.06\"/>\n",
        xd(0.08), xd(0.20) - xd(0.08)
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#2c3e50\">\
         a typical decline</text>\n",
        (xd(0.08) + xd(0.20)) / 2.0, mt + ph + 34.0
    ));

    // DEADBAND (closed form 1/(1−d)) — plotted as a smooth curve, but NO formula shown.
    let mut db = String::new();
    let steps = 80;
    for k in 0..=steps {
        let d = dmax * k as f64 / steps as f64;
        db.push_str(&format!("{:.1},{:.1} ", xd(d), yv(1.0 / (1.0 - d))));
    }
    s.push_str(&format!(
        "<polyline points=\"{db}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"2.8\"/>\n"
    ));

    // CHAMPION measured (arm 0, rate 20 — the cap-free read, matching main()'s
    // independent cross-check). Capture the plotted series for the teeth-check.
    let mut champion_series: Vec<f64> = Vec::new();
    let mut champ = String::new();
    for &depth in depths {
        let (_, trough) = get(0, 20.0, depth);
        let infl = if trough.is_finite() { 1.0 / trough } else { f64::NAN };
        champion_series.push(infl);
        if infl.is_finite() {
            champ.push_str(&format!("{:.1},{:.1} ", xd(depth as f64), yv(infl)));
        }
    }
    s.push_str(&format!(
        "<polyline points=\"{champ}\" fill=\"none\" stroke=\"#2980b9\" stroke-width=\"2.8\"/>\n"
    ));

    // plain curve labels.
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"12\" font-weight=\"bold\" \
         fill=\"#c0392b\">a bad controller</text>\n",
        xd(0.72), yv(1.0 / (1.0 - 0.72)) - 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"12\" font-weight=\"bold\" \
         fill=\"#1f618d\">a good controller</text>\n",
        xd(0.42), yv(1.30) - 6.0
    ));

    // frame + x axis (plain percentages).
    s.push_str(&format!(
        "<rect x=\"{ml:.1}\" y=\"{mt:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\" fill=\"none\" \
         stroke=\"#bbb\"/>\n"
    ));
    for d in [0.0, 0.2, 0.4, 0.6, 0.8] {
        let x = xd(d);
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"#888\">{:.0}%</text>\n",
            mt + ph + 16.0, d * 100.0
        ));
    }
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11.5\" fill=\"#555\">\
         how much hashrate the miner has lost</text>\n",
        ml + pw / 2.0, mt + ph + 50.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" fill=\"#777\">\
         (noisier, not smaller — you still earn what you're owed, the payouts just swing more)</text>\n",
        w / 2.0, h - 14.0
    ));

    s.push_str("</svg>\n");
    (s, champion_series)
}

/// Render the (rate × depth) phase portrait: two panels sharing the depth axis,
/// the structural-deadband (left) and the champion (right). The deadband's
/// boundaries are drawn from the CLOSED FORM (`e=−ln(1−d)` → gate at d≈5%, θ at
/// d=1−θ) — the figure illustrates a derived fact, it is not the evidence for it
/// (spec §10). The champion panel is a measured trough heatmap (it reacts, so it
/// must be traced, not derived) and crosses neither threshold on this slice. The
/// cap-unreachable region (depth > 4%·rate) is hatched and labeled unreachable —
/// NOT empty-because-safe (the value-vs-domain distinction, §10 caveat).
fn render_svg(
    rates: &[f32],
    depths: &[f32],
    get: &dyn Fn(usize, f32, f32) -> (f64, f64),
    theta: f64,
    spm: f32,
) -> String {
    let (w, h) = (1180.0f64, 600.0f64);
    let (mt, ph) = (78.0f64, 392.0f64); // plot top, plot height
    let pw = 430.0f64; // panel width
    let lml = 70.0f64; // left panel left margin
    let rml = lml + pw + 92.0; // right panel left margin (gap for y-axis breath)
    let rmin = 1.0f64;
    let rmax = 40.0f64;
    let dmax_axis = 0.85f64; // depth axis top (fraction lost)
    let cap_hours = DECLINE_MAX_MIN as f64 / 60.0;

    // closed-form deadband thresholds.
    let gate_depth = 1.0 - (-DECLINE_SAFETY_GATE_PCT / 100.0).exp(); // ≈0.0488
    let disc_depth = 1.0 - theta; // ≈0.64

    // coordinate maps (rate: log-x; depth: linear-y, increasing DOWNWARD).
    let xr = |ml: f64, rate: f64| -> f64 {
        ml + pw * (rate.ln() - rmin.ln()) / (rmax.ln() - rmin.ln())
    };
    let yd = |depth: f64| -> f64 { mt + ph * (depth / dmax_axis).min(1.0) };

    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         font-family=\"system-ui, sans-serif\" font-size=\"13\">\n"
    ));
    // hatch pattern for the unreachable region.
    // transparent background so the colored zones below show through the hatch —
    // the unreachable region is an OVERLAY (you can't READ here), not an erasure.
    s.push_str(
        "<defs><pattern id=\"unreach\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\" \
         patternTransform=\"rotate(45)\">\
         <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"8\" stroke=\"#78909c\" stroke-width=\"1.1\" \
         stroke-opacity=\"0.55\"/></pattern></defs>\n",
    );
    // title + subtitle.
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"24\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" \
         fill=\"#222\">The cost is interior, not at the boundary: one error surface, two thresholds, \
         the modal declines in the gap</text>\n",
        w / 2.0
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"43\" text-anchor=\"middle\" font-size=\"11\" fill=\"#888\" \
         font-style=\"italic\">phase-portrait.rs (generated). (rate × depth) plane sliced at \
         spm={spm}. Deadband boundaries are the closed form e=−ln(1−d); champion trough is \
         measured. Disconnect uses θ={theta}.</text>\n",
        w / 2.0
    ));

    // ---- per-panel drawing ----
    for (pi, (ml, title, is_deadband)) in [
        (lml, "structural-deadband (no-op in decline)", true),
        (rml, "champion Ewma360 (reacts)", false),
    ]
    .iter()
    .enumerate()
    {
        let ml = *ml;
        // panel title.
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12.5\" \
             font-weight=\"bold\" fill=\"#333\">{}</text>\n",
            ml + pw / 2.0,
            mt - 14.0,
            title
        ));

        if *is_deadband {
            // ZONES (rate-independent → full-width horizontal bands = the
            // half-plane-in-depth, drawn straight because the controller is inert).
            // safe (d < gate): light green.
            s.push_str(&format!(
                "<rect x=\"{ml:.1}\" y=\"{:.1}\" width=\"{pw:.1}\" height=\"{:.1}\" \
                 fill=\"#eafaf1\"/>\n",
                yd(0.0),
                yd(gate_depth) - yd(0.0)
            ));
            // interior fails, boundary clear (gate < d < θ): amber — the result.
            s.push_str(&format!(
                "<rect x=\"{ml:.1}\" y=\"{:.1}\" width=\"{pw:.1}\" height=\"{:.1}\" \
                 fill=\"#fdebd0\"/>\n",
                yd(gate_depth),
                yd(disc_depth) - yd(gate_depth)
            ));
            // both fail (d > θ): red.
            s.push_str(&format!(
                "<rect x=\"{ml:.1}\" y=\"{:.1}\" width=\"{pw:.1}\" height=\"{:.1}\" \
                 fill=\"#f9d6d5\"/>\n",
                yd(disc_depth),
                yd(dmax_axis) - yd(disc_depth)
            ));
            // threshold lines + closed-form labels.
            s.push_str(&format!(
                "<line x1=\"{ml:.1}\" y1=\"{0:.1}\" x2=\"{1:.1}\" y2=\"{0:.1}\" \
                 stroke=\"#e67e22\" stroke-width=\"2\"/>\n",
                yd(gate_depth),
                ml + pw
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"10\" \
                 fill=\"#b9690e\" font-weight=\"bold\">over-difficulty gate · d≈5% (e=−ln(1−d)=5%)</text>\n",
                ml + pw - 4.0,
                yd(gate_depth) - 4.0
            ));
            s.push_str(&format!(
                "<line x1=\"{ml:.1}\" y1=\"{0:.1}\" x2=\"{1:.1}\" y2=\"{0:.1}\" \
                 stroke=\"#c0392b\" stroke-width=\"2\"/>\n",
                yd(disc_depth),
                ml + pw
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"10\" \
                 fill=\"#c0392b\" font-weight=\"bold\">disconnect floor · d=1−θ≈{:.0}%</text>\n",
                ml + pw - 4.0,
                yd(disc_depth) - 4.0,
                disc_depth * 100.0
            ));
            // zone labels.
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"11\" \
                 fill=\"#1e8449\">safe (under-difficulty)</text>\n",
                ml + 6.0,
                yd(gate_depth / 2.0) + 4.0
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"11\" \
                 font-weight=\"bold\" fill=\"#a85c00\">interior fails · boundary clear</text>\n",
                ml + 6.0,
                yd((gate_depth + disc_depth) / 2.0)
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"10\" \
                 fill=\"#a85c00\">over-difficulty past the gate; miner not dropped</text>\n",
                ml + 6.0,
                yd((gate_depth + disc_depth) / 2.0) + 15.0
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"11\" \
                 fill=\"#922b21\">both fail (cliff depth)</text>\n",
                ml + 6.0,
                yd((disc_depth + dmax_axis) / 2.0)
            ));
        } else {
            // CHAMPION: measured trough heatmap (it reacts → must be traced).
            // Cell rects with blue opacity ∝ depth of the trough below 1 — shows
            // the corner (deepens down-and-right) and that it never reaches red.
            for (di, &depth) in depths.iter().enumerate() {
                for (ri, &rate) in rates.iter().enumerate() {
                    // arm 0 = sleepy/champion (arm 1 is the deadband) — see `arms`.
                    let (_, trough) = get(0, rate, depth);
                    if !trough.is_finite() {
                        continue;
                    }
                    // cell extent: midpoints between neighbors on each axis.
                    let x0 = if ri == 0 { xr(ml, rmin as f64) } else {
                        xr(ml, (rates[ri - 1] as f64 * rate as f64).sqrt())
                    };
                    let x1 = if ri == rates.len() - 1 { xr(ml, rmax) } else {
                        xr(ml, (rate as f64 * rates[ri + 1] as f64).sqrt())
                    };
                    let y0 = if di == 0 { yd(0.0) } else {
                        yd((depths[di - 1] as f64 + depth as f64) / 2.0)
                    };
                    let y1 = if di == depths.len() - 1 { yd(dmax_axis) } else {
                        yd((depth as f64 + depths[di + 1] as f64) / 2.0)
                    };
                    let op = ((1.0 - trough) * 1.6).clamp(0.0, 0.85);
                    s.push_str(&format!(
                        "<rect x=\"{x0:.1}\" y=\"{y0:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"#2980b9\" fill-opacity=\"{op:.2}\"/>\n",
                        x1 - x0,
                        y1 - y0
                    ));
                }
            }
            // reference threshold lines (champion crosses NEITHER).
            s.push_str(&format!(
                "<line x1=\"{ml:.1}\" y1=\"{0:.1}\" x2=\"{1:.1}\" y2=\"{0:.1}\" \
                 stroke=\"#e67e22\" stroke-width=\"1.3\" stroke-dasharray=\"4 3\"/>\n",
                yd(gate_depth),
                ml + pw
            ));
            s.push_str(&format!(
                "<line x1=\"{ml:.1}\" y1=\"{0:.1}\" x2=\"{1:.1}\" y2=\"{0:.1}\" \
                 stroke=\"#c0392b\" stroke-width=\"1.3\" stroke-dasharray=\"4 3\"/>\n",
                yd(disc_depth),
                ml + pw
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" \
                 fill=\"#1f618d\">reacts → trough stays shallow (corner, deepens down-right)</text>\n",
                ml + pw / 2.0,
                yd(0.30)
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" \
                 fill=\"#555\">crosses NEITHER threshold on this slice;</text>\n",
                ml + pw / 2.0,
                yd(0.30) + 16.0
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" \
                 fill=\"#555\">its only failure is off-slice at sparse rate (§9.2, +2.7%)</text>\n",
                ml + pw / 2.0,
                yd(0.30) + 30.0
            ));
        }

        // UNREACHABLE hatch (both panels): depth > 4%·rate (cap-limited).
        // Polygon under the curve depth=cap_hours·rate/100.
        let mut poly = String::new();
        let steps = 60;
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let rate = (rmin.ln() + f * (rmax.ln() - rmin.ln())).exp();
            let dmax_r = (cap_hours * rate / 100.0).min(dmax_axis);
            poly.push_str(&format!("{:.1},{:.1} ", xr(ml, rate), yd(dmax_r)));
        }
        poly.push_str(&format!("{:.1},{:.1} ", ml + pw, yd(dmax_axis)));
        poly.push_str(&format!("{:.1},{:.1}", ml, yd(dmax_axis)));
        s.push_str(&format!(
            "<polygon points=\"{poly}\" fill=\"url(#unreach)\" stroke=\"#90a4ae\" \
             stroke-width=\"1\" stroke-dasharray=\"3 2\"/>\n"
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" \
             fill=\"#607d8b\" font-style=\"italic\">unreachable in a bounded (≤4h) decline — NOT safe</text>\n",
            ml + pw * 0.34,
            yd(dmax_axis) - 8.0
        ));

        // MODAL-decline cloud (both panels): depth 0.06–0.22 over gentle rates.
        let (cx0, cx1) = (xr(ml, 2.5), xr(ml, 22.0));
        let (cy0, cy1) = (yd(0.07), yd(0.20));
        s.push_str(&format!(
            "<rect x=\"{cx0:.1}\" y=\"{cy0:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"14\" \
             fill=\"none\" stroke=\"#2c3e50\" stroke-width=\"2\" stroke-dasharray=\"6 4\"/>\n",
            cx1 - cx0,
            cy1 - cy0
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" \
             font-weight=\"bold\" fill=\"#2c3e50\">modal declines (what miners actually see)</text>\n",
            (cx0 + cx1) / 2.0,
            cy1 + 15.0
        ));

        // plot frame + axes.
        s.push_str(&format!(
            "<rect x=\"{ml:.1}\" y=\"{mt:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\" \
             fill=\"none\" stroke=\"#999\"/>\n"
        ));
        // x ticks (rate).
        for &rate in rates {
            let x = xr(ml, rate as f64);
            s.push_str(&format!(
                "<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9\" \
                 fill=\"#666\">{}</text>\n",
                mt + ph + 14.0,
                rate as u32
            ));
        }
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" \
             fill=\"#444\">decline rate (%/hr, log)</text>\n",
            ml + pw / 2.0,
            mt + ph + 32.0
        ));
        // y ticks (depth) — only on left panel to save space.
        if pi == 0 {
            for d in [0.0, 0.2, 0.4, 0.6, 0.8] {
                let y = yd(d);
                s.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"9\" \
                     fill=\"#666\">{:.0}%</text>\n",
                    ml - 6.0,
                    y + 3.0,
                    d * 100.0
                ));
            }
            s.push_str(&format!(
                "<text x=\"22\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#444\" \
                 transform=\"rotate(-90 22 {:.1})\">decline depth (fraction of hashrate lost) →</text>\n",
                mt + ph / 2.0,
                mt + ph / 2.0
            ));
        }
    }

    // bottom caption — the read, with the honest scope.
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10.5\" fill=\"#555\">\
         Deadband: one no-op identity (e=−ln(1−d)) yields both boundaries — the over-difficulty \
         half-plane is vertical in rate because nothing acts on it. Modal declines sit in the gap: \
         past the gate, far from disconnect.</text>\n",
        w / 2.0,
        h - 14.0
    ));

    s.push_str("</svg>\n");
    s
}
