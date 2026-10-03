//! Defines types, encodings, and conversions between custom datatype and standard Rust type,
//! providing abstractions for encoding, decoding, and error handling of SV2 data types.
//!
//! # Overview
//!
//! Enables conversion between various Rust types and SV2-specific data formats for efficient
//! network communication. Provides utilities to encode and decode data types according to the SV2
//! specifications.
//!
//! ## Type Mappings
//! The following table illustrates how standard Rust types map to their SV2 counterparts:
//!
//! ```txt
//! bool     <-> BOOL
//! u8       <-> U8
//! u16      <-> U16
//! U24      <-> U24
//! u32      <-> U32
//! f32      <-> F32
//! u64      <-> U64     
//! U256     <-> U256
//! Str0255  <-> STRO_255
//! Mac      <-> MAC
//! Signature<-> SIGNATURE
//! B08      <-> B0_255 with the payload capped at 8 bytes, the cap Spec 7.3 of the Template
//! Distribution Protocol places on the coinbase_prefix of NewTemplate
//! B032     <-> B0_32   
//! B0255    <-> B0_255
//! B064K    <-> B0_64K
//! B016M    <-> B0_16M
//! Pubkey   <-> PUBKEY
//! Seq0255  <-> SEQ0_255[T]
//! Seq064K  <-> SEQ0_64K[T]
//! ```
//!
//! `BYTES` is not in this table: it only appears as the length-prefixed frame payload, whose
//! length comes from the frame header, so it is handled by the framing layer (`framing-sv2`)
//! rather than by this crate.
//!
//! # Encoding & Decoding
//!
//! Enables conversion between various Rust types and SV2-specific data formats for efficient
//! network communication. Provides utilities to encode and decode data types according to the SV2
//! specifications.
//!
//! - **to_bytes**: Encodes an SV2 data type into a byte vector.
//! - **to_writer**: Encodes an SV2 data type into a byte slice.
//! - **from_bytes**: Decodes an SV2-encoded byte slice into the specified data type.
//!
//! # Error Handling
//!
//! Defines an `Error` enum for handling failure conditions during encoding, decoding, and data
//! manipulation. Common errors include:
//! - Out-of-bounds accesses
//! - Size mismatches during encoding/decoding
//! - Invalid data representations, such as non-boolean values interpreted as booleans.
//!
//! # Build Options
//!
//! Supports optional features like `no_std` for environments without standard library support.

#![cfg_attr(feature = "no_std", no_std)]

pub use decodable::Decodable as Deserialize;
pub use derive_codec_sv2::{Decodable as Deserialize, Encodable as Serialize};
pub use encodable::Encodable as Serialize;

mod codec;
mod datatypes;
pub use datatypes::{
    B016MOwned, B0255Owned, B032Owned, B064KOwned, B08Owned, EllSwiftPubKey, EllSwiftPubKeyOwned,
    Mac, MacOwned, PubKey, PubKeyOwned, Seq0255, Seq0255Owned, Seq064K, Seq064KOwned, Signature,
    SignatureOwned, Str0255, Str0255Owned, Sv2DataType, Sv2Option, Sv2OptionOwned, U256Owned,
    B016M, B0255, B032, B064K, B08, ERROR_SAMPLE_LEN, U24, U256,
};

pub use crate::codec::{
    decodable::{Decodable, GetMarker},
    encodable::{Encodable, EncodableField},
    Fixed, GetSize, SizeHint,
};

use alloc::vec::Vec;

/// Converts the provided SV2 data type to a byte vector based on the SV2 encoding format.
#[allow(clippy::wrong_self_convention)]
pub fn to_bytes<T: Encodable + GetSize>(src: T) -> Result<Vec<u8>, Error> {
    let mut result = vec![0_u8; src.get_size()];
    src.to_bytes(&mut result)?;
    Ok(result)
}

/// Encodes the SV2 data type to the provided byte slice and returns the number of bytes
/// written.
///
/// `dst` may be larger than the encoded value; the bytes past the returned length are left
/// untouched, so a caller reusing a buffer must only transmit `&dst[..written]`.
#[allow(clippy::wrong_self_convention)]
pub fn to_writer<T: Encodable>(src: T, dst: &mut [u8]) -> Result<usize, Error> {
    src.to_bytes(dst)
}

/// Decodes an SV2-encoded byte slice into the specified data type.
pub fn from_bytes<'a, T: Decodable<'a>>(data: &'a mut [u8]) -> Result<T, Error> {
    T::from_bytes(data)
}

/// Provides an interface and implementation details for decoding complex data structures
/// from raw bytes. Handles deserialization of nested and primitive data structures through
/// traits, enums, and helper functions for managing the decoding process.
///
/// # Overview
/// The [`Decodable`] trait serves as the core component, offering methods to define a type's
/// structure, decode raw byte data, and construct instances from decoded fields.
///
/// # Key Concepts and Types
/// - **[`Decodable`] Trait**: Defines methods to decode types from byte data, process individual
///   fields, and construct complete types.
/// - **[`FieldMarker`] and `PrimitiveMarker`**: Enums that represent data types or structures,
///   guiding the decoding process by defining field structures and types.
/// - **[`DecodableField`] and `DecodablePrimitive`**: Represent decoded fields as either primitives
///   or nested structures, forming the building blocks for complex data types.
///
/// # Error Handling
/// Custom error types manage issues during decoding, such as insufficient data or unsupported
/// types. Errors are surfaced through `Result` types to ensure reliability in data parsing tasks.
///
pub mod decodable {
    pub use crate::codec::decodable::{Decodable, DecodableField, FieldMarker};
    //pub use crate::codec::decodable::PrimitiveMarker;
}

/// Provides an encoding framework for serializing various data types into bytes.
///
/// The [`Encodable`] trait is the core of this framework, enabling types to define
/// how they serialize data into bytes. This is essential for transmitting data
/// between components or systems in a consistent, byte-oriented format.
///
/// ## Overview
///
/// Supports a wide variety of data types, including basic types (e.g., integers,
/// booleans, and byte arrays) and complex structures. Each type’s encoding logic is
/// encapsulated in enums like [`EncodablePrimitive`] and [`EncodableField`], enabling
/// structured and hierarchical data serialization.
///
/// ### Key Types
///
/// - **[`Encodable`]**: Defines methods for converting an object into a byte array. It supports
///   both primitive types and complex structures.
/// - **[`EncodablePrimitive`]**: Represents basic types that can be serialized directly. Includes
///   data types like integers, booleans, and byte arrays.
/// - **[`EncodableField`]**: Extends [`EncodablePrimitive`] to support structured and nested data,
///   enabling recursive encoding of complex structures.
///
/// ## Error Handling
///
/// Errors during encoding are handled through the [`Error`] type. Common failure scenarios include
/// buffer overflows and type-specific serialization errors. Each encoding method returns an
/// appropriate error if encoding fails, supporting comprehensive error management.
///
/// ## Trait Details
///
/// ### [`Encodable`]
/// - **`to_bytes`**: Encodes the instance into a byte slice, returning the number of bytes written
///   or an error if encoding fails.
///
/// ### Additional Enums and Methods
///
/// Includes utility types and methods for calculating sizes, encoding hierarchical data,
/// and supporting both owned and reference-based data variants.
///
/// - **[`EncodablePrimitive`]**: Handles encoding logic for primitive types, addressing
///   serialization requirements specific to each type.
/// - **[`EncodableField`]**: Extends encoding to support composite types and structured data,
///   enabling recursive encoding of nested structures.
///
/// ## Summary
///
/// Designed for flexibility and extensibility, this module supports a wide range of data
/// serialization needs through customizable encoding strategies. Implementing the
/// [`Encodable`] trait for custom types ensures efficient and consistent data serialization
/// across applications.
pub mod encodable {
    pub use crate::codec::encodable::{Encodable, EncodableField, EncodablePrimitive};
}

#[macro_use]
extern crate alloc;

/// Error types used within the protocol library to indicate various failure conditions.
#[derive(PartialEq, Eq)]
pub enum Error {
    /// Indicates an attempt to read beyond a valid range.
    OutOfBound,

    /// Occurs when an unexpected size mismatch arises during a write operation, specifying
    /// expected and actual sizes.
    WriteError(usize, usize),

    /// Indicates an invalid `u24` representation.
    InvalidU24(u32),

    /// Generic conversion error related to primitive types.
    PrimitiveConversionError,

    /// Error occurring during decoding due to conversion issues.
    DecodableConversionError,

    /// Error triggered when a decoder is used without initialization.
    UnInitializedDecoder,

    /// Raised when an unexpected mismatch occurs during read operations, specifying expected and
    /// actual read sizes.
    ReadError(usize, usize),

    /// Used as a marker error for fields that should remain void or empty.
    VoidFieldMarker,

    /// Signifies a value overflow based on protocol restrictions, containing details about
    /// fixed/variable size, maximum size allowed, and the offending length.
    ///
    /// The `Vec<u8>` is a bounded diagnostic sample holding at most the first [`ERROR_SAMPLE_LEN`]
    /// bytes of the offending value, or only the length prefix when the overflow is detected from
    /// a declared encoded length. The final field reports the complete offending length.
    ValueExceedsMaxSize(bool, usize, usize, usize, Vec<u8>, usize),

    /// Triggered when a sequence type (`Seq0255`, `Seq064K`) exceeds its maximum allowable size.
    SeqExceedsMaxSize,

    /// Raised when no valid decodable field is provided during decoding.
    NoDecodableFieldPassed,

    /// Error for protocol-specific invalid values.
    ValueIsNotAValidProtocol(u8),

    /// Indicates a protocol constraint violation where `Sv2Option` unexpectedly contains multiple
    /// elements.
    Sv2OptionHaveMoreThenOneElement(u8),
}

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::OutOfBound => f.write_str("OutOfBound"),
            Error::WriteError(expected, actual) => f
                .debug_tuple("WriteError")
                .field(expected)
                .field(actual)
                .finish(),
            Error::InvalidU24(value) => f.debug_tuple("InvalidU24").field(value).finish(),
            Error::PrimitiveConversionError => f.write_str("PrimitiveConversionError"),
            Error::DecodableConversionError => f.write_str("DecodableConversionError"),
            Error::UnInitializedDecoder => f.write_str("UnInitializedDecoder"),
            Error::ReadError(expected, actual) => f
                .debug_tuple("ReadError")
                .field(expected)
                .field(actual)
                .finish(),
            Error::VoidFieldMarker => f.write_str("VoidFieldMarker"),
            Error::ValueExceedsMaxSize(is_fixed, size, header_size, max_size, _, actual_size) => f
                .debug_tuple("ValueExceedsMaxSize")
                .field(is_fixed)
                .field(size)
                .field(header_size)
                .field(max_size)
                .field(&format_args!("[redacted]"))
                .field(actual_size)
                .finish(),
            Error::SeqExceedsMaxSize => f.write_str("SeqExceedsMaxSize"),
            Error::NoDecodableFieldPassed => f.write_str("NoDecodableFieldPassed"),
            Error::ValueIsNotAValidProtocol(value) => f
                .debug_tuple("ValueIsNotAValidProtocol")
                .field(value)
                .finish(),
            Error::Sv2OptionHaveMoreThenOneElement(value) => f
                .debug_tuple("Sv2OptionHaveMoreThenOneElement")
                .field(value)
                .finish(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use alloc::{string::ToString, vec};

    // The variant is public, so a downstream crate can construct it with a payload larger than
    // the in-crate cap. Display must not write that payload out regardless.
    #[test]
    fn binary_error_display_does_not_dump_embedded_payload() {
        let sample = vec![0xAB_u8; 64 * 1024];
        let err = Error::ValueExceedsMaxSize(false, 1, 1, 32, sample, 64 * 1024);

        let rendered = err.to_string();

        assert!(
            !rendered.contains("171"),
            "Display leaked sample bytes: {rendered}"
        );
        assert!(
            rendered.len() < 128,
            "Display grew with the sample: {} bytes",
            rendered.len()
        );
        assert!(rendered.contains("[redacted]"));
        assert!(rendered.contains("65536"));
    }

    #[test]
    fn binary_error_debug_does_not_dump_embedded_payload() {
        let sample = vec![0xAB_u8; super::ERROR_SAMPLE_LEN];
        let err = Error::ValueExceedsMaxSize(false, 1, 1, 32, sample, super::ERROR_SAMPLE_LEN + 1);

        let rendered = alloc::format!("{err:?}");

        assert!(
            !rendered.contains("171"),
            "Debug leaked sample bytes: {rendered}"
        );
        assert_eq!(
            rendered,
            "ValueExceedsMaxSize(false, 1, 1, 32, [redacted], 33)"
        );
    }
}
