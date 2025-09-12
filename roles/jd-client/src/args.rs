use clap::Parser;
use ext_config::{Config, File, FileFormat};
use jd_client_sv2::{config::JobDeclaratorClientConfig, error::JDCError};

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

#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> Result<JobDeclaratorClientConfig, JDCError> {
    let args = Args::parse();

    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        JDCError::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    let mut config = settings.try_deserialize::<JobDeclaratorClientConfig>()?;

    config.set_log_file(args.log_file);

    Ok(config)
}
