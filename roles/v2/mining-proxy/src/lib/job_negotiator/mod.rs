pub mod message_handler;
use std::convert::TryInto;
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::{SendTo_},
    parsers::{PoolMessages, TemplateDistribution},
    utils::Mutex,
};
use codec_sv2::Frame;
use roles_logic_sv2::handlers::job_negotiation::ParseServerJobNegotiationMessages;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    task,
};

pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<roles_logic_sv2::parsers::JobNegotiation<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;


pub struct JobNegotiator {
    sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
}

impl JobNegotiator {
    pub async fn new(address: SocketAddr, authority_public_key: [u8; 32]) {
        let stream = TcpStream::connect(address).await.unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Initiator(initiator)).await;

        let self_ = Arc::new(Mutex::new( JobNegotiator{
            sender,
            receiver,
        }));

        let cloned = self_.clone();
        task::spawn(async move {
            loop {
                let receiver = cloned.safe_lock(|d| d.receiver.clone()).unwrap();
                let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                JobNegotiator::next(cloned.clone(), incoming).await
            }
        });
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send = ParseServerJobNegotiationMessages::handle_message_job_negotiation(
            self_mutex.clone(),
            message_type,
            payload
        );
        match next_message_to_send {
            Ok(SendTo::RelayNewMessage(message)) => {
                todo!();
            }
            Ok(SendTo::Respond(message)) => {
                todo!();
            }
            Ok(SendTo::None(m)) => match m {
                _ => todo!(),
            },
            Ok(_) => panic!(),
            Err(_) => todo!(),
        }
    }

}
