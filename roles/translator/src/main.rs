#![allow(special_module_name)]
mod args;
mod lib;

use args::Args;
use async_channel::{bounded, unbounded};
use error::{Error, ProxyResult};
use futures::{select, FutureExt};
use lib::{downstream_sv1, error, proxy, proxy_config, status, upstream_sv2};
use proxy_config::ProxyConfig;
use rand::Rng;
use roles_logic_sv2::utils::Mutex;
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    str::FromStr,
    sync::Arc,
};

use ext_config::{Config, File, FileFormat};
use tokio::{sync::broadcast, task, task::AbortHandle, time::Duration};
use v1::server_to_client;

use crate::status::{State, Status};
use tracing::{debug, error, info, warn};
/// Process CLI args, if any.
#[allow(clippy::result_large_err)]
fn process_cli_args<'a>() -> ProxyResult<'a, ProxyConfig> {
    // Parse CLI arguments
    let args = Args::from_args().map_err(|help| {
        error!("{}", help);
        Error::BadCliArgs
    })?;

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        Error::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    // Deserialize settings into ProxyConfig
    let config = settings.try_deserialize::<ProxyConfig>()?;
    Ok(config)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {}", e),
    };
    info!("Proxy Config: {:?}", &proxy_config);

    let (tx_status, rx_status) = unbounded();

    let target = Arc::new(Mutex::new(vec![0; 32]));

    // Sender/Receiver to send SV1 `mining.notify` message from the `Bridge` to the `Downstream`
    let (tx_sv1_notify, _rx_sv1_notify): (
        broadcast::Sender<server_to_client::Notify>,
        broadcast::Receiver<server_to_client::Notify>,
    ) = broadcast::channel(10);

    let task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>> = Arc::new(Mutex::new(Vec::new()));

    start(
        tx_sv1_notify.clone(),
        target.clone(),
        tx_status.clone(),
        task_collector.clone(),
        proxy_config.clone(),
    )
    .await;

    debug!("Starting up signal listener");
    let task_collector_ = task_collector.clone();

    let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());
    debug!("Starting up status listener");
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
                break;
            }
        };
        let task_status: Status = task_status.unwrap();

        match task_status.state {
            // Should only be sent by the downstream listener
            State::DownstreamShutdown(err) => {
                error!("SHUTDOWN from: {}", err);
                break;
            }
            State::BridgeShutdown(err) => {
                error!("SHUTDOWN from: {}", err);
                break;
            }
            State::UpstreamShutdown(err) => {
                error!("SHUTDOWN from: {}", err);
                break;
            }
            State::UpstreamTryReconnect(err) => {
                error!("SHUTDOWN from: {}", err);

                // wait a random amount of time between 0 and 3000ms
                // if all the downstreams try to reconnect at the same time, the upstream may fail
                let mut rng = rand::thread_rng();
                let wait_time = rng.gen_range(0..=3000);
                tokio::time::sleep(Duration::from_millis(wait_time)).await;

                // kill al the tasks
                let task_collector_aborting = task_collector_.clone();
                kill_tasks(task_collector_aborting.clone());

                warn!("Trying reconnecting to upstream");
                start(
                    tx_sv1_notify.clone(),
                    target.clone(),
                    tx_status.clone(),
                    task_collector_.clone(),
                    proxy_config.clone(),
                )
                .await;
            }
            State::Healthy(msg) => {
                info!("HEALTHY message: {}", msg);
            }
        }
    }
}

fn kill_tasks(task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>) {
    let _ = task_collector.safe_lock(|t| {
        while let Some(handle) = t.pop() {
            handle.0.abort();
            warn!("Killed task: {:?}", handle.1);
        }
    });
}

async fn start<'a>(
    tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    target: Arc<Mutex<Vec<u8>>>,
    tx_status: async_channel::Sender<Status<'static>>,
    task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    proxy_config: ProxyConfig,
) {
    // Sender/Receiver to send a SV2 `SubmitSharesExtended` from the `Bridge` to the `Upstream`
    // (Sender<SubmitSharesExtended<'static>>, Receiver<SubmitSharesExtended<'static>>)
    let (tx_sv2_submit_shares_ext, rx_sv2_submit_shares_ext) = bounded(10);

    // `tx_sv1_bridge` sender is used by `Downstream` to send a `DownstreamMessages` message to
    // `Bridge` via the `rx_sv1_downstream` receiver
    // (Sender<downstream_sv1::DownstreamMessages>, Receiver<downstream_sv1::DownstreamMessages>)
    let (tx_sv1_bridge, rx_sv1_downstream) = unbounded();

    // Sender/Receiver to send a SV2 `NewExtendedMiningJob` message from the `Upstream` to the
    // `Bridge`
    // (Sender<NewExtendedMiningJob<'static>>, Receiver<NewExtendedMiningJob<'static>>)
    let (tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(10);

    // Sender/Receiver to send a new extranonce from the `Upstream` to this `main` function to be
    // passed to the `Downstream` upon a Downstream role connection
    // (Sender<ExtendedExtranonce>, Receiver<ExtendedExtranonce>)
    let (tx_sv2_extranonce, rx_sv2_extranonce) = bounded(1);

    // Sender/Receiver to send a SV2 `SetNewPrevHash` message from the `Upstream` to the `Bridge`
    // (Sender<SetNewPrevHash<'static>>, Receiver<SetNewPrevHash<'static>>)
    let (tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(10);

    // Format `Upstream` connection address
    let upstream_addr = (
        proxy_config.upstream_address.as_str(),
        proxy_config.upstream_port,
    )
        .to_socket_addrs()
        .unwrap_or_else(|e| {
            panic!(
                "Invalid upstream address {}:{}: {}",
                proxy_config.upstream_address, proxy_config.upstream_port, e
            )
        })
        .next()
        .unwrap();

    let diff_config = Arc::new(Mutex::new(proxy_config.upstream_difficulty_config.clone()));
    let task_collector_upstream = task_collector.clone();
    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = match upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey,
        rx_sv2_submit_shares_ext,
        tx_sv2_set_new_prev_hash,
        tx_sv2_new_ext_mining_job,
        proxy_config.min_extranonce2_size,
        tx_sv2_extranonce,
        status::Sender::Upstream(tx_status.clone()),
        target.clone(),
        diff_config.clone(),
        task_collector_upstream,
    )
    .await
    {
        Ok(upstream) => upstream,
        Err(e) => {
            error!("Failed to create upstream: {}", e);
            return;
        }
    };
    let task_collector_init_task = task_collector.clone();
    // Spawn a task to do all of this init work so that the main thread
    // can listen for signals and failures on the status channel. This
    // allows for the tproxy to fail gracefully if any of these init tasks
    //fail
    let task = task::spawn(async move {
        // Connect to the SV2 Upstream role
        match upstream_sv2::Upstream::connect(
            upstream.clone(),
            proxy_config.min_supported_version,
            proxy_config.max_supported_version,
        )
        .await
        {
            Ok(_) => info!("Connected to Upstream!"),
            Err(e) => {
                error!("Failed to connect to Upstream EXITING! : {}", e);
                return;
            }
        }

        // Start receiving messages from the SV2 Upstream role
        if let Err(e) = upstream_sv2::Upstream::parse_incoming(upstream.clone()) {
            error!("failed to create sv2 parser: {}", e);
            return;
        }

        debug!("Finished starting upstream listener");
        // Start task handler to receive submits from the SV1 Downstream role once it connects
        if let Err(e) = upstream_sv2::Upstream::handle_submit(upstream.clone()) {
            error!("Failed to create submit handler: {}", e);
            return;
        }

        // Receive the extranonce information from the Upstream role to send to the Downstream role
        // once it connects also used to initialize the bridge
        let (extended_extranonce, up_id) = rx_sv2_extranonce.recv().await.unwrap();
        loop {
            let target: [u8; 32] = target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
            if target != [0; 32] {
                break;
            };
            async_std::task::sleep(std::time::Duration::from_millis(100)).await;
        }

        let task_collector_bridge = task_collector_init_task.clone();
        // Instantiate a new `Bridge` and begins handling incoming messages
        let b = proxy::Bridge::new(
            rx_sv1_downstream,
            tx_sv2_submit_shares_ext,
            rx_sv2_set_new_prev_hash,
            rx_sv2_new_ext_mining_job,
            tx_sv1_notify.clone(),
            status::Sender::Bridge(tx_status.clone()),
            extended_extranonce,
            target,
            up_id,
            task_collector_bridge,
        );
        proxy::Bridge::start(b.clone());

        // Format `Downstream` connection address
        let downstream_addr = SocketAddr::new(
            IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
            proxy_config.downstream_port,
        );

        let task_collector_downstream = task_collector_init_task.clone();
        // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
        downstream_sv1::Downstream::accept_connections(
            downstream_addr,
            tx_sv1_bridge,
            tx_sv1_notify,
            status::Sender::DownstreamListener(tx_status.clone()),
            b,
            proxy_config.downstream_difficulty_config,
            diff_config,
            task_collector_downstream,
        );
    }); // End of init task
    let _ = task_collector.safe_lock(|t| t.push((task.abort_handle(), "init task".to_string())));
}
