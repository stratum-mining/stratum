//! Entry point for the Job Declarator Server (JDS).
//!
//! This binary parses CLI arguments, loads the TOML configuration file, and
//! starts the main runtime defined in `jd_server::JobDeclaratorServer`.
//!
//! The actual task orchestration and shutdown logic are managed in `lib/mod.rs`.
mod args;
use args::process_cli_args;
use config_helpers::logging::init_logging;
use jd_server::{config::JobDeclaratorServerConfig, JobDeclaratorServer};
use tracing::error;

use crate::args::Args;

/// Entrypoint for the Job Declarator Server binary.
///
/// Loads the configuration from TOML and initializes the main runtime
/// defined in `jd_server::JobDeclaratorServer`. Errors during startup are logged.
#[tokio::main]
async fn main() {
    let (config, args): (JobDeclaratorServerConfig, Args) = match process_cli_args() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to process CLI arguments: {}", e);
            return;
        }
    };
    init_logging(args.log_file.as_ref(), &args.log_level, args.verbose_stdout);
    let _ = JobDeclaratorServer::new(config).start().await;
}
