use core::fmt;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    /// Errors from the `binary_sv2` crate
    BinarySv2Error(binary_sv2::Error),
    DalekError(ed25519_dalek::ed25519::Error),
    /// Errors if handshake initiator step is invalid. Valid steps are 0, 1, or 2.
    HSInitiatorStepNotFound(usize),
    /// Errors if handshake responder step is invalid. Valid steps are 0, 1, or 2.
    HSResponderStepNotFound(usize),
    /// Errors if negotiation encryption algorithm is unsupported. Valid values are `1196639553`
    /// (AESGCM) or `1212368963` (ChaChaPoly).
    InvalidEncryptionAlgorithm(u32),
    SnowError(snow::Error),
    /// Errors on `get_remote_static_key` return is `None`. Occurs is chosen Noise pattern doesn’t
    /// necessitate a remote static key, or if the remote static key is not yet known (as can be
    /// the case in the XX pattern, for example).
    /// https://docs.rs/snow/latest/snow/struct.HandshakeState.html#method.get_remote_static
    SnowNoRemoteStaticKey,
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
            InvalidEncryptionAlgorithm(u) => write!(f, "Invalid encryption algorithm. Expected `1196639553` (AESGCM) or `1212368963` (ChaChaPoly). Got: `{}`", u),
            SnowError(e) => write!(f, "Snow Error: `{:?}`", e),
            SnowNoRemoteStaticKey => write!(f, "Snow Error: No remote static key found"),
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
