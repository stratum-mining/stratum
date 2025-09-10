#![allow(clippy::new_ret_no_self)]
use crate::{
    noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    Error,
};
use async_channel::{unbounded, Receiver, Sender};
use codec_sv2::{
    binary_sv2::{Deserialize, GetSize, Serialize},
    HandshakeRole, StandardEitherFrame,
};
use std::sync::Arc;
use tokio::{net::TcpStream, task};
use tracing::{debug, error};

pub struct Connection;

struct ConnectionState<Message> {
    sender_incoming: Sender<StandardEitherFrame<Message>>,
    receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    sender_outgoing: Sender<StandardEitherFrame<Message>>,
    receiver_outgoing: Receiver<StandardEitherFrame<Message>>,
}

impl<Message> ConnectionState<Message> {
    fn close_all(&self) {
        self.sender_incoming.close();
        self.receiver_incoming.close();
        self.sender_outgoing.close();
        self.receiver_outgoing.close();
    }
}

impl Connection {
    pub async fn new<Message>(
        stream: TcpStream,
        role: HandshakeRole,
    ) -> Result<
        (
            Receiver<StandardEitherFrame<Message>>,
            Sender<StandardEitherFrame<Message>>,
        ),
        Error,
    >
    where
        Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
    {
        let (sender_incoming, receiver_incoming) = unbounded();
        let (sender_outgoing, receiver_outgoing) = unbounded();

        let conn_state = Arc::new(ConnectionState {
            sender_incoming,
            receiver_incoming: receiver_incoming.clone(),
            sender_outgoing: sender_outgoing.clone(),
            receiver_outgoing,
        });

        let (read_half, write_half) = NoiseTcpStream::<Message>::new(stream, role)
            .await?
            .into_split();

        Self::spawn_reader(read_half, Arc::clone(&conn_state));
        Self::spawn_writer(write_half, conn_state);

        Ok((receiver_incoming, sender_outgoing))
    }
    fn spawn_reader<Message>(
        mut read_half: NoiseTcpReadHalf<Message>,
        conn_state: Arc<ConnectionState<Message>>,
    ) -> task::JoinHandle<()>
    where
        Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
    {
        let sender_incoming = conn_state.sender_incoming.clone();

        task::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        debug!("Reader received shutdown signal.");
                        break;
                    }
                    res = read_half.read_frame() => match res {
                        Ok(frame) => {
                            if sender_incoming.send(frame).await.is_err() {
                                error!("Reader: channel closed, shutting down.");
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Reader: error while reading frame: {e:?}");
                            break;
                        }
                    }
                }
            }

            conn_state.close_all();
        })
    }

    fn spawn_writer<Message>(
        mut write_half: NoiseTcpWriteHalf<Message>,
        conn_state: Arc<ConnectionState<Message>>,
    ) -> task::JoinHandle<()>
    where
        Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
    {
        let receiver_outgoing = conn_state.receiver_outgoing.clone();

        task::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        debug!("Writer received shutdown signal.");
                        break;
                    }
                    res = receiver_outgoing.recv() => match res {
                        Ok(frame) => {
                            if let Err(e) = write_half.write_frame(frame).await {
                                error!("Writer: error while writing frame: {e:?}");
                                break;
                            }
                        }
                        Err(_) => {
                            debug!("Writer: channel closed, shutting down.");
                            break;
                        }
                    }
                }
            }

            if let Err(e) = write_half.shutdown().await {
                error!("Writer: error during shutdown: {e:?}");
            }

            conn_state.close_all();
        })
    }
}
