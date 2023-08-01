//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use async_std::task;
use criterion::{Criterion, Throughput,black_box};
use std::env;
use v1::ClientStatus;
use v1::IsClient;

#[path = "./lib/client.rs"]
mod client;
use crate::client::*;
use async_std::sync::{Arc, Mutex,MutexGuard};
use std::time::Duration;

fn benchmark_get_subscribe(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-get-subscribe");

    group.bench_function("client-sv1-get-subscribe", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            // TODO add bench also for Some(extranonce)
            client.subscribe(black_box(10), None).unwrap();
        });
    });
}

fn benchmark_subscribe_serialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize");

    group.bench_function("client-sv1-subscribe-serialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            // TODO add bench also for Some(extranonce)
            let mut subscribe = client.subscribe(black_box(10), None).unwrap();
            Client::serialize_message(black_box(subscribe));
        });
    });
}

fn benchmark_subscribe_serialize_deserialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize-deserialize");

    group.bench_function("client-sv1-subscribe-serialize-deserialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let mut subscribe = client.subscribe(black_box(10), None).unwrap();
            let mut serialized = Client::serialize_message(black_box(subscribe));
            Client::parse_message(black_box(serialized));
        });
    });
}

fn benchmark_subscribe_serialize_deserialize_handle(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize-deserialize-handle");

    group.bench_function("client-sv1-subscribe-serialize-deserialize-handle", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let mut subscribe = client.subscribe(black_box(10), None).unwrap();
            let mut serialized = Client::serialize_message(black_box(subscribe));
            let deserilized = Client::parse_message(black_box(serialized));
            client.handle_message(black_box(deserilized));
        });
    });
}

fn benchmark_get_authorize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-get-authorize");

    group.bench_function("client-sv1-get-authorize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client.authorize(black_box(10),black_box("user".to_string()),black_box("passowrd".to_string())).unwrap();
        });
    });
}

fn benchmark_authorize_serialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-authorize-serialize");

    group.bench_function("client-sv1-authorize-serialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client.authorize(black_box(10),black_box("user".to_string()),black_box("passowrd".to_string())).unwrap();
            let serialized = Client::serialize_message(black_box(authorize));
        });
    });
}

fn benchmark_authorize_serialize_deserialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-authorize-serialize-deserialize");

    group.bench_function("client-sv1-authorize-serialize-deserialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client.authorize(black_box(10),black_box("user".to_string()),black_box("passowrd".to_string())).unwrap();
            let serialized = Client::serialize_message(black_box(authorize));
            Client::parse_message(black_box(serialized));
        });
    });
}

fn benchmark_authorize_serialize_deserialize_handle(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-authorize-serialize-deserialize-handle");

    group.bench_function("client-sv1-authorize-serialize-deserialize-handle", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client.authorize(black_box(10),black_box("user".to_string()),black_box("passowrd".to_string())).unwrap();
            let serialized = Client::serialize_message(black_box(authorize));
            let deserilized = Client::parse_message(black_box(serialized));
            client.handle_message(black_box(deserilized));
        });
    });
}

fn main() {
    let mut criterion = Criterion::default().sample_size(50).measurement_time(std::time::Duration::from_secs(5));
    let client = Client::new(90);
    benchmark_get_subscribe(&mut criterion, client.clone());
    benchmark_subscribe_serialize(&mut criterion, client.clone());
    benchmark_subscribe_serialize_deserialize(&mut criterion, client.clone());
    benchmark_subscribe_serialize_deserialize_handle(&mut criterion, client.clone());
    benchmark_get_authorize(&mut criterion, client.clone());
    benchmark_authorize_serialize(&mut criterion, client.clone());
    benchmark_authorize_serialize_deserialize(&mut criterion, client.clone());
    benchmark_authorize_serialize_deserialize_handle(&mut criterion, client.clone());
    criterion.final_summary();
}
