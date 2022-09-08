use crate::methods::{Method, MethodError};

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Errors if `ClientStatus` is in an unexpected state when a message is received. For example,
    /// if a `mining.subscribed` is received when the `ClientStatus` is in the `Init` state.
    IncorrectClientStatus(String),
    Infalliable(std::convert::Infallible),
    /// Errors if server receives a `json_rpc` request as the server should only receive responses.
    /// TODO: Should update to accommodate miner requesting a difficulty change
    InvalidJsonRpcMessageKind,
    /// Errors if the client receives an invalid message that was intended to be sent from the
    /// client to the server, NOT from the server to the client.
    #[allow(clippy::upper_case_acronyms)]
    InvalidReceiver(Method),
    /// Errors if server receives and invalid `mining.submit` from the client.
    InvalidSubmission,
    /// Errors encountered during conversion between valid `json_rpc` messages and SV1 messages.
    Method(MethodError),
    /// Errors if server does not recognize the client's `id`.
    UnknownID(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::IncorrectClientStatus(s) => {
                write!(f, "Client status is incompatible with message: `{}`", s)
            }
            Error::Infalliable(ref e) => write!(f, "{:?}", e),
            Error::InvalidJsonRpcMessageKind => write!(
                f,
                "Server received a `json_rpc` response when it should only receive requests"
            ),
            Error::InvalidReceiver(ref e) => write!(
                f,
                "Client received an invalid message that was intended to be sent from the
            client to the server, NOT from the server to the client. Invalid message: `{:?}`",
                e
            ),
            Error::InvalidSubmission => {
                write!(f, "Server received an invalid `mining.submit` message.")
            }
            Error::Method(ref e) => {
                write!(
                    f,
                    "Error converting valid `json_rpc` SV1 message: `{:?}`",
                    e
                )
            }
            Error::UnknownID(e) => write!(f, "Server did not recognize the client id: `{}`.", e),
        }
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(e: std::convert::Infallible) -> Self {
        Error::Infalliable(e)
    }
}

impl From<MethodError> for Error {
    fn from(inner: MethodError) -> Self {
        Error::Method(inner)
    }
}
