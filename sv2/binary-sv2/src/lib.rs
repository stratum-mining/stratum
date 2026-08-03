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
//! f32      <-> F32     // Not in the spec, but used
//! u64      <-> U64     
//! U256     <-> U256
//! Str0255  <-> STRO_255
//! Mac      <-> MAC
//! Signature<-> SIGNATURE
//! B032     <-> B0_32   
//! B0255    <-> B0_255
//! B064K    <-> B0_64K
//! B016M    <-> B0_16M
//! [u8]     <-> BYTES
//! Pubkey   <-> PUBKEY
//! Seq0255  <-> SEQ0_255[T]
//! Seq064K  <-> SEQ0_64K[T]
//! ```
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
    B016MOwned, B0255Owned, B032Owned, B064KOwned, Mac, MacOwned, PubKey, PubKeyOwned, Seq0255,
    Seq0255Owned, Seq064K, Seq064KOwned, Signature, SignatureOwned, Str0255, Str0255Owned,
    Sv2DataType, Sv2Option, Sv2OptionOwned, U256Owned, B016M, B0255, B032, B064K, U24, U256,
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

/// Encodes the SV2 data type to the provided byte slice.
#[allow(clippy::wrong_self_convention)]
pub fn to_writer<T: Encodable>(src: T, dst: &mut [u8]) -> Result<(), Error> {
    src.to_bytes(dst)?;
    Ok(())
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
#[derive(Debug, PartialEq, Eq)]
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
    /// fixed/variable size, maximum size allowed, and the offending value details.
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

/// Vec<u8> is used as the Sv2 type Bytes
impl GetSize for Vec<u8> {
    fn get_size(&self) -> usize {
        self.len()
    }
}

impl From<Vec<u8>> for EncodableField<'_> {
    fn from(v: Vec<u8>) -> Self {
        EncodableField::Struct(v.into_iter().map(Into::into).collect())
    }
}

#[cfg(feature = "with_buffer_pool")]
impl From<buffer_sv2::Slice> for EncodableField<'_> {
    fn from(_v: buffer_sv2::Slice) -> Self {
        unreachable!()
    }
}
