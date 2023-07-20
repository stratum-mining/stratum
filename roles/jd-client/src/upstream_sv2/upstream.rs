use crate::downstream::DownstreamMiningNode as Downstream;

use crate::{
    error::Error::{CodecNoise, PoisonLock, UpstreamIncoming},
    status,
    upstream_sv2::{EitherFrame, Message, StdFrame},
    ProxyResult,
};
use async_channel::{Receiver, Sender};
use binary_sv2::{Seq0255, U256};
use codec_sv2::{Frame, HandshakeRole, Initiator};
use error_handling::handle_result;
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    bitcoin::BlockHash,
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
    utils::Mutex,
    Error as RolesLogicError,
};
use std::{net::SocketAddr, sync::Arc, thread::sleep, time::Duration};
use tokio::{net::TcpStream, task, task::AbortHandle};
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PrevHash {
    /// `prevhash` of mining job.
    prev_hash: BlockHash,
    /// `nBits` encoded difficulty target.
    nbits: u32,
}

#[derive(Debug)]
pub struct Upstream {
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection,
    /// e.g. it is used for broadcasting new jobs by the `NewExtendedMiningJob` message.
    channel_id: Option<u32>,
    /// Identifier of the job as provided by the ` SetCustomMiningJobSucces` message
    pub last_job_id: u32,
    /// This allows the upstream threads to be able to communicate back to the main thread its
    /// current status.
    tx_status: status::Sender,
    /// Minimum `extranonce2` size. Initially requested in the `proxy-config.toml`, and ultimately
    /// set by the SV2 Upstream via the SV2 `OpenExtendedMiningChannelSuccess` message.
    pub min_extranonce_size: u16,
    pub upstream_extranonce1_size: usize,
    /// Receives messages from the SV2 Upstream role
    pub receiver: Receiver<EitherFrame>,
    /// Sends messages to the SV2 Upstream role
    pub sender: Sender<EitherFrame>,
    pub downstream: Option<Arc<Mutex<Downstream>>>,
    pub channel_factory_sender: Sender<PoolChannelFactory>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
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
        authority_public_key: codec_sv2::noise_sv2::formats::EncodedEd25519PublicKey,
        min_extranonce_size: u16,
        tx_status: status::Sender,
        channel_factory_sender: Sender<PoolChannelFactory>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
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

        let pub_key: codec_sv2::noise_sv2::formats::EncodedEd25519PublicKey = authority_public_key;
        let initiator = Initiator::from_raw_k(*pub_key.into_inner().as_bytes())?;

        info!(
            "PROXY SERVER - ACCEPTING FROM UPSTREAM: {}",
            socket.peer_addr()?
        );

        // Channel to send and receive messages to the SV2 Upstream role
        let (receiver, sender) = Connection::new(socket, HandshakeRole::Initiator(initiator)).await;

        Ok(Arc::new(Mutex::new(Self {
            channel_id: None,
            last_job_id: 0,
            min_extranonce_size,
            upstream_extranonce1_size: 16, // 16 is the default since that is the only value the pool supports currently
            tx_status,
            receiver,
            sender,
            downstream: None,
            channel_factory_sender,
            task_collector,
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

        debug!("Sent SetupConnection to Upstream, waiting for response");
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

        info!("Up: Receiving: {:?}", &incoming);
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

    pub async fn set_custom_jobs(
        self_: &Arc<Mutex<Self>>,
        declare_mining_job: DeclareMiningJob<'static>,
        set_new_prev_hash: roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
        merkle_path: Seq0255<'static, U256<'static>>,
        signed_token: binary_sv2::B0255<'static>,
    ) -> ProxyResult<'static, ()> {
        let to_send = SetCustomMiningJob {
            channel_id: self_
                .safe_lock(|s| *s.channel_id.as_ref().unwrap())
                .unwrap(),
            request_id: 0,
            token: signed_token,
            version: declare_mining_job.version,
            prev_hash: set_new_prev_hash.prev_hash,
            min_ntime: set_new_prev_hash.header_timestamp,
            nbits: set_new_prev_hash.n_bits,
            coinbase_tx_version: declare_mining_job.coinbase_tx_version,
            coinbase_prefix: declare_mining_job.coinbase_prefix,
            coinbase_tx_input_n_sequence: declare_mining_job.coinbase_tx_input_n_sequence,
            coinbase_tx_value_remaining: declare_mining_job.coinbase_tx_value_remaining,
            coinbase_tx_outputs: declare_mining_job.coinbase_tx_outputs,
            coinbase_tx_locktime: declare_mining_job.coinbase_tx_locktime,
            merkle_path: merkle_path,
            extranonce_size: declare_mining_job.min_extranonce_size,
        };
        let message = PoolMessages::Mining(Mining::SetCustomMiningJob(to_send));
        let frame: StdFrame = message.try_into().unwrap();
        Self::send(self_, frame).await
    }

    /// Parses the incoming SV2 message from the Upstream role and routes the message to the
    /// appropriate handler.
    #[allow(clippy::result_large_err)]
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let clone = self_.clone();
        let (recv, tx_status) = clone
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
                        Ok(SendTo::None(None)) => (),
                        // No need to handle impossible state just panic cause are impossible and we
                        // will never panic ;-) Verified: handle_message_mining only either panics,
                        // returns Ok(SendTo::None(None)) or Ok(SendTo::None(Some(m))), or returns Err
                        // This is a transparent proxy it will only relay messages as received
                        Ok(_) => panic!(),
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
        debug!("Up: Handling SetupConnectionSuccess");
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
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));

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
            roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::Pool;
        let mut channel_factory = PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            channel_kind,
            vec![],
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
        self.channel_factory_sender
            .send_blocking(channel_factory)
            .unwrap();

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
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Up: Successfully Submitted Share");
        debug!("Up: Handling SubmitSharesSuccess: {:?}", &m);
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
    }

    /// Handles the SV2 `SubmitSharesError` message.
    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Up: Rejected Submitted Share");
        debug!("Up: Handling SubmitSharesError: {:?}", &m);
        Ok(SendTo::RelaySameMessageToRemote(
            self.downstream.as_ref().unwrap().clone(),
        ))
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
        info!("Extended job received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SetNewPrevHash` message which is used (along with the SV2
    /// `NewExtendedMiningJob` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_set_new_prev_hash(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("SNPH received from upstream, proxy ignore it, and use the one declared by JOB DECLARATOR");
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SetCustomMiningJobSuccess` message (TODO).
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        // TODO
        self.last_job_id = m.job_id;
        Ok(SendTo::None(None))
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
        _: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
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
