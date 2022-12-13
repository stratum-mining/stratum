use std::convert::TryInto;
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

impl<'a> From<server_to_client::Notify<'a>> for Job {
    fn from(notify_msg: v1::methods::server_to_client::Notify) -> Self {
        // TODO: Hard coded for demo. Should be properly translated from received Notify message
        // Right now, Notify.job_id is a string, but the Job.job_id is a u32 here.
        let job_id = 1u32;

        // Convert prev hash from Vec<u8> into expected [u32; 8]
        let prev_hash_vec: Vec<u8> = notify_msg.prev_hash.into();
        let prev_hash_slice: &[u8] = prev_hash_vec.as_slice();
        let prev_hash: &[u8; 32] = prev_hash_slice.try_into().expect("Expected len 32");
        let prev_hash = *prev_hash;

        // Make a fake merkle root for the demo
        // TODO: Should instead update Job to have cb1, cb2, and merkle_branches instead of
        // merkle_root, then generate a random extranonce, build the cb by concatenating cb1 +
        // extranonce + cb2, then calculate the merkle_root with the full branches
        let merkle_root: [u8; 32] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        Job {
            job_id,
            prev_hash,
            nbits: notify_msg.bits.0,
            version: notify_msg.version.0,
            merkle_root,
        }
    }
}
