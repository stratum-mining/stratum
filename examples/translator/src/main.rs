use async_channel::{Receiver, Sender};
use codec_sv2::StandardEitherFrame;
use network_helpers::plain_connection_tokio::PlainConnection;
use roles_logic_sv2::{
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    common_properties::CommonDownstreamData,
    errors::Error,
    handlers::common::{ParseDownstreamCommonMessages, ParseUpstreamCommonMessages},
    parsers::{CommonMessages, MiningDeviceMessages},
    routing_logic::NoRouting,
    utils::Mutex,
};
use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use tokio::net::{TcpListener, TcpStream};

/// Downstream client (typically the Mining Device) connection address + port
const DOWNSTREAM_ADDR: &str = "127.0.0.1:34255";

pub type Message = MiningDeviceMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// Upstream configuration values
#[derive(Debug, Deserialize)]
pub struct UpstreamValues {
    address: String,
    port: u16,
    pub_key: [u8; 32],
}

/// Upstream server connection configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    upstreams: Vec<UpstreamValues>,
    listen_address: String,
    listen_mining_port: u16,
    max_supported_version: u16,
    min_supported_version: u16,
}

/// Handles the opening connections:
/// 1. Downstream (Mining Device) <-> Upstream Proxy
/// 2. Downstream Proxy <-> Upstream Pool
struct SetupConnectionHandler {}

/// Implement the `ParseUpstreamCommonMessages` trait for `SetupConnectionHandler`.
impl ParseUpstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    /// Upstream sends the Downstream (this proxy) back a `SetupConnection.Success` message on a
    /// successful connection setup. This functions handles that response.
    fn handle_setup_connection_success(
        &mut self,
        _: SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        Ok(SendTo::None(None))
    }

    /// Upstream sends the Downstream (this proxy) back a `SetupConnection.Error` message on an
    /// unsuccessful connection setup. This functions handles that response.
    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }
}

impl ParseDownstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
        _: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        // self.header_only = Some(header_only);
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: 0,
                used_version: 2,
            }),
        ))
    }
}

/// A 1 to 1 connection with an upstream node that implements the mining (sub)protocol.
/// The upstream node it connects with is most typically a pool, but could also be another proxy.
pub struct Upstream {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
}

pub struct Downstream {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
}

/// Sv1 Upstream (Miner) <-> Sv1/Sv2 Proxy <-> Sv2 Upstream (Pool)
/// 1. Define the socket where the server will listen for the incoming connection
/// 2. Server binds to a socket and starts listening
/// 3. A Downstream client connects
/// 4. Server opens the connection and initializes it via a `PlainConnection` that returns a
/// `Receiver<EitherFrame>` and a `Sender<EitherFrame>`. Messages are sent to the downstream client
/// (most typically the Mining Device) via the `Sender`. Messages sent by the downstream client are
/// received by the proxy via the `Receiver`, then parsed.
#[tokio::main]
async fn main() {
    println!("Hello, sv1 to sv2 translator!");

    // 1. Define the socket where the server will listen for the incoming connection
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    let socket = SocketAddr::new(
        IpAddr::from_str(&config.listen_address).unwrap(),
        config.listen_mining_port,
    );
    // 2. Server binds to a socket and starts listening
    let listner = TcpListener::bind(DOWNSTREAM_ADDR).await.unwrap();
    println!("PROXY INITIALIZED");

    // Spawn downstream tasks
    tokio::task::spawn(async {
        // 3. A Downstream client connects
        let stream = TcpStream::connect(DOWNSTREAM_ADDR).await.unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let received = receiver.recv().await;
    });

    // 4. Server opens the connection and initializes it via a `PlainConnection` that returns a
    // `Receiver<EitherFrame>` and a `Sender<EitherFrame>`. Messages are sent to the downstream client
    // (most typically the Mining Device) via the `Sender`. Messages sent by the downstream client are
    // received by the proxy via the `Receiver`, then parsed.
    while let Ok((stream, _)) = listner.accept().await {
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let received = receiver.recv().await;
    }
}
