//! ## Upstream Module
//!
//! The `Upstream` module provides the necessary constructs and methods for establishing and
//! managing connections from the Job Declarator Client (JDC) to upstream pools, as well as handling
//! related message processing.
//!
//! It includes trait implementations required for representing an upstream connection,
//! intercepting messages from upstream nodes (pools), and generating appropriate responses.
//!
//! This module acts as the client-side implementation for communicating with a pool
//! that supports the SV2 Mining Protocol.
//!
//! Trait implementations within this module:
//! - [`IsUpstream`]: Represents a generic upstream connection (possibly redundant in this specific
//!   structure).
//! - [`IsMiningUpstream`]: Represents an upstream connection specifically for the Mining Protocol
//!   (possibly redundant).
//! - [`ParseCommonMessagesFromUpstream`]: Handles the interpretation of common SV2 messages like
//!   `SetupConnectionSuccess` and `SetupConnectionError` received from the upstream pool during the
//!   initial handshake.
//! - [`ParseMiningMessagesFromUpstream<Downstream>`]: Processes messages specific to the SV2 Mining
//!   Protocol received from the upstream pool and determines how to handle them, potentially
//!   relaying them to a downstream mining node or updating internal state.

use super::super::downstream::DownstreamMiningNode as Downstream;

use super::super::{
    error::{
        Error::{CodecNoise, PoisonLock, UpstreamIncoming},
        ProxyResult,
    },
    status,
    upstream_sv2::{EitherFrame, Message, StdFrame},
    PoolChangerTrigger,
};
use async_channel::{Receiver, Sender};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    thread::sleep,
    time::Duration,
};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2,
    roles_logic_sv2::{
        channel_logic::channel_factory::PoolChannelFactory,
        codec_sv2::{
            self, binary_sv2,
            binary_sv2::{Seq0255, U256},
            framing_sv2, HandshakeRole, Initiator,
        },
        common_messages_sv2::{Protocol, Reconnect, SetupConnection},
        common_properties::{IsMiningUpstream, IsUpstream},
        handlers::{
            common::{ParseCommonMessagesFromUpstream, SendTo as SendToCommon},
            mining::{ParseMiningMessagesFromUpstream, SendTo, SupportedChannelTypes},
        },
        job_declaration_sv2::DeclareMiningJob,
        mining_sv2::{ExtendedExtranonce, Extranonce, SetCustomMiningJob, SetGroupChannel},
        parsers::{AnyMessage, Mining, MiningDeviceMessages},
        utils::{Id, Mutex},
        Error as RolesLogicError,
    },
};
use tokio::{net::TcpStream, task, task::AbortHandle};
use tracing::{debug, error, info, warn};

// A fixed-capacity circular buffer used for storing mappings with a limited history.
//
// When a new element is inserted and the buffer is at capacity, the oldest element
// is automatically removed from the front.
#[derive(Debug)]
struct CircularBuffer {
    // The internal `VecDeque` storing the key-value pairs.
    buffer: VecDeque<(u64, u32)>,
    // The maximum number of elements the buffer can hold.
    capacity: usize,
}

impl CircularBuffer {
    // Creates a new `CircularBuffer` with the specified capacity.
    fn new(capacity: usize) -> Self {
        CircularBuffer {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    // Inserts a new key-value pair into the buffer.
    fn insert(&mut self, key: u64, value: u32) {
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back((key, value));
    }

    // Retrieves the value associated with a given key from the buffer.
    fn get(&self, id: u64) -> Option<u32> {
        self.buffer
            .iter()
            .find_map(|&(key, value)| if key == id { Some(value) } else { None })
    }
}

impl std::default::Default for CircularBuffer {
    fn default() -> Self {
        Self::new(10)
    }
}

// Maintains mappings between request IDs, template IDs, and upstream-assigned job IDs.
//
// This struct is used to correlate different identifiers received from the
// Template Provider and the upstream pool throughout the job declaration process.
#[derive(Debug, Default)]
struct TemplateToJobId {
    // A circular buffer mapping Template IDs (u64) to upstream-assigned Job IDs (u32).
    // This buffer has a limited capacity to store recent mappings.
    template_id_to_job_id: CircularBuffer,
    // A HashMap mapping Request IDs (u32) from `DeclareMiningJob` messages to Template IDs (u64).
    request_id_to_template_id: HashMap<u32, u64>,
}

impl TemplateToJobId {
    // Registers a mapping between a Request ID and a Template ID.
    //
    // This is typically done when a `DeclareMiningJob` is sent, associating the
    // request ID with the template ID it's based on.
    fn register_template_id(&mut self, template_id: u64, request_id: u32) {
        self.request_id_to_template_id
            .insert(request_id, template_id);
    }

    // Registers a mapping between a Template ID and an upstream-assigned Job ID.
    //
    // This is typically done when a `SetCustomMiningJobSuccess` message is received
    // from the upstream pool, which provides the upstream's job ID for a previously
    // declared job (associated with a template ID). This mapping is stored in
    // the circular buffer.
    fn register_job_id(&mut self, template_id: u64, job_id: u32) {
        self.template_id_to_job_id.insert(template_id, job_id);
    }

    // Retrieves the upstream-assigned Job ID for a given Template ID from the circular buffer.
    fn get_job_id(&mut self, template_id: u64) -> Option<u32> {
        self.template_id_to_job_id.get(template_id)
    }

    // Removes and returns the Template ID associated with a given Request ID.
    fn take_template_id(&mut self, request_id: u32) -> Option<u64> {
        self.request_id_to_template_id.remove(&request_id)
    }

    // Creates a new `TemplateToJobId` instance with a default-sized circular buffer.
    fn new() -> Self {
        Self::default()
    }
}

/// Upstream struct representing all possible requirement for upstream instantiation.
#[derive(Debug)]
pub struct Upstream {
    // The channel ID assigned by the upstream pool for this connection.
    // This is received in the `OpenExtendedMiningChannelSuccess` message.
    channel_id: Option<u32>,
    /// This allows the upstream threads to be able to communicate back to the main thread its
    /// current status.
    tx_status: status::Sender,
    // The size of the `extranonce1` provided by the upstream pool.
    // Currently hardcoded to 16, which is the only size the pool is expected to support. ->
    // Inaccuracy.
    #[allow(dead_code)]
    pub upstream_extranonce1_size: usize,
    // Receiver channel for incoming messages from the upstream pool.
    pub receiver: Receiver<EitherFrame>,
    // Sender channel for sending messages to the upstream pool.
    pub sender: Sender<EitherFrame>,
    /// `DownstreamMiningNode` instance, present when a downstream miner is connected.
    pub downstream: Option<Arc<Mutex<Downstream>>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    // Trigger mechanism to detect unresponsive upstream behavior and initiate a pool change.
    pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
    // Optional `PoolChannelFactory` instance. This factory is created upon receiving
    // `OpenExtendedMiningChannelSuccess` and is used by the template provider client
    // to check shares received from the downstream, simulating the upstream's
    // channel logic
    channel_factory: Option<PoolChannelFactory>,
    // Manager for mapping Template IDs, Request IDs, and upstream Job IDs.
    template_to_job_id: TemplateToJobId,
    // Simple ID generator for creating unique request IDs for messages sent to the upstream.
    req_ids: Id,
    // The JDC's signature, used in the `ExtendedExtranonce` calculation.
    jdc_signature: String,
}

impl Upstream {
    /// This method sends message to upstream.
    pub async fn send(self_: &Arc<Mutex<Self>>, sv2_frame: StdFrame) -> ProxyResult<'static, ()> {
        let sender = self_
            .safe_lock(|s| s.sender.clone())
            .map_err(|_| PoisonLock)?;
        let either_frame = sv2_frame.into();
        sender.send(either_frame).await.map_err(|e| {
            super::super::error::Error::ChannelErrorSender(
                super::super::error::ChannelSendError::General(e.to_string()),
            )
        })?;
        Ok(())
    }
    /// Instantiates a new `Upstream` connection to the specified SV2 pool address.
    ///
    /// This method establishes a TCP connection, performs the Noise handshake
    /// , and initializes the `Upstream` struct with
    /// the necessary communication channels and state managers. It includes
    /// retry logic for the initial TCP connection attempt.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        address: SocketAddr,
        authority_public_key: Secp256k1PublicKey,
        tx_status: status::Sender,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
        jdc_signature: String,
    ) -> ProxyResult<'static, Arc<Mutex<Self>>> {
        // Attempt to connect to the SV2 Upstream role (pool) with retry logic.
        let socket = loop {
            match TcpStream::connect(address).await {
                Ok(socket) => break socket,
                Err(e) => {
                    error!(
                        "Failed to connect to Upstream role at {}, retrying in 5s: {}",
                        address, e
                    );

                    sleep(Duration::from_secs(5));
                }
            }
        };

        let pub_key: Secp256k1PublicKey = authority_public_key;
        let initiator = Initiator::from_raw_k(pub_key.into_bytes())?;

        info!(
            "PROXY SERVER - ACCEPTING FROM UPSTREAM: {}",
            socket.peer_addr()?
        );

        // Channel to send and receive messages to the SV2 Upstream role
        let (receiver, sender) = Connection::new(socket, HandshakeRole::Initiator(initiator))
            .await
            .expect("Failed to create connection");

        Ok(Arc::new(Mutex::new(Self {
            channel_id: None,
            upstream_extranonce1_size: 16, /* 16 is the default since that is the only value the
                                            * pool supports currently */
            tx_status,
            receiver,
            sender,
            downstream: None,
            task_collector,
            pool_chaneger_trigger,
            channel_factory: None,
            template_to_job_id: TemplateToJobId::new(),
            req_ids: Id::new(),
            jdc_signature,
        })))
    }

    /// Setups the connection with the SV2 Upstream role (most typically a SV2 Pool).
    pub async fn setup_connection(
        self_: Arc<Mutex<Self>>,
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<'static, ()> {
        // Get the `SetupConnection` message with Mining Device information (currently hard coded)
        let setup_connection = Self::get_setup_connection_message(min_version, max_version, true)?;

        // Put the `SetupConnection` message in a `StdFrame` to be sent over the wire
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        // Send the `SetupConnection` frame to the SV2 Upstream role
        // Only one Upstream role is supported, panics if multiple connections are encountered
        Self::send(&self_, sv2_frame).await?;

        let recv = self_
            .safe_lock(|s| s.receiver.clone())
            .map_err(|_| PoisonLock)?;

        // Wait for the SV2 Upstream to respond with either a `SetupConnectionSuccess` or a
        // `SetupConnectionError` inside a SV2 binary message frame
        let mut incoming: StdFrame = match recv.recv().await {
            Ok(frame) => frame.try_into()?,
            Err(e) => {
                error!("Upstream connection closed: {}", e);
                return Err(CodecNoise(
                    codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage,
                ));
            }
        };

        // Gets the binary frame message type from the message header
        let message_type = if let Some(header) = incoming.get_header() {
            header.msg_type()
        } else {
            return Err(framing_sv2::Error::ExpectedHandshakeFrame.into());
        };
        // Gets the message payload
        let payload = incoming.payload();

        // Handle the incoming message (should be either `SetupConnectionSuccess` or
        // `SetupConnectionError`)
        ParseCommonMessagesFromUpstream::handle_message_common(
            self_.clone(),
            message_type,
            payload,
        )?;
        Ok(())
    }

    // Constructs and sends a `SetCustomMiningJob` message to the upstream pool.
    //
    // This method is called after a job is declared to the JDS and validated
    // (receiving `DeclareMiningJobSuccess`). It takes the declared job details,
    // the latest `SetNewPrevHash` information, and the signed mining job token
    // to create the `SetCustomMiningJob` message. This message instructs the
    // upstream pool to make this specific job available to connected downstream.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_custom_jobs(
        self_: &Arc<Mutex<Self>>,
        declare_mining_job: DeclareMiningJob<'static>,
        set_new_prev_hash: roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
        merkle_path: Seq0255<'static, U256<'static>>,
        signed_token: binary_sv2::B0255<'static>,
        coinbase_tx_version: u32,
        coinbase_prefix: binary_sv2::B0255<'static>,
        coinbase_tx_input_n_sequence: u32,
        coinbase_tx_value_remaining: u64,
        coinbase_tx_outs: Vec<u8>,
        coinbase_tx_locktime: u32,
        template_id: u64,
    ) -> ProxyResult<'static, ()> {
        info!("Sending set custom mining job");

        // Get a new request ID for the SetCustomMiningJob message.
        let request_id = self_.safe_lock(|s| s.req_ids.next()).unwrap();

        // Wait until the channel ID is available (received in OpenExtendedMiningChannelSuccess).
        let channel_id = loop {
            if let Some(id) = self_.safe_lock(|s| s.channel_id).unwrap() {
                break id;
            };
            tokio::task::yield_now().await;
        };

        // Get the current timestamp for min_ntime.
        let updated_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        // Construct the SetCustomMiningJob message.
        let to_send = SetCustomMiningJob {
            channel_id,
            request_id,
            token: signed_token,
            version: declare_mining_job.version,
            prev_hash: set_new_prev_hash.prev_hash,
            min_ntime: updated_timestamp,
            nbits: set_new_prev_hash.n_bits,
            coinbase_tx_version,
            coinbase_prefix,
            coinbase_tx_input_n_sequence,
            coinbase_tx_value_remaining,
            coinbase_tx_outputs: coinbase_tx_outs.try_into().unwrap(),
            coinbase_tx_locktime,
            merkle_path,
        };
        let message = AnyMessage::Mining(Mining::SetCustomMiningJob(to_send));
        let frame: StdFrame = message.try_into().unwrap();

        // Register the mapping between the template ID and the request ID for this message.
        self_
            .safe_lock(|s| {
                s.template_to_job_id
                    .register_template_id(template_id, request_id)
            })
            .unwrap();
        Self::send(self_, frame).await
    }

    /// Parses incoming SV2 messages from the Upstream role and routes them to the
    /// appropriate handler for processing.
    ///
    /// This is the main loop for receiving and processing messages from the pool.
    /// It dispatches mining-specific messages to the `ParseMiningMessagesFromUpstream`
    /// trait implementation. Based on the handler's return value (`SendTo`), it
    /// either relays the message to the downstream mining node or performs other actions.
    /// Errors during message handling or receiving are reported via the status channel.
    #[allow(clippy::result_large_err)]
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let (recv, tx_status) = self_
            .safe_lock(|s| (s.receiver.clone(), s.tx_status.clone()))
            .map_err(|_| PoisonLock)?;

        // Spawn the main task for receiving and processing upstream messages.
        let main_task = {
            let self_ = self_.clone();
            task::spawn(async move {
                loop {
                    // Waiting to receive a message from the SV2 Upstream role
                    let incoming = handle_result!(tx_status, recv.recv().await);
                    let mut incoming: StdFrame = handle_result!(tx_status, incoming.try_into());
                    // On message receive, get the message type from the message header and get the
                    // message payload
                    let message_type =
                        incoming
                            .get_header()
                            .ok_or(super::super::error::Error::FramingSv2(
                                framing_sv2::Error::ExpectedSv2Frame,
                            ));

                    let message_type = handle_result!(tx_status, message_type).msg_type();

                    let payload = incoming.payload();

                    // Gets the response message for the received SV2 Upstream role message
                    // `handle_message_mining` takes care of the SetupConnection +
                    // SetupConnection.Success
                    let next_message_to_send =
                        Upstream::handle_message_mining(self_.clone(), message_type, payload);

                    // Routes the incoming messages accordingly
                    match next_message_to_send {
                        // This is a transparent proxy it will only relay messages as received
                        Ok(SendTo::RelaySameMessageToRemote(downstream_mutex)) => {
                            let sv2_frame: codec_sv2::Sv2Frame<
                                MiningDeviceMessages,
                                buffer_sv2::Slice,
                            > = incoming.map(|payload| payload.try_into().unwrap());
                            Downstream::send(&downstream_mutex, sv2_frame)
                                .await
                                .unwrap();
                        }
                        // No need to handle impossible state just panic cause are impossible and we
                        // will never panic ;-) Verified: handle_message_mining only either panics,
                        // returns Ok(SendTo::None(None)) or Ok(SendTo::None(Some(m))), or returns
                        // Err This is a transparent proxy it will only
                        // relay messages as received
                        Ok(SendTo::None(_)) => (),
                        Ok(_) => unreachable!(),
                        Err(e) => {
                            let status = status::Status {
                                state: status::State::UpstreamShutdown(UpstreamIncoming(e)),
                            };
                            error!(
                                "TERMINATING: Error handling pool role message: {:?}",
                                status
                            );
                            if let Err(e) = tx_status.send(status).await {
                                error!("Status channel down: {:?}", e);
                            }

                            break;
                        }
                    }
                }
            })
        };
        self_
            .safe_lock(|s| {
                s.task_collector
                    .safe_lock(|c| c.push(main_task.abort_handle()))
                    .unwrap()
            })
            .unwrap();
        Ok(())
    }

    /// Creates the `SetupConnection` message to setup the connection with the SV2 Upstream role.
    /// TODO: The Mining Device information is hard coded here, need to receive from Downstream
    /// instead.
    #[allow(clippy::result_large_err)]
    fn get_setup_connection_message(
        min_version: u16,
        max_version: u16,
        is_work_selection_enabled: bool,
    ) -> ProxyResult<'static, SetupConnection<'static>> {
        let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
        let vendor = String::new().try_into()?;
        let hardware_version = String::new().try_into()?;
        let firmware = String::new().try_into()?;
        let device_id = String::new().try_into()?;
        let flags = match is_work_selection_enabled {
            false => 0b0000_0000_0000_0000_0000_0000_0000_0100,
            true => 0b0000_0000_0000_0000_0000_0000_0000_0110,
        };
        Ok(SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version,
            max_version,
            flags,
            endpoint_host,
            endpoint_port: 50,
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }

    /// This method provides the `PoolChannelFactory` once it has been created
    /// (upon receiving `OpenExtendedMiningChannelSuccess`).
    ///
    /// This method is used by other components (like the template provider client)
    /// that need access to the channel factory to perform share validation or
    /// other channel-related operations. It waits until the factory is available.
    pub async fn take_channel_factory(self_: Arc<Mutex<Self>>) -> PoolChannelFactory {
        // Wait until the channel_factory field is populated.
        while self_.safe_lock(|s| s.channel_factory.is_none()).unwrap() {
            tokio::task::yield_now().await;
        }
        self_
            .safe_lock(|s| {
                let mut factory = None;
                std::mem::swap(&mut s.channel_factory, &mut factory);
                factory.unwrap()
            })
            .unwrap()
    }

    /// This method retrieves the upstream-assigned job ID for a given template ID.
    ///
    /// This method checks the `template_to_job_id` mapper. If the mapping is not
    /// immediately available (because `SetCustomMiningJobSuccess` hasn't been
    /// processed yet), it waits until the job ID is registered.
    pub async fn get_job_id(self_: &Arc<Mutex<Self>>, template_id: u64) -> u32 {
        loop {
            if let Some(id) = self_
                .safe_lock(|s| s.template_to_job_id.get_job_id(template_id))
                .unwrap()
            {
                return id;
            }
            tokio::task::yield_now().await;
        }
    }
}

// not really used..
impl IsUpstream for Upstream {
    fn get_version(&self) -> u16 {
        todo!()
    }

    fn get_flags(&self) -> u32 {
        todo!()
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        todo!()
    }

    fn get_id(&self) -> u32 {
        todo!()
    }

    fn get_mapper(&mut self) -> Option<&mut roles_logic_sv2::common_properties::RequestIdMapper> {
        todo!()
    }
}

// Not really used...
impl IsMiningUpstream for Upstream {
    fn total_hash_rate(&self) -> u64 {
        todo!()
    }

    fn add_hash_rate(&mut self, _to_add: u64) {
        todo!()
    }

    fn get_opened_channels(
        &mut self,
    ) -> &mut Vec<roles_logic_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, _c: roles_logic_sv2::common_properties::UpstreamChannel) {
        todo!()
    }
}

impl ParseCommonMessagesFromUpstream for Upstream {
    // Handles a `SetupConnectionSuccess` message received from the upstream pool.
    //
    // Returns `Ok(SendToCommon::None(None))` as no immediate response is required.
    fn handle_setup_connection_success(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendToCommon, RolesLogicError> {
        info!(
            "Received `SetupConnectionSuccess` from Pool: version={}, flags={:b}",
            m.used_version, m.flags
        );
        Ok(SendToCommon::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }

    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }
}

/// Connection-wide SV2 Upstream role messages parser implemented by a downstream ("downstream"
/// here is relative to the SV2 Upstream role and is represented by this `Upstream` struct).
impl ParseMiningMessagesFromUpstream<Downstream> for Upstream {
    // Returns the channel type supported between the SV2 Upstream role (pool) and this
    // `Upstream` instance. For a JDC, this is always `Extended`
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    // Indicates whether work selection is enabled for this connection..
    fn is_work_selection_enabled(&self) -> bool {
        true
    }

    /// Handles an `OpenStandardMiningChannelSuccess` message.
    ///
    /// This method panics because standard mining channels are explicitly NOT
    /// used between the JDC and the SV2 Upstream role.
    /// Only Extended channels are expected.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    // Handles an `OpenExtendedMiningChannelSuccess` message received from the upstream pool.
    //
    // This message confirms that an extended mining channel has been successfully opened.
    // It provides the assigned `channel_id`, `extranonce_prefix`, `extranonce_size`, and `target`.
    // This method uses this information to:
    // 1. Store the assigned `channel_id`.
    // 2. Create a `PoolChannelFactory` instance that simulates the upstream's channel logic for the
    //    template provider client to use in share validation.
    // 3. Relays the original `OpenExtendedMiningChannelSuccess` message to the downstream mining
    //    node if one is connected.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` if a downstream
    // is connected, indicating the message should be relayed.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}",
            m.request_id, m.channel_id
        );
        debug!("OpenStandardMiningChannelSuccess: {:?}", m);
        // --- Create the PoolChannelFactory  ---
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
        let jdc_signature_len = self.jdc_signature.len();
        let prefix_len = m.extranonce_prefix.to_vec().len();
        let self_len = 0;
        let total_len = prefix_len + m.extranonce_size as usize;
        let range_0 = 0..prefix_len;
        let range_1 = prefix_len..prefix_len + jdc_signature_len + self_len;
        let range_2 = prefix_len + jdc_signature_len + self_len..total_len;

        // Create an ExtendedExtranonce structure defining the layout of the extranonce.
        let extranonces = ExtendedExtranonce::new(
            range_0,
            range_1,
            range_2,
            Some(self.jdc_signature.as_bytes().to_vec()),
        )
        .map_err(|err| RolesLogicError::ExtendedExtranonceCreationFailed(format!("{:?}", err)))?;

        // Job creator for the factory.
        let creator = roles_logic_sv2::job_creator::JobsCreators::new(total_len as u8);
        // Placeholder shares per minute
        let share_per_min = 1.0;

        let channel_kind =
            roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::ProxyJd {
                upstream_target: m.target.clone().into(),
            };

        // Create the PoolChannelFactory instance.
        let mut channel_factory = PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            channel_kind,
            vec![],
        );

        // Replicate the upstream's extended channel information within the factory.
        let extranonce: Extranonce = m
            .extranonce_prefix
            .into_static()
            .to_vec()
            .try_into()
            .unwrap();

        // Store the assigned channel ID.
        self.channel_id = Some(m.channel_id);
        channel_factory
            .replicate_upstream_extended_channel_only_jd(
                m.target.into_static(),
                extranonce,
                m.channel_id,
                m.extranonce_size,
            )
            .expect("Impossible to open downstream channel");
        self.channel_factory = Some(channel_factory);

        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles an `OpenMiningChannelError` message received from the upstream pool.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received OpenExtendedMiningChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles an `UpdateChannelError` message received from the upstream pool.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_update_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received UpdateChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles a `CloseChannel` message received from the upstream pool.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_close_channel(
        &mut self,
        m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received CloseChannel for channel id: {}", m.channel_id);
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles a `SetExtranoncePrefix` message received from the upstream pool.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_set_extranonce_prefix(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received SetExtranoncePrefix for channel id: {}",
            m.channel_id
        );
        debug!("SetExtranoncePrefix: {:?}", m);
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles a `SubmitSharesSuccess` message received from the upstream pool.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Received SubmitSharesSuccess");
        debug!("SubmitSharesSuccess: {:?}", m);
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles a `SubmitSharesError` message received from the upstream pool.
    //
    // This message indicates that a share submitted by a miner was rejected by
    // the pool. The current implementation logs the error code and triggers
    // the `pool_changer_trigger`, which may initiate a pool fallback if multiple
    // share errors occur. It does NOT relay the error message downstream,
    // as the JDC handles pool fallback.
    //
    // Returns `Ok(SendTo::None(None))` as no message is relayed downstream in this case.
    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received SubmitSharesError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        self.pool_chaneger_trigger
            .safe_lock(|t| t.start(self.tx_status.clone()))
            .unwrap();
        Ok(SendTo::None(None))
    }

    // Handles a `NewMiningJob` message.
    fn handle_new_mining_job(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::NewMiningJob,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    // Handles a `NewExtendedMiningJob` message received from the upstream pool.
    //
    // This message provides a new mining job using the extended format. However,
    // in this JDC implementation, the job information is primarily derived from
    // the Template Provider and Job Declarator. Therefore, this message from
    // the upstream pool is logged as a warning and ignored, as the JDC relies
    // on its declared jobs.
    //
    // Returns `Ok(SendTo::None(None))` indicating that the message is processed
    // but no action or response is needed.
    fn handle_new_extended_mining_job(
        &mut self,
        _: roles_logic_sv2::mining_sv2::NewExtendedMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        warn!("Extended job received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(SendTo::None(None))
    }

    // Handles a `SetNewPrevHash` message received from the upstream pool.
    //
    // This message indicates that the previous block hash has changed. Similar
    // to `NewExtendedMiningJob`, this message from the upstream pool is logged
    // as a warning and ignored, as the JDC relies on the `SetNewPrevHash` received
    // from the Template Provider which triggers the promotion of future jobs
    // declared via the JDS.
    //
    // Returns `Ok(SendTo::None(None))` indicating that the message is processed
    // but no action or response is needed.
    fn handle_set_new_prev_hash(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        warn!("SNPH received from upstream, proxy ignored it, and used the one declared by JDC");
        Ok(SendTo::None(None))
    }

    // Handles a `SetCustomMiningJobSuccess` message received from the upstream pool.
    //
    // This message confirms that a `SetCustomMiningJob` request previously sent
    // by the JDC has been successfully processed by the upstream pool. It provides
    // the upstream's assigned `job_id` for this job. This method logs the success
    // and registers the mapping between the original template ID (derived from
    // the `request_id`) and the upstream's `job_id` in the `template_to_job_id` mapper.
    //
    // Returns `Ok(SendTo::None(None))` as no message is relayed downstream for this event.
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        // TODO
        info!(
            "Received SetCustomMiningJobSuccess for channel id: {} for job id: {}",
            m.channel_id, m.job_id
        );
        debug!("SetCustomMiningJobSuccess: {:?}", m);
        if let Some(template_id) = self.template_to_job_id.take_template_id(m.request_id) {
            self.template_to_job_id
                .register_job_id(template_id, m.job_id);
            Ok(SendTo::None(None))
        } else {
            error!("Attention received a SetupConnectionSuccess with unknown request_id");
            Ok(SendTo::None(None))
        }
    }

    // Handles a `SetCustomMiningJobError` message received from the upstream pool.
    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    // Handles a `SetTarget` message received from the upstream pool.
    //
    // This message updates the mining target (difficulty) for a specific channel.
    // This method updates the target in the internal `PoolChannelFactory` and
    // in the downstream mining node's channel status to ensure miners are working
    // on the correct difficulty. It also relays the original message downstream.
    //
    // Returns `Ok(SendTo::RelaySameMessageToRemote(downstream_mutex))` to relay
    // the message downstream.
    fn handle_set_target(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received SetTarget for channel id: {}", m.channel_id);
        debug!("SetTarget: {:?}", m);
        if let Some(factory) = self.channel_factory.as_mut() {
            factory.update_target_for_channel(m.channel_id, m.maximum_target.clone().into());
            factory.set_target(&mut m.maximum_target.clone().into());
        }
        if let Some(downstream) = &self.downstream {
            let _ = downstream.safe_lock(|d| {
                let factory = d.status.get_channel();
                factory.set_target(&mut m.maximum_target.clone().into());
                factory.update_target_for_channel(m.channel_id, m.maximum_target.into());
            });
        }
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    // Handles a `SetGroupChannel` message received from the upstream pool. Not implemented.
    fn handle_set_group_channel(
        &mut self,
        _m: SetGroupChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        todo!()
    }
}
