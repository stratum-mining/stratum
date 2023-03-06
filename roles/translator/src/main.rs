mod args;
mod downstream_sv1;
mod error;
mod job_negotiator;
mod proxy;
mod proxy_config;
mod status;
mod template_receiver;
mod upstream_sv2;
use args::Args;
use error::{Error, ProxyResult};
use job_negotiator::JobNegotiator;
use proxy_config::ProxyConfig;
use roles_logic_sv2::utils::Mutex;
use template_receiver::TemplateRx;

const SELF_EXTRNONCE_LEN: usize = 2;

use async_channel::{bounded, unbounded};
use futures::{join, select, FutureExt};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use tokio::sync::broadcast;
use v1::server_to_client;

use crate::status::{State, Status};
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

#[tokio::main]
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
    let (tx_sv1_notify, _rx_sv1_notify): (
        broadcast::Sender<server_to_client::Notify>,
        broadcast::Receiver<server_to_client::Notify>,
    ) = broadcast::channel(10);

    // Format `Upstream` connection address
    let upstream_addr = SocketAddr::new(
        IpAddr::from_str(&proxy_config.upstream_address)
            .expect("Failed to parse upstream address!"),
        proxy_config.upstream_port,
    );

    // channel for template
    let (send_tp, recv_tp) = bounded(10);
    // channel for prev hash
    let (send_ph, recv_ph) = bounded(10);

    let (send_mining_job, recv_mining_job) = bounded(10);

    let (send_coinbase_out, recv_coinbase_out) = bounded(10);
    let (send_solution, recv_solution) = bounded(10);

    // If there is a jn_config in proxy_config creates a reciver for template and prev hash.
    // They will be used by the JN once is initialized
    let (bridge_upstream_kind, upstream_upstream_kind) = match proxy_config.jn_config.clone() {
        None => (
            proxy::bridge::UpstreamKind::Standard,
            upstream_sv2::UpstreamKind::Standard,
        ),
        Some(_jn_config) => (
            proxy::bridge::UpstreamKind::WithNegotiator {
                recv_tp,
                recv_ph,
                send_mining_job,
                recv_coinbase_out,
                send_solution,
            },
            upstream_sv2::UpstreamKind::WithNegotiator { recv_mining_job },
        ),
    };

    // Instantiate a new `Upstream` (SV2 Pool)
    let upstream = match upstream_sv2::Upstream::new(
        upstream_addr,
        proxy_config.upstream_authority_pubkey.clone(),
        rx_sv2_submit_shares_ext,
        tx_sv2_set_new_prev_hash,
        tx_sv2_new_ext_mining_job,
        proxy_config.min_extranonce2_size,
        tx_sv2_extranonce,
        status::Sender::Upstream(tx_status.clone()),
        target.clone(),
        upstream_upstream_kind,
    )
    .await
    {
        Ok(upstream) => upstream,
        Err(e) => {
            error!("Failed to create upstream: {}", e);
            return;
        }
    };

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

    // If jn_config start JN and TempalteRx
    match proxy_config.jn_config.clone() {
        None => (),
        Some(jn_config) => {
            let (send_comas, recv_comas) = bounded(10);
            let mut parts = jn_config.tp_address.split(':');
            let ip_tp = parts.next().unwrap().to_string();
            let port_tp = parts.next().unwrap().parse::<u16>().unwrap();
            let mut parts = jn_config.jn_address.split(':');
            let ip_jn = parts.next().unwrap().to_string();
            let port_jn = parts.next().unwrap().parse::<u16>().unwrap();
            join!(
                JobNegotiator::new(
                    SocketAddr::new(IpAddr::from_str(ip_jn.as_str()).unwrap(), port_jn,),
                    proxy_config
                        .upstream_authority_pubkey
                        .clone()
                        .into_inner()
                        .as_bytes()
                        .to_owned(),
                    send_comas,
                    send_coinbase_out,
                    proxy_config.clone(),
                ),
                TemplateRx::connect(
                    SocketAddr::new(IpAddr::from_str(ip_tp.as_str()).unwrap(), port_tp,),
                    send_tp,
                    send_ph,
                    recv_comas,
                    recv_solution,
                    status::Sender::TemplateReceiver(tx_status.clone()),
                ),
            );
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

    // Instantiate a new `Bridge` and begins handling incoming messages
    let b = proxy::Bridge::new(
        rx_sv1_submit,
        tx_sv2_submit_shares_ext,
        rx_sv2_set_new_prev_hash,
        rx_sv2_new_ext_mining_job,
        tx_sv1_notify.clone(),
        status::Sender::Bridge(tx_status.clone()),
        extended_extranonce,
        target,
        up_id,
        bridge_upstream_kind,
    );
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
        tx_sv1_notify,
        status::Sender::DownstreamListener(tx_status.clone()),
        b,
    );

    let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());

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
            State::Healthy(msg) => {
                info!("HEALTHY message: {}", msg);
            }
        }
    }
}
