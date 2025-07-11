//! ## Upstream SV2 Module
//!
//! This module encapsulates the logic for handling the upstream connection using the SV2 protocol.
//!
//! The module is organized into the following sub-modules:
//! - [`diff_management`]: Contains logic related to managing difficulty and hashrate updates.
//! - [`upstream`]: Defines the main [`Upstream`] struct and its core functionalities.
//! - [`upstream_connection`]: Handles the underlying connection details and frame
//!   sending/receiving.

use stratum_common::roles_logic_sv2::{
    codec_sv2::{StandardEitherFrame, StandardSv2Frame},
    parsers_sv2::AnyMessage,
};

pub mod diff_management;
pub mod upstream;
pub mod upstream_connection;
pub use upstream::Upstream;
pub use upstream_connection::UpstreamConnection;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// Represents the state or parameters negotiated during an SV2 Setup Connection message.
#[derive(Clone, Copy, Debug)]
pub struct Sv2MiningConnection {
    _version: u16,
    _setup_connection_flags: u32,
    #[allow(dead_code)]
    setup_connection_success_flags: u32,
}
