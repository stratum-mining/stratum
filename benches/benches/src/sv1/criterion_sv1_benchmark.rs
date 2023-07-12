//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use async_std::task;
use criterion::{criterion_group, criterion_main, Criterion};
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

fn benchmark_send_configure(c: &mut Criterion) {
    c.bench_function("connection_time", |b| {
        b.iter(|| {
            let mut client = task::block_on(client_connect());
            task::block_on(send_configure_benchmark(&mut client));
            drop(client);
        });
    });
}

fn benchmark_send_subscribe(c: &mut Criterion) {
    c.bench_function("send_subscribe_latency", |b| {
        b.iter(|| {
            let mut client = task::block_on(client_connect());
            task::block_on(send_configure_benchmark(&mut client));
            task::block_on(send_subscribe_benchmark(&mut client));
        });
    });
}

fn benchmark_send_submit(c: &mut Criterion) {
    c.bench_function("benchmark_code", |b| {
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
                            task::sleep(Duration::from_millis(2000)).await;
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
}

criterion_group!(
    benches,
    benchmark_send_configure,
    benchmark_send_subscribe,
    benchmark_send_submit
);
criterion_main!(benches);
