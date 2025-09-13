use super::DownstreamMessages;
use async_channel::{Receiver, Sender};
use tokio::sync::broadcast;
use tracing::debug;
use v1::json_rpc;

#[derive(Debug)]
pub struct DownstreamChannelState {
    pub downstream_sv1_sender: Sender<json_rpc::Message>,
    pub downstream_sv1_receiver: Receiver<json_rpc::Message>,
    pub sv1_server_sender: Sender<DownstreamMessages>,
    pub sv1_server_receiver: broadcast::Receiver<(u32, Option<u32>, json_rpc::Message)>, /* channel_id, optional downstream_id, message */
}

impl DownstreamChannelState {
    pub fn new(
        downstream_sv1_sender: Sender<json_rpc::Message>,
        downstream_sv1_receiver: Receiver<json_rpc::Message>,
        sv1_server_sender: Sender<DownstreamMessages>,
        sv1_server_receiver: broadcast::Receiver<(u32, Option<u32>, json_rpc::Message)>,
    ) -> Self {
        Self {
            downstream_sv1_receiver,
            downstream_sv1_sender,
            sv1_server_receiver,
            sv1_server_sender,
        }
    }

    pub fn drop(&self) {
        debug!("Dropping downstream channel state");
        self.downstream_sv1_receiver.close();
        self.downstream_sv1_sender.close();
    }
}
