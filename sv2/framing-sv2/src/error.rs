//! # Error Handling
//!
//! This module defines error types and utilities for handling errors in the `framing_sv2` module.

// use crate::framing2::EitherFrame;
use core::fmt;

use crate::SV2_FRAME_HEADER_SIZE;

// pub type FramingResult<T> = core::result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Binary Sv2 data format error.
    BinarySv2Error(binary_sv2::Error),
    ExpectedHandshakeFrame,
    ExpectedSv2Frame,
    UnexpectedHeaderLength(isize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(ref e) => {
                write!(f, "BinarySv2Error: `{e:?}`")
            }
            ExpectedHandshakeFrame => {
                write!(f, "Expected `HandshakeFrame`, received `Sv2Frame`")
            }
            ExpectedSv2Frame => {
                write!(f, "Expected `Sv2Frame`, received `HandshakeFrame`")
            }
            UnexpectedHeaderLength(actual_size) => {
                write!(
                    f,
                    "Unexpected `Header` length: `{actual_size}`, should be equal or more to {SV2_FRAME_HEADER_SIZE}"
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
