use codec_sv2::{HandshakeRole, Responder, Frame};
use roles_logic_sv2::{
    job_negotiation_sv2::CommitMiningJob,
    utils::{Id, Mutex}, parsers::CommonMessages, common_messages_sv2::SetupConnectionSuccess,
};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
//use messages_sv2::parsers::JobNegotiation;
use network_helpers::noise_connection_tokio::Connection;
use crate::{EitherFrame, Configuration, StdFrame, lib::mining_pool::setup_connection};
use async_channel::{Receiver, Sender};
use roles_logic_sv2::{
    handlers::{SendTo_},parsers::PoolMessages};
use tokio::task;
use roles_logic_sv2::handlers::job_negotiation::ParseClientJobNegotiationMessages;
pub type SendTo = SendTo_<roles_logic_sv2::parsers::JobNegotiation<'static>, ()>;
mod message_handlers;

#[derive(Debug)]
struct CommittedMiningJob {}

impl<'a> From<CommitMiningJob<'a>> for CommittedMiningJob {
    fn from(v: CommitMiningJob) -> Self {
        todo!()
    }
}
#[derive(Debug)]
pub struct JobNegotiatorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    token_to_job_map: HashMap<u32, Option<CommittedMiningJob>>,
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
        let next_message_to_send = ParseClientJobNegotiationMessages::handle_message_job_negotiation(
            self_mutex.clone(),
            message_type,
            payload
        );
        match next_message_to_send {
            Ok(SendTo::RelayNewMessage(message)) => {
                todo!();
            }
            Ok(SendTo::Respond(message)) => {
                println!("I'M HERE, ready to send the message: {:?}", message);
                Self::send(self_mutex, message).await.unwrap()
            }
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
    pub async fn start_messaging(self_mutex: Arc<Mutex<JobNegotiator>>, receiver: Receiver<EitherFrame>) {
        loop {
            let message_from_proxy_jn = receiver.recv().await.unwrap();
            let message_from_proxy_jn: StdFrame = message_from_proxy_jn.try_into().unwrap();
            // Here the pop is not a good choice since it will remove the last element so downstreams vector won't ever grow.
            let self_downstreams_ = Arc::new(Mutex::new(self_mutex.clone().
                safe_lock(|job_negotiator| job_negotiator.downstreams.pop().unwrap()).unwrap()));
            JobNegotiatorDownstream::next(self_downstreams_, message_from_proxy_jn).await;
        }
    }
}

pub struct JobNegotiator {
    downstreams: Vec<JobNegotiatorDownstream>,
}

impl JobNegotiator {
    pub async fn start(config: Configuration) {
        let self_ = Arc::new(Mutex::new(Self {
            downstreams: Vec::new(),
        }));
        println!("JN INITIALIZED");
        Self::accept_incoming_connection(self_, config).await;
    }
    async fn accept_incoming_connection(self_: Arc<Mutex<JobNegotiator>>, config: Configuration) {
        let listner = TcpListener::bind(&config.listen_jn_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            
            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            ).unwrap();
        
            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;

            let downstream = JobNegotiatorDownstream::new(receiver.clone(), sender.clone());

            self_
                .safe_lock(|job_negotiator| job_negotiator.downstreams.push(downstream))
                .unwrap();

            println!("NUMBER of proxies JN: {:?}", self_.safe_lock(|job_negotiator| job_negotiator.downstreams.len()).unwrap());


            let setup_message_from_proxy_jn = receiver.recv().await.unwrap();
            println!("Setup connection message from proxy: {:?}", setup_message_from_proxy_jn);

            let setup_connection_success_to_proxy = SetupConnectionSuccess{ used_version: 2, flags: 0b_0000_0000_0000_0000_0000_0000_0000_0001 };
            let sv2_frame: StdFrame = PoolMessages::Common(setup_connection_success_to_proxy.into()).try_into().unwrap();
            let sv2_frame = sv2_frame.into();

            println!("Sending success message for proxy");
            sender.send(sv2_frame).await.unwrap();
            let cloned = self_.clone();
            task::spawn(async move {
                JobNegotiatorDownstream::start_messaging(cloned, receiver).await;
            });
        }
    }
}
