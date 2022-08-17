use core::fmt;
#[cfg(feature = "noise_sv2")]
use noise_sv2::Error as NoiseError;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    /// Error if Noise protocol state is not as expected
    UnexpectedNoiseState,
    /// Errors if there are missing bytes in the Noise protocol
    MissingBytes(usize),
    #[cfg(feature = "noise_sv2")]
    NoiseSv2Error(NoiseError),
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
            #[cfg(feature = "noise_sv2")]
            NoiseSv2Error(e) => write!(f, "Noise SV2 Error: `{:?}`", e),
            CodecCatchAll => write!(f, "Codec Sv2 Error: CATCH ALL"),
        }
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::CodecCatchAll
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseError> for Error {
    fn from(e: NoiseError) -> Self {
        Error::NoiseSv2Error(e)
    }
}
