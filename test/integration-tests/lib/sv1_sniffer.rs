#![cfg(feature = "sv1")]
use async_channel::{Receiver, Sender};
use network_helpers_sv2::sv1_connection::ConnectionSV1;
use std::{collections::VecDeque, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::Mutex,
    time::sleep,
};

#[derive(Debug, PartialEq)]
enum SnifferError {
    DownstreamClosed,
    UpstreamClosed,
}

/// Represents an SV1 sniffer.
///
/// This struct acts as a middleman between two SV1 roles. It forwards messages from one role to
/// the other and vice versa. It also provides methods to wait for specific messages to be received
/// from the downstream or upstream role.
#[derive(Debug, Clone)]
pub struct SnifferSV1 {
    listening_address: SocketAddr,
    upstream_address: SocketAddr,
    messages_from_downstream_sv1: MessagesAggregatorSV1,
    messages_from_upstream_sv1: MessagesAggregatorSV1,
}

impl SnifferSV1 {
    /// Create a new [`SnifferSV1`] instance.
    ///
    /// The listening address is the address the sniffer will listen on for incoming connections
    /// from the downstream role. The upstream address is the address the sniffer will connect to
    /// in order to forward messages to the upstream role.
    pub fn new(listening_address: SocketAddr, upstream_address: SocketAddr) -> Self {
        Self {
            listening_address,
            upstream_address,
            messages_from_downstream_sv1: MessagesAggregatorSV1::new(),
            messages_from_upstream_sv1: MessagesAggregatorSV1::new(),
        }
    }

    /// Start the sniffer.
    pub fn start(&self) {
        let upstream_address = self.upstream_address.clone();
        let listening_address = self.listening_address.clone();
        let messages_from_downstream_sv1 = self.messages_from_downstream_sv1.clone();
        let messages_from_upstream_sv1 = self.messages_from_upstream_sv1.clone();
        tokio::spawn(async move {
            let sniffer_to_upstream_stream = TcpStream::connect(upstream_address)
                .await
                .expect("Failed to connect to upstream");
            let listener = TcpListener::bind(listening_address)
                .await
                .expect("Failed to listen on given address");
            let (downstream_stream, _) = listener
                .accept()
                .await
                .expect("Failed to accept downstream connection");
            let sniffer_to_upstream_connection =
                ConnectionSV1::new(sniffer_to_upstream_stream).await;
            let downstream_to_sniffer_connection = ConnectionSV1::new(downstream_stream).await;
            select! {
                _ = tokio::signal::ctrl_c() => { },
                _ = Self::recv_from_down_send_to_up_sv1(
                    downstream_to_sniffer_connection.receiver(),
                    sniffer_to_upstream_connection.sender(),
                    messages_from_downstream_sv1
                ) => { },
                _ = Self::recv_from_up_send_to_down_sv1(
                    sniffer_to_upstream_connection.receiver(),
                    downstream_to_sniffer_connection.sender(),
                    messages_from_upstream_sv1
                ) => { },
            };
        });
    }

    /// Wait for a specific message to be received from the downstream role.
    pub async fn wait_for_message_from_downstream(&self, message: &str) {
        let now = std::time::Instant::now();
        tokio::select!(
            _ = tokio::signal::ctrl_c() => { },
            _ = async {
                loop {
                    if self.messages_from_downstream_sv1.has_message(&[message]).await {
                        break;
                    }
                    if now.elapsed().as_secs() > 60 {
                        panic!(
                            "Timeout: SV1 message {} from downstream not received",
                            message
                        );
                    } else {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                }
            } => {}
        );
    }

    /// Wait for a specific message to be received from the upstream role.
    pub async fn wait_for_message_from_upstream(&self, message: &[&str]) {
        let now = std::time::Instant::now();
        tokio::select!(
            _ = tokio::signal::ctrl_c() => { },
            _ = async {
                loop {
                    if self.messages_from_upstream_sv1.has_message(message).await {
                        break;
                    }
                    if now.elapsed().as_secs() > 60 {
                        panic!(
                            "Timeout: SV1 message {} from upstream not received",
                            message.join(" ")
                        );
                    } else {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                }
            } => {}
        );
    }

    async fn recv_from_up_send_to_down_sv1(
        recv: Receiver<sv1_api::Message>,
        send: Sender<sv1_api::Message>,
        upstream_messages: MessagesAggregatorSV1,
    ) -> Result<(), SnifferError> {
        while let Ok(msg) = recv.recv().await {
            send.send(msg.clone())
                .await
                .map_err(|_| SnifferError::DownstreamClosed)?;
            upstream_messages.add_message(msg.clone()).await;
            sleep(Duration::from_millis(100)).await;
        }
        Err(SnifferError::UpstreamClosed)
    }

    async fn recv_from_down_send_to_up_sv1(
        recv: Receiver<sv1_api::Message>,
        send: Sender<sv1_api::Message>,
        downstream_messages: MessagesAggregatorSV1,
    ) -> Result<(), SnifferError> {
        while let Ok(msg) = recv.recv().await {
            send.send(msg.clone())
                .await
                .map_err(|_| SnifferError::UpstreamClosed)?;
            downstream_messages.add_message(msg).await;
        }
        Err(SnifferError::DownstreamClosed)
    }
}

/// Represents a SV1 message manager.
///
/// This struct can be used in order to aggregate and manage SV1 messages.
#[derive(Debug, Clone)]
pub(crate) struct MessagesAggregatorSV1 {
    messages: Arc<Mutex<VecDeque<sv1_api::Message>>>,
}

impl MessagesAggregatorSV1 {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn add_message(&self, message: sv1_api::Message) {
        let mut messages = self.messages.lock().await;
        messages.push_back(message);
    }

    async fn has_message(&self, expected_msg: &[&str]) -> bool {
        let messages = self.messages.lock().await;
        let ret = messages.iter().any(|msg| match msg {
            sv1_api::Message::StandardRequest(req) => req.method == *expected_msg.get(0).unwrap(),
            sv1_api::Message::Notification(notif) => notif.method == *expected_msg.get(0).unwrap(),
            sv1_api::Message::OkResponse(res) => {
                if let Ok(res) = corepc_node::serde_json::to_string(&res) {
                    for m in expected_msg {
                        if !res.contains(m) {
                            return false;
                        }
                    }
                    return true;
                } else {
                    false
                }
            }
            sv1_api::Message::ErrorResponse(res) => {
                res.error.clone().unwrap().message == *expected_msg.get(0).unwrap()
            }
        });
        ret
    }
}
