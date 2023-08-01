//! The code uses iai library to measure the system requirements of sv1 client.

use iai::{black_box, main};
use std::{env, time::Duration, boxed::Box};
use v1::{ClientStatus,IsClient};

#[path = "./lib/client.rs"]
mod client;
use crate::client::Client;
use std::cell::UnsafeCell;

fn get_subscirbe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    black_box(client.subscribe(black_box(10), None).unwrap());
}

fn serialize_subscirbe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let mut subscribe = client.subscribe(black_box(10), None).unwrap();
    black_box(Client::serialize_message(black_box(subscribe)));
}

fn serialize_deserialize_subscirbe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let mut subscribe = client.subscribe(black_box(10), None).unwrap();
    let serlialized = Client::serialize_message(black_box(subscribe));
    Client::parse_message(black_box(serlialized));
}

fn serialize_deserialize_handle_subscirbe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let mut subscribe = client.subscribe(black_box(10), None).unwrap();
    let serlialized = Client::serialize_message(black_box(subscribe));
    let serialized = Client::parse_message(black_box(serlialized));
    client.handle_message(black_box(serialized));
}

main!(get_subscirbe,serialize_subscirbe,serialize_deserialize_subscirbe,serialize_deserialize_handle_subscirbe);
