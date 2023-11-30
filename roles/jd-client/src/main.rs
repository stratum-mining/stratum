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
    time::Duration,
};
use tokio::task::AbortHandle;

use crate::status::{State, Status};
use std::sync::atomic::AtomicBool;
use tracing::{error, info};

///
/// Is used by the template receiver and the downstream. When a NewTemplate is received the context
/// that is running the template receiver set this value to false and then the message is sent to
/// the context that is running the Downstream that do something and then set it back to true.
///
/// In the meantime if the context that is running the template receiver receives a SetNewPrevHash
/// it wait until the value of this global is true before doing anything.
///
/// Acuire and Release memory ordering is used.
///
/// Memory Ordering Explanation:
/// We use Acquire-Release ordering instead of SeqCst or Relaxed for the following reasons:
/// 1. Acquire in template receiver context ensures we see all operations before the Release store
///    the downstream.
/// 2. Within the same execution context (template receiver), a Relaxed store followed by an Acquire
///    load is sufficient. This is because operations within the same context execute in the order
///    they appear in the code.
/// 3. The combination of Release in downstream and Acquire in template receiver contexts establishes
///    a happens-before relationship, guaranteeing that we handle the SetNewPrevHash message after
///    that downstream have finished handling the NewTemplate.
/// 3. SeqCst is overkill we only need to synchronize two contexts, a globally agreed-upon order
///    between all the contexts is not necessary.
pub static IS_NEW_TEMPLATE_HANDLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug)]
pub struct PoolChangerTrigger {
    timeout: Duration,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl PoolChangerTrigger {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            task: None,
        }
    }

    pub fn start(&mut self, sender: status::Sender) {
        let timeout = self.timeout;
        let task = tokio::task::spawn(async move {
            tokio::time::sleep(timeout).await;
            let _ = sender
                .send(status::Status {
                    state: status::State::UpstreamRogue,
                })
                .await;
        });
        self.task = Some(task);
    }

    pub fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

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

    let mut upstream_index = 0;
    let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());

    // Channel used to manage failed tasks
    let (tx_status, rx_status) = unbounded();

    let task_collector = Arc::new(Mutex::new(vec![]));

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(_) => return,
    };

    loop {
        {
            let task_collector = task_collector.clone();
            let tx_status = tx_status.clone();

            if let Some(upstream) = proxy_config.upstreams.get(upstream_index) {
                let initialize = initialize_jd(
                    tx_status.clone(),
                    task_collector,
                    upstream.clone(),
                    proxy_config.timeout,
                );
                tokio::task::spawn(initialize);
            } else {
                let initialize = initialize_jd_as_solo_miner(
                    tx_status.clone(),
                    task_collector,
                    proxy_config.timeout,
                );
                tokio::task::spawn(initialize);
            }
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
                State::UpstreamRogue => {
                    error!("Changin Pool");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    upstream_index += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                State::Healthy(msg) => {
                    info!("HEALTHY message: {}", msg);
                }
            }
        }
    }
}
async fn initialize_jd_as_solo_miner(
    tx_status: async_channel::Sender<Status<'static>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    timeout: Duration,
) {
    let proxy_config = process_cli_args().unwrap();
    let miner_tx_out = crate::proxy_config::get_coinbase_output(&proxy_config).unwrap();

    // When Downstream receive a share that meets bitcoin target it transformit in a
    // SubmitSolution and send it to the TemplateReceiver
    let (send_solution, recv_solution) = bounded(10);

    // Format `Downstream` connection address
    let downstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
        proxy_config.downstream_port,
    );

    // Wait for downstream to connect
    let downstream = downstream::listen_for_downstream_mining(
        downstream_addr,
        None,
        send_solution,
        proxy_config.withhold,
        proxy_config.authority_public_key.clone(),
        proxy_config.authority_secret_key.clone(),
        proxy_config.cert_validity_sec,
        task_collector.clone(),
        status::Sender::Downstream(tx_status.clone()),
        miner_tx_out.clone(),
        None,
    )
    .await
    .unwrap();

    // Initialize JD part
    let mut parts = proxy_config.tp_address.split(':');
    let ip_tp = parts.next().unwrap().to_string();
    let port_tp = parts.next().unwrap().parse::<u16>().unwrap();

    TemplateRx::connect(
        SocketAddr::new(IpAddr::from_str(ip_tp.as_str()).unwrap(), port_tp),
        recv_solution,
        status::Sender::TemplateReceiver(tx_status.clone()),
        None,
        downstream,
        task_collector,
        Arc::new(Mutex::new(PoolChangerTrigger::new(timeout))),
        miner_tx_out.clone(),
        proxy_config.tp_authority_pub_key,
        false,
    )
    .await;
}

async fn initialize_jd(
    tx_status: async_channel::Sender<Status<'static>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    upstream_config: proxy_config::Upstream,
    timeout: Duration,
) {
    let proxy_config = process_cli_args().unwrap();
    let test_only_do_not_send_solution_to_tp = proxy_config
        .test_only_do_not_send_solution_to_tp
        .unwrap_or(false);

    // Format `Upstream` connection address
    let mut parts = upstream_config.pool_address.split(':');
    let address = parts
        .next()
        .unwrap_or_else(|| panic!("Invalid pool address {}", upstream_config.pool_address));
    let port = parts
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("Invalid pool address {}", upstream_config.pool_address));
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(address)
            .unwrap_or_else(|_| panic!("Invalid pool address {}", upstream_config.pool_address)),
        port,
    );

    // When Downstream receive a share that meets bitcoin target it transformit in a
    // SubmitSolution and send it to the TemplateReceiver
    let (send_solution, recv_solution) = bounded(10);

    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = match upstream_sv2::Upstream::new(
        upstream_addr,
        upstream_config.authority_pubkey.clone(),
        0, // TODO
        upstream_config.pool_signature.clone(),
        status::Sender::Upstream(tx_status.clone()),
        task_collector.clone(),
        Arc::new(Mutex::new(PoolChangerTrigger::new(timeout))),
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

    // Initialize JD part
    let mut parts = proxy_config.tp_address.split(':');
    let ip_tp = parts.next().unwrap().to_string();
    let port_tp = parts.next().unwrap().parse::<u16>().unwrap();

    let mut parts = upstream_config.jd_address.split(':');
    let ip_jd = parts.next().unwrap().to_string();
    let port_jd = parts.next().unwrap().parse::<u16>().unwrap();
    let jd = match JobDeclarator::new(
        SocketAddr::new(IpAddr::from_str(ip_jd.as_str()).unwrap(), port_jd),
        upstream_config.authority_pubkey.clone().into_bytes(),
        proxy_config.clone(),
        upstream.clone(),
        task_collector.clone(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx_status
                .send(Status {
                    state: State::UpstreamShutdown(e),
                })
                .await;
            return;
        }
    };

    // Wait for downstream to connect
    let downstream = downstream::listen_for_downstream_mining(
        downstream_addr,
        Some(upstream),
        send_solution,
        proxy_config.withhold,
        proxy_config.authority_public_key.clone(),
        proxy_config.authority_secret_key.clone(),
        proxy_config.cert_validity_sec,
        task_collector.clone(),
        status::Sender::Downstream(tx_status.clone()),
        vec![],
        Some(jd.clone()),
    )
    .await
    .unwrap();

    TemplateRx::connect(
        SocketAddr::new(IpAddr::from_str(ip_tp.as_str()).unwrap(), port_tp),
        recv_solution,
        status::Sender::TemplateReceiver(tx_status.clone()),
        Some(jd.clone()),
        downstream,
        task_collector,
        Arc::new(Mutex::new(PoolChangerTrigger::new(timeout))),
        vec![],
        proxy_config.tp_authority_pub_key,
        test_only_do_not_send_solution_to_tp,
    )
    .await;
}
