use async_std::{net::TcpListener, prelude::*, task};
use codec_sv2::{HandshakeRole, Responder};
use network_helpers::Connection;
use serde::Deserialize;
use std::sync::Arc as SArc;

use async_channel::{Receiver, Sender};
use async_std::sync::Arc;
use binary_sv2::{u256_from_int, B032};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::{
        common::ParseDownstreamCommonMessages,
        mining::{ParseDownstreamMiningMessages, SendTo, SupportedChannelTypes},
    },
    mining_sv2::*,
    parsers::{CommonMessages, Mining, PoolMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::Mutex,
};
use std::convert::TryInto;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug, Deserialize)]
struct Configuration {
    listen_address: String,
    authority_public_key: EncodedEd25519PublicKey,
    authority_secret_key: EncodedEd25519SecretKey,
    cert_validity_sec: u64,
}

async fn server_pool(config: &Configuration) {
    let listner = TcpListener::bind(&config.listen_address).await.unwrap();
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
            config.authority_public_key.clone().into_inner().as_bytes(),
            config.authority_secret_key.clone().into_inner().as_bytes(),
            std::time::Duration::from_secs(config.cert_validity_sec),
        )
        .unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Responder(responder), 10).await;
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

mod args {
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct Args {
        pub config_path: PathBuf,
    }

    enum ArgsState {
        Next,
        ExpectPath,
        Done,
    }

    enum ArgsResult {
        Config(PathBuf),
        None,
        Help(String),
    }

    impl Args {
        const DEFAULT_CONFIG_PATH: &'static str = "pool-config.toml";

        pub fn from_args() -> Result<Self, String> {
            let cli_args = std::env::args();

            let config_path = cli_args
                .scan(ArgsState::Next, |state, item| {
                    match std::mem::replace(state, ArgsState::Done) {
                        ArgsState::Next => match item.as_str() {
                            "-c" | "--config" => {
                                *state = ArgsState::ExpectPath;
                                Some(ArgsResult::None)
                            }
                            "-h" | "--help" => Some(ArgsResult::Help(format!(
                                "Usage: -h/--help, -c/--config <path|default {}>",
                                Self::DEFAULT_CONFIG_PATH
                            ))),
                            _ => {
                                *state = ArgsState::Next;

                                Some(ArgsResult::None)
                            }
                        },
                        ArgsState::ExpectPath => Some(ArgsResult::Config(PathBuf::from(item))),
                        ArgsState::Done => None,
                    }
                })
                .last();
            let config_path = match config_path {
                Some(ArgsResult::Config(p)) => p,
                Some(ArgsResult::Help(h)) => return Err(h),
                _ => PathBuf::from(Self::DEFAULT_CONFIG_PATH),
            };
            Ok(Self { config_path })
        }
    }
}

#[async_std::main]
async fn main() {
    let args = match args::Args::from_args() {
        Ok(cfg) => cfg,
        Err(help) => {
            println!("{}", help);
            return;
        }
    };
    let config_file = std::fs::read_to_string(args.config_path).expect("TODO: Error handling");
    let config = match toml::from_str::<Configuration>(&config_file) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Failed to parse config file: {}", e);
            return;
        }
    };
    server_pool(&config).await;
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
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        self.header_only = Some(header_only);
        println!("POOL: setup connection");
        println!("POOL: connection require_std_job: {}", header_only);
        Ok(SendTo::RelayNewMessageToSv2(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: 0,
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
            Ok(SendTo::RelayNewMessageToSv2(_, message)) => {
                let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into().unwrap();
                let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
                sender.send(sv2_frame.into()).await.unwrap();
                //self.send(sv2_frame).await.unwrap();
            }
            Ok(_) => panic!(),
            Err(Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }
    }
}

use noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey};
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
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
        _m: Option<Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        let request_id = incoming.get_request_id_as_u32();
        let message = match (self.is_header_only, self.group_id) {
            (false, Some(group_channel_id)) => {
                let channel_id = self.channel_id_generator.safe_lock(|x| x.next()).unwrap();
                self.channels_id.push(channel_id);
                println!(
                    "POOL: channel opened: channel id is {}, group id is {}, request id is {}",
                    channel_id, group_channel_id, request_id,
                );
                OpenStandardMiningChannelSuccess {
                    request_id: request_id.into(),
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
                    request_id: request_id.into(),
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
        Ok(SendTo::RelayNewMessageToSv2(
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
