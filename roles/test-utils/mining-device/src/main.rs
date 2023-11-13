use async_std::net::TcpStream;
use network_helpers::PlainConnection;
use roles_logic_sv2::utils::Id;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use stratum_common::bitcoin::{
    blockdata::block::BlockHeader, hash_types::BlockHash, hashes::Hash, util::uint::Uint256,
};

async fn connect(address: SocketAddr, handicap: u32) {
    let stream = TcpStream::connect(address).await.unwrap();
    let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        PlainConnection::new(stream, 10).await;
    Device::start(receiver, sender, address, handicap).await
}

#[async_std::main]
async fn main() {
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 34255);
    //task::spawn(async move { connect(socket, 10000).await });
    //task::spawn(async move { connect(socket, 11070).await });
    //task::spawn(async move { connect(socket, 7040).await });
    println!("start");
    connect(socket, 0).await
}

use async_channel::{Receiver, Sender};
use binary_sv2::u256_from_int;
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
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
    fn get_setup_connection_message(address: SocketAddr) -> SetupConnection<'static> {
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
    pub async fn setup(
        self_: Arc<Mutex<Self>>,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) {
        let setup_connection = Self::get_setup_connection_message(address);

        let sv2_frame: StdFrame = MiningDeviceMessages::Common(setup_connection.into())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.unwrap();

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

fn open_channel() -> OpenStandardMiningChannel<'static> {
    let user_identity = "ABC".to_string().try_into().unwrap();
    let id: u32 = 10;
    println!("MINING DEVICE: send open channel with request id {}", id);
    OpenStandardMiningChannel {
        request_id: id.into(),
        user_identity,
        nominal_hash_rate: 1000.0, // use 1000 or 10000 to test group channels
        max_target: u256_from_int(567_u64),
    }
}

impl Device {
    async fn start(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        addr: SocketAddr,
        handicap: u32,
    ) {
        let setup_connection_handler = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        SetupConnectionHandler::setup(setup_connection_handler, &mut receiver, &mut sender, addr)
            .await;
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
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        let self_mutex = std::sync::Arc::new(Mutex::new(self_));
        let cloned = self_mutex.clone();

        let (share_send, share_recv) = async_channel::unbounded();

        let handicap = miner.safe_lock(|m| m.handicap).unwrap();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_micros(handicap.into()));
            if miner.safe_lock(|m| m.next_share()).unwrap().is_ok() {
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
        println!("SUCCESS {:?}", m);
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_error(&mut self, _: SubmitSharesError) -> Result<SendTo<()>, Error> {
        println!("Submit shares error");
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

    fn handle_set_target(&mut self, _: SetTarget) -> Result<SendTo<()>, Error> {
        todo!()
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
    pub fn next_share(&mut self) -> Result<(), ()> {
        let header = self.header.as_ref().ok_or(())?;
        let mut hash = header.block_hash().as_hash().into_inner();
        hash.reverse();
        let hash = Uint256::from_be_bytes(hash);
        if hash < *self.target.as_ref().ok_or(())? {
            println!(
                "Found share with nonce: {}, for target: {:?}, with hash: {:?}",
                header.nonce, self.target, hash,
            );
            Ok(())
        } else {
            Err(())
        }
    }
}
