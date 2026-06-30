# vardiff_sim

Deterministic in-process simulation framework for characterizing the
behavioral attributes of any [`Vardiff`](../src/vardiff/mod.rs)
implementation. The framework decomposes a vardiff algorithm into three
orthogonal stages (Estimator, Boundary, UpdateRule) and lets each be
characterized in isolation, so a metric change can be attributed to the
specific stage that changed.

## Start here — why → how → proof

Two reader essays (why, how) and the technical paper they rest on. The essays
are the on-ramps — written to stand on their own; the paper is the proof
everything else points at, and the place to revalidate any claim. For a
reviewer the paper is the authoritative artifact, not optional; for a general
reader the essays suffice. Reading order:

1. **why** — [`docs/essays/better-vardiff.html`](./docs/essays/better-vardiff.html):
   *A Better Vardiff — What "Better" Turned Out to Mean.* The reasoning in plain
   language: why "better" didn't mean "tracks better."
2. **how** — [`docs/essays/building-a-vardiff.html`](./docs/essays/building-a-vardiff.html):
   *Building a Vardiff, Part by Part.* The classic controller taken apart and
   rebuilt — measure / decide / move — and where improving each stops mattering.
3. **proof** — [`docs/information-floor.md`](./docs/information-floor.md):
   *Vardiff at the Information Floor.* The closed theory, the derivations, the
   hardware validation, and the exact scope of every claim.

## Docs

The proof is `information-floor.md`; `THEORY.md` is its derivation companion;
everything else is supporting record under `docs/records/`.

- [`docs/information-floor.md`](./docs/information-floor.md) — **the paper
  (the proof)**: the closed theory (information-floor framing, death-spiral
  mechanism, the decline-safety selection criterion) and the metric it
  implies. Each result is labelled theory / simulation-only / hardware.
- [`docs/THEORY.md`](./docs/THEORY.md) — the derivation-and-falsification
  **notebook**: how the theory was derived and what was tried and killed.
  Reasoning-in-progress; not every claim here survived (see its banner).
  For *what is true and decided*, read the paper.
- [`docs/records/`](./docs/records/) — supporting record (architecture, findings,
  the per-controller investigations, and the test specs & status registers), each
  with a status banner pinning its scope and currency. **Start at
  [`docs/records/README.md`](./docs/records/README.md)** — the index that maps
  every record by purpose and reading group at a glance.
- [`docs/figures/`](./docs/figures/) — the generated SVG figures the paper embeds.
- [`docs/claims/`](./docs/claims/) — the claim-warrant validator and its fixture.

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

The algorithms in the registry are accessible via `AlgorithmSpec`
factories in `sim/src/grid.rs`. Each is a three-stage
`Composed<Estimator, Boundary, UpdateRule>`:

| Factory | Composition (Estimator / Boundary / Update) | Role |
|---------|---------------------------------------------|------|
| `classic_vardiff_state()` | classic triple, introspection-blind (`AsObservable`) | the classic comparison anchor — "the algorithm we used to run"; not the shipped default |
| `classic_composed()` | `CumulativeCounter / StepFunction(classic) / FullRetargetWithClamp` | fire-for-fire equivalent of the classic monolith; introspectable |
| `parametric()` | `CumulativeCounter / PoissonCI(z=2.576) / FullRetargetWithClamp` | classic with rate-aware threshold |
| `parametric_strict()` | `CumulativeCounter / PoissonCI(z=3.0) / FullRetargetWithClamp` | parametric with 99.7% CI |
| `ewma_60s()` / `ewma(τ)` | `EwmaEstimator(τ) / PoissonCI(z=2.576) / PartialRetarget(0.5)` | smoothed estimator |
| `sliding_window(n)` | `SlidingWindowEstimator(n) / PoissonCI / FullRetargetNoClamp` | last-n-ticks averaging |
| `classic_partial_retarget(η)` | `CumulativeCounter / StepFunction(classic) / PartialRetarget(η)` | classic with damped update |
| `full_remedy()` | `EwmaEstimator(120s) / PoissonCI / PartialRetarget(0.2)` | a strong mid-arc contender; superseded as the recommendation (see below) |

**The shipped production algorithm is the champion**,
`composed::champion_composed` (wired as the default behind
[`VardiffState`](../src/vardiff/classic.rs)):
`EwmaEstimator(360s) / AdaptiveSignPersist(spm_threshold=6) /
AcceleratingPartialRetarget(0.2, 0.6, 0.05)`. It was selected by minimax
over the target share rate under a **decline-safety constraint** — the
gentlest configuration that stays decline-safe across the rate band. The
earlier `full_remedy` recommendation was a scalar-fitness pick on a fast
drop; it does not clear the slow-decline gate. See
[`docs/information-floor.md`](./docs/information-floor.md) for the
selection criterion and [`docs/records/CKPOOL_INVESTIGATION.md`](./docs/records/CKPOOL_INVESTIGATION.md)
for why the fitness pick and the gate pick diverge.

## Running

The crate is structured as its own Cargo workspace (see the note on
`Cargo.lock` below). All commands run from `sv2/channels-sv2/sim/`:

```bash
# Fast unit tests (~1 second)
cargo test --lib

# Slow regression test against the committed baseline (~5–15 seconds)
cargo test --release --lib -- --ignored

# Regenerate the tracked CI fixtures (classic anchor + shipped champion)
cargo run --release --bin generate-baseline

# Generate cross-algorithm comparison baselines (~2–3 minutes; not tracked)
cargo run --release --bin compare-algorithms

# Trace one trial for a chosen (algorithm, scenario, SPM, seed)
cargo run --release --bin trace-trial -- -a vardiff_state -s cold_start --spm 6 --seed 0xCAFE

# Find worst-case seeds via overshoot-scan
cargo run --release --bin trace-trial -- -a vardiff_state -s cold_start --spm 6 --scan-overshoot 100
```

(`-a vardiff_state` traces the shipped production algorithm — `VardiffState`,
which delegates to the champion; `-a classic_composed` traces the classic
comparison anchor. See `trace-trial.rs` for the full key list.)

## Reproducing the claims

Every quantitative claim in the theory traces to a command here. The surface
is **two-tier**, by design:

**Tier 1 — CI-guarded gates.** The claims the production selection *rests on*
are guarded by runnable assertions that re-derive the value each run and fail
on drift (so the docs cite the *guard*, not a frozen number that could drift
from the code). One command runs all three:

```bash
cargo test --release --lib -- --ignored
```

| Guard (test name) | What it pins |
|-------------------|--------------|
| `decline_safety::…::champion_clears_decline_gate` | the shipped champion clears the **decline-safety gate** — its selection criterion (`information-floor.md` §9.2). The gate threshold lives once, in `decline_safety::DECLINE_SAFETY_GATE_PCT`. |
| `regression::…::champion_algorithm_no_regression` | the shipped champion's typical-behavior grid (cold-start/stable/step) has not drifted from `baseline_champion.toml`. |
| `regression::…::classic_algorithm_no_regression` | the classic comparison anchor has not drifted from `baseline_classic.toml`. |

**Tier 2 — pointer index.** Everything else is characterization: the command
below *regenerates* the figure or number, which you read against the prose
claim (the index points at the command without restating the value, so it
cannot fall out of sync with the code). Run from `sv2/channels-sv2/sim/`:

| Claim (see `docs/information-floor.md` §) | Regenerate with |
|-------------------------------------------|-----------------|
| Information floor `1/√(r*τ)`, the field is pinned to it (§3, §8) | `cargo run --release --bin sweep-minimax` |
| The τ-valley: worst-settled over-difficulty is U-shaped in the window, floor at 360 (§8.3) | `cargo run --release --bin tau-valley` |
| Per-rate optimum slides with rate, `τ*` sleepier when sparse (§8.3) | `cargo run --release --bin tau-family` |
| …and stays decline-admissible across the band (§8.3) | `cargo run --release --bin tau-family-safety` |
| The selection figure: champion is the gentlest point in the doubly-walled admissible island (§9) | `cargo run --release --bin tau-tradeoff` |
| Detection EXCESS climbs with `r*` — the one lever on the floor (§8.4) | `cargo run --release --bin excess-lever` |
| Steady-vs-transient safe frontier (§8.2) | `cargo run --release --bin steady-transient` |
| Dangerous-direction protection is regime-split (estimator sparse, boundary dense) (§6.1) | `cargo run --release --bin eager-ease-strength` and `--bin which-boundary` |
| Conservation-law falsification pass (regret ∝ δ², the death-spiral asymmetry) (§5.8) | `cargo run --release --bin regret-effort` |
| The full slow-decline safety sweep across rate × spm (the gate, in detail) | `cargo run --release --bin slow-decline` |
| Plain-language trajectory (estimate vs truth over time) | `cargo run --release --bin trajectory-plot` |

All binaries are deterministic given their seed (default base seed
`0xDEADBEEFCAFEF00D`); see **Determinism** below. The remaining binaries in
`src/bin/` are the **exploration record** — the search that produced these
results — and are not part of the reproduction surface; they compile (CI builds
them) so the search stays runnable, but a reviewer validating the result needs
only the commands above. The leaf exploration sweeps that no result or
investigation cites (parameter sweeps subsumed by the champion, dead-end probes)
were moved off this surface to tag `archive/exploration-bins` — reachable and
regenerable from there — so `src/bin/` holds the reproduction surface plus the
exploration bins still cited as provenance.

The slow regression test is `#[ignore]`-d by default so `cargo test`
stays fast. CI workflows that want full regression coverage should
invoke `cargo test --release --lib -- --ignored` explicitly.

## Reading the baseline output

`baseline_<AlgorithmName>.md` is organized by metric, with a final
decoupling-score table per share rate. For cross-algorithm
comparison:

```bash
diff baseline_classic.md baseline_champion.md
```

The `baseline_*.toml` files are the machine-readable form consumed
by the regression comparator. Two are tracked as CI regression
fixtures: `baseline_classic` (the classic comparison anchor) and
`baseline_champion` (the shipped algorithm). Cross-algorithm
comparison baselines are not tracked — regenerate them with
`cargo run --release --bin compare-algorithms`.

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
plugged into the simulation harness. The three-stage decomposition makes
this especially cheap: an algorithm that fits the
`Estimator/Boundary/UpdateRule` shape is one `Composed<E, B, U>`
construction and one `AlgorithmSpec` factory.

```rust
use vardiff_sim::{AlgorithmSpec, Composed, EwmaEstimator,
                  PoissonCI, PartialRetarget};

let spec = AlgorithmSpec::new("MyAlgo", |clock| {
    let v = Composed::new(
        EwmaEstimator::new(90),
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

For algorithms outside the three-stage decomposition (no
`Estimator/Boundary/Update` shape), implement `Vardiff`
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
├── docs/                    # information-floor.md (proof) + THEORY.md (notebook)
│   ├── essays/              #   why → how on-ramps (.html)
│   ├── records/             #   DESIGN, FINDINGS, investigations, test specs
│   ├── figures/             #   generated SVGs the paper embeds
│   └── claims/              #   claim-warrant validator + fixture
├── baseline_classic.{toml,md}   # CI regression fixture: classic anchor
├── baseline_champion.{toml,md}  # CI regression fixture: shipped champion
└── src/
    ├── lib.rs              # module declarations + re-exports
    ├── rng.rs              # XorShift64 + Poisson / exponential samplers
    ├── schedule.rs         # HashrateSchedule for trial scenarios
    ├── trial.rs            # run_trial + TickRecord + Observable
    ├── metrics.rs          # Metric trait + registry + bootstrap CI
    ├── baseline.rs         # Cell / CellResult / Scenario / Phase DSL / serializers
    ├── regression.rs       # baseline-parsing + tolerance assertions
    ├── grid.rs             # Grid + AlgorithmSpec + VardiffBox + run_paired
    ├── composed.rs         # Observable extension for Composed<E,B,U>
    └── bin/                # canonical entry points + exploration archive
        ├── generate-baseline.rs    # regression-fixture baseline(s)
        ├── compare-algorithms.rs   # cross-algorithm sweep
        ├── slow-decline.rs         # the decline-safety gate (champion selection)
        ├── trace-trial.rs          # single-trial tick-by-tick trace
        └── …                       # see the reproduction index below
```

The production three-stage algorithm itself lives in the parent crate at
[`../src/vardiff/composed/`](../src/vardiff/composed/) (`estimator.rs`,
`boundary.rs`, `update.rs`, `composed.rs`); the sim crate consumes it.
