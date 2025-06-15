/// Classic implementation test suite
use crate::vardiff::test::{
    simulate_shares_and_wait, TEST_MIN_ALLOWED_HASHRATE, TEST_SHARES_PER_MINUTE,
};
use crate::{utils::hash_rate_to_target, vardiff::VardiffError, VardiffState};

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
