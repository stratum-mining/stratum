//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use async_std::task;
use criterion::{Criterion, Throughput};
use v1::ClientStatus;

#[path = "./lib/client.rs"]
mod client;
use crate::client::*;
use async_std::sync::{Arc, Mutex};
use std::time::Duration;

async fn client_connect() -> Arc<Mutex<Client<'static>>> {
    let client = Client::new(0, "127.0.0.1:3002".parse().unwrap()).await;
    task::sleep(Duration::from_millis(200)).await;
    client
}
async fn send_configure_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    client.send_configure().await;
    client.status = ClientStatus::Configured;
}

async fn send_subscribe_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    client.send_subscribe().await;
    return client.status = ClientStatus::Subscribed;
}

async fn send_authorize_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    let username = "user";
    client
        .send_authorize(username.to_string(), "12345".to_string())
        .await;
}
async fn send_submit_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    let username = "user";
    client.send_submit(username).await;
}

fn benchmark_connection_time(c: &mut Criterion) {
    c.bench_function("connection_time", |b| {
        b.iter(|| {
            let client = task::block_on(client_connect());
            drop(client);
        });
    });
}

fn benchmark_configure(c: &mut Criterion) {
    const SUBSCRIBE_MESSAGE_SIZE: u64 = 112;
    let mut group = c.benchmark_group("sv1");
    group.throughput(Throughput::Bytes(SUBSCRIBE_MESSAGE_SIZE));

    group.bench_function("configure", |b| {
        b.iter(|| {
            let mut client = task::block_on(client_connect());
            task::block_on(send_configure_benchmark(&mut client));
            drop(client);
        });
    });
}
fn benchmark_subscribe(c: &mut Criterion) {
    const SUBSCRIBE_MESSAGE_SIZE: u64 = 112;
    let mut group = c.benchmark_group("sv1");
    group.throughput(Throughput::Bytes(SUBSCRIBE_MESSAGE_SIZE));

    group.bench_function("subscribe", |b| {
        b.iter(|| {
            let mut client = task::block_on(client_connect());
            task::block_on(send_configure_benchmark(&mut client));
            task::block_on(send_subscribe_benchmark(&mut client));
            drop(client);
        });
    });
    group.finish();
}
fn benchmark_share_submit(c: &mut Criterion) {
    const SUBSCRIBE_MESSAGE_SIZE: u64 = 112;
    let mut group = c.benchmark_group("sv1");
    group.throughput(Throughput::Bytes(SUBSCRIBE_MESSAGE_SIZE));

    group.bench_function("share_submission", |b| {
        b.iter(|| {
            task::block_on(async {
                let client_ = Client::new(0, "127.0.0.1:3002".parse().unwrap()).await;
                for _ in 0..3 {
                    let mut client = client_.lock().await;
                    let username = "tb1qcpztf26r85y9jhr5q0y0ghu257qcmf0vn88fy5.Prisca";
                    match client.status {
                        ClientStatus::Init => client.send_configure().await,
                        ClientStatus::Configured => client.send_subscribe().await,
                        ClientStatus::Subscribed => {
                            client
                                .send_authorize(username.to_string(), "12345".to_string())
                                .await;
                            task::sleep(Duration::from_millis(1000)).await;
                            drop(client);
                            task::sleep(Duration::from_millis(1000)).await;
                            let mut client = client_.lock().await;
                            client.send_submit(username).await;
                            task::sleep(Duration::from_millis(1000)).await;
                            break;
                        }
                    }
                    drop(client);
                    task::sleep(Duration::from_millis(1000)).await;
                }
            });
        });
    });

    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().sample_size(50);
    benchmark_connection_time(&mut criterion);
    benchmark_configure(&mut criterion);
    benchmark_subscribe(&mut criterion);
    benchmark_share_submit(&mut criterion);
    criterion.final_summary();
}
