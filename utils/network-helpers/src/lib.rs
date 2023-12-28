#[cfg(feature = "async_std")]
mod noise_connection_async_std;
#[cfg(feature = "async_std")]
mod plain_connection_async_std;
use binary_sv2::{Deserialize, GetSize, Serialize};
#[cfg(feature = "async_std")]
pub use noise_connection_async_std::{connect, listen, Connection};
#[cfg(feature = "async_std")]
pub use plain_connection_async_std::{plain_connect, plain_listen, PlainConnection};

#[cfg(feature = "tokio")]
pub mod noise_connection_tokio;
#[cfg(feature = "tokio")]
pub mod plain_connection_tokio;

use async_channel::{Receiver, RecvError, SendError, Sender};
use async_trait::async_trait;
use codec_sv2::{Error as CodecError, HandShakeFrame, HandshakeRole, StandardEitherFrame};
use futures::lock::Mutex;
use std::{
    convert::TryInto,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug)]
pub enum Error {
    HandshakeRemoteInvalidMessage,
    CodecError(CodecError),
    RecvError,
    SendError,
}

impl From<CodecError> for Error {
    fn from(e: CodecError) -> Self {
        Error::CodecError(e)
    }
}
impl From<RecvError> for Error {
    fn from(_: RecvError) -> Self {
        Error::RecvError
    }
}
impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Error::SendError
    }
}

#[async_trait]
trait SetState {
    async fn set_state(self_: Arc<Mutex<Self>>, state: codec_sv2::State);
}

async fn initialize_as_downstream<
    'a,
    Message: Serialize + Deserialize<'a> + GetSize,
    T: SetState,
>(
    self_: Arc<Mutex<T>>,
    role: HandshakeRole,
    sender_outgoing: Sender<StandardEitherFrame<Message>>,
    receiver_incoming: Receiver<StandardEitherFrame<Message>>,
) -> Result<(), Error> {
    let mut state = codec_sv2::State::initialized(role);

    // Create and send first handshake message
    let first_message = state.step_0()?;
    sender_outgoing.send(first_message.into()).await?;

    // Receive and deserialize second handshake message
    let second_message = receiver_incoming.recv().await?;
    let second_message: HandShakeFrame = second_message
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
    let second_message: [u8; 170] = second_message
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

async fn initialize_as_upstream<'a, Message: Serialize + Deserialize<'a> + GetSize, T: SetState>(
    self_: Arc<Mutex<T>>,
    role: HandshakeRole,
    sender_outgoing: Sender<StandardEitherFrame<Message>>,
    receiver_incoming: Receiver<StandardEitherFrame<Message>>,
) -> Result<(), Error> {
    let mut state = codec_sv2::State::initialized(role);

    // Receive and deserialize first handshake message
    let first_message: HandShakeFrame = receiver_incoming
        .recv()
        .await?
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;
    let first_message: [u8; 32] = first_message
        .get_payload_when_handshaking()
        .try_into()
        .map_err(|_| Error::HandshakeRemoteInvalidMessage)?;

    // Create and send second handshake message
    let (second_message, transport_mode) = state.step_1(first_message)?;
    HANDSHAKE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
    sender_outgoing.send(second_message.into()).await?;

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
