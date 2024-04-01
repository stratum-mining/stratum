pub mod message_handler;

use super::{
    error::{PoolError, PoolResult},
    mining_pool::{setup_connection::SetupConnectionHandler, EitherFrame, Pool, StdFrame},
    status,
};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use error_handling::handle_result;
use roles_logic_sv2::{
    channel_logic::channel_factory::PoolChannelFactory,
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::mining::{ParseDownstreamMiningMessages, SendTo},
    parsers::{Mining, PoolMessages},
    routing_logic::MiningRoutingLogic,
    template_distribution_sv2::SubmitSolution,
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::task;
use tracing::{debug, error, warn};

#[derive(Debug)]
pub struct Downstream {
    // Either group or channel id
    pub id: u32,
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    pub downstream_data: CommonDownstreamData,
    pub solution_sender: Sender<SubmitSolution<'static>>,
    pub channel_factory: Arc<Mutex<PoolChannelFactory>>,
}

impl IsDownstream for Downstream {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        self.downstream_data
    }
}

impl IsMiningDownstream for Downstream {}

impl Downstream {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        solution_sender: Sender<SubmitSolution<'static>>,
        pool: Arc<Mutex<Pool>>,
        channel_factory: Arc<Mutex<PoolChannelFactory>>,
        status_tx: status::Sender,
        address: SocketAddr,
    ) -> PoolResult<Arc<Mutex<Self>>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let downstream_data =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender, address)
                .await?;

        let id = match downstream_data.header_only {
            false => channel_factory.safe_lock(|c| c.new_group_id())?,
            true => channel_factory.safe_lock(|c| c.new_standard_id_for_hom())?,
        };

        let self_ = Arc::new(Mutex::new(Downstream {
            id,
            receiver,
            sender,
            downstream_data,
            solution_sender,
            channel_factory,
        }));

        let cloned = self_.clone();

        task::spawn(async move {
            debug!("Starting up downstream receiver");
            let receiver_res = cloned
                .safe_lock(|d| d.receiver.clone())
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let receiver = match receiver_res {
                Ok(recv) => recv,
                Err(e) => {
                    if let Err(e) = status_tx
                        .send(status::Status {
                            state: status::State::Healthy(format!(
                                "Downstream connection dropped: {}",
                                e
                            )),
                        })
                        .await
                    {
                        error!("Encountered Error but status channel is down: {}", e);
                    }

                    return;
                }
            };
            loop {
                match receiver.recv().await {
                    Ok(received) => {
                        let received: Result<StdFrame, _> = received
                            .try_into()
                            .map_err(|e| PoolError::Codec(codec_sv2::Error::FramingSv2Error(e)));
                        let std_frame = handle_result!(status_tx, received);
                        handle_result!(
                            status_tx,
                            Downstream::next(cloned.clone(), std_frame).await
                        );
                    }
                    _ => {
                        let res = pool
                            .safe_lock(|p| p.downstreams.remove(&id))
                            .map_err(|e| PoolError::PoisonLock(e.to_string()));
                        handle_result!(status_tx, res);
                        error!("Downstream {} disconnected", id);
                        break;
                    }
                }
            }
            warn!("Downstream connection dropped");
        });
        Ok(self_)
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) -> PoolResult<()> {
        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();
        debug!(
            "Received downstream message type: {:?}, payload: {:?}",
            message_type, payload
        );
        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            MiningRoutingLogic::None,
        );
        Self::match_send_to(self_mutex, next_message_to_send).await
    }

    #[async_recursion::async_recursion]
    pub async fn match_send_to(
        self_: Arc<Mutex<Self>>,
        send_to: Result<SendTo<()>, Error>,
    ) -> PoolResult<()> {
        match send_to {
            Ok(SendTo::Respond(message)) => {
                debug!("Sending to downstream: {:?}", message);
                // returning an error will send the error to the main thread,
                // and the main thread will drop the downstream from the pool
                if let &Mining::OpenMiningChannelError(_) = &message {
                    Self::send(self_.clone(), message.clone()).await?;
                    let downstream_id = self_
                        .safe_lock(|d| d.id)
                        .map_err(|e| Error::PoisonLock(e.to_string()))?;
                    return Err(PoolError::Sv2ProtocolError((
                        downstream_id,
                        message.clone(),
                    )));
                } else {
                    Self::send(self_, message.clone()).await?;
                }
            }
            Ok(SendTo::Multiple(messages)) => {
                debug!("Sending multiple messages to downstream");
                for message in messages {
                    debug!("Sending downstream message: {:?}", message);
                    Self::match_send_to(self_.clone(), Ok(message)).await?;
                }
            }
            Ok(SendTo::None(_)) => {}
            Ok(m) => {
                error!("Unexpected SendTo: {:?}", m);
                panic!();
            }
            Err(Error::UnexpectedMessage(_message_type)) => todo!(),
            Err(e) => {
                error!("Error: {:?}", e);
                todo!()
            }
        }
        Ok(())
    }

    async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::Mining<'static>,
    ) -> PoolResult<()> {
        //let message = if let Mining::NewExtendedMiningJob(job) = message {
        //    Mining::NewExtendedMiningJob(extended_job_to_non_segwit(job, 32)?)
        //} else {
        //    message
        //};
        let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into()?;
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone())?;
        sender.send(sv2_frame.into()).await?;
        Ok(())
    }
}
