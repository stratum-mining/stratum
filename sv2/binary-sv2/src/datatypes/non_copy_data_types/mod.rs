// Provides a flexible, low-level interface for representing fixed-size and variable-size byte
// arrays, simplifying serialization and deserialization of cryptographic and protocol data.
//
// The core component is the [`Inner`] type, a wrapper for managing both fixed and variable-length
// data slices or owned values. It offers aliases for commonly used data types like 32-byte hashes
// (`U256`), cryptographic signatures (`Signature`), and dynamically-sized arrays (`B0255`,
// `B064K`).

// # Features
// - **Fixed-size Aliases**: Types like [`U256`], [`Mac`], [`PubKey`], and [`Signature`] represent
//   specific byte sizes, often used in cryptographic contexts or protocol identifiers.
// - **Variable-size Aliases**: Types like [`B032`], [`B0255`], [`Str0255`], [`B064K`], and
//   [`B016M`] handle data with bounded sizes, providing flexibility for dynamic data.
// - **Traits and Conversions**: Implements traits like `From`, `TryFrom`, and `Clone` for
//   seamless transformations between owned and reference-based values.

// # Type Aliases
// - **[`U256`]**: 32-byte cryptographic hash (e.g., SHA-256 or protocol IDs).
// - **[`Mac`]**: 16-byte message authentication code.
// - **[`PubKey`]**: 32-byte Secp256k1 public key x-coordinate.
// - **[`Signature`]**: 64-byte cryptographic signature.
// - **[`B032`], [`B0255`], [`Str0255`]**: Variable-size representations for optional fields or
//   protocol data.

use alloc::{borrow::ToOwned, fmt, string::String};
use core::fmt::Write as _;

mod inner;
mod seq_inner;

pub use inner::Inner;
pub use seq_inner::{Seq0255, Seq064K, Sv2Option};

/// Type alias for a 32-byte slice or owned data (commonly used for cryptographic
/// hashes or IDs) represented using the `Inner` type with fixed-size configuration.
pub type U256<'a> = Inner<'a, true, 32, 0, 0>;
/// Type alias for a 16-byte message authentication code.
pub type Mac<'a> = Inner<'a, true, 16, 0, 0>;
/// Type alias for a 32-byte Secp256k1 public key x-coordinate.
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

fn bytes_to_hex<'a>(bytes: impl IntoIterator<Item = &'a u8>) -> String {
    let mut hex = String::new();
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
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
        let inner = bytes_to_hex(self.as_bytes());
        format!("B0255({inner})")
    }
}

impl Str0255<'_> {
    /// Returns the value as a UTF-8 string if possible, otherwise as a hex string prefixed with 0x.
    pub fn as_utf8_or_hex(&self) -> String {
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => alloc::string::String::from(s),
            Err(_) => format!("0x{}", bytes_to_hex(self.as_bytes())),
        }
    }
}

impl fmt::Display for B064K<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = bytes_to_hex(self.as_bytes());
        write!(f, "B064K({inner})")
    }
}

impl fmt::Display for U256<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = bytes_to_hex(self.as_bytes().iter().rev());
        write!(f, "U256({inner})")
    }
}

impl fmt::Display for Seq0255<'_, U256<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        let as_hex = |item: &U256<'_>| bytes_to_hex(item.as_bytes().iter().rev());
        write!(f, "Seq0255<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", as_hex(&self[0])),
            2 => write!(f, "[{}, {}]", as_hex(&self[0]), as_hex(&self[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[2])
            ),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[len - 2]),
                as_hex(&self[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, B016M<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();

        let as_hex = |item: &B016M<'_>| {
            let hex = bytes_to_hex(item.as_bytes());

            if hex.len() > 500 {
                format!("{}…<truncated {} chars>", &hex[..500], hex.len() - 500)
            } else {
                hex
            }
        };

        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", as_hex(&self[0])),
            2 => write!(f, "[{}, {}]", as_hex(&self[0]), as_hex(&self[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[2])
            ),
            _ => write!(
                f,
                "[{}, {}, … , {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[len - 2]),
                as_hex(&self[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, U256<'_>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        let as_hex = |item: &U256<'_>| bytes_to_hex(item.as_bytes().iter().rev());
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", as_hex(&self[0])),
            2 => write!(f, "[{}, {}]", as_hex(&self[0]), as_hex(&self[1])),
            3 => write!(
                f,
                "[{}, {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[2])
            ),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                as_hex(&self[0]),
                as_hex(&self[1]),
                as_hex(&self[len - 2]),
                as_hex(&self[len - 1])
            ),
        }
    }
}

impl fmt::Display for Seq064K<'_, u16> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", self[0]),
            2 => write!(f, "[{}, {}]", self[0], self[1]),
            3 => write!(f, "[{}, {}, {}]", self[0], self[1], self[2]),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                self[0],
                self[1],
                self[len - 2],
                self[len - 1]
            ),
        }
    }
}
impl fmt::Display for Seq064K<'_, u32> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", self[0]),
            2 => write!(f, "[{}, {}]", self[0], self[1]),
            3 => write!(f, "[{}, {}, {}]", self[0], self[1], self[2]),
            _ => write!(
                f,
                "[{}, {}, ... , {}, {}]",
                self[0],
                self[1],
                self[len - 2],
                self[len - 1]
            ),
        }
    }
}

impl fmt::Display for B032<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let item = bytes_to_hex(self.as_bytes());
        write!(f, "B032({item})")
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

// Attempts to convert a string slice into an owned `Str0255`.
impl TryFrom<&str> for Str0255<'_> {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.as_bytes().try_into()
    }
}
