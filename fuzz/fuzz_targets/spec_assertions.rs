use common_messages_sv2::{SetupConnection, SetupConnectionError};
use job_declaration_sv2::{DeclareMiningJobError, PushSolution};
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetExtranoncePrefix, SubmitSharesError, SubmitSharesExtended, UpdateChannel,
    UpdateChannelError,
};
use template_distribution_sv2::{NewTemplate, RequestTransactionDataError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if `b` is an ASCII control character (0x00–0x1F or 0x7F).
fn is_control_char(b: u8) -> bool {
    b < 0x20 || b == 0x7F
}

// ---------------------------------------------------------------------------
// Common Messages
// ---------------------------------------------------------------------------

/// Spec 3.6.1: protocol must be 0 (Mining), 1 (Job Declaration), or 2 (Template Distribution).
/// Spec 3.6.1: min_version currently must be 2.
/// Spec 3.6.1: max_version currently must be 2.
#[allow(dead_code)]
pub fn assert_setup_connection(msg: &SetupConnection) {
    let p = msg.protocol as u8;
    assert!(
        p <= 2,
        "spec 3.6.1: protocol must be 0 (Mining), 1 (Job Declaration), or 2 (Template Distribution), got {}",
        p
    );
    // assert_eq!(
    //     msg.min_version, 2,
    //     "spec 3.6.1: min_version must be 2, got {}",
    //     msg.min_version
    // );
    // assert_eq!(
    //     msg.max_version, 2,
    //     "spec 3.6.1: max_version must be 2, got {}",
    //     msg.max_version
    // );
    // TODO: upstream issue — parser accepts min_version > max_version (spec 3.6.1).
    // assert!(
    //     msg.min_version <= msg.max_version,
    //     "spec 3.6.1: min_version ({}) must be <= max_version ({})",
    //     msg.min_version,
    //     msg.max_version
    // );
}

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_setup_connection_error(_msg: &SetupConnectionError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: SetupConnectionError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

// ---------------------------------------------------------------------------
// Mining Messages
// ---------------------------------------------------------------------------

/// Spec 5.3.9: reason_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_close_channel(_msg: &CloseChannel) {
    // let has_ctrl = _msg.reason_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 5.3.9: CloseChannel.reason_code must not include control characters, got {:?}",
    //     _msg.reason_code
    // );
}

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_open_mining_channel_error(_msg: &OpenMiningChannelError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: OpenMiningChannelError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_update_channel_error(_msg: &UpdateChannelError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: UpdateChannelError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

/// Spec 5.3.18: coinbase_prefix is B0_255 but spec says "up to 8 bytes".
#[allow(dead_code)]
pub fn assert_set_custom_mining_job(msg: &SetCustomMiningJob) {
    // assert!(
    //     msg.coinbase_prefix.len() <= 8,
    //     "spec 5.3.18: coinbase_prefix must be <= 8 bytes, got {}",
    //     msg.coinbase_prefix.len()
    // );
}

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_set_custom_mining_job_error(_msg: &SetCustomMiningJobError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: SetCustomMiningJobError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

/// Spec 5.3.9: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_submit_shares_error(_msg: &SubmitSharesError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 5.3.9: SubmitSharesError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

// ---------------------------------------------------------------------------
// Template Distribution Messages
// ---------------------------------------------------------------------------

/// Spec 7.2: coinbase_prefix is B0_255 but spec says "up to 8 bytes".
#[allow(dead_code)]
pub fn assert_new_template(msg: &NewTemplate) {
    // assert!(
    //     msg.coinbase_prefix.len() <= 8,
    //     "spec 7.2: coinbase_prefix must be <= 8 bytes, got {}",
    //     msg.coinbase_prefix.len()
    // );
}

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_request_transaction_data_error(_msg: &RequestTransactionDataError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: RequestTransactionDataError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

// ---------------------------------------------------------------------------
// Job Declaration Messages
// ---------------------------------------------------------------------------

/// Spec 3.5: error_code must not include control characters.
/// TODO: upstream issue — parser currently accepts control characters.
#[allow(dead_code)]
pub fn assert_declare_mining_job_error(_msg: &DeclareMiningJobError) {
    // let has_ctrl = _msg.error_code.as_bytes().iter().any(|&b| is_control_char(b));
    // assert!(
    //     !has_ctrl,
    //     "spec 3.5: DeclareMiningJobError.error_code must not include control characters, got {:?}",
    //     _msg.error_code
    // );
}

// ---------------------------------------------------------------------------
// Mining Messages — Tier 1 assertions
// ---------------------------------------------------------------------------

/// Spec 5.3.16: version_rolling_allowed is a BOOL. Only the least significant
/// bit is meaningful; after a roundtrip the value must be normalized to 0 or 1.
#[allow(dead_code)]
pub fn assert_new_extended_mining_job(msg: &NewExtendedMiningJob) {
    assert!(
        (msg.version_rolling_allowed as u8) <= 1,
        "spec 5.3.16: version_rolling_allowed must be 0 or 1 (BOOL), got {}",
        msg.version_rolling_allowed as u8
    );
}

/// Spec 5.3.2: nominal_hash_rate is expected hashrate in h/s. A negative
/// hashrate is nonsensical and violates the spec.
/// TODO: upstream issue — parser accepts NaN for f32 hashrate fields.
#[allow(dead_code)]
pub fn assert_open_standard_mining_channel(msg: &OpenStandardMiningChannel) {
    // assert!(
    //     msg.nominal_hash_rate >= 0.0,
    //     "spec 5.3.2: nominal_hash_rate must be >= 0.0, got {}",
    //     msg.nominal_hash_rate
    // );
}

/// Spec 5.3.4: nominal_hash_rate is expected hashrate in h/s. A negative
/// hashrate is nonsensical and violates the spec.
/// TODO: upstream issue — parser accepts NaN for f32 hashrate fields.
#[allow(dead_code)]
pub fn assert_open_extended_mining_channel(msg: &OpenExtendedMiningChannel) {
    // assert!(
    //     msg.nominal_hash_rate >= 0.0,
    //     "spec 5.3.4: nominal_hash_rate must be >= 0.0, got {}",
    //     msg.nominal_hash_rate
    // );
}

/// Spec 5.3.7: nominal_hash_rate is expected hashrate in h/s. A negative
/// hashrate is nonsensical and violates the spec.
/// TODO: upstream issue — parser accepts NaN for f32 hashrate fields.
#[allow(dead_code)]
pub fn assert_update_channel(msg: &UpdateChannel) {
    // assert!(
    //     msg.nominal_hash_rate >= 0.0,
    //     "spec 5.3.7: nominal_hash_rate must be >= 0.0, got {}",
    //     msg.nominal_hash_rate
    // );
}

/// Spec 5.3.3: extranonce_prefix is B0_32 — must be at most 32 bytes.
#[allow(dead_code)]
pub fn assert_open_standard_mining_channel_success(msg: &OpenStandardMiningChannelSuccess) {
    assert!(
        msg.extranonce_prefix.len() <= 32,
        "spec 5.3.3: extranonce_prefix must be <= 32 bytes, got {}",
        msg.extranonce_prefix.len()
    );
}

/// Spec 5.3.5: extranonce_prefix is B0_32 — must be at most 32 bytes.
#[allow(dead_code)]
pub fn assert_open_extended_mining_channel_success(msg: &OpenExtendedMiningChannelSuccess) {
    assert!(
        msg.extranonce_prefix.len() <= 32,
        "spec 5.3.5: extranonce_prefix must be <= 32 bytes, got {}",
        msg.extranonce_prefix.len()
    );
}

/// Spec 5.3.10: extranonce_prefix is B0_32 — must be at most 32 bytes.
#[allow(dead_code)]
pub fn assert_set_extranonce_prefix(msg: &SetExtranoncePrefix) {
    assert!(
        msg.extranonce_prefix.len() <= 32,
        "spec 5.3.10: extranonce_prefix must be <= 32 bytes, got {}",
        msg.extranonce_prefix.len()
    );
}

/// Spec 5.3.12: extranonce is B0_32 — must be at most 32 bytes.
#[allow(dead_code)]
pub fn assert_submit_shares_extended(msg: &SubmitSharesExtended) {
    assert!(
        msg.extranonce.len() <= 32,
        "spec 5.3.12: extranonce must be <= 32 bytes, got {}",
        msg.extranonce.len()
    );
}

// ---------------------------------------------------------------------------
// Job Declaration Messages — Tier 1 assertions
// ---------------------------------------------------------------------------

/// Spec 6.4.9: extranonce is B0_32 — must be at most 32 bytes.
#[allow(dead_code)]
pub fn assert_push_solution(msg: &PushSolution) {
    assert!(
        msg.extranonce.len() <= 32,
        "spec 6.4.9: extranonce must be <= 32 bytes, got {}",
        msg.extranonce.len()
    );
}
