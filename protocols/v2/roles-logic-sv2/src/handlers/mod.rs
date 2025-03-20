//! # Handlers
//!
//! This module centralizes the logic for processing and routing Stratum V2 protocol messages,
//! defining traits and utilities to handle messages for both Downstream and Upstream roles.
//!
//! ## Purpose
//!
//! - Standardize the handling of protocol-specific messages.
//! - Enable efficient routing, transformation, and relaying of messages between nodes.
//! - Support modularity and scalability across Stratum V2 subprotocols.
//!
//! ## Structure
//!
//! The module is organized by subprotocol and role, with handler traits for:
//! - `ParseDownstream[Protocol]`: Handles messages from Downstream nodes.
//! - `ParseUpstream[Protocol]`: Handles messages from Upstream nodes.
//!
//! Supported subprotocols include:
//! - `common`: Shared messages across all Sv2 roles.
//! - `job_declaration`: Manages custom mining job declarations, transactions, and solutions.
//! - `mining`: Manages standard mining communication (e.g., job dispatch, shares submission).
//! - `template_distribution`: Handles block templates updates and transaction data.
//! - `Common Messages`: Shared across all Sv2 roles.
//!
//! ## Return Values
//!
//! Handlers return `Result<SendTo_, Error>`, where:
//! - `SendTo_` specifies the action (relay, respond, or no action).
//! - `Error` indicates processing issues.
pub mod common;
pub mod job_declaration;
pub mod mining;
pub mod template_distribution;
use crate::utils::Mutex;
use std::sync::Arc;

#[derive(Debug)]
/// Represents a serializable entity used for communication between Remotes.
/// The `SendTo_` enum adds context to the message, specifying the intended action.
pub enum SendTo_<Message, Remote> {
    /// Relay a new message to a specific remote.
    RelayNewMessageToRemote(Arc<Mutex<Remote>>, Message),
    /// Relay the same received message to a specific remote to avoid extra allocations.
    RelaySameMessageToRemote(Arc<Mutex<Remote>>),
    /// Relay a new message without specifying a specific remote.
    ///
    /// This is common in proxies that translate between SV1 and SV2 protocols, where messages are
    /// often broadcasted via extended channels.
    RelayNewMessage(Message),
    /// Directly respond to a received message.
    Respond(Message),
    /// Relay multiple messages to various destinations.
    Multiple(Vec<SendTo_<Message, Remote>>),
    /// Indicates that no immediate action is required for the message.
    ///
    /// This variant allows for cases where the message is still needed for later processing
    /// (e.g., transformations or when two roles are implemented in the same software).
    None(Option<Message>),
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    /// Extracts the message, if available.
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::RelayNewMessageToRemote(_, m) => Some(m),
            Self::RelaySameMessageToRemote(_) => None,
            Self::RelayNewMessage(m) => Some(m),
            Self::Respond(m) => Some(m),
            Self::Multiple(_) => None,
            Self::None(m) => m,
        }
    }

    /// Extracts the remote, if available.
    pub fn into_remote(self) -> Option<Arc<Mutex<Remote>>> {
        match self {
            Self::RelayNewMessageToRemote(r, _) => Some(r),
            Self::RelaySameMessageToRemote(r) => Some(r),
            Self::RelayNewMessage(_) => None,
            Self::Respond(_) => None,
            Self::Multiple(_) => None,
            Self::None(_) => None,
        }
    }
}
