use alloc::{
    fmt,
    string::{String, ToString},
    vec::Vec,
};

/// Maximum length for worker identifiers in bytes as per the spec.
pub const MAX_WORKER_ID_LENGTH: usize = 32;

/// Worker identity for hashrate tracking.
///
/// This structure represents a worker identifier that can be appended to
/// `SubmitSharesExtended` messages via TLV encoding when the Worker-Specific
/// Hashrate Tracking extension (0x0002) is negotiated.
///
/// The worker identity is stored as raw UTF-8 bytes (max 32 bytes) to match
/// the TLV specification exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// Worker identifier as raw UTF-8 bytes (max 32 bytes).
    ///
    /// The TLV Value field contains these raw bytes directly.
    pub(crate) user_identity: Vec<u8>,
}

impl UserIdentity {
    /// Creates a new UserIdentity from a string.
    ///
    /// # Arguments
    /// * `worker_name` - The worker identifier string (max 32 bytes when encoded as UTF-8)
    ///
    /// # Returns
    /// * `Ok(UserIdentity)` - If the worker name is valid (≤32 bytes)
    /// * `Err(&'static str)` - If the worker name exceeds 32 bytes
    ///
    /// # Example
    ///
    /// ```ignore
    /// use extensions_sv2::UserIdentity;
    ///
    /// let worker = UserIdentity::new("Worker_001")?;
    /// ```
    pub fn new(worker_name: &str) -> Result<Self, &'static str> {
        let bytes = worker_name.as_bytes();
        if bytes.len() > MAX_WORKER_ID_LENGTH {
            return Err("Worker name exceeds 32 bytes");
        }
        Ok(Self {
            user_identity: bytes.to_vec(),
        })
    }

    /// Returns the worker name as a string slice (if valid UTF-8).
    ///
    /// # Returns
    /// * `Some(&str)` - If the worker identity is valid UTF-8
    /// * `None` - If the worker identity contains invalid UTF-8
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.user_identity).ok()
    }

    /// Returns the worker name as a String (if valid UTF-8) or hex representation.
    ///
    /// This is useful for logging and display purposes where you always want
    /// a string representation, even if the worker identity contains invalid UTF-8.
    ///
    /// # Returns
    /// * UTF-8 string if valid
    /// * Hex string (prefixed with "0x") if invalid UTF-8
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

    /// Returns the raw bytes of the worker identity.
    ///
    /// This is useful when you need to serialize the identity or work with
    /// the raw bytes directly.
    pub fn as_bytes(&self) -> &[u8] {
        &self.user_identity
    }

    /// Returns the length of the worker identity in bytes.
    pub fn len(&self) -> usize {
        self.user_identity.len()
    }

    /// Returns true if the worker identity is empty.
    pub fn is_empty(&self) -> bool {
        self.user_identity.is_empty()
    }
}

impl fmt::Display for UserIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UserIdentity(user_identity: {})",
            self.as_string_or_hex()
        )
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
        assert_eq!(worker_name.len(), MAX_WORKER_ID_LENGTH);

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
}
