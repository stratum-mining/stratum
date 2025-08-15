//! # Pool Module
//! Core logic for running the mining pool server.
//!
//! Responsibilities:
//! - Spawns the Template Receiver client.
//! - Starts the Pool server for downstream miners.
//! - Monitors Pool status and handles shutdowns.
pub mod config;
pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;
use async_channel::{bounded, unbounded};
use config::PoolConfig;
use error::PoolError;
use mining_pool::Pool;
use std::sync::{Arc, Mutex};
use stratum_common::roles_logic_sv2::bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    script::ScriptBuf,
    transaction::{OutPoint, Transaction, Version},
    Amount, Sequence, TxIn, TxOut,
};
use template_receiver::TemplateRx;
use tokio::select;
use tracing::{error, info, warn};
/// Represents the PoolSv2 instance, which manages the pool's operations.
///
/// This struct holds the pool configuration and provides functionality to start
/// and manage the pool, handling both upstream (Template Provider) and downstream connections.
#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: PoolConfig,
    status_tx: Arc<Mutex<Option<async_channel::Sender<status::Status>>>>,
}

impl PoolSv2 {
    /// Creates a new PoolSv2 instance with the given configuration.
    pub fn new(config: PoolConfig) -> PoolSv2 {
        PoolSv2 {
            config,
            status_tx: Arc::new(Mutex::new(None)),
        }
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

        if let Ok(mut s_tx) = self.status_tx.lock() {
            *s_tx = Some(status_tx.clone());
        } else {
            error!("Failed to access Pool status lock");
            return Err(PoolError::Custom(
                "Failed to access Pool status lock".to_string(),
            ));
        }
        // Watch channel used to signal the downstream Pool listener to stop.
        let (send_stop_signal, recv_stop_signal) = tokio::sync::watch::channel(());

        // Bounded channels for specific data flow between TemplateRx and Pool.
        let (s_new_t, r_new_t) = bounded(10); // New template updates.
        let (s_prev_hash, r_prev_hash) = bounded(10); // Previous hash updates.
        let (s_solution, r_solution) = bounded(10); // Share solution submissions from downstream.

        // This channel does something weird, it sends zero sized data from downstream upon
        // retrieval of any message from template receiver, and make the template receiver
        // wait until it receivers confirmation from downstream. Can be removed.
        let (s_message_recv_signal, r_message_recv_signal) = bounded(10);

        // Prepare coinbase output information required by TemplateRx.
        // We use an empty output here only for calculation of the size and sigops of the coinbase
        // output. We still don't know the template revenue.
        let empty_coinbase_output = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: config.coinbase_reward_script().script_pubkey(),
        };
        let coinbase_output_len = empty_coinbase_output.size() as u32;
        let tp_authority_public_key = config.tp_authority_public_key().cloned();

        // create a dummy coinbase transaction with the empty output
        // this is used to calculate the sigops of the coinbase output
        let dummy_coinbase = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::from(vec![vec![0; 32]]),
            }],
            output: vec![empty_coinbase_output],
        };

        let coinbase_output_sigops = dummy_coinbase.total_sigop_cost(|_| None) as u16;

        // --- Spawn Template Receiver Task ---
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

        // --- Start Downstream Pool Listener ---
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
        // Monitor the status of Template Receiver and downstream connections.
        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
        // --- Spawn Status Monitoring and Shutdown Handling Loop ---
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

    /// Initiates a graceful shutdown of the running pool instance.
    ///
    /// It attempts to acquire the lock on the `status_tx` mutex. If successful
    /// and the pool is running (i.e., `status_tx` contains a `Some(sender)`),
    /// it sends a `State::Shutdown` message via the status channel.
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
    use integration_tests_sv2::template_provider::DifficultyLevel;

    #[tokio::test]
    async fn shutdown_pool() {
        let template_provider =
            integration_tests_sv2::start_template_provider(None, DifficultyLevel::Low);
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
