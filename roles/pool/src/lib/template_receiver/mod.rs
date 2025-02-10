#![allow(dead_code)]
#![allow(clippy::result_large_err)]
use super::{
    error::{PoolError, PoolResult},
    mining_pool::{EitherFrame, StdFrame},
    status,
};
use codec_sv2::{HandshakeRole, Initiator};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection_tokio_with_tokio_channels::Connection;
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
use tracing::{info, warn};

mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

#[derive(Clone)]
pub struct TemplateRx {
    receiver: tokio::sync::broadcast::Sender<EitherFrame>,
    sender: tokio::sync::broadcast::Sender<EitherFrame>,
    message_received_signal: tokio::sync::broadcast::Sender<()>,
    new_template_sender: tokio::sync::mpsc::Sender<NewTemplate<'static>>,
    new_prev_hash_sender: tokio::sync::mpsc::Sender<SetNewPrevHash<'static>>,
    status_tx: status::Sender,
}

impl TemplateRx {
    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
        address: SocketAddr,
        templ_sender: tokio::sync::mpsc::Sender<NewTemplate<'static>>,
        prev_h_sender: tokio::sync::mpsc::Sender<SetNewPrevHash<'static>>,
        solution_receiver: tokio::sync::mpsc::Receiver<SubmitSolution<'static>>,
        message_received_signal: tokio::sync::broadcast::Sender<()>,
        status_tx: status::Sender,
        coinbase_out_len: u32,
        expected_tp_authority_public_key: Option<Secp256k1PublicKey>,
        shutdown: Arc<tokio::sync::Notify>,
    ) -> PoolResult<()> {
        let stream = loop {
            match TcpStream::connect(address).await {
                Ok(stream) => break stream,
                Err(err) => {
                    warn!("Failed to connect to {}: {}. Retrying...", address, err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        };
        info!("Connected to template distribution server at {}", address);

        let initiator = match expected_tp_authority_public_key {
            Some(expected_tp_authority_public_key) => {
                Initiator::from_raw_k(expected_tp_authority_public_key.into_bytes())
            }
            None => Initiator::without_pk(),
        }?;
        let (mut receiver, mut sender, _, _) = Connection::new(
            stream,
            HandshakeRole::Initiator(initiator),
            10,
            shutdown.clone(),
        )
        .await
        .unwrap();

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address).await?;

        let self_ = Self {
            receiver,
            sender: sender.clone(),
            new_template_sender: templ_sender,
            new_prev_hash_sender: prev_h_sender,
            message_received_signal,
            status_tx: status_tx.clone(),
        };

        let c_additional_size = CoinbaseOutputDataSize {
            coinbase_output_max_additional_size: coinbase_out_len,
        };
        let frame = PoolMessages::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputDataSize(c_additional_size),
        )
        .try_into()?;

        Self::send(sender.clone(), frame)?;

        task::spawn(async { Self::start(self_).await });
        task::spawn(async { Self::on_new_solution(sender, status_tx, solution_receiver).await });

        Ok(())
    }

    pub async fn start(self_: Self) {
        let (mut recv_msg_signal, receiver, new_template_sender, new_prev_hash_sender, status_tx) = (
            self_.message_received_signal.subscribe(),
            self_.receiver.clone(),
            self_.new_template_sender.clone(),
            self_.new_prev_hash_sender.clone(),
            self_.status_tx.clone(),
        );
        let mut receiver = receiver.subscribe();
        loop {
            let message_from_tp = handle_result!(status_tx, receiver.recv().await);
            info!("Message: {:?}", message_from_tp);
            let mut message_from_tp: StdFrame = handle_result!(
                status_tx,
                message_from_tp
                    .try_into()
                    .map_err(|e| PoolError::Codec(codec_sv2::Error::FramingSv2Error(e)))
            );
            let message_type_res = message_from_tp
                .get_header()
                .ok_or_else(|| PoolError::Custom(String::from("No header set")));
            let message_type = handle_result!(status_tx, message_type_res).msg_type();
            let payload = message_from_tp.payload();
            let msg = handle_result!(
                status_tx,
                ParseServerTemplateDistributionMessages::handle_message_template_distribution(
                    Arc::new(Mutex::new(self_.clone())),
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
                roles_logic_sv2::handlers::SendTo_::None(None) => (),
                _ => {
                    info!("Error: {:?}", msg);
                    std::process::abort();
                }
            }
        }
    }

    pub fn send(
        sender: tokio::sync::broadcast::Sender<EitherFrame>,
        sv2_frame: StdFrame,
    ) -> PoolResult<()> {
        let either_frame = sv2_frame.into();
        sender.send(either_frame)?;
        Ok(())
    }

    async fn on_new_solution(
        sender: tokio::sync::broadcast::Sender<EitherFrame>,
        status_tx: status::Sender,
        mut rx: tokio::sync::mpsc::Receiver<SubmitSolution<'static>>,
    ) -> PoolResult<()> {
        while let Some(solution) = rx.recv().await {
            info!("Sending Solution to TP: {:?}", &solution);
            let sv2_frame_res: Result<StdFrame, _> =
                PoolMessages::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into();
            match sv2_frame_res {
                Ok(frame) => {
                    handle_result!(status_tx, Self::send(sender.clone(), frame));
                }
                Err(_e) => {
                    // return submit error
                    todo!()
                }
            };
        }
        Ok(())
    }
}
