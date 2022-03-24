use async_std::{net::TcpListener, prelude::*};
use codec_sv2::{HandshakeRole, Responder};
//use messages_sv2::parsers::JobNegotiation;
use network_helpers::Connection;

use crate::EitherFrame;
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
    pub async fn start() {
        let mut self_ = Self {
            downstreams: Vec::new(),
        };
        let listner = TcpListener::bind(crate::ADDR_JOB_NEGOTIATOR).await.unwrap();
        let mut incoming = listner.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let responder = Responder::from_authority_kp(
                &crate::AUTHORITY_PUBLIC_K[..],
                &crate::AUTHORITY_PRIVATE_K[..],
                crate::CERT_VALIDITY,
            );
            let (_receiver, _sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;

            let downstream = JobNegotiatorDownstream::new();
            self_.downstreams.push(downstream);
        }
    }
}
