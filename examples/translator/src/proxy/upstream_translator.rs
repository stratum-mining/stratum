use crate::upstream_sv2::EitherFrame;
use async_channel::{Receiver, Sender};

#[derive(Clone)]
pub(crate) struct UpstreamTranslator {
    /// Sends SV2 messages to the `Upstream::receiver_downstream`.
    pub(crate) sender: Sender<EitherFrame>,
    /// Receives SV2 messages from the `Upstream::sender_downstream`. These messages are then
    /// handled by either translating them from SV2 to SV1 or dropped if not applicable to the SV1
    /// protocol.
    pub(crate) receiver: Receiver<EitherFrame>,
}

impl UpstreamTranslator {
    pub(crate) fn new(sender: Sender<EitherFrame>, receiver: Receiver<EitherFrame>) -> Self {
        UpstreamTranslator { sender, receiver }
    }

    /// Sends SV2 message to the `Upstream.receiver_downstream`.
    pub(crate) async fn send_sv2(&mut self, message_sv2: EitherFrame) {
        self.sender.send(message_sv2).await.unwrap();
    }
}
