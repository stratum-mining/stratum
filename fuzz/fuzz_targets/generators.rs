//! These byte-sequence generators are based on the Stratum V2 specification.
//!
//! # Wire-valid only
//!
//! These generators produce **wire-format-valid** byte sequences, not
//! protocol-semantically valid messages. The output MUST always be parseable
//! by the corresponding `from_bytes` implementation for each type. Field
//! values may be intentionally non-spec-compliant, since the reference
//! implementation does not enforce protocol-level semantic constraints during
//! serialization or deserialization.
//!
//! If parsing fails, it may indicate a bug in the SRI implementation or an
//! issue in the generators themselves.
//!
//! Field layouts follow the current main branch of the Stratum V2
//! specification at revision: 025e2f76802d5942a10f6f3fea07941522b108ca.
//! Field comments use the specification's field names.
// Every fuzz target compiles this whole module, so generators used by only some
// targets show up as dead code in the others.
#![allow(dead_code)]

use arbitrary::Unstructured;

/// Generates a random-length byte payload with a maximum size of `max`.
///
/// The actual payload length is chosen by the fuzzer between 0 and `max`.
/// Choosing the length consumes input bytes, so the chosen length is clamped
/// to what remains afterward. This prevents requesting more bytes than are
/// available and avoids unnecessary parsing failures.
fn gen_var_payload(u: &mut Unstructured, max: usize) -> arbitrary::Result<Vec<u8>> {
    let cap = u.len().min(max);
    let len = u.int_in_range(0..=cap)?.min(u.len());
    Ok(u.bytes(len)?.to_vec())
}

// ---------------------------------------------------------------------------
// Fixed-size primitive (LE encoding)
// ---------------------------------------------------------------------------
/// Generates a single arbitrary byte.
pub fn gen_u8(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u8>()?.to_le_bytes().to_vec())
}

/// Generates a `u16` in little-endian wire format.
pub fn gen_u16(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u16>()?.to_le_bytes().to_vec())
}

/// Generates a `u32` in little-endian wire format.
pub fn gen_u32(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u32>()?.to_le_bytes().to_vec())
}

/// Generates a `u64` in little-endian wire format.
pub fn gen_u64(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u64>()?.to_le_bytes().to_vec())
}

/// Generates an `f32` in little-endian wire format.
pub fn gen_f32(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<f32>()?.to_le_bytes().to_vec())
}

// the bool type has a weird behavior where we should only look at the
// LEAST SIGNIFICANT BIT; but this behavior will be fuzzed in the macros.
// Here I just generate canonical u8;
/// Generates a canonical `bool` as a single `0` or `1` byte.
pub fn gen_bool(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(vec![u.arbitrary::<bool>()? as u8])
}

/// Generates a `U24` in three-byte little-endian wire format.
pub fn gen_u24(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let raw = u.int_in_range(0u32..=0x00FF_FFFF)?;
    let b = raw.to_le_bytes();
    Ok(vec![b[0], b[1], b[2]])
}

/// Generates a `U256` as 32 arbitrary bytes.
pub fn gen_u256(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 32]>()?.to_vec())
}

/// Generates a `PubKey` as 32 arbitrary bytes.
pub fn gen_pubkey(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 32]>()?.to_vec())
}

/// Generates a `Signature` as 64 arbitrary bytes.
pub fn gen_signature(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 64]>()?.to_vec())
}

// ---------------------------------------------------------------------------
// Variable-length byte arrays (length-prefixed)
// ---------------------------------------------------------------------------

/// Generates a `B032` with a one-byte length prefix.
pub fn gen_b032(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 32)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Generates a `B0255` with a one-byte length prefix.
pub fn gen_b0255(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 255)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Generates a `Str0255` with a one-byte length prefix.
pub fn gen_str0255(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 255)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Capped at 512 bytes of payload to keep fuzzer corpus sizes manageable.
pub fn gen_b064k(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 512)?;
    let len = payload.len() as u16;
    let mut buf = Vec::with_capacity(2 + payload.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Capped at 100 bytes of payload to keep fuzzer corpus sizes manageable.
pub fn gen_b016m(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 100)?;
    let len = payload.len() as u32;
    let b = len.to_le_bytes();
    let mut buf = Vec::with_capacity(3 + payload.len());
    buf.extend_from_slice(&b[..3]); // 3 bytes LE
    buf.extend_from_slice(&payload);
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Nested Types
// ---------------------------------------------------------------------------

/// Generates a `Seq0255` of up to `max_count` generated elements.
pub fn gen_seq0255(
    u: &mut Unstructured,
    gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
    max_count: usize,
) -> arbitrary::Result<Vec<u8>> {
    let mut gen_elem = gen_elem;
    let count = u.int_in_range(0..=max_count.min(255))?;
    let mut buf = Vec::with_capacity(1 + count);
    buf.push(count as u8);
    for _ in 0..count {
        buf.extend_from_slice(&gen_elem(u)?);
    }
    Ok(buf)
}

/// Generates a `Seq064K` of up to `max_count` generated elements.
pub fn gen_seq064k(
    u: &mut Unstructured,
    gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
    max_count: usize,
) -> arbitrary::Result<Vec<u8>> {
    let mut gen_elem = gen_elem;
    let count = u.int_in_range(0..=max_count.min(65535))?;
    let mut buf = Vec::with_capacity(2 + count);
    buf.extend_from_slice(&(count as u16).to_le_bytes());
    for _ in 0..count {
        buf.extend_from_slice(&gen_elem(u)?);
    }
    Ok(buf)
}

/// Generates an `Sv2Option`, optionally followed by the generated value.
pub fn gen_sv2_option(
    u: &mut Unstructured,
    mut gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
) -> arbitrary::Result<Vec<u8>> {
    if u.arbitrary::<bool>()? {
        let mut buf = vec![1u8];
        buf.extend_from_slice(&gen_elem(u)?);
        Ok(buf)
    } else {
        Ok(vec![0u8])
    }
}

// ============================================================================
// Common Messages
// ============================================================================

/// Generates a wire-valid `SetupConnection` message.
pub fn gen_setup_connection(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(128);
    // `protocol` is a wire-level enum: the decoder only accepts 0, 1, or 2.
    buf.push(u.int_in_range(0u8..=2)?); // protocol

    // Versions are intentionally arbitrary: the parser accepts any u16, which
    // exercises deserialization of non-spec-compliant values.
    buf.extend_from_slice(&gen_u16(u)?); // min_version
    buf.extend_from_slice(&gen_u16(u)?); // max_version
    buf.extend_from_slice(&gen_u32(u)?); // flags
    buf.extend_from_slice(&gen_str0255(u)?); // endpoint_host
    buf.extend_from_slice(&gen_u16(u)?); // endpoint_port
    buf.extend_from_slice(&gen_str0255(u)?); // vendor
    buf.extend_from_slice(&gen_str0255(u)?); // hardware_version
    buf.extend_from_slice(&gen_str0255(u)?); // firmware
    buf.extend_from_slice(&gen_str0255(u)?); // device_id
    Ok(buf)
}

/// Generates a wire-valid `SetupConnectionError` message.
pub fn gen_setup_connection_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // flags
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    Ok(buf)
}

/// Generates a wire-valid `SetupConnectionSuccess` message.
pub fn gen_setup_connection_success(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(6);
    buf.extend_from_slice(&gen_u16(u)?); // used_version
    buf.extend_from_slice(&gen_u32(u)?); // flags
    Ok(buf)
}

/// Generates a wire-valid `Reconnect` message.
pub fn gen_reconnect(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_str0255(u)?); // new_host
    buf.extend_from_slice(&gen_u16(u)?); // new_port
    Ok(buf)
}

/// Generates a wire-valid `ChannelEndpointChanged` message.
pub fn gen_channel_endpoint_changed(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    gen_u32(u) // channel_id
}

// ============================================================================
// Job Declaration Messages
// ============================================================================

/// Generates a wire-valid `AllocateMiningJobToken` message.
pub fn gen_allocate_mining_job_token(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_str0255(u)?); // user_identifier
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    Ok(buf)
}

/// Generates a wire-valid `AllocateMiningJobTokenSuccess` message.
pub fn gen_allocate_mining_job_token_success(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_b0255(u)?); // mining_job_token
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_outputs
    Ok(buf)
}

/// Generates a wire-valid `DeclareMiningJob` message.
pub fn gen_declare_mining_job(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_b0255(u)?); // mining_job_token
    buf.extend_from_slice(&gen_u32(u)?); // version
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_prefix
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_suffix
    buf.extend_from_slice(&gen_seq064k(u, gen_u256, 256)?); // wtxid_list
    buf.extend_from_slice(&gen_b064k(u)?); // excess_data
    Ok(buf)
}

/// Generates a wire-valid `DeclareMiningJobSuccess` message.
pub fn gen_declare_mining_job_success(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_b0255(u)?); // new_mining_job_token
    Ok(buf)
}

/// Generates a wire-valid `DeclareMiningJobError` message.
pub fn gen_declare_mining_job_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    buf.extend_from_slice(&gen_b064k(u)?); // error_details
    Ok(buf)
}

/// Generates a wire-valid `ProvideMissingTransactions` message.
pub fn gen_provide_missing_transactions(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_seq064k(u, gen_u16, 256)?); // unknown_tx_position_list
    Ok(buf)
}

/// Generates a wire-valid `ProvideMissingTransactionsSuccess` message.
pub fn gen_provide_missing_transactions_success(
    u: &mut Unstructured,
) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id

    // Capped count to keep fuzzer corpus sizes manageable.
    buf.extend_from_slice(&gen_seq064k(u, gen_b016m, 10)?); // transaction_list
    Ok(buf)
}

/// Generates a wire-valid `PushSolution` message.
pub fn gen_push_solution(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(52);
    buf.extend_from_slice(&gen_b032(u)?); // extranonce
    buf.extend_from_slice(&gen_u256(u)?); // prev_hash
    buf.extend_from_slice(&gen_u32(u)?); // nonce
    buf.extend_from_slice(&gen_u32(u)?); // ntime
    buf.extend_from_slice(&gen_u32(u)?); // nBits
    buf.extend_from_slice(&gen_u32(u)?); // version
    Ok(buf)
}

// ============================================================================
// Mining Messages
// ============================================================================

/// Generates a wire-valid `CloseChannel` message.
pub fn gen_close_channel(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_str0255(u)?); // reason_code
    Ok(buf)
}

/// Generates a wire-valid `NewMiningJob` message.
pub fn gen_new_mining_job(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(48);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // job_id
    buf.extend_from_slice(&gen_sv2_option(u, gen_u32)?); // ntime_start
    buf.extend_from_slice(&gen_u32(u)?); // version
    buf.extend_from_slice(&gen_u256(u)?); // merkle_root
    Ok(buf)
}

/// Generates a wire-valid `NewExtendedMiningJob` message.
pub fn gen_new_extended_mining_job(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // job_id
    buf.extend_from_slice(&gen_sv2_option(u, gen_u32)?); // ntime_start
    buf.extend_from_slice(&gen_u32(u)?); // version
    buf.extend_from_slice(&gen_bool(u)?); // version_rolling_allowed
    buf.extend_from_slice(&gen_seq0255(u, gen_u256, 255)?); // merkle_path
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_prefix
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_suffix
    Ok(buf)
}

/// Generates a wire-valid `OpenStandardMiningChannel` message.
pub fn gen_open_standard_mining_channel(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_str0255(u)?); // user_identity
    buf.extend_from_slice(&gen_f32(u)?); // nominal_hash_rate
    buf.extend_from_slice(&gen_u256(u)?); // max_target
    Ok(buf)
}
/// Generates a wire-valid `OpenStandardMiningChannelSuccess` message.
pub fn gen_open_standard_mining_channel_success(
    u: &mut Unstructured,
) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(72);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u256(u)?); // target
    buf.extend_from_slice(&gen_b032(u)?); // extranonce_prefix
    buf.extend_from_slice(&gen_u32(u)?); // group_channel_id
    Ok(buf)
}

/// Generates a wire-valid `OpenExtendedMiningChannel` message.
pub fn gen_open_extended_mining_channel(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_str0255(u)?); // user_identity
    buf.extend_from_slice(&gen_f32(u)?); // nominal_hash_rate
    buf.extend_from_slice(&gen_u256(u)?); // max_target
    buf.extend_from_slice(&gen_u16(u)?); // min_extranonce_size
    Ok(buf)
}

/// Generates a wire-valid `OpenExtendedMiningChannelSuccess` message.
pub fn gen_open_extended_mining_channel_success(
    u: &mut Unstructured,
) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(72);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u256(u)?); // target
    buf.extend_from_slice(&gen_u16(u)?); // extranonce_size
    buf.extend_from_slice(&gen_b032(u)?); // extranonce_prefix
    buf.extend_from_slice(&gen_u32(u)?); // group_channel_id
    Ok(buf)
}

/// Generates a wire-valid `OpenMiningChannelError` message.
pub fn gen_open_mining_channel_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    Ok(buf)
}

/// Generates a wire-valid `SetCustomMiningJob` message.
pub fn gen_set_custom_mining_job(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_b0255(u)?); // mining_job_token
    buf.extend_from_slice(&gen_u32(u)?); // version
    buf.extend_from_slice(&gen_u256(u)?); // prev_hash
    buf.extend_from_slice(&gen_u32(u)?); // ntime_start
    buf.extend_from_slice(&gen_u32(u)?); // nBits
    buf.extend_from_slice(&gen_u32(u)?); // coinbase_tx_version
    buf.extend_from_slice(&gen_b0255(u)?); // coinbase_prefix
    buf.extend_from_slice(&gen_u32(u)?); // coinbase_tx_input_nSequence
    buf.extend_from_slice(&gen_b064k(u)?); // coinbase_tx_outputs
    buf.extend_from_slice(&gen_u32(u)?); // coinbase_tx_locktime
    buf.extend_from_slice(&gen_seq0255(u, gen_u256, 255)?); // merkle_path
    Ok(buf)
}

/// Generates a wire-valid `SetCustomMiningJobSuccess` message.
pub fn gen_set_custom_mining_job_success(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(12);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_u32(u)?); // job_id
    Ok(buf)
}

/// Generates a wire-valid `SetCustomMiningJobError` message.
pub fn gen_set_custom_mining_job_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // request_id
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    Ok(buf)
}

/// Generates a wire-valid `SetExtranoncePrefix` message.
pub fn gen_set_extranonce_prefix(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(40);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_b032(u)?); // extranonce_prefix
    Ok(buf)
}
/// Generates a wire-valid `SetGroupChannel` message.
pub fn gen_set_group_channel(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&gen_u32(u)?); // group_channel_id
    buf.extend_from_slice(&gen_seq064k(u, gen_u32, 256)?); // channel_ids
    Ok(buf)
}

/// Generates a wire-valid `SetNewPrevHash` mining message.
pub fn gen_set_new_prev_hash_mining(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(48);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // job_id
    buf.extend_from_slice(&gen_u256(u)?); // prev_hash
    buf.extend_from_slice(&gen_u32(u)?); // ntime_start
    buf.extend_from_slice(&gen_u32(u)?); // nBits
    Ok(buf)
}

/// Generates a wire-valid `SetTarget` message.
pub fn gen_set_target(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(36);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u256(u)?); // target
    Ok(buf)
}

/// Generates a wire-valid `SubmitSharesStandard` message.
pub fn gen_submit_shares_standard(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(24);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // sequence_number
    buf.extend_from_slice(&gen_u32(u)?); // job_id
    buf.extend_from_slice(&gen_u32(u)?); // nonce
    buf.extend_from_slice(&gen_u32(u)?); // ntime
    buf.extend_from_slice(&gen_u32(u)?); // version
    Ok(buf)
}

/// Generates a wire-valid `SubmitSharesExtended` message.
pub fn gen_submit_shares_extended(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = gen_submit_shares_standard(u)?;
    buf.extend_from_slice(&gen_b032(u)?); // extranonce
    Ok(buf)
}

/// Generates a wire-valid `SubmitSharesSuccess` message.
pub fn gen_submit_shares_success(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // last_sequence_number
    buf.extend_from_slice(&gen_u32(u)?); // new_submits_accepted_count
    buf.extend_from_slice(&gen_u64(u)?); // new_shares_sum
    Ok(buf)
}

/// Generates a wire-valid `SubmitSharesError` message.
pub fn gen_submit_shares_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_u32(u)?); // sequence_number
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    Ok(buf)
}

/// Generates a wire-valid `UpdateChannel` message.
pub fn gen_update_channel(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(40);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_f32(u)?); // nominal_hash_rate
    buf.extend_from_slice(&gen_u256(u)?); // max_target
    Ok(buf)
}

/// Generates a wire-valid `UpdateChannelError` message.
pub fn gen_update_channel_error(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&gen_u32(u)?); // channel_id
    buf.extend_from_slice(&gen_str0255(u)?); // error_code
    Ok(buf)
}
