//! The code uses iai library to measure the system requirements of sv1 client.

use iai::{black_box, main};
use v1::{ClientStatus, IsClient};

#[path = "./lib/client.rs"]
mod client;
use crate::client::{extranonce_from_hex, notify, Client};

fn get_subscribe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    black_box(client.subscribe(black_box(10), None).unwrap());
}

fn serialize_subscribe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let subscribe = client.subscribe(black_box(10), None).unwrap();
    black_box(Client::serialize_message(black_box(subscribe)));
}

fn serialize_deserialize_subscribe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let subscribe = client.subscribe(black_box(10), None).unwrap();
    let serlialized = Client::serialize_message(black_box(subscribe));
    black_box(Client::parse_message(black_box(serlialized)));
}

fn serialize_deserialize_handle_subscribe() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let subscribe = client.subscribe(black_box(10), None).unwrap();
    let serlialized = Client::serialize_message(black_box(subscribe));
    let serialized = Client::parse_message(black_box(serlialized));
    black_box(client.handle_message(black_box(serialized)));
}

fn get_authorize() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    black_box(
        client
            .authorize(
                black_box(10),
                black_box("user".to_string()),
                black_box("passowrd".to_string()),
            )
            .unwrap(),
    );
}

fn serialize_authorize() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let authorize = client
        .authorize(
            black_box(10),
            black_box("user".to_string()),
            black_box("passowrd".to_string()),
        )
        .unwrap();
    black_box(Client::serialize_message(black_box(authorize)));
}

fn serialize_deserialize_authorize() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let authorize = client
        .authorize(
            black_box(10),
            black_box("user".to_string()),
            black_box("passowrd".to_string()),
        )
        .unwrap();
    let serlialized = Client::serialize_message(black_box(authorize));
    black_box(Client::parse_message(black_box(serlialized)));
}

fn serialize_deserialize_handle_authorize() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    let authorize = client
        .authorize(
            black_box(10),
            black_box("user".to_string()),
            black_box("passowrd".to_string()),
        )
        .unwrap();
    let serialized = Client::serialize_message(black_box(authorize));
    let deserialized = Client::parse_message(black_box(serialized));
    black_box(client.handle_message(black_box(deserialized)));
}

fn get_submit() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
    notify(&mut client);
    client.authorize_user_name("user".to_string());
    black_box(
        client
            .submit(
                0,
                "user".to_string(),
                extranonce_from_hex("00"),
                78,
                78,
                None,
            )
            .unwrap(),
    );
}

fn serialize_submit() {
    let mut client = Client::new(0);
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
    black_box(Client::serialize_message(black_box(submit)));
}

fn serialize_deserialize_submit() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
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
    let serlialized = Client::serialize_message(black_box(submit));
    black_box(Client::parse_message(black_box(serlialized)));
}

fn serialize_deserialize_handle_submit() {
    let mut client = Client::new(0);
    client.status = ClientStatus::Configured;
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
    let deserialized = Client::parse_message(black_box(serialized));
    black_box(client.handle_message(black_box(deserialized)));
}
main!(
    get_subscribe,
    serialize_subscribe,
    serialize_deserialize_subscribe,
    serialize_deserialize_handle_subscribe,
    get_authorize,
    serialize_authorize,
    serialize_deserialize_authorize,
    serialize_deserialize_handle_authorize,
    get_submit,
    serialize_submit,
    serialize_deserialize_submit,
    serialize_deserialize_handle_submit
);
