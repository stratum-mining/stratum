use std::fmt;
use std::future::Future;
use std::pin::Pin;

pub type ProxyResult<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    error: Box<dyn std::error::Error + Send + Sync>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    // BadSv1StandardRequest(String),
    BadSv1StdReq,
    // V1Error(v1::error::Error),
    // InvalidJsonRpcMessageKind(String),
}

impl Error {
    pub fn bad_sv1_std_req<E>(error: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self {
            kind: ErrorKind::BadSv1StdReq,
            error: error.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Converts the error into the underlying error.
    pub fn into_inner(self) -> Box<dyn std::error::Error + Send + Sync> {
        self.error
    }
}

// impl fmt::Display for Error {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         use Error::*;
//         match self {
//             BadSv1StandardRequest(s) => write!(f, "Bad SV1 Standard Request: `{}`", s),
//             V1Error(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
//             InvalidJsonRpcMessageKind(s) => write!(f, "INVALID: {}", s),
//         }
//     }
// }
//
// impl From<v1::error::Error> for Error {
//     fn from(e: v1::error::Error) -> Self {
//         Error::V1Error(e)
//     }
// }
