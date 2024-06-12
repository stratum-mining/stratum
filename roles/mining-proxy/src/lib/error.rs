use async_channel::SendError;
use codec_sv2::StandardFrame;
use roles_logic_sv2::parsers::PoolMessages;
use std::net::SocketAddr;

pub type Message = PoolMessages<'static>;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    SendError(SendError<StandardFrame<Message>>),
    UpstreamNotAvailabe(SocketAddr),
    SetupConnectionError(String),
    NoPayloadFound,
}

impl From<SendError<StandardFrame<Message>>> for Error {
    fn from(error: SendError<StandardFrame<Message>>) -> Self {
        Error::SendError(error)
    }
}
