//! This code uses `criterion` crate to benchmark the performance sv1.
//! It measures connection time, send subscription latency and share submission time.

use criterion::{black_box, Criterion};
use v1::{ClientStatus, IsClient};

#[path = "./lib/client.rs"]
mod client;
use crate::client::{extranonce_from_hex, notify, *};

fn benchmark_get_subscribe(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-get-subscribe");

    group.bench_function("client-sv1-get-subscribe", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let extranonce = Some(extranonce_from_hex("0000"));
            client.subscribe(black_box(10), extranonce).unwrap();
        });
    });
}

fn benchmark_subscribe_serialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize");

    group.bench_function("client-sv1-subscribe-serialize", |b| {
        client.status = ClientStatus::Configured;
        let extranonce = Some(extranonce_from_hex("0000"));
        let subscribe = client.subscribe(black_box(10), extranonce).unwrap();
        b.iter(|| {
            Client::serialize_message(black_box(subscribe.clone()));
        });
    });
}

fn benchmark_subscribe_serialize_deserialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize-deserialize");

    group.bench_function("client-sv1-subscribe-serialize-deserialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let subscribe = client.subscribe(black_box(10), None).unwrap();
            let serialized = Client::serialize_message(black_box(subscribe));
            Client::parse_message(black_box(serialized));
        });
    });
}

fn benchmark_subscribe_serialize_deserialize_handle(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-subscribe-serialize-deserialize-handle");

    group.bench_function("client-sv1-subscribe-serialize-deserialize-handle", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let subscribe = client.subscribe(black_box(10), None).unwrap();
            let serialized = Client::serialize_message(black_box(subscribe));
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
            client
                .authorize(
                    black_box(10),
                    black_box("user".to_string()),
                    black_box("passowrd".to_string()),
                )
                .unwrap();
        });
    });
}

fn benchmark_authorize_serialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-authorize-serialize");

    group.bench_function("client-sv1-authorize-serialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client
                .authorize(
                    black_box(10),
                    black_box("user".to_string()),
                    black_box("passowrd".to_string()),
                )
                .unwrap();
            Client::serialize_message(black_box(authorize));
        });
    });
}

fn benchmark_authorize_serialize_deserialize(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-sv1-authorize-serialize-deserialize");

    group.bench_function("client-sv1-authorize-serialize-deserialize", |b| {
        b.iter(|| {
            client.status = ClientStatus::Configured;
            let authorize = client
                .authorize(
                    black_box(10),
                    black_box("user".to_string()),
                    black_box("passowrd".to_string()),
                )
                .unwrap();
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
            let authorize = client
                .authorize(
                    black_box(10),
                    black_box("user".to_string()),
                    black_box("passowrd".to_string()),
                )
                .unwrap();
            let serialized = Client::serialize_message(black_box(authorize));
            let deserilized = Client::parse_message(black_box(serialized));
            client.handle_message(black_box(deserilized));
        });
    });
}

fn benchmark_get_submit(c: &mut Criterion, mut client: Client) {
    c.bench_function("client-sv1-get-submit", |b| {
        b.iter(|| {
            notify(&mut client);
            client.authorize_user_name("user".to_string());
            client
                .submit(
                    0,
                    "user".to_string(),
                    extranonce_from_hex("00"),
                    78,
                    78,
                    None,
                )
                .unwrap();
        });
    });
}

fn benchmark_submit_serialize(c: &mut Criterion, mut client: Client) {
    c.bench_function("client-submit-serialize", |b| {
        b.iter(|| {
            notify(&mut client);
            client.authorize_user_name("user".to_string());
            let submit = client
                .submit(
                    0,
                    "user".to_string(),
                    extranonce_from_hex("00"),
                    78,
                    78,
                    None,
                )
                .unwrap();
            Client::serialize_message(black_box(submit));
        });
    });
}

fn benchmark_submit_serialize_deserialize(c: &mut Criterion, mut client: Client) {
    c.bench_function("client-submit-serialize-deserialize", |b| {
        b.iter(|| {
            notify(&mut client);
            client.authorize_user_name("user".to_string());
            let submit = client
                .submit(
                    0,
                    "user".to_string(),
                    extranonce_from_hex("00"),
                    78,
                    78,
                    None,
                )
                .unwrap();
            let serialized = Client::serialize_message(black_box(submit));
            Client::parse_message(black_box(serialized));
        });
    });
}

fn benchmark_submit_serialize_deserialize_handle(c: &mut Criterion, mut client: Client) {
    let mut group = c.benchmark_group("client-submit-serialize-deserialize-handle");
    group.bench_function("client-submit-serialize-deserialize-handle", |b| {
        b.iter(|| {
            client.status = ClientStatus::Subscribed;
            notify(&mut client);
            client.authorize_user_name("user".to_string());
            let submit = client
                .submit(
                    0,
                    "user".to_string(),
                    extranonce_from_hex("00"),
                    78,
                    78,
                    None,
                )
                .unwrap();
            let serialized = Client::serialize_message(black_box(submit));
            let deserialized = Client::parse_message(serialized);
            client.handle_message(black_box(deserialized));
        });
    });
}

fn main() {
    let mut criterion = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(5));
    let client = Client::new(90);
    benchmark_get_subscribe(&mut criterion, client.clone());
    benchmark_subscribe_serialize(&mut criterion, client.clone());
    benchmark_subscribe_serialize_deserialize(&mut criterion, client.clone());
    benchmark_subscribe_serialize_deserialize_handle(&mut criterion, client.clone());
    benchmark_get_authorize(&mut criterion, client.clone());
    benchmark_authorize_serialize(&mut criterion, client.clone());
    benchmark_authorize_serialize_deserialize(&mut criterion, client.clone());
    benchmark_authorize_serialize_deserialize_handle(&mut criterion, client.clone());
    benchmark_get_submit(&mut criterion, client.clone());
    benchmark_submit_serialize(&mut criterion, client.clone());
    benchmark_submit_serialize_deserialize(&mut criterion, client.clone());
    benchmark_submit_serialize_deserialize_handle(&mut criterion, client.clone());
    criterion.final_summary();
}
