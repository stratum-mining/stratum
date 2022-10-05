use crate::{EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use logging::*;
use network_helpers::plain_connection_tokio::PlainConnection;
use roles_logic_sv2::{
    handlers::template_distribution::ParseServerTemplateDistributionMessages,
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{NewTemplate, SetNewPrevHash, SubmitSolution},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, ops::Deref, sync::Arc};
use tokio::{net::TcpStream, task, time::sleep};

mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx<L: Deref>
where
    L::Target: Logger,
{
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    new_template_sender: Sender<NewTemplate<'static>>,
    new_prev_hash_sender: Sender<SetNewPrevHash<'static>>,
    logger: L,
}

impl<L: Deref> TemplateRx<L>
where
    L::Target: Logger,
    L: 'static + Send,
{
    pub async fn connect(
        borrowed_logger: L,
        address: SocketAddr,
        templ_sender: Sender<NewTemplate<'static>>,
        prev_h_sender: Sender<SetNewPrevHash<'static>>,
        solution_receiver: Receiver<SubmitSolution<'static>>,
    ) {
        let mut attempts = 10;

        let stream = loop {
            match TcpStream::connect(address).await {
                Ok(st) => break st,
                Err(_) => {
                    attempts -= 1;
                    if attempts == 0 {
                        panic!("Failed to connect to template distribution server");
                    } else {
                        log_error!(
                            borrowed_logger,
                            "Failed to connect to template distribution server \
                            retrying in 5s, {} attempts left",
                            attempts
                        );
                        sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }
        };

        log_info!(borrowed_logger, "Connected to template distribution server");

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();

        let self_ = Arc::new(Mutex::new(Self {
            logger: borrowed_logger,
            receiver,
            sender,
            new_template_sender: templ_sender,
            new_prev_hash_sender: prev_h_sender,
        }));
        let cloned = self_.clone();

        task::spawn(async { Self::start(cloned).await });
        task::spawn(async { Self::on_new_solution(self_, solution_receiver).await });
    }

    pub async fn start(self_: Arc<Mutex<Self>>) {
        let (receiver, new_template_sender, new_prev_hash_sender) = self_
            .safe_lock(|s| {
                (
                    s.receiver.clone(),
                    s.new_template_sender.clone(),
                    s.new_prev_hash_sender.clone(),
                )
            })
            .unwrap();
        loop {
            let message_from_tp = receiver.recv().await.unwrap();
            let mut message_from_tp: StdFrame = message_from_tp.try_into().unwrap();
            let message_type = message_from_tp.get_header().unwrap().msg_type();
            let payload = message_from_tp.payload();

            match ParseServerTemplateDistributionMessages::handle_message_template_distribution(
                self_.clone(),
                message_type,
                payload,
            )
            .unwrap()
            {
                roles_logic_sv2::handlers::SendTo_::RelayNewMessageToRemote(_, m) => match m {
                    TemplateDistribution::CoinbaseOutputDataSize(_) => todo!(),
                    TemplateDistribution::NewTemplate(m) => {
                        new_template_sender.send(m).await.unwrap()
                    }
                    TemplateDistribution::RequestTransactionData(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataError(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataSuccess(_) => todo!(),
                    TemplateDistribution::SetNewPrevHash(m) => {
                        new_prev_hash_sender.send(m).await.unwrap()
                    }
                    TemplateDistribution::SubmitSolution(_) => todo!(),
                },
                _ => todo!(),
            }
        }
    }

    pub async fn send(self_: Arc<Mutex<Self>>, sv2_frame: StdFrame) -> Result<(), ()> {
        let either_frame = sv2_frame.into();
        let sender = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => {
                todo!()
            }
        }
    }

    async fn on_new_solution(self_: Arc<Mutex<Self>>, rx: Receiver<SubmitSolution<'static>>) {
        while let Ok(solution) = rx.recv().await {
            let sv2_frame: StdFrame =
                PoolMessages::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into()
                    .unwrap();
            Self::send(self_.clone(), sv2_frame).await.unwrap();
        }
    }
}
