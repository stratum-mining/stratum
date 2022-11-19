use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::utils::Mutex;

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::{
        template_distribution::{ ParseServerTemplateDistributionMessages,
        },
        SendTo_,
    },
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, SetNewPrevHash,
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
use std::{ convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, task};
mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    send_new_tp_to_negotiator: Sender<NewTemplate<'static>>,
    send_new_ph_to_negotiator: Sender<SetNewPrevHash<'static>>,
    receive_coinbase_output_max_additional_size: Receiver<CoinbaseOutputDataSize>
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
        
        Self::start_templates(self_mutex).await;
    }

    pub async fn send(self_: Arc<Mutex<Self>>, sv2_frame: StdFrame) {
        info!("\nMessage to TP: inside !\n");
        let either_frame = sv2_frame.into();
        let sender_to_tp = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender_to_tp.send(either_frame).await {
            Ok(_) => println!("\nMessage sent !!!\n"),
            Err(_) => println!("\nERROR !!!\n"),
            
        }
    }

        pub async fn start_templates(self_mutex: Arc<Mutex<Self>>) {
            tokio::task::spawn(async move {
                loop {
                    // Send CoinbaseOutputDataSize size to TP 
                    let cloned = self_mutex.clone();
                    let receiver_coinbase_output_max_additional_size = self_mutex
                    .clone()
                    .safe_lock(|s| s.receive_coinbase_output_max_additional_size.clone())
                    .unwrap();
                    let coinbase_output_max_additional_size: CoinbaseOutputDataSize =
                        receiver_coinbase_output_max_additional_size.recv().await.unwrap();

                    let sv2_frame: StdFrame = PoolMessages::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::CoinbaseOutputDataSize(coinbase_output_max_additional_size))
                        .try_into()
                        .unwrap();
                    info!("\nSV2 Frame: {:?}\n", sv2_frame);
                    task::spawn(async { Self::send(cloned, sv2_frame).await });
                    loop {
                        // Receive Templates and SetPrevHash from TP to send to JN
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
                        info!("\nNEXT MESSAGE TO SEND: {:?}\n", next_message_to_send);
                        match next_message_to_send {
                            Ok(SendTo::None(m)) => match m {
                                Some(TemplateDistribution::NewTemplate(m)) => {
                                    info!("\nSENDING NEW TEMPLATE\n");
                                    let sender = self_mutex
                                        .safe_lock(|s| s.send_new_tp_to_negotiator.clone())
                                        .unwrap();
                                    sender.send(m).await.unwrap();
                                }
                                Some(TemplateDistribution::SetNewPrevHash(m)) => {
                                    info!("\nSENDING NEW PREV HASH\n");
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
            }
            });
        }
    
}
