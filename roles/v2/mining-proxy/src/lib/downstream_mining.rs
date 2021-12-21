use super::upstream_mining::{UpstreamMiningNode, UpstreamMiningNodes};
use async_channel::{Receiver, SendError, Sender};
use messages_sv2::handlers::common::{SetupConnectionSuccess, UpstreamCommon};
use messages_sv2::handlers::mining::{ChannelType, SendTo, UpstreamMining};
use messages_sv2::MiningDeviceMessages;

use codec_sv2::Frame;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};

use crate::lib::mining_channel;

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
use mining_channel::{Channel, GroupChannel};

/// 1 to 1 connection with a downstream node that implement the mining (sub)protocol can be either
/// a mining device or a downstream proxy.
#[derive(Debug)]
pub struct DownstreamMiningNode {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    requires_standard_jobs: bool,
    requires_work_selection: bool,
    requires_version_rolling: bool,
    status: DownstreamMiningNodeStatus,
}

#[derive(Debug, Clone)]
enum DownstreamMiningNodeStatus {
    Initializing,
    Paired(Arc<Mutex<UpstreamMiningNode>>, Channel),
}

use async_std::sync::{Arc, Mutex};
use async_std::task;
use core::convert::TryInto;

impl DownstreamMiningNode {
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        requires_standard_jobs: bool,
        requires_work_selection: bool,
        requires_version_rolling: bool,
    ) -> Self {
        Self {
            receiver,
            sender,
            requires_standard_jobs,
            requires_work_selection,
            requires_version_rolling,
            status: DownstreamMiningNodeStatus::Initializing,
        }
    }

    /// Send SetupConnectionSuccess to donwstream and start processing new messages coming from
    /// downstream
    pub async fn initialize(
        self_: Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
        channel: Channel,
        upstream: Arc<Mutex<UpstreamMiningNode>>,
    ) {
        let cloned = self_.clone();
        let cloned2 = self_.clone();
        let status = self_.lock().await.status.clone();

        match status {
            DownstreamMiningNodeStatus::Initializing => {
                let mut mining_device = cloned.lock().await;
                let setup_connection_success: MiningDeviceMessages =
                    setup_connection_success.into();
                mining_device
                    .send(setup_connection_success.try_into().unwrap())
                    .await
                    .unwrap();
                mining_device.status = DownstreamMiningNodeStatus::Paired(upstream, channel);

                task::spawn(async move {
                    let mining_device = cloned2.clone();
                    let mut mining_device = mining_device.lock().await;
                    loop {
                        let incoming: StdFrame = mining_device
                            .receiver
                            .recv()
                            .await
                            .unwrap()
                            .try_into()
                            .unwrap();
                        mining_device.next(cloned2.clone(), incoming).await
                    }
                });
            }
            DownstreamMiningNodeStatus::Paired(_, _) => panic!(),
        }
    }

    /// Parse the received message and relay it to the right upstream
    pub async fn next(&mut self, self_as_arc: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        // New upstream connection-wide unique request id
        let upstream = self.get_upstream().await;
        let mapper = upstream.request_id_mapper.clone();
        let selector = upstream.downstream_selector.clone();
        drop(upstream);
        let next_message_to_send = UpstreamMining::handle_message(
            self,
            message_type,
            payload,
            selector,
            self_as_arc,
            Some(mapper),
        );

        match next_message_to_send {
            Ok(SendTo::Relay(_)) => {
                let sv2_frame: codec_sv2::Sv2Frame<messages_sv2::PoolMessages, Vec<u8>> =
                    incoming.map(|payload| payload.try_into().unwrap());
                let mut upstream = self.get_upstream().await;
                upstream.send(sv2_frame).await.unwrap();
            }
            Ok(_) => todo!(),
            Err(messages_sv2::Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }
    }

    async fn get_upstream(&mut self) -> async_std::sync::MutexGuard<'_, UpstreamMiningNode> {
        match &mut self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(upstream, _) => {
                let up: async_std::sync::MutexGuard<'_, UpstreamMiningNode> = upstream.lock().await;
                up
            }
        }
    }

    /// Send a message downstream
    pub async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), SendError<StdFrame>> {
        let either_frame = sv2_frame.into();
        match self.sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => {
                todo!()
            }
        }
    }

    fn get_upstream_channel(&self) -> Option<crate::lib::mining_channel::ChannelType> {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => None,
            DownstreamMiningNodeStatus::Paired(_, channel) => Some(channel.upstream),
        }
    }
}

use crate::lib::upstream_mining::Selector;

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl UpstreamMining<Arc<Mutex<DownstreamMiningNode>>, Selector> for DownstreamMiningNode {
    fn get_channel_type(&self) -> ChannelType {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(_, channel) => match channel.downstream {
                mining_channel::ChannelType::Extended(_) => ChannelType::Extended,
                mining_channel::ChannelType::Group(_) => ChannelType::Group,
                mining_channel::ChannelType::Standard => ChannelType::Standard,
            },
        }
    }

    fn is_work_selection_enabled(&self) -> bool {
        todo!()
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenStandardMiningChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        // TODO this function should check if the Downstream is header only mining.
        //   If is header only and a channel has already been opened should return an error.
        //   If not it can proceed.
        match &self.get_upstream_channel().unwrap() {
            mining_channel::ChannelType::Extended(_) => {
                todo!()
            }
            mining_channel::ChannelType::Group(_) => Ok(SendTo::Relay(None)),
            mining_channel::ChannelType::Standard => todo!(),
        }
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenExtendedMiningChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        unreachable!()
    }

    fn handle_update_channel(
        &mut self,
        _: messages_sv2::handlers::mining::UpdateChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        match &self.get_upstream_channel().unwrap() {
            mining_channel::ChannelType::Extended(_) => {
                todo!()
            }
            mining_channel::ChannelType::Group(_) => Ok(SendTo::Relay(None)),
            mining_channel::ChannelType::Standard => todo!(),
        }
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesStandard,
    ) -> Result<SendTo, messages_sv2::Error> {
        match &self.get_upstream_channel().unwrap() {
            mining_channel::ChannelType::Extended(_) => Err(messages_sv2::Error::UnexpectedMessage),
            mining_channel::ChannelType::Group(_) => Ok(SendTo::Relay(None)),
            mining_channel::ChannelType::Standard => todo!(),
        }
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesExtended,
    ) -> Result<SendTo, messages_sv2::Error> {
        match &self.get_upstream_channel().unwrap() {
            mining_channel::ChannelType::Extended(_) => {
                todo!()
            }
            mining_channel::ChannelType::Group(_) => Err(messages_sv2::Error::UnexpectedMessage),
            mining_channel::ChannelType::Standard => todo!(),
        }
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJob,
    ) -> Result<SendTo, messages_sv2::Error> {
        match (
            &self.get_upstream_channel().unwrap(),
            self.is_work_selection_enabled(),
        ) {
            (mining_channel::ChannelType::Extended(_), true) => {
                todo!()
            }
            _ => Err(messages_sv2::Error::UnexpectedMessage),
        }
    }
}

impl UpstreamCommon for DownstreamMiningNode {
    fn handle_setup_connection(
        &mut self,
        _: messages_sv2::handlers::common::SetupConnection,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        use messages_sv2::handlers::common::SendTo;
        Ok(SendTo::Relay(None))
    }
}

async fn set_requires_standard_job(
    node: Arc<Mutex<DownstreamMiningNode>>,
    require_standard_job: bool,
) {
    let mut node = node.lock().await;
    node.requires_standard_jobs = require_standard_job;
}

use async_std::net::TcpListener;
use async_std::prelude::*;
use network_helpers::PlainConnection;
use std::net::SocketAddr;
pub async fn listen_for_downstream_mining(
    address: SocketAddr,
    upstream_nodes: UpstreamMiningNodes,
) {
    let listner = TcpListener::bind(address).await.unwrap();
    let mut incoming = listner.incoming();
    let upstream_nodes = Arc::new(Mutex::new(upstream_nodes));
    let id_generator = Arc::new(Mutex::new(crate::Id::new()));

    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        // TODO
        let node = DownstreamMiningNode::new(receiver, sender, true, false, false);
        let upstream_nodes = upstream_nodes.clone();
        let id_generator = id_generator.clone();
        task::spawn(async move {
            let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();
            let node = Arc::new(Mutex::new(node));
            if let Ok(setup_connection) = DownstreamMiningNode::parse_message(message_type, payload)
            {
                set_requires_standard_job(node.clone(), setup_connection.requires_standard_job())
                    .await;

                let protocol = setup_connection.protocol;
                let flags = setup_connection.flags;
                let min_v = setup_connection.min_version;
                let max_v = setup_connection.max_version;
                let mut id_generator = id_generator.lock().await;
                GroupChannel::new_group_channel(
                    protocol,
                    min_v,
                    max_v,
                    flags,
                    upstream_nodes,
                    node.clone(),
                    id_generator.next(),
                )
                .await
                .unwrap();
            };
        });
    }
}
