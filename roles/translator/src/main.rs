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

use async_channel::{bounded, Receiver, Sender};
use async_std::task;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use v1::{server_to_client, utils::HexBytes};

/// Process CLI args, if any.
fn process_cli_args() -> ProxyResult<ProxyConfig> {
    let args = match Args::from_args() {
        Ok(cfg) => cfg,
        Err(help) => {
            println!("{}", help);
            return Err(Error::BadCliArgs);
        }
    };
    let config_file = std::fs::read_to_string(args.config_path)?;
    Ok(toml::from_str::<ProxyConfig>(&config_file)?)
}

#[async_std::main]
async fn main() {
    let proxy_config = process_cli_args().unwrap();
    println!("PC: {:?}", &proxy_config);
    // `sender_submit_from_sv1` sender is used by `Downstream` to send a `mining.submit` message to
    // `Bridge` via the `recv_submit_from_sv1` receiver
    // (Sender<v1::client_to_server::Submit>, Receiver<Submit>)
    let (sender_submit_from_sv1, recv_submit_from_sv1) = bounded(10);
    // `sender_submit_to_sv2` sender is used by `Bridge` to send a `SubmitSharesExtended` message
    // to `Upstream` via the `recv_submit_to_sv2` receiver
    // (Sender<SubmitSharesExtended<'static>>, Receiver<SubmitSharesExtended<'static>>)
    let (sender_submit_to_sv2, recv_submit_to_sv2) = bounded(10);

    // `sender_new_prev_hash` sender is used by `Upstream` to send a `SetNewPrevHash` to `Bridge`
    // via the `recv_new_prev_hash` receiver
    // (Sender<SetNewPrevHash<'static>>, Receiver<SetNewPrevHash<'static>>)
    let (sender_new_prev_hash, recv_new_prev_hash) = bounded(10);

    // `sender_new_extended_mining_job` sender is used by `Upstream` to send a
    // `NewExtendedMiningJob` to `Bridge` via the `recv_new_extended_mining_job` receiver
    // (Sender<NewExtendedMiningJob<'static>>, Receiver<NewExtendedMiningJob<'static>>)
    let (sender_new_extended_mining_job, recv_new_extended_mining_job) = bounded(10);

    // TODO add a channel to send new jobs from Bridge to Downstream
    // Put NextMiningNotify in a mutex
    // NextMiningNotify should have channel to Downstream?
    // Put recv in next_mining_notify struct
    let (sender_mining_notify_bridge, recv_mining_notify_downstream): (
        Sender<server_to_client::Notify>,
        Receiver<server_to_client::Notify>,
    ) = bounded(10);

    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address).unwrap(),
        proxy_config.upstream_port,
    );

    // Instantiate a new `Upstream`
    let upstream = upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey,
        recv_submit_to_sv2,
        sender_new_prev_hash,
        sender_new_extended_mining_job,
    )
    .await
    .unwrap();
    // Connects to the SV2 Upstream role
    upstream_sv2::Upstream::connect(
        upstream.clone(),
        proxy_config.min_supported_version,
        proxy_config.max_supported_version,
    )
    .await
    .unwrap();
    // Start receiving messages from the SV2 Upstream role
    upstream_sv2::Upstream::parse_incoming(upstream.clone());
    // Start receiving submit from the SV1 Downstream role
    upstream_sv2::Upstream::on_submit(upstream.clone());
    let next_mining_notify = Arc::new(Mutex::new(NextMiningNotify::new()));

    // Instantiates a new `Bridge` and begins handling incoming messages
    proxy::Bridge::new(
        recv_submit_from_sv1,
        sender_submit_to_sv2,
        recv_new_prev_hash,
        recv_new_extended_mining_job,
        next_mining_notify,
        sender_mining_notify_bridge,
    )
    .start();

    // Format `Downstream` connection address
    let downstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
        proxy_config.downstream_port,
    );

    // Get the `extranonce_size` size received from the Upstream to be sent to the Downstream as
    // the `extranonce2_size` field in the SV1 `mining.subscribe` message response.
    let extranonce_size = upstream.safe_lock(|u| u.extranonce_size).unwrap() as usize;

    // Get the `extranonce_prefix` size received from the Upstream to be sent to the Downstream as
    // the `extranonce1` field in the SV1 `mining.subscribe` message response.
    // let extranonce_prefix = upstream
    //     .safe_lock(|u| u.extranonce_prefix.clone().unwrap())
    //     .unwrap();
    // let extranonce_prefix =
    //     extranonce_prefix.expect("Expected `extranonce_prefix` to be set by the Upstream");
    // let extranonce1: HexBytes = extranonce_prefix.to_vec().try_into().unwrap();

    // TODO: Tmp until extranonce_prefix static lifetime can be fixed in Upstream
    let extranonce1: HexBytes = "08000002".try_into().unwrap();

    // Accept connections from one or more SV1 Downstream roles (SV1 Mining Devices)
    downstream_sv1::Downstream::accept_connections(
        downstream_addr,
        sender_submit_from_sv1,
        recv_mining_notify_downstream,
        extranonce1,
        extranonce_size,
    );

    // If this loop is not here, the proxy does not stay live long enough for a Downstream to
    // connect
    loop {
        task::sleep(Duration::from_secs(1)).await;
    }
}
