// use crate::framing2::EitherFrame;
use core::fmt;

// pub type FramingResult<T> = core::result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BinarySv2Error(binary_sv2::Error),
    ExpectedHandshakeFrame,
    ExpectedSv2Frame,
    UnexpectedHeaderLength(isize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(ref e) => {
                write!(f, "BinarySv2Error: `{:?}`", e)
            }
            ExpectedHandshakeFrame => {
                write!(f, "Expected `HandshakeFrame`, received `Sv2Frame`")
            }
            ExpectedSv2Frame => {
                write!(f, "Expected `Sv2Frame`, received `HandshakeFrame`")
            }
            UnexpectedHeaderLength(i) => {
                write!(f, "Unexpected `Header` length: `{}`", i)
            }
        }
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}
