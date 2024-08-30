#![allow(special_module_name)]

mod lib;
use ext_config::{Config, File, FileFormat};
pub use lib::{mining_pool::Configuration, status, PoolSv2};
use tracing::error;

mod args {
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct Args {
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
        const DEFAULT_CONFIG_PATH: &'static str = "pool-config.toml";
        const HELP_MSG: &'static str =
            "Usage: -h/--help, -c/--config <path|default pool-config.toml>";

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
                        ArgsState::ExpectPath => Some(ArgsResult::Config(PathBuf::from(item))),
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
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = match args::Args::from_args() {
        Ok(cfg) => cfg,
        Err(help) => {
            error!("{}", help);
            return;
        }
    };

    let config_path = args.config_path.to_str().expect("Invalid config path");

    // Load config
    let config: Configuration = match Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()
    {
        Ok(settings) => match settings.try_deserialize::<Configuration>() {
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
    PoolSv2::new(config).start().await;
}
