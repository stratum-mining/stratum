pub mod config;
pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;

use async_channel::{bounded, unbounded};

use config::PoolConfig;
use error::PoolError;
use mining_pool::{get_coinbase_output, Pool};
use template_receiver::TemplateRx;
use tracing::{error, info, warn};

use tokio::select;

/// Represents the PoolSv2 instance, which manages the pool's operations.
///
/// This struct holds the pool configuration and provides functionality to start
/// and manage the pool, handling both upstream (Template Provider) and downstream connections.
#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
}

impl PoolSv2 {
    /// Creates a new PoolSv2 instance with the given configuration.
    pub fn new(config: PoolConfig) -> PoolSv2 {
        PoolSv2 { config }
    }

    /// Starts the Pool-SV2 server and manages upstream and downstream connections.
    ///
    /// - Initializes a Template Receiver client to connect with the Template Provider.
    /// - Sets up a server for downstream miners to connect.
    /// - Creates multiple channels for communication between components.
    /// - Monitors system health and handles shutdown conditions.
    pub async fn start(&self) -> Result<(), PoolError> {
        let config = self.config.clone();
        // Channels for internal communication between Template Receiver and downstream miners.
        let (status_tx, status_rx) = unbounded(); // Monitors status of both upstream and downstream.
        let (s_new_t, r_new_t) = bounded(10); // New template updates.
        let (s_prev_hash, r_prev_hash) = bounded(10); // Previous hash updates.
        let (s_solution, r_solution) = bounded(10); // Solution submissions from downstream.
                                                    // This channel does something weird, it sends zero sized data from downstream upon
                                                    // retrieval of any message from template receiver, and make the template receiver
                                                    // wait until it receivers confirmation from downstream. Can be removed.
        let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
        let coinbase_output_result = get_coinbase_output(&config);
        let coinbase_output_len = coinbase_output_result?.len() as u32;
        let tp_authority_public_key = config.tp_authority_public_key();

        // Establish a connection to the Template Provider.
        TemplateRx::connect(
            config.tp_address().parse().unwrap(),
            s_new_t,
            s_prev_hash,
            r_solution,
            r_message_recv_signal,
            status::Sender::Upstream(status_tx.clone()),
            coinbase_output_len,
            tp_authority_public_key.cloned(),
        )
        .await?;

        // Start the pool server, allowing miners to connect.
        let pool = Pool::start(
            config.clone(),
            r_new_t,
            r_prev_hash,
            s_solution,
            s_message_recv_signal,
            status::Sender::DownstreamListener(status_tx),
            config.shares_per_minute(),
        );

        // Monitor the status of Template Receiver and downstream connections.
        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
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
