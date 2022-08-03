use async_std::net::TcpStream;

use async_channel::{Receiver, Sender};
use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub(crate) struct DownstreamConnection {
    /// Sends messages to the SV1 Downstream client node (most typically a SV1 Mining Device).
    pub(crate) sender_outgoing: Sender<json_rpc::Message>,
    /// Receives messages from the SV1 Downstream client node (most typically a SV1 Mining Device).
    pub(crate) receiver_incoming: Receiver<json_rpc::Message>,
    /// Sends to Translator::receiver_downstream
    pub(crate) sender_upstream: Sender<json_rpc::Message>,
    /// Receiver from Translator::sender_downstream
    pub(crate) receiver_upstream: Receiver<json_rpc::Message>,
}
