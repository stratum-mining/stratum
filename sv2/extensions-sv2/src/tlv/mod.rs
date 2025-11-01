//! Generic TLV (Type-Length-Value) encoding and decoding utilities.
//!
//! This module provides generic functions for working with TLV fields in Stratum V2 extensions.
//! TLV fields are used to extend base protocol messages with optional extension-specific data.

mod codec;
mod error;

pub use codec::{has_tlv_for_extension, has_valid_tlv_data, Tlv, TLV_HEADER_SIZE};
pub use error::TlvError;
