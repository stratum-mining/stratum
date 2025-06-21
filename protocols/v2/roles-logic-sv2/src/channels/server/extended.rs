//! Mining Server abstraction over the state of a Sv2 Extended Channel

use crate::{
    channels::{
        chain_tip::ChainTip,
        server::{
            error::ExtendedChannelError,
            jobs::{extended::ExtendedJob, factory::JobFactory, JobOrigin},
            share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
        },
    },
    utils::{
        bytes_to_hex, hash_rate_to_target, merkle_root_from_path, target_to_difficulty,
        u256_to_block_hash,
    },
};
use bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target as BitcoinTarget,
};
use codec_sv2::binary_sv2;
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended, Target, MAX_EXTRANONCE_LEN};
use std::{collections::HashMap, convert::TryInto};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp};
use tracing::debug;

/// Mining Server abstraction of a Sv2 Extended Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's rollable extranonce size
/// - the channel's requested max target (limit established by the client)
/// - the channel's target
/// - the channel's nominal hashrate
/// - the channels' mapping between `template_id`s and `job_id`s
/// - the channel's future jobs (indexed by `template_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the channel's active job
/// - the channel's past jobs (which were active jobs under the current chain tip, indexed by
///   `job_id`)
/// - the channel's stale jobs (which were past and active jobs under the previous chain tip,
///   indexed by `job_id`)
/// - the channel's share validation state
/// - the channel's job factory
/// - the channel's chain tip
#[derive(Clone, Debug)]
pub struct ExtendedChannel<'a> {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    requested_max_target: Target,
    target: Target, // todo: try to use Target from rust-bitcoin
    nominal_hashrate: f32,
    // maps template_id to job_id on future jobs
    future_template_to_job_id: HashMap<u64, u32>,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, ExtendedJob<'a>>,
    active_job: Option<ExtendedJob<'a>>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, ExtendedJob<'a>>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, ExtendedJob<'a>>,
    job_factory: JobFactory,
    share_accounting: ShareAccounting,
    expected_share_per_minute: f32,
    chain_tip: Option<ChainTip>,
}

impl<'a> ExtendedChannel<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        requested_min_rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
    ) -> Result<Self, ExtendedChannelError> {
        let target_u256 =
            match hash_rate_to_target(nominal_hashrate.into(), expected_share_per_minute.into()) {
                Ok(target_u256) => target_u256,
                Err(_) => {
                    return Err(ExtendedChannelError::InvalidNominalHashrate);
                }
            };

        let target: Target = target_u256.clone().into();

        if target > max_target {
            return Err(ExtendedChannelError::RequestedMaxTargetOutOfRange);
        }

        let available_rollable_extranonce_size =
            (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        if requested_min_rollable_extranonce_size > available_rollable_extranonce_size {
            return Err(ExtendedChannelError::RequestedMinExtranonceSizeTooLarge);
        }

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size: available_rollable_extranonce_size,
            requested_max_target: max_target,
            target,
            nominal_hashrate,
            future_template_to_job_id: HashMap::new(),
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            job_factory: JobFactory::new(version_rolling_allowed),
            share_accounting: ShareAccounting::new(share_batch_size),
            expected_share_per_minute,
            chain_tip: None,
        })
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

    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    pub fn get_shares_per_minute(&self) -> f32 {
        self.expected_share_per_minute
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Sets the extranonce prefix.
    ///
    /// Note: after this, all new jobs will be associated with the new extranonce prefix.
    /// Jobs created before this call will remain associated with the previous extranonce prefix,
    /// and share validation will be done accordingly.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: Vec<u8>,
    ) -> Result<(), ExtendedChannelError> {
        let new_rollable_extranonce_size =
            MAX_EXTRANONCE_LEN as u16 - extranonce_prefix.len() as u16;

        // we return an error if the new extranonce_prefix would violate
        // min_rollable_extranonce_size that was already established with the client when the
        // channel was created
        if new_rollable_extranonce_size < self.rollable_extranonce_size {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        self.extranonce_prefix = extranonce_prefix;
        self.rollable_extranonce_size = new_rollable_extranonce_size;

        Ok(())
    }

    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    pub fn get_requested_max_target(&self) -> &Target {
        &self.requested_max_target
    }

    pub fn get_target(&self) -> &Target {
        &self.target
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32> {
        &self.future_template_to_job_id
    }

    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    pub fn set_nominal_hashrate(&mut self, hashrate: f32) {
        self.nominal_hashrate = hashrate;
    }

    /// Updates the channel's nominal hashrate and target.
    ///
    /// If requested_max_target is None, we use the cached value in the channel state.
    pub fn update_channel(
        &mut self,
        new_nominal_hashrate: f32,
        requested_max_target: Option<Target>,
    ) -> Result<(), ExtendedChannelError> {
        let target_u256 = match hash_rate_to_target(
            new_nominal_hashrate.into(),
            self.expected_share_per_minute.into(),
        ) {
            Ok(target_u256) => target_u256,
            Err(_) => {
                return Err(ExtendedChannelError::InvalidNominalHashrate);
            }
        };

        let requested_max_target = match requested_max_target {
            Some(ref requested_max_target) => requested_max_target.clone(),
            None => self.requested_max_target.clone(),
        };

        // debug hex of target_u256 and max_Target
        // just like in share validation
        let mut target_bytes = target_u256.to_vec();
        target_bytes.reverse(); // Convert to big-endian for display
        let max_target_u256: binary_sv2::U256 = requested_max_target.clone().into();
        let mut max_target_bytes = max_target_u256.to_vec();
        max_target_bytes.reverse(); // Convert to big-endian for display

        // Get the old target for comparison on the debug log
        // Not really needed for the actual method functionality
        // But it's useful to have for debugging purposes
        let old_target_u256: binary_sv2::U256 = self.target.clone().into();
        let mut old_target_bytes = old_target_u256.to_vec();
        old_target_bytes.reverse(); // Convert to big-endian for display

        debug!(
            "updating channel target \nold target:\t{}\nnew target:\t{}\nmax_target:\t{}",
            bytes_to_hex(&old_target_bytes),
            bytes_to_hex(&target_bytes),
            bytes_to_hex(&max_target_bytes)
        );

        let new_target: Target = target_u256.into();

        if new_target > requested_max_target {
            return Err(ExtendedChannelError::RequestedMaxTargetOutOfRange);
        }

        self.nominal_hashrate = new_nominal_hashrate;
        self.target = new_target;
        self.requested_max_target = requested_max_target;

        Ok(())
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

    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates the channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    ///
    /// Only meant for usage on a Sv2 Pool Server or a Sv2 Job Declaration Client,
    /// but not on mining clients such as Mining Devices or Proxies.
    pub fn on_new_template(
        &mut self,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), ExtendedChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_extended_job(
                        self.channel_id,
                        None,
                        self.extranonce_prefix.clone(),
                        template.clone(),
                        coinbase_reward_outputs,
                    )
                    .map_err(ExtendedChannelError::JobFactoryError)?;
                let new_job_id = new_job.get_job_id();
                self.future_jobs.insert(new_job_id, new_job);
                self.future_template_to_job_id
                    .insert(template.template_id, new_job_id);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(ExtendedChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_extended_job(
                                self.channel_id,
                                Some(chain_tip),
                                self.extranonce_prefix.clone(),
                                template.clone(),
                                coinbase_reward_outputs,
                            )
                            .map_err(ExtendedChannelError::JobFactoryError)?;
                        // if there's already some active job, move it to the past jobs
                        // and set the new job as the active job
                        if let Some(active_job) = self.active_job.take() {
                            self.past_jobs.insert(active_job.get_job_id(), active_job);
                            self.active_job = Some(new_job);
                        } else {
                            // if there's no active job, simply set the new job as the active job
                            self.active_job = Some(new_job);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Updates the channel state with a new `SetNewPrevHash` message (Template Distribution
    /// Protocol variant).
    ///
    /// If there are no future jobs, returns an error.
    /// If there is some future job matching the `template_id`` that `SetNewPrevHash` points to,
    /// this future job is "activated" and set as the active job.
    ///
    /// All past jobs are cleared.
    ///
    /// The chain tip information is not kept in the channel state.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashTdp<'a>,
    ) -> Result<(), ExtendedChannelError> {
        match self.future_jobs.is_empty() {
            true => {
                return Err(ExtendedChannelError::TemplateIdNotFound);
            }
            false => {
                // the SetNewPrevHash message was addressed to a specific future template
                let future_job_id = self
                    .future_template_to_job_id
                    .remove(&set_new_prev_hash.template_id)
                    .ok_or(ExtendedChannelError::TemplateIdNotFound)?;

                // move currently active job to past jobs (so it can be marked as stale)
                let currently_active_job = self.active_job.take();
                if let Some(active_job) = currently_active_job {
                    self.past_jobs.insert(active_job.get_job_id(), active_job);
                }

                // activate the future job
                let mut activated_job = self
                    .future_jobs
                    .remove(&future_job_id)
                    .expect("future job must exist");

                activated_job.activate(set_new_prev_hash.header_timestamp);

                self.active_job = Some(activated_job);

                self.future_jobs.clear();
                self.future_template_to_job_id.clear();
            }
        }

        // mark all past jobs as stale, so that shares can be rejected with the appropriate error
        // code
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to validate shares for them
        self.past_jobs.clear();

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        // update the chain tip
        let set_new_prev_hash_static = set_new_prev_hash.into_static();
        let new_chain_tip = ChainTip::new(
            set_new_prev_hash_static.prev_hash,
            set_new_prev_hash_static.n_bits,
            set_new_prev_hash_static.header_timestamp,
        );
        self.chain_tip = Some(new_chain_tip);

        Ok(())
    }

    /// Updates the channel state with a new custom mining job.
    ///
    /// If there is an active job, it is moved to the past jobs.
    /// The new custom mining job is then set as the active job.
    ///
    /// Returns the job id of the new custom mining job.
    ///
    /// To be used by a Sv2 Pool Server upon receiving a `SetCustomMiningJob` message.
    pub fn on_set_custom_mining_job(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'a>,
    ) -> Result<u32, ExtendedChannelError> {
        let new_job = self
            .job_factory
            .new_custom_job(set_custom_mining_job, self.extranonce_prefix.clone())
            .map_err(ExtendedChannelError::JobFactoryError)?;

        let job_id = new_job.get_job_id();

        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }

        self.active_job = Some(new_job);

        Ok(job_id)
    }

    /// Validates a share.
    ///
    /// Updates the channel state with the result of the share validation.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtended,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.get_job_id() == job_id);

        // check if job_id is past job
        let is_past_job = self.past_jobs.contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.stale_jobs.contains_key(&job_id);

        if is_stale_job {
            return Err(ShareValidationError::Stale);
        }

        // if job_id is not active, past or stale, return error
        if !is_active_job && !is_past_job && !is_stale_job {
            return Err(ShareValidationError::InvalidJobId);
        }

        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else if is_past_job {
            self.past_jobs.get(&job_id).expect("past job must exist")
        } else {
            self.stale_jobs.get(&job_id).expect("stale job must exist")
        };

        let extranonce_prefix = job.get_extranonce_prefix();
        let mut full_extranonce = vec![];
        full_extranonce.extend(extranonce_prefix.clone());
        full_extranonce.extend(share.extranonce.inner_as_ref());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.get_coinbase_tx_prefix().inner_as_ref(),
            job.get_coinbase_tx_suffix().inner_as_ref(),
            full_extranonce.as_ref(),
            &job.get_merkle_path().inner_as_ref(),
        )
        .ok_or(ShareValidationError::Invalid)?
        .try_into()
        .expect("merkle root must be 32 bytes");

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        // validate when version rolling is not allowed
        if !job.version_rolling_allowed() {
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

            let mut coinbase = vec![];
            coinbase.extend(job.get_coinbase_tx_prefix().inner_as_ref());
            coinbase.extend(full_extranonce);
            coinbase.extend(job.get_coinbase_tx_suffix().inner_as_ref());

            match job.get_origin() {
                JobOrigin::NewTemplate(template) => {
                    let template_id = template.template_id;
                    return Ok(ShareValidationResult::BlockFound(
                        Some(template_id),
                        coinbase,
                    ));
                }
                JobOrigin::SetCustomMiningJob(_set_custom_mining_job) => {
                    return Ok(ShareValidationResult::BlockFound(None, coinbase));
                }
            }
        }

        // check if the share hash meets the channel target
        if hash_as_target <= self.target {
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

            let last_sequence_number = self.share_accounting.get_last_share_sequence_number();
            let new_submits_accepted_count = self.share_accounting.get_shares_accepted();
            let new_shares_sum = self.share_accounting.get_share_work_sum();

            // if sequence number is a multiple of share_batch_size
            // it's time to send a SubmitShares.Success
            if self.share_accounting.should_acknowledge() {
                Ok(ShareValidationResult::ValidWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ))
            } else {
                Ok(ShareValidationResult::Valid)
            }
        } else {
            Err(ShareValidationError::DoesNotMeetTarget)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::channels::{
        chain_tip::ChainTip,
        server::{
            error::ExtendedChannelError,
            extended::ExtendedChannel,
            jobs::JobOrigin,
            share_accounting::{ShareValidationError, ShareValidationResult},
        },
    };
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf};
    use codec_sv2::binary_sv2::Sv2Option;
    use mining_sv2::{NewExtendedMiningJob, SubmitSharesExtended, Target, MAX_EXTRANONCE_LEN};
    use std::convert::TryInto;
    use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

    const SATS_AVAILABLE_IN_TEMPLATE: u64 = 5000000000;

    #[test]
    fn test_future_job_activation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        assert!(channel.get_future_jobs().is_empty());
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();
        assert!(channel.get_active_job().is_none());

        let future_job_id = channel
            .get_future_template_to_job_id()
            .get(&template.template_id)
            .unwrap();

        let future_job = channel
            .get_future_jobs()
            .get(future_job_id)
            .unwrap()
            .clone();

        // we know that the provided template + coinbase_reward_outputs should generate this future
        // job
        let expected_job = NewExtendedMiningJob {
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

        assert_eq!(future_job.get_job_message(), &expected_job);

        let ntime = 1746839905;
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // we just activated the only future job
        assert!(channel.get_future_jobs().is_empty());

        let mut previously_future_job = future_job.clone();
        previously_future_job.activate(ntime);

        let activated_job = channel.get_active_job().unwrap();

        // assert that the activated job is the same as the previously future job
        assert_eq!(
            activated_job.get_job_message(),
            previously_future_job.get_job_message()
        );
    }

    #[test]
    fn test_non_future_job_creation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        let ntime = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let n_bits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        assert!(channel.get_future_jobs().is_empty());

        let active_job = channel.get_active_job().unwrap().clone();

        // we know that the provided template + coinbase_reward_outputs should generate this
        // non-future job
        let expected_job = NewExtendedMiningJob {
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

        assert_eq!(active_job.get_job_message(), &expected_job);
    }

    #[test]
    fn test_custom_job_creation_flow() {
        // this extended channel lives on JDC
        let jdc_channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        // this extended channel lives on JDC
        let mut jdc_extended_channel = ExtendedChannel::new(
            jdc_channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        jdc_extended_channel
            .on_new_template(template.clone(), coinbase_reward_outputs.clone())
            .unwrap();

        let ntime = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let n_bits = 503543726;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        jdc_extended_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();

        let jdc_active_job = jdc_extended_channel.get_active_job().unwrap().clone();

        let mining_job_token = vec![0].try_into().unwrap();
        let set_custom_mining_job = jdc_active_job
            .clone()
            .into_custom_job(0, mining_job_token, chain_tip)
            .unwrap();

        // this extended channel lives on Pool Mining Server
        let pool_channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        // this extended channel lives on Pool Mining Server
        let mut pool_extended_channel = ExtendedChannel::new(
            pool_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        pool_extended_channel
            .on_set_custom_mining_job(set_custom_mining_job.clone())
            .unwrap();

        let pool_active_job = pool_extended_channel.get_active_job().unwrap().clone();
        assert_eq!(
            pool_active_job.get_origin(),
            &JobOrigin::SetCustomMiningJob(set_custom_mining_job)
        );

        assert_eq!(
            pool_active_job.get_job_message(),
            jdc_active_job.get_job_message()
        );

        assert_eq!(pool_active_job.get_extranonce_prefix(), &extranonce_prefix);

        assert_eq!(
            pool_active_job.get_coinbase_outputs(),
            jdc_active_job.get_coinbase_outputs()
        );
    }

    #[test]
    fn test_coinbase_reward_outputs_sum_above_template_value() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);

        let invalid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE + 1), /* oops: one too many extra
                                                                      * sats */
            script_pubkey: script,
        }];

        let res = channel.on_new_template(template.clone(), invalid_coinbase_reward_outputs);

        assert!(res.is_err());
        assert!(channel.get_future_jobs().is_empty());
    }

    #[test]
    fn test_share_validation_block_found() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        // channel target: 04325c53ef368eb04325c53ef368eb04325c53ef368eb04325c53ef368eb0431

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 00009a270ad03f1256312c7f196ab1a66bf8951f282fc75d9c81393cbb6427a8
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

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound(_, _))));
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        // channel target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 6f33ea329093baa13e37d11b3afa91960f8d84f0ec064c1376522548c0852d79
        // which does not meet the channel target
        // 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
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
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        // channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network tarkget is: 000000000000d7c0000000000000000000000000000000000000000000000000
        let n_bits = 453040064;
        let ntime = 1745611105;
        let prev_hash = [
            23, 205, 72, 134, 153, 86, 220, 153, 224, 28, 216, 146, 228, 120, 227, 157, 213, 99,
            160, 163, 128, 59, 139, 190, 158, 62, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 0001099d7c957a0502952177aada0254921f04306a174543389263d1dd487cce
        // which does meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 159386,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);
        assert!(matches!(res, Ok(ShareValidationResult::Valid)));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 159386,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(repeated_share);

        // assert duplicate share is rejected
        assert!(matches!(res, Err(ShareValidationError::DuplicateShare)));
    }

    #[test]
    fn test_update_channel() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let expected_share_per_minute = 1.0;
        let initial_hashrate = 10.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        // this is the most permissive possible max_target
        let max_target: Target = [0xff; 32].into();

        // Create a channel with initial hashrate
        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target.clone(),
            initial_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        // Get the initial target
        let initial_target = channel.get_target().clone();

        // Update the channel with a new hashrate (higher)
        let new_hashrate = 100.0;
        channel
            .update_channel(new_hashrate, Some(max_target.clone()))
            .unwrap();

        // Get the new target after update
        let new_target = channel.get_target().clone();

        // The target should be different after updating with a different hashrate
        // old target: 006d0b803685c01b42e00da17006d0b803685c01b42e00da17006d0b803685bf
        // new target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        assert_ne!(initial_target, new_target);

        // The nominal hashrate should be updated
        assert_eq!(channel.get_nominal_hashrate(), new_hashrate);

        // Test invalid hashrate (negative)
        let result = channel.update_channel(-1.0, Some(max_target.clone()));
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ExtendedChannelError::InvalidNominalHashrate)
        ));

        // Create a not so permissive max_target so we can test a target that exceeds it
        let not_so_permissive_max_target: Target = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00,
        ]
        .into();

        // Try to update with a hashrate that would result in a target exceeding the max_target
        // new target: 2492492492492492492492492492492492492492492492492492492492492491
        // max target: 00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let very_small_hashrate = 0.1;
        let result = channel.update_channel(
            very_small_hashrate,
            Some(not_so_permissive_max_target.clone()),
        );
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ExtendedChannelError::RequestedMaxTargetOutOfRange)
        ));

        // Test successful update with not_so_permissive_max_target
        // new target: 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // max target: 00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let sufficiently_big_hashrate = 1000.0;
        let result = channel.update_channel(
            sufficiently_big_hashrate,
            Some(not_so_permissive_max_target),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_extranonce_prefix() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = (MAX_EXTRANONCE_LEN - extranonce_prefix.len()) as u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
        )
        .unwrap();

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &extranonce_prefix);

        let new_extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 2,
        ]
        .to_vec();

        channel
            .set_extranonce_prefix(new_extranonce_prefix.clone())
            .unwrap();
        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &new_extranonce_prefix);

        let new_extranonce_prefix_too_large = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 2, 0,
        ]
        .to_vec();

        // try to set a new extranonce_prefix that is too large
        let result = channel.set_extranonce_prefix(new_extranonce_prefix_too_large.clone());
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ExtendedChannelError::NewExtranoncePrefixTooLarge)
        ));
    }
}
