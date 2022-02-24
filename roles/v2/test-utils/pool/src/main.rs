use async_std::net::TcpListener;
use async_std::prelude::*;
use async_std::task;
use codec_sv2::{HandshakeRole, Responder};
use network_helpers::Connection;
use std::sync::Arc as SArc;

use async_channel::{Receiver, Sender};
use async_std::sync::Arc;
use binary_sv2::{u256_from_int, B032};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use messages_sv2::common_messages_sv2::{SetupConnection, SetupConnectionSuccess};
use messages_sv2::common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream};
use messages_sv2::errors::Error;
use messages_sv2::handlers::common::ParseDownstreamCommonMessages;
use messages_sv2::handlers::mining::{ChannelType, ParseDownstreamMiningMessages, SendTo};
use messages_sv2::mining_sv2::*;
use messages_sv2::parsers::Mining;
use messages_sv2::parsers::{CommonMessages, PoolMessages};
use messages_sv2::routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting};
use messages_sv2::selectors::NullDownstreamMiningSelector;
use messages_sv2::utils::Mutex;
use std::convert::TryInto;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
const ADDR: &str = "127.0.0.1:34254";

pub const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

pub const AUTHORITY_PRIVATE_K: [u8; 32] = [
    204, 93, 167, 220, 169, 204, 172, 35, 9, 84, 174, 208, 171, 89, 25, 53, 196, 209, 161, 148, 4,
    5, 173, 0, 234, 59, 15, 127, 31, 160, 136, 131,
];

const CERT_VALIDITY: std::time::Duration = std::time::Duration::from_secs(3600);

async fn server_pool() {
    let listner = TcpListener::bind(ADDR).await.unwrap();
    let mut incoming = listner.incoming();
    let group_id_generator = SArc::new(Mutex::new(Id::new()));
    let channel_id_generator = SArc::new(Mutex::new(Id::new()));
    let mut pool = Pool {
        downstreams: vec![],
    };
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!(
            "POOL: Accepting connection from: {}",
            stream.peer_addr().unwrap()
        );
        let responder = Responder::from_authority_kp(
            &AUTHORITY_PUBLIC_K[..],
            &AUTHORITY_PRIVATE_K[..],
            CERT_VALIDITY,
        );
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Responder(responder)).await;
        let downstream = Downstream::new(
            receiver,
            sender,
            group_id_generator.clone(),
            channel_id_generator.clone(),
        )
        .await;
        pool.downstreams.push(downstream);
    }
}

#[async_std::main]
async fn main() {
    server_pool().await;
}

#[derive(Debug)]
pub struct Id {
    state: u32,
}

impl Id {
    pub fn new() -> Self {
        Self { state: 0 }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

struct SetupConnectionHandler {
    header_only: Option<bool>,
}

impl SetupConnectionHandler {
    pub fn new() -> Self {
        Self { header_only: None }
    }
    pub async fn setup(
        self_: Arc<Mutex<Self>>,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
    ) -> bool {
        let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let response = ParseDownstreamCommonMessages::handle_message_common(
            self_.clone(),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )
        .unwrap();
        let sv2_frame: StdFrame = PoolMessages::Common(response.into_message().unwrap())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.unwrap();
        self_.safe_lock(|s| s.header_only.unwrap()).unwrap()
    }
}

impl ParseDownstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
        _: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<messages_sv2::handlers::common::SendTo, Error> {
        use messages_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        self.header_only = Some(header_only);
        println!("POOL: setup connection");
        println!("POOL: connection require_std_job: {}", header_only);
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: incoming.flags,
                used_version: 2,
            }),
        ))
    }
}

#[derive(Debug)]
struct Downstream {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    channel_id_generator: SArc<Mutex<Id>>,
    group_id_generator: SArc<Mutex<Id>>,
    group_id: Option<u32>,
    is_header_only: bool,
    channels_id: Vec<u32>,
}

struct Pool {
    downstreams: Vec<Arc<Mutex<Downstream>>>,
}

impl Downstream {
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        group_id_generator: SArc<Mutex<Id>>,
        channel_id_generator: SArc<Mutex<Id>>,
    ) -> Arc<Mutex<Self>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let is_header_only =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender).await;
        let group_id = None;
        let channels_id = vec![];
        let self_ = Arc::new(Mutex::new(Downstream {
            receiver,
            sender,
            channel_id_generator,
            group_id_generator,
            group_id,
            is_header_only,
            channels_id,
        }));
        let cloned = self_.clone();
        task::spawn(async move {
            loop {
                let receiver = cloned.safe_lock(|d| d.receiver.clone()).unwrap();
                let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                Downstream::next(cloned.clone(), incoming).await
            }
        });
        self_
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            MiningRoutingLogic::None,
        );
        match next_message_to_send {
            Ok(SendTo::RelayNewMessage(_, message)) => {
                let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into().unwrap();
                let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
                sender.send(sv2_frame.into()).await.unwrap();
                //self.send(sv2_frame).await.unwrap();
            }
            Ok(_) => panic!(),
            Err(Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }

        //TODO
    }
}

use rand::Rng;

fn get_random_extranonce() -> B032<'static> {
    let mut rng = rand::thread_rng();
    let mut val = vec![];
    for _ in 0..32 {
        let n: u8 = rng.gen();
        val.push(n);
    }
    val.try_into().unwrap()
}

impl IsDownstream for Downstream {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        CommonDownstreamData {
            header_only: false,
            work_selection: false,
            version_rolling: false,
        }
    }
}

impl IsMiningDownstream for Downstream {}

impl ParseDownstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Downstream {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
        _m: Option<Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        let request_id = incoming.request_id;
        let message = match (self.is_header_only, self.group_id) {
            (false, Some(group_channel_id)) => {
                let channel_id = self.channel_id_generator.safe_lock(|x| x.next()).unwrap();
                self.channels_id.push(channel_id);
                println!(
                    "POOL: channel opened: channel id is {}, group id is {}, request id is {}",
                    channel_id, group_channel_id, request_id,
                );
                OpenStandardMiningChannelSuccess {
                    request_id,
                    channel_id,
                    group_channel_id,
                    target: u256_from_int(45_u32),
                    extranonce_prefix: get_random_extranonce(),
                }
            }
            (false, None) => {
                let channel_id = self.channel_id_generator.safe_lock(|x| x.next()).unwrap();
                let group_channel_id = self.group_id_generator.safe_lock(|x| x.next()).unwrap();
                self.channels_id.push(channel_id);
                self.group_id = Some(group_channel_id);
                println!("POOL: created group channel with id: {}", group_channel_id);
                println!(
                    "POOL: channel opened: channel id is {}, group id is {}, request id is {}",
                    channel_id, group_channel_id, request_id,
                );
                OpenStandardMiningChannelSuccess {
                    request_id,
                    channel_id,
                    group_channel_id,
                    target: u256_from_int(45_u32),
                    extranonce_prefix: get_random_extranonce(),
                }
            }
            (true, None) => {
                todo!()
            }
            (true, Some(_)) => panic!(),
        };
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            Mining::OpenStandardMiningChannelSuccess(message),
        ))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_update_channel(&mut self, _: UpdateChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(&mut self, _: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        todo!()
    }
}
