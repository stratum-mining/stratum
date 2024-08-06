#![allow(special_module_name)]
mod args;
mod lib;

use args::Args;
use error::{Error, ProxyResult};
pub use lib::{downstream_sv1, error, proxy, proxy_config, status, upstream_sv2};
use proxy_config::ProxyConfig;

use ext_config::{Config, File, FileFormat};

use tracing::{error, info};

/// Process CLI args, if any.
#[allow(clippy::result_large_err)]
fn process_cli_args<'a>() -> ProxyResult<'a, ProxyConfig> {
    // Parse CLI arguments
    let args = Args::from_args().map_err(|help| {
        error!("{}", help);
        Error::BadCliArgs
    })?;

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        Error::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    // Deserialize settings into ProxyConfig
    let config = settings.try_deserialize::<ProxyConfig>()?;
    Ok(config)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {}", e),
    };
    info!("Proxy Config: {:?}", &proxy_config);

    lib::TranslatorSv2::new(proxy_config).start().await;
}
