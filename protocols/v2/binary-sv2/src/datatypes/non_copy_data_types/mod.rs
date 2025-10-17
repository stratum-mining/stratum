// Provides a flexible, low-level interface for representing fixed-size and variable-size byte
// arrays, simplifying serialization and deserialization of cryptographic and protocol data.
//
// The core component is the [`Inner`] type, a wrapper for managing both fixed and variable-length
// data slices or owned values. It offers aliases for commonly used data types like 32-byte hashes
// (`U256`), public keys (`PubKey`), cryptographic signatures (`Signature`), and dynamically-sized
// arrays (`B0255`, `B064K`).

// # Features
// - **Fixed-size Aliases**: Types like [`U32AsRef`], [`U256`], [`PubKey`], and [`Signature`]
//   represent specific byte sizes, often used in cryptographic contexts or protocol identifiers.
// - **Variable-size Aliases**: Types like [`B032`], [`B0255`], [`Str0255`], [`B064K`], and
//   [`B016M`] handle data with bounded sizes, providing flexibility for dynamic data.
// - **Traits and Conversions**: Implements traits like `From`, `TryFrom`, and [`IntoOwned`] for
//   seamless transformations between owned and reference-based values.
// - **Property Testing** (optional, requires the `prop_test` feature): Supports generating
//   arbitrary test data for property-based testing.

// # Type Aliases
// - **[`U32AsRef`]**: 4-byte representation for small identifiers or integer values.
// - **[`U256`]**: 32-byte cryptographic hash (e.g., SHA-256 or protocol IDs).
// - **[`PubKey`]**: 32-byte public key (e.g., Ed25519).
// - **[`Signature`]**: 64-byte cryptographic signature.
// - **[`B032`], [`B0255`], [`Str0255`]**: Variable-size representations for optional fields or
//   protocol data.

// # Feature Flags
// - **`prop_test`**: Enables property-based testing with the `quickcheck` crate. When enabled,
//   types like `U256` and `B016M` gain methods to generate arbitrary test data for testing
//   serialization and deserialization.
#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

#[cfg(feature = "prop_test")]
use alloc::vec::Vec;
use alloc::{borrow::ToOwned, fmt, string::String};

mod inner;
mod seq_inner;

#[allow(dead_code)]
trait IntoOwned {
    fn into_owned(self) -> Self;
}

pub use inner::Inner;
pub use seq_inner::{Seq0255, Seq064K, Sv2Option};

/// Type alias for a 4-byte slice or owned data represented using the `Inner`
/// type with fixed-size configuration.
pub type U32AsRef<'a> = Inner<'a, true, 4, 0, 0>;
/// Type alias for a 32-byte slice or owned data (commonly used for cryptographic
/// hashes or IDs) represented using the `Inner` type with fixed-size configuration.
pub type U256<'a> = Inner<'a, true, 32, 0, 0>;
/// Type alias for a 32-byte public key represented using the `Inner` type
/// with fixed-size configuration.
pub type PubKey<'a> = Inner<'a, true, 32, 0, 0>;
/// Type alias for a 64-byte cryptographic signature represented using the
/// `Inner` type with fixed-size configuration.
pub type Signature<'a> = Inner<'a, true, 64, 0, 0>;
/// Type alias for a variable-sized byte array with a maximum size of 32 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type B032<'a> = Inner<'a, false, 1, 1, 32>;
/// Type alias for a variable-sized byte array with a maximum size of 255 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type B0255<'a> = Inner<'a, false, 1, 1, 255>;
/// Type alias for a variable-sized string with a maximum size of 255 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type Str0255<'a> = Inner<'a, false, 1, 1, 255>;
/// Type alias for a variable-sized byte array with a maximum size of 64 KB,
/// represented using the `Inner` type with a 2-byte header.
pub type B064K<'a> = Inner<'a, false, 1, 2, { u16::MAX as usize }>;
/// Type alias for a variable-sized byte array with a maximum size of ~16 MB,
/// represented using the `Inner` type with a 3-byte header.
pub type B016M<'a> = Inner<'a, false, 1, 3, { 2_usize.pow(24) - 1 }>;

impl fmt::Display for U32AsRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner_as_ref();
        write!(
            f,
            "U32AsRef({})",
            u32::from_le_bytes([inner[0], inner[1], inner[2], inner[3]])
        )
    }
}

impl fmt::Display for Sv2Option<'_, u32> {
    // internally Sv2Option is pub struct Sv2Option<'a, T>(pub Vec<T>, PhantomData<&'a T>);
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.to_owned().into_inner();
        match inner {
            Some(value) => write!(f, "Sv2Option({value})"),
            None => write!(f, "Sv2Option(None)"),
        }
    }
}

impl B0255<'_> {
    pub fn as_hex(&self) -> String {
        let inner = self
            .inner_as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("B0255({inner})")
    }
}

impl Str0255<'_> {
    /// Returns the value as a UTF-8 string if possible, otherwise as a hex string prefixed with 0x.
    pub fn as_utf8_or_hex(&self) -> String {
        match core::str::from_utf8(self.inner_as_ref()) {
            Ok(s) => alloc::string::String::from(s),
            Err(_) => format!(
                "0x{}",
                self.inner_as_ref()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
        }
    }
}

impl fmt::Display for B064K<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self
            .inner_as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write!(f, "B064K({inner})")
    }
}

impl fmt::Display for U256<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self
            .inner_as_ref()
            .iter()
            .rev()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write!(f, "U256({inner})")
    }
}

impl fmt::Display for Seq0255<'_, U256<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.0.len();
        let as_hex = |item: &U256<'_>| {
            item.inner_as_ref()
                .iter()
                .rev()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        write!(f, "Seq0255<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "{}]", as_hex(&self.0[0])),
            2 => write!(f, "{}, {}]", as_hex(&self.0[0]), as_hex(&self.0[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[2])
            ),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[len - 2]),
                as_hex(&self.0[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, B016M<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.0.len();

        let as_hex = |item: &B016M<'_>| {
            let hex: String = item
                .inner_as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();

            if hex.len() > 500 {
                format!("{}…<truncated {} chars>", &hex[..500], hex.len() - 500)
            } else {
                hex
            }
        };

        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", as_hex(&self.0[0])),
            2 => write!(f, "[{}, {}]", as_hex(&self.0[0]), as_hex(&self.0[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[2])
            ),
            _ => write!(
                f,
                "[{}, {}, … , {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[len - 2]),
                as_hex(&self.0[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, U256<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.0.len();
        let as_hex = |item: &U256<'_>| {
            item.inner_as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", as_hex(&self.0[0])),
            2 => write!(f, "[{}, {}]", as_hex(&self.0[0]), as_hex(&self.0[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[2])
            ),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                as_hex(&self.0[0]),
                as_hex(&self.0[1]),
                as_hex(&self.0[len - 2]),
                as_hex(&self.0[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, u16> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.0.len();
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", self.0[0]),
            2 => write!(f, "[{}, {}]", self.0[0], self.0[1]),
            3 => write!(f, "[{}, {}, {}]", self.0[0], self.0[1], self.0[2]),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                self.0[0],
                self.0[1],
                self.0[len - 2],
                self.0[len - 1]
            ),
        }
    }
}
impl fmt::Display for Seq064K<'_, u32> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.0.len();
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", self.0[0]),
            2 => write!(f, "[{}, {}]", self.0[0], self.0[1]),
            3 => write!(f, "[{}, {}, {}]", self.0[0], self.0[1], self.0[2]),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                self.0[0],
                self.0[1],
                self.0[len - 2],
                self.0[len - 1]
            ),
        }
    }
}

impl fmt::Display for B032<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let item = self
            .inner_as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        write!(f, "B032({item})")
    }
}

impl From<[u8; 32]> for U256<'_> {
    fn from(v: [u8; 32]) -> Self {
        Inner::Owned(v.into())
    }
}

#[cfg(feature = "prop_test")]
impl<'a> U256<'a> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let mut inner = Vec::<u8>::arbitrary(g);
        inner.resize(32, 0);
        // 32 Bytes arrays are always converted into U256 unwrap never panic
        let inner: [u8; 32] = inner.try_into().unwrap();
        inner.into()
    }
}

#[cfg(feature = "prop_test")]
impl<'a> B016M<'a> {
    pub fn from_gen(g: &mut Gen) -> Self {
        // This can fail but is used only for tests purposes
        Vec::<u8>::arbitrary(g).try_into().unwrap()
    }
}

use core::convert::{TryFrom, TryInto};

// Attempts to convert a `String` into a `Str0255<'a>`.
impl TryFrom<String> for Str0255<'_> {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.into_bytes().try_into()
    }
}

/// Represents a reference to a 32-bit unsigned integer (`u32`),
/// providing methods for convenient conversions.
impl U32AsRef<'_> {
    /// Returns the `u32` value represented by this reference.
    pub fn as_u32(&self) -> u32 {
        let inner = self.inner_as_ref();
        u32::from_le_bytes([inner[0], inner[1], inner[2], inner[3]])
    }
}

impl From<u32> for U32AsRef<'_> {
    fn from(v: u32) -> Self {
        let bytes = v.to_le_bytes();
        let inner = vec![bytes[0], bytes[1], bytes[2], bytes[3]];
        U32AsRef::Owned(inner)
    }
}

impl<'a> From<&'a U32AsRef<'a>> for u32 {
    fn from(v: &'a U32AsRef<'a>) -> Self {
        let b = v.inner_as_ref();
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}
