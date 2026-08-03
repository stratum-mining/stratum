/// Classic implementation test suite
use crate::vardiff::test::{
    simulate_shares_and_wait, TEST_MIN_ALLOWED_HASHRATE, TEST_SHARES_PER_MINUTE,
};
use crate::{target::hash_rate_to_target, vardiff::VardiffError, VardiffState};

use super::{
    test_backwards_clock_step_reanchors_window, test_increment_and_reset_shares,
    test_try_vardiff_low_hashrate_decrease_target, test_try_vardiff_no_shares_30_to_60s_decrease,
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
fn test_backwards_clock_step_reanchors_window_classic() {
    let mut vardiff = new_test_vardiff_state().expect("Failed to create VardiffState");
    test_backwards_clock_step_reanchors_window(&mut vardiff);
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

// Every unusable prior hashrate must fall back to the configured floor. `hashrate` is
// the divisor for the delta percentage and the base every special-case cap scales, so
// without the fallback: `0.0` pins to the floor forever (`0.0 * 10.0` is absorbing),
// a value below the floor pins to it for the round, `NaN` makes every `should_update`
// arm compare false, and `inf` makes the percentage `inf / inf == NaN` with the same
// effect. A miner picks this value via `nominal_hash_rate` at channel open.
#[test]
fn test_try_vardiff_unusable_prior_hashrate_falls_back_to_floor() {
    for bad in [0.0_f32, 0.01, -5.0, f32::NAN, f32::INFINITY] {
        let mut vardiff = VardiffState::new_with_min(TEST_MIN_ALLOWED_HASHRATE)
            .expect("Failed to create VardiffState");

        let target = hash_rate_to_target(1000.0_f64, TEST_SHARES_PER_MINUTE.into()).unwrap();

        // 1000 shares in 16s => 3750 shares/min, far above TEST_SHARES_PER_MINUTE.
        simulate_shares_and_wait(&mut vardiff, 1000, 16);

        match vardiff.try_vardiff(bad, &target, TEST_SHARES_PER_MINUTE) {
            Ok(Some(v)) => assert!(
                v.is_finite() && v > TEST_MIN_ALLOWED_HASHRATE * 1.5,
                "prior hashrate {bad} pinned difficulty to the floor: {v}"
            ),
            other => panic!("prior hashrate {bad} silently disabled vardiff: {other:?}"),
        }
    }
}

// `min_allowed_hashrate` is a `pub` field, so the validation in `new_with_min` can be
// bypassed by direct assignment; `try_vardiff` must sanitize the floor itself before
// relying on it as the fallback baseline and the output clamp.
#[test]
fn test_try_vardiff_sanitizes_unusable_floor() {
    for bad_floor in [0.0_f32, -5.0, f32::NAN, f32::INFINITY] {
        let mut vardiff = VardiffState::new_with_min(TEST_MIN_ALLOWED_HASHRATE)
            .expect("Failed to create VardiffState");
        vardiff.min_allowed_hashrate = bad_floor;

        let target = hash_rate_to_target(1000.0_f64, TEST_SHARES_PER_MINUTE.into()).unwrap();

        // 1000 shares in 16s => 3750 shares/min, far above TEST_SHARES_PER_MINUTE.
        simulate_shares_and_wait(&mut vardiff, 1000, 16);

        // prior hashrate 0.0 forces the fallback onto the (unusable) floor
        match vardiff.try_vardiff(0.0, &target, TEST_SHARES_PER_MINUTE) {
            Ok(Some(v)) => assert!(
                v.is_finite() && v > 0.0,
                "floor {bad_floor} produced unusable hashrate {v}"
            ),
            other => panic!("floor {bad_floor} silently disabled vardiff: {other:?}"),
        }
    }
}

// The floor itself is the fallback baseline used by `try_vardiff`, so a non-positive or
// non-finite `min_allowed_hashrate` would reintroduce the division by zero.
#[test]
fn test_new_with_min_rejects_unusable_floor() {
    for bad in [0.0_f32, -5.0, f32::NAN, f32::INFINITY] {
        let vardiff = VardiffState::new_with_min(bad).expect("Failed to create VardiffState");
        let min = vardiff.min_allowed_hashrate();
        assert!(
            min.is_finite() && min > 0.0,
            "min_allowed_hashrate {min} from input {bad} is unusable as a baseline"
        );
    }
}
