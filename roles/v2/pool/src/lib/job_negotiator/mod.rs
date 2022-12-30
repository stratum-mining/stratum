use crate::{Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use binary_sv2::{Seq064K, B0255, B064K, U256};
use codec_sv2::{Frame, HandshakeRole, Responder};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    handlers::SendTo_,
    job_negotiation_sv2::SetCoinbase,
    parsers::{JobNegotiation, PoolMessages},
    utils::{Id, Mutex},
};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::{net::TcpListener, task};
use tracing::info;
pub type SendTo = SendTo_<roles_logic_sv2::parsers::JobNegotiation<'static>, ()>;

#[derive(Debug)]
pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
}

impl JobNegotiatorDownstream {
    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
        Self { receiver, sender }
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
    pub fn stay_alive(self_mutex: Arc<Mutex<Self>>) {
        tokio::spawn(async move {
            loop {
                let recv = self_mutex.safe_lock(|s| s.receiver.clone()).unwrap();
                let _ = recv.recv().await;
            }
        });
    }
}

pub struct JobNegotiator {
    downstreams: Vec<Arc<Mutex<JobNegotiatorDownstream>>>,
    id: Id,
}

impl JobNegotiator {
    pub async fn start(config: Configuration) {
        let self_ = Arc::new(Mutex::new(Self {
            downstreams: Vec::new(),
            id: Id::new(),
        }));
        info!("JN INITIALIZED");
        Self::accept_incoming_connection(self_, config).await;
    }
    async fn accept_incoming_connection(self_: Arc<Mutex<JobNegotiator>>, config: Configuration) {
        let listner = TcpListener::bind(&config.listen_jn_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            )
            .unwrap();

            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;
            let setup_message_from_proxy_jn = receiver.recv().await.unwrap();
            info!(
                "Setup connection message from proxy: {:?}",
                setup_message_from_proxy_jn
            );

            let setup_connection_success_to_proxy = SetupConnectionSuccess {
                used_version: 2,
                flags: 0b_0000_0000_0000_0000_0000_0000_0000_0001,
            };
            let sv2_frame: StdFrame =
                PoolMessages::Common(setup_connection_success_to_proxy.into())
                    .try_into()
                    .unwrap();
            let sv2_frame = sv2_frame.into();

            info!("Sending success message for proxy");
            sender.send(sv2_frame).await.unwrap();

            let jndownstream = Arc::new(Mutex::new(JobNegotiatorDownstream::new(
                receiver.clone(),
                sender.clone(),
            )));

            self_
                .safe_lock(|job_negotiator| job_negotiator.downstreams.push(jndownstream.clone()))
                .unwrap();

            println!(
                "NUMBER of proxies JN: {:?}",
                self_
                    .safe_lock(|job_negotiator| job_negotiator.downstreams.len())
                    .unwrap()
            );
            let coinbase_add_size = JobNegotiation::SetCoinbase(SetCoinbase {
                coinbase_output_max_additional_size: crate::COINBASE_ADD_SZIE,
                token: self_.safe_lock(|s| s.id.next()).unwrap() as u64,

                coinbase_tx_prefix: crate::COINBASE_PREFIX.try_into().unwrap(),
                coinbase_tx_suffix: crate::COINBASE_SUFFIX.try_into().unwrap(),
            });
            JobNegotiatorDownstream::send(jndownstream.clone(), coinbase_add_size)
                .await
                .unwrap();
            JobNegotiatorDownstream::stay_alive(jndownstream);
        }
    }
}
