use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        bitcoin::{
            self, absolute::LockTime, transaction::Version, OutPoint, ScriptBuf, Sequence,
            Transaction, TxIn, TxOut, Witness,
        },
        codec_sv2::{self, framing_sv2, HandshakeRole, Initiator},
        handlers_sv2::HandleCommonMessagesFromServerAsync,
        parsers_sv2::{AnyMessage, TemplateDistribution},
        template_distribution_sv2::CoinbaseOutputConstraints,
        utils::Mutex,
    },
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, instrument, warn, Instrument};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::{self, TaskManager},
    utils::{
        get_setup_connection_message_tp, message_from_frame, spawn_io_tasks,
        spawn_io_tasks_tracing, EitherFrame, Message, ShutdownMessage, StdFrame,
    },
};

mod message_handler;

pub struct TemplateReceiverData;

#[derive(Clone)]
pub struct TemplateReceiverChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: Receiver<EitherFrame>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct TemplateReceiver {
    template_receiver_data: Arc<Mutex<TemplateReceiverData>>,
    template_receiver_channel: TemplateReceiverChannel,
    tp_address: String,
}

impl TemplateReceiver {
    #[instrument(
        skip_all,
        fields(tp_address = %tp_address)
    )]
    pub async fn new(
        tp_address: String,
        public_key: Option<Secp256k1PublicKey>,
        channel_manager_receiver: Receiver<EitherFrame>,
        channel_manager_sender: Sender<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: tokio::sync::mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<TemplateReceiver, JDCError> {
        const MAX_RETRIES: usize = 3;

        for attempt in 1..=MAX_RETRIES {
            info!(attempt, MAX_RETRIES, "Connecting to template provider");

            let initiator = match public_key {
                Some(pub_key) => {
                    debug!(attempt, "Using public key for initiator handshake");
                    Initiator::from_raw_k(pub_key.into_bytes())
                }
                None => {
                    debug!(attempt, "Using anonymous initiator (no public key)");
                    Initiator::without_pk()
                }
            }?;

            match TcpStream::connect(tp_address.as_str()).await {
                Ok(stream) => {
                    info!(
                        attempt,
                        "TCP connection established, starting Noise handshake"
                    );

                    match NoiseTcpStream::<Message>::new(
                        stream,
                        HandshakeRole::Initiator(initiator),
                    )
                    .await
                    {
                        Ok(noise_stream) => {
                            info!(attempt, "Noise handshake completed successfully");

                            let (noise_stream_reader, noise_stream_writer) =
                                noise_stream.into_split();

                            let status_sender = StatusSender::TemplateReceiver(status_sender);
                            let (inbound_tx, inbound_rx) = unbounded::<EitherFrame>();
                            let (outbound_tx, outbound_rx) = unbounded::<EitherFrame>();

                            info!(attempt, "Spawning IO tasks for template receiver");
                            let span = tracing::Span::current();
                            spawn_io_tasks_tracing(
                                task_manager.clone(),
                                noise_stream_reader,
                                noise_stream_writer,
                                outbound_rx,
                                inbound_tx,
                                notify_shutdown,
                                status_sender,
                                span,
                            );

                            let template_receiver_data = Arc::new(Mutex::new(TemplateReceiverData));
                            let template_receiver_channel = TemplateReceiverChannel {
                                channel_manager_receiver,
                                channel_manager_sender,
                                inbound_rx,
                                outbound_tx,
                            };

                            info!(attempt, "TemplateReceiver initialized successfully");
                            return Ok(TemplateReceiver {
                                template_receiver_channel,
                                template_receiver_data,
                                tp_address,
                            });
                        }
                        Err(e) => {
                            error!(attempt, error = ?e, "Noise handshake failed");
                        }
                    }
                }
                Err(e) => {
                    warn!(attempt, MAX_RETRIES, error = ?e, "Failed to connect to template provider");
                }
            }

            if attempt < MAX_RETRIES {
                debug!(attempt, "Retrying connection after backoff");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }

        error!("Exhausted all connection attempts, shutting down TemplateReceiver");
        Err(JDCError::Shutdown)
    }

    #[instrument(skip_all, fields(peer_addr = %socket_address))]
    pub async fn start(
        mut self,
        socket_address: String,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
        coinbase_outputs: Vec<u8>,
    ) {
        let status_sender = StatusSender::TemplateReceiver(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        info!("Initialized state for starting template receiver");
        self.setup_connection(socket_address).await;

        self.coinbase_constraints(coinbase_outputs).await;

        info!("Setup Connection done. connection with template receiver is now done");
        task_manager.spawn(
            async move {
                loop {
                    let mut self_clone_1 = self.clone();
                    let self_clone_2 = self.clone();
                    tokio::select! {
                        message = shutdown_rx.recv() => {
                            if let Ok(ShutdownMessage::ShutdownAll) = message {
                                info!("Template Receiver: received shutdown signal");
                                break;
                            }
                        }
                        res = self_clone_1.handle_template_provider_message() => {
                            if let Err(e) = res {
                                error!("TemplateReceiver template provider handler failed: {e:?}");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        }
                        res = self_clone_2.handle_channel_manager_message() => {
                            if let Err(e) = res {
                                error!("TemplateReceiver channel manager handler failed: {e:?}");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        },
                    }
                }
                warn!("TemplateReceiver: unified message loop exited.");
            }
            .instrument(tracing::Span::current()),
        );
    }

    #[instrument(name = "template_provider_message", skip_all)]
    pub async fn handle_template_provider_message(&mut self) -> Result<(), JDCError> {
        let read_frame = self.template_receiver_channel.inbound_rx.recv().await?;

        debug!("Received frame from template provider");

        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();

                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        debug!(?message_type, "Handling common message from server");
                        self.handle_common_message_from_server(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::TemplateDistribution(_) => {
                        debug!(
                            ?message_type,
                            "Forwarding TemplateDistribution message to channel manager"
                        );
                        self.template_receiver_channel
                            .channel_manager_sender
                            .send(EitherFrame::Sv2(std_frame))
                            .await
                            .map_err(|e| {
                                error!(
                                    "Failed to forward mining message to channel manager: {e:?}"
                                );
                                JDCError::ChannelErrorSender
                            })?;
                    }
                    _ => {
                        warn!(
                            ?message_type,
                            "Unsupported message type from Template Provider"
                        );
                    }
                }
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!(?handshake_frame, "Received handshake frame");
            }
        }

        Ok(())
    }

    #[instrument(name = "channel_manager_message", skip_all)]
    pub async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        let msg = self
            .template_receiver_channel
            .channel_manager_receiver
            .recv()
            .await?;
        debug!("Forwarding message from channel manager to outbound_tx");
        self.template_receiver_channel
            .outbound_tx
            .send(msg)
            .await
            .map_err(|_| JDCError::ChannelErrorSender)?;

        Ok(())
    }

    #[instrument(skip_all, fields(outputs_len = coinbase_outputs.len()))]
    pub async fn coinbase_constraints(
        &mut self,
        coinbase_outputs: Vec<u8>,
    ) -> Result<(), JDCError> {
        debug!(
            "Deserializing coinbase outputs ({} bytes)",
            coinbase_outputs.len()
        );
        let outputs: Vec<TxOut> = bitcoin::consensus::deserialize(&coinbase_outputs)?;

        let max_size: u32 = outputs.iter().map(|o| o.size() as u32).sum();
        debug!(
            max_size,
            outputs_count = outputs.len(),
            "Calculated max coinbase output size"
        );

        let dummy_coinbase = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::from(vec![vec![0; 32]]),
            }],
            output: outputs,
        };

        let max_sigops = dummy_coinbase.total_sigop_cost(|_| None) as u16;
        debug!(max_sigops, "Calculated max sigops for coinbase");

        let constraints = CoinbaseOutputConstraints {
            coinbase_output_max_additional_size: max_size,
            coinbase_output_max_additional_sigops: max_sigops,
        };

        let msg = AnyMessage::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputConstraints(constraints),
        );

        let frame: StdFrame = msg.try_into()?;
        info!("Sending CoinbaseOutputConstraints message upstream");
        self.template_receiver_channel
            .outbound_tx
            .send(frame.into())
            .await
            .map_err(|_| {
                error!("Failed to send CoinbaseOutputConstraints message upstream");
                JDCError::ChannelErrorSender
            })?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn setup_connection(&mut self, addr: String) -> Result<(), JDCError> {
        let socket: SocketAddr = addr.parse().map_err(|_| {
            error!(%addr, "Invalid socket address");
            JDCError::InvalidSocketAddress(addr.clone())
        })?;

        info!(%socket, "Building setup connection message for upstream");
        let setup_msg = get_setup_connection_message_tp(socket);
        let frame: StdFrame = Message::Common(setup_msg.into()).try_into()?;

        info!("Sending setup connection message to upstream");
        self.template_receiver_channel
            .outbound_tx
            .send(frame.into())
            .await
            .map_err(|_| {
                error!("Failed to send setup connection message upstream");
                JDCError::ChannelErrorSender
            })?;

        info!("Waiting for upstream handshake response");
        let mut incoming: StdFrame = self
            .template_receiver_channel
            .inbound_rx
            .recv()
            .await
            .map_err(|e| {
                error!(?e, "Upstream connection closed during handshake");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?
            .try_into()?;

        let msg_type = incoming
            .get_header()
            .ok_or(framing_sv2::Error::ExpectedHandshakeFrame)?
            .msg_type();
        debug!(?msg_type, "Received upstream handshake response");

        self.handle_common_message_from_server(msg_type, incoming.payload())
            .await?;
        info!("Handshake with upstream completed successfully");
        Ok(())
    }
}
