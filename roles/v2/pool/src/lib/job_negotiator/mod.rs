use crate::{Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use binary_sv2::B0255;
use bitcoin::consensus::Encodable;
use codec_sv2::{Frame, HandshakeRole, Responder};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    handlers::job_negotiation::{ParseClientJobNegotiationMessages, SendTo},
    job_negotiation_sv2::{AllocateMiningJobTokenSuccess, CommitMiningJobSuccess, *},
    parsers::{JobNegotiation, PoolMessages},
    utils::Mutex,
};
use std::{convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;

#[derive(Debug)]
pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    // TODO this should be computed for each new template so that fees are included
    coinbase_output: Vec<u8>,
}

impl JobNegotiatorDownstream {
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        config: &Configuration,
    ) -> Self {
        let mut coinbase_output = vec![];
        crate::get_coinbase_output(config)[0]
            .consensus_encode(&mut coinbase_output)
            .expect("invalid coinbase output in config");
        Self {
            receiver,
            sender,
            coinbase_output,
        }
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
    pub fn start(self_mutex: Arc<Mutex<Self>>) {
        let recv = self_mutex.safe_lock(|s| s.receiver.clone()).unwrap();
        tokio::spawn(async move {
            loop {
                if let Ok(message) = recv.recv().await {
                    let mut frame: StdFrame = message.try_into().unwrap();
                    let message_type = frame.get_header().unwrap().msg_type();
                    let payload = frame.payload();
                    let next_message_to_send =
                        ParseClientJobNegotiationMessages::handle_message_job_negotiation(
                            self_mutex.clone(),
                            message_type,
                            payload,
                        );
                    match next_message_to_send {
                        Ok(SendTo::Respond(message)) => {
                            Self::send(self_mutex.clone(), message).await.unwrap();
                        }
                        _ => unreachable!(),
                    }
                } else {
                    todo!();
                }
            }
        });
    }
}

impl ParseClientJobNegotiationMessages for JobNegotiatorDownstream {
    fn handle_allocate_mining_job(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, roles_logic_sv2::Error> {
        let res = JobNegotiation::AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: get_random_token(),
            coinbase_output_max_additional_size: crate::COINBASE_ADD_SZIE,
            coinbase_output: self.coinbase_output.clone().try_into().unwrap(),
            async_mining_allowed: true,
        });
        Ok(SendTo::Respond(res))
    }

    // Just accept any proposed job without veryfing it and rely only on the downstreams to make
    // sure that jobs are valid
    fn handle_commit_mining_job(
        &mut self,
        message: CommitMiningJob,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, roles_logic_sv2::Error> {
        let res = JobNegotiation::CommitMiningJobSuccess(CommitMiningJobSuccess {
            request_id: message.request_id,
            new_mining_job_token: message.mining_job_token.into_static().clone(),
        });
        Ok(SendTo::Respond(res))
    }
}

fn get_random_token() -> B0255<'static> {
    let inner: [u8; 32] = rand::random();
    inner.to_vec().try_into().unwrap()
}

pub struct JobNegotiator {
    downstreams: Vec<Arc<Mutex<JobNegotiatorDownstream>>>,
}

impl JobNegotiator {
    pub async fn start(config: Configuration) {
        let self_ = Arc::new(Mutex::new(Self {
            downstreams: Vec::new(),
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
                // Setup flags for async_mining_allowed
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
                &config,
            )));

            self_
                .safe_lock(|job_negotiator| job_negotiator.downstreams.push(jndownstream.clone()))
                .unwrap();

            JobNegotiatorDownstream::start(jndownstream);
        }
    }
}
