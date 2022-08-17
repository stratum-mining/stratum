use core::fmt;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    /// Errors from the `binary_sv2` crate
    BinarySv2Error(binary_sv2::Error),
    /// Catch all
    Todo,
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
            Todo => write!(f, "Noise Sv2 Error: TODO"),
        }
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::Todo
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}
