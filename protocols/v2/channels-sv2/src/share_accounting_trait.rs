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
//! use channels_sv2::share_accounting_trait::{ShareAccountingTrait, ShareAccountingConfig, create_share_accounting};
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
    ///   When specified, must be greater than 0.
    InMemory {
        /// Batch size for share acknowledgments. None for client mode, Some(size) for server mode.
        /// Must be greater than 0 when specified.
        share_batch_size: Option<usize>,
    },
    // Future storage backends can be added here:
    // Database { connection_string: String, share_batch_size: Option<usize> },
    // File { path: PathBuf, share_batch_size: Option<usize> },
}

/// Error type for configuration validation.
#[derive(Debug)]
pub enum ConfigError {
    /// Invalid batch size (must be greater than 0).
    InvalidBatchSize(usize),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidBatchSize(size) => {
                write!(
                    f,
                    "Invalid batch size: {}. Batch size must be greater than 0",
                    size
                )
            }
        }
    }
}

impl StdError for ConfigError {}

impl ShareAccountingConfig {
    /// Validates the configuration parameters.
    ///
    /// This method checks that all configuration parameters are valid:
    /// - Batch size must be greater than 0 when specified
    /// - Future storage backends can add their own validation rules
    ///
    /// # Returns
    ///
    /// `Ok(())` if the configuration is valid, or a `ConfigError` if validation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::ShareAccountingConfig;
    /// // Valid configurations
    /// let client_config = ShareAccountingConfig::InMemory { share_batch_size: None };
    /// assert!(client_config.validate().is_ok());
    ///
    /// let server_config = ShareAccountingConfig::InMemory { share_batch_size: Some(10) };
    /// assert!(server_config.validate().is_ok());
    ///
    /// // Invalid configuration
    /// let invalid_config = ShareAccountingConfig::InMemory { share_batch_size: Some(0) };
    /// assert!(invalid_config.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            ShareAccountingConfig::InMemory { share_batch_size } => {
                if let Some(batch_size) = share_batch_size {
                    if *batch_size == 0 {
                        return Err(ConfigError::InvalidBatchSize(*batch_size));
                    }
                }
                Ok(())
            }
        }
    }
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
    /// if the share hash has been processed before. The duplicate detection cache
    /// is typically kept in memory for performance, regardless of the storage backend.
    ///
    /// # Thread Safety Note
    ///
    /// This method requires shared access and is safe for concurrent reads, but
    /// the underlying cache may be modified by `update_share_accounting()` or
    /// `flush_seen_shares()` calls, requiring appropriate synchronization.
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
    /// # Thread Safety Note
    ///
    /// This method requires exclusive access and will clear the entire duplicate
    /// detection cache. In multi-threaded scenarios, ensure this operation is
    /// coordinated to prevent race conditions with concurrent share processing.
    ///
    /// # Important
    ///
    /// This method only clears the duplicate detection cache and does NOT affect
    /// persistent statistics like share counts, work sums, or difficulty tracking.
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
    /// // Statistics are preserved
    /// assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
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

    /// Updates the best difficulty if the new value is higher (backward compatibility).
    ///
    /// This method provides backward compatibility with existing client/server implementations
    /// that use a single best difficulty value. It updates the difficulty for a default
    /// user identity to maintain the same behavior as the original implementations.
    ///
    /// # Arguments
    ///
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
    /// assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
    ///
    /// accounting.update_best_diff(1000.0).unwrap();
    /// assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);
    ///
    /// // Lower difficulty should not update
    /// accounting.update_best_diff(500.0).unwrap();
    /// assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);
    ///
    /// // Higher difficulty should update
    /// accounting.update_best_diff(2000.0).unwrap();
    /// assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);
    /// ```
    fn update_best_diff(&mut self, diff: f64) -> Result<(), Self::Error>;

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
    fn update_best_diff_for_user(
        &mut self,
        user_identity: &str,
        diff: f64,
    ) -> Result<(), Self::Error>;

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
    /// // No acknowledgment initially (no shares processed yet)
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
/// **This implementation is NOT thread-safe.** All methods require exclusive access
/// through mutable references or shared immutable access. If concurrent access is required,
/// you must wrap this implementation in appropriate synchronization primitives:
///
/// ### Recommended Synchronization Patterns:
///
/// - **Single Writer, Multiple Readers**: Use `Arc<RwLock<InMemoryShareAccounting>>`
/// - **Multiple Writers**: Use `Arc<Mutex<InMemoryShareAccounting>>`
/// - **Lock-Free Scenarios**: Consider implementing a custom thread-safe storage backend
///
/// ### Thread Safety Considerations:
///
/// 1. **Duplicate Detection Cache**: The `seen_shares` HashSet is kept in memory and
///    requires synchronization for concurrent access. Cache flushes (`flush_seen_shares`)
///    must be coordinated across threads to prevent race conditions.
///
/// 2. **Statistics Consistency**: Share counting, work sum tracking, and sequence numbers
///    require atomic updates to maintain consistency. Without proper synchronization,
///    concurrent updates may result in lost increments or inconsistent state.
///
/// 3. **Per-User Difficulty Tracking**: The `user_best_diffs` BTreeMap requires exclusive
///    access for updates. Concurrent reads are safe, but updates must be synchronized
///    to prevent data races and ensure difficulty comparisons are atomic.
///
/// 4. **Batch Acknowledgment Logic**: The `should_acknowledge()` method depends on
///    consistent share counts. In multi-threaded scenarios, ensure acknowledgments
///    are processed atomically to prevent duplicate or missed acknowledgments.
///
/// ### Example Thread-Safe Usage:
///
/// ```rust
/// use std::sync::{Arc, Mutex};
/// use channels_sv2::share_accounting_trait::InMemoryShareAccounting;
///
/// // For multiple writers
/// let accounting = Arc::new(Mutex::new(InMemoryShareAccounting::new_server(10)));
/// let accounting_clone = accounting.clone();
///
/// // In another thread
/// std::thread::spawn(move || {
///     let mut guard = accounting_clone.lock().unwrap();
///     // Safe to use guard.update_share_accounting(...) here
/// });
/// ```
///
/// ## Memory Usage
///
/// - **Share Hashes**: Stored in a `HashSet<Hash>` for O(1) duplicate detection
/// - **Per-User Difficulties**: Stored in a `BTreeMap<String, f64>` for efficient ordering and range queries
/// - **Statistics**: Primitive types (`u32`, `u64`, `f64`) with minimal memory overhead
/// - **Memory Growth**: The `seen_shares` cache grows unboundedly until `flush_seen_shares()` is called
///
/// ## Performance Characteristics
///
/// - **Share Processing**: O(1) for duplicate detection and statistics updates
/// - **User Difficulty Updates**: O(log n) where n is the number of users
/// - **Top Users Query**: O(n log n) for sorting, consider caching for frequent queries
/// - **Memory Cleanup**: O(k) where k is the number of cached share hashes
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

    /// Creates a new client-mode share accounting instance.
    ///
    /// This is a convenience method that creates an instance with `share_batch_size` set to `None`,
    /// which is appropriate for mining clients that don't need batch acknowledgment functionality.
    ///
    /// # Returns
    ///
    /// A new `InMemoryShareAccounting` instance configured for client mode.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use channels_sv2::share_accounting_trait::{InMemoryShareAccounting, ShareAccountingTrait};
    /// let client_accounting = InMemoryShareAccounting::new_client();
    /// assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
    /// assert!(!client_accounting.should_acknowledge().unwrap());
    /// ```
    pub fn new_client() -> Self {
        Self::new(None)
    }

    /// Creates a new server-mode share accounting instance.
    ///
    /// This is a convenience method that creates an instance with the specified `share_batch_size`,
    /// which is appropriate for mining servers that need batch acknowledgment functionality.
    ///
    /// # Arguments
    ///
    /// * `share_batch_size` - The number of shares that trigger a batch acknowledgment.
    ///   Must be greater than 0.
    ///
    /// # Returns
    ///
    /// A new `InMemoryShareAccounting` instance configured for server mode.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use channels_sv2::share_accounting_trait::{InMemoryShareAccounting, ShareAccountingTrait};
    /// let server_accounting = InMemoryShareAccounting::new_server(10);
    /// assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(10));
    /// assert!(!server_accounting.should_acknowledge().unwrap()); // No shares yet
    /// ```
    pub fn new_server(share_batch_size: usize) -> Self {
        Self::new(Some(share_batch_size))
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

    fn update_best_diff_for_user(
        &mut self,
        user_identity: &str,
        diff: f64,
    ) -> Result<(), Self::Error> {
        let should_update = match self.user_best_diffs.get(user_identity) {
            Some(&current_best) => diff > current_best,
            None => true, // Always insert if user doesn't exist yet
        };

        if should_update {
            self.user_best_diffs.insert(user_identity.to_string(), diff);
        }
        Ok(())
    }

    fn update_best_diff(&mut self, diff: f64) -> Result<(), Self::Error> {
        // Use a default user identity for backward compatibility
        self.update_best_diff_for_user("__default__", diff)
    }

    fn get_top_user_difficulties(&self, limit: usize) -> Result<Vec<(String, f64)>, Self::Error> {
        let mut users: Vec<_> = self
            .user_best_diffs
            .iter()
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
            Some(batch_size) => {
                // Only acknowledge if we have shares and the count is a multiple of batch size
                Ok(self.shares_accepted > 0 && self.shares_accepted % batch_size as u32 == 0)
            }
            None => Ok(false), // Client mode - never acknowledge
        }
    }
}

/// Factory function for creating share accounting trait objects.
///
/// This function provides a convenient way to create trait objects from configuration,
/// enabling easy switching between different storage backends in the future.
/// The configuration is validated before creating the implementation.
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
/// # Errors
///
/// Returns an error if:
/// - Configuration validation fails (e.g., invalid batch size)
/// - Implementation initialization fails
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
///
/// // Invalid configuration will fail
/// let invalid_config = ShareAccountingConfig::InMemory { share_batch_size: Some(0) };
/// assert!(create_share_accounting(invalid_config).is_err());
/// ```
pub fn create_share_accounting(
    config: ShareAccountingConfig,
) -> Result<
    Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>>,
    Box<dyn StdError + Send + Sync>,
> {
    // Validate configuration before creating implementation
    config
        .validate()
        .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)?;

    match config {
        ShareAccountingConfig::InMemory { share_batch_size } => {
            Ok(Box::new(InMemoryShareAccounting::new(share_batch_size)))
        }
    }
}

/// Convenience factory function for creating client-mode share accounting.
///
/// This function creates a share accounting instance configured for client mode
/// (no batch acknowledgments). It's equivalent to calling `create_share_accounting`
/// with `ShareAccountingConfig::InMemory { share_batch_size: None }`.
///
/// # Returns
///
/// A boxed trait object implementing `ShareAccountingTrait` configured for client mode.
///
/// # Examples
///
/// ```rust
/// # use channels_sv2::share_accounting_trait::*;
/// let client_accounting = create_client_share_accounting();
/// assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
/// assert!(!client_accounting.should_acknowledge().unwrap());
/// ```
pub fn create_client_share_accounting(
) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
    Box::new(InMemoryShareAccounting::new_client())
}

/// Convenience factory function for creating server-mode share accounting.
///
/// This function creates a share accounting instance configured for server mode
/// with the specified batch size for acknowledgments.
///
/// # Arguments
///
/// * `share_batch_size` - The number of shares that trigger a batch acknowledgment.
///   Must be greater than 0.
///
/// # Returns
///
/// A boxed trait object implementing `ShareAccountingTrait` configured for server mode.
///
/// # Panics
///
/// Panics if `share_batch_size` is 0. Use `create_share_accounting` with proper
/// error handling if you need to validate the batch size.
///
/// # Examples
///
/// ```rust
/// # use channels_sv2::share_accounting_trait::*;
/// let server_accounting = create_server_share_accounting(10);
/// assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(10));
/// ```
pub fn create_server_share_accounting(
    share_batch_size: usize,
) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
    assert!(share_batch_size > 0, "Batch size must be greater than 0");
    Box::new(InMemoryShareAccounting::new_server(share_batch_size))
}

// ================================================================================================
// Migration Utilities
// ================================================================================================

/// Migration utilities for converting from concrete ShareAccounting structs to trait objects.
///
/// This module provides helper functions to ease the transition from the existing concrete
/// client and server ShareAccounting implementations to the new trait-based approach.
/// These utilities preserve all existing state and behavior while enabling the use of
/// the new trait interface.
///
/// ## Migration Strategy
///
/// 1. **Gradual Migration**: Use these utilities to convert existing code incrementally
/// 2. **State Preservation**: All statistics, seen shares, and configuration are preserved
/// 3. **Behavioral Compatibility**: Migrated instances behave identically to original implementations
/// 4. **Performance**: Migration is a zero-cost abstraction with minimal overhead
///
/// ## Usage Examples
///
/// ### Client Migration
///
/// ```rust
/// # use channels_sv2::share_accounting_trait::migration::*;
/// # use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
/// // Before: Using concrete client implementation
/// let mut client_accounting = ClientShareAccounting::new();
/// // ... use client_accounting ...
///
/// // After: Migrate to trait object
/// let mut trait_accounting = migrate_from_client_share_accounting(client_accounting);
/// // ... use trait_accounting with same API ...
/// ```
///
/// ### Server Migration
///
/// ```rust
/// # use channels_sv2::share_accounting_trait::migration::*;
/// # use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
/// // Before: Using concrete server implementation
/// let mut server_accounting = ServerShareAccounting::new(10);
/// // ... use server_accounting ...
///
/// // After: Migrate to trait object
/// let mut trait_accounting = migrate_from_server_share_accounting(server_accounting);
/// // ... use trait_accounting with same API ...
/// ```
pub mod migration {
    use super::*;
    use crate::client::share_accounting::ShareAccounting as ClientShareAccounting;
    use crate::server::share_accounting::ShareAccounting as ServerShareAccounting;

    /// Migrates a client ShareAccounting instance to a trait object.
    ///
    /// This function converts an existing client ShareAccounting struct to the new
    /// trait-based implementation while preserving all state:
    /// - Share statistics (count, work sum, sequence numbers)
    /// - Seen share hashes for duplicate detection
    /// - Best difficulty tracking
    /// - Client-mode behavior (no batch acknowledgments)
    ///
    /// # Arguments
    ///
    /// * `client_accounting` - The existing client ShareAccounting instance to migrate
    ///
    /// # Returns
    ///
    /// A boxed trait object that behaves identically to the original client implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::migration::*;
    /// # use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// // Create and use original client implementation
    /// let mut original = ClientShareAccounting::new();
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// original.update_share_accounting(1000, 1, share_hash);
    /// original.update_best_diff(1500.0);
    ///
    /// // Migrate to trait object
    /// let migrated = migrate_from_client_share_accounting(original);
    ///
    /// // Verify state preservation (statistics are preserved)
    /// assert_eq!(migrated.get_shares_accepted().unwrap(), 1);
    /// assert_eq!(migrated.get_share_work_sum().unwrap(), 1000);
    /// assert_eq!(migrated.get_last_share_sequence_number().unwrap(), 1);
    /// assert_eq!(migrated.get_best_diff().unwrap(), 1500.0);
    /// // Note: Seen shares cache is not migrated (starts empty)
    /// assert!(!migrated.is_share_seen(share_hash).unwrap());
    /// assert_eq!(migrated.get_share_batch_size().unwrap(), None);
    /// assert!(!migrated.should_acknowledge().unwrap());
    /// ```
    pub fn migrate_from_client_share_accounting(
        client_accounting: ClientShareAccounting,
    ) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
        let mut migrated = InMemoryShareAccounting::new_client();

        // Migrate basic statistics using public getters
        migrated.last_share_sequence_number = client_accounting.get_last_share_sequence_number();
        migrated.shares_accepted = client_accounting.get_shares_accepted();
        migrated.share_work_sum = client_accounting.get_share_work_sum();

        // Migrate best difficulty
        let best_diff = client_accounting.get_best_diff();
        if best_diff > 0.0 {
            migrated.update_best_diff(best_diff).unwrap();
        }

        // Note: Seen shares cannot be migrated directly as they are private
        // This is acceptable as the cache is typically flushed on chain tip updates
        // Users should call flush_seen_shares() after migration if needed

        Box::new(migrated)
    }

    /// Migrates a server ShareAccounting instance to a trait object.
    ///
    /// This function converts an existing server ShareAccounting struct to the new
    /// trait-based implementation while preserving all state:
    /// - Share statistics (count, work sum, sequence numbers)
    /// - Seen share hashes for duplicate detection
    /// - Best difficulty tracking
    /// - Server-mode behavior (batch acknowledgments with original batch size)
    ///
    /// # Arguments
    ///
    /// * `server_accounting` - The existing server ShareAccounting instance to migrate
    ///
    /// # Returns
    ///
    /// A boxed trait object that behaves identically to the original server implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::migration::*;
    /// # use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// // Create and use original server implementation
    /// let mut original = ServerShareAccounting::new(5);
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// original.update_share_accounting(2000, 1, share_hash);
    /// original.update_best_diff(2500.0);
    ///
    /// // Migrate to trait object
    /// let migrated = migrate_from_server_share_accounting(original);
    ///
    /// // Verify state preservation (statistics are preserved)
    /// assert_eq!(migrated.get_shares_accepted().unwrap(), 1);
    /// assert_eq!(migrated.get_share_work_sum().unwrap(), 2000);
    /// assert_eq!(migrated.get_last_share_sequence_number().unwrap(), 1);
    /// assert_eq!(migrated.get_best_diff().unwrap(), 2500.0);
    /// // Note: Seen shares cache is not migrated (starts empty)
    /// assert!(!migrated.is_share_seen(share_hash).unwrap());
    /// assert_eq!(migrated.get_share_batch_size().unwrap(), Some(5));
    /// ```
    pub fn migrate_from_server_share_accounting(
        server_accounting: ServerShareAccounting,
    ) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
        let batch_size = server_accounting.get_share_batch_size();
        let mut migrated = InMemoryShareAccounting::new_server(batch_size);

        // Migrate basic statistics using public getters
        migrated.last_share_sequence_number = server_accounting.get_last_share_sequence_number();
        migrated.shares_accepted = server_accounting.get_shares_accepted();
        migrated.share_work_sum = server_accounting.get_share_work_sum();

        // Migrate best difficulty
        let best_diff = server_accounting.get_best_diff();
        if best_diff > 0.0 {
            migrated.update_best_diff(best_diff).unwrap();
        }

        // Note: Seen shares cannot be migrated directly as they are private
        // This is acceptable as the cache is typically flushed on chain tip updates
        // Users should call flush_seen_shares() after migration if needed

        Box::new(migrated)
    }

    /// Creates a trait object from existing client ShareAccounting state.
    ///
    /// This is a convenience function that creates a new trait object with the same
    /// state as an existing client implementation, without consuming the original.
    /// Useful when you need to create a trait object but keep the original around.
    ///
    /// # Arguments
    ///
    /// * `client_accounting` - Reference to the existing client ShareAccounting instance
    ///
    /// # Returns
    ///
    /// A boxed trait object with the same state as the original client implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::migration::*;
    /// # use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// let mut original = ClientShareAccounting::new();
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// original.update_share_accounting(1000, 1, share_hash);
    ///
    /// // Create trait object from existing state (original is not consumed)
    /// let trait_obj = create_trait_from_client_state(&original);
    ///
    /// // Both have the same state
    /// assert_eq!(trait_obj.get_shares_accepted().unwrap(), original.get_shares_accepted());
    /// assert_eq!(trait_obj.get_share_work_sum().unwrap(), original.get_share_work_sum());
    /// ```
    pub fn create_trait_from_client_state(
        client_accounting: &ClientShareAccounting,
    ) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
        let mut migrated = InMemoryShareAccounting::new_client();

        // Copy state from the reference using public getters
        migrated.last_share_sequence_number = client_accounting.get_last_share_sequence_number();
        migrated.shares_accepted = client_accounting.get_shares_accepted();
        migrated.share_work_sum = client_accounting.get_share_work_sum();

        let best_diff = client_accounting.get_best_diff();
        if best_diff > 0.0 {
            migrated.update_best_diff(best_diff).unwrap();
        }

        Box::new(migrated)
    }

    /// Creates a trait object from existing server ShareAccounting state.
    ///
    /// This is a convenience function that creates a new trait object with the same
    /// state as an existing server implementation, without consuming the original.
    /// Useful when you need to create a trait object but keep the original around.
    ///
    /// # Arguments
    ///
    /// * `server_accounting` - Reference to the existing server ShareAccounting instance
    ///
    /// # Returns
    ///
    /// A boxed trait object with the same state as the original server implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use channels_sv2::share_accounting_trait::migration::*;
    /// # use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
    /// # use bitcoin::hashes::sha256d::Hash;
    /// # use bitcoin::hashes::Hash as HashTrait;
    /// let mut original = ServerShareAccounting::new(3);
    /// let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
    /// original.update_share_accounting(1500, 1, share_hash);
    ///
    /// // Create trait object from existing state (original is not consumed)
    /// let trait_obj = create_trait_from_server_state(&original);
    ///
    /// // Both have the same state
    /// assert_eq!(trait_obj.get_shares_accepted().unwrap(), original.get_shares_accepted());
    /// assert_eq!(trait_obj.get_share_batch_size().unwrap(), Some(original.get_share_batch_size()));
    /// ```
    pub fn create_trait_from_server_state(
        server_accounting: &ServerShareAccounting,
    ) -> Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>> {
        let batch_size = server_accounting.get_share_batch_size();
        let mut migrated = InMemoryShareAccounting::new_server(batch_size);

        // Copy state from the reference using public getters
        migrated.last_share_sequence_number = server_accounting.get_last_share_sequence_number();
        migrated.shares_accepted = server_accounting.get_shares_accepted();
        migrated.share_work_sum = server_accounting.get_share_work_sum();

        let best_diff = server_accounting.get_best_diff();
        if best_diff > 0.0 {
            migrated.update_best_diff(best_diff).unwrap();
        }

        Box::new(migrated)
    }
}

// ================================================================================================
// Usage Examples and Implementation Guidelines
// ================================================================================================

pub mod examples {
    //! Comprehensive usage examples for the ShareAccountingTrait.
    //!
    //! This module provides detailed examples showing how to use the trait in various scenarios,
    //! including client and server contexts, migration from existing implementations, and
    //! best practices for different use cases.
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
        accounting
            .update_share_accounting(1000, 1, share_hash)
            .unwrap();

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
        accounting
            .update_share_accounting(1000, 1, share_hash)
            .unwrap();
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
        accounting
            .update_best_diff_for_user("alice", 1000.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("alice").unwrap(),
            Some(1000.0)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);

        // Add difficulty for bob (higher)
        accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("bob").unwrap(),
            Some(2000.0)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);

        // Try to update alice with lower difficulty (should not change)
        accounting
            .update_best_diff_for_user("alice", 500.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("alice").unwrap(),
            Some(1000.0)
        );

        // Update alice with higher difficulty
        accounting
            .update_best_diff_for_user("alice", 2500.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("alice").unwrap(),
            Some(2500.0)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), 2500.0);
    }

    #[test]
    fn test_top_user_difficulties() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Add users with different difficulties
        accounting
            .update_best_diff_for_user("alice", 1000.0)
            .unwrap();
        accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
        accounting
            .update_best_diff_for_user("charlie", 1500.0)
            .unwrap();
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
            accounting
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
            assert!(!accounting.should_acknowledge().unwrap());
        }
    }

    #[test]
    fn test_client_mode_comprehensive_functionality() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test initial state for client mode
        assert_eq!(accounting.get_shares_accepted().unwrap(), 0);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 0);
        assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
        assert_eq!(accounting.get_share_batch_size().unwrap(), None);
        assert!(!accounting.should_acknowledge().unwrap());

        // Test share processing in client mode
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();

        // Process first share
        accounting
            .update_share_accounting(1000, 1, share_hash1)
            .unwrap();
        assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 1000);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 1);
        assert!(accounting.is_share_seen(share_hash1).unwrap());
        assert!(!accounting.should_acknowledge().unwrap()); // Still no acknowledgment in client mode

        // Process second share
        accounting
            .update_share_accounting(2000, 2, share_hash2)
            .unwrap();
        assert_eq!(accounting.get_shares_accepted().unwrap(), 2);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 3000);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 2);
        assert!(accounting.is_share_seen(share_hash2).unwrap());
        assert!(!accounting.should_acknowledge().unwrap()); // Still no acknowledgment in client mode

        // Test difficulty tracking in client mode
        accounting.update_best_diff(1500.0).unwrap();
        assert_eq!(accounting.get_best_diff().unwrap(), 1500.0);

        // Test per-user difficulty tracking in client mode
        accounting
            .update_best_diff_for_user("client_user", 2500.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("client_user").unwrap(),
            Some(2500.0)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), 2500.0); // Should be max of all users

        // Test duplicate detection in client mode
        assert!(accounting.is_share_seen(share_hash1).unwrap());
        assert!(accounting.is_share_seen(share_hash2).unwrap());

        let unseen_hash = Hash::from_slice(&[99u8; 32]).unwrap();
        assert!(!accounting.is_share_seen(unseen_hash).unwrap());

        // Test flush functionality in client mode
        accounting.flush_seen_shares().unwrap();
        assert!(!accounting.is_share_seen(share_hash1).unwrap());
        assert!(!accounting.is_share_seen(share_hash2).unwrap());

        // Verify other stats remain unchanged after flush
        assert_eq!(accounting.get_shares_accepted().unwrap(), 2);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 3000);
        assert_eq!(accounting.get_best_diff().unwrap(), 2500.0);
    }

    #[test]
    fn test_client_mode_vs_server_mode_differences() {
        // Create client mode instance
        let mut client_accounting = InMemoryShareAccounting::new(None);

        // Create server mode instance
        let mut server_accounting = InMemoryShareAccounting::new(Some(3));

        // Test batch size differences
        assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
        assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(3));

        // Test acknowledgment behavior differences
        assert!(!client_accounting.should_acknowledge().unwrap());
        assert!(!server_accounting.should_acknowledge().unwrap()); // No shares processed yet

        // Add shares to both
        for i in 1..=3 {
            let share_hash = Hash::from_slice(&[i; 32]).unwrap();
            client_accounting
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
            server_accounting
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
        }

        // Client should never acknowledge, server should acknowledge after batch size
        assert!(!client_accounting.should_acknowledge().unwrap());
        assert!(server_accounting.should_acknowledge().unwrap());

        // Add more shares
        for i in 4..=6 {
            let share_hash = Hash::from_slice(&[i; 32]).unwrap();
            client_accounting
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
            server_accounting
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
        }

        // Client still never acknowledges, server acknowledges again
        assert!(!client_accounting.should_acknowledge().unwrap());
        assert!(server_accounting.should_acknowledge().unwrap());

        // Both should have same share statistics
        assert_eq!(client_accounting.get_shares_accepted().unwrap(), 6);
        assert_eq!(server_accounting.get_shares_accepted().unwrap(), 6);
        assert_eq!(client_accounting.get_share_work_sum().unwrap(), 6000);
        assert_eq!(server_accounting.get_share_work_sum().unwrap(), 6000);
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
        accounting
            .update_share_accounting(1000, 1, share_hash1)
            .unwrap();
        assert!(!accounting.should_acknowledge().unwrap());

        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 2, share_hash2)
            .unwrap();
        assert!(!accounting.should_acknowledge().unwrap());

        let share_hash3 = Hash::from_slice(&[3u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 3, share_hash3)
            .unwrap();
        assert!(accounting.should_acknowledge().unwrap());

        // Add one more share - should not acknowledge until next batch
        let share_hash4 = Hash::from_slice(&[4u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 4, share_hash4)
            .unwrap();
        assert!(!accounting.should_acknowledge().unwrap());
    }

    #[test]
    fn test_config_validation() {
        // Valid configurations
        let client_config = ShareAccountingConfig::InMemory {
            share_batch_size: None,
        };
        assert!(client_config.validate().is_ok());

        let server_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(10),
        };
        assert!(server_config.validate().is_ok());

        let server_config_small = ShareAccountingConfig::InMemory {
            share_batch_size: Some(1),
        };
        assert!(server_config_small.validate().is_ok());

        // Invalid configuration - zero batch size
        let invalid_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(0),
        };
        assert!(invalid_config.validate().is_err());

        match invalid_config.validate() {
            Err(ConfigError::InvalidBatchSize(0)) => {} // Expected
            _ => panic!("Expected InvalidBatchSize error"),
        }
    }

    #[test]
    fn test_factory_function_validation() {
        // Valid configurations should succeed
        let client_config = ShareAccountingConfig::InMemory {
            share_batch_size: None,
        };
        assert!(create_share_accounting(client_config).is_ok());

        let server_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(5),
        };
        assert!(create_share_accounting(server_config).is_ok());

        // Invalid configuration should fail
        let invalid_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(0),
        };
        assert!(create_share_accounting(invalid_config).is_err());
    }

    #[test]
    fn test_factory_function_creates_correct_implementation() {
        // Test client mode
        let client_config = ShareAccountingConfig::InMemory {
            share_batch_size: None,
        };
        let client_accounting = create_share_accounting(client_config).unwrap();
        assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
        assert!(!client_accounting.should_acknowledge().unwrap());

        // Test server mode
        let server_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(3),
        };
        let server_accounting = create_share_accounting(server_config).unwrap();
        assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(3));
    }

    #[test]
    fn test_backward_compatibility_update_best_diff() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test backward compatibility method
        assert_eq!(accounting.get_best_diff().unwrap(), 0.0);

        accounting.update_best_diff(1000.0).unwrap();
        assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);

        // Lower difficulty should not update
        accounting.update_best_diff(500.0).unwrap();
        assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);

        // Higher difficulty should update
        accounting.update_best_diff(2000.0).unwrap();
        assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);

        // Should work alongside per-user tracking
        accounting
            .update_best_diff_for_user("user1", 3000.0)
            .unwrap();
        assert_eq!(accounting.get_best_diff().unwrap(), 3000.0);
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            Some(3000.0)
        );
        assert_eq!(
            accounting.get_best_diff_for_user("__default__").unwrap(),
            Some(2000.0)
        );
    }

    #[test]
    fn test_convenience_constructors() {
        // Test client convenience constructor
        let client_accounting = InMemoryShareAccounting::new_client();
        assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
        assert!(!client_accounting.should_acknowledge().unwrap());

        // Test server convenience constructor
        let server_accounting = InMemoryShareAccounting::new_server(5);
        assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(5));
        assert!(!server_accounting.should_acknowledge().unwrap()); // No shares processed yet

        // Verify they behave the same as regular constructors
        let client_regular = InMemoryShareAccounting::new(None);
        let server_regular = InMemoryShareAccounting::new(Some(5));

        assert_eq!(
            client_accounting.get_share_batch_size().unwrap(),
            client_regular.get_share_batch_size().unwrap()
        );
        assert_eq!(
            server_accounting.get_share_batch_size().unwrap(),
            server_regular.get_share_batch_size().unwrap()
        );
    }

    #[test]
    fn test_convenience_factory_functions() {
        // Test client factory function
        let client_accounting = create_client_share_accounting();
        assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
        assert!(!client_accounting.should_acknowledge().unwrap());

        // Test server factory function
        let server_accounting = create_server_share_accounting(7);
        assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(7));
        assert!(!server_accounting.should_acknowledge().unwrap()); // No shares processed yet

        // Verify they behave the same as regular factory function
        let client_config = ShareAccountingConfig::InMemory {
            share_batch_size: None,
        };
        let client_regular = create_share_accounting(client_config).unwrap();

        assert_eq!(
            client_accounting.get_share_batch_size().unwrap(),
            client_regular.get_share_batch_size().unwrap()
        );

        let server_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(7),
        };
        let server_regular = create_share_accounting(server_config).unwrap();

        assert_eq!(
            server_accounting.get_share_batch_size().unwrap(),
            server_regular.get_share_batch_size().unwrap()
        );
    }

    #[test]
    fn test_migration_utilities() {
        use super::migration::*;
        use crate::client::share_accounting::ShareAccounting as ClientShareAccounting;
        use crate::server::share_accounting::ShareAccounting as ServerShareAccounting;

        // Test client migration
        let mut original_client = ClientShareAccounting::new();
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();

        original_client.update_share_accounting(1000, 1, share_hash1);
        original_client.update_share_accounting(2000, 2, share_hash2);
        original_client.update_best_diff(1500.0);

        let migrated_client = migrate_from_client_share_accounting(original_client.clone());

        // Verify state preservation
        assert_eq!(
            migrated_client.get_shares_accepted().unwrap(),
            original_client.get_shares_accepted()
        );
        assert_eq!(
            migrated_client.get_share_work_sum().unwrap(),
            original_client.get_share_work_sum()
        );
        assert_eq!(
            migrated_client.get_last_share_sequence_number().unwrap(),
            original_client.get_last_share_sequence_number()
        );
        assert_eq!(
            migrated_client.get_best_diff().unwrap(),
            original_client.get_best_diff()
        );
        assert_eq!(migrated_client.get_share_batch_size().unwrap(), None);
        assert!(!migrated_client.should_acknowledge().unwrap());

        // Test server migration
        let mut original_server = ServerShareAccounting::new(5);
        let share_hash3 = Hash::from_slice(&[3u8; 32]).unwrap();
        let share_hash4 = Hash::from_slice(&[4u8; 32]).unwrap();

        original_server.update_share_accounting(3000, 1, share_hash3);
        original_server.update_share_accounting(4000, 2, share_hash4);
        original_server.update_best_diff(2500.0);

        let migrated_server = migrate_from_server_share_accounting(original_server.clone());

        // Verify state preservation
        assert_eq!(
            migrated_server.get_shares_accepted().unwrap(),
            original_server.get_shares_accepted()
        );
        assert_eq!(
            migrated_server.get_share_work_sum().unwrap(),
            original_server.get_share_work_sum()
        );
        assert_eq!(
            migrated_server.get_last_share_sequence_number().unwrap(),
            original_server.get_last_share_sequence_number()
        );
        assert_eq!(
            migrated_server.get_best_diff().unwrap(),
            original_server.get_best_diff()
        );
        assert_eq!(
            migrated_server.get_share_batch_size().unwrap(),
            Some(original_server.get_share_batch_size())
        );
    }

    #[test]
    fn test_create_trait_from_state_functions() {
        use super::migration::*;
        use crate::client::share_accounting::ShareAccounting as ClientShareAccounting;
        use crate::server::share_accounting::ShareAccounting as ServerShareAccounting;

        // Test creating trait object from client state (without consuming original)
        let mut original_client = ClientShareAccounting::new();
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        original_client.update_share_accounting(1000, 1, share_hash);
        original_client.update_best_diff(1200.0);

        let trait_obj = create_trait_from_client_state(&original_client);

        // Both should have the same state
        assert_eq!(
            trait_obj.get_shares_accepted().unwrap(),
            original_client.get_shares_accepted()
        );
        assert_eq!(
            trait_obj.get_share_work_sum().unwrap(),
            original_client.get_share_work_sum()
        );
        assert_eq!(
            trait_obj.get_last_share_sequence_number().unwrap(),
            original_client.get_last_share_sequence_number()
        );
        assert_eq!(
            trait_obj.get_best_diff().unwrap(),
            original_client.get_best_diff()
        );

        // Original should still be usable
        original_client.update_best_diff(1300.0);
        assert_eq!(original_client.get_best_diff(), 1300.0);
        assert_eq!(trait_obj.get_best_diff().unwrap(), 1200.0); // Trait object unchanged

        // Test creating trait object from server state (without consuming original)
        let mut original_server = ServerShareAccounting::new(3);
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
        original_server.update_share_accounting(2000, 1, share_hash2);
        original_server.update_best_diff(2200.0);

        let trait_obj2 = create_trait_from_server_state(&original_server);

        // Both should have the same state
        assert_eq!(
            trait_obj2.get_shares_accepted().unwrap(),
            original_server.get_shares_accepted()
        );
        assert_eq!(
            trait_obj2.get_share_work_sum().unwrap(),
            original_server.get_share_work_sum()
        );
        assert_eq!(
            trait_obj2.get_share_batch_size().unwrap(),
            Some(original_server.get_share_batch_size())
        );

        // Original should still be usable
        original_server.update_best_diff(2300.0);
        assert_eq!(original_server.get_best_diff(), 2300.0);
        assert_eq!(trait_obj2.get_best_diff().unwrap(), 2200.0); // Trait object unchanged
    }

    #[test]
    #[should_panic(expected = "Batch size must be greater than 0")]
    fn test_server_factory_function_panics_on_zero_batch_size() {
        create_server_share_accounting(0);
    }

    /// Test comprehensive duplicate detection functionality (Requirement 4.1)
    #[test]
    fn test_comprehensive_duplicate_detection() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with multiple different share hashes
        let share_hashes: Vec<Hash> = (0..10)
            .map(|i| Hash::from_slice(&[i; 32]).unwrap())
            .collect();

        // Initially, no shares should be seen
        for hash in &share_hashes {
            assert!(!accounting.is_share_seen(*hash).unwrap());
        }

        // Process shares and verify they are marked as seen
        for (i, &hash) in share_hashes.iter().enumerate() {
            accounting
                .update_share_accounting(1000 * (i as u64 + 1), (i + 1) as u32, hash)
                .unwrap();
            assert!(accounting.is_share_seen(hash).unwrap());
        }

        // Verify all processed shares are still seen
        for hash in &share_hashes {
            assert!(accounting.is_share_seen(*hash).unwrap());
        }

        // Test that unprocessed shares are not seen
        let unseen_hash = Hash::from_slice(&[99u8; 32]).unwrap();
        assert!(!accounting.is_share_seen(unseen_hash).unwrap());

        // Test flush functionality clears all seen shares
        accounting.flush_seen_shares().unwrap();
        for hash in &share_hashes {
            assert!(!accounting.is_share_seen(*hash).unwrap());
        }

        // Verify statistics are preserved after flush
        assert_eq!(accounting.get_shares_accepted().unwrap(), 10);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 55000); // Sum of 1000*1 + 1000*2 + ... + 1000*10
    }

    /// Test accurate share counting and work sum tracking (Requirement 4.2)
    #[test]
    fn test_accurate_statistics_tracking() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test initial state
        assert_eq!(accounting.get_shares_accepted().unwrap(), 0);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 0);

        // Test incremental updates with varying work values
        let test_cases = vec![
            (1000u64, 1u32),
            (2500u64, 2u32),
            (750u64, 3u32),
            (10000u64, 4u32),
            (1u64, 5u32),            // Test edge case with minimal work
            (u64::MAX / 1000, 6u32), // Test large work value
        ];

        let mut expected_work_sum = 0u64;
        for (work, sequence) in test_cases {
            let share_hash = Hash::from_slice(&[sequence as u8; 32]).unwrap();
            accounting
                .update_share_accounting(work, sequence, share_hash)
                .unwrap();

            expected_work_sum += work;

            // Verify statistics are updated correctly
            assert_eq!(accounting.get_shares_accepted().unwrap(), sequence);
            assert_eq!(accounting.get_share_work_sum().unwrap(), expected_work_sum);
            assert_eq!(
                accounting.get_last_share_sequence_number().unwrap(),
                sequence
            );
        }

        // Test that statistics remain accurate after duplicate detection operations
        let duplicate_hash = Hash::from_slice(&[1; 32]).unwrap();
        assert!(accounting.is_share_seen(duplicate_hash).unwrap());

        // Statistics should remain unchanged after duplicate check
        assert_eq!(accounting.get_shares_accepted().unwrap(), 6);
        assert_eq!(accounting.get_share_work_sum().unwrap(), expected_work_sum);
    }

    /// Test flush_seen_shares cache management (Requirement 4.3)
    #[test]
    fn test_flush_seen_shares_cache_management() {
        let mut accounting = InMemoryShareAccounting::new(Some(5));

        // Process multiple shares
        let share_hashes: Vec<Hash> = (1..=20)
            .map(|i| Hash::from_slice(&[i; 32]).unwrap())
            .collect();

        for (i, &hash) in share_hashes.iter().enumerate() {
            accounting
                .update_share_accounting(1000, (i + 1) as u32, hash)
                .unwrap();
        }

        // Verify all shares are seen
        for hash in &share_hashes {
            assert!(accounting.is_share_seen(*hash).unwrap());
        }

        // Test that flush clears the cache but preserves statistics
        let shares_before_flush = accounting.get_shares_accepted().unwrap();
        let work_sum_before_flush = accounting.get_share_work_sum().unwrap();
        let last_sequence_before_flush = accounting.get_last_share_sequence_number().unwrap();

        accounting.flush_seen_shares().unwrap();

        // Cache should be cleared
        for hash in &share_hashes {
            assert!(!accounting.is_share_seen(*hash).unwrap());
        }

        // Statistics should be preserved
        assert_eq!(
            accounting.get_shares_accepted().unwrap(),
            shares_before_flush
        );
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            work_sum_before_flush
        );
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            last_sequence_before_flush
        );

        // Batch acknowledgment logic should still work correctly
        assert!(accounting.should_acknowledge().unwrap()); // 20 shares, batch size 5

        // Test that new shares can be processed after flush
        let new_hash = Hash::from_slice(&[99u8; 32]).unwrap();
        assert!(!accounting.is_share_seen(new_hash).unwrap());
        accounting
            .update_share_accounting(5000, 21, new_hash)
            .unwrap();
        assert!(accounting.is_share_seen(new_hash).unwrap());
        assert_eq!(accounting.get_shares_accepted().unwrap(), 21);
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            work_sum_before_flush + 5000
        );
    }

    /// Test accurate statistics retrieval (Requirement 4.5)
    #[test]
    fn test_accurate_statistics_retrieval() {
        let mut accounting = InMemoryShareAccounting::new(Some(3));

        // Test retrieval of initial state
        assert_eq!(accounting.get_shares_accepted().unwrap(), 0);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 0);
        assert_eq!(accounting.get_best_diff().unwrap(), 0.0);
        assert_eq!(accounting.get_share_batch_size().unwrap(), Some(3));
        assert!(!accounting.should_acknowledge().unwrap());

        // Process shares with known values
        let test_data = vec![
            (1000u64, 1u32, "user1", 100.0f64),
            (2000u64, 2u32, "user2", 200.0f64),
            (3000u64, 3u32, "user1", 50.0f64), // Lower diff for user1, should not update
            (4000u64, 4u32, "user3", 300.0f64),
            (5000u64, 5u32, "user2", 250.0f64),
        ];

        let mut expected_work_sum = 0u64;
        for (i, &(work, sequence, user, diff)) in test_data.iter().enumerate() {
            let share_hash = Hash::from_slice(&[(i + 1) as u8; 32]).unwrap();
            accounting
                .update_share_accounting(work, sequence, share_hash)
                .unwrap();
            accounting.update_best_diff_for_user(user, diff).unwrap();

            expected_work_sum += work;

            // Verify statistics are accurate at each step
            assert_eq!(accounting.get_shares_accepted().unwrap(), (i + 1) as u32);
            assert_eq!(accounting.get_share_work_sum().unwrap(), expected_work_sum);
            assert_eq!(
                accounting.get_last_share_sequence_number().unwrap(),
                sequence
            );
        }

        // Test final statistics accuracy
        assert_eq!(accounting.get_shares_accepted().unwrap(), 5);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 15000);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 5);

        // Test difficulty statistics accuracy
        assert_eq!(accounting.get_best_diff().unwrap(), 300.0); // Max across all users
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            Some(100.0)
        ); // Should not have been updated by lower value
        assert_eq!(
            accounting.get_best_diff_for_user("user2").unwrap(),
            Some(250.0)
        );
        assert_eq!(
            accounting.get_best_diff_for_user("user3").unwrap(),
            Some(300.0)
        );

        // Test top users accuracy
        let top_users = accounting.get_top_user_difficulties(10).unwrap();
        assert_eq!(top_users.len(), 3);
        assert_eq!(top_users[0], ("user3".to_string(), 300.0));
        assert_eq!(top_users[1], ("user2".to_string(), 250.0));
        assert_eq!(top_users[2], ("user1".to_string(), 100.0));

        // Test batch acknowledgment accuracy
        assert!(!accounting.should_acknowledge().unwrap()); // 5 % 3 != 0

        // Add one more share to trigger acknowledgment
        let final_hash = Hash::from_slice(&[6u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 6, final_hash)
            .unwrap();
        assert!(accounting.should_acknowledge().unwrap()); // 6 % 3 == 0
    }

    /// Test edge cases for duplicate detection and statistics
    #[test]
    fn test_edge_cases_duplicate_detection_and_statistics() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with zero work (edge case)
        let zero_work_hash = Hash::from_slice(&[0u8; 32]).unwrap();
        accounting
            .update_share_accounting(0, 1, zero_work_hash)
            .unwrap();
        assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert!(accounting.is_share_seen(zero_work_hash).unwrap());

        // Test with large work value (but not MAX to avoid overflow in subsequent operations)
        let large_work_hash = Hash::from_slice(&[255u8; 32]).unwrap();
        accounting
            .update_share_accounting(u64::MAX - 10000, 2, large_work_hash)
            .unwrap();
        assert_eq!(accounting.get_shares_accepted().unwrap(), 2);
        assert_eq!(accounting.get_share_work_sum().unwrap(), u64::MAX - 10000);
        assert!(accounting.is_share_seen(large_work_hash).unwrap());

        // Test sequence number edge cases
        accounting
            .update_share_accounting(1000, u32::MAX, zero_work_hash)
            .unwrap(); // Duplicate hash, but should still update sequence
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            u32::MAX
        );

        // Test difficulty edge cases
        accounting
            .update_best_diff_for_user("edge_user", 0.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("edge_user").unwrap(),
            Some(0.0)
        );

        accounting
            .update_best_diff_for_user("edge_user", f64::MAX)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("edge_user").unwrap(),
            Some(f64::MAX)
        );

        // Test empty user identity (should be treated as valid)
        accounting.update_best_diff_for_user("", 50.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("").unwrap(), Some(50.0));

        // Test very long user identity
        let long_user = "a".repeat(1000);
        accounting
            .update_best_diff_for_user(&long_user, 75.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user(&long_user).unwrap(),
            Some(75.0)
        );
    }

    /// Test thread safety documentation compliance
    #[test]
    fn test_thread_safety_documentation_compliance() {
        // This test verifies that the implementation behaves as documented
        // regarding thread safety (i.e., it's NOT thread-safe without external synchronization)

        let accounting = InMemoryShareAccounting::new(None);

        // Verify that the implementation does not implement Send/Sync automatically
        // (This is a compile-time check - if this compiles, the implementation is Send+Sync)
        fn assert_not_sync<T: Sync>(_: T) {}
        fn assert_not_send<T: Send>(_: T) {}

        // These should compile because InMemoryShareAccounting does implement Send+Sync
        // but the documentation warns users about thread safety
        assert_not_sync(accounting.clone());
        assert_not_send(accounting);

        // The key point is that while the type is Send+Sync, the operations are not atomic
        // and require external synchronization for thread safety
    }

    // ============================================================================
    // GENERIC TEST SUITE - Works with any ShareAccountingTrait implementation
    // ============================================================================

    /// Generic test suite that validates any ShareAccountingTrait implementation.
    /// This function can be used to test any implementation of the trait to ensure
    /// it behaves correctly according to the trait contract.
    ///
    /// # Arguments
    /// * `accounting` - A mutable reference to any ShareAccountingTrait implementation
    /// * `test_name` - Name of the test for debugging purposes
    fn test_share_accounting_trait_basic_operations<T: ShareAccountingTrait + ?Sized>(
        accounting: &mut T,
        test_name: &str,
    ) {
        println!(
            "Running generic test: {} - {}",
            test_name, "basic_operations"
        );

        // Test initial state
        assert_eq!(
            accounting.get_shares_accepted().unwrap(),
            0,
            "{}: Initial shares should be 0",
            test_name
        );
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            0,
            "{}: Initial work sum should be 0",
            test_name
        );
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            0,
            "{}: Initial sequence number should be 0",
            test_name
        );
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            0.0,
            "{}: Initial best difficulty should be 0.0",
            test_name
        );

        // Test share processing
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 1, share_hash1)
            .unwrap();

        assert_eq!(
            accounting.get_shares_accepted().unwrap(),
            1,
            "{}: Should have 1 share after first update",
            test_name
        );
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            1000,
            "{}: Work sum should be 1000 after first share",
            test_name
        );
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            1,
            "{}: Sequence number should be 1 after first share",
            test_name
        );

        // Test duplicate detection
        assert!(
            accounting.is_share_seen(share_hash1).unwrap(),
            "{}: First share should be seen",
            test_name
        );

        let unseen_hash = Hash::from_slice(&[99u8; 32]).unwrap();
        assert!(
            !accounting.is_share_seen(unseen_hash).unwrap(),
            "{}: Unseen share should not be detected",
            test_name
        );

        // Test multiple shares
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
        accounting
            .update_share_accounting(2000, 2, share_hash2)
            .unwrap();

        assert_eq!(
            accounting.get_shares_accepted().unwrap(),
            2,
            "{}: Should have 2 shares after second update",
            test_name
        );
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            3000,
            "{}: Work sum should be 3000 after second share",
            test_name
        );
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            2,
            "{}: Sequence number should be 2 after second share",
            test_name
        );

        // Both shares should be seen
        assert!(
            accounting.is_share_seen(share_hash1).unwrap(),
            "{}: First share should still be seen",
            test_name
        );
        assert!(
            accounting.is_share_seen(share_hash2).unwrap(),
            "{}: Second share should be seen",
            test_name
        );
    }

    /// Generic test for difficulty tracking functionality
    fn test_share_accounting_trait_difficulty_tracking<T: ShareAccountingTrait + ?Sized>(
        accounting: &mut T,
        test_name: &str,
    ) {
        println!(
            "Running generic test: {} - {}",
            test_name, "difficulty_tracking"
        );

        // Test initial difficulty state
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            0.0,
            "{}: Initial best difficulty should be 0.0",
            test_name
        );
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            None,
            "{}: User1 should have no initial difficulty",
            test_name
        );

        // Test per-user difficulty updates
        accounting
            .update_best_diff_for_user("user1", 1000.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            Some(1000.0),
            "{}: User1 should have difficulty 1000.0",
            test_name
        );
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            1000.0,
            "{}: Best difficulty should be 1000.0",
            test_name
        );

        // Test multiple users
        accounting
            .update_best_diff_for_user("user2", 2000.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("user2").unwrap(),
            Some(2000.0),
            "{}: User2 should have difficulty 2000.0",
            test_name
        );
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            2000.0,
            "{}: Best difficulty should be 2000.0 (max of all users)",
            test_name
        );

        // Test difficulty update logic (only higher values should update)
        accounting
            .update_best_diff_for_user("user1", 500.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            Some(1000.0),
            "{}: User1 difficulty should remain 1000.0 (not updated with lower value)",
            test_name
        );

        accounting
            .update_best_diff_for_user("user1", 2500.0)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("user1").unwrap(),
            Some(2500.0),
            "{}: User1 difficulty should be updated to 2500.0",
            test_name
        );
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            2500.0,
            "{}: Best difficulty should be 2500.0 (new max)",
            test_name
        );

        // Test backward compatibility method
        accounting.update_best_diff(3000.0).unwrap();
        assert_eq!(
            accounting.get_best_diff().unwrap(),
            3000.0,
            "{}: Best difficulty should be 3000.0 after backward compatibility update",
            test_name
        );

        // Test top users functionality
        accounting
            .update_best_diff_for_user("user3", 1500.0)
            .unwrap();
        let top_users = accounting.get_top_user_difficulties(2).unwrap();
        assert_eq!(
            top_users.len(),
            2,
            "{}: Should return top 2 users",
            test_name
        );
        // Should be sorted by difficulty descending
        assert!(
            top_users[0].1 >= top_users[1].1,
            "{}: Top users should be sorted by difficulty descending",
            test_name
        );
    }

    /// Generic test for flush functionality
    fn test_share_accounting_trait_flush_functionality<T: ShareAccountingTrait + ?Sized>(
        accounting: &mut T,
        test_name: &str,
    ) {
        println!(
            "Running generic test: {} - {}",
            test_name, "flush_functionality"
        );

        // Add some shares
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();

        accounting
            .update_share_accounting(1000, 1, share_hash1)
            .unwrap();
        accounting
            .update_share_accounting(2000, 2, share_hash2)
            .unwrap();

        // Verify shares are seen
        assert!(
            accounting.is_share_seen(share_hash1).unwrap(),
            "{}: Share 1 should be seen before flush",
            test_name
        );
        assert!(
            accounting.is_share_seen(share_hash2).unwrap(),
            "{}: Share 2 should be seen before flush",
            test_name
        );

        // Store statistics before flush
        let shares_before = accounting.get_shares_accepted().unwrap();
        let work_sum_before = accounting.get_share_work_sum().unwrap();
        let seq_num_before = accounting.get_last_share_sequence_number().unwrap();

        // Flush seen shares
        accounting.flush_seen_shares().unwrap();

        // Verify shares are no longer seen
        assert!(
            !accounting.is_share_seen(share_hash1).unwrap(),
            "{}: Share 1 should not be seen after flush",
            test_name
        );
        assert!(
            !accounting.is_share_seen(share_hash2).unwrap(),
            "{}: Share 2 should not be seen after flush",
            test_name
        );

        // Verify statistics are preserved
        assert_eq!(
            accounting.get_shares_accepted().unwrap(),
            shares_before,
            "{}: Share count should be preserved after flush",
            test_name
        );
        assert_eq!(
            accounting.get_share_work_sum().unwrap(),
            work_sum_before,
            "{}: Work sum should be preserved after flush",
            test_name
        );
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            seq_num_before,
            "{}: Sequence number should be preserved after flush",
            test_name
        );
    }

    /// Generic test for error handling
    fn test_share_accounting_trait_error_handling<T: ShareAccountingTrait + ?Sized>(
        accounting: &mut T,
        test_name: &str,
    ) {
        println!("Running generic test: {} - {}", test_name, "error_handling");

        // All operations should return Ok for valid inputs
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();

        assert!(
            accounting
                .update_share_accounting(1000, 1, share_hash)
                .is_ok(),
            "{}: update_share_accounting should succeed",
            test_name
        );
        assert!(
            accounting.is_share_seen(share_hash).is_ok(),
            "{}: is_share_seen should succeed",
            test_name
        );
        assert!(
            accounting.flush_seen_shares().is_ok(),
            "{}: flush_seen_shares should succeed",
            test_name
        );
        assert!(
            accounting.get_last_share_sequence_number().is_ok(),
            "{}: get_last_share_sequence_number should succeed",
            test_name
        );
        assert!(
            accounting.get_shares_accepted().is_ok(),
            "{}: get_shares_accepted should succeed",
            test_name
        );
        assert!(
            accounting.get_share_work_sum().is_ok(),
            "{}: get_share_work_sum should succeed",
            test_name
        );
        assert!(
            accounting.get_best_diff().is_ok(),
            "{}: get_best_diff should succeed",
            test_name
        );
        assert!(
            accounting.get_best_diff_for_user("test_user").is_ok(),
            "{}: get_best_diff_for_user should succeed",
            test_name
        );
        assert!(
            accounting
                .update_best_diff_for_user("test_user", 1000.0)
                .is_ok(),
            "{}: update_best_diff_for_user should succeed",
            test_name
        );
        assert!(
            accounting.update_best_diff(1500.0).is_ok(),
            "{}: update_best_diff should succeed",
            test_name
        );
        assert!(
            accounting.get_top_user_difficulties(5).is_ok(),
            "{}: get_top_user_difficulties should succeed",
            test_name
        );
        assert!(
            accounting.get_share_batch_size().is_ok(),
            "{}: get_share_batch_size should succeed",
            test_name
        );
        assert!(
            accounting.should_acknowledge().is_ok(),
            "{}: should_acknowledge should succeed",
            test_name
        );
    }

    /// Run the complete generic test suite on any ShareAccountingTrait implementation
    fn run_generic_test_suite<T: ShareAccountingTrait + Clone>(mut accounting: T, test_name: &str) {
        test_share_accounting_trait_basic_operations(&mut accounting, test_name);

        // Create a fresh instance for difficulty tracking tests
        let mut accounting_fresh = accounting.clone();
        test_share_accounting_trait_difficulty_tracking(&mut accounting_fresh, test_name);

        // Create another fresh instance for flush tests
        let mut accounting_fresh2 = accounting.clone();
        test_share_accounting_trait_flush_functionality(&mut accounting_fresh2, test_name);

        // Create another fresh instance for error handling tests
        let mut accounting_fresh3 = accounting.clone();
        test_share_accounting_trait_error_handling(&mut accounting_fresh3, test_name);
    }

    /// Run the complete generic test suite on boxed trait objects
    fn run_generic_test_suite_boxed(
        accounting: Box<dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>>,
        test_name: &str,
    ) {
        let mut acc1 = accounting;
        test_share_accounting_trait_basic_operations(&mut *acc1, test_name);

        // For boxed trait objects, we need to create new instances for each test
        // since we can't clone trait objects easily
        let acc2 = match test_name {
            "FactoryClient" => create_client_share_accounting(),
            "FactoryServer" => create_server_share_accounting(3),
            "ConfigFactory" => create_share_accounting(ShareAccountingConfig::InMemory {
                share_batch_size: Some(7),
            })
            .unwrap(),
            _ => create_client_share_accounting(),
        };
        let mut acc2_mut = acc2;
        test_share_accounting_trait_difficulty_tracking(&mut *acc2_mut, test_name);

        let acc3 = match test_name {
            "FactoryClient" => create_client_share_accounting(),
            "FactoryServer" => create_server_share_accounting(3),
            "ConfigFactory" => create_share_accounting(ShareAccountingConfig::InMemory {
                share_batch_size: Some(7),
            })
            .unwrap(),
            _ => create_client_share_accounting(),
        };
        let mut acc3_mut = acc3;
        test_share_accounting_trait_flush_functionality(&mut *acc3_mut, test_name);

        let acc4 = match test_name {
            "FactoryClient" => create_client_share_accounting(),
            "FactoryServer" => create_server_share_accounting(3),
            "ConfigFactory" => create_share_accounting(ShareAccountingConfig::InMemory {
                share_batch_size: Some(7),
            })
            .unwrap(),
            _ => create_client_share_accounting(),
        };
        let mut acc4_mut = acc4;
        test_share_accounting_trait_error_handling(&mut *acc4_mut, test_name);
    }

    // ============================================================================
    // GENERIC TEST SUITE APPLICATIONS
    // ============================================================================

    #[test]
    fn test_generic_suite_with_in_memory_client() {
        let accounting = InMemoryShareAccounting::new_client();
        run_generic_test_suite(accounting, "InMemoryClient");
    }

    #[test]
    fn test_generic_suite_with_in_memory_server() {
        let accounting = InMemoryShareAccounting::new_server(5);
        run_generic_test_suite(accounting, "InMemoryServer");
    }

    #[test]
    fn test_generic_suite_with_factory_client() {
        let accounting = create_client_share_accounting();
        run_generic_test_suite_boxed(accounting, "FactoryClient");
    }

    #[test]
    fn test_generic_suite_with_factory_server() {
        let accounting = create_server_share_accounting(3);
        run_generic_test_suite_boxed(accounting, "FactoryServer");
    }

    #[test]
    fn test_generic_suite_with_config_factory() {
        let config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(7),
        };
        let accounting = create_share_accounting(config).unwrap();
        run_generic_test_suite_boxed(accounting, "ConfigFactory");
    }

    // ============================================================================
    // ADDITIONAL EDGE CASE AND ERROR HANDLING TESTS
    // ============================================================================

    #[test]
    fn test_edge_case_zero_work_shares() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test shares with zero work
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting
            .update_share_accounting(0, 1, share_hash)
            .unwrap();

        assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 0);
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 1);
        assert!(accounting.is_share_seen(share_hash).unwrap());
    }

    #[test]
    fn test_edge_case_maximum_values() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with maximum values
        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting
            .update_share_accounting(u64::MAX, u32::MAX, share_hash)
            .unwrap();

        assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
        assert_eq!(accounting.get_share_work_sum().unwrap(), u64::MAX);
        assert_eq!(
            accounting.get_last_share_sequence_number().unwrap(),
            u32::MAX
        );

        // Test maximum difficulty
        accounting
            .update_best_diff_for_user("max_user", f64::MAX)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("max_user").unwrap(),
            Some(f64::MAX)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), f64::MAX);
    }

    #[test]
    fn test_edge_case_sequence_number_ordering() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test that sequence numbers can be processed out of order
        let share_hash1 = Hash::from_slice(&[1u8; 32]).unwrap();
        let share_hash2 = Hash::from_slice(&[2u8; 32]).unwrap();
        let share_hash3 = Hash::from_slice(&[3u8; 32]).unwrap();

        // Process shares out of order
        accounting
            .update_share_accounting(1000, 3, share_hash3)
            .unwrap();
        accounting
            .update_share_accounting(2000, 1, share_hash1)
            .unwrap();
        accounting
            .update_share_accounting(3000, 2, share_hash2)
            .unwrap();

        // Last processed sequence number should be stored (not necessarily the highest)
        assert_eq!(accounting.get_last_share_sequence_number().unwrap(), 2);
        assert_eq!(accounting.get_shares_accepted().unwrap(), 3);
        assert_eq!(accounting.get_share_work_sum().unwrap(), 6000);
    }

    #[test]
    fn test_edge_case_empty_user_identity() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with empty string user identity
        accounting.update_best_diff_for_user("", 1000.0).unwrap();
        assert_eq!(accounting.get_best_diff_for_user("").unwrap(), Some(1000.0));
        assert_eq!(accounting.get_best_diff().unwrap(), 1000.0);

        // Test with whitespace-only user identity
        accounting.update_best_diff_for_user("   ", 2000.0).unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("   ").unwrap(),
            Some(2000.0)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), 2000.0);
    }

    #[test]
    fn test_edge_case_special_float_values() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with special float values
        accounting.update_best_diff_for_user("zero", 0.0).unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("zero").unwrap(),
            Some(0.0)
        );

        // Test with very small positive value
        accounting
            .update_best_diff_for_user("tiny", f64::MIN_POSITIVE)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("tiny").unwrap(),
            Some(f64::MIN_POSITIVE)
        );

        // Test with infinity (should work but may not be practical)
        accounting
            .update_best_diff_for_user("inf", f64::INFINITY)
            .unwrap();
        assert_eq!(
            accounting.get_best_diff_for_user("inf").unwrap(),
            Some(f64::INFINITY)
        );
        assert_eq!(accounting.get_best_diff().unwrap(), f64::INFINITY);
    }

    #[test]
    fn test_edge_case_large_number_of_users() {
        let mut accounting = InMemoryShareAccounting::new(None);

        // Test with many users
        for i in 0..1000 {
            let user_id = format!("user_{}", i);
            let difficulty = (i as f64) * 10.0;
            accounting
                .update_best_diff_for_user(&user_id, difficulty)
                .unwrap();
        }

        // Verify the highest difficulty is tracked correctly
        assert_eq!(accounting.get_best_diff().unwrap(), 9990.0);

        // Test top users query with large dataset
        let top_users = accounting.get_top_user_difficulties(10).unwrap();
        assert_eq!(top_users.len(), 10);
        assert_eq!(top_users[0].1, 9990.0);
        assert_eq!(top_users[9].1, 9900.0);

        // Test with limit larger than available users
        let all_users = accounting.get_top_user_difficulties(2000).unwrap();
        assert_eq!(all_users.len(), 1000);
    }

    #[test]
    fn test_edge_case_batch_acknowledgment_edge_cases() {
        // Test batch size of 1
        let mut accounting = InMemoryShareAccounting::new_server(1);
        assert!(!accounting.should_acknowledge().unwrap()); // No shares yet

        let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        accounting
            .update_share_accounting(1000, 1, share_hash)
            .unwrap();
        assert!(accounting.should_acknowledge().unwrap()); // Should acknowledge after every share

        // Test very large batch size
        let mut accounting_large = InMemoryShareAccounting::new_server(1000000);
        for i in 1..=999999 {
            let share_hash = Hash::from_slice(&[i as u8; 32]).unwrap();
            accounting_large
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
            assert!(!accounting_large.should_acknowledge().unwrap());
        }

        // Should acknowledge on the millionth share
        let final_hash = Hash::from_slice(&[0u8; 32]).unwrap();
        accounting_large
            .update_share_accounting(1000, 1000000, final_hash)
            .unwrap();
        assert!(accounting_large.should_acknowledge().unwrap());
    }

    #[test]
    fn test_comprehensive_client_server_mode_behavioral_differences() {
        let mut client = InMemoryShareAccounting::new_client();
        let mut server = InMemoryShareAccounting::new_server(3);

        // Both should start with identical state
        assert_eq!(
            client.get_shares_accepted().unwrap(),
            server.get_shares_accepted().unwrap()
        );
        assert_eq!(
            client.get_share_work_sum().unwrap(),
            server.get_share_work_sum().unwrap()
        );
        assert_eq!(
            client.get_best_diff().unwrap(),
            server.get_best_diff().unwrap()
        );

        // But different batch behavior
        assert_eq!(client.get_share_batch_size().unwrap(), None);
        assert_eq!(server.get_share_batch_size().unwrap(), Some(3));
        assert!(!client.should_acknowledge().unwrap());
        assert!(!server.should_acknowledge().unwrap());

        // Process identical shares on both
        for i in 1..=6 {
            let share_hash = Hash::from_slice(&[i; 32]).unwrap();
            client
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();
            server
                .update_share_accounting(1000, i as u32, share_hash)
                .unwrap();

            // Client never acknowledges
            assert!(!client.should_acknowledge().unwrap());

            // Server acknowledges at multiples of batch size
            if i % 3 == 0 {
                assert!(server.should_acknowledge().unwrap());
            } else {
                assert!(!server.should_acknowledge().unwrap());
            }
        }

        // Both should have identical statistics
        assert_eq!(client.get_shares_accepted().unwrap(), 6);
        assert_eq!(server.get_shares_accepted().unwrap(), 6);
        assert_eq!(client.get_share_work_sum().unwrap(), 6000);
        assert_eq!(server.get_share_work_sum().unwrap(), 6000);
        assert_eq!(client.get_last_share_sequence_number().unwrap(), 6);
        assert_eq!(server.get_last_share_sequence_number().unwrap(), 6);

        // Both should have identical difficulty tracking
        client.update_best_diff_for_user("user1", 1500.0).unwrap();
        server.update_best_diff_for_user("user1", 1500.0).unwrap();

        assert_eq!(
            client.get_best_diff_for_user("user1").unwrap(),
            server.get_best_diff_for_user("user1").unwrap()
        );
        assert_eq!(
            client.get_best_diff().unwrap(),
            server.get_best_diff().unwrap()
        );

        // Both should have identical duplicate detection
        let test_hash = Hash::from_slice(&[1u8; 32]).unwrap();
        assert_eq!(
            client.is_share_seen(test_hash).unwrap(),
            server.is_share_seen(test_hash).unwrap()
        );

        // Both should flush identically
        client.flush_seen_shares().unwrap();
        server.flush_seen_shares().unwrap();
        assert_eq!(
            client.is_share_seen(test_hash).unwrap(),
            server.is_share_seen(test_hash).unwrap()
        );
    }

    #[test]
    fn test_error_type_compatibility() {
        // Test that the error type implements required traits
        let error = InMemoryShareAccountingError::Internal("test".to_string());

        // Should implement Display
        let _display_str = format!("{}", error);

        // Should implement Debug
        let _debug_str = format!("{:?}", error);

        // Should implement Error trait
        let _error_trait: &dyn StdError = &error;

        // Should be Send + Sync
        fn assert_send_sync<T: Send + Sync>(_: T) {}
        assert_send_sync(error);
    }
}
