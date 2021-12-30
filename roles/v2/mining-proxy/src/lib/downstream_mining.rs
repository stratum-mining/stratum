use super::upstream_mining::{UpstreamMiningNode, UpstreamMiningNodes};
use async_channel::{Receiver, SendError, Sender};
use messages_sv2::handlers::common::{Protocol, SetupConnectionSuccess, UpstreamCommon};
use messages_sv2::handlers::mining::{ChannelType, SendTo, UpstreamMining};
use messages_sv2::MiningDeviceMessages;

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
    requires_standard_jobs: bool,
    requires_work_selection: bool,
    requires_version_rolling: bool,
    status: DownstreamMiningNodeStatus,
}

#[derive(Debug, Clone)]
enum DownstreamMiningNodeStatus {
    Initializing,
    // TODO make it an HashMap so it will be possible to split a downstreams connection to more
    // upstreams
    Paired(Arc<Mutex<UpstreamMiningNode>>),
}

use crate::Mutex;
use async_std::sync::Arc;
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
        self_mutex: Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
        //channel: Channel,
        upstream: Arc<Mutex<UpstreamMiningNode>>,
    ) {
        let status = self_mutex.safe_lock(|self_| self_.status.clone()).await;

        match status {
            DownstreamMiningNodeStatus::Initializing => {
                let setup_connection_success: MiningDeviceMessages =
                    setup_connection_success.into();

                {
                    let mut mining_device = self_mutex.lock().await;
                    mining_device
                        .send(setup_connection_success.try_into().unwrap())
                        .await
                        .unwrap();
                    mining_device.status = DownstreamMiningNodeStatus::Paired(upstream);
                    drop(mining_device);
                }

                task::spawn(async move {
                    loop {
                        let receiver = self_mutex.safe_lock(|self_| self_.receiver.clone()).await;
                        if let Ok(message) = receiver.try_recv() {
                            let incoming: StdFrame = message.try_into().unwrap();
                            Self::next(self_mutex.clone(), incoming).await
                        }
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

        // New upstream connection-wide unique request id
        let upstream_mutex = self_mutex.safe_lock(|self_| self_.get_upstream()).await;
        let (selector, mapper) = upstream_mutex
            .safe_lock(|upstream| {
                (
                    Some(upstream.downstream_selector.clone()),
                    upstream.request_id_mapper.clone(),
                )
            })
            .await;

        let next_message_to_send = self_mutex
            .safe_lock(|self_| {
                self_.handle_message(
                    message_type,
                    payload,
                    selector,
                    self_mutex.clone(),
                    Some(mapper),
                )
            })
            .await;

        match next_message_to_send {
            Ok(SendTo::Relay(_)) => {
                let sv2_frame: codec_sv2::Sv2Frame<messages_sv2::PoolMessages, Vec<u8>> =
                    incoming.map(|payload| payload.try_into().unwrap());
                let upstream_mutex = self_mutex.safe_lock(|self_| self_.get_upstream()).await;
                UpstreamMiningNode::send(upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(_) => todo!(),
            Err(messages_sv2::Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }
    }

    fn get_upstream(&mut self) -> Arc<Mutex<UpstreamMiningNode>> {
        match &mut self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(upstream) => upstream.clone(),
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
}

use crate::lib::upstream_mining::Selector;

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl UpstreamMining<Arc<Mutex<DownstreamMiningNode>>, Selector> for DownstreamMiningNode {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        todo!()
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenStandardMiningChannel,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        // TODO this function should check if the Downstream is header only mining.
        //   If is header only and a channel has already been opened should return an error.
        //   If not it can proceed.
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenExtendedMiningChannel,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        unreachable!()
    }

    fn handle_update_channel(
        &mut self,
        _: messages_sv2::handlers::mining::UpdateChannel,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesStandard,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesExtended,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJob,
    ) -> Result<SendTo<Arc<Mutex<DownstreamMiningNode>>>, messages_sv2::Error> {
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }
}

impl UpstreamCommon for DownstreamMiningNode {
    fn handle_setup_connection(
        &mut self,
        _: messages_sv2::handlers::common::SetupConnection,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        use messages_sv2::handlers::common::SendTo;
        Ok(SendTo::Relay(Vec::with_capacity(0)))
    }
}

async fn set_requires_standard_job(
    node: Arc<Mutex<DownstreamMiningNode>>,
    require_standard_job: bool,
) {
    node.safe_lock(|node| {
        node.requires_standard_jobs = require_standard_job;
    })
    .await;
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

    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let node = DownstreamMiningNode::new(receiver, sender, true, false, false);

        let upstream_nodes = upstream_nodes.clone();

        task::spawn(async move {
            let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();

            if let Ok(setup_connection) = DownstreamMiningNode::parse_message(message_type, payload)
            {
                let node = Arc::new(Mutex::new(node));
                // node is locked and then dropped
                set_requires_standard_job(node.clone(), setup_connection.requires_standard_job())
                    .await;

                let protocol = setup_connection.protocol;
                let flags = setup_connection.flags;
                let min_v = setup_connection.min_version;
                let max_v = setup_connection.max_version;

                pair_upstream_for_header_only_connection(
                    protocol,
                    min_v,
                    max_v,
                    flags,
                    upstream_nodes,
                    node,
                )
                .await
                .unwrap();
            };
        });
    }
}

async fn pair_upstream_for_header_only_connection(
    protocol: Protocol,
    min_v: u16,
    max_v: u16,
    flags: u32,
    upstream_nodes_mutex: Arc<Mutex<UpstreamMiningNodes>>,
    downstream: Arc<Mutex<DownstreamMiningNode>>,
) -> Result<(), ()> {
    let mut upstream_nodes = upstream_nodes_mutex.lock().await;
    let (upstream_mutex, used_version) = upstream_nodes
        .pair_downstream(protocol, min_v, max_v, flags)
        .await?;
    drop(upstream_nodes);

    let setup_connection_success = SetupConnectionSuccess {
        used_version,
        flags,
    };

    DownstreamMiningNode::initialize(
        downstream.clone(),
        setup_connection_success,
        //channel,
        upstream_mutex,
    )
    .await;
    Ok(())
}
