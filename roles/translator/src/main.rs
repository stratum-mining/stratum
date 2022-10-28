mod args;
mod downstream_sv1;
mod error;
mod proxy;
mod proxy_config;
mod upstream_sv2;
use args::Args;
use error::{Error, ProxyResult};
use proxy::next_mining_notify::NextMiningNotify;
use proxy_config::ProxyConfig;
use roles_logic_sv2::utils::Mutex;

const SELF_EXTRNONCE_LEN: usize = 2;

use async_channel::{bounded, Receiver, Sender};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use v1::server_to_client;

use tracing::{error, info};

/// Process CLI args, if any.
fn process_cli_args() -> ProxyResult<ProxyConfig> {
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

    // Sender/Receiver to send a SV1 `mining.submit` from the `Downstream` to the `Bridge`
    // (Sender<v1::client_to_server::Submit>, Receiver<Submit>)
    let (sender_submit_from_sv1, recv_submit_from_sv1) = async_channel::unbounded();

    // Sender/Receiver to send a SV2 `SubmitSharesExtended` from the `Bridge` to the `Upstream`
    // (Sender<SubmitSharesExtended<'static>>, Receiver<SubmitSharesExtended<'static>>)
    let (sender_submit_to_sv2, recv_submit_to_sv2) = bounded(10);

    // Sender/Receiver to send a SV2 `SetNewPrevHash` message from the `Upstream` to the `Bridge`
    // (Sender<SetNewPrevHash<'static>>, Receiver<SetNewPrevHash<'static>>)
    let (sender_new_prev_hash, recv_new_prev_hash) = bounded(10);

    // Sender/Receiver to send a SV2 `NewExtendedMiningJob` message from the `Upstream` to the
    // `Bridge`
    // (Sender<NewExtendedMiningJob<'static>>, Receiver<NewExtendedMiningJob<'static>>)
    let (sender_new_extended_mining_job, recv_new_extended_mining_job) = bounded(10);

    // Sender/Receiver to send a new extranonce from the `Upstream` to this `main` function to be
    // passed to the `Downstream` upon a Downstream role connection
    // (Sender<ExtendedExtranonce>, Receiver<ExtendedExtranonce>)
    let (sender_extranonce, recv_extranonce) = bounded(1);
    let target = Arc::new(Mutex::new(vec![0; 32]));

    // Sender/Receiver to send SV1 `mining.notify` message from the `Bridge` to the `Downstream`
    let (sender_mining_notify_bridge, recv_mining_notify_downstream): (
        Sender<server_to_client::Notify>,
        Receiver<server_to_client::Notify>,
    ) = bounded(10);

    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address).unwrap(),
        proxy_config.upstream_port,
    );

    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey,
        recv_submit_to_sv2,
        sender_new_prev_hash,
        sender_new_extended_mining_job,
        proxy_config.min_extranonce2_size,
        sender_extranonce,
        target.clone(),
    )
    .await
    .unwrap();

    // Connect to the SV2 Upstream role
    upstream_sv2::Upstream::connect(
        upstream.clone(),
        proxy_config.min_supported_version,
        proxy_config.max_supported_version,
    )
    .await
    .unwrap();

    // Start receiving messages from the SV2 Upstream role
    upstream_sv2::Upstream::parse_incoming(upstream.clone());

    // Start task handler to receive submits from the SV1 Downstream role once it connects
    upstream_sv2::Upstream::handle_submit(upstream.clone());

    // Setup to store the latest SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages received
    // from the Upstream role before any Downstream role connects
    let next_mining_notify = Arc::new(Mutex::new(NextMiningNotify::new()));
    let last_notify: Arc<Mutex<Option<server_to_client::Notify>>> = Arc::new(Mutex::new(None));

    // Instantiate a new `Bridge` and begins handling incoming messages
    proxy::Bridge::new(
        recv_submit_from_sv1,
        sender_submit_to_sv2,
        recv_new_prev_hash,
        recv_new_extended_mining_job,
        next_mining_notify,
        sender_mining_notify_bridge,
        last_notify.clone(),
    )
    .start();

    // Format `Downstream` connection address
    let downstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
        proxy_config.downstream_port,
    );

    // Receive the extranonce information from the Upstream role to send to the Downstream role
    // once it connects
    let extended_extranonce = recv_extranonce.recv().await.unwrap();
    //let extranonce_len = extended_extranonce.get_len();
    //let min_extranonce_size = upstream.safe_lock(|s| s.min_extranonce_size).unwrap() as usize;

    // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
    downstream_sv1::Downstream::accept_connections(
        downstream_addr,
        sender_submit_from_sv1,
        recv_mining_notify_downstream,
        //extranonce_len - min_extranonce_size - (SELF_EXTRNONCE_LEN - 1),
        extended_extranonce,
        last_notify,
        target,
    )
    .await;
}
