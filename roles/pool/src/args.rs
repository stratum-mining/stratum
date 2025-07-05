//! CLI argument parsing for the Pool binary.
//!
//! Defines the `Args` struct and a function to process CLI arguments into a PoolConfig.

use clap::Parser;
use ext_config::{Config, File, FileFormat};
use pool_sv2::config::PoolConfig;
use std::path::PathBuf;

/// Holds the parsed CLI arguments for the Pool binary.
#[derive(Parser, Debug)]
#[command(author, version, about = "Pool CLI", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "pool-config.toml"
    )]
    pub config_path: PathBuf,
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

/// Parses CLI arguments and loads the PoolConfig from the specified file.
pub fn process_cli_args() -> (PoolConfig, Args) {
    let args = Args::parse();
    let config_path = args.config_path.to_str().expect("Invalid config path");
    let config: PoolConfig = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
        .and_then(|settings| settings.try_deserialize::<PoolConfig>())
        .expect("Failed to load or deserialize config");
    (config, args)
}
