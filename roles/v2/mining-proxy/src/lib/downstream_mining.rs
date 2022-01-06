use super::upstream_mining::UpstreamMiningNode;
use async_channel::{Receiver, SendError, Sender};
use messages_sv2::handlers::common::{ParseDownstreamCommonMessages, SetupConnectionSuccess};
use messages_sv2::handlers::mining::{ChannelType, ParseDownstreamMiningMessages, SendTo};
use messages_sv2::handlers::{IsDownstream, MiningDownstreamData, PairSettings, RoutingLogic};
use messages_sv2::MiningDeviceMessages;

use codec_sv2::Frame;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};

use crate::Mutex;

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// 1 to 1 connection with a downstream node that implement the mining (sub)protocol can be either
/// a mining device or a downstream proxy.
#[derive(Debug)]
pub struct DownstreamMiningNode {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    status: DownstreamMiningNodeStatus,
}

#[derive(Debug, Clone)]
enum DownstreamMiningNodeStatus {
    Initializing,
    // TODO make it an HashMap so it will be possible to split a downstreams connection to more
    // upstreams
    Paired(MiningDownstreamData),
}

use async_std::sync::Arc;
use async_std::task;
use core::convert::TryInto;

impl DownstreamMiningNode {
    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
        Self {
            receiver,
            sender,
            status: DownstreamMiningNodeStatus::Initializing,
        }
    }

    /// Send SetupConnectionSuccess to donwstream and start processing new messages coming from
    /// downstream
    pub async fn initialize(
        self_mutex: Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
        mining_downstream_data: MiningDownstreamData,
    ) {
        let status = self_mutex.safe_lock(|self_| self_.status.clone()).unwrap();

        match status {
            DownstreamMiningNodeStatus::Initializing => {
                let setup_connection_success: MiningDeviceMessages =
                    setup_connection_success.into();

                {
                    DownstreamMiningNode::send(
                        self_mutex.clone(),
                        setup_connection_success.try_into().unwrap(),
                    )
                    .await
                    .unwrap();
                    self_mutex
                        .safe_lock(|self_| {
                            self_.status =
                                DownstreamMiningNodeStatus::Paired(mining_downstream_data);
                        })
                        .unwrap();
                }

                task::spawn(async move {
                    loop {
                        let receiver = self_mutex
                            .safe_lock(|self_| self_.receiver.clone())
                            .unwrap();
                        let message = receiver.recv().await.unwrap();
                        let incoming: StdFrame = message.try_into().unwrap();
                        Self::next(self_mutex.clone(), incoming).await
                    }
                });
            }
            DownstreamMiningNodeStatus::Paired(_) => panic!(),
        }
    }

    /// Parse the received message and relay it to the right upstream
    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let routing_logic = RoutingLogic::Proxy(crate::get_routing_logic());

        let next_message_to_send = ParseDownstreamMiningMessages::handle_message(
            self_mutex.clone(),
            message_type,
            payload,
            routing_logic,
        );

        match next_message_to_send {
            Ok(SendTo::Relay(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<messages_sv2::PoolMessages, Vec<u8>> =
                    incoming.map(|payload| payload.try_into().unwrap());
                UpstreamMiningNode::send(upstream_mutex[0].clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(_) => todo!("147"),
            Err(messages_sv2::Error::UnexpectedMessage) => todo!("148"),
            Err(_) => todo!("149"),
        }
    }

    /// Send a message downstream
    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        sv2_frame: StdFrame,
    ) -> Result<(), SendError<StdFrame>> {
        let either_frame = sv2_frame.into();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => {
                todo!("172")
            }
        }
    }
}

use super::upstream_mining::ProxyRemoteSelector;

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl ParseDownstreamMiningMessages<UpstreamMiningNode, ProxyRemoteSelector>
    for DownstreamMiningNode
{
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenStandardMiningChannel,
        up: Option<Arc<Mutex<UpstreamMiningNode>>>,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        // TODO this function should check if the Downstream is header only mining.
        //   If is header only and a channel has already been opened should return an error.
        //   If not it can proceed.
        Ok(SendTo::Relay(vec![up.unwrap()]))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenExtendedMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        unreachable!()
    }

    fn handle_update_channel(
        &mut self,
        _: messages_sv2::handlers::mining::UpdateChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesStandard,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesExtended,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJob,
    ) -> Result<SendTo<UpstreamMiningNode>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }
}

impl ParseDownstreamCommonMessages for DownstreamMiningNode {
    fn handle_setup_connection(
        &mut self,
        _: messages_sv2::handlers::common::SetupConnection,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        use messages_sv2::handlers::common::SendTo;
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }
}

use async_std::net::TcpListener;
use async_std::prelude::*;
use network_helpers::PlainConnection;
use std::net::SocketAddr;

pub async fn listen_for_downstream_mining(address: SocketAddr) {
    let listner = TcpListener::bind(address).await.unwrap();
    let mut incoming = listner.incoming();

    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let node = DownstreamMiningNode::new(receiver, sender);

        task::spawn(async move {
            let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();

            if let Ok(setup_connection) = DownstreamMiningNode::parse_message(message_type, payload)
            {
                let node = Arc::new(Mutex::new(node));

                let protocol = setup_connection.protocol;
                let flags = setup_connection.flags;
                let min_v = setup_connection.min_version;
                let max_v = setup_connection.max_version;

                let pair_settings = PairSettings {
                    protocol,
                    min_v,
                    max_v,
                    flags,
                };

                let (downstream_data, setup_connection_success) = crate::get_routing_logic()
                    .safe_lock(|r_logic| {
                        r_logic
                            .on_setup_connection_mining_header_only(&pair_settings)
                            .unwrap()
                    })
                    .unwrap();
                DownstreamMiningNode::initialize(node, setup_connection_success, downstream_data)
                    .await;
            };
        });
    }
}

impl IsDownstream for DownstreamMiningNode {
    fn get_downstream_mining_data(&self) -> messages_sv2::handlers::MiningDownstreamData {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(settings) => settings,
        }
    }
}
