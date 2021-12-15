use async_std::net::TcpStream;
use async_std::task;
use network_helpers::PlainConnection;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
use messages_sv2::handlers::common::{
    DownstreamCommon, Protocol, SetupConnection, SetupConnectionSuccess,
};
use messages_sv2::handlers::mining::{
    ChannelType, DownstreamMining, OpenStandardMiningChannel, OpenStandardMiningChannelSuccess,
    SendTo,
};
use messages_sv2::{Mining, MiningDeviceMessages};

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
            flags: 0b0000_0000_0000_0000_0000,
            endpoint_host,
            endpoint_port: address.port(),
            vendor,
            hardware_version,
            firmware,
            device_id,
        }
    }
    pub async fn setup(
        &mut self,
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
        self.handle_message(message_type, payload).unwrap();
    }
}

impl DownstreamCommon for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        _: SetupConnectionSuccess,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        use messages_sv2::handlers::common::SendTo;
        Ok(SendTo::None)
    }

    fn handle_setup_connection_error(
        &mut self,
        _: messages_sv2::handlers::common::SetupConnectionError,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: messages_sv2::handlers::common::ChannelEndpointChanged,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::Error> {
        todo!()
    }
}

pub struct Device {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    #[allow(dead_code)]
    channel_opened: bool,
}

fn open_channel() -> OpenStandardMiningChannel<'static> {
    let user_identity = "ABC".to_string().try_into().unwrap();
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
        SetupConnectionHandler::new()
            .setup(&mut receiver, &mut sender, addr)
            .await;
        let mut self_ = Self {
            channel_opened: false,
            receiver,
            sender,
        };
        let open_channel =
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
        let frame: StdFrame = open_channel.try_into().unwrap();
        self_.sender.send(frame.into()).await.unwrap();
        loop {
            let mut incoming: StdFrame = self_.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();
            let next = self_.handle_message(message_type, payload).unwrap();
            match next {
                SendTo::Upstream(m) => {
                    let sv2_frame: StdFrame = MiningDeviceMessages::Mining(m).try_into().unwrap();
                    let either_frame: EitherFrame = sv2_frame.into();
                    self_.sender.send(either_frame).await.unwrap();
                }
                SendTo::None => (),
                _ => panic!(),
            }
        }
    }
}

impl DownstreamMining for Device {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Standard
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel_success(
        &mut self,
        _: OpenStandardMiningChannelSuccess,
    ) -> Result<SendTo, messages_sv2::Error> {
        self.channel_opened = true;
        Ok(SendTo::None)
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        _: messages_sv2::handlers::mining::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo, messages_sv2::Error> {
        unreachable!()
    }

    fn handle_open_mining_channel_error(
        &mut self,
        _: messages_sv2::handlers::mining::OpenMiningChannelError,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_update_channel_error(
        &mut self,
        _: messages_sv2::handlers::mining::UpdateChannelError,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_close_channel(
        &mut self,
        _: messages_sv2::handlers::mining::CloseChannel,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        _: messages_sv2::handlers::mining::SetExtranoncePrefix,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_submit_shares_success(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesSuccess,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_submit_shares_error(
        &mut self,
        _: messages_sv2::handlers::mining::SubmitSharesError,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_new_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::NewMiningJob,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_new_extended_mining_job(
        &mut self,
        _: messages_sv2::handlers::mining::NewExtendedMiningJob,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_new_prev_hash(
        &mut self,
        _: messages_sv2::handlers::mining::SetNewPrevHash,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJobSuccess,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_custom_mining_job_error(
        &mut self,
        _: messages_sv2::handlers::mining::SetCustomMiningJobError,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_set_target(
        &mut self,
        _: messages_sv2::handlers::mining::SetTarget,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }

    fn handle_reconnect(
        &mut self,
        _: messages_sv2::handlers::mining::Reconnect,
    ) -> Result<SendTo, messages_sv2::Error> {
        todo!()
    }
}
