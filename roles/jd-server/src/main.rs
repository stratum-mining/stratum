//! Entry point for the Job Declarator Server (JDS).
//!
//! This binary parses CLI arguments, loads the TOML configuration file, and
//! starts the main runtime defined in `jd_server::JobDeclaratorServer`.
//!
//! The actual task orchestration and shutdown logic are managed in `lib/mod.rs`.

use clap::Parser;
use jd_server::{config::JobDeclaratorServerConfig, JobDeclaratorServer};
use tracing::error;

use ext_config::{Config, File, FileFormat};

/// CLI argument parser for the JDS binary.
///
/// Supports the following flags:
/// - `-c`, `--config`: specify a custom config file path
/// - `-h`, `--help`: print help and usage info
#[derive(Parser, Debug)]
#[command(author, version, about = "Job Declarator Server (JDS)", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "jds-config.toml"
    )]
    pub config_path: std::path::PathBuf,
}

/// Entrypoint for the Job Declarator Server binary.
///
/// Loads the configuration from TOML and initializes the main runtime
/// defined in `jd_server::JobDeclaratorServer`. Errors during startup are logged.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let config_path = args.config_path.to_str().expect("Invalid config path");

    // Load config
    let config: JobDeclaratorServerConfig = match Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
    {
        Ok(settings) => match settings.try_deserialize::<JobDeclaratorServerConfig>() {
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

    let _ = JobDeclaratorServer::new(config).start().await;
}
