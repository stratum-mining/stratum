use async_channel::{bounded, Receiver, Sender};
use binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task,
};

use binary_sv2::GetSize;
use codec_sv2::{
    Frame, HandShakeFrame, HandshakeRole, Initiator, Responder, StandardEitherFrame,
    StandardNoiseDecoder,
};

use tracing::{debug, error, info};

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

impl Connection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
        role: HandshakeRole,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        let address = stream.peer_addr().unwrap();

        let (mut reader, mut writer) = stream.into_split();

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(10); // TODO caller should provide this param
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(10); // TODO caller should provide this param

        let state = codec_sv2::State::new();

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

                        if let Ok(x) = decoder.next_frame(&mut connection.state) {
                            if sender_incoming.send(x).await.is_err() {
                                task::yield_now().await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            "Disconnected from client while reading : {} - {}",
                            e, &address
                        );

                        //kill thread without a panic - don't need to panic everytime a client disconnects
                        sender_incoming.close();
                        task::yield_now().await;
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

                        let b = encoder.encode(frame, &mut connection.state).unwrap();

                        // Handshake state means this is the last message so after
                        //this is encoded we can move to transport mode!
                        if connection.state.is_in_handshake() {
                            connection.state =
                                connection.state.take().into_transport_mode().unwrap();
                        }
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
            }
        });

        // DO THE NOISE HANDSHAKE
        match role {
            HandshakeRole::Initiator(_) => {
                debug!("Initializing as downstream for - {}", &address);
                Self::initialize_as_downstream(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    receiver_incoming.clone(),
                )
                .await
            }
            HandshakeRole::Responder(_) => {
                debug!("Initializing as upstream for - {}", &address);
                Self::initialize_as_upstream(
                    connection.clone(),
                    role,
                    sender_outgoing.clone(),
                    receiver_incoming.clone(),
                )
                .await
            }
        };
        debug!("Noise handshake complete - {}", &address);
        (receiver_incoming, sender_outgoing)
    }

    async fn set_state(self_: Arc<Mutex<Self>>, state: codec_sv2::State) {
        loop {
            if let Ok(mut connection) = self_.try_lock() {
                connection.state = state;
                break;
            };
        }
    }

    async fn initialize_as_downstream<'a, Message: Serialize + Deserialize<'a> + GetSize>(
        self_: Arc<Mutex<Self>>,
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) {
        let mut state = codec_sv2::State::initialize(role);

        let first_message = state.step(None).unwrap();
        sender_outgoing.send(first_message.into()).await.unwrap();

        let second_message = receiver_incoming.recv().await.unwrap();
        let mut second_message: HandShakeFrame = second_message.try_into().unwrap();
        let second_message = second_message.payload().to_vec();

        let third_message = state.step(Some(second_message)).unwrap();
        sender_outgoing.send(third_message.into()).await.unwrap();

        let fourth_message = receiver_incoming.recv().await.unwrap();
        let mut fourth_message: HandShakeFrame = fourth_message.try_into().unwrap();
        let fourth_message = fourth_message.payload().to_vec();

        state
            .step(Some(fourth_message))
            .expect("Error on fourth message step");

        Self::set_state(self_, state.into_transport_mode().unwrap()).await;
    }

    async fn initialize_as_upstream<'a, Message: Serialize + Deserialize<'a> + GetSize>(
        self_: Arc<Mutex<Self>>,
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) {
        let mut state = codec_sv2::State::initialize(role);

        let mut first_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let first_message = first_message.payload().to_vec();

        let second_message = state.step(Some(first_message)).unwrap();

        sender_outgoing.send(second_message.into()).await.unwrap();

        let mut third_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let third_message_vec = third_message.payload().to_vec();

        let fourth_message = state.step(Some(third_message_vec)).unwrap();

        // This sets the state to Handshake state - this prompts the task above to move the state
        // to transport mode so that the next incoming message will be decoded correctly
        // It is important to do this directly before sending the fourth message
        Self::set_state(self_, state).await;
        sender_outgoing.send(fourth_message.into()).await.unwrap();
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
    loop {
        if let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                &authority_public_key[..],
                &authority_private_key[..],
                cert_validity,
            )
            .unwrap();
            let role = HandshakeRole::Responder(responder);
            let _ = sender.send((stream, role)).await;
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
