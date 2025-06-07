//! ## CLI Arguments Parsing Module
//!
//! This module is responsible for parsing the command-line arguments provided
//! to the application.

use clap::Parser;
use std::path::PathBuf;

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
}
