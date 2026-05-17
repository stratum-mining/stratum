# FullRemedy z sweep (1000 trials/cell, base_seed = 0xdeadbeefcafef00d)

Pareto-explore the PoissonCI z parameter on the FullRemedy family. Holds the other two FullRemedy axes fixed (`EwmaEstimator(τ = 120s)`, `PartialRetarget(η = 0.2)`) and varies only z. Higher z widens the threshold, suppressing false fires under stable load at the cost of slower reaction on small steps.

## Decoupling score (higher = better)

`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, J_max = 0.50 fires/min. 1.0 = perfect; > 0.8 = strong; < 0.3 = poor.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.815 | 0.800 | 0.877 | 0.844 | 0.808 |
| 12 | 0.878 | 0.895 | 0.974 | 0.967 | 0.959 |
| 30 | 0.909 | 1.000 | 1.000 | 1.000 | 1.000 |
| 60 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| 120 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |

## Jitter mean under stable load (lower = better)

Fires per minute, post-convergence. Higher z directly raises the threshold floor, suppressing stable-load fires — this is the metric z was introduced to bound.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.064 | 0.046 | 0.034 | 0.016 | 0.005 |
| 12 | 0.058 | 0.040 | 0.028 | 0.011 | 0.003 |
| 30 | 0.042 | 0.024 | 0.016 | 0.005 | 0.001 |
| 60 | 0.023 | 0.011 | 0.006 | 0.002 | 0.001 |
| 120 | 0.007 | 0.002 | 0.001 | 0.001 | 0.000 |

## Reaction rate at −50% step (higher = better)

Fraction of trials that fire within 5 min of a 50% drop in true hashrate. Higher z lifts the threshold; if it lifts past the post-step δ at low SPM, reaction rate collapses.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.93 | 0.89 | 0.88 | 0.84 | 0.81 |
| 12 | 0.99 | 0.99 | 0.97 | 0.97 | 0.96 |
| 30 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 60 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 120 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |

## Reaction rate at −10% step (higher = better)

Small-step sensitivity. A 10% drop is the hardest signal to distinguish from Poisson noise; z controls how readily the algorithm fires on it.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 0.43 | 0.36 | 0.33 | 0.27 | 0.20 |
| 12 | 0.45 | 0.40 | 0.36 | 0.28 | 0.20 |
| 30 | 0.51 | 0.43 | 0.38 | 0.30 | 0.23 |
| 60 | 0.54 | 0.49 | 0.45 | 0.38 | 0.30 |
| 120 | 0.68 | 0.62 | 0.57 | 0.48 | 0.39 |

## Ramp target overshoot p90 — cold start (lower = better)

Higher z means the algorithm fires less readily on the Phase-1-end Poisson spike; this trades against responsiveness on slow ramps.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 6.8% | 3.0% | 0.0% | 0.0% | 0.0% |
| 12 | 3.0% | 0.0% | 0.0% | 0.0% | 0.0% |
| 30 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |
| 60 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

## Ramp target overshoot p99 — cold start (lower = better)

Worst-trial tail of the ramp overshoot distribution.

| SPM | z=1.960 | z=2.326 | z=2.576 | z=3.000 | z=3.500 |
| --- | --- | --- | --- | --- | --- |
| 6 | 16.2% | 12.1% | 10.0% | 7.0% | 3.0% |
| 12 | 12.7% | 7.6% | 5.9% | 2.6% | 0.0% |
| 30 | 4.8% | 3.2% | 0.7% | 0.1% | 0.0% |
| 60 | 2.3% | 0.3% | 0.0% | 0.0% | 0.0% |
| 120 | 0.3% | 0.0% | 0.0% | 0.0% | 0.0% |

