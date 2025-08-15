mod args;
use std::process;

use config_helpers_sv2::logging::init_logging;
pub use translator_sv2::{config, error, status, sv1, sv2, TranslatorSv2};

use crate::args::process_cli_args;

/// Entrypoint for the Translator binary.
///
/// Loads the configuration from TOML and initializes the main runtime
/// defined in `translator_sv2::TranslatorSv2`. Errors during startup are logged.
#[tokio::main]
async fn main() {
    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {e}"),
    };

    init_logging(proxy_config.log_dir());

    TranslatorSv2::new(proxy_config).start().await;

    process::exit(1);
}
