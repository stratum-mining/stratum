pub mod config;
pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;

use std::sync::Arc;

use async_channel::{bounded, unbounded};
use config::PoolConfig;
use error::PoolError;
use mining_pool::{get_coinbase_output, Pool};
use template_receiver::TemplateRx;
use tokio::select;
use tracing::{error, info, warn};

use tokio::{select, sync::Notify, task};

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
    shutdown: Arc<Notify>,
}

impl PoolSv2 {
    pub fn new(config: PoolConfig) -> PoolSv2 {
        PoolSv2 {
            config,
            shutdown: Arc::new(Notify::new()),
        }
    }

    pub async fn start(&self) -> Result<(), PoolError> {
        let config = self.config.clone();
        let (status_tx, status_rx) = unbounded();
        let (s_new_t, r_new_t) = bounded(10);
        let (s_prev_hash, r_prev_hash) = bounded(10);
        let (s_solution, r_solution) = bounded(10);
        let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
        let coinbase_output_result = get_coinbase_output(&config)?;
        let coinbase_output_len = coinbase_output_result.len() as u32;
        let tp_authority_public_key = config.tp_authority_public_key();
        let coinbase_output_sigops = coinbase_output_result
            .iter()
            .map(|output| output.script_pubkey.count_sigops() as u16)
            .sum::<u16>();

        TemplateRx::connect(
            config.tp_address().parse().unwrap(),
            s_new_t,
            s_prev_hash,
            r_solution,
            r_message_recv_signal,
            status::Sender::Upstream(status_tx.clone()),
            coinbase_output_len,
            coinbase_output_sigops,
            tp_authority_public_key.cloned(),
        )
        .await?;
        let pool = Pool::start(
            config.clone(),
            r_new_t,
            r_prev_hash,
            s_solution,
            s_message_recv_signal,
            status::Sender::DownstreamListener(status_tx),
            config.shares_per_minute(),
        );

        task::spawn({
            let shutdown_signal = self.shutdown.clone();
            async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    info!("Interrupt received");
                    shutdown_signal.notify_waiters();
                }
            }
        });

        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
        loop {
            let task_status = select! {
                task_status = status_rx.recv() => task_status,
                _ = self.shutdown.notified() => {
                    info!("Shutting down gracefully...");
                    break Ok(());
                }
            };
            let task_status: status::Status = task_status.unwrap();

            match task_status.state {
                // Should only be sent by the downstream listener
                status::State::DownstreamShutdown(err) => {
                    error!(
                        "SHUTDOWN from Downstream: {}\nTry to restart the downstream listener",
                        err
                    );
                    break Ok(());
                }
                status::State::TemplateProviderShutdown(err) => {
                    error!("SHUTDOWN from Upstream: {}\nTry to reconnecting or connecting to a new upstream", err);
                    break Ok(());
                }
                status::State::Healthy(msg) => {
                    info!("HEALTHY message: {}", msg);
                }
                status::State::DownstreamInstanceDropped(downstream_id) => {
                    warn!("Dropping downstream instance {} from pool", downstream_id);
                    if pool
                        .safe_lock(|p| p.remove_downstream(downstream_id))
                        .is_err()
                    {
                        break Ok(());
                    }
                }
            }
        }
    }

    /// Notifies the Pool to shut down gracefully.
    ///
    /// This method triggers the shutdown process by sending a notification.
    /// It ensures that any ongoing operations are properly handled before
    /// the pool stops functioning.
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ext_config::{Config, File, FileFormat};

    #[tokio::test]
    async fn pool_bad_coinbase_output() {
        let invalid_coinbase_output = vec![config::CoinbaseOutput::new(
            "P2PK".to_string(),
            "wrong".to_string(),
        )];
        let config_path = "config-examples/pool-config-hosted-tp-example.toml";
        let mut config: PoolConfig = match Config::builder()
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
        config.set_coinbase_outputs(invalid_coinbase_output);
        let pool = PoolSv2::new(config);
        let result = pool.start().await;
        assert!(result.is_err());
    }
}
