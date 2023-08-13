use async_channel::{unbounded, Receiver, Sender};
use binary_sv2::{
    binary_codec_sv2::from_bytes, u256_from_int, Decodable, Deserialize, Error, Serialize,
};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use criterion::{black_box, Criterion};
use framing_sv2::framing2::NoiseFrame;
use mining_sv2::{CloseChannel, SetCustomMiningJob, UpdateChannel};
use roles_logic_sv2::{
    handlers::{common::ParseUpstreamCommonMessages, mining::ParseUpstreamMiningMessages},
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic},
    utils::{Id, Mutex},
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

// I get error `Err(WriteError(4, 0))` on thus serialize as well as all other serilaize function.
fn client_sv2_setup_connection_serialize(c: &mut Criterion) {
    c.bench_function("client_sv2_setup_connection_serialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            let setup_message: roles_logic_sv2::common_messages_sv2::SetupConnection<'_> =
                SetupConnectionHandler::get_setup_connection_message(address);
            let mut serialized_data = Vec::new();
            setup_message.to_bytes(&mut serialized_data);
        });
    });
}

// deserialize::from_bytes() is unimplemented
fn client_sv2_setup_connection_serialize_deserialize(c: &mut Criterion) {
    c.bench_function("client_sv2_setup_connection_serialize_deserialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        let setup_message: roles_logic_sv2::common_messages_sv2::SetupConnection<'_> =
            SetupConnectionHandler::get_setup_connection_message(address);
        let mut serialized_data = Vec::new();
        setup_message.to_bytes(&mut serialized_data);
        b.iter(|| {
            let deserialized: Result<roles_logic_sv2::parsers::CommonMessages, Error> =
                black_box(from_bytes(&mut serialized_data));
        });
    });
}

fn client_sv2_open_channel(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            black_box(open_channel());
        });
    });
}

fn client_sv2_open_channel_serialize(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel_serialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            let mut serialized_data = Vec::new();
            let open_channel_message = black_box(open_channel());
            black_box(open_channel_message.to_bytes(&mut serialized_data))
        });
    });
}

fn client_sv2_open_channel_serialize_deserialize(c: &mut Criterion) {
    c.bench_function("client_sv2_open_channel_serialize_deserialize", |b| {
        let address: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
        b.iter(|| {
            let mut serialized_data = Vec::new();
            let open_channel_message = black_box(open_channel());
            let serialized = black_box(open_channel_message.to_bytes(&mut serialized_data));
            let deserialized: Result<roles_logic_sv2::parsers::CommonMessages, Error> =
                black_box(from_bytes(&mut serialized_data));
        });
    });
}

fn client_sv2_mining_message_standard(c: &mut Criterion) {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    c.bench_function("client_sv2_mining_message_standard", |b| {
        b.iter(|| {
            Device::send_mining_message(self_mutex.clone(), nonce, job_id, version, ntime);
        });
    });
}

fn client_sv2_mining_message_standard_serialize(c: &mut Criterion) {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    c.bench_function("client_sv2_mining_message_standard_serialize", |b| {
        b.iter(|| {
            let submit_share_message =
                Device::send_mining_message(self_mutex.clone(), nonce, job_id, version, ntime);
            let mut serialized_data = Vec::new();
            submit_share_message.to_bytes(&mut serialized_data);
        });
    });
}

fn client_sv2_update_channel_serialize(c: &mut Criterion) {
    let channel_id = 123;
    let nominal_hash_rate = 42.5;
    let maximum_target = u256_from_int(u64::MAX);
    let update_channel = UpdateChannel {
        channel_id,
        nominal_hash_rate,
        maximum_target,
    };
    c.bench_function("client_sv2_update_channel_serialize", |b| {
        b.iter(|| {
            let mut serialized_data = Vec::new();
            black_box(update_channel.clone().to_bytes(&mut serialized_data));
        });
    });
}

fn benchmark_handle_message_mining(c: &mut Criterion) {
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let frame = create_mock_frame();
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let mut payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    c.bench_function("client-sv2-message_mining", |b| {
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

fn benchmark_handle_common_message(c: &mut Criterion) {
    let self_ = Arc::new(Mutex::new(SetupConnectionHandler {}));
    let message_type = u8::from_str_radix("8", 16).unwrap();
    let mut payload: u8 = 200;
    let payload: &mut [u8] = &mut [payload];
    c.bench_function("client-sv2-message-common", |b| {
        b.iter(|| {
            black_box(ParseUpstreamCommonMessages::handle_message_common(
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
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(5));
    client_sv2_setup_connection(&mut criterion);
    client_sv2_setup_connection_serialize(&mut criterion);
    // client_sv2_setup_connection_serialize_deserialize(&mut criterion);
    client_sv2_mining_message_standard(&mut criterion);
    client_sv2_mining_message_standard_serialize(&mut criterion);
    client_sv2_open_channel(&mut criterion);
    client_sv2_open_channel_serialize(&mut criterion);
    //client_sv2_open_channel_serialize_deserialize(&mut criterion);
    client_sv2_update_channel_serialize(&mut criterion);
    benchmark_handle_common_message(&mut criterion);
    benchmark_handle_message_mining(&mut criterion);
    criterion.final_summary();
}
