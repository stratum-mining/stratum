//! ## Error Module
//!
//! Defines [`PoolError`], the main error type used across the Pool.
//!
//! Centralizes errors from:
//! - I/O operations
//! - Channel send/receive
//! - SV2 stack: Binary, Codec, Noise, Framing, Roles Logic
//! - Locking (PoisonError)
//!
//! Ensures all errors are easy to pass around, including across async boundaries.

use std::{
    convert::From,
    fmt::Debug,
    sync::{MutexGuard, PoisonError},
};

use roles_logic_sv2::{
    channel_management::{
        extended::factory::error::ExtendedChannelFactoryError,
        standard::factory::error::StandardChannelFactoryError,
    },
    extranonce_prefix_management::error::ExtranoncePrefixFactoryError,
    parsers::Mining,
};

/// Represents various errors that can occur in the pool implementation.
#[derive(std::fmt::Debug)]
pub enum PoolError {
    /// I/O-related error.
    Io(std::io::Error),
    /// Error when sending a message through a channel.
    ChannelSend(Box<dyn std::marker::Send + Debug>),
    /// Error when receiving a message from an asynchronous channel.
    ChannelRecv(async_channel::RecvError),
    /// Error from the `binary_sv2` crate.
    BinarySv2(binary_sv2::Error),
    /// Error from the `codec_sv2` crate.
    Codec(codec_sv2::Error),
    /// Error from the `noise_sv2` crate.
    Noise(noise_sv2::Error),
    /// Error from the `roles_logic_sv2` crate.
    RolesLogic(roles_logic_sv2::Error),
    /// Error related to SV2 message framing.
    Framing(codec_sv2::framing_sv2::Error),
    /// Error due to a poisoned lock, typically from a failed mutex operation.
    PoisonLock(String),
    /// Error indicating that a component has shut down unexpectedly.
    ComponentShutdown(String),
    /// Custom error message.
    Custom(String),
    /// Error related to the SV2 protocol, including an error code and a `Mining` message.
    Sv2ProtocolError((u32, Mining<'static>)),
    ExtranoncePrefixFactoryStandard(ExtranoncePrefixFactoryError),
    ExtranoncePrefixFactoryExtended(ExtranoncePrefixFactoryError),
    StandardChannelFactoryError(StandardChannelFactoryError),
    ExtendedChannelFactoryError(ExtendedChannelFactoryError),
    LastSetNewPrevHashNotFound,
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
            ExtranoncePrefixFactoryStandard(ref e) => {
                write!(f, "Extranonce Prefix Factory Standard error: `{:?}`", e)
            }
            ExtranoncePrefixFactoryExtended(ref e) => {
                write!(f, "Extranonce Prefix Factory Extended error: `{:?}`", e)
            }
            StandardChannelFactoryError(ref e) => {
                write!(f, "Standard Channel Factory error: `{:?}`", e)
            }
            ExtendedChannelFactoryError(ref e) => {
                write!(f, "Extended Channel Factory error: `{:?}`", e)
            }
            LastSetNewPrevHashNotFound => {
                write!(f, "Last SetNewPrevHash Not Found")
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
