//! # Stratum V2 Codec Library
//!
//! This crate provides the encoding and decoding functionality for the Stratum V2 protocol,
//! handling secure communication between clients and servers.
//!
//! `codec-sv2` is essential for implementing the Sv2 protocol correctly and securely. It abstracts
//! the complexity of message encoding, decoding, and encryption, providing a reliable and
//! consistent foundation for mining software imperative for interoperability.
//!
//! ## Usage
//!
//! This crate is designed to be used in mining software that needs to communicate securely
//! using the Stratum V2 protocol. It supports optional Noise protocol features for encryption
//! and ensures data integrity and confidentiality.
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//! * `with_serde`: builds `binary_sv2` and `buffer_sv2` crates with `serde`-based encoding and
//!   decoding.
//! * `with_buffer_pool`: uses `buffer_sv2` to provide a more efficient allocation method for
//!   `non_std` environments. Please refer to `buffer_sv2` crate docs for more context.
//! * `noise_sv2`: enables encryption via Noise protocol.
//!
//! The `with_serde` feature flag is only used for the Message Generator, and deprecated for any
//! other kind of usage. It will likely be fully deprecated in the future.

#![cfg_attr(feature = "no_std", no_std)]

pub use framing_sv2::framing::Frame;

extern crate alloc;

#[cfg(feature = "noise_sv2")]
use alloc::boxed::Box;

mod decoder;
mod encoder;
pub mod error;

pub use error::{CError, Error, Result};

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

pub use framing_sv2::{self, framing::handshake_message_to_frame as h2f};

/// Represents the state of the codec, which can be in different phases such as initialization,
/// handshake, or transport mode where encryption and decryption are fully operational.
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum State {
    /// The codec has not yet been initialized.
    ///
    /// This state is used when the codec is first created or reset, and before it has started the
    /// handshake process. This state carries information about the expected size of the handshake
    /// message, which can be different for initiators and responders.
    NotInitialized(usize),

    /// The codec is in handshake mode, where it is negotiating cryptographic keys.
    HandShake(HandshakeRole),

    /// The codec is in transport mode, where AEAD encryption and decryption are fully operational.
    /// The `NoiseCodec` object in this variant performs the encryption and decryption.
    Transport(NoiseCodec),
}

#[cfg(feature = "noise_sv2")]
impl State {
    /// Initiates the first step of the handshake process for the initiator.
    ///
    /// This step involves creating and sending the initial handshake message for the initiator.
    /// Responders cannot perform this step.
    ///
    /// nb: This method returns a `HandShakeFrame` but does not update the current state (`self`).
    /// The state remains `HandShake(Initiator)` until `step_1` is called.
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
    /// This step involves receiving the public key from the initiator, generating a response
    /// message containing the handshake frame, and creates the `NoiseCodec` for transitioning the
    /// state to transport mode for the initiator in `step_2`.
    ///
    /// nb: This method returns a new state (`State::Transport`), but the caller is responsible for
    /// updating the current state (`self`). This allows for more flexible control, as the caller
    /// can decide what to do with the new state.
    pub fn step_1(
        &mut self,
        re_pub: [u8; const_sv2::RESPONDER_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
    ) -> core::result::Result<(HandShakeFrame, Self), Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Responder(r) => {
                    let (message, codec) = r.step_1(re_pub)?;
                    Ok((h2f(message), Self::Transport(codec)))
                }
                HandshakeRole::Initiator(_) => Err(Error::InvalidStepForInitiator),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }

    /// Processes the final step of the handshake process for initiator.
    ///
    /// This step involves receiving the response message containing the handshake frame from the
    /// responder and transitions the state to transport mode.
    ///
    /// nb: This method directly updates the current state (`self`) to the new state
    /// (`State::Transport`), finalizing the transition.
    pub fn step_2(
        &mut self,
        message: [u8; const_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE],
    ) -> core::result::Result<Self, Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Initiator(i) => {
                    i.step_2(message).map_err(|e| e.into()).map(Self::Transport)
                }
                HandshakeRole::Responder(_) => Err(Error::InvalidStepForResponder),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }
}

/// The role in the Noise handshake process, either as an initiator or a responder.
#[allow(clippy::large_enum_variant)]
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum HandshakeRole {
    /// The initiator role in the Noise handshake process.
    Initiator(Box<noise_sv2::Initiator>),

    /// The responder role in the Noise handshake process.
    Responder(Box<noise_sv2::Responder>),
}

#[cfg(feature = "noise_sv2")]
impl State {
    /// Creates a new `NotInitialized` state with the expected handshake message size for the
    /// given role.
    pub fn not_initialized(role: &HandshakeRole) -> Self {
        match role {
            HandshakeRole::Initiator(_) => {
                Self::NotInitialized(const_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE)
            }
            HandshakeRole::Responder(_) => {
                Self::NotInitialized(const_sv2::RESPONDER_EXPECTED_HANDSHAKE_MESSAGE_SIZE)
            }
        }
    }

    /// Initializes the codec state to `HandShake` mode with the given handshake role.
    pub fn initialized(inner: HandshakeRole) -> Self {
        Self::HandShake(inner)
    }

    /// Transitions the codec state to `Transport` mode with the given `NoiseCodec`.
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
