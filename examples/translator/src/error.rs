use std::fmt;

pub type ProxyResult<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
    BadSv1StdReq,
    /// Errors from `binary_sv2` crate.
    BinarySv2Error(binary_sv2::Error),
    /// Errors on bad noise handshake.
    CodecNoiseError(codec_sv2::noise_sv2::Error),
    /// Errors from `framing_sv2` crate.
    FramingSv2Error(framing_sv2::Error),
    /// Errors on bad `TcpStream` connection.
    IoError(std::io::Error),
    /// Errors if SV1 downstream returns a `mining.submit` with no version bits.
    NoSv1VersionBits,
    /// Errors on bad `String` to `int` conversion.
    ParseInt(std::num::ParseIntError),
    /// Errors from `roles_logic_sv2` crate.
    RolesSv2LogicError(roles_logic_sv2::errors::Error),
    // NoTranslationRequired,
    /// SV1 protocol library error
    V1ProtocolError(v1::error::Error),
    /// Errors if a `NoiseFrame` was returned, when a `SV2Frame` was expected.
    UnexpectedNoiseFrame,
    // InvalidJsonRpcMessageKind(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            BadSv1StdReq => write!(f, "Bad SV1 Standard Request"),
            BinarySv2Error(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            CodecNoiseError(ref e) => write!(f, "Noise error: `{:?}", e),
            FramingSv2Error(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            IoError(ref e) => write!(f, "I/O error: `{:?}", e),
            NoSv1VersionBits => write!(
                f,
                "`mining.submit` received from SV1 downstream does not contain `version_bits`"
            ),
            ParseInt(ref e) => write!(f, "Bad convert from `String` to `int`: `{:?}`", e),
            RolesSv2LogicError(ref e) => write!(f, "Roles SV2 Logic Error: `{:?}`", e),
            UnexpectedNoiseFrame => write!(f, "Expected `SV2Frame`, received `NoiseFrame`"),
            V1ProtocolError(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
        }
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Self {
        Error::BinarySv2Error(e)
    }
}

impl From<codec_sv2::noise_sv2::Error> for Error {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        Error::CodecNoiseError(e)
    }
}

impl From<framing_sv2::Error> for Error {
    fn from(e: framing_sv2::Error) -> Self {
        Error::FramingSv2Error(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<roles_logic_sv2::errors::Error> for Error {
    fn from(e: roles_logic_sv2::errors::Error) -> Self {
        Error::RolesSv2LogicError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::BadSerdeJson(e)
    }
}

impl From<v1::error::Error> for Error {
    fn from(e: v1::error::Error) -> Self {
        Error::V1ProtocolError(e)
    }
}
