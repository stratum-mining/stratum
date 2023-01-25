pub mod message_handler;
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::SendTo_,
    parsers::{JobNegotiation, PoolMessages},
    utils::Mutex,
};
use std::{convert::TryInto, str::FromStr};
use tracing::info;

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::job_negotiation::ParseServerJobNegotiationMessages,
    template_distribution_sv2::CoinbaseOutputDataSize,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpStream, task};

pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;
use crate::Config;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct JobNegotiator {
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
    _sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    sender_coinbase_output_max_additional_size: Sender<(CoinbaseOutputDataSize, u64)>,
}

impl JobNegotiator {
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        sender_coinbase_output_max_additional_size: Sender<(CoinbaseOutputDataSize, u64)>,
    ) -> Arc<Mutex<Self>> {
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

        info!(
            "JN proxy: setupconnection Proxy address: {:?}",
            proxy_address
        );

        SetupConnectionHandler::setup(&mut receiver, &mut sender, proxy_address)
            .await
            .unwrap();

        info!("JN CONNECTED");

        let self_ = Arc::new(Mutex::new(JobNegotiator {
            receiver,
            _sender: sender,
            sender_coinbase_output_max_additional_size,
        }));

        Self::on_upstream_message(self_.clone());
        self_
    }

    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
                let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                let message_type = incoming.get_header().unwrap().msg_type();
                let payload = incoming.payload();

                let next_message_to_send =
                    ParseServerJobNegotiationMessages::handle_message_job_negotiation(
                        self_mutex.clone(),
                        message_type,
                        payload,
                    );
                match next_message_to_send {
                    Ok(SendTo::None(Some(JobNegotiation::SetCoinbase(m)))) => {
                        let sender = self_mutex
                            .safe_lock(|s| s.sender_coinbase_output_max_additional_size.clone())
                            .unwrap();
                        let coinbase_output_max_additional_size = CoinbaseOutputDataSize {
                            coinbase_output_max_additional_size: m
                                .coinbase_output_max_additional_size,
                        };
                        sender
                            .send((coinbase_output_max_additional_size, m.token))
                            .await
                            .unwrap();
                    }
                    Ok(_) => unreachable!(),
                    Err(_) => todo!(),
                }
            }
        });
    }
}
