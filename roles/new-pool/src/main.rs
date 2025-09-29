use config_helpers_sv2::logging::init_logging;
use new_pool_sv2::PoolSv2;

use crate::args::process_cli_args;

mod args;

#[tokio::main]
async fn main() {
    let config = process_cli_args();
    init_logging(config.log_dir());
    let _ = PoolSv2::new(config).start().await;
}
