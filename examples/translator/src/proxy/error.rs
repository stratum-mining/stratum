use std::fmt;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    BadSv1StandardRequest(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadSv1StandardRequest(s) => write!(f, "Bad SV1 Standard Request: `{}`", s),
        }
    }
}
