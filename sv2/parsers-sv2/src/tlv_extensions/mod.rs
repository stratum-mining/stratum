//! TLV field implementations for known SV2 extensions.
//!
//! This module provides `TlvField` trait implementations for extension types
//! defined in the `extensions_sv2` crate, along with extension-specific error types.

mod error;

pub use error::{ExtensionError, UserIdentityError};

use super::{Tlv, TlvField};
use crate::ParserError;
use extensions_sv2::{
    UserIdentity, EXTENSION_TYPE_WORKER_HASHRATE_TRACKING, TLV_FIELD_TYPE_USER_IDENTITY,
};

extern crate alloc;
use alloc::vec::Vec;

/// Maximum length for worker identifiers in bytes as per the spec.
const MAX_USER_IDENTITY_LENGTH: usize = 32;

/// Implementation of TlvField trait for UserIdentity.
///
/// This provides the standard interface for encoding/decoding UserIdentity
/// as a TLV field in the Worker-Specific Hashrate Tracking extension.
impl TlvField for UserIdentity {
    const EXTENSION_TYPE: u16 = EXTENSION_TYPE_WORKER_HASHRATE_TRACKING;
    const FIELD_TYPE: u8 = TLV_FIELD_TYPE_USER_IDENTITY;

    /// Decodes a TLV from raw bytes.
    fn from_bytes(bytes: &[u8]) -> Result<Tlv, ParserError> {
        // TlvError auto-converts to ParserError::TlvError
        Tlv::decode(bytes).map_err(Into::into)
    }

    /// Encodes this UserIdentity as raw TLV bytes.
    fn to_bytes(&self) -> Result<Vec<u8>, ParserError> {
        let tlv = self.to_tlv()?;
        // TlvError auto-converts to ParserError::TlvError
        tlv.encode().map_err(Into::into)
    }

    /// Creates a UserIdentity from a parsed TLV.
    fn from_tlv(tlv: &Tlv) -> Result<Self, ParserError> {
        // Verify extension type
        if tlv.r#type.extension_type != Self::EXTENSION_TYPE {
            // UserIdentityError -> ExtensionError -> ParserError
            return Err(UserIdentityError::InvalidExtensionType(tlv.r#type.extension_type).into());
        }

        // Verify field type
        if tlv.r#type.field_type != Self::FIELD_TYPE {
            return Err(UserIdentityError::InvalidFieldType(tlv.r#type.field_type).into());
        }

        // Enforce 32-byte maximum
        if tlv.value.len() > MAX_USER_IDENTITY_LENGTH {
            return Err(UserIdentityError::TooLong(tlv.value.len()).into());
        }

        // Create UserIdentity from raw bytes
        UserIdentity::from_bytes(&tlv.value)
            .map_err(|e| UserIdentityError::InvalidUtf8(e.into()).into())
    }

    /// Converts this UserIdentity into a TLV structure.
    fn to_tlv(&self) -> Result<Tlv, ParserError> {
        // Validate length
        if self.as_bytes().len() > MAX_USER_IDENTITY_LENGTH {
            return Err(UserIdentityError::TooLong(self.as_bytes().len()).into());
        }

        Ok(Tlv::new(
            Self::EXTENSION_TYPE,
            Self::FIELD_TYPE,
            self.as_bytes().to_vec(),
        ))
    }
}
