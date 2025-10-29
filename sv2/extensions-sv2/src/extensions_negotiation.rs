//! Extensions Negotiation (extension_type=0x0001)
//!
//! This extension allows clients and servers to negotiate support for protocol extensions
//! immediately after the SetupConnection exchange.
//!
//! Unlike most extensions, this extension does NOT use TLV fields - it defines its own
//! message types with fixed structures.

use alloc::{fmt, vec::Vec};
use binary_sv2::{self, Deserialize, Seq064K, Serialize};

/// Extension type for Extensions Negotiation
pub const EXTENSION_TYPE: u16 = 0x0001;

/// Message type constants
pub const MESSAGE_TYPE_REQUEST_EXTENSIONS: u8 = 0x00;
pub const MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS: u8 = 0x01;
pub const MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR: u8 = 0x02;

/// Channel message bits (all false for extensions as per spec)
pub const CHANNEL_BIT_REQUEST_EXTENSIONS: bool = false;
pub const CHANNEL_BIT_REQUEST_EXTENSIONS_SUCCESS: bool = false;
pub const CHANNEL_BIT_REQUEST_EXTENSIONS_ERROR: bool = false;

/// Message used by client to request support for protocol extensions.
///
/// This message is sent immediately after the SetupConnection.Success exchange
/// to indicate which extensions the client wishes to use. The server responds with
/// either RequestExtensions.Success or RequestExtensions.Error.
///
/// Clients MUST NOT use any features from extensions that are not confirmed as
/// supported by the server.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RequestExtensions<'decoder> {
    /// Unique identifier for pairing request/response.
    ///
    /// The server will echo this value in its response to allow the client
    /// to match responses to requests.
    pub request_id: u16,

    /// List of requested extension identifiers.
    ///
    /// Each u16 value corresponds to a specific extension identifier.
    /// For example: 0x0001 for Extensions Negotiation (this extension itself)
    pub requested_extensions: Seq064K<'decoder, u16>,
}

impl fmt::Display for RequestExtensions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestExtensions(request_id: {}, requested_extensions: {})",
            self.request_id, self.requested_extensions
        )
    }
}

/// Message used by server to accept an extensions request.
///
/// This message is sent in response to a RequestExtensions message to indicate
/// which of the requested extensions the server supports and will enable for
/// this connection.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RequestExtensionsSuccess<'decoder> {
    /// Unique identifier for pairing request/response.
    ///
    /// This MUST match the request_id from the corresponding RequestExtensions message.
    pub request_id: u16,

    /// List of supported extension identifiers.
    ///
    /// This must be a subset of the extensions requested in the RequestExtensions
    /// message. Extensions not listed here are not supported and MUST NOT be used
    /// by the client.
    pub supported_extensions: Seq064K<'decoder, u16>,
}

impl fmt::Display for RequestExtensionsSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestExtensions.Success(request_id: {}, supported_extensions: {})",
            self.request_id, self.supported_extensions
        )
    }
}

/// Message used by server to reject an extensions request or indicate an error.
///
/// This message is sent in response to a RequestExtensions message when the server
/// cannot support some or all of the requested extensions, or when the server requires
/// extensions that were not requested by the client.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RequestExtensionsError<'decoder> {
    /// Unique identifier for pairing request/response.
    ///
    /// This MUST match the request_id from the corresponding RequestExtensions message.
    pub request_id: u16,

    /// List of unsupported extension identifiers.
    ///
    /// Contains the extension identifiers from the client's request that the server
    /// does not support.
    pub unsupported_extensions: Seq064K<'decoder, u16>,

    /// List of extension identifiers required by the server.
    ///
    /// Contains extension identifiers that the server requires but were not included
    /// in the client's request. If the client does not retry with these required
    /// extensions, the server MUST disconnect the client.
    pub required_extensions: Seq064K<'decoder, u16>,
}

impl fmt::Display for RequestExtensionsError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RequestExtensions.Error(request_id: {}, unsupported: {}, required: {})",
            self.request_id, self.unsupported_extensions, self.required_extensions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use binary_sv2::Seq064K;

    #[test]
    fn test_request_extensions() {
        let extensions = vec![0x0001_u16];
        let msg = RequestExtensions {
            request_id: 1,
            requested_extensions: Seq064K::new(extensions).unwrap(),
        };
        assert_eq!(msg.request_id, 1);
        let inner = msg.requested_extensions.into_inner();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0], 0x0001);
    }

    #[test]
    fn test_request_extensions_success() {
        let extensions = vec![0x0001_u16];
        let msg = RequestExtensionsSuccess {
            request_id: 1,
            supported_extensions: Seq064K::new(extensions).unwrap(),
        };
        assert_eq!(msg.request_id, 1);
        let inner = msg.supported_extensions.into_inner();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0], 0x0001);
    }

    #[test]
    fn test_request_extensions_error() {
        let unsupported = vec![0x0003_u16];
        let required = vec![0x0005_u16];
        let msg = RequestExtensionsError {
            request_id: 1,
            unsupported_extensions: Seq064K::new(unsupported).unwrap(),
            required_extensions: Seq064K::new(required).unwrap(),
        };
        assert_eq!(msg.request_id, 1);
        let unsupported = msg.unsupported_extensions.into_inner();
        let required = msg.required_extensions.into_inner();
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0], 0x0003);
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], 0x0005);
    }
}
