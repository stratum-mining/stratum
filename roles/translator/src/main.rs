#![allow(special_module_name)]
mod args;
mod lib;

use args::Args;
use async_channel::{bounded, unbounded, Receiver, Sender};
use downstream_sv1::DownstreamMessages;
use error::{Error, ProxyResult};
use futures::{select, FutureExt};
use lib::{downstream_sv1, error, proxy, proxy_config, status, upstream_sv2};
use proxy_config::ProxyConfig;
use rand::Rng;
use roles_logic_sv2::{
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended},
    utils::Mutex,
};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use tokio::{sync::broadcast, task, time::Duration};
use tokio_util::sync::CancellationToken;
use v1::server_to_client;

use crate::status::{State, Status};
use tracing::{debug, error, info, warn};
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {}", e),
    };
    info!("PC: {:?}", &proxy_config);

    let (tx_status, rx_status) = unbounded();

    // `tx_sv1_bridge` sender is used by `Downstream` to send a `DownstreamMessages` message to
    // `Bridge` via the `rx_sv1_downstream` receiver
    // (Sender<downstream_sv1::DownstreamMessages>, Receiver<downstream_sv1::DownstreamMessages>)
    let (tx_sv1_bridge, rx_sv1_downstream) = unbounded();

    // Sender/Receiver to send a SV2 `SubmitSharesExtended` from the `Bridge` to the `Upstream`
    // (Sender<SubmitSharesExtended<'static>>, Receiver<SubmitSharesExtended<'static>>)
    let (tx_sv2_submit_shares_ext, rx_sv2_submit_shares_ext) = bounded(10);

    // Sender/Receiver to send a SV2 `SetNewPrevHash` message from the `Upstream` to the `Bridge`
    // (Sender<SetNewPrevHash<'static>>, Receiver<SetNewPrevHash<'static>>)
    let (tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(10);

    // Sender/Receiver to send a SV2 `NewExtendedMiningJob` message from the `Upstream` to the
    // `Bridge`
    // (Sender<NewExtendedMiningJob<'static>>, Receiver<NewExtendedMiningJob<'static>>)
    let (tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(10);

    // Sender/Receiver to send a new extranonce from the `Upstream` to this `main` function to be
    // passed to the `Downstream` upon a Downstream role connection
    // (Sender<ExtendedExtranonce>, Receiver<ExtendedExtranonce>)
    let (tx_sv2_extranonce, rx_sv2_extranonce) = bounded(1);
    let target = Arc::new(Mutex::new(vec![0; 32]));

    // Sender/Receiver to send SV1 `mining.notify` message from the `Bridge` to the `Downstream`
    let (tx_sv1_notify, _rx_sv1_notify): (
        broadcast::Sender<server_to_client::Notify>,
        broadcast::Receiver<server_to_client::Notify>,
    ) = broadcast::channel(10);

    let cancellation_token = CancellationToken::new();
    start(
        rx_sv2_submit_shares_ext.clone(),
        tx_sv2_submit_shares_ext.clone(),
        tx_sv2_new_ext_mining_job.clone(),
        tx_sv2_set_new_prev_hash.clone(),
        tx_sv2_extranonce.clone(),
        rx_sv2_extranonce.clone(),
        rx_sv2_set_new_prev_hash.clone(),
        rx_sv2_new_ext_mining_job.clone(),
        rx_sv1_downstream.clone(),
        tx_sv1_bridge.clone(),
        tx_sv1_notify.clone(),
        target.clone(),
        tx_status.clone(),
        cancellation_token.clone(),
    )
    .await;

    debug!("Starting up signal listener");

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
                cancellation_token.clone().cancel();

                // wait a random amount of time between 0 and 3000ms
                // if all the downstreams try to reconnect at the same time, the upstream may fail
                tokio::time::sleep(Duration::from_millis(1000)).await;
                let mut rng = rand::thread_rng();
                let wait_time = rng.gen_range(0..=3000);
                tokio::time::sleep(Duration::from_millis(wait_time)).await;

                // create a new token
                let cancellation_token = CancellationToken::new();

                error!("Trying recconnecting to upstream");
                start(
                    rx_sv2_submit_shares_ext.clone(),
                    tx_sv2_submit_shares_ext.clone(),
                    tx_sv2_new_ext_mining_job.clone(),
                    tx_sv2_set_new_prev_hash.clone(),
                    tx_sv2_extranonce.clone(),
                    rx_sv2_extranonce.clone(),
                    rx_sv2_set_new_prev_hash.clone(),
                    rx_sv2_new_ext_mining_job.clone(),
                    rx_sv1_downstream.clone(),
                    tx_sv1_bridge.clone(),
                    tx_sv1_notify.clone(),
                    target.clone(),
                    tx_status.clone(),
                    cancellation_token.clone(),
                )
                .await;
            }
            State::Healthy(msg) => {
                info!("HEALTHY message: {}", msg);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn start<'a>(
    rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
    tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
    tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
    tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
    tx_sv2_extranonce: Sender<(ExtendedExtranonce, u32)>,
    rx_sv2_extranonce: Receiver<(ExtendedExtranonce, u32)>,
    rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    rx_sv1_downstream: Receiver<DownstreamMessages>,
    tx_sv1_bridge: Sender<DownstreamMessages>,
    tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    target: Arc<Mutex<Vec<u8>>>,
    tx_status: async_channel::Sender<Status<'static>>,
    cancellation_token: CancellationToken,
) {
    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => panic!("failed to load config: {}", e),
    };
    info!("Proxy Config: {:?}", &proxy_config);
    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address)
            .expect("Failed to parse upstream address!"),
        proxy_config.upstream_port,
    );

    let diff_config = Arc::new(Mutex::new(proxy_config.upstream_difficulty_config.clone()));
    let cancellation_token_upstream = cancellation_token.clone();
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
        cancellation_token_upstream,
    )
    .await
    {
        Ok(upstream) => upstream,
        Err(e) => {
            error!("Failed to create upstream: {}", e);
            return;
        }
    };
    let cancellation_token_init_task = cancellation_token.clone();
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

        let cancellation_token_bridge = cancellation_token_init_task.clone();
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
            cancellation_token_bridge,
        );
        proxy::Bridge::start(b.clone());

        // Format `Downstream` connection address
        let downstream_addr = SocketAddr::new(
            IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
            proxy_config.downstream_port,
        );

        let cancellation_token_downstream = cancellation_token_init_task.clone();
        // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
        downstream_sv1::Downstream::accept_connections(
            downstream_addr,
            tx_sv1_bridge,
            tx_sv1_notify,
            status::Sender::DownstreamListener(tx_status.clone()),
            b,
            proxy_config.downstream_difficulty_config,
            diff_config,
            cancellation_token_downstream,
        );
    }); // End of init task
    tokio::select! {
        _ = task => {},
        _ = cancellation_token.cancelled() => {
            warn!("Shutting init task");
        },
    }
}
