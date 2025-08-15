//! ## Translator Error Module
//!
//! Defines the custom error types used throughout the translator proxy.
//!
//! This module centralizes error handling by providing:
//! - A primary `Error` enum encompassing various error kinds from different sources (I/O, parsing,
//!   protocol logic, channels, configuration, etc.).
//! - A specific `ChannelSendError` enum for errors occurring during message sending over
//!   asynchronous channels.

use ext_config::ConfigError;
use std::{fmt, sync::PoisonError};
use tokio::sync::broadcast;
use v1::server_to_client::SetDifficulty;

#[derive(Debug)]
pub enum TproxyError {
    /// Error converting a vector to a fixed-size slice
    VecToSlice32(Vec<u8>),
    /// Generic SV1 protocol error
    SV1Error,
    /// Error from the network helpers library
    NetworkHelpersError(network_helpers_sv2::Error),
    /// Error from the roles logic library
    RolesSv2LogicError(roles_logic_sv2::Error),
    /// Error from roles logic parser library
    ParserError(roles_logic_sv2::parsers_sv2::ParserError),
    /// Error from roles logic handlers Library
    RolesSv2LogicHandlerError(roles_logic_sv2::handlers_sv2::HandlerError),
    /// Errors on bad CLI argument input.
    BadCliArgs,
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
    /// Errors on bad `config` TOML deserialize.
    BadConfigDeserialize(ConfigError),
    /// Errors from `binary_sv2` crate.
    BinarySv2(binary_sv2::Error),
    /// Errors on bad noise handshake.
    CodecNoise(codec_sv2::noise_sv2::Error),
    /// Errors from `framing_sv2` crate.
    FramingSv2(framing_sv2::Error),
    /// Errors on bad `TcpStream` connection.
    Io(std::io::Error),
    /// Errors due to invalid extranonce from upstream
    InvalidExtranonce(String),
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// Error parsing incoming upstream messages
    UpstreamIncoming(roles_logic_sv2::errors::Error),
    /// Mining subprotocol error
    #[allow(dead_code)]
    SubprotocolMining(String),
    /// Mutex poison lock error
    PoisonLock,
    /// Channel receiver error
    ChannelErrorReceiver(async_channel::RecvError),
    /// Channel sender error
    ChannelErrorSender,
    /// Broadcast channel receiver error
    BroadcastChannelErrorReceiver(broadcast::error::RecvError),
    /// Tokio channel receiver error
    TokioChannelErrorRecv(tokio::sync::broadcast::error::RecvError),
    /// Error converting SetDifficulty to Message
    SetDifficultyToMessage(SetDifficulty),
    /// Target calculation error
    #[allow(clippy::enum_variant_names)]
    TargetError(roles_logic_sv2::errors::Error),
    /// SV1 message exceeds maximum length
    Sv1MessageTooLong,
    /// Received an unexpected message type
    UnexpectedMessage,
    /// Job not found during share validation
    JobNotFound,
    /// Invalid merkle root during share validation
    InvalidMerkleRoot,
    /// Shutdown signal received
    Shutdown,
    /// Represents a generic channel send failure, described by a string.
    General(String),
    /// Error bubbling up from translator-core library
    TranslatorCore(stratum_translation::error::StratumTranslationError),
}

impl std::error::Error for TproxyError {}

impl fmt::Display for TproxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TproxyError::*;
        match self {
            General(e) => write!(f, "{e}"),
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{e:?}`"),
            BadConfigDeserialize(ref e) => write!(f, "Bad `config` TOML deserialize: `{e:?}`"),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{e:?}`"),
            CodecNoise(ref e) => write!(f, "Noise error: `{e:?}"),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{e:?}`"),
            InvalidExtranonce(ref e) => write!(f, "Invalid Extranonce error: `{e:?}"),
            Io(ref e) => write!(f, "I/O error: `{e:?}"),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{e:?}`"),
            SubprotocolMining(ref e) => write!(f, "Subprotocol Mining Error: `{e:?}`"),
            UpstreamIncoming(ref e) => write!(f, "Upstream parse incoming error: `{e:?}`"),
            PoisonLock => write!(f, "Poison Lock error"),
            ChannelErrorReceiver(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            BroadcastChannelErrorReceiver(ref e) => {
                write!(f, "Broadcast channel receive error: {e:?}")
            }
            ChannelErrorSender => write!(f, "Sender error"),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            SetDifficultyToMessage(ref e) => {
                write!(f, "Error converting SetDifficulty to Message: `{e:?}`")
            }
            VecToSlice32(ref e) => write!(f, "Standard Error: `{e:?}`"),
            TargetError(ref e) => {
                write!(f, "Impossible to get target from hashrate: `{e:?}`")
            }
            Sv1MessageTooLong => {
                write!(f, "Received an sv1 message that is longer than max len")
            }
            UnexpectedMessage => {
                write!(f, "Received a message type that was not expected")
            }
            JobNotFound => write!(f, "Job not found during share validation"),
            InvalidMerkleRoot => write!(f, "Invalid merkle root during share validation"),
            Shutdown => write!(f, "Shutdown signal"),
            SV1Error => write!(f, "Sv1 error"),
            TranslatorCore(ref e) => write!(f, "Translator core error: {e:?}"),
            NetworkHelpersError(ref e) => write!(f, "Network helpers error: {e:?}"),
            RolesSv2LogicError(ref e) => write!(f, "Roles logic error: {e:?}"),
            ParserError(ref e) => write!(f, "Roles logic parser error: {e:?}"),
            RolesSv2LogicHandlerError(ref e) => write!(f, "Roles logic handler error: {e:?}"),
        }
    }
}

impl From<binary_sv2::Error> for TproxyError {
    fn from(e: binary_sv2::Error) -> Self {
        TproxyError::BinarySv2(e)
    }
}

impl From<roles_logic_sv2::handlers_sv2::HandlerError> for TproxyError {
    fn from(value: roles_logic_sv2::handlers_sv2::HandlerError) -> Self {
        TproxyError::RolesSv2LogicHandlerError(value)
    }
}

impl From<codec_sv2::noise_sv2::Error> for TproxyError {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        TproxyError::CodecNoise(e)
    }
}

impl From<framing_sv2::Error> for TproxyError {
    fn from(e: framing_sv2::Error) -> Self {
        TproxyError::FramingSv2(e)
    }
}

impl From<std::io::Error> for TproxyError {
    fn from(e: std::io::Error) -> Self {
        TproxyError::Io(e)
    }
}

impl From<std::num::ParseIntError> for TproxyError {
    fn from(e: std::num::ParseIntError) -> Self {
        TproxyError::ParseInt(e)
    }
}

impl From<serde_json::Error> for TproxyError {
    fn from(e: serde_json::Error) -> Self {
        TproxyError::BadSerdeJson(e)
    }
}

impl From<ConfigError> for TproxyError {
    fn from(e: ConfigError) -> Self {
        TproxyError::BadConfigDeserialize(e)
    }
}

impl From<async_channel::RecvError> for TproxyError {
    fn from(e: async_channel::RecvError) -> Self {
        TproxyError::ChannelErrorReceiver(e)
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for TproxyError {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        TproxyError::TokioChannelErrorRecv(e)
    }
}

//*** LOCK ERRORS ***
impl<T> From<PoisonError<T>> for TproxyError {
    fn from(_e: PoisonError<T>) -> Self {
        TproxyError::PoisonLock
    }
}

impl From<Vec<u8>> for TproxyError {
    fn from(e: Vec<u8>) -> Self {
        TproxyError::VecToSlice32(e)
    }
}

impl From<SetDifficulty> for TproxyError {
    fn from(e: SetDifficulty) -> Self {
        TproxyError::SetDifficultyToMessage(e)
    }
}

impl<'a> From<v1::error::Error<'a>> for TproxyError {
    fn from(_: v1::error::Error<'a>) -> Self {
        TproxyError::SV1Error
    }
}

impl From<network_helpers_sv2::Error> for TproxyError {
    fn from(value: network_helpers_sv2::Error) -> Self {
        TproxyError::NetworkHelpersError(value)
    }
}

impl From<stratum_translation::error::StratumTranslationError> for TproxyError {
    fn from(e: stratum_translation::error::StratumTranslationError) -> Self {
        TproxyError::TranslatorCore(e)
    }
}
