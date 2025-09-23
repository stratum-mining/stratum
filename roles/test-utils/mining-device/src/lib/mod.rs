#![allow(clippy::option_map_unit_fn)]
use async_channel::{Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use num_format::{Locale, ToFormattedString};
use primitive_types::U256;
use rand::{thread_rng, Rng};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::available_parallelism,
    time::{Duration, Instant},
};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        self,
        bitcoin::{blockdata::block::Header, hash_types::BlockHash, hashes::Hash, CompactTarget},
        codec_sv2,
        codec_sv2::{Initiator, StandardEitherFrame, StandardSv2Frame},
        common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess},
        errors::Error,
        handlers::{
            common::ParseCommonMessagesFromUpstream,
            mining::{ParseMiningMessagesFromUpstream, SendTo, SupportedChannelTypes},
        },
        mining_sv2::*,
        parsers_sv2::{Mining, MiningDeviceMessages},
        utils::{Id, Mutex},
    },
};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

// Fast SHA256d midstate hasher
use sha2::{
    compress256,
    digest::generic_array::{typenum::U64, GenericArray},
};
use stratum_common::roles_logic_sv2::bitcoin::consensus::encode::serialize as btc_serialize;

// Tuneable: how many nonces to try per mining loop iteration when fast hasher is available.
// Runtime-configurable so the binary and benches can adjust it without changing code.
use std::sync::atomic::AtomicU32;
static NONCES_PER_CALL_RUNTIME: AtomicU32 = AtomicU32::new(32);
// Runtime-configurable number of worker threads; 0 means "auto" (N-1)
static WORKER_OVERRIDE: AtomicU32 = AtomicU32::new(0);

#[inline]
pub fn set_nonces_per_call(n: u32) {
    // Avoid zero (would stall the loop); clamp to at least 1
    let n = n.max(1);
    NONCES_PER_CALL_RUNTIME.store(n, Ordering::Relaxed);
}

#[inline]
fn nonces_per_call() -> u32 {
    NONCES_PER_CALL_RUNTIME.load(Ordering::Relaxed).max(1)
}

/// Override the number of mining worker threads. If set to 0, auto mode (N-1) is used.
#[inline]
pub fn set_cores(n: u32) {
    WORKER_OVERRIDE.store(n, Ordering::Relaxed);
}

/// Resolve effective worker count: if override is 0, use max(1, logical_cpus-1).
#[inline]
fn worker_count() -> u32 {
    let total_cpus = available_parallelism().map(|p| p.get()).unwrap_or(1) as u32;
    let auto = total_cpus.saturating_sub(1).max(1);
    let override_n = WORKER_OVERRIDE.load(Ordering::Relaxed);
    if override_n == 0 {
        auto
    } else {
        // Clamp to [1, total_cpus] to avoid oversubscription or zero
        override_n.clamp(1, total_cpus)
    }
}

/// Public helper: current effective worker threads (after considering override and auto mode)
#[inline]
pub fn effective_worker_count() -> u32 {
    worker_count()
}

/// Public helper: total logical CPUs detected
#[inline]
pub fn total_logical_cpus() -> u32 {
    available_parallelism().map(|p| p.get()).unwrap_or(1) as u32
}

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
use std::convert::TryInto;
use stratum_common::roles_logic_sv2::{bitcoin::block::Version, common_messages_sv2::Reconnect};

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
    let measured_total_hs = measure_hashrate(5, handicap);
    let measured_total_mhs = measured_total_hs / 1_000_000.0;
    info!(
        "Measured CPU hashrate ≈ {} MH/s",
        format_mhs(measured_total_mhs)
    );
    let measured_hashrate = measured_total_hs as f32;
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
                && (message_type == roles_logic_sv2::mining_sv2::MESSAGE_TYPE_NEW_MINING_JOB
                    || message_type
                        == roles_logic_sv2::mining_sv2::MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH
                    || message_type == roles_logic_sv2::mining_sv2::MESSAGE_TYPE_SET_TARGET)
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
        debug!("SubmitSharesSuccess: {}", m);
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
        debug!("NewMiningJob: {}", m);
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
        debug!("SetNewPrevHash: {}", m);
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
        debug!("SetTarget: {}", m);
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
    // Optimized hashing state
    fast_hasher: Option<FastSha256d>,
}

impl Miner {
    fn new(handicap: u32) -> Self {
        Self {
            target: None,
            header: None,
            job_id: None,
            version: None,
            handicap,
            fast_hasher: None,
        }
    }

    fn new_target(&mut self, target: Vec<u8>) {
        // target is sent in LE format, we'll keep it that way
        let hex_string = target
            .iter()
            .fold("".to_string(), |acc, b| acc + format!("{b:02x}").as_str());
        info!("Set target to {}", hex_string);
        // Store the target as U256 in little-endian format
        self.target = Some(U256::from_little_endian(target.as_slice()));
    }

    // Same as new_target but without logging (useful for internal probes)
    fn new_target_silent(&mut self, target: Vec<u8>) {
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
        // Build a fast hasher with midstate prepared for the static parts of the header
        if let Some(h) = &self.header {
            self.fast_hasher = Some(FastSha256d::from_header_static(h));
        } else {
            self.fast_hasher = None;
        }
    }
    pub fn next_share(&mut self) -> NextShareOutcome {
        if let Some(header) = self.header.as_ref() {
            // Use optimized path if available
            let hash: [u8; 32] = if let Some(fast) = &mut self.fast_hasher {
                fast.hash_with_nonce_time(header.nonce, header.time)
            } else {
                let hash_ = header.block_hash();
                *hash_.to_raw_hash().as_ref()
            };

            // Compare hash against target quickly in little-endian u32 words (most significant at
            // index 7)
            if let Some(target) = self.target {
                let tgt_le = target.to_little_endian();
                // Interpret as 8 little-endian u32 words
                let mut is_below = false;
                let mut is_equal = true;
                // Compare from most significant word (index 7) to least (index 0)
                for i in (0..8).rev() {
                    let off = i * 4;
                    let hw = u32::from_le_bytes([
                        hash[off],
                        hash[off + 1],
                        hash[off + 2],
                        hash[off + 3],
                    ]);
                    let tw = u32::from_le_bytes([
                        tgt_le[off],
                        tgt_le[off + 1],
                        tgt_le[off + 2],
                        tgt_le[off + 3],
                    ]);
                    match hw.cmp(&tw) {
                        core::cmp::Ordering::Less => {
                            is_below = true;
                            is_equal = false;
                            break;
                        }
                        core::cmp::Ordering::Greater => {
                            is_below = false;
                            is_equal = false;
                            break;
                        }
                        core::cmp::Ordering::Equal => {}
                    }
                }

                if is_below || is_equal {
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

// A fast double-SHA256 hasher specialized for Bitcoin block headers.
// It precomputes the midstate of the first 64 bytes (version, prev_blockhash, merkle_root[0..28])
// and allows quickly hashing varying (time, nonce) fields.
#[derive(Clone, Debug)]
pub struct FastSha256d {
    // Midstate after processing the first 64 bytes of the header (chunk 0)
    state0: [u32; 8],
    // Second block for the first SHA256 (contains merkle tail, time, bits, nonce, padding, length)
    // We mutate only the time (bytes 4..8) and nonce (bytes 12..16) per attempt.
    block1: GenericArray<u8, U64>,
    // Reusable buffer for the second SHA256 block. Bytes 32 and 56..64 are constant; we only
    // overwrite the first 32 bytes with the first digest each attempt.
    second_block: GenericArray<u8, U64>,
}

impl FastSha256d {
    pub fn from_header_static(h: &Header) -> Self {
        // Use consensus serialization to get correct 80-byte header (proper endianness).
        let header_ser = btc_serialize(h);
        debug_assert_eq!(header_ser.len(), 80, "Serialized header must be 80 bytes");
        let mut header_bytes = [0u8; 80];
        header_bytes.copy_from_slice(&header_ser);

        // First SHA256 pass: split into two 64-byte chunks
        let chunk0 = &header_bytes[0..64];
        let chunk1_last16 = &header_bytes[64..80]; // 16 bytes: merkle_tail(4), time(4), bits(4), nonce(4)

        // Compute midstate after chunk0 using compress256 on an initial state
        let mut state0 = sha256_initial_state();
        let mut block = [0u8; 64];
        block.copy_from_slice(chunk0);
        let ga0 = GenericArray::<u8, U64>::clone_from_slice(&block);
        compress256(&mut state0, std::slice::from_ref(&ga0));

        // Prepare block1 template (64 bytes) which will be:
        // bytes 0..16: last 16 bytes of header (time, bits, nonce)
        // bytes 16: 0x80 padding
        // bytes 17..56: zeros
        // bytes 56..64: length in bits of the message (80 bytes -> 640 bits) in big-endian
        let mut block1 = GenericArray::<u8, U64>::default();
        block1[0..16].copy_from_slice(chunk1_last16);
        block1[16] = 0x80;
        block1[56..64].copy_from_slice(&640u64.to_be_bytes());

        // Prepare reusable second block: set constants once
        let mut second_block = GenericArray::<u8, U64>::default();
        second_block[32] = 0x80;
        // 33..56 are already zero via default
        second_block[56..64].copy_from_slice(&256u64.to_be_bytes());

        Self {
            state0,
            block1,
            second_block,
        }
    }

    // Hashes header where only time and nonce vary, returns double-SHA256 as [u8;32] (little-endian
    // like rust-bitcoin output)
    pub fn hash_with_nonce_time(&mut self, nonce: u32, time: u32) -> [u8; 32] {
        // First SHA256 second chunk: update time and nonce at offsets 68..72 and 76..80 within
        // 80-byte header In our block1_template (offset 0..16 == 64..80 of header):
        // time at 0..4, bits at 4..8, nonce at 12..16
        // Update time and nonce in place
        self.block1[4..8].copy_from_slice(&time.to_le_bytes());
        self.block1[12..16].copy_from_slice(&nonce.to_le_bytes());

        // Compute first SHA256 digest using midstate + block1
        let mut state1 = self.state0;
        compress256(&mut state1, std::slice::from_ref(&self.block1));

        // Now perform the second SHA256 over the 32-byte first digest Build 64-byte block:
        // [digest(32)] + [0x80] + [zeros] + [length=256 bits]
        // state1 words -> big-endian bytes per SHA-256 spec (fill first 32 bytes)
        for (i, word) in state1.iter().enumerate() {
            self.second_block[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }

        let mut state2 = sha256_initial_state();
        compress256(&mut state2, std::slice::from_ref(&self.second_block));

        // Convert state2 words to bytes (big-endian), then reverse for Bitcoin-style
        // little-endian
        let mut out = [0u8; 32];
        for (i, word) in state2.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn sha256_initial_state() -> [u32; 8] {
    [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ]
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

#[inline]
fn hash_meets_target_le(hash: &[u8; 32], tgt_le: &[u8; 32]) -> bool {
    // Compare from most significant u32 word (index 7) to least (index 0)
    let mut is_below = false;
    let mut is_equal = true;
    for i in (0..8).rev() {
        let off = i * 4;
        let hw = u32::from_le_bytes([hash[off], hash[off + 1], hash[off + 2], hash[off + 3]]);
        let tw = u32::from_le_bytes([
            tgt_le[off],
            tgt_le[off + 1],
            tgt_le[off + 2],
            tgt_le[off + 3],
        ]);
        match hw.cmp(&tw) {
            core::cmp::Ordering::Less => {
                is_below = true;
                is_equal = false;
                break;
            }
            core::cmp::Ordering::Greater => {
                is_below = false;
                is_equal = false;
                break;
            }
            core::cmp::Ordering::Equal => {}
        }
    }
    is_below || is_equal
}

// Format MH/s with thousands separators and 2 decimal places using en locale separators
fn format_mhs(val_mhs: f64) -> String {
    let rounded = val_mhs.round() as i64;
    rounded.to_formatted_string(&Locale::en)
}

// returns hashrate by running all worker threads in parallel for the given duration
fn measure_hashrate(duration_secs: u64, handicap: u32) -> f64 {
    use std::sync::Barrier;

    // Prepare a random header template to hash
    let mut rng = thread_rng();
    let prev_hash: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let prev_hash = Hash::from_byte_array(prev_hash);
    let merkle_root: [u8; 32] = generate_random_32_byte_array().to_vec().try_into().unwrap();
    let merkle_root = Hash::from_byte_array(merkle_root);
    let header_template = Header {
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

    let duration = Duration::from_secs(duration_secs);
    let p = worker_count() as usize;
    let barrier = Arc::new(Barrier::new(p + 1)); // +1 for coordinator

    let mut handles = Vec::with_capacity(p);
    // Log a single consolidated target-setting message for the probe
    info!("Set target to {}", "0".repeat(64));
    for _ in 0..p {
        let barrier = barrier.clone();
        // Each thread gets its own miner and header copy
        let mut miner = Miner::new(handicap);
        // Set target to zero (silently) so we never trigger share submits; we're only counting
        // hashes
        miner.new_target_silent(vec![0_u8; 32]);
        miner.header = Some(header_template);
        if let Some(h) = miner.header.as_ref() {
            miner.fast_hasher = Some(FastSha256d::from_header_static(h));
        }
        handles.push(std::thread::spawn(move || {
            // Synchronize start across threads
            barrier.wait();
            let start = Instant::now();
            let mut hashes: u64 = 0;
            while start.elapsed() < duration {
                miner.next_share();
                hashes += 1;
            }
            hashes
        }));
    }

    // Release all workers simultaneously
    barrier.wait();
    let mut total_hashes: u64 = 0;
    for h in handles {
        total_hashes += h.join().unwrap_or(0);
    }
    // Each thread ran for approximately `duration`, so total hashes per second is total/duration
    (total_hashes as f64) / (duration_secs as f64)
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
            // Determine number of workers based on override or auto (N-1)
            let p = worker_count();
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
            // Prefer fast path with micro-batching when possible
            let can_fast =
                miner.fast_hasher.is_some() && miner.target.is_some() && miner.header.is_some();
            if can_fast {
                let header = miner.header.as_mut().unwrap();
                let time = header.time;
                let start = header.nonce;
                let tgt_le = miner.target.unwrap().to_little_endian();
                let fast = miner.fast_hasher.as_mut().unwrap();
                let mut found = None;
                let batch = nonces_per_call();
                for i in 0..batch {
                    let nonce = start.wrapping_add(i);
                    let hash = fast.hash_with_nonce_time(nonce, time);
                    if hash_meets_target_le(&hash, &tgt_le) {
                        found = Some((nonce, hash));
                        break;
                    }
                }
                if let Some((nonce, hash)) = found {
                    header.nonce = nonce;
                    info!(
                        "Found share with nonce: {}, for target: {:?}, with hash: {:?}",
                        header.nonce, miner.target, hash,
                    );
                    let job_id = miner.job_id.unwrap();
                    let version = miner.version;
                    share_send
                        .try_send((nonce, job_id, version.unwrap(), time))
                        .unwrap();
                }
                // Advance nonce window
                header.nonce = start.wrapping_add(batch);
            } else {
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
        }
    } else {
        loop {
            // Prefer fast path with micro-batching when possible
            if kill.load(Ordering::Relaxed) {
                break;
            }
            let can_fast =
                miner.fast_hasher.is_some() && miner.target.is_some() && miner.header.is_some();
            if can_fast {
                let header = miner.header.as_mut().unwrap();
                let time = header.time;
                let start = header.nonce;
                let tgt_le = miner.target.unwrap().to_little_endian();
                let fast = miner.fast_hasher.as_mut().unwrap();
                let mut found = None;
                let batch = nonces_per_call();
                for i in 0..batch {
                    let nonce = start.wrapping_add(i);
                    let hash = fast.hash_with_nonce_time(nonce, time);
                    if hash_meets_target_le(&hash, &tgt_le) {
                        found = Some((nonce, hash));
                        break;
                    }
                }
                if let Some((nonce, hash)) = found {
                    header.nonce = nonce;
                    info!(
                        "Found share with nonce: {}, for target: {:?}, with hash: {:?}",
                        header.nonce, miner.target, hash,
                    );
                    let job_id = miner.job_id.unwrap();
                    let version = miner.version;
                    share_send
                        .try_send((nonce, job_id, version.unwrap(), time))
                        .unwrap();
                }
                // Advance nonce window
                header.nonce = start.wrapping_add(batch);
            } else {
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
}
