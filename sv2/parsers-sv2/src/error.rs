#[derive(Debug)]
pub enum ParserError {
    UnexpectedMessage(u8),
    BadPayloadSize,
    UnexpectedPoolMessage,
    BinaryError(binary_sv2::Error),
    TlvError(crate::tlv::TlvError),
    ExtensionError(crate::tlv_extensions::ExtensionError),
}

impl From<binary_sv2::Error> for ParserError {
    fn from(e: binary_sv2::Error) -> Self {
        ParserError::BinaryError(e)
    }
}

impl From<crate::tlv::TlvError> for ParserError {
    fn from(e: crate::tlv::TlvError) -> Self {
        ParserError::TlvError(e)
    }
}

impl From<crate::tlv_extensions::ExtensionError> for ParserError {
    fn from(e: crate::tlv_extensions::ExtensionError) -> Self {
        ParserError::ExtensionError(e)
    }
}

impl core::fmt::Display for ParserError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParserError::UnexpectedMessage(msg_type) => {
                write!(f, "Unexpected message type: {msg_type}")
            }
            ParserError::BadPayloadSize => write!(f, "Bad payload size"),
            ParserError::UnexpectedPoolMessage => write!(f, "Unexpected pool message"),
            ParserError::BinaryError(e) => write!(f, "Binary error: {e:?}"),
            ParserError::TlvError(e) => write!(f, "TLV error: {e}"),
            ParserError::ExtensionError(e) => write!(f, "Extension error: {e}"),
        }
    }
}
