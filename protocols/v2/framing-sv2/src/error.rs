// use crate::framing2::EitherFrame;
use core::fmt;

// pub type FramingResult<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    ExpectedHandshakeFrame,
    ExpectedSv2Frame,
    Todo,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            ExpectedHandshakeFrame => {
                write!(f, "Expected `HandshakeFrame`, received `Sv2Frame`")
            }
            ExpectedSv2Frame => {
                write!(f, "Expected `Sv2Frame`, received `HandshakeFrame`")
            }
            Todo => write!(f, "framing_sv2 error TODO"),
        }
    }
}
