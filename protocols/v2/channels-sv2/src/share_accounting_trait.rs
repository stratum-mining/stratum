//! Share Accounting Trait - Storage-Agnostic Share Accounting Interface.
//!
//! This module provides a trait-based abstraction for share accounting operations that can be
//! implemented with different storage backends. The trait maintains compatibility with existing
//! client and server ShareAccounting implementations while enabling future extensibility.
//!
//! ## Overview
//!
//! The [`ShareAccountingTrait`] defines a common interface for:
//! - Share validation state tracking
//! - Duplicate share detection
//! - Share statistics and work accounting
//! - Batch acknowledgment logic (server-specific)
//! - Per-user and per-process difficulty tracking
//!
//! ## Usage
//!
//! ```rust
//! use channels_sv2::share_accounting_trait::{ShareAccountingTrait, ShareAccountingConfig};
//! use bitcoin::hashes::sha256d::Hash;
//! use bitcoin::hashes::Hash as HashTrait;
//!
//! // Create an in-memory implementation for server use
//! let config = ShareAccountingConfig::InMemory { share_batch_size: Some(10) };
//! let mut accounting = create_share_accounting(config).unwrap();
//!
//! // Process a share
//! let share_hash = Hash::from_slice(&[0u8; 32]).unwrap();
//! accounting.update_share_accounting(1000, 1, share_hash).unwrap();
//!
//! // Check statistics
//! assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
//! assert_eq!(accounting.get_share_work_sum().unwrap(), 1000);
//! ```

use bitcoin::hashes::sha256d::Hash;
use std::collections::{BTreeMap, HashSet};
use std::error::Error as StdError;

/// Configuration for different share accounting storage backends.
///
/// This enum allows for initialization of different storage implementations
/// while maintaining a common interface. Future storage backends can be added
/// as new variants without breaking existing code.
#[derive(Debug, Clone)]
pub enum ShareAccountingConfig {
    /// In-memory storage configuration.
    ///
    /// - `share_batch_size`: Optional batch size for server acknowledgments.
    ///   Use `None` for client mode, `Some(size)` for server mode.
    InMemory {
        /// Batch size for share acknowledgments. None for client mode, Some(size) for server mode.
        share_batch_size: Option<usize>,
    },
    // Future storage backends can be added here:
    // Database { connection_string: String, share_batch_size: Option<usize> },
    // File { path: PathBuf, share_batch_size: Option<usize> },
}

/// Trait defining the interface for share accounting operations.
///
/// This trait abstracts share accounting functionality to support different storage backends
/// while maintaining compatibility with existing client and server implementations.
/// All methods return `Result` types to accommodate storage operations that might fail
/// in persistent storage scenarios.
///
/// ## Client vs Server Behavior
///
/// - **Client mode**: `share_batch_size` is `None`, batch-related methods return appropriate defaults
/// - **Server mode**: `share_batch_size` is configured, batch acknowledgment logic is active
///
/// ## Thread Safety
///
/// Implementations should document their thread safety guarantees. The trait itself
/// does not require `Send` or `Sync`, allowing for both thread-safe and single-threaded
/// implementations.
pub trait ShareAccountingTrait {
    /// The error type returned by trait methods.
    ///
    /// This associated type allows each implementation to define appropriate error types
    /// for their storage backend (e.g., database errors, I/O errors, serialization errors).
    type Error: StdError + Send + Sync + 'static;

    /// Updates the accounting state with a newly accepted share.
    ///
    /// This method should:
    /// - Increment the total share count
    /// - Add the share work to the cumulative work sum
    /// - Update the last share sequence number
    /// - Record the share hash for duplicate detection
    ///
    /// # Arguments
    ///
    /// * `share_work` - The work value contributed by this share
    /// * `share_sequence_number` - The sequence number of this share
    /// * `share_hash` - The hash of the share for duplicate detection
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// accounting.update_share_accounting(1000, 1, share_hash).unwrap();
    /// assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
    /// assert_eq!(accounting.get_share_work_sum().unwrap(), 1000);
    /// ```
    fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) -> Result<(), Self::Error>;

    /// Checks if a share hash has already been seen (duplicate detection).
    ///
    /// This method is used to prevent duplicate share submissions by checking
    /// if the share hash has been processed before.
    ///
    /// # Arguments
    ///
    /// * `share_hash` - The hash of the share to check
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the share has been seen before, `Ok(false)` if it's new,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// assert_eq!(accounting.is_share_seen(share_hash).unwrap(), false);
    /// 
    /// accounting.update_share_accounting(1000, 1, share_hash).unwrap();
    /// assert_eq!(accounting.is_share_seen(share_hash).unwrap(), true);
    /// ```
    fn is_share_seen(&self, share_hash: Hash) -> Result<bool, Self::Error>;

    /// Clears the set of seen share hashes.
    ///
    /// This method should be called on chain tip updates to prevent unbounded
    /// memory growth and allow shares for the new chain tip. The duplicate
    /// detection cache is typically kept in memory regardless of the storage backend.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// accounting.update_share_accounting(1000, 1, share_hash).unwrap();
    /// assert_eq!(accounting.is_share_seen(share_hash).unwrap(), true);
    /// 
    /// accounting.flush_seen_shares().unwrap();
    /// assert_eq!(accounting.is_share_seen(share_hash).unwrap(), false);
    /// ```
    fn flush_seen_shares(&mut self) -> Result<(), Self::Error>;

    /// Returns the sequence number of the last accepted share.
    ///
    /// # Returns
    ///
    /// `Ok(sequence_number)` with the last share's sequence number,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 0);
    /// 
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// accounting.update_share_accounting(1000, 42, share_hash).unwrap();
    /// assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 42);
    /// ```
    fn get_last_share_sequence_number(&self) -> Result<u32, Self::Error>;

    /// Returns the total number of shares accepted.
    ///
    /// # Returns
    ///
    /// `Ok(count)` with the total number of accepted shares,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// assert_eq!(accounting.get_shares_accepted().unwrap(), 0);
    /// 
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// accounting.update_share_accounting(1000, 1, share_hash).unwrap();
    /// assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
    /// ```
    fn get_shares_accepted(&self) -> Result<u32, Self::Error>;

    /// Returns the cumulative work of all accepted shares.
    ///
    /// # Returns
    ///
    /// `Ok(work_sum)` with the total work contributed by all accepted shares,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
    /// 
    /// let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
    /// let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
    /// accounting.update_share_accounting(1000, 1, share_hash1).unwrap();
    /// accounting.update_share_accounting(2000, 2, share_hash2).unwrap();
    /// assert_eq!(accounting.get_share_work_sum().unwrap(), 3000);
    /// ```
    fn get_share_work_sum(&self) -> Result<u64, Self::Error>;

    /// Returns the highest difficulty found across all users and shares.
    ///
    /// This represents the per-process best difficulty, which is derived from
    /// the maximum of all per-user difficulties.
    ///
    /// # Returns
    ///
    /// `Ok(difficulty)` with the highest difficulty value found,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
    /// 
    /// accounting.update_best_diff_for_user("user1", 1000.0).unwrap();
    /// accounting.update_best_diff_for_user("user2", 2000.0).unwrap();
    /// assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);
    /// ```
    fn get_best_diff(&self) -> Result<f64, Self::Error>;

    /// Returns the best difficulty for a specific user.
    ///
    /// # Arguments
    ///
    /// * `user_identity` - The user identity string (treated as opaque)
    ///
    /// # Returns
    ///
    /// `Ok(Some(difficulty))` if the user has recorded shares,
    /// `Ok(None)` if the user has no recorded shares,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// assert_eq!(accounting.get_best_diff_for_user("user1").unwrap(), None);
    /// 
    /// accounting.update_best_diff_for_user("user1", 1500.0).unwrap();
    /// assert_eq!(accounting.get_best_diff_for_user("user1").unwrap(), Some(1500.0));
    /// ```
    fn get_best_diff_for_user(&self, user_identity: &str) -> Result<Option<f64>, Self::Error>;

    /// Updates the best difficulty for a specific user if the new value is higher.
    ///
    /// This method should only update the stored difficulty if the new difficulty
    /// is higher than the current best for this user.
    ///
    /// # Arguments
    ///
    /// * `user_identity` - The user identity string (treated as opaque)
    /// * `diff` - The new difficulty value to consider
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// accounting.update_best_diff_for_user("user1", 1000.0).unwrap();
    /// assert_eq!(accounting.get_best_diff_for_user("user1").unwrap(), Some(1000.0));
    /// 
    /// // Lower difficulty should not update
    /// accounting.update_best_diff_for_user("user1", 500.0).unwrap();
    /// assert_eq!(accounting.get_best_diff_for_user("user1").unwrap(), Some(1000.0));
    /// 
    /// // Higher difficulty should update
    /// accounting.update_best_diff_for_user("user1", 2000.0).unwrap();
    /// assert_eq!(accounting.get_best_diff_for_user("user1").unwrap(), Some(2000.0));
    /// ```
    fn update_best_diff_for_user(&mut self, user_identity: &str, diff: f64) -> Result<(), Self::Error>;

    /// Returns the top users by difficulty in descending order.
    ///
    /// This method provides efficient querying of the highest-difficulty users,
    /// useful for statistics and monitoring.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of users to return
    ///
    /// # Returns
    ///
    /// `Ok(users)` with a vector of (user_identity, difficulty) tuples ordered by
    /// difficulty in descending order, or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # let mut accounting = create_share_accounting(
    /// #     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// # ).unwrap();
    /// accounting.update_best_diff_for_user("alice", 1000.0).unwrap();
    /// accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
    /// accounting.update_best_diff_for_user("charlie", 1500.0).unwrap();
    /// 
    /// let top_users = accounting.get_top_user_difficulties(2).unwrap();
    /// assert_eq!(top_users.len(), 2);
    /// assert_eq!(top_users[0], ("bob".to_string(), 2000.0));
    /// assert_eq!(top_users[1], ("charlie".to_string(), 1500.0));
    /// ```
    fn get_top_user_difficulties(&self, limit: usize) -> Result<Vec<(String, f64)>, Self::Error>;

    /// Returns the configured share batch size for server acknowledgments.
    ///
    /// This method returns the batch size configuration used for determining
    /// when to send batch acknowledgments. Returns `None` for client implementations.
    ///
    /// # Returns
    ///
    /// `Ok(Some(batch_size))` for server implementations,
    /// `Ok(None)` for client implementations,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// // Client mode
    /// let client_accounting = create_share_accounting(
    ///     ShareAccountingConfig::InMemory { share_batch_size: None }
    /// ).unwrap();
    /// assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
    /// 
    /// // Server mode
    /// let server_accounting = create_share_accounting(
    ///     ShareAccountingConfig::InMemory { share_batch_size: Some(10) }
    /// ).unwrap();
    /// assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(10));
    /// ```
    fn get_share_batch_size(&self) -> Result<Option<usize>, Self::Error>;

    /// Determines if the current share count triggers a batch acknowledgment.
    ///
    /// For server implementations, this returns `true` when the number of accepted
    /// shares is a multiple of the configured batch size. For client implementations,
    /// this should return `false`.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if an acknowledgment should be sent,
    /// `Ok(false)` otherwise,
    /// or an error if the storage operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::*;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// let mut accounting = create_share_accounting(
    ///     ShareAccountingConfig::InMemory { share_batch_size: Some(3) }
    /// ).unwrap();
    /// 
    /// // No acknowledgment initially
    /// assert_eq!(accounting.should_acknowledge().unwrap(), false);
    /// 
    /// // Add shares
    /// for i in 1..=3 {
    ///     let share_hash = Hash::from_slice(&[i; 32]).unwrap();
    ///     accounting.update_share_accounting(1000, i as u32, share_hash).unwrap();
    /// }
    /// 
    /// // Should acknowledge after 3 shares (batch size)
    /// assert_eq!(accounting.should_acknowledge().unwrap(), true);
    /// ```
    fn should_acknowledge(&self) -> Result<bool, Self::Error>;
}

/// Error type for in-memory share accounting operations.
///
/// This error type is used by the [`InMemoryShareAccounting`] implementation.
/// Since in-memory operations rarely fail, this enum is primarily for future
/// extensibility and maintaining consistency with the trait interface.
#[derive(Debug)]
pub enum InMemoryShareAccountingError {
    /// Internal error that should not occur in normal operation.
    Internal(String),
}

impl std::fmt::Display for InMemoryShareAccountingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InMemoryShareAccountingError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl StdError for InMemoryShareAccountingError {}

/// In-memory implementation of the ShareAccountingTrait.
///
/// This implementation preserves the exact behavior of the existing client and server
/// ShareAccounting structs while implementing the new trait interface. It uses
/// in-memory data structures for all storage operations.
///
/// ## Thread Safety
///
/// This implementation is not thread-safe. If concurrent access is required,
/// wrap it in appropriate synchronization primitives (e.g., `Mutex`, `RwLock`).
///
/// ## Memory Usage
///
/// - Share hashes are stored in a `HashSet` for duplicate detection
/// - Per-user difficulties are stored in a `BTreeMap` for efficient ordering
/// - All other data uses primitive types with minimal memory overhead
#[derive(Clone, Debug)]
pub struct InMemoryShareAccounting {
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: u64,
    share_batch_size: Option<usize>,
    seen_shares: HashSet<Hash>,
    user_best_diffs: BTreeMap<String, f64>,
}

impl InMemoryShareAccounting {
    /// Creates a new in-memory share accounting instance.
    ///
    /// # Arguments
    ///
    /// * `share_batch_size` - Optional batch size for server acknowledgments.
    ///   Use `None` for client mode, `Some(size)` for server mode.
    ///
    /// # Returns
    ///
    /// A new `InMemoryShareAccounting` instance with all statistics initialized to zero.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::InMemoryShareAccounting;
    /// // Client mode
    /// let client_accounting = InMemoryShareAccounting::new(None);
    /// 
    /// // Server mode
    /// let server_accounting = InMemoryShareAccounting::new(Some(10));
    /// ```
    pub fn new(share_batch_size: Option<usize>) -> Self {
        Self {
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0,
            share_batch_size,
            seen_shares: HashSet::new(),
            user_best_diffs: BTreeMap::new(),
        }
    }
}

impl ShareAccountingTrait for InMemoryShareAccounting {
    type Error = InMemoryShareAccountingError;

    fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) -> Result<(), Self::Error> {
        self.last_share_sequence_number = share_sequence_number;
        self.shares_accepted += 1;
        self.share_work_sum += share_work;
        self.seen_shares.insert(share_hash);
        Ok(())
    }

    fn is_share_seen(&self, share_hash: Hash) -> Result<bool, Self::Error> {
        Ok(self.seen_shares.contains(&share_hash))
    }

    fn flush_seen_shares(&mut self) -> Result<(), Self::Error> {
        self.seen_shares.clear();
        Ok(())
    }

    fn get_last_share_sequence_number(&self) -> Result<u32, Self::Error> {
        Ok(self.last_share_sequence_number)
    }

    fn get_shares_accepted(&self) -> Result<u32, Self::Error> {
        Ok(self.shares_accepted)
    }

    fn get_share_work_sum(&self) -> Result<u64, Self::Error> {
        Ok(self.share_work_sum)
    }

    fn get_best_diff(&self) -> Result<f64, Self::Error> {
        // Return the maximum difficulty across all users
        Ok(self.user_best_diffs.values().copied().fold(0.0, f64::max))
    }

    fn get_best_diff_for_user(&self, user_identity: &str) -> Result<Option<f64>, Self::Error> {
        Ok(self.user_best_diffs.get(user_identity).copied())
    }

    fn update_best_diff_for_user(&mut self, user_identity: &str, diff: f64) -> Result<(), Self::Error> {
        let current_best = self.user_best_diffs.get(user_identity).copied().unwrap_or(0.0);
        if diff > current_best {
            self.user_best_diffs.insert(user_identity.to_string(), diff);
        }
        Ok(())
    }

    fn get_top_user_difficulties(&self, limit: usize) -> Result<Vec<(String, f64)>, Self::Error> {
        let mut users: Vec<_> = self.user_best_diffs.iter()
            .map(|(user, &diff)| (user.clone(), diff))
            .collect();
        
        // Sort by difficulty in descending order
        users.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Take only the requested number of users
        users.truncate(limit);
        
        Ok(users)
    }

    fn get_share_batch_size(&self) -> Result<Option<usize>, Self::Error> {
        Ok(self.share_batch_size)
    }

    fn should_acknowledge(&self) -> Result<bool, Self::Error> {
        match self.share_batch_size {
            Some(batch_size) => Ok(self.shares_accepted % batch_size as u32 == 0 && self.shares_accepted > 0),
            None => Ok(false), // Client mode - never acknowledge
        }
    }
}

/// Factory function for creating share accounting trait objects.
///
/// This function provides a convenient way to create trait objects from configuration,
/// enabling easy switching between different storage backends in the future.
///
/// # Arguments
///
/// * `config` - Configuration specifying the storage backend and parameters
///
/// # Returns
///
/// A boxed trait object implementing `ShareAccountingTrait`, or an error if
/// the configuration is invalid or initialization fails.
///
/// # Examples
///
/// ```rust
/// # use channels_sv2::share_accounting_trait::*;
/// // Create client-mode accounting
/// let client_config = ShareAccountingConfig::InMemory { share_batch_size: None };
/// let client_accounting = create_share_accounting(client_config).unwrap();
/// 
/// // Create server-mode accounting
/// let server_config = ShareAccountingConfig::InMemory { share_batch_size: Some(10) };
/// let server_accounting = create_share_accounting(server_config).unwrap();
/// ```
pub fn create_share_accounting(
    config: ShareAccountingConfig,
) -> Result<Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>>, Box<dyn StdError + Send + Sync>> {
    match config {
        ShareAccountingConfig::InMemory { share_batch_size } => {
            Ok(Box::new(InMemoryShareAccounting::new(share_batch_size)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::hashes::Hash as HashTrait;

    #[test]
    fn test_in_memory_share_accounting_basic_operations() {
        let mut accounting = InMemoryShareAccounting::new(None);
        
        // Initial state
        assert_eq!(accounting.get_shares_accepted().unwrap(), 0);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 0);
        assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
        
        // Add a share
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 1, share_hash).unwrap();
        
        // Check updated state
        assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 1000);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 1);
        assert!(accounting.is_share_seen(share_hash).unwrap());
    }

    #[test]
    fn test_duplicate_detection() {
        let mut accounting = InMemoryShareAccounting::new(None);
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        
        // Share not seen initially
        assert!(!accounting.is_share_seen(share_hash).unwrap());
        
        // Add share
        accounting.update_share_accounting(1000, 1, share_hash).unwrap();
        assert!(accounting.is_share_seen(share_hash).unwrap());
        
        // Flush and check again
        accounting.flush_seen_shares().unwrap();
        assert!(!accounting.is_share_seen(share_hash).unwrap());
    }

    #[test]
    fn test_user_difficulty_tracking() {
        let mut accounting = InMemoryShareAccounting::new(None);
        
        // No users initially
        assert_eq!(accounting.get_best_diff_for_user("alice").unwrap(), None);
        assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
        
        // Add difficulty for alice
        accounting.update_best_diff_for_user("alice", 1000.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("alice").unwrap(), Some(1000.0));
        assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);
        
        // Add difficulty for bob (higher)
        accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("bob").unwrap(), Some(2000.0));
        assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);
        
        // Try to update alice with lower difficulty (should not change)
        accounting.update_best_diff_for_user("alice", 500.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("alice").unwrap(), Some(1000.0));
        
        // Update alice with higher difficulty
        accounting.update_best_diff_for_user("alice", 2500.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("alice").unwrap(), Some(2500.0));
        assert_eq!(accounting.get_best_diff().unwrap(), 2500.0);
    }

    #[test]
    fn test_top_user_difficulties() {
        let mut accounting = InMemoryShareAccounting::new(None);
        
        // Add users with different difficulties
        accounting.update_best_diff_for_user("alice", 1000.0).unwrap();
        accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
        accounting.update_best_diff_for_user("charlie", 1500.0).unwrap();
        accounting.update_best_diff_for_user("dave", 500.0).unwrap();
        
        // Get top 2 users
        let top_users = accounting.get_top_user_difficulties(2).unwrap();
        assert_eq!(top_users.len(), 2);
        assert_eq!(top_users[0], ("bob".to_string(), 2000.0));
        assert_eq!(top_users[1], ("charlie".to_string(), 1500.0));
        
        // Get all users
        let all_users = accounting.get_top_user_difficulties(10).unwrap();
        assert_eq!(all_users.len(), 4);
        assert_eq!(all_users[0], ("bob".to_string(), 2000.0));
        assert_eq!(all_users[1], ("charlie".to_string(), 1500.0));
        assert_eq!(all_users[2], ("alice".to_string(), 1000.0));
        assert_eq!(all_users[3], ("dave".to_string(), 500.0));
    }

    #[test]
    fn test_client_mode_batch_behavior() {
        let mut accounting = InMemoryShareAccounting::new(None);
        
        // Client mode should not have batch size
        assert_eq!(accounting.get_share_batch_size().unwrap(), None);
        
        // Should never acknowledge in client mode
        assert!(!accounting.should_acknowledge().unwrap());
        
        // Add shares
        for i in 1..=10 {
            let share_hash = Hash::from_slice(&[i; 32]).unwrap();
            accounting.update_share_accounting(1000, i as u32, share_hash).unwrap();
            assert!(!accounting.should_acknowledge().unwrap());
        }
    }

    #[test]
    fn test_server_mode_batch_behavior() {
        let mut accounting = InMemoryShareAccounting::new(Some(3));
        
        // Server mode should have batch size
        assert_eq!(accounting.get_share_batch_size().unwrap(), Some(3));
        
        // Should not acknowledge initially
        assert!(!accounting.should_acknowledge().unwrap());
        
        // Add shares and check acknowledgment
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 1, share_hash1).unwrap();
        assert!(!accounting.should_acknowledge().unwrap());
        
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 2, share_hash2).unwrap();
        assert!(!accounting.should_acknowledge().unwrap());
        
        let share_hash3 = Hash::from_slice(&[3u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 3, share_hash3).unwrap();
        assert!(accounting.should_acknowledge().unwrap()); // Should acknowledge after 3 shares
        
        // Add more shares
        let share_hash4 = Hash::from_slice(&[4u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 4, share_hash4).unwrap();
        assert!(!accounting.should_acknowledge().unwrap());
        
        let share_hash5 = Hash::from_slice(&[5u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 5, share_hash5).unwrap();
        assert!(!accounting.should_acknowledge().unwrap());
        
        let share_hash6 = Hash::from_slice(&[6u8; 32]).unwrap();
        accounting.update_share_accounting(1000, 6, share_hash6).unwrap();
        assert!(accounting.should_acknowledge().unwrap()); // Should acknowledge after 6 shares
    }

    #[test]
    fn test_factory_function() {
        // Test client configuration
        let client_config = ShareAccountingConfig::InMemory { share_batch_size: None };
        let client_accounting = create_share_accounting(client_config).unwrap();
        assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
        
        // Test server configuration
        let server_config = ShareAccountingConfig::InMemory { share_batch_size: Some(5) };
        let server_accounting = create_share_accounting(server_config).unwrap();
        assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(5));
    }
}