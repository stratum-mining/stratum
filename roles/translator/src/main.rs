mod args;
use std::process;

use args::Args;
use config::TranslatorConfig;
use translator_sv2::error::TproxyError;
pub use translator_sv2::{config, error, status, sv1, sv2, TranslatorSv2};

use ext_config::{Config, File, FileFormat};

use tracing::error;

/// Process CLI args, if any.
#[allow(clippy::result_large_err)]
fn process_cli_args() -> Result<TranslatorConfig, TproxyError> {
    // Parse CLI arguments
    let args = Args::from_args().map_err(|help| {
        error!("{}", help);
        TproxyError::BadCliArgs
    })?;

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        TproxyError::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    // Deserialize settings into TranslatorConfig
    let config = settings.try_deserialize::<TranslatorConfig>()?;
    Ok(config)
}

/// Entrypoint for the Translator binary.
///
/// Loads the configuration from TOML and initializes the main runtime
/// defined in `translator_sv2::TranslatorSv2`. Errors during startup are logged.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {e}"),
    };

    TranslatorSv2::new(proxy_config).start().await;

    process::exit(1);
}
