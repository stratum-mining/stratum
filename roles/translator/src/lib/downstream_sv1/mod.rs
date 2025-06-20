//! ## Downstream SV1 Module
//!
//! This module defines the structures, messages, and utility functions
//! used for handling the downstream connection with SV1 mining clients.
//!
//! It includes definitions for messages exchanged with a Bridge component,
//! structures for submitting shares and updating targets, and constants
//! and functions for managing client interactions.
//!
//! The module is organized into the following sub-modules:
//! - [`diff_management`]: (Declared here, likely contains downstream difficulty logic)
//! - [`downstream`]: Defines the core [`Downstream`] struct and its functionalities.

use stratum_common::roles_logic_sv2::mining_sv2::Target;
use v1::{client_to_server::Submit, utils::HexU32Be};
pub mod diff_management;
pub mod downstream;
pub use downstream::Downstream;

/// This constant defines a timeout duration. It is used to enforce
/// that clients sending a `mining.subscribe` message must follow up
/// with a `mining.authorize` within this period. This prevents
/// resource exhaustion attacks where clients open connections
/// with only `mining.subscribe` without intending to mine.
const SUBSCRIBE_TIMEOUT_SECS: u64 = 10;

/// The messages that are sent from the downstream handling logic
/// to a central "Bridge" component for further processing.
#[derive(Debug)]
pub enum DownstreamMessages {
    /// Represents a submitted share from a downstream miner,
    /// wrapped with the relevant channel ID.
    SubmitShares(SubmitShareWithChannelId),
    /// Represents an update to the downstream target for a specific channel.
    SetDownstreamTarget(SetDownstreamTarget),
}

/// wrapper around a `mining.submit` with extra channel informationfor the Bridge to
/// process
#[derive(Debug)]
pub struct SubmitShareWithChannelId {
    pub channel_id: u32,
    pub share: Submit<'static>,
    pub extranonce: Vec<u8>,
    pub extranonce2_len: usize,
    pub version_rolling_mask: Option<HexU32Be>,
}

/// message for notifying the bridge that a downstream target has updated
/// so the Bridge can process the update
#[derive(Debug)]
pub struct SetDownstreamTarget {
    pub channel_id: u32,
    pub new_target: Target,
}

/// This is just a wrapper function to send a message on the Downstream task shutdown channel
/// it does not matter what message is sent because the receiving ends should shutdown on any
/// message
pub async fn kill(sender: &async_channel::Sender<bool>) {
    // safe to unwrap since the only way this can fail is if all receiving channels are dropped
    // meaning all tasks have already dropped
    sender.send(true).await.unwrap();
}

/// Generates a new, hardcoded string intended to be used as a subscription ID.
///
/// FIXME
pub fn new_subscription_id() -> String {
    "ae6812eb4cd7735a302a8a9dd95cf71f".into()
}
