use config_helpers_sv2::logging::init_logging;
use new_jd_client::JobDeclaratorClient;

use crate::args::process_cli_args;

mod args;

#[tokio::main]
async fn main() {
    let jdc_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Job Declarator Client Config error: {}", e);
            return;
        }
    };

    init_logging(jdc_config.log_file());

    let jdc = JobDeclaratorClient::new(jdc_config);
    jdc.start().await;
}
