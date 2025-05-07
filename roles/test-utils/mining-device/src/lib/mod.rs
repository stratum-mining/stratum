#![allow(clippy::option_map_unit_fn)]
use async_channel::{Receiver, Sender};
use codec_sv2::{Initiator, StandardEitherFrame, StandardSv2Frame};
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection::Connection;
use primitive_types::U256;
use rand::{thread_rng, Rng};
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess},
    errors::Error,
    handlers::{
        common::ParseCommonMessagesFromUpstream,
        mining::{ParseMiningMessagesFromUpstream, SendTo, SupportedChannelTypes},
    },
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages},
    utils::{Id, Mutex},
};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::available_parallelism,
    time::{Duration, Instant},
};
use stratum_common::bitcoin::{
    blockdata::block::Header, hash_types::BlockHash, hashes::Hash, CompactTarget,
};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

pub async fn connect(
    address: String,
    pub_key: Option<Secp256k1PublicKey>,
    device_id: Option<String>,
    user_id: Option<String>,
    handicap: u32,
    nominal_hashrate_multiplier: Option<f32>,
    single_submit: bool,
) {
    let address = address
        .clone()
        .to_socket_addrs()
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port")
        .next()
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port");
    info!("Connecting to pool at {}", address);
    let socket = loop {
        let pool = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(address)).await;
        match pool {
            Ok(result) => match result {
                Ok(socket) => break socket,
                Err(e) => {
                    error!(
                        "Failed to connect to Upstream role at {}, retrying in 5s: {}",
                        address, e
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
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
    let (receiver, sender) =
        Connection::new(socket, codec_sv2::HandshakeRole::Initiator(initiator))
            .await
            .unwrap();
    info!("Pool noise connection established at {}", address);
    Device::start(
        receiver,
        sender,
        address,
        device_id,
        user_id,
        handicap,
        nominal_hashrate_multiplier,
        single_submit,
    )
    .await
}

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

struct SetupConnectionHandler {}
use roles_logic_sv2::common_messages_sv2::Reconnect;
use std::convert::TryInto;
use stratum_common::bitcoin::block::Version;

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
        ParseCommonMessagesFromUpstream::handle_message_common(self_, message_type, payload)
            .unwrap();
    }
}

impl ParseCommonMessagesFromUpstream for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        info!(
            "Received `SetupConnectionSuccess`: version={}, flags={:b}",
            m.used_version, m.flags
        );
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

    fn handle_reconnect(
        &mut self,
        _m: Reconnect,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct NewWorkNotifier {
    should_send: bool,
    sender: Sender<()>,
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
    notify_changes_to_mining_thread: NewWorkNotifier,
}

fn open_channel(
    device_id: Option<String>,
    nominal_hashrate_multiplier: Option<f32>,
    handicap: u32,
) -> OpenStandardMiningChannel<'static> {
    let user_identity = device_id.unwrap_or_default().try_into().unwrap();
    let id: u32 = 10;
    info!("Measuring CPU hashrate");
    let measured_hashrate = measure_hashrate(5, handicap) as f32;
    info!("Measured CPU hashrate is {}", measured_hashrate);
    let nominal_hash_rate = match nominal_hashrate_multiplier {
        Some(m) => measured_hashrate * m,
        None => measured_hashrate,
    };

    info!("MINING DEVICE: send open channel with request id {}", id);

    OpenStandardMiningChannel {
        request_id: id.into(),
        user_identity,
        nominal_hash_rate,
        max_target: vec![0xFF_u8; 32].try_into().unwrap(),
    }
}

impl Device {
    #[allow(clippy::too_many_arguments)]
    async fn start(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        addr: SocketAddr,
        device_id: Option<String>,
        user_id: Option<String>,
        handicap: u32,
        nominal_hashrate_multiplier: Option<f32>,
        single_submit: bool,
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
        let (notify_changes_to_mining_thread, update_miners) = async_channel::unbounded();
        let self_ = Self {
            channel_opened: false,
            receiver: receiver.clone(),
            sender: sender.clone(),
            miner: miner.clone(),
            jobs: Vec::new(),
            prev_hash: None,
            channel_id: None,
            sequence_numbers: Id::new(),
            notify_changes_to_mining_thread: NewWorkNotifier {
                should_send: true,
                sender: notify_changes_to_mining_thread,
            },
        };
        let open_channel = MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(
            open_channel(user_id, nominal_hashrate_multiplier, handicap),
        ));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        let self_mutex = std::sync::Arc::new(Mutex::new(self_));
        let cloned = self_mutex.clone();

        let (share_send, share_recv) = async_channel::unbounded();

        start_mining_threads(update_miners, miner, share_send);
        tokio::task::spawn(async move {
            let recv = share_recv.clone();
            loop {
                let (nonce, job_id, version, ntime) = recv.recv().await.unwrap();
                Self::send_share(cloned.clone(), nonce, job_id, version, ntime).await;
                if single_submit {
                    break;
                }
            }
        });

        loop {
            let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();
            let next =
                Device::handle_message_mining(self_mutex.clone(), message_type, payload).unwrap();
            let mut notify_changes_to_mining_thread = self_mutex
                .safe_lock(|s| s.notify_changes_to_mining_thread.clone())
                .unwrap();
            if notify_changes_to_mining_thread.should_send
                && (message_type == stratum_common::MESSAGE_TYPE_NEW_MINING_JOB
                    || message_type == stratum_common::MESSAGE_TYPE_SET_NEW_PREV_HASH
                    || message_type == stratum_common::MESSAGE_TYPE_SET_TARGET)
            {
                notify_changes_to_mining_thread
                    .sender
                    .send(())
                    .await
                    .unwrap();
                notify_changes_to_mining_thread.should_send = false;
            };
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

impl ParseMiningMessagesFromUpstream<()> for Device {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Standard
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
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
        self.notify_changes_to_mining_thread.should_send = true;
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
        info!("Received SubmitSharesSuccess");
        debug!("SubmitSharesSuccess: {:?}", m);
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_error(&mut self, m: SubmitSharesError) -> Result<SendTo<()>, Error> {
        error!(
            "Received SubmitSharesError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(None))
    }

    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<()>, Error> {
        info!(
            "Received new mining job for channel id: {} with job id: {} is future: {}",
            m.channel_id,
            m.job_id,
            m.is_future()
        );
        debug!("NewMiningJob: {:?}", m);
        match (m.is_future(), self.prev_hash.as_ref()) {
            (false, Some(p_h)) => {
                self.miner
                    .safe_lock(|miner| miner.new_header(p_h, &m))
                    .unwrap();
                self.jobs = vec![m.as_static()];
                self.notify_changes_to_mining_thread.should_send = true;
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
        info!(
            "Received SetNewPrevHash channel id: {}, job id: {}",
            m.channel_id, m.job_id
        );
        debug!("SetNewPrevHash: {:?}", m);
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
                self.notify_changes_to_mining_thread.should_send = true;
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
        info!("Received SetTarget for channel id: {}", m.channel_id);
        debug!("SetTarget: {:?}", m);
        self.miner
            .safe_lock(|miner| miner.new_target(m.maximum_target.to_vec()))
            .unwrap();
        self.notify_changes_to_mining_thread.should_send = true;
        Ok(SendTo::None(None))
    }

    fn handle_set_group_channel(&mut self, _m: SetGroupChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct Miner {
    header: Option<Header>,
    target: Option<U256>,
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

    fn new_target(&mut self, target: Vec<u8>) {
        // target is sent in LE format, we'll keep it that way
        let hex_string = target
            .iter()
            .fold("".to_string(), |acc, b| acc + format!("{:02x}", b).as_str());
        info!("Set target to {}", hex_string);
        // Store the target as U256 in little-endian format
        self.target = Some(U256::from_little_endian(target.as_slice()));
    }

    fn new_header(&mut self, set_new_prev_hash: &SetNewPrevHash, new_job: &NewMiningJob) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = set_new_prev_hash.prev_hash.to_vec().try_into().unwrap();
        let prev_hash = Hash::from_byte_array(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = Hash::from_byte_array(merkle_root);
        // fields need to be added as BE and the are converted to LE in the background before
        // hashing
        let header = Header {
            version: Version::from_consensus(new_job.version as i32),
            prev_blockhash: BlockHash::from_raw_hash(prev_hash),
            merkle_root,
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
                .unwrap()
                .as_secs() as u32,
            bits: CompactTarget::from_consensus(set_new_prev_hash.nbits),
            nonce: 0,
        };
        self.header = Some(header);
    }
    pub fn next_share(&mut self) -> NextShareOutcome {
        if let Some(header) = self.header.as_ref() {
            let hash_ = header.block_hash();
            let hash: [u8; 32] = *hash_.to_raw_hash().as_ref();

            // Convert both hash and target to Target type for comparison
            let hash_target: Target = hash.into();

            // Convert U256 target to [u8; 32] array and then to Target
            if let Some(target) = self.target {
                let target_bytes = target.to_little_endian();
                let mut target_array = [0u8; 32];
                target_array.copy_from_slice(&target_bytes);
                let target: Target = target_array.into();
                if hash_target <= target {
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
                NextShareOutcome::NoTarget
            }
        } else {
            std::thread::yield_now();
            NextShareOutcome::NoHeader
        }
    }
}

enum NextShareOutcome {
    ValidShare,
    InvalidShare,
    NoTarget,
    NoHeader,
}

impl NextShareOutcome {
    pub fn is_valid(&self) -> bool {
        matches!(self, NextShareOutcome::ValidShare)
    }
}

// returns hashrate based on how fast the device hashes over the given duration
fn measure_hashrate(duration_secs: u64, handicap: u32) -> f64 {
    let mut rng = thread_rng();
    let prev_hash: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let prev_hash = Hash::from_byte_array(prev_hash);
    // We create a random block that we can hash, we are only interested in knowing how many hashes
    // per unit of time we can do
    let merkle_root: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let merkle_root = Hash::from_byte_array(merkle_root);
    let header = Header {
        version: Version::from_consensus(rng.gen()),
        prev_blockhash: BlockHash::from_raw_hash(prev_hash),
        merkle_root,
        time: std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60))
            .unwrap()
            .as_secs() as u32,
        bits: CompactTarget::from_consensus(rng.gen()),
        nonce: 0,
    };
    let start_time = Instant::now();
    let mut hashes: u64 = 0;
    let duration = Duration::from_secs(duration_secs);
    let mut miner = Miner::new(handicap);
    // We put the target to 0 we are only interested in how many hashes per unit of time we can do
    // and do not want to be botherd by messages about valid shares found.
    miner.new_target(vec![0_u8; 32]);
    miner.header = Some(header);

    while start_time.elapsed() < duration {
        miner.next_share();
        hashes += 1;
    }

    let elapsed_secs = start_time.elapsed().as_secs_f64();
    let hashrate_single_thread = hashes as f64 / elapsed_secs;

    // we just measured for a single thread, need to multiply by the available parallelism
    let p = available_parallelism().unwrap().get();

    hashrate_single_thread * p as f64
}
fn generate_random_32_byte_array() -> [u8; 32] {
    let mut rng = thread_rng();
    let mut arr = [0u8; 32];
    rng.fill(&mut arr[..]);
    arr
}

fn start_mining_threads(
    have_new_job: Receiver<()>,
    miner: Arc<Mutex<Miner>>,
    share_send: Sender<(u32, u32, u32, u32)>,
) {
    tokio::task::spawn(async move {
        let mut killers: Vec<Arc<AtomicBool>> = vec![];
        loop {
            let p = available_parallelism().unwrap().get() as u32;
            let unit = u32::MAX / p;
            while have_new_job.recv().await.is_ok() {
                while let Some(killer) = killers.pop() {
                    killer.store(true, Ordering::Relaxed);
                }
                let miner = miner.safe_lock(|m| m.clone()).unwrap();
                for i in 0..p {
                    let mut miner = miner.clone();
                    let share_send = share_send.clone();
                    let killer = Arc::new(AtomicBool::new(false));
                    miner.header.as_mut().map(|h| h.nonce = i * unit);
                    killers.push(killer.clone());
                    std::thread::spawn(move || {
                        mine(miner, share_send, killer);
                    });
                }
            }
        }
    });
}

fn mine(mut miner: Miner, share_send: Sender<(u32, u32, u32, u32)>, kill: Arc<AtomicBool>) {
    if miner.handicap != 0 {
        loop {
            if kill.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_micros(miner.handicap.into()));
            if miner.next_share().is_valid() {
                let nonce = miner.header.unwrap().nonce;
                let time = miner.header.unwrap().time;
                let job_id = miner.job_id.unwrap();
                let version = miner.version;
                share_send
                    .try_send((nonce, job_id, version.unwrap(), time))
                    .unwrap();
            }
            miner
                .header
                .as_mut()
                .map(|h| h.nonce = h.nonce.wrapping_add(1));
        }
    } else {
        loop {
            if miner.next_share().is_valid() {
                if kill.load(Ordering::Relaxed) {
                    break;
                }
                let nonce = miner.header.unwrap().nonce;
                let time = miner.header.unwrap().time;
                let job_id = miner.job_id.unwrap();
                let version = miner.version;
                share_send
                    .try_send((nonce, job_id, version.unwrap(), time))
                    .unwrap();
            }
            miner
                .header
                .as_mut()
                .map(|h| h.nonce = h.nonce.wrapping_add(1));
        }
    }
}
