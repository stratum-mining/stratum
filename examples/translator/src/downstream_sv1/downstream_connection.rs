use async_channel::{Receiver, Sender};
use v1::json_rpc;

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub(crate) struct DownstreamConnection {
    /// Sends to SV1 messages parsed by `Downstream` to the `receiver_outgoing` which makes the
    /// message available to be written to the SV1 Downstream Mining Device socket
    pub(crate) sender_outgoing: Sender<json_rpc::Message>,
    /// Receiver from `Translator::sender_to_downstream`
    pub(crate) receiver_outgoing: Receiver<json_rpc::Message>,
    /// Sends to `Translator::receiver_from_downstream`
    pub(crate) sender_upstream: Sender<json_rpc::Message>,
    /// Receiver from `Translator::sender_to_downstream`
    pub(crate) receiver_upstream: Receiver<json_rpc::Message>,
}
