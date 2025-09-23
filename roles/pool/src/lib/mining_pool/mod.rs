//! ## Mining Pool
//!
//! The core functionality for a mining pool, including
//! management of downstream miners, job templates, and solution submissions.
//!
//! The [`Pool`] struct maintains the state of active downstream connections, handles
//! the acceptance of new connections, distributes new mining jobs, and processes
//! solutions submitted by miners.
//!
//! The [`Downstream`] struct represents a single connected miner, managing its
//! communication channels, incoming messages, and assigned mining jobs.
//!
//! Key functionalities include:
//! - Secure handshake and connection setup for downstream miners
//! - Broadcasting new mining templates and previous hash updates
//! - Handling mining shares submitted by downstreams
//!
//! Components:
//! - `Pool`: Central manager for all downstream connections and job updates.
//! - `Downstream`: Represents a miner and handles its connection lifecycle.
//! - `PoolChannelFactory`: Manages the creation and tracking of mining channels.
use super::{
    error::{PoolError, PoolResult},
    status,
};
use crate::config::PoolConfig;
use async_channel::{Receiver, Sender};
use config_helpers_sv2::CoinbaseRewardScript;
use error_handling::handle_result;
use key_utils::SignatureService;
use nohash_hasher::BuildNoHashHasher;
use secp256k1;
use std::{
    collections::HashMap,
    convert::TryInto,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        self,
        bitcoin::{Amount, TxOut},
        channels_sv2::server::{
            extended::ExtendedChannel,
            group::GroupChannel,
            jobs::{extended::ExtendedJob, job_store::DefaultJobStore, standard::StandardJob},
            standard::StandardChannel,
        },
        codec_sv2::{
            self, binary_sv2::U256, HandshakeRole, Responder, StandardEitherFrame, StandardSv2Frame,
        },
        errors::Error,
        handlers::mining::{ParseMiningMessagesFromDownstream, SendTo},
        mining_sv2::{
            ExtendedExtranonce, SetNewPrevHash as SetNewPrevHashMp, SetTarget, Target,
            MAX_EXTRANONCE_LEN,
        },
        parsers_sv2::{AnyMessage, Mining},
        template_distribution_sv2::{
            NewTemplate, SetNewPrevHash as SetNewPrevHashTdp, SubmitSolution,
        },
        utils::{Id as IdFactory, Mutex},
        VardiffState,
    },
};

use roles_logic_sv2::Vardiff;

use tokio::{net::TcpListener, task};
use tracing::{debug, error, info, warn};

pub mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub mod message_handler;
/// Represents a generic SV2 message with a static lifetime.
pub type Message = AnyMessage<'static>;
/// A standard SV2 frame containing a message.
pub type StdFrame = StandardSv2Frame<Message>;
/// A standard SV2 frame that can contain either type of frame.
pub type EitherFrame = StandardEitherFrame<Message>;

/// Represents a single connection to a downstream miner.
///
/// Encapsulates the state and communication channels for one miner. An instance
/// is created for each accepted TCP connection after the Noise handshake and SV2
/// setup messages are successfully exchanged. Each `Downstream` runs its own message
/// receiving loop in a separate Tokio task.
#[derive(Debug)]
pub struct Downstream {
    // The unique identifier for this downstream connection's channel or group.
    id: u32,
    // Channel receiver for incoming SV2 frames from the network connection task.
    receiver: Receiver<EitherFrame>,
    // Channel sender for outgoing SV2 frames to the network connection task.
    sender: Sender<EitherFrame>,
    // Whether the downstream requires standard jobs.
    requires_standard_jobs: bool,
    // Whether the downstream requires custom work.
    requires_custom_work: bool,
    // Sender channel to forward valid `SubmitSolution` messages received from this
    // downstream miner to the main [`Pool`] task, which then sends them upstream.
    solution_sender: Sender<SubmitSolution<'static>>,
    channel_id_factory: IdFactory,
    extranonce_prefix_factory_extended: Arc<Mutex<ExtendedExtranonce>>,
    extranonce_prefix_factory_standard: Arc<Mutex<ExtendedExtranonce>>,
    // A map of all extended channels, keyed by their ID.
    extended_channels:
        HashMap<u32, Arc<RwLock<ExtendedChannel<'static, DefaultJobStore<ExtendedJob<'static>>>>>>,
    // A map of all standard channels, keyed by their ID.
    standard_channels:
        HashMap<u32, Arc<RwLock<StandardChannel<'static, DefaultJobStore<StandardJob<'static>>>>>>,
    vardiff: HashMap<u32, Arc<RwLock<VardiffState>>>,
    // naive approach:
    // we create one group channel for the connection
    // and add all standard channels to this same single group channel
    // (that is, only if SetupConnection.REQUIRES_STANDARD_JOBS flag is set)
    group_channel:
        Option<Arc<RwLock<GroupChannel<'static, DefaultJobStore<ExtendedJob<'static>>>>>>,
    share_batch_size: usize,
    shares_per_minute: f32,
    last_future_template: NewTemplate<'static>,
    last_new_prev_hash: SetNewPrevHashTdp<'static>,
    coinbase_reward_script: CoinbaseRewardScript,
    // string to be written into the coinbase scriptSig on non-JD jobs
    pool_tag_string: String,
}

/// The central state manager for the mining pool.
///
/// Holds all active downstream connections and manages the overall pool logic.
/// It receives job updates (templates, prev_hashes) from template receiver and distributes
/// them to the appropriate downstreams. It also receives solutions from downstreams
/// and forwards them upstream.
pub struct Pool {
    // A map storing all active downstream connections.
    // Keyed by the downstream's channel/group ID (`u32`).
    downstreams: HashMap<u32, Arc<Mutex<Downstream>>, BuildNoHashHasher<u32>>,
    // Sender channel to forward solutions received from any downstream connection
    // to the upstream Template Provider connection task.
    solution_sender: Sender<SubmitSolution<'static>>,
    // Flag indicating whether at least one `NewTemplate` has been received and processed.
    // Might be used to ensure initial jobs are sent before accepting solutions??.
    new_template_processed: bool,
    downstream_id_factory: IdFactory,
    // Sender channel for reporting status updates and errors to the main monitoring loop.
    status_tx: status::Sender,
    extranonce_prefix_factory_extended: Arc<Mutex<ExtendedExtranonce>>,
    extranonce_prefix_factory_standard: Arc<Mutex<ExtendedExtranonce>>,
    share_batch_size: usize,
    last_future_template: Option<NewTemplate<'static>>,
    last_new_prev_hash: Option<SetNewPrevHashTdp<'static>>,
    // string to be written into the coinbase scriptSig on non-JD jobs
    pool_tag_string: String,
}

impl Downstream {
    /// Creates a new `Downstream` instance representing a miner connection.
    ///
    /// This function orchestrates the setup of a new downstream connection after the
    /// underlying TCP and Noise handshake are complete. It handles the initial SV2
    /// message exchange (`SetupConnection`), assigns a channel ID using the `channel_factory`,
    /// stores the connection, and spawns a dedicated Tokio task (`Downstream::run_receiver`)
    /// to handle incoming messages from this specific miner.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        solution_sender: Sender<SubmitSolution<'static>>,
        pool: Arc<Mutex<Pool>>,
        status_tx: status::Sender,
        address: SocketAddr,
        shares_per_minute: f32,
        coinbase_reward_script: CoinbaseRewardScript,
    ) -> PoolResult<Arc<Mutex<Self>>> {
        // Handle the SV2 SetupConnection message exchange.
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let (requires_standard_jobs, requires_custom_work) =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender, address)
                .await?;

        let id = pool.safe_lock(|p| p.downstream_id_factory.next())?;

        let channel_id_factory = IdFactory::new();

        // extranonce prefix factories are shared across all downstreams
        // that avoids extranonce_prefix collision across different downstreams
        let extranonce_prefix_factory_extended =
            pool.safe_lock(|p| p.extranonce_prefix_factory_extended.clone())?;
        let extranonce_prefix_factory_standard =
            pool.safe_lock(|p| p.extranonce_prefix_factory_standard.clone())?;

        let share_batch_size = pool.safe_lock(|p| p.share_batch_size)?;

        // prevents undefined behavior if some client connects
        // before the first template and prev hash are cached
        let mut has_cached_prev_hash = false;
        while !has_cached_prev_hash {
            pool.safe_lock(|p| {
                if p.last_new_prev_hash.is_some() {
                    has_cached_prev_hash = true;
                }
            })?;
            tokio::task::yield_now().await;
        }

        let last_future_template = pool.safe_lock(|p| {
            p.last_future_template
                .clone()
                .expect("last_future_template must be Some")
        })?;

        let last_new_prev_hash = pool.safe_lock(|p| {
            p.last_new_prev_hash
                .clone()
                .expect("last_new_prev_hash must be Some")
        })?;

        let pool_tag = pool.safe_lock(|p| p.pool_tag_string.clone())?;

        // Create the Downstream instance, wrapped for shared access.
        let self_ = Arc::new(Mutex::new(Downstream {
            id,
            receiver,
            sender: sender.clone(),
            requires_standard_jobs,
            requires_custom_work,
            solution_sender,
            channel_id_factory,
            extended_channels: HashMap::new(),
            standard_channels: HashMap::new(),
            vardiff: HashMap::new(),
            group_channel: None,
            extranonce_prefix_factory_extended,
            extranonce_prefix_factory_standard,
            share_batch_size,
            shares_per_minute,
            last_future_template,
            last_new_prev_hash,
            coinbase_reward_script,
            pool_tag_string: pool_tag,
        }));

        tokio::spawn(spawn_vardiff_loop(self_.clone(), sender.clone(), id));

        let cloned = self_.clone();

        // Spawn a dedicated task to continuously receive and process messages from this downstream.
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
                                "Downstream connection dropped: {e}"
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
                        // Process the valid standard frame using the `next` handler.
                        handle_result!(
                            status_tx,
                            Downstream::next(cloned.clone(), std_frame).await
                        );
                    }
                    _ => {
                        // Attempt to remove the downstream from the main pool's map.
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
            sender.close();
        });
        Ok(self_)
    }

    /// Processes a single incoming message (`StdFrame`) received from the downstream miner.
    ///
    /// It extracts the message type and payload, then uses the `roles_logic_sv2`
    /// (`ParseMiningMessagesFromDownstream`) to determine the appropriate
    /// response. Finally, it dispatches any necessary response(s) using
    /// `Downstream::match_send_to`.
    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) -> PoolResult<()> {
        // Extract message type and payload.
        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();
        debug!(
            "Received downstream message type: {:?}, payload: {:?}",
            message_type, payload
        );

        // Use the message handler implementation to parse the message and determine the response.
        let next_message_to_send = ParseMiningMessagesFromDownstream::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
        );

        // Send the determined response(s) back to the miner.
        Self::match_send_to(self_mutex, next_message_to_send).await
    }

    /// Dispatches messages back to the downstream miner based on the `SendTo` directive.
    ///
    /// Handles different scenarios: sending a single response, sending multiple messages,
    /// or doing nothing. It recursively calls itself for `SendTo::Multiple`.
    /// If an `OpenMiningChannelError` is encountered, it sends the error message and
    /// then returns a specific `PoolError` to signal that this downstream connection
    /// should be dropped by the caller (the receiver loop).
    #[async_recursion::async_recursion]
    async fn match_send_to(
        self_: Arc<Mutex<Self>>,
        send_to: Result<SendTo<()>, Error>,
    ) -> PoolResult<()> {
        match send_to {
            Ok(SendTo::Respond(message)) => {
                debug!("Sending to downstream: {}", message);
                // returning an error will send the error to the main thread,
                // and the main thread will drop the downstream from the pool
                if let &Mining::OpenMiningChannelError(_) = &message {
                    Self::send(self_.clone(), message.clone()).await?;
                    let downstream_id = self_.safe_lock(|d| d.id)?;
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
                // Recursively call match_send_to for each message in the sequence.
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

    /// This method is used to send message to downstream.
    async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers_sv2::Mining<'static>,
    ) -> PoolResult<()> {
        //let message = if let Mining::NewExtendedMiningJob(job) = message {
        //    Mining::NewExtendedMiningJob(extended_job_to_non_segwit(job, 32)?)
        //} else {
        //    message
        //};
        let sv2_frame: StdFrame = AnyMessage::Mining(message).try_into()?;
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone())?;
        sender.send(sv2_frame.into()).await?;
        Ok(())
    }
}

// Verifies token for a custom job which is the signed tx_hash_list_hash by Job Declarator Server
//TODO: implement the use of this function in main.rs
#[allow(dead_code)]
pub fn verify_token(
    tx_hash_list_hash: U256,
    signature: secp256k1::schnorr::Signature,
    pub_key: key_utils::Secp256k1PublicKey,
) -> Result<(), secp256k1::Error> {
    let message: Vec<u8> = tx_hash_list_hash.to_vec();

    let secp = SignatureService::default();

    let is_verified = secp.verify(tx_hash_list_hash.to_vec(), signature, pub_key.0);

    // debug
    debug!("Message: {}", std::str::from_utf8(&message).unwrap());
    debug!("Verified signature {:?}", is_verified);
    is_verified
}

impl Pool {
    /// Binds to the configured listen address and starts accepting incoming TCP connections.
    ///
    /// Runs in a loop, accepting connections, performing the Noise handshake, and then
    /// calling `Pool::accept_incoming_connection_` to handle the SV2 setup and downstream
    /// creation for each successful connection.
    async fn accept_incoming_connection(
        self_: Arc<Mutex<Pool>>,
        config: PoolConfig,
        mut recv_stop_signal: tokio::sync::watch::Receiver<()>,
        shares_per_minute: f32,
    ) -> PoolResult<()> {
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;
        // Bind the TCP listener to the address specified in the config.
        let listener = TcpListener::bind(&config.listen_address()).await?;
        info!("Pool is running on: {}", config.listen_address());
        // Spawn the main accept loop in a separate task.
        task::spawn(async move {
            loop {
                tokio::select! {
                    // Listen for the shutdown signal.
                    _ = recv_stop_signal.changed() => {
                        info!("Pool is stopping the server after stop shutdown signal received");
                        break;
                    },
                    // Accept new incoming TCP connections.
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _)) => {
                                let address = stream.peer_addr().unwrap();
                                info!("New connection from {:?}", stream.peer_addr().map_err(PoolError::Io));
                                // Create a Noise protocol Responder using the pool's authority keys.
                                let responder = Responder::from_authority_kp(
                                    &config.authority_public_key().into_bytes(),
                                    &config.authority_secret_key().into_bytes(),
                                    std::time::Duration::from_secs(config.cert_validity_sec()),
                                );

                                match responder {
                                    Ok(resp) => {
                                        if let Ok((receiver, sender)) = Connection::new::<Message>(stream, HandshakeRole::Responder(resp)).await {
                                            handle_result!(
                                                status_tx,
                                                Self::accept_incoming_connection_(
                                                    self_.clone(),
                                                    receiver,
                                                    sender,
                                                    address,
                                                    shares_per_minute,
                                                    config.coinbase_reward_script().clone()
                                                ).await
                                            );
                                        }
                                    }
                                    Err(_) => {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error accepting connection: {:?}", e);
                            }
                        }
                    }
                }
            }
        });
        Ok(())
    }

    /// Handles the post-handshake setup for a newly connected miner.
    ///
    /// Called by `accept_incoming_connection` after TCP and Noise handshake succeed.
    /// It creates the `Downstream` instance (which includes SV2 setup), and adds the
    /// new downstream to the pool's central `downstreams` map.
    async fn accept_incoming_connection_(
        self_: Arc<Mutex<Pool>>,
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        address: SocketAddr,
        shares_per_minute: f32,
        coinbase_reward_script: CoinbaseRewardScript,
    ) -> PoolResult<()> {
        let solution_sender = self_.safe_lock(|p| p.solution_sender.clone())?;
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;

        // Create the Downstream instance
        let downstream = Downstream::new(
            receiver,
            sender,
            solution_sender,
            self_.clone(),
            // convert Listener variant to Downstream variant
            status_tx.listener_to_connection(),
            address,
            shares_per_minute,
            coinbase_reward_script,
        )
        .await?;

        // Extract the assigned ID after successful creation.
        let channel_id = downstream.safe_lock(|d| d.id)?;

        // Add the new downstream to the central map.
        self_.safe_lock(|p| {
            p.downstreams.insert(channel_id, downstream);
        })?;
        Ok(())
    }

    /// Task to handle incoming `SetNewPrevHash` messages from the upstream source.
    ///
    /// Runs in a loop, receiving messages from the `rx` channel. For each message,
    /// it updates the pool's `last_prev_hash_template_id`, uses the `channel_factory`
    /// to generate the appropriate `SetNewPrevHash` message for downstream miners,
    /// and broadcasts it to all connected downstreams. Sends an acknowledgement signal
    /// on `sender_message_received_signal` after processing each message.
    async fn on_new_prev_hash(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<SetNewPrevHashTdp<'static>>,
        sender_message_received_signal: Sender<()>,
    ) -> PoolResult<()> {
        let status_tx = self_
            .safe_lock(|s| s.status_tx.clone())
            .map_err(|e| PoolError::PoisonLock(e.to_string()))?;
        while let Ok(new_prev_hash) = rx.recv().await {
            debug!("New prev hash received: {}", new_prev_hash);
            let res = self_
                .safe_lock(|s| {
                    s.last_new_prev_hash = Some(new_prev_hash.clone());
                })
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            handle_result!(status_tx, res);

            let downstreams = self_
                .safe_lock(|s| s.downstreams.clone())
                .map_err(|e| PoolError::PoisonLock(e.to_string()));

            let downstreams = handle_result!(status_tx, downstreams);

            for (_downstream_id, downstream) in downstreams {
                downstream.safe_lock(|d| {
                    d.last_new_prev_hash = new_prev_hash.clone();
                })?;

                let mining_set_new_prev_hash_messages = downstream.safe_lock(|d| {
                    let mut messages = Vec::new();

                    // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                    // if no, we need to send the SetNewPrevHashMp to the group channel
                    if let Some(group_channel_guard) = &d.group_channel {
                        let mut group_channel = group_channel_guard
                            .write()
                            .map_err(|e| Error::PoisonLock(e.to_string()))?;

                        group_channel
                            .on_set_new_prev_hash(new_prev_hash.clone())
                            .map_err(Error::FailedToProcessSetNewPrevHashGroupChannel)?;

                        let group_channel_id = group_channel.get_group_channel_id();
                        let activated_group_job_id = group_channel
                            .get_active_job()
                            .expect("active job must exist")
                            .get_job_id();

                        let set_new_prev_hash_message = SetNewPrevHashMp {
                            channel_id: group_channel_id,
                            job_id: activated_group_job_id,
                            prev_hash: new_prev_hash.prev_hash.clone(),
                            min_ntime: new_prev_hash.header_timestamp,
                            nbits: new_prev_hash.n_bits,
                        };
                        messages.push(set_new_prev_hash_message.into_static());
                    }

                    for (standard_channel_id, standard_channel_lock) in d.standard_channels.iter() {
                        let mut standard_channel = standard_channel_lock
                            .write()
                            .map_err(|e| Error::PoisonLock(e.to_string()))?;

                        // process the SetNewPrevHashTdp for the standard channel
                        // regardless of the REQUIRES_STANDARD_JOBS flag
                        // because this is the only way we can verify shares later
                        standard_channel
                            .on_set_new_prev_hash(new_prev_hash.clone())
                            .map_err(Error::FailedToProcessSetNewPrevHashStandardChannel)?;

                        // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                        // if yes, there's no group channel, so we need to send the SetNewPrevHashMp
                        // to each standard channel
                        if d.group_channel.is_none() {
                            let activated_standard_job_id = standard_channel
                                .get_active_job()
                                .expect("active job must exist")
                                .get_job_id();
                            let set_new_prev_hash_message = SetNewPrevHashMp {
                                channel_id: *standard_channel_id,
                                job_id: activated_standard_job_id,
                                prev_hash: new_prev_hash.prev_hash.clone(),
                                min_ntime: new_prev_hash.header_timestamp,
                                nbits: new_prev_hash.n_bits,
                            };
                            messages.push(set_new_prev_hash_message.into_static());
                        }
                    }

                    for (extended_channel_id, extended_channel_lock) in d.extended_channels.iter() {
                        let mut extended_channel = extended_channel_lock
                            .write()
                            .map_err(|e| Error::PoisonLock(e.to_string()))?;
                        extended_channel
                            .on_set_new_prev_hash(new_prev_hash.clone())
                            .map_err(Error::FailedToProcessSetNewPrevHashExtendedChannel)?;

                        // don't send any SetNewPrevHash messages to Extended Channels
                        // if the downstream requires custom work
                        if d.requires_custom_work {
                            continue;
                        }

                        let activated_extended_job_id = extended_channel
                            .get_active_job()
                            .expect("active job must exist")
                            .get_job_id();
                        let set_new_prev_hash_message = SetNewPrevHashMp {
                            channel_id: *extended_channel_id,
                            job_id: activated_extended_job_id,
                            prev_hash: new_prev_hash.prev_hash.clone(),
                            min_ntime: new_prev_hash.header_timestamp,
                            nbits: new_prev_hash.n_bits,
                        };
                        messages.push(set_new_prev_hash_message.into_static());
                    }
                    Ok::<_, PoolError>(messages)
                })??;

                for message in mining_set_new_prev_hash_messages {
                    let res = Downstream::match_send_to(
                        downstream.clone(),
                        Ok(SendTo::Respond(Mining::SetNewPrevHash(message))),
                    )
                    .await;
                    handle_result!(status_tx, res);
                }
            }
            handle_result!(status_tx, sender_message_received_signal.send(()).await);
        }
        Ok(())
    }

    /// Task to handle incoming `NewTemplate` messages from the upstream source.
    ///
    /// Runs in a loop, receiving messages from the `rx` channel. For each template,
    /// it uses the `channel_factory` to generate the appropriate mining job messages
    /// (e.g., `NewMiningJob`, `SetExtranoncePrefix`) for each relevant downstream channel.
    /// It then sends these specific messages to the corresponding downstream miners.
    /// Sets the `new_template_processed` flag and sends an acknowledgement signal
    /// on `sender_message_received_signal` after processing.
    async fn on_new_template(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<NewTemplate<'static>>,
        sender_message_received_signal: Sender<()>,
    ) -> PoolResult<()> {
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;
        while let Ok(new_template) = rx.recv().await {
            info!(
                "New template received, creating a new mining job(s): {}",
                new_template
            );

            let downstreams = self_
                .safe_lock(|s| s.downstreams.clone())
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let downstreams = handle_result!(status_tx, downstreams);

            for (_downstream_id, downstream) in downstreams {
                if new_template.future_template {
                    downstream.safe_lock(|d| {
                        d.last_future_template = new_template.clone();
                    })?;
                }

                let standard_job_messages = downstream.safe_lock(|d| {
                    let mut messages = Vec::new();

                    let pool_coinbase_output = TxOut {
                        value: Amount::from_sat(new_template.coinbase_tx_value_remaining),
                        script_pubkey: d.coinbase_reward_script.script_pubkey(),
                    };

                    match new_template.future_template {
                        true => {
                            for (_standard_channel_id, standard_channel_lock) in
                                d.standard_channels.iter()
                            {
                                let mut standard_channel = standard_channel_lock
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                                // if yes, there's no group channel, so we need to send the future
                                // job to each standard channel
                                // if no, there's a group channel and there's no standard job to
                                // send
                                if d.group_channel.is_none() {
                                    standard_channel
                                        .on_new_template(
                                            new_template.clone(),
                                            vec![pool_coinbase_output.clone()],
                                        )
                                        .map_err(
                                            Error::FailedToProcessNewTemplateStandardChannel,
                                        )?;

                                    let standard_job_id = standard_channel
                                        .get_future_template_to_job_id()
                                        .get(&new_template.template_id)
                                        .expect("job_id must exist");
                                    let standard_job = standard_channel
                                        .get_future_jobs()
                                        .get(standard_job_id)
                                        .expect("standard job must exist");
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push(standard_job_message.clone().into_static());
                                }
                            }
                        }
                        false => {
                            for (_standard_channel_id, standard_channel_lock) in
                                d.standard_channels.iter()
                            {
                                let mut standard_channel = standard_channel_lock
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                                // if yes, there's no group channel, so we need to send the
                                // non-future job to each standard channel
                                // if no, there is a group channel, so there's no standard job to
                                // send
                                if d.group_channel.is_none() {
                                    standard_channel
                                        .on_new_template(
                                            new_template.clone(),
                                            vec![pool_coinbase_output.clone()],
                                        )
                                        .map_err(
                                            Error::FailedToProcessNewTemplateStandardChannel,
                                        )?;

                                    let standard_job = standard_channel
                                        .get_active_job()
                                        .expect("standard job must exist");
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push(standard_job_message.clone().into_static());
                                }
                            }
                        }
                    }
                    Ok::<_, PoolError>(messages)
                })??;

                for standard_job_message in standard_job_messages {
                    let res = Downstream::match_send_to(
                        downstream.clone(),
                        Ok(SendTo::Respond(Mining::NewMiningJob(standard_job_message))),
                    )
                    .await;
                    handle_result!(status_tx, res);
                }

                let extended_job_messages = downstream.safe_lock(|d| {
                    // if the downstream requires custom work, we don't need to send any extended
                    // jobs
                    if d.requires_custom_work {
                        return Ok(Vec::new());
                    }

                    let pool_coinbase_output = TxOut {
                        value: Amount::from_sat(new_template.coinbase_tx_value_remaining),
                        script_pubkey: d.coinbase_reward_script.script_pubkey(),
                    };

                    let mut messages = Vec::new();
                    match new_template.future_template {
                        true => {
                            // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                            // if yes, we don't care about Group Channel
                            // if no, we need to send the future job to the Group Channel
                            if let Some(group_channel_guard) = &d.group_channel {
                                let mut group_channel = group_channel_guard
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;
                                group_channel
                                    .on_new_template(
                                        new_template.clone(),
                                        vec![pool_coinbase_output.clone()],
                                    )
                                    .map_err(|e| {
                                        Error::FailedToProcessNewTemplateGroupChannel(e)
                                    })?;
                                let future_job_id = group_channel
                                    .get_future_template_to_job_id()
                                    .get(&new_template.template_id)
                                    .expect("job_id must exist");
                                let future_job = group_channel
                                    .get_future_jobs()
                                    .get(future_job_id)
                                    .expect("future job must exist");

                                // also update the standard channels states with the future job
                                // so they're able to validate shares later
                                for (_standard_channel_id, standard_channel_lock) in
                                    d.standard_channels.iter()
                                {
                                    let mut standard_channel = standard_channel_lock
                                        .write()
                                        .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                    standard_channel
                                        .on_group_channel_job(future_job.clone())
                                        .map_err(Error::FailedToProcessGroupChannelJob)?;
                                }

                                let future_job_message = future_job.get_job_message();
                                messages.push(future_job_message.clone().into_static());
                            }

                            // also send the future job to each extended channel
                            for (_extended_channel_id, extended_channel_lock) in
                                d.extended_channels.iter()
                            {
                                let mut extended_channel = extended_channel_lock
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                extended_channel
                                    .on_new_template(
                                        new_template.clone(),
                                        vec![pool_coinbase_output.clone()],
                                    )
                                    .map_err(|e| {
                                        Error::FailedToProcessNewTemplateExtendedChannel(e)
                                    })?;

                                let extended_job_id = extended_channel
                                    .get_future_template_to_job_id()
                                    .get(&new_template.template_id)
                                    .expect("job_id must exist");

                                let extended_job = extended_channel
                                    .get_future_jobs()
                                    .get(extended_job_id)
                                    .expect("extended job must exist");

                                let extended_job_message = extended_job.get_job_message();
                                messages.push(extended_job_message.clone().into_static());
                            }
                        }
                        false => {
                            // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                            // if yes, we don't care about Group Channel
                            // if no, we need to send the non-future job to the Group Channel
                            if let Some(group_channel_guard) = &d.group_channel {
                                let mut group_channel = group_channel_guard
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;
                                group_channel
                                    .on_new_template(
                                        new_template.clone(),
                                        vec![pool_coinbase_output.clone()],
                                    )
                                    .map_err(|e| {
                                        Error::FailedToProcessNewTemplateGroupChannel(e)
                                    })?;
                                let active_job = group_channel
                                    .get_active_job()
                                    .expect("active job must exist");

                                // also update the standard channels states with the active job
                                // so they're able to validate shares later
                                for (_standard_channel_id, standard_channel_lock) in
                                    d.standard_channels.iter()
                                {
                                    let mut standard_channel = standard_channel_lock
                                        .write()
                                        .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                    standard_channel
                                        .on_group_channel_job(active_job.clone())
                                        .map_err(Error::FailedToProcessGroupChannelJob)?;
                                }
                                let active_job_message = active_job.get_job_message();
                                messages.push(active_job_message.clone().into_static());
                            }

                            // also send the non-future job to each extended channel
                            for (_extended_channel_id, extended_channel_lock) in
                                d.extended_channels.iter()
                            {
                                let mut extended_channel = extended_channel_lock
                                    .write()
                                    .map_err(|e| Error::PoisonLock(e.to_string()))?;

                                extended_channel
                                    .on_new_template(
                                        new_template.clone(),
                                        vec![pool_coinbase_output.clone()],
                                    )
                                    .map_err(|e| {
                                        Error::FailedToProcessNewTemplateExtendedChannel(e)
                                    })?;

                                let extended_job = extended_channel
                                    .get_active_job()
                                    .expect("active job must exist");

                                let extended_job_message = extended_job.get_job_message();
                                messages.push(extended_job_message.clone().into_static());
                            }
                        }
                    }
                    Ok::<_, PoolError>(messages)
                })??;

                for extended_job_message in extended_job_messages {
                    let res = Downstream::match_send_to(
                        downstream.clone(),
                        Ok(SendTo::Respond(Mining::NewExtendedMiningJob(
                            extended_job_message,
                        ))),
                    )
                    .await;
                    handle_result!(status_tx, res);
                }
            }

            if new_template.future_template {
                let res = self_
                    .safe_lock(|s| {
                        s.last_future_template = Some(new_template);
                        s.new_template_processed = true;
                    })
                    .map_err(|e| PoolError::PoisonLock(e.to_string()));
                handle_result!(status_tx, res);
            }

            handle_result!(status_tx, sender_message_received_signal.send(()).await);
        }
        Ok(())
    }

    /// Starts the main pool logic, including the connection listener and message handling tasks.
    ///
    /// Initializes the `PoolChannelFactory` and the `Pool` state struct. Spawns three key
    /// background tasks:
    /// 1. `accept_incoming_connection`: Listens for and handles new downstream connections.
    /// 2. `on_new_prev_hash`: Processes previous hash updates from upstream.
    /// 3. `on_new_template`: Processes new job templates from upstream.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        config: PoolConfig,
        new_template_rx: Receiver<NewTemplate<'static>>,
        new_prev_hash_rx: Receiver<SetNewPrevHashTdp<'static>>,
        solution_sender: Sender<SubmitSolution<'static>>,
        sender_message_received_signal: Sender<()>,
        status_tx: status::Sender,
        shares_per_minute: f32,
        recv_stop_signal: tokio::sync::watch::Receiver<()>,
    ) -> Result<Arc<Mutex<Self>>, PoolError> {
        // range_1 is used for dynamically allocating extranonce_prefix across different channels
        // from these 8 bytes, the first 2 bytes are statically defined by static_prefix
        let range_1_start = 0;
        let range_1_end = 8;

        // range_0 is not used here
        let range_0 = std::ops::Range {
            start: range_1_start,
            end: range_1_start,
        };
        let range_1 = std::ops::Range {
            start: range_1_start,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        // simulating a scenario where there are multiple mining servers
        // this static prefix allows unique extranonce_prefix allocation
        // for this mining server
        let static_prefix = config.server_id().to_be_bytes().to_vec();

        let extranonce_prefix_factory_extended = ExtendedExtranonce::new(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            Some(static_prefix.clone()),
        )
        .expect("Failed to create ExtendedExtranonce with valid ranges");

        let extranonce_prefix_factory_standard =
            ExtendedExtranonce::new(range_0, range_1, range_2, Some(static_prefix.clone()))
                .expect("Failed to create ExtendedExtranonce with valid ranges");

        // --- Initialize Pool State ---
        let pool = Arc::new(Mutex::new(Pool {
            downstreams: HashMap::with_hasher(BuildNoHashHasher::default()),
            solution_sender,
            new_template_processed: false,
            downstream_id_factory: IdFactory::new(),
            status_tx: status_tx.clone(),
            extranonce_prefix_factory_extended: Arc::new(Mutex::new(
                extranonce_prefix_factory_extended,
            )),
            extranonce_prefix_factory_standard: Arc::new(Mutex::new(
                extranonce_prefix_factory_standard,
            )),
            share_batch_size: config.share_batch_size(),
            last_future_template: None,
            last_new_prev_hash: None,
            pool_tag_string: config.pool_signature().clone(),
        }));

        let cloned = pool.clone();
        let cloned2 = pool.clone();
        let cloned3 = pool.clone();

        info!("Starting up Pool server");
        let status_tx_clone = status_tx.clone();
        // Task to handle multiple downstream connection.
        if let Err(e) =
            Self::accept_incoming_connection(cloned, config, recv_stop_signal, shares_per_minute)
                .await
        {
            error!("Pool stopped accepting connections due to: {}", &e);
            let _ = status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Pool stopped accepting connections".to_string(),
                    )),
                })
                .await;

            return Err(e);
        }

        let cloned = sender_message_received_signal.clone();
        let status_tx_clone = status_tx.clone();
        // Task to handle new prev hash message from template provider.
        task::spawn(async move {
            if let Err(e) = Self::on_new_prev_hash(cloned2, new_prev_hash_rx, cloned).await {
                error!("{}", e);
            }
            // on_new_prev_hash shutdown
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting new prevhash".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });

        let status_tx_clone = status_tx;
        // Task to handle new template message from template provider.
        task::spawn(async move {
            if let Err(e) =
                Self::on_new_template(pool, new_template_rx, sender_message_received_signal).await
            {
                error!("{}", e);
            }
            // on_new_template shutdown
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting templates".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });
        Ok(cloned3)
    }

    /// Removes a downstream connection from the pool's active map.
    ///
    /// Called when a downstream disconnects or needs to be removed for other reasons
    /// (e.g., protocol error signaled via `PoolError::Sv2ProtocolError`).
    ///
    /// **Note:** There's a potential race condition. If job distribution tasks clone the
    /// `downstreams` map just before this removal happens, they might still attempt
    /// to send a message to the removed downstream. This attempt will likely fail
    /// harmlessly when `Downstream::send` tries to use the closed channel.
    pub fn remove_downstream(&mut self, downstream_id: u32) {
        self.downstreams.remove(&downstream_id);
    }
}

async fn send_set_target_downstream(
    sender: Sender<EitherFrame>,
    channel_id: u32,
    target: Target,
) -> Result<(), PoolError> {
    debug!("Attempting to send `SetTarget` for channel_id={channel_id}");

    let target_message = SetTarget {
        channel_id,
        maximum_target: target.into(),
    };

    let mining_msg = Mining::SetTarget(target_message);

    info!("Sending SetTarget message to downstream: {}", mining_msg);

    let sv2_frame: StdFrame = AnyMessage::Mining(mining_msg).try_into()?;

    sender.send(sv2_frame.into()).await?;

    Ok(())
}

fn run_vardiff_on_extended_channel(
    channel_id: u32,
    channel: Arc<RwLock<ExtendedChannel<'static, DefaultJobStore<ExtendedJob<'static>>>>>,
    vardiff_state: Arc<RwLock<VardiffState>>,
    updates: &mut Vec<(u32, Target)>,
) {
    let Ok(mut channel_state) = channel.write() else {
        debug!("Failed to lock extended channel {channel_id}");
        return;
    };

    let Ok(mut vardiff_state) = vardiff_state.write() else {
        debug!("Failed to lock vardiff state for extended channel {channel_id}");
        return;
    };

    let hashrate = channel_state.get_nominal_hashrate();
    let target = channel_state.get_target();
    let shares_per_minute = channel_state.get_shares_per_minute();

    let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
    else {
        debug!("Vardiff computation failed for extended channel {channel_id}");
        return;
    };

    if let Some(new_hashrate) = new_hashrate_opt {
        match channel_state.update_channel(new_hashrate, None) {
            Ok(()) => {
                let updated_target = channel_state.get_target();
                updates.push((channel_id, updated_target.clone()));

                debug!(
                    "Updated target for extended channel channel_id={channel_id} to {:?}",
                    updated_target
                );
            }
            Err(e) => warn!(
                "Failed to update extended channel channel_id={channel_id} during vardiff {e:?}"
            ),
        };
    }
}

fn run_vardiff_on_standard_channel(
    channel_id: u32,
    channel: Arc<RwLock<StandardChannel<'static, DefaultJobStore<StandardJob<'static>>>>>,
    vardiff_state: &Arc<RwLock<VardiffState>>,
    updates: &mut Vec<(u32, Target)>,
) {
    let Ok(mut channel_state) = channel.write() else {
        debug!("Failed to lock standard channel {channel_id}");
        return;
    };

    let Ok(mut vardiff_state) = vardiff_state.write() else {
        debug!("Failed to lock vardiff state for standard channel {channel_id}");
        return;
    };

    let hashrate = channel_state.get_nominal_hashrate();
    let target = channel_state.get_target();
    let shares_per_minute = channel_state.get_shares_per_minute();

    let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
    else {
        debug!("Vardiff computation failed for standard channel {channel_id}");
        return;
    };

    if let Some(new_hashrate) = new_hashrate_opt {
        match channel_state.update_channel(new_hashrate, None) {
            Ok(()) => {
                let updated_target = channel_state.get_target();
                updates.push((channel_id, updated_target.clone()));

                debug!(
                    "Updated target for standard channel channel_id={channel_id} to {:?}",
                    updated_target
                );
            }
            Err(e) => warn!(
                "Failed to update standard channel channel_id={channel_id} during vardiff {e:?}"
            ),
        };
    }
}

/// This method implements the pool's variable difficulty logic for a single downstream.
/// A downstream can have multiple active channels connected to the pool.
/// Every 60 seconds, this method updates the difficulty state for each channel belonging to the
/// downstream.
async fn spawn_vardiff_loop(
    downstream: Arc<Mutex<Downstream>>,
    sender: Sender<EitherFrame>,
    downstream_id: u32,
) {
    info!("Spawning vardiff adjustment loop for downstream: {downstream_id}");

    'vardiff_loop: loop {
        if sender.is_closed() {
            debug!("Downstream {downstream_id} closed, stopping vardiff loop");
            break;
        }

        tokio::time::sleep(Duration::from_secs(60)).await;

        debug!("Starting vardiff updates for downstream: {downstream_id}");
        let mut updates = Vec::new();

        _ = downstream.safe_lock(|d| {
            for (channel_id, vardiff_state) in &d.vardiff {
                if let Some(channel) = d.extended_channels.get(channel_id) {
                    run_vardiff_on_extended_channel(
                        *channel_id,
                        channel.clone(),
                        vardiff_state.clone(),
                        &mut updates,
                    );
                }

                if let Some(channel) = d.standard_channels.get(channel_id) {
                    run_vardiff_on_standard_channel(
                        *channel_id,
                        channel.clone(),
                        vardiff_state,
                        &mut updates,
                    );
                }
            }
        });

        for (channel_id, target) in updates {
            if let Err(e) = send_set_target_downstream(sender.clone(), channel_id, target).await {
                error!(
                    "Failed to send SetTarget message downstream for channel {channel_id}: {:?}",
                    e
                );
                break 'vardiff_loop;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use ext_config::{Config, File, FileFormat};
    use std::convert::TryInto;
    use stratum_common::roles_logic_sv2::{
        bitcoin::{
            self, absolute::LockTime, consensus, transaction::Version, Amount, Transaction, TxOut,
            Witness,
        },
        codec_sv2::binary_sv2::{B0255, B064K},
    };
    use tracing::error;

    use super::PoolConfig;

    // this test is used to verify the `coinbase_tx_prefix` and `coinbase_tx_suffix` values tested
    // against in message generator
    // `stratum/test/message-generator/test/pool-sri-test-extended.json`
    #[test]
    fn test_coinbase_outputs_from_config() {
        let config_path = "./config-examples/pool-config-local-tp-example.toml";

        // Load config
        let config: PoolConfig = match Config::builder()
            .add_source(File::new(config_path, FileFormat::Toml))
            .build()
        {
            Ok(settings) => match settings.try_deserialize::<PoolConfig>() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to deserialize config: {}", e);
                    return;
                }
            },
            Err(e) => {
                error!("Failed to build config: {}", e);
                return;
            }
        };

        // template from message generator test (mock TP template)
        let _extranonce_len = 3;
        let coinbase_prefix = vec![3, 76, 163, 38, 0];
        let _version = 536870912;
        let coinbase_tx_version = 2;
        let coinbase_tx_input_sequence = 4294967295;
        let coinbase_tx_value_remaining: u64 = 0;
        let _coinbase_tx_outputs_count = 0;
        let coinbase_tx_locktime = 0;
        // extranonce len set to max_extranonce_size in `ChannelFactory::new_extended_channel()`
        let extranonce_len = 32;

        // build coinbase TX from 'job_creator::coinbase()'

        let mut bip34_bytes = get_bip_34_bytes(coinbase_prefix.try_into().unwrap());
        let script_prefix_length = bip34_bytes.len() + config.pool_signature().len();
        bip34_bytes.extend_from_slice(config.pool_signature().as_bytes());
        bip34_bytes.extend_from_slice(&vec![0; extranonce_len as usize]);
        let witness = match bip34_bytes.len() {
            0 => Witness::from(vec![] as Vec<Vec<u8>>),
            _ => Witness::from(vec![vec![0; 32]]),
        };

        let tx_in = bitcoin::TxIn {
            previous_output: bitcoin::OutPoint::null(),
            script_sig: bip34_bytes.into(),
            sequence: bitcoin::Sequence(coinbase_tx_input_sequence),
            witness,
        };

        let coinbase_tx_output = TxOut {
            value: Amount::from_sat(coinbase_tx_value_remaining),
            script_pubkey: config.coinbase_reward_script().script_pubkey(),
        };
        let coinbase = Transaction {
            version: Version::non_standard(coinbase_tx_version),
            lock_time: LockTime::from_consensus(coinbase_tx_locktime),
            input: vec![tx_in],
            output: vec![coinbase_tx_output],
        };

        let coinbase_tx_prefix = coinbase_tx_prefix(&coinbase, script_prefix_length);
        let coinbase_tx_suffix =
            coinbase_tx_suffix(&coinbase, extranonce_len, script_prefix_length);
        assert!(
            coinbase_tx_prefix
                == [
                    2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 56, 3, 76, 163, 38,
                    0, 83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111,
                    111, 108
                ]
                .to_vec()
                .try_into()
                .unwrap(),
            "coinbase_tx_prefix incorrect"
        );
        assert!(
            coinbase_tx_suffix
                == [
                    255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                    194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 1,
                    32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
                .to_vec()
                .try_into()
                .unwrap(),
            "coinbase_tx_suffix incorrect"
        );
    }

    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_prefix(coinbase: &Transaction, script_prefix_len: usize) -> B064K<'static> {
        let encoded = consensus::serialize(coinbase);
        // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the
        // 0 witness
        let segwit_bytes = match script_prefix_len {
            0 => 0,
            _ => 2,
        };
        let index = 4    // tx version
            + segwit_bytes
            + 1  // number of inputs TODO can be also 3
            + 32 // prev OutPoint
            + 4  // index
            + 1  // bytes in script TODO can be also 3
            + script_prefix_len; // bip34_bytes
        let r = encoded[0..index].to_vec();
        r.try_into().unwrap()
    }

    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_suffix(
        coinbase: &Transaction,
        extranonce_len: u8,
        script_prefix_len: usize,
    ) -> B064K<'static> {
        let encoded = consensus::serialize(coinbase);
        // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the
        // 0 witness
        let segwit_bytes = match script_prefix_len {
            0 => 0,
            _ => 2,
        };
        let r = encoded[4    // tx version
        + segwit_bytes
        + 1  // number of inputs TODO can be also 3
        + 32 // prev OutPoint
        + 4  // index
        + 1  // bytes in script TODO can be also 3
        + script_prefix_len  // bip34_bytes
        + (extranonce_len as usize)..]
            .to_vec();
        r.try_into().unwrap()
    }

    fn get_bip_34_bytes(coinbase_prefix: B0255<'static>) -> Vec<u8> {
        let script_prefix = &coinbase_prefix.to_vec()[..];
        // add 1 cause 0 is push 1 2 is 1 is push 2 ecc ecc
        // add 1 cause in the len there is also the op code itself
        let bip34_len = script_prefix[0] as usize + 2;
        if bip34_len == script_prefix.len() {
            script_prefix[0..bip34_len].to_vec()
        } else {
            panic!("bip34 length does not match script prefix")
        }
    }
}
