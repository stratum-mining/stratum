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
    /// Request to open an extended mining channel for a downstream that just sent its first
    /// message.
    OpenChannel(u32), // downstream_id
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
