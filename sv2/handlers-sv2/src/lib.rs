mod common;
mod common_owned;
mod error;
mod extensions;
mod extensions_owned;
mod job_declaration;
mod job_declaration_owned;
mod mining;
mod mining_owned;
mod template_distribution;
mod template_distribution_owned;

pub use error::HandlerErrorType;

pub use common::{
    HandleCommonMessagesFromClientAsync, HandleCommonMessagesFromClientSync,
    HandleCommonMessagesFromServerAsync, HandleCommonMessagesFromServerSync,
};

pub use mining::{
    HandleMiningMessagesFromClientAsync, HandleMiningMessagesFromClientSync,
    HandleMiningMessagesFromServerAsync, HandleMiningMessagesFromServerSync,
};

pub use template_distribution::{
    HandleTemplateDistributionMessagesFromClientAsync,
    HandleTemplateDistributionMessagesFromClientSync,
    HandleTemplateDistributionMessagesFromServerAsync,
    HandleTemplateDistributionMessagesFromServerSync,
};

pub use job_declaration::{
    HandleJobDeclarationMessagesFromClientAsync, HandleJobDeclarationMessagesFromClientSync,
    HandleJobDeclarationMessagesFromServerAsync, HandleJobDeclarationMessagesFromServerSync,
};

pub use extensions::{
    HandleExtensionsFromClientAsync, HandleExtensionsFromClientSync,
    HandleExtensionsFromServerAsync, HandleExtensionsFromServerSync,
};

pub use common_owned::{
    HandleCommonMessagesFromClientOwnedAsync, HandleCommonMessagesFromClientOwnedSync,
    HandleCommonMessagesFromServerOwnedAsync, HandleCommonMessagesFromServerOwnedSync,
};

pub use mining_owned::{
    HandleMiningMessagesFromClientOwnedAsync, HandleMiningMessagesFromClientOwnedSync,
    HandleMiningMessagesFromServerOwnedAsync, HandleMiningMessagesFromServerOwnedSync,
};

pub use template_distribution_owned::{
    HandleTemplateDistributionMessagesFromClientOwnedAsync,
    HandleTemplateDistributionMessagesFromClientOwnedSync,
    HandleTemplateDistributionMessagesFromServerOwnedAsync,
    HandleTemplateDistributionMessagesFromServerOwnedSync,
};

pub use job_declaration_owned::{
    HandleJobDeclarationMessagesFromClientOwnedAsync,
    HandleJobDeclarationMessagesFromClientOwnedSync,
    HandleJobDeclarationMessagesFromServerOwnedAsync,
    HandleJobDeclarationMessagesFromServerOwnedSync,
};

pub use extensions_owned::{
    HandleExtensionsFromClientOwnedAsync, HandleExtensionsFromClientOwnedSync,
    HandleExtensionsFromServerOwnedAsync, HandleExtensionsFromServerOwnedSync,
};

use framing_sv2::header::Header;
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, ParserError, Tlv};

/// A parsed frame and the TLV fields of its negotiated extensions.
type ParsedFrame<'a> = (AnyMessage<'a>, Option<Vec<Tlv>>);

/// Parses a frame for a handler, or returns `None` for a frame whose `extension_type` this crate
/// does not implement.
///
/// Sv2 spec §3.4.1 requires a message with an unknown `extension_type` that is processed locally
/// to be discarded and ignored, so handlers drop it and return `Ok(())`. A proxy that has to forward
/// such a message (channel_msg bit set) does so before handing the frame to a handler.
fn parse_frame<'a>(
    header: Header,
    payload: &'a mut [u8],
    negotiated_extensions: &[u16],
) -> Result<Option<ParsedFrame<'a>>, ParserError> {
    match parse_message_frame_with_tlvs(header, payload, negotiated_extensions) {
        Ok(parsed) => Ok(Some(parsed)),
        Err(ParserError::UnknownExtension(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod test_utils {
    use super::*;

    #[derive(Debug)]
    pub(crate) enum TestError {
        Unexpected,
        Parse,
    }

    impl HandlerErrorType for TestError {
        fn unexpected_message(_: u16, _: u8) -> Self {
            TestError::Unexpected
        }
        fn parse_error(_: ParserError) -> Self {
            TestError::Parse
        }
    }

    pub(crate) fn header(extension_type: u16, message_type: u8, payload_length: usize) -> Header {
        let ext = extension_type.to_le_bytes();
        let len = (payload_length as u32).to_le_bytes();
        Header::from_bytes(&[ext[0], ext[1], message_type, len[0], len[1], len[2]]).unwrap()
    }
}
