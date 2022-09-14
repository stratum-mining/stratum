use std::fmt;

pub type ProxyResult<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Errors on bad `serde_json` serialize/deserialize.
    BadSerdeJson(serde_json::Error),
    BadSv1StdReq,
    /// Errors on bad noise handshake.
    CodecNoiseError(codec_sv2::noise_sv2::Error),
    /// Errors on bad `TcpStream` connection.
    IoError(std::io::Error),
    /// Errors from `roles_logic_sv2` crate.
    RolesSv2LogicError(roles_logic_sv2::errors::Error),
    // NoTranslationRequired,
    /// SV1 protocol library error
    V1ProtocolError(v1::error::Error),
    // InvalidJsonRpcMessageKind(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            CodecNoiseError(ref e) => write!(f, "Noise error: `{:?}", e),
            BadSerdeJson(ref e) => write!(f, "Bad serde json: `{:?}`", e),
            BadSv1StdReq => write!(f, "Bad SV1 Standard Request"),
            IoError(ref e) => write!(f, "I/O error: `{:?}", e),
            RolesSv2LogicError(ref e) => write!(f, "Roles SV2 Logic Error: `{:?}`", e),
            V1ProtocolError(ref e) => write!(f, "V1 Protocol Error: `{:?}`", e),
        }
    }
}

impl From<codec_sv2::noise_sv2::Error> for Error {
    fn from(e: codec_sv2::noise_sv2::Error) -> Self {
        Error::CodecNoiseError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
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
