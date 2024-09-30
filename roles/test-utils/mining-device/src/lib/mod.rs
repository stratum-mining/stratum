#![allow(clippy::option_map_unit_fn)]
pub(crate) mod device;
pub(crate) mod miner;
pub(crate) mod new_work_notifier;
pub(crate) mod next_share_outcome;
pub(crate) mod setup_connection_handler;

pub(crate) use device::Device;
pub(crate) use miner::Miner;
pub(crate) use new_work_notifier::NewWorkNotifier;
pub(crate) use next_share_outcome::NextShareOutcome;
pub(crate) use setup_connection_handler::SetupConnectionHandler;

use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection_tokio::Connection;
use std::{net::ToSocketAddrs, time::Duration};
use tokio::net::TcpStream;

use async_channel::{Receiver, Sender};
use codec_sv2::{Initiator, StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::MiningDeviceMessages;
use tracing::{error, info};

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub async fn connect(
    address: String,
    pub_key: Option<Secp256k1PublicKey>,
    device_id: Option<String>,
    user_id: Option<String>,
    handicap: u32,
) {
    let address = address
        .clone()
        .to_socket_addrs()
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port")
        .next()
        .expect("Invalid pool address, use one of this formats: ip:port, domain:port");
    info!("Connecting to pool at {}", address);
    let socket = loop {
        let pool = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(address)).await;
        match pool {
            Ok(result) => match result {
                Ok(socket) => break socket,
                Err(e) => {
                    error!(
                        "Failed to connect to Upstream role at {}, retrying in 5s: {}",
                        address, e
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
            Err(_) => {
                error!("Pool is unresponsive, terminating");
                std::process::exit(1);
            }
        }
    };
    info!("Pool tcp connection established at {}", address);
    let address = socket.peer_addr().unwrap();
    let initiator = Initiator::new(pub_key.map(|e| e.0));
    let (receiver, sender, _, _): (Receiver<EitherFrame>, Sender<EitherFrame>, _, _) =
        Connection::new(socket, codec_sv2::HandshakeRole::Initiator(initiator))
            .await
            .unwrap();
    info!("Pool noise connection established at {}", address);
    Device::start(receiver, sender, address, device_id, user_id, handicap).await
}
