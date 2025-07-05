use primitive_types::U256;
use roles_logic_sv2::{
    job_creator::extended_job_to_non_segwit,
    mining_sv2::{NewExtendedMiningJob, SetNewPrevHash, Target},
};
use std::ops::Div;
use tracing::debug;
use v1::{
    json_rpc, server_to_client,
    utils::{HexU32Be, MerkleNode, PrevHash},
};

use crate::error::TproxyError;

/// Creates a new SV1 `mining.notify` message from SV2 messages.
///
/// This function translates SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages
/// into a corresponding SV1 `mining.notify` message that can be sent to downstream
/// SV1 miners.
///
/// The function performs the following conversions:
/// - Converts the extended mining job to non-segwit format
/// - Extracts the previous block hash
/// - Converts coinbase transaction prefix and suffix
/// - Transforms the merkle path into SV1 format
/// - Sets appropriate version, bits, and timestamp fields
///
/// # Arguments
/// * `new_prev_hash` - SV2 message containing the previous block hash information
/// * `new_job` - SV2 message containing the new mining job details
/// * `clean_jobs` - Whether miners should abandon previous jobs
///
/// # Returns
/// A properly formatted SV1 `mining.notify` message
pub fn create_notify(
    new_prev_hash: SetNewPrevHash<'static>,
    new_job: NewExtendedMiningJob<'static>,
    clean_jobs: bool,
) -> server_to_client::Notify<'static> {
    // TODO 32 must be changed!
    let new_job = extended_job_to_non_segwit(new_job, 32)
        .expect("failed to convert extended job to non segwit");
    // Make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not future)
    let job_id = new_job.job_id.to_string();

    // U256<'static> -> MerkleLeaf
    let prev_hash = PrevHash(new_prev_hash.prev_hash.clone());

    // B064K<'static'> -> HexBytes
    let coin_base1 = new_job.coinbase_tx_prefix.to_vec().into();
    let coin_base2 = new_job.coinbase_tx_suffix.to_vec().into();

    // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>>
    let merkle_path = new_job.merkle_path.clone().into_static().0;
    let merkle_branch: Vec<MerkleNode> = merkle_path.into_iter().map(MerkleNode).collect();

    // u32 -> HexBytes
    let version = HexU32Be(new_job.version);
    let bits = HexU32Be(new_prev_hash.nbits);
    let time = HexU32Be(match new_job.is_future() {
        true => new_prev_hash.min_ntime,
        false => new_job.min_ntime.clone().into_inner().unwrap(),
    });

    let notify_response = server_to_client::Notify {
        job_id,
        prev_hash,
        coin_base1,
        coin_base2,
        merkle_branch,
        version,
        bits,
        time,
        clean_jobs,
    };
    debug!("\nNextMiningNotify: {:?}\n", notify_response);
    notify_response
}

/// Converts an SV2 target into an SV1 `mining.set_difficulty` message.
///
/// This function takes an SV2 target value and converts it to the corresponding
/// difficulty value that should be sent to SV1 miners via the `mining.set_difficulty`
/// message.
///
/// # Arguments
/// * `target` - The SV2 target value to convert
///
/// # Returns
/// * `Ok(json_rpc::Message)` - The properly formatted SV1 set_difficulty message
/// * `Err(TproxyError)` - If the target conversion fails
pub fn get_set_difficulty(target: Target) -> Result<json_rpc::Message, TproxyError> {
    let value = difficulty_from_target(target)?;
    debug!("Difficulty from target: {:?}", value);
    let set_target = v1::methods::server_to_client::SetDifficulty { value };
    let message: json_rpc::Message = set_target.into();
    Ok(message)
}

/// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
/// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
#[allow(clippy::result_large_err)]
pub(super) fn difficulty_from_target(target: Target) -> Result<f64, TproxyError> {
    // reverse because target is LE and this function relies on BE
    let mut target = binary_sv2::U256::from(target).to_vec();

    target.reverse();

    let target = target.as_slice();
    debug!("Target: {:?}", target);

    // If received target is 0, return 0
    if is_zero(target) {
        return Ok(0.0);
    }
    let target = U256::from_big_endian(target);
    let pdiff: [u8; 32] = [
        0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ];
    let pdiff = U256::from_big_endian(pdiff.as_ref());

    if pdiff > target {
        let diff = pdiff.div(target);
        Ok(diff.low_u64() as f64)
    } else {
        let diff = target.div(pdiff);
        let diff = diff.low_u64() as f64;
        // TODO still results in a difficulty that is too low
        Ok(1.0 / diff)
    }
}

/// Helper function to check if target is set to zero for some reason (typically happens when
/// Downstream role first connects).
/// https://stackoverflow.com/questions/65367552/checking-a-vecu8-to-see-if-its-all-zero
fn is_zero(buf: &[u8]) -> bool {
    let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };

    prefix.iter().all(|&x| x == 0)
        && suffix.iter().all(|&x| x == 0)
        && aligned.iter().all(|&x| x == 0)
}
