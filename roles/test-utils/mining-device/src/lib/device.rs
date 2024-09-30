use super::{EitherFrame, Miner, NewWorkNotifier, SetupConnectionHandler, StdFrame};
use async_channel::{Receiver, Sender};
use binary_sv2::u256_from_int;
use rand::{thread_rng, Rng};
use roles_logic_sv2::{
    common_messages_sv2::Protocol,
    common_properties::{IsMiningUpstream, IsUpstream},
    errors::Error,
    handlers::mining::{ParseUpstreamMiningMessages, SendTo, SupportedChannelTypes},
    mining_sv2::{
        CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannelSuccess,
        OpenMiningChannelError, OpenStandardMiningChannel, OpenStandardMiningChannelSuccess,
        Reconnect, SetCustomMiningJobError, SetCustomMiningJobSuccess, SetExtranoncePrefix,
        SetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesStandard, SubmitSharesSuccess,
        UpdateChannelError,
    },
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::{MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::{Id, Mutex},
};
use std::{
    convert::TryInto,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use stratum_common::bitcoin::{blockdata::block::BlockHeader, hash_types::BlockHash, hashes::Hash};
use tracing::info;

#[derive(Debug)]
pub struct Device {
    #[allow(dead_code)]
    pub(crate) receiver: Receiver<EitherFrame>,
    pub(crate) sender: Sender<EitherFrame>,
    #[allow(dead_code)]
    pub(crate) channel_opened: bool,
    pub(crate) channel_id: Option<u32>,
    pub(crate) miner: Arc<Mutex<Miner>>,
    pub(crate) jobs: Vec<NewMiningJob<'static>>,
    pub(crate) prev_hash: Option<SetNewPrevHash<'static>>,
    pub(crate) sequence_numbers: Id,
    pub(crate) notify_changes_to_mining_thread: NewWorkNotifier,
}

impl Device {
    pub(crate) async fn start(
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
            Device::open_channel(user_id),
        ));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        let self_mutex = std::sync::Arc::new(Mutex::new(self_));
        let cloned = self_mutex.clone();

        let (share_send, share_recv) = async_channel::unbounded();

        Device::start_mining_threads(update_miners, miner, share_send);
        tokio::task::spawn(async move {
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
            let mut notify_changes_to_mining_thread = self_mutex
                .safe_lock(|s| s.notify_changes_to_mining_thread.clone())
                .unwrap();
            if notify_changes_to_mining_thread.should_send
                && (message_type == const_sv2::MESSAGE_TYPE_NEW_MINING_JOB
                    || message_type == const_sv2::MESSAGE_TYPE_SET_NEW_PREV_HASH
                    || message_type == const_sv2::MESSAGE_TYPE_SET_TARGET)
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

    fn open_channel(device_id: Option<String>) -> OpenStandardMiningChannel<'static> {
        let user_identity = device_id.unwrap_or_default().try_into().unwrap();
        let id: u32 = 10;
        info!("Measuring CPU hashrate");
        let p = std::thread::available_parallelism().unwrap().get() as u32 - 3;
        let nominal_hash_rate = Device::measure_hashrate(5) as f32 * p as f32;
        info!("Pc hashrate is {}", nominal_hash_rate);
        info!("MINING DEVICE: send open channel with request id {}", id);
        OpenStandardMiningChannel {
            request_id: id.into(),
            user_identity,
            nominal_hash_rate,
            max_target: u256_from_int(567_u64),
        }
    }

    // returns hashrate based on how fast the device hashes over the given duration
    fn measure_hashrate(duration_secs: u64) -> f64 {
        let mut rng = thread_rng();
        let prev_hash: [u8; 32] = Device::generate_random_32_byte_array()
            .to_vec()
            .try_into()
            .unwrap();
        let prev_hash = Hash::from_inner(prev_hash);
        // We create a random block that we can hash, we are only interested in knowing how many hashes
        // per unit of time we can do
        let merkle_root: [u8; 32] = Device::generate_random_32_byte_array()
            .to_vec()
            .try_into()
            .unwrap();
        let merkle_root = Hash::from_inner(merkle_root);
        let header = BlockHeader {
            version: rng.gen(),
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root,
            time: std::time::SystemTime::now()
                .duration_since(
                    std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60),
                )
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

    fn start_mining_threads(
        have_new_job: Receiver<()>,
        miner: Arc<Mutex<Miner>>,
        share_send: Sender<(u32, u32, u32, u32)>,
    ) {
        tokio::task::spawn(async move {
            let mut killers: Vec<Arc<AtomicBool>> = vec![];
            loop {
                let available_parallelism = u32::max(
                    2,
                    std::thread::available_parallelism().unwrap().get() as u32,
                );
                let p = available_parallelism - 1;
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
                            Device::mine(miner, share_send, killer);
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
                miner.header.as_mut().map(|h| h.nonce += 1);
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
                miner.header.as_mut().map(|h| h.nonce += 1);
            }
        }
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
        self.miner
            .safe_lock(|miner| miner.new_target(m.maximum_target.to_vec()))
            .unwrap();
        self.notify_changes_to_mining_thread.should_send = true;
        Ok(SendTo::None(None))
    }

    fn handle_reconnect(&mut self, _: Reconnect) -> Result<SendTo<()>, Error> {
        todo!()
    }
}
