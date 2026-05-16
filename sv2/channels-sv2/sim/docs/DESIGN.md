# Vardiff simulation framework — design

Architectural reference for the `sv2/channels-sv2/sim` crate. Explains
the four-axis algorithm decomposition, the trait surface, the
algorithm registry, the trial-recording model, the scenario DSL, the
Grid, the Metric trait, and the recommended production composition.
Empirical results are in [`FINDINGS.md`](./FINDINGS.md).

## What this framework is

A deterministic in-process harness for characterizing the behavioral
attributes of any `Vardiff` implementation. Given an algorithm and a
specification of how shares arrive (Poisson with a configurable rate)
plus how the miner's true hashrate evolves over time (a piecewise
schedule), the framework runs many independent trials and reports a
fixed set of metrics — convergence time, settled accuracy, jitter,
reaction time, estimator bias and variance, peak excursion, ramp
overshoot — along with the cross-cutting *decoupling score* that
summarizes the variance-vs-detection trade-off.

The framework's distinctive feature is that it characterizes
*algorithms as compositions* rather than as opaque whole. A vardiff
algorithm makes four logically distinct decisions — how to estimate
the miner's hashrate, how to compute a test statistic from that
estimate, what threshold the statistic must cross to trigger a
retarget, and how to compute the new target — and the framework
exposes each as an independent extension point. Holding three
decisions fixed and varying the fourth lets the framework attribute
metric changes to the specific axis that changed.

## The four axes

The framework decomposes a vardiff algorithm into four
input-domain-disjoint pieces:

| Axis | What it does | Theoretical home | Design pressure |
|------|--------------|------------------|-----------------|
| **Estimator** | accumulates observations between ticks; produces a snapshot of the algorithm's belief about miner hashrate (`H̃`) | statistical estimation | responsiveness vs noise (window size) |
| **Statistic** | computes a scalar δ from `H̃` and the configured rate λ\* — "how surprised should I be?" | hypothesis testing | power vs simplicity |
| **Boundary** | computes the threshold θ that δ must exceed to fire | decision theory | Type I vs Type II error; rate-awareness |
| **UpdateRule** | when the algorithm fires, computes the new target | control theory | convergence speed vs steady-state stability |

In code:

```rust
pub trait Estimator {
    fn observe(&mut self, n_shares: u32);
    fn snapshot(&self, ctx: &EstimatorContext) -> EstimatorSnapshot;
    fn reset(&mut self, now_secs: u64);
}

pub trait Statistic {
    fn delta(&self, snap: &EstimatorSnapshot, shares_per_minute: f32) -> f64;
}

pub trait Boundary {
    fn threshold(&self, dt_secs: u64, shares_per_minute: f32) -> f64;
}

pub trait UpdateRule {
    fn next_hashrate(
        &self,
        snap: &EstimatorSnapshot,
        current_hashrate: f32,
        delta: f64,
        shares_per_minute: f32,
    ) -> f32;
}
```

The four traits are defined in `sim/src/composed/{estimator,
statistic, boundary, update}.rs`.

### Why these four axes

The decomposition is motivated and constrained by three filters:

1. **Named algorithms cluster on it.** Existing vardiff variants
   (Parametric, EWMA, Sliding-Window, CUSUM) each differ from the
   classic algorithm along specific subsets of the four axes — not
   smearing across all of them. If every algorithm differed in every
   way, the split would be post-hoc taxonomy rather than a cleavage
   of the design space.
2. **Each axis maps to its own theoretical framework.** An engineer
   designing or analyzing one piece can reach for axis-specific
   literature (estimation, hypothesis testing, decision theory,
   control) rather than re-projecting algorithm-as-a-whole reasoning.
3. **Axes are input-domain-disjoint.** Each is a pure function with
   its own inputs and outputs; swapping one implementation cannot
   change another's interface. This is what makes "hold three axes
   constant, vary the fourth, attribute the metric change to that
   axis" mechanically valid rather than a hope.

### Alternative decompositions considered

| Decomposition | Axes | Why this choice loses |
|---------------|:----:|----------------------|
| Observation / Decision | 2 | Bundles estimation with hypothesis testing. Can't characterize "same estimator, different statistic" or "same statistic, different threshold". |
| Estimator / Decision Rule / Update | 3 | Merges Statistic and Boundary into a single "Decision Rule". The most valuable per-axis swap — Parametric's rate-aware boundary holding everything else constant — can't be expressed as a single-axis change. |
| State / Statistic / Boundary / Update / Sample / Action | 6 | The extra axes (Sample = per-tick share count, Action = "set new target") are constant across every known algorithm. Adds trait surface without expressive power. |
| Per-tick / Per-fire / Per-trial | 3 | Slices along *time* rather than *function*. Doesn't decouple design decisions: each layer reimplements every operation. |
| State / Action / Reward (RL framing) | 3 | Treats vardiff as a Markov decision process. Mathematically valid but heavy; obscures the Poisson-statistics structure that makes the problem analytically tractable. |
| Predictor / Controller (control framing) | 2 | Bundles "what to predict" (Estimator + Statistic) with "when to actuate" (Boundary). Same expressive loss as Observation/Decision. |

The 3-axis Estimator/Decision-Rule/Update split is the closest miss
and the tempting alternative — symmetric and elegant. It loses on
filter (1): Classic and Parametric are *identical* on three axes and
differ only on Boundary. Merging Statistic and Boundary into a
"Decision Rule" makes that swap require forking the whole decision
rule, statistic-and-all, which loses the surgical capability that
makes the framework useful.

### Mathematical orthogonality

Each axis is a pure function with its own input domain:

```
Estimator::snapshot(...)                       → EstimatorSnapshot
Statistic::delta(snapshot, λ*)                 → f64
Boundary::threshold(dt, λ*)                    → f64
UpdateRule::next_hashrate(snapshot, ...)       → f32
```

Swapping any one implementation does not change another's interface.
Downstream observables can cascade — changing Estimator changes the
snapshot, which then changes δ via Statistic and the new target via
UpdateRule — but the cascade flows through stable interfaces. Every
single-axis swap is therefore a controlled intervention, and any
metric movement is attributable to the axis that changed.

A merged-axes decomposition loses this property. Swapping a combined
Statistic+Boundary changes two design choices at once, and the
framework cannot attribute the resulting metric change to either
piece in isolation.

## The `Composed` adapter

A composition of four axis impls is bundled by
`Composed<E, S, B, U>`, which carries a blanket `impl Vardiff`. The
adapter is therefore a drop-in replacement for `VardiffState` at any
production call site that holds a `Box<dyn Vardiff>` or accepts
`impl Vardiff`:

```rust
let v = Composed::new(
    EwmaEstimator::new(120),
    AbsoluteRatio,
    PoissonCI::default_parametric(),
    PartialRetarget::new(0.3),
    /* min_hashrate */ 1.0,
    clock,
);
// v: impl Vardiff
```

The state held directly by `Composed` (last-update timestamp,
minimum hashrate, clock) has the same shape `VardiffState` holds —
only the algorithm logic is factored into the four axis impls.

The production `Vardiff` trait, `Clock` injection, and the
`channels_sv2::vardiff` API surface are unchanged by the framework.
Algorithms outside the four-axis decomposition (notably the existing
`VardiffState`) continue to run through the same trial driver
directly.

## Algorithm registry

The framework ships eight algorithms, accessible via `AlgorithmSpec`
factories in `sim/src/grid.rs`:

| Algorithm | Estimator | Statistic | Boundary | UpdateRule |
|-----------|-----------|-----------|----------|-----------|
| `VardiffState` | (production reference, not introspectable) | | | |
| `ClassicComposed` | `CumulativeCounter` | `AbsoluteRatio` | `StepFunction::classic_table()` | `FullRetargetWithClamp::classic()` |
| `Parametric` | `CumulativeCounter` | `AbsoluteRatio` | `PoissonCI(z=2.576, margin=0.05)` | `FullRetargetWithClamp::classic()` |
| `ParametricStrict` | `CumulativeCounter` | `AbsoluteRatio` | `PoissonCI(z=3.0, margin=0.05)` | `FullRetargetWithClamp::classic()` |
| `ClassicPartialRetarget-30` | `CumulativeCounter` | `AbsoluteRatio` | `StepFunction::classic_table()` | `PartialRetarget(η=0.3)` |
| `EWMA-60s` | `EwmaEstimator(60s)` | `AbsoluteRatio` | `PoissonCI::default_parametric()` | `PartialRetarget(η=0.5)` |
| `SlidingWindow-10t` | `SlidingWindowEstimator(10 ticks)` | `AbsoluteRatio` | `PoissonCI::default_parametric()` | `FullRetargetNoClamp` |
| **`FullRemedy`** | **`EwmaEstimator(120s)`** | **`AbsoluteRatio`** | **`PoissonCI::default_parametric()`** | **`PartialRetarget(η=0.3)`** |

`VardiffState` is the production reference. `ClassicComposed` is its
four-axis representation; the two are asserted fire-for-fire
equivalent by the `equivalence_tests` module in
`sim/src/composed/composed.rs` (matching trial seeds and trial inputs
produce identical fire timestamps and target trajectories).

The remaining six are systematic explorations of the design space.
`FullRemedy` is the recommended production composition — see
[`FINDINGS.md`](./FINDINGS.md) for the empirical case.

### Per-axis implementations

| Trait | Implementations |
|-------|-----------------|
| `Estimator` | `CumulativeCounter`, `EwmaEstimator(tau_secs)`, `SlidingWindowEstimator(n_ticks)` |
| `Statistic` | `AbsoluteRatio` |
| `Boundary` | `StepFunction(table)`, `PoissonCI(z, margin)` |
| `UpdateRule` | `FullRetargetWithClamp(mults)`, `PartialRetarget(eta)`, `FullRetargetNoClamp` |

`StepFunction::classic_table()` and
`FullRetargetWithClamp::classic()` are the specific parameterizations
that reproduce `VardiffState`. The `PoissonCI` boundary derives its
threshold from the Poisson confidence interval on the realized share
count under the null hypothesis:

```
λ̄ = (SPM / 60) × Δt              (expected share count under H₀)
θ_fraction = (z·√λ̄ + 0.5) / λ̄ + margin
θ_pct = θ_fraction × 100
```

`z = 2.576` corresponds to a ~99% CI. `PoissonCI::with_z(z, margin)`
and `PoissonCI::strict_3sigma()` (z = 3.0) are also available.

## Trial recording

The framework records every tick of every trial as a `TickRecord`,
not just the moments the algorithm fires:

```rust
pub struct TickRecord {
    pub t_secs: u64,
    pub n_shares: u32,
    pub current_hashrate_before: f32,
    pub delta: Option<f64>,         // δ at this tick (if observable)
    pub threshold: Option<f64>,     // θ at this tick (if observable)
    pub h_estimate: Option<f32>,    // H̃ at this tick (if observable)
    pub fired: bool,
    pub new_hashrate: Option<f32>,
}

pub struct Trial {
    pub seed: u64,
    pub ticks: Vec<TickRecord>,
    pub true_hashrate_at_end: f32,
    /* ... */
}
```

At a 60s tick interval over a 30-minute trial that's 30 records per
trial. Across 1000 trials per cell × 50 cells the working set is a
few tens of megabytes — bounded.

The optional δ/θ/H̃ fields are populated by `run_trial_observed` for
algorithms that implement the `Observable` trait, which exposes the
algorithm's per-tick decision state. `Composed<E, S, B, U>`
implements `Observable` natively; `VardiffState` doesn't (the
production type has no introspection API). An `AsObservable<V>`
wrapper provides a null impl that always returns `None`, so
non-introspectable algorithms flow through the same dispatch path
with the optional fields gracefully empty.

Introspection-only metrics (bias, variance) gracefully report `None`
for non-observable algorithms; the TOML serializer simply omits
absent keys.

## Scenarios

A `Scenario` is built from a list of `Phase` primitives:

```rust
pub enum Phase {
    Hold { duration_secs: u64, h: f32 },
    Ramp { duration_secs: u64, from: f32, to: f32 },
    Stall { duration_secs: u64 },
}
```

The named scenarios are convenience shorthands:

| Scenario | Phases |
|----------|--------|
| `ColdStart` | `Hold(1800s, 1e15)` with `initial_hashrate = 1e10` |
| `Stable` | `Hold(1800s, 1e15)` |
| `Step { delta_pct }` | `Hold(900s, 1e15), Hold(900s, 1e15 × (1 + δ%))` |

New scenarios (stall recovery, sustained noise, transient spike,
slow ramp) compose the same primitives via `Scenario::Custom { name,
phases, initial_estimate }`.

## Grid

A `Grid` is a Cartesian product over `algorithms × share_rates ×
scenarios`. One `Grid::run` produces every algorithm's
characterization in one sweep:

```rust
let grid = Grid {
    algorithms: vec![
        AlgorithmSpec::classic_vardiff_state(),
        AlgorithmSpec::full_remedy(),
    ],
    share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
    scenarios: vec![Scenario::ColdStart, Scenario::Stable,
                    Scenario::Step { delta_pct: -50 }],
    trial_count: 1000,
    base_seed: 0xDEAD_BEEF_CAFE_F00D,
};
let results = grid.run();  // HashMap<algorithm_name, Vec<CellResult>>
```

`Grid::run_paired` strips the algorithm index from the seed
derivation so all algorithms see the same trial inputs at each cell.
Cross-algorithm metric differences are then attributable to the
algorithm alone, not to seed disparity. Use this for clean A/B
comparisons; use the default `Grid::run` for baseline generation
(where each algorithm gets its own seed space).

### Seeding

Per-trial seeds are derived as
`base_seed + (cell_index << 20) + trial_index` with
`cell_index = algo_idx × N_spm × N_scen + spm_idx × N_scen +
scen_idx`. The `<< 20` shift gives each cell a 1,048,576-entry seed
range; collision-free as long as `trial_count ≤ 2²⁰`. The default
1000-trial baselines are well within range.

## Metrics

Each metric is a `Box<dyn Metric>` in the `registry`. The trait
exposes everything about a metric: how it's computed, which cells it
applies to, its tolerance policy for regression checks, and how it
renders to Markdown.

```rust
pub trait Metric: Debug + Send + Sync {
    fn id(&self) -> &'static str;
    fn category(&self) -> MetricCategory;
    fn class(&self) -> MetricClass;
    fn applies_to(&self, cell: &Cell) -> bool;
    fn compute(&self, trials: &[Trial], cell: &Cell) -> MetricValues;
    fn tolerance_checks(&self, cell: &Cell) -> Vec<ToleranceCheck>;
    fn render_markdown(&self, ...) -> String;
}
```

The registered metrics are:

| ID | Category | What it measures |
|----|----------|------------------|
| `convergence_time` | Behavioral | seconds to first fire-followed-by-quiet-window after cold start |
| `settled_accuracy` | Behavioral | `\|final_hashrate / true_hashrate − 1\|` at trial end |
| `jitter` | Behavioral | fires per minute under stable load (post-convergence) |
| `reaction_time` | Behavioral | seconds from a step change to the first subsequent fire |
| `bias` | Estimator | `E[H̃ − H_true] / H_true` averaged over post-settle ticks |
| `variance` | Estimator | population variance of `H̃ / H_true` over post-settle ticks |
| `ramp_target_overshoot` | Behavioral | peak fire-target `/ H_true − 1` during cold start |
| `reaction_asymmetry` | Robustness | `reaction_rate(+δ%) − reaction_rate(−δ%)` per share rate, per magnitude δ ∈ {5, 10, 25, 50} |

Plus the derived cross-cutting `decoupling_score`:

```
decoupling = reaction_rate(Step−50%) × clamp(1 − jitter_p50 / J_max, 0, 1)
```

with `J_max = 0.50` fires/min. Higher is better; `1.0` indicates
perfect decoupling between reactivity and steady-state quietness.

The framework distinguishes two metric categories at the type level:

- **`Metric`** is per-cell (one cell = one share rate × one
  scenario × N trials). The trait's `compute(trials, cell)` produces
  one `MetricValues` per cell, serialized into the cell's TOML
  section.
- **`DerivedMetric`** is cross-cell — its `compute(results)` consumes
  the already-computed `CellResult` set and emits one `MetricValues`
  per share rate. `DecouplingScore` and `ReactionAsymmetry` are the
  shipped derived metrics; they're serialized into top-level
  `[derived.<id>.spm_<X>]` TOML sections.

Adding a new metric is one new `impl Metric` (per-cell) or one new
`impl DerivedMetric` (cross-cell). The TOML serializer, the
regression comparator, and the Markdown renderer iterate the relevant
registry; none has per-metric logic.

### Bootstrap CI

`bootstrap_percentile_ci(values, n_resamples, seed)` returns the
(2.5%, 97.5%) bounds on any percentile, using deterministic
`XorShift64` for reproducibility. Tolerance budgets in the regression
checks can therefore be calibrated against statistical noise rather
than against a single observed value.

## The composition argument

The four-axis decomposition's load-bearing claim is that *no single
axis change closes the worst-case failure modes* exposed by the
algorithm registry, but that *the right composition of three axis
changes does*. Both halves of the claim are empirically demonstrated;
the full details are in [`FINDINGS.md`](./FINDINGS.md).

The summary is:

- The Classic boundary at low share rates (SPM=6) is permeable to
  Poisson outliers, producing peak fire-target excursions up to 187%
  above truth in worst-case trials.
- Tightening the boundary (Parametric, ParametricStrict) doesn't
  close this — the PoissonCI threshold formula has a structural
  ceiling at low λ that no z-tightening can lift.
- Switching the Estimator alone (EWMA-60s) absorbs single-window
  Poisson spikes but pays a settled-accuracy cost from slow late
  convergence.
- Switching the UpdateRule alone (ClassicPartialRetarget) bounds the
  per-fire magnitude but breaks Phase 1 convergence (η=0.3 damping
  is too weak to recover from a single bad minute at low SPM).
- The composition **`FullRemedy = Composed<EwmaEstimator(120s),
  AbsoluteRatio, PoissonCI::default_parametric(),
  PartialRetarget(0.3)>`** closes the cascade: EWMA smooths the
  spike before it reaches the boundary, PoissonCI keeps stable-load
  false-fires near zero, PartialRetarget bounds the magnitude of any
  fire that does happen.

This composition argument is the empirical demonstration that the
four-axis vocabulary cuts the design space at the right joints:
distinct axis changes close distinct failure modes, and composability
is what makes a structurally tame algorithm reachable.

## Determinism

Every trial is fully deterministic given its `(config, schedule,
seed)` triple. Re-runs produce byte-identical output across machines
and time, modulo Rust toolchain version changes that affect
floating-point determinism. The `Trial` struct carries the seed it
was produced from, so any failing trial can be reproduced in
isolation via `cargo run --release --bin trace-trial --
--seed 0x...`.

## Production migration

The production `Vardiff` trait, `Clock` injection, and
`channels_sv2::vardiff::*` public surface are unchanged by the
framework. `Composed<E, S, B, U>` already implements `Vardiff`, so
the recommended `FullRemedy` composition is a drop-in replacement at
production call sites:

```rust
let vardiff = Composed::new(
    EwmaEstimator::new(120),
    AbsoluteRatio,
    PoissonCI::default_parametric(),
    PartialRetarget::new(0.3),
    min_hashrate,
    clock,
);
```

The migration path:

1. Promote the four traits + the `Composed<E, S, B, U>` adapter +
   the per-axis impls out of `sim/src/composed/` into
   `channels_sv2::vardiff`. They are stable and round-tripped
   through ~190 tests including the fire-for-fire `VardiffState`
   equivalence suite.
2. Add a factory on `VardiffState` (or a new constructor) that
   returns the `FullRemedy` composition. Keep the existing
   `VardiffState::new_with_clock` available so gradual migration is
   possible.
3. Update production tests that depended on the specific fire
   trajectory of the classic algorithm. The sim crate's regression
   baseline shifts from `baseline_VardiffState.toml` to
   `baseline_FullRemedy.toml`.
4. Document operator-facing behavioral changes: the ~2.7-fires/hour
   stable-load jitter floor at SPM=6 (active tracking rather than
   silent drift), and the mild negative cold-start bias.
5. Wire the regression harness into CI:
   `cargo test --release --lib -- --ignored` on each PR, against the
   per-metric tolerance budgets declared by each `Metric` impl.
