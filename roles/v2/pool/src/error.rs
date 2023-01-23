use codec_sv2::StandardEitherFrame;
use roles_logic_sv2::channel_logic::channel_factory::PoolChannelFactory;
use roles_logic_sv2::parsers::PoolMessages;
use std::convert::From;
use std::sync::{MutexGuard, PoisonError};

use crate::lib::mining_pool::{setup_connection::SetupConnectionHandler, Downstream, Pool};

pub type Message = PoolMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(std::fmt::Debug)]
pub enum ChannelSendVariant {
    Frame(async_channel::SendError<EitherFrame>),
    NewTemplate(
        async_channel::SendError<roles_logic_sv2::template_distribution_sv2::NewTemplate<'static>>,
    ),
    SetNewPrevHash(
        async_channel::SendError<
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
        >,
    ),
    Fn(async_channel::SendError<()>),
}

#[derive(std::fmt::Debug)]
pub enum PoolError {
    Io(std::io::Error),
    ChannelSend(Box<ChannelSendVariant>),
    ChannelRecv(async_channel::RecvError),
    BinarySv2(binary_sv2::Error),
    Codec(codec_sv2::Error),
    Noise(noise_sv2::Error),
    RolesLogic(roles_logic_sv2::Error),
    Framing(String),
    PoisonLock(String),
}

impl<'a> std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PoolError::*;
        match self {
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ChannelSend(ref e) => write!(f, "Channel send failed: `{:?}`", e),
            ChannelRecv(ref e) => write!(f, "Channel recv failed: `{:?}`", e),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            Codec(ref e) => write!(f, "Codec SV2 error: `{:?}", e),
            Framing(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            Noise(ref e) => write!(f, "Noise SV2 error: `{:?}", e),
            RolesLogic(ref e) => write!(f, "Roles Logic SV2 error: `{:?}`", e),
            PoisonLock(ref e) => write!(f, "Poison lock: {:?}", e),
        }
    }
}

pub type PoolResult<T> = Result<T, PoolError>;

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

impl From<noise_sv2::Error> for PoolError {
    fn from(e: noise_sv2::Error) -> PoolError {
        PoolError::Noise(e)
    }
}

impl From<roles_logic_sv2::Error> for PoolError {
    fn from(e: roles_logic_sv2::Error) -> PoolError {
        PoolError::RolesLogic(e)
    }
}

impl From<async_channel::SendError<EitherFrame>> for PoolError {
    fn from(e: async_channel::SendError<EitherFrame>) -> PoolError {
        PoolError::ChannelSend(Box::new(ChannelSendVariant::Frame(e)))
    }
}

impl From<async_channel::SendError<()>> for PoolError {
    fn from(e: async_channel::SendError<()>) -> PoolError {
        PoolError::ChannelSend(Box::new(ChannelSendVariant::Fn(e)))
    }
}

impl
    From<async_channel::SendError<roles_logic_sv2::template_distribution_sv2::NewTemplate<'static>>>
    for PoolError
{
    fn from(
        e: async_channel::SendError<
            roles_logic_sv2::template_distribution_sv2::NewTemplate<'static>,
        >,
    ) -> PoolError {
        PoolError::ChannelSend(Box::new(ChannelSendVariant::NewTemplate(e)))
    }
}

impl
    From<
        async_channel::SendError<
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
        >,
    > for PoolError
{
    fn from(
        e: async_channel::SendError<
            roles_logic_sv2::template_distribution_sv2::SetNewPrevHash<'static>,
        >,
    ) -> PoolError {
        PoolError::ChannelSend(Box::new(ChannelSendVariant::SetNewPrevHash(e)))
    }
}

impl From<String> for PoolError {
    fn from(e: String) -> PoolError {
        PoolError::Framing(e)
    }
}

impl From<PoisonError<MutexGuard<'_, PoolChannelFactory>>> for PoolError {
    fn from(e: PoisonError<MutexGuard<PoolChannelFactory>>) -> PoolError {
        PoolError::PoisonLock(e.to_string())
    }
}

impl From<PoisonError<MutexGuard<'_, Downstream>>> for PoolError {
    fn from(e: PoisonError<MutexGuard<Downstream>>) -> PoolError {
        PoolError::PoisonLock(e.to_string())
    }
}

impl From<PoisonError<MutexGuard<'_, Pool>>> for PoolError {
    fn from(e: PoisonError<MutexGuard<Pool>>) -> PoolError {
        PoolError::PoisonLock(e.to_string())
    }
}

impl From<PoisonError<MutexGuard<'_, SetupConnectionHandler>>> for PoolError {
    fn from(e: PoisonError<MutexGuard<SetupConnectionHandler>>) -> PoolError {
        PoolError::PoisonLock(e.to_string())
    }
}
