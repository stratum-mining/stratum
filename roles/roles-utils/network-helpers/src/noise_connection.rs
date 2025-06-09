use crate::Error;
use async_channel::{unbounded, Receiver, Sender};
use futures::lock::Mutex;
use roles_logic_sv2::codec_sv2::{
    self,
    binary_sv2::{Deserialize, GetSize, Serialize},
    HandshakeRole, StandardEitherFrame, StandardNoiseDecoder,
};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    select, task,
};
use tracing::{debug, error};

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

impl crate::SetState for Connection {
    async fn set_state(self_: Arc<Mutex<Self>>, state: codec_sv2::State) {
        loop {
            if crate::HANDSHAKE_READY.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(mut connection) = self_.try_lock() {
                    connection.state = state;
                    crate::TRANSPORT_READY.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                };
            }
            task::yield_now().await;
        }
    }
}

impl Connection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
        role: HandshakeRole,
    ) -> Result<
        (
            Receiver<StandardEitherFrame<Message>>,
            Sender<StandardEitherFrame<Message>>,
        ),
        Error,
    > {
        let address = stream.peer_addr().map_err(|_| Error::SocketClosed)?;

        let (mut reader, mut writer) = stream.into_split();

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = unbounded();
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = unbounded();

        let state = codec_sv2::State::not_initialized(&role);

        let connection = Arc::new(Mutex::new(Self { state }));

        let cloned1 = connection.clone();
        let cloned2 = connection.clone();

        task::spawn(async move {
            select!(
              _ = tokio::signal::ctrl_c() => { },
              _ = async {
                let mut decoder = StandardNoiseDecoder::<Message>::new();
                loop {
                  let writable = decoder.writable();
                  match reader.read_exact(writable).await {
                    Ok(_) => {
                      let mut connection = cloned1.lock().await;
                      let decoded = decoder.next_frame(&mut connection.state);
                      drop(connection);
                      match decoded {
                        Ok(x) => {
                          if sender_incoming.send(x).await.is_err() {
                            error!("Shutting down noise stream reader!");
                            break;
                          }
                        }
                        Err(e) => {
                          if let codec_sv2::Error::MissingBytes(_) = e {
                          } else {
                            error!("Shutting down noise stream reader! {:#?}", e);
                            sender_incoming.close();
                            break;
                          }
                        }
                      }
                    }
                    Err(e) => {
                      error!(
                        "Disconnected from client while reading : {} - {}",
                        e, &address
                      );
                      sender_incoming.close();
                      break;
                    }
                  }
                }
              } => {}
            );
        });

        let receiver_outgoing_cloned = receiver_outgoing.clone();
        task::spawn(async move {
            select!(
              _ = tokio::signal::ctrl_c() => { },
              _ = async {
                let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();
                loop {
                  let received = receiver_outgoing_cloned.recv().await;

                  match received {
                    Ok(frame) => {
                      let mut connection = cloned2.lock().await;
                      let b = encoder.encode(frame, &mut connection.state).unwrap();
                      drop(connection);
                      let b = b.as_ref();
                      match (writer).write_all(b).await {
                        Ok(_) => (),
                        Err(e) => {
                          let _ = writer.shutdown().await;
                          // Just fail and force to reinitialize everything
                          error!(
                            "Disconnecting from client due to error writing: {} - {}",
                            e, &address
                          );
                          task::yield_now().await;
                          break;
                        }
                      }
                    }
                    Err(e) => {
                      // Just fail and force to reinitialize everything
                      let _ = writer.shutdown().await;
                      error!(
                        "Disconnecting from client due to error receiving: {} - {}",
                        e, &address
                      );
                      task::yield_now().await;
                      break;
                    }
                  };
                  crate::HANDSHAKE_READY.store(true, std::sync::atomic::Ordering::Relaxed);
                }
              } => {}
            );
        });

        // DO THE NOISE HANDSHAKE
        match role {
            HandshakeRole::Initiator(_) => {
                debug!("Initializing as downstream for - {}", &address);
                crate::initialize_as_downstream(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    receiver_incoming.clone(),
                )
                .await?
            }
            HandshakeRole::Responder(_) => {
                debug!("Initializing as upstream for - {}", &address);
                crate::initialize_as_upstream(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    receiver_incoming.clone(),
                )
                .await?
            }
        };
        debug!("Noise handshake complete - {}", &address);
        Ok((receiver_incoming, sender_outgoing))
    }
}
