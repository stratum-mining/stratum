//! Mining Client abstraction over the state of a Sv2 Extended Channel

use crate::{
    chain_tip::ChainTip,
    client::{
        error::ExtendedChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    merkle_root::merkle_root_from_path,
    target::{bytes_to_hex, target_to_difficulty, u256_to_block_hash},
};
use binary_sv2::{self, Sv2Option};
use bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    CompactTarget, Target as BitcoinTarget,
};
use mining_sv2::{
    NewExtendedMiningJob, SetNewPrevHash as SetNewPrevHashMp, SubmitSharesExtended, Target,
    MAX_EXTRANONCE_LEN,
};
use std::{collections::HashMap, convert::TryInto};
use tracing::debug;

// ExtendedJob is a tuple of:
// - the NewExtendedMiningJob message
// - the extranonce_prefix associated with the channel at the time of job creation
pub type ExtendedJob<'a> = (NewExtendedMiningJob<'a>, Vec<u8>);

/// Mining Client abstraction over the state of a Sv2 Extended Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's rollable extranonce size
/// - the channel's target
/// - the channel's nominal hashrate
/// - the channel's version rolling
/// - the channel's future jobs (indexed by `job_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the channel's active job
/// - the channel's past jobs (which were active jobs under the current chain tip, indexed by
///   `job_id`)
/// - the channel's stale jobs (which were past and active jobs under the previous chain tip,
///   indexed by `job_id`)
/// - the channel's share accounting (as seen by the client)
/// - the channel's chain tip
#[derive(Clone, Debug)]
pub struct ExtendedChannel<'a> {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    target: Target, // todo: try to use Target from rust-bitcoin
    nominal_hashrate: f32,
    version_rolling: bool,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, ExtendedJob<'a>>,
    active_job: Option<ExtendedJob<'a>>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, ExtendedJob<'a>>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, ExtendedJob<'a>>,
    share_accounting: ShareAccounting,
    chain_tip: Option<ChainTip>,
}

impl<'a> ExtendedChannel<'a> {
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        target: Target,
        nominal_hashrate: f32,
        version_rolling: bool,
        rollable_extranonce_size: u16,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            target,
            nominal_hashrate,
            version_rolling,
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            share_accounting: ShareAccounting::new(),
            chain_tip: None,
        }
    }

    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    pub fn is_version_rolling(&self) -> bool {
        self.version_rolling
    }

    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Sets the extranonce prefix.
    ///
    /// Note: after this, all new jobs will be associated with the new extranonce prefix.
    /// Jobs created before this call will remain associated with the previous extranonce prefix,
    /// and share validation will be done accordingly.
    pub fn set_extranonce_prefix(
        &mut self,
        new_extranonce_prefix: Vec<u8>,
    ) -> Result<(), ExtendedChannelError> {
        let new_rollable_extranonce_size =
            MAX_EXTRANONCE_LEN as u16 - new_extranonce_prefix.len() as u16;

        // we return an error if the new extranonce_prefix would violate
        // min_rollable_extranonce_size that was already established with the client when the
        // channel was created
        if new_rollable_extranonce_size < self.rollable_extranonce_size {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        self.extranonce_prefix = new_extranonce_prefix;
        self.rollable_extranonce_size = new_rollable_extranonce_size;

        Ok(())
    }

    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    pub fn get_target(&self) -> &Target {
        &self.target
    }

    pub fn set_target(&mut self, new_target: Target) {
        self.target = new_target;
    }

    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    pub fn get_active_job(&self) -> Option<&ExtendedJob<'a>> {
        self.active_job.as_ref()
    }

    pub fn get_future_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.future_jobs
    }

    pub fn get_past_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.past_jobs
    }

    pub fn get_stale_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.stale_jobs
    }

    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Called when a `NewExtendedMiningJob` message is received from upstream.
    pub fn on_new_extended_mining_job(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJob<'a>,
    ) {
        match new_extended_mining_job.min_ntime.clone().into_inner() {
            Some(_min_ntime) => {
                if let Some(active_job) = self.active_job.clone() {
                    self.past_jobs.insert(active_job.0.job_id, active_job);
                }
                self.active_job = Some((new_extended_mining_job, self.extranonce_prefix.clone()));
            }
            None => {
                self.future_jobs.insert(
                    new_extended_mining_job.job_id,
                    (new_extended_mining_job, self.extranonce_prefix.clone()),
                );
            }
        }
    }

    /// Called when a `SetNewPrevHash` message is received from upstream.
    ///
    /// If the job_id addressed in the `SetNewPrevHash` is not a future job,
    /// returns an error.
    ///
    /// If the job_id addressed in the `SetNewPrevHash` is a future job,
    /// it is "activated" and set as the active job.
    ///
    /// All past jobs are marked as stale, so that shares are not propagated.
    ///
    /// The chain tip information is not kept in the channel state.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp<'a>,
    ) -> Result<(), ExtendedChannelError> {
        match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(mut activated_job) => {
                activated_job.0.min_ntime = Sv2Option::new(Some(set_new_prev_hash.min_ntime));
                self.active_job = Some(activated_job);
            }
            None => {
                return Err(ExtendedChannelError::JobIdNotFound);
            }
        }

        // all other future jobs are now useless
        self.future_jobs.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to propagate shares for them
        self.past_jobs.clear();

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        let set_new_prev_hash_static = set_new_prev_hash.into_static();
        let new_chain_tip = ChainTip::new(
            set_new_prev_hash_static.prev_hash,
            set_new_prev_hash_static.nbits,
            set_new_prev_hash_static.min_ntime,
        );
        self.chain_tip = Some(new_chain_tip);

        Ok(())
    }

    /// Validates a share, to be used before submission upstream.
    ///
    /// Updates the channel state with the result of the share validation.
    ///
    /// - Allows the mining client to avoid propagating stale, duplicate or low-diff shares.
    /// - Allows the mining client to know whether a block was found on some share.
    /// - Allows the mining client to keep a local version of the share accounting for comparison
    ///   with the acknowledgements coming from the upstream server.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtended,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.0.job_id == job_id);

        // check if job_id is past job
        let is_past_job = self.past_jobs.contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.stale_jobs.contains_key(&job_id);

        if is_stale_job {
            return Err(ShareValidationError::Stale);
        }

        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else if is_past_job {
            self.past_jobs.get(&job_id).expect("past job must exist")
        } else {
            return Err(ShareValidationError::InvalidJobId);
        };

        let mut full_extranonce = vec![];
        full_extranonce.extend_from_slice(job.1.as_slice());
        full_extranonce.extend_from_slice(share.extranonce.inner_as_ref());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.0.coinbase_tx_prefix.inner_as_ref(),
            job.0.coinbase_tx_suffix.inner_as_ref(),
            full_extranonce.as_ref(),
            &job.0.merkle_path.inner_as_ref(),
        )
        .ok_or(ShareValidationError::Invalid)?
        .try_into()
        .expect("merkle root must be 32 bytes");

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits: CompactTarget = CompactTarget::from_consensus(chain_tip.nbits());

        // validate when version rolling is not allowed
        if !job.0.version_rolling_allowed {
            // If version rolling is not allowed, ensure bits 13-28 are 0
            // This is done by checking if the version & 0x1fffe000 == 0
            // ref: https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki
            if (share.version & 0x1fffe000) != 0 {
                return Err(ShareValidationError::VersionRollingNotAllowed);
            }
        }

        // create the header for validation
        let header = Header {
            version: Version::from_consensus(share.version as i32),
            prev_blockhash: u256_to_block_hash(prev_hash.clone()),
            merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
            time: share.ntime,
            bits: nbits,
            nonce: share.nonce,
        };

        // convert the header hash to a target type for easy comparison
        let hash = header.block_hash();
        let raw_hash: [u8; 32] = *hash.to_raw_hash().as_ref();
        let hash_as_target: Target = raw_hash.into();
        let hash_as_diff = target_to_difficulty(hash_as_target.clone());

        let network_target = BitcoinTarget::from_compact(nbits);

        // print hash_as_target and self.target as human readable hex
        let hash_as_u256: binary_sv2::U256 = hash_as_target.clone().into();
        let mut hash_bytes = hash_as_u256.to_vec();
        hash_bytes.reverse(); // Convert to big-endian for display
        let target_u256: binary_sv2::U256 = self.target.clone().into();
        let mut target_bytes = target_u256.to_vec();
        target_bytes.reverse(); // Convert to big-endian for display

        debug!(
            "share validation \nshare:\t\t{}\nchannel target:\t{}\nnetwork target:\t{}",
            bytes_to_hex(&hash_bytes),
            bytes_to_hex(&target_bytes),
            format!("{:x}", network_target)
        );

        // check if a block was found
        if network_target.is_met_by(hash) {
            self.share_accounting.update_share_accounting(
                target_to_difficulty(self.target.clone()) as u64,
                share.sequence_number,
                hash.to_raw_hash(),
            );
            return Ok(ShareValidationResult::BlockFound);
        }

        // check if the share hash meets the channel target
        if hash_as_target < self.target {
            if self.share_accounting.is_share_seen(hash.to_raw_hash()) {
                return Err(ShareValidationError::DuplicateShare);
            }

            self.share_accounting.update_share_accounting(
                target_to_difficulty(self.target.clone()) as u64,
                share.sequence_number,
                hash.to_raw_hash(),
            );

            // update the best diff
            self.share_accounting.update_best_diff(hash_as_diff);

            return Ok(ShareValidationResult::Valid);
        }

        Err(ShareValidationError::DoesNotMeetTarget)
    }
}

#[cfg(test)]
mod tests {
    use crate::client::{
        extended::ExtendedChannel,
        share_accounting::{ShareValidationError, ShareValidationResult},
    };
    use binary_sv2::Sv2Option;
    use mining_sv2::{
        NewExtendedMiningJob, SetNewPrevHash as SetNewPrevHashMp, SubmitSharesExtended,
        MAX_EXTRANONCE_LEN,
    };
    use std::convert::TryInto;

    #[test]
    fn test_future_job_activation_flow() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(future_job.clone());

        assert_eq!(channel.get_future_jobs().len(), 1);
        assert_eq!(channel.get_active_job(), None);
        assert_eq!(channel.get_past_jobs().len(), 0);

        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits: 503543726,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        assert!(channel.get_future_jobs().is_empty());

        let mut previously_future_job = future_job.clone();
        previously_future_job.min_ntime = Sv2Option::new(Some(ntime));

        assert_eq!(
            channel.get_active_job(),
            Some(&(previously_future_job, extranonce_prefix))
        );
    }

    #[test]
    fn test_past_jobs_flow() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let ntime: u32 = 1746839905;
        let active_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(active_job.clone());

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&(active_job.clone(), extranonce_prefix.clone()))
        );
        assert_eq!(channel.get_past_jobs().len(), 0);

        let mut new_active_job = active_job.clone();
        new_active_job.job_id = 2;
        channel.on_new_extended_mining_job(new_active_job.clone());

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&(new_active_job, extranonce_prefix))
        );
        assert_eq!(channel.get_past_jobs().len(), 1);
    }

    #[test]
    fn test_share_validation_block_found() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(future_job.clone());

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 38b6a7d5b2cae08bc6c8b4b4fc13ff129ae0a07309240108f46ddf48c498b120
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block);

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound)));
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]
        .into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(future_job.clone());

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1746839905;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash bc1f25bceec05b1cc60fd0f0a3ede685efbb00d2a7d39c879d2c187b2af3538d
        // which does not meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget
        ));
    }

    #[test]
    fn test_share_validation_valid_share() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]
        .into();
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
        );

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(future_job.clone());

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits: u32 = 453040064;
        let ntime: u32 = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 0000d769e5ab58b7309b7507834cb0bc60749315c93015e8bba97b9752ced5b7
        // which does meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 2426,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid)));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 2426,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(repeated_share);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare
        ));
    }
}
