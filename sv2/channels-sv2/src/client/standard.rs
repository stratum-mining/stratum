//! Sv2 Standard Channel - Mining Client Abstraction.
//!
//! This module provides the [`StandardChannel`] struct, which models the state of a mining
//! client's Sv2 Standard Channel. It tracks channel-level job management, share accounting,
//! and chain tip state, enabling share validation and mining job lifecycle management.

extern crate alloc;
use super::{HashMap, MAX_FUTURE_JOBS, MAX_PAST_JOBS};
use crate::{
    chain_tip::ChainTip,
    client::{
        error::StandardChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    extranonce_manager::{prefix::RetiredExtranoncePrefixes, ExtranoncePrefix},
    merkle_root::merkle_root_from_path,
    target::{bytes_to_hex, u256_to_block_hash},
    MAX_EXTRANONCE_LEN, MAX_FUTURE_BLOCK_TIME, VERSION_ROLLING_MASK,
};
use alloc::{collections::VecDeque, format, string::String, vec::Vec};
use binary_sv2::Sv2OptionOwned;
use bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    CompactTarget, Target,
};
use mining_sv2::{
    NewExtendedMiningJobOwned, NewMiningJobOwned, SetNewPrevHashOwned as SetNewPrevHashMp,
    SubmitSharesStandardOwned, ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE, ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE, ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
};
use tracing::debug;

/// A standard mining job as tracked by a client [`StandardChannel`].
#[derive(Debug, Clone, PartialEq)]
pub struct StandardJob {
    /// The [`NewMiningJob`](mining_sv2::NewMiningJob) message the job was created from, with
    /// `min_ntime` set once the job is activated.
    pub job_message: NewMiningJobOwned,
    /// The `extranonce_prefix` in use when the job was created.
    pub extranonce_prefix: Vec<u8>,
    /// The target the job's shares are validated against.
    pub target: Target,
}

/// Mining Client abstraction over the state of a Sv2 Standard Channel.
///
/// Tracks:
/// - unique channel ID
/// - user identity string
/// - unique extranonce prefix
/// - channel target
/// - nominal hashrate in h/s
/// - future mining jobs (indexed by job_id, activated upon [`NewMiningJob`](mining_sv2::NewMiningJob) receipt, capped at [`MAX_FUTURE_JOBS`])
/// - active mining job
/// - past jobs (active jobs under current chain tip, indexed by job_id, capped at
///   [`MAX_PAST_JOBS`])
/// - stale jobs (jobs from previous chain tip, indexed by job_id). Upstream job IDs carry no
///   uniqueness guarantee, so a job may be installed under an ID a stale job holds; an ID names
///   either a live job or a stale one, never both, and the stale namesake is dropped. A late
///   share for it is then validated against the live job, as the channel cannot tell the two
///   apart.
/// - share accounting state
/// - chain tip state
/// - extranonce prefixes rotated out of the channel that live jobs were created under (see
///   [`set_extranonce_prefix`](Self::set_extranonce_prefix))
#[derive(Debug)]
pub struct StandardChannel {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: ExtranoncePrefix,
    target: Target,
    nominal_hashrate: f32,
    future_jobs: HashMap<u32, StandardJob>,
    // Future job IDs ordered by receipt, oldest at the front and newest at the back.
    // Replaced IDs move to the back; overflow evicts from the front.
    future_job_order: VecDeque<u32>,
    active_job: Option<StandardJob>,
    past_jobs: HashMap<u32, StandardJob>,
    // Past job IDs ordered by retirement, oldest at the front and newest at the back.
    // Replaced IDs move to the back; overflow evicts from the front.
    past_job_order: VecDeque<u32>,
    stale_jobs: HashMap<u32, StandardJob>,
    // Cap on `past_jobs` under the current chain tip, resolved from the constructor's
    // `Option<usize>`; `None` and `Some(0)` both resolve to `MAX_PAST_JOBS`.
    max_past_jobs: usize,
    share_accounting: ShareAccounting,
    chain_tip: Option<ChainTip>,
    // extranonce prefixes rotated out of the channel that are still referenced by at least one
    // job that can accept shares, so that their allocator slots stay reserved
    retired_extranonce_prefixes: RetiredExtranoncePrefixes,
}

impl StandardChannel {
    /// Creates a new [`StandardChannel`] instance with provided channel parameters.
    ///
    /// `max_past_jobs` caps the past jobs retained under the current chain tip. `None` and
    /// `Some(0)` both select [`MAX_PAST_JOBS`].
    ///
    /// Returns [`StandardChannelError::InvalidTarget`] if `target` is zero.
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: ExtranoncePrefix,
        target: Target,
        nominal_hashrate: f32,
        max_past_jobs: Option<usize>,
    ) -> Result<Self, StandardChannelError> {
        if target == Target::ZERO {
            return Err(StandardChannelError::InvalidTarget);
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
            target,
            nominal_hashrate,
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

    /// Returns the channel ID.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string associated with this channel.
    pub fn get_user_identity(&self) -> &str {
        &self.user_identity
    }

    /// Returns the latest chain tip information, if any.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Sets the [`ChainTip`].
    ///
    /// A first tip only initializes the channel, and setting the current tip again changes
    /// nothing. Replacing the tip with a different one is a chain-tip transition: future jobs are
    /// cleared, the active and past jobs go stale rather than stay validatable against a header
    /// the miner was never assigned, and seen shares are flushed if `prev_hash` changed (a
    /// repeated `prev_hash` keeps them, see [`ShareAccounting::flush_seen_shares`]).
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        match &self.chain_tip {
            None => self.chain_tip = Some(chain_tip),
            Some(current) if *current == chain_tip => {}
            Some(_) => self.update_chain_tip(chain_tip),
        }
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

        // the job that was active under the previous chain tip goes stale with them rather
        // than being silently dropped, bypassing the MAX_PAST_JOBS cap: retiring it through
        // the capped past path would push the oldest past job out of the stale set, and a
        // late share for either job would be rejected as InvalidJobId instead of Stale
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

    /// Sets the extranonce prefix for the channel.
    ///
    /// All new jobs will use the new extranonce prefix. Jobs created before
    /// this call will continue using their previous prefix for share validation.
    ///
    /// Because of that, a previous prefix minted by a local
    /// [`ExtranonceAllocator`](crate::extranonce_manager::ExtranonceAllocator) (e.g. by a proxy
    /// sub-allocating an upstream-assigned extranonce space) is not released here: its slot
    /// stays reserved until no future, active or past job created under it remains, so that the
    /// allocator cannot hand the same extranonce space to another live channel while those jobs
    /// still validate shares. Wire-sourced prefixes hold no slot and are simply dropped.
    ///
    /// Returns an error if the prefix is too large.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: ExtranoncePrefix,
    ) -> Result<(), StandardChannelError> {
        if extranonce_prefix.len() > MAX_EXTRANONCE_LEN as usize {
            return Err(StandardChannelError::NewExtranoncePrefixTooLarge);
        }

        let retired_extranonce_prefix =
            core::mem::replace(&mut self.extranonce_prefix, extranonce_prefix);
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
    /// For an allocator-produced prefix, `local_prefix | local_index`, rollable padding, and its
    /// allocation are preserved. For a wire-sourced prefix, the entire prefix is
    /// `upstream_prefix` and is replaced. Jobs received before this call retain their captured
    /// prefix bytes; new jobs use the updated prefix. Returns
    /// [`StandardChannelError::NewExtranoncePrefixTooLarge`] without changing the channel if the
    /// resulting prefix would exceed [`MAX_EXTRANONCE_LEN`].
    ///
    /// Old prefix bytes share ownership of the same allocator slot while any future, active or
    /// past job uses them. A later `set_extranonce_prefix` rotation cannot release that slot
    /// prematurely. Once those jobs are stale or evicted, only the current prefix (if it still
    /// uses this allocation) keeps the slot reserved. This update consumes no additional slot.
    pub fn set_upstream_extranonce_prefix(
        &mut self,
        upstream_prefix: &[u8],
    ) -> Result<(), StandardChannelError> {
        let snapshot = self
            .extranonce_prefix
            .snapshot_for_upstream_update(upstream_prefix);
        self.extranonce_prefix
            .set_upstream_prefix(upstream_prefix)
            .map_err(|_| StandardChannelError::NewExtranoncePrefixTooLarge)?;
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

    /// Returns the bytes representing the first part of the extranonce.
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

    /// Returns the current target for the channel.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Sets a new target for the channel.
    ///
    /// Per the Sv2 spec, the new target also applies to jobs that were already received with an
    /// empty `min_ntime` (i.e. queued future jobs), so their associated target is refreshed here.
    /// Jobs that were already received with a set `min_ntime` (active, past and stale jobs) keep
    /// their target.
    ///
    /// Returns [`StandardChannelError::InvalidTarget`] if `target` is zero, leaving the channel
    /// unchanged.
    pub fn set_target(&mut self, target: Target) -> Result<(), StandardChannelError> {
        if target == Target::ZERO {
            return Err(StandardChannelError::InvalidTarget);
        }

        self.target = target;
        for future_job in self.future_jobs.values_mut() {
            future_job.target = target;
        }

        Ok(())
    }

    /// Returns the nominal hashrate of the channel in h/s.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Returns an iterator over all future jobs for this channel.
    ///
    /// The list is cleared once a [`StandardChannel::on_set_new_prev_hash`] is processed, and holds
    /// at most [`MAX_FUTURE_JOBS`] jobs (oldest evicted first).
    pub fn get_future_jobs(&self) -> impl Iterator<Item = (&u32, &StandardJob)> + '_ {
        self.future_jobs.iter()
    }

    /// Returns a reference to a future job by `job_id`, if present.
    pub fn get_future_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.future_jobs.get(&job_id)
    }

    /// Returns the number of future jobs tracked by this channel.
    pub fn get_future_jobs_count(&self) -> usize {
        self.future_jobs.len()
    }

    /// Returns the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&StandardJob> {
        self.active_job.as_ref()
    }

    /// Returns an iterator over all past jobs for the channel (active jobs under current chain tip).
    ///
    /// At most [`MAX_PAST_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_past_jobs(&self) -> impl Iterator<Item = (&u32, &StandardJob)> + '_ {
        self.past_jobs.iter()
    }

    /// Returns a reference to a past job by `job_id`, if present.
    pub fn get_past_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.past_jobs.get(&job_id)
    }

    /// Returns the number of past jobs tracked by this channel.
    ///
    /// At most [`MAX_PAST_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_past_jobs_count(&self) -> usize {
        self.past_jobs.len()
    }

    /// Returns an iterator over all stale jobs for the channel (jobs from previous chain tip).
    pub fn get_stale_jobs(&self) -> impl Iterator<Item = (&u32, &StandardJob)> + '_ {
        self.stale_jobs.iter()
    }

    /// Returns a reference to a stale job by `job_id`, if present.
    pub fn get_stale_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.stale_jobs.get(&job_id)
    }

    /// Returns the number of stale jobs tracked by this channel.
    pub fn get_stale_jobs_count(&self) -> usize {
        self.stale_jobs.len()
    }

    /// Returns the share accounting state for this channel.
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

    /// Handles a new group channel job by converting it into a standard job
    /// and activating it in this channel's context.
    ///
    /// The new job is constructed using the current extranonce prefix.
    ///
    /// Returns [`StandardChannelError::InvalidCoinbase`] if the job's coinbase transaction
    /// (prefix + this channel's extranonce prefix + suffix) is malformed, and
    /// [`StandardChannelError::JobMinNtimeBelowChainTip`] if the job is immediately active with a
    /// `min_ntime` below the current chain tip's; in both cases the job is discarded and channel
    /// state is left untouched.
    pub fn on_new_group_channel_job(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJobOwned,
    ) -> Result<(), StandardChannelError> {
        let merkle_root = merkle_root_from_path(
            new_extended_mining_job.coinbase_tx_prefix.as_bytes(),
            new_extended_mining_job.coinbase_tx_suffix.as_bytes(),
            self.extranonce_prefix.as_bytes(),
            new_extended_mining_job.merkle_path.as_slice(),
        )
        .ok_or(StandardChannelError::InvalidCoinbase)?
        .into();

        let new_mining_job = NewMiningJobOwned {
            channel_id: self.channel_id,
            job_id: new_extended_mining_job.job_id,
            merkle_root,
            version: new_extended_mining_job.version,
            min_ntime: new_extended_mining_job.min_ntime,
        };

        self.on_new_mining_job(new_mining_job)
    }

    /// Handles a newly received [`NewMiningJob`](mining_sv2::NewMiningJob) message from upstream.
    ///
    /// - If `min_ntime` is present, the job is activated and replaces the current active job.
    /// - If `min_ntime` is empty, the job is added to future jobs. At most [`MAX_FUTURE_JOBS`]
    ///   future jobs are kept: storing a new one beyond that limit evicts the oldest.
    /// - If an active job exists, it is moved to past jobs on activation. At most
    ///   [`MAX_PAST_JOBS`] past jobs are kept: retiring one beyond that limit evicts the oldest.
    ///
    /// An immediately-active job is mined against the current chain tip, so a `min_ntime` below
    /// the tip's is refused with [`StandardChannelError::JobMinNtimeBelowChainTip`], leaving the
    /// channel unchanged.
    pub fn on_new_mining_job(
        &mut self,
        new_mining_job: NewMiningJobOwned,
    ) -> Result<(), StandardChannelError> {
        match new_mining_job.min_ntime.clone().into_inner() {
            Some(min_ntime) => {
                // the job is mined against the chain tip, whose min_ntime is the smallest nTime
                // available for it; a job allowing earlier shares would have them carry a
                // timestamp the tip declared unavailable
                if self
                    .chain_tip
                    .as_ref()
                    .is_some_and(|chain_tip| min_ntime < chain_tip.min_ntime())
                {
                    return Err(StandardChannelError::JobMinNtimeBelowChainTip);
                }

                // an ID names either a live job or a stale one, never both
                self.stale_jobs.remove(&new_mining_job.job_id);
                // the new job is installed before the displaced one is retired: retirement may
                // prune retired extranonce prefixes, and the new job is a live user of its bytes
                let displaced_job = self.active_job.replace(StandardJob {
                    job_message: new_mining_job,
                    extranonce_prefix: self.extranonce_prefix.as_bytes().to_vec(),
                    target: self.target,
                });
                if let Some(displaced_job) = displaced_job {
                    self.retire_job_to_past(displaced_job);
                }
            }
            None => {
                let job_id = new_mining_job.job_id;
                self.future_jobs.insert(
                    job_id,
                    StandardJob {
                        job_message: new_mining_job,
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

    // Moves a displaced job into past jobs, evicting the oldest past job beyond
    // [`MAX_PAST_JOBS`]. A share against an evicted job is rejected as `InvalidJobId` even
    // though it would otherwise have been accepted and propagated: a bounded loss of
    // creditable work, the price of bounding memory under a hostile upstream.
    fn retire_job_to_past(&mut self, job: StandardJob) {
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

    /// Handles an upstream [`SetNewPrevHash`](SetNewPrevHashMp) message.
    ///
    /// - Activates the matching future job as the new active job.
    /// - Clears all future jobs.
    /// - Marks the previously active job and all past jobs as stale (they are no longer valid for
    ///   share propagation).
    /// - Clears past jobs, and the set of seen shares if `prev_hash` changed (a repeated
    ///   `prev_hash` keeps them, see [`ShareAccounting::flush_seen_shares`]).
    /// - Updates chain tip information. Returns error if no matching future job found, leaving
    ///   channel state untouched.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp,
    ) -> Result<(), StandardChannelError> {
        // the previously active job is only displaced once activation is known to succeed, so
        // that the JobIdNotFound path below does not corrupt channel state
        let previously_active_job = match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(mut activated_job) => {
                activated_job.job_message.min_ntime =
                    Sv2OptionOwned::new(Some(set_new_prev_hash.min_ntime));
                self.active_job.replace(activated_job)
            }
            None => return Err(StandardChannelError::JobIdNotFound),
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

    /// Validates a share before submission upstream.
    ///
    /// - Checks if the share refers to an active or past job; rejects stale jobs.
    /// - Verifies the share meets the channel target, is not a duplicate, is not stale, and has
    ///   `ntime` within `[min_ntime, min_ntime + MAX_FUTURE_BLOCK_TIME]`, where `min_ntime` is
    ///   the referenced job's: the `SetNewPrevHash` timestamp for a job activated from the
    ///   future queue, or the value its own message advertised for an immediately-active job
    ///   (see [`MAX_FUTURE_BLOCK_TIME`] for how this clockless upper bound relates to the
    ///   spec's elapsed-time window).
    /// - Updates share accounting state based on validation result. Duplicate detection is
    ///   bounded at [`MAX_SEEN_SHARES`](crate::client::MAX_SEEN_SHARES) validated shares per
    ///   `prev_hash` (oldest evicted first), so an evicted share can be validated again; see
    ///   [`ShareAccounting`] for why this replay window is accepted on clients.
    /// - Returns whether the share is valid or resulted in a block being found.
    /// - Returns error describing why share is not valid.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesStandardOwned,
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

        let merkle_root = job.job_message.merkle_root.to_array();

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

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

        // Only the non-rollable version bits are compared: `!VERSION_ROLLING_MASK` zeroes
        // the BIP323 general-purpose bits the miner may change, so any remaining difference
        // from the job's advertised version means an unauthorized change. Standard channels
        // always allow version rolling within the mask.
        if (share.version & !VERSION_ROLLING_MASK)
            != (job.job_message.version & !VERSION_ROLLING_MASK)
        {
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
            ));
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
    use super::{ChainTip, StandardJob};
    use crate::{
        client::{
            error::StandardChannelError,
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
            MAX_FUTURE_JOBS, MAX_PAST_JOBS,
        },
        extranonce_manager::{ExtranonceAllocator, ExtranonceAllocatorError, ExtranoncePrefix},
    };
    use binary_sv2::Sv2OptionOwned as Sv2Option;
    use bitcoin::Target;
    use mining_sv2::{
        NewExtendedMiningJobOwned as NewExtendedMiningJob, NewMiningJobOwned as NewMiningJob,
        SetNewPrevHashOwned as SetNewPrevHashMp, SubmitSharesStandardOwned,
        ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    };

    #[test]
    fn set_upstream_extranonce_prefix_preserves_allocation_transactionally() {
        // local_prefix(1) + local_index(1) + padding(3) leave 27 bytes for upstream_prefix.
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 6, 256).unwrap();
        let allocated_prefix = allocator.allocate_standard().unwrap();
        let preserved_bytes = allocated_prefix.as_bytes()[1..].to_vec();
        assert_eq!(preserved_bytes, vec![0xbb, 0, 0, 0, 0]);
        let mut channel = StandardChannel::new(
            1,
            "user_identity".to_string(),
            allocated_prefix.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        channel.set_upstream_extranonce_prefix(&[0xcc; 27]).unwrap();
        assert_eq!(channel.upstream_prefix_len(), 27);
        assert_eq!(&channel.get_extranonce_prefix()[..27], &[0xcc; 27]);
        assert_eq!(&channel.get_extranonce_prefix()[27..], preserved_bytes);
        assert_eq!(allocator.allocated_count(), 1);
        let largest_valid_prefix = channel.get_extranonce_prefix().to_vec();

        assert!(matches!(
            channel.set_upstream_extranonce_prefix(&[0xdd; 28]),
            Err(StandardChannelError::NewExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), largest_valid_prefix);
        assert_eq!(channel.upstream_prefix_len(), 27);
        assert_eq!(allocator.allocated_count(), 1);

        drop(channel);
        assert_eq!(allocator.allocated_count(), 0);
    }

    #[test]
    fn set_upstream_extranonce_prefix_replaces_wire_prefix_transactionally() {
        let mut channel = StandardChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0xaa, 0xbb]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        channel.set_upstream_extranonce_prefix(&[0xcc; 32]).unwrap();
        assert_eq!(channel.get_extranonce_prefix(), &[0xcc; 32]);
        assert_eq!(channel.upstream_prefix_len(), 32);

        assert!(matches!(
            channel.set_upstream_extranonce_prefix(&[0xdd; 33]),
            Err(StandardChannelError::NewExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), &[0xcc; 32]);
        assert_eq!(channel.upstream_prefix_len(), 32);
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

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
            Some(&StandardJob {
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

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let future_job = NewMiningJob {
            channel_id,
            job_id: 0,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        let flood_size = 10_000u32;
        for job_id in 0..flood_size {
            let mut job = future_job.clone();
            job.job_id = job_id;
            channel.on_new_mining_job(job).unwrap();
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

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let future_job = NewMiningJob {
            channel_id,
            job_id: 0,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        // fill the store with MAX_FUTURE_JOBS distinct job_ids
        for job_id in 0..MAX_FUTURE_JOBS as u32 {
            let mut job = future_job.clone();
            job.job_id = job_id;
            channel.on_new_mining_job(job).unwrap();
        }

        // re-send job_id 0: it should move to the back of the eviction order
        channel.on_new_mining_job(future_job.clone()).unwrap();

        // one more distinct job_id: job_id 1 is now the oldest and gets evicted
        let mut job = future_job.clone();
        job.job_id = MAX_FUTURE_JOBS as u32;
        channel.on_new_mining_job(job).unwrap();

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
    fn test_past_jobs_respect_constructor_override() {
        // Some(cap) must override MAX_PAST_JOBS on the client standard channel's own eviction
        // path, which keeps its past jobs directly rather than delegating to a JobStore.
        let custom_cap = 3usize;
        assert!(custom_cap < MAX_PAST_JOBS);

        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            Some(custom_cap),
        )
        .unwrap();

        let active_job = NewMiningJob {
            channel_id,
            job_id: 0,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(Some(1746839905)),
        };

        let job_count = 20u32;
        for job_id in 0..job_count {
            let mut job = active_job.clone();
            job.job_id = job_id;
            channel.on_new_mining_job(job).unwrap();
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
        let mut zero_cap_channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0; 27]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            Some(0),
        )
        .unwrap();
        for job_id in 0..MAX_PAST_JOBS as u32 + 2 {
            let mut job = active_job.clone();
            job.job_id = job_id;
            zero_cap_channel.on_new_mining_job(job).unwrap();
        }
        assert_eq!(zero_cap_channel.get_past_jobs_count(), MAX_PAST_JOBS);
    }

    #[test]
    fn test_past_jobs_are_bounded() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let active_job = NewMiningJob {
            channel_id,
            job_id: 0,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(Some(1746839905)),
        };

        let flood_size = 10_000u32;
        for job_id in 0..flood_size {
            let mut job = active_job.clone();
            job.job_id = job_id;
            channel.on_new_mining_job(job).unwrap();
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

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(active_job.clone()).unwrap();

        assert_eq!(channel.get_future_jobs_count(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&StandardJob {
                job_message: active_job.clone(),
                extranonce_prefix: extranonce_prefix.clone(),
                target: channel.get_target().clone()
            })
        );
        assert_eq!(channel.get_past_jobs_count(), 0);

        let mut new_active_job = active_job.clone();
        new_active_job.job_id = 2;
        channel.on_new_mining_job(new_active_job.clone()).unwrap();

        assert_eq!(channel.get_future_jobs_count(), 0);
        assert_eq!(
            channel.get_active_job(),
            Some(&StandardJob {
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
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
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
        let share_valid_block = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
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
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        // set min_ntime one second above the share's ntime (1745596932 + 1)
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: 1745596933,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        let share_below_min_ntime = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
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
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
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
        let share_low_diff = SubmitSharesStandardOwned {
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
            ShareValidationError::DoesNotMeetTarget(_)
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
        let target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
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
        let valid_share = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 244405,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_share_validation_invalid_non_rollable_version_bit() {
        // on standard channels, version rolling is always allowed within the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // only the bits inside the mask may differ from the job version
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // the job version is 536870912 (0x20000000)
        // this share has version 0, which clears bit 29, outside the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // no nonce should be accepted
        for nonce in 0..1024u32 {
            let share = SubmitSharesStandardOwned {
                channel_id,
                sequence_number: nonce,
                job_id: future_job.job_id,
                nonce,
                ntime: 1745596932,
                version: 0,
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
    }

    #[test]
    fn test_share_validation_rollable_version_bits() {
        // on standard channels, version rolling is always allowed within the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // shares that only differ in the bits inside the mask are accepted
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let nbits = 545259519;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
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
        // 3a6fcdff7b8c6f8fb41a5851273548ad3561be6418eee05dd71aee5b669f2bf8
        // which does meet the channel target
        let share = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 0,
            ntime: 1745596932,
            version: rolled_version,
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
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        // channel target: 0000ffff00000000000000000000000000000000000000000000000000000000
        let target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0x00, 0x00,
        ]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

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

        channel.on_new_mining_job(future_job.clone()).unwrap();

        // the future job was stored under the old target, now we tighten it to
        // 0000500000000000000000000000000000000000000000000000000000000000
        let new_target = Target::from_le_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x50, 0x00, 0x00,
        ]);
        channel.set_target(new_target).unwrap();

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let nbits = 453040064;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        let ntime: u32 = 1745596930;
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id,
            job_id: future_job.job_id,
            prev_hash: prev_hash.into(),
            nbits,
            min_ntime: ntime,
        };

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // this share has hash 0000762e88282a2ed8e7097aef06f413a962a47e32206a80ecbfc1f0b4bd1493
        // which meets the old target, but not the new one
        let share = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: future_job.job_id,
            nonce: 244405,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = channel.validate_share(share);

        assert!(matches!(
            res,
            Err(ShareValidationError::DoesNotMeetTarget(_))
        ));
    }

    #[test]
    fn test_set_new_prev_hash_retires_active_job() {
        // Regression test: the previously active job used to be silently overwritten by the
        // activated future job, landing in neither past_jobs nor stale_jobs, so a late share
        // for it was rejected as InvalidJobId instead of Stale.
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let merkle_root = [
            189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84, 56,
            86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
        ];

        // job 1 is a non-future job, so it becomes active immediately
        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: 1,
                merkle_root: merkle_root.into(),
                version: 536870912,
                min_ntime: Sv2Option::new(Some(1746839900)),
            })
            .unwrap();

        // job 2 is a future job, waiting for a SetNewPrevHash
        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: 2,
                merkle_root: merkle_root.into(),
                version: 536870912,
                min_ntime: Sv2Option::new(None),
            })
            .unwrap();

        let prev_hash: [u8; 32] = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];

        // a SetNewPrevHash for an unknown job id must leave channel state untouched
        assert!(matches!(
            channel.on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 42,
                prev_hash: prev_hash.into(),
                nbits: 503543726,
                min_ntime: 1746839905,
            }),
            Err(StandardChannelError::JobIdNotFound)
        ));
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert_eq!(channel.get_stale_jobs_count(), 0);

        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: 2,
                prev_hash: prev_hash.into(),
                nbits: 503543726,
                min_ntime: 1746839905,
            })
            .unwrap();

        // job 1 was active under the previous chain tip, so it is now stale
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 2);
        assert_eq!(channel.get_stale_jobs_count(), 1);
        assert!(channel.get_stale_job(1).is_some());
        assert_eq!(channel.get_past_jobs_count(), 0);
    }

    #[test]
    fn test_set_new_prev_hash_keeps_all_past_jobs_in_stale_set() {
        // Regression test: with past jobs at the MAX_PAST_JOBS cap, retiring the displaced
        // active job through the capped past path evicted the oldest past job right before
        // past drained into stale, so its late share was rejected as InvalidJobId instead of
        // Stale. The displaced job must go stale with the whole past set (bounded at
        // MAX_PAST_JOBS + 1).
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let merkle_root = [
            189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84, 56,
            86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
        ];

        // fill past jobs to the cap: jobs 0..=MAX_PAST_JOBS are immediately active, each
        // retiring its predecessor
        for job_id in 0..=MAX_PAST_JOBS as u32 {
            channel
                .on_new_mining_job(NewMiningJob {
                    channel_id,
                    job_id,
                    merkle_root: merkle_root.into(),
                    version: 536870912,
                    min_ntime: Sv2Option::new(Some(1746839900)),
                })
                .unwrap();
        }

        // a future job to activate on the tip transition
        let future_job_id = 100;
        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: future_job_id,
                merkle_root: merkle_root.into(),
                version: 536870912,
                min_ntime: Sv2Option::new(None),
            })
            .unwrap();

        let prev_hash: [u8; 32] = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id,
                job_id: future_job_id,
                prev_hash: prev_hash.into(),
                nbits: 503543726,
                min_ntime: 1746839905,
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
    fn test_share_validation_ntime_below_job_min_ntime() {
        // Regression test: an immediately-active job carries its own min_ntime, which may be
        // later than the chain tip's minimum. A share in the gap
        // (chain_tip.min_ntime <= ntime < job.min_ntime) must be rejected.
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let merkle_root = [
            189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84, 56,
            86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
        ];

        // activate a chain tip at nTime t via a future job
        let tip_ntime: u32 = 1745596930;
        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: 1,
                merkle_root: merkle_root.into(),
                version: 536870912,
                min_ntime: Sv2Option::new(None),
            })
            .unwrap();
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

        // install an immediately-active job whose own min_ntime is later than the tip's
        let job_min_ntime = tip_ntime + 3;
        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: 2,
                merkle_root: merkle_root.into(),
                version: 536870912,
                min_ntime: Sv2Option::new(Some(job_min_ntime)),
            })
            .unwrap();

        // a share in the gap (at or above the tip's min_ntime, below the job's) is rejected
        let share_in_gap = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 0,
            job_id: 2,
            nonce: 3,
            ntime: job_min_ntime - 1,
            version: 536870912,
        };
        let res = channel.validate_share(share_in_gap);
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // the upper bound is anchored at the job's min_ntime as well: one second past it is
        // rejected, exactly on it is accepted
        let share_past_upper_bound = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 2,
            job_id: 2,
            nonce: 3,
            ntime: job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME + 1,
            version: 536870912,
        };
        let res = channel.validate_share(share_past_upper_bound);
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        let share_on_upper_bound = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 3,
            job_id: 2,
            nonce: 3,
            ntime: job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME,
            version: 536870912,
        };
        let res = channel.validate_share(share_on_upper_bound);
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        // at the job's min_ntime the share is accepted (channel target is permissive)
        let share_at_job_min_ntime = SubmitSharesStandardOwned {
            channel_id,
            sequence_number: 1,
            job_id: 2,
            nonce: 3,
            ntime: job_min_ntime,
            version: 536870912,
        };
        let res = channel.validate_share(share_at_job_min_ntime);
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }
    #[test]
    fn test_on_new_group_channel_job_invalid_coinbase() {
        // Regression test for a malicious/malformed upstream coinbase: empty prefix and suffix
        // must produce an error instead of panicking.
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            nominal_hashrate,
            None,
        )
        .unwrap();

        let malformed_job = NewExtendedMiningJob {
            channel_id,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![].try_into().unwrap(),
            coinbase_tx_suffix: vec![].try_into().unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        let res = channel.on_new_group_channel_job(malformed_job);

        assert!(matches!(
            res.unwrap_err(),
            StandardChannelError::InvalidCoinbase
        ));
        // no job must have been stored
        assert_eq!(channel.get_future_jobs_count(), 0);
        assert_eq!(channel.get_active_job(), None);
    }

    #[test]
    fn test_share_validation_ntime_above_max_future_block_time() {
        // Regression test: a share ntime beyond the chain tip's min_ntime +
        // MAX_FUTURE_BLOCK_TIME would put a consensus-invalid timestamp in the block header,
        // so it must be rejected; ntime exactly on the bound is still accepted.
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        channel
            .on_new_mining_job(NewMiningJob {
                channel_id,
                job_id: 1,
                merkle_root: [
                    189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24,
                    84, 56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
                ]
                .into(),
                version: 536870912,
                min_ntime: Sv2Option::new(None),
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

        let share = |sequence_number: u32, ntime: u32| SubmitSharesStandardOwned {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 3,
            ntime,
            version: 536870912,
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
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let job = |min_ntime: Option<u32>| NewMiningJob {
            channel_id,
            job_id: 1,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(min_ntime),
        };

        // job 1 is active under the current tip, and upstream reuses its ID for the next tip's
        // future job
        channel.on_new_mining_job(job(Some(1745596970))).unwrap();
        channel.on_new_mining_job(job(None)).unwrap();

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
        let share = |sequence_number: u32, ntime: u32| SubmitSharesStandardOwned {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 3,
            ntime,
            version: 536870912,
        };
        assert!(matches!(
            channel.validate_share(share(0, 1745596980)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // job 1 goes stale on the next tip transition, and upstream then reuses its ID for an
        // immediately-active job
        let mut future_job = job(None);
        future_job.job_id = 2;
        channel.on_new_mining_job(future_job).unwrap();
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
        channel.on_new_mining_job(job(Some(1745596990))).unwrap();

        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert!(channel.get_stale_job(1).is_none());
        assert!(matches!(
            channel.validate_share(share(1, 1745596990)),
            Ok(ShareValidationResult::Valid(_))
        ));
    }

    fn job_template(job_id: u32, min_ntime: Option<u32>) -> NewMiningJob {
        NewMiningJob {
            channel_id: 1,
            job_id,
            merkle_root: [
                189, 200, 25, 246, 119, 73, 34, 42, 209, 112, 237, 50, 169, 71, 163, 192, 24, 84,
                56, 86, 147, 71, 243, 44, 18, 107, 167, 169, 169, 66, 186, 98,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(min_ntime),
        }
    }

    // Builds a standard channel from a real allocator that only has room for two channels,
    // creates an active job under the first allocated prefix, then rotates the channel onto the
    // second one.
    //
    // Returns the allocator (now with both slots handed out), the channel and the bytes of the
    // rotated-out prefix.
    fn standard_channel_with_rotated_extranonce_prefix(
    ) -> (ExtranonceAllocator, StandardChannel, Vec<u8>) {
        let mut allocator = ExtranonceAllocator::new(vec![], 32, 2).unwrap();
        let prefix_1 = allocator.allocate_standard().unwrap();
        let prefix_2 = allocator.allocate_standard().unwrap();
        assert_ne!(prefix_1.as_bytes(), prefix_2.as_bytes());
        assert_eq!(allocator.allocated_count(), 2);

        let prefix_1_bytes = prefix_1.as_bytes().to_vec();

        let mut channel = StandardChannel::new(
            1,
            "user_identity".to_string(),
            prefix_1.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        // an immediately-active job, created under the first prefix
        channel
            .on_new_mining_job(job_template(1, Some(1745596970)))
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
            let prefix = allocator.allocate_standard().unwrap();
            let old_bytes = prefix.as_bytes().to_vec();

            let mut channel = StandardChannel::new(
                1,
                "user_identity".to_string(),
                prefix.into(),
                Target::from_le_bytes([0xff; 32]),
                1.0,
                Some(1),
            )
            .unwrap();
            let job_id = 1;
            channel
                .on_new_mining_job(job_template(
                    job_id,
                    if future { None } else { Some(1745596970) },
                ))
                .unwrap();

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
                Err(StandardChannelError::NewExtranoncePrefixTooLarge)
            ));
            assert_eq!(channel.get_extranonce_prefix(), current_bytes);
            assert_eq!(channel.retired_extranonce_prefixes.len(), 1);
            assert_eq!(allocator.allocated_count(), 1);

            // A later whole-prefix rotation must not release the slot used by job 1.
            let replacement = allocator.allocate_standard().unwrap();
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
                allocator.allocate_standard(),
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
                    .on_new_mining_job(job_template(job_id, Some(1745596970)))
                    .unwrap();
            }
            assert!(channel.get_past_job(1).is_none());
            assert_eq!(allocator.allocated_count(), 1);
            let reused = allocator.allocate_standard().unwrap();
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
            standard_channel_with_rotated_extranonce_prefix();

        // the rotated-out slot is still reserved, so the allocator is still full
        assert_eq!(allocator.allocated_count(), 2);
        assert!(matches!(
            allocator.allocate_standard(),
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
            standard_channel_with_rotated_extranonce_prefix();

        // a future job (created under the new prefix) to activate on the tip transition
        channel.on_new_mining_job(job_template(2, None)).unwrap();
        channel
            .on_set_new_prev_hash(SetNewPrevHashMp {
                channel_id: 1,
                job_id: 2,
                prev_hash: [
                    200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                    205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
                ]
                .into(),
                nbits: 545259519,
                min_ntime: 1745596980,
            })
            .unwrap();
        assert!(channel.get_stale_job(1).is_some());

        // the pre-rotation job can no longer accept shares, so its prefix was released
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_standard().is_ok());
    }

    #[test]
    fn test_retired_extranonce_prefix_released_after_job_eviction() {
        // Eviction counterpart of the test above: when the last job created under a
        // rotated-out prefix is evicted from past jobs, the prefix's slot must be released
        // right away — an upstream withholding the next chain transition must not be able to
        // pin allocator slots.
        let (mut allocator, mut channel, _prefix_1_bytes) =
            standard_channel_with_rotated_extranonce_prefix();

        // flood enough immediately-active jobs to push the pre-rotation job out of past jobs
        for job_id in 2..2 + MAX_PAST_JOBS as u32 + 2 {
            channel
                .on_new_mining_job(job_template(job_id, Some(1745596970)))
                .unwrap();
        }
        assert!(channel.get_past_job(1).is_none());

        // the evicted job was the last reference to the rotated-out prefix, so its slot is free
        // again
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_standard().is_ok());
    }

    #[test]
    fn test_retired_extranonce_prefix_survives_install_of_a_job_under_its_bytes() {
        // Retiring the displaced job may evict a past job and prune retired extranonce
        // prefixes. The job being installed is a live user of the current prefix bytes, so it
        // must be in place when that prune runs: a retired prefix from a recreated allocator can
        // share those bytes, and pruning before the install would release its slot while the
        // new job goes on to accept shares under them.
        let mut allocator_1 = ExtranonceAllocator::new(vec![], 32, 1).unwrap();
        let prefix_1 = allocator_1.allocate_standard().unwrap();
        let mut allocator_2 = ExtranonceAllocator::new(vec![], 32, 1).unwrap();
        let prefix_2 = allocator_2.allocate_standard().unwrap();
        assert_eq!(prefix_1.as_bytes(), prefix_2.as_bytes());
        let prefix_len = prefix_1.as_bytes().len();

        let mut channel = StandardChannel::new(
            1,
            "user_identity".to_string(),
            prefix_1.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            Some(1),
        )
        .unwrap();

        // job 1 under the first prefix, then a rotation onto a wire prefix and job 2 under it
        channel
            .on_new_mining_job(job_template(1, Some(1745596970)))
            .unwrap();
        channel
            .set_extranonce_prefix(ExtranoncePrefix::from_wire(vec![7; prefix_len]).unwrap())
            .unwrap();
        channel
            .on_new_mining_job(job_template(2, Some(1745596970)))
            .unwrap();
        assert_eq!(allocator_1.allocated_count(), 1);

        // rotate onto the second allocator's byte-identical prefix; installing job 3 under it
        // retires job 2 and evicts job 1, the last job created under the first prefix
        channel.set_extranonce_prefix(prefix_2.into()).unwrap();
        channel
            .on_new_mining_job(job_template(3, Some(1745596970)))
            .unwrap();
        assert!(channel.get_past_job(1).is_none());

        // job 3 lives under those bytes, so the first allocator's slot stays reserved
        assert_eq!(allocator_1.allocated_count(), 1);
        assert!(matches!(
            allocator_1.allocate_standard(),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));
    }

    #[test]
    fn test_zero_target_is_rejected() {
        // a zero target is refused at construction and on SetTarget, leaving the channel
        // unchanged (see InvalidTarget)
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let res = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            Target::ZERO,
            1.0,
            None,
        );
        assert!(matches!(res, Err(StandardChannelError::InvalidTarget)));

        let target = Target::from_le_bytes([0xff; 32]);
        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            target,
            1.0,
            None,
        )
        .unwrap();

        // a queued future job, whose target a SetTarget would otherwise refresh
        channel.on_new_mining_job(job_template(1, None)).unwrap();

        assert!(matches!(
            channel.set_target(Target::ZERO),
            Err(StandardChannelError::InvalidTarget)
        ));
        assert_eq!(channel.get_target(), &target);
        assert_eq!(channel.get_future_job(1).unwrap().target, target);
    }

    #[test]
    fn test_immediately_active_job_below_chain_tip_min_ntime_is_rejected() {
        // An immediately-active job is mined against the current chain tip, whose min_ntime is
        // the smallest nTime available for it, so a job with a lower min_ntime must be refused
        // and leave the channel unchanged, whether it arrives as NewMiningJob or is derived from
        // a group channel's NewExtendedMiningJob. A min_ntime equal to the tip's is the lowest
        // allowed.
        let channel_id = 1;
        // a 32-byte prefix: the group job's coinbase below carries a 32-byte extranonce
        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0; 32]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            None,
        )
        .unwrap();

        let tip_ntime: u32 = 1745596970;
        channel.on_new_mining_job(job_template(1, None)).unwrap();
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

        assert!(matches!(
            channel.on_new_mining_job(job_template(2, Some(tip_ntime - 1))),
            Err(StandardChannelError::JobMinNtimeBelowChainTip)
        ));
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);
        assert_eq!(channel.get_past_jobs_count(), 0);

        let group_job = NewExtendedMiningJob {
            channel_id,
            job_id: 2,
            min_ntime: Sv2Option::new(Some(tip_ntime - 1)),
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
        assert!(matches!(
            channel.on_new_group_channel_job(group_job),
            Err(StandardChannelError::JobMinNtimeBelowChainTip)
        ));
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 1);

        channel
            .on_new_mining_job(job_template(3, Some(tip_ntime)))
            .unwrap();
        assert_eq!(channel.get_active_job().unwrap().job_message.job_id, 3);
    }

    #[test]
    fn test_set_chain_tip_replacement_retires_the_jobs_of_the_previous_tip() {
        // Shares are hashed against the channel's current tip, so a job built under a tip that
        // set_chain_tip replaces must go stale, as it does on the message-driven transitions:
        // otherwise a late share for it would be validated against a header the miner was never
        // assigned. Re-setting the current tip changes nothing.
        let channel_id = 1;
        let mut channel = StandardChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0, 0, 0, 1]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
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
            .on_new_mining_job(job_template(1, Some(1745596970)))
            .unwrap();

        let share = |sequence_number: u32, nonce: u32| SubmitSharesStandardOwned {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce,
            ntime: 1745596970,
            version: 536870912,
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
