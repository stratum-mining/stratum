//! Share Validation - Mining Client Abstraction.
//!
//! This module provides types and logic for validating mining shares, tracking share
//! statistics, and reporting share validation results and errors. These abstractions
//! are intended for use in Mining Clients.

use super::HashSet;
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
/// - `Invalid`: The share is malformed or not valid.
/// - `Stale`: The share refers to an outdated job or block tip.
/// - `InvalidJobId`: The job ID referenced by the share is not recognized.
/// - `DoesNotMeetTarget`: The share does not meet the required target difficulty.
/// - `VersionRollingNotAllowed`: Version rolling is not permitted for this channel/job.
/// - `DuplicateShare`: The share has already been submitted (detected by hash).
/// - `NoChainTip`: The chain tip is unknown or unavailable.
/// - `BadExtranonceSize`: The share extranonce size is different from the channel's rollable
///   extranonce size.
#[derive(Debug)]
pub enum ShareValidationError {
    Invalid,
    Stale,
    InvalidJobId,
    DoesNotMeetTarget,
    VersionRollingNotAllowed,
    DuplicateShare,
    NoChainTip,
    BadExtranonceSize,
}

/// Tracks share validation and acceptance state for a specific channel (Extended or Standard).
///
/// Used only on Mining Clients. Share accounting is split into two phases:
///
/// **Validation phase** (updated by [`validate_share`] via [`track_validated_share`]):
/// - total validated shares (shares that passed local validation)
/// - hashes of seen shares (for duplicate detection)
/// - last received share's sequence number
/// - highest difficulty seen in validated shares
///
/// **Acceptance phase** (updated by the application layer via [`on_share_acknowledgement`]):
/// - total acknowledged shares (confirmed by upstream [`SubmitSharesSuccess`])
/// - total rejected shares (reported by upstream [`SubmitSharesError`])
/// - cumulative work from accepted shares
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
    validated_shares: u32,
    rejected_shares: u32,
    share_work_sum: u64,
    seen_shares: HashSet<Hash>,
    best_diff: u64,
    blocks_found: u32,
    last_batch_accepted: u32,
    last_batch_work_sum: u64,
    share_batch_size: u32,
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
            validated_shares: 0,
            rejected_shares: 0,
            share_work_sum: 0,
            seen_shares: HashSet::new(),
            best_diff: 0,
            blocks_found: 0,
            last_batch_accepted: 0,
            last_batch_work_sum: 0,
            share_batch_size: 0,
        }
    }

    /// Updates acceptance accounting based on a `SubmitSharesSuccess` message from the
    /// upstream server.
    ///
    /// This should be called by the application layer when it receives upstream confirmation
    /// that shares were accepted. It is intentionally **not** called from `validate_share` —
    /// local validation only tracks the share for duplicate detection (via
    /// `track_validated_share`).
    pub fn on_share_acknowledgement(
        &mut self,
        new_submits_accepted_count: u32,
        new_shares_sum: u64,
    ) {
        self.acknowledged_shares += new_submits_accepted_count;
        self.share_work_sum += new_shares_sum;
        self.last_batch_accepted = new_submits_accepted_count;
        self.last_batch_work_sum = new_shares_sum;
    }

    /// Updates rejection accounting based on a `SubmitSharesError` message from the upstream
    /// server.
    ///
    /// One call corresponds to one rejected share.
    pub fn on_share_rejection(&mut self) {
        self.rejected_shares += 1;
    }

    /// Records a share that passed local validation.
    ///
    /// Adds the hash to the seen set for duplicate detection and updates the last sequence
    /// number. Called from `validate_share` — does **not** count the share as accepted.
    /// Acceptance accounting is deferred to [`Self::on_share_acknowledgement`], which should be
    /// called when the upstream server confirms via `SubmitSharesSuccess`.
    pub fn track_validated_share(&mut self, share_sequence_number: u32, share_hash: Hash) {
        self.last_share_sequence_number = share_sequence_number;
        self.validated_shares += 1;
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

    /// Alias for [`Self::get_acknowledged_shares`].
    pub fn get_shares_accepted(&self) -> u32 {
        self.acknowledged_shares
    }

    /// Returns the total number of shares rejected by upstream.
    pub fn get_rejected_shares(&self) -> u32 {
        self.rejected_shares
    }

    /// Returns the cumulative work of all accepted shares.
    pub fn get_share_work_sum(&self) -> u64 {
        self.share_work_sum
    }

    /// Returns the total work (alias for share_work_sum).
    pub fn get_total_work(&self) -> u64 {
        self.share_work_sum
    }

    /// Checks if the given share hash has already been seen (duplicate detection).
    pub fn is_share_seen(&self, share_hash: Hash) -> bool {
        self.seen_shares.contains(&share_hash)
    }

    /// Returns the highest difficulty among all accepted shares.
    pub fn get_best_diff(&self) -> u64 {
        self.best_diff
    }

    /// Updates the best difficulty if the new difficulty is higher than the current best.
    pub fn update_best_diff(&mut self, diff: u64) {
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

    /// Returns the number of shares accepted in the last batch.
    pub fn get_last_batch_accepted(&self) -> u32 {
        self.last_batch_accepted
    }

    /// Returns the cumulative work of the last batch of accepted shares.
    pub fn get_last_batch_work_sum(&self) -> u64 {
        self.last_batch_work_sum
    }

    /// Returns the configured share batch size.
    pub fn get_share_batch_size(&self) -> u32 {
        self.share_batch_size
    }

    /// Sets the share batch size.
    pub fn set_share_batch_size(&mut self, size: u32) {
        self.share_batch_size = size;
    }

    /// Record a rejected share.
    pub fn increment_shares_rejected(&mut self) {
        self.rejected_shares += 1;
    }
}
