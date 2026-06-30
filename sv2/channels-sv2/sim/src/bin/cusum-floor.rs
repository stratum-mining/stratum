//! Optimal-detector (Page CUSUM) floor for a −g Poisson rate drop, computed
//! BY SIMULATION (not the analytic ln(ARL0)/KL approximation), so the
//! floor-relative responsiveness gate `reaction ≤ k × floor` is anchored to
//! a number whose false-alarm convention is stated, not inherited slop.
//!
//! Page CUSUM for a rate change r* → (1−g)·r*: per tick of expected count
//! μ0 = r*·Δt (no drop) vs μ1 = (1−g)·μ0 (dropped), the LLR increment for
//! observing N is  s = N·ln(μ1/μ0) − (μ1 − μ0).  Statistic S = max(0, S+s);
//! fire when S ≥ h. A low N (slow shares = rate drop) pushes S up.
//!
//! Calibrate h so the no-drop false-alarm rate hits a STATED ARL0 (mean
//! ticks between false alarms on a stable stream), then measure the median
//! detection delay after a real drop. Report across ARL0 ∈ {60,240,1440} min
//! so the floor's false-alarm dependence is VISIBLE and k is interpreted
//! against a named convention. This is the Lorden-optimal bound: no detector
//! reacts faster at the same false-alarm rate.
//!
//! Deterministic LCG RNG (seeded) so the floor is reproducible without
//! Date/rand. Usage: cargo run --release --bin cusum-floor

const TICK_MIN: f64 = 1.0; // 60s ticks, matching the sim

struct Lcg(u64);
impl Lcg {
    fn next_u64(&mut self) -> u64 { self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); self.0 }
    fn unif(&mut self) -> f64 { (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 }
    fn poisson(&mut self, lambda: f64) -> u32 {
        if lambda <= 0.0 { return 0; }
        // Knuth
        let l = (-lambda).exp(); let mut k = 0u32; let mut p = 1.0;
        loop { k += 1; p *= self.unif(); if p <= l { return k - 1; } }
    }
}

/// Median of a sorted-able vec.
fn median(mut v: Vec<f64>) -> f64 { if v.is_empty() { return f64::NAN; } v.sort_by(|a,b| a.partial_cmp(b).unwrap()); v[v.len()/2] }

/// Run CUSUM with threshold h on a no-drop stream of `horizon` ticks; return
/// whether it false-alarmed (fired) and at which tick.
fn run_nodrop(h: f64, mu0: f64, g: f64, horizon: usize, rng: &mut Lcg) -> Option<usize> {
    let mu1 = (1.0 - g) * mu0;
    let (ln_ratio, dmu) = ((mu1/mu0).ln(), mu1 - mu0);
    let mut s = 0.0;
    for t in 0..horizon {
        let n = rng.poisson(mu0) as f64;
        s = (s + n * ln_ratio - dmu).max(0.0);
        if s >= h { return Some(t); }
    }
    None
}

/// Detection delay (ticks) after a drop at tick 0; None if not detected in horizon.
fn run_drop(h: f64, mu0: f64, g: f64, horizon: usize, rng: &mut Lcg) -> Option<usize> {
    let mu1 = (1.0 - g) * mu0;
    let (ln_ratio, dmu) = ((mu1/mu0).ln(), mu1 - mu0);
    let mut s = 0.0;
    for t in 0..horizon {
        let n = rng.poisson(mu1) as f64; // post-drop stream
        s = (s + n * ln_ratio - dmu).max(0.0);
        if s >= h { return Some(t); }
    }
    None
}

/// Empirical ARL0 (mean ticks to false alarm) for threshold h.
fn arl0(h: f64, mu0: f64, g: f64, rng: &mut Lcg) -> f64 {
    let trials = 4000; let horizon = 100_000;
    let mut sum = 0.0; let mut cnt = 0;
    for _ in 0..trials {
        if let Some(t) = run_nodrop(h, mu0, g, horizon, rng) { sum += t as f64 + 1.0; cnt += 1; }
        else { sum += horizon as f64; cnt += 1; }
    }
    sum / cnt as f64
}

fn floor_delay_min(h: f64, mu0: f64, g: f64, rng: &mut Lcg) -> f64 {
    let trials = 8000; let horizon = 100_000;
    let mut delays = Vec::with_capacity(trials);
    for _ in 0..trials {
        if let Some(t) = run_drop(h, mu0, g, horizon, rng) { delays.push((t as f64 + 1.0) * TICK_MIN); }
    }
    median(delays)
}

fn main() {
    // Drop magnitude via env (VARDIFF_FLOOR_G, default 0.10). The −25% floor
    // MUST use the identical false-alarm convention (ARL0 grid + calibration)
    // as the −10% floor, or cross-stimulus comparison is meaningless.
    let g: f64 = std::env::var("VARDIFF_FLOOR_G").ok().and_then(|s| s.parse().ok()).unwrap_or(0.10);
    let spms = [4.0f64, 6.0]; // production rates (and 30 for high-r* context)
    let target_arl0_min = [60.0f64, 240.0, 1440.0]; // 1h, 4h, 1day false-alarm budgets

    println!("## CUSUM (Lorden-optimal) floor for −{}% drop, by simulation.", (g*100.0) as u32);
    println!("Floor = median detection delay of the optimal detector at a STATED false-alarm");
    println!("budget (ARL0). The responsiveness gate is reaction ≤ k × floor at the chosen ARL0.\n");
    println!("| spm | ARL0 (false-alarm budget) | calibrated h | floor delay (min) |");
    println!("| --- | --- | --- | --- |");
    for &spm in &[4.0f64, 6.0, 30.0] {
        let mu0 = spm * TICK_MIN; // expected count per tick on a stable stream
        for &arl in &target_arl0_min {
            // bisect h to hit target ARL0
            let mut rng = Lcg(0x1234_5678_9abc_def0 ^ ((spm as u64) << 8) ^ (arl as u64));
            let (mut lo, mut hi) = (0.0f64, 200.0f64);
            for _ in 0..22 {
                let mid = 0.5*(lo+hi);
                let a = arl0(mid, mu0, g, &mut rng);
                if a < arl { lo = mid; } else { hi = mid; }
            }
            let h = 0.5*(lo+hi);
            let mut rng2 = Lcg(0xfeed_face_0bad_c0de ^ ((spm as u64) << 8) ^ (arl as u64));
            let delay = floor_delay_min(h, mu0, g, &mut rng2);
            println!("| {} | {} min | {:.2} | {:.1} |", spm as u32, arl as u32, h, delay);
        }
    }
    let _ = spms;
    println!("\nInterpretation: at production 4–6 spm the −10% floor is LONG (Theorem 2 biting),");
    println!("which is exactly why the gate must be floor-RELATIVE (k×floor), not an absolute N.");
    println!("k is chosen against the floor at a STATED ARL0; pick the ARL0 matching the");
    println!("detection metric's false-alarm convention and report k against that.");
}
