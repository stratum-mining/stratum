use async_channel::{bounded, Receiver, Sender};
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;
use std::time::Duration;
use tracing::{debug, error};

use binary_sv2::GetSize;
use codec_sv2::{
    Frame, HandShakeFrame, HandshakeRole, Initiator, Responder, StandardEitherFrame,
    StandardNoiseDecoder,
};

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

impl Connection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
        role: HandshakeRole,
        capacity: usize,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        let (mut reader, writer) = (stream.clone(), stream.clone());

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);

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
                            sender_incoming.send(x).await.unwrap();
                        } else {
                            error!("Failed to decode noise frame!");
                        }
                    }
                    Err(e) => {
                        println!("{:#?}", e);
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
                let received = receiver_outgoing.recv().await;
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

                        let b = b.as_ref();

                        match (&writer).write_all(b).await {
                            Ok(_) => (),
                            Err(e) => {
                                println!("UUUU {:#?}", e);
                                let _ = writer.shutdown(async_std::net::Shutdown::Both);
                            }
                        }
                    }
                    Err(e) => {
                        println!("EEEE {:#?}", e);
                        let _ = writer.shutdown(async_std::net::Shutdown::Both);
                        break;
                    }
                };
            }
        });

        // DO THE NOISE HANDSHAKE
        let transport_mode = match role {
            HandshakeRole::Initiator(_) => {
                Self::initialize_as_downstream(
                    role,
                    sender_outgoing.clone(),
                    receiver_incoming.clone(),
                )
                .await
            }
            HandshakeRole::Responder(_) => {
                Self::initialize_as_upstream(
                    role,
                    sender_outgoing.clone(),
                    receiver_outgoing_cloned,
                    receiver_incoming.clone(),
                )
                .await
            }
        };
        Self::set_state(connection.clone(), transport_mode).await;
        (receiver_incoming, sender_outgoing)
    }

    async fn set_state(self_: Arc<Mutex<Self>>, state: codec_sv2::State) {
        loop {
            if let Some(mut connection) = self_.try_lock() {
                connection.state = state;
                break;
            };
        }
    }

    async fn initialize_as_downstream<'a, Message: Serialize + Deserialize<'a> + GetSize>(
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) -> codec_sv2::State {
        let mut state = codec_sv2::State::initialize(role);
        debug!("Initialized downstream noise handshake");
        let first_message = state.step(None).unwrap();
        sender_outgoing.send(first_message.into()).await.unwrap();
        debug!("Sent first message to upstream");

        let second_message = match receiver_incoming.recv().await {
            Ok(x) => x,
            Err(e) => {
                error!("Error receiving second message: {:#?}", e);
                return state;
            }
        };
        debug!("Received second message from upstream");
        let mut second_message: HandShakeFrame = second_message.try_into().unwrap();
        let second_message = second_message.payload().to_vec();

        let thirth_message = state.step(Some(second_message)).unwrap();
        sender_outgoing.send(thirth_message.into()).await.unwrap();

        debug!("Sent third message to upstream");
        let fourth_message = receiver_incoming.recv().await.unwrap();
        let mut fourth_message: HandShakeFrame = fourth_message.try_into().unwrap();
        let fourth_message = fourth_message.payload().to_vec();
        debug!("Received fourth message from upstream");
        state.step(Some(fourth_message)).unwrap();

        state.into_transport_mode().unwrap()
    }

    async fn initialize_as_upstream<'a, Message: Serialize + Deserialize<'a> + GetSize>(
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        sender_incoming: Receiver<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) -> codec_sv2::State {
        let mut state = codec_sv2::State::initialize(role);
        debug!("Noise handshake started");
        let mut first_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let first_message = first_message.payload().to_vec();

        let second_message = state.step(Some(first_message)).unwrap();

        sender_outgoing.send(second_message.into()).await.unwrap();

        let mut thirth_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let thirth_message = thirth_message.payload().to_vec();

        let fourth_message = state.step(Some(thirth_message)).unwrap();
        sender_outgoing.send(fourth_message.into()).await.unwrap();
        debug!("Noise handshake finished");

        // CHECK IF FOURTH MESSAGE HAS BEEN SENT
        loop {
            task::sleep(std::time::Duration::from_millis(1)).await;
            if sender_incoming.is_empty() {
                break;
            }
        }

        state.into_transport_mode().unwrap()
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
            &authority_public_key[..],
            &authority_private_key[..],
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
