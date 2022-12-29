mod args;
mod downstream_sv1;
mod error;
mod proxy;
mod proxy_config;
mod status;
mod upstream_sv2;
use args::Args;
use error::{Error, ProxyResult};
use proxy_config::ProxyConfig;
use roles_logic_sv2::utils::Mutex;

const SELF_EXTRNONCE_LEN: usize = 2;

use async_channel::{bounded, unbounded, Receiver, Sender};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use v1::server_to_client;

use crate::status::State;
use tracing::{debug, error, info};

/// Process CLI args, if any.
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

#[async_std::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let proxy_config = process_cli_args().unwrap();
    info!("PC: {:?}", &proxy_config);

    let (tx_status, rx_status) = unbounded();

    // `tx_sv1_submit` sender is used by `Downstream` to send a `mining.submit` message to
    // `Bridge` via the `rx_sv1_submit` receiver
    // (Sender<v1::client_to_server::Submit>, Receiver<Submit>)
    let (tx_sv1_submit, rx_sv1_submit) = unbounded();

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
    let (tx_sv1_notify, rx_sv1_notify): (
        Sender<server_to_client::Notify>,
        Receiver<server_to_client::Notify>,
    ) = bounded(10);

    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address)
            .expect("Failed to parse upstream address!"),
        proxy_config.upstream_port,
    );

    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey,
        rx_sv2_submit_shares_ext,
        tx_sv2_set_new_prev_hash,
        tx_sv2_new_ext_mining_job,
        proxy_config.min_extranonce2_size,
        tx_sv2_extranonce,
        tx_status.clone(),
        target.clone(),
    )
    .await
    .unwrap();

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
    upstream_sv2::Upstream::parse_incoming(upstream.clone());

    debug!("Finished starting upstream listener");
    // Start task handler to receive submits from the SV1 Downstream role once it connects
    upstream_sv2::Upstream::handle_submit(upstream.clone());

    // Receive the extranonce information from the Upstream role to send to the Downstream role
    // once it connects also used to initialize the bridge
    let extended_extranonce = rx_sv2_extranonce.recv().await.unwrap();

    loop {
        let target: [u8; 32] = target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
        if target != [0; 32] {
            break;
        };
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Instantiate a new `Bridge` and begins handling incoming messages
    let b = Arc::new(Mutex::new(proxy::Bridge::new(
        rx_sv1_submit,
        tx_sv2_submit_shares_ext,
        rx_sv2_set_new_prev_hash,
        rx_sv2_new_ext_mining_job,
        tx_sv1_notify,
        tx_status.clone(),
        extended_extranonce,
        target,
    )));
    proxy::Bridge::start(b.clone());

    // Format `Downstream` connection address
    let downstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
        proxy_config.downstream_port,
    );

    // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
    downstream_sv1::Downstream::accept_connections(
        downstream_addr,
        tx_sv1_submit,
        rx_sv1_notify,
        tx_status.clone(),
        b,
    )
    .await;

    // Check all tasks if is_finished() is true, if so exit
    loop {
        let task_status = rx_status.recv().await.unwrap();

        match task_status.state {
            State::Shutdown(err) => {
                error!("SHUTDOWN from: {}", err);
                break;
            }
            State::Healthy(msg) => {
                info!("HEALTHY message: {}", msg);
            }
        }
    }
}
