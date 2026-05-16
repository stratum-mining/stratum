# EWMA τ sweep (1000 trials/cell, base_seed = 0xdeadbeefcafef00d)

Pareto-explore the EWMA time constant τ. Lower τ → more responsive but more jittery; higher τ → smoother but slower to react. The right τ is the one that maximizes decoupling score without inflating settled accuracy beyond your tolerance.

## Decoupling score (higher = better)

`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, J_max = 0.50 fires/min. 1.0 = perfect; > 0.8 = strong; < 0.3 = poor.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.665 | 0.710 | 0.772 | 0.657 | 0.201 |
| 12 | 0.736 | 0.811 | 0.868 | 0.913 | 0.437 |
| 30 | 0.765 | 0.846 | 1.000 | 0.996 | 0.759 |
| 60 | 0.789 | 0.867 | 1.000 | 1.000 | 0.973 |
| 120 | 0.833 | 0.895 | 1.000 | 1.000 | 0.999 |

## Jitter p50 under stable load (lower = better)

Fires per minute, post-convergence. Smaller is better; 0 = no fires under stable load.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.130 | 0.091 | 0.056 | 0.000 | 0.000 |
| 12 | 0.125 | 0.083 | 0.050 | 0.000 | 0.000 |
| 30 | 0.118 | 0.077 | 0.000 | 0.000 | 0.000 |
| 60 | 0.105 | 0.067 | 0.000 | 0.000 | 0.000 |
| 120 | 0.083 | 0.053 | 0.000 | 0.000 | 0.000 |

## Reaction rate at −50% step (higher = better)

Fraction of trials that fire within 5 min of a 50% drop in true hashrate.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.90 | 0.87 | 0.87 | 0.66 | 0.20 |
| 12 | 0.98 | 0.97 | 0.96 | 0.91 | 0.44 |
| 30 | 1.00 | 1.00 | 1.00 | 1.00 | 0.76 |
| 60 | 1.00 | 1.00 | 1.00 | 1.00 | 0.97 |
| 120 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |

## Settled accuracy p50 (lower = better)

`|final_hashrate / true_hashrate − 1|` at trial end. Smaller = closer to truth.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 16.4% | 12.9% | 13.7% | 0.0% | 0.0% |
| 12 | 10.8% | 8.7% | 10.3% | 0.0% | 0.0% |
| 30 | 6.8% | 5.5% | 6.3% | 0.0% | 0.0% |
| 60 | 4.8% | 4.2% | 1.2% | 0.0% | 0.0% |
| 120 | 3.6% | 2.9% | 0.0% | 0.0% | 0.0% |

## Estimator variance p50 under stable load (lower = better)

Population variance of `H̃ / H_true` over post-settle ticks. Indicates how noisy the estimator's belief is.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.1208 | 0.0736 | 0.0373 | 0.0091 | 0.0032 |
| 12 | 0.0594 | 0.0355 | 0.0174 | 0.0044 | 0.0016 |
| 30 | 0.0239 | 0.0142 | 0.0066 | 0.0017 | 0.0008 |
| 60 | 0.0118 | 0.0071 | 0.0032 | 0.0009 | 0.0004 |
| 120 | 0.0058 | 0.0034 | 0.0015 | 0.0004 | 0.0002 |

## Ramp target overshoot p50 — cold start (lower = better)

`max(new_hashrate over fires) / H_true − 1`. Independent of the estimator-noise tails that affect `Phase1Overshoot`. Larger τ → more smoothing → larger overshoot when the EWMA finally catches up; this table quantifies that trade-off.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 29.6% | 19.6% | 7.8% | 0.0% | 0.0% |
| 12 | 19.4% | 12.5% | 4.2% | 0.0% | 0.0% |
| 30 | 12.0% | 8.4% | 1.3% | 0.0% | 0.0% |
| 60 | 7.0% | 3.8% | 0.0% | 0.0% | 0.0% |
| 120 | 4.5% | 2.1% | 0.0% | 0.0% | 0.0% |

## Ramp target overshoot p90 — cold start (lower = better)

Tail of the same distribution. Captures unlucky-Poisson ramp trajectories.

| SPM | τ=30s | τ=60s | τ=120s | τ=300s | τ=600s |
| --- | --- | --- | --- | --- | --- |
| 6 | 58.3% | 43.8% | 27.7% | 15.8% | 16.5% |
| 12 | 36.8% | 26.5% | 16.3% | 9.9% | 10.6% |
| 30 | 22.0% | 16.4% | 10.5% | 3.4% | 3.2% |
| 60 | 14.0% | 9.9% | 5.1% | 2.5% | 2.3% |
| 120 | 9.5% | 6.4% | 3.1% | 0.7% | 0.8% |

