//! Share Validation - Mining Client Abstraction.
//!
//! This module provides types and logic for validating mining shares, tracking share
//! statistics, and reporting share validation results and errors. These abstractions
//! are intended for use in Mining Clients.

extern crate alloc;
use super::{HashMap, MAX_SEEN_SHARES};
use alloc::{collections::VecDeque, string::String};
use bitcoin::hashes::sha256d::Hash;
use mining_sv2::{
    ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE, ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE, ERROR_CODE_SUBMIT_SHARES_INVALID_CHANNEL_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE, ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
    ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
};

/// Bucket used by [`ShareAccounting::on_share_rejection`] for `error_code` values that are not
/// part of the known `SubmitShares.Error` enumeration.
///
/// The `error_code` field of [`SubmitSharesError`](mining_sv2::SubmitSharesError) is a `Str0255`
/// fully controlled by the upstream server. Folding unrecognized values into a single bucket keeps
/// the rejected-share map bounded, instead of letting a malicious server grow it without limit.
pub const UNKNOWN_ERROR_CODE: &str = "unknown";

/// The `error_code` values that [`ShareAccounting`] tracks individually.
///
/// Anything else is counted under [`UNKNOWN_ERROR_CODE`].
const KNOWN_ERROR_CODES: [&str; 9] = [
    ERROR_CODE_SUBMIT_SHARES_INVALID_CHANNEL_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
    ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
    ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
    ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE,
    ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
];

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
/// **Validation phase** (updated by `validate_share` via [`track_validated_share`](Self::track_validated_share)):
/// - total validated shares (shares that passed local validation)
/// - cumulative validated work (based on each job's target difficulty)
/// - hashes of seen shares (for duplicate detection)
/// - last received share's sequence number
/// - highest difficulty seen in validated shares
///
/// **Acceptance phase** (updated by the application layer via [`on_share_acknowledgement`](Self::on_share_acknowledgement)):
/// - total acknowledged shares (confirmed by upstream [`SubmitSharesSuccess`](mining_sv2::SubmitSharesSuccess))
/// - total rejected shares (reported by upstream [`SubmitSharesError`](mining_sv2::SubmitSharesError)), bucketed by `error_code`
/// - cumulative acknowledged work (as reported by upstream [`SubmitSharesSuccess`](mining_sv2::SubmitSharesSuccess))
/// - number of blocks found
///
/// # Duplicate detection is bounded, and its overflow is an accepted replay window
///
/// `seen_shares` holds at most [`MAX_SEEN_SHARES`] validated hashes, evicting oldest-first.
/// After that many validated shares, a replay of an evicted share passes
/// [`is_share_seen`](Self::is_share_seen) again and is validated (and forwarded) a second time. This is deliberate: clients do not fail hard at the bound the way
/// server channels do, because
///
/// - the target is upstream-controlled, so no bound derived from an expected share rate can
///   constrain a hostile upstream; the flat, device-affordable [`MAX_SEEN_SHARES`] is the only
///   honest response to an untrusted rate input;
/// - the double count is a local, pre-upstream statistic (`validated_shares`,
///   `validated_work_sum`, `blocks_found`): the upstream's own dedup still rejects the replayed
///   share, so nothing is paid out on it;
/// - failing validation at the bound would hand a hostile upstream advertising a trivial target
///   a one-message channel-kill vector, whereas eviction keeps the channel alive at the cost of
///   a bounded, upstream-rejected replay.
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
    // Accepted share hashes, oldest at the front; bounded by `MAX_SEEN_SHARES`.
    // A flat `VecDeque` rather than a set plus a companion order queue: storing each hash once
    // halves the footprint that matters on embedded targets, and scanning 4 096 contiguous
    // hashes per share is negligible at client share rates.
    seen_shares: VecDeque<Hash>,
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
            seen_shares: VecDeque::new(),
            best_diff: 0.0,
            blocks_found: 0,
        }
    }

    /// Updates acceptance accounting based on a [`SubmitSharesSuccess`](mining_sv2::SubmitSharesSuccess) message from the
    /// upstream server.
    ///
    /// Both `acknowledged_shares` and `acknowledged_work_sum` saturate at their respective
    /// maximum values.
    ///
    /// This should be called by the application layer when it receives upstream confirmation
    /// that shares were accepted. It is intentionally **not** called from `validate_share` —
    /// local validation only tracks the share for duplicate detection (via
    /// [`track_validated_share`](Self::track_validated_share)).
    pub fn on_share_acknowledgement(
        &mut self,
        new_submits_accepted_count: u32,
        new_shares_sum: u64,
    ) {
        self.acknowledged_shares = self
            .acknowledged_shares
            .saturating_add(new_submits_accepted_count);
        self.acknowledged_work_sum = self.acknowledged_work_sum.saturating_add(new_shares_sum);
    }

    /// Updates rejection accounting based on a [`SubmitSharesError`](mining_sv2::SubmitSharesError) message from the upstream
    /// server.
    ///
    /// One call corresponds to one rejected share.
    ///
    /// `error_code` is only tracked under its own key if it is one of the known
    /// `SubmitShares.Error` codes; any other value is counted under [`UNKNOWN_ERROR_CODE`].
    /// This keeps the internal map bounded, since `error_code` is fully controlled by the
    /// upstream server.
    ///
    /// Each counter saturates at `u32::MAX`.
    pub fn on_share_rejection(&mut self, error_code: &str) {
        let key = if KNOWN_ERROR_CODES.contains(&error_code) {
            error_code
        } else {
            UNKNOWN_ERROR_CODE
        };
        if let Some(count) = self.rejected_shares.get_mut(key) {
            *count = count.saturating_add(1);
        } else {
            self.rejected_shares.insert(String::from(key), 1);
        }
    }

    /// Records a share that passed local validation.
    ///
    /// Adds the hash to the seen set for duplicate detection and updates the last sequence
    /// number. Called from `validate_share` — does **not** count the share as accepted.
    /// Acceptance accounting is deferred to [`on_share_acknowledgement`](Self::on_share_acknowledgement), which should be
    /// called when the upstream server confirms via [`SubmitSharesSuccess`](mining_sv2::SubmitSharesSuccess).
    ///
    /// `validated_shares` saturates at `u32::MAX`.
    ///
    /// At most [`MAX_SEEN_SHARES`] hashes are retained for duplicate detection; beyond that the
    /// oldest hash is evicted.
    ///
    /// Unlike the server side, overflow evicts rather than failing: a replay of an evicted hash
    /// only double-counts one local statistic — nothing is paid out, and nothing is forwarded as
    /// newly-validated that the upstream won't independently dedup. That is also why the bound
    /// is a flat constant here instead of being derived from the channel's target and hashrate:
    /// the target is upstream-controlled, so a derived bound could not constrain a hostile
    /// upstream anyway, and it would have to be clamped to something affordable on the smallest
    /// supported device regardless — which is exactly what [`MAX_SEEN_SHARES`] already is.
    pub fn track_validated_share(
        &mut self,
        share_sequence_number: u32,
        share_hash: Hash,
        share_work: f64,
    ) {
        self.last_share_sequence_number = share_sequence_number;
        self.validated_shares = self.validated_shares.saturating_add(1);
        self.validated_work_sum += share_work;
        if !self.seen_shares.contains(&share_hash) {
            // evict before inserting, so the queue never exceeds the bound even transiently and
            // its backing allocation settles at exactly `MAX_SEEN_SHARES` entries
            if self.seen_shares.len() == MAX_SEEN_SHARES {
                self.seen_shares.pop_front();
            }
            self.seen_shares.push_back(share_hash);
        }
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

    /// Returns the number of rejected shares tracked for a specific `error_code`.
    ///
    /// Only the known `SubmitShares.Error` codes are tracked individually; unrecognized upstream
    /// codes are counted under [`UNKNOWN_ERROR_CODE`], so querying an arbitrary string returns 0.
    pub fn get_rejected_shares_error_count(&self, error_code: &str) -> u32 {
        self.rejected_shares.get(error_code).copied().unwrap_or(0)
    }

    /// Returns the total number of rejected shares across all error codes.
    ///
    /// Saturates at `u32::MAX`.
    pub fn get_rejected_shares_count(&self) -> u32 {
        self.rejected_shares
            .values()
            .copied()
            .fold(0, u32::saturating_add)
    }

    /// Returns an iterator over rejected shares by error code.
    ///
    /// Yields at most the known `SubmitShares.Error` codes plus [`UNKNOWN_ERROR_CODE`].
    pub fn get_rejected_shares(&self) -> impl Iterator<Item = (&str, u32)> + '_ {
        self.rejected_shares
            .iter()
            .map(|(error_code, count)| (error_code.as_str(), *count))
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
    ///
    /// The underlying queue holds at most [`MAX_SEEN_SHARES`] hashes (oldest evicted first) and
    /// is kept across chain tip changes. Job IDs are not committed into the block header, so a
    /// job under a `prev_hash` the tip has already had, whether it stayed (a repeated
    /// `SetNewPrevHash`) or came back to it (A→B→A), can commit to the same header space as an
    /// earlier one, and forgetting its hashes would let a proof that was already validated (and
    /// forwarded) be validated again under the new job ID. Header hashes commit to `prev_hash`,
    /// so hashes from different tips never compare equal.
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
    ///
    /// Saturates at `u32::MAX`.
    pub fn increment_blocks_found(&mut self) {
        self.blocks_found = self.blocks_found.saturating_add(1);
    }

    /// Returns the total number of blocks found on this channel.
    pub fn get_blocks_found(&self) -> u32 {
        self.blocks_found
    }
}

#[cfg(test)]
mod tests {
    use super::{alloc::format, ShareAccounting, MAX_SEEN_SHARES, UNKNOWN_ERROR_CODE};
    use bitcoin::hashes::Hash as _;

    #[test]
    fn counters_saturate_at_u32_max() {
        let mut accounting = ShareAccounting::new();

        accounting.validated_shares = u32::MAX;
        accounting.acknowledged_shares = u32::MAX - 1;
        accounting.blocks_found = u32::MAX;

        accounting.track_validated_share(0, bitcoin::hashes::sha256d::Hash::all_zeros(), 1.0);
        assert_eq!(accounting.validated_shares, u32::MAX);
        accounting.track_validated_share(1, bitcoin::hashes::sha256d::Hash::all_zeros(), 1.0);
        assert_eq!(accounting.validated_shares, u32::MAX);

        accounting.on_share_acknowledgement(1, 0);
        assert_eq!(accounting.acknowledged_shares, u32::MAX);
        accounting.on_share_acknowledgement(1, 0);
        assert_eq!(accounting.acknowledged_shares, u32::MAX);

        accounting.increment_blocks_found();
        assert_eq!(accounting.blocks_found, u32::MAX);
        accounting.increment_blocks_found();
        assert_eq!(accounting.blocks_found, u32::MAX);
    }

    #[test]
    fn rejected_shares_count_saturates() {
        let mut accounting = ShareAccounting::new();

        accounting.rejected_shares.insert("a".to_string(), u32::MAX);
        accounting.rejected_shares.insert("b".to_string(), 1);

        assert_eq!(accounting.get_rejected_shares_count(), u32::MAX);
    }

    #[test]
    fn on_share_rejection_saturates() {
        let mut accounting = ShareAccounting::new();

        accounting
            .rejected_shares
            .insert("difficulty-too-low".to_string(), u32::MAX);
        accounting.on_share_rejection("difficulty-too-low");

        assert_eq!(
            accounting.rejected_shares.get("difficulty-too-low"),
            Some(&u32::MAX)
        );
    }

    #[test]
    fn unknown_error_codes_are_bounded() {
        let mut accounting = ShareAccounting::new();

        for i in 0..10_000 {
            accounting.on_share_rejection(&format!("attacker-controlled-garbage-{i}"));
        }
        accounting.on_share_rejection("difficulty-too-low");
        accounting.on_share_rejection("difficulty-too-low");

        // one bucket for the garbage, one for the known code
        assert_eq!(accounting.get_rejected_shares().count(), 2);
        assert_eq!(
            accounting.get_rejected_shares_error_count(UNKNOWN_ERROR_CODE),
            10_000
        );
        assert_eq!(
            accounting.get_rejected_shares_error_count("difficulty-too-low"),
            2
        );
        assert_eq!(accounting.get_rejected_shares_count(), 10_002);
    }

    #[test]
    fn seen_shares_are_bounded_by_fifo_eviction() {
        fn hash(i: u32) -> bitcoin::hashes::sha256d::Hash {
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            <bitcoin::hashes::sha256d::Hash as bitcoin::hashes::Hash>::from_slice(&bytes).unwrap()
        }

        let cap = MAX_SEEN_SHARES as u32;
        let overflow = 100;
        let mut accounting = ShareAccounting::new();

        // flood past the bound with unique hashes, as an adversarial upstream advertising a
        // trivial target would
        for i in 0..cap + overflow {
            accounting.track_validated_share(i, hash(i), 1.0);
        }

        // retention is bounded, and it is the oldest hashes that were dropped
        assert_eq!(accounting.seen_shares.len(), cap as usize);
        for i in 0..overflow {
            assert!(!accounting.is_share_seen(hash(i)));
        }
        for i in overflow..cap + overflow {
            assert!(accounting.is_share_seen(hash(i)));
        }
    }
}
