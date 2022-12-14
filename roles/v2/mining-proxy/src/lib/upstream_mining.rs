use super::downstream_mining::{Channel, DownstreamMiningNode, StdFrame as DownstreamFrame};
use async_channel::{Receiver, SendError, Sender};
use async_recursion::async_recursion;
use codec_sv2::{Frame, HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    channel_logic::{
        channel_factory::ProxyExtendedChannelFactory, proxy_group_channel::GroupChannels,
    },
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{
        IsMiningDownstream, IsMiningUpstream, IsUpstream, RequestIdMapper, UpstreamChannel,
    },
    errors::Error,
    handlers::mining::{ParseUpstreamMiningMessages, SendTo, SupportedChannelTypes},
    job_dispatcher::GroupChannelJobDispatcher,
    mining_sv2::*,
    parsers::{CommonMessages, Mining, MiningDeviceMessages, PoolMessages},
    routing_logic::MiningProxyRoutingLogic,
    selectors::{DownstreamMiningSelector, ProxyDownstreamMiningSelector as Prs},
    utils::Mutex,
};
use std::{collections::HashMap, sync::Arc};
use tokio::{net::TcpStream, task};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
pub type ProxyRemoteSelector = Prs<DownstreamMiningNode>;

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
    pub group_channels: GroupChannels,
    #[allow(dead_code)]
    channel_factory: Option<ProxyExtendedChannelFactory>,
}

use core::convert::TryInto;
use std::net::SocketAddr;
use tracing::debug;

/// It assume that endpoint NEVER change flags and version!
/// I can open both extended and group channel with upstream.
impl UpstreamMiningNode {
    pub fn new(id: u32, address: SocketAddr, authority_public_key: [u8; 32]) -> Self {
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
            group_channels: GroupChannels::new(),
            channel_factory: None,
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

    async fn receive(self_mutex: Arc<Mutex<Self>>) -> Result<StdFrame, ()> {
        let mut connection = self_mutex
            .safe_lock(|self_| self_.connection.clone())
            .unwrap();
        match connection.as_mut() {
            Some(connection) => match connection.receiver.recv().await {
                Ok(m) => Ok(m.try_into().unwrap()),
                Err(_) => {
                    Self::connect(self_mutex).await?;
                    Err(())
                }
            },
            None => todo!("177"),
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
            Err(Error::UnexpectedMessage) => todo!(),
            Err(e) => panic!("{:?}", e),
        }
    }

    #[async_recursion]
    async fn setup_flag_and_version(
        self_mutex: Arc<Mutex<Self>>,
        flags: Option<u32>,
        min_version: u16,
        max_version: u16,
    ) -> Result<(), ()> {
        let flags = flags.unwrap_or(0b0000_0000_0000_0000_0000_0000_0000_0110);
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
                    Self::setup_flag_and_version(self_mutex, Some(flags), min_version, max_version)
                        .await
                } else {
                    Err(())
                }
            }
            Ok(_) => todo!("356"),
            Err(_) => todo!("357"),
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

        let remote = remote.unwrap();
        if down_is_header_only {
            let mut res = vec![SendTo::RelaySameMessageToRemote(remote.clone())];
            for message in self
                .group_channels
                .on_channel_success_for_hom_downtream(&m)?
            {
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
            todo!()
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
            Some(d) => Ok(SendTo::RelaySameMessageToRemote(d.clone())),
            None => todo!(),
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
        Err(Error::UnexpectedMessage)
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
        debug!("Handling new extended mining job: {:?}", m);
        self.group_channels.on_new_extended_mining_job(&m);
        let downstreams = self
            .downstream_selector
            .get_downstreams_in_channel(m.channel_id)
            .ok_or(Error::NoDownstreamsConnected)?;

        let mut res = vec![];
        for downstream in downstreams {
            match downstream.safe_lock(|r| r.get_channel().clone()).unwrap() {
                Channel::DowntreamHomUpstreamGroup {
                    channel_id,
                    group_id,
                    ..
                } => {
                    let message = self
                        .group_channels
                        .last_received_job_to_standard_job(channel_id, group_id)?;

                    res.push(SendTo::RelayNewMessageToRemote(
                        downstream.clone(),
                        Mining::NewMiningJob(message),
                    ));
                }
                // TODO add support for the other two variants
                _ => todo!(),
            }
        }
        Ok(SendTo::Multiple(res))
    }

    fn handle_set_new_prev_hash(
        &mut self,
        m: SetNewPrevHash,
    ) -> Result<SendTo<DownstreamMiningNode>, Error> {
        self.group_channels.update_new_prev_hash(&m);

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

pub async fn scan(nodes: Vec<Arc<Mutex<UpstreamMiningNode>>>, min_version: u16, max_version: u16) {
    let spawn_tasks: Vec<task::JoinHandle<()>> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            task::spawn(async move {
                UpstreamMiningNode::setup_flag_and_version(node, None, min_version, max_version)
                    .await
                    .unwrap();
            })
        })
        .collect();
    for task in spawn_tasks {
        task.await.unwrap();
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
        let actual = UpstreamMiningNode::new(id, address, authority_public_key);

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
