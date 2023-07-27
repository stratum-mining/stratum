mod args;
mod downstream;
mod error;
mod job_declarator;
mod proxy_config;
mod status;
mod template_receiver;
mod upstream_sv2;
use args::Args;
use error::{Error, ProxyResult};
use job_declarator::JobDeclarator;
use proxy_config::ProxyConfig;
use roles_logic_sv2::utils::Mutex;
use template_receiver::TemplateRx;

use async_channel::{bounded, unbounded};
use futures::{select, FutureExt};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use tokio::task::AbortHandle;

use crate::status::{State, Status};
use std::sync::atomic::AtomicBool;
use tracing::{error, info};

/// USED to make sure that if a future new_temnplate and a set_new_prev_hash are received together
/// the future new_temnplate is always handled before the set new prev hash.
pub static IS_NEW_TEMPLATE_HANDLED: AtomicBool = AtomicBool::new(true);

/// Process CLI args, if any.
#[allow(clippy::result_large_err)]
fn process_cli_args<'a>() -> ProxyResult<'a, ProxyConfig> {
    let args = match Args::from_args() {
        Ok(cfg) => cfg,
        Err(help) => {
            error!("{}", help);
            return Err(Error::BadCliArgs);
        }
    };
    let config_file = std::fs::read_to_string(args.config_path)?;
    Ok(toml::from_str::<ProxyConfig>(&config_file)?)
}

/// TODO on the setup phase JDC must send a random nonce to bitcoind and JDS used for the tx
/// hashlist
///
/// TODO on setupconnection with bitcoind (TP) JDC must signal that want a tx short hash list with
/// the templates
///
/// TODO JDC must handle TxShortHahhList message
///
/// This will start:
/// 1. An Upstream, this will connect with the mining Pool
/// 2. A listner that will wait for a mining downstream with ExtendedChannel capabilities (tproxy,
///    minin-proxy)
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
/// to the JDS altoghether.
/// Then we receive CommitMiningJobSuccess and we use the new token to send SetCustomMiningJob to
/// the pool.
/// When we receive SetCustomMiningJobSuccess we set in Upstream job_id equal to the one received
/// in SetCustomMiningJobSuccess so that we sill send shares upstream with the right job_id.
///
/// The above procedure, let us send NewExtendedMiningJob downstream right after a NewTemplate has
/// been received this will reduce the time that pass from a NewTemplate and the mining-device
/// starting to mine on the new job.
///
/// In the case a future NewTemplate the SetCustomMiningJob is sent only if the canditate become
/// the actual NewTemplate so that we do not send a lot of usless future Job to the pool. That
/// means that SetCustomMiningJob is sent only when a NewTemplate becom "active"
///
/// The JobDeclarator always have 2 avaiable token, that means that whenever a token is used to
/// commit a job with upstream we require a new one. Having always a token when needed means that
/// whenever we want to commit a mining job we can do that without waiting for upstream to provide
/// a new token.
///
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mut failed = 0;
    let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());

    // Channel used to manage failed tasks
    let (tx_status, rx_status) = unbounded();

    let task_collector = Arc::new(Mutex::new(vec![]));

    let proxy_config = process_cli_args().unwrap();

    while failed < proxy_config.retry {
        {
            let task_collector = task_collector.clone();
            let tx_status = tx_status.clone();
            let initialize = initialize_jd(tx_status.clone(), task_collector.clone());
            tokio::task::spawn(async move { initialize.await });
        }
        // Check all tasks if is_finished() is true, if so exit
        loop {
            let task_status = select! {
                task_status = rx_status.recv().fuse() => task_status,
                interrupt_signal = interrupt_signal_future => {
                    match interrupt_signal {
                        Ok(()) => {
                            info!("Interrupt received");
                        },
                        Err(err) => {
                            error!("Unable to listen for interrupt signal: {}", err);
                            // we also shut down in case of error
                        },
                    }
                    std::process::exit(0);
                }
            };
            let task_status: Status = task_status.unwrap();

            match task_status.state {
                // Should only be sent by the downstream listener
                State::DownstreamShutdown(err) => {
                    error!("SHUTDOWN from: {}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                State::BridgeShutdown(err) => {
                    error!("SHUTDOWN from: {}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                State::UpstreamShutdown(err) => {
                    error!("SHUTDOWN from: {}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                State::Healthy(msg) => {
                    info!("HEALTHY message: {}", msg);
                }
            }
        }
        failed += 1;
    }
}

async fn initialize_jd(
    tx_status: async_channel::Sender<Status<'static>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
) {
    let proxy_config = process_cli_args().unwrap();

    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address)
            .expect("Failed to parse upstream address!"),
        proxy_config.upstream_port,
    );

    // When Downstream receive a share that meets bitcoin target it transformit in a
    // SubmitSolution and send it to the TemplateReceiver
    let (send_solution, recv_solution) = bounded(10);

    // When Upstream receive OpenExtendedMiningChannelSuccess it create a PoolChannelFactory that
    // is mirror the one in the pool, and send it to Downstream. With that factory Downstream when
    // receive templates and p_hash from the TemplateReceiver can create jobs like the one that
    // would have been created by the pool that opened the extended channel. That means the this
    // can be a completly transparent proxy for the downstream.
    let (send_channel_factory, recv_channel_factory) = bounded(1);

    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = match upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey.clone(),
        0, // TODO
        status::Sender::Upstream(tx_status.clone()),
        send_channel_factory,
        task_collector.clone(),
    )
    .await
    {
        Ok(upstream) => upstream,
        Err(e) => {
            error!("Failed to create upstream: {}", e);
            panic!()
        }
    };

    // Start receiving messages from the SV2 Upstream role
    if let Err(e) = upstream_sv2::Upstream::parse_incoming(upstream.clone()) {
        error!("failed to create sv2 parser: {}", e);
        panic!()
    }

    match upstream_sv2::Upstream::setup_connection(
        upstream.clone(),
        proxy_config.min_supported_version,
        proxy_config.max_supported_version,
    )
    .await
    {
        Ok(_) => info!("Connected to Upstream!"),
        Err(e) => {
            error!("Failed to connect to Upstream EXITING! : {}", e);
            panic!()
        }
    }

    // Format `Downstream` connection address
    let downstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
        proxy_config.downstream_port,
    );

    // Wait for downstream to connect
    let downstream = downstream::listen_for_downstream_mining(
        downstream_addr,
        &upstream,
        recv_channel_factory,
        send_solution,
        proxy_config.withhold,
        proxy_config.authority_public_key.clone(),
        proxy_config.authority_secret_key.clone(),
        proxy_config.cert_validity_sec,
        task_collector.clone(),
        status::Sender::Downstream(tx_status.clone()),
    )
    .await
    .unwrap();

    // Initialize JD part
    let jd_config = proxy_config.jd_config.clone();
    let mut parts = jd_config.tp_address.split(':');
    let ip_tp = parts.next().unwrap().to_string();
    let port_tp = parts.next().unwrap().parse::<u16>().unwrap();
    let mut parts = jd_config.jd_address.split(':');
    let ip_jd = parts.next().unwrap().to_string();
    let port_jd = parts.next().unwrap().parse::<u16>().unwrap();
    let jd = JobDeclarator::new(
        SocketAddr::new(IpAddr::from_str(ip_jd.as_str()).unwrap(), port_jd),
        proxy_config.upstream_authority_pubkey.clone().into_bytes(),
        proxy_config.clone(),
        upstream,
        task_collector.clone(),
    )
    .await;
    TemplateRx::connect(
        SocketAddr::new(IpAddr::from_str(ip_tp.as_str()).unwrap(), port_tp),
        recv_solution,
        status::Sender::TemplateReceiver(tx_status.clone()),
        jd.clone(),
        downstream,
        task_collector,
    )
    .await;
}
