#![allow(warnings)]
use std::sync::Arc;

use async_channel::unbounded;
use stratum_common::roles_logic_sv2::{
    bitcoin::consensus::Encodable, parsers_sv2::TemplateDistribution,
};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::{
    channel_manager::ChannelManager,
    config::PoolConfig,
    error::PoolResult,
    status::{State, Status},
    task_manager::TaskManager,
    template_receiver::TemplateReceiver,
    utils::ShutdownMessage,
};

pub mod channel_manager;
pub mod config;
pub mod downstream;
pub mod error;
pub mod status;
pub mod task_manager;
pub mod template_receiver;
pub mod utils;

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
    notify_shutdown: broadcast::Sender<ShutdownMessage>,
}

impl PoolSv2 {
    pub fn new(config: PoolConfig) -> Self {
        let (notify_shutdown, _) = tokio::sync::broadcast::channel::<ShutdownMessage>(100);
        Self {
            config,
            notify_shutdown,
        }
    }

    /// Starts the Job Declarator Client (JDC) main loop.
    pub async fn start(&self) -> PoolResult<()> {
        // info!(
        //     "Job declarator client starting... setting up subsystems, User Identity: {}",
        //     self.config.user_identity()
        // );

        let miner_coinbase_outputs = vec![self.config.get_txout()];
        let mut encoded_outputs = vec![];

        miner_coinbase_outputs
            .consensus_encode(&mut encoded_outputs)
            .expect("Invalid coinbase output in config");

        let notify_shutdown = self.notify_shutdown.clone();

        let task_manager = Arc::new(TaskManager::new());

        let (status_sender, status_receiver) = async_channel::unbounded::<Status>();

        let (channel_manager_to_downstream_sender, _channel_manager_to_downstream_receiver) =
            broadcast::channel(10);
        let (downstream_to_channel_manager_sender, downstream_to_channel_manager_receiver) =
            unbounded();

        let (channel_manager_to_tp_sender, channel_manager_to_tp_receiver) =
            unbounded::<TemplateDistribution<'static>>();
        let (tp_to_channel_manager_sender, tp_to_channel_manager_receiver) =
            unbounded::<TemplateDistribution<'static>>();

        debug!("Channels initialized.");

        let channel_manager = ChannelManager::new(
            self.config.clone(),
            channel_manager_to_tp_sender.clone(),
            tp_to_channel_manager_receiver.clone(),
            channel_manager_to_downstream_sender.clone(),
            downstream_to_channel_manager_receiver,
            status_sender.clone(),
            encoded_outputs.clone(),
        )
        .await
        .unwrap();

        let channel_manager_clone = channel_manager.clone();

        // Initialize the template Receiver
        let tp_address = self.config.tp_address().to_string();
        let tp_pubkey = self.config.tp_authority_public_key().copied();

        let template_receiver = TemplateReceiver::new(
            tp_address.clone(),
            tp_pubkey,
            channel_manager_to_tp_receiver,
            tp_to_channel_manager_sender,
            notify_shutdown.clone(),
            task_manager.clone(),
            status_sender.clone(),
        )
        .await
        .unwrap();

        info!("Template provider setup done");

        let notify_shutdown_cl = notify_shutdown.clone();
        let status_sender_cl = status_sender.clone();
        let task_manager_cl = task_manager.clone();

        template_receiver
            .start(
                tp_address,
                notify_shutdown_cl,
                status_sender_cl,
                task_manager_cl,
                encoded_outputs.clone(),
            )
            .await;

        channel_manager
            .start(
                notify_shutdown.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await;

        _ = channel_manager_clone
            .clone()
            .start_downstream_server(
                *self.config.authority_public_key(),
                *self.config.authority_secret_key(),
                self.config.cert_validity_sec(),
                *self.config.listen_address(),
                task_manager.clone(),
                notify_shutdown.clone(),
                status_sender.clone(),
                downstream_to_channel_manager_sender.clone(),
                channel_manager_to_downstream_sender.clone(),
            )
            .await;

        info!("Spawning status listener task...");
        let notify_shutdown_clone = notify_shutdown.clone();

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Ctrl+C received — initiating graceful shutdown...");
                    let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                    break;
                }
                message = status_receiver.recv() => {
                    if let Ok(status) = message {
                        match status.state {
                            State::DownstreamShutdown{downstream_id,..} => {
                                warn!("Downstream {downstream_id:?} disconnected — Channel manager.");
                                let _ = notify_shutdown_clone.send(ShutdownMessage::DownstreamShutdown(downstream_id));
                            }
                            State::TemplateReceiverShutdown(_) => {
                                warn!("Template Receiver shutdown requested — initiating full shutdown.");
                                let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                break;
                            }
                            State::ChannelManagerShutdown(_) => {
                                warn!("Channel Manager shutdown requested — initiating full shutdown.");
                                let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                break;
                            }
                        }
                    }
                }
            }
        }

        warn!("Graceful shutdown");
        task_manager.abort_all().await;

        info!("Joining remaining tasks...");
        task_manager.join_all().await;
        info!("JD Client shutdown complete.");
        Ok(())
    }
}
