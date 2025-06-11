use std::{convert::TryInto, sync::Arc};

use async_channel::{Receiver, SendError, Sender};
use tokio::{net::TcpListener, sync::oneshot::Receiver as TokioReceiver};
use tracing::{debug, info, trace, warn};

use super::{
    routing_logic::{CommonRouter, CommonRoutingLogic, MiningRouter, MiningRoutingLogic},
    upstream_mining::{StdFrame as UpstreamFrame, UpstreamMiningNode},
};
use stratum_common::{
    network_helpers_sv2::plain_connection::PlainConnection,
    roles_logic_sv2::{
        self, codec_sv2,
        codec_sv2::{binary_sv2, StandardEitherFrame, StandardSv2Frame},
        common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
        common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
        errors::Error,
        handlers::{
            common::{ParseCommonMessagesFromDownstream, SendTo as SendToCommon},
            mining::{ParseMiningMessagesFromDownstream, SendTo, SupportedChannelTypes},
        },
        mining_sv2::*,
        parsers::{AnyMessage, Mining, MiningDeviceMessages},
        utils::Mutex,
    },
};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// 1 to 1 connection with a downstream node that implement the mining (sub)protocol can be either
/// a mining device or a downstream proxy.
/// A downstream can only be linked with an upstream at a time. Support multi upstreams for
/// downstream do not make much sense.
#[derive(Debug, Clone)]
pub struct DownstreamMiningNode {
    id: u32,
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    pub status: DownstreamMiningNodeStatus,
    upstream: Option<Arc<Mutex<UpstreamMiningNode>>>,
}

#[derive(Debug, Clone)]
pub enum DownstreamMiningNodeStatus {
    Initializing,
    Paired(CommonDownstreamData),
    ChannelOpened(Channel),
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum Channel {
    DownstreamHomUpstreamGroup {
        data: CommonDownstreamData,
        channel_id: u32,
        group_id: u32,
    },
    DownstreamHomUpstreamExtended {
        data: CommonDownstreamData,
        channel_id: u32,
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
                let channel = Channel::DownstreamHomUpstreamGroup {
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

    fn open_channel_for_down_hom_up_extended(&mut self, channel_id: u32, _group_id: u32) {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => {
                let channel = Channel::DownstreamHomUpstreamExtended {
                    data: *data,
                    channel_id,
                };
                let self_ = Self::ChannelOpened(channel);
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::ChannelOpened(..) => panic!("Channel already opened"),
        }
    }
}

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

    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>, id: u32) -> Self {
        Self {
            receiver,
            sender,
            status: DownstreamMiningNodeStatus::Initializing,
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
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let next_message_to_send = ParseMiningMessagesFromDownstream::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
        );

        match next_message_to_send {
            Ok(SendTo::RelaySameMessageToRemote(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<AnyMessage, buffer_sv2::Slice> =
                    incoming.map(|payload| payload.try_into().unwrap());
                UpstreamMiningNode::send(upstream_mutex.clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::RelayNewMessageToRemote(upstream_mutex, message)) => {
                let message = AnyMessage::Mining(message);
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
            UpstreamMiningNode::remove_dowstream(up, &self_);
        };
        self_
            .safe_lock(|s| {
                s.receiver.close();
            })
            .unwrap();
    }
}

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl ParseMiningMessagesFromDownstream<UpstreamMiningNode> for DownstreamMiningNode {
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
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!(
            "Received OpenStandardMiningChannel from: {} with id: {}",
            std::str::from_utf8(req.user_identity.as_ref()).unwrap_or("Unknown identity"),
            req.get_request_id_as_u32()
        );
        debug!("OpenStandardMiningChannel: {:?}", req);
        let downstream_mining_data = self.get_downstream_mining_data();
        let routing_logic = super::get_routing_logic();

        let upstream = match routing_logic {
            MiningRoutingLogic::Proxy(r_logic) => {
                trace!("On OpenStandardMiningChannel r_logic is: {:?}", r_logic);
                let up = r_logic
                    .safe_lock(|r_logic| {
                        r_logic.on_open_standard_channel(
                            Arc::new(Mutex::new(self.clone())),
                            &mut req.clone(),
                            &downstream_mining_data,
                        )
                    })?;
                trace!("On OpenStandardMiningChannel best candidate is: {:?}", up);
                Some(up?)
            }
            // Variant just used for phantom data is ok to panic
            MiningRoutingLogic::_P(_) => panic!("Must use either MiningRoutingLogic::None or MiningRoutingLogic::Proxy for `routing_logic` param"),
            _ => unreachable!()
        };

        let channel_id = upstream
            .as_ref()
            .expect("No upstream initialized")
            .safe_lock(|s| s.channel_ids.safe_lock(|r| r.next()).unwrap())
            .unwrap();
        let cloned = upstream.as_ref().expect("No upstream initialized").clone();

        upstream
            .as_ref()
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
        info!("Received SubmitSharesStandard");
        debug!("SubmitSharesStandard {:?}", m);
        // TODO maybe we want to check if shares meet target before
        // sending them upstream If that is the case it should be
        // done by GroupChannel not here
        match &self.status {
            DownstreamMiningNodeStatus::Initializing => todo!(),
            DownstreamMiningNodeStatus::Paired(_) => todo!(),
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DownstreamHomUpstreamGroup {
                ..
            }) => {
                let remote = self.upstream.as_ref().unwrap();
                let message = Mining::SubmitSharesStandard(m);
                Ok(SendTo::RelayNewMessageToRemote(remote.clone(), message))
            }
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DownstreamHomUpstreamExtended {
                ..
            }) => {
                // Safe unwrap is channel have been opened it means that the dowsntream is paired
                // with an upstream
                let remote = self.upstream.as_ref().unwrap();
                let res = UpstreamMiningNode::handle_std_shr(remote.clone(), m).unwrap();
                Ok(SendTo::Respond(res))
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

impl ParseCommonMessagesFromDownstream for DownstreamMiningNode {
    fn handle_setup_connection(
        &mut self,
        m: SetupConnection,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            m.min_version, m.flags
        );
        let routing_logic = super::get_common_routing_logic();
        match routing_logic {
            CommonRoutingLogic::Proxy(r_logic) => {
                trace!("On SetupConnection r_logic is {:?}", r_logic);
                let result = r_logic.safe_lock(|r_logic| r_logic.on_setup_connection(&m))?;
                let (data, message) = result?;
                let upstream = match super::get_routing_logic() {
                    MiningRoutingLogic::Proxy(proxy_routing) => proxy_routing
                        .safe_lock(|r| r.downstream_to_upstream_map.get(&data).unwrap()[0].clone())
                        .unwrap(),
                    _ => unreachable!(),
                };
                self.upstream = Some(upstream);

                self.status.pair(data);
                Ok(SendToCommon::RelayNewMessageToRemote(
                    Arc::new(Mutex::new(())),
                    message.into(),
                ))
            }
            _ => unreachable!(),
        }
    }
}

pub async fn listen_for_downstream_mining(
    listener: TcpListener,
    mut shutdown_rx: TokioReceiver<()>,
) {
    let mut ids = roles_logic_sv2::utils::Id::new();
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, _) = accept_result.expect("failed to accept downstream connection");
                let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                    PlainConnection::new(stream).await;
                let node = DownstreamMiningNode::new(receiver, sender, ids.next());

                let mut incoming: StdFrame =
                    node.receiver.recv().await.unwrap().try_into().unwrap();
                let message_type = incoming.get_header().unwrap().msg_type();
                let payload = incoming.payload();
                let node = Arc::new(Mutex::new(node));

                // Call handle_setup_connection or fail
                let common_msg = DownstreamMiningNode::handle_message_common(
                    node.clone(),
                    message_type,
                    payload,
                ).expect("failed to process downstream message");


                if let SendToCommon::RelayNewMessageToRemote(_, relay_msg) = common_msg {
                    if let roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(setup_msg) = relay_msg {
                        DownstreamMiningNode::start(node, setup_msg).await;
                    }
                } else {
                    warn!("Received unexpected message from downstream");
                }
            }
            _ = &mut shutdown_rx => {
                info!("Closing listener");
                return;
            }
        }
    }
}

impl IsDownstream for DownstreamMiningNode {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => data,
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DownstreamHomUpstreamGroup {
                data,
                ..
            }) => data,
            DownstreamMiningNodeStatus::ChannelOpened(Channel::DownstreamHomUpstreamExtended {
                data,
                ..
            }) => data,
        }
    }
}
impl IsMiningDownstream for DownstreamMiningNode {}
