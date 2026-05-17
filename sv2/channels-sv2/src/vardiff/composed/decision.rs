//! Decision record — a snapshot of the per-tick state inside
//! [`super::Composed::try_vardiff`].
//!
//! Carried as `last_decision: Option<DecisionRecord>` on every
//! `Composed` instance. Production code can ignore the field; the
//! `vardiff_sim` crate's `Observable` extension trait reads it to
//! populate the per-tick `delta` / `threshold` / `h_estimate` columns
//! in the characterization output.

/// The per-tick algorithm state captured by `Composed::try_vardiff`
/// just after δ and θ are computed.
#[derive(Debug, Clone, Copy)]
pub struct DecisionRecord {
    /// Test statistic at this tick.
    pub delta: f64,
    /// Decision threshold at this tick.
    pub threshold: f64,
    /// Estimator's belief about miner hashrate at this tick.
    pub h_estimate: f32,
}
