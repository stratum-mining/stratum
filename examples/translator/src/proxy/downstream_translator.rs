use async_channel::{Receiver, Sender};
use v1::json_rpc;

#[derive(Clone)]
pub(crate) struct DownstreamTranslator {
    /// Sends SV1 messages to the `Downstream::receiver_upstream`. These messages are either
    /// translated from SV2 messages received from the `Upstream`, or generated specifically for
    /// the SV1 protocol.
    pub(crate) sender: Sender<json_rpc::Message>,
    /// Receives SV1 messages to the `Downstream::receiver_upstream`. These messages are then
    /// handles by either translating them from SV1 to SV2 or dropped if not applicable to the SV2
    /// protocol.
    pub(crate) receiver: Receiver<json_rpc::Message>,
}

impl DownstreamTranslator {
    pub(crate) fn new(
        sender: Sender<json_rpc::Message>,
        receiver: Receiver<json_rpc::Message>,
    ) -> Self {
        DownstreamTranslator { sender, receiver }
    }

    /// Sends SV1 message (translated from SV2) to the `Downstream.receiver_upstream`.
    /// TODO: Remove this fn
    pub(crate) async fn send_sv1(&mut self, message_sv1: json_rpc::Message) {
        println!("TP SENDS TRANSLATED SV1 MSG TO TD: {:?}", &message_sv1);
        self.sender.send(message_sv1).await.unwrap();
    }
}
