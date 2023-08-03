use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
    handlers::mining::{ParseUpstreamMiningMessages, SendTo},
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic},
    utils::Mutex,
};
#[path = "./lib/client.rs"]
mod client;
use crate::client::{open_channel, Device, Miner, SetupConnectionHandler};
use framing_sv2::framing2::NoiseFrame;
use network_helpers::PlainConnection;
use roles_logic_sv2::utils::Id;

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

use async_std::net::TcpStream;
use criterion::{black_box, Criterion};
use roles_logic_sv2::handlers::common::ParseUpstreamCommonMessages;
use std::{convert::TryInto, sync::Arc};

fn create_device() -> Device {
    let stream = async_std::task::block_on(TcpStream::connect("0.0.0.0:34254")).unwrap();
    let (receiver, sender) = async_std::task::block_on(PlainConnection::new(stream, 10));
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

fn benchmark_submit(c: &mut Criterion) {
    let self_ = create_device();
    let self_mutex = Arc::new(Mutex::new(self_));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    c.bench_function("handle_submit", |b| {
        b.iter(|| {
            Device::send_share(self_mutex.clone(), nonce, job_id, version, ntime);
        });
    });
}

fn create_mock_frame() -> StdFrame {
    let open_channel =
        MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
    let frame: StdFrame = open_channel.try_into().unwrap();
    frame.clone()
}

fn benchmark_handle_message_mining(c: &mut Criterion) {
    let self_mutex = Arc::new(Mutex::new(create_device()));
    let mock_frame = create_mock_frame();
    let mut incoming: StdFrame = black_box(mock_frame);
    let message_type = incoming.get_header().unwrap().msg_type();
    let payload = incoming.payload();
    c.bench_function("handle_message_mining_benchmark", |b| {
        b.iter(|| {
            Device::handle_message_mining(
                self_mutex.clone(),
                message_type,
                payload,
                MiningRoutingLogic::None,
            )
            .unwrap();
        });
    });
}

fn benchmark_handle_common_message(c: &mut Criterion) {
    let handler = Arc::new(Mutex::new(SetupConnectionHandler {}));
    let mock_frame = create_mock_frame();
    let mut incoming: StdFrame = black_box(mock_frame);
    let message_type = incoming.get_header().unwrap().msg_type();
    let payload = incoming.payload();
    c.bench_function("handle_message_mining_benchmark", |b| {
        b.iter(|| {
            ParseUpstreamCommonMessages::handle_message_common(
                handler.clone(),
                message_type,
                payload,
                CommonRoutingLogic::None,
            )
            .unwrap();
        });
    });
}

fn main() {
    let mut criterion = Criterion::default();
    benchmark_handle_common_message(&mut criterion);
    benchmark_handle_message_mining(&mut criterion);
    benchmark_submit(&mut criterion);

    criterion.final_summary();
}
