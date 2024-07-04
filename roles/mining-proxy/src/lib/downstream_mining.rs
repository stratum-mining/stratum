#![allow(dead_code)]

use super::upstream_mining::{StdFrame as UpstreamFrame, UpstreamMiningNode};
use async_channel::{Receiver, SendError, Sender};
use roles_logic_sv2::{
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::{
        common::{ParseDownstreamCommonMessages, SendTo as SendToCommon},
        mining::{ParseDownstreamMiningMessages, SendTo, SupportedChannelTypes},
    },
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages, PoolMessages},
    routing_logic::MiningProxyRoutingLogic,
    utils::Mutex,
};
use tracing::info;

use codec_sv2::{StandardEitherFrame, StandardSv2Frame};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// 1 to 1 connection with a downstream node that implement the mining (sub)protocol can be either
/// a mining device or a downstream proxy.
/// A downstream can only be linked with an upstream at a time. Support multi upstrems for
/// downstream do no make much sense.
#[derive(Debug)]
pub struct DownstreamMiningNode {
    id: u32,
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    pub status: DownstreamMiningNodeStatus,
    pub prev_job_id: Option<u32>,
    upstream: Option<Arc<Mutex<UpstreamMiningNode>>>,
}

#[derive(Debug)]
pub enum DownstreamMiningNodeStatus {
    Initializing,
    Paired(CommonDownstreamData),
    ChannelOpened(Channel),
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum Channel {
    DowntreamHomUpstreamGroup {
        data: CommonDownstreamData,
        channel_id: u32,
        group_id: u32,
    },
    DowntreamHomUpstreamExtended {
        data: CommonDownstreamData,
        channel_id: u32,
        group_id: u32,
    },
    // Below variant is not supported cause do not have much sense
    // DowntreamNonHomUpstreamGroup { data: CommonDownstreamData, group_ids: Vec<u32>, extended_ids: Vec<u32>},
    DowntreamNonHomUpstreamExtended {
        data: CommonDownstreamData,
        group_ids: Vec<u32>,
        extended_ids: Vec<u32>,
    },
}

impl DownstreamMiningNodeStatus {
    fn is_paired(&self) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing => false,
            DownstreamMiningNodeStatus::Paired(_) => true,
            DownstreamMiningNodeStatus::ChannelOpened(_) => true,
        }
    }

    fn pair(&mut self, data: CommonDownstreamData) {
        match self {
            DownstreamMiningNodeStatus::Initializing => {
                let self_ = Self::Paired(data);
                let _ = std::mem::replace(self, self_);
            }
            _ => panic!("Try to pair an already paired downstream"),
        }
    }

    pub fn get_channel(&mut self) -> &mut Channel {
        match self {
            DownstreamMiningNodeStatus::Initializing => {
                panic!("Downstream is not initialized no channle opened yet")
            }
            DownstreamMiningNodeStatus::Paired(_channels) => {
                panic!("Downstream is paired but not channle opened yet")
            }
            DownstreamMiningNodeStatus::ChannelOpened(k) => k,
        }
    }

    fn open_channel_for_down_hom_up_group(&mut self, channel_id: u32, group_id: u32) {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => {
                let channel = Channel::DowntreamHomUpstreamGroup {
                    data: *data,
                    channel_id,
                    group_id,
                };
                let self_ = Self::ChannelOpened(channel);
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::ChannelOpened(..) => panic!("Channel already opened"),
        }
    }

    fn open_channel_for_down_hom_up_extended(&mut self, channel_id: u32, group_id: u32) {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => {
                let channel = Channel::DowntreamHomUpstreamExtended {
                    data: *data,
                    channel_id,
                    group_id,
                };
                let self_ = Self::ChannelOpened(channel);
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::ChannelOpened(..) => panic!("Channel already opened"),
        }
    }

    fn add_extended_from_non_hom_for_up_extended(&mut self, id: u32) {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => {
                let channel = Channel::DowntreamNonHomUpstreamExtended {
                    data: *data,
                    group_ids: vec![],
                    extended_ids: vec![id],
                };
                let self_ = Self::ChannelOpened(channel);
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::ChannelOpened(
                Channel::DowntreamNonHomUpstreamExtended { extended_ids, .. },
            ) => {
                if !extended_ids.contains(&id) {
                    extended_ids.push(id)
                }
            }
            _ => panic!(),
        }
    }
}

use core::convert::TryInto;
use std::sync::Arc;
use tokio::task;

impl PartialEq for DownstreamMiningNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl DownstreamMiningNode {
    /// Return mining channel specific data
    pub fn get_channel(&mut self) -> &mut Channel {
        self.status.get_channel()
    }

    pub fn open_channel_for_down_hom_up_group(&mut self, channel_id: u32, group_id: u32) {
        self.status
            .open_channel_for_down_hom_up_group(channel_id, group_id);
    }
    pub fn open_channel_for_down_hom_up_extended(&mut self, channel_id: u32, group_id: u32) {
        self.status
            .open_channel_for_down_hom_up_extended(channel_id, group_id);
    }
    pub fn add_extended_from_non_hom_for_up_extended(&mut self, id: u32) {
        self.status.add_extended_from_non_hom_for_up_extended(id);
    }

    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>, id: u32) -> Self {
        Self {
            receiver,
            sender,
            status: DownstreamMiningNodeStatus::Initializing,
            prev_job_id: None,
            upstream: None,
            id,
        }
    }

    /// Send SetupConnectionSuccess to donwstream and start processing new messages coming from
    /// downstream
    pub async fn start(
        self_mutex: Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
    ) {
        if self_mutex
            .safe_lock(|self_| self_.status.is_paired())
            .unwrap()
        {
            let setup_connection_success: MiningDeviceMessages = setup_connection_success.into();

            {
                DownstreamMiningNode::send(
                    self_mutex.clone(),
                    setup_connection_success.try_into().unwrap(),
                )
                .await
                .unwrap();
            }
            let receiver = self_mutex
                .safe_lock(|self_| self_.receiver.clone())
                .unwrap();

            while let Ok(message) = receiver.recv().await {
                let incoming: StdFrame = message.try_into().unwrap();
                Self::next(self_mutex.clone(), incoming).await;
            }
            Self::exit(self_mutex);
        } else {
            panic!()
        }
    }

    /// Parse the received message and relay it to the right upstream
    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.header().msg_type();
        let payload = incoming.payload().unwrap();

        let routing_logic = super::get_routing_logic();

        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            routing_logic,
        );

        match next_message_to_send {
            Ok(SendTo::RelaySameMessageToRemote(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<PoolMessages, buffer_sv2::Slice> =
                    incoming.map(|payload| payload.try_into().unwrap());
                UpstreamMiningNode::send(upstream_mutex.clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::RelayNewMessageToRemote(upstream_mutex, message)) => {
                let message = PoolMessages::Mining(message);
                let frame: UpstreamFrame = message.try_into().unwrap();
                UpstreamMiningNode::send(upstream_mutex.clone(), frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::Respond(message)) => {
                let message = MiningDeviceMessages::Mining(message);
                let frame: StdFrame = message.try_into().unwrap();
                DownstreamMiningNode::send(self_mutex.clone(), frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::Multiple(sends_to)) => {
                for message in sends_to {
                    match message {
                        roles_logic_sv2::handlers::SendTo_::Respond(m) => match m {
                            Mining::NewMiningJob(_) => {
                                let message = MiningDeviceMessages::Mining(m);
                                let frame: StdFrame = message.try_into().unwrap();
                                DownstreamMiningNode::send(self_mutex.clone(), frame)
                                    .await
                                    .unwrap();
                            }
                            Mining::OpenStandardMiningChannelSuccess(_) => {
                                let message = MiningDeviceMessages::Mining(m);
                                let frame: StdFrame = message.try_into().unwrap();
                                DownstreamMiningNode::send(self_mutex.clone(), frame)
                                    .await
                                    .unwrap();
                            }
                            Mining::SetNewPrevHash(_) => {
                                let message = MiningDeviceMessages::Mining(m);
                                let frame: StdFrame = message.try_into().unwrap();
                                DownstreamMiningNode::send(self_mutex.clone(), frame)
                                    .await
                                    .unwrap();
                            }
                            m => panic!("{:?}", m),
                        },
                        m => panic!("{:?}", m),
                    }
                }
            }
            Ok(SendTo::None(_)) => (),
            Ok(_) => panic!(),
            Err(_) => todo!(),
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
                todo!()
            }
        }
    }

    pub fn exit(self_: Arc<Mutex<Self>>) {
        if let Some(up) = self_.safe_lock(|s| s.upstream.clone()).unwrap() {
            super::upstream_mining::UpstreamMiningNode::remove_dowstream(up, &self_);
        };
        self_
            .safe_lock(|s| {
                s.receiver.close();
            })
            .unwrap();
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
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &binary_sv2::Str0255,
    ) -> Result<bool, Error> {
        Ok(true)
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        req: OpenStandardMiningChannel,
        up: Option<Arc<Mutex<UpstreamMiningNode>>>,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        let channel_id = up
            .as_ref()
            .expect("No upstream initialized")
            .safe_lock(|s| s.channel_ids.safe_lock(|r| r.next()).unwrap())
            .unwrap();
        info!(channel_id);
        let cloned = up.as_ref().expect("No upstream initialized").clone();
        up.as_ref()
            .expect("No upstream initialized")
            .safe_lock(|up| {
                if up.channel_kind.is_extended() {
                    let messages = up.open_standard_channel_down(
                        req.request_id.as_u32(),
                        req.nominal_hash_rate,
                        true,
                        channel_id,
                    );
                    for m in &messages {
                        if let Mining::OpenStandardMiningChannelSuccess(m) = m {
                            self.open_channel_for_down_hom_up_extended(
                                m.channel_id,
                                m.group_channel_id,
                            );
                        }
                    }
                    let messages = messages.into_iter().map(SendTo::Respond).collect();
                    Ok(SendTo::Multiple(messages))
                } else {
                    Ok(SendTo::RelaySameMessageToRemote(cloned))
                }
            })
            .unwrap()
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        todo!()
    }

    fn handle_update_channel(
        &mut self,
        _: UpdateChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        // TODO maybe we want to check if shares meet target before
        // sending them upstream If that is the case it should be
        // done by GroupChannel not here
        match &self.status {
            DownstreamMiningNodeStatus::Initializing => todo!(),
            DownstreamMiningNodeStatus::Paired(_) => todo!(),
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DowntreamHomUpstreamGroup {
                ..
            }) => {
                let remote = self.upstream.as_ref().unwrap();
                let message = Mining::SubmitSharesStandard(m);
                Ok(SendTo::RelayNewMessageToRemote(remote.clone(), message))
            }
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DowntreamHomUpstreamExtended {
                ..
            }) => {
                // Safe unwrap is channel have been opened it means that the dowsntream is paired
                // with an upstream
                let remote = self.upstream.as_ref().unwrap();
                let res = UpstreamMiningNode::handle_std_shr(remote.clone(), m).unwrap();
                Ok(SendTo::Respond(res))
            }
            DownstreamMiningNodeStatus::ChannelOpened(
                Channel::DowntreamNonHomUpstreamExtended { .. },
            ) => {
                // unreachable cause the proxy do not support this kind of channel
                unreachable!();
            }
        }
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: SubmitSharesExtended,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: SetCustomMiningJob,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        todo!()
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
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        let (data, message) = result.unwrap().unwrap();
        let upstream = match super::get_routing_logic() {
            roles_logic_sv2::routing_logic::MiningRoutingLogic::Proxy(proxy_routing) => {
                proxy_routing
                    .safe_lock(|r| r.downstream_to_upstream_map.get(&data).unwrap()[0].clone())
                    .unwrap()
            }
            _ => unreachable!(),
        };
        self.upstream = Some(upstream);

        self.status.pair(data);
        Ok(SendToCommon::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            message.into(),
        ))
    }
}

use network_helpers_sv2::plain_connection_tokio::PlainConnection;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn listen_for_downstream_mining(address: SocketAddr) {
    info!("Listening for downstream mining connections on {}", address);
    let listner = TcpListener::bind(address).await.unwrap();
    let mut ids = roles_logic_sv2::utils::Id::new();

    while let Ok((stream, _)) = listner.accept().await {
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let node = DownstreamMiningNode::new(receiver, sender, ids.next());

        task::spawn(async move {
            let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.header().msg_type();
            let payload = incoming.payload().unwrap();
            let routing_logic = super::get_common_routing_logic();
            let node = Arc::new(Mutex::new(node));

            // Call handle_setup_connection or fail
            match DownstreamMiningNode::handle_message_common(
                node.clone(),
                message_type,
                payload,
                routing_logic,
            ) {
                Ok(SendToCommon::RelayNewMessageToRemote(_, message)) => {
                    let message = match message {
                        roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m) => m,
                        _ => panic!(),
                    };
                    DownstreamMiningNode::start(node, message).await
                }
                _ => panic!(),
            }
        });
    }
}

impl IsDownstream for DownstreamMiningNode {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => data,
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DowntreamHomUpstreamGroup {
                data,
                ..
            }) => data,
            DownstreamMiningNodeStatus::ChannelOpened(
                Channel::DowntreamNonHomUpstreamExtended { data, .. },
            ) => data,
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DowntreamHomUpstreamExtended {
                data,
                ..
            }) => data,
        }
    }
}
impl IsMiningDownstream for DownstreamMiningNode {}
