use bitcoin::hashes::sha256d::Hash;
use std::error::Error as StdError;

pub trait ShareAccountingTrait {
    /// The error type returned by trait methods.
    type Error: StdError + Send + Sync + 'static;

    /// Updates the accounting state with a newly accepted share.
    fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) -> Result<(), Self::Error>;

    /// Checks if a share hash has already been seen (duplicate detection).
    fn is_share_seen(&self, share_hash: Hash) -> Result<bool, Self::Error>;

    /// Clears the set of seen share hashes.
    fn flush_seen_shares(&mut self) -> Result<(), Self::Error>;

    /// Returns the sequence number of the last accepted share.
    fn get_last_share_sequence_number(&self) -> Result<u32, Self::Error>;

    /// Returns the total number of shares accepted.
    fn get_shares_accepted(&self) -> Result<u32, Self::Error>;

    /// Returns the cumulative work of all accepted shares.
    fn get_share_work_sum(&self) -> Result<u64, Self::Error>;

    /// Returns the highest difficulty found across all users and shares.
    fn get_best_diff(&self) -> Result<f64, Self::Error>;

    /// Updates the best difficulty if the new value is higher (backward compatibility).
    fn update_best_diff(&mut self, diff: f64) -> Result<(), Self::Error>;
}

pub trait ShareAccountingServerTrait<T> 
where 
    T: ShareAccountingTrait
{
    /// The error type returned by trait methods.
    type Error: StdError + Send + Sync + 'static;

    fn new(share_batch_size: usize) -> Self;

    /// Returns the configured share batch size for server acknowledgments.
    /// Returns `None` for client implementations.
    fn get_share_batch_size(&self) -> Result<Option<usize>, Self::Error>;

    /// Determines if the current share count triggers a batch acknowledgment.
    /// For server implementations, returns `true` when shares are a multiple of batch size.
    /// For client implementations, returns `false`.
    fn should_acknowledge(&self) -> Result<bool, Self::Error>;
}