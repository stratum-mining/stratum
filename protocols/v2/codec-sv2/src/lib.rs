//! # Stratum V2 Codec Library
//!
//! `codec_sv2` provides the message encoding and decoding functionality for the Stratum V2 (Sv2)
//! protocol, handling secure communication between Sv2 roles.
//!
//! This crate abstracts the complexity of message encoding/decoding with optional Noise protocol
//! support, ensuring both regular and encrypted messages can be serialized, transmitted, and
//! decoded consistently and reliably.
//!
//!
//! ## Usage
//! `codec-sv2` supports both standard Sv2 frames (unencrypted) and Noise-encrypted Sv2 frames to
//! ensure secure communication. To encode messages for transmission, choose between the
//! [`Encoder`] for standard Sv2 frames or the [`NoiseEncoder`] for encrypted frames. To decode
//! received messages, choose between the [`StandardDecoder`] for standard Sv2 frames or
//! [`StandardNoiseDecoder`] to decrypt Noise frames.
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//!
//! - `std`: Enable usage of rust `std` library, enabled by default.
//! - `noise_sv2`: Enables support for Noise protocol encryption and decryption.
//! - `with_buffer_pool`: Enables buffer pooling for more efficient memory management.
//!
//! In order to use this crate in a `#![no_std]` environment, use the `--no-default-features` to
//! remove the `std` feature.
//!
//! ## Examples
//!
//! See the examples for more information:
//!
//! - [Unencrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/unencrypted.rs)
//! - [Encrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/encrypted.rs)

#![cfg_attr(not(feature = "std"), no_std)]

pub use framing_sv2::framing::Frame;

extern crate alloc;

#[cfg(feature = "noise_sv2")]
use alloc::boxed::Box;

mod decoder;
mod encoder;
pub mod error;

pub use error::{Error, Result};

pub use decoder::{StandardEitherFrame, StandardSv2Frame};

pub use decoder::StandardDecoder;
#[cfg(feature = "noise_sv2")]
pub use decoder::StandardNoiseDecoder;

pub use encoder::Encoder;
#[cfg(feature = "noise_sv2")]
pub use encoder::NoiseEncoder;

#[cfg(feature = "noise_sv2")]
pub use framing_sv2::framing::HandShakeFrame;
pub use framing_sv2::framing::Sv2Frame;

#[cfg(feature = "noise_sv2")]
pub use noise_sv2::{self, Initiator, NoiseCodec, Responder};

pub use buffer_sv2;

pub use binary_sv2;

pub use framing_sv2::{self, framing::handshake_message_to_frame as h2f};

/// Represents the role in the Noise handshake process, either as an initiator or a responder.
///
/// The Noise protocol requires two roles during the handshake process:
/// - **Initiator**: The party that starts the handshake by sending the initial message.
/// - **Responder**: The party that waits for the initiator's message and responds to it.
///
/// This enum distinguishes between these two roles, allowing the codec to handle the handshake
/// process accordingly.
#[allow(clippy::large_enum_variant)]
#[cfg(feature = "noise_sv2")]
#[derive(Debug, Clone)]
pub enum HandshakeRole {
    /// The initiator role in the Noise handshake process.
    ///
    /// The initiator starts the handshake by sending the initial message. This variant stores an
    /// `Initiator` object, which contains the necessary state and cryptographic materials for the
    /// initiator's part in the Noise handshake.
    Initiator(Box<noise_sv2::Initiator>),

    /// The responder role in the Noise handshake process.
    ///
    /// The responder waits for the initiator's handshake message and then responds. This variant
    /// stores a `Responder` object, which contains the necessary state and cryptographic materials
    /// for the responder's part in the Noise handshake.
    Responder(Box<noise_sv2::Responder>),
}

/// Represents the state of the Noise protocol codec during different phases: initialization,
/// handshake, or transport mode, where encryption and decryption are fully operational.
///
/// The state transitions from initialization [`State::NotInitialized`] to handshake
/// [`State::HandShake`] and finally to transport mode [`State::Transport`] as the encryption
/// handshake is completed.
#[cfg(feature = "noise_sv2")]
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum State {
    /// The codec has not been initialized yet.
    ///
    /// This state is used when the codec is first created or reset, before the handshake process
    /// begins. The variant carries the expected size of the handshake message, which can vary
    /// depending on whether the codec is acting as an initiator or a responder.
    NotInitialized(usize),

    /// The codec is in the handshake phase, where cryptographic keys are being negotiated.
    ///
    /// In this state, the codec is in the process of establishing secure communication by
    /// exchanging handshake messages. Once the handshake is complete, the codec transitions to
    /// [`State::Transport`] mode.
    HandShake(HandshakeRole),

    /// The codec is in transport mode, where AEAD encryption and decryption are fully operational.
    ///
    /// In this state, the codec is performing full encryption and decryption using the Noise
    /// protocol in transport mode. The [`NoiseCodec`] object is responsible for handling the
    /// encryption and decryption of data.
    Transport(NoiseCodec),
}

#[cfg(feature = "noise_sv2")]
impl State {
    /// Initiates the first step of the handshake process for the initiator.
    ///
    /// Creates and sends the initial handshake message for the initiator. It is the first step in
    /// establishing a secure communication channel. Responders cannot perform this step.
    ///
    /// nb: This method returns a [`HandShakeFrame`] but does not change the current state
    /// (`self`). The state remains `State::HandShake(HandshakeRole::Initiator)` until `step_1` is
    /// called to advance the handshake process.
    pub fn step_0(&mut self) -> core::result::Result<HandShakeFrame, Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Initiator(i) => i.step_0().map_err(|e| e.into()).map(h2f),
                HandshakeRole::Responder(_) => Err(Error::InvalidStepForResponder),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }

    /// Processes the second step of the handshake process for the responder.
    ///
    /// The responder receives the public key from the initiator, generates a response message
    /// containing the handshake frame, and prepares the [`NoiseCodec`] for transitioning the
    /// initiator state to transport mode in `step_2`.
    ///
    /// nb: Returns a new state [`State::Transport`] but does not update the current state
    /// (`self`). The caller is responsible for updating the state, allowing for more flexible
    /// control over the handshake process as the caller decides what to do with this state.
    #[cfg(feature = "std")]
    pub fn step_1(
        &mut self,
        re_pub: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE],
    ) -> core::result::Result<(HandShakeFrame, Self), Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        self.step_1_with_now_rng(re_pub, now, &mut rand::thread_rng())
    }

    /// Processes the second step of the handshake process for the responder given
    /// the current time and a custom random number generator.
    ///
    /// See [`Self::step_1`] for more details.
    ///
    /// The current time and the custom random number generatorshould be provided in order to not
    /// implicitely rely on `std` and allow `no_std` environments to provide a hardware random
    /// number generator for example.
    #[inline]
    pub fn step_1_with_now_rng<R: rand::Rng + rand::CryptoRng>(
        &mut self,
        re_pub: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE],
        now: u32,
        rng: &mut R,
    ) -> core::result::Result<(HandShakeFrame, Self), Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Responder(r) => {
                    let (message, codec) = r.step_1_with_now_rng(re_pub, now, rng)?;
                    Ok((h2f(message), Self::Transport(codec)))
                }
                HandshakeRole::Initiator(_) => Err(Error::InvalidStepForInitiator),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }

    /// Processes the final step of the handshake process for the initiator.
    ///
    /// Receives the response message from the responder containing the handshake frame, and
    /// transitions the state to transport mode. This finalizes the secure communication setup and
    /// enables full encryption and decryption in [`State::Transport`] mode.
    ///
    /// nb: Directly updates the current state (`self`) to [`State::Transport`], completing the
    /// handshake process.
    #[cfg(feature = "std")]
    pub fn step_2(
        &mut self,
        message: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
    ) -> core::result::Result<Self, Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        self.step_2_with_now(message, now)
    }

    /// Processes the final step of the handshake process for the initiator given the
    /// current system time.
    ///
    /// See [`Self::step_2`] for more details.
    ///
    /// The current system time should be provided to avoid relying on `std` and allow `no_std`
    /// environments to use another source of time.
    #[inline]
    pub fn step_2_with_now(
        &mut self,
        message: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
        now: u32,
    ) -> core::result::Result<Self, Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Initiator(i) => i
                    .step_2_with_now(message, now)
                    .map_err(|e| e.into())
                    .map(Self::Transport),
                HandshakeRole::Responder(_) => Err(Error::InvalidStepForResponder),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl State {
    /// Creates a new uninitialized handshake [`State`].
    ///
    /// Sets the codec to the initial state, [`State::NotInitialized`], based on the provided
    /// handshake role. This state is used before the handshake process begins, and the handshake
    /// message size guides the codec on how much data to expect before advancing to the next step.
    /// The expected size of the handshake message is determined by whether the codec is acting as
    /// an initiator or responder.
    pub fn not_initialized(role: &HandshakeRole) -> Self {
        match role {
            HandshakeRole::Initiator(_) => {
                Self::NotInitialized(noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE)
            }
            HandshakeRole::Responder(_) => Self::NotInitialized(noise_sv2::ELLSWIFT_ENCODING_SIZE),
        }
    }

    /// Initializes the codec state to [`State::HandShake`] mode with the given handshake role.
    ///
    /// Transitions the codec into the handshake phase by accepting a [`HandshakeRole`], which
    /// determines whether the codec is the initiator or responder in the handshake process. Once
    /// in [`State::HandShake`] mode, the codec begins negotiating cryptographic keys with the
    /// peer, eventually transitioning to the secure [`State::Transport`] phase.
    ///
    /// The role passed to this method defines how the handshake proceeds:
    /// - [`HandshakeRole::Initiator`]: The codec will start the handshake process.
    /// - [`HandshakeRole::Responder`]: The codec will wait for the initiator's handshake message.
    pub fn initialized(inner: HandshakeRole) -> Self {
        Self::HandShake(inner)
    }

    /// Transitions the codec state to [`State::Transport`] mode with the given [`NoiseCodec`].
    ///
    /// Finalizes the handshake process and transitions the codec into [`State::Transport`] mode,
    /// where full encryption and decryption are active. The codec uses the provided [`NoiseCodec`]
    /// to perform encryption and decryption for all communication in this mode, ensuring secure
    /// data transmission.
    ///
    /// Once in [`State::Transport`] mode, the codec is fully operational for secure communication.
    pub fn with_transport_mode(tm: NoiseCodec) -> Self {
        Self::Transport(tm)
    }
}

#[cfg(test)]
#[cfg(feature = "noise_sv2")]
mod tests {
    use super::*;

    #[test]
    fn handshake_step_fails_if_state_is_not_initialized() {
        let mut state = State::NotInitialized(32);
        let actual = state.step_0().unwrap_err();
        let expect = Error::NotInHandShakeState;
        assert_eq!(actual, expect);
    }

    #[test]
    fn handshake_step_fails_if_state_is_in_transport_mode() {
        let mut state = State::NotInitialized(32);
        let actual = state.step_0().unwrap_err();
        let expect = Error::NotInHandShakeState;
        assert_eq!(actual, expect);
    }
}
