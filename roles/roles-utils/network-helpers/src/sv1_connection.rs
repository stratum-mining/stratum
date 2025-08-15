use async_channel::{unbounded, Receiver, Sender};
use futures::StreamExt;
use sv1_api::json_rpc;
use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
};
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::{error, trace, warn};

/// Represents a connection between two roles communicating using SV1 protocol.
///
/// This struct can be used to read and write messages to the other side of the connection.  The
/// channel is unidirectional, i.e., each [`ConnectionSV1`] instance handles the connection either
/// from the upstream perspective or the downstream perspective. In order to communicate in both
/// directions, you will need two instances of this struct.
#[derive(Debug)]
pub struct ConnectionSV1 {
    receiver: Receiver<json_rpc::Message>,
    sender: Sender<json_rpc::Message>,
}

struct ConnectionState {
    receiver_outgoing: Receiver<json_rpc::Message>,
    sender_outgoing: Sender<json_rpc::Message>,
    receiver_incoming: Receiver<json_rpc::Message>,
    sender_incoming: Sender<json_rpc::Message>,
}

impl ConnectionState {
    fn new(
        receiver_outgoing: Receiver<json_rpc::Message>,
        sender_outgoing: Sender<json_rpc::Message>,
        receiver_incoming: Receiver<json_rpc::Message>,
        sender_incoming: Sender<json_rpc::Message>,
    ) -> Self {
        Self {
            receiver_incoming,
            receiver_outgoing,
            sender_incoming,
            sender_outgoing,
        }
    }

    fn close(&self) {
        self.receiver_incoming.close();
        self.receiver_outgoing.close();
        self.sender_incoming.close();
        self.sender_outgoing.close();
    }
}

const MAX_LINE_LENGTH: usize = 1 << 16;

impl ConnectionSV1 {
    pub async fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        let (sender_incoming, receiver_incoming) = unbounded();
        let (sender_outgoing, receiver_outgoing) = unbounded();

        let buffer_read_half = BufReader::new(read_half);
        let buffer_write_half = BufWriter::new(write_half);

        let connection_state = ConnectionState::new(
            receiver_outgoing.clone(),
            sender_outgoing.clone(),
            receiver_incoming.clone(),
            sender_incoming.clone(),
        );

        tokio::spawn(async move {
            tokio::select! {
                _ = Self::run_reader(buffer_read_half, sender_incoming.clone()) => {
                    trace!("Reader task exited. Closing writer sender.");
                    connection_state.close();
                }
                _ = Self::run_writer(buffer_write_half, receiver_outgoing.clone()) => {
                    trace!("Writer task exited. Closing reader sender.");
                    connection_state.close();
                }
            }
        });

        Self {
            receiver: receiver_incoming,
            sender: sender_outgoing,
        }
    }

    async fn run_reader(
        reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
        sender: Sender<json_rpc::Message>,
    ) {
        let mut lines = FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_LINE_LENGTH));
        while let Some(result) = lines.next().await {
            match result {
                Ok(line) => match serde_json::from_str::<json_rpc::Message>(&line) {
                    Ok(msg) => {
                        if sender.send(msg).await.is_err() {
                            warn!("Receiver dropped, stopping reader");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to deserialize message: {e:?}");
                    }
                },
                Err(e) => {
                    error!("Error reading from stream: {e:?}");
                    break;
                }
            }
        }
    }

    async fn run_writer(
        mut writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
        receiver: Receiver<json_rpc::Message>,
    ) {
        while let Ok(msg) = receiver.recv().await {
            match serde_json::to_string(&msg) {
                Ok(line) => {
                    let data = format!("{line}\n");
                    if writer.write_all(data.as_bytes()).await.is_err() {
                        error!("Failed to write to stream");
                        break;
                    }
                    if writer.flush().await.is_err() {
                        error!("Failed to flush writer.");
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {e:?}");
                    break;
                }
            }
        }
    }

    /// Send a message to the other side of the connection.
    pub async fn send(&self, msg: json_rpc::Message) -> bool {
        self.sender.send(msg).await.is_ok()
    }

    /// Receive a message from the other side of the connection.
    pub async fn receive(&self) -> Option<json_rpc::Message> {
        self.receiver.recv().await.ok()
    }

    /// Get a clone of the receiver channel.
    pub fn receiver(&self) -> Receiver<json_rpc::Message> {
        self.receiver.clone()
    }

    /// Get a clone of the sender channel.
    pub fn sender(&self) -> Sender<json_rpc::Message> {
        self.sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn test_sv1_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let downstream_stream = TcpStream::connect(addr).await.unwrap();
        let (upstream_stream, _) = listener.accept().await.unwrap();

        let upstream_connection = ConnectionSV1::new(upstream_stream).await;
        let downstream_connection = ConnectionSV1::new(downstream_stream).await;
        let message = json_rpc::Message::StandardRequest(json_rpc::StandardRequest {
            id: 1,
            method: "test".to_string(),
            params: serde_json::Value::Null,
        });
        assert!(downstream_connection.send(message).await);
        let received_on_upstream = upstream_connection.receive().await.unwrap();
        match received_on_upstream {
            json_rpc::Message::StandardRequest(received) => {
                assert_eq!(received.id, 1);
                assert_eq!(received.method, "test".to_string());
                assert_eq!(received.params, serde_json::Value::Null);
            }
            _ => {
                panic!("Unexpected message type");
            }
        }
        let upstream_response = json_rpc::Message::OkResponse(json_rpc::Response {
            id: 1,
            result: serde_json::Value::String("response".to_string()),
            error: None,
        });
        assert!(upstream_connection.send(upstream_response).await);
        let received_upstream = downstream_connection.receive().await.unwrap();
        match received_upstream {
            json_rpc::Message::OkResponse(received) => {
                assert_eq!(received.id, 1);
                assert_eq!(
                    received.result,
                    serde_json::Value::String("response".to_string())
                );
            }
            _ => {
                panic!("Unexpected message type");
            }
        }
    }
}
