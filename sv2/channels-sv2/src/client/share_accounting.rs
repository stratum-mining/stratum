//! Share Validation - Mining Client Abstraction.
//!
//! This module provides types and logic for validating mining shares, tracking share
//! statistics, and reporting share validation results and errors. These abstractions
//! are intended for use in Mining Clients.

extern crate alloc;
use super::{HashMap, HashSet};
use alloc::string::String;
use bitcoin::hashes::sha256d::Hash;

/// The outcome of share validation, as seen by a Mining Client.
///
/// - `Valid`: The share is valid and accepted.
/// - `BlockFound`: The submitted share resulted in a new block being found.
#[derive(Debug)]
pub enum ShareValidationResult {
    Valid(Hash),
    BlockFound(Hash),
}

/// Possible errors encountered during share validation.
///
/// Variants carrying `&'static str` are intended to be used as `error_code` values in
/// [`SubmitSharesError`](mining_sv2::SubmitSharesError).
///
/// Variants without `&'static str` SHOULD lead to a client disconnection or application
/// shutdown.
///
/// - `Invalid`: The share is malformed or not valid.
/// - `Stale`: The share refers to an outdated job or block tip.
/// - `InvalidJobId`: The job ID referenced by the share is not recognized.
/// - `DoesNotMeetTarget`: The share does not meet the required target difficulty.
/// - `VersionRollingNotAllowed`: Version rolling is not permitted for this channel/job.
/// - `DuplicateShare`: The share has already been submitted (detected by hash).
/// - `BadExtranonceSize`: The share extranonce size is different from the channel's rollable
///   extranonce size.
/// - `NoChainTip`: The chain tip is unknown or unavailable.
#[derive(Debug)]
pub enum ShareValidationError {
    Invalid(&'static str),
    Stale(&'static str),
    InvalidJobId(&'static str),
    DoesNotMeetTarget(&'static str),
    VersionRollingNotAllowed(&'static str),
    DuplicateShare(&'static str),
    BadExtranonceSize(&'static str),
    NoChainTip,
}

/// Tracks share validation and acceptance state for a specific channel (Extended or Standard).
///
/// Used only on Mining Clients. Share accounting is split into two phases:
///
/// **Validation phase** (updated by [`validate_share`] via [`track_validated_share`]):
/// - total validated shares (shares that passed local validation)
/// - cumulative validated work (based on each job's target difficulty)
/// - hashes of seen shares (for duplicate detection)
/// - last received share's sequence number
/// - highest difficulty seen in validated shares
///
/// **Acceptance phase** (updated by the application layer via [`on_share_acknowledgement`]):
/// - total acknowledged shares (confirmed by upstream [`SubmitSharesSuccess`])
/// - total rejected shares (reported by upstream [`SubmitSharesError`])
/// - cumulative acknowledged work (as reported by upstream [`SubmitSharesSuccess`])
/// - number of blocks found
///
/// [`validate_share`]: super::extended::ExtendedChannel::validate_share
/// [`track_validated_share`]: ShareAccounting::track_validated_share
/// [`on_share_acknowledgement`]: ShareAccounting::on_share_acknowledgement
/// [`SubmitSharesSuccess`]: mining_sv2::SubmitSharesSuccess
/// [`SubmitSharesError`]: mining_sv2::SubmitSharesError
#[derive(Clone, Debug)]
pub struct ShareAccounting {
    last_share_sequence_number: u32,
    acknowledged_shares: u32,
    acknowledged_work_sum: u64,
    validated_shares: u32,
    validated_work_sum: f64,
    rejected_shares: HashMap<String, u32>, // <error_code, count>
    seen_shares: HashSet<Hash>,
    best_diff: f64,
    blocks_found: u32,
}

impl Default for ShareAccounting {
    fn default() -> Self {
        Self::new()
    }
}

impl ShareAccounting {
    /// Creates a new [`ShareAccounting`] instance, initializing all statistics to zero.
    pub fn new() -> Self {
        Self {
            last_share_sequence_number: 0,
            acknowledged_shares: 0,
            acknowledged_work_sum: 0,
            validated_shares: 0,
            validated_work_sum: 0.0,

            rejected_shares: HashMap::new(),
            seen_shares: HashSet::new(),
            best_diff: 0.0,
            blocks_found: 0,
        }
    }

    /// Updates acceptance accounting based on a [`SubmitSharesSuccess`] message from the
    /// upstream server.
    ///
    /// This should be called by the application layer when it receives upstream confirmation
    /// that shares were accepted. It is intentionally **not** called from [`validate_share`] —
    /// local validation only tracks the share for duplicate detection (via
    /// [`track_validated_share`]).
    pub fn on_share_acknowledgement(
        &mut self,
        new_submits_accepted_count: u32,
        new_shares_sum: u64,
    ) {
        self.acknowledged_shares += new_submits_accepted_count;
        self.acknowledged_work_sum = self.acknowledged_work_sum.saturating_add(new_shares_sum);
    }

    /// Updates rejection accounting based on a [`SubmitSharesError`] message from the upstream
    /// server.
    ///
    /// One call corresponds to one rejected share.
    pub fn on_share_rejection(&mut self, error_code: String) {
        self.rejected_shares
            .entry(error_code)
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }

    /// Records a share that passed local validation.
    ///
    /// Adds the hash to the seen set for duplicate detection and updates the last sequence
    /// number. Called from [`validate_share`] — does **not** count the share as accepted.
    /// Acceptance accounting is deferred to [`on_share_acknowledgement`], which should be
    /// called when the upstream server confirms via [`SubmitSharesSuccess`].
    pub fn track_validated_share(
        &mut self,
        share_sequence_number: u32,
        share_hash: Hash,
        share_work: f64,
    ) {
        self.last_share_sequence_number = share_sequence_number;
        self.validated_shares += 1;
        self.validated_work_sum += share_work;
        self.seen_shares.insert(share_hash);
    }

    /// Clears the set of seen share hashes.
    ///
    /// Should be called on every chain tip update
    /// to prevent unbounded memory growth.
    pub fn flush_seen_shares(&mut self) {
        self.seen_shares.clear();
    }

    /// Returns the sequence number of the last share received.
    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    /// Returns the total number of shares acknowledged by upstream.
    pub fn get_acknowledged_shares(&self) -> u32 {
        self.acknowledged_shares
    }

    /// Returns the total number of locally validated shares.
    pub fn get_validated_shares(&self) -> u32 {
        self.validated_shares
    }

    /// Returns a reference to the map of rejected shares by error code.
    pub fn get_rejected_shares(&self) -> &HashMap<String, u32> {
        &self.rejected_shares
    }

    /// Returns the cumulative work acknowledged by upstream via `SubmitSharesSuccess`.
    pub fn get_acknowledged_work_sum(&self) -> u64 {
        self.acknowledged_work_sum
    }

    /// Returns the cumulative work of all locally validated shares.
    ///
    /// Work is tracked using job-target difficulty (matching server-side accounting),
    /// not per-share hash difficulty.
    pub fn get_validated_work_sum(&self) -> f64 {
        self.validated_work_sum
    }

    /// Checks if the given share hash has already been seen (duplicate detection).
    pub fn is_share_seen(&self, share_hash: Hash) -> bool {
        self.seen_shares.contains(&share_hash)
    }

    /// Returns the highest difficulty among all accepted shares.
    pub fn get_best_diff(&self) -> f64 {
        self.best_diff
    }

    /// Updates the best difficulty if the new difficulty is higher than the current best.
    pub fn update_best_diff(&mut self, diff: f64) {
        if diff > self.best_diff {
            self.best_diff = diff;
        }
    }

    /// Increments the blocks found counter.
    pub fn increment_blocks_found(&mut self) {
        self.blocks_found += 1;
    }

    /// Returns the total number of blocks found on this channel.
    pub fn get_blocks_found(&self) -> u32 {
        self.blocks_found
    }
}
