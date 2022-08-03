use async_channel::{Receiver, Sender};
use async_std::net::TcpListener;

use async_std::prelude::*;
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

pub(crate) mod downstream;
pub(crate) mod downstream_connection;
pub(crate) use downstream::Downstream;
pub(crate) use downstream_connection::DownstreamConnection;

pub(crate) async fn listen_downstream(
    sender_upstream: Sender<json_rpc::Message>,
    receiver_upstream: Receiver<json_rpc::Message>,
) {
    let listner = TcpListener::bind(crate::LISTEN_ADDR).await.unwrap();
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let sender_upstream_clone = sender_upstream.clone();
        let receiver_upstream_clone = receiver_upstream.clone();
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let server = Downstream::new(stream, sender_upstream_clone, receiver_upstream_clone).await;
        Arc::new(Mutex::new(server));
    }
}
