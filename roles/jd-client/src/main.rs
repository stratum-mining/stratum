use jd_client_sv2::JobDeclaratorClient;
use stratum_apps::config_helpers::logging::init_logging;

use crate::args::process_cli_args;

mod args;

#[tokio::main]
async fn main() {
    let jdc_config = process_cli_args().unwrap_or_else(|e| {
        eprintln!("Job Declarator Client config error: {e}");
        std::process::exit(1);
    });

    init_logging(jdc_config.log_file());
    JobDeclaratorClient::new(jdc_config).start().await;
}
