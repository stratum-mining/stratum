pub mod error;
pub mod mining_pool;
pub mod status;
pub mod template_receiver;

use std::sync::Arc;

use async_channel::{bounded, unbounded};

use error::PoolError;
use mining_pool::{get_coinbase_output, Configuration, Pool};
use roles_logic_sv2::utils::Mutex;
use template_receiver::TemplateRx;
use tracing::{error, info, warn};

use tokio::select;

#[derive(Debug, Clone, PartialEq)]
pub struct DroppedDownstreams(Vec<u32>);

impl DroppedDownstreams {
    pub fn new() -> DroppedDownstreams {
        DroppedDownstreams(vec![])
    }
    pub fn push(&mut self, id: u32) {
        self.0.push(id);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PoolState {
    Initial,
    Running(DroppedDownstreams),
    DownstreamInstanceDropped,
}

#[derive(Debug, Clone)]
pub struct PoolSv2 {
    config: Configuration,
    state: Arc<Mutex<PoolState>>,
}

impl PoolSv2 {
    pub fn new(config: Configuration) -> PoolSv2 {
        PoolSv2 {
            config,
            state: Arc::new(Mutex::new(PoolState::Initial)),
        }
    }

    pub async fn start(&self) -> Result<(), PoolError> {
        let config = self.config.clone();
        let (status_tx, status_rx) = unbounded();
        let (s_new_t, r_new_t) = bounded(10);
        let (s_prev_hash, r_prev_hash) = bounded(10);
        let (s_solution, r_solution) = bounded(10);
        let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
        let coinbase_output_result = get_coinbase_output(&config);
        let coinbase_output_len = coinbase_output_result?.len() as u32;
        let tp_authority_public_key = config.tp_authority_public_key;
        TemplateRx::connect(
            config.tp_address.parse().unwrap(),
            s_new_t,
            s_prev_hash,
            r_solution,
            r_message_recv_signal,
            status::Sender::Upstream(status_tx.clone()),
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
            status::Sender::DownstreamListener(status_tx),
        );
        // Set the state to running
        let _ = self
            .state
            .safe_lock(|s| *s = PoolState::Running(DroppedDownstreams(vec![])));

        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
        let internal_state = self.state.clone();
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
                        error!(
                            "Failed to remove downstream instance {}, shutting down.",
                            downstream_id
                        );
                        break Ok(());
                    }
                    // Add the downstream id to the list of removed downstreams
                    internal_state
                        .safe_lock(|s| {
                            let mut current = s.clone();
                            match current {
                                PoolState::Running(ref mut removed) => {
                                    let new = DroppedDownstreams(
                                        removed
                                            .0
                                            .iter()
                                            .cloned()
                                            .chain(std::iter::once(downstream_id))
                                            .collect(),
                                    );
                                    *s = PoolState::Running(new);
                                }
                                _ => {
                                    // This should.. never happen
                                    warn!("Downstream instance dropped but pool is not running");
                                }
                            }
                        })
                        .expect("Failed to set state to DownstreamInstanceDropped");
                }
            }
        }
    }

    pub async fn state(&self) -> Arc<Mutex<PoolState>> {
        self.state.clone()
    }
}
