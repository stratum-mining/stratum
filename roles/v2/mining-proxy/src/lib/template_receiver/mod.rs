use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::utils::Mutex;

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::{template_distribution::ParseServerTemplateDistributionMessages, SendTo_},
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
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::net::TcpStream;
mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    send_new_tp_to_negotiator: Sender<(NewTemplate<'static>, u64)>,
    send_new_ph_to_negotiator: Sender<(SetNewPrevHash<'static>, u64)>,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        send_new_tp_to_negotiator: Sender<(NewTemplate<'static>, u64)>,
        send_new_ph_to_negotiator: Sender<(SetNewPrevHash<'static>, u64)>,
        receive_coinbase_output_max_additional_size: Receiver<(CoinbaseOutputDataSize, u64)>,
        solution_receiver: Receiver<SubmitSolution<'static>>,
    ) {
        let stream = TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();

        let (coinbase_output_max_additional_size, token) =
            receive_coinbase_output_max_additional_size
                .recv()
                .await
                .unwrap();

        let self_mutex = Arc::new(Mutex::new(Self {
            receiver: receiver.clone(),
            sender: sender.clone(),
            send_new_tp_to_negotiator,
            send_new_ph_to_negotiator,
        }));

        let sv2_frame: StdFrame = PoolMessages::TemplateDistribution(
            roles_logic_sv2::parsers::TemplateDistribution::CoinbaseOutputDataSize(
                coinbase_output_max_additional_size,
            ),
        )
        .try_into()
        .unwrap();
        Self::send(self_mutex.clone(), sv2_frame).await;
        let cloned = self_mutex.clone();

        tokio::task::spawn(Self::on_new_solution(cloned, solution_receiver));
        Self::start_templates(self_mutex, token);
    }

    pub async fn send(self_: Arc<Mutex<Self>>, sv2_frame: StdFrame) {
        let either_frame = sv2_frame.into();
        let sender_to_tp = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender_to_tp.send(either_frame).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
    }

    pub fn start_templates(self_mutex: Arc<Mutex<Self>>, token: u64) {
        tokio::task::spawn(async move {
            // Send CoinbaseOutputDataSize size to TP
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
                match next_message_to_send {
                    Ok(SendTo::None(m)) => match m {
                        Some(TemplateDistribution::NewTemplate(m)) => {
                            super::upstream_mining::IS_NEW_TEMPLATE_HANDLED
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                            let sender = self_mutex
                                .safe_lock(|s| s.send_new_tp_to_negotiator.clone())
                                .unwrap();
                            sender.send((m, token)).await.unwrap();
                        }
                        Some(TemplateDistribution::SetNewPrevHash(m)) => {
                            while !super::upstream_mining::IS_NEW_TEMPLATE_HANDLED
                                .load(std::sync::atomic::Ordering::SeqCst)
                            {
                                tokio::task::yield_now().await;
                            }
                            let sender = self_mutex
                                .safe_lock(|s| s.send_new_ph_to_negotiator.clone())
                                .unwrap();
                            sender.send((m, token)).await.unwrap();
                        }
                        _ => todo!(),
                    },
                    Ok(_) => panic!(),
                    Err(_) => todo!(),
                }
            }
        });
    }

    async fn on_new_solution(self_: Arc<Mutex<Self>>, rx: Receiver<SubmitSolution<'static>>) {
        while let Ok(solution) = rx.recv().await {
            let sv2_frame: StdFrame =
                PoolMessages::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into()
                    .expect("Failed to convert solution to sv2 frame!");
            Self::send(self_.clone(), sv2_frame).await
        }
    }
}
