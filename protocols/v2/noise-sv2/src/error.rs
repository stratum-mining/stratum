use core::fmt;

pub type Result<T> = core::result::Result<T, Error>;

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    BadSerdeJson(serde_json::Error),
    /// Errors on bad conversion from system time to UNIX timestamp.
    BadSystemTimeFromTimestamp(std::time::SystemTimeError),
    /// Errors on bad conversion from UNIX timestamp to system time.
    BadTimestampFromSystemTime(u32),
    /// Errors from the `binary_sv2` crate
    BinarySv2Error(binary_sv2::Error),
    Bs58DecodeError(bs58::decode::Error),
    /// Errors on expired certificate. Displays validity expiration time and current time:
    /// `(not_valid_after, now)`.
    CertificateExpired(u32, u32),
    /// Errors on invalid certificate. Displays validity start time and current time:
    /// `(valid_from, now)`.
    CertificateInvalid(u32, u32),
    DalekError(ed25519_dalek::ed25519::Error),
    /// Errors if negotiation encryption algorithm is unsupported. Valid values are `1196639553`
    /// (AESGCM) or `1212368963` (ChaChaPoly).
    EncryptionAlgorithmInvalid(u32),
    /// Errors if encryption algorithm is expected to be specified but is not.
    EncryptionAlgorithmNotFound,
    /// Errors if handshake step expected message but received `None`.
    ExpectedIncomingHandshakeMessage,
    /// Errors if handshake initiator step is invalid. Valid steps are 0, 1, or 2.
    HSInitiatorStepNotFound(usize),
    /// Errors if handshake responder step is invalid. Valid steps are 0, 1, or 2.
    HSResponderStepNotFound(usize),
    /// Failed to parse incoming message
    InvalidHandshakeMessage,
    /// I/O Error
    #[cfg(not(feature = "no_std"))]
    IoError,
    /// Errors if message to be decrypted is empty.
    MessageToDecryptIsEmpty,
    /// Errors if more than on encryption algorithm was returned when only one was expected.
    MustSpecifyOneEncryptionAlgorithm(usize),
    /// `snow` errors
    SnowError(snow::Error),
    /// Errors on `get_remote_static_key` return is `None`. Occurs is chosen Noise pattern doesn’t
    /// necessitate a remote static key, or if the remote static key is not yet known (as can be
    /// the case in the XX pattern, for example).
    /// https://docs.rs/snow/latest/snow/struct.HandshakeState.html#method.get_remote_static
    SnowNoRemoteStaticKey,
    /// Catch all
    NoiseTodo,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadSerdeJson(e) => write!(f, "Serde JSON Error: `{}`", e),
            BadSystemTimeFromTimestamp(e) => write!(f, "Error converting system time to UNIX timestamp: `{}`", e),
            BadTimestampFromSystemTime(u) => write!(f, "Error converting UNIX timestamp `{}` to system time", u),
            Bs58DecodeError(e) => write!(f, "Bs58 Error: `{}`", e),
            BinarySv2Error(e) => write!(f, "Binary Sv2 Error: `{:?}`", e),
            CertificateExpired(u1, u2) => write!(f, "Certificate expired. Provided certificate expired at `{}`. The current time is `{}`", u1, u2),
            CertificateInvalid(u1, u2) => write!(f, "Certificate invalid. Provided certificate is valid starting at `{}`. The current time is `{}`", u1, u2),
            DalekError(e) => write!(f, "ed25519 Dalek Error: `{:?}`", e),
            EncryptionAlgorithmInvalid(u) => write!(f, "Invalid encryption algorithm. Expected `1196639553` (AESGCM) or `1212368963` (ChaChaPoly). Got: `{}`", u),
            EncryptionAlgorithmNotFound => write!(f, "Expected encryption algorithm, but got `None`"),
            ExpectedIncomingHandshakeMessage => write!(f, "Handshake step expected message but got `None`"),
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
            InvalidHandshakeMessage => write!(f, "Failed to parse handshake message"),
            IoError => write!(f, "IO Error"),
            MessageToDecryptIsEmpty => write!(f, "Message to decrypt is empty"),
            MustSpecifyOneEncryptionAlgorithm(u) => write!(f, "Expected 1 encryption algorithm. Received `{}`", u),
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

impl From<bs58::decode::Error> for Error {
    fn from(e: bs58::decode::Error) -> Self {
        Error::Bs58DecodeError(e)
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

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}

impl From<snow::Error> for Error {
    fn from(e: snow::Error) -> Self {
        Error::SnowError(e)
    }
}

impl From<std::time::SystemTimeError> for Error {
    fn from(e: std::time::SystemTimeError) -> Self {
        Error::BadSystemTimeFromTimestamp(e)
    }
}
