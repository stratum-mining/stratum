//! Defines the structure and parsing logic for command-line arguments.
//!
//! It provides the `Args` struct to hold parsed arguments,
//! and the `from_args` function to parse them from the command line.
use clap::Parser;
use std::path::PathBuf;

/// Holds the parsed CLI arguments.
#[derive(Parser, Debug)]
#[command(author, version, about = "Translator Proxy", long_about = None)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "config",
        help = "Path to the TOML configuration file",
        default_value = "proxy-config.toml"
    )]
    pub config_path: PathBuf,
}
