pub(super) mod channel;
pub(super) mod data;
pub mod downstream;
mod message_handler;

use v1::{client_to_server::Submit, utils::HexU32Be};

/// Messages sent from downstream handling logic to the SV1 server.
///
/// This enum defines the types of messages that downstream connections can send
/// to the central SV1 server for processing and forwarding to upstream.
#[derive(Debug)]
pub enum DownstreamMessages {
    /// Represents a submitted share from a downstream miner,
    /// wrapped with the relevant channel ID.
    SubmitShares(SubmitShareWithChannelId),
}

/// A wrapper around a `mining.submit` message with additional channel information.
///
/// This struct contains all the necessary information to process a share submission
/// from an SV1 miner, including the share data itself and metadata needed for
/// proper routing and validation.
#[derive(Debug, Clone)]
pub struct SubmitShareWithChannelId {
    /// The SV2 channel ID this share belongs to
    pub channel_id: u32,
    /// The downstream connection ID that submitted this share
    pub downstream_id: u32,
    /// The actual SV1 share submission data
    pub share: Submit<'static>,
    /// The complete extranonce used for this share
    pub extranonce: Vec<u8>,
    /// The length of the extranonce2 field
    pub extranonce2_len: usize,
    /// Optional version rolling mask for the share
    pub version_rolling_mask: Option<HexU32Be>,
    /// The version field from the job, used for validation
    pub job_version: Option<u32>,
}

/// Sends a shutdown signal to a downstream task.
///
/// This is a convenience function that sends a message on the downstream task
/// shutdown channel. The specific message content doesn't matter as receiving
/// any message triggers shutdown.
///
/// # Arguments
/// * `sender` - The channel sender to signal shutdown on
///
/// # Panics
/// This function will panic if the channel send fails, which only happens if
/// all receiving ends have already been dropped (meaning tasks are already shut down).
pub async fn kill(sender: &async_channel::Sender<bool>) {
    // safe to unwrap since the only way this can fail is if all receiving channels are dropped
    // meaning all tasks have already dropped
    sender.send(true).await.unwrap();
}

/// Generates a subscription ID for SV1 mining connections.
///
/// Currently returns a hardcoded string value. This should be replaced with
/// a proper ID generation mechanism in the future.
///
/// # Returns
/// A string to be used as a subscription ID
///
/// # TODO
/// Replace with proper random ID generation
pub fn new_subscription_id() -> String {
    "ae6812eb4cd7735a302a8a9dd95cf71f".into()
}
