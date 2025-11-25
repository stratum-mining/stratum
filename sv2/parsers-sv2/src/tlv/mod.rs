//! Generic TLV (Type-Length-Value) encoding and decoding utilities.
//!
//! This module provides generic functions for working with TLV fields in Stratum V2 extensions.
//! TLV fields are used to extend base protocol messages with optional extension-specific data.

pub use crate::tlv::codec::Tlv;
use alloc::vec::Vec;

extern crate alloc;

mod codec;
mod error;
mod iter;
mod list;

pub use error::TlvError;
pub use list::TlvList;

/// TLV header size in bytes (3 bytes Type + 2 bytes Length)
pub const TLV_HEADER_SIZE: usize = 5;

/// Trait for types that can be encoded/decoded as TLV fields.
pub trait TlvField: Sized {
    /// The extension type this field belongs to.
    const EXTENSION_TYPE: u16;

    /// The field type within the extension.
    const FIELD_TYPE: u8;

    /// Decodes a TLV from raw bytes.
    ///
    /// This parses the TLV structure (Type-Length-Value) from the byte buffer.
    fn from_bytes(bytes: &[u8]) -> Result<Tlv, crate::ParserError>;

    /// Encodes this field as raw TLV bytes.
    ///
    /// Returns the complete TLV bytes including Type, Length, and Value fields.
    fn to_bytes(&self) -> Result<Vec<u8>, crate::ParserError>;

    /// Creates an instance of this field from a parsed TLV.
    ///
    /// This validates the TLV type matches this field's extension and field type,
    /// then extracts and parses the value.
    fn from_tlv(tlv: &Tlv) -> Result<Self, crate::ParserError>;

    /// Converts this field into a TLV structure.
    ///
    /// This creates a TLV with the correct extension_type and field_type for this field.
    fn to_tlv(&self) -> Result<Tlv, crate::ParserError>;
}
