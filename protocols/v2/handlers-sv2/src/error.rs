use parsers_sv2::ParserError;

#[derive(Debug)]
pub enum HandlerError {
    UnexpectedMessage(u8),
    ParserError(ParserError),
    External(Box<dyn std::error::Error + Send + Sync>),
}

impl From<ParserError> for HandlerError {
    fn from(value: ParserError) -> HandlerError {
        HandlerError::ParserError(value)
    }
}
