use super::NextShareOutcome;
use roles_logic_sv2::mining_sv2::{NewMiningJob, SetNewPrevHash};
use std::convert::TryInto;
use stratum_common::bitcoin::{
    blockdata::block::BlockHeader, hash_types::BlockHash, hashes::Hash, util::uint::Uint256,
};
use tracing::info;

#[derive(Debug, Clone)]
pub(crate) struct Miner {
    pub(crate) header: Option<BlockHeader>,
    pub(crate) target: Option<Uint256>,
    pub(crate) job_id: Option<u32>,
    pub(crate) version: Option<u32>,
    pub(crate) handicap: u32,
}

impl Miner {
    pub(crate) fn new(handicap: u32) -> Self {
        Self {
            target: None,
            header: None,
            job_id: None,
            version: None,
            handicap,
        }
    }

    pub(crate) fn new_target(&mut self, mut target: Vec<u8>) {
        // target is sent in LE and comparisons in this file are done in BE
        target.reverse();
        let hex_string = target
            .iter()
            .fold("".to_string(), |acc, b| acc + format!("{:02x}", b).as_str());
        info!("Set target to {}", hex_string);
        self.target = Some(Uint256::from_be_bytes(target.try_into().unwrap()));
    }

    pub(crate) fn new_header(
        &mut self,
        set_new_prev_hash: &SetNewPrevHash,
        new_job: &NewMiningJob,
    ) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = set_new_prev_hash.prev_hash.to_vec().try_into().unwrap();
        let prev_hash = Hash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = Hash::from_inner(merkle_root);
        // fields need to be added as BE and the are converted to LE in the background before hashing
        let header = BlockHeader {
            version: new_job.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root,
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
                .unwrap()
                .as_secs() as u32,
            bits: set_new_prev_hash.nbits,
            nonce: 0,
        };
        self.header = Some(header);
    }
    pub fn next_share(&mut self) -> NextShareOutcome {
        if let Some(header) = self.header.as_ref() {
            let mut hash = header.block_hash().as_hash().into_inner();
            hash.reverse();
            let hash = Uint256::from_be_bytes(hash);
            if hash < *self.target.as_ref().unwrap() {
                info!(
                    "Found share with nonce: {}, for target: {:?}, with hash: {:?}",
                    header.nonce, self.target, hash,
                );
                NextShareOutcome::ValidShare
            } else {
                NextShareOutcome::InvalidShare
            }
        } else {
            std::thread::yield_now();
            NextShareOutcome::InvalidShare
        }
    }
}
