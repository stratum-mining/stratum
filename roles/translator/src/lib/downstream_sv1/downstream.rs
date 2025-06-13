//! ## Downstream SV1 Module: Downstream Connection Logic
//!
//! Defines the [`Downstream`] structure, which represents and manages an
//! individual connection from a downstream SV1 mining client.
//!
//! This module is responsible for:
//! - Accepting incoming TCP connections from SV1 miners.
//! - Handling the SV1 protocol handshake (`mining.subscribe`, `mining.authorize`,
//!   `mining.configure`).
//! - Receiving SV1 `mining.submit` messages from miners.
//! - Translating SV1 `mining.submit` messages into internal [`DownstreamMessages`] (specifically
//!   [`SubmitShareWithChannelId`]) and sending them to the Bridge.
//! - Receiving translated SV1 `mining.notify` messages from the Bridge and sending them to the
//!   connected miner.
//! - Managing the miner's extranonce1, extranonce2 size, and version rolling parameters.
//! - Implementing downstream-specific difficulty management logic, including tracking submitted
//!   shares and updating the miner's difficulty target.
//! - Implementing the necessary SV1 server traits ([`IsServer`]) and SV2 roles logic traits
//!   ([`IsMiningDownstream`], [`IsDownstream`]).

use crate::{
    config::{DownstreamDifficultyConfig, UpstreamDifficultyConfig},
    downstream_sv1,
    error::ProxyResult,
    status,
};
use async_channel::{bounded, Receiver, Sender};
use error_handling::handle_result;
use futures::{FutureExt, StreamExt};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::broadcast,
    task::AbortHandle,
};

use super::{kill, DownstreamMessages, SubmitShareWithChannelId, SUBSCRIBE_TIMEOUT_SECS};

use roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream},
    mining_sv2::Target,
    utils::{hash_rate_to_target, Mutex},
    vardiff::Vardiff,
    VardiffState,
};

use crate::error::Error;
use futures::select;
use tokio_util::codec::{FramedRead, LinesCodec};

use std::{net::SocketAddr, sync::Arc};
use tracing::{debug, info, warn};
use v1::{
    client_to_server::{self, Submit},
    json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be},
    IsServer,
};

/// The maximum allowed length for a single line (JSON-RPC message) received from an SV1 client.
const MAX_LINE_LENGTH: usize = 2_usize.pow(16);

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub struct Downstream {
    /// The unique identifier assigned to this downstream connection/channel.
    pub(super) connection_id: u32,
    /// List of authorized Downstream Mining Devices.
    authorized_names: Vec<String>,
    /// The extranonce1 value assigned to this downstream miner.
    extranonce1: Vec<u8>,
    /// `extranonce1` to be sent to the Downstream in the SV1 `mining.subscribe` message response.
    //extranonce1: Vec<u8>,
    //extranonce2_size: usize,
    /// Version rolling mask bits
    version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    version_rolling_min_bit: Option<HexU32Be>,
    /// Sends a SV1 `mining.submit` message received from the Downstream role to the `Bridge` for
    /// translation into a SV2 `SubmitSharesExtended`.
    tx_sv1_bridge: Sender<DownstreamMessages>,
    /// Sends message to the SV1 Downstream role.
    tx_outgoing: Sender<json_rpc::Message>,
    /// True if this is the first job received from `Upstream`.
    first_job_received: bool,
    /// The expected size of the extranonce2 field provided by the miner.
    extranonce2_len: usize,
    // Current Channel target
    pub target: Target,
    // Current channel hashrate
    pub hashrate: f32,
    /// Configuration and state for managing difficulty adjustments specific
    /// to this individual downstream miner.
    pub(super) difficulty_mgmt: Box<dyn Vardiff>,
    /// Configuration settings for the upstream channel's difficulty management.
    pub(super) upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
}

impl Downstream {
    // not huge fan of test specific code in codebase.
    #[cfg(test)]
    pub fn new(
        connection_id: u32,
        authorized_names: Vec<String>,
        extranonce1: Vec<u8>,
        version_rolling_mask: Option<HexU32Be>,
        version_rolling_min_bit: Option<HexU32Be>,
        tx_sv1_bridge: Sender<DownstreamMessages>,
        tx_outgoing: Sender<json_rpc::Message>,
        first_job_received: bool,
        extranonce2_len: usize,
        difficulty_mgmt: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
    ) -> Self {
        use roles_logic_sv2::utils::hash_rate_to_target;

        let hashrate = difficulty_mgmt.min_individual_miner_hashrate;
        let target = hash_rate_to_target(hashrate.into(), difficulty_mgmt.shares_per_minute.into())
            .unwrap()
            .into();
        let downstream_difficulty_state =
            VardiffState::new(difficulty_mgmt.shares_per_minute).unwrap();
        Downstream {
            connection_id,
            authorized_names,
            extranonce1,
            version_rolling_mask,
            version_rolling_min_bit,
            tx_sv1_bridge,
            tx_outgoing,
            first_job_received,
            extranonce2_len,
            hashrate,
            target,
            difficulty_mgmt: Box::new(downstream_difficulty_state),
            upstream_difficulty_config,
        }
    }
    /// Instantiates and manages a new handler for a single downstream SV1 client connection.
    ///
    /// This is the primary function called for each new incoming TCP stream from a miner.
    /// It sets up the communication channels, initializes the `Downstream` struct state,
    /// and spawns the necessary tasks to handle:
    /// 1. Reading incoming messages from the miner's socket.
    /// 2. Writing outgoing messages to the miner's socket.
    /// 3. Sending job notifications to the miner (handling initial job and subsequent updates).
    ///
    /// It uses shutdown channels to coordinate graceful termination of the spawned tasks.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_downstream(
        stream: TcpStream,
        connection_id: u32,
        tx_sv1_bridge: Sender<DownstreamMessages>,
        mut rx_sv1_notify: broadcast::Receiver<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        extranonce1: Vec<u8>,
        last_notify: Option<server_to_client::Notify<'static>>,
        extranonce2_len: usize,
        host: String,
        difficulty_config: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) {
        let hashrate = difficulty_config.min_individual_miner_hashrate;
        let target =
            hash_rate_to_target(hashrate.into(), difficulty_config.shares_per_minute.into())
                .expect("Couldn't convert hashrate to target")
                .into();

        let downstream_difficulty_state = VardiffState::new(difficulty_config.shares_per_minute)
            .expect("Couldn't initialize vardiff module");
        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, mut socket_writer) = stream.into_split();
        let (tx_outgoing, receiver_outgoing) = bounded(10);

        let downstream = Arc::new(Mutex::new(Downstream {
            connection_id,
            authorized_names: vec![],
            extranonce1,
            //extranonce1: extranonce1.to_vec(),
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            tx_sv1_bridge,
            tx_outgoing,
            first_job_received: false,
            extranonce2_len,
            hashrate,
            target,
            difficulty_mgmt: Box::new(downstream_difficulty_state),
            upstream_difficulty_config,
        }));
        let self_ = downstream.clone();

        let host_ = host.clone();
        // The shutdown channel is used local to the `Downstream::new_downstream()` function.
        // Each task is set broadcast a shutdown message at the end of their lifecycle with
        // `kill()`, and each task has a receiver to listen for the shutdown message. When a
        // shutdown message is received the task should `break` its loop. For any errors that should
        // shut a task down, we should `break` out of the loop, so that the `kill` function
        // can send the shutdown broadcast. EXTRA: The since all downstream tasks rely on
        // receiving messages with a future (either TCP recv or Receiver<_>) we use the
        // futures::select! macro to merge the receiving end of a task channels into a single loop
        // within the task
        let (tx_shutdown, rx_shutdown): (Sender<bool>, Receiver<bool>) = async_channel::bounded(3);

        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        let tx_status_reader = tx_status.clone();
        let task_collector_mining_device = task_collector.clone();
        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Depending on the
        // SV1 message received, a message response is sent directly back to the SV1 Downstream
        // role, or the message is sent upwards to the Bridge for translation into a SV2 message
        // and then sent to the SV2 Upstream role.
        let socket_reader_task = tokio::task::spawn(async move {
            let reader = BufReader::new(socket_reader);
            let mut messages =
                FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_LINE_LENGTH));
            loop {
                // Read message from SV1 Mining Device Client socket
                // On message receive, parse to `json_rpc:Message` and send to Upstream
                // `Translator.receive_downstream` via `sender_upstream` done in
                // `send_message_upstream`.
                select! {
                    res = messages.next().fuse() => {
                        match res {
                            Some(Ok(incoming)) => {
                                debug!("Receiving from Mining Device {}: {:?}", &host_, &incoming);
                                let incoming: json_rpc::Message = handle_result!(tx_status_reader, serde_json::from_str(&incoming));
                                // Handle what to do with message
                                // if let json_rpc::Message

                                // if message is Submit Shares update difficulty management
                                if let v1::Message::StandardRequest(standard_req) = incoming.clone() {
                                    if let Ok(Submit{..}) = standard_req.try_into() {
                                        handle_result!(tx_status_reader, Self::save_share(self_.clone()));
                                    }
                                }

                                let res = Self::handle_incoming_sv1(self_.clone(), incoming).await;
                                handle_result!(tx_status_reader, res);
                            }
                            Some(Err(_)) => {
                                handle_result!(tx_status_reader, Err(Error::Sv1MessageTooLong));
                            }
                            None => {
                                handle_result!(tx_status_reader, Err(
                                    std::io::Error::new(
                                        std::io::ErrorKind::ConnectionAborted,
                                        "Connection closed by client"
                                    )
                                ));
                            }
                        }
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                        break;
                    }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!("Downstream: Shutting down sv1 downstream reader");
        });
        let _ = task_collector_mining_device.safe_lock(|a| {
            a.push((
                socket_reader_task.abort_handle(),
                "socket_reader_task".to_string(),
            ))
        });

        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        let tx_status_writer = tx_status.clone();
        let host_ = host.clone();

        let task_collector_new_sv1_message_no_transl = task_collector.clone();
        // Task to receive SV1 message responses to SV1 messages that do NOT need translation.
        // These response messages are sent directly to the SV1 Downstream role.
        let socket_writer_task = tokio::task::spawn(async move {
            loop {
                select! {
                    res = receiver_outgoing.recv().fuse() => {
                        let to_send = handle_result!(tx_status_writer, res);
                        let to_send = match serde_json::to_string(&to_send) {
                            Ok(string) => format!("{}\n", string),
                            Err(_e) => {
                                debug!("\nDownstream: Bad SV1 server message\n");
                                break;
                            }
                        };
                        debug!("Sending to Mining Device: {} - {:?}", &host_, &to_send);
                        let res = socket_writer
                                    .write_all(to_send.as_bytes())
                                    .await;
                        handle_result!(tx_status_writer, res);
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                            break;
                        }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!(
                "Downstream: Shutting down sv1 downstream writer: {}",
                &host_
            );
        });
        let _ = task_collector_new_sv1_message_no_transl.safe_lock(|a| {
            a.push((
                socket_writer_task.abort_handle(),
                "socket_writer_task".to_string(),
            ))
        });

        let tx_status_notify = tx_status;
        let self_ = downstream.clone();

        let task_collector_notify_task = task_collector.clone();
        let notify_task = tokio::task::spawn(async move {
            let timeout_timer = std::time::Instant::now();
            let mut first_sent = false;
            loop {
                let is_a = match downstream.safe_lock(|d| !d.authorized_names.is_empty()) {
                    Ok(is_a) => is_a,
                    Err(_e) => {
                        debug!("\nDownstream: Poison Lock - authorized_names\n");
                        break;
                    }
                };
                if is_a && !first_sent && last_notify.is_some() {
                    let target = downstream
                        .safe_lock(|d| d.target.clone())
                        .expect("downstream target couldn't be computed");
                    // make sure the mining start time is initialized and reset number of shares
                    // submitted
                    handle_result!(
                        tx_status_notify,
                        Self::init_difficulty_management(downstream.clone()).await
                    );
                    let message =
                        handle_result!(tx_status_notify, Self::get_set_difficulty(target));
                    handle_result!(
                        tx_status_notify,
                        Downstream::send_message_downstream(downstream.clone(), message).await
                    );

                    let sv1_mining_notify_msg = last_notify.clone().unwrap();

                    let message: json_rpc::Message = sv1_mining_notify_msg.into();
                    handle_result!(
                        tx_status_notify,
                        Downstream::send_message_downstream(downstream.clone(), message).await
                    );
                    if let Err(_e) = downstream.clone().safe_lock(|s| {
                        s.first_job_received = true;
                    }) {
                        debug!("\nDownstream: Poison Lock - first_job_received\n");
                        break;
                    }
                    first_sent = true;
                } else if is_a {
                    // if hashrate has changed, update difficulty management, and send new
                    // mining.set_difficulty
                    select! {
                        res = rx_sv1_notify.recv().fuse() => {
                            // if hashrate has changed, update difficulty management, and send new mining.set_difficulty
                            handle_result!(tx_status_notify, Self::try_update_difficulty_settings(downstream.clone()).await);

                            let sv1_mining_notify_msg = handle_result!(tx_status_notify, res);
                            let message: json_rpc::Message = sv1_mining_notify_msg.clone().into();

                            handle_result!(tx_status_notify, Downstream::send_message_downstream(downstream.clone(), message).await);
                        },
                        _ = rx_shutdown.recv().fuse() => {
                                break;
                            }
                    };
                } else {
                    // timeout connection if miner does not send the authorize message after sending
                    // a subscribe
                    if timeout_timer.elapsed().as_secs() > SUBSCRIBE_TIMEOUT_SECS {
                        debug!(
                            "Downstream: miner.subscribe/miner.authorize TIMOUT for {}",
                            &host
                        );
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            let _ = Self::remove_miner_hashrate_from_channel(self_);
            kill(&tx_shutdown).await;
            warn!(
                "Downstream: Shutting down sv1 downstream job notifier for {}",
                &host
            );
        });

        let _ = task_collector_notify_task
            .safe_lock(|a| a.push((notify_task.abort_handle(), "notify_task".to_string())));
    }

    /// Accepts incoming TCP connections from SV1 mining clients on the configured address.
    ///
    /// For each new connection, it attempts to open a new SV1 downstream channel
    /// via the Bridge (`bridge.on_new_sv1_connection`). If successful, it spawns
    /// a new task using `Downstream::new_downstream` to handle
    /// the communication and logic for that specific miner connection.
    /// This method runs indefinitely, listening for and accepting new connections.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_connections(
        downstream_addr: SocketAddr,
        tx_sv1_submit: Sender<DownstreamMessages>,
        tx_mining_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        bridge: Arc<Mutex<crate::proxy::Bridge>>,
        downstream_difficulty_config: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) {
        let accept_connections = tokio::task::spawn({
            let task_collector = task_collector.clone();
            async move {
                let listener = TcpListener::bind(downstream_addr).await.unwrap();

                while let Ok((stream, _)) = listener.accept().await {
                    let expected_hash_rate =
                        downstream_difficulty_config.min_individual_miner_hashrate;
                    let open_sv1_downstream = bridge
                        .safe_lock(|s| s.on_new_sv1_connection(expected_hash_rate))
                        .unwrap();

                    let host = stream.peer_addr().unwrap().to_string();

                    match open_sv1_downstream {
                        Ok(opened) => {
                            info!("PROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}", host);
                            Downstream::new_downstream(
                                stream,
                                opened.channel_id,
                                tx_sv1_submit.clone(),
                                tx_mining_notify.subscribe(),
                                tx_status.listener_to_connection(),
                                opened.extranonce,
                                opened.last_notify,
                                opened.extranonce2_len as usize,
                                host,
                                downstream_difficulty_config.clone(),
                                upstream_difficulty_config.clone(),
                                task_collector.clone(),
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to create a new downstream connection: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        });
        let _ = task_collector.safe_lock(|a| {
            a.push((
                accept_connections.abort_handle(),
                "accept_connections".to_string(),
            ))
        });
    }

    /// Handles incoming SV1 JSON-RPC messages from a downstream miner.
    ///
    /// This function acts as the entry point for processing messages received
    /// from a miner after framing. It uses the `IsServer` trait implementation
    /// to parse and handle standard SV1 requests (`mining.subscribe`, `mining.authorize`,
    /// `mining.submit`, `mining.configure`). Depending on the message type, it may generate a
    /// direct SV1 response to be sent back to the miner or indicate that the message needs to
    /// be translated and sent upstream (handled elsewhere, typically by the Bridge).
    async fn handle_incoming_sv1(
        self_: Arc<Mutex<Self>>,
        message_sv1: json_rpc::Message,
    ) -> Result<(), super::super::error::Error<'static>> {
        // `handle_message` in `IsServer` trait + calls `handle_request`
        // TODO: Map err from V1Error to Error::V1Error
        let response = self_.safe_lock(|s| s.handle_message(message_sv1)).unwrap();
        match response {
            Ok(res) => {
                if let Some(r) = res {
                    // If some response is received, indicates no messages translation is needed
                    // and response should be sent directly to the SV1 Downstream. Otherwise,
                    // message will be sent to the upstream Translator to be translated to SV2 and
                    // forwarded to the `Upstream`
                    // let sender = self_.safe_lock(|s| s.connection.sender_upstream)
                    if let Err(e) = Self::send_message_downstream(self_, r.into()).await {
                        return Err(e.into());
                    }
                    Ok(())
                } else {
                    // If None response is received, indicates this SV1 message received from the
                    // Downstream MD is passed to the `Translator` for translation into SV2
                    Ok(())
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Sends a SV1 JSON-RPC message to the downstream miner's socket writer task.
    ///
    /// This method is used to send response messages or notifications (like
    /// `mining.notify` or `mining.set_difficulty`) to the connected miner.
    /// The message is sent over the internal `tx_outgoing` channel, which is
    /// read by the socket writer task responsible for serializing and writing
    /// the message to the TCP stream.
    pub(super) async fn send_message_downstream(
        self_: Arc<Mutex<Self>>,
        response: json_rpc::Message,
    ) -> Result<(), async_channel::SendError<v1::Message>> {
        let sender = self_.safe_lock(|s| s.tx_outgoing.clone()).unwrap();
        debug!("To DOWN: {:?}", response);
        sender.send(response).await
    }

    /// Sends a message originating from the downstream handler to the Bridge.
    ///
    /// This function is used to forward messages that require translation or
    /// central processing by the Bridge, such as `SubmitShares` or `SetDownstreamTarget`.
    /// The message is sent over the internal `tx_sv1_bridge` channel.
    pub(super) async fn send_message_upstream(
        self_: Arc<Mutex<Self>>,
        msg: DownstreamMessages,
    ) -> ProxyResult<'static, ()> {
        let sender = self_.safe_lock(|s| s.tx_sv1_bridge.clone()).unwrap();
        debug!("To Bridge: {:?}", msg);
        let _ = sender.send(msg).await;
        Ok(())
    }
}

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer<'static> for Downstream {
    /// Handles the incoming SV1 `mining.configure` message.
    ///
    /// This message is received after `mining.subscribe` and `mining.authorize`.
    /// It allows the miner to negotiate capabilities, particularly regarding
    /// version rolling. This method processes the version rolling mask and
    /// minimum bit count provided by the client.
    ///
    /// Returns a tuple containing:
    /// 1. `Option<server_to_client::VersionRollingParams>`: The version rolling parameters
    ///    negotiated by the server (proxy).
    /// 2. `Option<bool>`: A boolean indicating whether the server (proxy) supports version rolling
    ///    (always `Some(false)` for TProxy according to the SV1 spec when not supporting work
    ///    selection).
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        info!("Down: Configuring");
        debug!("Down: Handling mining.configure: {:?}", &request);

        // TODO 0x1FFFE000 should be configured
        // = 11111111111111110000000000000
        // this is a reasonable default as it allows all 16 version bits to be used
        // If the tproxy/pool needs to use some version bits this needs to be configurable
        // so upstreams can negotiate with downstreams. When that happens this should consider
        // the min_bit_count in the mining.configure message
        self.version_rolling_mask = request
            .version_rolling_mask()
            .map(|mask| HexU32Be(mask & 0x1FFFE000));
        self.version_rolling_min_bit = request.version_rolling_min_bit_count();

        debug!(
            "Negotiated version_rolling_mask is {:?}",
            self.version_rolling_mask
        );
        (
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap_or(HexU32Be(0)),
                self.version_rolling_min_bit.clone().unwrap_or(HexU32Be(0)),
            ).expect("Version mask invalid, automatic version mask selection not supported, please change it in carte::downstream_sv1::mod.rs")),
            Some(false),
        )
    }

    /// Handles the incoming SV1 `mining.subscribe` message.
    ///
    /// This is typically the first message received from a new client. In the SV1
    /// protocol, it's used to subscribe to job notifications and receive session
    /// details like extranonce1 and extranonce2 size. This method acknowledges the subscription and
    /// provides the necessary details derived from the upstream SV2 connection (extranonce1 and
    /// extranonce2 size). It also provides subscription IDs for the
    /// `mining.set_difficulty` and `mining.notify` methods.
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        info!("Down: Subscribing");
        debug!("Down: Handling mining.subscribe: {:?}", &request);

        let set_difficulty_sub = (
            "mining.set_difficulty".to_string(),
            downstream_sv1::new_subscription_id(),
        );
        let notify_sub = (
            "mining.notify".to_string(),
            "ae6812eb4cd7735a302a8a9dd95cf71f".to_string(),
        );

        vec![set_difficulty_sub, notify_sub]
    }

    /// Any numbers of workers may be authorized at any time during the session. In this way, a
    /// large number of independent Mining Devices can be handled with a single SV1 connection.
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool {
        info!("Down: Authorizing");
        debug!("Down: Handling mining.authorize: {:?}", &request);
        true
    }

    /// Handles the incoming SV1 `mining.submit` message.
    ///
    /// This message is sent by the miner when they find a share that meets
    /// their current difficulty target. It contains the job ID, ntime, nonce,
    /// and extranonce2.
    ///
    /// This method processes the submitted share, potentially validates it
    /// against the downstream target (although this might happen in the Bridge
    /// or difficulty management logic), translates it into a
    /// [`SubmitShareWithChannelId`], and sends it to the Bridge for
    /// translation to SV2 and forwarding upstream if it meets the upstream target.
    fn handle_submit(&self, request: &client_to_server::Submit<'static>) -> bool {
        info!("Down: Submitting Share {:?}", request);
        debug!("Down: Handling mining.submit: {:?}", &request);

        // TODO: Check if receiving valid shares by adding diff field to Downstream

        let to_send = SubmitShareWithChannelId {
            channel_id: self.connection_id,
            share: request.clone(),
            extranonce: self.extranonce1.clone(),
            extranonce2_len: self.extranonce2_len,
            version_rolling_mask: self.version_rolling_mask.clone(),
        };

        self.tx_sv1_bridge
            .try_send(DownstreamMessages::SubmitShares(to_send))
            .unwrap();

        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    /// Checks if a Downstream role is authorized.
    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_names.contains(&name.to_string())
    }

    /// Authorizes a Downstream role.
    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string());
    }

    /// Sets the `extranonce1` field sent in the SV1 `mining.notify` message to the value specified
    /// by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce1(
        &mut self,
        _extranonce1: Option<Extranonce<'static>>,
    ) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Returns the `Downstream`'s `extranonce1` value.
    fn extranonce1(&self) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Sets the `extranonce2_size` field sent in the SV1 `mining.notify` message to the value
    /// specified by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_len
    }

    /// Returns the `Downstream`'s `extranonce2_size` value.
    fn extranonce2_size(&self) -> usize {
        self.extranonce2_len
    }

    /// Returns the version rolling mask.
    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    /// Sets the version rolling mask.
    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    /// Sets the minimum version rolling bit.
    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, v1::error::Error> {
        unreachable!()
    }
}

// Can we remove this?
impl IsMiningDownstream for Downstream {}
// Can we remove this?
impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use binary_sv2::U256;
    use roles_logic_sv2::mining_sv2::Target;

    use super::*;

    #[test]
    fn gets_difficulty_from_target() {
        let target = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 255, 127,
            0, 0, 0, 0, 0,
        ];
        let target_u256 = U256::Owned(target);
        let target = Target::from(target_u256);
        let actual = Downstream::difficulty_from_target(target).unwrap();
        let expect = 512.0;
        assert_eq!(actual, expect);
    }
}
