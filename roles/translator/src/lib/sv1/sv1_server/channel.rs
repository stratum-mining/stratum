use crate::sv1::downstream::DownstreamMessages;
use async_channel::{unbounded, Receiver, Sender};
use stratum_common::roles_logic_sv2::parsers_sv2::Mining;

use tokio::sync::broadcast;
use v1::json_rpc;

pub struct Sv1ServerChannelState {
    pub sv1_server_to_downstream_sender: broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
    pub downstream_to_sv1_server_sender: Sender<DownstreamMessages>,
    pub downstream_to_sv1_server_receiver: Receiver<DownstreamMessages>,
    pub channel_manager_receiver: Receiver<Mining<'static>>,
    pub channel_manager_sender: Sender<Mining<'static>>,
}

impl Sv1ServerChannelState {
    pub fn new(
        channel_manager_receiver: Receiver<Mining<'static>>,
        channel_manager_sender: Sender<Mining<'static>>,
    ) -> Self {
        let (sv1_server_to_downstream_sender, _) = broadcast::channel(100);
        let (downstream_to_sv1_server_sender, downstream_to_sv1_server_receiver) = unbounded();

        Self {
            sv1_server_to_downstream_sender,
            downstream_to_sv1_server_receiver,
            downstream_to_sv1_server_sender,
            channel_manager_receiver,
            channel_manager_sender,
        }
    }

    pub fn drop(&self) {
        self.channel_manager_receiver.close();
        self.channel_manager_sender.close();
        self.downstream_to_sv1_server_receiver.close();
        self.downstream_to_sv1_server_sender.close();
        self.channel_manager_receiver.close();
        self.channel_manager_sender.close();
    }
}
