use crate::upstream_sv2::MiningMessage;
use async_channel::{Receiver, Sender};

#[derive(Clone)]
pub(crate) struct UpstreamTranslator {
    /// Sends SV2 messages to the `Upstream::receiver_downstream`.
    pub(crate) sender: Sender<MiningMessage>,
    /// Receives SV2 messages from the `Upstream::sender_downstream`. These messages are then
    /// handled by either translating them from SV2 to SV1 or dropped if not applicable to the SV1
    /// protocol.
    pub(crate) receiver: Receiver<MiningMessage>,
}

impl UpstreamTranslator {
    pub(crate) fn new(sender: Sender<MiningMessage>, receiver: Receiver<MiningMessage>) -> Self {
        UpstreamTranslator { sender, receiver }
    }

    /// Sends SV2 message to the `Upstream.receiver_downstream`.
    pub(crate) async fn _send_sv2(&mut self, message_sv2: MiningMessage) {
        println!("TP SENDS TRANSLATED SV2 MSG TO TU: {:?}", &message_sv2);
        self.sender.send(message_sv2).await.unwrap();
    }
}
