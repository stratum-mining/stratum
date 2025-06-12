use clap::Parser;
use ext_config::{Config, File, FileFormat};
use jd_server::{
    config::JobDeclaratorServerConfig,
    error::JdsError,
    // error::{Error, ProxyResult},
};

use tracing::error;

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

/// Process CLI args and load configuration.
#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> Result<JobDeclaratorServerConfig, JdsError> {
    // Parse CLI arguments
    let args = Args::parse();

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        JdsError::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
        .map_err(|e| {
            error!("Failed to build config: {}", e);
            JdsError::BadCliArgs
        })?;

    // Deserialize settings into JobDeclaratorServerConfig
    let config = settings
        .try_deserialize::<JobDeclaratorServerConfig>()
        .map_err(|e| {
            error!("Failed to deserialize config: {}", e);
            JdsError::BadCliArgs
        })?;
    Ok(config)
}
