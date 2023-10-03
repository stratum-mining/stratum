use std::{
    convert::From,
    fmt::Debug,
    sync::{MutexGuard, PoisonError},
};

use roles_logic_sv2::parsers::Mining;

#[derive(std::fmt::Debug)]
pub enum PoolError {
    Io(std::io::Error),
    ChannelSend(Box<dyn std::marker::Send + Debug>),
    ChannelRecv(async_channel::RecvError),
    BinarySv2(binary_sv2::Error),
    Codec(codec_sv2::Error),
    Noise(noise_sv2::Error),
    RolesLogic(roles_logic_sv2::Error),
    Framing(codec_sv2::framing_sv2::Error),
    PoisonLock(String),
    ComponentShutdown(String),
    Custom(String),
    Sv2ProtocolError((u32, Mining<'static>)),
}

impl std::fmt::Display for PoolError {
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
            ComponentShutdown(ref e) => write!(f, "Component shutdown: {:?}", e),
            Custom(ref e) => write!(f, "Custom SV2 error: `{:?}`", e),
            Sv2ProtocolError(ref e) => {
                write!(f, "Received Sv2 Protocol Error from upstream: `{:?}`", e)
            }
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
impl From<codec_sv2::framing_sv2::Error> for PoolError {
    fn from(e: codec_sv2::framing_sv2::Error) -> PoolError {
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
