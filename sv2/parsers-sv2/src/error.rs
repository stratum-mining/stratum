#[derive(Debug)]
pub enum ParserError {
    UnexpectedMessage(u8),
    BadPayloadSize(framing_sv2::Error),
    UnexpectedPoolMessage,
    BinaryError(binary_sv2::Error),
    TlvError(crate::tlv::TlvError),
    ExtensionError(crate::tlv_extensions::ExtensionError),
    /// The payload is not as long as its `Header` declares.
    PayloadLengthMismatch {
        declared: usize,
        actual: usize,
    },
    /// The `Header`'s channel_msg bit differs from the one the decoded message type defines.
    ChannelBitMismatch {
        message_type: u8,
        channel_msg: bool,
    },
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
            ParserError::BadPayloadSize(e) => write!(f, "Bad payload size: {e}"),
            ParserError::UnexpectedPoolMessage => write!(f, "Unexpected pool message"),
            ParserError::BinaryError(e) => write!(f, "Binary error: {e}"),
            ParserError::TlvError(e) => write!(f, "TLV error: {e}"),
            ParserError::ExtensionError(e) => write!(f, "Extension error: {e}"),
            ParserError::PayloadLengthMismatch { declared, actual } => write!(
                f,
                "Payload is {actual} bytes, its header declares {declared}"
            ),
            ParserError::ChannelBitMismatch {
                message_type,
                channel_msg,
            } => write!(
                f,
                "Header channel_msg bit is {channel_msg} for message type {message_type:#04x}"
            ),
        }
    }
}
