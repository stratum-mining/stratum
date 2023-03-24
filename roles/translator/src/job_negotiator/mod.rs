pub mod message_handler;
use crate::Error;
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::Connection;
use roles_logic_sv2::{
    handlers::SendTo_,
    parsers::{JobNegotiation, PoolMessages},
    utils::Mutex,
};
use std::{convert::TryInto, str::FromStr};
use tracing::info;

use codec_sv2::Frame;
use roles_logic_sv2::{
    bitcoin::{consensus::Decodable, TxOut},
    handlers::job_negotiation::ParseServerJobNegotiationMessages,
    job_negotiation_sv2::AllocateMiningJobToken,
    template_distribution_sv2::CoinbaseOutputDataSize,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

use crate::proxy_config::ProxyConfig;

pub struct JobNegotiator {
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
    sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    sender_coinbase_output_max_additional_size: Sender<(CoinbaseOutputDataSize, Vec<u8>)>,
    sender_coinbase_out: Sender<(Vec<TxOut>, Vec<u8>)>,
}

impl JobNegotiator {
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        sender_coinbase_output_max_additional_size: Sender<(CoinbaseOutputDataSize, Vec<u8>)>,
        sender_coinbase_out: Sender<(Vec<TxOut>, Vec<u8>)>,
        config: ProxyConfig,
    ) -> Arc<Mutex<Self>> {
        let stream = async_std::net::TcpStream::connect(address).await.unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Initiator(initiator), 10).await;

        let proxy_address = SocketAddr::new(
            IpAddr::from_str(&config.downstream_address).unwrap(),
            config.downstream_port,
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
            sender,
            sender_coinbase_output_max_additional_size,
            sender_coinbase_out,
        }));

        Self::allocate_token(self_.clone()).await;
        Self::on_upstream_message(self_.clone());
        self_
    }

    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        async_std::task::spawn(async move {
            let sender_max_size = self_mutex
                .safe_lock(|s| s.sender_coinbase_output_max_additional_size.clone())
                .unwrap();
            let sender_out_script = self_mutex
                .safe_lock(|s| s.sender_coinbase_out.clone())
                .unwrap();
            let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
            loop {
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
                    Ok(SendTo::None(Some(JobNegotiation::AllocateMiningJobTokenSuccess(m)))) => {
                        let coinbase_output_max_additional_size = CoinbaseOutputDataSize {
                            coinbase_output_max_additional_size: m
                                .coinbase_output_max_additional_size,
                        };
                        let token = m.mining_job_token.to_vec();
                        sender_max_size
                            .send((coinbase_output_max_additional_size, token.clone()))
                            .await
                            .unwrap();
                        // Assume pool is sending only one output
                        let out_script = vec![TxOut::consensus_decode(
                            m.coinbase_output.inner_as_ref(),
                        )
                        .unwrap()];
                        sender_out_script.send((out_script, token)).await.unwrap();
                    }
                    Ok(_) => unreachable!(),
                    Err(_) => todo!(),
                }
            }
        });
    }
    async fn allocate_token(self_mutex: Arc<Mutex<Self>>) {
        let message = JobNegotiation::AllocateMiningJobToken(AllocateMiningJobToken {
            user_identifier: "todo".to_string().try_into().unwrap(),
            request_id: 0,
        });
        let sender = self_mutex.safe_lock(|s| s.sender.clone()).unwrap();
        // Safe unwrap message is build above and is valid, below can never panic
        let frame: StdFrame = PoolMessages::JobNegotiation(message).try_into().unwrap();
        let _ = sender.send(frame.into()).await;
    }
}
