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
use network_helpers::PlainConnection;
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
mod message_handler;
mod setup_connection;
use crate::{error::Error::PoisonLock, status};
use error_handling::handle_result;
use setup_connection::SetupConnectionHandler;
use tracing::info;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    send_new_tp_to_negotiator: Sender<(NewTemplate<'static>, u64)>,
    send_new_ph_to_negotiator: Sender<(SetNewPrevHash<'static>, u64)>,
    /// Allows the tp recv to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        send_new_tp_to_negotiator: Sender<(NewTemplate<'static>, u64)>,
        send_new_ph_to_negotiator: Sender<(SetNewPrevHash<'static>, u64)>,
        receive_coinbase_output_max_additional_size: Receiver<(CoinbaseOutputDataSize, u64)>,
        solution_receiver: Receiver<SubmitSolution<'static>>,
        tx_status: status::Sender,
    ) {
        let stream = async_std::net::TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream, 10).await;

        info!("Template Receiver try to set up connection");
        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();
        info!("Template Receiver connection set up");

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
            tx_status,
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
        let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
        tokio::task::spawn(async move {
            // Send CoinbaseOutputDataSize size to TP
            loop {
                // Receive Templates and SetPrevHash from TP to send to JN
                let receiver = self_mutex
                    .clone()
                    .safe_lock(|s| s.receiver.clone())
                    .unwrap();
                let received = handle_result!(tx_status.clone(), receiver.recv().await);
                let mut frame: StdFrame = handle_result!(tx_status.clone(), received.try_into());
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
                            crate::upstream_sv2::upstream::IS_NEW_TEMPLATE_HANDLED
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                            let sender = self_mutex
                                .safe_lock(|s| s.send_new_tp_to_negotiator.clone())
                                .unwrap();
                            sender.send((m, token)).await.unwrap();
                        }
                        Some(TemplateDistribution::SetNewPrevHash(m)) => {
                            info!("Received SetNewPrevHash, waiting for IS_NEW_TEMPLATE_HANDLED");
                            while !crate::upstream_sv2::upstream::IS_NEW_TEMPLATE_HANDLED
                                .load(std::sync::atomic::Ordering::SeqCst)
                            {
                                tokio::task::yield_now().await;
                            }
                            info!("IS_NEW_TEMPLATE_HANDLED ok");
                            let partial = self_mutex
                                .safe_lock(|s| s.send_new_ph_to_negotiator.clone())
                                .map_err(|_| PoisonLock);
                            let sender = handle_result!(tx_status.clone(), partial);
                            handle_result!(tx_status.clone(), sender.send((m, token)).await);
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
