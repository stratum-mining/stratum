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
    Valid,
    BlockFound,
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
#[derive(Debug)]
pub enum ShareValidationError {
    Invalid,
    Stale,
    InvalidJobId,
    DoesNotMeetTarget,
    VersionRollingNotAllowed,
    DuplicateShare,
    NoChainTip,
}

/// Tracks share validation state for a specific channel (Extended or Standard).
///
/// Used only on Mining Clients.
/// Keeps statistics and state for shares submitted through the channel:
/// - last received share's sequence number
/// - total accepted shares
/// - cumulative work from accepted shares
/// - hashes of seen shares (for duplicate detection)
/// - highest difficulty seen in accepted shares
#[derive(Clone, Debug)]
pub struct ShareAccounting {
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: u64,
    seen_shares: HashSet<Hash>,
    best_diff: f64,
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
            shares_accepted: 0,
            share_work_sum: 0,
            seen_shares: HashSet::new(),
            best_diff: 0.0,
        }
    }

    /// Updates the accounting state with a newly accepted share.
    ///
    /// - Increments share count and total work.
    /// - Updates last share sequence number.
    /// - Records share hash to detect duplicates.
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
    /// Should be called on every chain tip update
    /// to prevent unbounded memory growth.
    pub fn flush_seen_shares(&mut self) {
        self.seen_shares.clear();
    }

    /// Returns the sequence number of the last share received.
    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    /// Returns the total number of shares accepted.
    pub fn get_shares_accepted(&self) -> u32 {
        self.shares_accepted
    }

    /// Returns the cumulative work of all accepted shares.
    pub fn get_share_work_sum(&self) -> u64 {
        self.share_work_sum
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
}
