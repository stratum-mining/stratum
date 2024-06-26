use roles_logic_sv2::{
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob},
    parsers::Mining,
};
use std::{fmt, sync::PoisonError};
use v1::server_to_client::{Notify, SetDifficulty};

use stratum_common::bitcoin::util::uint::ParseLengthError;

pub type TProxyResult<'a, T> = core::result::Result<T, TProxyError<'a>>;

#[derive(Debug)]
pub enum ChannelSendError<'a> {
    SubmitSharesExtended(
        async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ),
    SetNewPrevHash(async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>),
    NewExtendedMiningJob(async_channel::SendError<NewExtendedMiningJob<'a>>),
    Notify(tokio::sync::broadcast::error::SendError<Notify<'a>>),
    V1Message(async_channel::SendError<v1::Message>),
    General(String),
    Extranonce(async_channel::SendError<(ExtendedExtranonce, u32)>),
    SetCustomMiningJob(
        async_channel::SendError<roles_logic_sv2::mining_sv2::SetCustomMiningJob<'a>>,
    ),
    NewTemplate(
        async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    ),
}

#[derive(Debug)]
pub enum TProxyError<'a> {
    VecToSlice32(Vec<u8>),
    ConfigError(ext_config::ConfigError),
    /// Errors on bad CLI argument input.
    #[allow(dead_code)]
    BadCliArgs,
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
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
    RolesLogicSv2(roles_logic_sv2::errors::Error),
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
    Uint256Conversion(ParseLengthError),
    SetDifficultyToMessage(SetDifficulty),
    Infallible(std::convert::Infallible),
    // used to handle SV2 protocol error messages from pool
    #[allow(clippy::enum_variant_names)]
    Sv2ProtocolError(Mining<'a>),
    #[allow(clippy::enum_variant_names)]
    TargetError(roles_logic_sv2::errors::Error),
    Sv1MessageTooLong,
}

impl<'a> fmt::Display for TProxyError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TProxyError::*;
        match self {
            ConfigError(e) => write!(f, "Config error: {:?}", e),
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            CodecNoise(ref e) => write!(f, "Noise error: `{:?}", e),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            InvalidExtranonce(ref e) => write!(f, "Invalid Extranonce error: `{:?}", e),
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{:?}`", e),
            RolesLogicSv2(ref e) => write!(f, "Roles SV2 Logic Error: `{:?}`", e),
            V1Protocol(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
            SubprotocolMining(ref e) => write!(f, "Subprotocol Mining Error: `{:?}`", e),
            UpstreamIncoming(ref e) => write!(f, "Upstream parse incoming error: `{:?}`", e),
            PoisonLock => write!(f, "Poison Lock error"),
            ChannelErrorReceiver(ref e) => write!(f, "Channel receive error: `{:?}`", e),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{:?}`", e),
            ChannelErrorSender(ref e) => write!(f, "Channel send error: `{:?}`", e),
            Uint256Conversion(ref e) => write!(f, "U256 Conversion Error: `{:?}`", e),
            SetDifficultyToMessage(ref e) => {
                write!(f, "Error converting SetDifficulty to Message: `{:?}`", e)
            }
            VecToSlice32(ref e) => write!(f, "Standard Error: `{:?}`", e),
            Infallible(ref e) => write!(f, "Infallible Error:`{:?}`", e),
            Sv2ProtocolError(ref e) => {
                write!(f, "Received Sv2 Protocol Error from upstream: `{:?}`", e)
            }
            TargetError(ref e) => {
                write!(f, "Impossible to get target from hashrate: `{:?}`", e)
            }
            Sv1MessageTooLong => {
                write!(f, "Received an sv1 message that is longer than max len")
            }
        }
    }
}

impl<'a> From<ext_config::ConfigError> for TProxyError<'a> {
    fn from(e: ext_config::ConfigError) -> TProxyError<'a> {
        TProxyError::ConfigError(e)
    }
}

impl<'a> From<binary_sv2::Error> for TProxyError<'a> {
    fn from(e: binary_sv2::Error) -> Self {
        TProxyError::BinarySv2(e)
    }
}

impl<'a> From<codec_sv2::noise_sv2::Error> for TProxyError<'a> {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        TProxyError::CodecNoise(e)
    }
}

impl<'a> From<framing_sv2::Error> for TProxyError<'a> {
    fn from(e: framing_sv2::Error) -> Self {
        TProxyError::FramingSv2(e)
    }
}

impl<'a> From<std::io::Error> for TProxyError<'a> {
    fn from(e: std::io::Error) -> Self {
        TProxyError::Io(e)
    }
}

impl<'a> From<std::num::ParseIntError> for TProxyError<'a> {
    fn from(e: std::num::ParseIntError) -> Self {
        TProxyError::ParseInt(e)
    }
}

impl<'a> From<roles_logic_sv2::errors::Error> for TProxyError<'a> {
    fn from(e: roles_logic_sv2::errors::Error) -> Self {
        TProxyError::RolesLogicSv2(e)
    }
}

impl<'a> From<serde_json::Error> for TProxyError<'a> {
    fn from(e: serde_json::Error) -> Self {
        TProxyError::BadSerdeJson(e)
    }
}

impl<'a> From<v1::error::Error<'a>> for TProxyError<'a> {
    fn from(e: v1::error::Error<'a>) -> Self {
        TProxyError::V1Protocol(e)
    }
}

impl<'a> From<async_channel::RecvError> for TProxyError<'a> {
    fn from(e: async_channel::RecvError) -> Self {
        TProxyError::ChannelErrorReceiver(e)
    }
}

impl<'a> From<tokio::sync::broadcast::error::RecvError> for TProxyError<'a> {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        TProxyError::TokioChannelErrorRecv(e)
    }
}

//*** LOCK ERRORS ***
impl<'a, T> From<PoisonError<T>> for TProxyError<'a> {
    fn from(_e: PoisonError<T>) -> Self {
        TProxyError::PoisonLock
    }
}

// *** CHANNEL SENDER ERRORS ***
impl<'a> From<async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>>
    for TProxyError<'a>
{
    fn from(
        e: async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::SubmitSharesExtended(e))
    }
}

impl<'a> From<async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>>
    for TProxyError<'a>
{
    fn from(e: async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::SetNewPrevHash(e))
    }
}

impl<'a> From<tokio::sync::broadcast::error::SendError<Notify<'a>>> for TProxyError<'a> {
    fn from(e: tokio::sync::broadcast::error::SendError<Notify<'a>>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::Notify(e))
    }
}

impl<'a> From<async_channel::SendError<v1::Message>> for TProxyError<'a> {
    fn from(e: async_channel::SendError<v1::Message>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::V1Message(e))
    }
}

impl<'a> From<async_channel::SendError<(ExtendedExtranonce, u32)>> for TProxyError<'a> {
    fn from(e: async_channel::SendError<(ExtendedExtranonce, u32)>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::Extranonce(e))
    }
}

impl<'a> From<async_channel::SendError<NewExtendedMiningJob<'a>>> for TProxyError<'a> {
    fn from(e: async_channel::SendError<NewExtendedMiningJob<'a>>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::NewExtendedMiningJob(e))
    }
}

impl<'a> From<async_channel::SendError<SetCustomMiningJob<'a>>> for TProxyError<'a> {
    fn from(e: async_channel::SendError<SetCustomMiningJob<'a>>) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::SetCustomMiningJob(e))
    }
}

impl<'a>
    From<
        async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    > for TProxyError<'a>
{
    fn from(
        e: async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    ) -> Self {
        TProxyError::ChannelErrorSender(ChannelSendError::NewTemplate(e))
    }
}

impl<'a> From<Vec<u8>> for TProxyError<'a> {
    fn from(e: Vec<u8>) -> Self {
        TProxyError::VecToSlice32(e)
    }
}

impl<'a> From<ParseLengthError> for TProxyError<'a> {
    fn from(e: ParseLengthError) -> Self {
        TProxyError::Uint256Conversion(e)
    }
}

impl<'a> From<SetDifficulty> for TProxyError<'a> {
    fn from(e: SetDifficulty) -> Self {
        TProxyError::SetDifficultyToMessage(e)
    }
}

impl<'a> From<std::convert::Infallible> for TProxyError<'a> {
    fn from(e: std::convert::Infallible) -> Self {
        TProxyError::Infallible(e)
    }
}

impl<'a> From<Mining<'a>> for TProxyError<'a> {
    fn from(e: Mining<'a>) -> Self {
        TProxyError::Sv2ProtocolError(e)
    }
}
