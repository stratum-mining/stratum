use bitcoin::hashes::sha256d::Hash;

pub trait ShareAccounting {
    /// Updates the accounting state with a newly accepted share.
    fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    );

    /// Checks if a share hash has already been seen (duplicate detection).
    fn is_share_seen(&self, share_hash: Hash) -> bool;

    /// Clears the set of seen share hashes.
    fn flush_seen_shares(&mut self);

    /// Returns the sequence number of the last accepted share.
    fn get_last_share_sequence_number(&self) -> u32;

    /// Returns the total number of shares accepted.
    fn get_shares_accepted(&self) -> u32;

    /// Returns the cumulative work of all accepted shares.
    fn get_share_work_sum(&self) -> u64;

    /// Returns the highest difficulty found across all users and shares.
    fn get_best_diff(&self) -> f64;

    /// Updates the best difficulty if the new value is higher (backward compatibility).
    fn update_best_diff(&mut self, diff: f64);
}

pub trait ShareAccountingClient: ShareAccounting {
    fn new() -> Self;
}

pub trait ShareAccountingServer: ShareAccounting {
    /// Returns a new ShareAccountingTrait instance with batch size defined
    fn new(share_batch_size: usize) -> Self;

    /// Returns the configured share batch size for server acknowledgments.
    fn get_share_batch_size(&self) -> usize;

    /// Determines if the current share count triggers a batch acknowledgment.
    fn should_acknowledge(&self) -> bool;
}
