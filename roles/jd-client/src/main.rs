//! Entry point for the Job Declarator Client (JDC).
//!
//! This binary parses CLI arguments, loads the TOML configuration file, and
//! starts the main runtime defined in `jd_client::JobDeclaratorClient`.
//!
//! The actual task orchestration and shutdown logic are managed in `lib/mod.rs`.

mod args;
use args::process_cli_args;
use config_helpers_sv2::logging::init_logging;
use jd_client::JobDeclaratorClient;
use tracing::error;

/// This will start:
/// 1. An Upstream, this will connect with the mining Pool
/// 2. A listener that will wait for a tproxy
/// 3. A JobDeclarator, this will connect with the job-declarator-server
/// 4. A TemplateRx, this will connect with bitcoind
///
/// Setup phase
/// 1. Upstream: ->SetupConnection, <-SetupConnectionSuccess
/// 2. Downstream: <-SetupConnection, ->SetupConnectionSuccess, <-OpenExtendedMiningChannel
/// 3. Upstream: ->OpenExtendedMiningChannel, <-OpenExtendedMiningChannelSuccess
/// 4. Downstream: ->OpenExtendedMiningChannelSuccess
///
/// Setup phase
/// 1. JobDeclarator: ->SetupConnection, <-SetupConnectionSuccess, ->AllocateMiningJobToken(x2),
///    <-AllocateMiningJobTokenSuccess (x2)
/// 2. TemplateRx: ->CoinbaseOutputDataSize
///
/// Main loop:
/// 1. TemplateRx: <-NewTemplate, SetNewPrevHash
/// 2. JobDeclarator: -> CommitMiningJob (JobDeclarator::on_new_template), <-CommitMiningJobSuccess
/// 3. Upstream: ->SetCustomMiningJob, Downstream: ->NewExtendedMiningJob, ->SetNewPrevHash
/// 4. Downstream: <-Share
/// 5. Upstream: ->Share
///
/// When we have a NewTemplate we send the NewExtendedMiningJob downstream and the CommitMiningJob
/// to the JDS altogether.
/// Then we receive CommitMiningJobSuccess and we use the new token to send SetCustomMiningJob to
/// the pool.
/// When we receive SetCustomMiningJobSuccess we set in Upstream job_id equal to the one received
/// in SetCustomMiningJobSuccess so that we still send shares upstream with the right job_id.
///
/// The above procedure, let us send NewExtendedMiningJob downstream right after a NewTemplate has
/// been received this will reduce the time that pass from a NewTemplate and the mining-device
/// starting to mine on the new job.
///
/// In the case a future NewTemplate the SetCustomMiningJob is sent only if the candidate become
/// the actual NewTemplate so that we do not send a lot of useless future Job to the pool. That
/// means that SetCustomMiningJob is sent only when a NewTemplate become "active"
///
/// The JobDeclarator always have 2 available token, that means that whenever a token is used to
/// commit a job with upstream we require a new one. Having always a token when needed means that
/// whenever we want to commit a mining job we can do that without waiting for upstream to provide
/// a new token.
///
/// Entrypoint for the Job Declarator Client binary.
///
/// Loads the configuration from TOML and initializes the main runtime
/// defined in `jd_client::JobDeclaratorClient`. Errors during startup are logged.
#[tokio::main]
async fn main() {
    let jdc_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => {
            error!("Job Declarator Client Config error: {}", e);
            return;
        }
    };

    init_logging(jdc_config.log_file());

    let jdc = JobDeclaratorClient::new(jdc_config);
    jdc.start().await;
}
