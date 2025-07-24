//! Pool CLI entry point.
//!
//! This binary parses CLI arguments, loads the TOML configuration,
//! and starts the main runtime via `pool_sv2::start`.
//!
//! Task orchestration and shutdown are handled in `lib/mod.rs`.

pub use pool_sv2::{config, status, PoolSv2};
use tokio::select;
use tracing::{error, info};

mod args;
use args::process_cli_args;
use config_helpers_sv2::logging::init_logging;

/// Initializes logging, parses arguments, loads configuration, and starts the Pool runtime.
#[tokio::main]
async fn main() {
    let config = process_cli_args();
    init_logging(config.log_dir());
    let _ = PoolSv2::new(config).start().await;
    select! {
        interrupt_signal = tokio::signal::ctrl_c() => {
            match interrupt_signal {
                Ok(()) => {
                    info!("Pool(bin): Caught interrupt signal. Shutting down...");
                    return;
                },
                Err(err) => {
                    error!("Pool(bin): Unable to listen for interrupt signal: {}", err);
                    return;
                },
            }
        }
    };
}
