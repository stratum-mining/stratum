#![no_std]

extern crate alloc;

#[cfg(feature = "noise_sv2")]
use alloc::{boxed::Box, vec::Vec};

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

pub use framing_sv2::framing2::{Frame, Sv2Frame};
#[cfg(feature = "noise_sv2")]
pub use framing_sv2::framing2::{HandShakeFrame, NoiseFrame};

#[cfg(feature = "noise_sv2")]
pub use noise_sv2::{self, handshake::Step, Initiator, Responder, TransportMode};

pub use buffer_sv2;

#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum State {
    /// Not yet initialized
    NotInitialized,
    /// Handshake mode where codec is negotiating keys
    HandShake(Box<HandshakeRole>),
    /// Transport mode where AEAD is fully operational. The `TransportMode` object in this variant
    /// as able to perform encryption and decryption resp.
    Transport(TransportMode),
}

#[allow(clippy::large_enum_variant)]
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum HandshakeRole {
    Initiator(noise_sv2::Initiator),
    Responder(noise_sv2::Responder),
}

#[cfg(feature = "noise_sv2")]
impl HandshakeRole {
    pub fn step(&mut self, in_msg: Option<Vec<u8>>) -> Result<HandShakeFrame> {
        match self {
            Self::Initiator(stepper) => {
                let message = stepper.step(in_msg)?.inner();
                Ok(HandShakeFrame::from_message(message.into(), 0, 0, false)
                    .ok_or(Error::CodecTodo)?)
            }

            Self::Responder(stepper) => {
                let message = stepper.step(in_msg)?.inner();
                Ok(HandShakeFrame::from_message(message.into(), 0, 0, false)
                    .ok_or(())
                    .map_err(|_| Error::CodecTodo)?)
            }
        }
    }

    pub fn into_transport(self) -> Result<TransportMode> {
        match self {
            Self::Initiator(stepper) => {
                let tp = stepper.into_handshake_state().into_transport_mode()?;
                Ok(TransportMode::new(tp))
            }

            Self::Responder(stepper) => {
                let tp = stepper.into_handshake_state().into_transport_mode()?;
                Ok(TransportMode::new(tp))
            }
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl State {
    #[inline(always)]
    pub fn is_in_transport_mode(&self) -> bool {
        match self {
            Self::NotInitialized => false,
            Self::HandShake(_) => false,
            Self::Transport(_) => true,
        }
    }

    #[inline(always)]
    pub fn is_not_initialized(&self) -> bool {
        match self {
            Self::NotInitialized => true,
            Self::HandShake(_) => false,
            Self::Transport(_) => false,
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl State {
    pub fn take(&mut self) -> Self {
        let mut new_me = Self::NotInitialized;
        core::mem::swap(&mut new_me, self);
        new_me
    }

    pub fn new() -> Self {
        Self::NotInitialized
    }

    pub fn initialize(inner: HandshakeRole) -> Self {
        Self::HandShake(Box::new(inner))
    }

    pub fn with_transport_mode(tm: TransportMode) -> Self {
        Self::Transport(tm)
    }

    pub fn step(&mut self, in_msg: Option<Vec<u8>>) -> Result<HandShakeFrame> {
        match self {
            Self::NotInitialized => Err(Error::UnexpectedNoiseState),
            Self::HandShake(stepper) => stepper.step(in_msg),
            Self::Transport(_) => Err(Error::UnexpectedNoiseState),
        }
    }

    pub fn into_transport_mode(self) -> Result<Self> {
        match self {
            Self::NotInitialized => Err(Error::UnexpectedNoiseState),
            Self::HandShake(stepper) => {
                let tp = stepper.into_transport()?;

                Ok(Self::with_transport_mode(tp))
            }
            Self::Transport(_) => Ok(self),
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_step_fails_if_state_is_not_initialized() {
        let mut state = State::new();
        let msg = None;
        let actual = state.step(msg).unwrap_err();
        let expect = Error::UnexpectedNoiseState;
        assert_eq!(actual, expect);
    }

    #[test]
    fn handshake_step_fails_if_state_is_in_transport_mode() {
        let mut state = State::new();
        let msg = None;
        let actual = state.step(msg).unwrap_err();
        let expect = Error::UnexpectedNoiseState;
        assert_eq!(actual, expect);
    }

    #[test]
    fn into_transport_mode_errs_if_state_is_not_initialized() {
        let state = State::new();
        let actual = state.into_transport_mode().unwrap_err();
        let expect = Error::UnexpectedNoiseState;
        assert_eq!(actual, expect);
    }
}
