use async_channel::{bounded, Receiver, Sender};
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use core::convert::TryInto;
use serde::{Deserialize, Serialize};

use codec_sv2::{Frame, HandShakeFrame, HandshakeRole, StandardEitherFrame, StandardNoiseDecoder};
use serde_sv2::GetLen;

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

impl Connection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetLen + Send + 'static>(
        stream: TcpStream,
        role: HandshakeRole,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        let (mut reader, writer) = (stream.clone(), stream.clone());

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

                let _r = reader.read_exact(writable).await.unwrap();

                loop {
                    if let Some(mut connection) = cloned1.try_lock() {
                        match decoder.next_frame(&mut connection.state) {
                            Ok(x) => {
                                sender_incoming.send(x).await.unwrap();
                                break;
                            }
                            Err(_) => break,
                        }
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

                if let Ok(frame) = received {
                    loop {
                        if let Some(mut connection) = cloned2.try_lock() {
                            let b = encoder.encode(frame, &mut connection.state).unwrap();
                            (&writer).write_all(b).await.unwrap();
                            break;
                        }
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

    async fn initialize_as_downstream<'a, Message: Serialize + Deserialize<'a> + GetLen>(
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) -> codec_sv2::State {
        let mut state = codec_sv2::State::initialize(role);

        let first_message = state.step(None).unwrap();
        sender_outgoing.send(first_message.into()).await.unwrap();

        let mut second_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let second_message = second_message.payload().to_vec();

        state.step(Some(second_message)).unwrap();

        state.into_transport_mode().unwrap()
    }

    async fn initialize_as_upstream<'a, Message: Serialize + Deserialize<'a> + GetLen>(
        role: HandshakeRole,
        sender_outgoing: Sender<StandardEitherFrame<Message>>,
        sender_incoming: Receiver<StandardEitherFrame<Message>>,
        receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    ) -> codec_sv2::State {
        let mut state = codec_sv2::State::initialize(role);

        let mut first_message: HandShakeFrame =
            receiver_incoming.recv().await.unwrap().try_into().unwrap();
        let first_message = first_message.payload().to_vec();

        let second_message = state.step(Some(first_message)).unwrap();

        sender_outgoing.send(second_message.into()).await.unwrap();

        // CHECK IF SECOND_MESSAGE HAS BEEN SENT
        loop {
            if sender_incoming.is_empty() {
                break;
            }
        }

        state.into_transport_mode().unwrap()
    }
}
