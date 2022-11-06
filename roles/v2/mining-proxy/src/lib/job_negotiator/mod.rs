pub mod message_handler;
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::SendTo_,
    job_negotiation_sv2::AllocateMiningJobToken,
    parsers::{JobNegotiation, PoolMessages, TemplateDistribution},
    utils::Mutex,
};
use std::{convert::TryInto, str::FromStr};
use tracing::info;

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::job_negotiation::ParseServerJobNegotiationMessages,
    template_distribution_sv2::{NewTemplate, SetNewPrevHash},
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{
    net::{TcpListener, TcpStream},
    task,
};

pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;
use crate::Config;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct JobNegotiator {
    sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
    receiver_new_template: Receiver<NewTemplate<'static>>,
    receiver_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    last_new_template: Option<NewTemplate<'static>>,
    set_new_prev_hash: Option<SetNewPrevHash<'static>>,
}

impl JobNegotiator {
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        receiver_new_template: Receiver<NewTemplate<'static>>,
        receiver_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    ) {
        let stream = TcpStream::connect(address).await.unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Initiator(initiator)).await;

        let config_file: String = std::fs::read_to_string("proxy-config.toml").unwrap();
        let config: Config = toml::from_str(&config_file).unwrap();
        let proxy_address = SocketAddr::new(
            IpAddr::from_str(&config.listen_address).unwrap(),
            config.listen_mining_port,
        );

        println!(
            "JN proxy: setupconnection \nProxy address: {:?}",
            proxy_address
        );

        SetupConnectionHandler::setup(&mut receiver, &mut sender, proxy_address)
            .await
            .unwrap();

        println!("JN CONNECTED");

        let self_ = Arc::new(Mutex::new(JobNegotiator {
            sender,
            receiver,
            receiver_new_template,
            receiver_set_new_prev_hash,
            last_new_template: None,
            set_new_prev_hash: None,
        }));

        let allocate_token_message =
            JobNegotiation::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: "4ss0".to_string().try_into().unwrap(),
                request_id: 1,
            });

        println!("Allocating token message {:?}", &allocate_token_message);
        Self::send(self_.clone(), allocate_token_message)
            .await
            .unwrap();

        let cloned = self_.clone();
        Self::on_new_template(cloned.clone());
        Self::on_new_prev_hash(cloned.clone());
        loop {
            match cloned.safe_lock(|s| s.last_new_template.clone()).unwrap() {
                Some(_) => break,
                None => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            }
        }

        Self::on_upstream_message(cloned);

    }

    pub fn on_new_template(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver_new_tp = self_mutex
                    .clone()
                    .safe_lock(|d| d.receiver_new_template.clone())
                    .unwrap();
                let incoming_new_template: NewTemplate =
                    receiver_new_tp.recv().await.unwrap().try_into().unwrap();
                println!("New template recieved {:?}", incoming_new_template);
                self_mutex.safe_lock(|t| {
                    t.last_new_template = Some(incoming_new_template);
                });
            }
        });
    }

    pub fn on_new_prev_hash(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver_new_ph = self_mutex
                    .clone()
                    .safe_lock(|d| d.receiver_set_new_prev_hash.clone())
                    .unwrap();
                let incoming_set_new_ph: SetNewPrevHash =
                    receiver_new_ph.recv().await.unwrap().try_into().unwrap();
                println!("SET new prev hash recieved {:?}", incoming_set_new_ph);
                self_mutex.safe_lock(|t| {
                    t.set_new_prev_hash = Some(incoming_set_new_ph);
                });
            }
        });
    }

    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
                let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                println!("next message {:?}", incoming);
                JobNegotiator::next(self_mutex.clone(), incoming).await
            }
        });
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send =
            ParseServerJobNegotiationMessages::handle_message_job_negotiation(
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
                _ => info!("MVP2 ENDS HERE"),
            },
            Ok(_) => panic!(),
            Err(_) => todo!(),
        }
    }

    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: JobNegotiation<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::JobNegotiation(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }
}
