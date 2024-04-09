use crate::lib::{error::TProxyResult, tproxy_config::TProxyConfig};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "Path to TOML configuration file")]
    config_path: String,
}

#[allow(clippy::result_large_err)]
pub fn process_cli_args<'a>() -> TProxyResult<'a, TProxyConfig> {
    let args = Args::parse();
    let config = match config::Config::builder()
        .add_source(config::File::with_name(&args.config_path))
        .build()
    {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("{:?}", e);
            std::process::exit(1)
        }
    };

    let proxy_config: TProxyConfig = config.try_deserialize()?;

    Ok(proxy_config)
}
