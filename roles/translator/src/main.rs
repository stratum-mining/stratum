mod args;

pub use translator_sv2::{
    config, downstream_sv1, error, proxy, status, upstream_sv2, TranslatorSv2,
};

use tracing::info;

use crate::args::process_cli_args;
use config_helpers::logging::init_logging;
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
    info!("Proxy Config: {:?}", &proxy_config);

    TranslatorSv2::new(proxy_config).start().await;
}
