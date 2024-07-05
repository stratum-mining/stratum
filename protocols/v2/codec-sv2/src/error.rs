#[cfg(test)]
use core::cmp;
use core::fmt;
use framing_sv2::Error as FramingError;
#[cfg(feature = "noise_sv2")]
use noise_sv2::{AeadError, Error as NoiseError};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Binary Sv2 data format error.
    BinarySv2Error(binary_sv2::Error),

    /// Sv2 framing error.
    FramingSv2Error(framing_sv2::Error),

    /// Missing bytes in the Noise protocol.
    MissingBytes(usize),

    /// Sv2 Noise protocol error.
    #[cfg(feature = "noise_sv2")]
    NoiseSv2Error(NoiseError),

    /// AEAD (`snow`) error in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    AeadError(AeadError),

    /// Unexpected state in the Noise protocol.
    UnexpectedNoiseState,

    /// Invalid step for responder in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    InvalidStepForResponder,

    /// Invalid step for initiator in the Noise protocol.
    #[cfg(feature = "noise_sv2")]
    InvalidStepForInitiator,

    /// Noise protocol is not in the expected handshake state.
    #[cfg(feature = "noise_sv2")]
    NotInHandShakeState,

    /// Framing Sv2 error.
    FramingError(FramingError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{:?}`", e),
            FramingSv2Error(e) => write!(f, "Framing Sv2 Error: `{:?}`", e),
            MissingBytes(u) => write!(f, "Missing `{}` Noise bytes", u),
            #[cfg(feature = "noise_sv2")]
            NoiseSv2Error(e) => write!(f, "Noise SV2 Error: `{:?}`", e),
            #[cfg(feature = "noise_sv2")]
            AeadError(e) => write!(f, "Aead Error: `{:?}`", e),
            UnexpectedNoiseState => {
                write!(f, "Noise state is incorrect")
            }
            #[cfg(feature = "noise_sv2")]
            InvalidStepForResponder => write!(
                f,
                "This noise handshake step can not be executed by a responder"
            ),
            #[cfg(feature = "noise_sv2")]
            InvalidStepForInitiator => write!(
                f,
                "This noise handshake step can not be executed by an initiato"
            ),
            #[cfg(feature = "noise_sv2")]
            NotInHandShakeState => write!(
                f,
                "This operation can be executed only during the noise handshake"
            ),
            FramingError(e) => write!(f, "Framing error in codec: `{:?}`", e),
        }
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}

impl From<framing_sv2::Error> for Error {
    fn from(e: framing_sv2::Error) -> Self {
        Error::FramingSv2Error(e)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<AeadError> for Error {
    fn from(e: AeadError) -> Self {
        Error::AeadError(e)
    }
}

#[cfg(feature = "noise_sv2")]
impl From<NoiseError> for Error {
    fn from(e: NoiseError) -> Self {
        Error::NoiseSv2Error(e)
    }
}

#[repr(C)]
#[derive(Debug)]
pub enum CError {
    /// Binary Sv2 data format error.
    BinarySv2Error,

    /// Framing Sv2 error.
    FramingSv2Error,

    /// Missing bytes in the Noise protocol.
    MissingBytes(usize),

    /// Sv2 Noise protocol error.
    NoiseSv2Error,

    /// AEAD (`snow`) error in the Noise protocol.
    AeadError,

    /// Unexpected state in the Noise protocol.
    UnexpectedNoiseState,

    /// Invalid step for responder in the Noise protocol.
    InvalidStepForResponder,

    /// Invalid step for initiator in the Noise protocol.
    InvalidStepForInitiator,

    /// Noise protocol is not in the expected handshake state.
    NotInHandShakeState,

    /// Framing Sv2 error.
    FramingError,
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
            Error::FramingSv2Error(_) => CError::FramingSv2Error,
            Error::MissingBytes(u) => CError::MissingBytes(u),
            #[cfg(feature = "noise_sv2")]
            Error::NoiseSv2Error(_) => CError::NoiseSv2Error,
            #[cfg(feature = "noise_sv2")]
            Error::AeadError(_) => CError::AeadError,
            Error::UnexpectedNoiseState => CError::UnexpectedNoiseState,
            #[cfg(feature = "noise_sv2")]
            Error::InvalidStepForResponder => CError::InvalidStepForResponder,
            #[cfg(feature = "noise_sv2")]
            Error::InvalidStepForInitiator => CError::InvalidStepForInitiator,
            #[cfg(feature = "noise_sv2")]
            Error::NotInHandShakeState => CError::NotInHandShakeState,
            Error::FramingError(_) => CError::FramingError,
        }
    }
}

impl Drop for CError {
    fn drop(&mut self) {
        match self {
            CError::BinarySv2Error => (),
            CError::FramingSv2Error => (),
            CError::MissingBytes(_) => (),
            CError::NoiseSv2Error => (),
            CError::AeadError => (),
            CError::UnexpectedNoiseState => (),
            CError::InvalidStepForResponder => (),
            CError::InvalidStepForInitiator => (),
            CError::NotInHandShakeState => (),
            CError::FramingError => (),
        };
    }
}
