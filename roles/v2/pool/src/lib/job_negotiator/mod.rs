use codec_sv2::{HandshakeRole, Responder};
use roles_logic_sv2::utils::Mutex;
use std::{convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
//use messages_sv2::parsers::JobNegotiation;
use network_helpers::noise_connection_tokio::Connection;
use crate::{EitherFrame, Configuration, StdFrame};
use async_channel::{Receiver, Sender};
use roles_logic_sv2::parsers::PoolMessages;
use tokio::task;
mod message_handlers;
pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
}

impl JobNegotiatorDownstream {
    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
        Self { receiver, sender }
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        // let message_type = incoming.get_header().unwrap().msg_type();
        todo!()
    }
    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::JobNegotiation<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::JobNegotiation(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
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
