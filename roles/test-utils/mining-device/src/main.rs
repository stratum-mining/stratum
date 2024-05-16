use async_std::net::TcpStream;
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::Connection;
use roles_logic_sv2::utils::Id;
use std::{net::SocketAddr, sync::Arc, thread::sleep, time::Duration};

use async_std::net::ToSocketAddrs;
use clap::Parser;
use rand::{thread_rng, Rng};
use std::time::Instant;
use stratum_common::bitcoin::{
    blockdata::block::BlockHeader, hash_types::BlockHash, hashes::Hash, util::uint::Uint256,
};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        help = "Pool pub key, when left empty the pool certificate is not checked"
    )]
    pubkey_pool: Option<Secp256k1PublicKey>,
    #[arg(
        short,
        long,
        help = "Sometimes used by the pool to identify the device"
    )]
    id_device: Option<String>,
    #[arg(
        short,
        long,
        help = "Address of the pool in this format ip:port or domain:port"
    )]
    address_pool: String,
    #[arg(
        long,
        help = "This value is used to slow down the cpu miner, it rapresents the number of micro-seconds that are awaited between hashes",
        default_value = "0"
    )]
    handicap: u32,
    #[arg(
        long,
        help = "User id, used when a new channel is opened, it can be used by the pool to identify the miner"
    )]
    id_user: Option<String>,
}

async fn connect(
    address: String,
    pub_key: Option<Secp256k1PublicKey>,
    device_id: Option<String>,
    user_id: Option<String>,
    handicap: u32,
) {
    let address = address
        .clone()
        .to_socket_addrs()
        .await
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port")
        .next()
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port");
    info!("Connecting to pool at {}", address);
    let socket = loop {
        let pool =
            async_std::future::timeout(Duration::from_secs(5), TcpStream::connect(address)).await;
        match pool {
            Ok(result) => match result {
                Ok(socket) => break socket,
                Err(e) => {
                    error!(
                        "Failed to connect to Upstream role at {}, retrying in 5s: {}",
                        address, e
                    );
                    sleep(Duration::from_secs(5));
                }
            },
            Err(_) => {
                error!("Pool is unresponsive, terminating");
                std::process::exit(1);
            }
        }
    };
    info!("Pool tcp connection established at {}", address);
    let address = socket.peer_addr().unwrap();
    let initiator = Initiator::new(pub_key.map(|e| e.0));
    let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        Connection::new(socket, codec_sv2::HandshakeRole::Initiator(initiator), 10)
            .await
            .unwrap();
    info!("Pool noise connection established at {}", address);
    Device::start(receiver, sender, address, device_id, user_id, handicap).await
}

#[async_std::main]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt::init();
    info!("start");
    connect(
        args.address_pool,
        args.pubkey_pool,
        args.id_device,
        args.id_user,
        args.handicap,
    )
    .await
}

use async_channel::{Receiver, Sender};
use binary_sv2::u256_from_int;
use codec_sv2::{Frame, Initiator, StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess},
    common_properties::{IsMiningUpstream, IsUpstream},
    errors::Error,
    handlers::{
        common::ParseUpstreamCommonMessages,
        mining::{ParseUpstreamMiningMessages, SendTo, SupportedChannelTypes},
    },
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::Mutex,
};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

struct SetupConnectionHandler {}
use std::convert::TryInto;

impl SetupConnectionHandler {
    pub fn new() -> Self {
        SetupConnectionHandler {}
    }
    fn get_setup_connection_message(
        address: SocketAddr,
        device_id: Option<String>,
    ) -> SetupConnection<'static> {
        let endpoint_host = address.ip().to_string().into_bytes().try_into().unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = device_id.unwrap_or_default();
        info!(
            "Creating SetupConnection message with device id: {:?}",
            device_id
        );
        SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0001,
            endpoint_host,
            endpoint_port: address.port(),
            vendor,
            hardware_version,
            firmware,
            device_id: device_id.try_into().unwrap(),
        }
    }
    pub async fn setup(
        self_: Arc<Mutex<Self>>,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        device_id: Option<String>,
        address: SocketAddr,
    ) {
        let setup_connection = Self::get_setup_connection_message(address, device_id);

        let sv2_frame: StdFrame = MiningDeviceMessages::Common(setup_connection.into())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.unwrap();
        info!("Setup connection sent to {}", address);

        let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        ParseUpstreamCommonMessages::handle_message_common(
            self_,
            message_type,
            payload,
            CommonRoutingLogic::None,
        )
        .unwrap();
    }
}

impl ParseUpstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        _: SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        info!("Setup connection success");
        Ok(SendTo::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        error!("Setup connection error");
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }
}

#[derive(Debug)]
pub struct Device {
    #[allow(dead_code)]
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    #[allow(dead_code)]
    channel_opened: bool,
    channel_id: Option<u32>,
    miner: Arc<Mutex<Miner>>,
    jobs: Vec<NewMiningJob<'static>>,
    prev_hash: Option<SetNewPrevHash<'static>>,
    sequence_numbers: Id,
}

fn open_channel(device_id: Option<String>) -> OpenStandardMiningChannel<'static> {
    let user_identity = device_id.unwrap_or_default().try_into().unwrap();
    let id: u32 = 10;
    info!("Measuring CPU hashrate");
    let nominal_hash_rate = measure_hashrate(5) as f32;
    info!("Pc hashrate is {}", nominal_hash_rate);
    info!("MINING DEVICE: send open channel with request id {}", id);
    OpenStandardMiningChannel {
        request_id: id.into(),
        user_identity,
        nominal_hash_rate,
        max_target: u256_from_int(567_u64),
    }
}

impl Device {
    async fn start(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        addr: SocketAddr,
        device_id: Option<String>,
        user_id: Option<String>,
        handicap: u32,
    ) {
        let setup_connection_handler = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        SetupConnectionHandler::setup(
            setup_connection_handler,
            &mut receiver,
            &mut sender,
            device_id,
            addr,
        )
        .await;
        info!("Pool sv2 connection established at {}", addr);
        let miner = Arc::new(Mutex::new(Miner::new(handicap)));
        let self_ = Self {
            channel_opened: false,
            receiver: receiver.clone(),
            sender: sender.clone(),
            miner: miner.clone(),
            jobs: Vec::new(),
            prev_hash: None,
            channel_id: None,
            sequence_numbers: Id::new(),
        };
        let open_channel =
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel(user_id)));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        let self_mutex = std::sync::Arc::new(Mutex::new(self_));
        let cloned = self_mutex.clone();

        let (share_send, share_recv) = async_channel::unbounded();

        let handicap = miner.safe_lock(|m| m.handicap).unwrap();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_micros(handicap.into()));
            if miner.safe_lock(|m| m.next_share()).unwrap().is_valid() {
                let nonce = miner.safe_lock(|m| m.header.unwrap().nonce).unwrap();
                let time = miner.safe_lock(|m| m.header.unwrap().time).unwrap();
                let job_id = miner.safe_lock(|m| m.job_id).unwrap();
                let version = miner.safe_lock(|m| m.version).unwrap();
                share_send
                    .try_send((nonce, job_id.unwrap(), version.unwrap(), time))
                    .unwrap();
            }
            miner
                .safe_lock(|m| m.header.as_mut().map(|h| h.nonce += 1))
                .unwrap();
        });

        async_std::task::spawn(async move {
            let recv = share_recv.clone();
            loop {
                let (nonce, job_id, version, ntime) = recv.recv().await.unwrap();
                Self::send_share(cloned.clone(), nonce, job_id, version, ntime).await;
            }
        });

        loop {
            let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();
            let next = Device::handle_message_mining(
                self_mutex.clone(),
                message_type,
                payload,
                MiningRoutingLogic::None,
            )
            .unwrap();
            match next {
                SendTo::RelayNewMessageToRemote(_, m) => {
                    let sv2_frame: StdFrame = MiningDeviceMessages::Mining(m).try_into().unwrap();
                    let either_frame: EitherFrame = sv2_frame.into();
                    sender.send(either_frame).await.unwrap();
                }
                SendTo::None(_) => (),
                _ => panic!(),
            }
        }
    }

    async fn send_share(
        self_mutex: Arc<Mutex<Self>>,
        nonce: u32,
        job_id: u32,
        version: u32,
        ntime: u32,
    ) {
        let share =
            MiningDeviceMessages::Mining(Mining::SubmitSharesStandard(SubmitSharesStandard {
                channel_id: self_mutex.safe_lock(|s| s.channel_id.unwrap()).unwrap(),
                sequence_number: self_mutex.safe_lock(|s| s.sequence_numbers.next()).unwrap(),
                job_id,
                nonce,
                ntime,
                version,
            }));
        let frame: StdFrame = share.try_into().unwrap();
        let sender = self_mutex.safe_lock(|s| s.sender.clone()).unwrap();
        sender.send(frame.into()).await.unwrap();
    }
}

impl IsUpstream<(), NullDownstreamMiningSelector> for Device {
    fn get_version(&self) -> u16 {
        todo!()
    }

    fn get_flags(&self) -> u32 {
        todo!()
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        todo!()
    }

    fn get_id(&self) -> u32 {
        todo!()
    }

    fn get_mapper(&mut self) -> Option<&mut roles_logic_sv2::common_properties::RequestIdMapper> {
        todo!()
    }

    fn get_remote_selector(&mut self) -> &mut NullDownstreamMiningSelector {
        todo!()
    }
}

impl IsMiningUpstream<(), NullDownstreamMiningSelector> for Device {
    fn total_hash_rate(&self) -> u64 {
        todo!()
    }

    fn add_hash_rate(&mut self, _to_add: u64) {
        todo!()
    }
    fn get_opened_channels(
        &mut self,
    ) -> &mut Vec<roles_logic_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, _: roles_logic_sv2::common_properties::UpstreamChannel) {
        todo!()
    }
}

impl ParseUpstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Device {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Standard
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
        _: Option<std::sync::Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        self.channel_opened = true;
        self.channel_id = Some(m.channel_id);
        let req_id = m.get_request_id_as_u32();
        info!(
            "MINING DEVICE: channel opened with: group id {}, channel id {}, request id {}",
            m.group_channel_id, m.channel_id, req_id
        );
        self.miner
            .safe_lock(|miner| miner.new_target(m.target.to_vec()))
            .unwrap();
        Ok(SendTo::None(None))
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        _: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<()>, Error> {
        unreachable!()
    }

    fn handle_open_mining_channel_error(
        &mut self,
        _: OpenMiningChannelError,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_update_channel_error(&mut self, _: UpdateChannelError) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_close_channel(&mut self, _: CloseChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        _: SetExtranoncePrefix,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<()>, Error> {
        info!("SUCCESS {:?}", m);
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_error(&mut self, _: SubmitSharesError) -> Result<SendTo<()>, Error> {
        info!("Submit shares error");
        Ok(SendTo::None(None))
    }

    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<()>, Error> {
        match (m.is_future(), self.prev_hash.as_ref()) {
            (false, Some(p_h)) => {
                self.miner
                    .safe_lock(|miner| miner.new_header(p_h, &m))
                    .unwrap();
                self.jobs = vec![m.as_static()];
            }
            (true, _) => self.jobs.push(m.as_static()),
            (false, None) => {
                panic!()
            }
        }
        Ok(SendTo::None(None))
    }

    fn handle_new_extended_mining_job(
        &mut self,
        _: NewExtendedMiningJob,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo<()>, Error> {
        let jobs: Vec<&NewMiningJob<'static>> = self
            .jobs
            .iter()
            .filter(|j| j.job_id == m.job_id && j.is_future())
            .collect();
        match jobs.len() {
            0 => {
                self.prev_hash = Some(m.as_static());
            }
            1 => {
                self.miner
                    .safe_lock(|miner| miner.new_header(&m, jobs[0]))
                    .unwrap();
                self.jobs = vec![jobs[0].clone()];
                self.prev_hash = Some(m.as_static());
            }
            _ => panic!(),
        }
        Ok(SendTo::None(None))
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        _: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job_error(
        &mut self,
        _: SetCustomMiningJobError,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo<()>, Error> {
        self.miner
            .safe_lock(|miner| miner.new_target(m.maximum_target.to_vec()))
            .unwrap();
        Ok(SendTo::None(None))
    }

    fn handle_reconnect(&mut self, _: Reconnect) -> Result<SendTo<()>, Error> {
        todo!()
    }
}

#[derive(Debug)]
struct Miner {
    header: Option<BlockHeader>,
    target: Option<Uint256>,
    job_id: Option<u32>,
    version: Option<u32>,
    handicap: u32,
}

impl Miner {
    fn new(handicap: u32) -> Self {
        Self {
            target: None,
            header: None,
            job_id: None,
            version: None,
            handicap,
        }
    }

    fn new_target(&mut self, mut target: Vec<u8>) {
        // target is sent in LE and comparisons in this file are done in BE
        target.reverse();
        let hex_string = target
            .iter()
            .fold("".to_string(), |acc, b| acc + format!("{:02x}", b).as_str());
        info!("Set target to {}", hex_string);
        self.target = Some(Uint256::from_be_bytes(target.try_into().unwrap()));
    }

    fn new_header(&mut self, set_new_prev_hash: &SetNewPrevHash, new_job: &NewMiningJob) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = set_new_prev_hash.prev_hash.to_vec().try_into().unwrap();
        let prev_hash = Hash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = Hash::from_inner(merkle_root);
        // fields need to be added as BE and the are converted to LE in the background before hashing
        let header = BlockHeader {
            version: new_job.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root,
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
                .unwrap()
                .as_secs() as u32,
            bits: set_new_prev_hash.nbits,
            nonce: 0,
        };
        self.header = Some(header);
    }
    pub fn next_share(&mut self) -> NextShareOutcome {
        if let Some(header) = self.header.as_ref() {
            let mut hash = header.block_hash().as_hash().into_inner();
            hash.reverse();
            let hash = Uint256::from_be_bytes(hash);
            if hash < *self.target.as_ref().unwrap() {
                info!(
                    "Found share with nonce: {}, for target: {:?}, with hash: {:?}",
                    header.nonce, self.target, hash,
                );
                NextShareOutcome::ValidShare
            } else {
                NextShareOutcome::InvalidShare
            }
        } else {
            std::thread::yield_now();
            NextShareOutcome::InvalidShare
        }
    }
}

enum NextShareOutcome {
    ValidShare,
    InvalidShare,
}

impl NextShareOutcome {
    pub fn is_valid(&self) -> bool {
        matches!(self, NextShareOutcome::ValidShare)
    }
}

// returns hashrate based on how fast the device hashes over the given duration
fn measure_hashrate(duration_secs: u64) -> f64 {
    let mut rng = thread_rng();
    let prev_hash: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let prev_hash = Hash::from_inner(prev_hash);
    // We create a random block that we can hash, we are only interested in knowing how many hashes
    // per unit of time we can do
    let merkle_root: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let merkle_root = Hash::from_inner(merkle_root);
    let header = BlockHeader {
        version: rng.gen(),
        prev_blockhash: BlockHash::from_hash(prev_hash),
        merkle_root,
        time: std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60))
            .unwrap()
            .as_secs() as u32,
        bits: rng.gen(),
        nonce: 0,
    };
    let start_time = Instant::now();
    let mut hashes: u64 = 0;
    let duration = Duration::from_secs(duration_secs);
    let mut miner = Miner::new(0);
    // We put the target to 0 we are only interested in how many hashes per unit of time we can do
    // and do not want to be botherd by messages about valid shares found.
    miner.new_target(vec![0_u8; 32]);
    miner.header = Some(header);

    while start_time.elapsed() < duration {
        miner.next_share();
        hashes += 1;
    }

    let elapsed_secs = start_time.elapsed().as_secs_f64();
    hashes as f64 / elapsed_secs
}
fn generate_random_32_byte_array() -> [u8; 32] {
    let mut rng = thread_rng();
    let mut arr = [0u8; 32];
    rng.fill(&mut arr[..]);
    arr
}
