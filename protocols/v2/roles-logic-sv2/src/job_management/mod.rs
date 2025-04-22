pub mod chain_tip;
pub mod extended;
pub mod standard;

use crate::utils::merkle_root_from_path;
use extended::ExtendedJob;
use mining_sv2::NewMiningJob;
use standard::StandardJob;

use std::convert::TryInto;

pub fn extended_to_standard_job(
    channel_id: u32,
    extended_job: ExtendedJob<'static>,
    extranonce_prefix: Vec<u8>,
) -> StandardJob<'static> {
    let merkle_root = merkle_root_from_path(
        extended_job.get_coinbase_tx_prefix().inner_as_ref(),
        extended_job.get_coinbase_tx_suffix().inner_as_ref(),
        &extranonce_prefix,
        &extended_job.get_merkle_path().inner_as_ref(),
    )
    .expect("merkle root must be valid")
    .try_into()
    .expect("merkle root must be 32 bytes");

    let standard_job_message = NewMiningJob {
        channel_id,
        job_id: extended_job.get_job_id(),
        merkle_root,
        version: extended_job.get_version(),
        min_ntime: extended_job.get_min_ntime(),
    };

    StandardJob::new(
        extended_job
            .get_template()
            .expect("template must be present")
            .clone(),
        extended_job.get_coinbase_reward_outputs().clone(),
        standard_job_message,
    )
}
