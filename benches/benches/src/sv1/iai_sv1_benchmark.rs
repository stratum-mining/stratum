//! The code uses iai library to measure the system requirements of sv1 client.

use async_std::task;
use iai::{black_box, main};
use std::time::Duration;
use v1::ClientStatus;

#[path = "./lib/client.rs"]
mod client;
use crate::client::Client;

async fn initialize_client() {
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
}

fn iai_initialize_client() {
    black_box(task::block_on(initialize_client()))
}
main!(iai_initialize_client);
