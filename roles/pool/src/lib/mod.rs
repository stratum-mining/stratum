pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;

use std::sync::Arc;

use async_channel::{bounded, unbounded};

use error::PoolError;
use mining_pool::{get_coinbase_output, Configuration, Pool};
use template_receiver::TemplateRx;
use tracing::{error, info, warn};

use tokio::{select, sync::Notify, task};

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: Configuration,
    shutdown: Arc<Notify>,
}

impl PoolSv2 {
    pub fn new(config: Configuration) -> PoolSv2 {
        PoolSv2 {
            config,
            shutdown: Arc::new(Notify::new()),
        }
    }

    pub async fn start(&self) -> Result<(), PoolError> {
        let config = self.config.clone();
        // Single consumer, multiple producer. (mpsc can be used)
        // consumer on some line in start method we are using it.
        // producer are used in templateRx::connect and Pool start
        // producers are clonable so no issue. but its unbounded.
        // tokio also provide unbounded mpsc.
        // let (status_tx, status_rx) = unbounded();
        let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
        // r_new_t consumer is sent in pool::start,  s_new_t is sent in templateRx::connect
        // sender or producer I dont give a damn about. even the r_new_t is passed in only
        // start then to on_new_template, so mpsc makes sense here as well.
        // let (s_new_t, r_new_t) = bounded(10);
        let (s_new_t, r_new_t) = tokio::sync::mpsc::channel(10);
        // s_prev_hash (no one gives a damn about clonable stuff), which is only passed to
        // TemplateRx and nothing new.  r_prev_hash is sent to pool::start which is also
        // sent to on_new_prevhash, so mpsc also works here.
        // let (s_prev_hash, r_prev_hash) = bounded(10);
        let (s_prev_hash, r_prev_hash) = tokio::sync::mpsc::channel(10);
        // s_solution is sent to pool (no one give a damn about clonable), r_solution is sent
        // to templateRx and then to on_new_solution, so mpsc works.
        let (s_solution, r_solution) = tokio::sync::mpsc::channel(10);
        // This is spicy, as the r_message_recv_signal is cloning at few of the places, so, we can
        // use broadcast.
        // let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
        let (s_message_recv_signal, _) = tokio::sync::broadcast::channel(10);
        let coinbase_output_result = get_coinbase_output(&config);
        let coinbase_output_len = coinbase_output_result?.len() as u32;
        let tp_authority_public_key = config.tp_authority_public_key;
        TemplateRx::connect(
            config.tp_address.parse().unwrap(),
            s_new_t,
            s_prev_hash,
            r_solution,
            s_message_recv_signal.clone(),
            status::Sender::UpstreamTokio(status_tx.clone()),
            coinbase_output_len,
            tp_authority_public_key,
        )
        .await?;
        let pool = Pool::start(
            config.clone(),
            r_new_t,
            r_prev_hash,
            s_solution,
            s_message_recv_signal,
            status::Sender::DownstreamListenerTokio(status_tx),
            self.shutdown.clone(),
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
        let invalid_coinbase_output = vec![mining_pool::CoinbaseOutput::new(
            "P2PK".to_string(),
            "wrong".to_string(),
        )];
        let config_path = "config-examples/pool-config-hosted-tp-example.toml";
        let mut config: Configuration = match Config::builder()
            .add_source(File::new(config_path, FileFormat::Toml))
            .build()
        {
            Ok(settings) => match settings.try_deserialize::<Configuration>() {
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
        config.coinbase_outputs = invalid_coinbase_output;
        let pool = PoolSv2::new(config);
        let result = pool.start().await;
        assert!(result.is_err());
    }
}
