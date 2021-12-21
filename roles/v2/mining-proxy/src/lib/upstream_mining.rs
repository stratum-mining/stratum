use super::downstream_mining::DownstreamMiningNode;
use async_channel::{Receiver, SendError, Sender};
use async_recursion::async_recursion;
use async_std::net::TcpStream;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use codec_sv2::{Frame, HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use messages_sv2::handlers::common::{CommonMessages, Protocol, SetupConnection};
use messages_sv2::handlers::mining::{DownstreamSelector, RemoteSelector, RequestIdMapper};
use messages_sv2::PoolMessages;
use network_helpers::Connection;
use std::collections::HashMap;
use std::sync::{Arc as ArcSync, Mutex as MutexSync};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

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
    //async fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
    //    Self { receiver, sender }
    //}

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
    mining_flags: u32,
}

#[derive(Debug, Clone)]
struct Downstream {
    downstream: Arc<Mutex<DownstreamMiningNode>>,
    /// (group channel id, channel id)
    group: Option<(u32, u32)>,
    /// extended channel id, the channel id will be the global unique ID used by UpstreamMiningNode
    /// as key to store this Downstream
    extended: Option<u32>,
}

impl Downstream {
    pub fn new(downstream: Arc<Mutex<DownstreamMiningNode>>) -> Self {
        Self {
            downstream,
            group: None,
            extended: None,
        }
    }
}

/// Can be either a mining pool or another proxy
#[derive(Debug)]
pub struct UpstreamMiningNode {
    address: SocketAddr,
    //port: u32,
    connection: Option<UpstreamMiningConnection>,
    sv2_connection: Option<Sv2MiningConnection>,
    downstreams: HashMap<u32, Downstream>,
    authority_public_key: [u8; 32],
    /// List of all group channels opened with upstream (as today only one group channel per
    /// upstream is supported) TODO
    group_channels_id: Vec<u32>,
    /// Each relayd message that have a request_id field must have a unique request_id number
    /// connection-wise.
    /// request_id from downstream is not garanted to be uniquie so must be changed
    pub request_id_mapper: ArcSync<MutexSync<RequestIdMapper>>,
    pub downstream_selector: ArcSync<MutexSync<Selector>>,
}

use crate::MAX_SUPPORTED_VERSION;
use crate::MIN_SUPPORTED_VERSION;
use core::convert::TryInto;
use std::net::SocketAddr;

/// It assume that endpoint NEVER change flags and version! TODO add test for it
impl UpstreamMiningNode {
    pub fn new(address: SocketAddr, authority_public_key: [u8; 32]) -> Self {
        let request_id_mapper = ArcSync::new(MutexSync::new(RequestIdMapper::new()));
        let downstream_selector = ArcSync::new(MutexSync::new(Selector::new()));
        Self {
            address,
            connection: None,
            sv2_connection: None,
            downstreams: HashMap::new(),
            authority_public_key,
            group_channels_id: Vec::new(),
            request_id_mapper,
            downstream_selector,
        }
    }

    /// Try send a message to the upstream node.
    /// If the node is connected and there are no error return Ok(())
    /// If the node is connected and there is an error the message is not sent and an error is
    ///     returned and the upstream is marked as not connected.
    /// If the node is not connected it try to connect and send the message and everything is ok
    ///     the upstream is marked as connected and Ok(()) is returned if not an error is returned.
    ///     TODO verify and test the above statements
    pub async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), SendError<EitherFrame>> {
        match (self.connection.as_mut(), self.sv2_connection) {
            (Some(connection), Some(_sv2_connection)) => match connection.send(sv2_frame).await {
                Ok(_) => Ok(()),
                Err(_e) => {
                    self.connect().await.unwrap();
                    // It assume that enpoint NEVER change flags and version! TODO add test for
                    // that
                    match self.setup_connection().await {
                        Ok(()) => Ok(()),
                        Err(()) => panic!(),
                    }
                }
            },
            // It assume that no downstream try to send messages before that the upstream is
            // initialized. This assumption is enforced by the fact that
            // UpstreamMiningNode::pair only pair downstream noder with already
            // initialized upstream nodes! TODO add test for that
            (Some(connection), None) => match connection.send(sv2_frame).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    //self.connect().await;
                    //// It assume that enpoint NEVER change flags and version!
                    //self.setup_flag_and_version(None).await;
                    Err(e)
                }
            },
            (None, _) => {
                self.connect().await.unwrap();
                match self.connection.as_mut().unwrap().send(sv2_frame).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        self.connect().await.unwrap();
                        Err(e)
                    }
                }
            }
        }
    }

    /// Get the id of the first created group channel
    pub fn get_group_channel_id(&self) -> Option<u32> {
        self.group_channels_id.get(0).copied()
    }

    async fn receive(&mut self) -> Result<StdFrame, ()> {
        match self.connection.as_mut() {
            Some(connection) => match connection.receiver.recv().await {
                Ok(m) => Ok(m.try_into()?),
                Err(_) => {
                    self.connect().await?;
                    Err(())
                }
            },
            None => todo!(),
        }
    }

    async fn connect(&mut self) -> Result<(), ()> {
        match self.connection {
            Some(_) => Ok(()),
            None => {
                let socket = TcpStream::connect(self.address).await.map_err(|_| ())?;
                let initiator = Initiator::from_raw_k(self.authority_public_key);
                let (receiver, sender) =
                    Connection::new(socket, HandshakeRole::Initiator(initiator)).await;
                let connection = UpstreamMiningConnection { receiver, sender };
                self.connection = Some(connection);
                Ok(())
            }
        }
    }

    #[async_recursion]
    async fn setup_connection(&mut self) -> Result<(), ()> {
        match self.sv2_connection {
            None => self.setup_flag_and_version(None).await,
            Some(sv2_connection) => {
                let flags = sv2_connection.mining_flags;
                let version = sv2_connection.version;
                let frame = self.new_setup_connection_frame(flags, version, version);
                self.send(frame).await.map_err(|_| ())?;
                let response = &mut self.receive().await?;
                let message_type = response.get_header().unwrap().msg_type();
                let payload = response.payload();
                match (message_type, payload).try_into() {
                    Ok(CommonMessages::SetupConnectionSuccess(_)) => {
                        //let receiver = self.connection.clone().unwrap().receiver;
                        //Self::relay_incoming_messages(self.downstreams.clone(), receiver);
                        Ok(())
                    }
                    _ => panic!(),
                }
            }
        }
    }

    //fn relay_incoming_messages(
    //    downstreams: HashMap<u32, Downstream>,
    //    receiver: Receiver<EitherFrame>,
    //) {
    //    task::spawn(async move {
    //        loop {
    //            // lock the receiver and try wait for a message
    //            // when message lock ? and send down the message
    //            //let self_ = self_.lock();
    //            let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();

    //            //self.next(incoming, todo!()).await;

    //            //let downstream = downstreams.get(&1).unwrap();
    //            //let mut downstream = downstream.downstream.lock().await;
    //            //// TODO when implement JobNegotiation try_into can fail cause the pool can send
    //            //// a JobNegotiation message and this kind of message can not be transformed into a
    //            //// MiningDeviceMessages so the below try_into will fail.
    //            //downstream
    //            //    .send(incoming.map(|x| x.try_into().unwrap()))
    //            //    .await
    //            //    .unwrap();
    //        }
    //    });
    //}

    //pub async fn next(
    //    &mut self,
    //    mut incoming: StdFrame,
    //    downstream_selector: &'static mut Option<ChannelIdToDownstreamId>,
    //) {
    //    let message_type = incoming.get_header().unwrap().msg_type();
    //    let payload = incoming.payload();
    //    let next_message_to_send = DownstreamMining::handle_message(
    //        self,
    //        message_type,
    //        payload,
    //        Some(self.request_id_mapper.clone()),
    //    );
    //    match next_message_to_send {
    //        Ok(SendTo::Relay(downstream_id)) => {
    //            let downstream = self.downstreams.get(&downstream_id.unwrap()).unwrap();
    //            let mut downstream = downstream.downstream.lock().await;
    //            // TODO when implement JobNegotiation try_into can fail cause the pool can send
    //            // a JobNegotiation message and this kind of message can not be transformed into a
    //            // MiningDeviceMessages so the below try_into will fail.
    //            let sv2_frame: codec_sv2::Sv2Frame<messages_sv2::MiningDeviceMessages, Vec<u8>> =
    //                incoming.map(|payload| payload.try_into().unwrap());
    //            downstream.send(sv2_frame).await.unwrap();
    //        }
    //        Ok(_) => todo!(),
    //        Err(messages_sv2::Error::UnexpectedMessage) => todo!(),
    //        Err(_) => todo!(),
    //    }
    //}

    #[async_recursion]
    async fn setup_flag_and_version(&mut self, flags: Option<u32>) -> Result<(), ()> {
        let flags = flags.unwrap_or(0b0111_0000_0000_0000_0000_0000_0000_0000);
        let min_version = MIN_SUPPORTED_VERSION;
        let max_version = MAX_SUPPORTED_VERSION;
        let frame = self.new_setup_connection_frame(flags, min_version, max_version);
        self.send(frame).await.map_err(|_| ())?;
        let response = &mut self.receive().await?;
        let message_type = response.get_header().unwrap().msg_type();
        let payload = response.payload();
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => {
                self.sv2_connection = Some(Sv2MiningConnection {
                    version: m.used_version,
                    mining_flags: m.flags,
                });
                Ok(())
            }
            Ok(CommonMessages::SetupConnectionError(m)) => {
                if m.flags != 0 {
                    let flags = flags ^ m.flags;
                    // We need to send SetupConnection again as we do not yet know the version of
                    // upstream
                    // TODO debounce this?
                    self.setup_flag_and_version(Some(flags)).await
                } else {
                    Err(())
                }
            }
            Ok(_) => todo!(),
            Err(_) => todo!(),
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

//impl DownstreamMining for UpstreamMiningNode {
//    fn get_channel_type(&self) -> ChannelType {
//        todo!()
//    }
//
//    fn is_work_selection_enabled(&self) -> bool {
//        todo!()
//    }
//
//    fn handle_open_standard_mining_channel_success(
//        &mut self,
//        m: messages_sv2::handlers::mining::OpenStandardMiningChannelSuccess,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_open_extended_mining_channel_success(
//        &mut self,
//        m: messages_sv2::handlers::mining::OpenExtendedMiningChannelSuccess,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_open_mining_channel_error(
//        &mut self,
//        m: messages_sv2::handlers::mining::OpenMiningChannelError,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_update_channel_error(
//        &mut self,
//        m: messages_sv2::handlers::mining::UpdateChannelError,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_close_channel(
//        &mut self,
//        m: messages_sv2::handlers::mining::CloseChannel,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_set_extranonce_prefix(
//        &mut self,
//        m: messages_sv2::handlers::mining::SetExtranoncePrefix,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_submit_shares_success(
//        &mut self,
//        m: messages_sv2::handlers::mining::SubmitSharesSuccess,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_submit_shares_error(
//        &mut self,
//        m: messages_sv2::handlers::mining::SubmitSharesError,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_new_mining_job(
//        &mut self,
//        m: messages_sv2::handlers::mining::NewMiningJob,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_new_extended_mining_job(
//        &mut self,
//        m: messages_sv2::handlers::mining::NewExtendedMiningJob,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_set_new_prev_hash(
//        &mut self,
//        m: messages_sv2::handlers::mining::SetNewPrevHash,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_set_custom_mining_job_success(
//        &mut self,
//        m: messages_sv2::handlers::mining::SetCustomMiningJobSuccess,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_set_custom_mining_job_error(
//        &mut self,
//        m: messages_sv2::handlers::mining::SetCustomMiningJobError,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_set_target(
//        &mut self,
//        m: messages_sv2::handlers::mining::SetTarget,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//
//    fn handle_reconnect(
//        &mut self,
//        m: messages_sv2::handlers::mining::Reconnect,
//    ) -> Result<messages_sv2::handlers::mining::SendTo, messages_sv2::Error> {
//        todo!()
//    }
//}

/// HashMap: channel id/request id -> downstream id
/// maps the channel id to the respective Downstream
/// channel_links is surjective:
///   An header only Downstream will be punted by only one channel
///   A non header only Downstream can be punted by more than one channel
///
/// When a Downstrem send OpenStandardChannel is registred in the map as
///   OpenStandardChannel.request_id -> downstream id
///
/// When OpenStandardChannelSucces is received by Upstream the Downstream is registered as
///   channel id -> downstream id
#[derive(Debug)]
pub struct ChannelIdToDownstreamId {
    channel_links: HashMap<u32, u32>,
    request_links: HashMap<u32, u32>,
}

impl RemoteSelector<u32> for ChannelIdToDownstreamId {
    fn on_open_standard_channel_request(&mut self, request_id: u32, downstream_id: u32) {
        self.request_links
            .insert(request_id, downstream_id)
            .unwrap();
    }

    fn on_open_standard_channel_success(&mut self, request_id: u32, channel_id: u32) {
        let downstream_id = self.request_links.remove(&request_id);
        debug_assert!(downstream_id.is_some());
        self.channel_links
            .insert(channel_id, downstream_id.unwrap())
            .unwrap();
    }

    fn get_remote_id(&self, channel_id: u32) -> Option<u32> {
        self.channel_links
            .get(&channel_id)
            .copied()
    }
}

#[derive(Debug)]
pub struct Selector {
    inner: HashMap<u32, Arc<Mutex<DownstreamMiningNode>>>,
}

impl Selector {
    pub fn new() -> Self {
        Selector {
            inner: HashMap::new(),
        }
    }
}

impl DownstreamSelector<Arc<Mutex<DownstreamMiningNode>>> for Selector {
    fn on_request(&mut self, request_id: u32, downstream: Arc<Mutex<DownstreamMiningNode>>) {
        self.inner.insert(request_id, downstream);
    }

    fn get_dowstream(&mut self, downstream_id: u32) -> Arc<Mutex<DownstreamMiningNode>> {
        self.inner.remove(&downstream_id).unwrap()
    }
}

#[derive(Debug)]
pub struct UpstreamMiningNodes {
    pub nodes: Vec<Arc<Mutex<UpstreamMiningNode>>>,
}

impl UpstreamMiningNodes {
    /// Scan all the upstreams and initialize them
    pub async fn scan(&mut self) {
        let spawn_tasks: Vec<task::JoinHandle<()>> = self
            .nodes
            .iter()
            .map(|node| {
                let node = node.clone();
                task::spawn(async move {
                    let mut node = node.lock().await;
                    node.setup_flag_and_version(None).await.unwrap();
                })
            })
            .collect();
        for task in spawn_tasks {
            task.await
        }
    }

    /// When the proxy receive SetupConnection from a downstream with protocol = Mining Protocol it
    /// check all the upstream and if it find one compatible it pair them.
    pub async fn pair_downstream(
        &mut self,
        protocol: Protocol,
        min_v: u16,
        max_v: u16,
        flags: u32,
        downstream: Arc<Mutex<DownstreamMiningNode>>,
        downstream_id: u32,
    ) -> Result<(Arc<Mutex<UpstreamMiningNode>>, u16), ()> {
        for node_ in &self.nodes {
            let mut node = node_.lock().await;

            let sv2_connection = node.sv2_connection.unwrap();
            let upstream_version = sv2_connection.version;
            let check_version = upstream_version >= min_v && upstream_version <= max_v;

            if check_version
                && SetupConnection::check_flags(protocol, flags, sv2_connection.mining_flags)
            {
                let downstream = Downstream::new(downstream.clone());
                node.downstreams.insert(downstream_id, downstream);
                return Ok((node_.clone(), upstream_version));
            }
        }
        Err(())
    }
}
