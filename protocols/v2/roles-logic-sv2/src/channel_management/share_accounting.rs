//! Abstractions for share validation

use mining_sv2::Target;
use std::collections::HashSet;
use stratum_common::bitcoin::hashes::sha256d::Hash;

/// The outcome of share validation.
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
pub enum ShareValidationError {
    Invalid,
    Stale,
    InvalidJobId,
    DoesNotMeetTarget,
    VersionRollingNotAllowed,
    DuplicateShare,
    InvalidCoinbase,
}

/// The state of share validation on the context of some specific channel (either Extended or
/// Standard)
#[derive(Clone, Debug)]
pub struct ShareAccounting {
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: f64,
    share_batch_size: usize,
    seen_shares: HashSet<Hash>,
    best_target: Option<Target>,
}

impl ShareAccounting {
    pub fn new(share_batch_size: usize) -> Self {
        Self {
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0.0,
            share_batch_size,
            seen_shares: HashSet::new(),
            best_target: None,
        }
    }

    pub fn update_share_accounting(
        &mut self,
        share_work: f64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) {
        self.last_share_sequence_number = share_sequence_number;
        self.shares_accepted += 1;
        self.share_work_sum += share_work;
        self.seen_shares.insert(share_hash);
    }

    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    pub fn get_shares_accepted(&self) -> u32 {
        self.shares_accepted
    }

    pub fn get_share_work_sum(&self) -> f64 {
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

    pub fn get_best_target(&self) -> Option<Target> {
        self.best_target.clone()
    }

    /// Updates the best target if the new target is lower.
    pub fn update_best_target(&mut self, target: Target) {
        if self.best_target.is_none()
            || target < self.best_target.clone().expect("best target must exist")
        {
            self.best_target = Some(target);
        }
    }
}
