use core::fmt;

#[repr(C)]
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum Error {
    /// Error if Noise protocol state is not as expected
    UnexpectedNoiseState,
    /// Errors if there are missing bytes in the Noise protocol
    MissingBytes(usize),
    NoiseSv2Error(noise_sv2::Error),
    CodecTodo,
    /// Catch all
    CodecCatchAll,
}

pub type Result<T> = core::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            UnexpectedNoiseState => {
                write!(f, "Noise state is incorrect")
            }
            MissingBytes(u) => write!(f, "Missing `{}` Noise bytes", u),
            NoiseSv2Error(e) => write!(f, "Noise SV2 Error: `{:?}`", e),
            CodecTodo => write!(f, "Codec Sv2 Error: TODO"),
            CodecCatchAll => write!(f, "Codec Sv2 Error: CATCH ALL"),
        }
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::CodecCatchAll
    }
}

impl From<noise_sv2::Error> for Error {
    fn from(e: noise_sv2::Error) -> Self {
        Error::NoiseSv2Error(e)
    }
}
