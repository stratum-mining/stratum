use crate::EXTRANONCE_RANGE_1_LENGTH;
use roles_logic_sv2::utils::Id;

use super::downstream_mining::{Channel, DownstreamMiningNode, StdFrame as DownstreamFrame};
use async_channel::{Receiver, SendError, Sender};
use async_recursion::async_recursion;
use codec_sv2::{Frame, HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    bitcoin::TxOut,
    channel_logic::{
        channel_factory::{ExtendedChannelKind, OnNewShare, ProxyExtendedChannelFactory, Share},
        proxy_group_channel::GroupChannels,
    },
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{
        IsMiningDownstream, IsMiningUpstream, IsUpstream, RequestIdMapper, UpstreamChannel,
    },
    errors::Error,
    handlers::mining::{ParseUpstreamMiningMessages, SendTo, SupportedChannelTypes},
    job_creator::JobsCreators,
    job_dispatcher::GroupChannelJobDispatcher,
    mining_sv2::*,
    parsers::{CommonMessages, Mining, MiningDeviceMessages, PoolMessages},
    routing_logic::MiningProxyRoutingLogic,
    selectors::{DownstreamMiningSelector, ProxyDownstreamMiningSelector as Prs},
    template_distribution_sv2::{
        NewTemplate, SetNewPrevHash as SetNewPrevHashTemplate, SubmitSolution,
    },
    utils::{GroupId, Mutex},
};
use std::{collections::HashMap, sync::Arc};
use tokio::{net::TcpStream, task};
use tracing::error;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
pub type ProxyRemoteSelector = Prs<DownstreamMiningNode>;

use std::sync::atomic::AtomicBool;
/// USED to make sure that if a future new_temnplate and a set_new_prev_hash are received together
/// the future new_temnplate is always handled before the set new prev hash.
pub static IS_NEW_TEMPLATE_HANDLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug)]
pub enum ChannelKind {
    Group(GroupChannels),
    Extended(Option<ProxyExtendedChannelFactory>),
    ExtendedWithNegotiator(Option<ProxyExtendedChannelFactory>),
}
impl ChannelKind {
    pub fn is_extended(&self) -> bool {
        match self {
            ChannelKind::Group(_) => false,
            ChannelKind::Extended(_) => true,
            ChannelKind::ExtendedWithNegotiator(_) => true,
        }
    }

    fn is_initialized(&self) -> bool {
        !matches!(
            self,
            ChannelKind::Extended(None) | ChannelKind::ExtendedWithNegotiator(None)
        )
    }

    fn get_factory(&mut self) -> &mut ProxyExtendedChannelFactory {
        match self {
            ChannelKind::Extended(Some(f)) => f,
            ChannelKind::ExtendedWithNegotiator(Some(f)) => f,
            _ => panic!("Channel factory not available"),
        }
    }

    fn has_factory(&mut self) -> bool {
        matches!(
            self,
            ChannelKind::Extended(Some(_)) | ChannelKind::ExtendedWithNegotiator(Some(_))
        )
    }

    fn initialize_factory(
        &mut self,
        group_id: Arc<Mutex<GroupId>>,
        extranonces: ExtendedExtranonce,
        downstream_share_per_minute: f32,
        upstream_target: Target,
        len: u16,
        up_id: u32,
    ) {
        match self {
            ChannelKind::Group(_) => panic!("Impossible to initialize factory for group channel"),
            ChannelKind::Extended(Some(_)) => panic!("Factory already initialized"),
            ChannelKind::ExtendedWithNegotiator(Some(_)) => panic!("Factory already initialized"),
            ChannelKind::Extended(None) => {
                let kind = ExtendedChannelKind::Proxy { upstream_target };
                let factory = ProxyExtendedChannelFactory::new(
                    group_id,
                    extranonces,
                    None,
                    downstream_share_per_minute,
                    kind,
                    Some(vec![]),
                    up_id,
                );
                *self = Self::Extended(Some(factory));
            }
            ChannelKind::ExtendedWithNegotiator(None) => {
                let kind = ExtendedChannelKind::ProxyJn { upstream_target };
                let job_creator: JobsCreators = JobsCreators::new(len as u8);
                let factory = ProxyExtendedChannelFactory::new(
                    group_id,
                    extranonces,
                    Some(job_creator),
                    downstream_share_per_minute,
                    kind,
                    Some(vec![]),
                    up_id,
                );
                *self = Self::ExtendedWithNegotiator(Some(factory));
            }
        }
    }
}

impl From<crate::ChannelKind> for ChannelKind {
    fn from(v: crate::ChannelKind) -> Self {
        match v {
            crate::ChannelKind::Group => Self::Group(GroupChannels::new()),
            crate::ChannelKind::Extended => Self::Extended(None),
            crate::ChannelKind::ExtendedWithNegotiator => Self::ExtendedWithNegotiator(None),
        }
    }
}

/// 1 to 1 connection with a pool
/// Can be either a mining pool or another proxy
/// 1 to 1 connection with an upstream node that implement the mining (sub)protocol can be either a a pool or an
/// upstream proxy.
#[derive(Debug, Clone)]
struct UpstreamMiningConnection {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
}

impl UpstreamMiningConnection {
    async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), SendError<EitherFrame>> {
        let either_frame = sv2_frame.into();
        match self.sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sv2MiningConnection {
    version: u16,
    setup_connection_flags: u32,
    #[allow(dead_code)]
    setup_connection_success_flags: u32,
}

// Efficient stack do use JobDispatcher so the smaller variant (None) do not impact performance
// cause is used in already non performant environments. That to justify the below allow.
// https://rust-lang.github.io/rust-clippy/master/index.html#large_enum_varianT
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum JobDispatcher {
    Group(GroupChannelJobDispatcher),
    None,
}

/// Can be either a mining pool or another proxy
#[derive(Debug)]
pub struct UpstreamMiningNode {
    id: u32,
    total_hash_rate: u64,
    address: SocketAddr,
    connection: Option<UpstreamMiningConnection>,
    sv2_connection: Option<Sv2MiningConnection>,
    authority_public_key: [u8; 32],
    /// group_channel id/channel_id -> dispatcher
    pub channel_id_to_job_dispatcher: HashMap<u32, JobDispatcher>,
    /// Each relayed message that has a `request_id` field must have a unique `request_id` number,
    /// connection-wise.
    /// The `request_id` from the downstream is NOT guaranteed to be unique, so it must be changed.
    request_id_mapper: RequestIdMapper,
    downstream_selector: ProxyRemoteSelector,
    pub channel_kind: ChannelKind,
    group_id: Arc<Mutex<GroupId>>,
    recv_tp: Option<Receiver<(NewTemplate<'static>, u64)>>,
    recv_ph: Option<Receiver<(SetNewPrevHashTemplate<'static>, u64)>>,
    request_ids: Arc<Mutex<Id>>,
    pub channel_ids: Arc<Mutex<Id>>,
    downstream_share_per_minute: f32,
    pub solution_sender: Option<Sender<SubmitSolution<'static>>>,
    pub recv_coinbase_out: Option<Receiver<(Vec<TxOut>, u64)>>,
    #[allow(dead_code)]
    tx_outs: HashMap<u64, Vec<TxOut>>,
    first_ph_received: bool,
    // When a future job is recieved from an extended channel this is transformed to severla std
    // job for HOM downstream. If the job is future we need to keep track of the original job id and
    // the new job ids used for the std job and also which downstream received which id. When a set
    // new prev hash is received if it refer one of these ids we use this map and build the right
    // set new pre hash for each downstream.
    #[allow(clippy::type_complexity)]
    job_up_to_down_ids: HashMap<u32, Vec<(Arc<Mutex<DownstreamMiningNode>>, u32)>>,
}

use core::convert::TryInto;
use std::net::SocketAddr;
use tracing::{debug, info};

/// It assume that endpoint NEVER change flags and version!
/// I can open both extended and group channel with upstream.
impl UpstreamMiningNode {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        address: SocketAddr,
        authority_public_key: [u8; 32],
        channel_kind: crate::ChannelKind,
        group_id: Arc<Mutex<GroupId>>,
        recv_tp: Option<Receiver<(NewTemplate<'static>, u64)>>,
        recv_ph: Option<Receiver<(SetNewPrevHashTemplate<'static>, u64)>>,
        request_ids: Arc<Mutex<Id>>,
        channel_ids: Arc<Mutex<Id>>,
        downstream_share_per_minute: f32,
        solution_sender: Option<Sender<SubmitSolution<'static>>>,
        recv_coinbase_out: Option<Receiver<(Vec<TxOut>, u64)>>,
    ) -> Self {
        let request_id_mapper = RequestIdMapper::new();
        let downstream_selector = ProxyRemoteSelector::new();
        Self {
            id,
            total_hash_rate: 0,
            address,
            connection: None,
            sv2_connection: None,
            authority_public_key,
            channel_id_to_job_dispatcher: HashMap::new(),
            request_id_mapper,
            downstream_selector,
            channel_kind: channel_kind.into(),
            group_id,
            recv_tp,
            recv_ph,
            request_ids,
            channel_ids,
            downstream_share_per_minute,
            solution_sender,
            recv_coinbase_out,
            tx_outs: HashMap::new(),
            first_ph_received: false,
            job_up_to_down_ids: HashMap::new(),
        }
    }

    pub fn start_receiving_pool_coinbase_outs(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            let recv = self_mutex
                .safe_lock(|s| s.recv_coinbase_out.clone())
                .unwrap()
                .unwrap();
            while !self_mutex
                .safe_lock(|s| s.channel_kind.has_factory())
                .unwrap()
            {
                tokio::task::yield_now().await;
            }
            while let Ok((outs, _id)) = recv.recv().await {
                // TODO assuming that only one coinbase is negotiated with the pool not handling
                // different tokens, in order to do that we can use the out hashmap:
                // self.tx_outs.insert(id, outs)
                self_mutex
                    .safe_lock(|s| {
                        match &mut s.channel_kind {
                            ChannelKind::Group(_) => unreachable!(),
                            ChannelKind::Extended(_) => unreachable!(),
                            ChannelKind::ExtendedWithNegotiator(factory) => {
                                factory.as_mut().unwrap().update_pool_outputs(outs)
                            }
                        };
                    })
                    .unwrap();
            }
        });
    }

    pub fn start_receiving_new_template(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            Self::wait_for_channel_factory(self_mutex.clone()).await;
            let new_template_reciver = self_mutex
                .safe_lock(|a| a.recv_tp.clone())
                .unwrap()
                .unwrap();
            while !self_mutex.safe_lock(|s| s.first_ph_received).unwrap() {
                tokio::task::yield_now().await;
            }
            loop {
                let (mut message_new_template, token): (NewTemplate, u64) =
                    new_template_reciver.recv().await.unwrap();
                let (channel_id_to_new_job_msg, custom_job) = self_mutex
                    .safe_lock(|a| {
                        a.channel_kind
                            .get_factory()
                            .on_new_template(&mut message_new_template) // This is not efficient, to be changed if needed
                    })
                    .unwrap()
                    .unwrap();
                if let Some(custom_job) = custom_job {
                    let req_id = self_mutex
                        .safe_lock(|s| s.request_ids.safe_lock(|r| r.next()).unwrap())
                        .unwrap();
                    let custom_mining_job =
                        PoolMessages::Mining(Mining::SetCustomMiningJob(SetCustomMiningJob {
                            channel_id: self_mutex.safe_lock(|s| s.id).unwrap(),
                            request_id: req_id,
                            version: custom_job.version,
                            prev_hash: custom_job.prev_hash,
                            min_ntime: custom_job.min_ntime,
                            nbits: custom_job.nbits,
                            coinbase_tx_version: custom_job.coinbase_tx_version,
                            coinbase_prefix: custom_job.coinbase_prefix.clone(),
                            coinbase_tx_input_n_sequence: custom_job.coinbase_tx_input_n_sequence,
                            coinbase_tx_value_remaining: custom_job.coinbase_tx_value_remaining,
                            coinbase_tx_outputs: custom_job.coinbase_tx_outputs.clone(),
                            coinbase_tx_locktime: custom_job.coinbase_tx_locktime,
                            merkle_path: custom_job.merkle_path,
                            extranonce_size: custom_job.extranonce_size,
                            future_job: message_new_template.future_template,
                            token,
                        }));
                    Self::send(self_mutex.clone(), custom_mining_job.try_into().unwrap())
                        .await
                        .unwrap();
                }

                for (id, job) in channel_id_to_new_job_msg {
                    let downstream = self_mutex
                        .safe_lock(|a| a.downstream_selector.downstream_from_channel_id(id))
                        .unwrap()
                        .unwrap();
                    let message = MiningDeviceMessages::Mining(job);
                    DownstreamMiningNode::send(downstream, message.try_into().unwrap())
                        .await
                        .unwrap();
                }
                IS_NEW_TEMPLATE_HANDLED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }

    pub fn start_receiving_new_prev_hash(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            Self::wait_for_channel_factory(self_mutex.clone()).await;
            let prev_hash_reciver = self_mutex
                .safe_lock(|a| a.recv_ph.clone())
                .unwrap()
                .unwrap();
            loop {
                let (message_prev_hash, token) = prev_hash_reciver.recv().await.unwrap();
                let custom = self_mutex
                    .safe_lock(|a| {
                        a.channel_kind
                            .get_factory()
                            .on_new_prev_hash_from_tp(&message_prev_hash)
                    })
                    .unwrap();
                self_mutex
                    .safe_lock(|s| s.first_ph_received = true)
                    .unwrap();

                if let Ok(Some(custom_job)) = custom {
                    let req_id = self_mutex
                        .safe_lock(|s| s.request_ids.safe_lock(|r| r.next()).unwrap())
                        .unwrap();
                    let custom_mining_job =
                        PoolMessages::Mining(Mining::SetCustomMiningJob(SetCustomMiningJob {
                            channel_id: self_mutex.safe_lock(|s| s.id).unwrap(),
                            request_id: req_id,
                            version: custom_job.version,
                            prev_hash: custom_job.prev_hash,
                            min_ntime: custom_job.min_ntime,
                            nbits: custom_job.nbits,
                            coinbase_tx_version: custom_job.coinbase_tx_version,
                            coinbase_prefix: custom_job.coinbase_prefix.clone(),
                            coinbase_tx_input_n_sequence: custom_job.coinbase_tx_input_n_sequence,
                            coinbase_tx_value_remaining: custom_job.coinbase_tx_value_remaining,
                            coinbase_tx_outputs: custom_job.coinbase_tx_outputs.clone(),
                            coinbase_tx_locktime: custom_job.coinbase_tx_locktime,
                            merkle_path: custom_job.merkle_path,
                            extranonce_size: custom_job.extranonce_size,
                            future_job: false,
                            token,
                        }));
                    Self::send(self_mutex.clone(), custom_mining_job.try_into().unwrap())
                        .await
                        .unwrap();
                }
            }
        });
    }

    /// Try send a message to the upstream node.
    /// If the node is connected and there are no error return Ok(())
    /// If the node is connected and there is an error the message is not sent and an error is
    ///     returned and the upstream is marked as not connected.
    /// If the node is not connected it try to connect and send the message and everything is ok
    ///     the upstream is marked as connected and Ok(()) is returned if not an error is returned.
    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        sv2_frame: StdFrame,
    ) -> Result<(), crate::lib::error::Error> {
        let (has_sv2_connetcion, mut connection) = self_mutex
            .safe_lock(|self_| (self_.sv2_connection.is_some(), self_.connection.clone()))
            .unwrap();
        //let mut self_ = self_mutex.lock().await;

        match (connection.as_mut(), has_sv2_connetcion) {
            (Some(connection), true) => match connection.send(sv2_frame).await {
                Ok(_) => Ok(()),
                Err(_e) => {
                    Self::connect(self_mutex.clone()).await.unwrap();
                    // It assume that enpoint NEVER change flags and version!
                    match Self::setup_connection(self_mutex).await {
                        Ok(()) => Ok(()),
                        Err(()) => panic!(),
                    }
                }
            },
            // It assume that no downstream try to send messages before that the upstream is
            // initialized. This assumption is enforced by the fact that
            // UpstreamMiningNode::pair only pair downstream noder with already
            // initialized upstream nodes!
            (Some(connection), false) => match connection.send(sv2_frame).await {
                Ok(_) => Ok(()),
                Err(e) => Err(e.into()),
            },
            (None, _) => {
                Self::connect(self_mutex.clone()).await?;
                let mut connection = self_mutex
                    .safe_lock(|self_| self_.connection.clone())
                    .unwrap();
                match connection.as_mut().unwrap().send(sv2_frame).await {
                    Ok(_) => match Self::setup_connection(self_mutex).await {
                        Ok(()) => Ok(()),
                        Err(()) => panic!(),
                    },
                    Err(e) => {
                        //Self::connect(self_mutex.clone()).await.unwrap();
                        Err(e.into())
                    }
                }
            }
        }
    }

    async fn receive(self_mutex: Arc<Mutex<Self>>) -> Result<StdFrame, crate::lib::error::Error> {
        let mut connection = self_mutex
            .safe_lock(|self_| self_.connection.clone())
            .unwrap();
        match connection.as_mut() {
            Some(connection) => match connection.receiver.recv().await {
                Ok(m) => Ok(m.try_into().unwrap()),
                Err(_) => {
                    let address = self_mutex.safe_lock(|s| s.address).unwrap();
                    Err(crate::lib::error::Error::UpstreamNotAvailabe(address))
                }
            },
            None => todo!(),
        }
    }

    async fn connect(self_mutex: Arc<Mutex<Self>>) -> Result<(), crate::lib::error::Error> {
        let has_connection = self_mutex
            .safe_lock(|self_| self_.connection.is_some())
            .unwrap();
        match has_connection {
            true => Ok(()),
            false => {
                let (address, authority_public_key) = self_mutex
                    .safe_lock(|self_| (self_.address, self_.authority_public_key))
                    .unwrap();
                let socket = TcpStream::connect(address)
                    .await
                    .map_err(|_| crate::lib::error::Error::UpstreamNotAvailabe(address))?;
                let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
                let (receiver, sender) =
                    Connection::new(socket, HandshakeRole::Initiator(initiator)).await;
                let connection = UpstreamMiningConnection { receiver, sender };
                self_mutex
                    .safe_lock(|self_| {
                        self_.connection = Some(connection);
                    })
                    .unwrap();
                Ok(())
            }
        }
    }

    #[async_recursion]
    async fn setup_connection(self_mutex: Arc<Mutex<Self>>) -> Result<(), ()> {
        let sv2_connection = self_mutex.safe_lock(|self_| self_.sv2_connection).unwrap();

        match sv2_connection {
            None => Ok(()),
            Some(sv2_connection) => {
                let flags = sv2_connection.setup_connection_flags;
                let version = sv2_connection.version;
                let frame = self_mutex
                    .safe_lock(|self_| self_.new_setup_connection_frame(flags, version, version))
                    .unwrap();
                Self::send(self_mutex.clone(), frame)
                    .await
                    .map_err(|_| ())?;

                let cloned = self_mutex.clone();
                let mut response = task::spawn(async { Self::receive(cloned).await })
                    .await
                    .unwrap()
                    .unwrap();

                let message_type = response.get_header().unwrap().msg_type();
                let payload = response.payload();
                match (message_type, payload).try_into() {
                    Ok(CommonMessages::SetupConnectionSuccess(_)) => {
                        let receiver = self_mutex
                            .safe_lock(|self_| self_.connection.clone().unwrap().receiver)
                            .unwrap();
                        Self::relay_incoming_messages(self_mutex, receiver);
                        Ok(())
                    }
                    _ => panic!(),
                }
            }
        }
    }

    fn relay_incoming_messages(
        self_: Arc<Mutex<Self>>,
        //_downstreams: HashMap<u32, Downstream>,
        receiver: Receiver<EitherFrame>,
    ) {
        task::spawn(async move {
            loop {
                if let Ok(message) = receiver.recv().await {
                    let incoming: StdFrame = message.try_into().unwrap();
                    Self::next(self_.clone(), incoming).await;
                } else {
                    Self::exit(self_);
                    break;
                }
            }
        });
    }
    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn remove_dowstream(self_: Arc<Mutex<Self>>, down: &Arc<Mutex<DownstreamMiningNode>>) {
        self_
            .safe_lock(|s| s.downstream_selector.remove_downstream(down))
            .unwrap();
    }

    fn exit(self_: Arc<Mutex<Self>>) {
        crate::remove_upstream(self_.safe_lock(|s| s.id).unwrap());
        let downstreams = self_
            .safe_lock(|s| s.downstream_selector.get_all_downstreams())
            .unwrap();
        let mut dowstreams_: Vec<Arc<Mutex<DownstreamMiningNode>>> = vec![];
        for d in downstreams {
            if let Some(id) = d
                .safe_lock(|d| match &d.status {
                    super::downstream_mining::DownstreamMiningNodeStatus::Initializing => None,
                    super::downstream_mining::DownstreamMiningNodeStatus::Paired(_) => None,
                    super::downstream_mining::DownstreamMiningNodeStatus::ChannelOpened(
                        channel,
                    ) => match channel {
                        Channel::DowntreamHomUpstreamGroup { channel_id, .. } => Some(*channel_id),
                        Channel::DowntreamHomUpstreamExtended { channel_id, .. } => {
                            Some(*channel_id)
                        }
                        Channel::DowntreamNonHomUpstreamExtended { .. } => todo!(),
                    },
                })
                .unwrap()
            {
                self_
                    .safe_lock(|s| s.downstream_selector.remove_downstreams_in_channel(id))
                    .unwrap();
                {
                    dowstreams_.push(d);
                }
            }
        }
        for d in dowstreams_ {
            // TODO make sure that each reference have been dropped
            if dbg!(Arc::strong_count(&d)) > 1 {
                //todo!()
            }
            DownstreamMiningNode::exit(d);
        }
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let routing_logic = crate::get_routing_logic();

        let next_message_to_send = UpstreamMiningNode::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            routing_logic,
        );
        match next_message_to_send {
            Ok(SendTo::RelaySameMessageToRemote(downstream)) => {
                let sv2_frame: codec_sv2::Sv2Frame<MiningDeviceMessages, buffer_sv2::Slice> =
                    incoming.map(|payload| payload.try_into().unwrap());

                DownstreamMiningNode::send(downstream.clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::RelayNewMessageToRemote(downstream_mutex, message)) => {
                let message = MiningDeviceMessages::Mining(message);
                let frame: DownstreamFrame = message.try_into().unwrap();
                DownstreamMiningNode::send(downstream_mutex, frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::Respond(message)) => {
                let message = PoolMessages::Mining(message);
                let frame: StdFrame = message.try_into().unwrap();
                UpstreamMiningNode::send(self_mutex, frame).await.unwrap();
            }
            Ok(SendTo::Multiple(sends_to)) => {
                for send_to in sends_to {
                    match send_to {
                        SendTo::RelayNewMessageToRemote(downstream_mutex, message) => {
                            let message = MiningDeviceMessages::Mining(message);
                            let frame: DownstreamFrame = message.try_into().unwrap();
                            DownstreamMiningNode::send(downstream_mutex, frame)
                                .await
                                .unwrap();
                        }
                        SendTo::RelaySameMessageToRemote(downstream_mutex) => {
                            let frame: codec_sv2::Sv2Frame<
                                MiningDeviceMessages,
                                buffer_sv2::Slice,
                            > = incoming.clone().map(|payload| payload.try_into().unwrap());
                            DownstreamMiningNode::send(downstream_mutex, frame)
                                .await
                                .unwrap();
                        }
                        SendTo::Respond(message) => {
                            let message = PoolMessages::Mining(message);
                            let frame: StdFrame = message.try_into().unwrap();
                            UpstreamMiningNode::send(self_mutex.clone(), frame)
                                .await
                                .unwrap();
                        }
                        SendTo::None(_) => (),
                        SendTo::Multiple(_) => panic!("Nested SendTo::Multiple not supported"),
                        _ => panic!(),
                    }
                }
            }
            Ok(SendTo::None(_)) => (),
            Ok(_) => panic!(),
            Err(Error::NoDownstreamsConnected) => (),
            Err(e) => panic!("{:?}", e),
        }
    }

    #[async_recursion]
    async fn setup_flag_and_version(
        self_mutex: Arc<Mutex<Self>>,
        flags: Option<u32>,
        min_version: u16,
        max_version: u16,
    ) -> Result<(), crate::lib::error::Error> {
        let flags = flags.unwrap_or(0b0000_0000_0000_0000_0000_0000_0000_0110);
        let frame = self_mutex
            .safe_lock(|self_| self_.new_setup_connection_frame(flags, min_version, max_version))
            .unwrap();
        Self::send(self_mutex.clone(), frame).await?;

        let cloned = self_mutex.clone();
        let mut response = task::spawn(async { Self::receive(cloned).await })
            .await
            .unwrap()
            .unwrap();

        let message_type = response.get_header().unwrap().msg_type();
        let payload = response.payload();
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => {
                let receiver = self_mutex
                    .safe_lock(|self_| {
                        self_.sv2_connection = Some(Sv2MiningConnection {
                            version: m.used_version,
                            setup_connection_flags: flags,
                            setup_connection_success_flags: m.flags,
                        });
                        self_.connection.clone().unwrap().receiver
                    })
                    .unwrap();
                Self::relay_incoming_messages(self_mutex.clone(), receiver);
                if self_mutex
                    .safe_lock(|s| s.channel_kind.is_extended())
                    .unwrap()
                {
                    Self::open_extended_channel(self_mutex.clone()).await
                }
                Ok(())
            }
            Ok(CommonMessages::SetupConnectionError(m)) => {
                if m.flags != 0 {
                    let flags = flags ^ m.flags;
                    // We need to send SetupConnection again as we do not yet know the version of
                    // upstream
                    // debounce this?
                    Self::setup_flag_and_version(self_mutex, Some(flags), min_version, max_version)
                        .await
                } else {
                    let error_message = std::str::from_utf8(m.error_code.inner_as_ref())
                        .unwrap()
                        .to_string();
                    Err(crate::lib::error::Error::SetupConnectionError(
                        error_message,
                    ))
                }
            }
            Ok(_) => todo!(),
            Err(_) => todo!(),
        }
    }

    async fn open_extended_channel(self_mutex: Arc<Mutex<Self>>) {
        let message = PoolMessages::Mining(Mining::OpenExtendedMiningChannel(
            OpenExtendedMiningChannel {
                request_id: 0,
                user_identity: "proxy".to_string().try_into().unwrap(),
                nominal_hash_rate: 100_000_000_000_000.0,
                max_target: [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                ]
                .try_into()
                .unwrap(),
                min_extranonce_size: crate::MIN_EXTRANONCE_SIZE,
            },
        ));
        #[allow(unused_must_use)]
        Self::send(self_mutex.clone(), message.try_into().unwrap())
            .await
            .unwrap();

        Self::wait_for_channel_factory(self_mutex).await;
    }

    async fn wait_for_channel_factory(self_mutex: Arc<Mutex<UpstreamMiningNode>>) {
        while !self_mutex
            .safe_lock(|s| s.channel_kind.is_initialized())
            .unwrap()
        {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    fn new_setup_connection_frame(
        &self,
        flags: u32,
        min_version: u16,
        max_version: u16,
    ) -> StdFrame {
        let endpoint_host = self
            .address
            .ip()
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = String::new().try_into().unwrap();
        let setup_connection: PoolMessages = SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version,
            max_version,
            flags,
            endpoint_host,
            endpoint_port: self.address.port(),
            vendor,
            hardware_version,
            firmware,
            device_id,
        }
        .into();
        setup_connection.try_into().unwrap()
    }

    pub fn open_standard_channel_down(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        id_header_only: bool,
        channel_id: u32,
    ) -> Vec<Mining<'static>> {
        match &mut self.channel_kind {
            // When channel kind is Group (that means that no extended channels is open between
            // proxy and this upstream) opening channel is handled by upstream and proxy must only
            // relay messages
            ChannelKind::Group(_) => {
                panic!("Open satandard channel down for group up not supported")
            }
            ChannelKind::Extended(Some(factory)) => {
                self.downstream_selector
                    .on_open_standard_channel_success(request_id, 0, channel_id)
                    .unwrap();
                let messages = factory
                    .add_standard_channel(
                        request_id,
                        downstream_hash_rate,
                        id_header_only,
                        channel_id,
                    )
                    .unwrap();
                messages.into_iter().map(|x| x.into_static()).collect()
            }
            ChannelKind::ExtendedWithNegotiator(Some(factory)) => {
                self.downstream_selector
                    .on_open_standard_channel_success(request_id, 0, channel_id)
                    .unwrap();
                let messages = factory
                    .add_standard_channel(
                        request_id,
                        downstream_hash_rate,
                        id_header_only,
                        channel_id,
                    )
                    .unwrap();
                messages.into_iter().map(|x| x.into_static()).collect()
            }
            _ => panic!("Channel factory not initialized"),
        }
    }

    pub fn handle_std_shr(
        self_: Arc<Mutex<Self>>,
        share_: SubmitSharesStandard,
    ) -> Result<Mining<'static>, Error> {
        if self_.safe_lock(|s| s.channel_kind.is_extended()).unwrap() {
            let share = self_
                .safe_lock(|s| {
                    let factory = s.channel_kind.get_factory();
                    factory.on_submit_shares_standard(share_.clone())
                })
                .unwrap()?;
            match share {
                OnNewShare::SendErrorDownstream(e) => Ok(Mining::SubmitSharesError(e)),
                OnNewShare::SendSubmitShareUpstream(s) => match s {
                    Share::Extended(s) => {
                        let message = Mining::SubmitSharesExtended(s);
                        let message = PoolMessages::Mining(message);
                        let frame: StdFrame = message.try_into().unwrap();
                        tokio::task::spawn(async move {
                            UpstreamMiningNode::send(self_.clone(), frame)
                                .await
                                .unwrap();
                        });
                        let success = SubmitSharesSuccess {
                            channel_id: share_.channel_id,
                            last_sequence_number: share_.sequence_number,
                            new_submits_accepted_count: 1,
                            new_shares_sum: 1,
                        };
                        let message = Mining::SubmitSharesSuccess(success);
                        Ok(message)
                    }
                    Share::Standard(_) => unreachable!(),
                },
                OnNewShare::RelaySubmitShareUpstream => todo!(),
                OnNewShare::ShareMeetBitcoinTarget((share, template_id, coinbase)) => match share {
                    Share::Extended(s) => {
                        let solution = SubmitSolution {
                            template_id,
                            version: s.version,
                            header_timestamp: s.ntime,
                            header_nonce: s.nonce,
                            coinbase_tx: coinbase.try_into().unwrap(),
                        };
                        let sender = self_
                            .safe_lock(|s| s.solution_sender.clone())
                            .unwrap()
                            .unwrap();
                        // The below channel should never be full is ok to block
                        sender.send_blocking(solution).unwrap();

                        let message = Mining::SubmitSharesExtended(s);
                        let message = PoolMessages::Mining(message);
                        let frame: StdFrame = message.try_into().unwrap();
                        tokio::task::spawn(async move {
                            UpstreamMiningNode::send(self_.clone(), frame)
                                .await
                                .unwrap();
                        });
                        let success = SubmitSharesSuccess {
                            channel_id: share_.channel_id,
                            last_sequence_number: share_.sequence_number,
                            new_submits_accepted_count: 1,
                            new_shares_sum: 1,
                        };
                        let message = Mining::SubmitSharesSuccess(success);
                        Ok(message)
                    }
                    Share::Standard(_) => {
                        // on_submit_shares_standard call check_target that in the case of a Proxy
                        // and a share that is below the bitcoin target if the share is a standard
                        // share call share.into_extended making this branch unreachable.
                        unreachable!()
                    }
                },
                OnNewShare::ShareMeetDownstreamTarget => {
                    let success = SubmitSharesSuccess {
                        channel_id: share_.channel_id,
                        last_sequence_number: share_.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum: 1,
                    };
                    let message = Mining::SubmitSharesSuccess(success);
                    Ok(message)
                }
            }
        } else {
            unreachable!("Calling share_into_extended for an non extended upstream make no sense")
        }
    }

    // Example of how next could be implemented more efficently if no particular good log are
    // needed it just relay the majiority of messages downstream without serializing and
    // deserializing them. In order to find the Downstream at which the message must bu relayed the
    // channel id must be deserialized, but knowing the message type that is a very easy task is
    // either 4 bytes after the header or the first 4 bytes after the header + 4 bytes
    // #[cfg(test)]
    // #[allow(unused)]
    // pub async fn next_faster(&mut self, mut incoming: StdFrame) {
    //     let message_type = incoming.get_header().unwrap().msg_type();

    //     // When a channel is opened we need to setup the channel id in order to relay next messages
    //     // to the right Downstream
    //     if todo!() { // check if message_type is channel related

    //         // When a mining message is received (that is not a channel related message) always relay it downstream
    //     } else if todo!()  { // check if message_type is is a mining message
    //         // everything here can be just relayed downstream

    //         // Other sub(protocol) messages
    //     } else {
    //         todo!()
    //     }
    // }
}

impl
    ParseUpstreamMiningMessages<
        DownstreamMiningNode,
        ProxyRemoteSelector,
        MiningProxyRoutingLogic<DownstreamMiningNode, Self, ProxyRemoteSelector>,
    > for UpstreamMiningNode
{
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled(&self) -> bool {
        true
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
        remote: Option<Arc<Mutex<DownstreamMiningNode>>>,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        match &mut self.channel_kind {
            ChannelKind::Group(group) => {
                let down_is_header_only = remote
                    .as_ref()
                    .unwrap()
                    .safe_lock(|remote| remote.is_header_only())
                    .unwrap();
                let remote = remote.unwrap();
                if down_is_header_only {
                    let mut res = vec![SendTo::RelaySameMessageToRemote(remote.clone())];
                    for message in group.on_channel_success_for_hom_downtream(&m)? {
                        res.push(SendTo::RelayNewMessageToRemote(remote.clone(), message));
                    }
                    remote
                        .safe_lock(|r| {
                            r.open_channel_for_down_hom_up_group(m.channel_id, m.group_channel_id)
                        })
                        .unwrap();
                    Ok(SendTo::Multiple(res))
                } else {
                    // Here we want to support only the case where downstream is non HOM and want to open
                    // extended channels with the proxy. Dowstream non HOM that try to open standard
                    // channel (grouped in groups) do not make much sense so for now is not supported
                    panic!()
                }
            }
            // If we opened and extended channel upstreams we should not receive this message
            ChannelKind::Extended(_) => todo!(),
            ChannelKind::ExtendedWithNegotiator(_) => todo!(),
        }
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        let extranonce_prefix: Extranonce = m.extranonce_prefix.clone().try_into().unwrap();
        let range_0 = 0..m.extranonce_prefix.clone().to_vec().len();
        let range_1 = range_0.end..(range_0.end + EXTRANONCE_RANGE_1_LENGTH);
        let range_2 = range_1.end..(range_0.end + m.extranonce_size as usize);
        let extranonces = ExtendedExtranonce::from_upstream_extranonce(
            extranonce_prefix,
            range_0,
            range_1,
            range_2,
        )
        .unwrap();
        let len = if self.is_work_selection_enabled() {
            extranonces.get_len() as u16
        } else {
            m.extranonce_size as u16
        };

        self.channel_kind.initialize_factory(
            self.group_id.clone(),
            extranonces,
            self.downstream_share_per_minute,
            m.target.clone().try_into().unwrap(),
            len,
            m.channel_id,
        );
        Ok(SendTo::None(None))
    }

    fn handle_open_mining_channel_error(
        &mut self,
        _m: OpenMiningChannelError,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("460")
    }

    fn handle_update_channel_error(
        &mut self,
        _m: UpdateChannelError,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("470")
    }

    fn handle_close_channel(
        &mut self,
        _m: CloseChannel,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("480")
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        _m: SetExtranoncePrefix,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("490")
    }

    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        match &self
            .downstream_selector
            .downstream_from_channel_id(m.channel_id)
        {
            Some(d) => Ok(SendTo::RelaySameMessageToRemote(d.clone())),
            None => {
                info!("Share success");
                Ok(SendTo::None(None))
            }
        }
    }

    fn handle_submit_shares_error(
        &mut self,
        _m: SubmitSharesError,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        Ok(SendTo::None(None))
    }

    // TODO this is usefull only for non hom upstream do we really want to support non hom upstream
    // it do not make much sense IMO.
    // For now I comment the code and put here an Error
    fn handle_new_mining_job(
        &mut self,
        _m: NewMiningJob,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!()
        //// One and only one downstream cause the message is not extended
        //match &self
        //    .downstream_selector
        //    .get_downstreams_in_channel(m.channel_id)
        //{
        //    Some(downstreams) => {
        //        let downstream = &downstreams[0];
        //        crate::add_job_id(
        //            m.job_id,
        //            self.id,
        //            downstream.safe_lock(|d| d.prev_job_id).unwrap(),
        //        );
        //        Ok(SendTo::RelaySameMessageToRemote(downstream.clone()))
        //    }
        //    None => Err(Error::NoDownstreamsConnected),
        //}
    }

    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        debug!("Handling new extended mining job: {:?} {}", m, self.id);

        let mut res = vec![];
        match &mut self.channel_kind {
            ChannelKind::Group(group) => {
                group.on_new_extended_mining_job(&m);
                let downstreams = self
                    .downstream_selector
                    .get_downstreams_in_channel(m.channel_id)
                    .ok_or(Error::NoDownstreamsConnected)?;
                for downstream in downstreams {
                    match downstream.safe_lock(|r| r.get_channel().clone()).unwrap() {
                        Channel::DowntreamHomUpstreamGroup {
                            channel_id,
                            group_id,
                            ..
                        } => {
                            let message =
                                group.last_received_job_to_standard_job(channel_id, group_id)?;

                            res.push(SendTo::RelayNewMessageToRemote(
                                downstream.clone(),
                                Mining::NewMiningJob(message),
                            ));
                        }
                        _ => unreachable!(),
                    }
                }
            }
            ChannelKind::Extended(Some(factory)) => {
                if let Ok(messages) = factory.on_new_extended_mining_job(m.clone().as_static()) {
                    let mut new_p_hash_added = false;
                    let is_future = m.future_job;
                    let original_job_id = m.job_id;
                    if is_future {
                        self.job_up_to_down_ids.insert(original_job_id, vec![]);
                    };
                    for (id, message) in messages {
                        match &message {
                            Mining::NewExtendedMiningJob(_) => {
                                // TODO implement it if support for non HOM downstream is needed
                                todo!()
                            }
                            Mining::NewMiningJob(m) => {
                                let downstream = self
                                    .downstream_selector
                                    .downstream_from_channel_id(id)
                                    .ok_or(Error::NoDownstreamsConnected)?;
                                if is_future {
                                    let ids =
                                        self.job_up_to_down_ids.get_mut(&original_job_id).unwrap();
                                    ids.push((downstream.clone(), m.job_id));
                                };
                                res.push(SendTo::RelayNewMessageToRemote(
                                    downstream,
                                    Mining::NewMiningJob(m.clone()),
                                ));
                            }
                            Mining::SetNewPrevHash(m) => {
                                if !new_p_hash_added {
                                    let downstreams =
                                        self.downstream_selector.get_all_downstreams();
                                    for downstream in downstreams {
                                        res.push(SendTo::RelayNewMessageToRemote(
                                            downstream.clone(),
                                            Mining::SetNewPrevHash(m.clone()),
                                        ));
                                    }
                                    new_p_hash_added = true;
                                }
                            }
                            _ => todo!(),
                        }
                    }
                } else {
                    todo!()
                }
            }
            ChannelKind::Extended(None) => panic!("Factory not initialized"),
            // Just ignore it we use the negotiator
            ChannelKind::ExtendedWithNegotiator(_) => (),
        }
        Ok(SendTo::Multiple(res))
    }

    fn handle_set_new_prev_hash(
        &mut self,
        mut m: SetNewPrevHash,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        match &mut self.channel_kind {
            ChannelKind::Group(group) => {
                group.update_new_prev_hash(&m);

                let downstreams = self
                    .downstream_selector
                    .get_downstreams_in_channel(m.channel_id)
                    .ok_or(Error::NoDownstreamsConnected)?;

                let mut res = vec![];
                for downstream in downstreams {
                    let message = Mining::SetNewPrevHash(m.clone().into_static());
                    res.push(SendTo::RelayNewMessageToRemote(downstream.clone(), message));
                }
                Ok(SendTo::Multiple(res))
            }
            ChannelKind::Extended(factory) => {
                if factory
                    .as_mut()
                    .expect("Factory not initialized")
                    .on_new_prev_hash(m.clone().into_static())
                    .is_ok()
                {
                    match self.job_up_to_down_ids.get(&m.job_id) {
                        Some(downstreams) => {
                            let mut res = vec![];
                            for (downstream, job_id) in downstreams {
                                m.job_id = *job_id;
                                let message = Mining::SetNewPrevHash(m.clone().into_static());
                                res.push(SendTo::RelayNewMessageToRemote(
                                    downstream.clone(),
                                    message.clone(),
                                ));
                            }
                            self.job_up_to_down_ids = HashMap::new();
                            Ok(SendTo::Multiple(res))
                        }
                        None => {
                            let downstrems = self.downstream_selector.get_all_downstreams();
                            let mut res = vec![];
                            m.job_id = 0;
                            let message = Mining::SetNewPrevHash(m.into_static());
                            for downstream in downstrems {
                                res.push(SendTo::RelayNewMessageToRemote(
                                    downstream,
                                    message.clone(),
                                ));
                            }
                            self.job_up_to_down_ids = HashMap::new();
                            Ok(SendTo::Multiple(res))
                        }
                    }
                } else {
                    todo!()
                }
            }
            // we ignore set new prev hash when we use the JN
            ChannelKind::ExtendedWithNegotiator(_) => Ok(SendTo::None(None)),
        }
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        _m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        info!("SET CUSTOM MINIG JOB SUCCESS");
        Ok(SendTo::None(None))
    }

    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: SetCustomMiningJobError,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("560")
    }

    fn handle_set_target(&mut self, _m: SetTarget) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("570")
    }

    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("580")
    }

    fn get_request_id_mapper(&mut self) -> Option<Arc<Mutex<RequestIdMapper>>> {
        None
    }
}

pub async fn scan(
    nodes: Vec<Arc<Mutex<UpstreamMiningNode>>>,
    min_version: u16,
    max_version: u16,
) -> Vec<Arc<Mutex<UpstreamMiningNode>>> {
    let res = Arc::new(Mutex::new(Vec::with_capacity(nodes.len())));
    let spawn_tasks: Vec<task::JoinHandle<()>> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            let cloned = res.clone();
            task::spawn(async move {
                if let Err(e) = UpstreamMiningNode::setup_flag_and_version(
                    node.clone(),
                    None,
                    min_version,
                    max_version,
                )
                .await
                {
                    error!("{:?}", e)
                } else {
                    cloned.safe_lock(|r| r.push(node.clone())).unwrap();
                }
            })
        })
        .collect();
    for task in spawn_tasks {
        task.await.unwrap()
    }
    res.safe_lock(|r| r.clone()).unwrap()
}

impl IsUpstream<DownstreamMiningNode, ProxyRemoteSelector> for UpstreamMiningNode {
    fn get_version(&self) -> u16 {
        self.sv2_connection.unwrap().version
    }

    fn get_flags(&self) -> u32 {
        self.sv2_connection.unwrap().setup_connection_flags
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        vec![Protocol::MiningProtocol]
    }

    fn get_id(&self) -> u32 {
        self.id
    }

    fn get_mapper(&mut self) -> Option<&mut RequestIdMapper> {
        Some(&mut self.request_id_mapper)
    }

    fn get_remote_selector(&mut self) -> &mut ProxyRemoteSelector {
        &mut self.downstream_selector
    }
}
impl IsMiningUpstream<DownstreamMiningNode, ProxyRemoteSelector> for UpstreamMiningNode {
    fn total_hash_rate(&self) -> u64 {
        self.total_hash_rate
    }
    fn add_hash_rate(&mut self, to_add: u64) {
        self.total_hash_rate += to_add;
    }
    fn get_opened_channels(&mut self) -> &mut Vec<UpstreamChannel> {
        todo!()
    }
    fn update_channels(&mut self, _channel: UpstreamChannel) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn new_upstream_minining_node() {
        let id = 0;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let authority_public_key = [
            215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176,
            190, 90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
        ];
        let ids = Arc::new(Mutex::new(GroupId::new()));
        let recv_tp = None;
        let recv_ph = None;
        let request_ids = Arc::new(Mutex::new(Id::new()));
        let channel_ids = Arc::new(Mutex::new(Id::new()));
        let actual = UpstreamMiningNode::new(
            id,
            address,
            authority_public_key,
            crate::ChannelKind::Group,
            ids,
            recv_tp,
            recv_ph,
            request_ids,
            channel_ids,
            10.0,
            None,
            None,
        );

        assert_eq!(actual.id, id);

        assert_eq!(actual.total_hash_rate, 0);
        assert_eq!(actual.address, address);

        if actual.connection.is_some() {
            panic!("`UpstreamMiningNode::connection` should be `None` on call to `UpstreamMiningNode::new()`");
        }

        if actual.sv2_connection.is_some() {
            panic!("`UpstreamMiningNode::sv2_connection` should be `None` on call to `UpstreamMiningNode::new()`");
        }

        // How to test
        // assert_eq!(actual.downstream_selector, ProxyRemoteSelector::new());

        assert_eq!(actual.authority_public_key, authority_public_key);
        assert!(actual.channel_id_to_job_dispatcher.is_empty());
        assert_eq!(actual.request_id_mapper, RequestIdMapper::new());
    }
}
