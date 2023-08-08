use async_channel::{unbounded, Receiver, Sender};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use criterion::{black_box, Criterion};
use framing_sv2::framing2::NoiseFrame;
use roles_logic_sv2::{
    handlers::{common::ParseUpstreamCommonMessages, mining::ParseUpstreamMiningMessages},
    parsers::{Mining, MiningDeviceMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic},
    utils::{Id, Mutex},
};
use std::{convert::TryInto, sync::Arc};

#[path = "./lib/client.rs"]
mod client;
use crate::client::{create_noise_frame, open_channel, Device, Miner, SetupConnectionHandler};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

fn create_client() -> Device {
    let (sender, receiver): (
        Sender<framing_sv2::framing2::EitherFrame<MiningDeviceMessages<'static>, Vec<_>>>,
        Receiver<framing_sv2::framing2::EitherFrame<MiningDeviceMessages<'static>, Vec<_>>>,
    ) = unbounded();
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
    let client = create_client();
    let self_mutex = Arc::new(Mutex::new(client));
    let nonce: u32 = 96;
    let job_id: u32 = 1;
    let version = 78;
    let ntime = 2;
    c.bench_function("client-sv2-get-submit", |b| {
        b.iter(|| {
            Device::send_share(self_mutex.clone(), nonce, job_id, version, ntime);
        });
    });
}

fn create_mock_frame() -> StdFrame {
    let client = create_client();
    let open_channel =
        MiningDeviceMessages::Mining(Mining::OpenStandardMiningChannel(open_channel()));
    open_channel.try_into().unwrap()
}

fn serialize_benchmark(c: &mut Criterion) {
    c.bench_function("serialize_noise_frame", |b| {
        b.iter(|| {
            let noise_frame = create_noise_frame();
            let mut buffer = vec![0u8; noise_frame.encoded_length()];
            noise_frame.serialize(black_box(&mut buffer)).unwrap();
        });
    });
}

fn deserialize_noise_frame(c: &mut Criterion) {
    c.bench_function("deserialize_noise_frame", |b| {
        let noise_frame = create_noise_frame();
        let mut buffer = vec![0u8; noise_frame.encoded_length()];
        let serialized = noise_frame.serialize(&mut buffer).unwrap();
        b.iter(|| {
            let cloned = buffer.clone();
            black_box(NoiseFrame::from_bytes_unchecked(cloned.into()));
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
    let mut criterion = Criterion::default();
    serialize_benchmark(&mut criterion);
    deserialize_noise_frame(&mut criterion);
    benchmark_handle_common_message(&mut criterion);
    benchmark_handle_message_mining(&mut criterion);
    benchmark_submit(&mut criterion);

    criterion.final_summary();
}
