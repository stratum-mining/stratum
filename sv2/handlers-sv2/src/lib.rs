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

use framing_sv2::{header::Header, Error as FramingError};
use parsers_sv2::ParserError;

pub use error::HandlerErrorType;

fn validate_payload_length(header: Header, payload: &[u8]) -> Result<(), ParserError> {
    if payload.len() == header.payload_length() {
        Ok(())
    } else {
        Err(ParserError::BadPayloadSize(
            FramingError::PayloadLengthMismatch {
                declared: header.payload_length(),
                written: payload.len(),
            },
        ))
    }
}

pub use common::{
    HandleCommonMessagesFromClientAsync, HandleCommonMessagesFromClientSync,
    HandleCommonMessagesFromServerAsync, HandleCommonMessagesFromServerSync,
};

pub use mining::{
    HandleMiningMessagesFromClientAsync, HandleMiningMessagesFromClientSync,
    HandleMiningMessagesFromServerAsync, HandleMiningMessagesFromServerSync, SupportedChannelTypes,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_payload_lengths_that_do_not_match_the_header() {
        let header = Header::from_bytes(&[0, 0, 0, 4, 0, 0]).unwrap();

        assert!(matches!(
            validate_payload_length(header, &[0; 3]),
            Err(ParserError::BadPayloadSize(
                FramingError::PayloadLengthMismatch {
                    declared: 4,
                    written: 3
                }
            ))
        ));
        assert!(matches!(
            validate_payload_length(header, &[0; 5]),
            Err(ParserError::BadPayloadSize(
                FramingError::PayloadLengthMismatch {
                    declared: 4,
                    written: 5
                }
            ))
        ));
        assert!(validate_payload_length(header, &[0; 4]).is_ok());
    }
}
