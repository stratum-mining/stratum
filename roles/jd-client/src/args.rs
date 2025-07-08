//! ## CLI Arguments Parsing Module
//!
//! This module is responsible for parsing the command-line arguments provided
//! to the application.

use clap::Parser;
use ext_config::{Config, File, FileFormat};
use jd_client::{
    config::JobDeclaratorClientConfig,
    error::{Error, ProxyResult},
};

use std::path::PathBuf;
use tracing::error;
#[derive(Debug, Parser)]
#[command(author, version, about = "JD Client", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "jdc-config.toml"
    )]
    pub config_path: PathBuf,
    #[arg(
        short = 'f',
        long = "log-file",
        help = "Path to the log file. If not set, logs will only be written to stdout."
    )]
    pub log_file: Option<PathBuf>,
}

/// Process CLI args and load configuration.
#[allow(clippy::result_large_err)]
pub fn process_cli_args<'a>() -> ProxyResult<'a, JobDeclaratorClientConfig> {
    // Parse CLI arguments
    let args = Args::parse();

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        Error::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    // Deserialize settings into JobDeclaratorClientConfig
    let mut config = settings.try_deserialize::<JobDeclaratorClientConfig>()?;

    config.set_log_file(args.log_file);

    Ok(config)
}
