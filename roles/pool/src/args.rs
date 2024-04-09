use crate::lib::{
    error::PoolResult,
    pool_config::PoolConfig,
};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
    short,
    long,
    help = "Path to TOML configuration file"
    )]
    config_path: String,
}

#[allow(clippy::result_large_err)]
pub fn process_cli_args() -> PoolResult<PoolConfig> {
    let args = Args::parse();
    let config = match config::Config::builder()
        .add_source(config::File::with_name(&args.config_path))
        .build() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("{:?}", e);
            std::process::exit(1)
        }
    };

    let pool_config: PoolConfig = config.try_deserialize()?;

    Ok(pool_config)
}


