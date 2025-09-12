//! Sv2 Standard Channel - Mining Client Abstraction.
//!
//! This module provides the [`StandardChannel`] struct, which models the state of a mining
//! client's Sv2 Standard Channel. It tracks channel-level job management, share accounting,
//! and chain tip state, enabling share validation and mining job lifecycle management.

extern crate alloc;
use super::HashMap;
use crate::{
    chain_tip::ChainTip,
    client::{
        error::StandardChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    merkle_root::merkle_root_from_path,
    target::{bytes_to_hex, target_to_difficulty, u256_to_block_hash},
};
use alloc::{format, string::String, vec::Vec};
use binary_sv2::{self, Sv2Option};
use bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    CompactTarget, Target as BitcoinTarget,
};
use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, SetNewPrevHash as SetNewPrevHashMp, SubmitSharesStandard,
    Target, MAX_EXTRANONCE_LEN,
};
use tracing::debug;

/// Mining Client abstraction over the state of a Sv2 Standard Channel.
///
/// Tracks:
/// - unique channel ID
/// - user identity string
/// - unique extranonce prefix
/// - channel target
/// - nominal hashrate in h/s
/// - future mining jobs (indexed by job_id, activated upon [`NewMiningJob`] receipt)
/// - active mining job
/// - past jobs (active jobs under current chain tip, indexed by job_id)
/// - stale jobs (jobs from previous chain tip, indexed by job_id)
/// - share accounting state
/// - chain tip state
#[derive(Debug, Clone)]
pub struct StandardChannel<'a> {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    target: Target,
    nominal_hashrate: f32,
    future_jobs: HashMap<u32, NewMiningJob<'a>>,
    active_job: Option<NewMiningJob<'a>>,
    past_jobs: HashMap<u32, NewMiningJob<'a>>,
    stale_jobs: HashMap<u32, NewMiningJob<'a>>,
    share_accounting: ShareAccounting,
    chain_tip: Option<ChainTip>,
}

impl<'a> StandardChannel<'a> {
    /// Creates a new [`StandardChannel`] instance with provided channel parameters.
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        target: Target,
        nominal_hashrate: f32,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            share_accounting: ShareAccounting::new(),
            chain_tip: None,
        }
    }

    /// Returns the channel ID.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string associated with this channel.
    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    /// Returns the latest chain tip information, if any.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Sets the [`ChainTip`]
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Sets the extranonce prefix for the channel.
    ///
    /// All new jobs will use the new extranonce prefix. Jobs created before
    /// this call will continue using their previous prefix for share validation.
    /// Returns an error if the prefix is too large.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: Vec<u8>,
    ) -> Result<(), StandardChannelError> {
        if extranonce_prefix.len() > MAX_EXTRANONCE_LEN {
            return Err(StandardChannelError::NewExtranoncePrefixTooLarge);
        }

        self.extranonce_prefix = extranonce_prefix;
        Ok(())
    }

    /// Returns the bytes representing the first part of the extranonce.
    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    /// Returns the current target for the channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Sets a new target for the channel.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Returns the nominal hashrate of the channel in h/s.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Returns all future jobs for this channel.
    ///
    /// The list is cleared once a [`StandardChannel::on_set_new_prev_hash`] is processed.
    pub fn get_future_jobs(&self) -> &HashMap<u32, NewMiningJob<'a>> {
        &self.future_jobs
    }

    /// Returns the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&NewMiningJob<'a>> {
        self.active_job.as_ref()
    }

    /// Returns all past jobs for the channel (active jobs under current chain tip).
    pub fn get_past_jobs(&self) -> &HashMap<u32, NewMiningJob<'a>> {
        &self.past_jobs
    }

    /// Returns all stale jobs for the channel (jobs from previous chain tip).
    pub fn get_stale_jobs(&self) -> &HashMap<u32, NewMiningJob<'a>> {
        &self.stale_jobs
    }

    /// Returns the share accounting state for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Handles a new group channel job by converting it into a standard job
    /// and activating it in this channel's context.
    ///
    /// The new job is constructed using the current extranonce prefix.
    pub fn on_new_group_channel_job(&mut self, new_extended_mining_job: NewExtendedMiningJob<'a>) {
        let merkle_root = merkle_root_from_path(
            new_extended_mining_job.coinbase_tx_prefix.inner_as_ref(),
            new_extended_mining_job.coinbase_tx_suffix.inner_as_ref(),
            &self.extranonce_prefix,
            &new_extended_mining_job.merkle_path.inner_as_ref(),
        )
        .expect("merkle root must be valid")
        .try_into()
        .expect("merkle root must be 32 bytes");

        let new_mining_job = NewMiningJob {
            channel_id: self.channel_id,
            job_id: new_extended_mining_job.job_id,
            merkle_root,
            version: new_extended_mining_job.version,
            min_ntime: new_extended_mining_job.min_ntime,
        };

        self.on_new_mining_job(new_mining_job);
    }

    /// Handles a newly received [`NewMiningJob`] message from upstream.
    ///
    /// - If `min_ntime` is present, the job is activated and replaces the current active job.
    /// - If `min_ntime` is empty, the job is added to future jobs.
    /// - If an active job exists, it is moved to past jobs on activation.
    pub fn on_new_mining_job(&mut self, new_mining_job: NewMiningJob<'a>) {
        match new_mining_job.min_ntime.clone().into_inner() {
            Some(_min_ntime) => {
                if let Some(active_job) = self.active_job.as_ref() {
                    self.past_jobs.insert(active_job.job_id, active_job.clone());
                }
                self.active_job = Some(new_mining_job);
            }
            None => {
                self.future_jobs
                    .insert(new_mining_job.job_id, new_mining_job);
            }
        }
    }

    /// Handles an upstream [`SetNewPrevHash`](SetNewPrevHashMp) message.
    ///
    /// - Activates the matching future job as the new active job.
    /// - Clears all future jobs.
    /// - Marks all past jobs as stale (they are no longer valid for share propagation).
    /// - Clears past jobs and the set of seen shares (to avoid memory growth and stale share
    ///   submissions).
    /// - Updates chain tip information. Returns error if no matching future job found.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp<'a>,
    ) -> Result<(), StandardChannelError> {
        match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(mut activated_job) => {
                activated_job.min_ntime = Sv2Option::new(Some(set_new_prev_hash.min_ntime));
                self.active_job = Some(activated_job);
            }
            None => return Err(StandardChannelError::JobIdNotFound),
        }

        // all other future jobs are now useless
        self.future_jobs.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to propagate shares for them
        self.past_jobs.clear();

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Validates a share before submission upstream.
    ///
    /// - Checks if the share refers to an active or past job; rejects stale jobs.
    /// - Verifies the share meets the channel target, is not a duplicate, and is not stale.
    /// - Updates share accounting state based on validation result.
    /// - Returns whether the share is valid or resulted in a block being found.
    /// - Returns error describing why share is not valid.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesStandard,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.job_id == job_id);

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

        let merkle_root: [u8; 32] = job
            .merkle_root
            .inner_as_ref()
            .try_into()
            .expect("merkle root must be 32 bytes");

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

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
        share_accounting::{ShareValidationError, ShareValidationResult},
        standard::StandardChannel,
    };
    use binary_sv2::Sv2Option;
    use mining_sv2::{NewMiningJob, SetNewPrevHash as SetNewPrevHashMp, SubmitSharesStandard};

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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
        );

        let future_job = NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        channel.on_new_mining_job(future_job.clone());

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

        assert_eq!(channel.get_active_job(), Some(&previously_future_job));
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
        );

        let ntime: u32 = 1746839905;
        let active_job = NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(Some(ntime)),
        };

        channel.on_new_mining_job(active_job.clone());

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(channel.get_active_job(), Some(&active_job));
        assert_eq!(channel.get_past_jobs().len(), 0);

        let mut new_active_job = active_job.clone();
        new_active_job.job_id = 2;
        channel.on_new_mining_job(new_active_job.clone());

        assert_eq!(channel.get_future_jobs().len(), 0);
        assert_eq!(channel.get_active_job(), Some(&new_active_job));
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
        );

        let future_job = NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        channel.on_new_mining_job(future_job.clone());

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

        // this share has hash 61e8fe82487d10282fdededed636403eb2c8cb05ce792951dd410a9011a94ebb
        // which satisfied the network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesStandard {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
        );

        let future_job = NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        channel.on_new_mining_job(future_job.clone());

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

        // this share has hash 45ec7dbd7b599599e6724ab32e6936dad033f46ccff97e743579d8c047cf3243
        // which does not meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let share_low_diff = SubmitSharesStandard {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            target,
            nominal_hashrate,
        );

        let future_job = NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        channel.on_new_mining_job(future_job.clone());

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

        // this share has hash 0000762e88282a2ed8e7097aef06f413a962a47e32206a80ecbfc1f0b4bd1493
        // which meets the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesStandard {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 244405,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid)));
    }
}
