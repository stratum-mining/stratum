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

pub struct NoiseTcpStream<Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static> {
    reader: NoiseTcpReadHalf<Message>,
    writer: NoiseTcpWriteHalf<Message>,
}

pub struct NoiseTcpReadHalf<Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static>
{
    reader: OwnedReadHalf,
    decoder: StandardNoiseDecoder<Message>,
    state: State,
    current_frame_buf: Vec<u8>,
    bytes_read: usize,
}

pub struct NoiseTcpWriteHalf<
    Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static,
> {
    writer: OwnedWriteHalf,
    encoder: NoiseEncoder<Message>,
    state: State,
}

impl<Message> NoiseTcpStream<Message>
where
    Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static,
{
    pub fn into_split(self) -> (NoiseTcpReadHalf<Message>, NoiseTcpWriteHalf<Message>) {
        (self.reader, self.writer)
    }

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
}

impl<Message> NoiseTcpWriteHalf<Message>
where
    Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static,
{
    pub async fn write_frame(&mut self, frame: StandardEitherFrame<Message>) -> Result<(), Error> {
        let buf = self.encoder.encode(frame, &mut self.state)?;
        self.writer
            .write_all(buf.as_ref())
            .await
            .map_err(|_| Error::SocketClosed)?;
        Ok(())
    }
}

impl<Message> NoiseTcpReadHalf<Message>
where
    Message: Serialize + for<'a> Deserialize<'a> + GetSize + Send + 'static,
{
    pub async fn read_frame(&mut self) -> Result<StandardEitherFrame<Message>, Error> {
        let frame_len = self.decoder.writable_len();

        if self.bytes_read == 0 {
            self.current_frame_buf.resize(frame_len, 0);
        }

        while self.bytes_read < frame_len {
            let n = self
                .reader
                .read(&mut self.current_frame_buf[self.bytes_read..frame_len])
                .await
                .map_err(|_| Error::SocketClosed)?;

            if n == 0 {
                return Err(Error::SocketClosed);
            }

            self.bytes_read += n;
        }

        self.decoder
            .writable()
            .copy_from_slice(&self.current_frame_buf);

        self.bytes_read = 0;

        self.decoder
            .next_frame(&mut self.state)
            .map_err(Error::CodecError)
    }
}

async fn send_message<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
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

async fn receive_message<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
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
