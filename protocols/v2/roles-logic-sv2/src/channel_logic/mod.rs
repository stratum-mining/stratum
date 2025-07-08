//! # Channel Logic
//!
//! A module for managing channels on applications.
//!
//! Divided in two submodules:
//! - [`channel_factory`]
//! - [`proxy_group_channel`]

pub mod channel_factory;

use mining_sv2::{NewExtendedMiningJob, NewMiningJob};
use std::convert::TryInto;

/// Convert extended to standard job by calculating the merkle root
pub fn extended_to_standard_job<'a>(
    extended: &NewExtendedMiningJob,
    coinbase_script: &[u8],
    channel_id: u32,
    job_id: Option<u32>,
) -> Option<NewMiningJob<'a>> {
    let merkle_root = crate::utils::merkle_root_from_path(
        extended.coinbase_tx_prefix.inner_as_ref(),
        extended.coinbase_tx_suffix.inner_as_ref(),
        coinbase_script,
        &extended.merkle_path.inner_as_ref(),
    );

    Some(NewMiningJob {
        channel_id,
        job_id: job_id.unwrap_or(extended.job_id),
        min_ntime: extended.min_ntime.clone().into_static(),
        version: extended.version,
        merkle_root: merkle_root?.try_into().ok()?,
    })
}
