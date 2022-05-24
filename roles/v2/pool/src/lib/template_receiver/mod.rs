use crate::{EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
//use std::sync::mpsc::Sender as SSender;
use async_std::{net::TcpStream, task};
use codec_sv2::Frame;
use network_helpers::PlainConnection;
use roles_logic_sv2::{
    handlers::template_distribution::ParseServerTemplateDistributionMessages,
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{NewTemplate, SetNewPrevHash, SubmitSolution},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};

mod message_handler;
mod setup_connection;
pub mod test_template;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    new_template_sender: Sender<NewTemplate<'static>>,
    new_prev_hash_sender: Sender<SetNewPrevHash<'static>>,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        templ_sender: Sender<NewTemplate<'static>>,
        prev_h_sender: Sender<SetNewPrevHash<'static>>,
        solution_receiver: Receiver<SubmitSolution<'static>>,
    ) {
        let stream = TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();

        let self_ = Arc::new(Mutex::new(Self {
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
                roles_logic_sv2::handlers::SendTo_::RelayNewMessage(_, m) => match m {
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
