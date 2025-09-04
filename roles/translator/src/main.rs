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
    let proxy_config = process_cli_args().unwrap_or_else(|e| {
        eprintln!("Translator proxy config error: {e}");
        std::process::exit(1);
    });

    init_logging(proxy_config.log_dir());

    TranslatorSv2::new(proxy_config).start().await;

    process::exit(1);
}
