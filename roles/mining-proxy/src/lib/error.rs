use async_channel::SendError;
use codec_sv2::StandardEitherFrame;
use roles_logic_sv2::parsers::PoolMessages;
use std::net::SocketAddr;

pub type Message = PoolMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub type ProxyResult<T> = core::result::Result<T, ProxyError>;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
pub enum ProxyError {
    BadCliArgs,
    BadTomlDeserialize(toml::de::Error),
    Io(std::io::Error),
    SendError(SendError<EitherFrame>),
    UpstreamNotAvailabe(SocketAddr),
    SetupConnectionError(String),
}

impl From<std::io::Error> for ProxyError {
    fn from(e: std::io::Error) -> ProxyError {
        ProxyError::Io(e)
    }
}

impl From<toml::de::Error> for ProxyError {
    fn from(e: toml::de::Error) -> Self {
        ProxyError::BadTomlDeserialize(e)
    }
}

impl From<SendError<EitherFrame>> for ProxyError {
    fn from(error: SendError<EitherFrame>) -> Self {
        ProxyError::SendError(error)
    }
}
