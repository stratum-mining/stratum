//! # Stratum V2 Extensions Messages Crate.
//!
//! This crate defines extension messages for Stratum V2 protocol.
//!
//! ## Extensions Supported
//!
//! - **Extensions Negotiation** (extension_type=0x0001): Allows endpoints to negotiate
//!   which optional extensions are supported during connection setup.
//! - **Worker-Specific Hashrate Tracking** (extension_type=0x0002): Enables tracking per-worker hashrates
//!   within extended channels via TLV fields.
//!
//! ## Architecture
//!
//! The crate is organized into:
//! - `tlv/`: Generic TLV encoding/decoding utilities for any extension
//! - `extensions_negotiation/`: Extension negotiation protocol (no TLV)
//! - `worker_specific_hashrate_tracking/`: Worker-Specific Hashrate Tracking extension (uses TLV)
//!
//! For further information about the extensions, please refer to:
//! - [Extensions Negotiation Spec](https://github.com/stratum-mining/sv2-spec/blob/main/extensions/extensions-negotiation.md)
//! - [Worker-Specific Hashrate Tracking Spec](https://github.com/stratum-mining/sv2-spec/blob/main/extensions/worker-specific-hashrate-tracking.md)

#![no_std]

extern crate alloc;

// Generic TLV encoding/decoding utilities
pub mod tlv;

// Extensions Negotiation (0x0001) - has no TLV fields
pub mod extensions_negotiation;

// Worker-Specific Hashrate Tracking (0x0002) - uses TLV fields
pub mod worker_specific_hashrate_tracking;

// Re-export commonly used items from extensions_negotiation
pub use extensions_negotiation::{
    RequestExtensions, RequestExtensionsError, RequestExtensionsSuccess,
    CHANNEL_BIT_REQUEST_EXTENSIONS, CHANNEL_BIT_REQUEST_EXTENSIONS_ERROR,
    CHANNEL_BIT_REQUEST_EXTENSIONS_SUCCESS,
    EXTENSION_TYPE as EXTENSION_TYPE_EXTENSIONS_NEGOTIATION, MESSAGE_TYPE_REQUEST_EXTENSIONS,
    MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR, MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
};

// Re-export commonly used items from worker_specific_hashrate_tracking
pub use worker_specific_hashrate_tracking::{
    build_submit_shares_extended_with_user_identity_frame,
    decode_user_identity_from_tlv_bytes,
    encode_user_identity_as_tlv_bytes,
    // extract_worker_identity_from_submit_shares, // Commented out - may be useful later
    UserIdentity,
    EXTENSION_TYPE as EXTENSION_TYPE_WORKER_HASHRATE_TRACKING,
    FIELD_TYPE_USER_IDENTITY as TLV_FIELD_TYPE_USER_IDENTITY,
    MAX_WORKER_ID_LENGTH,
};

// Re-export TLV utilities
pub use tlv::{has_tlv_for_extension, has_valid_tlv_data, Tlv, TlvError, TLV_HEADER_SIZE};
