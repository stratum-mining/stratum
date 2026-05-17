# FullRemedy η sweep (1000 trials/cell, base_seed = 0xdeadbeefcafef00d)

Pareto-explore the PartialRetarget η parameter on the FullRemedy family. Holds the other two FullRemedy axes fixed (`EwmaEstimator(τ = 120s)`, `PoissonCI(z = 2.576, margin = 0.05)`) and varies only η. Smaller η means each fire moves a smaller fraction of the gap from `current_h` to the estimator's belief — tighter overshoot bounds at the cost of slower reaction to large step changes.

## Decoupling score (higher = better)

`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, J_max = 0.50 fires/min. 1.0 = perfect; > 0.8 = strong; < 0.3 = poor.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.884 | 0.877 | 0.796 | 0.772 | 0.754 | 0.745 |
| 12 | 0.974 | 0.974 | 0.970 | 0.868 | 0.835 | 0.803 |
| 30 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.993 |
| 60 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| 120 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |

## Jitter mean under stable load (lower = better)

Fires per minute, post-convergence. Smaller = less active stable-load tracking. FullRemedy by design fires under stable load (the cost of active tracking) so absolute jitter scales with η as well as with the boundary.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.029 | 0.034 | 0.036 | 0.046 | 0.055 | 0.070 |
| 12 | 0.026 | 0.028 | 0.031 | 0.042 | 0.050 | 0.062 |
| 30 | 0.011 | 0.016 | 0.019 | 0.026 | 0.031 | 0.037 |
| 60 | 0.004 | 0.006 | 0.008 | 0.014 | 0.017 | 0.019 |
| 120 | 0.001 | 0.001 | 0.002 | 0.004 | 0.006 | 0.005 |

## Reaction rate at −50% step (higher = better)

Fraction of trials that fire within 5 min of a 50% drop in true hashrate. Smaller η damps the per-fire move so multiple fires may be needed to traverse the post-step gap — this table captures whether smaller η costs reactive sensitivity.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.88 | 0.88 | 0.88 | 0.87 | 0.86 | 0.87 |
| 12 | 0.97 | 0.97 | 0.97 | 0.96 | 0.95 | 0.92 |
| 30 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 0.99 |
| 60 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 120 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |

## Reaction rate at −10% step (higher = better)

Small-step sensitivity: a 10% drop is closer to Poisson noise and harder to distinguish from chance. Smaller η means each fire moves less, requiring more fires to traverse the gap — which costs sensitivity on shallow steps.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.32 | 0.33 | 0.35 | 0.37 | 0.40 | 0.44 |
| 12 | 0.35 | 0.36 | 0.38 | 0.41 | 0.43 | 0.47 |
| 30 | 0.37 | 0.38 | 0.39 | 0.40 | 0.42 | 0.45 |
| 60 | 0.45 | 0.45 | 0.45 | 0.46 | 0.47 | 0.51 |
| 120 | 0.57 | 0.57 | 0.57 | 0.58 | 0.58 | 0.59 |

## Settled accuracy p50 under stable load (lower = better)

`|final_hashrate / true_hashrate − 1|` at trial end. Smaller = closer to truth.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 3.5% | 6.5% | 9.2% | 13.7% | 16.7% | 21.9% |
| 12 | 2.5% | 4.8% | 6.9% | 10.3% | 11.5% | 15.1% |
| 30 | 1.7% | 3.3% | 4.7% | 6.3% | 6.0% | 8.2% |
| 60 | 1.2% | 2.3% | 2.1% | 1.2% | 1.7% | 2.8% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

## Ramp target overshoot p50 — cold start (lower = better)

`max(new_hashrate over fires) / H_true − 1`. Smaller η directly caps the per-fire jump; this is the metric η was introduced to bound.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.0% | 0.0% | 0.0% | 7.8% | 13.7% | 25.7% |
| 12 | 0.0% | 0.0% | 0.0% | 4.2% | 9.7% | 25.0% |
| 30 | 0.0% | 0.0% | 0.0% | 1.3% | 40.0% | 100.0% |
| 60 | 0.0% | 0.0% | 0.0% | 0.0% | 2.9% | 0.1% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 1.3% | 0.0% |

## Ramp target overshoot p90 — cold start (lower = better)

Tail of the same distribution. Captures unlucky-Poisson ramp trajectories. The η-sensitivity headline metric.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.0% | 0.0% | 10.4% | 27.7% | 40.7% | 57.3% |
| 12 | 0.0% | 0.0% | 5.7% | 16.3% | 24.2% | 36.4% |
| 30 | 0.0% | 0.0% | 1.9% | 10.5% | 40.0% | 100.0% |
| 60 | 0.0% | 0.0% | 0.1% | 5.1% | 10.0% | 16.1% |
| 120 | 0.0% | 0.0% | 0.0% | 3.1% | 6.4% | 11.3% |

## Ramp target overshoot p99 — cold start (lower = better)

Worst-trial tail. This is the metric that the original axis analysis identified PartialRetarget as the closure for.

| SPM | η=0.10 | η=0.20 | η=0.30 | η=0.50 | η=0.70 | η=1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.0% | 10.0% | 26.9% | 50.5% | 70.6% | 95.9% |
| 12 | 0.0% | 5.9% | 15.3% | 31.3% | 48.8% | 66.5% |
| 30 | 0.0% | 0.7% | 7.4% | 16.7% | 40.0% | 100.0% |
| 60 | 0.0% | 0.0% | 5.6% | 11.1% | 16.9% | 25.5% |
| 120 | 0.0% | 0.0% | 2.7% | 7.1% | 11.6% | 16.7% |

