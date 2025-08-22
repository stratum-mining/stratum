use std::{collections::HashMap, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        channels_sv2::server::{
            extended::ExtendedChannel, group::GroupChannel, standard::StandardChannel,
        },
        codec_sv2::{self, Frame, Sv2Frame},
        common_messages_sv2::SetupConnectionSuccess,
        handlers_sv2::HandleCommonMessagesFromClientAsync,
        mining_sv2::{SetTarget, Target},
        parsers_sv2::{AnyMessage, IsSv2Message, Mining},
        utils::{Id as IdFactory, Mutex},
        Vardiff,
    },
};

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, instrument, warn, Instrument, Span};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        message_from_frame, spawn_io_tasks, spawn_io_tasks_tracing, EitherFrame, Message,
        ShutdownMessage, StdFrame,
    },
};

mod message_handler;

pub struct DownstreamData {
    pub require_std_job: bool,
    pub group_channels: Option<GroupChannel<'static>>,
    pub extended_channels: HashMap<u32, ExtendedChannel<'static>>,
    pub standard_channels: HashMap<u32, StandardChannel<'static>>,
    pub vardiff: HashMap<u32, Box<dyn Vardiff>>,
}

#[derive(Clone)]
pub struct DownstreamChannel {
    channel_manager_sender: Sender<(u32, EitherFrame)>,
    channel_manager_receiver: broadcast::Sender<(u32, Message)>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct Downstream {
    pub downstream_data: Arc<Mutex<DownstreamData>>,
    downstream_channel: DownstreamChannel,
    pub downstream_id: u32,
}

impl Downstream {
    #[instrument(
        skip_all,
        fields(downstream_id = downstream_id),
        parent = Span::current()
    )]
    pub fn new(
        downstream_id: u32,
        channel_manager_sender: Sender<(u32, EitherFrame)>,
        channel_manager_receiver: broadcast::Sender<(u32, Message)>,
        noise_stream: NoiseTcpStream<Message>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Self {
        let (noise_stream_reader, noise_stream_writer) = noise_stream.into_split();
        let status_sender = StatusSender::Downstream {
            downstream_id,
            tx: status_sender,
        };
        let (inbound_tx, inbound_rx) = unbounded::<EitherFrame>();
        let (outbound_tx, outbound_rx) = unbounded::<EitherFrame>();
        spawn_io_tasks_tracing(
            task_manager,
            noise_stream_reader,
            noise_stream_writer,
            outbound_rx,
            inbound_tx,
            notify_shutdown,
            status_sender,
            Span::current(),
        );

        let downstream_channel = DownstreamChannel {
            channel_manager_receiver,
            channel_manager_sender,
            outbound_tx,
            inbound_rx,
        };
        let downstream_data = Arc::new(Mutex::new(DownstreamData {
            require_std_job: false,
            extended_channels: HashMap::new(),
            standard_channels: HashMap::new(),
            group_channels: None,
            vardiff: HashMap::new(),
        }));
        Downstream {
            downstream_channel,
            downstream_data,
            downstream_id,
        }
    }

    #[tracing::instrument(skip_all, fields(downstream_id = self.downstream_id))]
    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::Downstream {
            downstream_id: self.downstream_id,
            tx: status_sender,
        };

        let mut shutdown_rx = notify_shutdown.subscribe();
        let self_clone = self.clone();
        let status_sender_clone = status_sender.clone();

        // Setup initial connection
        if let Err(e) = self.setup_connection().await {
            error!(?e, "Failed to set up downstream connection");
            handle_error(&status_sender, e).await;
            return;
        }

        task_manager.spawn(async move {
            let mut shutdown_rx = notify_shutdown.subscribe();
            let downstream_id = self_clone.downstream_id;
            info!("Spawning vardiff adjustment loop for downstream: {downstream_id}");
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Vardiff for downstream {downstream_id}: received global shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(id)) if id == downstream_id => {
                                info!("Vardiff for downstream {downstream_id}: received shutdown order");
                                break;
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdown)  => {
                                debug!("Downstream {downstream_id}: Received job declaratorShutdown shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamShutdown)  => {
                                debug!("Downstream {downstream_id}: Received job Upstream shutdown");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = self_clone.spawn_vardiff() => {
                        if let Err(e) = res {
                            error!(?e, "Vardiff loop failed for downstream {downstream_id}");
                            handle_error(&status_sender_clone, e).await;
                            break;
                        }
                    }
                }
            }
            debug!("Vardiff loop exited for downstream {downstream_id}");
        }.instrument(Span::current()));

        let mut receiver = self.downstream_channel.channel_manager_receiver.subscribe();
        task_manager.spawn(async move {
            loop {
                let mut self_clone_1 = self.clone();
                let downstream_id = self_clone_1.downstream_id;
                let mut self_clone_2 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                debug!("Downstream {downstream_id}: Received global shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(id)) if downstream_id == id => {
                                debug!("Downstream {downstream_id}: Received downstream {id} shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdown)  => {
                                debug!("Downstream {downstream_id}: Received job declaratorShutdown shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamShutdown)  => {
                                debug!("Downstream {downstream_id}: Received job Upstream shutdown");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = self_clone_1.handle_downstream_message() => {
                        if let Err(e) = res {
                            error!(?e, "Error handling downstream message for {downstream_id}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_channel_manager_message(&mut receiver) => {
                        if let Err(e) = res {
                            error!(?e, "Error handling channel manager message for {downstream_id}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                }
            }
            warn!("Downstream: unified message loop exited.");
        }.instrument(Span::current()));
    }

    #[instrument(skip_all)]
    async fn setup_connection(&mut self) -> Result<(), JDCError> {
        let mut frame = self.downstream_channel.inbound_rx.recv().await?;
        let (msg_type, mut payload, parsed) = message_from_frame(&mut frame).map_err(|e| {
            error!(?e, "Failed to parse incoming frame");
            e
        })?;

        if let AnyMessage::Common(_) = parsed {
            self.handle_common_message_from_client(msg_type, &mut payload)
                .await?;
            return Ok(());
        }

        Err(JDCError::UnexpectedMessage)
    }

    #[instrument(name = "channel_manager_message", skip_all)]
    async fn handle_channel_manager_message(
        mut self,
        receiver: &mut broadcast::Receiver<(u32, AnyMessage<'static>)>,
    ) -> Result<(), JDCError> {
        let (downstream_id, frame) = match receiver.recv().await {
            Ok(msg) => msg,
            Err(e) => {
                warn!(?e, "Broadcast receive failed");
                return Ok(());
            }
        };

        if downstream_id != self.downstream_id {
            debug!(
                ?downstream_id,
                "Message ignored for non-matching downstream"
            );
            return Ok(());
        }

        let message_type = frame.message_type();
        let std_frame = match StdFrame::from_message(frame, message_type, 0, true) {
            Some(f) => f,
            None => {
                debug!("Invalid frame conversion; skipping message");
                return Ok(());
            }
        };

        self.downstream_channel
            .outbound_tx
            .send(std_frame.into())
            .await
            .map_err(|e| {
                error!(?e, "Downstream send failed");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        Ok(())
    }

    #[instrument(name = "downstream_message", skip_all)]
    async fn handle_downstream_message(mut self) -> Result<(), JDCError> {
        let read_frame = self.downstream_channel.inbound_rx.recv().await?;

        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();
                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        self.handle_common_message_from_client(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::Mining(_) => {
                        self.downstream_channel
                            .channel_manager_sender
                            .send((self.downstream_id, EitherFrame::Sv2(std_frame)))
                            .await
                            .map_err(|e| {
                                error!(?e, "Failed to send mining message to channel manager");
                                JDCError::ChannelErrorSender
                            })?;
                    }
                    _ => {
                        error!("Unsupported message type from upstream");
                        return Err(JDCError::UnexpectedMessage);
                    }
                }
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!(?handshake_frame, "Received handshake frame");
            }
        }
        Ok(())
    }

    #[instrument(
        name = "vardiff_on_extended_channel",
        skip_all,
        fields(channel_id = channel_id, hashrate = %channel_state.get_nominal_hashrate(), target = ?channel_state.get_target()),
    )]
    fn run_vardiff_on_extended_channel(
        channel_id: u32,
        channel_state: &mut ExtendedChannel<'static>,
        vardiff_state: &mut Box<dyn Vardiff>,
        updates: &mut Vec<(u32, AnyMessage)>,
    ) {
        let (hashrate, target, shares_per_minute) = (
            channel_state.get_nominal_hashrate(),
            channel_state.get_target(),
            channel_state.get_shares_per_minute(),
        );

        let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
        else {
            debug!("Vardiff computation failed for extended channel {channel_id}");
            return;
        };

        let Some(new_hashrate) = new_hashrate_opt else {
            return;
        };

        match channel_state.update_channel(new_hashrate, None) {
            Ok(()) => {
                let updated_target = channel_state.get_target();
                updates.push((
                    channel_id,
                    AnyMessage::Mining(Mining::SetTarget(SetTarget {
                        channel_id,
                        maximum_target: updated_target.clone().into(),
                    })),
                ));
                debug!("Updated target for extended channel_id={channel_id} to {updated_target:?}",);
            }
            Err(_) => warn!("Failed to update extended channel {channel_id}"),
        }
    }

    #[instrument(
        name = "vardiff_on_standard_channel",
        skip_all,
        fields(
            channel_id = channel_id,
            hashrate = %channel.get_nominal_hashrate(),
            target = ?channel.get_target(),
        )
    )]
    fn run_vardiff_on_standard_channel(
        channel_id: u32,
        channel: &mut StandardChannel<'static>,
        vardiff_state: &mut Box<dyn Vardiff>,
        updates: &mut Vec<(u32, AnyMessage)>,
    ) {
        let hashrate = channel.get_nominal_hashrate();
        let target = channel.get_target();
        let shares_per_minute = channel.get_shares_per_minute();

        let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
        else {
            debug!("Vardiff computation failed for standard channel {channel_id}");
            return;
        };

        if let Some(new_hashrate) = new_hashrate_opt {
            match channel.update_channel(new_hashrate, None) {
                Ok(()) => {
                    let updated_target = channel.get_target();
                    updates.push((
                        channel_id,
                        AnyMessage::Mining(Mining::SetTarget(SetTarget {
                            channel_id,
                            maximum_target: updated_target.clone().into(),
                        })),
                    ));
                    debug!(?updated_target, "Updated target for standard channel");
                }
                Err(_) => warn!("Failed to update standard channel {channel_id}"),
            }
        }
    }

    #[instrument(skip_all)]
    async fn spawn_vardiff(&self) -> Result<(), JDCError> {
        let downstream_id = self.downstream_id;

        if self.downstream_channel.outbound_tx.is_closed() {
            debug!("Downstream {downstream_id} closed, stopping vardiff loop");
            return Err(JDCError::ChannelErrorSender);
        }

        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        debug!("Starting vardiff updates for downstream: {downstream_id}");

        let messages = self.downstream_data.super_safe_lock(|data| {
            let mut messages = vec![];
            for (channel_id, vardiff_state) in data.vardiff.iter_mut() {
                if let Some(channel) = data.extended_channels.get_mut(channel_id) {
                    Self::run_vardiff_on_extended_channel(
                        *channel_id,
                        channel,
                        vardiff_state,
                        &mut messages,
                    );
                }
                if let Some(channel) = data.standard_channels.get_mut(channel_id) {
                    Self::run_vardiff_on_standard_channel(
                        *channel_id,
                        channel,
                        vardiff_state,
                        &mut messages,
                    );
                }
            }
            messages
        });

        for (channel_id, target) in messages {
            let frame: StdFrame = target.try_into()?;
            if let Err(e) = self.downstream_channel.outbound_tx.send(frame.into()).await {
                error!(
                    downstream_id = downstream_id,
                    channel_id = channel_id,
                    error = ?e,
                    "Failed to send SetTarget message downstream"
                );
                return Err(JDCError::ChannelErrorSender);
            }
            debug!(
                downstream_id = downstream_id,
                channel_id = channel_id,
                "SetTarget message successfully sent"
            );
        }
        info!(
            downstream_id = downstream_id,
            "Vardiff update cycle complete"
        );
        Ok(())
    }
}
