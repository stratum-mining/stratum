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
        /// Work value of the accepted share
        share_work: u64,
        /// Sequence number of the share
        share_sequence_number: u32,
        /// Hash of the accepted share (for duplicate detection)
        share_hash: Hash,
        /// Total shares accepted after this update
        total_shares_accepted: u32,
        /// Total work sum after this update
        total_share_work_sum: u64,
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

/// Trait for persisting share accounting events.
///
/// Implementations of this trait handle the persistence of share accounting events,
/// ensuring that persistence operations are non-blocking and can handle failures internally.
pub trait Persistence {
    /// The type of channel sender used to send events for persistence.
    ///
    /// This is typically something like `tokio::sync::mpsc::UnboundedSender<ShareAccountingEvent>`
    /// or `async_channel::Sender<ShareAccountingEvent>`.
    type Sender: Clone + Send + 'static;

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

    fn new(sender: Self::Sender) -> Self;
}

/// A no-op persistence implementation for when persistence is disabled.
///
/// This allows ShareAccounting to always call persistence methods without
/// needing conditional compilation throughout the hot path code.
#[derive(Debug, Clone)]
pub struct NoPersistence;

impl NoPersistence {
    /// Creates a new no-op persistence handler.
    pub fn new() -> Self {
        Self
    }

    /// No-op method for share accounting events.
    pub fn persist_event(&self, _event: ShareAccountingEvent) {}

    /// No-op flush method.
    pub fn flush(&self) {}

    /// No-op shutdown method.
    pub fn shutdown(&self) {}
}

impl Default for NoPersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl Persistence for NoPersistence {
    type Sender = ();

    fn persist_event(&self, _event: ShareAccountingEvent) {}

    fn new(_sender: Self::Sender) -> Self {
        Self
    }
}
