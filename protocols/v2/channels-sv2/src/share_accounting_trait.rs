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
    /// # use channels_sv2::share_accounting_trait::InMemoryShareAccounting;
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
    /// # use channels_sv2::share_accounting_trait::InMemoryShareAccounting;
    /// let server_accounting = InMemoryShareAccounting::new_server(10);
    /// assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(10));
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
                Ok(self.shares_accepted % batch_size as u32 == 0 && self.shares_accepted > 0)
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
        assert!(!server_accounting.should_acknowledge().unwrap()); // Initially false for both

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
        assert!(!server_accounting.should_acknowledge().unwrap()); // Initially false

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
        assert!(!server_accounting.should_acknowledge().unwrap()); // Initially false

        // Verify they behave the same as regular factory function
        let client_config = ShareAccountingConfig::InMemory {
            share_batch_size: None,
        };
        let client_regular = create_share_accounting(client_config).unwrap();

        let server_config = ShareAccountingConfig::InMemory {
            share_batch_size: Some(7),
        };
        let server_regular = create_share_accounting(server_config).unwrap();

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
            (1u64, 5u32), // Test edge case with minimal work
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
        assert_eq!(accounting.get_shares_accepted().unwrap(), shares_before_flush);
        assert_eq!(accounting.get_share_work_sum().unwrap(), work_sum_before_flush);
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
            assert_eq!(accounting.get_last_share_sequence_number().unwrap(), sequence);
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
        accounting.update_best_diff_for_user("edge_user", 0.0).unwrap();
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
}
