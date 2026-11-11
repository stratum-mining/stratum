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

use framing_sv2::header::Header;
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, ParserError, Tlv};

/// Decodes a frame through the header-checked `AnyMessage` parser, reading the TLV suffix only
/// when extensions were negotiated.
fn decode_frame<'a>(
    header: Header,
    payload: &'a mut [u8],
    negotiated_extensions: &[u16],
) -> Result<(AnyMessage<'a>, Option<Vec<Tlv>>), ParserError> {
    if negotiated_extensions.is_empty() {
        return Ok((AnyMessage::try_from((header, payload))?, None));
    }
    parse_message_frame_with_tlvs(header, payload, negotiated_extensions)
}

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
