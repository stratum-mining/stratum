//! Configurable Sv2 it support extended and group channel
//! Upstream means another proxy or a pool
//! Downstream means another proxy or a mining device
//!
//! UpstreamMining is the trait that a proxy must implement in order to
//! understant Downstream mining messages.
//!
//! DownstreamMining is the trait that a proxy must implement in order to
//! understand Upstream mining messages
//!
//! Same thing for DownstreamCommon and UpstreamCommon but for common messages
//!
//! DownstreamMiningNode implement both UpstreamMining and UpstreamCommon
//!
//! UpstreamMiningNode implement both DownstreamMining and DownstreamCommon
//!
//! A Downstream that signal the capacity to handle group channels can open more than one channel.
//! A Downstream that signal the incapacity to handle group channels can open only one channel.
//!
#![allow(special_module_name)]
mod lib;

use lib::Config;
use roles_logic_sv2::utils::{GroupId, Mutex};
use std::{net::SocketAddr, sync::Arc};
use tracing::{error, info};

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
        const DEFAULT_CONFIG_PATH: &'static str = "proxy-config.toml";
        const HELP_MSG: &'static str =
            "Usage: -h/--help, -c/--config <path|default proxy-config.toml>";

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

/// 1. the proxy scan all the upstreams and map them
/// 2. donwstream open a connetcion with proxy
/// 3. downstream send SetupConnection
/// 4. a mining_channle::Upstream is created
/// 5. upstream_mining::UpstreamMiningNodes is used to pair this downstream with the most suitable
///    upstream
/// 6. mining_channle::Upstream create a new downstream_mining::DownstreamMiningNode embedding
///    itself in it
/// 7. normal operation between the paired downstream_mining::DownstreamMiningNode and
///    upstream_mining::UpstreamMiningNode begin
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

    // Scan all the upstreams and map them
    let config_file = std::fs::read_to_string(args.config_path.clone())
        .unwrap_or_else(|_| panic!("Can not open {:?}", args.config_path));
    let config = match toml::from_str::<Config>(&config_file) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to parse config file: {}", e);
            return;
        }
    };

    let group_id = Arc::new(Mutex::new(GroupId::new()));
    lib::ROUTING_LOGIC
        .set(Mutex::new(
            lib::initialize_r_logic(&config.upstreams, group_id, config.clone()).await,
        ))
        .expect("BUG: Failed to set ROUTING_LOGIC");
    info!("PROXY INITIALIZING");
    lib::initialize_upstreams(config.min_supported_version, config.max_supported_version).await;
    info!("PROXY INITIALIZED");

    // Wait for downstream connection
    let socket = SocketAddr::new(
        config.listen_address.parse().unwrap(),
        config.listen_mining_port,
    );

    info!("PROXY INITIALIZED");
    crate::lib::downstream_mining::listen_for_downstream_mining(socket).await
}
