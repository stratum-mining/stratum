pub mod config;
pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;
use async_channel::{bounded, unbounded};
use config::PoolConfig;
use error::PoolError;
use mining_pool::{get_coinbase_output, Pool};
use std::sync::{Arc, Mutex};
use template_receiver::TemplateRx;
use tokio::select;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
    status_tx: Arc<Mutex<Option<async_channel::Sender<status::Status>>>>,
}

impl PoolSv2 {
    pub fn new(config: PoolConfig) -> PoolSv2 {
        PoolSv2 {
            config,
            status_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), PoolError> {
        let config = self.config.clone();
        let (status_tx, status_rx) = unbounded();

        if let Ok(mut s_tx) = self.status_tx.lock() {
            *s_tx = Some(status_tx.clone());
        } else {
            error!("Failed to access Pool status lock");
            return Err(PoolError::Custom(
                "Failed to access Pool status lock".to_string(),
            ));
        }
        let (send_stop_signal, recv_stop_signal) = tokio::sync::watch::channel(());
        let (s_new_t, r_new_t) = bounded(10);
        let (s_prev_hash, r_prev_hash) = bounded(10);
        let (s_solution, r_solution) = bounded(10);
        let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
        let coinbase_output_result = get_coinbase_output(&config)?;
        let coinbase_output_len = coinbase_output_result.len() as u32;
        let tp_authority_public_key = config.tp_authority_public_key().cloned();
        let coinbase_output_sigops = coinbase_output_result
            .iter()
            .map(|output| output.script_pubkey.count_sigops() as u16)
            .sum::<u16>();

        let tp_address = config.tp_address().clone();
        let cloned_status_tx = status_tx.clone();
        tokio::spawn(async move {
            let _ = TemplateRx::connect(
                tp_address.parse().unwrap(),
                s_new_t,
                s_prev_hash,
                r_solution,
                r_message_recv_signal,
                status::Sender::Upstream(cloned_status_tx),
                coinbase_output_len,
                coinbase_output_sigops,
                tp_authority_public_key,
            )
            .await;
        });
        let pool = Pool::start(
            config.clone(),
            r_new_t,
            r_prev_hash,
            s_solution,
            s_message_recv_signal,
            status::Sender::DownstreamListener(status_tx),
            config.shares_per_minute(),
            recv_stop_signal,
        )
        .await?;
        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
        tokio::spawn(async move {
            loop {
                let task_status = select! {
                    task_status = status_rx.recv() => task_status,
                    interrupt_signal = tokio::signal::ctrl_c() => {
                        match interrupt_signal {
                            Ok(()) => {
                                info!("Interrupt received");
                            },
                            Err(err) => {
                                error!("Unable to listen for interrupt signal: {}", err);
                                // we also shut down in case of error
                            },
                        }
                        break;
                    }
                };
                let task_status: status::Status = task_status.unwrap();

                match task_status.state {
                    status::State::Shutdown => {
                        info!("Received shutdown signal");
                        let _ = send_stop_signal.send(());
                        break;
                    }
                    // Should only be sent by the downstream listener
                    status::State::DownstreamShutdown(err) => {
                        error!(
                            "SHUTDOWN from Downstream: {}\nTry to restart the downstream listener",
                            err
                        );
                        let _ = send_stop_signal.send(());
                        break;
                    }
                    status::State::TemplateProviderShutdown(err) => {
                        error!("SHUTDOWN from Upstream: {}\nTry to reconnecting or connecting to a new upstream", err);
                        let _ = send_stop_signal.send(());
                        break;
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
                            let _ = send_stop_signal.send(());
                            break;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    pub fn shutdown(&self) {
        info!("Attempting to shutdown pool");
        if let Ok(status_tx) = &self.status_tx.lock() {
            if let Some(status_tx) = status_tx.as_ref().cloned() {
                info!("Pool is running, sending shutdown signal");
                tokio::spawn(async move {
                    if let Err(e) = status_tx
                        .send(status::Status {
                            state: status::State::Shutdown,
                        })
                        .await
                    {
                        error!("Failed to send shutdown signal to status loop: {:?}", e);
                    } else {
                        info!("Sent shutdown signal to Pool");
                    }
                });
            } else {
                info!("Pool is not running.");
            }
        } else {
            error!("Failed to access Pool status lock");
        }
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

    #[tokio::test]
    async fn shutdown_pool() {
        let template_provider = integration_tests_sv2::start_template_provider(None);
        let config_path = "config-examples/pool-config-local-tp-example.toml";
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
        config.set_tp_address(template_provider.1.to_string());
        let pool_0 = PoolSv2::new(config.clone());
        let pool_1 = PoolSv2::new(config);
        assert!(pool_0.start().await.is_ok());
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        assert!(pool_1.start().await.is_err());
        pool_0.shutdown();
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        assert!(pool_1.start().await.is_ok());
    }
}
