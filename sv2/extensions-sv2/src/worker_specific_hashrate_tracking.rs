//! Worker-Specific Hashrate Tracking (extension_type=0x0002)
//!
//! This extension enables tracking of individual worker hashrates within aggregated extended channels
//! by appending TLV-encoded worker identity fields to SubmitSharesExtended messages.

use alloc::{
    fmt,
    string::{String, ToString},
    vec::Vec,
};

/// Maximum length for user identity in bytes as per the spec.
pub const MAX_USER_IDENTITY_LENGTH: usize = 32;

/// Extension type for Worker-Specific Hashrate Tracking
pub const EXTENSION_TYPE: u16 = 0x0002;

/// TLV field type for user identity within this extension
pub const FIELD_TYPE_USER_IDENTITY: u8 = 0x01;

/// UserIdentity for Worker-Specific Hashrate Tracking.
///
/// This structure represents a UserIdentity that can be appended to
/// `SubmitSharesExtended` messages via TLV encoding when the Worker-Specific Hashrate Tracking
/// extension (0x0002) is negotiated.
///
/// The UserIdentity is stored as raw UTF-8 bytes (max 32 bytes) to
/// match the TLV specification exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// UserIdentity as raw UTF-8 bytes (max 32 bytes).
    ///
    /// The TLV Value field contains these raw bytes directly.
    pub(crate) user_identity: Vec<u8>,
}

impl UserIdentity {
    /// Creates a new UserIdentity from a string.
    pub fn new(user_identity: &str) -> Result<Self, &'static str> {
        Self::from_bytes(user_identity.as_bytes())
    }

    /// Creates a UserIdentity directly from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_USER_IDENTITY_LENGTH {
            return Err("UserIdentity exceeds 32 bytes");
        }
        Ok(Self {
            user_identity: bytes.to_vec(),
        })
    }

    /// Returns the UserIdentity as a string slice (if valid UTF-8).
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.user_identity).ok()
    }

    /// Returns the UserIdentity as a String (if valid UTF-8) or hex representation.
    pub fn as_string_or_hex(&self) -> String {
        match core::str::from_utf8(&self.user_identity) {
            Ok(s) => s.to_string(),
            Err(_) => {
                let mut hex = String::from("0x");
                for byte in &self.user_identity {
                    use core::fmt::Write;
                    let _ = write!(&mut hex, "{:02x}", byte);
                }
                hex
            }
        }
    }

    /// Returns the raw bytes of the UserIdentity.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.user_identity.as_slice()
    }

    /// Returns the length of the UserIdentity in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.user_identity.as_slice().len()
    }

    /// Returns true if the UserIdentity is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.user_identity.is_empty()
    }
}

impl fmt::Display for UserIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UserIdentity({})", self.as_string_or_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_user_identity() {
        let msg = UserIdentity::new("Worker_001").unwrap();
        assert_eq!(msg.as_str().unwrap(), "Worker_001");
        assert_eq!(msg.len(), 10);
    }

    #[test]
    fn test_user_identity_max_length() {
        let worker_name = "Worker_1234567890123456789012345";
        assert_eq!(worker_name.len(), MAX_USER_IDENTITY_LENGTH);

        let msg = UserIdentity::new(worker_name).unwrap();
        assert_eq!(msg.as_str().unwrap(), worker_name);
        assert_eq!(msg.len(), 32);
    }

    #[test]
    fn test_user_identity_short() {
        let msg = UserIdentity::new("W1").unwrap();
        assert_eq!(msg.as_str().unwrap(), "W1");
        assert_eq!(msg.len(), 2);
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_user_identity_empty() {
        let msg = UserIdentity::new("").unwrap();
        assert!(msg.is_empty());
        assert_eq!(msg.len(), 0);
    }

    #[test]
    fn test_user_identity_too_long() {
        let too_long = "x".repeat(33);
        let result = UserIdentity::new(&too_long);
        assert!(result.is_err());
    }

    #[test]
    fn test_as_string_or_hex() {
        let msg = UserIdentity::new("Worker").unwrap();
        assert_eq!(msg.as_string_or_hex(), "Worker");

        // Test with invalid UTF-8
        let invalid_utf8 = UserIdentity {
            user_identity: vec![0xFF, 0xFE],
        };
        assert!(invalid_utf8.as_string_or_hex().starts_with("0x"));
    }

    #[test]
    fn test_from_bytes() {
        let bytes = b"Worker_123";
        let identity = UserIdentity::from_bytes(bytes).unwrap();
        assert_eq!(identity.as_bytes(), bytes);
        assert_eq!(identity.len(), 10);
    }

    #[test]
    fn test_from_bytes_too_long() {
        let bytes = [b'x'; MAX_USER_IDENTITY_LENGTH + 1];
        let result = UserIdentity::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_display() {
        let identity = UserIdentity::new("TestWorker").unwrap();
        let display = alloc::format!("{}", identity);
        assert!(display.contains("TestWorker"));
    }
}
