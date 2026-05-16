# vardiff_sim

Deterministic in-process simulation framework for characterizing the
behavioral attributes of any [`Vardiff`](../src/vardiff/mod.rs)
implementation. The framework decomposes a vardiff algorithm into four
orthogonal axes (Estimator, Statistic, Boundary, UpdateRule) and lets
each be characterized in isolation, so a metric change can be
attributed to the specific axis that changed.

## Docs

- [`docs/DESIGN.md`](./docs/DESIGN.md) — architectural reference. The
  four-axis decomposition, the trait surface, the `Composed<E, S, B,
  U>` adapter, the algorithm registry, the trial-recording model, the
  scenario DSL, the Grid, the Metric trait, the recommended production
  composition, and the production migration plan.
- [`docs/FINDINGS.md`](./docs/FINDINGS.md) — what the framework has
  measured about the algorithm registry: the cross-algorithm
  decoupling Pareto, the SPM=6 cold-start cascade mechanism, the
  composition argument (no single axis closes the cascade — the right
  composition does), the FullRemedy validation, the EWMA τ
  Pareto, the asymmetric step-response analysis, the U256 precision
  fix to `channels_sv2::target`.

## What the framework measures

Given any `Vardiff` implementation, the framework produces a
distribution for each metric across many independent simulated trials
(1000 per cell by default), reported as percentiles plus mean and
count, over a parameterized grid of share rates × hashrate scenarios ×
algorithms:

| Metric | Category | What it measures |
|--------|----------|------------------|
| `convergence_time` | Behavioral | seconds to first fire-followed-by-quiet-window after cold start |
| `settled_accuracy` | Behavioral | `\|final_hashrate / true_hashrate − 1\|` at trial end |
| `jitter` | Behavioral | fires per minute under stable load (post-convergence) |
| `reaction_time` | Behavioral | seconds from a step change to the first subsequent fire |
| `bias` | Estimator | `E[H̃ − H_true] / H_true` averaged over post-settle ticks (introspectable algorithms only) |
| `variance` | Estimator | population variance of `H̃ / H_true` over post-settle ticks (introspectable algorithms only) |
| `ramp_target_overshoot` | Behavioral | peak fire-target above truth during cold start (works for any algorithm) |
| `reaction_asymmetry` | Robustness | `reaction_rate(+δ%) − reaction_rate(−δ%)` per share rate, per magnitude δ ∈ {5, 10, 25, 50} (derived cross-cell) |

Plus the cross-cutting `decoupling_score` (`reaction_rate × clamp(1 −
jitter_p50 / J_max, 0, 1)`) that summarizes the
variance-vs-detection trade-off in one number per share rate.

## Algorithms shipped

The eight algorithms in the registry are accessible via
`AlgorithmSpec` factories in `sim/src/grid.rs`:

| Factory | Composition | Role |
|---------|-------------|------|
| `classic_vardiff_state()` | (production reference, not introspectable) | the existing production algorithm |
| `classic_composed()` | `CumulativeCounter + AbsoluteRatio + StepFunction(classic) + FullRetargetWithClamp` | fire-for-fire equivalent four-axis representation of `VardiffState`; introspectable |
| `parametric()` | `… + PoissonCI(z=2.576) + …` | classic with rate-aware threshold |
| `parametric_strict()` | `… + PoissonCI(z=3.0) + …` | parametric with 99.7% CI |
| `ewma_60s()` / `ewma(τ)` | `EwmaEstimator(τ) + … + PartialRetarget(0.5)` | smoothed estimator |
| `sliding_window(n)` | `SlidingWindowEstimator(n) + … + FullRetargetNoClamp` | last-n-ticks averaging |
| `classic_partial_retarget(η)` | `… + PartialRetarget(η)` | classic with damped update |
| `full_remedy()` | `EwmaEstimator(120s) + AbsoluteRatio + PoissonCI + PartialRetarget(0.3)` | **production recommendation** |

`FullRemedy` dominates every other algorithm on every operationally
meaningful metric — see [`docs/FINDINGS.md`](./docs/FINDINGS.md) for
the case.

## Running

The crate is structured as its own Cargo workspace (see the note on
`Cargo.lock` below). All commands run from `sv2/channels-sv2/sim/`:

```bash
# Fast unit tests (~1 second)
cargo test --lib

# Slow regression test against the committed baseline (~5–15 seconds)
cargo test --release --lib -- --ignored

# Generate a fresh single-algorithm baseline
cargo run --release --bin generate-baseline

# Generate cross-algorithm comparison baselines (~2–3 minutes for 8 algorithms × 50 cells × 1000 trials)
cargo run --release --bin compare-algorithms

# EWMA τ Pareto sweep (~90 seconds for 5 τ values × 50 cells × 1000 trials)
cargo run --release --bin sweep-ewma-tau

# Trace one trial for a chosen (algorithm, scenario, SPM, seed)
cargo run --release --bin trace-trial -- -a full_remedy -s cold_start --spm 6 --seed 0xCAFE

# Find worst-case seeds via overshoot-scan
cargo run --release --bin trace-trial -- -a vardiff_state -s cold_start --spm 6 --scan-overshoot 100
```

The slow regression test is `#[ignore]`-d by default so `cargo test`
stays fast. CI workflows that want full regression coverage should
invoke `cargo test --release --lib -- --ignored` explicitly.

## Reading the baseline output

`baseline_<AlgorithmName>.md` is organized by metric, with a final
decoupling-score table per share rate. For cross-algorithm
comparison:

```bash
diff baseline_VardiffState.md baseline_FullRemedy.md
```

The `baseline_*.toml` files are the machine-readable form consumed
by the regression comparator.

## Updating the baseline

When a proposed algorithm change legitimately improves metrics, the
baseline must be regenerated. Workflow:

1. Make the algorithm change.
2. Run `cargo test --release --lib -- --ignored` to see which metrics
   changed and how. The failure message identifies the cell and
   metric.
3. Decide whether each change is intended improvement vs. regression.
   Discuss in PR review.
4. If the changes are intended, regenerate:
   `cargo run --release --bin compare-algorithms` (or
   `generate-baseline` for the single-algorithm path).
5. Review the diff on the `.toml` and `.md` files carefully. The
   TOML diff is the structured source of truth; the markdown is for
   human review.
6. Commit the regenerated baseline alongside the algorithm change.

Updating the baseline is intentionally manual — automatic baseline
drift would defeat the purpose of regression detection. The PR review
is the gate.

## Adding a new algorithm

Any implementor of the [`Vardiff`](../src/vardiff/mod.rs) trait can be
plugged into the simulation harness. The four-axis decomposition makes
this especially cheap: an algorithm that fits the
`Estimator/Statistic/Boundary/UpdateRule` shape is one
`Composed<E, S, B, U>` construction and one `AlgorithmSpec` factory.

```rust
use vardiff_sim::{AlgorithmSpec, Composed, EwmaEstimator, AbsoluteRatio,
                  PoissonCI, PartialRetarget};

let spec = AlgorithmSpec::new("MyAlgo", |clock| {
    let v = Composed::new(
        EwmaEstimator::new(90),
        AbsoluteRatio,
        PoissonCI::default_parametric(),
        PartialRetarget::new(0.4),
        /* min_h */ 1.0,
        clock,
    );
    vardiff_sim::grid::VardiffBox(Box::new(v))
});
```

To characterize the new algorithm against the rest of the registry,
add it to the `algorithms` list in `bin/compare-algorithms.rs` (or
your own binary) and re-run the sweep.

For algorithms outside the four-axis decomposition (no
`Estimator/Statistic/Boundary/Update` shape), implement `Vardiff`
directly and wrap with `AsObservable<V>` so the grid's dispatch path
accepts it; introspection-only metrics will gracefully degrade to
`None` for that algorithm. See `AlgorithmSpec::classic_vardiff_state`
for the canonical example.

## Project-specific notes

### `Cargo.lock` is checked in

The sim crate is declared as its own workspace (see the `[workspace]`
block in `Cargo.toml`) and ships with a `Cargo.lock` that pins
transitive dependency versions. This is **by design** — the parent
stratum workspace is pinned to Rust 1.75, which cannot write the `v4`
lockfile format produced by newer toolchains. Adding the sim crate as
a workspace member would force a lockfile rewrite that 1.75 can't
perform.

If you delete `Cargo.lock` "to refresh" the dep tree, the build will
break with `feature 'edition2024' is required` errors from transitive
deps. Recover by copying the parent's lockfile back:

```bash
cp ../../../Cargo.lock .
```

### Determinism

Every trial is fully deterministic given its `(config, schedule,
seed)` triple. The grid uses a fixed `base_seed` (default
`0xDEADBEEFCAFEF00D`) and derives per-cell, per-trial seeds via
`base_seed.wrapping_add(cell_index << 20).wrapping_add(trial_index)`.
Re-runs produce byte-identical output across machines and time,
modulo Rust toolchain version changes that affect floating-point
determinism (e.g., a different `f64::ln` implementation).

The `Trial` struct carries the seed it was produced from, so a
failing trial in the regression test can be reproduced in isolation
by replaying with the same seed via `trace-trial`.

### Simulation fidelity vs production

The simulation models share arrivals as pure Poisson with rate
matching the algorithm's configured target. Real miners cluster
shares (TCP batching, hash-device timing, network jitter) so the
variance under real-world load is wider than Poisson predicts. The
framework's claims are therefore **conservative comparisons between
algorithms under identical simulated load** — "A jitters 87% less
than B" — rather than absolute predictions about deployment
behavior. Real-world plausibility is the job of integration tests
and deployment observation.

## Layout

```
sim/
├── Cargo.toml              # crate manifest + [workspace] declaration
├── Cargo.lock              # copy of parent workspace's lockfile (see above)
├── README.md               # this file
├── docs/
│   ├── DESIGN.md           # architectural reference
│   └── FINDINGS.md         # characterization results
├── baseline_*.{toml,md}    # per-algorithm baselines
├── ewma_tau_sweep.md       # τ Pareto report
└── src/
    ├── lib.rs              # module declarations + re-exports
    ├── rng.rs              # XorShift64 + Poisson / exponential samplers
    ├── schedule.rs         # HashrateSchedule for trial scenarios
    ├── trial.rs            # run_trial + TickRecord + Observable
    ├── metrics.rs          # Metric trait + registry + 8 metrics + bootstrap CI
    ├── baseline.rs         # Cell / CellResult / Scenario / Phase DSL / serializers
    ├── regression.rs       # baseline-parsing + tolerance assertions
    ├── grid.rs             # Grid + AlgorithmSpec + VardiffBox + run_paired
    ├── composed/
    │   ├── mod.rs
    │   ├── estimator.rs    # Cumulative / EWMA / SlidingWindow
    │   ├── statistic.rs    # AbsoluteRatio
    │   ├── boundary.rs     # StepFunction (classic) + PoissonCI
    │   ├── update.rs       # FullRetargetWithClamp + PartialRetarget + FullRetargetNoClamp
    │   └── composed.rs     # Composed<E,S,B,U> + classic_composed equivalence tests
    └── bin/
        ├── generate-baseline.rs    # single-algorithm baseline
        ├── compare-algorithms.rs   # cross-algorithm sweep
        ├── sweep-ewma-tau.rs       # τ Pareto sweep
        └── trace-trial.rs          # single-trial tick-by-tick trace
```
