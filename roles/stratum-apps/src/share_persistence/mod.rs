//! Share Persistence File Handler
//!
//! This module provides a generic file-based persistence implementation for share accounting
//! events. It is designed to be reusable across different Stratum V2 roles (Pool, JD Client,
//! Translator, etc.).
//!
//! ## Design
//!
//! - **Non-blocking**: Uses async channels to decouple persistence from critical mining operations
//! - **Generic**: No role-specific dependencies, uses standard `tracing` for logging
//! - **File-based**: Writes share accounting events to a text file for audit/analysis
//!
//! ## Usage
//!
//! ```rust,ignore
//! use stratum_apps::share_persistence::{ShareFileHandler, ShareFilePersistence};
//! use stratum_apps::stratum_core::channels_sv2::persistence::Persistence;
//!
//! // Create the handler
//! let mut handler = ShareFileHandler::new("/path/to/shares.txt").await;
//! let sender = handler.get_sender();
//! let receiver = handler.get_receiver();
//!
//! // Spawn background task to write events
//! tokio::spawn(async move {
//!     loop {
//!         match receiver.recv().await {
//!             Ok(event) => handler.write_event_to_file(event).await,
//!             Err(_) => break,
//!         }
//!     }
//! });
//!
//! // Create persistence handler for share accounting
//! let persistence = Persistence::new(Some(ShareFilePersistence::new(sender)));
//! ```

use stratum_core::channels_sv2::persistence::{PersistenceHandler, ShareAccountingEvent};
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info};

/// File-based handler for writing share accounting events to disk.
///
/// This handler maintains an async channel for receiving events and writes them
/// to a file in a human-readable format. It uses non-blocking I/O to ensure
/// persistence operations don't impact mining performance.
pub struct ShareFileHandler {
    file: tokio::fs::File,
    receiver: async_channel::Receiver<ShareAccountingEvent>,
    sender: async_channel::Sender<ShareAccountingEvent>,
}

impl ShareFileHandler {
    /// Creates a new share file handler that writes to the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - File path where share events will be written
    ///
    /// # Panics
    ///
    /// Panics if the file cannot be created.
    pub async fn new(path: &str) -> Self {
        let file = tokio::fs::File::create(path).await.unwrap();
        let (sender, receiver) = async_channel::bounded(1024);
        info!("Share accounting file created at {}", path);
        Self {
            file,
            receiver,
            sender,
        }
    }

    /// Returns a receiver for consuming share accounting events.
    ///
    /// The receiver should be used in a background task that calls
    /// `write_event_to_file` for each received event.
    pub fn get_receiver(&self) -> async_channel::Receiver<ShareAccountingEvent> {
        self.receiver.clone()
    }

    /// Returns a sender for producing share accounting events.
    ///
    /// This sender should be wrapped in a `ShareFilePersistence` and passed
    /// to the channel's persistence configuration.
    pub fn get_sender(&self) -> async_channel::Sender<ShareAccountingEvent> {
        self.sender.clone()
    }

    /// Writes a share accounting event to the file.
    ///
    /// Events are formatted as human-readable text lines. Errors are logged
    /// using `tracing::error!` but do not propagate to avoid impacting mining.
    ///
    /// Special events like block discoveries are logged at `info` level.
    pub async fn write_event_to_file(&mut self, event: ShareAccountingEvent) {
        match event {
            ShareAccountingEvent::ShareAccepted {
                channel_id,
                user_identity,
                share_work,
                share_sequence_number,
                share_hash,
                total_shares_accepted,
                total_share_work_sum,
                timestamp,
                block_found,
            } => {
                let result = self.file.write_all(
                    format!(
                        "ShareAccepted: channel_id: {}, user_identity: {}, share_work: {}, share_sequence_number: {}, share_hash: {}, total_shares_accepted: {}, total_share_work_sum: {}, timestamp: {:?}, block_found: {}\n",
                        channel_id,
                        user_identity,
                        share_work,
                        share_sequence_number,
                        share_hash,
                        total_shares_accepted,
                        total_share_work_sum,
                        timestamp,
                        block_found
                    )
                    .as_bytes(),
                ).await;

                if let Err(e) = result {
                    error!(
                        target = "share_file_handler",
                        "Failed to write share event: {}", e
                    );
                }
                debug!("Wrote record")
            }
            ShareAccountingEvent::BestDifficultyUpdated {
                channel_id,
                new_best_diff,
                previous_best_diff,
                timestamp,
            } => {
                let result = self.file.write_all(
                    format!(
                        "BestDifficultyUpdated: channel_id: {}, new_best_diff: {}, previous_best_diff: {}, timestamp: {:?}\n",
                        channel_id,
                        new_best_diff,
                        previous_best_diff,
                        timestamp
                    )
                    .as_bytes(),
                ).await;

                if let Err(e) = result {
                    error!(
                        target = "share_file_handler",
                        "Failed to write difficulty update: {}", e
                    );
                }
            }
        }
    }
}

/// Channel-based persistence handler for share accounting.
///
/// This implements the `PersistenceHandler` trait and sends events through
/// an async channel to be processed by a `ShareFileHandler` in a background task.
#[derive(Clone, Debug)]
pub struct ShareFilePersistence {
    sender: async_channel::Sender<ShareAccountingEvent>,
}

impl ShareFilePersistence {
    /// Creates a new persistence handler with the given event sender.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender connected to a `ShareFileHandler`
    pub fn new(sender: async_channel::Sender<ShareAccountingEvent>) -> Self {
        info!("File persistence enabled for share accounting.");
        Self { sender }
    }
}

impl PersistenceHandler for ShareFilePersistence {
    /// Sends a share accounting event for persistence.
    ///
    /// This is non-blocking and will not return errors. Failed sends are logged
    /// but do not propagate to avoid impacting the hot path.
    fn persist_event(&self, event: ShareAccountingEvent) {
        let _ = self
            .sender
            .try_send(event)
            .map_err(|e| error!(target = "share_file_persistence", "{}", e));
    }
}

/// No-op persistence handler for roles that don't need persistence.
///
/// This is a unit-like type that implements `PersistenceHandler` but does nothing.
/// It's useful for roles that need to instantiate channels but don't want to persist
/// share accounting events (e.g., JD Client, Translator).
///
/// ## Usage
///
/// ```rust,ignore
/// use stratum_apps::share_persistence::NoOpPersistence;
/// use stratum_apps::stratum_core::channels_sv2::persistence::Persistence;
///
/// // Create disabled persistence
/// type DisabledPersistence = Persistence<NoOpPersistence>;
/// let persistence = Persistence::Disabled; // or Persistence::new(None)
/// ```
#[derive(Debug, Clone, Copy)]
pub struct NoOpPersistence;

impl PersistenceHandler for NoOpPersistence {
    fn persist_event(&self, _event: ShareAccountingEvent) {
        // No-op - this should never be called when using Persistence::Disabled
    }
}
