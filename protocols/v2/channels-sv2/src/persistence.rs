//! Share Accounting Persistence Abstraction.
//!
//! This module provides a trait-based abstraction for persisting share accounting data
//! without blocking critical mining operations. The design uses async channels to decouple
//! the hot path share processing from persistence operations.
//!
//! ## Design Goals
//!
//! - **Zero Hot Path Impact**: No `Result` types or blocking operations in share accounting methods
//! - **Async Channel Based**: Events are sent via channels for background persistence processing
//! - **Flexible Implementation**: Trait allows different persistence backends (database, file,
//!   etc.)
//!
//! ## Usage
//!
//! The persistence system works by sending events through async channels when share accounting
//! state changes occur. Implementations of the `Persistence` trait handle these events in
//! background tasks without affecting mining performance.

use bitcoin::hashes::sha256d::Hash;

/// Events that can be persisted from share accounting operations.
///
/// These events capture all the critical state changes that occur during share processing,
/// allowing persistence implementations to maintain complete historical records.
#[derive(Debug, Clone)]
pub enum ShareAccountingEvent {
    /// A share was accepted and accounting was updated.
    ShareAccepted {
        /// The channel identifier (server-assigned for server channels, client-assigned for client
        /// channels)
        channel_id: u32,
        /// User identity associated with the channel
        user_identity: String,
        /// Work value of the accepted share (difficulty as floating point)
        share_work: f64,
        /// Sequence number of the share
        share_sequence_number: u32,
        /// Hash of the accepted share (for duplicate detection)
        share_hash: Hash,
        /// Total shares accepted after this update
        total_shares_accepted: u32,
        /// Total work sum after this update (sum of all share difficulties)
        total_share_work_sum: f64,
        /// Timestamp when the event occurred
        timestamp: std::time::SystemTime,
        /// Block found?
        block_found: bool,
    },
    /// Best difficulty was updated for the channel.
    BestDifficultyUpdated {
        /// The channel identifier
        channel_id: u32,
        /// The new best difficulty
        new_best_diff: f64,
        /// The previous best difficulty
        previous_best_diff: f64,
        /// Timestamp when the update occurred
        timestamp: std::time::SystemTime,
    },
}

/// Trait for handling persistence of share accounting events.
///
/// Implementations of this trait handle the actual persistence operations,
/// ensuring that persistence operations are non-blocking and can handle failures internally.
pub trait PersistenceHandler {
    /// Sends a share accounting event for persistence.
    ///
    /// This method MUST be non-blocking and infallible from the caller's perspective.
    /// Any persistence failures should be handled internally (e.g., logged, retried, etc.).
    fn persist_event(&self, event: ShareAccountingEvent);

    /// Optional method to flush any pending events.
    ///
    /// This is a hint that the caller would like any buffered events to be processed
    /// immediately, but implementations are free to ignore this if not applicable.
    fn flush(&self) {}

    /// Optional method called when the persistence handler is being dropped.
    ///
    /// Implementations can use this for cleanup operations, but should not block.
    fn shutdown(&self) {}
}

/// Main persistence abstraction that handles enabled/disabled states.
#[derive(Debug, Clone)]
pub enum Persistence<T> {
    /// Persistence is enabled with the given handler
    Enabled(T),
    /// Persistence is disabled (no-op)
    Disabled,
}

impl<T: PersistenceHandler> Persistence<T> {
    /// Creates persistence state from an Option.
    /// Some(persistence) becomes Enabled, None becomes Disabled.
    pub fn new(persistence: Option<T>) -> Self {
        match persistence {
            Some(p) => Self::Enabled(p),
            None => Self::Disabled,
        }
    }
}

impl<T: PersistenceHandler> Default for Persistence<T> {
    /// Default persistence state is Disabled (no persistence).
    fn default() -> Self {
        Self::Disabled
    }
}

impl<T: PersistenceHandler> PersistenceHandler for Persistence<T> {
    fn persist_event(&self, event: ShareAccountingEvent) {
        match self {
            Self::Enabled(persistence) => persistence.persist_event(event),
            Self::Disabled => {
                // No-op - persistence is disabled
            }
        }
    }

    fn flush(&self) {
        match self {
            Self::Enabled(persistence) => persistence.flush(),
            Self::Disabled => {
                // No-op - persistence is disabled
            }
        }
    }

    fn shutdown(&self) {
        match self {
            Self::Enabled(persistence) => persistence.shutdown(),
            Self::Disabled => {
                // No-op - persistence is disabled
            }
        }
    }
}
