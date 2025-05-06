//! ## CLI Arguments Parsing Module
//!
//! This module is responsible for parsing the command-line arguments provided
//! to the application.

use std::path::PathBuf;

/// Holds the parsed CLI arguments
#[derive(Debug)]
pub struct Args {
    /// Path to the TOML configuration file.
    pub config_path: PathBuf,
}

enum ArgsState {
    Next,
    ExpectPath,
    Done,
}

enum ArgsResult {
    Config(PathBuf),
    None,
    Help(String),
}

impl Args {
    const DEFAULT_CONFIG_PATH: &'static str = "jdc-config.toml";
    const HELP_MSG: &'static str = "Usage: -h/--help, -c/--config <path|default jdc-config.toml>";

    /// Parses the CLI arguments and returns a populated `Args` struct.
    ///
    /// If no `-c` flag is provided, it defaults to `jdc-config.toml`.
    /// If `--help` is passed, it returns a help message as an error.
    pub fn from_args() -> Result<Self, String> {
        let cli_args = std::env::args();

        if cli_args.len() == 1 {
            println!("Using default config path: {}", Self::DEFAULT_CONFIG_PATH);
            println!("{}\n", Self::HELP_MSG);
        }

        let config_path = cli_args
            .scan(ArgsState::Next, |state, item| {
                match std::mem::replace(state, ArgsState::Done) {
                    ArgsState::Next => match item.as_str() {
                        "-c" | "--config" => {
                            *state = ArgsState::ExpectPath;
                            Some(ArgsResult::None)
                        }
                        "-h" | "--help" => Some(ArgsResult::Help(Self::HELP_MSG.to_string())),
                        _ => {
                            *state = ArgsState::Next;

                            Some(ArgsResult::None)
                        }
                    },
                    ArgsState::ExpectPath => {
                        let path = PathBuf::from(item.clone());
                        if !path.exists() {
                            return Some(ArgsResult::Help(format!(
                                "Error: File '{}' does not exist!",
                                path.display()
                            )));
                        }
                        Some(ArgsResult::Config(path))
                    }
                    ArgsState::Done => None,
                }
            })
            .last();
        let config_path = match config_path {
            Some(ArgsResult::Config(p)) => p,
            Some(ArgsResult::Help(h)) => return Err(h),
            _ => PathBuf::from(Self::DEFAULT_CONFIG_PATH),
        };
        Ok(Self { config_path })
    }
}
