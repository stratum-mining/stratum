use crate::{
    error::{JdsError, JdsResult},
    status, EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use error_handling::handle_result;
use network_helpers::plain_connection_tokio::PlainConnection;
use roles_logic_sv2::{
    handlers::template_distribution::ParseServerTemplateDistributionMessages,
    parsers::{PoolMessages as JdsMessages, TemplateDistribution},
    template_distribution_sv2::CoinbaseOutputDataSize,
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
    status_tx: status::Sender,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        status_tx: status::Sender,
        coinbase_out_len: u32,
    ) -> JdsResult<()> {
        let stream = TcpStream::connect(address).await?;
        info!("Connected to template distribution server at {}", address);

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address).await?;

        let self_ = Arc::new(Mutex::new(Self {
            receiver,
            sender,
            status_tx,
        }));
        let cloned = self_.clone();

        let c_additional_size = CoinbaseOutputDataSize {
            coinbase_output_max_additional_size: coinbase_out_len,
        };
        let frame = JdsMessages::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputDataSize(c_additional_size),
        )
        .try_into()?;

        Self::send(self_.clone(), frame).await?;

        task::spawn(async { Self::start(cloned).await });

        Ok(())
    }

    pub async fn start(self_: Arc<Mutex<Self>>) {
        let (receiver, status_tx) = self_
            .safe_lock(|s| (s.receiver.clone(), s.status_tx.clone()))
            .unwrap();
        loop {
            let message_from_tp = handle_result!(status_tx, receiver.recv().await);
            let mut message_from_tp: StdFrame = handle_result!(
                status_tx,
                message_from_tp
                    .try_into()
                    .map_err(|e| JdsError::Codec(codec_sv2::Error::FramingSv2Error(e)))
            );
            let message_type_res = message_from_tp
                .get_header()
                .ok_or_else(|| JdsError::Custom(String::from("No header set")));
            let message_type = handle_result!(status_tx, message_type_res).msg_type();
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
                    TemplateDistribution::NewTemplate(_) => (),
                    TemplateDistribution::RequestTransactionData(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataError(_) => todo!(),
                    TemplateDistribution::RequestTransactionDataSuccess(_) => todo!(),
                    TemplateDistribution::SetNewPrevHash(_) => (),
                    TemplateDistribution::SubmitSolution(_) => todo!(),
                },
                roles_logic_sv2::handlers::SendTo_::None(None) => (),
                _ => todo!(),
            }
        }
    }

    pub async fn send(self_: Arc<Mutex<Self>>, sv2_frame: StdFrame) -> JdsResult<()> {
        let either_frame = sv2_frame.into();
        let sender = self_
            .safe_lock(|self_| self_.sender.clone())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?;
        sender.send(either_frame).await?;
        Ok(())
    }
}
