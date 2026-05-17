//! Classic implementation test suite.
//!
//! These tests deliberately exercise the historical `VardiffState`
//! threshold-ladder algorithm via its deprecated constructors
//! (`new_with_min` etc.). The deprecation warnings nudge new
//! *production* code toward `channels_sv2::vardiff::default()` —
//! they are correctly suppressed here because the explicit purpose
//! of this module is to verify classic-algorithm behavior.
#![allow(deprecated)]

use crate::vardiff::clock::MockClock;
use crate::vardiff::test::{
    simulate_shares_and_wait, TEST_MIN_ALLOWED_HASHRATE, TEST_SHARES_PER_MINUTE,
};
use crate::{target::hash_rate_to_target, vardiff::VardiffError, VardiffState};
use std::sync::Arc;

use super::{
    test_increment_and_reset_shares, test_try_vardiff_low_hashrate_decrease_target,
    test_try_vardiff_no_shares_30_to_60s_decrease,
    test_try_vardiff_no_shares_less_than_30s_decrease,
    test_try_vardiff_no_shares_more_than_60s_decrease,
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change,
    test_try_vardiff_with_less_spm_than_expected, test_try_vardiff_with_shares_30_to_60s,
    test_try_vardiff_with_shares_less_than_30, test_try_vardiff_with_shares_more_than_60s, Vardiff,
};

fn new_test_vardiff_state() -> Result<VardiffState, VardiffError> {
    VardiffState::new_with_min(TEST_MIN_ALLOWED_HASHRATE)
}

#[test]
fn test_initialization_and_getters() {
    let vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");

    assert_eq!(vardiff.min_allowed_hashrate(), TEST_MIN_ALLOWED_HASHRATE);
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

#[test]
fn test_increment_and_reset_shares_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_increment_and_reset_shares(&mut vardiff)
}

#[test]
fn test_try_vardiff_stable_hashrate_minimal_change_or_no_change_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_low_hashrate_decrease_target_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_low_hashrate_decrease_target(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_less_than_30_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_with_shares_less_than_30(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_30_to_60s_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_with_shares_30_to_60s(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_more_than_60s_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_with_shares_more_than_60s(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_30_to_60s_decrease_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_no_shares_30_to_60s_decrease(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_more_than_60s_decrease_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_no_shares_more_than_60s_decrease(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_less_than_30s_decrease_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_no_shares_less_than_30s_decrease(&mut vardiff);
}

#[test]
fn test_try_vardiff_with_less_spm_than_expected_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_with_less_spm_than_expected(&mut vardiff);
}

#[test]
fn test_try_vardiff_hashrate_clamps_to_minimum() {
    let hashrate = TEST_MIN_ALLOWED_HASHRATE * 1.5;
    let target = hash_rate_to_target(hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();

    let mut vardiff = VardiffState::new_with_min(TEST_MIN_ALLOWED_HASHRATE)
        .expect("Failed to create VardiffState");

    let simulation_duration_secs = 16;
    simulate_shares_and_wait(&mut vardiff, 0, simulation_duration_secs);

    let result = vardiff
        .try_vardiff(hashrate, &target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(result.is_some(), "Hashrate should update");
    let new_hashrate = result.unwrap();

    assert_eq!(
        new_hashrate, TEST_MIN_ALLOWED_HASHRATE,
        "Hashrate should be clamped to minimum"
    );
    assert_eq!(
        new_hashrate, TEST_MIN_ALLOWED_HASHRATE,
        "Stored hashrate should be clamped"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Verifies that `VardiffState::new_with_clock` actually wires the provided clock
// into the algorithm — i.e., timestamps come from the injected clock, not from
// `SystemTime::now()`. This is the integration the simulation framework relies on.
#[test]
fn test_vardiff_state_reads_from_injected_clock() {
    let clock = Arc::new(MockClock::new(1_700_000_000));
    let mut vardiff = VardiffState::new_with_clock(TEST_MIN_ALLOWED_HASHRATE, clock.clone())
        .expect("construction with mock clock should succeed");

    // Initial timestamp must match the mock's starting value, not wall clock.
    assert_eq!(vardiff.last_update_timestamp(), 1_700_000_000);

    // Advancing the mock advances what the algorithm sees as "now."
    // reset_counter reads the clock and stores the new timestamp.
    clock.advance(60);
    vardiff
        .reset_counter()
        .expect("reset_counter should succeed");
    assert_eq!(vardiff.last_update_timestamp(), 1_700_000_060);

    // A larger advance is also observable.
    clock.advance(3600);
    vardiff
        .reset_counter()
        .expect("reset_counter should succeed");
    assert_eq!(vardiff.last_update_timestamp(), 1_700_003_660);
}

// Verifies that `try_vardiff`'s `delta_time` computation reads from the injected
// clock. With the mock clock advanced by exactly 16s after reset, delta_time
// crosses the `delta_time <= 15` early-return guard and the algorithm proceeds
// to evaluate. With zero shares the algorithm hits the realized==0 branch and
// returns a reduced hashrate (or clamps to minimum).
#[test]
fn test_try_vardiff_uses_injected_clock_for_delta_time() {
    let clock = Arc::new(MockClock::new(0));
    let initial_hashrate: f32 = 1_000_000.0;
    let target = hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    let mut vardiff = VardiffState::new_with_clock(TEST_MIN_ALLOWED_HASHRATE, clock.clone())
        .expect("construction with mock clock should succeed");

    // Advance below the 15s early-return threshold; algorithm should return None.
    clock.advance(10);
    let result = vardiff
        .try_vardiff(initial_hashrate, &target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_none(),
        "try_vardiff should early-return when delta_time <= 15s"
    );

    // Advance past the threshold; algorithm should now proceed and (with
    // 0 shares observed) fire a downward adjustment via the realized==0 branch.
    clock.advance(60); // total 70s elapsed since last reset
    let result = vardiff
        .try_vardiff(initial_hashrate, &target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "try_vardiff should fire with delta_time > 15s and realized rate == 0"
    );
}
