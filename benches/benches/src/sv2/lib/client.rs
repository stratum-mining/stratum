use async_channel::{Receiver, Sender};
use async_std::channel::unbounded;
use binary_sv2::u256_from_int;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use primitive_types::U256;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess},
    common_properties::{IsMiningUpstream, IsUpstream},
    errors::Error,
    handlers::{
        common::ParseCommonMessagesFromUpstream,
        mining::{ParseMiningMessagesFromUpstream, SendTo, SupportedChannelTypes},
    },
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::NoRouting,
    selectors::NullDownstreamMiningSelector,
    utils::{Id, Mutex},
};
use std::{net::SocketAddr, sync::Arc};
use stratum_common::bitcoin::{blockdata::block::Header, hash_types::BlockHash, hashes::Hash};
use tracing::{debug, error, info, trace};
pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub fn create_client() -> Device {
    let (sender, receiver) = unbounded();
    let miner = Arc::new(Mutex::new(Miner::new(10)));

    Device {
        channel_opened: false,
        receiver: receiver.clone(),
        sender: sender.clone(),
        miner: miner.clone(),
        jobs: Vec::new(),
        prev_hash: None,
        channel_id: None,
        sequence_numbers: Id::new(),
    }
}

pub fn create_mock_frame() -> StdFrame {
    let _client = create_client();
    let open_channel =
        MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
    open_channel.try_into().unwrap()
}

pub struct SetupConnectionHandler {}
use std::convert::TryInto;

impl SetupConnectionHandler {
    pub fn get_setup_connection_message(address: SocketAddr) -> SetupConnection<'static> {
        let endpoint_host = address.ip().to_string().into_bytes().try_into().unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = String::new().try_into().unwrap();
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
            device_id,
        }
    }
}

impl ParseCommonMessagesFromUpstream<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        _: SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        Ok(SendTo::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
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
    pub receiver: Receiver<EitherFrame>,
    pub sender: Sender<EitherFrame>,
    #[allow(dead_code)]
    pub channel_opened: bool,
    pub channel_id: Option<u32>,
    pub miner: Arc<Mutex<Miner>>,
    pub jobs: Vec<NewMiningJob<'static>>,
    pub prev_hash: Option<SetNewPrevHash<'static>>,
    pub sequence_numbers: Id,
}

pub fn open_channel() -> OpenStandardMiningChannel<'static> {
    let user_identity = "ABC".to_string().try_into().unwrap();
    let id: u32 = 10;
    OpenStandardMiningChannel {
        request_id: id.into(),
        user_identity,
        nominal_hash_rate: 1000.0, // use 1000 or 10000 to test group channels
        max_target: u256_from_int(567_u64),
    }
}

impl Device {
    pub fn send_mining_message(
        self_mutex: Arc<Mutex<Self>>,
        nonce: u32,
        job_id: u32,
        version: u32,
        ntime: u32,
    ) -> MiningDeviceMessages<'static> {
        let share: MiningDeviceMessages<'_> =
            MiningDeviceMessages::Mining(Mining::SubmitSharesStandard(SubmitSharesStandard {
                channel_id: self_mutex.safe_lock(|_: &mut Device| 0).unwrap(),
                sequence_number: self_mutex.safe_lock(|s| s.sequence_numbers.next()).unwrap(),
                job_id,
                nonce,
                ntime,
                version,
            }));
        share
    }
}

impl IsUpstream for Device {
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

impl IsMiningUpstream for Device {
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

impl ParseMiningMessagesFromUpstream<(), NullDownstreamMiningSelector, NoRouting> for Device {
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
        println!(
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

    fn handle_set_target(&mut self, _: SetTarget) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_reconnect(&mut self, _: Reconnect) -> Result<SendTo<()>, Error> {
        todo!()
    }
}

#[derive(Debug)]
pub struct Miner {
    header: Option<Header>,
    target: Option<U256>,
    job_id: Option<u32>,
    version: Option<u32>,
    handicap: u32,
}

impl Miner {
    pub fn new(handicap: u32) -> Self {
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
        self.target = Some(U256::from_big_endian(target.try_into().unwrap()));
    }

    fn new_header(&mut self, set_new_prev_hash: &SetNewPrevHash, new_job: &NewMiningJob) {
        self.job_id = Some(new_job.job_id);
        self.version = Some(new_job.version);
        let prev_hash: [u8; 32] = set_new_prev_hash.prev_hash.to_vec().try_into().unwrap();
        let prev_hash = Hash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = new_job.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = Hash::from_inner(merkle_root);
        // fields need to be added as BE and the are converted to LE in the background before
        // hashing
        let header = Header {
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
}
