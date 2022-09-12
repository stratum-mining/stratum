use std::fmt;

pub type ProxyResult<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    BadSerdeJson(serde_json::Error),
    BadSv1StdReq,
    // NoTranslationRequired,
    V1ProtocolError(v1::error::Error),
    // InvalidJsonRpcMessageKind(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            BadSv1StdReq => write!(f, "Bad SV1 Standard Request"),
            V1ProtocolError(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
        }
    }
}

impl From<v1::error::Error> for Error {
    fn from(e: v1::error::Error) -> Self {
        Error::V1ProtocolError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}
