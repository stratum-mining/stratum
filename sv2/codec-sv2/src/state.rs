//! # Noise Handshake State
//!
//! One type per phase of the Noise protocol, so that the transitions between them are checked at
//! compile time:
//!
//! ```text
//! Handshake<Initiator>     --step_0()-------> (HandshakeMessage, Handshake<InitiatorSent>)
//! Handshake<InitiatorSent> --step_2(msg)----> Transport
//! Handshake<Responder>     --step_1(re_pub)-> (HandshakeMessage, Transport)
//! Transport                --split()--------> (TransportEncryptState, TransportDecryptState)
//! ```
//!
//! Each role only has the steps it may take, and every step consumes the state it leaves. Each
//! one mixes into the handshake transcript, so taking one twice would corrupt it; the types make
//! that impossible rather than leaving it to fail later as a decryption error.
//!
//! The two states that are waiting on their counterpart, `Handshake<InitiatorSent>` and
//! `Handshake<Responder>`, are the ones a decoder can read for: that is what
//! [`ExpectsHandshakeMessage`] names. A `Handshake<Initiator>` has sent nothing, so nothing is
//! coming back to it.

use crate::Result;
use alloc::boxed::Box;
use buffer_sv2::Buffer as IsBuffer;
use framing_sv2::framing::HandshakeMessage;
use noise_sv2::{
    AeadBuffer, AeadError, Initiator, NoiseDecryptor, NoiseEncryptor, NoiseEngine, Responder,
    ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE,
};

/// A handshake role that is waiting on a message from its counterpart.
///
/// An [`Initiator`] that has not sent its first message yet is deliberately not one: nothing is
/// coming back until it does, so a decoder cannot be asked to read for it.
pub trait ExpectsHandshakeMessage {
    /// Size of the handshake message this role is waiting for.
    const EXPECTED_MESSAGE_SIZE: usize;
}

impl ExpectsHandshakeMessage for InitiatorSent {
    const EXPECTED_MESSAGE_SIZE: usize = INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE;
}

impl ExpectsHandshakeMessage for Responder {
    const EXPECTED_MESSAGE_SIZE: usize = ELLSWIFT_ENCODING_SIZE;
}

/// An [`Initiator`] that has sent its first handshake message, waiting for the responder's reply.
///
/// [`Handshake::step_0`] is the only thing that builds one, which is what stops a caller from
/// sending that first message twice.
#[derive(Debug)]
pub struct InitiatorSent(Initiator);

/// The codec state while the handshake runs, in the role `R`: [`Initiator`] before it has sent
/// its first message, [`InitiatorSent`] after, or [`Responder`]. Frames exchanged in this state
/// are not encrypted yet.
///
/// A step belonging to the other role does not exist:
///
/// ```compile_fail,E0599
/// use codec_sv2::Handshake;
/// use noise_sv2::Responder;
///
/// fn responder() -> Handshake<Responder> { unimplemented!() }
///
/// responder().step_0().unwrap();
/// ```
///
/// Neither does replaying a handshake that is over:
///
/// ```compile_fail,E0382
/// use codec_sv2::Handshake;
/// use noise_sv2::{Responder, ELLSWIFT_ENCODING_SIZE};
///
/// fn responder() -> Handshake<Responder> { unimplemented!() }
/// fn rng() -> rand::rngs::ThreadRng { unimplemented!() }
///
/// let responder = responder();
/// let (_first, _transport) = responder
///     .step_1_with_now_rng([0; ELLSWIFT_ENCODING_SIZE], 0, &mut rng())
///     .unwrap();
/// let _second = responder.step_1_with_now_rng([0; ELLSWIFT_ENCODING_SIZE], 0, &mut rng());
/// ```
///
/// Nor does sending the first message twice, which would mix it into the transcript twice and
/// leave `step_2` to fail as a decryption error:
///
/// ```compile_fail,E0382
/// use codec_sv2::Handshake;
/// use noise_sv2::Initiator;
///
/// fn initiator() -> Handshake<Initiator> { unimplemented!() }
///
/// let initiator = initiator();
/// let (_first, _sent) = initiator.step_0().unwrap();
/// let _again = initiator.step_0();
/// ```
///
/// Nor does taking the responder's reply before that first message has been sent:
///
/// ```compile_fail,E0599
/// use codec_sv2::Handshake;
/// use noise_sv2::{Initiator, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE};
///
/// fn initiator() -> Handshake<Initiator> { unimplemented!() }
///
/// let _transport =
///     initiator().step_2_with_now([0; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE], 0);
/// ```
#[derive(Debug)]
pub struct Handshake<R> {
    role: Box<R>,
}

impl Handshake<Initiator> {
    /// Starts a handshake as the initiator.
    pub fn initiator(role: Box<Initiator>) -> Self {
        Self { role }
    }

    /// Creates the initial handshake message, consuming the state.
    ///
    /// The returned [`Handshake<InitiatorSent>`] is what takes the responder's reply, in
    /// [`Handshake::step_2`]. Sending is the caller's job: if it fails, the whole handshake
    /// starts over from a fresh [`Initiator`], because this step has already mixed the message
    /// into the transcript and repeating it would leave the two sides unable to agree.
    pub fn step_0(mut self) -> Result<(HandshakeMessage, Handshake<InitiatorSent>)> {
        let message = self.role.step_0()?;
        Ok((
            HandshakeMessage::from_message(message),
            Handshake {
                role: Box::new(InitiatorSent(*self.role)),
            },
        ))
    }
}

impl Handshake<InitiatorSent> {
    /// Completes the handshake with the responder's reply, consuming the state.
    #[cfg(feature = "std")]
    pub fn step_2(
        mut self,
        message: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
    ) -> Result<Transport> {
        self.role
            .0
            .step_2(message)
            .map_err(Into::into)
            .map(Transport::new)
    }

    /// [`Self::step_2`] given the current system time, for `no_std` environments that have
    /// another source of time.
    #[inline]
    pub fn step_2_with_now(
        mut self,
        message: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
        now: u32,
    ) -> Result<Transport> {
        self.role
            .0
            .step_2_with_now(message, now)
            .map_err(Into::into)
            .map(Transport::new)
    }
}

impl Handshake<Responder> {
    /// Starts a handshake as the responder.
    pub fn responder(role: Box<Responder>) -> Self {
        Self { role }
    }

    /// Answers the initiator's public key, completing the handshake on this side and consuming
    /// the state. The returned frame is what the initiator needs for its [`Handshake::step_2`].
    #[cfg(feature = "std")]
    pub fn step_1(
        mut self,
        re_pub: [u8; ELLSWIFT_ENCODING_SIZE],
    ) -> Result<(HandshakeMessage, Transport)> {
        let (message, engine) = self.role.step_1(re_pub)?;
        Ok((
            HandshakeMessage::from_message(message),
            Transport::new(engine),
        ))
    }

    /// [`Self::step_1`] given the current time and a random number generator, for `no_std`
    /// environments that have their own of each.
    #[inline]
    pub fn step_1_with_now_rng<G: rand::Rng + rand::CryptoRng>(
        mut self,
        re_pub: [u8; ELLSWIFT_ENCODING_SIZE],
        now: u32,
        rng: &mut G,
    ) -> Result<(HandshakeMessage, Transport)> {
        let (message, engine) = self.role.step_1_with_now_rng(re_pub, now, rng)?;
        Ok((
            HandshakeMessage::from_message(message),
            Transport::new(engine),
        ))
    }
}

/// The codec state once the handshake is complete, where AEAD encryption and decryption are
/// operational.
///
/// Completing a [`Handshake`] is the only way to build one, which is what makes
/// [`Transport::split`] infallible:
///
/// ```compile_fail,E0451
/// use codec_sv2::Transport;
/// use noise_sv2::NoiseEngine;
///
/// fn engine() -> NoiseEngine { unimplemented!() }
///
/// let transport = Transport { engine: engine() };
/// ```
#[derive(Debug)]
pub struct Transport {
    engine: NoiseEngine,
}

impl Transport {
    fn new(engine: NoiseEngine) -> Self {
        Self { engine }
    }

    /// Splits into the half the encoder uses and the half the decoder uses, consuming it.
    pub fn split(self) -> (TransportEncryptState, TransportDecryptState) {
        let (encryption, decryption) = self.engine.into_split();
        (
            TransportEncryptState { encryption },
            TransportDecryptState { decryption },
        )
    }
}

// The committed bytes of `buffer` from `start` on: the one chunk AEAD encrypts or decrypts in
// place, growing it by the tag or shrinking it by the tag.
pub(crate) struct Chunk<'a, B> {
    buffer: &'a mut B,
    start: usize,
}

impl<'a, B: IsBuffer> Chunk<'a, B> {
    pub(crate) fn new(buffer: &'a mut B, start: usize) -> Self {
        Self { buffer, start }
    }
}

impl<B: IsBuffer> AsRef<[u8]> for Chunk<'_, B> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer.frame()[self.start..]
    }
}

impl<B: IsBuffer> AsMut<[u8]> for Chunk<'_, B> {
    fn as_mut(&mut self) -> &mut [u8] {
        let start = self.start;
        &mut self.buffer.frame_mut()[start..]
    }
}

impl<B: IsBuffer> AeadBuffer for Chunk<'_, B> {
    fn extend_from_slice(&mut self, other: &[u8]) -> core::result::Result<(), AeadError> {
        self.buffer.reserve(other.len()).copy_from_slice(other);
        self.buffer.commit(other.len());
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        self.buffer.truncate(self.start.saturating_add(len));
    }
}

/// The encrypting half of a [`Transport`].
#[derive(Debug)]
pub struct TransportEncryptState {
    encryption: NoiseEncryptor,
}

impl TransportEncryptState {
    pub(crate) fn encrypt<T: AeadBuffer>(&mut self, msg: &mut T) -> Result<()> {
        self.encryption.encrypt(msg).map_err(Into::into)
    }
}

/// The decrypting half of a [`Transport`].
#[derive(Debug)]
pub struct TransportDecryptState {
    decryption: NoiseDecryptor,
}

impl TransportDecryptState {
    pub(crate) fn decrypt<T: AeadBuffer>(&mut self, msg: &mut T) -> Result<()> {
        self.decryption.decrypt(msg).map_err(Into::into)
    }
}
