use async_std::net::TcpListener;
use async_std::prelude::*;
use async_std::task;
use codec_sv2::{HandshakeRole, Responder};
use network_helpers::Connection;
use std::sync::{Arc as SArc, Mutex as SMutex};

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
    let group_id_generator = SArc::new(SMutex::new(Id::new()));
    let channel_id_generator = SArc::new(SMutex::new(Id::new()));
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

use async_channel::{Receiver, SendError, Sender};
use async_std::sync::{Arc, Mutex};
use binary_sv2::{u256_from_int, B032};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use messages_sv2::handlers::common::{SetupConnectionSuccess, UpstreamCommon};
use messages_sv2::handlers::mining::{
    ChannelType, Mining, OpenStandardMiningChannelSuccess, SendTo, UpstreamMining,
};
use messages_sv2::PoolMessages;
use std::convert::TryInto;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

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
        &mut self,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
    ) -> bool {
        let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let response = self.handle_message(message_type, payload).unwrap();
        let sv2_frame: StdFrame = PoolMessages::Common(response.into_inner().unwrap())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.unwrap();
        self.header_only.unwrap()
    }
}

impl UpstreamCommon for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: messages_sv2::handlers::common::SetupConnection,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        use messages_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        self.header_only = Some(header_only);
        println!("POOL: setup connection");
        println!("POOL: connection require_std_job: {}", header_only);
        Ok(SendTo::Downstream(
            messages_sv2::CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: incoming.flags,
                used_version: 2,
            }),
        ))
    }
}

struct Downstream {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    channel_id_generator: SArc<SMutex<Id>>,
    group_id_generator: SArc<SMutex<Id>>,
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
        group_id_generator: SArc<SMutex<Id>>,
        channel_id_generator: SArc<SMutex<Id>>,
    ) -> Arc<Mutex<Self>> {
        let is_header_only = SetupConnectionHandler::new()
            .setup(&mut receiver, &mut sender)
            .await;
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
                let mut downstream = cloned.lock().await;
                let incoming: StdFrame = downstream
                    .receiver
                    .recv()
                    .await
                    .unwrap()
                    .try_into()
                    .unwrap();
                downstream.next(incoming).await
            }
        });
        self_
    }

    pub async fn next(&mut self, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send = UpstreamMining::handle_message(self, message_type, payload);
        match next_message_to_send {
            Ok(SendTo::Downstream(message)) => {
                let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into().unwrap();
                self.send(sv2_frame).await.unwrap();
            }
            Ok(_) => panic!(),
            Err(messages_sv2::Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }

        //TODO
    }

    async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), SendError<StdFrame>> {
        let either_frame = sv2_frame.into();
        match self.sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => {
                todo!()
            }
        }
    }
}

use rand::Rng;

fn get_random_extranonce() -> B032<'static> {
    let mut rng = rand::thread_rng();
    let mut val = vec![];
    for _ in 0..32 {
        let n: u8 = rng.gen();
        val.push(n)
    }
    val.try_into().unwrap()
}

impl UpstreamMining for Downstream {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: messages_sv2::handlers::mining::OpenStandardMiningChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        let request_id = incoming.request_id;
        let message = match (self.is_header_only, self.group_id) {
            (false, Some(group_channel_id)) => {
                let mut channel_id_generator = self.channel_id_generator.lock().unwrap();
                let channel_id = channel_id_generator.next();
                self.channels_id.push(channel_id);
                println!(
                    "POOL: channel opened channel id is {} group id is {}",
                    channel_id, group_channel_id
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
                let mut channel_id_generator = self.channel_id_generator.lock().unwrap();
                let mut group_id_generator = self.group_id_generator.lock().unwrap();
                let channel_id = channel_id_generator.next();
                let group_channel_id = group_id_generator.next();
                self.channels_id.push(channel_id);
                self.group_id = Some(group_channel_id);
                println!("POOL: created group channel with id: {}", group_channel_id);
                println!(
                    "POOL: channel opened channel id is {} group id is {}",
                    channel_id, group_channel_id
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
        Ok(SendTo::Downstream(
            Mining::OpenStandardMiningChannelSuccess(message),
        ))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: messages_sv2::handlers::mining::OpenExtendedMiningChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_update_channel(
        &mut self,
        _: messages_sv2::handlers::mining::UpdateChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesStandard,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesExtended,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJob,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }
}
