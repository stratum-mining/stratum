//! ## Downstream Module
//!
//! It contains the logic and structures for the Job Declarator Client (JDC) to spawn a server and
//! provide ports for downstream proxies or mining nodes to connect to.
//!
//! It handles the lifecycle of downstream connections, including establishing secure
//! communication, processing incoming SV2 messages from the downstream, interpreting
//! these messages based on the SV2 Mining Protocol, and forwarding relevant messages
//! to the appropriate upstream components (either the main Pool or the Job Declarator Server).
//!
//! All the traits required for downstream connection handling, message parsing,
//! and message interpretation are implemented within this module for the `DownstreamMiningNode`
//! struct.
//!
//! Implemented Traits:
//! - [`IsDownstream`] — Provides access to common downstream data.
//! - [`ParseMiningMessagesFromDownstream<UpstreamMiningNode>`] — Handles all messages specific to
//!   the SV2 Mining Protocol received from the downstream.
//! - [`ParseCommonMessagesFromDownstream`] — Handles common SV2 messages like `SetupConnection`
//!   received during the initial handshake.
//! - [`IsMiningDownstream`] — (possibly redundant.)

use super::{config::JobDeclaratorClientConfig, template_receiver::TemplateRx, PoolChangerTrigger};

use super::{
    job_declarator::JobDeclarator,
    status::{self, State},
    upstream_sv2::Upstream as UpstreamMiningNode,
};
use async_channel::{bounded, Receiver, SendError, Sender};
use stratum_common::roles_logic_sv2::{
    self,
    bitcoin::{consensus::deserialize, Amount, TxOut},
    channel_logic::channel_factory::{OnNewShare, PoolChannelFactory, Share},
    codec_sv2,
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    errors::Error,
    handlers::{
        common::{ParseCommonMessagesFromDownstream, SendTo as SendToCommon},
        mining::{ParseMiningMessagesFromDownstream, SendTo, SupportedChannelTypes},
    },
    job_creator::JobsCreators,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, Mining, MiningDeviceMessages},
    template_distribution_sv2::{NewTemplate, SubmitSolution},
    utils::Mutex,
};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

use codec_sv2::{HandshakeRole, Responder, StandardEitherFrame, StandardSv2Frame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// Represents a connection to a single downstream mining node or proxy.
///
/// **NOTE:** The current implementation of the JDC's downstream handling is
/// limited to a one-to-one connection with a single downstream entity. This is
/// noted in the code as a conceptual mistake that needs refactoring to support
/// multiple downstream connections.
///
/// A downstream can be either a direct mining device or another proxy. It is
/// assumed that a single downstream node connects to only one upstream pool
/// at a time via this JDC.
#[derive(Debug)]
pub struct DownstreamMiningNode {
    // Receiver channel for incoming messages from the downstream node.
    receiver: Receiver<EitherFrame>,
    // Sender channel for sending messages to the downstream node.
    sender: Sender<EitherFrame>,
    /// The current status of the downstream connection, tracking its lifecycle stage.
    pub status: DownstreamMiningNodeStatus,
    // This field might be used in future for job tracking. or not sure.
    #[allow(dead_code)]
    pub prev_job_id: Option<u32>,
    // Sender channel for forwarding validated miner solutions to the template receiver
    // or other components that handle block solution submission.
    solution_sender: Sender<SubmitSolution<'static>>,
    // Needs more discussion..
    withhold: bool,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    // Sender for communicating status updates (e.g., disconnection) back to the main Status loop.
    tx_status: status::Sender,
    // The miner's configured coinbase output. Used in solo mining mode.
    miner_coinbase_output: TxOut,
    // The template ID of the last job sent to this downstream. Used to correlate
    // submitted shares with the correct job ID when sending upstream.
    last_template_id: u64,
    /// `JobDeclarator` instance. Present when connected to a pool and using the Job Declaration
    /// Protocol. Absent in solo mining mode.
    pub jd: Option<Arc<Mutex<JobDeclarator>>>,
    // The JDC's signature string, used in solo mining channel setup.
    jdc_signature: String,
}

/// Represents the different lifecycle stages of a connection with a downstream mining node or
/// proxy.
///
/// This enum tracks the state transitions for both regular downstream connections
/// (paired with an upstream pool) and solo mining downstream connections.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum DownstreamMiningNodeStatus {
    /// The downstream node is in the initial handshake/initialization phase.
    /// Holds an optional reference to the upstream if connecting to a pool.
    Initializing(Option<Arc<Mutex<UpstreamMiningNode>>>),
    /// The downstream node has completed the initial setup and is paired with an upstream pool.
    /// Holds common downstream data and a reference to the upstream.
    Paired(Arc<Mutex<UpstreamMiningNode>>),
    /// A mining channel (specifically an Extended channel in this implementation)
    /// has been successfully opened for the downstream node.
    /// Holds the `PoolChannelFactory` for this channel and a reference to the upstream.
    ChannelOpened((PoolChannelFactory, Arc<Mutex<UpstreamMiningNode>>)),
    /// The downstream node has completed initialization and is operating in solo mining mode.
    /// Holds common downstream data.
    SoloMinerPaired,
    /// The solo miner has opened a mining channel.
    /// Holds the `PoolChannelFactory` for this channel and common downstream data.
    SoloMinerChannelOpend(PoolChannelFactory),
}

impl DownstreamMiningNodeStatus {
    // Checks if the downstream connection is in a paired state (either with an upstream pool or in
    // solo mining).
    fn is_paired(&self) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing(_) => false,
            DownstreamMiningNodeStatus::Paired(_) => true,
            DownstreamMiningNodeStatus::ChannelOpened(_) => true,
            DownstreamMiningNodeStatus::SoloMinerPaired => true,
            DownstreamMiningNodeStatus::SoloMinerChannelOpend(_) => true,
        }
    }

    // Transitions the status from `Initializing` to either `Paired` (if an upstream exists)
    // or `SoloMinerPaired` (if in solo mining mode).
    fn pair(&mut self) {
        match self {
            DownstreamMiningNodeStatus::Initializing(Some(up)) => {
                let self_ = Self::Paired(up.clone());
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::Initializing(None) => {
                let self_ = Self::SoloMinerPaired;
                let _ = std::mem::replace(self, self_);
            }
            _ => panic!("Try to pair an already paired downstream"),
        }
    }

    // Sets the `PoolChannelFactory` for the downstream connection and transitions the status
    // to either `ChannelOpened` (if paired with upstream) or `SoloMinerChannelOpend`
    // (if in solo mining mode).
    fn set_channel(&mut self, channel: PoolChannelFactory) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing(_) => false,
            DownstreamMiningNodeStatus::Paired(up) => {
                let self_ = Self::ChannelOpened((channel, up.clone()));
                let _ = std::mem::replace(self, self_);
                true
            }
            DownstreamMiningNodeStatus::ChannelOpened(_) => false,
            DownstreamMiningNodeStatus::SoloMinerPaired => {
                let self_ = Self::SoloMinerChannelOpend(channel);
                let _ = std::mem::replace(self, self_);
                true
            }
            DownstreamMiningNodeStatus::SoloMinerChannelOpend(_) => false,
        }
    }

    /// Returns a mutable reference to the `PoolChannelFactory` if the downstream is in a
    /// channel-opened state (either pooled or solo mining).
    pub fn get_channel(&mut self) -> &mut PoolChannelFactory {
        match self {
            DownstreamMiningNodeStatus::Initializing(_) => panic!(),
            DownstreamMiningNodeStatus::Paired(_) => panic!(),
            DownstreamMiningNodeStatus::ChannelOpened((channel, _)) => channel,
            DownstreamMiningNodeStatus::SoloMinerPaired => panic!(),
            DownstreamMiningNodeStatus::SoloMinerChannelOpend(channel) => channel,
        }
    }

    // Checks if the downstream connection has an opened mining channel.
    fn have_channel(&self) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing(_) => false,
            DownstreamMiningNodeStatus::Paired(_) => false,
            DownstreamMiningNodeStatus::ChannelOpened(_) => true,
            DownstreamMiningNodeStatus::SoloMinerPaired => false,
            DownstreamMiningNodeStatus::SoloMinerChannelOpend(_) => true,
        }
    }

    // Returns an optional Arc-wrapped Mutex reference to the upstream mining node
    // if the downstream is connected to one.
    fn get_upstream(&mut self) -> Option<Arc<Mutex<UpstreamMiningNode>>> {
        match self {
            DownstreamMiningNodeStatus::Initializing(Some(up)) => Some(up.clone()),
            DownstreamMiningNodeStatus::Paired(up) => Some(up.clone()),
            DownstreamMiningNodeStatus::ChannelOpened((_, up)) => Some(up.clone()),
            DownstreamMiningNodeStatus::Initializing(None) => None,
            DownstreamMiningNodeStatus::SoloMinerPaired => None,
            DownstreamMiningNodeStatus::SoloMinerChannelOpend(_) => None,
        }
    }

    // Checks if the downstream connection is operating in solo mining mode.
    fn is_solo_miner(&mut self) -> bool {
        matches!(
            self,
            DownstreamMiningNodeStatus::Initializing(None)
                | DownstreamMiningNodeStatus::SoloMinerPaired
                | DownstreamMiningNodeStatus::SoloMinerChannelOpend(_)
        )
    }
}

use core::convert::TryInto;
use std::{net::IpAddr, str::FromStr, sync::Arc};

impl DownstreamMiningNode {
    /// Constructs a new `DownstreamMiningNode` instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        upstream: Option<Arc<Mutex<UpstreamMiningNode>>>,
        solution_sender: Sender<SubmitSolution<'static>>,
        withhold: bool,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        tx_status: status::Sender,
        miner_coinbase_output: TxOut,
        jd: Option<Arc<Mutex<JobDeclarator>>>,
        jdc_signature: String,
    ) -> Self {
        Self {
            receiver,
            sender,
            status: DownstreamMiningNodeStatus::Initializing(upstream),
            prev_job_id: None,
            solution_sender,
            withhold,
            task_collector,
            tx_status,
            miner_coinbase_output,
            // set it to an arbitrary value cause when we use it we always updated it.
            // Is used before sending the share to upstream in the main loop when we have a share.
            // Is upated in the message handler that si called earlier in the main loop.
            last_template_id: 0,
            jd,
            jdc_signature,
        }
    }

    /// Starts the processing of messages from the downstream mining node.
    ///
    /// This method is called after the initial `SetupConnection` handshake is complete
    /// and the downstream's status has been set to `Paired` or `SoloMinerPaired`.
    /// It sends a `SetupConnectionSuccess` message back to the downstream and then
    /// enters a loop to continuously receive and process incoming messages.
    /// If the downstream connection closes, it reports a `DownstreamShutdown` status.
    pub async fn start(
        self_mutex: &Arc<Mutex<Self>>,
        setup_connection_success: SetupConnectionSuccess,
    ) {
        // Ensure the downstream is in a paired state before starting message processing.
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

    // Sets up the `PoolChannelFactory` for this downstream connection.
    //
    // In pooled mining mode, it waits for the `OpenExtendedMiningChannelSuccess`
    // message from the upstream, which provides the necessary information to
    // take the initialized factory from the upstream instance.
    // In solo mining mode, it creates a new `PoolChannelFactory` instance locally.
    fn set_channel_factory(self_mutex: Arc<Mutex<Self>>) {
        // Check if the downstream is in solo mining mode.
        if !self_mutex.safe_lock(|s| s.status.is_solo_miner()).unwrap() {
            // Safe unwrap already checked if it contains an upstream with `is_solo_miner`
            let upstream = self_mutex
                .safe_lock(|s| s.status.get_upstream().unwrap())
                .unwrap();
            // Spawn a task to wait for and take the channel factory from the upstream.
            let recv_factory = {
                let self_mutex = self_mutex.clone();
                tokio::task::spawn(async move {
                    let factory = UpstreamMiningNode::take_channel_factory(upstream).await;
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
                        .safe_lock(|c| c.push(recv_factory.abort_handle()))
                        .unwrap()
                })
                .unwrap();
        }
    }

    /// Parses the received message from the downstream and dispatches it to the
    /// appropriate handler based on the `ParseMiningMessagesFromDownstream` trait.
    ///
    /// After processing, it calls `match_send_to` to handle the result from the handler.
    pub async fn next(self_mutex: &Arc<Mutex<Self>>, mut incoming: StdFrame) {
        // Extract message type and payload. Panics on missing header or type.
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();

        let next_message_to_send = ParseMiningMessagesFromDownstream::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
        );

        // Process the result from the message handler
        Self::match_send_to(self_mutex.clone(), next_message_to_send, Some(incoming)).await;
    }

    /// Recursively handles the `SendTo` result from a message handler.
    ///
    /// This method determines how to proceed based on the instruction received
    /// from the handler:
    /// - `RelaySameMessageToRemote`: Relays the original incoming message to the upstream.
    /// - `RelayNewMessage`: Sends a newly constructed message (e.g., a processed share) to the
    ///   upstream. Includes logic for retrieving the correct job ID for shares.
    /// - `Multiple`: Processes a vector of `SendTo` instructions recursively.
    /// - `Respond`: Sends a message as a response back to the downstream.
    /// - `None`: Indicates no immediate action is required.
    #[async_recursion::async_recursion]
    async fn match_send_to(
        self_mutex: Arc<Mutex<Self>>,
        next_message_to_send: Result<SendTo<UpstreamMiningNode>, Error>,
        incoming: Option<StdFrame>,
    ) {
        match next_message_to_send {
            // If the handler requests to relay the same message upstream.
            Ok(SendTo::RelaySameMessageToRemote(upstream_mutex)) => {
                let sv2_frame: codec_sv2::Sv2Frame<AnyMessage, buffer_sv2::Slice> =
                    incoming.unwrap().map(|payload| payload.try_into().unwrap());

                // Send the message to the upstream.
                UpstreamMiningNode::send(&upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            // If the handler requests to relay a new message, specifically SubmitSharesExtended.
            Ok(SendTo::RelayNewMessage(Mining::SubmitSharesExtended(mut share))) => {
                // This case is for pooled mining, where the JDC forwards a share.
                let upstream_mutex = self_mutex
                    .safe_lock(|s| s.status.get_upstream().unwrap())
                    .unwrap();

                // When re receive SetupConnectionSuccess we link the last_template_id with the
                // pool's job_id. The below return as soon as we have a pairable job id for the
                // template_id associated with this share.
                let last_template_id = self_mutex.safe_lock(|s| s.last_template_id).unwrap();
                let job_id_future =
                    UpstreamMiningNode::get_job_id(&upstream_mutex, last_template_id);

                // Wait for the job ID with a timeout. If it times out, don't send the share
                let job_id = match timeout(Duration::from_secs(10), job_id_future).await {
                    Ok(job_id) => job_id,
                    Err(_) => {
                        return;
                    }
                };

                // Set the job ID in the share message.
                share.job_id = job_id;
                debug!(
                    "Sending valid block solution upstream, with job_id {}",
                    job_id
                );
                let message = Mining::SubmitSharesExtended(share);
                let message: AnyMessage = AnyMessage::Mining(message);
                let sv2_frame: codec_sv2::Sv2Frame<AnyMessage, buffer_sv2::Slice> =
                    message.try_into().unwrap();

                // Send the share message to the upstream.
                UpstreamMiningNode::send(&upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            // If the handler requests to relay a *new* message.
            Ok(SendTo::RelayNewMessage(message)) => {
                let message: AnyMessage = AnyMessage::Mining(message);
                let sv2_frame: codec_sv2::Sv2Frame<AnyMessage, buffer_sv2::Slice> =
                    message.try_into().unwrap();
                let upstream_mutex = self_mutex.safe_lock(|s| s.status.get_upstream().expect("We should return RelayNewMessage only if we are not in solo mining mode")).unwrap();
                UpstreamMiningNode::send(&upstream_mutex, sv2_frame)
                    .await
                    .unwrap();
            }
            // If the handler requests to send multiple messages.
            Ok(SendTo::Multiple(messages)) => {
                // Iterate through the messages and handle each one recursively.
                for message in messages {
                    // Note: We pass None for `incoming` as these are new messages, not the
                    // original.
                    Self::match_send_to(self_mutex.clone(), Ok(message), None).await;
                }
            }
            // If the handler requests to send a response back to the downstream.
            Ok(SendTo::Respond(message)) => {
                let message = MiningDeviceMessages::Mining(message);
                let sv2_frame: codec_sv2::Sv2Frame<MiningDeviceMessages, buffer_sv2::Slice> =
                    message.try_into().unwrap();

                // Send the response message downstream.
                Self::send(&self_mutex, sv2_frame).await.unwrap();
            }
            Ok(SendTo::None(None)) => (),
            Ok(m) => unreachable!("Unexpected message type: {:?}", m),
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

    /// Handles a `NewTemplate` message received from the template receiver.
    ///
    /// This method is called directly from the template receiver when a new block
    /// template is available. It updates the internal `PoolChannelFactory` with
    /// the new template and the pool's coinbase output (if a channel is open).
    /// It then generates the appropriate mining messages (`NewExtendedMiningJob`)
    /// using the channel factory and sends them downstream to the miner.
    /// Finally, it updates the `last_template_id` and sets the `IS_NEW_TEMPLATE_HANDLED`
    /// global flag to `true` (Release ordering) to signal that the downstream
    /// has finished processing the new template.
    pub async fn on_new_template(
        self_mutex: &Arc<Mutex<Self>>,
        mut new_template: NewTemplate<'static>,
        pool_outputs: &[u8],
    ) -> Result<(), Error> {
        // Check if a channel is open. If not, just set the flag and return.
        if !self_mutex.safe_lock(|s| s.status.have_channel()).unwrap() {
            super::IS_NEW_TEMPLATE_HANDLED.store(true, std::sync::atomic::Ordering::Release);
            return Ok(());
        }

        let mut deserialized_outputs: Vec<TxOut> = deserialize(pool_outputs).unwrap();

        // we know the first output is where the template revenue must
        // be allocated
        deserialized_outputs[0].value = Amount::from_sat(new_template.coinbase_tx_value_remaining);

        // Update the channel factory with the new template and pool outputs and get messages to
        // send downstream.
        let to_send = self_mutex
            .safe_lock(|s| {
                let channel = s.status.get_channel();
                channel.update_pool_outputs(deserialized_outputs);
                channel.on_new_template(&mut new_template)
            })
            .unwrap()?;

        // to_send is a HashMap<channel_id, messages_to_send>. Since this implementation
        // currently only supports one downstream connection and one channel, we can
        // take all messages from the map's values.
        let to_send = to_send.into_values();

        // Send each generated message downstream.
        for message in to_send {
            // If the message is NewExtendedMiningJob, update the JobDeclarator's coinbase prefix
            // and suffix.
            let message = if let Mining::NewExtendedMiningJob(job) = message {
                if let Some(jd) = self_mutex.safe_lock(|s| s.jd.clone()).unwrap() {
                    jd.safe_lock(|jd| {
                        jd.coinbase_tx_prefix = job.coinbase_tx_prefix.clone();
                        jd.coinbase_tx_suffix = job.coinbase_tx_suffix.clone();
                    })
                    .unwrap();
                }
                Mining::NewExtendedMiningJob(job)
            } else {
                message
            };
            let message = MiningDeviceMessages::Mining(message);
            let frame: StdFrame = message.try_into().unwrap();
            Self::send(self_mutex, frame).await.unwrap();
        }

        // Set the global flag to true to indicate that the downstream
        // has finished handling the NewTemplate message.
        super::IS_NEW_TEMPLATE_HANDLED.store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Handles a `SetNewPrevHash` message received from the template receiver.
    ///
    /// This method is called directly from the template receiver when a new
    /// previous block hash is available. It updates the internal `PoolChannelFactory`
    /// with the new previous hash (if a channel is open), which is necessary for
    /// the factory to generate correct job IDs. It then generates the appropriate
    /// `SetNewPrevHash` message for the downstream miner and sends it.
    pub async fn on_set_new_prev_hash(
        self_mutex: &Arc<Mutex<Self>>,
        new_prev_hash: roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
    ) -> Result<(), Error> {
        // Check if a channel is open. If not, return immediately.
        if !self_mutex.safe_lock(|s| s.status.have_channel()).unwrap() {
            return Ok(());
        }

        // Update the channel factory with the new previous hash and get the corresponding job ID.
        let job_id = self_mutex
            .safe_lock(|s| {
                let channel = s.status.get_channel();
                channel.on_new_prev_hash_from_tp(&new_prev_hash)
            })
            .unwrap()?;

        // Get the extended channel IDs from the factory. Expect exactly one in this 1:1 setup.
        let channel_ids = self_mutex
            .safe_lock(|s| s.status.get_channel().get_extended_channels_ids())
            .unwrap();

        // Determine the channel ID. Panics if the number of channels is not 1.
        let channel_id = match channel_ids.len() {
            1 => channel_ids[0],
            _ => unreachable!(),
        };

        // Construct the SetNewPrevHash message for the downstream miner.
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

/// It impl UpstreamMining cause the proxy act as an upstream node for the DownstreamMiningNode
impl ParseMiningMessagesFromDownstream<UpstreamMiningNode> for DownstreamMiningNode {
    // Returns the channel type supported between the downstream mining node and
    // this JDC instance. Only `Extended` channels are supported.
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    // Indicates whether work selection is enabled for this connection.
    // In this JDC implementation acting as an upstream for the downstream, work
    // selection is handled upstream (by the pool or JDC logic), not by the downstream.
    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    // Checks if a downstream user identity is authorized to connect.
    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &Str0255,
    ) -> Result<bool, Error> {
        Ok(true)
    }

    // Handles an `OpenStandardMiningChannel` message received from the downstream.
    //
    // This method logs a warning and ignores the message because the JDC
    // is configured to only support `Extended` mining channels with downstream nodes.
    //
    // Returns `Ok(SendTo::None(None))` indicating no action is taken.
    fn handle_open_standard_mining_channel(
        &mut self,
        _: OpenStandardMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        warn!("Ignoring OpenStandardMiningChannel");
        Ok(SendTo::None(None))
    }

    // Handles an `OpenExtendedMiningChannel` message received from the downstream.
    //
    // This is the expected message from a downstream miner to open a mining channel.
    // - If the JDC is connected to a pool, it relays this message to the upstream pool to request
    //   an extended channel there.
    // - If the JDC is in solo mining mode, it creates a local `PoolChannelFactory` for this channel
    //   and responds with `OpenExtendedMiningChannelSuccess`.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(upstream_mutex))` if in pooled mining mode.
    // Returns `Ok(SendTo::Multiple(messages))` if in solo mining mode, containing
    // the `OpenExtendedMiningChannelSuccess` and potentially other initial messages.
    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!(
            "Received OpenExtendedMiningChannel from: {} with id: {}",
            std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
            m.get_request_id_as_u32()
        );
        debug!("OpenExtendedMiningChannel: {}", m);

        // Check if the downstream is in solo mining mode.
        if !self.status.is_solo_miner() {
            // If not solo mining, it's pooled mining. Relay the message upstream.
            // Safe unwrap: is_solo_miner being false implies there's an upstream.
            Ok(SendTo::RelaySameMessageToRemote(
                self.status.get_upstream().unwrap(),
            ))
        } else {
            // If in solo mining mode, create a local channel factory.
            // The channel factory is created here to ensure it exists when a channel is opened.
            // hardcoded value
            let extranonce_len = 32;
            let jdc_signature_len = self.jdc_signature.len();
            let range_0 = std::ops::Range { start: 0, end: 0 };

            // JDC only allows for one single downstream, so we don't need any free bytes on range_1
            // we just allocate enough space for the JDC signature
            let range_1 = std::ops::Range {
                start: 0,
                end: jdc_signature_len,
            };
            let range_2 = std::ops::Range {
                start: jdc_signature_len,
                end: extranonce_len,
            };
            let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
            let coinbase_output = self.miner_coinbase_output.clone();

            // Create the ExtendedExtranonce structure.
            let extranonces = ExtendedExtranonce::new(
                range_0,
                range_1,
                range_2,
                Some(self.jdc_signature.as_bytes().to_vec()),
            )
            .map_err(|_| {
                roles_logic_sv2::Error::ExtendedExtranonceCreationFailed(
                    "Failed to create ExtendedExtranonce".into(),
                )
            })?;
            let creator = JobsCreators::new(extranonce_len as u8);
            // hardcoded value
            let share_per_min = 1.0;
            let kind = roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::Pool;

            // Create the PoolChannelFactory instance for solo mining.
            let channel_factory = PoolChannelFactory::new(
                ids,
                extranonces,
                creator,
                share_per_min,
                kind,
                vec![coinbase_output],
            );

            // Set the created channel factory in the downstream's status.
            self.status.set_channel(channel_factory);

            // Process the new extended channel request in the locally created factory.
            let request_id = m.request_id;
            let hash_rate = m.nominal_hash_rate;
            let min_extranonce_size = m.min_extranonce_size;
            let messages_res = self.status.get_channel().new_extended_channel(
                request_id,
                hash_rate,
                min_extranonce_size,
            );

            // Based on the factory's response, generate messages to send back to the downstream.
            match messages_res {
                Ok(messages) => {
                    let messages = messages.into_iter().map(SendTo::Respond).collect();
                    Ok(SendTo::Multiple(messages))
                }
                Err(_) => Err(roles_logic_sv2::Error::ChannelIsNeitherExtendedNeitherInAPool),
            }
        }
    }

    // Handles an `UpdateChannel` message received from the downstream.
    //
    // - If in pooled mining mode, it relays this message upstream to the pool.
    // - If in solo mining mode, it updates the maximum target in the local `PoolChannelFactory`
    //   based on the nominal hash rate provided by the miner and responds with a `SetTarget`
    //   message to the downstream.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(upstream_mutex))` if in pooled mining mode.
    // Returns `Ok(SendTo::Respond(Mining::SetTarget(set_target)))` if in solo mining mode.
    fn handle_update_channel(
        &mut self,
        m: UpdateChannel,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!("Received UpdateChannel message");

        // Check if the downstream is in solo mining mode.
        if !self.status.is_solo_miner() {
            // If not solo mining, relay the message upstream.
            // Safe unwrap: is_solo_miner being false implies there's an upstream
            Ok(SendTo::RelaySameMessageToRemote(
                self.status.get_upstream().unwrap(),
            ))
        } else {
            // If in solo mining mode, calculate the target based on the miner's hash rate.
            let maximum_target =
                roles_logic_sv2::utils::hash_rate_to_target(m.nominal_hash_rate.into(), 10.0)?;

            // Update the target in the local channel factory.
            self.status
                .get_channel()
                .update_target_for_channel(m.channel_id, maximum_target.clone().into());

            // Construct the SetTarget message for the downstream.
            let set_target = SetTarget {
                channel_id: m.channel_id,
                maximum_target,
            };
            Ok(SendTo::Respond(Mining::SetTarget(set_target)))
        }
    }

    // Handles a `SubmitSharesStandard` message received from the downstream.
    //
    // Returns `Ok(SendTo::None(None))` indicating no action is taken.
    fn handle_submit_shares_standard(
        &mut self,
        _: SubmitSharesStandard,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        warn!("Ignoring SubmitSharesStandard");
        Ok(SendTo::None(None))
    }

    /// Handles a `SubmitSharesExtended` message received from the downstream.
    ///
    /// This method processes a submitted share from the miner using the internal
    /// `PoolChannelFactory` to validate it against the current job's target.
    /// - If the share meets the downstream target, it is processed further.
    /// - If the share meets the Bitcoin target (in solo mining), it's a potential block.
    /// - If the share meets the upstream pool's target (in pooled mining and withholding is off),
    ///   it is relayed upstream.
    /// - If the share is invalid, an error response is sent downstream.
    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        info!("Received SubmitSharesExtended message");
        debug!("SubmitSharesExtended {}", m);

        // Process the submitted share using the channel factory.
        match self
            .status
            .get_channel()
            .on_submit_shares_extended(m.clone())
            .unwrap()
        {
            // If the share does not meet the downstream target.
            OnNewShare::SendErrorDownstream(s) => {
                error!("Share does not meet the downstream target");
                Ok(SendTo::Respond(Mining::SubmitSharesError(s)))
            }
            // If the share is valid and should be sent upstream (pooled mining).
            OnNewShare::SendSubmitShareUpstream((m, Some(template_id))) => {
                if !self.status.is_solo_miner() {
                    match m {
                        Share::Extended(share) => {
                            // Update the last_template_id for correlating with upstream job ID.
                            let for_upstream = Mining::SubmitSharesExtended(share);
                            self.last_template_id = template_id;
                            Ok(SendTo::RelayNewMessage(for_upstream))
                        }
                        // We are in an extended channel shares are extended
                        Share::Standard(_) => unreachable!(),
                    }
                } else {
                    // This case should not happen in solo mining, as shares meeting Bitcoin target
                    // are handled separately.
                    Ok(SendTo::None(None))
                }
            }
            OnNewShare::RelaySubmitShareUpstream => unreachable!(),
            // If the share meets the Bitcoin target (potential block found).
            OnNewShare::ShareMeetBitcoinTarget((
                share,
                Some(template_id),
                coinbase,
                extranonce,
            )) => {
                match share {
                    Share::Extended(share) => {
                        // Get the solution sender channel
                        let solution_sender = self.solution_sender.clone();

                        // Construct the SubmitSolution message for the template receiver.
                        let solution = SubmitSolution {
                            template_id,
                            version: share.version,
                            header_timestamp: share.ntime,
                            header_nonce: share.nonce,
                            coinbase_tx: coinbase.try_into()?,
                        };

                        // Send the solution to the solution sender. Blocking send is used,
                        // expecting the channel to not be full. Panics on send failure.
                        solution_sender.send_blocking(solution).unwrap();

                        // If not in solo mining mode, send the solution to the Job Declarator
                        // to potentially push it as a block candidate to the JDS.
                        if !self.status.is_solo_miner() {
                            {
                                let jd = self.jd.clone();
                                let mut share = share.clone();
                                // Update the share's extranonce with the full calculated
                                // extranonce.
                                share.extranonce = extranonce.try_into().unwrap();
                                // Spawn a task to send the solution to the Job Declarator.
                                tokio::task::spawn(async move {
                                    JobDeclarator::on_solution(&jd.unwrap(), share).await
                                });
                            }
                        }

                        // If not withholding and not in solo mining mode, relay the share upstream.
                        // This is likely for block propagation to the pool.
                        // Safe unwrap: is_solo_miner being false implies there's an upstream.
                        if !self.withhold && !self.status.is_solo_miner() {
                            self.last_template_id = template_id;
                            let for_upstream = Mining::SubmitSharesExtended(share);
                            Ok(SendTo::RelayNewMessage(for_upstream))
                        } else {
                            // If withholding or in solo mining, no action is needed upstream.
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
            OnNewShare::SendSubmitShareUpstream(_) => unreachable!(),
            OnNewShare::ShareMeetDownstreamTarget => Ok(SendTo::None(None)),
        }
    }

    /// Handles a `SetCustomMiningJob` message received from the downstream.
    ///
    /// Returns `Ok(SendTo::None(None))` indicating no action is taken.
    fn handle_set_custom_mining_job(
        &mut self,
        _: SetCustomMiningJob,
    ) -> Result<SendTo<UpstreamMiningNode>, Error> {
        warn!("Ignoring SetCustomMiningJob");
        Ok(SendTo::None(None))
    }
}

impl ParseCommonMessagesFromDownstream for DownstreamMiningNode {
    /// Handles a `SetupConnection` message received from the downstream.
    ///
    /// Returns `Ok(SendToCommon::Respond(response.into()))` indicating that a
    /// `SetupConnectionSuccess` message should be sent back to the downstream
    fn handle_setup_connection(
        &mut self,
        m: SetupConnection,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            m.min_version, m.flags
        );
        let response = SetupConnectionSuccess {
            used_version: 2,
            // require extended channels
            flags: 0b0000_0000_0000_0010,
        };
        self.status.pair();
        Ok(SendToCommon::Respond(response.into()))
    }
}

use std::net::SocketAddr;
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::codec_sv2::binary_sv2::Str0255,
};
use tokio::{
    net::TcpListener,
    task::AbortHandle,
    time::{timeout, Duration},
};

/// Starts listening for incoming downstream mining node connections on the specified address.
///
/// This function sets up a TCP listener and continuously accepts incoming connections.
/// For each accepted connection, it performs a Noise handshake,
/// initializes a `DownstreamMiningNode` instance, and spawns an task to handle
/// that specific downstream connection's lifecycle and message processing.
///
/// NOTE: The current implementation is explicitly designed to handle only one downstream
/// connection at a time. If a second connection attempt is made while one is active,
/// it will be ignored and logged. This limitation needs refactoring to properly manage
/// the state and resources of multiple downstream nodes concurrently for a production environment.
///
/// FIX ME: There is a noted issue where the downstream connection is established
/// and fully processed (including the initial handshake and starting its main loop)
/// *before* the connection with the Template Provider is initiated. This order can lead
/// to the downstream miner receiving jobs before the JDC has received template information.
/// The connection order should either be mutually exclusive or carefully managed to ensure
/// template information is available before providing jobs to the downstream.
#[allow(clippy::too_many_arguments)]
pub async fn listen_for_downstream_mining(
    address: SocketAddr,
    upstream: Option<Arc<Mutex<UpstreamMiningNode>>>,
    withhold: bool,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    tx_status: async_channel::Sender<status::Status<'static>>,
    miner_coinbase_output: TxOut,
    jd: Option<Arc<Mutex<JobDeclarator>>>,
    config: JobDeclaratorClientConfig,
    shutdown: Arc<Notify>,
    jdc_signature: String,
) {
    info!("Listening for downstream mining connections on {}", address);

    // Bind to the listener address.
    let listener = TcpListener::bind(address).await.unwrap();
    let mut has_downstream = false;

    // Loop indefinitely to accept incoming connections or handle shutdown.
    loop {
        tokio::select! {
            // Handle shutdown signal.
            _ = shutdown.notified() => {
                info!("Shutdown signal received. Stopping downstream mining listener.");
                break;
            }
            // Accept an incoming TCP connection.
            Ok((stream, _)) = listener.accept() => {
                // Check if a downstream connection is already active.
                if has_downstream {
                    error!("A downstream connection is already active. Ignoring additional connections.");
                    continue;
                }
                has_downstream = true;
                let task_collector = task_collector.clone();
                let miner_coinbase_output = miner_coinbase_output.clone();
                let jd = jd.clone();
                let upstream = upstream.clone();
                let timeout = config.timeout();
                let mut parts = config.tp_address().split(':');
                let ip_tp = parts.next().unwrap().to_string();
                let port_tp = parts.next().unwrap().parse::<u16>().unwrap();

                // Create a channel for sending miner solutions from this downstream to the template receiver.
                let (send_solution, recv_solution) = bounded(10);

                let responder = Responder::from_authority_kp(
                    &authority_public_key.into_bytes(),
                    &authority_secret_key.into_bytes(),
                    std::time::Duration::from_secs(cert_validity_sec),
                )
                .unwrap();
                let (receiver, sender) =
                    Connection::new(stream, HandshakeRole::Responder(responder))
                        .await
                        .expect("impossible to connect");

                let tx_status_downstream = status::Sender::Downstream(tx_status.clone());

                // Create a new DownstreamMiningNode instance for this connection.
                let node = DownstreamMiningNode::new(
                    receiver,
                    sender,
                    upstream.clone(),
                    send_solution,
                    withhold,
                    task_collector.clone(),
                    tx_status_downstream,
                    miner_coinbase_output,
                    jd.clone(),
                    jdc_signature.clone(),
                );

                // The first message from the downstream should be SetupConnection.
                // Receive and attempt to parse this initial message.
                let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
                let message_type = incoming.get_header().unwrap().msg_type();
                let payload = incoming.payload();
                let node = Arc::new(Mutex::new(node));

                // If connected to an upstream pool, set this downstream as the upstream's downstream.
                if let Some(upstream) = upstream {
                    upstream
                        .safe_lock(|s| s.downstream = Some(node.clone()))
                        .unwrap();
                }

                if let Ok(SendToCommon::Respond(message)) = DownstreamMiningNode::handle_message_common(
                    node.clone(),
                    message_type,
                    payload,
                ) {
                    let message = match message {
                        roles_logic_sv2::parsers_sv2::CommonMessages::SetupConnectionSuccess(m) => m,
                        _ => panic!(),
                    };

                    // Spawn a task to start the main message processing loop for this downstream.
                    let main_task = tokio::task::spawn({
                        let node = node.clone();
                        async move {
                            DownstreamMiningNode::start(&node, message).await;
                        }
                    });

                    node.safe_lock(|n| {
                        n.task_collector
                            .safe_lock(|c| {
                                c.push(main_task.abort_handle());
                            })
                            .unwrap()
                    })
                    .unwrap();


                    TemplateRx::connect(
                        SocketAddr::new(IpAddr::from_str(ip_tp.as_str()).unwrap(), port_tp),
                        recv_solution,
                        status::Sender::TemplateReceiver(tx_status.clone()),
                        jd,
                        node,
                        task_collector,
                        Arc::new(Mutex::new(PoolChangerTrigger::new(timeout))),
                        vec![],
                        config.tp_authority_public_key().cloned(),
                    )
                    .await;
                }
            }
        }
    }

    info!("Downstream mining listener has shut down.");
}
