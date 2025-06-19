use std::convert::TryInto;
use stratum_common::roles_logic_sv2;
use v1::server_to_client;

/// Represents a new Job built from an incoming `mining.notify` message from the Upstream server.
pub(crate) struct Job {
    /// ID of the job used while submitting share generated from this job.
    /// TODO: Currently is `u32` and is hardcoded, but should be String and set by the incoming
    /// `mining.notify` message.
    pub(crate) job_id: u32,
    /// Hash of previous block
    pub(crate) prev_hash: [u8; 32],
    /// Merkle root
    /// TODO: Currently is hardcoded. This field should be replaced with three fields: 1)
    /// `coinbase_1` - the first half of the coinbase transaction before the `extranonce` which is
    /// inserted by the miner, 2) `coinbase_2` - the second half of the coinbase transaction after
    /// the `extranonce` which is inserted by the miner, and 3) `merkle_branches` - the merkle
    /// branches to build the merkle root sans the coinbase transaction
    // coinbase_1: Vec<u32>,
    // coinbase_2: Vec<u32>,
    // merkle_brances: Vec<[u8; 32]>,
    pub(crate) merkle_root: [u8; 32],
    pub(crate) version: u32,
    pub(crate) nbits: u32,
}

impl Job {
    pub fn from_notify(notify_msg: server_to_client::Notify<'_>, extranonce: Vec<u8>) -> Self {
        let job_id = notify_msg
            .job_id
            .parse::<u32>()
            .expect("expect valid job_id on String");

        // Convert prev hash from Vec<u8> into expected [u32; 8]
        let prev_hash_vec: Vec<u8> = notify_msg.prev_hash.into();
        let prev_hash_slice: &[u8] = prev_hash_vec.as_slice();
        let prev_hash: &[u8; 32] = prev_hash_slice.try_into().expect("Expected len 32");
        let prev_hash = *prev_hash;

        let coinbase_tx_prefix: Vec<u8> = notify_msg.coin_base1.into();
        let coinbase_tx_suffix: Vec<u8> = notify_msg.coin_base2.into();
        let path: Vec<Vec<u8>> = notify_msg
            .merkle_branch
            .into_iter()
            .map(|node| node.into())
            .collect();

        let merkle_root = roles_logic_sv2::utils::merkle_root_from_path(
            &coinbase_tx_prefix,
            &coinbase_tx_suffix,
            &extranonce,
            &path,
        )
        .unwrap();
        let merkle_root: [u8; 32] = merkle_root.try_into().unwrap();

        Job {
            job_id,
            prev_hash,
            nbits: notify_msg.bits.0,
            version: notify_msg.version.0,
            merkle_root,
        }
    }
}
