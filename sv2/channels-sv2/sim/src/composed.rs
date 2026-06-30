//! Sim-side facade over [`channels_sv2::vardiff::composed`].
//!
//! Re-exports the production pipeline types and adds the sim-only
//! [`crate::trial::Observable`] extension trait to `Composed<E, B, U>`.

pub use channels_sv2::vardiff::composed::*;

use crate::trial::Observable;

impl<E, B, U> Observable for Composed<E, B, U>
where
    E: Estimator,
    B: Boundary,
    U: UpdateRule,
{
    fn last_decision(&self) -> Option<crate::trial::DecisionRecord> {
        self.last_decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::HashrateSchedule;
    use crate::trial::{run_trial, TrialConfig};
    use channels_sv2::vardiff::MockClock;
    use channels_sv2::VardiffState;
    use std::sync::Arc;

    #[test]
    fn vardiff_state_delegates_to_composed_and_produces_fires() {
        // VardiffState now wraps AdaCUSUM internally. Verify it
        // actually fires during a cold-start scenario (basic smoke test).
        let clock = Arc::new(MockClock::new(0));
        let state = VardiffState::new_with_clock(1.0, clock.clone()).unwrap();
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        };
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial(state, clock, config, &schedule, 0xCAFE);
        let fires = trial.fires();
        assert!(
            fires.len() >= 3,
            "VardiffState (AdaCUSUM) should fire multiple times during cold start, got {}",
            fires.len()
        );
        // Final hashrate should be in the right ballpark (within 2x of truth)
        let ratio = trial.final_hashrate as f64 / 1.0e15;
        assert!(
            ratio > 0.5 && ratio < 2.0,
            "Final hashrate should be near truth, got ratio {}",
            ratio
        );
    }
}
