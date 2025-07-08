#[derive(Debug)]
pub enum ParserError {
    UnexpectedMessage(u8),
    BadPayloadSize,
    UnexpectedPoolMessage,
    BinaryError(binary_sv2::Error),
}

impl From<binary_sv2::Error> for ParserError {
    fn from(e: binary_sv2::Error) -> Self {
        ParserError::BinaryError(e)
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::UnexpectedMessage(msg_type) => {
                write!(f, "Unexpected message type: {msg_type}")
            }
            ParserError::BadPayloadSize => write!(f, "Bad payload size"),
            ParserError::UnexpectedPoolMessage => write!(f, "Unexpected pool message"),
            ParserError::BinaryError(e) => write!(f, "Binary error: {e:?}"),
        }
    }
}

impl std::error::Error for ParserError {}
