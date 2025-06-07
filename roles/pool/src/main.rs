//! Pool CLI entry point.
//!
//! This binary parses CLI arguments, loads the TOML configuration,
//! and starts the main runtime via `pool_sv2::start`.
//!
//! Task orchestration and shutdown are handled in `lib/mod.rs`.

use clap::Parser;
use ext_config::{Config, File, FileFormat};
pub use pool_sv2::{config, status, PoolSv2};
use tokio::select;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about = "Pool CLI", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "pool-config.toml"
    )]
    pub config_path: std::path::PathBuf,
}

/// Initializes logging, parses arguments, loads configuration, and starts the Pool runtime.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let config_path = args.config_path.to_str().expect("Invalid config path");

    // Load config
    let config: config::PoolConfig = match Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
    {
        Ok(settings) => match settings.try_deserialize::<config::PoolConfig>() {
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
