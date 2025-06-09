use async_channel::SendError;
use network_helpers_sv2::roles_logic_sv2::{codec_sv2::StandardEitherFrame, parsers::AnyMessage};
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
}

impl From<SendError<EitherFrame>> for Error {
    fn from(error: SendError<EitherFrame>) -> Self {
        Error::SendError(error)
    }
}
