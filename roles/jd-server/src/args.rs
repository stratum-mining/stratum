use std::path::PathBuf;

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
    #[arg(
        short = 'f',
        long = "log-file",
        help = "Path to the log file. If not set, logs will only be written to stdout."
    )]
    pub log_file: Option<PathBuf>,
    #[arg(
        long = "log-level",
        help = "Log level (error, warn, info, debug, trace)",
        default_value = "info"
    )]
    pub log_level: String,
    #[arg(
        short = 'v',
        long = "verbose-stdout",
        help = "If set, logs will also be written to stdout. Requires --log-file (-f).",
        default_value_t = false,
        action = clap::ArgAction::SetTrue,
        requires= "log_file"
    )]
    pub verbose_stdout: bool,
}

/// Process CLI args and load configuration.
#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> Result<(JobDeclaratorServerConfig, Args), JdsError> {
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
    Ok((config, args))
}
