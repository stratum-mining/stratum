#![allow(clippy::new_ret_no_self)]
use crate::Error;
use async_channel::{unbounded, Receiver, Sender};
use codec_sv2::{
    binary_sv2::{Deserialize, GetSize, Serialize},
    noise_sv2::{ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE},
    HandShakeFrame, HandshakeRole, StandardEitherFrame, StandardNoiseDecoder, State,
};
use futures::future::poll_fn;
use pin_project::pin_project;
use std::{
    convert::TryInto,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    task,
};
use tracing::{debug, error, info};

#[pin_project]
pub struct MessageReader<R, M: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    reader: R,
    state: State,
    decoder: StandardNoiseDecoder<M>,
    read_pos: usize,
    buffer: Vec<u8>,
}

impl<R, M> MessageReader<R, M>
where
    R: AsyncRead + Unpin,
    M: Serialize + Deserialize<'static> + GetSize + Send + 'static,
{
    pub fn new(reader: R, state: State, decoder: StandardNoiseDecoder<M>) -> Self {
        Self {
            reader,
            state,
            decoder,
            read_pos: 0,
            buffer: vec![],
        }
    }

    /// Cancellation-safe **only if** the same `MessageReader` instance is
    /// polled repeatedly across wakeups (e.g., using `poll_fn`).
    ///
    /// Partial reads are tracked via internal state (`read_pos`) and will
    /// resume correctly on the next poll. `decoder.next_frame` is only
    /// called once a full frame (as determined by the protocol) has been received.
    ///
    /// ⚠️ Not cancellation-safe if this method is wrapped in a new future
    /// instance (e.g., `.await` on a short-lived wrapper), as internal
    /// state would be lost.
    pub fn poll_next_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StandardEitherFrame<M>, Error>> {
        let mut this = self.project();
        let buf = this.buffer;
        let expected_len = this.decoder.writable_len();
        buf.resize(expected_len, 0);

        while *this.read_pos < expected_len {
            let mut read_buf = ReadBuf::new(&mut buf[*this.read_pos..]);
            let reader = Pin::new(&mut this.reader);

            match reader.poll_read(cx, &mut read_buf) {
                Poll::Ready(Ok(())) => {
                    let n = read_buf.filled().len();
                    if n == 0 {
                        return Poll::Ready(Err(Error::SocketClosed));
                    }
                    *this.read_pos += n;
                }
                Poll::Ready(Err(_)) => return Poll::Ready(Err(Error::SocketClosed)),
                Poll::Pending => return Poll::Pending,
            }
        }

        this.decoder.writable().copy_from_slice(buf);
        *this.read_pos = 0;

        match this.decoder.next_frame(this.state) {
            Ok(frame) => Poll::Ready(Ok(frame)),
            Err(e) => Poll::Ready(Err(Error::CodecError(e))),
        }
    }
}

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
    let buf = decoder.writable();
    reader
        .read_exact(buf)
        .await
        .map_err(|_| Error::SocketClosed)?;
    decoder.next_frame(state).map_err(Error::CodecError)
}

impl Connection {
    pub async fn new<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static>(
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

        // error!("Handshake completed with state: {:?}", state);

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
        Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
    >(
        reader: OwnedReadHalf,
        mut writer: OwnedWriteHalf,
        state: State,
        address: std::net::SocketAddr,
        conn_state: ConnectionState<Message>,
    ) -> task::JoinHandle<()> {
        let sender_incoming = conn_state.sender_incoming.clone();
        let receiver_outgoing = conn_state.receiver_outgoing.clone();

        task::spawn(async move {
            let decoder = StandardNoiseDecoder::<Message>::new();
            let mut encoder = codec_sv2::NoiseEncoder::<Message>::new();
            let read_state = state.clone();
            let mut write_state = state;

            let mut reader_fut = MessageReader::new(reader, read_state, decoder);

            loop {
                tokio::select! {
                    biased;
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
                    _ = tokio::signal::ctrl_c() => {
                        info!("Ctrl+C received for {}, shutting down connection", address);
                        break;
                    }
                    // Incoming message from network
                    res = poll_fn(|cx| Pin::new(&mut reader_fut).poll_next_frame(cx)) => {
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
                }
            }

            let _ = writer.shutdown().await;
            conn_state.close_all();
            info!("Connection to {} shut down cleanly", address);
        })
    }
}
