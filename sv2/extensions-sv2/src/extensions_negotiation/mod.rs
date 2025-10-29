//! Extensions Negotiation (extension_type=0x0001)
//!
//! This extension allows clients and servers to negotiate support for protocol extensions
//! immediately after the SetupConnection exchange.
//!
//! Unlike most extensions, this extension does NOT use TLV fields - it defines its own
//! message types with fixed structures.

mod messages;

pub use messages::{RequestExtensions, RequestExtensionsError, RequestExtensionsSuccess};

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
