use config_helpers_sv2::logging::init_logging;
use jd_client_sv2::JobDeclaratorClient;

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
