//use bytes::BytesMut;
use alloc::vec::Vec;
use snow::HandshakeState;

use crate::error::Result;

/// Handshake message
pub type Message = Vec<u8>;

/// Describes the step result what the relevant party should do after sending out the
/// provided message (if any)
#[derive(Debug, PartialEq, Eq)]
pub enum StepResult {
    /// variant and expect a reply
    ExpectReply(Message),
    /// This message is yet to be sent to the counter party and we are allowed to switch to
    /// transport mode
    NoMoreReply(Message),
    /// The handshake is complete, no more messages are expected and nothing is to be sent. The
    /// protocol can be switched to transport mode now.
    Done,
}

impl StepResult {
    pub fn inner(self) -> Vec<u8> {
        match self {
            Self::ExpectReply(m) => m,
            Self::NoMoreReply(m) => m,
            Self::Done => Vec::new(),
        }
    }
}

/// Objects that can perform 1 handshake step implement this trait
pub trait Step {
    /// Proceeds with the handshake and processes an optional incoming message - `in_msg` and
    /// generates a new handshake message to be sent out
    ///
    /// `in_msg` - optional input message to be processed
    /// this buffer and returned as appropriate `StepResult`
    fn step(&mut self, in_msg: Option<Message>) -> Result<StepResult>;

    /// Transforms step into the handshake state
    fn into_handshake_state(self) -> HandshakeState;
}
