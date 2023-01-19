use crate::{
    error::{PoolError, PoolResult},
    status, EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use error_handling::handle_result;
use network_helpers::plain_connection_tokio::PlainConnection;
use roles_logic_sv2::{
    handlers::template_distribution::ParseServerTemplateDistributionMessages,
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, SetNewPrevHash, SubmitSolution,
    },
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, task};
use tracing::info;

mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    message_received_signal: Receiver<()>,
    new_template_sender: Sender<NewTemplate<'static>>,
    new_prev_hash_sender: Sender<SetNewPrevHash<'static>>,
    status_tx: status::Sender,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        templ_sender: Sender<NewTemplate<'static>>,
        prev_h_sender: Sender<SetNewPrevHash<'static>>,
        solution_receiver: Receiver<SubmitSolution<'static>>,
        message_received_signal: Receiver<()>,
        status_tx: status::Sender,
    ) -> PoolResult<()> {
        let stream = TcpStream::connect(address).await?;
        info!("Connected to template distribution server at {}", address);

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address).await?;

        let self_ = Arc::new(Mutex::new(Self {
            receiver,
            sender,
            new_template_sender: templ_sender,
            new_prev_hash_sender: prev_h_sender,
            message_received_signal,
            status_tx,
        }));
        let cloned = self_.clone();

        let c_additional_size = CoinbaseOutputDataSize {
            coinbase_output_max_additional_size: crate::COINBASE_ADD_SZIE,
        };
        let frame = PoolMessages::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputDataSize(c_additional_size),
        )
        .try_into()?;

        Self::send(self_.clone(), frame).await?;

        task::spawn(async { Self::start(cloned).await });
        task::spawn(async { Self::on_new_solution(self_, solution_receiver).await });

        Ok(())
    }

    pub async fn start(self_: Arc<Mutex<Self>>) {
        let recv_msg_signal = self_
            .safe_lock(|s| s.message_received_signal.clone())
            .unwrap();
        let (receiver, new_template_sender, new_prev_hash_sender, status_tx) = self_
            .safe_lock(|s| {
                (
                    s.receiver.clone(),
                    s.new_template_sender.clone(),
                    s.new_prev_hash_sender.clone(),
                    s.status_tx.clone(),
                )
            })
            .unwrap();
        loop {
            let message_from_tp = handle_result!(status_tx, receiver.recv().await);
            let mut message_from_tp: StdFrame = handle_result!(
                status_tx,
                message_from_tp
                    .try_into()
                    .map_err(|e| PoolError::Codec(codec_sv2::Error::FramingSv2Error(e)))
            );
            let message_type = message_from_tp.get_header().unwrap().msg_type();
            let payload = message_from_tp.payload();
            let msg = handle_result!(
                status_tx,
                ParseServerTemplateDistributionMessages::handle_message_template_distribution(
                    self_.clone(),
                    message_type,
                    payload,
                )
            );
            match msg {
                roles_logic_sv2::handlers::SendTo_::RelayNewMessageToRemote(_, m) => match m {
                    TemplateDistribution::CoinbaseOutputDataSize(_) => todo!(),
                    TemplateDistribution::NewTemplate(m) => {
                        let res = new_template_sender.send(m).await;
                        handle_result!(status_tx, res);
                        handle_result!(status_tx, recv_msg_signal.recv().await);
                    }
                    TemplateDistribution::RequestTransactionData(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataError(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataSuccess(_) => todo!(),
                    TemplateDistribution::SetNewPrevHash(m) => {
                        let res = new_prev_hash_sender.send(m).await;
                        handle_result!(status_tx, res);
                        handle_result!(status_tx, recv_msg_signal.recv().await);
                    }
                    TemplateDistribution::SubmitSolution(_) => todo!(),
                },
                _ => todo!(),
            }
        }
    }

    pub async fn send(self_: Arc<Mutex<Self>>, sv2_frame: StdFrame) -> PoolResult<()> {
        let either_frame = sv2_frame.into();
        let sender = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(either_frame).await?;
        Ok(())
    }

    async fn on_new_solution(self_: Arc<Mutex<Self>>, rx: Receiver<SubmitSolution<'static>>) {
        let status_tx = self_.safe_lock(|s| s.status_tx.clone()).unwrap();
        while let Ok(solution) = rx.recv().await {
            let sv2_frame_res: Result<StdFrame, _> =
                PoolMessages::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into();
            let sv2_frame = handle_result!(status_tx, sv2_frame_res);
            handle_result!(status_tx, Self::send(self_.clone(), sv2_frame).await);
        }
    }
}
