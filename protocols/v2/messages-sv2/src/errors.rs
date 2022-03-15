use binary_sv2::Error as BinarySv2Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum Error {
    // TODO: what is this error used for? A general catch all?
    BinarySv2Error(BinarySv2Error),
    WrongMessageType(u8),
    UnexpectedMessage,
    // min_v max_v all falgs supported
    NoPairableUpstream((u16, u16, u32)),
    /// Error if the hashmap `future_jobs` field in the `GroupChannelJobDispatcher` is empty.
    NoFutureJobs,
}

impl From<BinarySv2Error> for Error {
    fn from(v: BinarySv2Error) -> Error {
        Error::BinarySv2Error(v)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(v) => write!(f, "BinarySv2Error: TODO better description: {:?}", v),
            WrongMessageType(m) => write!(f, "Wrong message type: {}", m),
            UnexpectedMessage => write!(f, "Error: Unexpected message received"),
            NoPairableUpstream(a) => {
                write!(f, "No pairable upstream node: {:?}", a)
            }
            NoFutureJobs => write!(f, "GroupChannelJobDispatcher does not have any future jobs"),
        }
    }
}
