use async_channel::{Receiver, Sender};
use stratum_common::roles_logic_sv2::{codec_sv2::StandardEitherFrame, parsers_sv2::AnyMessage};
use tracing::debug;

pub type Message = AnyMessage<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug, Clone)]
pub struct UpstreamChannelState {
    /// Receiver for the SV2 Upstream role
    pub upstream_receiver: Receiver<EitherFrame>,
    /// Sender for the SV2 Upstream role
    pub upstream_sender: Sender<EitherFrame>,
    /// Sender for the ChannelManager thread
    pub channel_manager_sender: Sender<EitherFrame>,
    /// Receiver for the ChannelManager thread
    pub channel_manager_receiver: Receiver<EitherFrame>,
}

impl UpstreamChannelState {
    pub fn new(
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: Receiver<EitherFrame>,
        upstream_receiver: Receiver<EitherFrame>,
        upstream_sender: Sender<EitherFrame>,
    ) -> Self {
        Self {
            channel_manager_sender,
            channel_manager_receiver,
            upstream_receiver,
            upstream_sender,
        }
    }

    pub fn drop(&self) {
        debug!("Closing all upstream channels");
        self.upstream_receiver.close();
        self.upstream_receiver.close();
    }
}
