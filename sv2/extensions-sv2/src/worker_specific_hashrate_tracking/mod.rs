//! Worker-Specific Hashrate Tracking (extension_type=0x0002)
//!
//! This extension enables tracking of individual worker hashrates within aggregated extended channels
//! by appending TLV-encoded worker identity fields to SubmitSharesExtended messages.

mod tlvs;

pub use tlvs::{UserIdentity, MAX_USER_IDENTITY_LENGTH};

/// Extension type for Worker-Specific Hashrate Tracking
pub const EXTENSION_TYPE: u16 = 0x0002;

/// TLV field type for user identity within this extension
pub const FIELD_TYPE_USER_IDENTITY: u8 = 0x01;
