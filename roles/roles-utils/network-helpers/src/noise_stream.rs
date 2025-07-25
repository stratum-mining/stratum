//! A Noise-encrypted wrapper around a `TcpStream`, providing framed read/write I/O using the SV2
//! protocol and a stateful Noise handshake.
//!
//! This module provides `NoiseTcpStream`, which wraps a `TcpStream` and performs a Noise-based
//! authenticated key exchange based on the provided [`HandshakeRole`].
//!
//! After a successful handshake, the stream can be split into a `NoiseTcpReadHalf` and
//! `NoiseTcpWriteHalf`, which support frame-based encoding/decoding of SV2 messages with optional
//! non-blocking behavior.

use crate::Error;
use codec_sv2::{
    binary_sv2::{Deserialize, GetSize, Serialize},
    noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE,
    HandshakeRole, NoiseEncoder, StandardNoiseDecoder, State,
};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};

use codec_sv2::{noise_sv2::ELLSWIFT_ENCODING_SIZE, HandShakeFrame, StandardEitherFrame};
use std::convert::TryInto;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error};

/// A Noise-secured duplex stream over TCP that wraps a `TcpStream`
/// and provides secure read/write capabilities using the Noise protocol.
///
/// This stream performs the full Noise handshake during construction
/// and returns a bidirectional encrypted stream split into read and write halves.
///
/// **Note:** This struct is **not cancellation-safe**.
/// If `read_frame()` or `write_frame()` is canceled mid-way,
/// internal state may be left in an inconsistent state, which can lead to
/// protocol errors or dropped frames.
pub struct NoiseTcpStream<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    reader: NoiseTcpReadHalf<Message>,
    writer: NoiseTcpWriteHalf<Message>,
}

/// The reading half of a `NoiseTcpStream`.
///
/// It buffers incoming encrypted bytes, attempts to decode full Noise frames,
/// and exposes a method to retrieve structured messages of type `Message`.
pub struct NoiseTcpReadHalf<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    reader: OwnedReadHalf,
    decoder: StandardNoiseDecoder<Message>,
    state: State,
    current_frame_buf: Vec<u8>,
    bytes_read: usize,
}

/// The writing half of a `NoiseTcpStream`.
///
/// It accepts structured messages, encodes them via the Noise protocol,
/// and writes the result to the socket.
pub struct NoiseTcpWriteHalf<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    writer: OwnedWriteHalf,
    encoder: NoiseEncoder<Message>,
    state: State,
}

impl<Message> NoiseTcpStream<Message>
where
    Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
{
    /// Constructs a new `NoiseTcpStream` over the given TCP stream,
    /// performing the Noise handshake in the given `role`.
    ///
    /// On success, returns a stream with encrypted communication channels.
    pub async fn new(stream: TcpStream, role: HandshakeRole) -> Result<Self, Error> {
        let (mut reader, mut writer) = stream.into_split();

        let mut decoder = StandardNoiseDecoder::<Message>::new();
        let mut encoder = NoiseEncoder::<Message>::new();
        let mut state = State::initialized(role.clone());

        match role {
            HandshakeRole::Initiator(_) => {
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
        Ok(Self {
            reader: NoiseTcpReadHalf {
                reader,
                decoder,
                state: state.clone(),
                current_frame_buf: vec![],
                bytes_read: 0,
            },
            writer: NoiseTcpWriteHalf {
                writer,
                encoder,
                state,
            },
        })
    }

    /// Consumes the stream and returns its reader and writer halves.
    pub fn into_split(self) -> (NoiseTcpReadHalf<Message>, NoiseTcpWriteHalf<Message>) {
        (self.reader, self.writer)
    }
}

impl<Message> NoiseTcpWriteHalf<Message>
where
    Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
{
    /// Encrypts and writes a full message frame to the socket.
    ///
    /// Returns an error if the socket is closed or the message cannot be encoded.
    ///
    /// Not cancellation-safe: A canceled write may cause partial writes or state corruption.
    pub async fn write_frame(&mut self, frame: StandardEitherFrame<Message>) -> Result<(), Error> {
        let buf = self.encoder.encode(frame, &mut self.state)?;
        self.writer
            .write_all(buf.as_ref())
            .await
            .map_err(|_| Error::SocketClosed)?;
        Ok(())
    }

    /// Attempts to write a message without blocking.
    ///
    /// Returns:
    /// - `Ok(true)` if the entire frame was written successfully.
    /// - `Ok(false)` if the socket is not ready (would block).
    /// - `Err(_)` on socket or encoding errors.
    pub fn try_write_frame(&mut self, frame: StandardEitherFrame<Message>) -> Result<bool, Error> {
        let buf = self.encoder.encode(frame, &mut self.state)?;

        match self.writer.try_write(buf.as_ref()) {
            Ok(n) if n == buf.len() => Ok(true),
            Ok(_) => Err(Error::SocketClosed),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(_) => Err(Error::SocketClosed),
        }
    }

    /// Gracefully shuts down the writing half of the stream.
    ///
    /// Returns an error if the shutdown fails.
    pub async fn shutdown(&mut self) -> Result<(), Error> {
        self.writer
            .shutdown()
            .await
            .map_err(|_| Error::SocketClosed)
    }
}

impl<Message> NoiseTcpReadHalf<Message>
where
    Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
{
    /// Reads and decodes a complete frame from the socket.
    ///
    /// This method blocks until a full frame is read and decoded,
    /// handling `MissingBytes` errors from the codec automatically.
    ///
    /// Not cancellation-safe: Cancellation may leave partially-read state behind.
    pub async fn read_frame(&mut self) -> Result<StandardEitherFrame<Message>, Error> {
        loop {
            let expected = self.decoder.writable_len();

            if self.current_frame_buf.len() != expected {
                self.current_frame_buf.resize(expected, 0);
                self.bytes_read = 0;
            }

            while self.bytes_read < expected {
                let n = self
                    .reader
                    .read(&mut self.current_frame_buf[self.bytes_read..])
                    .await
                    .map_err(|_| Error::SocketClosed)?;

                if n == 0 {
                    return Err(Error::SocketClosed);
                }

                self.bytes_read += n;
            }

            self.decoder
                .writable()
                .copy_from_slice(&self.current_frame_buf[..]);

            self.bytes_read = 0;

            match self.decoder.next_frame(&mut self.state) {
                Ok(frame) => return Ok(frame),
                Err(codec_sv2::Error::MissingBytes(_)) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Err(e) => return Err(Error::CodecError(e)),
            }
        }
    }

    /// Attempts to read and decode a frame without blocking.
    ///
    /// Returns:
    /// - `Ok(Some(frame))` if a full frame is successfully decoded.
    /// - `Ok(None)` if not enough data is available yet.
    /// - `Err(_)` on socket or decoding errors.
    pub fn try_read_frame(&mut self) -> Result<Option<StandardEitherFrame<Message>>, Error> {
        let expected = self.decoder.writable_len();

        if self.current_frame_buf.len() != expected {
            self.current_frame_buf.resize(expected, 0);
            self.bytes_read = 0;
        }

        match self
            .reader
            .try_read(&mut self.current_frame_buf[self.bytes_read..])
        {
            Ok(0) => return Err(Error::SocketClosed),
            Ok(n) => self.bytes_read += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(_) => return Err(Error::SocketClosed),
        }

        if self.bytes_read < expected {
            return Ok(None);
        }

        self.decoder
            .writable()
            .copy_from_slice(&self.current_frame_buf[..]);

        self.bytes_read = 0;

        match self.decoder.next_frame(&mut self.state) {
            Ok(frame) => Ok(Some(frame)),
            Err(codec_sv2::Error::MissingBytes(_)) => Ok(None),
            Err(e) => Err(Error::CodecError(e)),
        }
    }
}

async fn send_message<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static>(
    writer: &mut OwnedWriteHalf,
    msg: StandardEitherFrame<Message>,
    state: &mut State,
    encoder: &mut NoiseEncoder<Message>,
) -> Result<(), Error> {
    let buffer = encoder.encode(msg, state)?;
    writer
        .write_all(buffer.as_ref())
        .await
        .map_err(|_| Error::SocketClosed)?;
    Ok(())
}

async fn receive_message<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static>(
    reader: &mut OwnedReadHalf,
    state: &mut State,
    decoder: &mut StandardNoiseDecoder<Message>,
) -> Result<StandardEitherFrame<Message>, Error> {
    let mut buffer = vec![0u8; decoder.writable_len()];
    reader
        .read_exact(&mut buffer)
        .await
        .map_err(|_| Error::SocketClosed)?;
    decoder.writable().copy_from_slice(&buffer);
    decoder.next_frame(state).map_err(Error::CodecError)
}
