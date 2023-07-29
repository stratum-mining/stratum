//! The code uses iai library to measure the system requirements of sv1 client.

use async_std::{
    sync::{Arc, Mutex},
    task,
};
use iai::{black_box, main};
use std::{env, time::Duration};
use v1::ClientStatus;

#[path = "./lib/client.rs"]
mod client;
use crate::client::Client;

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

async fn bench_submit() {
    let mut client = client_connect().await;
    send_configure_benchmark(&mut client).await;
    send_subscribe_benchmark(&mut client).await;
    task::sleep(Duration::from_millis(1000)).await;
    send_authorize_benchmark(&mut client).await;
    send_submit_benchmark(&mut client).await
}

fn benchmark_submit_share() {
    task::block_on(bench_submit());
}

fn iai_sv1_share_submit() {
    black_box(benchmark_submit_share());
}

main!(iai_sv1_share_submit);
