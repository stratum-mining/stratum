//! Abstractions for share validation for a Mining Client

use bitcoin::hashes::sha256d::Hash;
use std::collections::HashSet;

/// The outcome of share validation, from the perspective of a Mining Client.
#[derive(Debug)]
pub enum ShareValidationResult {
    Valid,
    BlockFound,
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
    NoChainTip,
}

/// The state of share validation on the context of some specific channel (either Extended or
/// Standard)
///
/// Only meant for usage on Mining Clients.
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
    pub fn new() -> Self {
        Self {
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0,
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
