use async_channel::{unbounded, Receiver, Sender};
use futures::{FutureExt, StreamExt};
use sv1_api::json_rpc;
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::TcpStream,
};
use tokio_util::codec::{FramedRead, LinesCodec};

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

const MAX_LINE_LENGTH: usize = 2_usize.pow(16);

impl ConnectionSV1 {
    /// Create a new connection set up to communicate with the other side of the given stream.
    ///
    /// Two tasks are spawned to handle reading and writing messages. The reading task will read
    /// messages from the stream and send them to the receiver channel. The writing task will read
    /// messages from the sender channel and write them to the stream.
    pub async fn new(stream: TcpStream) -> Self {
        let (reader_stream, mut writer_stream) = stream.into_split();
        let (sender_incoming, receiver_incoming) = unbounded();
        let (sender_outgoing, receiver_outgoing) = unbounded::<json_rpc::Message>();

        // Read Job
        tokio::task::spawn(async move {
            let reader = BufReader::new(reader_stream);
            let mut messages =
                FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_LINE_LENGTH));
            loop {
                tokio::select! {
                    res = messages.next().fuse() => {
                        match res {
                            Some(Ok(incoming)) => {
                                let incoming: json_rpc::Message = serde_json::from_str(&incoming).expect("Failed to parse incoming message");
                                if sender_incoming
                                    .send(incoming)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }

                            }
                            Some(Err(e)) => {
                                break tracing::error!("Error reading from stream: {:?}", e);
                            }
                            None => {
                                tracing::error!("No message received");
                            }
                        }
                    },
                    _ = tokio::signal::ctrl_c().fuse() => {
                        break;
                    }
                };
            }
        });

        // Write Job
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    res = receiver_outgoing.recv().fuse() => {
                        let to_send = res.expect("Failed to receive message");
                        let to_send = match serde_json::to_string(&to_send) {
                            Ok(string) => format!("{}\n", string),
                            Err(_e) => {
                                break;
                            }
                        };
                        let _ = writer_stream
                            .write_all(to_send.as_bytes())
                            .await;
                    },
                    _ = tokio::signal::ctrl_c().fuse() => {
                        break;
                    }
                };
            }
        });
        Self {
            receiver: receiver_incoming,
            sender: sender_outgoing,
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
