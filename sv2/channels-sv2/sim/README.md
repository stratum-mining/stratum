# vardiff_sim

A deterministic in-process simulation framework for characterizing the behavioral attributes of any [`Vardiff`](../src/vardiff/mod.rs) implementation, plus a regression test that asserts the current algorithm against a checked-in baseline.

For the conceptual design, metric definitions, and tradeoff rationale see [`VARDIFF_SIMULATION_FRAMEWORK.md`](./VARDIFF_SIMULATION_FRAMEWORK.md).

## What this crate measures

Given any `Vardiff` implementation, the framework characterizes five behavioral attributes across a parameterized grid of share rates and hashrate scenarios:

- **Convergence time** — how long the algorithm takes to settle after a cold start
- **Settled accuracy** — how close to truth the algorithm lands once settled
- **Steady-state jitter** — how often the algorithm fires on noise after settling
- **Reaction time** — how long until the algorithm fires after a genuine load change
- **Reaction sensitivity** — what magnitude of change reliably triggers a fire

Each metric is a distribution across many independent simulated trials (1000 per cell by default), reported as percentiles. The full output for the current algorithm lives in [`vardiff_baseline.md`](./vardiff_baseline.md) (human-readable) and [`vardiff_baseline.toml`](./vardiff_baseline.toml) (machine-readable, consumed by the regression test).

## Running

The crate is structured as its own Cargo workspace (see the note on `Cargo.lock` below). All commands run from `sv2/channels-sv2/sim/`:

```bash
# Fast unit tests (~1 second)
cargo test

# Generate a fresh baseline (~5-15 seconds)
cargo run --release --bin generate-baseline

# Run the slow regression test against the committed baseline (~5-15 seconds)
cargo test --release --lib -- --ignored
```

The regression test is `#[ignore]`-d by default so `cargo test` stays fast. CI workflows that want full regression coverage should invoke `cargo test --release --lib -- --ignored` explicitly.

## Reading the baseline output

`vardiff_baseline.md` is organized by metric:

- **Convergence time**: per share rate, the fraction of trials that converged plus percentile of convergence time. Look at `rate` first (catastrophic regressions show up here as <90%) then at `p90` and `p99` (tail-end slowdowns).
- **Settled accuracy**: relative error between the algorithm's final hashrate estimate and truth. Smaller is better; at low share rates (6 spm) this is bounded by Poisson noise on the share count, so don't expect zero.
- **Steady-state jitter**: fires per minute during the settled period. Smaller is better; ideal is zero under stable load.
- **Reaction time to -50% drop**: per share rate, fraction of trials that reacted within 5 minutes plus percentile of reaction time.
- **Reaction sensitivity curve**: for each Δ in {±5, ±10, ±25, ±50}%, fraction of trials that fired within 5 minutes. The shape of this curve reveals threshold structure — sharp step = tight thresholds, smooth ramp = loose thresholds.

## Updating the baseline

When a proposed algorithm change legitimately improves metrics, the baseline must be regenerated. Workflow:

1. Make the algorithm change.
2. Run `cargo test --release --lib -- --ignored` to see which metrics changed and how. The failure message identifies the cell and metric.
3. Decide whether each change is intended improvement vs. regression. Discuss in PR review.
4. If the changes are intended, regenerate: `cargo run --release --bin generate-baseline`.
5. Review the diff on `vardiff_baseline.toml` and `vardiff_baseline.md` carefully. The TOML diff is the structured source of truth; the markdown is for human review.
6. Commit the regenerated baseline alongside the algorithm change.

Updating the baseline is intentionally manual — automatic baseline drift would defeat the purpose of regression detection. The PR review is the gate.

## Adding a new algorithm

Any implementor of the [`Vardiff`](../src/vardiff/mod.rs) trait can be plugged into the simulation harness:

```rust
let clock = Arc::new(MockClock::new(0));
let my_vardiff = MyAlgorithm::new_with_clock(1.0, clock.clone());
let trial = run_trial(my_vardiff, clock, config, &schedule, seed);
```

To get a characterization comparable to the checked-in baseline, write a binary similar to [`bin/generate-baseline.rs`](./src/bin/generate-baseline.rs) that constructs your algorithm in place of `VardiffState`.

## Project-specific notes

### `Cargo.lock` is checked in

The sim crate is declared as its own workspace (see the `[workspace]` block in `Cargo.toml`) and ships with a `Cargo.lock` that pins transitive dependency versions. This is **by design** — the parent stratum workspace is pinned to Rust 1.75, which cannot write the `v4` lockfile format produced by newer toolchains. Adding the sim crate as a workspace member would force a lockfile rewrite that 1.75 can't perform.

If you delete `Cargo.lock` "to refresh" the dep tree, the build will break with `feature 'edition2024' is required` errors from transitive deps. Recover by copying the parent's lockfile back:

```bash
cp ../../../Cargo.lock .
```

### Determinism

Every trial is fully deterministic given its `(config, schedule, seed)` triple. The baseline run uses a fixed `base_seed` (default `0xDEADBEEFCAFEF00D`) and derives per-cell, per-trial seeds via `base_seed.wrapping_add(cell_index << 20).wrapping_add(trial_index)`. Re-runs of `generate-baseline` produce byte-identical output across machines and time, modulo Rust toolchain version changes that affect floating-point determinism (e.g., a different `f64::ln` implementation).

The `Trial` struct carries the seed it was produced from, so a failing trial in the regression test can be reproduced in isolation by replaying with the same seed.

### Simulation fidelity vs production

The simulation models share arrivals as pure Poisson with rate matching the algorithm's configured target. Real miners cluster shares (TCP batching, hash-device timing, network jitter) so the variance under real-world load is wider than Poisson predicts. The framework's claims are therefore **conservative comparisons between algorithms under identical simulated load** — "A jitters 87% less than B" — rather than absolute predictions about deployment behavior. Real-world plausibility is the job of integration tests and deployment observation.

## Layout

```
sim/
├── Cargo.toml              # crate manifest + [workspace] declaration
├── Cargo.lock              # copy of parent workspace's lockfile (see above)
├── README.md               # this file
├── VARDIFF_SIMULATION_FRAMEWORK.md  # design proposal
├── vardiff_baseline.toml   # machine-readable baseline (regression-test input)
├── vardiff_baseline.md     # human-readable baseline summary
└── src/
    ├── lib.rs              # module declarations + re-exports
    ├── rng.rs              # XorShift64 + Poisson / exponential samplers
    ├── schedule.rs         # HashrateSchedule for trial scenarios
    ├── trial.rs            # run_trial: the per-tick simulation loop
    ├── metrics.rs          # Distribution + the five metric functions
    ├── baseline.rs         # Cell / CellResult / run_baseline + serializers
    ├── regression.rs       # baseline-parsing + tolerance assertions
    └── bin/
        └── generate-baseline.rs   # CLI binary
```
