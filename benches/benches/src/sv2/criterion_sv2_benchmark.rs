use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use criterion::{black_box, Criterion};
use roles_logic_sv2::{
    handlers::{common::ParseCommonMessagesFromUpstream, mining::ParseMiningMessagesFromUpstream},
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
use crate::client::{
    create_client, create_mock_frame, open_channel, Device, SetupConnectionHandler,
};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

fn client_sv2_setup_connection(c: &mut Criterion) {
    c.bench_function("client_sv2_setup_connection", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            SetupConnectionHandler::get_setup_connection_message(address);
        });
    });
}

fn client_sv2_setup_connection_serialize(c: &mut Criterion) {
    c.bench_function("client_sv2_setup_connection_serialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        let setup_message = SetupConnectionHandler::get_setup_connection_message(address);
        let setup_message: Message = setup_message.into();
        let frame: StdFrame = setup_message.try_into().unwrap();
        let size = frame.encoded_length();
        let mut dst = vec![0; size];
        b.iter(move || black_box(frame.clone()).serialize(&mut dst));
    });
}

fn client_sv2_setup_connection_serialize_deserialize(c: &mut Criterion) {
    c.bench_function("client_sv2_setup_connection_serialize_deserialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        let setup_message = SetupConnectionHandler::get_setup_connection_message(address);
        let setup_message: Message = setup_message.into();
        let frame: StdFrame = setup_message.try_into().unwrap();
        let size = frame.encoded_length();
        let mut dst = vec![0; size];
        let _serialized = frame.serialize(&mut dst);
        b.iter(|| {
            let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
            let type_ = frame.get_header().unwrap().msg_type().clone();
            let payload = frame.payload();
            let _ = AnyMessage::try_from((type_, payload)).unwrap();
        });
    });
}

fn client_sv2_open_channel(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel", |b| {
        let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            black_box(MiningDeviceMessages::Mining(
                Mining::OpenStandardMiningChannel(open_channel()),
            ));
        });
    });
}

fn client_sv2_open_channel_serialize(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel_serialize", |b| {
        let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        let open_channel =
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
        let frame: StdFrame = open_channel.try_into().unwrap();
        let size = frame.encoded_length();
        let mut dst = vec![0; size];
        b.iter(|| black_box(frame.clone().serialize(&mut dst)));
    });
}

fn client_sv2_open_channel_serialize_deserialize(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel_serialize_deserialize", |b| {
        let _address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        let open_channel =
            MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
        let frame: StdFrame = open_channel.try_into().unwrap();
        let size = frame.encoded_length();
        let mut dst = vec![0; size];
        frame.serialize(&mut dst);
        b.iter(|| {
            let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
            let type_ = frame.get_header().unwrap().msg_type().clone();
            let payload = frame.payload();
            black_box(AnyMessage::try_from((type_, payload)).unwrap());
        });
    });
}

fn client_sv2_mining_message_submit_standard(c: &mut Criterion) {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    c.bench_function("client_sv2_mining_message_submit_standard", |b| {
        b.iter(|| {
            Device::send_mining_message(self_mutex.clone(), nonce, job_id, version, ntime);
        });
    });
}

fn client_sv2_mining_message_submit_standard_serialize(c: &mut Criterion) {
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
    c.bench_function("client_sv2_mining_message_submit_standard_serialize", |b| {
        b.iter(|| black_box(frame.clone().serialize(&mut dst)));
    });
}

fn client_sv2_mining_message_submit_standard_serialize_deserialize(c: &mut Criterion) {
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
    c.bench_function(
        "client_sv2_mining_message_submit_standard_serialize_deserialize",
        |b| {
            b.iter(|| {
                let mut frame = StdFrame::from_bytes(black_box(dst.clone().into())).unwrap();
                let type_ = frame.get_header().unwrap().msg_type().clone();
                let payload = frame.payload();
                black_box(AnyMessage::try_from((type_, payload)).unwrap());
            });
        },
    );
}

fn client_sv2_handle_message_mining(c: &mut Criterion) {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let _frame = create_mock_frame();
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    c.bench_function("client_sv2_handle_message_mining", |b| {
        b.iter(|| {
            black_box(Device::handle_message_mining(
                self_mutex.clone(),
                message_type,
                payload,
                MiningRoutingLogic::None,
            ))
        });
    });
}

fn client_sv2_handle_message_common(c: &mut Criterion) {
    let self_ = Arc::new(Mutex::new(SetupConnectionHandler {}));
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    c.bench_function("client_sv2_handle_message_common", |b| {
        b.iter(|| {
            black_box(ParseCommonMessagesFromUpstream::handle_message_common(
                self_.clone(),
                message_type,
                payload,
                CommonRoutingLogic::None,
            ))
        });
    });
}

fn main() {
    let mut criterion = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(5));
    client_sv2_setup_connection(&mut criterion);
    client_sv2_setup_connection_serialize(&mut criterion);
    client_sv2_setup_connection_serialize_deserialize(&mut criterion);
    client_sv2_open_channel(&mut criterion);
    client_sv2_open_channel_serialize(&mut criterion);
    client_sv2_open_channel_serialize_deserialize(&mut criterion);
    client_sv2_mining_message_submit_standard(&mut criterion);
    client_sv2_mining_message_submit_standard_serialize(&mut criterion);
    client_sv2_mining_message_submit_standard_serialize_deserialize(&mut criterion);
    client_sv2_handle_message_common(&mut criterion);
    client_sv2_handle_message_mining(&mut criterion);
    criterion.final_summary();
}
