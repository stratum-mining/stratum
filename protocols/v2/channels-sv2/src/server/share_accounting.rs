//! Abstractions for share validation for a Mining Server

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
    Valid,
    // last_sequence_number, new_submits_accepted_count, new_shares_sum
    ValidWithAcknowledgement(u32, u32, u64),
    // template_id, coinbase
    // template_id is None if custom job
    BlockFound(Option<u64>, Vec<u8>),
}

/// The error variants that can occur during share validation
#[derive(Debug)]
pub enum ShareValidationError {
    Invalid,
    Stale,
    InvalidJobId,
    DoesNotMeetTarget,
    VersionRollingNotAllowed,
    DuplicateShare,
    InvalidCoinbase,
    NoChainTip,
}

/// The state of share validation on the context of some specific channel (either Extended or
/// Standard)
///
/// Only meant for usage on Mining Servers.
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

    /// clears the hashset of seen shares
    ///
    /// should be called on every chain tip update
    /// to avoid unbounded growth of memory
    pub fn flush_seen_shares(&mut self) {
        self.seen_shares.clear();
    }

    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    pub fn get_shares_accepted(&self) -> u32 {
        self.shares_accepted
    }

    pub fn get_share_work_sum(&self) -> u64 {
        self.share_work_sum
    }

    pub fn get_share_batch_size(&self) -> usize {
        self.share_batch_size
    }

    pub fn should_acknowledge(&self) -> bool {
        self.shares_accepted % self.share_batch_size as u32 == 0
    }

    /// Checks if the share has been seen.
    /// Useful to avoid duplicate shares.
    pub fn is_share_seen(&self, share_hash: Hash) -> bool {
        self.seen_shares.contains(&share_hash)
    }

    pub fn get_best_diff(&self) -> f64 {
        self.best_diff
    }

    /// Updates the best diff if the new diff is higher.
    pub fn update_best_diff(&mut self, diff: f64) {
        if diff > self.best_diff {
            self.best_diff = diff;
        }
    }
}
