#[cfg(test)]
use core::cmp;
use core::fmt;
#[cfg(feature = "noise_sv2")]
use noise_sv2::Error as NoiseError;
#[cfg(feature = "noise_sv2")]
use noise_sv2::NoiseSv2SnowError;

pub type Result<T> = core::result::Result<T, Error>;

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
            (MissingBytes(a), MissingBytes(b)) => a == b,
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

#[repr(C)]
#[derive(Debug)]
pub enum CError {
    /// Errors from the `binary_sv2` crate
    BinarySv2Error,
    /// Errors if there are missing bytes in the Noise protocol
    MissingBytes(usize),
    /// Errors from the `noise_sv2` crate
    NoiseSv2Error,
    /// `snow` errors
    SnowError,
    /// Error if Noise protocol state is not as expected
    UnexpectedNoiseState,
    CodecTodo,
}

/// Here only to force cbindgen to create header for CError
#[no_mangle]
pub extern "C" fn export_cerror() -> CError {
    unimplemented!()
}

impl From<Error> for CError {
    fn from(e: Error) -> CError {
        match e {
            Error::BinarySv2Error(_) => CError::BinarySv2Error,
            Error::MissingBytes(u) => CError::MissingBytes(u),
            #[cfg(feature = "noise_sv2")]
            Error::NoiseSv2Error(_) => CError::NoiseSv2Error,
            #[cfg(feature = "noise_sv2")]
            Error::SnowError(_) => CError::SnowError,
            Error::UnexpectedNoiseState => CError::UnexpectedNoiseState,
            Error::CodecTodo => CError::CodecTodo,
        }
    }
}

impl Drop for CError {
    fn drop(&mut self) {
        match self {
            CError::BinarySv2Error => (),
            CError::MissingBytes(_) => (),
            CError::NoiseSv2Error => (),
            CError::SnowError => (),
            CError::UnexpectedNoiseState => (),
            CError::CodecTodo => (),
        };
    }
}
