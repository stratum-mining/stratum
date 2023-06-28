//! The code uses iai library to measure the system requirements of sv1 client.

use async_std::task;
use std::time::Duration;
use v1::ClientStatus;
use iai::{black_box, main};

#[path = "./lib/client.rs"]
mod client;
use crate::client::*;

async fn initialize_client() {
    let client_ = Client::new(0, "127.0.0.1:3002".parse().unwrap()).await;
    loop {
        let mut client_ = client_.lock().await;
        match client_.status {
            ClientStatus::Init => client_.send_configure().await,
            ClientStatus::Configured => client_.send_subscribe().await,
            ClientStatus::Subscribed => {
                client_
                    .send_authorize(
                        "tb1qcpztf26r85y9jhr5q0y0ghu257qcmf0vn88fy5.Prisca".to_string(),
                        "passwd".to_string(),
                    )
                    .await;
                break;
            }
        }
        drop(client_);
        task::sleep(Duration::from_millis(1000)).await;
    }
    // task::sleep(Duration::from_millis(1000)).await;
    // for _ in 0..2 {
    //     let mut client_ = client_.lock().await;
    //     client_.send_submit("tb1qcpztf26r85y9jhr5q0y0ghu257qcmf0vn88fy5.Prisca").await;
    // }
}

fn iai_initialize_client() {
    black_box(task::block_on(initialize_client()))
}
main!(iai_initialize_client);
