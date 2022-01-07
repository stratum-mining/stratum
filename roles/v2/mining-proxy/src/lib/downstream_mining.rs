use super::upstream_mining::UpstreamMiningNode;
use async_channel::{Receiver, SendError, Sender};
use messages_sv2::common_messages_sv2::{SetupConnection, SetupConnectionSuccess};
use messages_sv2::common_properties::{
    CommonDownstreamData, IsDownstream, IsMiningDownstream
};
use messages_sv2::errors::Error;
use messages_sv2::handlers::common::{ParseDownstreamCommonMessages, SendTo as SendToCommon};
use messages_sv2::handlers::mining::{ChannelType, ParseDownstreamMiningMessages, SendTo};
use messages_sv2::mining_sv2::*;
use messages_sv2::parsers::{MiningDeviceMessages, PoolMessages};
use messages_sv2::routing_logic::{MiningProxyRoutingLogic,MiningRoutingLogic,CommonRoutingLogic};
use messages_sv2::utils::Mutex;

use codec_sv2::Frame;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};

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
    Paired(CommonDownstreamData),
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

    pub fn initialize(&mut self, data: CommonDownstreamData) {
        self.status = DownstreamMiningNodeStatus::Paired(data)
    }

    /// Send SetupConnectionSuccess to donwstream and start processing new messages coming from
    /// downstream
    pub async fn start(
        self_mutex: Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
    ) {

        let status = self_mutex.safe_lock(|self_| self_.status.clone()).unwrap();

        match status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(_) => {
                let setup_connection_success: MiningDeviceMessages =
                    setup_connection_success.into();

                {
                    DownstreamMiningNode::send(
                        self_mutex.clone(),
                        setup_connection_success.try_into().unwrap(),
                    )
                    .await
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
                }).await;
            }
        }
    }

    /// Parse the received message and relay it to the right upstream
    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let routing_logic = MiningRoutingLogic::Proxy(crate::get_routing_logic());

        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            routing_logic,
        );

        match next_message_to_send {
            Ok(SendTo::Relay(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<PoolMessages, Vec<u8>> =
                    incoming.map(|payload| payload.try_into().unwrap());
                UpstreamMiningNode::send(upstream_mutex[0].clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(_) => todo!("147"),
            Err(Error::UnexpectedMessage) => todo!("148"),
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
impl
    ParseDownstreamMiningMessages<
        UpstreamMiningNode,
        ProxyRemoteSelector,
        MiningProxyRoutingLogic<Self, UpstreamMiningNode, ProxyRemoteSelector>,
    > for DownstreamMiningNode
{
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        _: OpenStandardMiningChannel,
        up: Option<Arc<Mutex<UpstreamMiningNode>>>,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        // TODO this function should check if the Downstream is header only mining.
        //   If is header only and a channel has already been opened should return an error.
        //   If not it can proceed.
        Ok(SendTo::Relay(vec![up.unwrap()]))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        unreachable!()
    }

    fn handle_update_channel(
        &mut self,
        _: UpdateChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: SubmitSharesStandard,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: SubmitSharesExtended,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: SetCustomMiningJob,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }
}

impl
    ParseDownstreamCommonMessages<
        MiningProxyRoutingLogic<Self, UpstreamMiningNode, ProxyRemoteSelector>,
    > for DownstreamMiningNode
{
    fn handle_setup_connection(
        &mut self,
        _: SetupConnection,
        result: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<messages_sv2::handlers::common::SendTo, Error> {
        let (data, message) = result.unwrap().unwrap();
        self.initialize(data);
        Ok(SendToCommon::Downstream(message.try_into().unwrap()))
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
            let routing_logic = CommonRoutingLogic::Proxy(crate::get_routing_logic());
            let node = Arc::new(Mutex::new(node));

            // Call handle_setup_connection or fail
            match DownstreamMiningNode::handle_message_common(node.clone(),message_type,payload,routing_logic) {
                Ok(SendToCommon::Downstream(message)) => {
                        let message = match message {
                            messages_sv2::parsers::CommonMessages::SetupConnectionSuccess(m) => m,
                            _ => panic!(),
                        };
                        DownstreamMiningNode::start(node, message).await
                },
                _ => panic!()
            }

        });
    }
}

impl IsDownstream for DownstreamMiningNode {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(settings) => settings,
        }
    }
}
impl IsMiningDownstream for DownstreamMiningNode {}
