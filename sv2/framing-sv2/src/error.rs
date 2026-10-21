//! # Error Handling
//!
//! This module defines error types and utilities for handling errors in the `framing_sv2` module.

use core::fmt;

use crate::SV2_FRAME_HEADER_SIZE;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Binary Sv2 data format error.
    BinarySv2Error(binary_sv2::Error),

    /// The buffer passed to [`crate::framing::EncodableFrame::encode_into`] is not exactly as
    /// long as the frame.
    UnexpectedDestinationLength {
        /// Length the encoded frame takes.
        expected: usize,
        /// Length of the buffer that was passed.
        actual: usize,
    },

    /// The message serialized to a different number of bytes than the
    /// [`crate::header::Header`] declares, i.e. than its `GetSize` reported.
    PayloadLengthMismatch {
        /// Payload length the header declares.
        declared: usize,
        /// Number of bytes the message serialized to.
        written: usize,
    },

    /// The buffer is too short to hold a [`crate::header::Header`].
    UnexpectedHeaderLength(usize),

    /// A message of the given serialized length does not fit the 24-bit `msg_length` field of a
    /// [`crate::header::Header`].
    PayloadTooLong(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(ref e) => {
                write!(f, "BinarySv2Error: `{e}`")
            }
            UnexpectedDestinationLength { expected, actual } => {
                write!(
                    f,
                    "Destination buffer is `{actual}` bytes long, the encoded frame takes `{expected}`"
                )
            }
            PayloadLengthMismatch { declared, written } => {
                write!(
                    f,
                    "Message serialized to `{written}` bytes, its `Header` declares `{declared}`"
                )
            }
            UnexpectedHeaderLength(actual_size) => {
                write!(
                    f,
                    "Unexpected `Header` length: `{actual_size}`, should be equal or more to {SV2_FRAME_HEADER_SIZE}"
                )
            }
            PayloadTooLong(len) => {
                write!(
                    f,
                    "Payload of `{len}` bytes does not fit the 24-bit `msg_length` of a `Header`"
                )
            }
        }
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
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
