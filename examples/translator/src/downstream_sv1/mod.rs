use async_std::net::TcpListener;

use async_std::prelude::*;
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;

pub(crate) mod downstream;
pub(crate) mod downstream_connection;
pub(crate) use downstream::Downstream;
pub(crate) use downstream_connection::DownstreamConnection;

pub(crate) async fn listen_downstream() {
    let listner = TcpListener::bind(crate::LISTEN_ADDR).await.unwrap();
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let server = Downstream::new(stream).await;
        Arc::new(Mutex::new(server));
    }
}
