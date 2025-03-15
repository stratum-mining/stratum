pub(crate) mod client;
pub(crate) mod job;
pub(crate) mod miner;
use std::{
    net::SocketAddr,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

pub(crate) use client::Client;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let shutdown = Arc::new(AtomicBool::new(false));
    const ADDR: &str = "127.0.0.1:34255";
    Client::connect(
        80,
        SocketAddr::from_str(ADDR).expect("Invalid upstream address"),
        false,
        None,
        shutdown,
    )
    .await
}
