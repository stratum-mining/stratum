use crate::{
    status::{self, State},
    upstream_sv2::Upstream as UpstreamMiningNode,
};
use async_channel::{Receiver, SendError, Sender};
use roles_logic_sv2::{
    bitcoin::TxOut,
    channel_logic::channel_factory::{OnNewShare, PoolChannelFactory, Share},
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::{
        common::{ParseDownstreamCommonMessages, SendTo as SendToCommon},
        job_declaration::SendTo as SendToJD,
        mining::{ParseDownstreamMiningMessages, SendTo, SupportedChannelTypes},
    },
    job_creator::Decodable,
    mining_sv2::*,
    parsers::{JobDeclaration, Mining, MiningDeviceMessages, PoolMessages},
    template_distribution_sv2::{NewTemplate, SubmitSolution},
    utils::Mutex,
};
use tracing::info;

use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    Frame, HandshakeRole, Responder, StandardEitherFrame, StandardSv2Frame,
};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// 1 to 1 connection with a downstream node that implement the mining (sub)protocol can be either
/// a mining device or a downstream proxy.
/// A downstream can only be linked with an upstream at a time. Support multi upstrems for
/// downstream do no make much sense.
#[derive(Debug)]
pub struct DownstreamMiningNode {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    pub status: DownstreamMiningNodeStatus,
    pub prev_job_id: Option<u32>,
    upstream: Arc<Mutex<UpstreamMiningNode>>,
    recv_channel_factory: Receiver<PoolChannelFactory>,
    solution_sender: Sender<SubmitSolution<'static>>,
    withhold: bool,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    tx_status: status::Sender,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum DownstreamMiningNodeStatus {
    Initializing,
    Paired(CommonDownstreamData),
    ChannelOpened((PoolChannelFactory, CommonDownstreamData)),
}

impl DownstreamMiningNodeStatus {
    fn is_paired(&self) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing => false,
            DownstreamMiningNodeStatus::Paired(_) => true,
            DownstreamMiningNodeStatus::ChannelOpened(_) => true,
        }
    }

    fn pair(&mut self, data: CommonDownstreamData) {
        match self {
            DownstreamMiningNodeStatus::Initializing => {
                let self_ = Self::Paired(data);
                let _ = std::mem::replace(self, self_);
            }
            _ => panic!("Try to pair an already paired downstream"),
        }
    }

    fn set_channel(&mut self, channel: PoolChannelFactory) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing => false,
            DownstreamMiningNodeStatus::Paired(data) => {
                let self_ = Self::ChannelOpened((channel, *data));
                let _ = std::mem::replace(self, self_);
                true
            }
            DownstreamMiningNodeStatus::ChannelOpened(_) => false,
        }
    }

    fn get_channel(&mut self) -> &mut PoolChannelFactory {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(_) => panic!(),
            DownstreamMiningNodeStatus::ChannelOpened((channel, _)) => channel,
        }
    }
}

use core::convert::TryInto;
use std::sync::Arc;

impl DownstreamMiningNode {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        upstream: Arc<Mutex<UpstreamMiningNode>>,
        recv_channel_factory: Receiver<PoolChannelFactory>,
        solution_sender: Sender<SubmitSolution<'static>>,
        withhold: bool,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        tx_status: status::Sender,
    ) -> Self {
        Self {
            receiver,
            sender,
            status: DownstreamMiningNodeStatus::Initializing,
            prev_job_id: None,
            upstream,
            recv_channel_factory,
            solution_sender,
            withhold,
            task_collector,
            tx_status,
        }
    }

    /// Send SetupConnectionSuccess to donwstream and start processing new messages coming from
    /// downstream
    pub async fn start(
        self_mutex: &Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
    ) {
        if self_mutex
            .safe_lock(|self_| self_.status.is_paired())
            .unwrap()
        {
            let setup_connection_success: MiningDeviceMessages = setup_connection_success.into();

            {
                DownstreamMiningNode::send(
                    self_mutex,
                    setup_connection_success.try_into().unwrap(),
                )
                .await
                .unwrap();
            }
            let receiver = self_mutex
                .safe_lock(|self_| self_.receiver.clone())
                .unwrap();
            Self::set_channel_factory(self_mutex.clone());

            while let Ok(message) = receiver.recv().await {
                let incoming: StdFrame = message.try_into().unwrap();
                Self::next(self_mutex, incoming).await;
            }
            let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
            let err = Error::DownstreamDown;
            let status = status::Status {
                state: State::DownstreamShutdown(err.into()),
            };
            tx_status.send(status).await.unwrap();
        } else {
            panic!()
        }
    }

    // Whenever we have a channel factory avaibale in self.recv_channel_factory we set the status
    // to ChannelOpened and we exit
    fn set_channel_factory(self_mutex: Arc<Mutex<Self>>) {
        let recv_channel_factory = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                let receiver = self_mutex
                    .safe_lock(|s| s.recv_channel_factory.clone())
                    .unwrap();
                let factory = receiver.recv().await.unwrap();
                self_mutex
                    .safe_lock(|s| {
                        s.status.set_channel(factory);
                    })
                    .unwrap();
            })
        };
        self_mutex
            .safe_lock(|s| {
                s.task_collector
                    .safe_lock(|c| c.push(recv_channel_factory.abort_handle()))
                    .unwrap()
            })
            .unwrap();
    }

    /// Parse the received message and relay it to the right upstream
    pub async fn next(self_mutex: &Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let routing_logic = roles_logic_sv2::routing_logic::MiningRoutingLogic::None;

        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            routing_logic,
        );

        match next_message_to_send {
            Ok(SendTo::RelaySameMessageToRemote(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<PoolMessages, buffer_sv2::Slice> =
                    incoming.map(|payload| payload.try_into().unwrap());
                UpstreamMiningNode::send(&upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::RelayNewMessage(message)) => {
                let message: PoolMessages = PoolMessages::Mining(message);
                let sv2_frame: codec_sv2::Sv2Frame<PoolMessages, buffer_sv2::Slice> =
                    message.try_into().unwrap();
                let upstream_mutex = self_mutex.safe_lock(|s| s.upstream.clone()).unwrap();
                UpstreamMiningNode::send(&upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            Ok(SendTo::None(None)) => (),
            Ok(_) => unreachable!(),
            Err(_) => todo!(),
        }
    }

    /// Send a message downstream
    pub async fn send(
        self_mutex: &Arc<Mutex<Self>>,
        sv2_frame: StdFrame,
    ) -> Result<(), SendError<StdFrame>> {
        let either_frame = sv2_frame.into();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(_) => {
                todo!()
            }
        }
    }

    pub async fn on_new_template(
        self_mutex: &Arc<Mutex<Self>>,
        mut new_template: NewTemplate<'static>,
        pool_output: &[u8],
    ) -> Result<(), Error> {
        let pool_output =
            TxOut::consensus_decode(&mut pool_output.clone()).expect("Upstream sent an invalid coinbase");
        let to_send = self_mutex
            .safe_lock(|s| {
                let channel = s.status.get_channel();
                channel.update_pool_outputs(vec![pool_output]);
                channel.on_new_template(&mut new_template)
            })
            .unwrap()?;
        // to_send is HashMap<channel_id, messages_to_send> but here we have only one downstream so
        // only one channel opened downstream. That means that we can take all the messages in the
        // map and send them downstream.
        let to_send = to_send.into_values();
        for message in to_send {
            //let message = if let Mining::NewExtendedMiningJob(job) = message {
            //    Mining::NewExtendedMiningJob(extended_job_to_non_segwit(job, 32)?)
            //} else {
            //    message
            //};
            let message = MiningDeviceMessages::Mining(message);
            let frame: StdFrame = message.try_into().unwrap();
            Self::send(self_mutex, frame).await.unwrap();
        }
        crate::IS_NEW_TEMPLATE_HANDLED.store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    pub async fn on_set_new_prev_hash(
        self_mutex: &Arc<Mutex<Self>>,
        new_prev_hash: roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
    ) -> Result<(), Error> {
        let job_id = self_mutex
            .safe_lock(|s| {
                let channel = s.status.get_channel();
                channel.on_new_prev_hash_from_tp(&new_prev_hash)
            })
            .unwrap()?;
        let channel_ids = self_mutex
            .safe_lock(|s| s.status.get_channel().get_extended_channels_ids())
            .unwrap();
        let channel_id = match channel_ids.len() {
            1 => channel_ids[0],
            _ => unreachable!(),
        };
        let to_send = SetNewPrevHash {
            channel_id,
            job_id,
            prev_hash: new_prev_hash.prev_hash,
            min_ntime: new_prev_hash.header_timestamp,
            nbits: new_prev_hash.n_bits,
        };
        let message = MiningDeviceMessages::Mining(Mining::SetNewPrevHash(to_send));
        let frame = message.try_into().unwrap();
        Self::send(self_mutex, frame).await.unwrap();
        Ok(())
    }
}

use roles_logic_sv2::selectors::NullDownstreamMiningSelector;

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl
    ParseDownstreamMiningMessages<
        UpstreamMiningNode,
        NullDownstreamMiningSelector,
        roles_logic_sv2::routing_logic::NoRouting,
    > for DownstreamMiningNode
{
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        _: OpenStandardMiningChannel,
        _: Option<Arc<Mutex<UpstreamMiningNode>>>,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!("Ignoring OpenStandardMiningChannel");
        Ok(SendTo::None(None))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::RelaySameMessageToRemote(self.upstream.clone()))
    }

    fn handle_update_channel(
        &mut self,
        _: UpdateChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        Ok(SendTo::RelaySameMessageToRemote(self.upstream.clone()))
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: SubmitSharesStandard,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!("Ignoring SubmitSharesStandard");
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        match self
            .status
            .get_channel()
            .on_submit_shares_extended(m.clone())
            .unwrap()
        {
            OnNewShare::SendErrorDownstream(_) => todo!(),
            OnNewShare::SendSubmitShareUpstream(m) => {
                let new_id = self.upstream.safe_lock(|s| s.last_job_id).unwrap();
                match m {
                    Share::Extended(mut share) => {
                        share.job_id = new_id;
                        let for_upstream = Mining::SubmitSharesExtended(share);
                        Ok(SendTo::RelayNewMessage(for_upstream))
                    }
                    // We are in an extended channel shares are extended
                    Share::Standard(_) => unreachable!(),
                }
            }
            OnNewShare::RelaySubmitShareUpstream => unreachable!(),
            OnNewShare::ShareMeetBitcoinTarget((share, Some(template_id), coinbase)) => {
                match share {
                    Share::Extended(mut share) => {
                        info!("SHARE MEETS BITCOIN TARGET");
                        // send found share to JD and pool
                        let for_jd_server = JobDeclaration::SubmitSharesExtended(share.clone());
                        #[allow(clippy::no_effect)]
                        SendToJD::RelayNewMessage(for_jd_server);
                        let solution_sender = self.solution_sender.clone();
                        let solution = SubmitSolution {
                            template_id,
                            version: share.version,
                            header_timestamp: share.ntime,
                            header_nonce: share.nonce,
                            coinbase_tx: coinbase.try_into()?,
                        };
                        // The below channel should never be full is ok to block
                        solution_sender.send_blocking(solution).unwrap();

                        if !self.withhold {
                            let new_id = self.upstream.safe_lock(|s| s.last_job_id).unwrap();
                            share.job_id = new_id;
                            let for_upstream = Mining::SubmitSharesExtended(share);
                            Ok(SendTo::RelayNewMessage(for_upstream))
                        } else {
                            Ok(SendTo::None(None))
                        }
                    }
                    // We are in an extended channel shares are extended
                    Share::Standard(_) => unreachable!(),
                }
            }
            // When we have a ShareMeetBitcoinTarget it means that the proxy know the bitcoin
            // target that means that the proxy must have JD capabilities that means that the
            // second tuple elements can not be None but must be Some(template_id)
            OnNewShare::ShareMeetBitcoinTarget(_) => unreachable!(),
            OnNewShare::ShareMeetDownstreamTarget => Ok(SendTo::None(None)),
        }
    }

    fn handle_set_custom_mining_job(
        &mut self,
        _: SetCustomMiningJob,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!("Ignoring SetCustomMiningJob");
        Ok(SendTo::None(None))
    }
}

impl ParseDownstreamCommonMessages<roles_logic_sv2::routing_logic::NoRouting>
    for DownstreamMiningNode
{
    fn handle_setup_connection(
        &mut self,
        _: SetupConnection,
        _: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        let response = SetupConnectionSuccess {
            used_version: 2,
            // require extended channels
            flags: 0b0000_0000_0000_0010,
        };
        let data = CommonDownstreamData {
            header_only: false,
            work_selection: false,
            version_rolling: true,
        };
        self.status.pair(data);
        Ok(SendToCommon::Respond(response.try_into().unwrap()))
    }
}

use network_helpers::noise_connection_tokio::Connection;
use std::net::SocketAddr;
use tokio::{net::TcpListener, task::AbortHandle};

/// Strat listen for downstream mining node. Return as soon as one downstream connect.
#[allow(clippy::too_many_arguments)]
pub async fn listen_for_downstream_mining(
    address: SocketAddr,
    upstream: &Arc<Mutex<UpstreamMiningNode>>,
    recv_channel_factory: Receiver<PoolChannelFactory>,
    solution_sender: Sender<SubmitSolution<'static>>,
    withhold: bool,
    authority_public_key: EncodedEd25519PublicKey,
    authority_secret_key: EncodedEd25519SecretKey,
    cert_validity_sec: u64,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    tx_status: status::Sender,
) -> Result<Arc<Mutex<DownstreamMiningNode>>, Error> {
    info!("Listening for downstream mining connections on {}", address);
    let listner = TcpListener::bind(address).await.unwrap();

    if let Ok((stream, _)) = listner.accept().await {
        let responder = Responder::from_authority_kp(
            authority_public_key.clone().into_inner().as_bytes(),
            authority_secret_key.clone().into_inner().as_bytes(),
            std::time::Duration::from_secs(cert_validity_sec),
        )
        .unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Responder(responder)).await;
        let node = DownstreamMiningNode::new(
            receiver,
            sender,
            upstream.clone(),
            recv_channel_factory,
            solution_sender,
            withhold,
            task_collector,
            tx_status,
        );

        let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let routing_logic = roles_logic_sv2::routing_logic::CommonRoutingLogic::None;
        let node = Arc::new(Mutex::new(node));
        upstream
            .safe_lock(|s| s.downstream = Some(node.clone()))
            .unwrap();

        // Call handle_setup_connection or fail
        match DownstreamMiningNode::handle_message_common(
            node.clone(),
            message_type,
            payload,
            routing_logic,
        ) {
            Ok(SendToCommon::Respond(message)) => {
                let message = match message {
                    roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m) => m,
                    _ => panic!(),
                };
                let main_task = tokio::task::spawn({
                    let node = node.clone();
                    async move {
                        DownstreamMiningNode::start(&node, message).await;
                    }
                });
                node.safe_lock(|n| {
                    n.task_collector
                        .safe_lock(|c| c.push(main_task.abort_handle()))
                        .unwrap()
                })
                .unwrap();
                Ok(node)
            }
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    } else {
        todo!()
    }
}

impl IsDownstream for DownstreamMiningNode {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        match self.status {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired(data) => data,
            DownstreamMiningNodeStatus::ChannelOpened((_, data)) => data,
        }
    }
}
impl IsMiningDownstream for DownstreamMiningNode {}
