//! # Error Handling and Result Types
//!
//! This module defines error types and utilities for handling errors in the `codec_sv2` module.
//! It includes the [`Error`] enum for representing various errors and a `Result` type alias for
//! convenience.

use core::fmt;
use framing_sv2::framing::SizeHint;
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
/// errors from related crates like [`binary_sv2`], [`framing_sv2`], and `noise_sv2`.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// AEAD (`snow`) error in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    AeadError(AeadError),

    /// Binary Sv2 data format error.
    BinarySv2Error(binary_sv2::Error),

    /// Framing Sv2 error.
    FramingSv2Error(framing_sv2::Error),

    /// Sv2 Noise protocol error.
    #[cfg(feature = "noise_sv2")]
    NoiseSv2Error(NoiseError),

    /// The bytes taken out of the decoder buffer do not hold exactly one frame.
    UnexpectedFrameSize(SizeHint),

    /// A decoder was told more bytes were read than its read window holds.
    ReadBeyondWindow {
        /// The number of bytes reported as read.
        read: usize,
        /// The size of the window the last `read_buf` returned, or `0` if none is open.
        window: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            #[cfg(feature = "noise_sv2")]
            AeadError(e) => write!(f, "Aead Error: `{e:?}`"),
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{e}`"),
            FramingSv2Error(e) => write!(f, "Framing Sv2 Error: `{e}`"),
            #[cfg(feature = "noise_sv2")]
            NoiseSv2Error(e) => match e {
                NoiseError::InvalidCertificate(msg) => {
                    write!(f, "Invalid Certificate: {}", msg)
                }
                other => {
                    write!(f, "Noise SV2 Error: {:?}", other)
                }
            },
            UnexpectedFrameSize(hint) => {
                write!(f, "Buffered bytes do not hold exactly one frame: {hint}")
            }
            ReadBeyondWindow { read, window } => {
                write!(f, "Reported {read} bytes read into a window of {window}")
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

impl From<SizeHint> for Error {
    fn from(hint: SizeHint) -> Self {
        Error::UnexpectedFrameSize(hint)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseError> for Error {
    fn from(e: NoiseError) -> Self {
        Error::NoiseSv2Error(e)
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use alloc::{string::ToString, vec};

    // `ValueExceedsMaxSize` carries a peer-derived sample and this error is logged, so no
    // formatting path may write that sample out.
    #[test]
    fn binary_error_display_does_not_dump_embedded_payload() {
        let sample = vec![0xAB_u8; 64 * 1024];
        let err = Error::BinarySv2Error(binary_sv2::Error::ValueExceedsMaxSize(
            false,
            1,
            1,
            32,
            sample,
            64 * 1024,
        ));

        let rendered = err.to_string();

        assert!(
            !rendered.contains("171"),
            "Display leaked sample bytes: {rendered}"
        );
        assert!(
            rendered.len() < 128,
            "Display grew with the sample: {} bytes",
            rendered.len()
        );
    }
}
