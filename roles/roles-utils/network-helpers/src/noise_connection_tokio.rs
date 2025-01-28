use crate::Error;
use binary_sv2::{Deserialize, Serialize};
use const_sv2::{INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE, RESPONDER_EXPECTED_HANDSHAKE_MESSAGE_SIZE};
use futures::lock::Mutex;
use std::{fmt::Debug, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{self, AbortHandle},
};

use std::{
    convert::TryInto,
    sync::atomic::AtomicBool,
};

use binary_sv2::GetSize;
use codec_sv2::{HandShakeFrame, HandshakeRole, Initiator, Responder, StandardEitherFrame, StandardNoiseDecoder};

use tracing::{debug, error};

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

impl crate::SetState for Connection {
    async fn set_state(self_: Arc<Mutex<Self>>, state: codec_sv2::State) {
        loop {
            if HANDSHAKE_READY.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(mut connection) = self_.try_lock() {
                    connection.state = state;
                    TRANSPORT_READY.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                };
            }
            task::yield_now().await;
        }
    }
}

impl Connection {
    #[allow(clippy::new_ret_no_self)]
    // Debug added for some trait requirement
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static + Clone + Debug>(
        stream: TcpStream,
        role: HandshakeRole,
    ) -> Result<
        (
            tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
            tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
            AbortHandle,
            AbortHandle,
        ),
        Error,
    > {
        let address = stream.peer_addr().map_err(|_| Error::SocketClosed)?;

        let (mut reader, mut writer) = stream.into_split();

        // let (sender_incoming, receiver_incoming): (
        //     Sender<StandardEitherFrame<Message>>,
        //     Receiver<StandardEitherFrame<Message>>,
        // ) = bounded(10); // TODO caller should provide this param
        let (sender_incoming, _): (
            tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
            tokio::sync::broadcast::Receiver<StandardEitherFrame<Message>>,
        ) = tokio::sync::broadcast::channel(10); // TODO caller should provide this param
        
        // let (sender_outgoing, receiver_outgoing): (
        //     Sender<StandardEitherFrame<Message>>,
        //     Receiver<StandardEitherFrame<Message>>,
        // ) = bounded(10); // TODO caller should provide this param

        let (sender_outgoing, mut receiver_outgoing): (
            tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
            tokio::sync::broadcast::Receiver<StandardEitherFrame<Message>>,
        ) = tokio::sync::broadcast::channel(10); // TODO caller should provide this param

        let state = codec_sv2::State::not_initialized(&role);

        let connection = Arc::new(Mutex::new(Self { state }));

        let cloned1 = connection.clone();
        let cloned2 = connection.clone();
        let sender_incoming_clone = sender_incoming.clone();
        // RECEIVE AND PARSE INCOMING MESSAGES FROM TCP STREAM
        let recv_task = task::spawn(async move {
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
                                if sender_incoming_clone.send(x).is_err() {
                                    error!("Shutting down noise stream reader!");
                                    task::yield_now().await;
                                    break;
                                }
                            }
                            Err(e) => {
                                if let codec_sv2::Error::MissingBytes(_) = e {
                                } else {
                                    error!("Shutting down noise stream reader! {:#?}", e);
                                    // sender_incoming.close();
                                    task::yield_now().await;
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

                        //kill thread without a panic - don't need to panic everytime a client
                        // disconnects
                        // sender_incoming.close();
                        task::yield_now().await;
                        break;
                    }
                }
            }
        });

        // ENCODE AND SEND INCOMING MESSAGES TO TCP STREAM
        let send_task = task::spawn(async move {
            let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();

            loop {
                let received = receiver_outgoing.recv().await;
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
                HANDSHAKE_READY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });

        // DO THE NOISE HANDSHAKE
        match role {
            HandshakeRole::Initiator(_) => {
                debug!("Initializing as downstream for - {}", &address);
                initialize_as_downstream_tokio(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    sender_incoming.clone().subscribe(),
                )
                .await?
            }
            HandshakeRole::Responder(_) => {
                debug!("Initializing as upstream for - {}", &address);
                initialize_as_upstream_tokio(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    sender_incoming.clone().subscribe(),
                )
                .await?
            }
        };
        debug!("Noise handshake complete - {}", &address);
        Ok((
            sender_incoming,
            sender_outgoing,
            recv_task.abort_handle(),
            send_task.abort_handle(),
        ))
    }
}

pub async fn listen(
    address: &str,
    authority_public_key: [u8; 32],
    authority_private_key: [u8; 32],
    cert_validity: Duration,
    sender: tokio::sync::broadcast::Sender<(TcpStream, HandshakeRole)>,
) {
    let listner = TcpListener::bind(address).await.unwrap();
    loop {
        if let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                &authority_public_key,
                &authority_private_key,
                cert_validity,
            )
            .unwrap();
            let role = HandshakeRole::Responder(responder);
            let _ = sender.send((stream, role));
        }
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



async fn initialize_as_downstream_tokio<
    'a,
    // Debug added for this unwrap, remove later
    // Clone is also trait requirement for broadcast channel
    Message: Serialize + Deserialize<'a> + GetSize + Debug + Clone,
    T: crate::SetState,
>(
    self_: Arc<Mutex<T>>,
    role: HandshakeRole,
    sender_outgoing: tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
    mut receiver_incoming: tokio::sync::broadcast::Receiver<StandardEitherFrame<Message>>,
) -> Result<(), Error> {
    let mut state = codec_sv2::State::initialized(role);

    // Create and send first handshake message
    let first_message = state.step_0()?;
    sender_outgoing.send(first_message.into()).unwrap();

    // Receive and deserialize second handshake message
    let second_message = receiver_incoming.recv().await.unwrap();

    let second_message: HandShakeFrame = second_message
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
    let second_message: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] = second_message
        .get_payload_when_handshaking()
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;

    // Create and send thirth handshake message
    let transport_mode = state.step_2(second_message)?;

    T::set_state(self_, transport_mode).await;
    while !TRANSPORT_READY.load(std::sync::atomic::Ordering::SeqCst) {
        std::hint::spin_loop()
    }
    Ok(())
}

// Addition of Debug, for some trait requirement
// clone is again a broadcast requirement
async fn initialize_as_upstream_tokio<'a, Message: Serialize + Deserialize<'a> + GetSize + Debug + Clone, T: crate::SetState>(
    self_: Arc<Mutex<T>>,
    role: HandshakeRole,
    sender_outgoing: tokio::sync::broadcast::Sender<StandardEitherFrame<Message>>,
    mut receiver_incoming: tokio::sync::broadcast::Receiver<StandardEitherFrame<Message>>,
) -> Result<(), Error> {
    let mut state = codec_sv2::State::initialized(role);

    // Receive and deserialize first handshake message
    let first_message: HandShakeFrame = receiver_incoming
        .recv()
        .await.unwrap()
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
    let first_message: [u8; RESPONDER_EXPECTED_HANDSHAKE_MESSAGE_SIZE] = first_message
        .get_payload_when_handshaking()
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;

    // Create and send second handshake message
    let (second_message, transport_mode) = state.step_1(first_message)?;
    HANDSHAKE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
    sender_outgoing.send(second_message.into()).unwrap();

    // This sets the state to Handshake state - this prompts the task above to move the state
    // to transport mode so that the next incoming message will be decoded correctly
    // It is important to do this directly before sending the fourth message
    T::set_state(self_, transport_mode).await;
    while !TRANSPORT_READY.load(std::sync::atomic::Ordering::SeqCst) {
        std::hint::spin_loop()
    }

    Ok(())
}

static HANDSHAKE_READY: AtomicBool = AtomicBool::new(false);
static TRANSPORT_READY: AtomicBool = AtomicBool::new(false);
