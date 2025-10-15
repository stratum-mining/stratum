use pool_sv2::PoolSv2;
use stratum_apps::config_helpers::logging::init_logging;

use crate::args::process_cli_args;

mod args;

#[tokio::main]
async fn main() {
    let config = process_cli_args();
    init_logging(config.log_dir());
    if let Err(e) = PoolSv2::new(config).start().await {
        tracing::error!("Pool Error'ed out: {e}");
    };
}
