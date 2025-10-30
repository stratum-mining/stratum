//! Worker-Specific Hashrate Tracking (extension_type=0x0002)
//!
//! This extension enables tracking of individual worker hashrates within extended channels
//! by appending TLV-encoded worker identity fields to SubmitSharesExtended messages.

mod tlv;
mod user_identity;

pub use tlv::{
    build_submit_shares_extended_with_user_identity_frame,
    decode_user_identity_from_tlv_bytes,
    encode_user_identity_as_tlv_bytes,
    // extract_worker_identity_from_submit_shares, // Commented out - may be useful later
};
pub use user_identity::{UserIdentity, MAX_WORKER_ID_LENGTH};

/// Extension type for Worker-Specific Hashrate Tracking
pub const EXTENSION_TYPE: u16 = 0x0002;

/// TLV field type for user identity within this extension
pub const FIELD_TYPE_USER_IDENTITY: u8 = 0x01;
