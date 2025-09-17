//! Sv2 Standard Channel - Mining Server Abstraction.
//!
//! This module provides the [`StandardChannel`] struct, which models and manages the state of a
//! Stratum V2 (SV2) standard channel as maintained on a mining server.
//!
//! ## Responsibilities
//!
//! `StandardChannel` is responsible for managing all the state associated with an SV2 standard
//! channel, including:
//!
//! - **Channel Parameters**: Unique `channel_id`, `user_identity`, `extranonce_prefix`, maximum
//!   target, nominal hashrate, and other properties negotiated at channel opening.
//! - **Target Difficulty**: Maintains both the requested maximum target and the current working
//!   target for the channel, recalculated as hashrate or share rate changes.
//! - **Job Lifecycle Management**: Manages active, future, past, and stale jobs, including
//!   activation on new chain tips and template updates.
//! - **Share Validation and Accounting**: Validates submitted shares, updates share accounting
//!   state, detects duplicates, and manages batch acknowledgements for SV2 `SubmitShares.Success`
//!   responses.
//! - **Chain Tip Management**: Tracks the latest known chain tip (block height, previous hash,
//!   timestamp, and target) for constructing headers and validating shares.
//!
//! ## Usage
//!
//! Intended for use by pool servers or SV2-compliant job declaration clients (JDC), not by mining
//! devices or proxies. Encapsulates logic for handling SV2 messages such as `NewTemplate`,
//! `SetNewPrevHash`, and `SubmitSharesStandard`.
//!
//! ## Notes
//!
//! - Only one active job is allowed at a time. When a chain tip updates, jobs from the previous tip
//!   become stale and are tracked accordingly.
//! - Share batch acknowledgment logic is tied to the configured batch size.
//! - Extranonce prefix updates must be consistent with SV2 protocol constraints.
//! - Job lifecycle and share accounting are managed on a per-channel basis.
use crate::{
    chain_tip::ChainTip,
    server::{
        error::StandardChannelError,
        jobs::{
            extended::ExtendedJob, factory::JobFactory, job_store::JobStore, standard::StandardJob,
        },
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    target::{bytes_to_hex, hash_rate_to_target, target_to_difficulty, u256_to_block_hash},
};
use binary_sv2::{self};
use bitcoin::{
    absolute::LockTime,
    blockdata::{
        block::{Header, Version},
        witness::Witness,
    },
    consensus::Encodable,
    hashes::sha256d::Hash,
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version as TxVersion},
    CompactTarget, Sequence, Target as BitcoinTarget,
};
use mining_sv2::{SubmitSharesStandard, Target, MAX_EXTRANONCE_LEN};
use std::{collections::HashMap, convert::TryInto, marker::PhantomData};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::debug;

/// Abstraction of a Sv2 Standard Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's requested max target (limit established by the client)
/// - the channel's target
/// - the channel's nominal hashrate
/// - the channel's active job
/// - the channel's future jobs (indexed by `template_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the channel's past jobs (which were active jobs under the current chain tip, indexed by
///   `job_id`)
/// - the channel's stale jobs (which were past and active jobs under the previous chain tip,
///   indexed by `job_id`)
/// - the channel's job factory
/// - the channel's chain tip
#[derive(Debug)]
pub struct StandardChannel<'a, J>
where
    J: JobStore<StandardJob<'a>>,
{
    pub channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    requested_max_target: Target,
    target: Target,
    nominal_hashrate: f32,
    share_accounting: ShareAccounting,
    expected_share_per_minute: f32,
    job_store: J,
    job_factory: JobFactory,
    chain_tip: Option<ChainTip>,
    phantom: PhantomData<&'a ()>,
}

impl<'a, J> StandardChannel<'a, J>
where
    J: JobStore<StandardJob<'a>>,
{
    /// Constructor of `StandardChannel` for a Sv2 Pool Server.
    /// Not meant for usage on a Sv2 Job Declaration Client.
    ///
    /// Initializes the standard channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met.
    ///
    /// For non-JD jobs, `pool_tag_string` is added to the coinbase scriptSig in between `/`
    /// and `//` delimiters: `/pool_tag_string//`
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_pool(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: J,
        pool_tag_string: String,
    ) -> Result<Self, StandardChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            requested_max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            Some(pool_tag_string),
            None,
        )
    }

    /// Constructor of `StandardChannel` for a Sv2 Job Declaration Client.
    /// Not meant for usage on a Sv2 Pool Server.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met.
    ///
    /// The `pool_tag_string` and `miner_tag_string` are added to the coinbase scriptSig in between
    /// `/` delimiters: `/pool_tag_string/miner_tag_string/`
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_job_declaration_client(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: J,
        pool_tag_string: Option<String>,
        miner_tag_string: String,
    ) -> Result<Self, StandardChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            requested_max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            pool_tag_string,
            Some(miner_tag_string),
        )
    }

    // private constructor
    #[allow(clippy::too_many_arguments)]
    fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: J,
        pool_tag_string: Option<String>,
        miner_tag_string: Option<String>,
    ) -> Result<Self, StandardChannelError> {
        let calculated_target =
            match hash_rate_to_target(nominal_hashrate.into(), expected_share_per_minute.into()) {
                Ok(target_u256) => target_u256,
                Err(_) => {
                    return Err(StandardChannelError::InvalidNominalHashrate);
                }
            };

        let target: Target = calculated_target.into();

        if target > requested_max_target {
            return Err(StandardChannelError::RequestedMaxTargetOutOfRange);
        }

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            requested_max_target,
            target,
            nominal_hashrate,
            share_accounting: ShareAccounting::new(share_batch_size),
            expected_share_per_minute,
            job_factory: JobFactory::new(true, pool_tag_string, miner_tag_string),
            chain_tip: None,
            job_store,
            phantom: PhantomData,
        })
    }

    /// Returns the unique channel ID for this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string for this channel.
    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    /// Returns the extranonce prefix bytes.
    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    /// Sets a new extranonce prefix for this channel.
    ///
    /// Returns an error if the new prefix is too large.
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
    /// Updates the current target for this channel.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }
    /// Updates the nominal hashrate for this channel.
    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }
    /// Returns the requested maximum target for this channel.
    pub fn get_requested_max_target(&self) -> &Target {
        &self.requested_max_target
    }
    /// Returns the current target for this channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }
    /// Returns the nominal hashrate for this channel.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Updates channel configuration with a new nominal hashrate.
    ///
    /// Adjusts target difficulty and internal state. Returns an error if
    /// any input parameters are invalid or constraints are violated.
    ///
    /// This can be used in two scenarios:
    /// - Client sent `UpdateChannel` message, which contains a `requested_max_target` parameter
    ///   that's also used as input.
    /// - vardiff algorithm estimated a new nominal hashrate, in which case `requested_max_target`
    ///   is `None` and we use the value from the channel state (that was set either during channel
    ///   opening or some previous `UpdateChannel` message).
    pub fn update_channel(
        &mut self,
        nominal_hashrate: f32,
        requested_max_target: Option<Target>,
    ) -> Result<(), StandardChannelError> {
        let target_u256 = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute.into(),
        ) {
            Ok(target_u256) => target_u256,
            Err(_) => {
                return Err(StandardChannelError::InvalidNominalHashrate);
            }
        };

        let requested_max_target = match requested_max_target {
            Some(ref requested_max_target) => requested_max_target.clone(),
            None => self.requested_max_target.clone(),
        };

        // debug hex of target_u256 and max_target
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
            return Err(StandardChannelError::RequestedMaxTargetOutOfRange);
        }

        self.target = new_target;
        self.nominal_hashrate = nominal_hashrate;
        self.requested_max_target = requested_max_target;
        Ok(())
    }
    /// Returns the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&StandardJob<'a>> {
        self.job_store.get_active_job()
    }
    /// Returns the mapping of future template IDs to job IDs.
    pub fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32> {
        self.job_store.get_future_template_to_job_id()
    }

    /// Returns all future jobs for this channel.
    pub fn get_future_jobs(&self) -> &HashMap<u32, StandardJob<'a>> {
        self.job_store.get_future_jobs()
    }

    /// Returns all past jobs for this channel.
    pub fn get_past_jobs(&self) -> &HashMap<u32, StandardJob<'a>> {
        self.job_store.get_past_jobs()
    }

    /// Returns all stale jobs for this channel.
    pub fn get_stale_jobs(&self) -> &HashMap<u32, StandardJob<'a>> {
        self.job_store.get_stale_jobs()
    }

    /// Returns the expected number of shares per minute for this channel.
    pub fn get_shares_per_minute(&self) -> f32 {
        self.expected_share_per_minute
    }

    /// Returns the current chain tip, if set.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Returns a reference to the share accounting state for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates the channel state with a new job.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    ///
    /// Only meant for usage on a Sv2 Pool Server or a Sv2 Job Declaration Client,
    /// but not on mining clients such as Mining Devices or Proxies.
    ///
    /// Only meant to be used in case we want to broadcast standard jobs.
    /// In case we want to broadcast extended jobs via group channel, use `on_group_channel_job`
    /// instead.
    pub fn on_new_template(
        &mut self,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), StandardChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_standard_job(
                        self.channel_id,
                        None,
                        self.extranonce_prefix.clone(),
                        template.clone(),
                        coinbase_reward_outputs,
                    )
                    .map_err(StandardChannelError::JobFactoryError)?;
                self.job_store.add_future_job(template.template_id, new_job);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(StandardChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_standard_job(
                                self.channel_id,
                                Some(chain_tip),
                                self.extranonce_prefix.clone(),
                                template.clone(),
                                coinbase_reward_outputs,
                            )
                            .map_err(StandardChannelError::JobFactoryError)?;
                        self.job_store.add_active_job(new_job);
                    }
                }
            }
        }

        Ok(())
    }

    /// Used as an alternative to `on_new_template` when an extended job is meant to be broadcast
    /// to the group channel, instead of multiple standard jobs to diffferent standard channels.
    ///
    /// We use this method to update the channel state, so it can validate share from the job that
    /// was broadcasted to the group channel.
    pub fn on_group_channel_job(
        &mut self,
        extended_job: ExtendedJob<'a>,
    ) -> Result<(), StandardChannelError> {
        let standard_job = extended_job
            .into_standard_job(self.channel_id, self.extranonce_prefix.clone())
            .map_err(|_| StandardChannelError::FailedToConvertToStandardJob)?;

        match standard_job.is_future() {
            true => {
                self.job_store
                    .add_future_job(standard_job.get_template().template_id, standard_job);
            }
            false => {
                self.job_store.add_active_job(standard_job);
            }
        }

        Ok(())
    }

    /// Updates the channel state with a new `SetNewPrevHash` message.
    ///
    /// If there are no future jobs, returns an error.
    /// If there are future jobs, the active job is set to the job with the given `template_id`.
    ///
    /// All past jobs are cleared.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash<'a>,
    ) -> Result<(), StandardChannelError> {
        match self.job_store.get_future_jobs().is_empty() {
            true => {
                return Err(StandardChannelError::TemplateIdNotFound);
            }
            false => {
                if !self.job_store.activate_future_job(
                    set_new_prev_hash.template_id,
                    set_new_prev_hash.header_timestamp,
                ) {
                    return Err(StandardChannelError::TemplateIdNotFound);
                }
            }
        }

        // update the chain tip
        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Validates a submitted share and updates accounting state.
    ///
    /// Returns the result of share validation, including block found, valid share, duplicate, or
    /// error if the share is stale or does not meet target.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesStandard,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .job_store
            .get_active_job()
            .is_some_and(|job| job.get_job_id() == job_id);

        // check if job_id is past job
        let is_past_job = self.job_store.get_past_jobs().contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.job_store.get_stale_jobs().contains_key(&job_id);

        if is_stale_job {
            return Err(ShareValidationError::Stale);
        }

        // if job_id is not active, past or stale, return error
        if !is_active_job && !is_past_job && !is_stale_job {
            return Err(ShareValidationError::InvalidJobId);
        }

        let job = if is_active_job {
            self.job_store
                .get_active_job()
                .expect("active job must exist")
        } else if is_past_job {
            self.job_store
                .get_past_jobs()
                .get(&job_id)
                .expect("past job must exist")
        } else {
            self.job_store
                .get_stale_jobs()
                .get(&job_id)
                .expect("stale job must exist")
        };

        let merkle_root: [u8; 32] = job
            .get_merkle_root()
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

            let op_pushbytes_pool_miner_tag = self
                .job_factory
                .op_pushbytes_pool_miner_tag()
                .map_err(|_| ShareValidationError::InvalidCoinbase)?;

            let mut script_sig = job.get_template().coinbase_prefix.to_vec();
            script_sig.extend(op_pushbytes_pool_miner_tag);
            script_sig.push(MAX_EXTRANONCE_LEN as u8); // OP_PUSHBYTES_32 (for the extranonce)
            script_sig.extend(job.get_extranonce_prefix());

            let tx_in = TxIn {
                previous_output: OutPoint::null(),
                script_sig: script_sig.into(),
                sequence: Sequence(job.get_template().coinbase_tx_input_sequence),
                witness: Witness::from(vec![vec![0; 32]]),
            };

            let coinbase = Transaction {
                version: TxVersion::non_standard(job.get_template().coinbase_tx_version as i32),
                lock_time: LockTime::from_consensus(job.get_template().coinbase_tx_locktime),
                input: vec![tx_in],
                output: job.get_coinbase_outputs().to_vec(),
            };
            let mut serialized_coinbase = Vec::new();
            coinbase
                .consensus_encode(&mut serialized_coinbase)
                .map_err(|_| ShareValidationError::InvalidCoinbase)?;

            return Ok(ShareValidationResult::BlockFound(
                Some(job.get_template().template_id),
                serialized_coinbase,
            ));
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
    use crate::{
        chain_tip::ChainTip,
        server::{
            error::StandardChannelError,
            jobs::{job_store::DefaultJobStore, standard::StandardJob},
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
    };
    use binary_sv2::Sv2Option;
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf};
    use mining_sv2::{NewMiningJob, SubmitSharesStandard, Target};
    use std::convert::TryInto;
    use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp};

    const SATS_AVAILABLE_IN_TEMPLATE: u64 = 5000000000;

    #[test]
    fn test_future_job_activation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let max_target: Target = [0xff; 32].into();
        let nominal_hashrate = 10.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
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

        assert!(standard_channel.get_future_jobs().is_empty());

        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let expected_future_standard_job = NewMiningJob {
            channel_id: standard_channel_id,
            job_id: 1,
            merkle_root: [
                213, 241, 108, 144, 69, 96, 29, 8, 222, 2, 135, 14, 213, 87, 81, 21, 140, 98, 42,
                221, 221, 174, 219, 248, 106, 52, 168, 88, 18, 146, 186, 71,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        let future_standard_job_from_channel =
            standard_channel.get_future_jobs().get(&1).unwrap().clone();
        assert_eq!(
            future_standard_job_from_channel.get_job_message(),
            &expected_future_standard_job
        );

        let ntime = 1747092633;
        let set_new_prev_hash = SetNewPrevHashTdp {
            template_id: template.template_id,
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

        standard_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();
        let mut previously_future_job = future_standard_job_from_channel.clone();
        previously_future_job.activate(ntime);

        let activated_job = standard_channel.get_active_job().unwrap();

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

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let max_target: Target = [0xff; 32].into();
        let nominal_hashrate = 10.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;

        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        let ntime = 1747092633;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let nbits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, nbits, ntime);
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
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

        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let expected_active_standard_job = NewMiningJob {
            channel_id: standard_channel_id,
            job_id: 1,
            merkle_root: [
                213, 241, 108, 144, 69, 96, 29, 8, 222, 2, 135, 14, 213, 87, 81, 21, 140, 98, 42,
                221, 221, 174, 219, 248, 106, 52, 168, 88, 18, 146, 186, 71,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(Some(ntime)),
        };

        let active_standard_job_from_channel = standard_channel.get_active_job().unwrap().clone();

        assert_eq!(
            active_standard_job_from_channel.get_job_message(),
            &expected_active_standard_job
        );
    }

    #[test]
    fn test_share_validation_block_found() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target: Target = [0xff; 32].into();
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;

        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        // channel target: 04325c53ef368eb04325c53ef368eb04325c53ef368eb04325c53ef368eb0431
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
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

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();

        // this share has hash 3c34f63de61283c907b68e3127146d7d11f1fb14e50020a8317a292d11e2dab6
        // which satisfied the network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesStandard {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_standard_job.get_job_id(),
            nonce: 0,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = standard_channel.validate_share(share_valid_block);

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound(_, _))));
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target: Target = [0xff; 32].into();
        let nominal_hashrate = 100.0; // bigger hashrate to get higher difficulty
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;

        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        // channel target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
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

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();

        // this share has hash a5b65006d89dab9de2b23ececd3b0435f163607f7da1ba2f0bcde62b29e8cd44
        // which does not meet the channel target
        // 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let share_low_diff = SubmitSharesStandard {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_standard_job.get_job_id(),
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = standard_channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget
        ));
    }

    #[test]
    fn test_share_validation_valid_share() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target: Target = [0xff; 32].into();
        let nominal_hashrate = 1_000.0; // bigger hashrate to get higher difficulty
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;

        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        // channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
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

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 0000d603073772ba60af5922486242a6adb74cdf5baec768c7bd684977852cd8
        // which does meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesStandard {
            channel_id: standard_channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 134870,
            ntime: 1745611105,
            version: 536870912,
        };
        let res = standard_channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid)));
    }

    #[test]
    fn test_update_channel() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let expected_share_per_minute = 1.0;
        let initial_hashrate = 10.0;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::<StandardJob>::new();
        // this is the most permissive possible max_target
        let max_target: Target = [0xff; 32].into();

        // Create a channel with initial hashrate
        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target.clone(),
            initial_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
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
            Err(StandardChannelError::InvalidNominalHashrate)
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
            Err(StandardChannelError::RequestedMaxTargetOutOfRange)
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
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = [0xff; 32].into();
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::<StandardJob>::new();

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            None,
            None,
        )
        .unwrap();

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &extranonce_prefix);

        let new_extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ]
        .to_vec();

        channel
            .set_extranonce_prefix(new_extranonce_prefix.clone())
            .unwrap();
        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &new_extranonce_prefix);

        let new_extranonce_prefix_too_long = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1,
        ]
        .to_vec();
        assert!(channel
            .set_extranonce_prefix(new_extranonce_prefix_too_long)
            .is_err());
    }
}
