//! CLI argument parsing for the Mining Proxy binary.
//!
//! Defines the `Args` struct and a function to process CLI arguments into a MiningProxyConfig.

use clap::Parser;
use ext_config::{Config, File, FileFormat};
use mining_proxy_sv2::{error::Error, MiningProxyConfig};
use std::path::PathBuf;
use tracing::error;

/// Holds the parsed CLI arguments for the Mining Proxy binary.
#[derive(Parser, Debug)]
#[command(author, version, about = "Mining Proxy", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "proxy-config.toml"
    )]
    pub config_path: PathBuf,
}

/// Process CLI args and load configuration.
#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> Result<MiningProxyConfig, Error> {
    // Parse CLI arguments
    let args = Args::parse();

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        Error::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
        .map_err(|e| {
            error!("Failed to build config: {}", e);
            Error::BadCliArgs
        })?;

    // Deserialize settings into MiningProxyConfig
    let config = settings
        .try_deserialize::<MiningProxyConfig>()
        .map_err(|e| {
            error!("Failed to deserialize config: {}", e);
            Error::BadCliArgs
        })?;
    Ok(config)
}
