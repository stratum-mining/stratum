use std::sync::Arc;
use codec_sv2::{HandshakeRole, Responder};
use roles_logic_sv2::utils::Mutex;
use tokio::net::TcpListener;
//use messages_sv2::parsers::JobNegotiation;
use network_helpers::noise_connection_tokio::Connection;

use crate::{EitherFrame, Configuration};
use async_channel::{Receiver, Sender};
use tokio::task;

pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
}

impl JobNegotiatorDownstream {
    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
        Self { receiver, sender }
    }
}

pub struct JobNegotiator {
    downstreams: Vec<JobNegotiatorDownstream>,
}

impl JobNegotiator {
pub async fn start(config: Configuration) {
        let mut self_ = Arc::new(Mutex::new(Self {
            downstreams: Vec::new(),
        }));
        task::spawn(async move { Self::accept_incoming_connection(self_, config) });
    }
    async fn accept_incoming_connection(self_: Arc<Mutex<JobNegotiator>>, config: Configuration) {
        let listner = TcpListener::bind(&config.jn_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            ).unwrap();
            let (_receiver, _sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;

            let downstream = JobNegotiatorDownstream::new(_receiver, _sender);
            self_
                .safe_lock(|job_negotiator| job_negotiator.downstreams.push(downstream))
                .unwrap();
        }
    }
}
