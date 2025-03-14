use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use iai::{black_box, main};
use roles_logic_sv2::{
    handlers::{
        common::ParseCommonMessagesFromUpstream, mining::ParseMiningMessagesFromUpstream, SendTo_,
    },
    parsers::{AnyMessage, Mining, MiningDeviceMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic},
    utils::Mutex,
};
use std::{
    convert::TryInto,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

#[path = "./lib/client.rs"]
mod client;
use crate::client::{create_client, open_channel, Device, SetupConnectionHandler};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

fn client_sv2_setup_connection() {
    let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    black_box(SetupConnectionHandler::get_setup_connection_message(
        address,
    ));
}

fn client_sv2_setup_connection_serialize() -> Result<(), framing_sv2::Error> {
    let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    let setup_message = SetupConnectionHandler::get_setup_connection_message(address);
    let setup_message: Message = setup_message.into();
    let frame: StdFrame = setup_message.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    black_box(frame.clone()).serialize(&mut dst)
}

fn client_sv2_setup_connection_serialize_deserialize() {
    let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    let setup_message = SetupConnectionHandler::get_setup_connection_message(address);
    let setup_message: Message = setup_message.into();
    let frame: StdFrame = setup_message.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    frame.serialize(&mut dst);
    let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
    let type_ = frame.get_header().unwrap().msg_type().clone();
    let payload = frame.payload();
    black_box(AnyMessage::try_from((type_, payload)));
}

fn client_sv2_open_channel() {
    let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    black_box(MiningDeviceMessages::Mining(
        Mining::OpenStandardMiningChannel(open_channel()),
    ));
}

fn client_sv2_open_channel_serialize() -> Result<(), framing_sv2::Error> {
    let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    let open_channel =
        MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
    let frame: StdFrame = open_channel.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    black_box(frame.clone().serialize(&mut dst))
}

fn client_sv2_open_channel_serialize_deserialize() {
    let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    let open_channel =
        MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
    let frame: StdFrame = open_channel.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    frame.serialize(&mut dst);
    let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
    let type_ = frame.get_header().unwrap().msg_type().clone();
    let payload = frame.payload();
    black_box(AnyMessage::try_from((type_, payload)));
}

fn client_sv2_mining_message_submit_standard() {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    black_box(Device::send_mining_message(
        self_mutex.clone(),
        nonce,
        job_id,
        version,
        ntime,
    ));
}

fn client_sv2_mining_message_submit_standard_serialize() -> Result<(), framing_sv2::Error> {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    let submit_share_message =
        Device::send_mining_message(self_mutex.clone(), nonce, job_id, version, ntime);
    let frame: StdFrame = submit_share_message.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    black_box(frame.clone().serialize(&mut dst))
}

fn client_sv2_mining_message_submit_standard_serialize_deserialize() {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    let submit_share_message =
        Device::send_mining_message(self_mutex.clone(), nonce, job_id, version, ntime);
    let frame: StdFrame = submit_share_message.try_into().unwrap();
    let size = frame.encoded_length();
    let mut dst = vec![0; size];
    frame.serialize(&mut dst);
    let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
    let type_ = frame.get_header().unwrap().msg_type().clone();
    let payload = frame.payload();
    black_box(AnyMessage::try_from((type_, payload)));
}

fn client_sv2_handle_message_mining(
) -> Result<SendTo_<roles_logic_sv2::parsers::Mining<'static>, ()>, roles_logic_sv2::Error> {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    black_box(Device::handle_message_mining(
        self_mutex.clone(),
        message_type,
        payload,
        MiningRoutingLogic::None,
    ))
}

fn client_sv2_handle_message_common() {
    let self_ = Arc::new(Mutex::new(SetupConnectionHandler {}));
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    black_box(ParseCommonMessagesFromUpstream::handle_message_common(
        self_.clone(),
        message_type,
        payload,
        CommonRoutingLogic::None,
    ));
}

main! {
    client_sv2_setup_connection,
    client_sv2_setup_connection_serialize,
    client_sv2_setup_connection_serialize_deserialize,
    client_sv2_mining_message_submit_standard,
    client_sv2_mining_message_submit_standard_serialize,
    client_sv2_mining_message_submit_standard_serialize_deserialize,
    client_sv2_open_channel,
    client_sv2_open_channel_serialize,
    client_sv2_open_channel_serialize_deserialize,
    client_sv2_handle_message_common,
    client_sv2_handle_message_mining
}
