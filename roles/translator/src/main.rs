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
    // ------
    // debug with tokio-console and tracing logs
    use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};
    let subscriber = Registry::default();
    // Layer for tokio-console
    let console_layer = console_subscriber::spawn();

    // Layer for standard tracing output with a filter
    let fmt_layer = fmt::Layer::default()
        .with_filter(EnvFilter::new("debug")); // Only show DEBUG and above

    // Combine both layers
    let combined = subscriber.with(console_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(combined)
        .expect("Failed to set subscriber");

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {}", e),
    };
    info!("Proxy Config: {:?}", &proxy_config);

    lib::TranslatorSv2::new(proxy_config).start().await;
}
