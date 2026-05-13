# Vardiff Characterization Framework — Design Proposal

## Goal

Build a deterministic in-process simulation framework that measures the operationally-important behavioral attributes of any `Vardiff` trait implementation, and expose those measurements as an assertable CI policy.

The framework is the deliverable. The current `VardiffState` (classic) algorithm becomes the first characterized baseline. Any future algorithmic proposal — parametric thresholds, EWMA, SPRT, anything else — plugs into the same harness and produces a comparable delta report. The goal is to surface existing problems concretely (jitter, ramp-up time) so that proposed improvements have to demonstrate, with numbers, what they move and in which direction.

## Why simulation rather than network integration

Three properties make in-process simulation the right level for *characterization*. Network integration tests remain valuable for *plumbing verification* — they're complementary, not competing.

**Determinism.** Seeded RNG means every run produces identical numbers. Two CI runs of the same algorithm produce byte-identical reports. Statistical claims become checkable rather than "well, in my run I saw...".

**Speed.** A network integration test takes ~5-10 minutes per trial because the 60s ticker is real wall-clock time. The same trial in simulation runs in ~100-300μs — five orders of magnitude faster. That's exactly the gap needed to characterize a distribution from `N=1000+` independent trials per cell. The full baseline runs in ~10 seconds.

**Coverage.** Simulation can sweep parameters cheaply. A baseline characterization across 5 share rates × 6 hashrate scenarios × 1000 trials = 30,000 trials is comfortably tractable. Running the same as network integration would take ~150 days.

The tradeoff is fidelity: simulation uses idealized Poisson share arrival, while real miners exhibit clustering, batching, and irregular submission. The framework's claims are therefore *conservative comparisons between algorithms under identical Poisson load* ("A jitters 87% less than B"), not absolute predictions about deployment behavior. Real-world plausibility is the job of testnet4 deployment and network integration tests — not this framework.

## Architecture

Three layers, each independently testable:

**Layer 1 — `Vardiff` trait.** Already exists in `sv2/channels-sv2/src/vardiff/mod.rs`. `pub trait Vardiff` defines `try_vardiff`, `reset_counter`, etc. `VardiffState` is currently the sole implementor. Any new algorithm implements the same trait and becomes interchangeable in the harness.

**Layer 2 — Simulation harness.** New `vardiff-sim` crate. Drives a `Vardiff` implementation through a single trial against a mocked clock and a Poisson share stream. Produces a `Trial` record containing the full timeline of fires.

**Layer 3 — Metric computation and reporting.** Library functions taking the `Vec<Trial>` for a given parameter combination — what we'll call a **cell** — and producing metric distributions: percentile tables, sensitivity curves, summary statistics. A *cell* is one tuple of `(algorithm, share_rate, scenario)`, where `scenario` is a `HashrateSchedule` such as "stable 1 PH/s" or "step down 50% at 15 min." The full baseline run sweeps a fixed grid of cells (5 share rates × 6 scenarios = 30 cells in the default configuration). Output formats: structured TOML for CI assertions, markdown for human review.

## Simulation mechanism

### Mock clock

The classic algorithm currently calls `SystemTime::now()` directly inside `try_vardiff`. To run trials at >1000× wall-clock speed, the framework introduces a `Clock` trait:

```rust
trait Clock {
    fn now_secs(&self) -> u64;
}

struct SystemClock;
impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }
}

struct MockClock { now: AtomicU64 }
impl Clock for MockClock {
    fn now_secs(&self) -> u64 { self.now.load(Ordering::Relaxed) }
}
impl MockClock {
    fn advance(&self, secs: u64) { self.now.fetch_add(secs, Ordering::Relaxed); }
    fn set(&self, secs: u64) { self.now.store(secs, Ordering::Relaxed); }
}
```

`VardiffState::try_vardiff` and `reset_counter` are modified to accept a `&dyn Clock` parameter. `SystemClock` is the default; production behavior is identical to current code. The change is mechanical, ~50 lines.

This is the only invasive change to the existing algorithm. If clock injection is rejected by maintainers, the framework can't work — there's no good alternative.

### Poisson share stream

Given a true miner hashrate `H` (hashes/sec) and a current pool-side target `T`, the rate at which the miner produces accepted shares is `λ = H × T_normalized` per second, where `T_normalized` is the target as a fraction of `2^256`. Inter-arrival times are `Exp(λ)`.

Shares are *not* pre-generated in bulk for the whole trial — when the target changes mid-trial (vardiff fires and applies a new hashrate, which the harness converts to a new target), the share rate λ changes. Exponential interarrival is memoryless, so the next share's time is sampled afresh from `Exp(λ_new)` starting at the current simulated time. This is statistically correct and operationally simple.

The RNG is a deterministic `XorShift64` seeded from the trial's seed value. Same seed produces identical share arrival sequence.

### Hashrate schedule

A trial's true miner hashrate is a step function over time:

```rust
struct HashrateSchedule {
    segments: Vec<(u64, f32)>,  // (start_secs, hashrate_h_per_sec)
}

impl HashrateSchedule {
    fn at(&self, secs: u64) -> f32 {
        self.segments.iter()
            .rev()
            .find(|(start, _)| secs >= *start)
            .map(|(_, h)| *h)
            .unwrap_or(self.segments[0].1)
    }
}
```

This makes it cheap to express scenarios:

- **Stable**: `[(0, 1e15)]` — constant 1 PH/s for trial duration
- **Step down**: `[(0, 1e15), (900, 0.5e15)]` — halves at 15 min
- **Step up**: `[(0, 1e15), (900, 2e15)]` — doubles at 15 min
- **Throttle**: `[(0, 1e15), (900, 0.7e15), (1200, 1e15)]` — 30% drop for 5 min, then recovers
- **Cold start far off**: `[(0, 1e15)]` with `initial_target` chosen to imply 10 GH/s — algorithm has to find truth from way off

### Trial loop

```rust
fn run_trial<V: Vardiff>(
    vardiff: &mut V,
    clock: &MockClock,
    config: TrialConfig,
    schedule: &HashrateSchedule,
    seed: u64,
) -> Trial {
    let mut rng = XorShift64::new(seed);
    let mut current_target = config.initial_target;
    let mut configured_hashrate = config.initial_hashrate_estimate;
    let mut fires = Vec::new();
    
    let mut next_share_at = 0u64;
    let mut next_tick_at = 60u64;
    
    loop {
        let next_event_at = next_share_at.min(next_tick_at);
        if next_event_at > config.duration_secs { break; }
        clock.set(next_event_at);
        
        if next_share_at <= next_tick_at {
            // Share event
            vardiff.increment_shares_since_last_update();
            let true_h = schedule.at(clock.now_secs());
            let lambda = true_h as f64 * target_to_fraction(&current_target);
            let delta_secs = sample_exponential(&mut rng, lambda).round() as u64;
            next_share_at += delta_secs.max(1);
        } else {
            // Tick event
            if let Ok(Some(new_h)) = vardiff.try_vardiff(
                configured_hashrate,
                &current_target,
                config.shares_per_minute,
                clock,
            ) {
                fires.push(FireEvent {
                    at_secs: clock.now_secs(),
                    old_hashrate: configured_hashrate,
                    new_hashrate: new_h,
                });
                configured_hashrate = new_h;
                current_target = hashrate_to_target(new_h, config.shares_per_minute);
            }
            next_tick_at += 60;
        }
    }
    
    Trial {
        fires,
        final_target: current_target,
        final_configured_hashrate: configured_hashrate,
        final_true_hashrate: schedule.at(config.duration_secs),
        duration_secs: config.duration_secs,
        config: config.clone(),
    }
}
```

(Sketch — actual implementation handles edge cases more carefully. The structure is the point.)

### Determinism

Every trial is fully deterministic given its seed. The full baseline run fixes a `base_seed` and derives per-trial seeds as `base_seed.wrapping_add(cell_index << 16).wrapping_add(trial_index)`. Anyone running the same baseline gets byte-identical results — including the test runner in CI.

## The five metrics

Each metric is described as a pipeline: definition → measurement → collection → characterization → summarization → CI assertion form.

### 1. Convergence time

**Definition.** Wall-clock time from `t=0` (trial start, defined as first tick) until the algorithm has not fired for `quiet_window = 5 min`. The reported value is the timestamp of the **last fire** before the quiet window. If no fire happens at all during the trial, convergence_time is `0`. If fires never stop within `convergence_timeout = 30 min`, the trial is `DNF`.

**Measurement.** Iterate over `trial.fires` in chronological order. Convergence time = `f.at_secs` for the first fire `f` such that no subsequent fire exists with timestamp in `[f.at_secs, f.at_secs + quiet_window]`. If no such fire and `trial.duration_secs >= convergence_timeout`, mark `DNF`.

**Collection.** Per cell `(algorithm, share_rate, scenario)`, run `N=1000` trials with distinct seeds. Collect `Vec<Option<u64>>` — `Some(t)` for converged, `None` for DNF.

**Characterization.** Two outputs:
- `convergence_rate ∈ [0, 1]`: fraction of trials that converged
- `convergence_distribution`: p10, p25, p50, p75, p90, p95, p99 over converged trials

**Summarization.** Line in baseline report:

```
Convergence (12 spm, cold start 10 GH/s → 1 PH/s true):
  rate: 998/1000 (99.8%)
  p10=7m45s  p50=8m30s  p90=9m50s  p95=10m25s  p99=11m20s
```

**CI assertion.** Two-part:
- `current.convergence_rate >= baseline.convergence_rate - 0.01` — catastrophic regression guard
- `current.p90 <= baseline.p90 * 1.10` — tail-end slowdown guard

Tail percentiles (p90/p95) are asserted rather than median because regressions in convergence behavior most commonly surface at the tails — a small fraction of trials hitting a new pathology.

### 2. Settled accuracy

**Definition.** Relative error between the algorithm's settled target and the *ideal* target that produces exactly `shares_per_minute` at the true miner hashrate.

```
ideal_difficulty = true_hashrate × 60 / (shares_per_minute × 2^32)
ideal_target = difficulty_to_target(ideal_difficulty)
accuracy_error = abs(target_to_difficulty(final_target) / ideal_difficulty - 1)
```

Measured at trial end. Only meaningful for converged trials.

**Measurement.** Read `trial.final_target` and `trial.final_true_hashrate`. Compute per formula. Skip DNF trials.

**Collection.** `Vec<f64>` of accuracy errors over converged trials.

**Characterization.** Percentile distribution. The distribution can be asymmetric — Poisson noise pushes accuracy either direction; over-estimates tend to have a longer tail than under-estimates because the share-count distribution is right-skewed at low expected counts. Report both percentile distribution and asymmetry indicator (`p90 / p10` ratio).

**Summarization.**

```
Settled accuracy (12 spm, stable 1 PH/s):
  p10=2.1%   p50=8.9%   p90=17.4%   p95=20.9%   p99=24.3%
```

**CI assertion.** `current.p50 <= baseline.p50 * 1.15 && current.p90 <= baseline.p90 * 1.15`. The 15% tolerance accounts for natural variation in trial seed selection without trial-count adjustment; tighter risks false positives.

### 3. Steady-state jitter

**Definition.** Fires per minute during the *settled period* of a trial. Settled period = `[convergence_time + settle_buffer, trial_end]` where `settle_buffer = 2 min` ensures any post-convergence transient fires don't pollute the count. Trial must have converged with at least `min_settled_window = 10 min` remaining.

**Measurement.** Count fires in `[convergence_time + 2min, trial_end]`. Divide by `(trial_end - convergence_time - 2min) / 60.0` to get fires/min.

**Collection.** `Vec<f64>` of jitter values over qualifying trials.

**Characterization.** Percentile distribution. Most algorithms produce a distribution heavily concentrated near zero with occasional outlier spikes; both p50 and p95 matter. Report mean as well — useful for "expected fires per hour" calculations.

**Summarization.**

```
Jitter (12 spm, stable 1 PH/s, ~20-min settled window):
  p50=0.04 fires/min   p90=0.16   p95=0.22   p99=0.32
  mean=0.07            (~4 fires/hour at steady state)
```

**CI assertion.** Two-part:
- `current.p50 <= baseline.p50 + 0.02` — absolute tolerance because baseline can be near zero where multiplicative tolerance is meaningless
- `current.p95 <= baseline.p95 * 1.25` — multiplicative tolerance on tail

### 4. Reaction time

**Definition.** Wall-clock time from a scheduled hashrate change event to the first fire after that event. Trial schedule must include a step change at a fixed `change_at_secs` (default 15 min). If no fire occurs within `react_window = 10 min` after change, record `DNF`.

**Measurement.** Find smallest `f.at_secs - change_at_secs` over `f in trial.fires` with `f.at_secs > change_at_secs`. If none within `react_window`, mark `DNF`.

**Collection.** Per (algorithm, share_rate, change_magnitude) cell, `Vec<Option<u64>>`.

**Characterization.** `reaction_rate` (fraction reacting in window) + percentile distribution over reacting trials.

**Summarization.**

```
Reaction time (12 spm, -50% step at 15 min):
  reacted: 1000/1000 (100%)
  p10=1m15s  p50=2m30s  p90=4m45s  p95=5m50s  p99=7m20s
```

**CI assertion.** `current.reaction_rate >= baseline.reaction_rate - 0.02 && current.p50 <= baseline.p50 * 1.20`. Looser tolerance than convergence because reaction time has higher inherent variance — it depends on share-arrival timing relative to step change.

### 5. Reaction sensitivity

**Definition.** Probability of *any* fire within `react_window` after a step change of magnitude `Δ`, as a function of `Δ`. This is a *curve* — for each `Δ` in `{-50%, -25%, -10%, -5%, +5%, +10%, +25%, +50%}`, compute `P(fire within react_window | Δ)`.

**Measurement.** For each `(algorithm, share_rate, Δ)` cell, run `M=500` trials. Sensitivity at `Δ` = (# trials with ≥1 fire in `[change_at_secs, change_at_secs + react_window]`) / M.

**Collection.** Per cell, `HashMap<Delta, (f64, f64)>` mapping each Δ to `(point_estimate, 95%_CI_half_width)`. The CI is computed from the binomial distribution: `1.96 * sqrt(p * (1-p) / M)`.

**Characterization.** A 2D table — rows are Δ values, columns are share rates.

**Summarization.**

```
Reaction sensitivity (P[fire within 5 min of step change]):
              6 spm     12 spm    30 spm    60 spm    120 spm
  Δ=-50%:   0.97±0.02  1.00      1.00      1.00      1.00
  Δ=-25%:   0.65±0.04  0.81±0.03 0.94±0.02 0.99±0.01 1.00
  Δ=-10%:   0.18±0.03  0.31±0.04 0.55±0.04 0.78±0.04 0.95±0.02
  Δ=-5%:    0.05±0.02  0.09±0.03 0.18±0.03 0.31±0.04 0.52±0.04
  Δ=+5%:    0.06±0.02  0.10±0.03 0.20±0.04 0.33±0.04 0.55±0.04
  Δ=+10%:   0.22±0.04  0.33±0.04 0.59±0.04 0.80±0.04 0.96±0.02
  Δ=+25%:   0.71±0.04  0.85±0.03 0.95±0.02 1.00      1.00
  Δ=+50%:   0.99±0.01  1.00      1.00      1.00      1.00
```

**CI assertion.** The shape of the curve matters more than any single point. Two assertions enforce its boundary behavior:

- **Large-delta sensitivity floor**: `current.sensitivity[|Δ| ≥ 50%] >= baseline - 0.02` — algorithm must fire on genuine large changes
- **Small-delta sensitivity ceiling**: `current.sensitivity[|Δ| ≤ 5%] <= baseline + 0.05` — algorithm must not fire eagerly on noise-level deviations

Mid-range Δ values (10-25%) are reported but not asserted — they're where the legitimate algorithmic tradeoffs live, and a reviewer should look at the full curve in the PR comment to evaluate them. A PR that legitimately shifts mid-range sensitivity is making an intentional design choice, not a regression, and the assertions shouldn't pretend otherwise.

## Performance

Estimated wall-clock runtimes:

| Configuration | Trial count | Wall time | Use case |
| -- | -- | -- | -- |
| Smoke | 5 rates × 3 scenarios × 100 trials = 1,500 | ~500 ms | Every `cargo test` invocation |
| Standard | 5 rates × 6 scenarios × 1,000 trials = 30,000 | ~10 s | PR-gating CI workflow |
| Full baseline | 5 rates × 6 scenarios × 10,000 trials = 300,000 | ~100 s | Nightly drift detection |

Caveats:
- Single-threaded. Parallelizing across cells with rayon would shorten by a factor of (cores − 1).
- Release-mode Rust. Debug mode is 5-10× slower; the CI integration should pin release mode for the sim crate even when running under `cargo test`.
- Performance ceiling is `try_vardiff` invocation cost (~10 μs). If a future algorithm has substantially more expensive per-call logic (e.g., SPRT with bookkeeping), runtimes scale linearly.

**Verdict: standard CI is comfortable.** A 10-second PR-gating test fits well within typical CI budgets and provides better-than-current evidence of algorithmic behavior. The framework is fast enough that the limiting factor is assertion design, not compute.

## CI assertion policy

Three tiers:

**Tier 1: Smoke test in `cargo test`.**
Runs the smoke configuration on every test invocation, including local developer runs. Asserts only catastrophic regressions:
- All `convergence_rate >= baseline - 0.05`
- All `p50` metrics within 2× of baseline
- All `reaction_sensitivity[|Δ| ≥ 50%] >= 0.90`

Time budget: < 1 second. Purpose: catch outright algorithm breakage during development without waiting for full CI.

**Tier 2: PR characterization in CI.**
Runs the standard configuration. Generates a delta report against the checked-in baseline. Asserts the per-metric tolerances described above (~10-20% on percentile values, absolute floors on probabilities). Posts the delta report as a PR comment. Required for merge.

Time budget: ~15-30 seconds (with reporting overhead). Run on every PR touching `vardiff/*`.

**Tier 3: Nightly baseline refresh.**
Runs the full configuration. If current measurements differ from checked-in baseline by less than statistical-significance-thresholds (95% CI overlap), no action. If differs significantly, opens an issue rather than breaking the build — could indicate environmental drift, RNG behavior change, etc.

Purpose: detect slow drift, verify long-term stability of measurement infrastructure.

## Baseline file format

```toml
# vardiff_baseline.toml
[meta]
algorithm = "VardiffState"
algorithm_git_sha = "3ee7d8e1"
seed = 0xdeadbeef
trial_count = 1000
generated_at = "2026-05-09T..."

[cell.spm_12.cold_start_10gh_to_1ph]
convergence_rate = 0.998
convergence_p10 = 465
convergence_p50 = 510
convergence_p90 = 590
convergence_p95 = 625
convergence_p99 = 680

settled_accuracy_p10 = 0.021
settled_accuracy_p50 = 0.089
settled_accuracy_p90 = 0.174
settled_accuracy_p95 = 0.209
settled_accuracy_p99 = 0.243

[cell.spm_12.stable_1ph]
jitter_p50 = 0.04
jitter_p90 = 0.16
jitter_p95 = 0.22
jitter_p99 = 0.32
jitter_mean = 0.07

[cell.spm_12.step_minus_50_at_15min]
reaction_rate = 1.0
reaction_p10 = 75
reaction_p50 = 150
reaction_p90 = 285
reaction_p95 = 350
reaction_p99 = 440

[cell.spm_12.sensitivity]
delta_minus_50 = 1.00
delta_minus_25 = 0.81
delta_minus_10 = 0.31
delta_minus_5  = 0.09
delta_plus_5   = 0.10
delta_plus_10  = 0.33
delta_plus_25  = 0.85
delta_plus_50  = 1.00

# ... repeated for each share rate ...
```

The baseline is checked into the repo. When a PR legitimately improves performance, the PR includes regenerated baseline values (the framework provides `cargo run -p vardiff-sim --bin refresh-baseline` for this) and the reviewer can diff the TOML to see exactly what changed.

## Assertion code shape

```rust
// in vardiff-sim/tests/baseline_regression.rs

#[test]
fn classic_algorithm_no_regression() {
    let baseline = load_baseline("vardiff_baseline.toml");
    let report = run_standard_baseline::<VardiffState>(baseline.seed);
    
    for (cell_key, current) in &report.cells {
        let prev = baseline.cells.get(cell_key)
            .unwrap_or_else(|| panic!("missing baseline for {cell_key}"));
        
        if let (Some(c_rate), Some(b_rate)) = (current.convergence_rate, prev.convergence_rate) {
            assert!(c_rate >= b_rate - 0.01,
                "{cell_key}: convergence_rate {c_rate} < baseline {b_rate} - 0.01");
        }
        if let (Some(c), Some(b)) = (current.convergence_p90, prev.convergence_p90) {
            assert!(c <= (b as f64 * 1.10) as u64,
                "{cell_key}: convergence p90 {c} > baseline {b} × 1.10");
        }
        if let (Some(c), Some(b)) = (current.jitter_p50, prev.jitter_p50) {
            assert!(c <= b + 0.02,
                "{cell_key}: jitter p50 {c} > baseline {b} + 0.02");
        }
        // ... and so on
    }
}
```

Each assertion is named, identifies the cell, and produces a clear failure message showing baseline vs. current values.

## Implementation plan (scope-narrowed)

Five-step plan, ~3-4 days of focused work:

1. **Algorithm clock injection** — `Clock` trait + `MockClock` impl. Modify `VardiffState::try_vardiff` and `reset_counter` to accept `&dyn Clock`. Default everywhere to `SystemClock`. ~50 LOC + minor test updates.

2. **`vardiff-sim` crate** — New crate under `sv2/channels-sv2/sim/`. Contains: `Trial`, `TrialConfig`, `HashrateSchedule`, `FireEvent`, `run_trial`, the metric-computation functions, `Distribution<T>` helper, percentile helper, exponential sampler with seeded RNG. ~500 LOC.

3. **Baseline characterization binary** — `cargo run -p vardiff-sim --bin generate-baseline`. Runs the full baseline against `VardiffState`, writes `vardiff_baseline.toml` and a human-readable `vardiff_baseline.md`. ~200 LOC.

4. **Regression test** — `#[test]` in sim crate that runs standard configuration and asserts per-metric tolerances against checked-in baseline. ~100 LOC.

5. **Documentation** — `VARDIFF_SIMULATION.md` explaining the framework, what each metric means, how to read the baseline report, how to add a new algorithm, how to legitimately update baseline values in a PR. ~300 lines of prose.

Total: ~850 LOC + 300 lines of prose.

## Decisions deferred to implementation

The following design questions will be revisited when the framework is closer to implementation. Each affects the rigor of the assertion policy rather than the framework's structure, and can be added incrementally without redesigning the core.

- **Per-percentile confidence intervals on baseline values.** Whether to compute and store per-percentile CIs (binomial for sensitivity, bootstrap for time percentiles) and use "current value within baseline CI" as the assertion threshold rather than fixed tolerances.

- **Algorithm version tracking in baseline.** How to record the algorithm crate's identity (git SHA, version string) in the baseline file and detect mismatches between baseline and current algorithm.

- **RNG seed scope and derivation.** Whether the framework uses a single base seed for the entire baseline run or distinct per-cell seeds, and the derivation scheme used.

## What this framework does NOT do

Worth being explicit about scope limits:

- **Doesn't validate that vardiff is wired into pool/jdc/translator correctly.** That's the network integration test's job.
- **Doesn't validate behavior under real miner share-arrival patterns.** Poisson is idealized; real miners are messier. Testnet4 observation is the right tool there.
- **Doesn't predict absolute deployment metrics.** It produces *comparisons* between algorithms under identical simulated load. "Algorithm A jitters 87% less than B" is the kind of claim it supports; "Algorithm A will produce 0.04 fires per minute on testnet4" is not.
- **Doesn't account for cross-component interaction.** Pool, JDC, and translator each run independent vardiff state machines that can fire on each other's adjustments. The framework characterizes one channel in isolation, not the multi-layer system.

These are all valuable testing surfaces — they just live elsewhere in the test pyramid. The simulation framework's value is precisely that it isolates the algorithm from those other variables, so changes in algorithmic behavior are visible without confounders.
