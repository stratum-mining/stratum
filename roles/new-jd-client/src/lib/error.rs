//! ## Error Module
//!
//! Defines [`Error`], the central error enum used throughout the Job Declarator Client (JDC).
//!
//! It unifies errors from:
//! - I/O operations
//! - Channels (send/recv)
//! - SV2 stack: Binary, Codec, Noise, Framing, RolesLogic
//! - Locking logic (PoisonError)
//! - Domain-specific issues
//!
//! This module ensures that all errors can be passed around consistently, including across async
//! boundaries.
use ext_config::ConfigError;
use std::fmt;
use stratum_common::{
    network_helpers_sv2,
    roles_logic_sv2::{
        self, bitcoin,
        codec_sv2::{self, binary_sv2, framing_sv2},
        handlers_sv2::HandlerError,
        mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob},
        parsers_sv2::ParserError,
    },
};
use tokio::sync::broadcast;

#[derive(Debug)]
pub enum JDCError {
    #[allow(dead_code)]
    VecToSlice32(Vec<u8>),
    /// Errors on bad CLI argument input.
    BadCliArgs,
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
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// Errors from `roles_logic_sv2` crate.
    RolesSv2Logic(roles_logic_sv2::errors::Error),
    UpstreamIncoming(roles_logic_sv2::errors::Error),
    #[allow(dead_code)]
    SubprotocolMining(String),
    // Locking Errors
    PoisonLock,
    TokioChannelErrorRecv(tokio::sync::broadcast::error::RecvError),
    Infallible(std::convert::Infallible),
    Parser(ParserError),
    /// Channel receiver error
    ChannelErrorReceiver(async_channel::RecvError),
    /// Channel sender error
    ChannelErrorSender,
    /// Broadcast channel receiver error
    BroadcastChannelErrorReceiver(broadcast::error::RecvError),
    Shutdown,
    NetworkHelpersError(stratum_common::network_helpers_sv2::Error),
    HandlerError(HandlerError),
    UnexpectedMessage,
    InvalidUserIdentity(String),
    BitcoinEncodeError(bitcoin::consensus::encode::Error),
    InvalidSocketAddress(String),
}

impl std::error::Error for JDCError {}

impl fmt::Display for JDCError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use JDCError::*;
        match self {
            BadCliArgs => write!(f, "Bad CLI arg input"),
            BadConfigDeserialize(ref e) => write!(f, "Bad `config` TOML deserialize: `{e:?}`"),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{e:?}`"),
            CodecNoise(ref e) => write!(f, "Noise error: `{e:?}"),
            FramingSv2(ref e) => write!(f, "Framing SV2 error: `{e:?}`"),
            Io(ref e) => write!(f, "I/O error: `{e:?}"),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{e:?}`"),
            RolesSv2Logic(ref e) => write!(f, "Roles SV2 Logic Error: `{e:?}`"),
            SubprotocolMining(ref e) => write!(f, "Subprotocol Mining Error: `{e:?}`"),
            UpstreamIncoming(ref e) => write!(f, "Upstream parse incoming error: `{e:?}`"),
            PoisonLock => write!(f, "Poison Lock error"),
            ChannelErrorReceiver(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            TokioChannelErrorRecv(ref e) => write!(f, "Channel receive error: `{e:?}`"),
            VecToSlice32(ref e) => write!(f, "Standard Error: `{e:?}`"),
            Infallible(ref e) => write!(f, "Infallible Error:`{e:?}`"),
            Parser(ref e) => write!(f, "Parser error: `{e:?}`"),
            BroadcastChannelErrorReceiver(ref e) => {
                write!(f, "Broadcast channel receive error: {e:?}")
            }
            ChannelErrorSender => write!(f, "Sender error"),
            Shutdown => write!(f, "Shutdown"),
            NetworkHelpersError(ref e) => write!(f, "Network error: {e:?}"),
            HandlerError(ref e) => write!(f, "Error generated from handler: {e:?}"),
            UnexpectedMessage => write!(f, "Unexpected Message"),
            InvalidUserIdentity(ref s) => write!(f, "User ID is invalid"),
            BitcoinEncodeError(ref e) => write!(f, "Error generated during encoding"),
            InvalidSocketAddress(ref s) => write!(f, "Invalid socket address: {s}"),
        }
    }
}

impl From<HandlerError> for JDCError {
    fn from(value: HandlerError) -> Self {
        JDCError::HandlerError(value)
    }
}

impl From<ParserError> for JDCError {
    fn from(e: ParserError) -> Self {
        JDCError::Parser(e)
    }
}

impl From<binary_sv2::Error> for JDCError {
    fn from(e: binary_sv2::Error) -> Self {
        JDCError::BinarySv2(e)
    }
}

impl From<codec_sv2::noise_sv2::Error> for JDCError {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        JDCError::CodecNoise(e)
    }
}

impl From<framing_sv2::Error> for JDCError {
    fn from(e: framing_sv2::Error) -> Self {
        JDCError::FramingSv2(e)
    }
}

impl From<std::io::Error> for JDCError {
    fn from(e: std::io::Error) -> Self {
        JDCError::Io(e)
    }
}

impl From<std::num::ParseIntError> for JDCError {
    fn from(e: std::num::ParseIntError) -> Self {
        JDCError::ParseInt(e)
    }
}

impl From<roles_logic_sv2::errors::Error> for JDCError {
    fn from(e: roles_logic_sv2::errors::Error) -> Self {
        JDCError::RolesSv2Logic(e)
    }
}

impl From<ConfigError> for JDCError {
    fn from(e: ConfigError) -> Self {
        JDCError::BadConfigDeserialize(e)
    }
}

impl From<async_channel::RecvError> for JDCError {
    fn from(e: async_channel::RecvError) -> Self {
        JDCError::ChannelErrorReceiver(e)
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for JDCError {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        JDCError::TokioChannelErrorRecv(e)
    }
}

impl From<network_helpers_sv2::Error> for JDCError {
    fn from(value: network_helpers_sv2::Error) -> Self {
        JDCError::NetworkHelpersError(value)
    }
}

impl From<stratum_common::roles_logic_sv2::bitcoin::consensus::encode::Error> for JDCError {
    fn from(value: stratum_common::roles_logic_sv2::bitcoin::consensus::encode::Error) -> Self {
        JDCError::BitcoinEncodeError(value)
    }
}
