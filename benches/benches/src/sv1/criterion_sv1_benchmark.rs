//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use async_std::task;
use criterion::{Criterion, Throughput};
use std::env;
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
    let username = env::var("WALLET_ADDRESS").unwrap_or("user".to_string());
    client
        .send_authorize(username.to_string(), "12345".to_string())
        .await;
}
async fn send_submit_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    let username = env::var("WALLET_ADDRESS").unwrap_or("user".to_string());
    client.send_submit(username.as_str()).await;
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

async fn share_submit() {
    let mut client = client_connect().await;
    send_configure_benchmark(&mut client).await;
    send_subscribe_benchmark(&mut client).await;
    task::sleep(Duration::from_millis(1000)).await;
    send_authorize_benchmark(&mut client).await;
    send_submit_benchmark(&mut client).await
}

fn benchmark_share_submit(c: &mut Criterion) {
    const SUBSCRIBE_MESSAGE_SIZE: u64 = 112;
    let mut group = c.benchmark_group("sv1");
    group.throughput(Throughput::Bytes(SUBSCRIBE_MESSAGE_SIZE));
    group.bench_function("test_submit", |b| {
        b.iter(|| {
            task::block_on(share_submit());
        })
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
