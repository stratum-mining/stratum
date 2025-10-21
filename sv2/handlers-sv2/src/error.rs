use parsers_sv2::ParserError;
pub trait HandlerErrorType {
    fn unexpected_message(message_type: u8) -> Self;
    fn parse_error(error: ParserError) -> Self;
}
