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
use tracing::{debug, error, info};

pub struct Connection;

struct ConnectionState<Message> {
    sender_incoming: Sender<StandardEitherFrame<Message>>,
    receiver_incoming: Receiver<StandardEitherFrame<Message>>,
    sender_outgoing: Sender<StandardEitherFrame<Message>>,
    receiver_outgoing: Receiver<StandardEitherFrame<Message>>,
}

impl<Message> ConnectionState<Message> {
    fn close_all(&self) {
        debug!("Closing all channels");
        self.sender_incoming.close();
        self.receiver_incoming.close();
        self.sender_outgoing.close();
        self.receiver_outgoing.close();
    }
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
    let mut buf = vec![0u8; decoder.writable_len()];
    debug!("Bytes to read: {}", buf.len());

    reader
        .read_exact(&mut buf)
        .await
        .map_err(|_| Error::SocketClosed)?;

    debug!("Read complete, copying into decoder buffer");
    decoder.writable().copy_from_slice(&buf);
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

        error!("Handshake completed with state: {:?}", state);

        let (sender_incoming, receiver_incoming) = unbounded();
        let (sender_outgoing, receiver_outgoing) = unbounded();

        let conn_state = ConnectionState {
            sender_incoming,
            receiver_incoming: receiver_incoming.clone(),
            sender_outgoing: sender_outgoing.clone(),
            receiver_outgoing,
        };
        Self::spawn_connection_loop(reader, writer, state, address, conn_state);

        Ok((receiver_incoming, sender_outgoing))
    }

    fn spawn_connection_loop<
        'a,
        Message: Serialize + Deserialize<'a> + GetSize + Send + 'static,
    >(
        mut reader: OwnedReadHalf,
        mut writer: OwnedWriteHalf,
        state: State,
        address: std::net::SocketAddr,
        conn_state: ConnectionState<Message>,
    ) -> task::JoinHandle<()> {
        let sender_incoming = conn_state.sender_incoming.clone();
        let receiver_outgoing = conn_state.receiver_outgoing.clone();

        task::spawn(async move {
            let mut decoder = StandardNoiseDecoder::<Message>::new();
            let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();
            let mut read_state = state.clone();
            let mut write_state = state;

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        info!("Ctrl+C received for {}, shutting down connection", address);
                        break;
                    }
                    // Incoming message from network
                    res = receive_message(&mut reader, &mut read_state, &mut decoder) => {
                        match res {
                            Ok(frame) => {
                                if sender_incoming.send(frame).await.is_err() {
                                    debug!("Receiver dropped for {}", address);
                                    break;
                                }
                            }
                            Err(Error::CodecError(codec_sv2::Error::MissingBytes(_))) => {
                                debug!("Waiting for more stream bytes...");
                            }
                            Err(e) => {
                                error!("Read error for {}: {:?}, shutting down", address, e);
                                break;
                            }
                        }
                    }

                    // Outgoing message from channel
                    res = receiver_outgoing.recv() => {
                        match res {
                            Ok(frame) => {
                                if let Err(e) = send_message(&mut writer, frame, &mut write_state, &mut encoder).await {
                                    error!("Write error for {address}: {e:?}, shutting down");
                                    break;
                                }
                            }
                            Err(_) => {
                                debug!("Sender closed for {address}");
                                break;
                            }
                        }
                    }
                }
            }

            let _ = writer.shutdown().await;
            drop(reader);
            conn_state.close_all();
            info!("Connection to {} shut down cleanly", address);
        })
    }
}
