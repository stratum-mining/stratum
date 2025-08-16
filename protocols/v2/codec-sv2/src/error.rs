//! # Error Handling and Result Types
//!
//! This module defines error types and utilities for handling errors in the `codec_sv2` module.
//! It includes the [`Error`] enum for representing various errors and a `Result` type alias for
//! convenience.

use core::fmt;
use framing_sv2::Error as FramingError;
#[cfg(feature = "noise_sv2")]
use noise_sv2::{AeadError, Error as NoiseError};

/// A type alias for results returned by the `codec_sv2` modules.
///
/// `Result` is a convenient wrapper around the [`core::result::Result`] type, using the [`Error`]
/// enum defined in this crate as the error type.
pub type Result<T> = core::result::Result<T, Error>;

/// Enumeration of possible errors in the `codec_sv2` module.
///
/// This enum represents various errors that can occur within the `codec_sv2` module, including
/// errors from related crates like [`binary_sv2`], [`framing_sv2`], and [`noise_sv2`].
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// AEAD (`snow`) error in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    AeadError(AeadError),

    /// Binary Sv2 data format error.
    BinarySv2Error(binary_sv2::Error),

    /// Framing Sv2 error.
    FramingError(FramingError),

    /// Framing Sv2 error.
    FramingSv2Error(framing_sv2::Error),

    /// Invalid step for initiator in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    InvalidStepForInitiator,

    /// Invalid step for responder in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    InvalidStepForResponder,

    /// Incomplete frame with the number of missing bytes remaining to completion.
    MissingBytes(usize),

    /// Sv2 Noise protocol error.
    #[cfg(feature = "noise_sv2")]
    NoiseSv2Error(NoiseError),

    /// Noise protocol is not in the expected handshake state.
    #[cfg(feature = "noise_sv2")]
    NotInHandShakeState,

    /// Unexpected state in the Noise protocol.
    UnexpectedNoiseState,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            #[cfg(feature = "noise_sv2")]
            AeadError(e) => write!(f, "Aead Error: `{e:?}`"),
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{e:?}`"),
            FramingError(e) => write!(f, "Framing error in codec: `{e:?}`"),
            FramingSv2Error(e) => write!(f, "Framing Sv2 Error: `{e:?}`"),
            #[cfg(feature = "noise_sv2")]
            InvalidStepForInitiator => write!(
                f,
                "This noise handshake step can not be executed by an initiato"
            ),
            #[cfg(feature = "noise_sv2")]
            InvalidStepForResponder => write!(
                f,
                "This noise handshake step can not be executed by a responder"
            ),
            MissingBytes(u) => write!(f, "Missing `{u}` Noise bytes"),
            #[cfg(feature = "noise_sv2")]
            NoiseSv2Error(e) => write!(f, "Noise SV2 Error: `{e:?}`"),
            #[cfg(feature = "noise_sv2")]
            NotInHandShakeState => write!(
                f,
                "This operation can be executed only during the noise handshake"
            ),
            UnexpectedNoiseState => {
                write!(f, "Noise state is incorrect")
            }
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl From<AeadError> for Error {
    fn from(e: AeadError) -> Self {
        Error::AeadError(e)
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}

impl From<framing_sv2::Error> for Error {
    fn from(e: framing_sv2::Error) -> Self {
        Error::FramingSv2Error(e)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseError> for Error {
    fn from(e: NoiseError) -> Self {
        Error::NoiseSv2Error(e)
    }
}
