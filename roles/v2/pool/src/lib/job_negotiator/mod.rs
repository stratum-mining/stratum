use binary_sv2::{Seq064K, B0255, B064K, U256};
use codec_sv2::{Frame, HandshakeRole, Responder};
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    job_negotiation_sv2::CommitMiningJob,
    utils::{Id, Mutex},
};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;
//use messages_sv2::parsers::JobNegotiation;
use crate::{Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseClientJobNegotiationMessages, SendTo_},
    parsers::PoolMessages,
};
use tokio::task;
pub type SendTo = SendTo_<roles_logic_sv2::parsers::JobNegotiation<'static>, ()>;
mod message_handlers;

#[derive(Debug, Clone)]
struct CommittedMiningJob<'decoder> {
    request_id: u32,
    mining_job_token: B0255<'decoder>,
    version: u32,
    coinbase_tx_version: u32,
    coinbase_prefix: B0255<'decoder>,
    coinbase_tx_input_n_sequence: u32,
    coinbase_tx_value_remaining: u64,
    coinbase_tx_outputs: Seq064K<'decoder, B064K<'decoder>>,
    coinbase_tx_locktime: u32,
    min_extranonce_size: u16,
    tx_short_hash_nonce: u64,
    tx_short_hash_list: Seq064K<'decoder, u64>,
    tx_hash_list_hash: U256<'decoder>,
    excess_data: B064K<'decoder>,
}

impl<'a> From<CommitMiningJob<'a>> for CommittedMiningJob<'static> {
    fn from(m: CommitMiningJob) -> Self {
        m.into()
    }
}
#[derive(Debug, Clone)]
pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    token_to_job_map: HashMap<Vec<u8>, Option<CommittedMiningJob<'static>>>,
    tokens: Id,
}

impl JobNegotiatorDownstream {
    pub fn new(receiver: Receiver<EitherFrame>, sender: Sender<EitherFrame>) -> Self {
        Self {
            receiver,
            sender,
            token_to_job_map: HashMap::new(),
            tokens: Id::new(),
        }
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send =
            ParseClientJobNegotiationMessages::handle_message_job_negotiation(
                self_mutex.clone(),
                message_type,
                payload,
            );
        match next_message_to_send {
            Ok(SendTo::RelayNewMessage(message)) => {
                todo!();
            }
            Ok(SendTo::Respond(message)) => Self::send(self_mutex, message).await.unwrap(),
            Ok(SendTo::None(m)) => match m {
                _ => todo!(),
            },
            Ok(_) => panic!(),
            Err(_) => todo!(),
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
    fn check_job_validity(&mut self, _message: &CommitMiningJob) -> bool {
        true
    }
    pub async fn start_messaging(self_: Arc<Mutex<JobNegotiatorDownstream>>) {
        let receiver = self_.safe_lock(|s| s.receiver.clone()).unwrap();
        loop {
            let message_from_proxy_jn: StdFrame =
                receiver.recv().await.unwrap().try_into().unwrap();
            JobNegotiatorDownstream::next(self_.clone(), message_from_proxy_jn).await;
        }
    }
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
                flags: 0b_0000_0000_0000_0000_0000_0000_0000_0001,
            };
            let sv2_frame: StdFrame =
                PoolMessages::Common(setup_connection_success_to_proxy.into())
                    .try_into()
                    .unwrap();
            let sv2_frame = sv2_frame.into();

            println!("Sending success message for proxy");
            sender.send(sv2_frame).await.unwrap();

            let jndownstream = Arc::new(Mutex::new(JobNegotiatorDownstream::new(
                receiver.clone(),
                sender.clone(),
            )));
            let cloned = jndownstream.clone();
            task::spawn(async move {
                JobNegotiatorDownstream::start_messaging(cloned).await;
            });

            self_
                .safe_lock(|job_negotiator| job_negotiator.downstreams.push(jndownstream))
                .unwrap();

            println!(
                "NUMBER of proxies JN: {:?}",
                self_
                    .safe_lock(|job_negotiator| job_negotiator.downstreams.len())
                    .unwrap()
            );
        }
    }
}
