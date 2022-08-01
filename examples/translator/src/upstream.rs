use crate::{
    downstream::{DownstreamMiningNode, StdFrame as DownstreamFrame},
    max_supported_version, min_supported_version,
};
use async_channel::{Receiver, SendError, Sender};
use async_recursion::async_recursion;
use codec_sv2::{Frame, HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{
        DownstreamChannel, IsMiningDownstream, IsMiningUpstream, IsUpstream, RequestIdMapper,
        StandardChannel, UpstreamChannel,
    },
    errors::Error,
    handlers::mining::{ParseUpstreamMiningMessages, SendTo, SupportedChannelTypes},
    job_dispatcher::GroupChannelJobDispatcher,
    mining_sv2::{
        CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannelSuccess,
        OpenMiningChannelError, OpenStandardMiningChannelSuccess, Reconnect,
        SetCustomMiningJobError, SetCustomMiningJobSuccess, SetExtranoncePrefix, SetNewPrevHash,
        SetTarget, SubmitSharesError, SubmitSharesSuccess, UpdateChannelError,
    },
    parsers::{CommonMessages, Mining, MiningDeviceMessages, PoolMessages},
    routing_logic::MiningProxyRoutingLogic,
    selectors::{DownstreamMiningSelector, ProxyDownstreamMiningSelector as Prs},
    utils::{Id, Mutex},
};
use std::{collections::HashMap, convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, task};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
pub type ProxyRemoteSelector = Prs<DownstreamMiningNode>;

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

/// A 1 to 1 connection with an upstream node that implements the mining (sub)protocol.
/// The upstream node it connects with is most typically a pool, but could also be another proxy.
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

#[derive(Debug)]
pub struct UpstreamMiningNode {
    id: u32,
    job_ids: Arc<Mutex<Id>>,
    total_hash_rate: u64,
    address: SocketAddr,
    //port: u32,
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
    last_prev_hash: Option<SetNewPrevHash<'static>>,
    last_extended_jobs: Vec<NewExtendedMiningJob<'static>>,
}

/// This assumes that the endpoint NEVER changes its flags or version!
impl UpstreamMiningNode {
    pub fn new(
        id: u32,
        address: SocketAddr,
        authority_public_key: [u8; 32],
        job_ids: Arc<Mutex<Id>>,
    ) -> Self {
        let request_id_mapper = RequestIdMapper::new();
        let downstream_selector = ProxyRemoteSelector::new();
        Self {
            id,
            job_ids,
            total_hash_rate: 0,
            address,
            connection: None,
            sv2_connection: None,
            authority_public_key,
            channel_id_to_job_dispatcher: HashMap::new(),
            request_id_mapper,
            downstream_selector,
            last_prev_hash: None,
            last_extended_jobs: Vec::new(),
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

    #[async_recursion]
    async fn setup_flag_and_version(
        self_mutex: Arc<Mutex<Self>>,
        flags: Option<u32>,
    ) -> Result<(), ()> {
        let flags = flags.unwrap_or(0b0111_0000_0000_0000_0000_0000_0000_0000);
        let min_version = min_supported_version();
        let max_version = max_supported_version();
        let frame = self_mutex
            .safe_lock(|self_| self_.new_setup_connection_frame(flags, min_version, max_version))
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
                Self::relay_incoming_messages(self_mutex, receiver);
                Ok(())
            }
            Ok(CommonMessages::SetupConnectionError(m)) => {
                if m.flags != 0 {
                    let flags = flags ^ m.flags;
                    // We need to send SetupConnection again as we do not yet know the version of
                    // upstream
                    // debounce this?
                    Self::setup_flag_and_version(self_mutex, Some(flags)).await
                } else {
                    Err(())
                }
            }
            Ok(_) => todo!("356"),
            Err(_) => todo!("357"),
        }
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
    ) -> Result<(), SendError<EitherFrame>> {
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
                Err(e) => Err(e),
            },
            (None, _) => {
                Self::connect(self_mutex.clone()).await.unwrap();
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
                        Err(e)
                    }
                }
            }
        }
    }

    async fn connect(self_mutex: Arc<Mutex<Self>>) -> Result<(), ()> {
        let has_connection = self_mutex
            .safe_lock(|self_| self_.connection.is_some())
            .unwrap();
        match has_connection {
            true => Ok(()),
            false => {
                let (address, authority_public_key) = self_mutex
                    .safe_lock(|self_| (self_.address, self_.authority_public_key))
                    .unwrap();
                let socket = TcpStream::connect(address).await.map_err(|_| ())?;
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

    async fn receive(self_mutex: Arc<Mutex<Self>>) -> Result<StdFrame, ()> {
        let mut connection = self_mutex
            .safe_lock(|self_| self_.connection.clone())
            .unwrap();
        match connection.as_mut() {
            Some(connection) => match connection.receiver.recv().await {
                Ok(m) => Ok(m.try_into()?),
                Err(_) => {
                    Self::connect(self_mutex).await?;
                    Err(())
                }
            },
            None => todo!("177"),
        }
    }

    fn relay_incoming_messages(
        self_: Arc<Mutex<Self>>,
        //_downstreams: HashMap<u32, Downstream>,
        receiver: Receiver<EitherFrame>,
    ) {
        task::spawn(async move {
            loop {
                let message = receiver.recv().await.unwrap();
                let incoming: StdFrame = message.try_into().unwrap();
                Self::next(self_.clone(), incoming).await;
            }
        });
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
            Ok(SendTo::RelaySameMessage(downstream)) => {
                let sv2_frame: codec_sv2::Sv2Frame<MiningDeviceMessages, buffer_sv2::Slice> =
                    incoming.map(|payload| payload.try_into().unwrap());

                DownstreamMiningNode::send(downstream.clone(), sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::RelayNewMessage(downstream_mutex, message)) => {
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
                        SendTo::RelayNewMessage(downstream_mutex, message) => {
                            let message = MiningDeviceMessages::Mining(message);
                            let frame: DownstreamFrame = message.try_into().unwrap();
                            DownstreamMiningNode::send(downstream_mutex, frame)
                                .await
                                .unwrap();
                        }
                        SendTo::RelaySameMessage(downstream_mutex) => {
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
                    }
                }
            }
            Ok(SendTo::None(_)) => (),
            Err(Error::NoDownstreamsConnected) => (),
            Err(Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }
    }
}

impl
    ParseUpstreamMiningMessages<
        DownstreamMiningNode,
        ProxyRemoteSelector,
        MiningProxyRoutingLogic<DownstreamMiningNode, Self, ProxyRemoteSelector>,
    > for UpstreamMiningNode
{
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
        remote: Option<Arc<Mutex<DownstreamMiningNode>>>,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        let down_is_header_only = remote
            .as_ref()
            .unwrap()
            .safe_lock(|remote| remote.is_header_only())
            .unwrap();
        let up_is_header_only = self.is_header_only();
        match (down_is_header_only, up_is_header_only) {
            (true, true) => {
                let channel = DownstreamChannel::Standard(StandardChannel {
                    channel_id: m.channel_id,
                    group_id: m.group_channel_id,
                    target: m.target.into(),
                    extranonce: m.extranonce_prefix.into(),
                });
                remote
                    .as_ref()
                    .unwrap()
                    .safe_lock(|r| r.add_channel(channel))
                    .unwrap();
            }
            (true, false) => {
                let channel = DownstreamChannel::Standard(StandardChannel {
                    channel_id: m.channel_id,
                    group_id: m.group_channel_id,
                    target: m.target.into(),
                    extranonce: m.extranonce_prefix.into(),
                });
                if self
                    .channel_id_to_job_dispatcher
                    .get_mut(&m.group_channel_id)
                    .is_none()
                {
                    let dispatcher = GroupChannelJobDispatcher::new(self.job_ids.clone());
                    self.channel_id_to_job_dispatcher
                        .insert(m.group_channel_id, JobDispatcher::Group(dispatcher));
                }
                remote
                    .as_ref()
                    .unwrap()
                    .safe_lock(|r| r.add_channel(channel))
                    .unwrap();
            }
            (false, true) => {
                todo!()
            }
            (false, false) => {
                let channel = DownstreamChannel::Group(m.group_channel_id);
                remote
                    .as_ref()
                    .unwrap()
                    .safe_lock(|r| r.add_channel(channel))
                    .unwrap();
            }
        }

        let open_channel = SendTo::RelaySameMessage(remote.clone().unwrap());

        match (&self.last_prev_hash, &self.last_extended_jobs.len()) {
            (Some(_), 0) => {
                panic!();
            }
            (Some(new_prev_hash), _) => {
                let mut responses = vec![open_channel];
                let downstream = vec![remote.clone().unwrap()];
                let mut dispatcher = self
                    .channel_id_to_job_dispatcher
                    .get_mut(&m.group_channel_id);
                let new_prev_hash = Mining::SetNewPrevHash(SetNewPrevHash {
                    channel_id: m.channel_id,
                    job_id: new_prev_hash.job_id,
                    prev_hash: new_prev_hash.prev_hash.clone().into_static(),
                    min_ntime: new_prev_hash.min_ntime,
                    nbits: new_prev_hash.nbits,
                });
                responses.push(SendTo::RelayNewMessage(remote.unwrap(), new_prev_hash));
                for job in &self.last_extended_jobs {
                    // TODO the below unwrap is not safe
                    for job in
                        jobs_to_relay(self.id, job, &downstream, dispatcher.as_mut().unwrap())
                    {
                        responses.push(job)
                    }
                }
                Ok(SendTo::Multiple(responses))
            }
            (None, 0) => Ok(open_channel),
            (None, _) => panic!(),
        }
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        _m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("450")
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
            Some(d) => Ok(SendTo::RelaySameMessage(d.clone())),
            None => todo!(),
        }
    }

    fn handle_submit_shares_error(
        &mut self,
        _m: SubmitSharesError,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_new_mining_job(
        &mut self,
        m: NewMiningJob,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        // One and only one downstream cause the message is not extended
        match &self
            .downstream_selector
            .get_downstreams_in_channel(m.channel_id)
        {
            Some(downstreams) => {
                let downstream = &downstreams[0];
                crate::add_job_id(
                    m.job_id,
                    self.id,
                    downstream.safe_lock(|d| d.prev_job_id).unwrap(),
                );
                Ok(SendTo::RelaySameMessage(downstream.clone()))
            }
            None => Err(Error::NoDownstreamsConnected),
        }
    }

    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        self.last_extended_jobs.push(m.as_static());
        let id = self.id;
        let downstreams = self
            .downstream_selector
            .get_downstreams_in_channel(m.channel_id)
            .ok_or(Error::NoDownstreamsConnected)?;

        let dispacther = self
            .channel_id_to_job_dispatcher
            .get_mut(&m.channel_id)
            .unwrap();

        let messages = jobs_to_relay(id, &m, downstreams, dispacther);

        Ok(SendTo::Multiple(messages))
    }

    fn handle_set_new_prev_hash(
        &mut self,
        m: SetNewPrevHash,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        self.last_prev_hash = Some(m.as_static());
        self.last_extended_jobs = self
            .last_extended_jobs
            .clone()
            .into_iter()
            .filter(|x| x.job_id == m.job_id)
            .collect();
        match (
            self.is_header_only(),
            self.channel_id_to_job_dispatcher.get_mut(&m.channel_id),
        ) {
            (true, None) => {
                let downstreams = self
                    .downstream_selector
                    .get_downstreams_in_channel(m.channel_id)
                    .ok_or(Error::NoDownstreamsConnected)?;
                // If upstream is header only one and only one downstream is in channel
                Ok(SendTo::RelaySameMessage(downstreams[0].clone()))
            }
            (false, Some(JobDispatcher::Group(dispatcher))) => {
                let mut channel_id_to_job_id = dispatcher.on_new_prev_hash(&m).unwrap();
                let downstreams = self
                    .downstream_selector
                    .get_downstreams_in_channel(m.channel_id)
                    .ok_or(Error::NoDownstreamsConnected)?;
                let mut messages: Vec<SendTo<DownstreamMiningNode>> =
                    Vec::with_capacity(downstreams.len());
                for downstream in downstreams {
                    downstream
                        .safe_lock(|d| {
                            for channel in d.status.get_channels().get_mut(&m.channel_id).unwrap() {
                                match channel {
                                    DownstreamChannel::Extended(_) => todo!(),
                                    DownstreamChannel::Group(_) => todo!(),
                                    DownstreamChannel::Standard(channel) => {
                                        let new_prev_hash = SetNewPrevHash {
                                            channel_id: channel.channel_id,
                                            job_id: channel_id_to_job_id
                                                .remove(&channel.channel_id)
                                                .unwrap_or(m.job_id),
                                            prev_hash: m.prev_hash.clone().into_static(),
                                            min_ntime: m.min_ntime,
                                            nbits: m.nbits,
                                        };
                                        let message = Mining::SetNewPrevHash(new_prev_hash);
                                        messages.push(SendTo::RelayNewMessage(
                                            downstream.clone(),
                                            message,
                                        ));
                                    }
                                }
                            }
                        })
                        .unwrap();
                }
                Ok(SendTo::Multiple(messages))
            }
            (false, None) => Ok(SendTo::None(None)),
            _ => panic!(),
        }
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        _m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        todo!("550")
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

pub async fn scan(nodes: Vec<Arc<Mutex<UpstreamMiningNode>>>) {
    let spawn_tasks: Vec<task::JoinHandle<()>> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            task::spawn(async move {
                UpstreamMiningNode::setup_flag_and_version(node, None)
                    .await
                    .unwrap();
            })
        })
        .collect();
    for task in spawn_tasks {
        task.await.unwrap();
    }
}

fn jobs_to_relay(
    id: u32,
    m: &NewExtendedMiningJob,
    downstreams: &[Arc<Mutex<DownstreamMiningNode>>],
    dispacther: &mut JobDispatcher,
) -> Vec<SendTo<DownstreamMiningNode>> {
    let mut messages = Vec::with_capacity(downstreams.len());
    for downstream in downstreams {
        downstream
            .safe_lock(|d| {
                let prev_id = d.prev_job_id;
                for channel in d.status.get_channels().get_mut(&m.channel_id).unwrap() {
                    match channel {
                        DownstreamChannel::Extended(_) => todo!(),
                        DownstreamChannel::Group(_) => {
                            crate::add_job_id(m.job_id, id, prev_id);
                            messages.push(SendTo::RelaySameMessage(downstream.clone()))
                        }
                        DownstreamChannel::Standard(channel) => {
                            if let JobDispatcher::Group(d) = dispacther {
                                let job = d.on_new_extended_mining_job(m, channel).unwrap();
                                crate::add_job_id(job.job_id, id, prev_id);
                                let message = Mining::NewMiningJob(job);
                                messages.push(SendTo::RelayNewMessage(downstream.clone(), message));
                            } else {
                                panic!()
                            };
                        }
                    }
                }
            })
            .unwrap();
    }
    messages
}
