/// Classic implementation test suite
use crate::vardiff::test::{
    simulate_shares_and_wait, TEST_INITIAL_HASHRATE, TEST_MIN_ALLOWED_HASHRATE,
    TEST_SHARES_PER_MINUTE,
};
use crate::{vardiff::VardiffError, VardiffState};

use super::{
    test_increment_and_reset_shares, test_set_hashrate_updates_target,
    test_try_vardiff_high_hashrate_decrease_target, test_try_vardiff_low_hashrate_increase_target,
    test_try_vardiff_no_change_short_interval,
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change, Vardiff,
};

fn new_test_vardiff_state() -> Result<VardiffState, VardiffError> {
    VardiffState::new_with_min(
        TEST_SHARES_PER_MINUTE,
        TEST_INITIAL_HASHRATE,
        TEST_MIN_ALLOWED_HASHRATE,
    )
}

#[test]
fn test_initialization_and_getters() {
    let vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");

    assert_eq!(vardiff.hashrate(), TEST_INITIAL_HASHRATE);
    assert_eq!(vardiff.shares_per_minute(), TEST_SHARES_PER_MINUTE);
    assert_eq!(vardiff.min_allowed_hashrate(), TEST_MIN_ALLOWED_HASHRATE);
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

#[test]
fn test_set_hashrate_updates_target_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_set_hashrate_updates_target(&mut vardiff);
}

#[test]
fn test_increment_and_reset_shares_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_increment_and_reset_shares(&mut vardiff)
}

#[test]
fn test_try_vardiff_no_change_short_interval_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_no_change_short_interval(&mut vardiff)
}

#[test]
fn test_try_vardiff_stable_hashrate_minimal_change_or_no_change_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_low_hashrate_increase_target_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_low_hashrate_increase_target(&mut vardiff);
}

#[test]
fn test_try_vardiff_no_shares_long_period_drastic_decrease() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    let initial_hashrate = vardiff.hashrate();

    let simulation_duration = 250;
    simulate_shares_and_wait(&mut vardiff, 0, simulation_duration);

    let result = vardiff.try_vardiff().expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update after long period of no shares"
    );
    let new_hashrate = result.unwrap();

    assert!(
        new_hashrate < initial_hashrate / 2.9 && new_hashrate > initial_hashrate / 3.1
            || new_hashrate == TEST_MIN_ALLOWED_HASHRATE,
        "Hashrate should be roughly initial /3 or clamped. Got: {}",
        new_hashrate
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

#[test]
pub fn test_try_vardiff_high_hashrate_decrease_target_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_try_vardiff_high_hashrate_decrease_target(&mut vardiff);
}
