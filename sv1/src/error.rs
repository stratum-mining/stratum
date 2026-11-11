use bitcoin::hashes::hex::HexToBytesError;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    BadBytesConvert(binary_sv2::Error),
    /// Errors on bad hex decode/encode.
    HexError(HexToBytesError),
    /// A hex string has an invalid length, in characters.
    InvalidHexLen(usize),
    Decode(DecodeError),
    /// The result of the outstanding `method` request with this ID does not have that method's
    /// result type.
    InvalidResult {
        id: crate::json_rpc::Id,
        method: crate::json_rpc::Method,
        error: serde_json::Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::BadBytesConvert(ref e) => write!(
                f,
                "Bad U256 or B032 conversion (U256 length must be exactly 32 bytes; B032 length must be <= 32 bytes): {e:?}"
            ),
            Error::HexError(ref e) => write!(f, "Bad hex encode/decode: `{e}`"),
            Error::InvalidHexLen(len) => write!(f, "Hex string has an invalid length of {len}"),
            Error::Decode(e) => e.fmt(f),
            Error::InvalidResult { id, method, error } => {
                write!(f, "malformed result for {method} request {id}: {error}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<HexToBytesError> for Error {
    fn from(e: HexToBytesError) -> Self {
        Error::HexError(e)
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(inner: binary_sv2::Error) -> Self {
        Error::BadBytesConvert(inner)
    }
}

impl From<DecodeError> for Error {
    fn from(e: DecodeError) -> Self {
        Error::Decode(e)
    }
}

/// The longest ID or method name a [`DecodeError`] keeps.
const MAX_ECHOED_LEN: usize = 64;

/// A message that could not be decoded.
///
/// `id` is set only when the message carried a valid ID, so a reply never reflects an invalid
/// one.
#[derive(Debug)]
pub struct DecodeError {
    pub id: Option<crate::json_rpc::Id>,
    pub method: Option<crate::json_rpc::Method>,
    pub kind: DecodeErrorKind,
}

#[derive(Debug)]
pub enum DecodeErrorKind {
    /// The line is not JSON, not an object, or repeats a reserved member.
    Json(serde_json::Error),
    InvalidEnvelope(&'static str),
    /// The method is unknown, or is not sent in this direction.
    UnknownMethod,
    /// A request that needs a reply has no ID.
    MissingId,
    /// A response arrived where a request was expected.
    UnexpectedResponse,
    InvalidParams {
        field: &'static str,
        reason: &'static str,
    },
}

/// A parameter that failed validation, before the request it belongs to is known.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamError {
    pub(crate) field: &'static str,
    pub(crate) reason: &'static str,
}

impl ParamError {
    pub(crate) fn new(field: &'static str, reason: &'static str) -> Self {
        ParamError { field, reason }
    }
}

impl From<ParamError> for DecodeErrorKind {
    fn from(e: ParamError) -> Self {
        DecodeErrorKind::InvalidParams {
            field: e.field,
            reason: e.reason,
        }
    }
}

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl DecodeError {
    pub(crate) fn new(kind: DecodeErrorKind) -> Self {
        DecodeError::about(None, None, kind)
    }

    /// Keeps the ID and method only while they are short, so the error stays small whatever the
    /// size of the message.
    pub(crate) fn about(
        id: Option<crate::json_rpc::Id>,
        method: Option<crate::json_rpc::Method>,
        kind: DecodeErrorKind,
    ) -> Self {
        DecodeError {
            id: id.filter(|id| id.text_len() <= MAX_ECHOED_LEN),
            method: method.filter(|method| method.as_str().len() <= MAX_ECHOED_LEN),
            kind,
        }
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.kind {
            DecodeErrorKind::Json(e) => write!(f, "invalid SV1 message: {e}")?,
            DecodeErrorKind::InvalidEnvelope(reason) => {
                write!(f, "invalid SV1 envelope: {reason}")?
            }
            DecodeErrorKind::UnknownMethod => f.write_str("unknown SV1 method")?,
            DecodeErrorKind::MissingId => f.write_str("SV1 request has no id")?,
            DecodeErrorKind::UnexpectedResponse => f.write_str("unexpected SV1 response")?,
            DecodeErrorKind::InvalidParams { field, reason } => {
                write!(f, "invalid SV1 params: {field}: {reason}")?
            }
        }
        if let Some(method) = &self.method {
            write!(f, " (method {method})")?;
        }
        if let Some(id) = &self.id {
            write!(f, " (id {id})")?;
        }
        Ok(())
    }
}

impl std::error::Error for DecodeError {}

/// A response that did not carry the expected result.
#[derive(Debug)]
pub enum ResponseError {
    /// The peer answered with an error.
    Rpc(crate::json_rpc::RpcError),
    /// The result does not have the expected shape.
    InvalidResult(serde_json::Error),
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ResponseError::Rpc(e) => write!(f, "peer error {}: {:?}", e.code, e.message),
            ResponseError::InvalidResult(e) => write!(f, "unexpected result: {e}"),
        }
    }
}

impl std::error::Error for ResponseError {}
