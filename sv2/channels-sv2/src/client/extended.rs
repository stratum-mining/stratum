//! # Sv2 Extended Channel - Mining Client Abstraction
//!
//! This module provides an abstraction over the state of an [Sv2](https://stratumprotocol.org/specification)
//! **Extended Channel** within a mining client.

extern crate alloc;
use super::{HashMap, MAX_FUTURE_JOBS, MAX_PAST_JOBS};
use crate::{
    bip141::try_strip_bip141,
    chain_tip::ChainTip,
    client::{
        error::ExtendedChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    extranonce_manager::{prefix::RetiredExtranoncePrefixes, ExtranoncePrefix},
    merkle_root::merkle_root_from_path,
    target::{bytes_to_hex, u256_to_block_hash},
    MAX_EXTRANONCE_LEN, MAX_FUTURE_BLOCK_TIME, VERSION_ROLLING_MASK,
};
use alloc::{collections::VecDeque, format, string::String, vec, vec::Vec};
use binary_sv2::Sv2OptionOwned;
use bitcoin::{
    absolute::LockTime,
    blockdata::block::{Header, Version as BlockVersion},
    consensus::{serialize, Decodable},
    hashes::sha256d::Hash,
    transaction::Version,
    CompactTarget, OutPoint, Sequence, Target, Transaction, TxIn, TxOut, Witness,
};
use mining_sv2::{
    NewExtendedMiningJobOwned, SetCustomMiningJobOwned, SetCustomMiningJobSuccess,
    SetNewPrevHashOwned as SetNewPrevHashMp, SubmitSharesExtendedOwned,
    ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE, ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE, ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE, ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
    ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
};
use tracing::debug;

/// An extended mining job as tracked by a client [`ExtendedChannel`].
#[derive(Debug, Clone, PartialEq)]
pub struct ExtendedJob {
    /// The [`NewExtendedMiningJob`](mining_sv2::NewExtendedMiningJob) message the job was created
    /// from, with `min_ntime` set once the job is activated.
    pub job_message: NewExtendedMiningJobOwned,
    /// The `extranonce_prefix` in use when the job was created.
    pub extranonce_prefix: Vec<u8>,
    /// The target the job's shares are validated against.
    pub target: Target,
}

/// Mining Client abstraction for the state management of an Sv2 Extended Channel.
///
/// This struct encapsulates all channel-specific state for a mining client, including:
/// - The channel's unique `channel_id`.
/// - The channel's `user_identity` as seen by upstream.
/// - The channel's unique `extranonce_prefix`.
/// - The size of the rollable portion of the extranonce.
/// - The channel's current target.
/// - The channel's nominal hashrate.
/// - Whether version rolling is supported (see [BIP 323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki)).
/// - Future jobs (indexed by `job_id`, capped at [`MAX_FUTURE_JOBS`]) to be activated by a
///   [`SetNewPrevHash`](SetNewPrevHashMp) message.
/// - The currently active job.
/// - Past jobs (previously active under the current chain tip, indexed by `job_id`, capped at
///   [`MAX_PAST_JOBS`]).
/// - Stale jobs (previously active and past jobs under the previous chain tip, indexed by
///   `job_id`). Upstream job IDs carry no uniqueness guarantee, so a job may be installed under
///   an ID a stale job holds; an ID names either a live job or a stale one, never both, and the
///   stale namesake is dropped. A late share for it is then validated against the live job, as
///   the channel cannot tell the two apart.
/// - Share accounting for the channel (as tracked by the client).
/// - The channel's current chain tip.
/// - Extranonce prefixes rotated out of the channel that live jobs were created under (see
///   [`set_extranonce_prefix`](Self::set_extranonce_prefix)).
#[derive(Debug)]
pub struct ExtendedChannel {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: ExtranoncePrefix,
    rollable_extranonce_size: u16,
    target: Target,
    nominal_hashrate: f32,
    version_rolling: bool,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, ExtendedJob>,
    // Future job IDs ordered by receipt, oldest at the front and newest at the back.
    // Replaced IDs move to the back; overflow evicts from the front.
    future_job_order: VecDeque<u32>,
    active_job: Option<ExtendedJob>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, ExtendedJob>,
    // Past job IDs ordered by retirement, oldest at the front and newest at the back.
    // Replaced IDs move to the back; overflow evicts from the front.
    past_job_order: VecDeque<u32>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, ExtendedJob>,
    // Cap on `past_jobs` under the current chain tip, resolved from the constructor's
    // `Option<usize>`; `None` and `Some(0)` both resolve to `MAX_PAST_JOBS`.
    max_past_jobs: usize,
    share_accounting: ShareAccounting,
    chain_tip: Option<ChainTip>,
    // extranonce prefixes rotated out of the channel that are still referenced by at least one
    // job that can accept shares, so that their allocator slots stay reserved
    retired_extranonce_prefixes: RetiredExtranoncePrefixes,
}

impl ExtendedChannel {
    /// Constructs a new [`ExtendedChannel`].
    ///
    /// `max_past_jobs` caps the past jobs retained under the current chain tip. `None` and
    /// `Some(0)` both select [`MAX_PAST_JOBS`].
    ///
    /// Returns [`ExtendedChannelError::InvalidTarget`] if `target` is zero, or
    /// [`ExtendedChannelError::NewExtranoncePrefixTooLarge`] if the prefix and rollable extranonce
    /// together exceed [`MAX_EXTRANONCE_LEN`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: ExtranoncePrefix,
        target: Target,
        nominal_hashrate: f32,
        version_rolling: bool,
        rollable_extranonce_size: u16,
        max_past_jobs: Option<usize>,
    ) -> Result<Self, ExtendedChannelError> {
        if target == Target::ZERO {
            return Err(ExtendedChannelError::InvalidTarget);
        }

        if extranonce_prefix.len() + rollable_extranonce_size as usize > MAX_EXTRANONCE_LEN as usize
        {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        // fall back to the default when the caller has no opinion: `None`, or `Some(0)`, which
        // would otherwise evict the just-retired job and reject the most common late share
        let max_past_jobs = match max_past_jobs {
            Some(cap) if cap > 0 => cap,
            _ => MAX_PAST_JOBS,
        };

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            target,
            nominal_hashrate,
            version_rolling,
            future_jobs: HashMap::new(),
            future_job_order: VecDeque::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            past_job_order: VecDeque::new(),
            stale_jobs: HashMap::new(),
            max_past_jobs,
            share_accounting: ShareAccounting::new(),
            chain_tip: None,
            retired_extranonce_prefixes: RetiredExtranoncePrefixes::default(),
        })
    }

    /// Returns the unique `channel_id` of this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the `user_identity` used by the upstream node to identify this client.
    pub fn get_user_identity(&self) -> &str {
        &self.user_identity
    }

    /// Returns the bytes representing the first part of the `extranonce`.
    pub fn get_extranonce_prefix(&self) -> &[u8] {
        self.extranonce_prefix.as_bytes()
    }

    /// Returns the length of the leading `upstream_prefix` region of this
    /// channel's extranonce prefix.
    ///
    /// See [`ExtranoncePrefix::upstream_prefix_len`](crate::extranonce_manager::ExtranoncePrefix::upstream_prefix_len)
    /// for the full semantics.
    pub fn upstream_prefix_len(&self) -> u8 {
        self.extranonce_prefix.upstream_prefix_len()
    }

    /// Returns `true` if the channel supports version rolling as per [BIP 323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki).
    pub fn is_version_rolling(&self) -> bool {
        self.version_rolling
    }

    /// Returns a reference to the current [`ChainTip`], if any.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Sets the [`ChainTip`].
    ///
    /// A first tip only initializes the channel, and setting the current tip again changes
    /// nothing. Replacing the tip with a different one is a chain-tip transition, handled as in
    /// [`on_chain_tip_update`](Self::on_chain_tip_update): the jobs built under the previous tip
    /// go stale rather than stay validatable against a header the miner was never assigned.
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        match &self.chain_tip {
            None => self.chain_tip = Some(chain_tip),
            Some(current) if *current == chain_tip => {}
            Some(_) => self.update_chain_tip(chain_tip),
        }
    }

    /// Sets a new extranonce prefix for the channel.
    ///
    /// After this change, all new jobs will use the new extranonce prefix.
    /// Jobs created before this call retain the previous extranonce prefix,
    /// and share validation is performed accordingly.
    ///
    /// Because of that, a previous prefix minted by a local
    /// [`ExtranonceAllocator`](crate::extranonce_manager::ExtranonceAllocator) (e.g. by a proxy
    /// sub-allocating an upstream-assigned extranonce space) is not released here: its slot
    /// stays reserved until no future, active or past job created under it remains, so that the
    /// allocator cannot hand the same extranonce space to another live channel while those jobs
    /// still validate shares. Wire-sourced prefixes hold no slot and are simply dropped.
    ///
    /// Returns an error if the new extranonce prefix and the channel's rollable extranonce would
    /// exceed [`MAX_EXTRANONCE_LEN`].
    pub fn set_extranonce_prefix(
        &mut self,
        new_extranonce_prefix: ExtranoncePrefix,
    ) -> Result<(), ExtendedChannelError> {
        let full_extranonce_size =
            new_extranonce_prefix.len() + self.rollable_extranonce_size as usize;
        if full_extranonce_size > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        let retired_extranonce_prefix =
            core::mem::replace(&mut self.extranonce_prefix, new_extranonce_prefix);
        self.retired_extranonce_prefixes.retire(
            retired_extranonce_prefix,
            self.future_jobs
                .values()
                .chain(self.active_job.iter())
                .chain(self.past_jobs.values())
                .map(|job| job.extranonce_prefix.as_slice()),
        );

        Ok(())
    }

    /// Sets the upstream-assigned region of this channel's extranonce prefix.
    ///
    /// For an allocator-produced prefix, `local_prefix | local_index` and its allocation are
    /// preserved. For a wire-sourced prefix, the entire prefix is `upstream_prefix` and is
    /// replaced. Jobs received before this call retain their captured prefix bytes; new jobs use
    /// the updated prefix. The channel is left unchanged if the resulting full extranonce would
    /// exceed [`MAX_EXTRANONCE_LEN`].
    ///
    /// Old prefix bytes share ownership of the same allocator slot while any future, active or
    /// past job uses them. A later `set_extranonce_prefix` rotation cannot release that slot
    /// prematurely. Once those jobs are stale or evicted, only the current prefix (if it still
    /// uses this allocation) keeps the slot reserved. This update consumes no additional slot.
    pub fn set_upstream_extranonce_prefix(
        &mut self,
        upstream_prefix: &[u8],
    ) -> Result<(), ExtendedChannelError> {
        let full_extranonce_size = upstream_prefix.len()
            + self.extranonce_prefix.preserved_len()
            + self.rollable_extranonce_size as usize;
        if full_extranonce_size > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtendedChannelError::NewExtranoncePrefixTooLarge);
        }

        let snapshot = self
            .extranonce_prefix
            .snapshot_for_upstream_update(upstream_prefix);
        self.extranonce_prefix
            .set_upstream_prefix(upstream_prefix)
            .map_err(|_| ExtendedChannelError::NewExtranoncePrefixTooLarge)?;
        if let Some(snapshot) = snapshot {
            self.retired_extranonce_prefixes.retire(
                snapshot,
                self.future_jobs
                    .values()
                    .chain(self.active_job.iter())
                    .chain(self.past_jobs.values())
                    .map(|job| job.extranonce_prefix.as_slice()),
            );
        }
        Ok(())
    }

    /// Returns the full extranonce size in bytes.
    pub fn get_full_extranonce_size(&self) -> usize {
        self.extranonce_prefix.len() + self.rollable_extranonce_size as usize
    }

    /// Returns the available size, in bytes, of the rollable portion of the extranonce.
    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    /// Returns a reference to the current [`Target`] for this channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Sets a new [`Target`] for the channel.
    ///
    /// Per the Sv2 spec, the new target also applies to jobs that were already received with an
    /// empty `min_ntime` (i.e. queued future jobs), so their associated target is refreshed here.
    /// Jobs that were already received with a set `min_ntime` (active, past and stale jobs) keep
    /// their target.
    ///
    /// Returns [`ExtendedChannelError::InvalidTarget`] if `new_target` is zero, leaving the
    /// channel unchanged.
    pub fn set_target(&mut self, new_target: Target) -> Result<(), ExtendedChannelError> {
        if new_target == Target::ZERO {
            return Err(ExtendedChannelError::InvalidTarget);
        }

        self.target = new_target;
        for future_job in self.future_jobs.values_mut() {
            future_job.target = new_target;
        }

        Ok(())
    }

    /// Returns the cumulative nominal hashrate for the channel, in h/s.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Sets the nominal hashrate for the channel, in h/s.
    pub fn set_nominal_hashrate(&mut self, hashrate: f32) {
        self.nominal_hashrate = hashrate;
    }

    /// Returns a reference to the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&ExtendedJob> {
        self.active_job.as_ref()
    }

    /// Returns an iterator over all future jobs for this channel.
    ///
    /// At most [`MAX_FUTURE_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_future_jobs(&self) -> impl Iterator<Item = (&u32, &ExtendedJob)> + '_ {
        self.future_jobs.iter()
    }

    /// Returns a reference to a future job by `job_id`, if present.
    pub fn get_future_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.future_jobs.get(&job_id)
    }

    /// Returns the number of future jobs tracked by this channel.
    pub fn get_future_jobs_count(&self) -> usize {
        self.future_jobs.len()
    }

    /// Returns an iterator over all past jobs for this channel.
    ///
    /// At most [`MAX_PAST_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_past_jobs(&self) -> impl Iterator<Item = (&u32, &ExtendedJob)> + '_ {
        self.past_jobs.iter()
    }

    /// Returns a reference to a past job by `job_id`, if present.
    pub fn get_past_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.past_jobs.get(&job_id)
    }

    /// Returns the number of past jobs tracked by this channel.
    ///
    /// At most [`MAX_PAST_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_past_jobs_count(&self) -> usize {
        self.past_jobs.len()
    }

    /// Returns an iterator over all stale jobs for this channel.
    pub fn get_stale_jobs(&self) -> impl Iterator<Item = (&u32, &ExtendedJob)> + '_ {
        self.stale_jobs.iter()
    }

    /// Returns a reference to a stale job by `job_id`, if present.
    pub fn get_stale_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.stale_jobs.get(&job_id)
    }

    /// Returns the number of stale jobs tracked by this channel.
    pub fn get_stale_jobs_count(&self) -> usize {
        self.stale_jobs.len()
    }

    /// Returns a reference to the [`ShareAccounting`] for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates share accounting based on a [`SubmitSharesSuccess`](mining_sv2::SubmitSharesSuccess) message from the
    /// upstream server. Delegates to [`ShareAccounting::on_share_acknowledgement`].
    pub fn on_share_acknowledgement(
        &mut self,
        new_submits_accepted_count: u32,
        new_shares_sum: u64,
    ) {
        self.share_accounting
            .on_share_acknowledgement(new_submits_accepted_count, new_shares_sum);
    }

    /// Updates share accounting based on a [`SubmitSharesError`](mining_sv2::SubmitSharesError) message from the upstream
    /// server. Delegates to [`ShareAccounting::on_share_rejection`].
    pub fn on_share_rejection(&mut self, error_code: &str) {
        self.share_accounting.on_share_rejection(error_code);
    }

    /// Handles a [`NewExtendedMiningJob`](mining_sv2::NewExtendedMiningJob) message received from upstream.
    ///
    /// The message could be either directed at this channel, or at a group channel it belongs to.
    ///
    /// - If [`NewExtendedMiningJob::min_ntime`](mining_sv2::NewExtendedMiningJob::min_ntime) is empty, the job is considered a future job and
    ///   added to the future jobs list (see [`get_future_jobs`](ExtendedChannel::get_future_jobs)).
    ///   At most [`MAX_FUTURE_JOBS`] future jobs are kept: storing a new one beyond that limit
    ///   evicts the oldest.
    /// - Otherwise, the job is activated and previous active job moves to the past jobs list.
    ///   At most [`MAX_PAST_JOBS`] past jobs are kept: retiring one beyond that limit evicts the
    ///   oldest. Such a job is mined against the current chain tip, so a `min_ntime` below the
    ///   tip's is refused with [`ExtendedChannelError::JobMinNtimeBelowChainTip`], leaving the
    ///   channel unchanged.
    pub fn on_new_extended_mining_job(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJobOwned,
    ) -> Result<(), ExtendedChannelError> {
        let mut new_extended_mining_job = new_extended_mining_job;
        // try to strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix, if they are
        // present
        let new_extended_mining_job = match try_strip_bip141(
            new_extended_mining_job.coinbase_tx_prefix.as_bytes(),
            new_extended_mining_job.coinbase_tx_suffix.as_bytes(),
        )
        .map_err(ExtendedChannelError::FailedToTryToStripBip141)?
        {
            Some((coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141)) => {
                new_extended_mining_job.coinbase_tx_prefix = coinbase_tx_prefix_stripped_bip141
                    .try_into()
                    .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?;
                new_extended_mining_job.coinbase_tx_suffix = coinbase_tx_suffix_stripped_bip141
                    .try_into()
                    .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?;
                new_extended_mining_job
            }
            None => new_extended_mining_job,
        };

        match new_extended_mining_job.min_ntime.clone().into_inner() {
            Some(min_ntime) => {
                // the job is mined against the chain tip, whose min_ntime is the smallest nTime
                // available for it; a job allowing earlier shares would have them carry a
                // timestamp the tip declared unavailable
                if self
                    .chain_tip
                    .as_ref()
                    .is_some_and(|chain_tip| min_ntime < chain_tip.min_ntime())
                {
                    return Err(ExtendedChannelError::JobMinNtimeBelowChainTip);
                }

                // an ID names either a live job or a stale one, never both
                self.stale_jobs.remove(&new_extended_mining_job.job_id);
                // the new job is installed before the displaced one is retired: retirement may
                // prune retired extranonce prefixes, and the new job is a live user of its bytes
                let displaced_job = self.active_job.replace(ExtendedJob {
                    job_message: new_extended_mining_job,
                    extranonce_prefix: self.extranonce_prefix.as_bytes().to_vec(),
                    target: self.target,
                });
                if let Some(displaced_job) = displaced_job {
                    self.retire_job_to_past(displaced_job);
                }
            }
            None => {
                let job_id = new_extended_mining_job.job_id;
                self.future_jobs.insert(
                    job_id,
                    ExtendedJob {
                        job_message: new_extended_mining_job,
                        extranonce_prefix: self.extranonce_prefix.as_bytes().to_vec(),
                        target: self.target,
                    },
                );

                // a replaced job_id moves to the back of the eviction order
                self.future_job_order.retain(|id| *id != job_id);
                self.future_job_order.push_back(job_id);

                if self.future_jobs.len() > MAX_FUTURE_JOBS {
                    if let Some(evicted_job_id) = self.future_job_order.pop_front() {
                        self.future_jobs.remove(&evicted_job_id);
                    }
                }

                // a replaced or evicted job may have been the last one holding a retired
                // extranonce prefix alive; release such slots now rather than at the next chain
                // transition, which the upstream can withhold
                self.prune_retired_extranonce_prefixes();
            }
        }

        Ok(())
    }

    /// Handles a `SetCustomMiningJobSuccess` message from upstream.
    /// Requires the corresponding `SetCustomMiningJob`.
    ///
    /// The previous active job (if any) moves to the past jobs list. At most [`MAX_PAST_JOBS`]
    /// past jobs are kept: retiring one beyond that limit evicts the oldest.
    ///
    /// To be used by a Sv2 Job Declarator Client
    pub fn on_set_custom_mining_job_success(
        &mut self,
        set_custom_mining_job: SetCustomMiningJobOwned,
        set_custom_mining_job_success: SetCustomMiningJobSuccess,
    ) -> Result<(), ExtendedChannelError> {
        if set_custom_mining_job.channel_id != set_custom_mining_job_success.channel_id
            || set_custom_mining_job.channel_id != self.channel_id
        {
            return Err(ExtendedChannelError::ChannelIdMismatch);
        }

        if set_custom_mining_job.request_id != set_custom_mining_job_success.request_id {
            return Err(ExtendedChannelError::RequestIdMismatch);
        }

        let Some(chain_tip) = self.chain_tip.clone() else {
            return Err(ExtendedChannelError::NoChainTip);
        };

        if set_custom_mining_job.min_ntime != chain_tip.min_ntime()
            || set_custom_mining_job.prev_hash != chain_tip.prev_hash()
            || set_custom_mining_job.nbits != chain_tip.nbits()
        {
            return Err(ExtendedChannelError::ChainTipMismatch);
        }

        let deserialized_outputs = Vec::<TxOut>::consensus_decode(
            &mut set_custom_mining_job
                .coinbase_tx_outputs
                .to_owned_bytes()
                .as_slice(),
        )
        .map_err(|_| ExtendedChannelError::FailedToDeserializeCoinbaseOutputs)?;

        let mut script_sig = vec![];
        script_sig.extend_from_slice(set_custom_mining_job.coinbase_prefix.as_bytes());
        let full_extranonce_size = self.get_full_extranonce_size();
        let full_extranonce = vec![0; full_extranonce_size];
        script_sig.extend_from_slice(&full_extranonce);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(set_custom_mining_job.coinbase_tx_input_n_sequence),
            witness: Witness::from(vec![vec![0; 32]]), /* note: 32 bytes of zeros is only safe to
                                                        * assume now, this could change in future
                                                        * soft forks */
        };

        let coinbase = Transaction {
            version: Version::non_standard(set_custom_mining_job.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(set_custom_mining_job.coinbase_tx_locktime),
            input: vec![tx_in],
            output: deserialized_outputs,
        };

        let serialized_coinbase = serialize(&coinbase);

        let prefix_index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + set_custom_mining_job.coinbase_prefix.len();

        let coinbase_tx_prefix = serialized_coinbase[0..prefix_index].to_vec();

        let suffix_index = prefix_index + full_extranonce_size;

        let coinbase_tx_suffix = serialized_coinbase[suffix_index..].to_vec();

        // strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix
        let (coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141) =
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
                .map_err(ExtendedChannelError::FailedToTryToStripBip141)?
                .ok_or(ExtendedChannelError::FailedToStripBip141)?;

        let new_extended_mining_job = NewExtendedMiningJobOwned {
            channel_id: set_custom_mining_job.channel_id,
            job_id: set_custom_mining_job_success.job_id,
            min_ntime: Sv2OptionOwned::new(Some(set_custom_mining_job.min_ntime)),
            version: set_custom_mining_job.version,
            version_rolling_allowed: self.version_rolling,
            coinbase_tx_prefix: coinbase_tx_prefix_stripped_bip141
                .try_into()
                .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?,
            coinbase_tx_suffix: coinbase_tx_suffix_stripped_bip141
                .try_into()
                .map_err(|_| ExtendedChannelError::FailedToSerializeToB064K)?,
            merkle_path: set_custom_mining_job.merkle_path,
        };

        // an ID names either a live job or a stale one, never both
        self.stale_jobs.remove(&new_extended_mining_job.job_id);
        // the new job is installed before the displaced one is retired: retirement may prune
        // retired extranonce prefixes, and the new job is a live user of its bytes
        let displaced_job = self.active_job.replace(ExtendedJob {
            job_message: new_extended_mining_job,
            extranonce_prefix: self.extranonce_prefix.as_bytes().to_vec(),
            target: self.target,
        });
        if let Some(displaced_job) = displaced_job {
            self.retire_job_to_past(displaced_job);
        }

        Ok(())
    }

    // Moves a displaced job into past jobs, evicting the oldest past job beyond
    // [`MAX_PAST_JOBS`]. A share against an evicted job is rejected as `InvalidJobId` even
    // though it would otherwise have been accepted and propagated: a bounded loss of
    // creditable work, the price of bounding memory under a hostile upstream.
    fn retire_job_to_past(&mut self, job: ExtendedJob) {
        let job_id = job.job_message.job_id;
        self.past_jobs.insert(job_id, job);

        // a replaced job_id moves to the back of the eviction order
        self.past_job_order.retain(|id| *id != job_id);
        self.past_job_order.push_back(job_id);

        if self.past_jobs.len() > self.max_past_jobs {
            if let Some(evicted_job_id) = self.past_job_order.pop_front() {
                self.past_jobs.remove(&evicted_job_id);
            }
        }

        // a replaced or evicted job may have been the last one holding a retired extranonce
        // prefix alive; release such slots now rather than at the next chain transition, which
        // the upstream can withhold
        self.prune_retired_extranonce_prefixes();
    }

    // Drops every retired extranonce prefix that no future, active or past job still references,
    // releasing its allocator slot. Stale jobs are not live: shares against them are rejected,
    // so they hold no prefix.
    fn prune_retired_extranonce_prefixes(&mut self) {
        self.retired_extranonce_prefixes.prune(
            self.future_jobs
                .values()
                .chain(self.active_job.iter())
                .chain(self.past_jobs.values())
                .map(|job| job.extranonce_prefix.as_slice()),
        );
    }

    /// Handles a [`ChainTip`] update.
    ///
    /// To be used by a Sv2 Job Declarator Client, which should never receive a
    /// [`SetNewPrevHash`](SetNewPrevHashMp) (Mining Protocol) message, or will most likely
    /// ignore it if it does.
    ///
    /// So a [`SetNewPrevHash`](template_distribution_sv2::SetNewPrevHash) (Template Distribution
    /// Protocol) message should be converted into a [`ChainTip`] and passed to this function.
    ///
    /// - Clears all future jobs.
    /// - Retires the previously active job as stale, leaving the channel with no active job until
    ///   the next job message arrives.
    /// - Marks all past jobs as stale and clears them.
    /// - Clears all seen shares if `prev_hash` changed, as shares for the previous chain tip will
    ///   be rejected as stale; a repeated `prev_hash` keeps them, see
    ///   [`ShareAccounting::flush_seen_shares`].
    pub fn on_chain_tip_update(&mut self, chain_tip: ChainTip) -> Result<(), ExtendedChannelError> {
        self.update_chain_tip(chain_tip);
        Ok(())
    }

    // Moves the channel onto `chain_tip`: future jobs are dropped, the active and past jobs go
    // stale, retired extranonce prefixes are pruned and seen shares are flushed if `prev_hash`
    // changed.
    fn update_chain_tip(&mut self, chain_tip: ChainTip) {
        let is_new_prev_hash = self
            .chain_tip
            .as_ref()
            .is_some_and(|previous| previous.prev_hash() != chain_tip.prev_hash());
        self.chain_tip = Some(chain_tip);

        // all other future jobs are now useless
        self.future_jobs.clear();
        self.future_job_order.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = core::mem::take(&mut self.past_jobs);
        self.past_job_order.clear();

        // the previously active job belongs to the old chain tip, so it goes stale with them.
        // without this, a share arriving before the next SetCustomMiningJobSuccess would still
        // pass the is_active_job check in validate_share and be re-hashed against the new
        // prev_hash with the old job's coinbase/merkle path. it bypasses the MAX_PAST_JOBS
        // cap: retiring it through the capped past path would push the oldest past job out of
        // the stale set, misclassifying its late shares as InvalidJobId instead of Stale.
        if let Some(active_job) = self.active_job.take() {
            self.stale_jobs
                .insert(active_job.job_message.job_id, active_job);
        }

        // the jobs that just went stale can no longer accept shares, so any retired extranonce
        // prefix they were the last reference to is now releasable
        self.prune_retired_extranonce_prefixes();

        // hashes are retained while prev_hash is unchanged, see ShareAccounting::flush_seen_shares
        if is_new_prev_hash {
            self.share_accounting.flush_seen_shares();
        }
    }

    /// Handles a [`SetNewPrevHash`](SetNewPrevHashMp) message from upstream.
    ///
    /// The message could be either directed at this channel, or at a group channel it belongs to.
    ///
    /// - If the referenced `job_id` is not a future job, returns an error and leaves channel state
    ///   untouched.
    /// - If it is a future job, activates it as the current job.
    /// - Marks the previously active job and all past jobs as stale, and clears past jobs.
    /// - Clears all seen shares if `prev_hash` changed, as shares for the previous chain tip will
    ///   be rejected as stale; a repeated `prev_hash` keeps them, see
    ///   [`ShareAccounting::flush_seen_shares`].
    /// - Updates the chain tip for the channel.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp,
    ) -> Result<(), ExtendedChannelError> {
        // the previously active job is only displaced once activation is known to succeed, so
        // that the JobIdNotFound path below does not corrupt channel state
        let previously_active_job = match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(mut activated_job) => {
                activated_job.job_message.min_ntime =
                    Sv2OptionOwned::new(Some(set_new_prev_hash.min_ntime));
                self.active_job.replace(activated_job)
            }
            None => {
                return Err(ExtendedChannelError::JobIdNotFound);
            }
        };

        // all other future jobs are now useless
        self.future_jobs.clear();
        self.future_job_order.clear();

        // mark all past jobs as stale, so that shares are not propagated
        self.stale_jobs = core::mem::take(&mut self.past_jobs);
        self.past_job_order.clear();

        // the job that was active under the previous chain tip goes stale with them rather
        // than being silently dropped, bypassing the MAX_PAST_JOBS cap: retiring it through
        // the capped past path would push the oldest past job out of the stale set, and a
        // late share for either job would be rejected as InvalidJobId instead of Stale
        if let Some(previously_active_job) = previously_active_job {
            self.stale_jobs.insert(
                previously_active_job.job_message.job_id,
                previously_active_job,
            );
        }

        // the activated job may reuse the ID of a job that just went stale; an ID names either
        // a live job or a stale one, never both
        self.stale_jobs.remove(&set_new_prev_hash.job_id);

        // the jobs that just went stale can no longer accept shares, so any retired extranonce
        // prefix they were the last reference to is now releasable
        self.prune_retired_extranonce_prefixes();

        // hashes are retained while prev_hash is unchanged, see ShareAccounting::flush_seen_shares
        if self
            .chain_tip
            .as_ref()
            .is_some_and(|chain_tip| chain_tip.prev_hash() != set_new_prev_hash.prev_hash)
        {
            self.share_accounting.flush_seen_shares();
        }

        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Validates a share prior to submission upstream.
    ///
    /// Updates channel state with the share validation result:
    /// - Prevents propagation of stale, duplicate, low-difficulty, or out-of-window shares
    ///   (shares whose `ntime` is outside `[min_ntime, min_ntime + MAX_FUTURE_BLOCK_TIME]`,
    ///   where `min_ntime` is the referenced job's: the `SetNewPrevHash` timestamp for a job
    ///   activated from the future queue, or the value its own message advertised for an
    ///   immediately-active job; see [`MAX_FUTURE_BLOCK_TIME`] for how this clockless upper
    ///   bound relates to the spec's elapsed-time window).
    /// - Indicates whether a block was found from the share.
    /// - Maintains local share accounting for later reconciliation with upstream acknowledgements.
    ///   Duplicate detection is bounded at [`MAX_SEEN_SHARES`](crate::client::MAX_SEEN_SHARES)
    ///   validated shares per `prev_hash` (oldest evicted first), so an evicted share can be
    ///   validated again; see [`ShareAccounting`] for why this replay window is accepted on
    ///   clients.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtendedOwned,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .is_some_and(|job| job.job_message.job_id == job_id);

        // check if job_id is past job
        let is_past_job = self.past_jobs.contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.stale_jobs.contains_key(&job_id);

        if is_stale_job {
            return Err(ShareValidationError::Stale(
                ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
            ));
        }

        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else if is_past_job {
            self.past_jobs.get(&job_id).expect("past job must exist")
        } else {
            return Err(ShareValidationError::InvalidJobId(
                ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
            ));
        };

        let extranonce_size = share.extranonce.len();
        if extranonce_size != self.rollable_extranonce_size as usize {
            return Err(ShareValidationError::BadExtranonceSize(
                ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE,
            ));
        }

        let mut full_extranonce = vec![];
        full_extranonce.extend_from_slice(job.extranonce_prefix.as_slice());
        full_extranonce.extend_from_slice(share.extranonce.as_bytes());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.job_message.coinbase_tx_prefix.as_bytes(),
            job.job_message.coinbase_tx_suffix.as_bytes(),
            full_extranonce.as_ref(),
            job.job_message.merkle_path.as_slice(),
        )
        .ok_or(ShareValidationError::Invalid(
            ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
        ))?;

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits: CompactTarget = CompactTarget::from_consensus(chain_tip.nbits());

        // the share's ntime is bounded by the min_ntime of the job it references: a job
        // activated from the future queue carries the SetNewPrevHash timestamp that activated
        // it (which is also the chain tip's), an immediately-active job the value its own
        // message advertised. Every active or past job carries one: a job without it is a
        // future job, which is never mined on.
        let job_min_ntime = job
            .job_message
            .min_ntime
            .as_ref()
            .copied()
            .expect("active and past jobs carry a min_ntime");

        if share.ntime < job_min_ntime {
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
            ));
        }

        // consensus caps block timestamps at ~2h in the future; the allowance is anchored at the
        // receipt of the message that supplied min_ntime, since this crate has no clock (see
        // MAX_FUTURE_BLOCK_TIME)
        if share.ntime > job_min_ntime.saturating_add(MAX_FUTURE_BLOCK_TIME) {
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
            ));
        }

        // Only BIP323 general-purpose bits may differ from the job's advertised version.
        // When version rolling is not allowed, the share version must match the job version exactly.
        let version_rolling_mask = if job.job_message.version_rolling_allowed {
            VERSION_ROLLING_MASK
        } else {
            0
        };

        // Only the non-rollable version bits are compared: `!version_rolling_mask` zeroes
        // the BIP323 general-purpose bits the miner may change, so any remaining difference
        // from the job's advertised version means an unauthorized change. When version
        // rolling is not allowed, the mask is 0 and this degenerates to strict equality
        // with the job version.
        if (share.version & !version_rolling_mask)
            != (job.job_message.version & !version_rolling_mask)
        {
            if job.job_message.version_rolling_allowed {
                return Err(ShareValidationError::Invalid(
                    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
                ));
            }
            return Err(ShareValidationError::VersionRollingNotAllowed(
                ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
            ));
        }

        // create the header for validation
        let header = Header {
            version: BlockVersion::from_consensus(share.version as i32),
            prev_blockhash: u256_to_block_hash(prev_hash.clone()),
            merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
            time: share.ntime,
            bits: nbits,
            nonce: share.nonce,
        };

        // convert the header hash to a target type for easy comparison
        let share_hash = header.block_hash();
        let raw_share_hash: [u8; 32] = *share_hash.to_raw_hash().as_ref();
        let share_hash_target = Target::from_le_bytes(raw_share_hash);
        let share_hash_as_diff = share_hash_target.difficulty_float();

        let network_target = Target::from_compact(nbits);
        let job_target = job.target;

        // print hash_as_target and self.target as human readable hex
        let share_hash_target_bytes = share_hash_target.to_be_bytes();
        let job_target_bytes = job_target.to_be_bytes();

        debug!(
            "share validation \nshare:\t\t{}\njob target:\t{}\nnetwork target:\t{}",
            bytes_to_hex(&share_hash_target_bytes),
            bytes_to_hex(&job_target_bytes),
            format!("{:x}", network_target)
        );

        // check if a block was found
        if network_target.is_met_by(share_hash) {
            if self
                .share_accounting
                .is_share_seen(share_hash.to_raw_hash())
            {
                return Err(ShareValidationError::DuplicateShare(
                    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
                ));
            }
            self.share_accounting.track_validated_share(
                share.sequence_number,
                share_hash.to_raw_hash(),
                job_target.difficulty_float(),
            );
            self.share_accounting.increment_blocks_found();
            return Ok(ShareValidationResult::BlockFound(share_hash.to_raw_hash()));
        }

        // check if the share hash meets the job target
        if share_hash_target < job_target {
            if self
                .share_accounting
                .is_share_seen(share_hash.to_raw_hash())
            {
                return Err(ShareValidationError::DuplicateShare(
                    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
                ));
            }

            self.share_accounting.track_validated_share(
                share.sequence_number,
                share_hash.to_raw_hash(),
                job_target.difficulty_float(),
            );

            // update the best diff
            self.share_accounting.update_best_diff(share_hash_as_diff);

            return Ok(ShareValidationResult::Valid(share_hash.to_raw_hash()));
        }

        Err(ShareValidationError::DoesNotMeetTarget(
            ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::ExtendedJob;
    use crate::{
        chain_tip::ChainTip,
        client::{
            error::ExtendedChannelError,
            extended::ExtendedChannel,
            share_accounting::{ShareValidationError, ShareValidationResult},
            MAX_FUTURE_JOBS, MAX_PAST_JOBS,
        },
        extranonce_manager::{
            ExtranonceAllocator, ExtranonceAllocatorError, ExtranoncePrefix, MAX_EXTRANONCE_LEN,
        },
    };
    use binary_sv2::Sv2OptionOwned as Sv2Option;
    use bitcoin::Target;
    use mining_sv2::{
        NewExtendedMiningJobOwned as NewExtendedMiningJob, SetNewPrevHashOwned as SetNewPrevHashMp,
        SubmitSharesExtendedOwned as SubmitSharesExtended,
        ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    };
    use std::convert::TryInto;

    #[test]
    fn upstream_prefix_update_only_affects_subsequent_jobs() {
        // upstream(1) + local_prefix(1) + local_index(1) + rollable(2) = 5
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 5, 256).unwrap();
        let allocated_prefix = allocator.allocate_extended(2).unwrap();
        let old_prefix = allocated_prefix.as_bytes().to_vec();
        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            allocated_prefix.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            2,
            None,
        )
        .unwrap();
        let job = |job_id| NewExtendedMiningJob {
            channel_id: 1,
            job_id,
            min_ntime: Sv2Option::new(Some(1)),
            version: 536870912,
            version_rolling_allowed: true,
            // Six bytes are enough for the BIP141 detector; byte four is nonzero, so this is
            // already considered stripped for the purpose of this state-management test.
            coinbase_tx_prefix: vec![1, 0, 0, 0, 1, 0].try_into().unwrap(),
            coinbase_tx_suffix: vec![].try_into().unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(job(1)).unwrap();
        channel
            .set_upstream_extranonce_prefix(&[0xcc, 0xdd])
            .unwrap();
        let new_prefix = channel.get_extranonce_prefix().to_vec();
        channel.on_new_extended_mining_job(job(2)).unwrap();

        assert_eq!(old_prefix, &[0xaa, 0xbb, 0x00]);
        assert_eq!(new_prefix, &[0xcc, 0xdd, 0xbb, 0x00]);
        assert_eq!(
            &channel.get_past_job(1).unwrap().extranonce_prefix,
            &old_prefix
        );
        assert_eq!(
            &channel.get_active_job().unwrap().extranonce_prefix,
            &new_prefix
        );
        assert_eq!(allocator.allocated_count(), 1);
    }

    #[test]
    fn new_enforces_full_extranonce_size() {
        let result = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0xaa; MAX_EXTRANONCE_LEN as usize]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            1,
            None,
        );

        assert!(matches!(
            result,
            Err(ExtendedChannelError::NewExtranoncePrefixTooLarge)
        ));
    }

    #[test]
    fn set_extranonce_prefix_enforces_full_extranonce_size() {
        let rollable_extranonce_size = 4;
        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0xaa]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        let largest_valid_prefix =
            vec![0xbb; MAX_EXTRANONCE_LEN as usize - rollable_extranonce_size as usize];
        channel
            .set_extranonce_prefix(
                ExtranoncePrefix::from_wire(largest_valid_prefix.clone()).unwrap(),
            )
            .unwrap();
        assert_eq!(channel.get_extranonce_prefix(), &largest_valid_prefix);

        let result = channel.set_extranonce_prefix(
            ExtranoncePrefix::from_wire(vec![0xcc; largest_valid_prefix.len() + 1]).unwrap(),
        );
        assert!(matches!(
            result,
            Err(ExtendedChannelError::NewExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), &largest_valid_prefix);
    }

    #[test]
    fn set_upstream_extranonce_prefix_enforces_full_extranonce_size_transactionally() {
        // local_prefix(1) + local_index(1) and rollable(2) leave 28 bytes for upstream_prefix.
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 5, 256).unwrap();
        let allocated_prefix = allocator.allocate_extended(2).unwrap();
        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            allocated_prefix.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            2,
            None,
        )
        .unwrap();

        channel.set_upstream_extranonce_prefix(&[0xcc; 28]).unwrap();
        let largest_valid_prefix = channel.get_extranonce_prefix().to_vec();
        let upstream_prefix_len = channel.upstream_prefix_len();

        let result = channel.set_upstream_extranonce_prefix(&[0xdd; 29]);
        assert!(matches!(
            result,
            Err(ExtendedChannelError::NewExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), &largest_valid_prefix);
        assert_eq!(channel.upstream_prefix_len(), upstream_prefix_len);
        assert_eq!(allocator.allocated_count(), 1);
    }

    #[test]
    fn test_future_job_activation_flow() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 4u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs_count(), 1);
        assert_eq!(channel.get_active_job(), None);
        assert_eq!(channel.get_past_jobs_count(), 0);

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

        assert_eq!(channel.get_future_jobs_count(), 0);

        let mut previously_future_job = future_job.clone();
        previously_future_job.min_ntime = Sv2Option::new(Some(ntime));

        assert_eq!(
            channel.get_active_job(),
            Some(&ExtendedJob {
                job_message: previously_future_job,
                extranonce_prefix,
                target: channel.get_target().clone()
            })
        );
    }

    #[test]
    fn test_future_jobs_are_bounded() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            None,
        )
        .unwrap();

        let future_job = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        let flood_size = 10_000u32;
        for job_id in 0..flood_size {
            let mut job = future_job.clone();
            job.job_id = job_id;
            channel.on_new_extended_mining_job(job).unwrap();
        }

        assert_eq!(channel.get_future_jobs_count(), MAX_FUTURE_JOBS);

        for job_id in 0..flood_size - MAX_FUTURE_JOBS as u32 {
            assert!(channel.get_future_job(job_id).is_none());
        }
        for job_id in flood_size - MAX_FUTURE_JOBS as u32..flood_size {
            assert!(channel.get_future_job(job_id).is_some());
        }
    }

    #[test]
    fn test_replaced_future_job_moves_to_back_of_eviction_order() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            None,
        )
        .unwrap();

        let future_job = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        // fill the store with MAX_FUTURE_JOBS distinct job_ids
        for job_id in 0..MAX_FUTURE_JOBS as u32 {
            let mut job = future_job.clone();
            job.job_id = job_id;
            channel.on_new_extended_mining_job(job).unwrap();
        }

        // re-send job_id 0: it should move to the back of the eviction order
        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // one more distinct job_id: job_id 1 is now the oldest and gets evicted
        let mut job = future_job.clone();
        job.job_id = MAX_FUTURE_JOBS as u32;
        channel.on_new_extended_mining_job(job).unwrap();

        assert_eq!(channel.get_future_jobs_count(), MAX_FUTURE_JOBS);
        assert!(channel.get_future_job(1).is_none());
        assert!(channel.get_future_job(0).is_some());

        // the replaced job_id can still be activated
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: 0,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits: 503543726,
            min_ntime: 1746839905,
        };
        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();
    }

    #[test]
    fn test_past_jobs_are_bounded() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            None,
        )
        .unwrap();

        let active_job = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            min_ntime: Sv2Option::new(Some(1746839905)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        let flood_size = 10_000u32;
        for job_id in 0..flood_size {
            let mut job = active_job.clone();
            job.job_id = job_id;
            channel.on_new_extended_mining_job(job).unwrap();
        }

        assert_eq!(channel.get_past_jobs_count(), MAX_PAST_JOBS);

        // the last job is active; of the retired ones, only the newest MAX_PAST_JOBS survive
        for job_id in 0..flood_size - 1 - MAX_PAST_JOBS as u32 {
            assert!(channel.get_past_job(job_id).is_none());
        }
        for job_id in flood_size - 1 - MAX_PAST_JOBS as u32..flood_size - 1 {
            assert!(channel.get_past_job(job_id).is_some());
        }
    }

    #[test]
    fn test_past_jobs_respect_constructor_override() {
        // Some(cap) must override MAX_PAST_JOBS all the way through to the eviction path.
        let custom_cap = 3usize;
        assert!(custom_cap < MAX_PAST_JOBS);

        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            Some(custom_cap),
        )
        .unwrap();

        let active_job = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            min_ntime: Sv2Option::new(Some(1746839905)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        let job_count = 20u32;
        for job_id in 0..job_count {
            let mut job = active_job.clone();
            job.job_id = job_id;
            channel.on_new_extended_mining_job(job).unwrap();
        }

        // bounded by the override, not by MAX_PAST_JOBS
        assert_eq!(channel.get_past_jobs_count(), custom_cap);

        // the last job is active; only the newest `custom_cap` retired jobs survive
        for job_id in 0..job_count - 1 - custom_cap as u32 {
            assert!(channel.get_past_job(job_id).is_none());
        }
        for job_id in job_count - 1 - custom_cap as u32..job_count - 1 {
            assert!(channel.get_past_job(job_id).is_some());
        }

        // `Some(0)` is not a zero cap: it means "no opinion" and selects the default
        let mut zero_cap_channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0; 27]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            Some(0),
        )
        .unwrap();
        for job_id in 0..MAX_PAST_JOBS as u32 + 2 {
            let mut job = active_job.clone();
            job.job_id = job_id;
            zero_cap_channel.on_new_extended_mining_job(job).unwrap();
        }
        assert_eq!(zero_cap_channel.get_past_jobs_count(), MAX_PAST_JOBS);
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
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 4u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        let ntime: u32 = 1746839905;
        let active_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
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

        channel
            .on_new_extended_mining_job(active_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs_count(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&ExtendedJob {
                job_message: active_job.clone(),
                extranonce_prefix: extranonce_prefix.clone(),
                target: channel.get_target().clone()
            })
        );
        assert_eq!(channel.get_past_jobs_count(), 0);

        let mut new_active_job = active_job.clone();
        new_active_job.job_id = 2;
        channel
            .on_new_extended_mining_job(new_active_job.clone())
            .unwrap();

        assert_eq!(channel.get_future_jobs_count(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&ExtendedJob {
                job_message: new_active_job,
                extranonce_prefix,
                target: channel.get_target().clone()
            })
        );
        assert_eq!(channel.get_past_jobs_count(), 1);
    }

    #[test]
    fn test_share_validation_block_found() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 155d3f07a6fb97038dab34f71813b3f32e883c2d4ab4c75f606e1139d50eaebf
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block.clone());

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound(_))));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 1);

        // re-submitting the same valid block must be rejected as duplicate
        let res = channel.validate_share(share_valid_block);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare(_)
        ));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 1);
    }

    #[test]
    fn test_share_validation_ntime_below_min_ntime() {
        // Regression test: a share with ntime < min_ntime must be rejected.
        // Reuses the block-found test vectors but sets min_ntime one second
        // above the share's ntime.
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        // set min_ntime one second above the share's ntime (1745596971 + 1)
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: 1745596972,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        let share_below_min_ntime = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_below_min_ntime);

        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 0);
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = Target::from_be_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 84d3931c81d0e5af5440bf3ff94d84ff19a4dc0113e65cc11703330a7d599f61
        // which does not meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget(_)
        ));
    }

    #[test]
    fn test_share_validation_valid_share() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits: u32 = 453040064;
        let ntime: u32 = 1745596970;
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

        // this share has hash 00005e460def43b0153246e6300ce38d9da1c9abd8ef2157a88b2e9a12a8524a
        // which does meet the channel target
        // 0000ffff00000000000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 102103,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 102103,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(repeated_share);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare(_)
        ));
    }

    #[test]
    fn test_share_validation_invalid_non_rollable_version_bit() {
        // when version rolling is allowed on the channel,
        // only the BIP323 general-purpose bits may differ from the job version
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // the job version is 536870912 (0x20000000)
        // this share flips bit 0, which is outside the BIP323 general-purpose bits
        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870913,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share);
        let err = res.expect_err("share with non-rollable version bits must be rejected");
        match err {
            ShareValidationError::Invalid(code) => {
                assert_eq!(
                    code,
                    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT
                );
            }
            other => panic!("expected ShareValidationError::Invalid, got {other:?}"),
        }
    }

    #[test]
    fn test_share_validation_version_rolling_not_allowed() {
        // when version rolling is not allowed on the channel,
        // the share version must match the job version exactly
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = false;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: false,
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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // the job version is 536870912 (0x20000000)
        // any share version that differs from the job version must be rejected

        // this share flips bit 0, which is outside the BIP323 general-purpose bits mask
        let share_non_rollable_bit = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870913,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_non_rollable_bit);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::VersionRollingNotAllowed(_)
        ));

        // this share sets bit 5, which is inside the BIP323 general-purpose bits mask:
        // rolling it is still forbidden when version rolling is not allowed
        let share_rolled_bit = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 0x20000020,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_rolled_bit);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::VersionRollingNotAllowed(_)
        ));
    }

    #[test]
    fn test_share_validation_version_rolling_not_allowed_matching_version() {
        // when version rolling is not allowed on the channel,
        // a share whose version matches the job version must validate normally
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = false;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        let future_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: false,
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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 155d3f07a6fb97038dab34f71813b3f32e883c2d4ab4c75f606e1139d50eaebf
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        // its version matches the job version exactly
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block);

        assert!(matches!(res, Ok(ShareValidationResult::BlockFound(_))));
    }

    #[test]
    fn test_share_validation_rollable_version_bits() {
        // when version rolling is allowed on the channel,
        // shares that only differ in the BIP323 general-purpose bits are accepted
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596970;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // the job version is 536870912 (0x20000000)
        // this share version only sets bits 5-20, which are inside the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        let rolled_version = 0x20000000 | 0x1fffe0;

        // this share has hash
        // 03486fe2b699cc384427e3249569621e0e290a62a9be4d91cd45356d6c1acaa5
        // which does meet the channel target
        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: rolled_version,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        assert!(channel.validate_share(share).is_ok());
    }

    #[test]
    fn test_set_target_refreshes_future_jobs() {
        // Regression test: a target set while a future job is queued must also apply to that
        // job once it is activated.
        // Reuses the valid-share test vectors, but tightens the target after the future job
        // was already stored, so the share no longer meets it.
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;
        let version_rolling = true;
        let rollable_extranonce_size = 8u16;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            version_rolling,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

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

        channel
            .on_new_extended_mining_job(future_job.clone())
            .unwrap();

        // the future job was stored under the old target, now we tighten it to
        // 0000500000000000000000000000000000000000000000000000000000000000
        let new_target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x50, 0x00, 0x00,
        ]);
        channel.set_target(new_target).unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits: u32 = 453040064;
        let ntime: u32 = 1745596970;
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

        // this share has hash 00005e460def43b0153246e6300ce38d9da1c9abd8ef2157a88b2e9a12a8524a
        // which meets the old target, but not the new one
        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 102103,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share);

        assert!(matches!(
            res,
            Err(ShareValidationError::DoesNotMeetTarget(_))
        ));
    }

    #[test]
    fn test_chain_tip_update_retires_active_job() {
        // Regression test: on_chain_tip_update used to leave the previously active job in
        // place, so a share arriving before the next SetCustomMiningJobSuccess would still
        // pass the is_active_job check and be re-hashed against the new prev_hash.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        // a non-future job is activated immediately
        let active_job = NewExtendedMiningJob {
            channel_id,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(1745596970)),
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

        channel.on_new_extended_mining_job(active_job).unwrap();
        assert!(channel.get_active_job().is_some());

        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        channel
            .on_chain_tip_update(ChainTip::new(prev_hash.into(), 545259519, 1745596980))
            .unwrap();

        // the job that was active under the previous chain tip is now stale
        assert!(channel.get_active_job().is_none());
        assert_eq!(channel.get_stale_jobs_count(), 1);
        assert!(channel.get_stale_job(1).is_some());
        assert_eq!(channel.get_past_jobs_count(), 0);
    }

    #[test]
    fn test_set_new_prev_hash_retires_active_job() {
        // Regression test: the previously active job used to be silently overwritten by the
        // activated future job, landing in neither past_jobs nor stale_jobs, so a late share
        // for it was rejected as InvalidJobId instead of Stale.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let job_template = NewExtendedMiningJob {
            channel_id,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(1745596970)),
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

        // job 1 is a non-future job, so it becomes active immediately
        channel
            .on_new_extended_mining_job(job_template.clone())
            .unwrap();

        // job 2 is a future job, waiting for a SetNewPrevHash
        let mut future_job = job_template;
        future_job.job_id = 2;
        future_job.min_ntime = Sv2Option::new(None);
        channel.on_new_extended_mining_job(future_job).unwrap();

        let prev_hash: [u8; 32] = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];

        // a SetNewPrevHash for an unknown job id must leave channel state untouched
        let unknown_job_set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: 42,
            prev_hash: prev_hash.into(),
            nbits: 545259519,
            min_ntime: 1745596980,
        };
        assert!(matches!(
            channel.on_set_new_prev_hash(unknown_job_set_new_prev_hash),
            Err(ExtendedChannelError::JobIdNotFound)
        ));
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert_eq!(channel.get_stale_jobs_count(), 0);

        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: 2,
            prev_hash: prev_hash.into(),
            nbits: 545259519,
            min_ntime: 1745596980,
        };
        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // job 1 was active under the previous chain tip, so it is now stale
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 2);
        assert_eq!(channel.get_stale_jobs_count(), 1);
        assert!(channel.get_stale_job(1).is_some());
        assert_eq!(channel.get_past_jobs_count(), 0);
    }

    // Builds an extended channel whose past jobs sit at the MAX_PAST_JOBS cap, with job
    // MAX_PAST_JOBS as the active job. Returns the channel and the job used as template.
    fn extended_channel_with_past_jobs_at_cap() -> (ExtendedChannel, NewExtendedMiningJob) {
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let job_template = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            min_ntime: Sv2Option::new(Some(1745596970)),
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

        // jobs 0..=MAX_PAST_JOBS are immediately active, each retiring its predecessor, so
        // past jobs end up exactly at the cap
        for job_id in 0..=MAX_PAST_JOBS as u32 {
            let mut job = job_template.clone();
            job.job_id = job_id;
            channel.on_new_extended_mining_job(job).unwrap();
        }
        assert_eq!(channel.get_past_jobs_count(), MAX_PAST_JOBS);

        (channel, job_template)
    }

    #[test]
    fn test_set_new_prev_hash_keeps_all_past_jobs_in_stale_set() {
        // Regression test: with past jobs at the MAX_PAST_JOBS cap, retiring the displaced
        // active job through the capped past path evicted the oldest past job right before
        // past drained into stale, so its late share was rejected as InvalidJobId instead of
        // Stale. The displaced job must go stale with the whole past set (bounded at
        // MAX_PAST_JOBS + 1).
        let (mut channel, job_template) = extended_channel_with_past_jobs_at_cap();
        let channel_id = job_template.channel_id;

        // a future job to activate on the tip transition
        let future_job_id = 100;
        let mut future_job = job_template;
        future_job.job_id = future_job_id;
        future_job.min_ntime = Sv2Option::new(None);
        channel.on_new_extended_mining_job(future_job).unwrap();

        let prev_hash: [u8; 32] = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: future_job_id,
                prev_hash: prev_hash.into(),
                nbits: 545259519,
                min_ntime: 1745596980,
            })
            .unwrap();

        // the displaced active job and every retained past job are stale — none dropped
        assert_eq!(channel.get_stale_jobs_count(), MAX_PAST_JOBS + 1);
        for job_id in 0..=MAX_PAST_JOBS as u32 {
            assert!(channel.get_stale_job(job_id).is_some());
        }
        assert_eq!(channel.get_past_jobs_count(), 0);
        assert_eq!(
            channel.get_active_job().unwrap().job_message.job_id,
            future_job_id
        );
    }

    #[test]
    fn test_chain_tip_update_keeps_all_past_jobs_in_stale_set() {
        // Same regression as test_set_new_prev_hash_keeps_all_past_jobs_in_stale_set, for the
        // on_chain_tip_update path used by Job Declarator Clients.
        let (mut channel, _job_template) = extended_channel_with_past_jobs_at_cap();

        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        channel
            .on_chain_tip_update(ChainTip::new(prev_hash.into(), 545259519, 1745596980))
            .unwrap();

        // the displaced active job and every retained past job are stale — none dropped
        assert_eq!(channel.get_stale_jobs_count(), MAX_PAST_JOBS + 1);
        for job_id in 0..=MAX_PAST_JOBS as u32 {
            assert!(channel.get_stale_job(job_id).is_some());
        }
        assert_eq!(channel.get_past_jobs_count(), 0);
        assert!(channel.get_active_job().is_none());
    }

    #[test]
    fn test_share_validation_ntime_below_job_min_ntime() {
        // Regression test: an immediately-active job carries its own min_ntime, which may be
        // later than the chain tip's minimum. A share in the gap
        // (chain_tip.min_ntime <= ntime < job.min_ntime) must be rejected.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let job = |job_id: u32, min_ntime: Option<u32>| NewExtendedMiningJob {
            channel_id,
            job_id,
            min_ntime: Sv2Option::new(min_ntime),
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

        let share = |sequence_number: u32, job_id: u32, ntime: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id,
            nonce: 741057,
            ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        // activate a chain tip at nTime t via a future job
        let tip_ntime: u32 = 1745596930;
        channel.on_new_extended_mining_job(job(1, None)).unwrap();
        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 1,
                prev_hash: [
                    200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                    205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
                ]
                .into(),
                nbits: 453040064,
                min_ntime: tip_ntime,
            })
            .unwrap();

        // re-confirm the chain-tip lower bound still holds
        let res = channel.validate_share(share(0, 1, tip_ntime - 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // install an immediately-active job whose own min_ntime is later than the tip's
        let job_min_ntime = tip_ntime + 3;
        channel
            .on_new_extended_mining_job(job(2, Some(job_min_ntime)))
            .unwrap();

        // a share in the gap (at or above the tip's min_ntime, below the job's) is rejected
        let res = channel.validate_share(share(1, 2, job_min_ntime - 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // the upper bound is anchored at the job's min_ntime as well: one second past it is
        // rejected, exactly on it is accepted
        let res = channel.validate_share(share(
            3,
            2,
            job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME + 1,
        ));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        let res = channel.validate_share(share(4, 2, job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        let res = channel.validate_share(share(2, 2, job_min_ntime));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_share_validation_ntime_above_max_future_block_time() {
        // Regression test: a share ntime beyond the chain tip's min_ntime +
        // MAX_FUTURE_BLOCK_TIME would put a consensus-invalid timestamp in the block header,
        // so it must be rejected; ntime exactly on the bound is still accepted.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        channel
            .on_new_extended_mining_job(NewExtendedMiningJob {
                channel_id,
                job_id: 1,
                min_ntime: Sv2Option::new(None),
                version: 536870912,
                version_rolling_allowed: true,
                coinbase_tx_prefix: vec![
                    2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
                ]
                .try_into()
                .unwrap(),
                coinbase_tx_suffix: vec![
                    255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183,
                    220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252,
                    0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113,
                    209, 222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153,
                    98, 180, 139, 235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0,
                ]
                .try_into()
                .unwrap(),
                merkle_path: vec![].try_into().unwrap(),
            })
            .unwrap();

        let tip_ntime: u32 = 1745596930;
        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 1,
                prev_hash: [
                    200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                    205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
                ]
                .into(),
                nbits: 453040064,
                min_ntime: tip_ntime,
            })
            .unwrap();

        let share = |sequence_number: u32, ntime: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 741057,
            ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        // one second above the bound: rejected before any PoW evaluation
        let res = channel.validate_share(share(0, tip_ntime + crate::MAX_FUTURE_BLOCK_TIME + 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // u32::MAX is likewise rejected (the bound saturates instead of wrapping)
        let res = channel.validate_share(share(1, u32::MAX));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // exactly on the bound the share is accepted (channel target is permissive)
        let res = channel.validate_share(share(2, tip_ntime + crate::MAX_FUTURE_BLOCK_TIME));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_reused_job_id_resolves_to_the_live_job() {
        // Upstream job IDs carry no uniqueness guarantee: a job may be installed under the ID of
        // a job that went stale, both when a future job is activated and when an immediately-
        // active job arrives. An ID names either a live job or a stale one, never both, so the
        // stale namesake is dropped and shares for the ID validate against the live job.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let job = |min_ntime: Option<u32>| NewExtendedMiningJob {
            channel_id,
            job_id: 1,
            min_ntime: Sv2Option::new(min_ntime),
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

        // job 1 is active under the current tip, and upstream reuses its ID for the next tip's
        // future job
        channel
            .on_new_extended_mining_job(job(Some(1745596970)))
            .unwrap();
        channel.on_new_extended_mining_job(job(None)).unwrap();

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 1,
                prev_hash: [
                    200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                    205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
                ]
                .into(),
                nbits: 453040064,
                min_ntime: 1745596980,
            })
            .unwrap();

        // ID 1 names the activated job only: its displaced namesake is gone from the stale set
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert!(channel.get_stale_job(1).is_none());
        assert_eq!(channel.get_stale_jobs_count(), 0);

        // a share for the live job is validated (channel target is permissive)
        let share = |sequence_number: u32, ntime: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 0,
            ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share(0, 1745596980)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // job 1 goes stale on the next tip transition, and upstream then reuses its ID for an
        // immediately-active job
        let mut future_job = job(None);
        future_job.job_id = 2;
        channel.on_new_extended_mining_job(future_job).unwrap();
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 2,
                prev_hash: [
                    154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107,
                    73, 34, 0, 162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
                ]
                .into(),
                nbits: 453040064,
                min_ntime: 1745596990,
            })
            .unwrap();
        assert!(channel.get_stale_job(1).is_some());
        channel
            .on_new_extended_mining_job(job(Some(1745596990)))
            .unwrap();

        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert!(channel.get_stale_job(1).is_none());
        assert!(matches!(
            channel.validate_share(share(1, 1745596990)),
            Ok(ShareValidationResult::Valid(_))
        ));
    }

    #[test]
    fn test_repeated_prev_hash_keeps_seen_shares() {
        // Job IDs are not committed into the block header, so a non-conforming upstream can send
        // an identical future job under a new job_id and repeat the same SetNewPrevHash. The
        // replacement job commits to the same headers as the previous one, so the
        // validated-share hashes must survive the repeated tip or the same proof is validated
        // (and forwarded) again under the new job_id.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let future_job = |job_id: u32| NewExtendedMiningJob {
            channel_id,
            job_id,
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

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let set_new_prev_hash = |job_id: u32| SetNewPrevHashMp {
            channel_id,
            job_id,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits: 453040064,
            min_ntime: 1745596970,
        };

        let share = |sequence_number: u32, job_id: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        channel.on_new_extended_mining_job(future_job(1)).unwrap();
        channel.on_set_new_prev_hash(set_new_prev_hash(1)).unwrap();
        assert!(matches!(
            channel.validate_share(share(0, 1)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // the upstream repeats the tip under a new job_id: same header space, new job
        channel.on_new_extended_mining_job(future_job(2)).unwrap();
        channel.on_set_new_prev_hash(set_new_prev_hash(2)).unwrap();

        // the identical proof under the new job_id is a duplicate, not a second validation
        assert!(matches!(
            channel.validate_share(share(1, 2)),
            Err(ShareValidationError::DuplicateShare(_))
        ));
        assert_eq!(channel.get_share_accounting().get_validated_shares(), 1);
    }

    // An immediately-active job whose coinbase carries a 32-byte extranonce.
    fn active_job_template(job_id: u32) -> NewExtendedMiningJob {
        NewExtendedMiningJob {
            channel_id: 1,
            job_id,
            min_ntime: Sv2Option::new(Some(1745596970)),
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
        }
    }

    // Builds an extended channel from a real allocator that only has room for two channels,
    // creates an active job under the first allocated prefix, then rotates the channel onto the
    // second one.
    //
    // Returns the allocator (now with both slots handed out), the channel and the bytes of the
    // rotated-out prefix.
    fn extended_channel_with_rotated_extranonce_prefix(
    ) -> (ExtranonceAllocator, ExtendedChannel, Vec<u8>) {
        let mut allocator = ExtranonceAllocator::new(vec![], 32, 2).unwrap();
        let prefix_1 = allocator.allocate_extended(8).unwrap();
        let prefix_2 = allocator.allocate_extended(8).unwrap();
        assert_ne!(prefix_1.as_bytes(), prefix_2.as_bytes());
        assert_eq!(allocator.allocated_count(), 2);

        let prefix_1_bytes = prefix_1.as_bytes().to_vec();
        let rollable_extranonce_size = (32 - prefix_1_bytes.len()) as u16;

        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            prefix_1.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            rollable_extranonce_size,
            None,
        )
        .unwrap();

        // an immediately-active job, created under the first prefix
        channel
            .on_new_extended_mining_job(active_job_template(1))
            .unwrap();
        assert_eq!(
            channel.get_active_job().unwrap().extranonce_prefix,
            prefix_1_bytes
        );

        // rotate the channel onto the second prefix, while the job above is still live
        channel.set_extranonce_prefix(prefix_2.into()).unwrap();

        (allocator, channel, prefix_1_bytes)
    }

    #[test]
    fn test_mixed_prefix_updates_reserve_slot_until_job_eviction() {
        for future in [false, true] {
            let mut allocator =
                ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 32, 2).unwrap();
            let prefix = allocator.allocate_extended(8).unwrap();
            let old_bytes = prefix.as_bytes().to_vec();
            let rollable_size = (32 - prefix.len()) as u16;

            let mut channel = ExtendedChannel::new(
                1,
                "user_identity".to_string(),
                prefix.into(),
                Target::from_le_bytes([0xff; 32]),
                1.0,
                true,
                rollable_size,
                Some(1),
            )
            .unwrap();
            let job_id = 1;
            let mut job = active_job_template(job_id);
            if future {
                job.min_ntime = Sv2Option::new(None);
            }
            channel.on_new_extended_mining_job(job).unwrap();

            // Change only upstream_prefix, preserving local_prefix and local_index.
            allocator.set_upstream_prefix(vec![0xcc]).unwrap();
            channel.set_upstream_extranonce_prefix(&[0xcc]).unwrap();
            assert_eq!(allocator.allocated_count(), 1);
            assert_eq!(&channel.get_extranonce_prefix()[1..], &old_bytes[1..]);

            // Repeated updates without new jobs retain only the original job's prefix snapshot.
            for upstream in [0xdd, 0xaa, 0xcc, 0xcc] {
                allocator.set_upstream_prefix(vec![upstream]).unwrap();
                channel.set_upstream_extranonce_prefix(&[upstream]).unwrap();
                assert_eq!(channel.retired_extranonce_prefixes.len(), 1);
            }
            let current_bytes = channel.get_extranonce_prefix().to_vec();
            assert!(matches!(
                channel.set_upstream_extranonce_prefix(&[0xee; 2]),
                Err(ExtendedChannelError::NewExtranoncePrefixTooLarge)
            ));
            assert_eq!(channel.get_extranonce_prefix(), current_bytes);
            assert_eq!(channel.retired_extranonce_prefixes.len(), 1);
            assert_eq!(allocator.allocated_count(), 1);

            // A later whole-prefix rotation must not release the slot used by job 1.
            let replacement = allocator.allocate_extended(8).unwrap();
            channel.set_extranonce_prefix(replacement.into()).unwrap();
            let job = if future {
                channel.get_future_job(job_id).unwrap()
            } else {
                channel.get_active_job().unwrap()
            };
            assert_eq!(job.extranonce_prefix, old_bytes);
            assert_eq!(allocator.allocated_count(), 2);
            allocator.set_upstream_prefix(vec![0xaa]).unwrap();
            assert!(matches!(
                allocator.allocate_extended(8),
                Err(ExtranonceAllocatorError::CapacityExhausted)
            ));

            // Activating the future job must preserve its old prefix and allocation.
            if future {
                channel
                    .on_set_new_prev_hash(SetNewPrevHashMp {
                        channel_id: 1,
                        job_id,
                        prev_hash: [2; 32].into(),
                        min_ntime: 1745596970,
                        nbits: 545259519,
                    })
                    .unwrap();
                assert_eq!(
                    channel.get_active_job().unwrap().extranonce_prefix,
                    old_bytes
                );
                assert_eq!(allocator.allocated_count(), 2);
            }

            // The existing one-past-job limit evicts job 1 after jobs 2 and 3 arrive.
            for job_id in 2..=3 {
                channel
                    .on_new_extended_mining_job(active_job_template(job_id))
                    .unwrap();
            }
            assert!(channel.get_past_job(1).is_none());
            assert_eq!(allocator.allocated_count(), 1);
            let reused = allocator.allocate_extended(8).unwrap();
            assert_eq!(reused.as_bytes(), old_bytes);
            drop(reused);
            drop(channel);
            assert_eq!(allocator.allocated_count(), 0);
        }
    }

    #[test]
    fn test_rotated_extranonce_prefix_slot_not_reused_while_job_live() {
        // Rotating a locally allocated extranonce prefix must not return the old prefix's
        // allocator slot to the free pool while jobs created under it can still accept shares.
        // Otherwise the allocator could hand the very same extranonce space to a second live
        // channel, making the same work replayable across both.
        let (mut allocator, channel, prefix_1_bytes) =
            extended_channel_with_rotated_extranonce_prefix();

        // the rotated-out slot is still reserved, so the allocator is still full
        assert_eq!(allocator.allocated_count(), 2);
        assert!(matches!(
            allocator.allocate_extended(8),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));

        // and the pre-rotation job is still live under the old prefix bytes
        assert_eq!(
            channel.get_active_job().unwrap().extranonce_prefix,
            prefix_1_bytes
        );
    }

    #[test]
    fn test_retired_extranonce_prefix_released_after_jobs_go_stale() {
        // Counterpart of the test above: the deferred release must actually happen once the
        // jobs created under the old prefix become stale, otherwise slots would leak.
        let (mut allocator, mut channel, _prefix_1_bytes) =
            extended_channel_with_rotated_extranonce_prefix();

        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        channel
            .on_chain_tip_update(ChainTip::new(prev_hash.into(), 545259519, 1745596980))
            .unwrap();
        assert!(channel.get_stale_job(1).is_some());

        // the pre-rotation job can no longer accept shares, so its prefix was released
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_extended(8).is_ok());
    }

    #[test]
    fn test_retired_extranonce_prefix_released_after_job_eviction() {
        // Eviction counterpart of the test above: when the last job created under a
        // rotated-out prefix is evicted from past jobs, the prefix's slot must be released
        // right away — an upstream withholding the next chain transition must not be able to
        // pin allocator slots.
        let (mut allocator, mut channel, _prefix_1_bytes) =
            extended_channel_with_rotated_extranonce_prefix();

        // flood enough immediately-active jobs to push the pre-rotation job out of past jobs
        for job_id in 2..2 + MAX_PAST_JOBS as u32 + 2 {
            channel
                .on_new_extended_mining_job(active_job_template(job_id))
                .unwrap();
        }
        assert!(channel.get_past_job(1).is_none());

        // the evicted job was the last reference to the rotated-out prefix, so its slot is free
        // again
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_extended(8).is_ok());
    }

    #[test]
    fn test_retired_extranonce_prefix_survives_install_of_a_job_under_its_bytes() {
        // Retiring the displaced job may evict a past job and prune retired extranonce
        // prefixes. The job being installed is a live user of the current prefix bytes, so it
        // must be in place when that prune runs: a retired prefix from a recreated allocator can
        // share those bytes, and pruning before the install would release its slot while the
        // new job goes on to accept shares under them.
        let mut allocator_1 = ExtranonceAllocator::new(vec![], 32, 1).unwrap();
        let prefix_1 = allocator_1.allocate_extended(8).unwrap();
        let mut allocator_2 = ExtranonceAllocator::new(vec![], 32, 1).unwrap();
        let prefix_2 = allocator_2.allocate_extended(8).unwrap();
        assert_eq!(prefix_1.as_bytes(), prefix_2.as_bytes());
        let prefix_len = prefix_1.as_bytes().len();
        let rollable_extranonce_size = (32 - prefix_len) as u16;

        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            prefix_1.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            rollable_extranonce_size,
            Some(1),
        )
        .unwrap();

        // job 1 under the first prefix, then a rotation onto a wire prefix and job 2 under it
        channel
            .on_new_extended_mining_job(active_job_template(1))
            .unwrap();
        channel
            .set_extranonce_prefix(ExtranoncePrefix::from_wire(vec![7; prefix_len]).unwrap())
            .unwrap();
        channel
            .on_new_extended_mining_job(active_job_template(2))
            .unwrap();
        assert_eq!(allocator_1.allocated_count(), 1);

        // rotate onto the second allocator's byte-identical prefix; installing job 3 under it
        // retires job 2 and evicts job 1, the last job created under the first prefix
        channel.set_extranonce_prefix(prefix_2.into()).unwrap();
        channel
            .on_new_extended_mining_job(active_job_template(3))
            .unwrap();
        assert!(channel.get_past_job(1).is_none());

        // job 3 lives under those bytes, so the first allocator's slot stays reserved
        assert_eq!(allocator_1.allocated_count(), 1);
        assert!(matches!(
            allocator_1.allocate_extended(8),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));
    }

    #[test]
    fn test_zero_target_is_rejected() {
        // a zero target is refused at construction and on SetTarget, leaving the channel
        // unchanged (see InvalidTarget)
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let res = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            Target::ZERO,
            1.0,
            true,
            8u16,
            None,
        );
        assert!(matches!(res, Err(ExtendedChannelError::InvalidTarget)));

        let target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        // a queued future job, whose target a SetTarget would otherwise refresh
        let mut future_job = active_job_template(1);
        future_job.min_ntime = Sv2Option::new(None);
        channel.on_new_extended_mining_job(future_job).unwrap();

        assert!(matches!(
            channel.set_target(Target::ZERO),
            Err(ExtendedChannelError::InvalidTarget)
        ));
        assert_eq!(channel.get_target(), &target);
        assert_eq!(channel.get_future_job(1).unwrap().target, target);
    }

    #[test]
    fn test_immediately_active_job_below_chain_tip_min_ntime_is_rejected() {
        // An immediately-active job is mined against the current chain tip, whose min_ntime is
        // the smallest nTime available for it, so a job with a lower min_ntime must be refused
        // and leave the channel unchanged. A min_ntime equal to the tip's is the lowest allowed.
        let channel_id = 1;
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            ])
            .unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        let tip_ntime: u32 = 1745596970;
        let mut future_job = active_job_template(1);
        future_job.min_ntime = Sv2Option::new(None);
        channel.on_new_extended_mining_job(future_job).unwrap();
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 1,
                prev_hash: [
                    200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                    205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
                ]
                .into(),
                nbits: 453040064,
                min_ntime: tip_ntime,
            })
            .unwrap();

        let mut below_tip = active_job_template(2);
        below_tip.min_ntime = Sv2Option::new(Some(tip_ntime - 1));
        assert!(matches!(
            channel.on_new_extended_mining_job(below_tip),
            Err(ExtendedChannelError::JobMinNtimeBelowChainTip)
        ));
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert_eq!(channel.get_past_jobs_count(), 0);

        let mut at_tip = active_job_template(3);
        at_tip.min_ntime = Sv2Option::new(Some(tip_ntime));
        channel.on_new_extended_mining_job(at_tip).unwrap();
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 3);
    }

    #[test]
    fn test_set_chain_tip_replacement_retires_the_jobs_of_the_previous_tip() {
        // Shares are hashed against the channel's current tip, so a job built under a tip that
        // set_chain_tip replaces must go stale, as it does on the message-driven transitions:
        // otherwise a late share for it would be validated against a header the miner was never
        // assigned. Re-setting the current tip changes nothing.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            None,
        )
        .unwrap();

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let nbits = 453040064;
        let first_tip = ChainTip::new(
            [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits,
            1745596970,
        );
        channel.set_chain_tip(first_tip.clone());
        channel
            .on_new_extended_mining_job(active_job_template(1))
            .unwrap();

        let share = |sequence_number: u32, nonce: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce,
            ntime: 1745596970,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share(0, 0)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // the current tip again is not a transition: the job stays active
        channel.set_chain_tip(first_tip);
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert!(matches!(
            channel.validate_share(share(1, 1)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // a different prev_hash retires the job: its late share is stale, not re-hashed
        channel.set_chain_tip(ChainTip::new(
            [
                154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73,
                34, 0, 162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
            ]
            .into(),
            nbits,
            1745596980,
        ));
        assert!(channel.get_active_job().is_none());
        assert!(channel.get_stale_job(1).is_some());
        assert!(matches!(
            channel.validate_share(share(2, 0)),
            Err(ShareValidationError::Stale(_))
        ));
    }
}
