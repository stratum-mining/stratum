//! End-to-end behavioral test suite for the CLASSIC control law.
//!
//! History (why this file exists and what it tests): the generic
//! `test_try_vardiff_*` harness in [`super`] asserts the classic algorithm's
//! single-window retarget magnitudes (`dt≤30 → ×10`, `dt<60 → ×5`, `dt≥60 →
//! ×3`; no-shares `÷1.5`/`÷2`/`÷3`) and carries the U512 precision-fix
//! regression (see `test_try_vardiff_with_less_spm_than_expected`). It used to
//! be instantiated against `VardiffState`. `VardiffState` now delegates to the
//! shipped *champion* (`composed::champion_composed`), whose control law is an
//! EWMA estimator + sign-persistence CUSUM boundary that **accumulates evidence
//! across ticks before firing** — it has no single-window multiplier table, and
//! a single-shot `try_vardiff` would (correctly) return `None`. So the harness's
//! magnitude assertions describe the *classic* law specifically.
//!
//! The classic law still exists as [`composed::classic_composed`] — the sim
//! crate's comparison anchor ("the algorithm we used to run"). This suite
//! re-points the harness at its true subject, restoring the end-to-end coverage
//! the rewrite removed. The champion's structurally-correct end-to-end coverage
//! lives in the multi-tick simulation regression (`sim/` crate), not here: a
//! single-shot harness cannot honestly exercise an evidence-accumulating
//! controller.

use std::sync::Arc;

use crate::vardiff::clock::SystemClock;
use crate::vardiff::composed::{classic_composed, ClassicComposed};
use crate::vardiff::test::{
    simulate_shares_and_wait, TEST_MIN_ALLOWED_HASHRATE, TEST_SHARES_PER_MINUTE,
};
use crate::{target::hash_rate_to_target, Vardiff};

use super::{
    test_increment_and_reset_shares, test_try_vardiff_low_hashrate_decrease_target,
    test_try_vardiff_no_shares_30_to_60s_decrease,
    test_try_vardiff_no_shares_less_than_30s_decrease,
    test_try_vardiff_no_shares_more_than_60s_decrease,
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change,
    test_try_vardiff_with_less_spm_than_expected, test_try_vardiff_with_shares_30_to_60s,
    test_try_vardiff_with_shares_less_than_30, test_try_vardiff_with_shares_more_than_60s,
};

fn new_test_classic() -> ClassicComposed {
    classic_composed(TEST_MIN_ALLOWED_HASHRATE, Arc::new(SystemClock))
}

#[test]
fn test_initialization_and_getters() {
    let vardiff = new_test_classic();

    assert_eq!(vardiff.min_allowed_hashrate(), TEST_MIN_ALLOWED_HASHRATE);
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

#[test]
fn test_increment_and_reset_shares_classic() {
    let mut vardiff = new_test_classic();
    test_increment_and_reset_shares(&mut vardiff)
}

#[test]
fn test_try_vardiff_stable_hashrate_minimal_change_or_no_change_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_stable_hashrate_minimal_change_or_no_change(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_low_hashrate_decrease_target_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_low_hashrate_decrease_target(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_less_than_30_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_with_shares_less_than_30(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_30_to_60s_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_with_shares_30_to_60s(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_with_shares_more_than_60s_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_with_shares_more_than_60s(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_30_to_60s_decrease_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_no_shares_30_to_60s_decrease(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_more_than_60s_decrease_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_no_shares_more_than_60s_decrease(&mut vardiff);
}

#[test]
pub fn test_try_vardiff_no_shares_less_than_30s_decrease_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_no_shares_less_than_30s_decrease(&mut vardiff);
}

#[test]
fn test_try_vardiff_with_less_spm_than_expected_classic() {
    let mut vardiff = new_test_classic();
    test_try_vardiff_with_less_spm_than_expected(&mut vardiff);
}

#[test]
fn test_try_vardiff_hashrate_clamps_to_minimum() {
    let hashrate = TEST_MIN_ALLOWED_HASHRATE * 1.5;
    let target = hash_rate_to_target(hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();

    let mut vardiff = new_test_classic();

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
    assert_eq!(vardiff.shares_since_last_update(), 0);
}
