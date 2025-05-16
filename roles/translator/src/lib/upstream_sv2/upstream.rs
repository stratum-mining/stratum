//! ## Upstream SV2 Module: Upstream Connection Logic
//!
//! Defines the [`Upstream`] structure, which represents and manages the connection
//! to a single upstream role.
//!
//! This module is responsible for:
//! - Establishing and maintaining the network connection to the upstream role.
//! - Performing the SV2 handshake and opening mining channels.
//! - Sending translated SV2 `SubmitSharesExtended` messages received from the Bridge to the
//!   upstream pool.
//! - Receiving SV2 job messages (`SetNewPrevHash`, `NewExtendedMiningJob`, etc.) from the upstream
//!   pool and forwarding them to the Bridge for translation.
//! - Handling various SV2 messages related to connection setup, channel management, and mining
//!   operations.
//! - Managing difficulty updates for the upstream channel based on aggregated hashrate from
//!   downstream miners.
//! - Implementing the necessary SV2 roles logic traits (`IsUpstream`, `IsMiningUpstream`,
//!   `ParseCommonMessagesFromUpstream`, `ParseMiningMessagesFromUpstream`).

use crate::{
    config::UpstreamDifficultyConfig,
    downstream_sv1::Downstream,
    error::{
        Error::{CodecNoise, InvalidExtranonce, PoisonLock, UpstreamIncoming},
        ProxyResult,
    },
    status,
    upstream_sv2::{EitherFrame, Message, StdFrame, UpstreamConnection},
};
use async_channel::{Receiver, Sender};
use binary_sv2::u256_from_int;
use codec_sv2::{HandshakeRole, Initiator};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection::Connection;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::{
        common::{ParseCommonMessagesFromUpstream, SendTo as SendToCommon},
        mining::{ParseMiningMessagesFromUpstream, SendTo},
    },
    mining_sv2::{
        ExtendedExtranonce, Extranonce, NewExtendedMiningJob, OpenExtendedMiningChannel,
        SetNewPrevHash, SubmitSharesExtended,
    },
    parsers::Mining,
    utils::Mutex,
    Error as RolesLogicError,
    Error::NoUpstreamsConnected,
};
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::{
    net::TcpStream,
    task::AbortHandle,
    time::{sleep, Duration},
};
use tracing::{debug, error, info, warn};

use roles_logic_sv2::{
    common_messages_sv2::Reconnect, handlers::mining::SupportedChannelTypes,
    mining_sv2::SetGroupChannel,
};
use stratum_common::bitcoin::BlockHash;

/// Atomic boolean flag used for synchronization between receiving a new job
/// and handling a new previous hash. Indicates whether a `NewExtendedMiningJob`
/// has been fully processed.
pub static IS_NEW_JOB_HANDLED: AtomicBool = AtomicBool::new(true);
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

/// Represents a connection to a single SV2 Upstream role.
///
/// This struct holds the state and communication channels necessary to interact
/// with the upstream server, including sending share submissions, receiving job
/// templates, and managing the SV2 protocol handshake and channel lifecycle.
#[derive(Debug, Clone)]
pub struct Upstream {
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection,
    /// e.g. it is used for broadcasting new jobs by the `NewExtendedMiningJob` message.
    pub(super) channel_id: Option<u32>,
    /// Identifier of the job as provided by the `NewExtendedMiningJob` message.
    job_id: Option<u32>,
    /// Identifier of the job as provided by the ` SetCustomMiningJobSucces` message
    last_job_id: Option<u32>,
    /// Bytes used as implicit first part of `extranonce`.
    extranonce_prefix: Option<Vec<u8>>,
    /// Represents a connection to a SV2 Upstream role.
    pub(super) connection: UpstreamConnection,
    /// Receives SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages.
    /// Translated by and sent from the `Bridge`.
    rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
    /// Sends SV2 `SetNewPrevHash` messages to be translated (along with SV2 `NewExtendedMiningJob`
    /// messages) into SV1 `mining.notify` messages. Received and translated by the `Bridge`.
    tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
    /// Sends SV2 `NewExtendedMiningJob` messages to be translated (along with SV2 `SetNewPrevHash`
    /// messages) into SV1 `mining.notify` messages. Received and translated by the `Bridge`.
    tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
    /// Sends the extranonce1 and the channel id received in the SV2
    /// `OpenExtendedMiningChannelSuccess` message to be used by the `Downstream` and sent to
    /// the Downstream role in a SV2 `mining.subscribe` response message. Passed to the
    /// `Downstream` on connection creation.
    tx_sv2_extranonce: Sender<(ExtendedExtranonce, u32)>,
    /// This allows the upstream threads to be able to communicate back to the main thread its
    /// current status.
    tx_status: status::Sender,
    /// The first `target` is received by the Upstream role in the SV2
    /// `OpenExtendedMiningChannelSuccess` message, then updated periodically via SV2 `SetTarget`
    /// messages. Passed to the `Downstream` on connection creation and sent to the Downstream role
    /// via the SV1 `mining.set_difficulty` message.
    target: Arc<Mutex<Vec<u8>>>,
    /// Tracks the most recently sent nominal hashrate to prevent unnecessary updates.
    pub last_sent_hashrate: Option<f32>,
    /// Minimum `extranonce2` size. Initially requested in the `proxy-config.toml`, and ultimately
    /// set by the SV2 Upstream via the SV2 `OpenExtendedMiningChannelSuccess` message.
    pub min_extranonce_size: u16,
    /// The size of the extranonce1 provided by the upstream role.
    pub upstream_extranonce1_size: usize,
    // values used to update the channel with the correct nominal hashrate.
    // each Downstream instance will add and subtract their hashrates as needed
    // and the upstream just needs to occasionally check if it has changed more than
    // than the configured percentage
    pub(super) difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
    task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
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
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        address: SocketAddr,
        authority_public_key: Secp256k1PublicKey,
        rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
        tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
        tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
        min_extranonce_size: u16,
        tx_sv2_extranonce: Sender<(ExtendedExtranonce, u32)>,
        tx_status: status::Sender,
        target: Arc<Mutex<Vec<u8>>>,
        difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
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

                    sleep(Duration::from_secs(5)).await;
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
            .unwrap();
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
            last_job_id: None,
            min_extranonce_size,
            upstream_extranonce1_size: 16, /* 16 is the default since that is the only value the
                                            * pool supports currently */
            tx_sv2_extranonce,
            tx_status,
            target,
            last_sent_hashrate: None,
            difficulty_config,
            task_collector,
        })))
    }

    /// Performs the SV2 connection setup handshake with the Upstream role.
    ///
    /// Sends a `SetupConnection` message specifying supported protocol versions
    /// and flags. Waits for the upstream to respond with either `SetupConnectionSuccess`
    /// or `SetupConnectionError`.Upon successful setup, it then sends an
    /// `OpenExtendedMiningChannel` request to establish a mining channel, including the
    /// negotiated minimum extranonce size and initial nominal hashrate.
    pub async fn connect(
        self_: Arc<Mutex<Self>>,
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<'static, ()> {
        // Get the `SetupConnection` message with Mining Device information (currently hard coded)
        let setup_connection = Self::get_setup_connection_message(min_version, max_version, false)?;
        let mut connection = self_.safe_lock(|s| s.connection.clone())?;

        // Put the `SetupConnection` message in a `StdFrame` to be sent over the wire
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        // Send the `SetupConnection` frame to the SV2 Upstream role
        // Only one Upstream role is supported, panics if multiple connections are encountered
        connection.send(sv2_frame).await?;

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

        // Send open channel request before returning
        let nominal_hash_rate = self_.safe_lock(|u| {
            u.difficulty_config
                .safe_lock(|c| c.channel_nominal_hashrate)
                .map_err(|_e| PoisonLock)
        })??;
        let user_identity = "ABC".to_string().try_into()?;

        // Get the min_extranonce_size from the instance
        let min_extranonce_size = self_.safe_lock(|u| u.min_extranonce_size)?;

        let open_channel = Mining::OpenExtendedMiningChannel(OpenExtendedMiningChannel {
            request_id: 0, // TODO
            user_identity, // TODO
            nominal_hash_rate,
            max_target: u256_from_int(u64::MAX), // TODO
            min_extranonce_size,
        });

        // reset channel hashrate so downstreams can manage from now on out
        self_.safe_lock(|u| {
            u.difficulty_config
                .safe_lock(|d| d.channel_nominal_hashrate = 0.0)
                .map_err(|_e| PoisonLock)
        })??;

        let sv2_frame: StdFrame = Message::Mining(open_channel).try_into()?;
        connection.send(sv2_frame).await?;

        Ok(())
    }

    /// Spawns tasks to handle incoming SV2 messages from the Upstream role.
    ///
    /// This method creates two main asynchronous tasks:
    /// 1. A task to handle incoming SV2 frames, parsing them, routing them to the appropriate
    ///    message handlers (`handle_message_mining`), and forwarding translated messages to the
    ///    Bridge or responding directly to the upstream if necessary.
    /// 2. A task to periodically check and update the nominal hashrate sent to the upstream based
    ///    on th
    #[allow(clippy::result_large_err)]
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let clone = self_.clone();
        let task_collector = self_.safe_lock(|s| s.task_collector.clone()).unwrap();
        let collector1 = task_collector.clone();
        let collector2 = task_collector.clone();
        let (
            tx_frame,
            tx_sv2_extranonce,
            tx_sv2_new_ext_mining_job,
            tx_sv2_set_new_prev_hash,
            recv,
            tx_status,
        ) = clone.safe_lock(|s| {
            (
                s.connection.sender.clone(),
                s.tx_sv2_extranonce.clone(),
                s.tx_sv2_new_ext_mining_job.clone(),
                s.tx_sv2_set_new_prev_hash.clone(),
                s.connection.receiver.clone(),
                s.tx_status.clone(),
            )
        })?;
        {
            let self_ = self_.clone();
            let tx_status = tx_status.clone();
            let start_diff_management = tokio::task::spawn(async move {
                // No need to start diff management immediatly
                sleep(Duration::from_secs(10)).await;
                loop {
                    handle_result!(tx_status, Self::try_update_hashrate(self_.clone()).await);
                }
            });
            let _ = collector1.safe_lock(|a| {
                a.push((
                    start_diff_management.abort_handle(),
                    "start_diff_management".to_string(),
                ))
            });
        }

        let parse_incoming = tokio::task::spawn(async move {
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
                    // No translation required, simply respond to SV2 pool w a SV2 message
                    Ok(SendTo::Respond(message_for_upstream)) => {
                        let message = Message::Mining(message_for_upstream);

                        let frame: StdFrame = handle_result!(tx_status, message.try_into());
                        let frame: EitherFrame = frame.into();

                        // Relay the response message to the Upstream role
                        handle_result!(tx_status, tx_frame.send(frame).await);
                    }
                    // Does not send the messages anywhere, but instead handle them internally
                    Ok(SendTo::None(Some(m))) => {
                        match m {
                            Mining::OpenExtendedMiningChannelSuccess(m) => {
                                let prefix_len = m.extranonce_prefix.len();
                                // update upstream_extranonce1_size for tracking
                                let miner_extranonce2_size = self_
                                    .safe_lock(|u| {
                                        u.upstream_extranonce1_size = prefix_len;
                                        u.min_extranonce_size as usize
                                    })
                                    .map_err(|_e| PoisonLock);
                                let miner_extranonce2_size =
                                    handle_result!(tx_status, miner_extranonce2_size);
                                let extranonce_prefix: Extranonce = m.extranonce_prefix.into();
                                // Create the extended extranonce that will be saved in bridge and
                                // it will be used to open downstream (sv1) channels
                                // range 0 is the extranonce1 from upstream
                                // range 1 is the extranonce1 added by the tproxy
                                // range 2 is the extranonce2 used by the miner for rolling
                                // range 0 + range 1 is the extranonce1 sent to the miner
                                let tproxy_e1_len = super::super::utils::proxy_extranonce1_len(
                                    m.extranonce_size as usize,
                                    miner_extranonce2_size,
                                );
                                let range_0 = 0..prefix_len; // upstream extranonce1
                                let range_1 = prefix_len..prefix_len + tproxy_e1_len; // downstream extranonce1
                                let range_2 = prefix_len + tproxy_e1_len
                                    ..prefix_len + m.extranonce_size as usize; // extranonce2
                                let extended = handle_result!(tx_status, ExtendedExtranonce::from_upstream_extranonce(
                                    extranonce_prefix.clone(), range_0.clone(), range_1.clone(), range_2.clone(),
                                ).map_err(|err| InvalidExtranonce(format!("Impossible to create a valid extended extranonce from {:?} {:?} {:?} {:?}: {:?}",
                                    extranonce_prefix, range_0, range_1, range_2, err))));
                                handle_result!(
                                    tx_status,
                                    tx_sv2_extranonce.send((extended, m.channel_id)).await
                                );
                            }
                            Mining::NewExtendedMiningJob(m) => {
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
                                handle_result!(tx_status, tx_sv2_set_new_prev_hash.send(m).await);
                            }
                            Mining::CloseChannel(_m) => {
                                error!("Received Mining::CloseChannel msg from upstream!");
                                handle_result!(tx_status, Err(NoUpstreamsConnected));
                            }
                            Mining::OpenMiningChannelError(_)
                            | Mining::UpdateChannelError(_)
                            | Mining::SubmitSharesError(_)
                            | Mining::SetCustomMiningJobError(_) => {
                                error!("parse_incoming SV2 protocol error Message");
                                handle_result!(tx_status, Err(m));
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
        let _ = collector2
            .safe_lock(|a| a.push((parse_incoming.abort_handle(), "parse_incoming".to_string())));

        Ok(())
    }

    // Retrieves the current job ID.
    //
    // If work selection is enabled (which it is not for a Translator Proxy),
    // it would return the last `SetCustomMiningJobSuccess` job ID. If
    // work selection is disabled, it returns the job ID from the last
    // `NewExtendedMiningJob`
    #[allow(clippy::result_large_err)]
    fn get_job_id(
        self_: &Arc<Mutex<Self>>,
    ) -> Result<Result<u32, super::super::error::Error<'static>>, super::super::error::Error<'static>>
    {
        self_
            .safe_lock(|s| {
                if s.is_work_selection_enabled() {
                    s.last_job_id
                        .ok_or(super::super::error::Error::RolesSv2Logic(
                            RolesLogicError::NoValidTranslatorJob,
                        ))
                } else {
                    s.job_id.ok_or(super::super::error::Error::RolesSv2Logic(
                        RolesLogicError::NoValidJob,
                    ))
                }
            })
            .map_err(|_e| PoisonLock)
    }

    /// Spawns a task to handle outgoing `SubmitSharesExtended` messages.
    ///
    /// This task continuously receives `SubmitSharesExtended` messages from the
    /// `rx_sv2_submit_shares_ext` channel (populated by the Bridge). It updates
    /// the channel ID and job ID in the submit message (ensuring they match
    /// the current upstream channel details), encodes the message into an SV2 frame,
    /// and sends it to the upstream server.
    #[allow(clippy::result_large_err)]
    pub fn handle_submit(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let task_collector = self_.safe_lock(|s| s.task_collector.clone()).unwrap();
        let clone = self_.clone();
        let (tx_frame, receiver, tx_status) = clone.safe_lock(|s| {
            (
                s.connection.sender.clone(),
                s.rx_sv2_submit_shares_ext.clone(),
                s.tx_status.clone(),
            )
        })?;

        let handle_submit = tokio::task::spawn(async move {
            loop {
                let mut sv2_submit: SubmitSharesExtended =
                    handle_result!(tx_status, receiver.recv().await);

                let channel_id = self_
                    .safe_lock(|s| {
                        s.channel_id
                            .ok_or(super::super::error::Error::RolesSv2Logic(
                                RolesLogicError::NotFoundChannelId,
                            ))
                    })
                    .map_err(|_e| PoisonLock);
                sv2_submit.channel_id =
                    handle_result!(tx_status, handle_result!(tx_status, channel_id));
                let job_id = Self::get_job_id(&self_);
                sv2_submit.job_id = handle_result!(tx_status, handle_result!(tx_status, job_id));

                let message = Message::Mining(
                    roles_logic_sv2::parsers::Mining::SubmitSharesExtended(sv2_submit),
                );

                let frame: StdFrame = handle_result!(tx_status, message.try_into());
                // Doesnt actually send because of Braiins Pool issue that needs to be fixed

                let frame: EitherFrame = frame.into();
                handle_result!(tx_status, tx_frame.send(frame).await);
            }
        });
        let _ = task_collector
            .safe_lock(|a| a.push((handle_submit.abort_handle(), "handle_submit".to_string())));

        Ok(())
    }

    // Unimplemented method to check if a submitted share is contained within the upstream target.
    //
    // This method is currently unimplemented (`todo!()`). Its purpose would be
    // to validate a share against the target set by the upstream pool.
    fn _is_contained_in_upstream_target(&self, _share: SubmitSharesExtended) -> bool {
        todo!()
    }

    // Creates the initial `SetupConnection` message for the SV2 handshake.
    //
    // This message contains information about the proxy acting as a mining device,
    // including supported protocol versions, flags, and hardcoded endpoint details.
    //
    // TODO: The Mining Device information is currently hardcoded. It should ideally
    // be configurable or derived from the downstream connections.
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

// Can be removed?
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

// Can be removed?
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
    // Handles the SV2 `SetupConnectionSuccess` message received from the upstream.
    //
    // Returns `Ok(SendToCommon::None(None))` as this message is handled internally
    // and does not require a direct response or forwarding.
    fn handle_setup_connection_success(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendToCommon, RolesLogicError> {
        info!(
            "Received `SetupConnectionSuccess`: version={}, flags={:b}",
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
    /// Returns the type of channel used between this proxy and the SV2 Upstream.
    /// For a Translator Proxy, this is always `Extended`.
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    /// Indicates whether work selection is enabled for this upstream connection.
    /// For a Translator Proxy, work selection is handled by the upstream pool,
    /// so this method always returns `false`.
    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    /// The SV2 `OpenStandardMiningChannelSuccess` message is NOT handled because it is NOT used
    /// for the Translator Proxy as only `Extended` channels are used between the SV1/SV2 Translator
    /// Proxy and the SV2 Upstream role.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// Handles the SV2 `OpenExtendedMiningChannelSuccess` message.
    ///
    /// This message is received after requesting to open an extended mining channel.
    /// It provides the assigned `channel_id`, the extranonce prefix, the initial
    /// mining `target`, and the expected `extranonce_size`. It stores the `channel_id` and
    /// `extranonce_prefix`, updates the shared `target`, and prepares the extranonce
    /// information (including calculating the size for the TProxy's added extranonce1) to be
    /// sent to the Downstream handler for use with SV1 clients.
    ///
    /// Returns `Ok(SendTo<Downstream>::None(Some(Mining::OpenExtendedMiningChannelSuccess)))`
    /// to indicate that the message has been handled internally and should be
    /// forwarded to the Bridge.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}",
            m.request_id, m.channel_id
        );
        debug!("OpenStandardMiningChannelSuccess: {:?}", m);
        let tproxy_e1_len = super::super::utils::proxy_extranonce1_len(
            m.extranonce_size as usize,
            self.min_extranonce_size.into(),
        ) as u16;
        if self.min_extranonce_size + tproxy_e1_len < m.extranonce_size {
            return Err(RolesLogicError::InvalidExtranonceSize(
                self.min_extranonce_size,
                m.extranonce_size,
            ));
        }
        self.target.safe_lock(|t| *t = m.target.to_vec())?;

        info!("Up: Successfully Opened Extended Mining Channel");
        self.channel_id = Some(m.channel_id);
        self.extranonce_prefix = Some(m.extranonce_prefix.to_vec());
        let m = Mining::OpenExtendedMiningChannelSuccess(m.into_static());
        Ok(SendTo::None(Some(m)))
    }

    /// Handles the SV2 `OpenExtendedMiningChannelError` message (TODO).
    fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received OpenExtendedMiningChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(Some(Mining::OpenMiningChannelError(
            m.as_static(),
        ))))
    }

    /// Handles the SV2 `UpdateChannelError` message (TODO).
    fn handle_update_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received UpdateChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(Some(Mining::UpdateChannelError(
            m.as_static(),
        ))))
    }

    /// Handles the SV2 `CloseChannel` message (TODO).
    fn handle_close_channel(
        &mut self,
        m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received CloseChannel for channel id: {}", m.channel_id);
        Ok(SendTo::None(Some(Mining::CloseChannel(m.as_static()))))
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
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received SubmitSharesSuccess");
        debug!("SubmitSharesSuccess: {:?}", m);
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SubmitSharesError` message.
    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received SubmitSharesError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
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
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received new extended mining job for channel id: {} with job id: {} is_future: {}",
            m.channel_id,
            m.job_id,
            m.is_future()
        );
        debug!("NewExtendedMiningJob: {:?}", m);
        if self.is_work_selection_enabled() {
            Ok(SendTo::None(None))
        } else {
            IS_NEW_JOB_HANDLED.store(false, std::sync::atomic::Ordering::SeqCst);
            if !m.version_rolling_allowed {
                warn!("VERSION ROLLING NOT ALLOWED IS A TODO");
                // todo!()
            }

            let message = Mining::NewExtendedMiningJob(m.into_static());

            Ok(SendTo::None(Some(message)))
        }
    }

    /// Handles the SV2 `SetNewPrevHash` message which is used (along with the SV2
    /// `NewExtendedMiningJob` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_set_new_prev_hash(
        &mut self,
        m: SetNewPrevHash,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received SetNewPrevHash channel id: {}, job id: {}",
            m.channel_id, m.job_id
        );
        debug!("SetNewPrevHash: {:?}", m);
        if self.is_work_selection_enabled() {
            Ok(SendTo::None(None))
        } else {
            let message = Mining::SetNewPrevHash(m.into_static());
            Ok(SendTo::None(Some(message)))
        }
    }

    /// Handles the SV2 `SetCustomMiningJobSuccess` message (TODO).
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received SetCustomMiningJobSuccess for channel id: {} for job id: {}",
            m.channel_id, m.job_id
        );
        debug!("SetCustomMiningJobSuccess: {:?}", m);
        self.last_job_id = Some(m.job_id);
        Ok(SendTo::None(None))
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
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received SetTarget for channel id: {}", m.channel_id);
        debug!("SetTarget: {:?}", m);
        let m = m.into_static();
        self.target.safe_lock(|t| *t = m.maximum_target.to_vec())?;
        Ok(SendTo::None(None))
    }

    fn handle_set_group_channel(
        &mut self,
        _m: SetGroupChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        todo!()
    }
}
