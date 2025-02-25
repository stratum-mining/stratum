use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
use tracing::debug;
use v1::{
    server_to_client,
    utils::{HexU32Be, MerkleNode, PrevHash},
};

/// Creates a new SV1 `mining.notify` message if both SV2 `SetNewPrevHash` and
/// `NewExtendedMiningJob` messages have been received. If one of these messages is still being
/// waited on, the function returns `None`.
/// If clean_jobs = false, it means a new job is created, with the same PrevHash
pub fn create_notify(
    new_prev_hash: SetNewPrevHash<'static>,
    new_job: NewExtendedMiningJob<'static>,
    clean_jobs: bool,
) -> server_to_client::Notify<'static> {
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
