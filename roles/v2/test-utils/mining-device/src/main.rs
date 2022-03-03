use async_std::net::TcpStream;
use async_std::task;
use network_helpers::PlainConnection;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

async fn connect(address: SocketAddr) {
    let stream = TcpStream::connect(address).await.unwrap();
    let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        PlainConnection::new(stream).await;
    Device::start(receiver, sender, address).await
}

#[async_std::main]
async fn main() {
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 34255);
    task::spawn(async move { connect(socket).await });
    task::spawn(async move { connect(socket).await });
    task::spawn(async move { connect(socket).await });
    connect(socket).await
}

use async_channel::{Receiver, Sender};
use binary_sv2::u256_from_int;
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use messages_sv2::common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess};
use messages_sv2::common_properties::{IsMiningUpstream, IsUpstream};
use messages_sv2::errors::Error;
use messages_sv2::handlers::common::ParseUpstreamCommonMessages;
use messages_sv2::handlers::mining::{ChannelType, ParseUpstreamMiningMessages, SendTo};
use messages_sv2::mining_sv2::*;
use messages_sv2::parsers::{Mining, MiningDeviceMessages};
use messages_sv2::routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting};
use messages_sv2::selectors::NullDownstreamMiningSelector;
use messages_sv2::utils::Mutex;

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
            flags: 0b1000_0000_0000_0000_0000_0000_0000_0000,
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
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        use messages_sv2::handlers::common::SendTo;
        Ok(SendTo::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: messages_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: messages_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        todo!()
    }
}

#[derive(Debug)]
pub struct Device {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    #[allow(dead_code)]
    channel_opened: bool,
}

fn open_channel() -> OpenStandardMiningChannel<'static> {
    let user_identity = "ABC".to_string().try_into().unwrap();
    let id = 10;
    println!("MINING DEVICE: send open channel with request id {}", id);
    OpenStandardMiningChannel {
        request_id: 10,
        user_identity,
        nominal_hash_rate: 5.4,
        max_target: u256_from_int(567_u64),
    }
}

impl Device {
    async fn start(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        addr: SocketAddr,
    ) {
        let setup_connection_handler = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        SetupConnectionHandler::setup(setup_connection_handler, &mut receiver, &mut sender, addr)
            .await;
        let self_ = Self {
            channel_opened: false,
            receiver: receiver.clone(),
            sender: sender.clone(),
        };
        let open_channel =
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        let self_mutex = std::sync::Arc::new(Mutex::new(self_));
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
                SendTo::RelayNewMessage(_, m) => {
                    let sv2_frame: StdFrame = MiningDeviceMessages::Mining(m).try_into().unwrap();
                    let either_frame: EitherFrame = sv2_frame.into();
                    sender.send(either_frame).await.unwrap();
                }
                SendTo::None(_) => (),
                _ => panic!(),
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

    fn get_mapper(&mut self) -> Option<&mut messages_sv2::common_properties::RequestIdMapper> {
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
    ) -> &mut Vec<messages_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, _: messages_sv2::common_properties::UpstreamChannel) {
        todo!()
    }
}

impl ParseUpstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Device {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Standard
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
        println!(
            "MINING DEVICE: channel opened with: group id {}, channel id {}, request id {}",
            m.group_channel_id, m.channel_id, m.request_id
        );
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
        _: SubmitSharesSuccess,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_error(&mut self, _: SubmitSharesError) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_new_mining_job(&mut self, _: NewMiningJob) -> Result<SendTo<()>, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_new_extended_mining_job(
        &mut self,
        _: NewExtendedMiningJob,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_new_prev_hash(&mut self, _: SetNewPrevHash) -> Result<SendTo<()>, Error> {
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
