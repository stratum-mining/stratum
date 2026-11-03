use framing_sv2::header::Header;
use parsers_sv2::ParserError;
pub trait HandlerErrorType {
    fn unexpected_message(extension_type: u16, message_type: u8) -> Self;
    fn parse_error(error: ParserError) -> Self;
}

/// Rejects a frame that belongs to an extension namespace.
///
/// When no extensions are negotiated, the core protocol handlers decode the payload from the
/// message type alone. A message type is only meaningful inside its `extension_type` namespace,
/// so a frame with a non-zero `extension_type` must not be read as a core protocol message.
pub(crate) fn ensure_core_message<E: HandlerErrorType>(header: &Header) -> Result<(), E> {
    let extension_type = header.ext_type_without_channel_msg();
    if extension_type != 0 {
        return Err(E::unexpected_message(extension_type, header.msg_type()));
    }
    Ok(())
}
