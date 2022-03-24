use tokio::net::TcpListener;
use codec_sv2::{HandshakeRole, Responder};
//use messages_sv2::parsers::JobNegotiation;
use network_helpers::noise_connection_tokio::Connection;

use crate::{EitherFrame, Configuration};
use async_channel::{Receiver, Sender};

pub struct JobNegotiatorDownstream {}

impl JobNegotiatorDownstream {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct JobNegotiator {
    downstreams: Vec<JobNegotiatorDownstream>,
}

impl JobNegotiator {
    pub async fn start(config: Configuration) {
        let mut self_ = Self {
            downstreams: Vec::new(),
        };
        let listner = TcpListener::bind(&config.jn_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await  {
            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            ).unwrap();
            let (_receiver, _sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;

            let downstream = JobNegotiatorDownstream::new();
            self_.downstreams.push(downstream);
        }
    }
}
