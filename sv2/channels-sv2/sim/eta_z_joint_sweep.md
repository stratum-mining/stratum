# FullRemedy joint (η, z) sweep (1000 trials/cell, base_seed = 0xdeadbeefcafef00d)

Joint Pareto exploration of the PartialRetarget η and PoissonCI z axes on the FullRemedy family. Holds `EwmaEstimator(τ = 120s)` fixed and varies (η, z) over a 3 × 3 grid. Each row in the per-metric tables is a share rate; each column is one (η, z) point. The single-axis sweeps (`eta_sweep.md`, `z_sweep.md`) characterize the marginal effects; this report surfaces any cross-axis coupling — specifically, whether the joint optimum differs from the per-axis marginal optima.

## Decoupling score

`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, J_max = 0.50 fires/min.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.884 | 0.832 | 0.801 | 0.877 | 0.844 | 0.808 | 0.796 | 0.843 | 0.815 |
| 12 | 0.974 | 0.969 | 0.960 | 0.974 | 0.967 | 0.959 | 0.970 | 0.965 | 0.961 |
| 30 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| 60 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| 120 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |

## Jitter mean (stable load)

Fires per minute, post-convergence.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.029 | 0.012 | 0.003 | 0.034 | 0.016 | 0.005 | 0.036 | 0.020 | 0.006 |
| 12 | 0.026 | 0.010 | 0.003 | 0.028 | 0.011 | 0.003 | 0.031 | 0.014 | 0.005 |
| 30 | 0.011 | 0.004 | 0.001 | 0.016 | 0.005 | 0.001 | 0.019 | 0.009 | 0.003 |
| 60 | 0.004 | 0.001 | 0.001 | 0.006 | 0.002 | 0.001 | 0.008 | 0.003 | 0.001 |
| 120 | 0.001 | 0.000 | 0.000 | 0.001 | 0.001 | 0.000 | 0.002 | 0.001 | 0.000 |

## Reaction rate at −50% step

Fraction of trials that fire within 5 min of a 50% drop in true hashrate.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.88 | 0.83 | 0.80 | 0.88 | 0.84 | 0.81 | 0.88 | 0.84 | 0.81 |
| 12 | 0.97 | 0.97 | 0.96 | 0.97 | 0.97 | 0.96 | 0.97 | 0.96 | 0.96 |
| 30 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 60 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 120 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |

## Reaction rate at −10% step

Small-step sensitivity: a 10% drop is closer to Poisson noise.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.32 | 0.27 | 0.20 | 0.33 | 0.27 | 0.20 | 0.35 | 0.27 | 0.20 |
| 12 | 0.35 | 0.27 | 0.20 | 0.36 | 0.28 | 0.20 | 0.38 | 0.28 | 0.20 |
| 30 | 0.37 | 0.30 | 0.23 | 0.38 | 0.30 | 0.23 | 0.39 | 0.31 | 0.23 |
| 60 | 0.45 | 0.38 | 0.30 | 0.45 | 0.38 | 0.30 | 0.45 | 0.38 | 0.30 |
| 120 | 0.57 | 0.48 | 0.39 | 0.57 | 0.48 | 0.39 | 0.57 | 0.48 | 0.39 |

## Settled accuracy p50 (stable)

`|final_hashrate / true_hashrate − 1|` at trial end.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 3.5% | 3.6% | 3.8% | 6.5% | 7.1% | 7.4% | 9.2% | 10.3% | 11.0% |
| 12 | 2.5% | 2.6% | 2.6% | 4.8% | 5.1% | 5.1% | 6.9% | 7.5% | 7.5% |
| 30 | 1.7% | 1.7% | 0.0% | 3.3% | 3.4% | 0.0% | 4.7% | 4.9% | 0.0% |
| 60 | 1.2% | 0.0% | 0.0% | 2.3% | 0.0% | 0.0% | 2.1% | 0.0% | 0.0% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

## Ramp target overshoot p90 (cold start)

`max(new_hashrate over fires) / H_true − 1` — tail of the distribution.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 10.4% | 7.3% | 4.1% |
| 12 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 5.7% | 3.2% | 0.3% |
| 30 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 1.9% | 0.0% | 0.0% |
| 60 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.1% | 0.0% | 0.0% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% |

## Ramp target overshoot p99 (cold start)

Worst-trial tail of the ramp overshoot distribution.

| SPM | η0.10/z2.576 | η0.10/z3.000 | η0.10/z3.500 | η0.20/z2.576 | η0.20/z3.000 | η0.20/z3.500 | η0.30/z2.576 | η0.30/z3.000 | η0.30/z3.500 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6 | 0.0% | 0.0% | 0.0% | 10.0% | 7.0% | 3.0% | 26.9% | 18.7% | 18.0% |
| 12 | 0.0% | 0.0% | 0.0% | 5.9% | 2.6% | 0.0% | 15.3% | 11.3% | 9.5% |
| 30 | 0.0% | 0.0% | 0.0% | 0.7% | 0.1% | 0.0% | 7.4% | 4.8% | 3.2% |
| 60 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 5.6% | 2.9% | 1.3% |
| 120 | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 0.0% | 2.7% | 1.0% | 0.2% |

## Pareto summary at SPM=6

Concentrates the trade-off picture at the cell where the FullRemedy parameters have the largest cross-axis coupling (SPM=6 is the hardest cell — sparsest Poisson signal, longest ramp, widest threshold band). For each metric, the table lists the (η, z) point that wins. Reading down: if one (η, z) point wins multiple rows, it is a strong candidate for the new default.

| Metric | Best (η, z) at SPM=6 | Value |
| --- | --- | --- |
| Decoupling score | η=0.10, z=2.576 | 0.884 |
| Jitter mean (stable load) | η=0.10, z=3.500 | 0.003 |
| Reaction rate at −50% step | η=0.10, z=2.576 | 0.88 |
| Reaction rate at −10% step | η=0.30, z=2.576 | 0.35 |
| Settled accuracy p50 (stable) | η=0.10, z=2.576 | 3.5% |
| Ramp target overshoot p90 (cold start) | η=0.10, z=2.576 | 0.0% |
| Ramp target overshoot p99 (cold start) | η=0.10, z=2.576 | 0.0% |

## Pareto summary at SPM=120

The high-SPM cell. Small-step sensitivity matters most here (the reaction-rate tables in this section drive the operational small-step floor). A new default must not regress meaningfully on the SPM=120 metrics — particularly `reaction rate at −10% step`, which `sweep-z` showed is the canary for excessive z.

| Metric | Best (η, z) at SPM=120 | Value |
| --- | --- | --- |
| Decoupling score | η=0.10, z=2.576 | 1.000 |
| Jitter mean (stable load) | η=0.10, z=3.500 | 0.000 |
| Reaction rate at −50% step | η=0.10, z=2.576 | 1.00 |
| Reaction rate at −10% step | η=0.30, z=2.576 | 0.57 |
| Settled accuracy p50 (stable) | η=0.10, z=2.576 | 0.0% |
| Ramp target overshoot p90 (cold start) | η=0.10, z=2.576 | 0.0% |
| Ramp target overshoot p99 (cold start) | η=0.10, z=2.576 | 0.0% |

