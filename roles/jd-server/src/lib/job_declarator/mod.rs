pub mod message_handler;
use crate::{error::PoolError, Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use binary_sv2::B0255;
use bitcoin::consensus::Encodable;
use codec_sv2::{Frame, HandshakeRole, Responder};
use error_handling::handle_result;
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    handlers::job_declaration::{ParseClientJobDeclarationMessages, SendTo},
    job_declaration_sv2::{AllocateMiningJobTokenSuccess, CommitMiningJobSuccess, *},
    parsers::{JobDeclaration, PoolMessages},
    utils::Mutex,
};
use std::{convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;

#[derive(Debug)]
pub struct JobDeclaratorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    // TODO this should be computed for each new template so that fees are included
    coinbase_output: Vec<u8>,
}

impl JobDeclaratorDownstream {
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
        message: roles_logic_sv2::parsers::JobDeclaration<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::JobDeclaration(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }
    pub fn start(self_mutex: Arc<Mutex<Self>>, tx_status: crate::status::Sender) {
        let recv = self_mutex.safe_lock(|s| s.receiver.clone()).unwrap();
        tokio::spawn(async move {
            loop {
                if let Ok(message) = recv.recv().await {
                    let mut frame: StdFrame = handle_result!(tx_status, message.try_into());
                    let header = frame
                        .get_header()
                        .ok_or_else(|| PoolError::Custom(String::from("No header set")));
                    let header = handle_result!(tx_status, header);
                    let message_type = header.msg_type();
                    let payload = frame.payload();
                    let next_message_to_send =
                        ParseClientJobDeclarationMessages::handle_message_job_declaration(
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

impl ParseClientJobDeclarationMessages for JobDeclaratorDownstream {
    fn handle_allocate_mining_job(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, roles_logic_sv2::Error> {
        let res = JobDeclaration::AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: get_random_token(),
            coinbase_output_max_additional_size: self.coinbase_output.len() as u32,
            // Pool do not construct ouputs bigger than 64K bytes so, self.coinbase_output can be
            // safly transformed in B064K.
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
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, roles_logic_sv2::Error> {
        let res = JobDeclaration::CommitMiningJobSuccess(CommitMiningJobSuccess {
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

pub struct JobDeclarator {
    downstreams: Vec<Arc<Mutex<JobDeclaratorDownstream>>>,
}

impl JobDeclarator {
    pub async fn start(config: Configuration, status_tx: crate::status::Sender) {
        let self_ = Arc::new(Mutex::new(Self {
            downstreams: Vec::new(),
        }));
        info!("JD INITIALIZED");
        Self::accept_incoming_connection(self_, config, status_tx).await;
    }
    async fn accept_incoming_connection(
        self_: Arc<Mutex<JobDeclarator>>,
        config: Configuration,
        status_tx: crate::status::Sender,
    ) {
        let listner = TcpListener::bind(&config.listen_jd_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            )
            .unwrap();

            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;
            let setup_message_from_proxy_jd = receiver.recv().await.unwrap();
            info!(
                "Setup connection message from proxy: {:?}",
                setup_message_from_proxy_jd
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

            let jddownstream = Arc::new(Mutex::new(JobDeclaratorDownstream::new(
                receiver.clone(),
                sender.clone(),
                &config,
            )));

            self_
                .safe_lock(|job_declarator| job_declarator.downstreams.push(jddownstream.clone()))
                .unwrap();

            JobDeclaratorDownstream::start(jddownstream, status_tx.clone());
        }
    }
}
