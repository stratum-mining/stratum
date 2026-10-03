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

pub(crate) mod inner;
mod seq_inner;

use inner::HexPrefix;
pub use inner::ERROR_SAMPLE_LEN;
pub(crate) use inner::{Inner, InnerOwned};
pub use seq_inner::{Seq0255, Seq0255Owned, Seq064K, Seq064KOwned, Sv2Option, Sv2OptionOwned};

/// Type alias for a 32-byte slice or owned data (commonly used for cryptographic
/// hashes or IDs) represented using the `Inner` type with fixed-size configuration.
pub type U256<'a> = Inner<'a, true, 32, 0, 0>;
pub type U256Owned = InnerOwned<true, 32, 0, 0>;
/// Type alias for a 16-byte message authentication code.
pub type Mac<'a> = Inner<'a, true, 16, 0, 0>;
pub type MacOwned = InnerOwned<true, 16, 0, 0>;
/// Type alias for a 32-byte Secp256k1 public key x-coordinate.
pub type PubKey<'a> = Inner<'a, true, 32, 0, 0>;
pub type PubKeyOwned = InnerOwned<true, 32, 0, 0>;
/// Type alias for a 64-byte ElligatorSwift encoded Secp256k1 public key x-coordinate
/// (see BIP 324), as exchanged during the Noise handshake.
pub type EllSwiftPubKey<'a> = Inner<'a, true, 64, 0, 0>;
pub type EllSwiftPubKeyOwned = InnerOwned<true, 64, 0, 0>;
/// Type alias for a 64-byte cryptographic signature represented using the
/// `Inner` type with fixed-size configuration.
pub type Signature<'a> = Inner<'a, true, 64, 0, 0>;
pub type SignatureOwned = InnerOwned<true, 64, 0, 0>;
/// Type alias for a variable-sized byte array with a maximum size of 32 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type B032<'a> = Inner<'a, false, 1, 1, 32>;
pub type B032Owned = InnerOwned<false, 1, 1, 32>;
/// Type alias for a variable-sized byte array with a maximum size of 255 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type B0255<'a> = Inner<'a, false, 1, 1, 255>;
pub type B0255Owned = InnerOwned<false, 1, 1, 255>;
/// Type alias for a variable-sized string with a maximum size of 255 bytes,
/// represented using the `Inner` type with a 1-byte header.
pub type Str0255<'a> = Inner<'a, false, 1, 1, 255>;
pub type Str0255Owned = InnerOwned<false, 1, 1, 255>;
/// Type alias for a variable-sized byte array with a maximum size of 64 KB,
/// represented using the `Inner` type with a 2-byte header.
pub type B064K<'a> = Inner<'a, false, 1, 2, { u16::MAX as usize }>;
pub type B064KOwned = InnerOwned<false, 1, 2, { u16::MAX as usize }>;
/// Type alias for a variable-sized byte array with a maximum size of ~16 MB,
/// represented using the `Inner` type with a 3-byte header.
pub type B016M<'a> = Inner<'a, false, 1, 3, { 2_usize.pow(24) - 1 }>;
pub type B016MOwned = InnerOwned<false, 1, 3, { 2_usize.pow(24) - 1 }>;
/// Type alias for a variable-sized byte array with a maximum size of 8 bytes,
/// represented using the `Inner` type with a 1-byte header.
///
/// Not a distinct wire type: it shares the `B0_255` encoding, and additionally enforces the
/// Sv2 spec constraint that a `coinbase_prefix` payload is up to 8 bytes (not including the
/// length byte), as described for `NewTemplate` (Template Distribution Protocol 7.3).
pub type B08<'a> = Inner<'a, false, 1, 1, 8>;
pub type B08Owned = InnerOwned<false, 1, 1, 8>;

fn bytes_to_hex<'a>(bytes: impl IntoIterator<Item = &'a u8>) -> String {
    let mut hex = String::new();
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

impl fmt::Display for Sv2Option<'_, u32> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.to_owned().into_inner();
        match inner {
            Some(value) => write!(f, "Sv2Option({value})"),
            None => write!(f, "Sv2Option(None)"),
        }
    }
}

impl fmt::Display for Sv2OptionOwned<u32> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.clone().into_inner();
        match inner {
            Some(value) => write!(f, "Sv2Option({value})"),
            None => write!(f, "Sv2Option(None)"),
        }
    }
}

impl B0255<'_> {
    pub fn as_hex(&self) -> String {
        format!("B0255({})", HexPrefix(self.as_bytes()))
    }
}

impl B0255Owned {
    pub fn as_hex(&self) -> String {
        format!("B0255({})", HexPrefix(self.as_bytes()))
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

impl Str0255Owned {
    /// Returns the value as a UTF-8 string if possible, otherwise as a hex string prefixed with 0x.
    pub fn as_utf8_or_hex(&self) -> String {
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => alloc::string::String::from(s),
            Err(_) => format!("0x{}", bytes_to_hex(self.as_bytes())),
        }
    }
}

impl B08<'_> {
    pub fn as_hex(&self) -> String {
        format!("B08({})", HexPrefix(self.as_bytes()))
    }
}

impl B08Owned {
    pub fn as_hex(&self) -> String {
        format!("B08({})", HexPrefix(self.as_bytes()))
    }
}

impl fmt::Display for B064K<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B064K({})", HexPrefix(self.as_bytes()))
    }
}

impl fmt::Display for B064KOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B064K({})", HexPrefix(self.as_bytes()))
    }
}

impl fmt::Display for U256<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = bytes_to_hex(self.as_bytes().iter().rev());
        write!(f, "U256({inner})")
    }
}

impl fmt::Display for U256Owned {
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

impl fmt::Display for Seq0255Owned<U256Owned> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        let as_hex = |item: &U256Owned| bytes_to_hex(item.as_bytes().iter().rev());
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

        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", HexPrefix(self[0].as_bytes())),
            2 => write!(
                f,
                "[{}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes())
            ),
            3 => write!(
                f,
                "[{}, {}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes()),
                HexPrefix(self[2].as_bytes())
            ),
            _ => write!(
                f,
                "[{}, {}, … , {}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes()),
                HexPrefix(self[len - 2].as_bytes()),
                HexPrefix(self[len - 1].as_bytes())
            ),
        }
    }
}

impl fmt::Display for Seq064KOwned<B016MOwned> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();

        write!(f, "Seq064K<len={len}: ")?;
        match len {
            0 => write!(f, "[]"),
            1 => write!(f, "[{}]", HexPrefix(self[0].as_bytes())),
            2 => write!(
                f,
                "[{}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes())
            ),
            3 => write!(
                f,
                "[{}, {}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes()),
                HexPrefix(self[2].as_bytes())
            ),
            _ => write!(
                f,
                "[{}, {}, … , {}, {}]",
                HexPrefix(self[0].as_bytes()),
                HexPrefix(self[1].as_bytes()),
                HexPrefix(self[len - 2].as_bytes()),
                HexPrefix(self[len - 1].as_bytes())
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

impl fmt::Display for Seq064KOwned<U256Owned> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.len();
        let as_hex = |item: &U256Owned| bytes_to_hex(item.as_bytes().iter().rev());
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

impl fmt::Display for Seq064KOwned<u16> {
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

impl fmt::Display for Seq064KOwned<u32> {
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

impl fmt::Display for B032Owned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let item = bytes_to_hex(self.as_bytes());
        write!(f, "B032({item})")
    }
}

use core::convert::{TryFrom, TryInto};

// Attempts to convert a `String` into an owned `Str0255`.
impl TryFrom<String> for Str0255Owned {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.into_bytes().try_into()
    }
}

// Attempts to convert a string slice into an owned `Str0255`.
impl TryFrom<&str> for Str0255Owned {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.as_bytes().try_into()
    }
}

impl<'a> TryFrom<&'a str> for Str0255<'a> {
    type Error = crate::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        value.as_bytes().try_into()
    }
}

impl<'a> TryFrom<&'a String> for Str0255<'a> {
    type Error = crate::Error;

    fn try_from(value: &'a String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl<'a> TryFrom<&'a mut String> for Str0255<'a> {
    type Error = crate::Error;

    fn try_from(value: &'a mut String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

#[cfg(test)]
mod test {
    use super::{inner::MAX_DISPLAY_BYTES, B0255Owned, B064KOwned, Seq064K, B016M, B0255, B064K};
    use alloc::{format, string::ToString, vec, vec::Vec};

    #[test]
    fn b064k_display_is_bounded() {
        let full = vec![0xCD_u8; u16::MAX as usize];
        let expected = format!(
            "B064K({}…<truncated 130560 chars>)",
            "cd".repeat(MAX_DISPLAY_BYTES)
        );

        assert_eq!(B064K::new(&full).unwrap().to_string(), expected);
        let owned: B064KOwned = full.try_into().unwrap();
        assert_eq!(owned.to_string(), expected);

        let short = [0x01_u8, 0x02];
        assert_eq!(B064K::new(&short).unwrap().to_string(), "B064K(0102)");
    }

    #[test]
    fn b0255_as_hex_is_never_truncated() {
        let full = vec![0xEF_u8; 255];
        let expected = format!("B0255({})", "ef".repeat(255));

        assert_eq!(B0255::new(&full).unwrap().as_hex(), expected);
        let owned: B0255Owned = full.try_into().unwrap();
        assert_eq!(owned.as_hex(), expected);

        let short = [0xAB_u8];
        assert_eq!(B0255::new(&short).unwrap().as_hex(), "B0255(ab)");
    }

    #[test]
    fn seq064k_b016m_display_is_unchanged() {
        let long = vec![0x11_u8; MAX_DISPLAY_BYTES + 1];
        let short = [0x22_u8, 0x33];
        let items: Vec<B016M<'_>> = vec![B016M::new(&long).unwrap(), B016M::new(&short).unwrap()];
        let seq = Seq064K::new(items).unwrap();

        assert_eq!(
            seq.to_string(),
            format!(
                "Seq064K<len=2: [{}…<truncated 2 chars>, 2233]",
                "11".repeat(MAX_DISPLAY_BYTES)
            )
        );
    }

    // Derived Debug on sequences and messages goes through the element's Debug, so a
    // B016M-carrying sequence must inherit the bound too.
    #[test]
    fn seq_debug_inherits_the_bound() {
        let big = vec![0x99_u8; 1 << 20];
        let seq = Seq064K::new(vec![B016M::new(&big).unwrap()]).unwrap();

        let rendered = format!("{seq:?}");
        assert!(rendered.contains("len: 1048576"), "{rendered}");
        assert!(
            rendered.contains("…<truncated 2096642 chars>"),
            "{rendered}"
        );
        assert!(
            rendered.len() < 2 * MAX_DISPLAY_BYTES + 200,
            "{}",
            rendered.len()
        );
    }
}
