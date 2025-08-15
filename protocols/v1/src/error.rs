use crate::{
    methods::{Method, MethodError},
    utils::HexU32Be,
};

#[derive(Debug)]
#[non_exhaustive]
pub enum Error<'a> {
    BadBytesConvert(binary_sv2::Error),
    BTCHashError(bitcoin_hashes::Error),
    /// Errors on bad hex decode/encode.
    HexError(hex::FromHexError),
    /// Errors if `ClientStatus` is in an unexpected state when a message is received. For example,
    /// if a `mining.subscribed` is received when the `ClientStatus` is in the `Init` state.
    IncorrectClientStatus(String),
    Infallible(std::convert::Infallible),
    /// Errors if server receives a `json_rpc` request as the server should only receive responses.
    /// TODO: Should update to accommodate miner requesting a difficulty change
    InvalidJsonRpcMessageKind,
    /// Errors if the client receives an invalid message that was intended to be sent from the
    /// client to the server, NOT from the server to the client.
    #[allow(clippy::upper_case_acronyms)]
    InvalidReceiver(Box<Method<'a>>),
    /// Errors if server receives and invalid `mining.submit` from the client.
    InvalidSubmission,
    /// Errors encountered during conversion between valid `json_rpc` messages and SV1 messages.
    Method(Box<MethodError<'a>>),
    /// Errors if action is attempted that requires the client to be authorized, but it is
    /// unauthorized. The client username is given in the error message.
    UnauthorizedClient(String),
    /// Errors if server does not recognize the client's `id`.
    UnknownID(u64),
    InvalidVersionMask(HexU32Be),
    /// Errors when an unexpected or unsupported message/method is called.
    UnexpectedMessage(String),
}

impl std::fmt::Display for Error<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::BadBytesConvert(ref e) => write!(
                f,
                "Bad U256 or B032 conversion (U256 length must be exactly 32 bytes; B032 length must be <= 32 bytes): {e:?}"
            ),
            Error::BTCHashError(ref e) => write!(f, "Bitcoin Hashes Error: `{e:?}`"),
            Error::HexError(ref e) => write!(f, "Bad hex encode/decode: `{e:?}`"),
            Error::IncorrectClientStatus(s) => {
                write!(f, "Client status is incompatible with message: `{s}`")
            }
            Error::Infallible(ref e) => write!(f, "Infallible error{e:?}"),
            Error::InvalidJsonRpcMessageKind => write!(
                f,
                "Server received a `json_rpc` response when it should only receive requests"
            ),
            Error::InvalidReceiver(ref e) => write!(
                f,
                "Client received an invalid message that was intended to be sent from the
            client to the server, NOT from the server to the client. Invalid message: `{e:?}`"
            ),
            Error::InvalidSubmission => {
                write!(f, "Server received an invalid `mining.submit` message.")
            }
            Error::Method(ref e) => {
                write!(
                    f,
                    "Error converting valid `json_rpc` SV1 message: `{e:?}`"
                )
            }
            Error::UnauthorizedClient(id) => write!(
                f,
                "Client with id `{id}` expected to be authorized but is unauthorized."
            ),
            Error::UnknownID(e) => write!(f, "Server did not recognize the client id: `{e}`."),
            Error::InvalidVersionMask(e) => write!(f, "First 3 bits of version rolling mask must be 0 and last 13 bits of version rolling mask must be 0. Version rolling mask is: `{:b}`.", e.0),
            Error::UnexpectedMessage(method) => write!(f, "Unexpected or unsupported message/method called: `{method}`."),
        }
    }
}

impl From<bitcoin_hashes::Error> for Error<'_> {
    fn from(e: bitcoin_hashes::Error) -> Self {
        Error::BTCHashError(e)
    }
}

impl From<hex::FromHexError> for Error<'_> {
    fn from(e: hex::FromHexError) -> Self {
        Error::HexError(e)
    }
}

impl From<std::convert::Infallible> for Error<'_> {
    fn from(e: std::convert::Infallible) -> Self {
        Error::Infallible(e)
    }
}

impl<'a> From<MethodError<'a>> for Error<'a> {
    fn from(inner: MethodError<'a>) -> Self {
        Error::Method(Box::new(inner))
    }
}

impl From<binary_sv2::Error> for Error<'_> {
    fn from(inner: binary_sv2::Error) -> Self {
        Error::BadBytesConvert(inner)
    }
}
