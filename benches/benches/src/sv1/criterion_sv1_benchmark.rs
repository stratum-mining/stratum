//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use async_std::task;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use v1::ClientStatus;

#[path = "./lib/client.rs"]
mod client;
use crate::client::*;
use async_std::sync::{Arc, Mutex};
use std::time::Duration;

async fn client_connect() -> Arc<Mutex<Client<'static>>> {
    let client = Client::new(0, "127.0.0.1:3002".parse().unwrap()).await;
    client
}
async fn send_configure_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    client.send_configure().await;
    client.status = ClientStatus::Configured;
}

async fn send_submit_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    let username = "tb1qcpztf26r85y9jhr5q0y0ghu257qcmf0vn88fy5.Prisca";
    client.send_submit(username).await;
}

async fn send_authorize_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    client
        .send_authorize(
            "tb1qcpztf26r85y9jhr5q0y0ghu257qcmf0vn88fy5.Prisca".to_string(),
            "12345".to_string(),
        )
        .await;
    client.status = ClientStatus::Subscribed;
}

async fn send_subscribe_benchmark(client: &mut Arc<Mutex<Client<'static>>>) {
    let mut client = client.lock().await;
    client.send_subscribe().await;
    return client.status = ClientStatus::Subscribed;
}

fn benchmark_send_configure(c: &mut Criterion) {
    c.bench_function("connection_time", |b| {
        b.iter(|| {
            black_box(|| {
                let mut client = task::block_on(client_connect());
                task::block_on(send_configure_benchmark(&mut client));
            })
        });
    });
}

fn benchmark_send_subscribe(c: &mut Criterion) {
    c.bench_function("send_subscribe_latency", |b| {
        b.iter(|| {
            black_box(|| {
                let mut client = task::block_on(client_connect());
                task::block_on(send_configure_benchmark(&mut client));
                task::block_on(send_subscribe_benchmark(&mut client));
            })
        });
    });
}

// fn benchmark_send_authorize(c: &mut Criterion) {
//     c.bench_function("send_authorize_latency", |b| {
//         b.iter_custom(|iters| {
//             black_box(|| {
//                 let mut client = task::block_on(client_connect());
//                 task::block_on(send_configure_benchmark(&mut client));
//                 task::block_on(send_subscribe_benchmark(&mut client));
//                 task::block_on(send_authorize_benchmark(&mut client));
//             })
//         });
//     });
// }

fn benchmark_send_submit(c: &mut Criterion) {
    c.bench_function("share_submission_time", |b| {
        b.iter(|| {
            black_box(|| {
                let mut client = task::block_on(client_connect());
                task::block_on(send_configure_benchmark(&mut client));
                task::block_on(send_subscribe_benchmark(&mut client));
                task::block_on(send_authorize_benchmark(&mut client));
                task::block_on(async {
                    task::sleep(Duration::from_secs(1)).await;
                    send_submit_benchmark(&mut client).await;
                });
            })
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
