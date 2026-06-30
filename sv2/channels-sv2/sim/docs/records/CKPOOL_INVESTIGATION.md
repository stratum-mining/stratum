# Ckpool Vardiff Investigation

> **Re-validated against current upstream (2026-06-27) — HELD VERBATIM, zero drift.**
> Every mechanism this doc rests on was re-checked against current
> `ckolivas/ckpool` source and matches exactly: `decay_time` (`libckpool.c:2051` —
> `fprop = 1 − e^(−fsecs/interval)`, the formula quoted below); the five per-client
> EMAs (1m/5m/1h/1d/1w); the `ssdc≥72` dual-window switch (1m vs 5m EMA), the
> 72-shares-OR-240s gate, the drr `(0.15, 0.4)` hysteresis band around the 0.3
> product, `optimal = dsps×3.33`, and the `ssdc==1` oscillation guard — all in
> `stratifier.c:5778–5859`. No content change needed; recorded so the revalidation is
> on file and not re-litigated unless the source moves. (Companion: the DMND/PID pool
> was re-validated the same pass and DID drift — see `PID_INVESTIGATION.md` dead-zone §
> — making this verbatim-hold the contrast case.)

## Background

[ckpool](https://github.com/ckolivas/ckpool) is a mature Stratum V1 pool
by Con Kolivas with a well-regarded vardiff implementation. A [Rust
reimplementation](https://github.com/parasitepool/para/blob/master/src/vardiff.rs)
by @paratoxicdev made the algorithm more configurable. This investigation
ports ckpool's core ideas to our tick-based SV2 framework, benchmarks
them against the existing roster, and identifies which ideas transfer
and which are context-dependent.

## Ckpool's Algorithm (src/stratifier.c)

### Core: Exponentially Decaying Shares-Per-Second

ckpool tracks `dsps` (difficulty-weighted shares per second) using an EMA
updated on every share submission via `decay_time()`:

```c
fprop = 1 - e^(-elapsed / interval)
f += (share_diff / elapsed) * fprop
f /= (1 + fprop)
```

Five parallel EMAs with different time constants (1m, 5m, 1h, 1d, 1w) are
maintained per client.

### Adaptive Window Switching

When shares flood in (`ssdc >= 72`), the 1-minute EMA is used for
evaluation — enabling rapid ramp-up. Otherwise the 5-minute EMA provides
conservative steady-state tracking. The threshold 72 = `240s / 3.33s`
(shares expected in 80% of the long window at target rate).

### Hysteresis Band

No retarget if the diff-rate-ratio (`drr = dsps / current_diff`) falls
within [0.15, 0.4] around the target 0.3 (~1 share per 3.33s). The band
is asymmetric: 0.5× below target, 1.33× above.

### Oscillation Guard

Suppress difficulty decrease if only 1 share has been observed since the
last change — prevents premature drops after idle periods.

### Time-Bias Warmup Correction

`bias = 1 - e^(-elapsed/period)` compensates for EMA suppression when the
client has been active for less than one full window period.

## Mapping to the Three-Stage Pipeline

### First Attempt: Direct Translation (Batch EMA + Time-Bias)

The initial port treated one tick as one "observation" — batching all shares
into a single EMA step, then dividing by the time-bias factor to compensate
for warmup:

| Stage | Component |
|-------|-----------|
| Estimator | Dual-window EWMA (τ_short=60s, τ_long=300s) with time-bias correction |
| Boundary | HysteresisGate [0.5, 1.33] with data gate (72 shares OR 240s) |
| Update | CkpoolRetarget (full retarget + oscillation guard) |

**Result: Catastrophic.** The hysteresis band was too wide for 60s-tick
evaluation — rate ratios wandered far from 1.0 while staying "inside" the
band. Settled accuracy 59-73% at SPM 6-12. Overshoot 100-200%.

### CkpoolRemedy (ckpool estimator + FullRemedy boundary/update)

> *`FullRemedy` here is the mid-arc waypoint of the Classic→champion derivation
> (EWMA120 / PoissonCI / PartialRetarget) — a named benchmark baseline in this doc,
> **superseded as the recommendation** (it does not clear the slow-decline gate; the
> shipped pick is the champion `Ewma360`). See [`FINDINGS.md`](./FINDINGS.md).*

Kept ckpool's estimator but replaced the boundary and update with proven
components (PoissonCI + PartialRetarget η=0.2):

**Result: Also bad.** Settled accuracy 177-275%, overshoot 350-427%.
The time-bias correction (`1 / (1 - e^(-60/300)) ≈ 1/0.18 ≈ 5.5×`)
massively amplified the rate estimate on every tick after a fire.

### Root Cause: Time-Bias Is Not Portable

The time-bias formula was calibrated for per-share evaluation (~3.33s
intervals) where `dt` grows smoothly. In the tick-based framework,
`dt` after a fire is always exactly 60s. With τ=300s:

```
time_bias(60, 300) = 1 - e^(-0.2) ≈ 0.18
correction = rate / 0.18 = rate × 5.5
```

This amplification is the overshoot source: the EMA is naturally low
after a fire (only one tick of data), and dividing by 0.18 inflates it
by 5.5× — a 450% bias on the first tick after every retarget.

## The Fix: Per-Share `decay_time()` Simulation

Instead of batch updates with post-hoc bias correction, we simulate
ckpool's exact per-share EMA updates within `snapshot()`:

When N shares arrive in a 60s tick, run N individual `decay_time()` calls
with `elapsed = 60/N` seconds each. This faithfully reproduces what
ckpool's EMA would have seen — the decay accumulates organically through
per-share updates rather than needing artificial amplification.

```rust
fn snapshot(&self, dt_secs: u64, ctx: &EstimatorContext) -> EstimatorSnapshot {
    // ... 
    if pending > 0 {
        let inter_share_secs = self.tick_secs as f64 / pending as f64;
        for _ in 0..pending {
            dsps_s = Self::decay_time(dsps_s, 1.0, inter_share_secs, self.tau_short as f64);
            dsps_l = Self::decay_time(dsps_l, 1.0, inter_share_secs, self.tau_long as f64);
        }
    }
    // ...
}
```

**Result:** Completely fixed the bias. Settled accuracy dropped from
177% to 4.6-7.5% — matching FullRemedy. Overshoot collapsed from 350%
to 0-9%.

### Lesson

When porting a continuous-time algorithm (per-share evaluation) to a
discrete-time framework (per-tick evaluation), the correct approach is to
simulate the original update cadence within each tick, not to apply a
time-domain correction factor. The `on_fire()` feedback mechanism of
the Estimator trait enables this: the estimator knows when retargets
happened and can simulate share arrivals between ticks accordingly.

## Parameter Sweep

With the per-share simulation working correctly, we swept the key axes:

| Variant | τ_short | τ_long | ft | Boundary | η | Key insight |
|---------|---------|--------|-----|----------|-----|-------------|
| CkpoolRemedy | 60 | 300 | 72 | PoissonCI | 0.2 | Zero overshoot, weak reaction |
| CkpoolRemedy-ft12 | 60 | 300 | 12 | PoissonCI | 0.2 | Better reaction, more overshoot |
| Ck-tl120-eta20 | 60 | 120 | 12 | PoissonCI | 0.2 | **Best balanced** |
| Ck-tl120-eta35 | 60 | 120 | 12 | PoissonCI | 0.35 | Accuracy/overshoot degrades |
| Ck-cusum-eta20 | 60 | 300 | 12 | AsymCUSUM | 0.2 | 96-98% reaction, high jitter |
| Ck-tl120-cusum-eta30 | 60 | 120 | 12 | AsymCUSUM | 0.3 | Reaction good, fitness poor |
| Ck-ts30-tl120-eta30 | 30 | 120 | 8 | PoissonCI | 0.3 | Shorter τ_short helps ramp |
| Ck-tl120-accel | 60 | 120 | 12 | PoissonCI | 0.2→0.6 | Overshoot too high from η ramp |

### Results at SPM 4-5 (where ckpool's ideas matter most)

| Variant | Reaction -50% | Accuracy p50 | Overshoot p99 | Jitter | Fitness |
|---------|:---:|:---:|:---:|:---:|:---:|
| FullRemedy | 70 / 82% | 7.6 / 7.4% | 7.7 / 7.1% | 0.042 / 0.032 | **0.691 / 0.732** |
| Ck-cusum-eta20 | **96 / 98%** | 9.3 / 9.1% | 24.5 / 15.1% | 0.158 / 0.140 | 0.612 / 0.644 |
| Ck-tl120-eta20 | 67 / 77% | 9.7 / 7.4% | 20.1 / 19.3% | 0.065 / 0.057 | 0.656 / 0.675 |
| CkpoolRemedy | 55 / 53% | 9.1 / 7.7% | **0 / 0%** | **0.009 / 0.031** | 0.679 / 0.682 |
| VardiffState | **99 / 99%** | 12.3 / 11.8% | 44.4 / 32.4% | 0.143 / 0.118 | 0.626 / 0.658 |
| AdaCUSUM η=0.2 | **99 / 99%** | 8.4 / 6.4% | 29.5 / 28.2% | 0.220 / 0.192 | 0.633 / 0.662 |

### Key Findings

1. **The boundary is the decisive axis.** CUSUM gives 96-99% reaction at
   the cost of 3-5× jitter. PoissonCI gives low jitter at the cost of
   67-77% reaction. No ckpool estimator tuning escapes this trade-off —
   it's the same fundamental boundary-axis trade-off that differentiates
   FullRemedy from AdaCUSUM.

2. **Shorter τ_long (120s) helps more than lower fast-threshold.** Once
   the long-window EMA is responsive enough (τ=120s), the dual-window
   switching becomes redundant — you're effectively running a single
   EWMA(120s) that happens to have slightly different decay dynamics.

3. **Higher η degrades everything except reaction rate.** η=0.35 improves
   reaction by ~3% but costs 7% accuracy and 15% overshoot. The
   AcceleratingPartialRetarget variant is even worse (91% overshoot) because
   it ramps η during cold-start when the estimator is already noisy.

4. **CkpoolRemedy (default ft=72) achieves zero overshoot** because the
   long-window EMA (τ=300s) dampens cold-start ramp so aggressively that
   the target never overshoots truth. But this same conservatism kills
   reaction rate (55% at SPM 4).

## What Transfers and What Doesn't

### Transfers (worth keeping)

- **Per-share `decay_time()` simulation** — the correct technique for
  porting continuous-time EMAs to tick-based evaluation. Available via
  `CkpoolEstimator` for future composition experiments.

- **Oscillation guard** — `CkpoolRetarget`'s suppression of difficulty
  decrease on insufficient data is a sound principle, though
  PartialRetarget's damping (η=0.2) already limits per-fire moves enough
  to make the guard redundant in practice.

### Does not transfer

- **Time-bias warmup correction** — calibrated for per-share evaluation
  intervals (~3.33s); catastrophic at 60s ticks. The per-share simulation
  makes it unnecessary.

- **Wide hysteresis band [0.5, 1.33]** — designed for a context where the
  estimator converges tightly before the gate opens. At 60s ticks, the
  estimator is noisier when evaluated, so the band must be narrower or
  replaced entirely with a statistical boundary.

- **Dual-window adaptive switching** — an elegant idea in ckpool's native
  context (short window for fast ramp-up, long window for stability), but
  once τ_long is shortened to match the tick cadence, the switching adds
  complexity without performance benefit. A single EWMA(120s) achieves the
  same balance.

- **Share-count data gate (72 shares)** — meaningless in the tick framework
  where evaluation happens on a fixed schedule regardless of share arrival.

## Hysteresis Boundary Sweep

The original investigation only tested ckpool's native hysteresis [0.5, 1.33]
and one narrowed variant [0.8, 1.2]. A proper parameter sweep was run across
band widths from [0.5, 1.33] (native) through [0.9, 1.1] (very narrow), with
data gates of 2/4/6 shares, paired with both PartialRetarget(0.2) and
AcceleratingPartialRetarget(0.2, 0.4, 0.2).

### Results (comprehensive fitness, SPM 4 / 10 / 20 / 30)

| Boundary | SPM 4 | SPM 10 | SPM 20 | SPM 30 |
|----------|:---:|:---:|:---:|:---:|
| AdaptiveBoundary-spm10 (production) | 0.689 | 0.774 | 0.882 | 0.869 |
| VardiffState (CUSUM) | 0.706 | 0.783 | 0.894 | 0.874 |
| Hyst [0.7, 1.3] gate4 | 0.621 | 0.695 | 0.666 | 0.608 |
| Hyst [0.8, 1.2] gate4 | 0.572 | 0.675 | 0.723 | 0.726 |
| Hyst [0.85, 1.15] gate4 | 0.578 | 0.634 | 0.706 | 0.730 |
| Hyst [0.9, 1.1] gate4 | 0.582 | 0.615 | 0.637 | 0.669 |
| Hyst [0.8, 1.2] + AccelRetarget | 0.487 | 0.664 | 0.740 | 0.758 |
| Hyst [0.5, 1.33] gate4 (native) | 0.630 | 0.542 | 0.508 | 0.504 |

### Key Finding: Hysteresis trades reaction for jitter

Narrowing the band improves reaction rate dramatically — `Hyst [0.8, 1.2]`
achieves 96–100% reaction across all SPMs (better than any statistical
boundary). But jitter explodes to 0.15–0.32 fires/min at mid-SPMs vs
0.03/min for PoissonCI. The fundamental issue: hysteresis fires whenever
the rate ratio crosses the band threshold, with no evidence accumulation.
Statistical boundaries (PoissonCI, CUSUM) distinguish real changes from
noise by requiring cumulative evidence before firing.

No hysteresis parameterization achieved competitive comprehensive fitness.
The best variant (`Hyst [0.7, 1.3]`) peaked at 0.706 at SPM 15 but
averaged 0.644 across the range vs 0.798 for the adaptive boundary.

## Estimator Equivalence Test (Revised)

The initial investigation claimed "estimation quality is equivalent"
between `CkpoolEstimator` (per-share `decay_time()` simulation) and
`EwmaEstimator(120s)`. This was tested under PoissonCI + PartialRetarget,
where the two estimators produce similar comprehensive fitness (~5% gap).

A follow-up test paired `CkpoolEstimator` with the production-tuned
boundary and update (`AdaptivePoissonCusum(10) + AcceleratingPartialRetarget
(0.2, 0.4, 0.2)`) to confirm equivalence under the more demanding
composition:

### Results: Not equivalent under CUSUM

| Composition | SPM 4 | SPM 8 | SPM 12 | SPM 20 | SPM 30 |
|-------------|:---:|:---:|:---:|:---:|:---:|
| EWMA(120s) + AdpBnd + AccelRet (production) | 0.689 | 0.768 | 0.787 | 0.882 | 0.869 |
| CkpoolEstimator(60,300) + same bnd/update | 0.698 | 0.638 | 0.696 | 0.777 | 0.858 |
| CkpoolEstimator(60,120) + same bnd/update | 0.598 | 0.688 | 0.764 | 0.805 | 0.858 |

The ckpool estimator underperforms EWMA(120s) significantly at mid-SPMs
(0.638–0.696 vs 0.768–0.787). Jitter is 2–3× higher (0.06–0.09/min vs
0.03/min), and reaction rate at low SPM drops to 48–57% (vs 73–94%).

### Root cause: per-share simulation is noisier per tick

The per-share `decay_time()` simulation runs N separate EMA decay steps
per tick (one per simulated share arrival). Each step introduces rounding
and the uniform inter-share spacing assumption adds variance. A single
batch EWMA update (`α × old + (1-α) × new`) produces a cleaner signal
per tick because it avoids the N-step accumulation error.

Under PoissonCI (which requires large deviations to fire), both estimators
are "good enough" — the boundary's conservatism masks the noise difference.
Under CUSUM (which fires on smaller accumulated deviations), the ckpool
estimator's per-tick noise triggers more false fires → higher jitter →
lower fitness.

### Revised conclusion on evaluation cadence

The original claim "the evaluation cadence is just scheduling, not
information" was too strong. While the *information content* of N shares
in T seconds is identical regardless of evaluation cadence, the
*numerical stability* of the estimate depends on how that information is
processed. A single batch update is numerically cleaner than N simulated
updates, and this difference is operationally significant under aggressive
boundaries.

## Conclusion

ckpool's vardiff is well-optimized for its native per-share evaluation
context. When ported to the tick-based SV2 framework:

1. **Estimator**: The per-share `decay_time()` simulation produces noisier
   estimates than a batch EWMA(120s) under aggressive boundaries. The
   simpler `EwmaEstimator(120s)` is preferred for production.

2. **Boundary**: Hysteresis [0.5, 1.33] does not transfer — too wide for
   60s ticks. Narrower bands trade reaction for jitter without achieving
   competitive fitness. Statistical boundaries (PoissonCI, CUSUM) dominate.

3. **Update rule**: ckpool's full retarget is equivalent to η=1.0 in the
   partial retarget framework. The damped AcceleratingPartialRetarget
   (η capped at 0.4) outperforms full retarget on overshoot and jitter.

The production recommendation:

```rust
Composed::new(
    EwmaEstimator::new(120),
    AdaptivePoissonCusum::new(10),
    AcceleratingPartialRetarget::new(0.2, 0.4, 0.2),
    min_allowed_hashrate,
    clock,
)
```

The `CkpoolEstimator` is preserved in the codebase for reference and as
a benchmark contender in the simulation grid.

### Gate-test addendum — `EwmaEstimator::new(120)` is decline-gate-UNSAFE (do not ship as-is)

> **The recommendation above was selected by SCALAR FITNESS on a −50% drop.** That
> is the *easy, non-binding* decline direction (information-floor §9.1 — every config
> catches a large fast drop). The binding test is the **slow/moderate decline gate**
> (the 1–40 %/hr sweep, `slow-decline.rs`) that selected the `Ewma360` champion, and
> it was never run here. Running EWMA(120) through that gate (pre-registered, three
> configs at τ=120; see `slow-decline.rs` header + plan) is decisive:

- **`EWMA(120)` fails the gate: worst settled +5.9% at the sub-guard 2-spm cell, over
  the +5% runaway line** — vs the champion `Ewma360` at **+2.7%** (passes) and
  `Ewma240` at **+3.5%** (passes). The gradient 360→240→120 = +2.7→+3.5→+5.9 is the
  §8.3 τ-valley's left (twitchy) flank; the gate crossing is **between 240 and 120**.
  The scalar-fitness selection rewarded 120's responsiveness while tolerating the
  over-difficulty the gate refuses — the gate-vs-fitness gap, dimensioned.
- **The failure is the WINDOW, not the boundary.** At the sub-guard cells all configs
  run the *identical* PoissonCI guard; EWMA(120) fails there (+5.9%) whether paired
  with the champion's `AdaptiveSignPersist` OR ckpool's `AdaptivePoissonCusum(10)` —
  bit-identical at sub-guard. So swapping the boundary (this doc's other rec) does
  **not** rescue it; the validity problem is the 120 window at sparse rate, full stop.
- **The update-rule difference is gate-immaterial.** ckpool's
  `AccelRetarget(0.2,0.4,0.2)` vs the champion's `(0.2,0.6,0.05)`, boundary held,
  moves settled-e by < 1pp on the decline (both guard and aggressive regimes). So the
  rec's update rule is fine; the estimator window is the problem.
- **Caveat this addendum, honestly:** the run also NARROWED a claim in *our own* doc —
  `information-floor.md` §8.3's sensitivity-invariance ("worst-settled is an estimator-window
  property, ~boundary-independent") was measured only across SignPersistenceCusum
  *sensitivities*; this run shows it does **not** extrapolate across boundary *type*
  (at spm6–8, the same τ=120 sits at −13% under SignPersist vs +1.8% under
  PoissonCusum — a ~15pp boundary-type effect). So "120 is left of the valley → worse
  under *any* boundary" was an over-extrapolation; 120's gate-failure is specifically
  a sub-guard-regime + short-window effect, not a universal-boundary one. Recorded as
  a scope-correction to §8.3 (see `information-floor.md` §8.3), not buried.

**Net: keep this doc's architecture finding (rate-fixed estimator + rate-adaptive
boundary) and the update-rule and hysteresis verdicts — but the specific
`EwmaEstimator::new(120)` value is gate-unsafe at the sub-guard cells; ship `Ewma360`
(or, if a shorter window is wanted, gate-test it — the crossing is between 240 and
120, so 240 is the shortest gate-passing window measured).** The scalar-fitness
ranking here is correct on the axis it tested (−50% drop responsiveness); the gate is
the axis it didn't, and decline-safety is a constraint, not a fitness term (§9.3).

## Files Added

### Production components (`src/vardiff/composed/`)

- `estimator.rs` — `CkpoolEstimator` (per-share `decay_time()` simulation),
  `TimeBiasEwmaEstimator` (single-window EWMA with time-bias correction)
- `boundary.rs` — `HysteresisGate` (ckpool's binary fire/no-fire gate)
- `update.rs` — `CkpoolRetarget` (full retarget with oscillation guard)

### Grid registrations (`sim/src/grid.rs`)

- `AlgorithmSpec::ckpool()` — native ckpool composition
- `AlgorithmSpec::ckpool_remedy()` — ckpool estimator + PoissonCI + PartialRetarget
- `AlgorithmSpec::ckpool_remedy_ft(n)` — with custom fast-threshold
- `AlgorithmSpec::ckpool_narrow_hyst()` — tightened hysteresis variant
- `AlgorithmSpec::ckpool_with(τs, τl, lo, hi)` — parametric exploration
- `AlgorithmSpec::time_bias_remedy()` — isolated time-bias test
