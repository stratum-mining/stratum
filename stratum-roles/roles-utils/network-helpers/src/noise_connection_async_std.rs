use async_channel::{bounded, Receiver, Sender};
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use binary_sv2::{Deserialize, Serialize};
use futures::lock::Mutex;
use std::{sync::Arc, time::Duration};
use tracing::{debug, error};

use binary_sv2::GetSize;
use codec_sv2::{HandshakeRole, Initiator, Responder, StandardEitherFrame, StandardNoiseDecoder};

use crate::Error;

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
        capacity: usize,
    ) -> Result<
        (
            Receiver<StandardEitherFrame<Message>>,
            Sender<StandardEitherFrame<Message>>,
        ),
        Error,
    > {
        let address = stream.peer_addr().map_err(|_| Error::SocketClosed)?;
        let (mut reader, writer) = (stream.clone(), stream.clone());

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);

        let state = codec_sv2::State::not_initialized(&role);

        let connection = Arc::new(Mutex::new(Self { state }));

        let cloned1 = connection.clone();
        let cloned2 = connection.clone();

        // RECEIVE AND PARSE INCOMING MESSAGES FROM TCP STREAM
        task::spawn(async move {
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
                                    task::yield_now().await;
                                    break;
                                }
                            }
                            Err(e) => {
                                if let codec_sv2::Error::MissingBytes(_) = e {
                                } else {
                                    error!("Shutting down noise stream reader! {:#?}", e);
                                    let _ = reader.shutdown(async_std::net::Shutdown::Both);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Shutting down noise stream reader! {:#?}", e);
                        let _ = reader.shutdown(async_std::net::Shutdown::Both);
                        break;
                    }
                }
            }
        });

        let receiver_outgoing_cloned = receiver_outgoing.clone();

        // ENCODE AND SEND INCOMING MESSAGES TO TCP STREAM
        task::spawn(async move {
            let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();

            loop {
                let received = receiver_outgoing_cloned.recv().await;
                match received {
                    Ok(frame) => {
                        let mut connection = cloned2.lock().await;
                        let b = match encoder.encode(frame, &mut connection.state) {
                            Ok(b) => b,
                            Err(e) => {
                                error!("Failed to encode noise frame: {:#?}", e);
                                let _ = writer.shutdown(async_std::net::Shutdown::Both);
                                break;
                            }
                        };

                        drop(connection);

                        let b = b.as_ref();

                        match (&writer).write_all(b).await {
                            Ok(_) => (),
                            Err(_e) => {
                                let _ = writer.shutdown(async_std::net::Shutdown::Both);
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = writer.shutdown(async_std::net::Shutdown::Both);
                        break;
                    }
                };
                crate::HANDSHAKE_READY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
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

pub async fn listen(
    address: &str,
    authority_public_key: [u8; 32],
    authority_private_key: [u8; 32],
    cert_validity: Duration,
    sender: Sender<(TcpStream, HandshakeRole)>,
) {
    let listner = TcpListener::bind(address).await.unwrap();
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        let responder = Responder::from_authority_kp(
            &authority_public_key,
            &authority_private_key,
            cert_validity,
        )
        .unwrap();
        let role = HandshakeRole::Responder(responder);
        let _ = sender.send((stream, role)).await;
    }
}
pub async fn connect(
    address: &str,
    authority_public_key: [u8; 32],
) -> Result<(TcpStream, HandshakeRole), ()> {
    let stream = TcpStream::connect(address).await.map_err(|_| ())?;
    let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
    let role = HandshakeRole::Initiator(initiator);
    Ok((stream, role))
}
