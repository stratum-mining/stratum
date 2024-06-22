use crate::lib::{error::ProxyResult, proxy_config::ProxyConfig};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "Path to TOML configuration file")]
    config_path: String,
}

#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> ProxyResult<ProxyConfig> {
    let args = Args::parse();
    let config = match ext_config::Config::builder()
        .add_source(ext_config::File::with_name(&args.config_path))
        .build()
    {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("{:?}", e);
            std::process::exit(1)
        }
    };

    let proxy_config: ProxyConfig = config.try_deserialize()?;

    Ok(proxy_config)
}
