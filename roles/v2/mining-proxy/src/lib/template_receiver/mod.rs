use codec_sv2::{StandardEitherFrame, StandardSv2Frame, Sv2Frame};
use roles_logic_sv2::utils::Mutex;

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::{
        template_distribution::{
            ParseClientTemplateDistributionMessages, ParseServerTemplateDistributionMessages,
        },
        SendTo_,
    },
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, SetNewPrevHash, SubmitSolution,
    },
};
pub type SendTo = SendTo_<roles_logic_sv2::parsers::TemplateDistribution<'static>, ()>;
//use messages_sv2::parsers::JobNegotiation;
pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
use async_channel::{Receiver, Sender};
use network_helpers::plain_connection_tokio::PlainConnection;
use tracing::info;
use std::{char::ParseCharError, convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, task};
mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    send_new_tp_to_negotiator: Sender<NewTemplate<'static>>,
    send_new_ph_to_negotiator: Sender<SetNewPrevHash<'static>>,
    receive_coinbase_output_max_additional_size: Receiver<CoinbaseOutputDataSize>,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        send_new_tp_to_negotiator: Sender<NewTemplate<'static>>,
        send_new_ph_to_negotiator: Sender<SetNewPrevHash<'static>>,
        receive_coinbase_output_max_additional_size: Receiver<CoinbaseOutputDataSize>,
    ) {
        let stream = TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();

        println!("TP CONNECTED");
        let self_mutex = Arc::new(Mutex::new(Self {
            receiver: receiver.clone(),
            sender: sender.clone(),
            send_new_tp_to_negotiator,
            send_new_ph_to_negotiator,
            receive_coinbase_output_max_additional_size,
        }));
        let cloned = self_mutex.clone();

        task::spawn(async { Self::send_comas(cloned).await });
        task::spawn(async { Self::start_templates(self_mutex).await });
    }

    pub async fn send_comas(self_mutex: Arc<Mutex<Self>>) {
        // coinbase_output_max_additional_size will be needed by CoinbaseOutputDataSize
        // to start templates exchanges. This receiver takes messages from the proxy JN.
        let receiver_comas = self_mutex
            .clone()
            .safe_lock(|s| s.receive_coinbase_output_max_additional_size.clone())
            .unwrap();
        let coinbase_output_max_additional_size: CoinbaseOutputDataSize =
            receiver_comas.recv().await.unwrap();


        let sv2_frame: StdFrame = PoolMessages::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::CoinbaseOutputDataSize(coinbase_output_max_additional_size))
            .try_into()
            .unwrap();
        let sender = self_mutex
            .clone()
            .safe_lock(|s| s.sender.clone())
            .unwrap();
        let response = sender.send(sv2_frame.into()).await;

        match response {
            Ok(_m) => info!("CoinbaseOutputDataSize SENT"),
            Err(_) => info!("Problem sending CoinbaseOutputDataSize"),
        }

        
    }

    pub async fn start_templates(self_mutex: Arc<Mutex<Self>>) {
        tokio::task::spawn(async move {
            loop {
                let receiver = self_mutex
                    .clone()
                    .safe_lock(|s| s.receiver.clone())
                    .unwrap();
                let mut frame: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                let message_type = frame.get_header().unwrap().msg_type();
                let payload = frame.payload();

                let next_message_to_send =
                    ParseServerTemplateDistributionMessages::handle_message_template_distribution(
                        self_mutex.clone(),
                        message_type,
                        payload,
                    );
                match next_message_to_send {
                    Ok(SendTo::None(m)) => match m {
                        Some(TemplateDistribution::NewTemplate(m)) => {
                            let sender = self_mutex
                                .safe_lock(|s| s.send_new_tp_to_negotiator.clone())
                                .unwrap();
                            sender.send(m).await.unwrap();
                        }
                        Some(TemplateDistribution::SetNewPrevHash(m)) => {
                            let sender = self_mutex
                                .safe_lock(|s| s.send_new_ph_to_negotiator.clone())
                                .unwrap();
                            sender.send(m).await.unwrap();
                        }
                        _ => todo!(),
                    },
                    Ok(_) => panic!(),
                    Err(_) => todo!(),
                }
            }
        
        });
        
    }
}
