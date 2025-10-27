use std::{
    convert::From,
    fmt::Debug,
    sync::{MutexGuard, PoisonError},
};

use stratum_apps::stratum_core::{
    binary_sv2, bitcoin,
    channels_sv2::{
        server::{
            error::{ExtendedChannelError, GroupChannelError, StandardChannelError},
            share_accounting::ShareValidationError,
        },
        vardiff::error::VardiffError,
    },
    codec_sv2, framing_sv2,
    handlers_sv2::HandlerErrorType,
    mining_sv2::ExtendedExtranonceError,
    noise_sv2,
    parsers_sv2::{Mining, ParserError},
};

pub type PoolResult<T> = Result<T, PoolError>;

#[derive(Debug)]
pub enum ChannelSv2Error {
    ExtendedChannelServerSide(ExtendedChannelError),
    StandardChannelServerSide(StandardChannelError),
    GroupChannelServerSide(GroupChannelError),
    ExtranonceError(ExtendedExtranonceError),
    ShareValidationError(ShareValidationError),
}

/// Represents various errors that can occur in the pool implementation.
#[derive(std::fmt::Debug)]
pub enum PoolError {
    /// I/O-related error.
    Io(std::io::Error),
    ChannelSv2(ChannelSv2Error),
    /// Error when sending a message through a channel.
    ChannelSend(Box<dyn std::marker::Send + Debug>),
    /// Error when receiving a message from an asynchronous channel.
    ChannelRecv(async_channel::RecvError),
    /// Error from the `binary_sv2` crate.
    BinarySv2(binary_sv2::Error),
    /// Error from the `codec_sv2` crate.
    Codec(codec_sv2::Error),
    /// Error related to parsing a coinbase output specification.
    CoinbaseOutput(stratum_apps::config_helpers::CoinbaseOutputError),
    /// Error from the `noise_sv2` crate.
    Noise(noise_sv2::Error),
    /// Error related to SV2 message framing.
    Framing(framing_sv2::Error),
    /// Error due to a poisoned lock, typically from a failed mutex operation.
    PoisonLock(String),
    /// Error indicating that a component has shut down unexpectedly.
    ComponentShutdown(String),
    /// Custom error message.
    Custom(String),
    /// Error related to the SV2 protocol, including an error code and a `Mining` message.
    Sv2ProtocolError((u32, Mining<'static>)),
    /// Vardiff Error
    Vardiff(VardiffError),
    /// Parser Error
    Parser(ParserError),
    /// Shutdown
    Shutdown,
    /// Unexpected message
    UnexpectedMessage(u8),
    /// Channel error sender
    ChannelErrorSender,
    /// Invalid socket address
    InvalidSocketAddress(String),
    /// Bitcoin Encode Error
    BitcoinEncodeError(bitcoin::consensus::encode::Error),
    /// Downstream not found for the channel
    DownstreamNotFoundWithChannelId(u32),
    /// Downstream not found
    DownstreamNotFound(usize),
    /// Downstream Id not found
    DownstreamIdNotFound,
    /// Future template not present
    FutureTemplateNotPresent,
    /// Last new prevhash not found
    LastNewPrevhashNotFound,
    /// Vardiff associated to channel not found
    VardiffNotFound(u32),
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// Failed to create group channel
    FailedToCreateGroupChannel(GroupChannelError),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PoolError::*;
        match self {
            Io(e) => write!(f, "I/O error: `{e:?}"),
            ChannelSend(e) => write!(f, "Channel send failed: `{e:?}`"),
            ChannelRecv(e) => write!(f, "Channel recv failed: `{e:?}`"),
            BinarySv2(e) => write!(f, "Binary SV2 error: `{e:?}`"),
            Codec(e) => write!(f, "Codec SV2 error: `{e:?}"),
            CoinbaseOutput(e) => write!(f, "Coinbase output error: `{e:?}"),
            Framing(e) => write!(f, "Framing SV2 error: `{e:?}`"),
            Noise(e) => write!(f, "Noise SV2 error: `{e:?}"),
            PoisonLock(e) => write!(f, "Poison lock: {e:?}"),
            ComponentShutdown(e) => write!(f, "Component shutdown: {e:?}"),
            Custom(e) => write!(f, "Custom SV2 error: `{e:?}`"),
            Sv2ProtocolError(e) => {
                write!(f, "Received Sv2 Protocol Error from upstream: `{e:?}`")
            }
            PoolError::Vardiff(e) => {
                write!(f, "Received Vardiff Error : {e:?}")
            }
            Parser(e) => write!(f, "Parser error: `{e:?}`"),
            Shutdown => write!(f, "Shutdown"),
            UnexpectedMessage(message_type) => write!(f, "message type: {message_type:?}"),
            ChannelErrorSender => write!(f, "Channel sender error"),
            InvalidSocketAddress(address) => write!(f, "Invalid socket address: {address:?}"),
            BitcoinEncodeError(_) => write!(f, "Error generated during encoding"),
            DownstreamNotFoundWithChannelId(channel_id) => {
                write!(f, "Downstream not found for channel id: {channel_id}")
            }
            DownstreamNotFound(downstream_id) => write!(
                f,
                "Downstream not found with downstream id: {downstream_id}"
            ),
            DownstreamIdNotFound => write!(f, "Downstream id not found"),
            FutureTemplateNotPresent => write!(f, "future template not present"),
            LastNewPrevhashNotFound => write!(f, "last prev hash not present"),
            VardiffNotFound(downstream_id) => write!(
                f,
                "Vardiff not found available for downstream id: {downstream_id}"
            ),
            ParseInt(e) => write!(f, "Conversion error: {e:?}"),
            ChannelSv2(channel_error) => {
                write!(f, "Channel error: {channel_error:?}")
            }
            FailedToCreateGroupChannel(ref e) => {
                write!(f, "Failed to create group channel: {e:?}")
            }
        }
    }
}

impl From<std::io::Error> for PoolError {
    fn from(e: std::io::Error) -> PoolError {
        PoolError::Io(e)
    }
}

impl From<async_channel::RecvError> for PoolError {
    fn from(e: async_channel::RecvError) -> PoolError {
        PoolError::ChannelRecv(e)
    }
}

impl From<binary_sv2::Error> for PoolError {
    fn from(e: binary_sv2::Error) -> PoolError {
        PoolError::BinarySv2(e)
    }
}

impl From<codec_sv2::Error> for PoolError {
    fn from(e: codec_sv2::Error) -> PoolError {
        PoolError::Codec(e)
    }
}

impl From<stratum_apps::config_helpers::CoinbaseOutputError> for PoolError {
    fn from(e: stratum_apps::config_helpers::CoinbaseOutputError) -> PoolError {
        PoolError::CoinbaseOutput(e)
    }
}

impl From<noise_sv2::Error> for PoolError {
    fn from(e: noise_sv2::Error) -> PoolError {
        PoolError::Noise(e)
    }
}

impl<T: 'static + std::marker::Send + Debug> From<async_channel::SendError<T>> for PoolError {
    fn from(e: async_channel::SendError<T>) -> PoolError {
        PoolError::ChannelSend(Box::new(e))
    }
}

impl From<String> for PoolError {
    fn from(e: String) -> PoolError {
        PoolError::Custom(e)
    }
}
impl From<framing_sv2::Error> for PoolError {
    fn from(e: framing_sv2::Error) -> PoolError {
        PoolError::Framing(e)
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for PoolError {
    fn from(e: PoisonError<MutexGuard<T>>) -> PoolError {
        PoolError::PoisonLock(e.to_string())
    }
}

impl From<(u32, Mining<'static>)> for PoolError {
    fn from(e: (u32, Mining<'static>)) -> Self {
        PoolError::Sv2ProtocolError(e)
    }
}

impl HandlerErrorType for PoolError {
    fn parse_error(error: ParserError) -> Self {
        PoolError::Parser(error)
    }

    fn unexpected_message(message_type: u8) -> Self {
        PoolError::UnexpectedMessage(message_type)
    }
}

impl From<stratum_apps::stratum_core::bitcoin::consensus::encode::Error> for PoolError {
    fn from(value: stratum_apps::stratum_core::bitcoin::consensus::encode::Error) -> Self {
        PoolError::BitcoinEncodeError(value)
    }
}

impl From<ExtendedChannelError> for PoolError {
    fn from(value: ExtendedChannelError) -> Self {
        PoolError::ChannelSv2(ChannelSv2Error::ExtendedChannelServerSide(value))
    }
}

impl From<StandardChannelError> for PoolError {
    fn from(value: StandardChannelError) -> Self {
        PoolError::ChannelSv2(ChannelSv2Error::StandardChannelServerSide(value))
    }
}

impl From<GroupChannelError> for PoolError {
    fn from(value: GroupChannelError) -> Self {
        PoolError::ChannelSv2(ChannelSv2Error::GroupChannelServerSide(value))
    }
}

impl From<ExtendedExtranonceError> for PoolError {
    fn from(value: ExtendedExtranonceError) -> Self {
        PoolError::ChannelSv2(ChannelSv2Error::ExtranonceError(value))
    }
}

impl From<VardiffError> for PoolError {
    fn from(value: VardiffError) -> Self {
        PoolError::Vardiff(value)
    }
}

impl From<ParserError> for PoolError {
    fn from(value: ParserError) -> Self {
        PoolError::Parser(value)
    }
}

impl From<ShareValidationError> for PoolError {
    fn from(value: ShareValidationError) -> Self {
        PoolError::ChannelSv2(ChannelSv2Error::ShareValidationError(value))
    }
}
