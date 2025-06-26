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
use stratum_common::roles_logic_sv2::{
    self,
    codec_sv2::{self, binary_sv2, framing_sv2, Frame},
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob},
    parsers::{AnyMessage, Mining},
    vardiff::error::VardiffError,
};
use v1::server_to_client::{Notify, SetDifficulty};

pub type ProxyResult<'a, T> = core::result::Result<T, Error<'a>>;

/// Represents specific errors that can occur when sending messages over various
/// channels used within the translator.
///
/// Each variant corresponds to a failure in sending a particular type of message
/// on its designated channel.
#[derive(Debug)]
pub enum ChannelSendError<'a> {
    /// Failure sending an SV2 `SubmitSharesExtended` message.
    SubmitSharesExtended(
        async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ),
    /// Failure sending an SV2 `SetNewPrevHash` message.
    SetNewPrevHash(async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>),
    /// Failure sending an SV2 `NewExtendedMiningJob` message.
    NewExtendedMiningJob(async_channel::SendError<NewExtendedMiningJob<'a>>),
    /// Failure broadcasting an SV1 `Notify` message
    Notify(tokio::sync::broadcast::error::SendError<Notify<'a>>),
    /// Failure sending a generic SV1 message.
    V1Message(async_channel::SendError<v1::Message>),
    /// Represents a generic channel send failure, described by a string.
    General(String),
    /// Failure sending extranonce information.
    Extranonce(async_channel::SendError<(ExtendedExtranonce, u32)>),
    /// Failure sending an SV2 `SetCustomMiningJob` message.
    SetCustomMiningJob(
        async_channel::SendError<roles_logic_sv2::mining_sv2::SetCustomMiningJob<'a>>,
    ),
    /// Failure sending new template information (prevhash and coinbase).
    NewTemplate(
        async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    ),
}

#[derive(Debug)]
pub enum Error<'a> {
    VecToSlice32(Vec<u8>),
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
    /// Errors from `roles_logic_sv2` crate.
    RolesSv2Logic(roles_logic_sv2::errors::Error),
    UpstreamIncoming(roles_logic_sv2::errors::Error),
    /// SV1 protocol library error
    V1Protocol(v1::error::Error<'a>),
    #[allow(dead_code)]
    SubprotocolMining(String),
    // Locking Errors
    PoisonLock,
    // Channel Receiver Error
    ChannelErrorReceiver(async_channel::RecvError),
    TokioChannelErrorRecv(tokio::sync::broadcast::error::RecvError),
    // Channel Sender Errors
    ChannelErrorSender(ChannelSendError<'a>),
    SetDifficultyToMessage(SetDifficulty),
    Infallible(std::convert::Infallible),
    // used to handle SV2 protocol error messages from pool
    #[allow(clippy::enum_variant_names)]
    Sv2ProtocolError(Mining<'a>),
    #[allow(clippy::enum_variant_names)]
    TargetError(roles_logic_sv2::errors::Error),
    Sv1MessageTooLong,
}

impl fmt::Display for Error<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{e:?}`"),
            BadConfigDeserialize(ref e) => write!(f, "Bad `config` TOML deserialize: `{e:?}`"),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{e:?}`"),
            CodecNoise(ref e) => write!(f, "Noise error: `{e:?}"),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{e:?}`"),
            InvalidExtranonce(ref e) => write!(f, "Invalid Extranonce error: `{e:?}"),
            Io(ref e) => write!(f, "I/O error: `{e:?}"),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{e:?}`"),
            RolesSv2Logic(ref e) => write!(f, "Roles SV2 Logic Error: `{e:?}`"),
            V1Protocol(ref e) => write!(f, "V1 Protocol Error: `{e:?}`"),
            SubprotocolMining(ref e) => write!(f, "Subprotocol Mining Error: `{e:?}`"),
            UpstreamIncoming(ref e) => write!(f, "Upstream parse incoming error: `{e:?}`"),
            PoisonLock => write!(f, "Poison Lock error"),
            ChannelErrorReceiver(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            ChannelErrorSender(ref e) => write!(f, "Channel send error: `{e:?}`"),
            SetDifficultyToMessage(ref e) => {
                write!(f, "Error converting SetDifficulty to Message: `{e:?}`")
            }
            VecToSlice32(ref e) => write!(f, "Standard Error: `{e:?}`"),
            Infallible(ref e) => write!(f, "Infallible Error:`{e:?}`"),
            Sv2ProtocolError(ref e) => {
                write!(f, "Received Sv2 Protocol Error from upstream: `{e:?}`")
            }
            TargetError(ref e) => {
                write!(f, "Impossible to get target from hashrate: `{e:?}`")
            }
            Sv1MessageTooLong => {
                write!(f, "Received an sv1 message that is longer than max len")
            }
        }
    }
}

impl From<binary_sv2::Error> for Error<'_> {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2(e)
    }
}

impl From<codec_sv2::noise_sv2::Error> for Error<'_> {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        Error::CodecNoise(e)
    }
}

impl From<framing_sv2::Error> for Error<'_> {
    fn from(e: framing_sv2::Error) -> Self {
        Error::FramingSv2(e)
    }
}

impl From<std::io::Error> for Error<'_> {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<std::num::ParseIntError> for Error<'_> {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<roles_logic_sv2::errors::Error> for Error<'_> {
    fn from(e: roles_logic_sv2::errors::Error) -> Self {
        Error::RolesSv2Logic(e)
    }
}

impl From<serde_json::Error> for Error<'_> {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}

impl From<ConfigError> for Error<'_> {
    fn from(e: ConfigError) -> Self {
        Error::BadConfigDeserialize(e)
    }
}

impl<'a> From<v1::error::Error<'a>> for Error<'a> {
    fn from(e: v1::error::Error<'a>) -> Self {
        Error::V1Protocol(e)
    }
}

impl From<async_channel::RecvError> for Error<'_> {
    fn from(e: async_channel::RecvError) -> Self {
        Error::ChannelErrorReceiver(e)
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for Error<'_> {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        Error::TokioChannelErrorRecv(e)
    }
}

//*** LOCK ERRORS ***
impl<T> From<PoisonError<T>> for Error<'_> {
    fn from(_e: PoisonError<T>) -> Self {
        Error::PoisonLock
    }
}

// *** CHANNEL SENDER ERRORS ***
impl<'a> From<async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>>
    for Error<'a>
{
    fn from(
        e: async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ) -> Self {
        Error::ChannelErrorSender(ChannelSendError::SubmitSharesExtended(e))
    }
}

impl<'a> From<async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>>
    for Error<'a>
{
    fn from(e: async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::SetNewPrevHash(e))
    }
}

impl<'a> From<tokio::sync::broadcast::error::SendError<Notify<'a>>> for Error<'a> {
    fn from(e: tokio::sync::broadcast::error::SendError<Notify<'a>>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::Notify(e))
    }
}

impl From<async_channel::SendError<v1::Message>> for Error<'_> {
    fn from(e: async_channel::SendError<v1::Message>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::V1Message(e))
    }
}

impl From<async_channel::SendError<(ExtendedExtranonce, u32)>> for Error<'_> {
    fn from(e: async_channel::SendError<(ExtendedExtranonce, u32)>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::Extranonce(e))
    }
}

impl<'a> From<async_channel::SendError<NewExtendedMiningJob<'a>>> for Error<'a> {
    fn from(e: async_channel::SendError<NewExtendedMiningJob<'a>>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::NewExtendedMiningJob(e))
    }
}

impl<'a> From<async_channel::SendError<SetCustomMiningJob<'a>>> for Error<'a> {
    fn from(e: async_channel::SendError<SetCustomMiningJob<'a>>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::SetCustomMiningJob(e))
    }
}

impl<'a>
    From<
        async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    > for Error<'a>
{
    fn from(
        e: async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    ) -> Self {
        Error::ChannelErrorSender(ChannelSendError::NewTemplate(e))
    }
}

impl From<Vec<u8>> for Error<'_> {
    fn from(e: Vec<u8>) -> Self {
        Error::VecToSlice32(e)
    }
}

impl From<SetDifficulty> for Error<'_> {
    fn from(e: SetDifficulty) -> Self {
        Error::SetDifficultyToMessage(e)
    }
}

impl From<std::convert::Infallible> for Error<'_> {
    fn from(e: std::convert::Infallible) -> Self {
        Error::Infallible(e)
    }
}

impl<'a> From<Mining<'a>> for Error<'a> {
    fn from(e: Mining<'a>) -> Self {
        Error::Sv2ProtocolError(e)
    }
}

impl From<async_channel::SendError<Frame<AnyMessage<'_>, codec_sv2::buffer_sv2::Slice>>>
    for Error<'_>
{
    fn from(
        value: async_channel::SendError<Frame<AnyMessage<'_>, codec_sv2::buffer_sv2::Slice>>,
    ) -> Self {
        Error::ChannelErrorSender(ChannelSendError::General(value.to_string()))
    }
}

impl<'a> From<VardiffError> for Error<'a> {
    fn from(value: VardiffError) -> Self {
        Self::RolesSv2Logic(value.into())
    }
}
