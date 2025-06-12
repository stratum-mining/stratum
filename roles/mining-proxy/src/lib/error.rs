use async_channel::SendError;
use codec_sv2::StandardEitherFrame;
use core::fmt;
use roles_logic_sv2::parsers::AnyMessage;
use std::net::SocketAddr;

pub type Message = AnyMessage<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
#[allow(dead_code)]
pub enum Error {
    SendError(SendError<EitherFrame>),
    UpstreamNotAvailabe(SocketAddr),
    SetupConnectionError(String),
    BadCliArgs,
}

impl From<SendError<EitherFrame>> for Error {
    fn from(error: SendError<EitherFrame>) -> Self {
        Error::SendError(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SendError(e) => write!(f, "Send error: {}", e),
            Error::UpstreamNotAvailabe(addr) => write!(f, "Upstream not available: {}", addr),
            Error::SetupConnectionError(msg) => write!(f, "Setup connection error: {}", msg),
            Error::BadCliArgs => write!(f, "Bad CLI arguments provided"),
        }
    }
}
