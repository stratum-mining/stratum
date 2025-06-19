#![allow(clippy::new_ret_no_self)]
use crate::Error;
use async_channel::{unbounded, Receiver, Sender};
use codec_sv2::{
    binary_sv2::{Deserialize, GetSize, Serialize},
    noise_sv2::{ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE},
    HandShakeFrame, HandshakeRole, StandardEitherFrame, StandardNoiseDecoder, State,
};
use std::convert::TryInto;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    task,
};
use tracing::{debug, error};

#[derive(Debug)]
pub struct Connection {
    pub state: codec_sv2::State,
}

async fn send_message<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
    writer: &mut OwnedWriteHalf,
    msg: StandardEitherFrame<Message>,
    state: &mut State,
    encoder: &mut codec_sv2::NoiseEncoder<Message>,
) -> Result<(), Error> {
    let buffer = encoder.encode(msg, state)?;
    writer
        .write_all(buffer.as_ref())
        .await
        .map_err(|_| Error::SocketClosed)?;
    Ok(())
}

async fn receive_message<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
    reader: &mut OwnedReadHalf,
    state: &mut State,
    decoder: &mut StandardNoiseDecoder<Message>,
) -> Result<StandardEitherFrame<Message>, Error> {
    let writable = decoder.writable();
    reader
        .read_exact(writable)
        .await
        .map_err(|_| Error::SocketClosed)?;
    decoder.next_frame(state).map_err(Error::CodecError)
}

impl Connection {
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
        let mut decoder = StandardNoiseDecoder::<Message>::new();
        let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();
        let mut state = codec_sv2::State::initialized(role.clone());

        // Handshake Phase
        match role {
            HandshakeRole::Initiator(_) => {
                debug!("Initializing as downstream for {}", address);
                let mut responder_state = codec_sv2::State::not_initialized(&role);
                let first_msg = state.step_0()?;
                send_message(&mut writer, first_msg.into(), &mut state, &mut encoder).await?;
                debug!("First handshake message sent");

                loop {
                    match receive_message(&mut reader, &mut responder_state, &mut decoder).await {
                        Ok(second_msg) => {
                            debug!("Second handshake message received");
                            let handshake_frame: HandShakeFrame = second_msg
                                .try_into()
                                .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
                            let payload: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
                                handshake_frame
                                    .get_payload_when_handshaking()
                                    .try_into()
                                    .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
                            let transport_state = state.step_2(payload)?;
                            state = transport_state;
                            break;
                        }
                        Err(Error::CodecError(codec_sv2::Error::MissingBytes(_))) => {
                            debug!("Waiting for more bytes during handshake");
                        }
                        Err(e) => {
                            error!("Handshake failed with upstream: {:?}", e);
                            return Err(e);
                        }
                    }
                }
            }
            HandshakeRole::Responder(_) => {
                debug!("Initializing as upstream for {}", address);
                let mut initiator_state = codec_sv2::State::not_initialized(&role);

                loop {
                    match receive_message(&mut reader, &mut initiator_state, &mut decoder).await {
                        Ok(first_msg) => {
                            debug!("First handshake message received");
                            let handshake_frame: HandShakeFrame = first_msg
                                .try_into()
                                .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
                            let payload: [u8; ELLSWIFT_ENCODING_SIZE] = handshake_frame
                                .get_payload_when_handshaking()
                                .try_into()
                                .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
                            let (second_msg, transport_state) = state.step_1(payload)?;
                            send_message(&mut writer, second_msg.into(), &mut state, &mut encoder)
                                .await?;
                            debug!("Second handshake message sent");
                            state = transport_state;
                            break;
                        }
                        Err(Error::CodecError(codec_sv2::Error::MissingBytes(_))) => {
                            debug!("Waiting for more bytes during handshake");
                        }
                        Err(e) => {
                            error!("Handshake failed with downstream: {:?}", e);
                            return Err(e);
                        }
                    }
                }
            }
        };

        debug!("Handshake completed with state: {:?}", state);

        let (sender_incoming, receiver_incoming) = unbounded();
        let (sender_outgoing, receiver_outgoing) = unbounded();

        // Spawn Reader
        let read_state = state.clone();
        Self::spawn_reader(reader, read_state, address, sender_incoming.clone());

        // Spawn Writer
        let write_state = state;
        Self::spawn_writer(writer, write_state, address, receiver_outgoing.clone());

        Ok((receiver_incoming, sender_outgoing))
    }

    fn spawn_reader<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        mut reader: OwnedReadHalf,
        mut reader_state: State,
        address: std::net::SocketAddr,
        sender_incoming: Sender<StandardEitherFrame<Message>>,
    ) -> task::JoinHandle<()> {
        task::spawn(async move {
            let mut decoder = StandardNoiseDecoder::<Message>::new();
            loop {
                match receive_message(&mut reader, &mut reader_state, &mut decoder).await {
                    Ok(frame) => {
                        if sender_incoming.send(frame).await.is_err() {
                            error!("Shutting down reader for {}", address);
                            break;
                        }
                    }
                    Err(Error::CodecError(codec_sv2::Error::MissingBytes(_))) => {
                        debug!("Waiting for more bytes while reading stream");
                    }
                    Err(e) => {
                        error!("Reader shutting down due to error: {:?}", e);
                        sender_incoming.close();
                        break;
                    }
                }
            }
        })
    }

    fn spawn_writer<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        mut writer: OwnedWriteHalf,
        mut write_state: State,
        address: std::net::SocketAddr,
        receiver_outgoing: Receiver<StandardEitherFrame<Message>>,
    ) -> task::JoinHandle<()> {
        task::spawn(async move {
            let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();
            while let Ok(frame) = receiver_outgoing.recv().await {
                if let Err(e) =
                    send_message(&mut writer, frame, &mut write_state, &mut encoder).await
                {
                    error!("Error while writing to client {}: {:?}", address, e);
                    let _ = writer.shutdown().await;
                    break;
                }
            }
            let _ = writer.shutdown().await;
        })
    }
}
