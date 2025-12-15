//! # SV2 Extended Channel - Mining Server Abstraction.
//!
//! This module defines the [`ExtendedChannel`] struct, which provides an abstraction of a SV2
//! extended channel as maintained by a mining pool server.
//!
//! ## Responsibilities
//!
//! `ExtendedChannel` is responsible for managing all the state associated with an SV2 extended
//! channel, including:
//!
//! - **Channel Parameters**: Holds the unique `channel_id`, `user_identity`, `extranonce_prefix`,
//!   and other parameters negotiated during channel opening.
//! - **Target Difficulty**: Manages the target difficulty (`target`) and maximum allowed target
//!   (`requested_max_target`), based on client requests and nominal hashrate.
//! - **Job Lifecycle Management**: Stores jobs received from new templates or custom job messages,
//!   including:
//!   - Future jobs (indexed by `template_id`)
//!   - Active job (currently being mined)
//!   - Past and stale jobs (for share validation over time)
//! - **Share Validation and Accounting**: Validates shares submitted by the miner, updating
//!   internal accounting and detecting duplicates or stale submissions. Determines if a share meets
//!   the channel or network target and responds accordingly.
//! - **Chain Tip Management**: Tracks the latest known chain tip (previous hash, timestamp, and
//!   target) for constructing headers and validating shares.
//! - **Version Rolling**: Honors server configuration on whether version rolling is permitted,
//!   validating submitted BIP320 header versions accordingly.
//!
//! ## Usage
//!
//! This struct is intended for use on the **pool server side** or by SV2-compliant job declaration
//! clients. It encapsulates logic for responding to SV2 messages such as `NewTemplate`,
//! `SetNewPrevHash`, `SetCustomMiningJob`, and `SubmitSharesExtended`.
//!
//! ## Notes
//!
//! - Only one active job is allowed at a time. Jobs from a previous chain tip become stale when a
//!   new chain tip is set.
//! - Share acknowledgment logic is tied to a configured batch size (e.g., every `N` valid shares).
//! - Extranonce validation supports dynamic updates of `extranonce_prefix` but enforces consistency
//!   with previously agreed parameters.

use crate::{
    chain_tip::ChainTip,
    server::{
        error::ExtendedChannelError,
        jobs::{
            either_job::{validate_either_share, EitherJob, EitherShare},
            factory::JobFactory,
            job_store::{JobLifecycleState, JobStore},
            Job,
        },
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    target::{bytes_to_hex, hash_rate_to_target},
    MAX_EXTRANONCE_PREFIX_LEN,
};
use bitcoin::{hashes::Hash, transaction::TxOut, Target};
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended};
use std::marker::PhantomData;
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
#[derive(Debug)]
pub struct ExtendedChannel<'a, JobIn, JobOut, Store>
where
    Store: JobStore<'a, JobIn, JobOut>,
    JobIn: Job<'a> + From<EitherJob<'a>> + Clone,
    JobOut: Job<'a> + Clone,
{
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    requested_max_target: Target,
    target: Target, // todo: try to use Target from rust-bitcoin
    nominal_hashrate: f32,
    job_store: Store,
    job_factory: JobFactory,
    share_accounting: ShareAccounting,
    expected_share_per_minute: f32,
    chain_tip: Option<ChainTip>,
    _phantom: PhantomData<(&'a JobIn, &'a JobOut)>,
}

impl<'a, JobIn, JobOut, Store> ExtendedChannel<'a, JobIn, JobOut, Store>
where
    Store: JobStore<'a, JobIn, JobOut>,
    JobIn: Job<'a> + From<EitherJob<'a>> + Clone,
    JobOut: Job<'a> + Clone,
{
    /// Constructor of `ExtendedChannel` for a Sv2 Pool Server.
    /// Not meant for usage on a Sv2 Job Declaration Client.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
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
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: Store,
        pool_tag_string: String,
    ) -> Result<Self, ExtendedChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            job_store,
            Some(pool_tag_string),
            None,
        )
    }

    /// Constructor of `ExtendedChannel` for a Sv2 Job Declaration Client.
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
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: Store,
        pool_tag_string: Option<String>,
        miner_tag_string: String,
    ) -> Result<Self, ExtendedChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
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
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        job_store: Store,
        pool_tag: Option<String>,
        miner_tag: Option<String>,
    ) -> Result<Self, ExtendedChannelError> {
        let target =
            match hash_rate_to_target(nominal_hashrate.into(), expected_share_per_minute.into()) {
                Ok(target) => target,
                Err(_) => {
                    return Err(ExtendedChannelError::InvalidNominalHashrate);
                }
            };

        if target > max_target {
            println!("target: {:?}", target.to_be_bytes());
            println!("max_target: {:?}", max_target.to_be_bytes());
            return Err(ExtendedChannelError::RequestedMaxTargetOutOfRange);
        }

        if extranonce_prefix.len() > MAX_EXTRANONCE_PREFIX_LEN {
            return Err(ExtendedChannelError::ExtranoncePrefixTooLarge);
        }

        let script_sig_size = 5 + // BIP34
            1 + // OP_PUSHBYTES
            3 + // `/` delimiters
            pool_tag.as_ref().map_or(0, |s| s.len()) +
            miner_tag.as_ref().map_or(0, |s| s.len()) +
            1 + // OP_PUSHBYTES
            extranonce_prefix.len() +
            rollable_extranonce_size as usize;

        if script_sig_size > 100 {
            return Err(ExtendedChannelError::ScriptSigSizeTooLarge);
        }

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            requested_max_target: max_target,
            target,
            nominal_hashrate,
            job_store,
            job_factory: JobFactory::new(version_rolling_allowed, pool_tag, miner_tag),
            share_accounting: ShareAccounting::new(share_batch_size),
            expected_share_per_minute,
            chain_tip: None,
            _phantom: PhantomData,
        })
    }

    /// Returns the unique channel ID for this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string associated with this channel.
    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    /// Returns the extranonce prefix bytes for this channel.
    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    /// Returns the current chain tip, if set.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Returns the expected number of shares per minute configured for this channel.
    pub fn get_shares_per_minute(&self) -> f32 {
        self.expected_share_per_minute
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Updates the extranonce prefix for this channel.
    ///
    /// After this call, all newly created jobs will reference the new prefix.
    /// Jobs created before the update will continue to use the previous prefix,
    /// and share validation will be performed accordingly.
    ///
    /// Returns an error if the new extranonce prefix is too large.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: Vec<u8>,
    ) -> Result<(), ExtendedChannelError> {
        if extranonce_prefix.len() > MAX_EXTRANONCE_PREFIX_LEN {
            return Err(ExtendedChannelError::ExtranoncePrefixTooLarge);
        }

        self.extranonce_prefix = extranonce_prefix;

        Ok(())
    }

    /// Returns the number of bytes available for the rollable portion of the extranonce.
    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    /// Returns the full extranonce size in bytes.
    pub fn get_full_extranonce_size(&self) -> usize {
        self.extranonce_prefix.len() + self.rollable_extranonce_size as usize
    }

    /// Returns the requested maximum target for this channel.
    pub fn get_requested_max_target(&self) -> &Target {
        &self.requested_max_target
    }

    /// Returns the current target for this channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Updates the current target for this channel.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Returns the job ID for a future job from a template ID, if any.
    pub fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.job_store
            .get_future_job_id_from_template_id(template_id)
    }

    /// Returns the nominal hashrate for this channel.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Updates the nominal hashrate for this channel.
    pub fn set_nominal_hashrate(&mut self, hashrate: f32) {
        self.nominal_hashrate = hashrate;
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
        new_nominal_hashrate: f32,
        requested_max_target: Option<Target>,
    ) -> Result<(), ExtendedChannelError> {
        let target = match hash_rate_to_target(
            new_nominal_hashrate.into(),
            self.expected_share_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => {
                return Err(ExtendedChannelError::InvalidNominalHashrate);
            }
        };

        let requested_max_target = match requested_max_target {
            Some(ref requested_max_target) => requested_max_target,
            None => &self.requested_max_target,
        };

        // debug hex of target_u256 and max_Target
        // just like in share validation
        // big-endian for display
        let target_bytes = target.to_be_bytes();
        let max_target = requested_max_target;
        let max_target_bytes = max_target.to_be_bytes();

        // Get the old target for comparison on the debug log
        // Not really needed for the actual method functionality
        // But it's useful to have for debugging purposes
        let old_target = self.target;
        let old_target_bytes = old_target.to_be_bytes();

        debug!(
            "updating channel target \nold target:\t{}\nnew target:\t{}\nmax_target:\t{}",
            bytes_to_hex(&old_target_bytes),
            bytes_to_hex(&target_bytes),
            bytes_to_hex(&max_target_bytes)
        );

        let new_target: Target = target;

        if new_target > *requested_max_target {
            return Err(ExtendedChannelError::RequestedMaxTargetOutOfRange);
        }

        self.nominal_hashrate = new_nominal_hashrate;
        self.target = new_target;
        self.requested_max_target = *requested_max_target;

        Ok(())
    }

    pub fn remove_future_job(&mut self, job_id: u32) -> Option<JobIn> {
        self.job_store.remove_future_job(job_id)
    }

    /// Returns a reference to the share accounting state for this channel.
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
    ) -> Result<JobLifecycleState<JobIn, JobOut>, ExtendedChannelError> {
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
                        self.get_full_extranonce_size(),
                    )
                    .map_err(ExtendedChannelError::JobFactoryError)?;
                let job_id = new_job.get_job_id();
                self.job_store
                    .add_future_job(template.template_id, JobIn::from(new_job));
                return Ok(self.job_store.get_job(job_id));
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
                                self.get_full_extranonce_size(),
                            )
                            .map_err(ExtendedChannelError::JobFactoryError)?;
                        let job_id = new_job.get_job_id();
                        self.job_store.add_active_job(JobIn::from(new_job));
                        return Ok(self.job_store.get_job(job_id));
                    }
                }
            }
        }
    }

    /// Updates the channel state with a new `SetNewPrevHash` message (Template Distribution
    /// Protocol variant).
    ///
    /// If there are future jobs in the Job Store, it activates the future job matching the
    /// `template_id` and sets it as the active job.
    ///
    /// If there are future jobs in the Job Store, but the template id is not found, returns an
    /// error.
    ///
    /// All past jobs are cleared.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashTdp<'a>,
    ) -> Result<JobLifecycleState<JobIn, JobOut>, ExtendedChannelError> {
        self.job_store.mark_past_jobs_as_stale();
        let job_id = self
            .job_store
            .get_future_job_id_from_template_id(set_new_prev_hash.template_id)
            .ok_or(ExtendedChannelError::TemplateIdNotFound)?;

        // try to activate the future job, and also mark past jobs as stale
        if !self.job_store.activate_future_job(
            set_new_prev_hash.template_id,
            set_new_prev_hash.header_timestamp,
        ) {
            return Err(ExtendedChannelError::TemplateIdNotFound);
        }
        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        // update the chain tip
        self.chain_tip = Some(set_new_prev_hash.into());
        Ok(self.job_store.get_job(job_id))
    }

    /// Updates the channel state with a new custom mining job.
    ///
    /// If there is an active job, it is moved to the past jobs.
    /// The new custom mining job is then set as the active job.
    ///
    /// Assumes SetCustomMiningJob.{prev_hash, nbits, min_ntime} have already been validated.
    /// Updates the channel's `ChainTip``.
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
            .new_extended_job_from_custom_job(
                set_custom_mining_job.clone(),
                self.extranonce_prefix.clone(),
                self.get_full_extranonce_size(),
            )
            .map_err(ExtendedChannelError::JobFactoryError)?;

        let job_id = new_job.get_job_id();

        self.job_store.add_active_job(JobIn::from(new_job));

        // update the chain tip
        let set_custom_mining_job_static = set_custom_mining_job.into_static();
        let prev_hash = set_custom_mining_job_static.prev_hash;
        let nbits = set_custom_mining_job_static.nbits;
        let min_ntime = set_custom_mining_job_static.min_ntime;
        let new_chain_tip = ChainTip::new(prev_hash, nbits, min_ntime);
        self.chain_tip = Some(new_chain_tip);

        Ok(job_id)
    }

    /// Validates a share.
    ///
    /// Updates the channel state with the result of the share validation.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtended,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let either_share = EitherShare::from(share);
        let job_id = either_share.job_id;
        self.job_store.try_validate_job(job_id, |job| {
            let job = match job {
                JobLifecycleState::Active(job) => job,
                JobLifecycleState::Past(job) => job,
                JobLifecycleState::Stale(_job) => {
                    return Err(ShareValidationError::Stale);
                }
                JobLifecycleState::Future(_) => {
                    return Err(ShareValidationError::InvalidJobId);
                }
                JobLifecycleState::NotFound => {
                    return Err(ShareValidationError::InvalidJobId);
                }
            };

            let result = validate_either_share(
                job,
                &either_share,
                self.chain_tip
                    .as_ref()
                    .ok_or(ShareValidationError::NoChainTip)?,
                &self.target,
                Some(self.rollable_extranonce_size),
            );

            // extract hash from result or return result early
            let hash = match result.as_ref() {
                Ok(ShareValidationResult::Valid(hash)) => hash,
                Ok(ShareValidationResult::BlockFound(hash, _template_id, _coinbase)) => hash,
                Err(_validation_error) => return result,
            };
            let hash_as_target = Target::from_le_bytes(hash.to_byte_array());
            // check for duplicate shares
            if self.share_accounting.is_share_seen(&hash_as_target) {
                return Err(ShareValidationError::DuplicateShare);
            }
            // update share accounting

            self.share_accounting
                .update_share_accounting(either_share.sequence_number, hash_as_target);

            // update the best diff
            self.share_accounting
                .update_best_diff(hash_as_target.difficulty_float());
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Job;
    use crate::{
        chain_tip::ChainTip,
        server::{
            error::ExtendedChannelError,
            extended::ExtendedChannel,
            jobs::{
                job_store::{DefaultJobStore, JobLifecycleState, JobStore},
                JobMessage,
            },
            share_accounting::{ShareValidationError, ShareValidationResult},
        },
    };
    use binary_sv2::Sv2Option;
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf, Target};
    use mining_sv2::{NewExtendedMiningJob, SubmitSharesExtended};
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
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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

        assert!(!channel.job_store.has_future_jobs());
        let job = channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        if let JobLifecycleState::Active(job) = job {
            panic!("expected future job, got active job: {:?}", job);
        }

        let future_job_id = channel
            .get_future_job_id_from_template_id(template.template_id)
            .unwrap();

        let future_job = channel.remove_future_job(future_job_id).unwrap();

        // we know that the provided template + coinbase_reward_outputs should generate this future
        // job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 38, 82, 0, 3, 47, 47, 47, 31,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        assert_eq!(
            future_job.get_job_message(),
            &JobMessage::NewExtendedMiningJob(expected_job)
        );

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

        let activated_job = match channel.on_set_new_prev_hash(set_new_prev_hash).unwrap() {
            JobLifecycleState::Active(job) => job,
            _ => panic!("expected active job"),
        };

        // we just activated the only future job
        assert!(!channel.job_store.has_future_jobs());

        let mut previously_future_job = future_job.clone();
        previously_future_job.activate(ntime);

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
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
            job_store,
            None,
            None,
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

        let active_job = match channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap()
        {
            JobLifecycleState::Active(job) => job,
            _ => panic!("expected active job"),
        };

        assert!(!channel.job_store.has_future_jobs());

        // we know that the provided template + coinbase_reward_outputs should generate this
        // non-future job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 38, 82, 0, 3, 47, 47, 47, 31,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        assert_eq!(
            active_job.get_job_message(),
            &JobMessage::NewExtendedMiningJob(expected_job)
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
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
        assert!(!channel.job_store.has_future_jobs());
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
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
            job_store,
            None,
            None,
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

        // this share has hash 4c68f79a585c8b609e9b43113f73311eada20ec88a70a999406267db3499f1d9
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 8,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block);

        assert!(matches!(
            res,
            Ok(ShareValidationResult::BlockFound(_, _, _))
        ));
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
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
            job_store,
            None,
            None,
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

        // this share has hash d5767872f3a26e7f9f21cd968f27cfdb8b4061bb9ce0959852594ee8620f4efb
        // which does not meet the channel target
        // 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
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
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
            job_store,
            None,
            None,
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

        // this share has hash 000004f9d35777e4d56eedc20b1d05d251a7c0ed0b4e3013b5a809852844e218
        // which does meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 51208,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 51208,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
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
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

        // this is the most permissive possible max_target
        let max_target = Target::from_le_bytes([0xff; 32]);

        // Create a channel with initial hashrate
        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix,
            max_target,
            initial_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
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
            .update_channel(new_hashrate, Some(max_target))
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
        let result = channel.update_channel(-1.0, Some(max_target));
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ExtendedChannelError::InvalidNominalHashrate)
        ));

        // Create a not so permissive max_target so we can test a target that exceeds it
        let not_so_permissive_max_target = Target::from_le_bytes([
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00,
        ]);

        // Try to update with a hashrate that would result in a target exceeding the max_target
        // new target: 2492492492492492492492492492492492492492492492492492492492492491
        // max target: 00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let very_small_hashrate = 0.1;
        let result =
            channel.update_channel(very_small_hashrate, Some(not_so_permissive_max_target));
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
        let extranonce_prefix = [0, 0, 0, 0, 0, 0, 0, 1].to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let job_store = DefaultJobStore::new();

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
            job_store,
            None,
            None,
        )
        .unwrap();

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &extranonce_prefix);

        let new_extranonce_prefix = [0, 0, 0, 0, 0, 0, 0, 0, 0, 2].to_vec();

        channel
            .set_extranonce_prefix(new_extranonce_prefix.clone())
            .unwrap();
        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, &new_extranonce_prefix);

        let new_extranonce_prefix_too_large = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec();

        // try to set a new extranonce_prefix that is too large
        let result = channel.set_extranonce_prefix(new_extranonce_prefix_too_large.clone());
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ExtendedChannelError::ExtranoncePrefixTooLarge)
        ));
    }
}
