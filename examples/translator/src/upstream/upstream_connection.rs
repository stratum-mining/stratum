use super::{EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug, Clone)]
pub(crate) struct UpstreamConnection {
    /// Receives messages from the SV2 Upstream role
    pub(crate) receiver: Receiver<EitherFrame>,
    /// Sends messages to the SV2 Upstream role
    pub(crate) sender: Sender<EitherFrame>,
}

impl UpstreamConnection {
    /// Send a SV2 message to the Upstream role
    pub(crate) async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), ()> {
        let either_frame = sv2_frame.into();
        match self.sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }
}
