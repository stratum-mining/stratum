use crate::downstream::DownstreamMiningNode as Downstream;

use crate::{
    error::Error::{CodecNoise, PoisonLock, UpstreamIncoming},
    status,
    upstream_sv2::{EitherFrame, Message, StdFrame},
    PoolChangerTrigger, ProxyResult,
};
use async_channel::{Receiver, Sender};
use binary_sv2::{Seq0255, U256};
use codec_sv2::{Frame, HandshakeRole, Initiator};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    channel_logic::channel_factory::PoolChannelFactory,
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::{
        common::{ParseUpstreamCommonMessages, SendTo as SendToCommon},
        mining::{ParseUpstreamMiningMessages, SendTo},
    },
    job_declaration_sv2::DeclareMiningJob,
    mining_sv2::{ExtendedExtranonce, Extranonce, SetCustomMiningJob},
    parsers::{Mining, MiningDeviceMessages, PoolMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::{Id, Mutex},
    Error as RolesLogicError,
};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, thread::sleep, time::Duration};
use tokio::{net::TcpStream, task, task::AbortHandle};
use tracing::{error, info, warn};

use std::collections::VecDeque;

#[derive(Debug)]
struct CircularBuffer {
    buffer: VecDeque<(u64, u32)>,
    capacity: usize,
}

impl CircularBuffer {
    fn new(capacity: usize) -> Self {
        CircularBuffer {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn insert(&mut self, key: u64, value: u32) {
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back((key, value));
    }

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

#[derive(Debug, Default)]
struct TemplateToJobId {
    template_id_to_job_id: CircularBuffer,
    request_id_to_template_id: HashMap<u32, u64>,
}

impl TemplateToJobId {
    fn register_template_id(&mut self, template_id: u64, request_id: u32) {
        self.request_id_to_template_id
            .insert(request_id, template_id);
    }

    fn register_job_id(&mut self, template_id: u64, job_id: u32) {
        self.template_id_to_job_id.insert(template_id, job_id);
    }

    fn get_job_id(&mut self, template_id: u64) -> Option<u32> {
        self.template_id_to_job_id.get(template_id)
    }

    fn take_template_id(&mut self, request_id: u32) -> Option<u64> {
        self.request_id_to_template_id.remove(&request_id)
    }

    fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
pub struct Upstream {
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection,
    /// e.g. it is used for broadcasting new jobs by the `NewExtendedMiningJob` message.
    channel_id: Option<u32>,
    /// This allows the upstream threads to be able to communicate back to the main thread its
    /// current status.
    tx_status: status::Sender,
    /// Minimum `extranonce2` size. Initially requested in the `jdc-config.toml`, and ultimately
    /// set by the SV2 Upstream via the SV2 `OpenExtendedMiningChannelSuccess` message.
    pub min_extranonce_size: u16,
    pub upstream_extranonce1_size: usize,
    /// String be included in coinbase tx input scriptsig
    pub pool_signature: String,
    /// Receives messages from the SV2 Upstream role
    pub receiver: Receiver<EitherFrame>,
    /// Sends messages to the SV2 Upstream role
    pub sender: Sender<EitherFrame>,
    pub downstream: Option<Arc<Mutex<Downstream>>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
    channel_factory: Option<PoolChannelFactory>,
    template_to_job_id: TemplateToJobId,
    req_ids: Id,
}

impl Upstream {
    pub async fn send(self_: &Arc<Mutex<Self>>, sv2_frame: StdFrame) -> ProxyResult<'static, ()> {
        let sender = self_
            .safe_lock(|s| s.sender.clone())
            .map_err(|_| PoisonLock)?;
        let either_frame = sv2_frame.into();
        sender.send(either_frame).await.map_err(|e| {
            crate::Error::ChannelErrorSender(crate::error::ChannelSendError::General(e.to_string()))
        })?;
        Ok(())
    }
    /// Instantiate a new `Upstream`.
    /// Connect to the SV2 Upstream role (most typically a SV2 Pool). Initializes the
    /// `UpstreamConnection` with a channel to send and receive messages from the SV2 Upstream
    /// role and uses channels provided in the function arguments to send and receive messages
    /// from the `Downstream`.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::too_many_arguments))]
    pub async fn new(
        address: SocketAddr,
        authority_public_key: Secp256k1PublicKey,
        min_extranonce_size: u16,
        pool_signature: String,
        tx_status: status::Sender,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
    ) -> ProxyResult<'static, Arc<Mutex<Self>>> {
        // Connect to the SV2 Upstream role retry connection every 5 seconds.
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
        let (receiver, sender, _, _) = Connection::new(socket, HandshakeRole::Initiator(initiator))
            .await
            .expect("Failed to create connection");

        Ok(Arc::new(Mutex::new(Self {
            channel_id: None,
            min_extranonce_size,
            upstream_extranonce1_size: 16, // 16 is the default since that is the only value the pool supports currently
            pool_signature,
            tx_status,
            receiver,
            sender,
            downstream: None,
            task_collector,
            pool_chaneger_trigger,
            channel_factory: None,
            template_to_job_id: TemplateToJobId::new(),
            req_ids: Id::new(),
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
        ParseUpstreamCommonMessages::handle_message_common(
            self_.clone(),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )?;
        Ok(())
    }

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
        let request_id = self_.safe_lock(|s| s.req_ids.next()).unwrap();
        let channel_id = loop {
            if let Some(id) = self_.safe_lock(|s| s.channel_id).unwrap() {
                break id;
            };
            tokio::task::yield_now().await;
        };

        let updated_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

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
            extranonce_size: 0,
        };
        let message = PoolMessages::Mining(Mining::SetCustomMiningJob(to_send));
        let frame: StdFrame = message.try_into().unwrap();
        self_
            .safe_lock(|s| {
                s.template_to_job_id
                    .register_template_id(template_id, request_id)
            })
            .unwrap();
        Self::send(self_, frame).await
    }

    /// Parses the incoming SV2 message from the Upstream role and routes the message to the
    /// appropriate handler.
    #[allow(clippy::result_large_err)]
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let (recv, tx_status) = self_
            .safe_lock(|s| (s.receiver.clone(), s.tx_status.clone()))
            .map_err(|_| PoisonLock)?;

        let main_task = {
            let self_ = self_.clone();
            task::spawn(async move {
                loop {
                    // Waiting to receive a message from the SV2 Upstream role
                    let incoming = handle_result!(tx_status, recv.recv().await);
                    let mut incoming: StdFrame = handle_result!(tx_status, incoming.try_into());
                    // On message receive, get the message type from the message header and get the
                    // message payload
                    let message_type = incoming.get_header().ok_or(
                        crate::error::Error::FramingSv2(framing_sv2::Error::ExpectedSv2Frame),
                    );

                    let message_type = handle_result!(tx_status, message_type).msg_type();

                    let payload = incoming.payload();

                    // Since this is not communicating with an SV2 proxy, but instead a custom SV1
                    // proxy where the routing logic is handled via the `Upstream`'s communication
                    // channels, we do not use the mining routing logic in the SV2 library and specify
                    // no mining routing logic here
                    let routing_logic = MiningRoutingLogic::None;

                    // Gets the response message for the received SV2 Upstream role message
                    // `handle_message_mining` takes care of the SetupConnection +
                    // SetupConnection.Success
                    let next_message_to_send = Upstream::handle_message_mining(
                        self_.clone(),
                        message_type,
                        payload,
                        routing_logic,
                    );

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
                        // returns Ok(SendTo::None(None)) or Ok(SendTo::None(Some(m))), or returns Err
                        // This is a transparent proxy it will only relay messages as received
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

    pub async fn take_channel_factory(self_: Arc<Mutex<Self>>) -> PoolChannelFactory {
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

impl IsUpstream<Downstream, NullDownstreamMiningSelector> for Upstream {
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

    fn get_remote_selector(&mut self) -> &mut NullDownstreamMiningSelector {
        todo!()
    }
}

impl IsMiningUpstream<Downstream, NullDownstreamMiningSelector> for Upstream {
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

impl ParseUpstreamCommonMessages<NoRouting> for Upstream {
    fn handle_setup_connection_success(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendToCommon, RolesLogicError> {
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
}

/// Connection-wide SV2 Upstream role messages parser implemented by a downstream ("downstream"
/// here is relative to the SV2 Upstream role and is represented by this `Upstream` struct).
impl ParseUpstreamMiningMessages<Downstream, NullDownstreamMiningSelector, NoRouting> for Upstream {
    /// Returns the channel type between the SV2 Upstream role and the `Upstream`, which will
    /// always be `Extended` for a SV1/SV2 Translator Proxy.
    fn get_channel_type(&self) -> roles_logic_sv2::handlers::mining::SupportedChannelTypes {
        roles_logic_sv2::handlers::mining::SupportedChannelTypes::Extended
    }

    /// Work selection is disabled for SV1/SV2 Translator Proxy and all work selection is performed
    /// by the SV2 Upstream role.
    fn is_work_selection_enabled(&self) -> bool {
        true
    }

    /// The SV2 `OpenStandardMiningChannelSuccess` message is NOT handled because it is NOT used
    /// for the Translator Proxy as only `Extended` channels are used between the SV1/SV2 Translator
    /// Proxy and the SV2 Upstream role.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
        _remote: Option<Arc<Mutex<Downstream>>>,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// This is a transparent proxy so OpenExtendedMiningChannel is sent as it is downstream.
    /// This message is used also to create a PoolChannelFactory that mock the upstream pool.
    /// this PoolChannelFactory is used by the template provider client in order to check shares
    /// received by downstream using the right extranonce and seeing the same hash that the downstream
    /// saw. PoolChannelFactory coinbase pre and suf are setted by the JD client.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Receive open extended mining channel success");
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
        let pool_signature = self.pool_signature.clone();
        let prefix_len = m.extranonce_prefix.to_vec().len();
        let self_len = 0;
        let total_len = prefix_len + m.extranonce_size as usize;
        let range_0 = 0..prefix_len;
        let range_1 = prefix_len..prefix_len + self_len;
        let range_2 = prefix_len + self_len..total_len;

        let extranonces = ExtendedExtranonce::new(range_0, range_1, range_2);
        let creator = roles_logic_sv2::job_creator::JobsCreators::new(total_len as u8);
        let share_per_min = 1.0;
        let channel_kind =
            roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::ProxyJd {
                upstream_target: m.target.clone().into(),
            };
        let mut channel_factory = PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            channel_kind,
            vec![],
            pool_signature,
        );
        let extranonce: Extranonce = m
            .extranonce_prefix
            .into_static()
            .to_vec()
            .try_into()
            .unwrap();
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

    /// Handles the SV2 `OpenExtendedMiningChannelError` message (TODO).
    fn handle_open_mining_channel_error(
        &mut self,
        _: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `UpdateChannelError` message (TODO).
    fn handle_update_channel_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `CloseChannel` message (TODO).
    fn handle_close_channel(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `SetExtranoncePrefix` message (TODO).
    fn handle_set_extranonce_prefix(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `SubmitSharesSuccess` message.
    fn handle_submit_shares_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `SubmitSharesError` message.
    fn handle_submit_shares_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        self.pool_chaneger_trigger
            .safe_lock(|t| t.start(self.tx_status.clone()))
            .unwrap();
        Ok(SendTo::None(None))
    }

    /// The SV2 `NewMiningJob` message is NOT handled because it is NOT used for the Translator
    /// Proxy as only `Extended` channels are used between the SV1/SV2 Translator Proxy and the SV2
    /// Upstream role.
    fn handle_new_mining_job(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::NewMiningJob,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// Handles the SV2 `NewExtendedMiningJob` message which is used (along with the SV2
    /// `SetNewPrevHash` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_new_extended_mining_job(
        &mut self,
        _: roles_logic_sv2::mining_sv2::NewExtendedMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        warn!("Extended job received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SetNewPrevHash` message which is used (along with the SV2
    /// `NewExtendedMiningJob` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_set_new_prev_hash(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        warn!("SNPH received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SetCustomMiningJobSuccess` message (TODO).
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        // TODO
        info!("Set custom mining job success {}", m.job_id);
        if let Some(template_id) = self.template_to_job_id.take_template_id(m.request_id) {
            self.template_to_job_id
                .register_job_id(template_id, m.job_id);
            Ok(SendTo::None(None))
        } else {
            error!("Attention received a SetupConnectionSuccess with unknown request_id");
            Ok(SendTo::None(None))
        }
    }

    /// Handles the SV2 `SetCustomMiningJobError` message (TODO).
    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `SetTarget` message which updates the Downstream role(s) target
    /// difficulty via the SV1 `mining.set_difficulty` message.
    fn handle_set_target(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        if let Some(factory) = self.channel_factory.as_mut() {
            factory.update_target_for_channel(
                m.channel_id,
                m.maximum_target.clone().try_into().unwrap(),
            );
            factory.set_target(&mut m.maximum_target.clone().try_into().unwrap());
        }
        if let Some(downstream) = &self.downstream {
            let _ = downstream.safe_lock(|d| {
                let factory = d.status.get_channel();
                factory.set_target(&mut m.maximum_target.clone().try_into().unwrap());
                factory
                    .update_target_for_channel(m.channel_id, m.maximum_target.try_into().unwrap());
            });
        }
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `Reconnect` message (TODO).
    fn handle_reconnect(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::Reconnect,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }
}
