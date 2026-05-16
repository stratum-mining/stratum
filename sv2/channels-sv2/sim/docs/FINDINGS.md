# Vardiff characterization findings

What the framework has measured about the algorithm registry. The
architectural reference for terms used here (Estimator, Statistic,
Boundary, UpdateRule, the algorithm compositions) is
[`DESIGN.md`](./DESIGN.md).

## 1. The cross-algorithm Pareto

Every shipped algorithm characterized across the canonical 5 × 10
grid (1000 trials per cell, `base_seed = 0xDEADBEEFCAFEF00D`), reading
the cross-cutting decoupling score (higher is better; `1.0` is
perfect):

| SPM | VardiffState | Parametric | EWMA-60s | **FullRemedy** |
|-----|--------------|------------|----------|----------------|
| 6   | 0.696        | 0.030      | 0.711    | **0.786**      |
| 12  | 0.549        | 0.080      | 0.803    | **0.986**      |
| 30  | 0.336        | 0.230      | 0.845    | **1.000**      |
| 60  | 0.188        | 0.440      | 0.867    | **1.000**      |
| 120 | 0.098        | 0.810      | 0.889    | **1.000**      |

Reading down each column reveals the shape of each algorithm:

- **`VardiffState`** is monotonically decreasing in SPM. Its
  share-rate-blind threshold ladder fires at the wrong rate as SPM
  rises: at SPM=120, reaction rate to a −50% step is 9.8% (the
  algorithm is structurally blind to a halving of the miner's
  hashrate 91% of the time). The 15% floor that's correctly
  conservative against Poisson noise at SPM=6 becomes a 91%
  missed-detection rate at SPM=120.
- **`Parametric`** is monotonically increasing. Switching to a
  rate-aware PoissonCI boundary fixes the high-SPM blindness but
  over-corrects at low SPM: the threshold becomes wide enough that
  small step changes don't fire.
- **`EWMA-60s`** is flat-high but pays a settled-accuracy and
  jitter cost.
- **`FullRemedy`** is flat-high *and* dominates `EWMA-60s` at every
  SPM. It is the only algorithm in the registry that doesn't collapse
  in either direction.

The reaction-rate-at-−50%-step table tells the same story directly:

| SPM | VardiffState | Parametric | EWMA-60s | **FullRemedy** |
|-----|--------------|------------|----------|----------------|
| 6   | 70%          | 3%         | 87%      | **87%**        |
| 12  | 55%          | 8%         | 97%      | **99%**        |
| 30  | 33%          | 23%        | 100%     | **100%**       |
| 60  | 16%          | 44%        | 100%     | **100%**       |
| 120 | 9%           | 81%        | 100%     | **100%**       |

## 2. The SPM=6 cold-start cascade

The hardest individual cell is cold-start at SPM=6. At low share
rates the Poisson noise on a single 60-second window has 41%
relative standard deviation (`√6/6`), and a fresh cold start ramps
through orders of magnitude before settling — so the algorithm is
combining a noisy estimator, a wide-bouncing threshold, and an
unconstrained ramp trajectory all at once.

The `ramp_target_overshoot` metric (peak new-fire-target over
trial-end truth) reveals what the cell produces in its worst-case
trials. Running `trace-trial --scan-overshoot 100 --spm 6` against
`VardiffState` surfaces seed `0xcb3b`, which produces a 187% peak.
Tick by tick:

| Tick | t (s) | shares | current_h (before) | h_estimate | fired | new_h |
|------|-------|--------|-------------------|------------|-------|-------|
| 1–9  | 60–540 | (ramp) | 1e10 → 6.6e13 | — | YES (×3 each) | (Classic Phase 1 multiplicative ramp) |
| 10 | 600 | 35 | 1.97e14 | (lands near truth) | YES | 1.15e15 |
| 11 | 660 | 15 | 1.15e15 | 2.87e15 | **YES** | **2.87e15** |

At tick 10 the Classic Phase 1 ramp correctly lands `current_h ≈
1.15e15` (just above truth) on a 35-share window. At tick 11, with
`current_h = 1.15e15`, the expected shares at SPM=6 are `6 × (1e15 /
1.15e15) ≈ 5.2`. The observed 15 is a Poisson(5.2) upper-tail event
(~0.1% per minute) — but with 1800s of trial time and 100 seeds,
hitting it once is reliable. The CumulativeCounter estimator
computes `h_estimate = (15/6) × 1.15e15 = 2.87e15`, a 188%
deviation. The Classic step threshold at `dt=60s` is 100%; 188%
crosses; the algorithm fires; the new target is `2.87e15` — **187%
above truth**.

The same mechanism accounts for `VardiffState`'s 17%
convergence-failure rate at SPM=6 in the canonical baseline: trials
that hit this cascade spend the rest of the 30-minute window
oscillating between extreme over- and under-targets.

## 3. No single-axis change closes the cascade

Running the same `--scan-overshoot 100 --seed 0xCAFE --spm 6` against
each candidate single-axis remedy:

| Algorithm | Worst-case peak | Tick | Worst-trial settled accuracy |
|-----------|----------------:|:----:|------------------------------:|
| `VardiffState` | 187.0% | 11 | 2.3% |
| `Parametric` (z = 2.576) | 187.0% | 11 | 7.6% |
| `ParametricStrict` (z = 3.0) | 187.0% | 11 | 11.6% |
| `ClassicPartialRetarget(η=0.3)` | 47.0% | 26 | **32.3%** |
| `EWMA-60s` | 56.1% | 23 | **56.1%** |
| `SlidingWindow-10t` | 70.0% | 15 | 15.0% |
| **`FullRemedy`** | **23.4%** | 20 | **2.8%** |

**Tightening the boundary alone doesn't help.** The PoissonCI
threshold at `λ̄ = 5.2` with `z = 2.576` is
`(2.576·√5.2 + 0.5)/5.2 × 100 + 5 = 128%` — permeable to the 188%
deviation. Raising z to 3.0 lifts the threshold to 146%, still
permeable. The threshold formula has a structural ceiling at low
`λ̄` that no z-tightening can close without producing massive Type
II error elsewhere. Both Parametric variants fire on the same seed
`0xcb3b` at the same tick with the same 2.87e15 target.

**Damping the update alone doesn't help.** `ClassicPartialRetarget`
drops the worst peak to 47%, but the absence of Classic's Phase 1
multiplicative clamps creates a different failure mode: at low SPM
the η=0.3 damping is too weak for the algorithm to recover from any
single bad Poisson minute. Settled accuracy on the worst trial is
32%.

**Smoothing the estimator alone doesn't help.** `EWMA-60s` absorbs
the Phase-1-end Poisson spike, dropping the worst peak to 56% (and
at a *later* tick, 23, because the EWMA's memory carries the
disturbance forward). But settled accuracy on the worst trial is
56% — the EWMA's lag at SPM=6 means slow late convergence within the
30-minute trial.

Each axis change closes a different failure mode; none closes both
the Phase-1 cascade and low-SPM settled accuracy together.

## 4. The composition closes it

`FullRemedy = Composed<EwmaEstimator(120s), AbsoluteRatio,
PoissonCI::default_parametric(), PartialRetarget(0.3)>` drops the
worst-case at SPM=6 to 23% with 2.8% settled accuracy. Three axis
changes from the classic algorithm.

Each axis contributes a distinct closure:

| Axis | Contribution |
|------|-------------|
| `EwmaEstimator(120s)` | Spreads the Poisson(5.2)→15 minute across ~120 seconds of memory. Tick-11 estimate becomes `≈ 0.39·15 + 0.61·prior_mean ≈ 9`, a 32% deviation that doesn't cross the boundary. |
| `PoissonCI(z=2.576)` | Keeps stable-load false fires near zero (decoupling score 1.0 from SPM=12 upward). |
| `PartialRetarget(0.3)` | Defense-in-depth — caps the magnitude of any fire that does happen at 30% of the current-to-estimate gap. |

`FullRemedy` dominates the registry on every operationally
meaningful metric. The full per-cell comparison is in
[`baseline_FullRemedy.md`](../baseline_FullRemedy.md); the headline
deltas vs `VardiffState`:

| Metric | VardiffState | FullRemedy | At SPM |
|--------|--------------|------------|--------|
| Cold-start convergence rate | 83.2% | **100%** | 6 |
| Cold-start convergence p50 | 12m | **6m** | 6 |
| Reaction rate to −50% step | 9.8% | **100%** | 120 |
| Reaction sensitivity at ±10% | 0.00 / 0.03 | **0.59 / 0.61** | 120 |
| Ramp target overshoot p99 | 145% | **31%** | 6 |
| Decoupling score | 0.10 | **1.00** | 120 |

## 5. The trade-offs FullRemedy makes

Two trade-offs are deliberate and well-bounded.

**Active stable-load tracking.** Under stable load at SPM=6,
`FullRemedy` fires ~2.7 times per hour (mean 0.045/min); at SPM=120
it's near zero. `VardiffState` and `Parametric` fire roughly never
under stable load — because they're not actively tracking, just
coasting. The cost of FullRemedy's active tracking is what pays for
its ability to detect a ±10% step at SPM=120 (sensitivity 0.59/0.61
vs VardiffState's 0.00/0.03). Operationally, a few defensible
retargets per hour are worth substantially better load tracking.

**Mild negative cold-start bias.** The EWMA(120s) estimate lags
during the cold-start ramp: mean cold-start bias is −1.9% at SPM=6
decreasing to −0.6% at SPM=120. This is harmless or beneficial:
under-estimating during ramp keeps the target slightly easier than
truth, which accelerates share arrival and therefore accelerates the
estimator's own convergence. It disappears once the algorithm hits
stable load.

The settled-accuracy comparison needs careful reading. `VardiffState`
shows 0.0% p50 settled accuracy at SPM=120 because it almost never
fires — `current_h` stays near its initialized value. `FullRemedy`
shows a wider median band (0.0–9.3% p50 across SPM) because it
actively tracks; but its `p99` is much tighter at low SPM (28% vs
`VardiffState`'s 70% at SPM=6). The choice is between "narrow
distribution but heavy unreliability tail" and "wider distribution
but bounded tail." Bounded tails are worth more operationally.

## 6. The EWMA τ Pareto

Within the EWMA family, the time constant τ exposes a
responsiveness/stability trade-off worth characterizing in its own
right. The full sweep is in
[`../ewma_tau_sweep.md`](../ewma_tau_sweep.md); the key tables:

**Decoupling score** (higher better):

| SPM | τ=30 | τ=60 | τ=120 | τ=300 | τ=600 |
|-----|------|------|-------|-------|-------|
| 6   | 0.66 | 0.71 | **0.77** | 0.66 | 0.20 |
| 12  | 0.74 | 0.81 | 0.86  | **0.91** | 0.43 |
| 30  | 0.77 | 0.85 | **1.00** | 1.00 | 0.76 |
| 60  | 0.79 | 0.87 | **1.00** | 1.00 | 0.97 |
| 120 | 0.82 | 0.89 | **1.00** | 1.00 | 1.00 |

**Ramp target overshoot p90 — cold start** (lower better):

| SPM | τ=30 | τ=60 | τ=120 | τ=300 | τ=600 |
|-----|------|------|-------|-------|-------|
| 6   | 58.3% | 43.8% | **27.7%** | 15.8% | 16.5% |
| 12  | 36.8% | 26.5% | 16.3% | 9.9% | 10.6% |
| 30  | 22.0% | 16.4% | 10.5% | 3.4% | 3.2% |
| 60  | 14.0% | 9.9%  | 5.1%  | 2.5% | 2.3% |
| 120 | 9.5%  | 6.4%  | 3.1%  | 0.7% | 0.8% |

The Pareto-optimal τ depends on the operational SPM range floor:

- **τ = 120s** wins the general case. Decoupling saturates at 1.0
  from SPM=30 upward; reaction rate at SPM=6 is 87%; ramp overshoot
  p90 is 27.7% at SPM=6. This is the default in `FullRemedy`.
- **τ = 300s** has slightly worse decoupling at SPM=6 (0.66 vs
  τ=120's 0.77) but perfect settled accuracy at every SPM and a
  tighter ramp overshoot tail at low SPM. Use this if the
  operational SPM range is bounded above 12.
- **τ = 600s** is too slow at SPM=6 (decoupling 0.20). Useful only
  if the operational SPM range is firmly ≥ 60. The non-monotone
  ramp p90 at τ=600 vs τ=300 (16.5% vs 15.8% at SPM=6) is a
  "you've over-smoothed past the trial duration" signal.

## 7. Asymmetric step response under VardiffState

At SPM=120 with a −50% step at 15 minutes, `VardiffState`'s reaction
rate is 9.8%; with a +50% step it's 14%. The asymmetry is structural
and explained by Poisson relative noise.

After a −50% step the expected per-minute share count is
`120 × 0.5 = 60`, with `√60/60 ≈ 13%` relative stddev. After a +50%
step it's `120 × 1.5 = 180`, with `√180/180 ≈ 7.5%` — almost 1.7×
less relative noise. The classic threshold ladder, which is
share-rate-blind, fires later (or not at all) on the noisier down-
side because Poisson variance routinely produces minutes that look
like a partial recovery.

`Parametric` reduces but doesn't eliminate the asymmetry (0.81 vs
0.95 at SPM=120 — the rate-aware threshold adapts to the lower
expected count post-step). `EWMA-60s` and `FullRemedy` eliminate it
entirely (both close to 1.0 / 1.0 at SPM=120) because the smoothed
estimator's per-tick variance is smaller than the threshold even on
the noisier side.

## 8. U256 precision fix in `channels_sv2::target`

While tracing a 45.8% peak excursion at SPM=30 cold-start, the
mechanism that surfaced was not algorithm behavior but an integer
truncation in the production hashrate-to-target conversion path
(`channels_sv2/src/target.rs`). The intermediate computation
`60 / share_per_min × 100` truncated to integer before being scaled
through U256, losing ~5 digits of precision at SPM=30. The fix
(scale factor `100 → 100_000` at line 184 and the matching
`from(100) → from(100_000)` at line 197) brings the round-trip
inflation under 2%, asserted by the
`hash_rate_round_trip_is_precise_after_u256_fix` regression test in
this crate. The fix is included in this change set.

The cold-start cell at SPM=30 no longer shows the 49% inflation in
the algorithm's peak estimator excursion; the framework now sees the
real estimator-noise behavior. `RampTargetOvershoot` (which uses the
actual fire target rather than the estimator's belief) is the
behavioral overshoot metric used by every analysis in this document,
because it's independent of estimator-tail noise.

## 9. The recommendation

Use `FullRemedy` in production. It dominates the registry on every
operationally meaningful metric, with two well-bounded trade-offs
that are operationally desirable rather than concerning. The
production migration is mechanical — see [`DESIGN.md`](./DESIGN.md)
§ "Production migration" for the four-step plan.
