use crate::proxy;
use roles_logic_sv2::{
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob},
    parsers::Mining,
};
use std::{
    fmt,
    sync::{MutexGuard, PoisonError},
};
use v1::server_to_client::{Notify, SetDifficulty};

use stratum_common::bitcoin::util::uint::ParseLengthError;

pub type ProxyResult<'a, T> = core::result::Result<T, Error<'a>>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum LockError<'a> {
    Bridge(PoisonError<MutexGuard<'a, proxy::Bridge>>),
}

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
pub enum Error<'a> {
    VecToSlice32(Vec<u8>),
    /// Errors on bad CLI argument input.
    BadCliArgs,
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
    /// Errors on bad `toml` deserialize.
    BadTomlDeserialize(toml::de::Error),
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
    Uint256Conversion(ParseLengthError),
    SetDifficultyToMessage(SetDifficulty),
    Infallible(std::convert::Infallible),
    // used to handle SV2 protocol error messages from pool
    Sv2ProtocolError(Mining<'a>),
    TargetError(roles_logic_sv2::errors::Error),
}

impl<'a> fmt::Display for Error<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            BadTomlDeserialize(ref e) => write!(f, "Bad `toml` deserialize: `{:?}`", e),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            CodecNoise(ref e) => write!(f, "Noise error: `{:?}", e),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            InvalidExtranonce(ref e) => write!(f, "Invalid Extranonce error: `{:?}", e),
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{:?}`", e),
            RolesSv2Logic(ref e) => write!(f, "Roles SV2 Logic Error: `{:?}`", e),
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
        }
    }
}

impl<'a> From<binary_sv2::Error> for Error<'a> {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2(e)
    }
}

impl<'a> From<codec_sv2::noise_sv2::Error> for Error<'a> {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        Error::CodecNoise(e)
    }
}

impl<'a> From<framing_sv2::Error> for Error<'a> {
    fn from(e: framing_sv2::Error) -> Self {
        Error::FramingSv2(e)
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

impl<'a> From<roles_logic_sv2::errors::Error> for Error<'a> {
    fn from(e: roles_logic_sv2::errors::Error) -> Self {
        Error::RolesSv2Logic(e)
    }
}

impl<'a> From<serde_json::Error> for Error<'a> {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}

impl<'a> From<toml::de::Error> for Error<'a> {
    fn from(e: toml::de::Error) -> Self {
        Error::BadTomlDeserialize(e)
    }
}

impl<'a> From<v1::error::Error<'a>> for Error<'a> {
    fn from(e: v1::error::Error<'a>) -> Self {
        Error::V1Protocol(e)
    }
}

impl<'a> From<async_channel::RecvError> for Error<'a> {
    fn from(e: async_channel::RecvError) -> Self {
        Error::ChannelErrorReceiver(e)
    }
}

impl<'a> From<tokio::sync::broadcast::error::RecvError> for Error<'a> {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        Error::TokioChannelErrorRecv(e)
    }
}

// *** LOCK ERRORS ***
// impl<'a> From<PoisonError<MutexGuard<'a, proxy::Bridge>>> for Error<'a> {
//     fn from(e: PoisonError<MutexGuard<'a, proxy::Bridge>>) -> Self {
//         Error::PoisonLock(
//             LockError::Bridge(e)
//         )
//     }
// }

// impl<'a> From<PoisonError<MutexGuard<'a, NextMiningNotify>>> for Error<'a> {
//     fn from(e: PoisonError<MutexGuard<'a, NextMiningNotify>>) -> Self {
//         Error::PoisonLock(
//             LockError::NextMiningNotify(e)
//         )
//     }
// }

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

impl<'a> From<async_channel::SendError<v1::Message>> for Error<'a> {
    fn from(e: async_channel::SendError<v1::Message>) -> Self {
        Error::ChannelErrorSender(ChannelSendError::V1Message(e))
    }
}

impl<'a> From<async_channel::SendError<(ExtendedExtranonce, u32)>> for Error<'a> {
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

impl<'a> From<Mining<'a>> for Error<'a> {
    fn from(e: Mining<'a>) -> Self {
        Error::Sv2ProtocolError(e)
    }
}
