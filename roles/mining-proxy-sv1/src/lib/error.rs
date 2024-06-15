use std::{fmt, sync::PoisonError};
use v1::server_to_client::{Notify, SetDifficulty};

use stratum_common::bitcoin::util::uint::ParseLengthError;

pub type Result<'a, T> = core::result::Result<T, Error<'a>>;

#[derive(Debug)]
pub enum Error<'a> {
    VecToSlice32(Vec<u8>),
    ConfigError(config::ConfigError),
    /// Errors on bad CLI argument input.
    #[allow(dead_code)]
    BadCliArgs,
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
    /// Errors on bad `TcpStream` connection.
    Io(std::io::Error),
    /// Errors due to invalid extranonce from upstream
    InvalidExtranonce(String),
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// SV1 protocol library error
    V1Protocol(v1::error::Error<'a>),
    // Locking Errors
    PoisonLock,
    TokioChannelErrorRecv(tokio::sync::broadcast::error::RecvError),
    // Channel Sender Errors
    Uint256Conversion(ParseLengthError),
    SetDifficultyToMessage(SetDifficulty),
    Infallible(std::convert::Infallible),
    Sv1MessageTooLong,
}

impl<'a> fmt::Display for Error<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            ConfigError(e) => write!(f, "Config error: {:?}", e),
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            InvalidExtranonce(ref e) => write!(f, "Invalid Extranonce error: `{:?}", e),
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{:?}`", e),
            V1Protocol(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
            PoisonLock => write!(f, "Poison Lock error"),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{:?}`", e),
            Uint256Conversion(ref e) => write!(f, "U256 Conversion Error: `{:?}`", e),
            SetDifficultyToMessage(ref e) => {
                write!(f, "Error converting SetDifficulty to Message: `{:?}`", e)
            }
            VecToSlice32(ref e) => write!(f, "Standard Error: `{:?}`", e),
            Infallible(ref e) => write!(f, "Infallible Error:`{:?}`", e),
            Sv1MessageTooLong => {
                write!(f, "Received an sv1 message that is longer than max len")
            }
        }
    }
}

impl<'a> From<config::ConfigError> for Error<'a> {
    fn from(e: config::ConfigError) -> Error<'a> {
        Error::ConfigError(e)
    }
}

impl<'a> From<std::io::Error> for Error<'a> {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl<'a> From<std::num::ParseIntError> for Error<'a> {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl<'a> From<serde_json::Error> for Error<'a> {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}

impl<'a> From<v1::error::Error<'a>> for Error<'a> {
    fn from(e: v1::error::Error<'a>) -> Self {
        Error::V1Protocol(e)
    }
}

impl<'a> From<tokio::sync::broadcast::error::RecvError> for Error<'a> {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        Error::TokioChannelErrorRecv(e)
    }
}

//*** LOCK ERRORS ***
impl<'a, T> From<PoisonError<T>> for Error<'a> {
    fn from(_e: PoisonError<T>) -> Self {
        Error::PoisonLock
    }
}

impl<'a> From<Vec<u8>> for Error<'a> {
    fn from(e: Vec<u8>) -> Self {
        Error::VecToSlice32(e)
    }
}

impl<'a> From<ParseLengthError> for Error<'a> {
    fn from(e: ParseLengthError) -> Self {
        Error::Uint256Conversion(e)
    }
}

impl<'a> From<SetDifficulty> for Error<'a> {
    fn from(e: SetDifficulty) -> Self {
        Error::SetDifficultyToMessage(e)
    }
}

impl<'a> From<std::convert::Infallible> for Error<'a> {
    fn from(e: std::convert::Infallible) -> Self {
        Error::Infallible(e)
    }
}
