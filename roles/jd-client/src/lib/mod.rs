pub mod downstream;
pub mod error;
pub mod job_declarator;
pub mod proxy_config;
pub mod status;
pub mod template_receiver;
pub mod upstream_sv2;

use std::{sync::atomic::AtomicBool, time::Duration};

use job_declarator::JobDeclarator;
use proxy_config::ProxyConfig;
use template_receiver::TemplateRx;

use async_channel::{bounded, unbounded};
use futures::{select, FutureExt};
use roles_logic_sv2::utils::Mutex;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use tokio::task::AbortHandle;

use tracing::{error, info};

/// Is used by the template receiver and the downstream. When a NewTemplate is received the context
/// that is running the template receiver set this value to false and then the message is sent to
/// the context that is running the Downstream that do something and then set it back to true.
///
/// In the meantime if the context that is running the template receiver receives a SetNewPrevHash
/// it wait until the value of this global is true before doing anything.
///
/// Acquire and Release memory ordering is used.
///
/// Memory Ordering Explanation:
/// We use Acquire-Release ordering instead of SeqCst or Relaxed for the following reasons:
/// 1. Acquire in template receiver context ensures we see all operations before the Release store
///    the downstream.
/// 2. Within the same execution context (template receiver), a Relaxed store followed by an Acquire
///    load is sufficient. This is because operations within the same context execute in the order
///    they appear in the code.
/// 3. The combination of Release in downstream and Acquire in template receiver contexts
///    establishes a happens-before relationship, guaranteeing that we handle the SetNewPrevHash
///    message after that downstream have finished handling the NewTemplate.
/// 3. SeqCst is overkill we only need to synchronize two contexts, a globally agreed-upon order
///    between all the contexts is not necessary.
pub static IS_NEW_TEMPLATE_HANDLED: AtomicBool = AtomicBool::new(true);

/// Job Declarator Client (or JDC) is the role which is Miner-side, in charge of creating new
/// mining jobs from the templates received by the Template Provider to which it is connected. It
/// declares custom jobs to the JDS, in order to start working on them.
/// JDC is also responsible for putting in action the Pool-fallback mechanism, automatically
/// switching to backup Pools in case of declared custom jobs refused by JDS (which is Pool side).
/// As a solution of last-resort, it is able to switch to Solo Mining until new safe Pools appear
/// in the market.
#[derive(Debug, Clone)]
pub struct JobDeclaratorClient {
    /// Configuration of the proxy server [`JobDeclaratorClient`] is connected to.
    config: ProxyConfig,
}

impl JobDeclaratorClient {
    pub fn new(config: ProxyConfig) -> Self {
        Self { config }
    }

    pub async fn start(self) {
        let mut upstream_index = 0;
        let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());

        // Channel used to manage failed tasks
        let (tx_status, rx_status) = unbounded();

        let task_collector = Arc::new(Mutex::new(vec![]));

        let proxy_config = &self.config;

        loop {
            let task_collector = task_collector.clone();
            let tx_status = tx_status.clone();
            if let Some(upstream) = proxy_config.upstreams.get(upstream_index) {
                self.initialize_jd(tx_status.clone(), task_collector.clone(), upstream.clone())
                    .await;
            } else {
                self.initialize_jd_as_solo_miner(tx_status.clone(), task_collector.clone())
                    .await;
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
                let task_status: status::Status = task_status.unwrap();

                match task_status.state {
                    // Should only be sent by the downstream listener
                    status::State::DownstreamShutdown(err) => {
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
                    status::State::UpstreamShutdown(err) => {
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
                    status::State::UpstreamRogue => {
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
                    status::State::Healthy(msg) => {
                        info!("HEALTHY message: {}", msg);
                    }
                }
            }
        }
    }

    async fn initialize_jd_as_solo_miner(
        &self,
        tx_status: async_channel::Sender<status::Status<'static>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    ) {
        let proxy_config = &self.config;
        let timeout = proxy_config.timeout;
        let miner_tx_out = proxy_config::get_coinbase_output(proxy_config).unwrap();

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
            proxy_config.authority_public_key,
            proxy_config.authority_secret_key,
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
            proxy_config.tp_authority_public_key,
            false,
        )
        .await;
    }

    async fn initialize_jd(
        &self,
        tx_status: async_channel::Sender<status::Status<'static>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        upstream_config: proxy_config::Upstream,
    ) {
        let proxy_config = &self.config;
        let timeout = proxy_config.timeout;
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
            IpAddr::from_str(address).unwrap_or_else(|_| {
                panic!("Invalid pool address {}", upstream_config.pool_address)
            }),
            port,
        );

        // When Downstream receive a share that meets bitcoin target it transformit in a
        // SubmitSolution and send it to the TemplateReceiver
        let (send_solution, recv_solution) = bounded(10);

        // Instantiate a new `Upstream` (SV2 Pool)
        let upstream = match upstream_sv2::Upstream::new(
            upstream_addr,
            upstream_config.authority_pubkey,
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

        // Start receiving messages from the SV2 Upstream role
        if let Err(e) = upstream_sv2::Upstream::parse_incoming(upstream.clone()) {
            error!("failed to create sv2 parser: {}", e);
            panic!()
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
            upstream_config.authority_pubkey.into_bytes(),
            proxy_config.clone(),
            upstream.clone(),
            task_collector.clone(),
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx_status
                    .send(status::Status {
                        state: status::State::UpstreamShutdown(e),
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
            proxy_config.authority_public_key,
            proxy_config.authority_secret_key,
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
            proxy_config.tp_authority_public_key,
            test_only_do_not_send_solution_to_tp,
        )
        .await;
    }
}

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
