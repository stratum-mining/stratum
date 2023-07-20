use async_channel::{Receiver, Sender};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use criterion::{criterion_group, criterion_main, Criterion};
use roles_logic_sv2::parsers::MiningDeviceMessages;

use async_std::net::TcpStream;
use codec_sv2::{HandshakeRole, Initiator};

use network_helpers::Connection;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[path = "./lib/client.rs"]
mod client;
use crate::client::Device;

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

fn handle_connection_benchmark(c: &mut Criterion) {
    // Define the benchmark function
    c.bench_function("handle_connection", |b| {
        b.iter(|| {
            async_std::task::block_on(async {
                let addr: SocketAddr =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
                let stream = TcpStream::connect(addr).await.unwrap();
                let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                    Connection::new(
                        stream,
                        HandshakeRole::Initiator(
                            Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap(),
                        ),
                        10,
                    )
                    .await;
                Device::connect(addr, receiver.clone(), sender.clone()).await
            });
        });
    });
}

fn handle_share_submission_benchmark(c: &mut Criterion) {
    // Define the benchmark function
    c.bench_function("handle_share_submission", |b| {
        b.iter(|| {
            async_std::task::block_on(async {
                let addr: SocketAddr =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
                let stream = TcpStream::connect(addr).await.unwrap();
                let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                    Connection::new(
                        stream,
                        HandshakeRole::Initiator(
                            Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap(),
                        ),
                        10,
                    )
                    .await;

                let handicap: u32 = 10;
                Device::share_submission(addr, receiver.clone(), sender.clone(), handicap).await
            });
        });
    });
}

criterion_group!(
    benches,
    handle_connection_benchmark,
    // handle_share_submission_benchmark
);
criterion_main!(benches);
