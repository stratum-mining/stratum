use crate::{
    downstream_sv1::Downstream,
    error::Error::{CodecNoise, PoisonLock, UpstreamIncoming},
    status,
    upstream_sv2::{EitherFrame, Message, StdFrame, UpstreamConnection},
    ProxyResult,
};
use async_channel::{Receiver, Sender};
use async_std::{net::TcpStream, task};
use binary_sv2::u256_from_int;
use codec_sv2::{Frame, HandshakeRole, Initiator};
use error_handling::handle_result;
use network_helpers::Connection;
use roles_logic_sv2::{
    bitcoin::BlockHash,
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::{
        common::{ParseUpstreamCommonMessages, SendTo as SendToCommon},
        mining::{ParseUpstreamMiningMessages, SendTo},
    },
    mining_sv2::{
        ExtendedExtranonce, Extranonce, NewExtendedMiningJob, OpenExtendedMiningChannel,
        SetNewPrevHash, SubmitSharesExtended,
    },
    parsers::Mining,
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::Mutex,
    Error as RolesLogicError,
};
use std::{net::SocketAddr, sync::Arc, thread::sleep, time::Duration};
use tracing::{debug, error, info, warn};
/// Represents the currently active `prevhash` of the mining job being worked on OR being submitted
/// from the Downstream role.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PrevHash {
    /// `prevhash` of mining job.
    prev_hash: BlockHash,
    /// `nBits` encoded difficulty target.
    nbits: u32,
}

#[derive(Debug, Clone)]
pub struct Upstream {
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection,
    /// e.g. it is used for broadcasting new jobs by the `NewExtendedMiningJob` message.
    channel_id: Option<u32>,
    /// Identifier of the job as provided by the `NewExtendedMiningJob` message.
    job_id: Option<u32>,
    /// Bytes used as implicit first part of `extranonce`.
    extranonce_prefix: Option<Vec<u8>>,
    /// Represents a connection to a SV2 Upstream role.
    connection: UpstreamConnection,
    /// Receives SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages.
    /// Translated by and sent from the `Bridge`.
    rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
    /// Sends SV2 `SetNewPrevHash` messages to be translated (along with SV2 `NewExtendedMiningJob`
    /// messages) into SV1 `mining.notify` messages. Received and translated by the `Bridge`.
    tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
    /// Sends SV2 `NewExtendedMiningJob` messages to be translated (along with SV2 `SetNewPrevHash`
    /// messages) into SV1 `mining.notify` messages. Received and translated by the `Bridge`.
    tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
    /// Sends the extranonce1 and the channel id received in the SV2 `OpenExtendedMiningChannelSuccess` message to be
    /// used by the `Downstream` and sent to the Downstream role in a SV2 `mining.subscribe`
    /// response message. Passed to the `Downstream` on connection creation.
    tx_sv2_extranonce: Sender<(ExtendedExtranonce, u32)>,
    /// This allows the upstream threads to be able to communicate back to the main thread its
    /// current status.
    tx_status: status::Sender,
    /// The first `target` is received by the Upstream role in the SV2
    /// `OpenExtendedMiningChannelSuccess` message, then updated periodically via SV2 `SetTarget`
    /// messages. Passed to the `Downstream` on connection creation and sent to the Downstream role
    /// via the SV1 `mining.set_difficulty` message.
    target: Arc<Mutex<Vec<u8>>>,
    /// Minimum `extranonce2` size. Initially requested in the `proxy-config.toml`, and ultimately
    /// set by the SV2 Upstream via the SV2 `OpenExtendedMiningChannelSuccess` message.
    pub min_extranonce_size: u16,
}

impl PartialEq for Upstream {
    fn eq(&self, other: &Self) -> bool {
        self.channel_id == other.channel_id
    }
}

impl Upstream {
    /// Instantiate a new `Upstream`.
    /// Connect to the SV2 Upstream role (most typically a SV2 Pool). Initializes the
    /// `UpstreamConnection` with a channel to send and receive messages from the SV2 Upstream
    /// role and uses channels provided in the function arguments to send and receive messages
    /// from the `Downstream`.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::too_many_arguments))]
    pub async fn new(
        address: SocketAddr,
        authority_public_key: String,
        rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
        tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
        tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
        min_extranonce_size: u16,
        tx_sv2_extranonce: Sender<(ExtendedExtranonce, u32)>,
        tx_status: status::Sender,
        target: Arc<Mutex<Vec<u8>>>,
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

        let pub_key: codec_sv2::noise_sv2::formats::EncodedEd25519PublicKey = authority_public_key
            .try_into()
            .expect("Authority Public Key malformed in proxy-config");
        let initiator = Initiator::from_raw_k(*pub_key.into_inner().as_bytes()).unwrap();

        info!(
            "PROXY SERVER - ACCEPTING FROM UPSTREAM: {}",
            socket.peer_addr()?
        );

        // Channel to send and receive messages to the SV2 Upstream role
        let (receiver, sender) =
            Connection::new(socket, HandshakeRole::Initiator(initiator), 10).await;
        // Initialize `UpstreamConnection` with channel for SV2 Upstream role communication and
        // channel for downstream Translator Proxy communication
        let connection = UpstreamConnection { receiver, sender };

        Ok(Arc::new(Mutex::new(Self {
            connection,
            rx_sv2_submit_shares_ext,
            extranonce_prefix: None,
            tx_sv2_set_new_prev_hash,
            tx_sv2_new_ext_mining_job,
            channel_id: None,
            job_id: None,
            min_extranonce_size,
            tx_sv2_extranonce,
            tx_status,
            target,
        })))
    }

    /// Setups the connection with the SV2 Upstream role (most typically a SV2 Pool).
    pub async fn connect(
        self_: Arc<Mutex<Self>>,
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<'static, ()> {
        // Get the `SetupConnection` message with Mining Device information (currently hard coded)
        let setup_connection = Self::get_setup_connection_message(min_version, max_version)?;
        let mut connection = self_
            .safe_lock(|s| s.connection.clone())
            .map_err(|_e| PoisonLock)?;

        info!("Up: Sending: {:?}", &setup_connection);

        // Put the `SetupConnection` message in a `StdFrame` to be sent over the wire
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        // Send the `SetupConnection` frame to the SV2 Upstream role
        // Only one Upstream role is supported, panics if multiple connections are encountered
        connection.send(sv2_frame).await?;

        debug!("Sent SetupConnection to Upstream, waiting for response");
        // Wait for the SV2 Upstream to respond with either a `SetupConnectionSuccess` or a
        // `SetupConnectionError` inside a SV2 binary message frame
        let mut incoming: StdFrame = match connection.receiver.recv().await {
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

        // Send open channel request before returning
        let user_identity = "ABC".to_string().try_into()?;
        let min_extranonce_size = self_
            .safe_lock(|s| s.min_extranonce_size)
            .map_err(|_e| PoisonLock)?;
        let open_channel = Mining::OpenExtendedMiningChannel(OpenExtendedMiningChannel {
            request_id: 0,                       // TODO
            user_identity,                       // TODO
            nominal_hash_rate: 10_000_000_000.0, // TODO
            max_target: u256_from_int(u64::MAX), // TODO
            min_extranonce_size,
        });

        info!("Up: Sending: {:?}", &open_channel);

        let sv2_frame: StdFrame = Message::Mining(open_channel).try_into()?;
        connection.send(sv2_frame).await?;
        Ok(())
    }

    /// Parses the incoming SV2 message from the Upstream role and routes the message to the
    /// appropriate handler.
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let clone = self_.clone();
        let (
            tx_frame,
            tx_sv2_extranonce,
            tx_sv2_new_ext_mining_job,
            tx_sv2_set_new_prev_hash,
            recv,
            tx_status,
        ) = clone
            .safe_lock(|s| {
                (
                    s.connection.sender.clone(),
                    s.tx_sv2_extranonce.clone(),
                    s.tx_sv2_new_ext_mining_job.clone(),
                    s.tx_sv2_set_new_prev_hash.clone(),
                    s.connection.receiver.clone(),
                    s.tx_status.clone(),
                )
            })
            .map_err(|_| PoisonLock)?;

        task::spawn(async move {
            loop {
                // Waiting to receive a message from the SV2 Upstream role
                let incoming = handle_result!(tx_status, recv.recv().await);
                let mut incoming: StdFrame = handle_result!(tx_status, incoming.try_into());
                // On message receive, get the message type from the message header and get the
                // message payload
                let message_type = incoming.get_header().ok_or(crate::error::Error::FramingSv2(
                    framing_sv2::Error::ExpectedSv2Frame,
                ));

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
                    // No translation required, simply respond to SV2 pool w a SV2 message
                    Ok(SendTo::Respond(message_for_upstream)) => {
                        let message = Message::Mining(message_for_upstream);
                        info!("Up: Sending: {:?}", &message);

                        let frame: StdFrame = handle_result!(tx_status, message.try_into());
                        let frame: EitherFrame = handle_result!(tx_status, frame.try_into());

                        // Relay the response message to the Upstream role
                        handle_result!(
                            tx_status,
                            tx_frame.send(frame).await.map_err(|e| {
                                crate::Error::ChannelErrorSender(
                                    crate::error::ChannelSendError::General(e.to_string()),
                                )
                            })
                        );
                    }
                    // Does not send the messages anywhere, but instead handle them internally
                    Ok(SendTo::None(Some(m))) => {
                        match m {
                            Mining::OpenExtendedMiningChannelSuccess(m) => {
                                let prefix_len = dbg!(m.extranonce_prefix.len());
                                let extranonce: Extranonce =
                                    handle_result!(tx_status, m.extranonce_prefix.try_into());

                                // Create the extended extranonce that will be saved in bridge and
                                // it will be used to open downstream (sv1) channels
                                // range 0 is extranonce1 with upstream
                                // range 1 e range 2 are extranonce2 with upstream
                                //
                                // range0 and range1 are extranonce1 with downstream
                                // range2 is extranonce2 with downstream
                                let range_0 = 0..prefix_len;
                                let range_1 = prefix_len..prefix_len + crate::SELF_EXTRNONCE_LEN;
                                let range_2 = prefix_len + crate::SELF_EXTRNONCE_LEN
                                    ..prefix_len + m.extranonce_size as usize;
                                let extended = ExtendedExtranonce::from_upstream_extranonce(
                                    extranonce.clone(), range_0.clone(), range_1.clone(), range_2.clone(),
                                ).unwrap_or_else(|| panic!("Impossible to create a valid extended extranonce from {:?} {:?} {:?} {:?}", extranonce,range_0,range_1,range_2));

                                handle_result!(
                                    tx_status,
                                    tx_sv2_extranonce.send((extended, m.channel_id)).await
                                );
                            }
                            Mining::NewExtendedMiningJob(m) => {
                                debug!("parse_incoming Mining::NewExtendedMiningJob msg");
                                let job_id = m.job_id;
                                let res = self_
                                    .safe_lock(|s| {
                                        let _ = s.job_id.insert(job_id);
                                    })
                                    .map_err(|_e| PoisonLock);
                                handle_result!(tx_status, res);
                                handle_result!(tx_status, tx_sv2_new_ext_mining_job.send(m).await);
                            }
                            Mining::SetNewPrevHash(m) => {
                                debug!("parse_incoming Mining::SetNewPrevHash msg");
                                handle_result!(tx_status, tx_sv2_set_new_prev_hash.send(m).await);
                            }
                            // impossible state: handle_message_mining only returns
                            // the above 3 messages in the Ok(SendTo::None(Some(m))) case to be sent
                            // to the bridge for translation.
                            _ => panic!(),
                        }
                    }
                    Ok(SendTo::None(None)) => (),
                    // No need to handle impossible state just panic cause are impossible and we
                    // will never panic ;-) Verified: handle_message_mining only either panics,
                    // returns Ok(SendTo::None(None)) or Ok(SendTo::None(Some(m))), or returns Err
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
        });
        Ok(())
    }

    pub fn handle_submit(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let clone = self_.clone();
        let (tx_frame, receiver, tx_status) = clone
            .safe_lock(|s| {
                (
                    s.connection.sender.clone(),
                    s.rx_sv2_submit_shares_ext.clone(),
                    s.tx_status.clone(),
                )
            })
            .map_err(|_| PoisonLock)?;

        task::spawn(async move {
            loop {
                let mut sv2_submit: SubmitSharesExtended =
                    handle_result!(tx_status, receiver.recv().await);

                let channel_id = self_
                    .safe_lock(|s| {
                        s.channel_id.ok_or(crate::error::Error::RolesSv2Logic(
                            RolesLogicError::NotFoundChannelId,
                        ))
                    })
                    .map_err(|_e| PoisonLock);
                sv2_submit.channel_id =
                    handle_result!(tx_status, handle_result!(tx_status, channel_id));

                let job_id = self_
                    .safe_lock(|s| {
                        s.job_id.ok_or(crate::error::Error::RolesSv2Logic(
                            RolesLogicError::NoValidJob,
                        ))
                    })
                    .map_err(|_e| PoisonLock);
                sv2_submit.job_id = handle_result!(tx_status, handle_result!(tx_status, job_id));

                debug!("Up: Handling SubmitSharesExtended: {:?}", &sv2_submit);

                let message = Message::Mining(
                    roles_logic_sv2::parsers::Mining::SubmitSharesExtended(sv2_submit),
                );

                let frame: StdFrame = handle_result!(tx_status, message.try_into());
                // Doesnt actually send because of Braiins Pool issue that needs to be fixed
                info!("Up: Sending: {:?}", &frame);

                let frame: EitherFrame = handle_result!(tx_status, frame.try_into());
                handle_result!(
                    tx_status,
                    tx_frame
                        .send(frame)
                        .await
                        .map_err(|e| crate::Error::ChannelErrorSender(
                            crate::error::ChannelSendError::General(e.to_string())
                        ))
                );
            }
        });
        Ok(())
    }

    fn _is_contained_in_upstream_target(&self, _share: SubmitSharesExtended) -> bool {
        todo!()
    }

    /// Creates the `SetupConnection` message to setup the connection with the SV2 Upstream role.
    /// TODO: The Mining Device information is hard coded here, need to receive from Downstream
    /// instead.
    fn get_setup_connection_message(
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<'static, SetupConnection<'static>> {
        let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
        let vendor = String::new().try_into()?;
        let hardware_version = String::new().try_into()?;
        let firmware = String::new().try_into()?;
        let device_id = String::new().try_into()?;
        let flags = 0b0000_0000_0000_0000_0000_0000_0000_1110;
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
        false
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

    /// Handles the SV2 `OpenExtendedMiningChannelSuccess` message which provides important
    /// parameters including the `target` which is sent to the Downstream role in a SV1
    /// `mining.set_difficulty` message, and the extranonce values which is sent to the Downstream
    /// role in a SV1 `mining.subscribe` message response.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        if self.min_extranonce_size < m.extranonce_size {
            return Err(RolesLogicError::InvalidExtranonceSize(
                self.min_extranonce_size,
                m.extranonce_size,
            ));
        }
        self.target
            .safe_lock(|t| *t = m.target.to_vec())
            .map_err(|e| RolesLogicError::PoisonLock(e.to_string()))?;
        // Set the `min_extranonce_size` in accordance to the SV2 Pool
        self.min_extranonce_size = m.extranonce_size;

        info!("Up: Successfully Opened Extended Mining Channel");
        debug!("Up: Handling OpenExtendedMiningChannelSuccess: {:?}", &m);
        self.channel_id = Some(m.channel_id);
        self.extranonce_prefix = Some(m.extranonce_prefix.to_vec());
        let m = Mining::OpenExtendedMiningChannelSuccess(m.into_static());
        Ok(SendTo::None(Some(m)))
    }

    /// Handles the SV2 `OpenExtendedMiningChannelError` message (TODO).
    fn handle_open_mining_channel_error(
        &mut self,
        _: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `UpdateChannelError` message (TODO).
    fn handle_update_channel_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `CloseChannel` message (TODO).
    fn handle_close_channel(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `SetExtranoncePrefix` message (TODO).
    fn handle_set_extranonce_prefix(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `SubmitSharesSuccess` message.
    fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Up: Successfully Submitted Share");
        debug!("Up: Handling SubmitSharesSuccess: {:?}", &m);
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SubmitSharesError` message.
    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Up: Rejected Submitted Share");
        debug!("Up: Handling SubmitSharesError: {:?}", &m);
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
        m: roles_logic_sv2::mining_sv2::NewExtendedMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        debug!("Received NewExtendedMiningJob: {:?}", &m);
        info!("Is future job: {}\n", &m.future_job);

        if !m.version_rolling_allowed {
            warn!("VERSION ROLLING NOT ALLOWED IS A TODO");
            // todo!()
        }

        let message = Mining::NewExtendedMiningJob(m.into_static());

        info!("Up: New Extended Mining Job");
        debug!("Up: Handling NewExtendedMiningJob: {:?}", &message);

        Ok(SendTo::None(Some(message)))
    }

    /// Handles the SV2 `SetNewPrevHash` message which is used (along with the SV2
    /// `NewExtendedMiningJob` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_set_new_prev_hash(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        info!("Up: Set New Prev Hash");
        debug!("Up: Handling SetNewPrevHash: {:?}", &m);

        let message = Mining::SetNewPrevHash(m.into_static());
        Ok(SendTo::None(Some(message)))
    }

    /// Handles the SV2 `SetCustomMiningJobSuccess` message (TODO).
    fn handle_set_custom_mining_job_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        unimplemented!()
    }

    /// Handles the SV2 `SetCustomMiningJobError` message (TODO).
    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        unimplemented!()
    }

    /// Handles the SV2 `SetTarget` message which updates the Downstream role(s) target
    /// difficulty via the SV1 `mining.set_difficulty` message.
    fn handle_set_target(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        let m = m.into_static();

        info!("Up: Updating Target to: {:?}", &m.maximum_target);
        debug!("Up: Handling SetTarget: {:?}", &m);

        self.target
            .safe_lock(|t| *t = m.maximum_target.to_vec())
            .map_err(|e| RolesLogicError::PoisonLock(e.to_string()))?;
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `Reconnect` message (TODO).
    fn handle_reconnect(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::Reconnect,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        unimplemented!()
    }
}
