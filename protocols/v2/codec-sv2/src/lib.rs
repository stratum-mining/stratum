#![no_std]

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
pub use framing_sv2::framing2::HandShakeFrame;
pub use framing_sv2::framing2::{Frame, Sv2Frame};

#[cfg(feature = "noise_sv2")]
pub use noise_sv2::{self, Initiator, NoiseCodec, Responder};

pub use buffer_sv2;

pub use framing_sv2::{self, framing2::handshake_message_to_frame as h2f};

#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum State {
    /// Not yet initialized
    NotInitialized(usize),
    /// Handshake mode where codec is negotiating keys
    HandShake(HandshakeRole),
    /// Transport mode where AEAD is fully operational. The `TransportMode` object in this variant
    /// as able to perform encryption and decryption resp.
    Transport(NoiseCodec),
}
#[cfg(feature = "noise_sv2")]
impl State {
    pub fn step_0(&mut self) -> core::result::Result<HandShakeFrame, Error> {
        match self {
            Self::HandShake(h) => match h {
                HandshakeRole::Initiator(i) => i.step_0().map_err(|e| e.into()).map(h2f),
                HandshakeRole::Responder(_) => Err(Error::InvalidStepForResponder),
            },
            _ => Err(Error::NotInHandShakeState),
        }
    }

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
#[allow(clippy::large_enum_variant)]
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum HandshakeRole {
    Initiator(Box<noise_sv2::Initiator>),
    Responder(Box<noise_sv2::Responder>),
}

#[cfg(feature = "noise_sv2")]
impl State {
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

    pub fn initialized(inner: HandshakeRole) -> Self {
        Self::HandShake(inner)
    }

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
