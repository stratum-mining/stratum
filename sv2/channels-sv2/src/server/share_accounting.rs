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
//!   duplicate shares, tracks rejected shares, and maintains best difficulty found.
//!
//! ## Usage
//!
//! Intended for use within mining server implementations that process SV2 share submissions and
//! issue `SubmitShares.Success` or `SubmitShares.Error` messages. Not intended for use by mining
//! clients.

use bitcoin::hashes::sha256d::Hash;
use std::collections::{HashMap, HashSet};

/// The outcome of share validation, from the perspective of a Mining Server.
///
/// The [`ShareValidationResult::Valid`] variant carries the hash of the accepted share.
///
/// The [`ShareValidationResult::BlockFound`] variant carries:
/// - `share_hash`: The hash of the share that solved the block.
/// - `template_id`: The template ID associated with the job (as `Option<u64>`), or `None` for custom and prebuilt jobs.
/// - `coinbase`: The serialized coinbase transaction for the block (as `Vec<u8>`). For a prebuilt
///   job this is `coinbase_tx_prefix || extranonce || coinbase_tx_suffix` as handed over, without
///   the BIP141 data a SegWit block needs; see
///   [`PrebuiltJob`](crate::server::jobs::PrebuiltJob).
#[derive(Debug)]
pub enum ShareValidationResult {
    /// The share is valid and accepted.
    Valid(Hash),
    /// The share solves a block.
    /// Contains:
    /// - `share_hash`: The hash of the share that solved the block.
    /// - `template_id`: The template ID associated with the job, or `None` for custom and prebuilt jobs.
    /// - `coinbase`: The serialized coinbase transaction for the block. For a prebuilt job this is
    ///   `coinbase_tx_prefix || extranonce || coinbase_tx_suffix` as handed over, without the
    ///   BIP141 data a SegWit block needs; see [`PrebuiltJob`](crate::server::jobs::PrebuiltJob).
    BlockFound(Hash, Option<u64>, Vec<u8>),
}

/// The error variants that can occur during share validation.
///
/// Variants carrying `&'static str` are intended to be used as `error_code` values in
/// [`SubmitSharesError`](mining_sv2::SubmitSharesError).
///
/// Variants without `&'static str` SHOULD lead to a client disconnection or application
/// shutdown.
#[derive(Debug)]
pub enum ShareValidationError {
    /// The share is invalid for unspecified reasons.
    Invalid(&'static str),
    /// The share is stale due to chain tip changes.
    Stale(&'static str),
    /// The submitted job ID does not refer to any known job for this channel.
    InvalidJobId(&'static str),
    /// The share does not meet the required target difficulty.
    DoesNotMeetTarget(&'static str),
    /// The submitted share attempts version rolling when not allowed.
    VersionRollingNotAllowed(&'static str),
    /// The share is a duplicate of a previously accepted share.
    DuplicateShare(&'static str),
    /// The share extranonce size is different from the channel's rollable extranonce size.
    BadExtranonceSize(&'static str),
    /// No chain tip is set for the channel (required for share validation).
    NoChainTip,
    /// The accepted-share dedup cache is full (see [`crate::seen_shares_budget`]).
    ///
    /// Forgetting a still-valid hash would re-enable duplicate-share replay (double-counted
    /// shares are payout fraud), so on the server side the budget is a hard limit rather than an
    /// eviction threshold. Reaching it means the share rate has far outrun the channel's
    /// configured expectation for an entire worst-case tip (see [`crate::seen_shares_budget`],
    /// including its block-cadence assumption), so the embedding application should close the
    /// channel.
    SeenSharesBudgetExhausted,
}

/// The state of share validation in the context of some specific channel (either Extended or
/// Standard).
///
/// This struct manages per-channel share statistics, batch acknowledgment, duplicate detection,
/// rejected-share accounting, and difficulty tracking. Only meant for usage on Mining Servers.
#[derive(Clone, Debug)]
pub struct ShareAccounting {
    last_share_sequence_number: u32,
    shares_accepted: u32,
    rejected_shares: HashMap<String, u32>,
    share_work_sum: f64,
    last_batch_accepted: u32,
    last_batch_work_sum: f64,
    batch_acknowledged: bool,
    share_batch_size: usize,
    seen_shares: HashSet<Hash>,
    seen_shares_budget: usize,
    best_diff: f64,
    blocks_found: u32,
}

impl ShareAccounting {
    /// Constructs a new `ShareAccounting` instance for a channel.
    ///
    /// `share_batch_size` controls how many accepted shares trigger a batch acknowledgment.
    ///
    /// `seen_shares_budget` bounds the accepted-share dedup cache; channels derive it from their
    /// expected share rate via [`crate::seen_shares_budget`]. Once
    /// [`is_seen_shares_budget_exhausted`](Self::is_seen_shares_budget_exhausted) reports `true`,
    /// share validation fails with [`ShareValidationError::SeenSharesBudgetExhausted`].
    pub fn new(share_batch_size: usize, seen_shares_budget: usize) -> Self {
        Self {
            last_share_sequence_number: 0,
            shares_accepted: 0,
            rejected_shares: HashMap::new(),
            share_work_sum: 0.0,
            last_batch_accepted: 0,
            last_batch_work_sum: 0.0,
            batch_acknowledged: false,
            share_batch_size,
            seen_shares: HashSet::new(),
            seen_shares_budget,
            best_diff: 0.0,
            blocks_found: 0,
        }
    }

    /// Increments rejected-share accounting for a share-validation `error_code`.
    ///
    /// Intended to be called by channel validation paths when returning a
    /// [`ShareValidationError`] variant that carries an `error_code`.
    /// Validation errors that do not map to `SubmitShares.Error` should not be counted here.
    ///
    /// Saturates at `u32::MAX`.
    pub fn increment_rejected_shares(&mut self, error_code: &str) {
        if let Some(count) = self.rejected_shares.get_mut(error_code) {
            *count = count.saturating_add(1);
        } else {
            self.rejected_shares.insert(error_code.to_string(), 1);
        }
    }

    /// Updates internal accounting for a newly accepted share.
    ///
    /// - Increments total shares accepted (saturates at `u32::MAX`) and work sum.
    /// - Increments last batch accepted (saturates at `u32::MAX`) and work sum,
    ///   resetting when a new batch starts.
    /// - Updates last accepted sequence number.
    /// - Records the share hash to detect duplicates.
    pub fn update_share_accounting(
        &mut self,
        share_work: f64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) {
        self.last_share_sequence_number = share_sequence_number;
        self.shares_accepted = self.shares_accepted.saturating_add(1);
        self.share_work_sum += share_work;
        self.seen_shares.insert(share_hash);

        if self.batch_acknowledged || self.should_acknowledge() {
            self.last_batch_accepted = 1;
            self.last_batch_work_sum = share_work;
            self.batch_acknowledged = false;
        } else {
            self.last_batch_accepted = self.last_batch_accepted.saturating_add(1);
            self.last_batch_work_sum += share_work;
        }
    }

    /// Clears the set of seen share hashes.
    ///
    /// Must be called whenever the chain tip's `prev_hash` changes, and only then. Accepted
    /// hashes are retained for as long as `prev_hash` is unchanged: job and template IDs are not
    /// committed into the block header, so a job replaced under the same `prev_hash` commits to
    /// the same header space as its predecessor, and flushing then would let a proof that was
    /// already credited be credited again under the new job ID. This is also what makes the
    /// seen-shares budget per-`prev_hash`: the set only ever holds the shares accepted while one
    /// `prev_hash` was current.
    pub fn flush_seen_shares(&mut self) {
        self.seen_shares.clear();
    }

    /// Returns `true` once the accepted-share dedup cache has reached its budget.
    ///
    /// Evicting a still-valid hash is never acceptable on a server (it would re-enable
    /// duplicate-share replay), so overflow is an explicit failure instead: share validation
    /// returns [`ShareValidationError::SeenSharesBudgetExhausted`] and the embedding application
    /// should close the channel. The budget is unreachable by a well-behaved channel, see
    /// [`crate::seen_shares_budget`].
    pub fn is_seen_shares_budget_exhausted(&self) -> bool {
        self.seen_shares.len() >= self.seen_shares_budget
    }

    /// Returns the sequence number of the last accepted share.
    pub fn get_last_share_sequence_number(&self) -> u32 {
        self.last_share_sequence_number
    }

    /// Returns the number of shares accepted in the last batch.
    pub fn get_last_batch_accepted(&self) -> u32 {
        self.last_batch_accepted
    }

    /// Returns the sum of work contributed by shares in the last batch.
    ///
    /// Note: this is meant to be used for `SubmitShares.Success` messages.
    /// Therefore, it truncates `f64` into `u64`.
    pub fn get_last_batch_work_sum(&self) -> u64 {
        self.last_batch_work_sum as u64
    }

    /// Returns the total number of shares accepted on this channel.
    ///
    /// Note: this is not what we use for `SubmitShares.Success` messages.
    /// Instead, there we should use `get_last_batch_accepted()`.
    pub fn get_shares_accepted(&self) -> u32 {
        self.shares_accepted
    }

    /// Returns the number of rejected shares tracked for a specific `error_code`.
    pub fn get_rejected_shares_error_count(&self, error_code: &str) -> u32 {
        self.rejected_shares.get(error_code).copied().unwrap_or(0)
    }

    /// Returns an iterator over rejected shares by error code.
    pub fn get_rejected_shares(&self) -> impl Iterator<Item = (&str, u32)> + '_ {
        self.rejected_shares
            .iter()
            .map(|(error_code, count)| (error_code.as_str(), *count))
    }

    /// Returns the total number of rejected shares on this channel.
    ///
    /// Saturates at `u32::MAX`.
    pub fn get_rejected_shares_count(&self) -> u32 {
        self.rejected_shares
            .values()
            .copied()
            .fold(0, u32::saturating_add)
    }

    /// Returns the sum of work contributed by all accepted shares.
    ///
    /// Note: this is not what we use for `SubmitShares.Success` messages.
    /// Instead, there we should use `get_last_batch_work_sum()`.
    pub fn get_share_work_sum(&self) -> f64 {
        self.share_work_sum
    }

    /// Returns the configured batch size for share acknowledgments.
    pub fn get_share_batch_size(&self) -> usize {
        self.share_batch_size
    }

    /// Returns true if the current batch is full and ready for acknowledgment.
    pub fn should_acknowledge(&self) -> bool {
        self.last_batch_accepted == self.share_batch_size as u32
    }

    /// Checks if the share hash has already been accepted (duplicate detection).
    ///
    /// The underlying set holds at most `seen_shares_budget` hashes (see
    /// [`is_seen_shares_budget_exhausted`](Self::is_seen_shares_budget_exhausted)) and is flushed
    /// whenever the chain tip's `prev_hash` changes.
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

    /// Increments the blocks found counter.
    ///
    /// Saturates at `u32::MAX`.
    pub fn increment_blocks_found(&mut self) {
        self.blocks_found = self.blocks_found.saturating_add(1);
    }

    /// Marks the current batch as acknowledged so the next accepted share starts a fresh batch.
    ///
    /// Call this after a mid-batch `SubmitShares.Success` is guaranteed to be sent (e.g. on
    /// block-found events) to avoid double-counting shares in the next batch boundary
    /// acknowledgment.
    pub fn mark_batch_acknowledged(&mut self) {
        self.batch_acknowledged = true;
    }

    /// Returns the total number of blocks found on this channel.
    pub fn get_blocks_found(&self) -> u32 {
        self.blocks_found
    }
}

#[cfg(test)]
mod tests {
    use super::ShareAccounting;
    use bitcoin::hashes::Hash as _;

    #[test]
    fn rejected_shares_are_tracked_by_error_code() {
        let mut accounting = ShareAccounting::new(10, 1_200);

        accounting.increment_rejected_shares("difficulty-too-low");
        accounting.increment_rejected_shares("duplicate-share");
        accounting.increment_rejected_shares("difficulty-too-low");

        assert_eq!(accounting.get_rejected_shares_count(), 3);
        assert_eq!(
            accounting.get_rejected_shares_error_count("difficulty-too-low"),
            2
        );
        assert_eq!(
            accounting.get_rejected_shares_error_count("duplicate-share"),
            1
        );
    }

    #[test]
    fn counters_saturate_at_u32_max() {
        let mut accounting = ShareAccounting::new(10, 1_200);

        accounting.shares_accepted = u32::MAX - 1;
        accounting.blocks_found = u32::MAX;
        accounting.last_batch_accepted = u32::MAX - 1;

        accounting.update_share_accounting(1.0, 0, bitcoin::hashes::sha256d::Hash::all_zeros());
        assert_eq!(accounting.shares_accepted, u32::MAX);
        assert_eq!(accounting.last_batch_accepted, u32::MAX);

        accounting.update_share_accounting(1.0, 1, bitcoin::hashes::sha256d::Hash::all_zeros());
        assert_eq!(accounting.shares_accepted, u32::MAX);
        assert_eq!(accounting.last_batch_accepted, u32::MAX);

        accounting.increment_blocks_found();
        assert_eq!(accounting.blocks_found, u32::MAX);
        accounting.increment_blocks_found();
        assert_eq!(accounting.blocks_found, u32::MAX);
    }

    #[test]
    fn rejected_shares_count_saturates() {
        let mut accounting = ShareAccounting::new(10, 1_200);

        accounting.rejected_shares.insert("a".to_string(), u32::MAX);
        accounting.rejected_shares.insert("b".to_string(), 1);

        assert_eq!(accounting.get_rejected_shares_count(), u32::MAX);
    }

    #[test]
    fn increment_rejected_shares_saturates() {
        let mut accounting = ShareAccounting::new(10, 1_200);

        accounting
            .rejected_shares
            .insert("err".to_string(), u32::MAX);
        accounting.increment_rejected_shares("err");

        assert_eq!(accounting.rejected_shares.get("err"), Some(&u32::MAX));
    }

    #[test]
    fn seen_shares_budget_is_a_hard_limit() {
        fn hash(i: u32) -> bitcoin::hashes::sha256d::Hash {
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            <bitcoin::hashes::sha256d::Hash as bitcoin::hashes::Hash>::from_slice(&bytes).unwrap()
        }

        let budget = 100;
        let mut accounting = ShareAccounting::new(10, budget);

        for i in 0..budget as u32 {
            assert!(!accounting.is_seen_shares_budget_exhausted());
            accounting.update_share_accounting(1.0, i, hash(i));
        }
        assert!(accounting.is_seen_shares_budget_exhausted());

        // every hash accepted within the budget stays resident: nothing is evicted, so no
        // duplicate can ever be re-admitted
        for i in 0..budget as u32 {
            assert!(accounting.is_share_seen(hash(i)));
        }

        // a chain-tip transition resets the budget
        accounting.flush_seen_shares();
        assert!(!accounting.is_seen_shares_budget_exhausted());
        assert!(!accounting.is_share_seen(hash(0)));
    }
}
