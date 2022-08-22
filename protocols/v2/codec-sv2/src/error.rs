#[cfg(test)]
use core::cmp;
use core::fmt;
#[cfg(feature = "noise_sv2")]
use noise_sv2::Error as NoiseError;
#[cfg(feature = "noise_sv2")]
use noise_sv2::NoiseSv2SnowError;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    /// Errors from the `binary_sv2` crate
    BinarySv2Error(binary_sv2::Error),
    /// Errors if there are missing bytes in the Noise protocol
    MissingBytes(usize),
    /// Errors from the `noise_sv2` crate
    #[cfg(feature = "noise_sv2")]
    NoiseSv2Error(NoiseError),
    /// `snow` errors
    #[cfg(feature = "noise_sv2")]
    SnowError(NoiseSv2SnowError),
    /// Error if Noise protocol state is not as expected
    UnexpectedNoiseState,
    CodecTodo,
}

pub type Result<T> = core::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{:?}`", e),
            MissingBytes(u) => write!(f, "Missing `{}` Noise bytes", u),
            #[cfg(feature = "noise_sv2")]
            NoiseSv2Error(e) => write!(f, "Noise SV2 Error: `{:?}`", e),
            #[cfg(feature = "noise_sv2")]
            SnowError(e) => write!(f, "Snow Error: `{:?}`", e),
            UnexpectedNoiseState => {
                write!(f, "Noise state is incorrect")
            }
            CodecTodo => write!(f, "Codec Sv2 Error: TODO"),
        }
    }
}

#[cfg(test)]
impl cmp::PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        use Error::*;
        match (self, other) {
            (BinarySv2Error(_), BinarySv2Error(_)) => true,
            (MissingBytes(_), MissingBytes(_)) => true,
            (NoiseSv2Error(_), NoiseSv2Error(_)) => true,
            (SnowError(_), SnowError(_)) => true,
            (UnexpectedNoiseState, UnexpectedNoiseState) => true,
            (CodecTodo, CodecTodo) => true,
            _ => false,
        }
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseError> for Error {
    fn from(e: NoiseError) -> Self {
        Error::NoiseSv2Error(e)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseSv2SnowError> for Error {
    fn from(e: NoiseSv2SnowError) -> Self {
        Error::SnowError(e)
    }
}
