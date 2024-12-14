use crate::job::Job;
use std::convert::TryInto;
use stratum_common::bitcoin::{
    blockdata::block::BlockHeader,
    hash_types::{BlockHash, TxMerkleNode},
    hashes::{sha256d::Hash as DHash, Hash},
    util::uint::Uint256,
};
use tracing::info;

/// A mock representation of a Mining Device that produces block header hashes to be submitted by
/// the `Client` to the Upstream node (either a SV1 Pool server or a SV1 <-> SV2 Translator Proxy
/// server).
#[derive(Debug)]
pub(crate) struct Miner {
    /// Mock of mined candidate block header.
    pub(crate) header: Option<BlockHeader>,
    /// Current mining target.
    pub(crate) target: Option<Uint256>,
    /// ID of the job used while submitting share generated from this job.
    pub(crate) job_id: Option<u32>,
    /// Block header version
    pub(crate) version: Option<u32>,
    /// TODO: RRQ: Remove?
    pub(crate) _handicap: u32,
}

impl Miner {
    /// Instantiates a new Miner instance.
    pub(crate) fn new(handicap: u32) -> Self {
        Self {
            target: None,
            header: None,
            job_id: None,
            version: None,
            _handicap: handicap,
        }
    }

    /// Updates target when a new target is received by the SV1 `Client`.
    pub(crate) fn new_target(&mut self, target: Uint256) {
        self.target = Some(target);
    }

    /// Mocks out the mining of a new candidate block header.
    /// `Client` calls `new_header` when it receives a new `mining.notify` message from the
    /// Upstream node indicating the `Miner` should start mining on a new job.
    pub(crate) fn new_header(&mut self, new_job: Job) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = new_job.prev_hash;
        let prev_hash = DHash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = DHash::from_inner(merkle_root);
        let header = BlockHeader {
            version: new_job.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root: TxMerkleNode::from_hash(merkle_root),
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
                .unwrap()
                .as_secs() as u32,
            bits: new_job.nbits,
            nonce: 0,
        };
        self.header = Some(header);
    }

    /// Called by the `Client` to retrieve the latest candidate block header hash. The actual
    /// incrementing of the nonce is mocked out in a thread in `Client::new()`.
    pub(crate) fn next_share(&mut self) -> Result<(), ()> {
        let header = self.header.as_ref().ok_or(())?;
        let mut hash = header.block_hash().as_hash().into_inner();
        hash.reverse();
        let hash = Uint256::from_be_bytes(hash);
        if hash < *self.target.as_ref().ok_or(())? {
            info!(
                "Found share with nonce: {}, for target: {:?}, hash: {:?}",
                header.nonce, self.target, hash
            );
            Ok(())
        } else {
            Err(())
        }
    }
}
