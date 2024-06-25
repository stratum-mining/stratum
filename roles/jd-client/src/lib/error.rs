use std::fmt;

use roles_logic_sv2::mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob};
use stratum_common::bitcoin::util::uint::ParseLengthError;

pub type JdcResult<'a, T> = core::result::Result<T, JdcError<'a>>;

#[derive(Debug)]
pub enum ChannelSendError<'a> {
    SubmitSharesExtended(
        async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ),
    SetNewPrevHash(async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>),
    NewExtendedMiningJob(async_channel::SendError<NewExtendedMiningJob<'a>>),
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
pub enum JdcError<'a> {
    VecToSlice32(Vec<u8>),
    ConfigError(ext_config::ConfigError),
    /// Errors from `binary_sv2` crate.
    BinarySv2(binary_sv2::Error),
    /// Errors on bad noise handshake.
    CodecNoise(codec_sv2::noise_sv2::Error),
    /// Errors from `framing_sv2` crate.
    FramingSv2(framing_sv2::Error),
    /// Errors on bad `TcpStream` connection.
    Io(std::io::Error),
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// Errors from `roles_logic_sv2` crate.
    RolesLogicSv2(roles_logic_sv2::Error),
    UpstreamIncoming(roles_logic_sv2::Error),
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
    Infallible(std::convert::Infallible),
}

impl<'a> fmt::Display for JdcError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use JdcError::*;
        match self {
            ConfigError(e) => write!(f, "Config error: {:?}", e),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            CodecNoise(ref e) => write!(f, "Noise error: `{:?}", e),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{:?}`", e),
            RolesLogicSv2(ref e) => write!(f, "Roles SV2 Logic Error: `{:?}`", e),
            SubprotocolMining(ref e) => write!(f, "Subprotocol Mining Error: `{:?}`", e),
            UpstreamIncoming(ref e) => write!(f, "Upstream parse incoming error: `{:?}`", e),
            PoisonLock => write!(f, "Poison Lock error"),
            ChannelErrorReceiver(ref e) => write!(f, "Channel receive error: `{:?}`", e),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{:?}`", e),
            ChannelErrorSender(ref e) => write!(f, "Channel send error: `{:?}`", e),
            Uint256Conversion(ref e) => write!(f, "U256 Conversion Error: `{:?}`", e),
            VecToSlice32(ref e) => write!(f, "Standard Error: `{:?}`", e),
            Infallible(ref e) => write!(f, "Infallible Error:`{:?}`", e),
        }
    }
}

impl<'a> From<ext_config::ConfigError> for JdcError<'a> {
    fn from(e: ext_config::ConfigError) -> JdcError<'a> {
        JdcError::ConfigError(e)
    }
}

impl<'a> From<binary_sv2::Error> for JdcError<'a> {
    fn from(e: binary_sv2::Error) -> Self {
        JdcError::BinarySv2(e)
    }
}

impl<'a> From<codec_sv2::noise_sv2::Error> for JdcError<'a> {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        JdcError::CodecNoise(e)
    }
}

impl<'a> From<framing_sv2::Error> for JdcError<'a> {
    fn from(e: framing_sv2::Error) -> Self {
        JdcError::FramingSv2(e)
    }
}

impl<'a> From<std::io::Error> for JdcError<'a> {
    fn from(e: std::io::Error) -> Self {
        JdcError::Io(e)
    }
}

impl<'a> From<std::num::ParseIntError> for JdcError<'a> {
    fn from(e: std::num::ParseIntError) -> Self {
        JdcError::ParseInt(e)
    }
}

impl<'a> From<roles_logic_sv2::Error> for JdcError<'a> {
    fn from(e: roles_logic_sv2::Error) -> Self {
        JdcError::RolesLogicSv2(e)
    }
}

impl<'a> From<async_channel::RecvError> for JdcError<'a> {
    fn from(e: async_channel::RecvError) -> Self {
        JdcError::ChannelErrorReceiver(e)
    }
}

impl<'a> From<tokio::sync::broadcast::error::RecvError> for JdcError<'a> {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        JdcError::TokioChannelErrorRecv(e)
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
    for JdcError<'a>
{
    fn from(
        e: async_channel::SendError<roles_logic_sv2::mining_sv2::SubmitSharesExtended<'a>>,
    ) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::SubmitSharesExtended(e))
    }
}

impl<'a> From<async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>>
    for JdcError<'a>
{
    fn from(e: async_channel::SendError<roles_logic_sv2::mining_sv2::SetNewPrevHash<'a>>) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::SetNewPrevHash(e))
    }
}

impl<'a> From<async_channel::SendError<(ExtendedExtranonce, u32)>> for JdcError<'a> {
    fn from(e: async_channel::SendError<(ExtendedExtranonce, u32)>) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::Extranonce(e))
    }
}

impl<'a> From<async_channel::SendError<NewExtendedMiningJob<'a>>> for JdcError<'a> {
    fn from(e: async_channel::SendError<NewExtendedMiningJob<'a>>) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::NewExtendedMiningJob(e))
    }
}

impl<'a> From<async_channel::SendError<SetCustomMiningJob<'a>>> for JdcError<'a> {
    fn from(e: async_channel::SendError<SetCustomMiningJob<'a>>) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::SetCustomMiningJob(e))
    }
}

impl<'a>
    From<
        async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    > for JdcError<'a>
{
    fn from(
        e: async_channel::SendError<(
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'a>,
            Vec<u8>,
        )>,
    ) -> Self {
        JdcError::ChannelErrorSender(ChannelSendError::NewTemplate(e))
    }
}

impl<'a> From<Vec<u8>> for JdcError<'a> {
    fn from(e: Vec<u8>) -> Self {
        JdcError::VecToSlice32(e)
    }
}

impl<'a> From<ParseLengthError> for JdcError<'a> {
    fn from(e: ParseLengthError) -> Self {
        JdcError::Uint256Conversion(e)
    }
}

impl<'a> From<std::convert::Infallible> for JdcError<'a> {
    fn from(e: std::convert::Infallible) -> Self {
        JdcError::Infallible(e)
    }
}
