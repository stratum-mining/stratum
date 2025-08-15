use crate::sv2::upstream::upstream::EitherFrame;
use async_channel::{Receiver, Sender};
use stratum_common::roles_logic_sv2::parsers_sv2::Mining;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct ChannelState {
    pub upstream_sender: Sender<EitherFrame>,
    pub upstream_receiver: Receiver<EitherFrame>,
    pub sv1_server_sender: Sender<Mining<'static>>,
    pub sv1_server_receiver: Receiver<Mining<'static>>,
}

impl ChannelState {
    pub fn new(
        upstream_sender: Sender<EitherFrame>,
        upstream_receiver: Receiver<EitherFrame>,
        sv1_server_sender: Sender<Mining<'static>>,
        sv1_server_receiver: Receiver<Mining<'static>>,
    ) -> Self {
        Self {
            upstream_sender,
            upstream_receiver,
            sv1_server_sender,
            sv1_server_receiver,
        }
    }

    pub fn drop(&self) {
        debug!("Dropping channel manager channels");
        self.upstream_receiver.close();
        self.upstream_sender.close();
        self.sv1_server_receiver.close();
        self.sv1_server_sender.close();
    }
}
