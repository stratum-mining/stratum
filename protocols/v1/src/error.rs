use crate::methods::MethodError;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Method(MethodError),
    InvalidJsonRpcMessageKind,
    InvalidSubmission,
    UnknownID,
}

impl From<MethodError> for Error {
    fn from(inner: MethodError) -> Self {
        Error::Method(inner)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, _f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            _ => todo!(),
        }
    }
}
