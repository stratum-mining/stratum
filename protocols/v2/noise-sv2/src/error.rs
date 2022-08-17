use core::fmt;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    /// Errors from the `binary_sv2` crate
    BinarySv2Error(binary_sv2::Error),
    DalekError(ed25519_dalek::ed25519::Error),
    SnowError(snow::Error),
    /// Errors if handshake initiator step is invalid. Valid steps are 0, 1, or 2.
    HSInitiatorStepNotFound(usize),
    /// Errors if handshake responder step is invalid. Valid steps are 0, 1, or 2.
    HSResponderStepNotFound(usize),
    /// Catch all
    NoiseTodo,
}
pub type Result<T> = core::result::Result<T, Error>;

//impl From<core::io::Error> for Error {
//    fn from(_: core::io::Error) -> Self {
//        Error {}
//    }
//}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{:?}`", e),
            DalekError(e) => write!(f, "ed25519 Dalek Error: `{:?}`", e),
            SnowError(e) => write!(f, "Snow Error: `{:?}`", e),
            HSInitiatorStepNotFound(u) => write!(
                f,
                "Invalid handshake initiator step: `{}`. Valid steps are 0, 1, or 2.",
                u
            ),
            HSResponderStepNotFound(u) => write!(
                f,
                "Invalid handshake responder step: `{}`. Valid steps are 0, 1, or 2.",
                u
            ),
            NoiseTodo => write!(f, "Noise Sv2 Error: TODO"),
        }
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::NoiseTodo
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}

impl From<ed25519_dalek::ed25519::Error> for Error {
    fn from(e: ed25519_dalek::ed25519::Error) -> Self {
        Error::DalekError(e)
    }
}

impl From<snow::Error> for Error {
    fn from(e: snow::Error) -> Self {
        Error::SnowError(e)
    }
}
