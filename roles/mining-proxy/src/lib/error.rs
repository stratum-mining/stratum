use codec_sv2::StandardEitherFrame;
use roles_logic_sv2::parsers::PoolMessages;
use std::net::SocketAddr;

pub type Message = PoolMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
#[allow(dead_code)]
pub enum Error {
    SendError(tokio::sync::broadcast::error::SendError<EitherFrame>),
    UpstreamNotAvailabe(SocketAddr),
    SetupConnectionError(String),
}

impl From<tokio::sync::broadcast::error::SendError<EitherFrame>> for Error {
    fn from(error: tokio::sync::broadcast::error::SendError<EitherFrame>) -> Self {
        Error::SendError(error)
    }
}
