//! Share Validation - Mining Server Abstraction.
//!
//! This module provides types and logic for validating mining shares and tracking share accounting
//! state on a mining server. It is used to determine the outcome of submitted shares, track
//! duplicate submissions, maintain batch acknowledgment state, and compute share statistics for
//! downstream Stratum V2 (SV2) messaging.
//!
//! ## Responsibilities
//!
//! - **Share Validation Result**: Encapsulates the result of validating a mining share, including
//!   success, batch acknowledgment, and block discovery.
//! - **Share Validation Error**: Enumerates possible failure reasons when validating a share.
//! - **Share Accounting**: Tracks per-channel share statistics, acknowledges batches, detects
//!   duplicate shares, and maintains best difficulty found.
//!
//! ## Usage
//!
//! Intended for use within mining server implementations that process SV2 share submissions and
//! issue `SubmitShares.Success` messages. Not intended for use by mining clients.

use bitcoin::hashes::sha256d::Hash;
use std::collections::HashSet;

/// The outcome of share validation, from the perspective of a Mining Server.
///
/// The [`ShareValidationResult::ValidWithAcknowledgement`] variant carries:
/// - `last_sequence_number` (as `u32`)
/// - `new_submits_accepted_count` (as `u32`)
/// - `new_shares_sum` (as `u64`)
///
/// which are used to craft `SubmitShares.Success` Sv2 messages.
///
/// The [`ShareValidationResult::BlockFound`] variant carries:
/// - `template_id` (as `Option<u64>`)
/// - `coinbase` (as `Vec<u8>`)
///
/// where `template_id` is `None` if the share is for a custom job.
#[derive(Debug)]
pub enum ShareValidationResult {
    /// The share is valid and accepted.
    Valid,
    /// The share is valid and triggers a batch acknowledgment.
    /// Contains:
    /// - `last_sequence_number`: The sequence number of the last accepted share in the batch.
    /// - `new_submits_accepted_count`: The number of new shares accepted in this batch.
    /// - `new_shares_sum`: The total work contributed by shares in this batch.
    ValidWithAcknowledgement(u32, u32, u64),
    /// The share solves a block.
    /// Contains:
    /// - `template_id`: The template ID associated with the job, or `None` for custom jobs.
    /// - `coinbase`: The serialized coinbase transaction for the block.
    BlockFound(Option<u64>, Vec<u8>),
}

/// The error variants that can occur during share validation.
#[derive(Debug)]
pub enum ShareValidationError {
    /// The share is invalid for unspecified reasons.
    Invalid,
    /// The share is stale due to chain tip changes.
    Stale,
    /// The submitted job ID does not refer to any known job for this channel.
    InvalidJobId,
    /// The share does not meet the required target difficulty.
    DoesNotMeetTarget,
    /// The submitted share attempts version rolling when not allowed.
    VersionRollingNotAllowed,
    /// The share is a duplicate of a previously accepted share.
    DuplicateShare,
    /// The coinbase transaction was invalid or malformed.
    InvalidCoinbase,
    /// No chain tip is set for the channel (required for share validation).
    NoChainTip,
}

/// The state of share validation in the context of some specific channel (either Extended or
/// Standard).
///
/// This struct manages per-channel share statistics, batch acknowledgment, duplicate detection,
/// and difficulty tracking. Only meant for usage on Mining Servers.
#[derive(Clone, Debug)]
pub struct ShareAccounting {
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: u64,
    share_batch_size: usize,
    seen_shares: HashSet<Hash>,
    best_diff: f64,
}

impl ShareAccounting {
    /// Constructs a new `ShareAccounting` instance for a channel.
    ///
    /// `share_batch_size` controls how many accepted shares trigger a batch acknowledgment.
    pub fn new(share_batch_size: usize) -> Self {
        Self {
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0,
            share_batch_size,
            seen_shares: HashSet::new(),
            best_diff: 0.0,
        }
    }

    /// Updates internal accounting for a newly accepted share.
    ///
    /// - Increments total shares accepted and work sum.
    /// - Updates last accepted sequence number.
    /// - Records the share hash to detect duplicates.
    pub fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) {
        self.last_share_sequence_number = share_sequence_number;
        self.shares_accepted += 1;
        self.share_work_sum += share_work;
        self.seen_shares.insert(share_hash);
    }

    /// Clears the set of seen share hashes.
    ///
    /// Should be called on every chain tip update to avoid unbounded growth of memory
    /// and allow new shares for the new tip.
    pub fn flush_seen_shares(&mut self) {
        self.seen_shares.clear();
    }

    /// Returns the sequence number of the last accepted share.
    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    /// Returns the total number of shares accepted on this channel.
    pub fn get_shares_accepted(&self) -> u32 {
        self.shares_accepted
    }

    /// Returns the sum of work contributed by all accepted shares.
    pub fn get_share_work_sum(&self) -> u64 {
        self.share_work_sum
    }

    /// Returns the configured batch size for share acknowledgments.
    pub fn get_share_batch_size(&self) -> usize {
        self.share_batch_size
    }

    /// Returns true if the current count of accepted shares triggers an acknowledgment.
    pub fn should_acknowledge(&self) -> bool {
        self.shares_accepted % self.share_batch_size as u32 == 0
    }

    /// Checks if the share hash has already been accepted (duplicate detection).
    pub fn is_share_seen(&self, share_hash: Hash) -> bool {
        self.seen_shares.contains(&share_hash)
    }

    /// Returns the highest difficulty found among accepted shares.
    pub fn get_best_diff(&self) -> f64 {
        self.best_diff
    }

    /// Updates the best difficulty if the new value is higher.
    pub fn update_best_diff(&mut self, diff: f64) {
        if diff > self.best_diff {
            self.best_diff = diff;
        }
    }
}
