//! Serialize and de-serialize binary data into and from Stratum V2 types.
//!
//! # Overview
//!
//! Enables conversion between various Rust types and SV2-specific data formats for efficient
//! network communication. Provides utilities to encode and decode data types according to the SV2
//! specifications.
//!
//! ## Type Mappings
//! The following table illustrates how standard Rust types or serde data model map to their SV2
//! counterparts:
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
//! # Cross-Language Interoperability
//!
//! To support foreign function interface (FFI) use cases, the module includes `CError` and `CVec`
//! types that represent SV2 data and errors in a format suitable for cross-language compatibility.
//!
//! # Build Options
//!
//! Supports optional features like `no_std` for environments without standard library support.
//! Error types are conditionally compiled to work with or without `std`.
//!
//! ## Conditional Compilation
//! - With the `no_std` feature enabled, I/O-related errors use a simplified `IoError`
//!   representation.
//! - Standard I/O errors (`std::io::Error`) are used when `no_std` is disabled.
//!
//! # FFI Interoperability
//!
//! Provides utilities for FFI (Foreign Function Interface) to enable data passing between Rust and
//! other languages. Includes:
//! - `CVec`: Represents a byte vector for safe passing between C and Rust.
//! - `CError`: A C-compatible error type.
//! - `CVec2`: Manages collections of `CVec` objects across FFI boundaries.
//!
//! Facilitates integration of SV2 functionality into cross-language projects.
//!
//! ## Features
//! - **prop_test**: Adds support for property testing for protocol types.
//! - **with_buffer_pool**: Enables support for buffer pooling to optimize memory usage during
//!   serialization and deserialization.
#![cfg_attr(feature = "no_std", no_std)]

#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, ErrorKind};

pub mod codec;
pub mod datatypes;

pub use datatypes::{
    PubKey, Seq0255, Seq064K, ShortTxId, Signature, Str0255, Sv2DataType, Sv2Option, U32AsRef,
    B016M, B0255, B032, B064K, U24, U256,
};

#[macro_use]
extern crate alloc;

use core::convert::TryInto;

pub use crate::codec::{decodable::Decodable as Deserialize, encodable::Encodable as Serialize, *};
pub use derive_codec_sv2::{Decodable as Deserialize, Encodable as Serialize};

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
/// from raw bytes or I/O streams. Handles deserialization of nested and primitive data
/// structures through traits, enums, and helper functions for managing the decoding process.
///
/// # Overview
/// The [`Decodable`] trait serves as the core component, offering methods to define a type's
/// structure, decode raw byte data, and construct instances from decoded fields. It supports both
/// in-memory byte slices and I/O streams for flexibility across deserialization use cases.
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
/// # `no_std` Support
/// Compatible with `no_std` environments through conditional compilation. Omits I/O-dependent
/// methods like `from_reader` when `no_std` is enabled, ensuring lightweight builds for constrained
/// environments.
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
/// - **[`Encodable`]**: Defines methods for converting an object into a byte array or writing it
///   directly to an output stream. It supports both primitive types and complex structures.
/// - **[`EncodablePrimitive`]**: Represents basic types that can be serialized directly. Includes
///   data types like integers, booleans, and byte arrays.
/// - **[`EncodableField`]**: Extends [`EncodablePrimitive`] to support structured and nested data,
///   enabling recursive encoding of complex structures.
///
/// ### `no_std` Compatibility
///
/// When compiled with the `no_std` feature enabled, this module omits the `to_writer` method
/// to support environments without the standard library. Only buffer-based encoding
/// (`to_bytes`) is available in this mode.
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
/// - **`to_writer`** (requires `std`): Encodes the instance into any [`Write`] implementor, such as
///   a file or network stream.
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

/// Error types used within the protocol library to indicate various failure conditions.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Error {
    /// Indicates an attempt to read beyond a valid range.
    OutOfBound,

    /// Raised when a non-binary value is interpreted as a boolean.
    NotABool(u8),

    /// Occurs when an unexpected size mismatch arises during a write operation, specifying
    /// expected and actual sizes.
    WriteError(usize, usize),

    /// Signifies an overflow condition where a `u32` exceeds the maximum allowable `u24` value.
    U24TooBig(u32),

    /// Reports a size mismatch for a signature, such as when it does not match the expected size.
    InvalidSignatureSize(usize),

    /// Raised when a `u256` value is invalid, typically due to size discrepancies.
    InvalidU256(usize),

    /// Indicates an invalid `u24` representation.
    InvalidU24(u32),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B0255`.
    InvalidB0255Size(usize),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B064K`.
    InvalidB064KSize(usize),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B016M`.
    InvalidB016MSize(usize),

    /// Raised when a sequence size exceeds `0255`.
    InvalidSeq0255Size(usize),

    /// Error when trying to encode a non-primitive data type.
    NonPrimitiveTypeCannotBeEncoded,

    /// Generic conversion error related to primitive types.
    PrimitiveConversionError,

    /// Error occurring during decoding due to conversion issues.
    DecodableConversionError,

    /// Error triggered when a decoder is used without initialization.
    UnInitializedDecoder,

    /// Represents I/O-related errors
    IoError,

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

    /// Raised when an unsupported or unknown message type is encountered.
    UnknownMessageType(u8),

    /// Indicates a protocol constraint violation where `Sv2Option` unexpectedly contains multiple
    /// elements.
    Sv2OptionHaveMoreThenOneElement(u8),
}

#[cfg(not(feature = "no_std"))]
impl From<E> for Error {
    fn from(v: E) -> Self {
        match v.kind() {
            ErrorKind::UnexpectedEof => Error::OutOfBound,
            _ => Error::IoError,
        }
    }
}

/// `CError` is a foreign function interface (FFI)-compatible version of the `Error` enum to
/// facilitate cross-language compatibility.
#[repr(C)]
#[derive(Debug)]
pub enum CError {
    /// Indicates an attempt to read beyond a valid range.
    OutOfBound,

    /// Raised when a non-binary value is interpreted as a boolean.
    NotABool(u8),

    /// Occurs when an unexpected size mismatch arises during a write operation, specifying
    /// expected and actual sizes.
    WriteError(usize, usize),

    /// Signifies an overflow condition where a `u32` exceeds the maximum allowable `u24` value.
    U24TooBig(u32),

    /// Reports a size mismatch for a signature, such as when it does not match the expected size.
    InvalidSignatureSize(usize),

    /// Raised when a `u256` value is invalid, typically due to size discrepancies.
    InvalidU256(usize),

    /// Indicates an invalid `u24` representation.
    InvalidU24(u32),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B0255`.
    InvalidB0255Size(usize),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B064K`.
    InvalidB064KSize(usize),

    /// Error indicating that a byte array exceeds the maximum allowed size for `B016M`.
    InvalidB016MSize(usize),

    /// Raised when a sequence size exceeds `0255`.
    InvalidSeq0255Size(usize),

    /// Error when trying to encode a non-primitive data type.
    NonPrimitiveTypeCannotBeEncoded,

    /// Generic conversion error related to primitive types.
    PrimitiveConversionError,

    /// Error occurring during decoding due to conversion issues.
    DecodableConversionError,

    /// Error triggered when a decoder is used without initialization.
    UnInitializedDecoder,

    /// Represents I/O-related errors.
    IoError,

    /// Raised when an unexpected mismatch occurs during read operations, specifying expected and
    /// actual read sizes.
    ReadError(usize, usize),

    /// Used as a marker error for fields that should remain void or empty.
    VoidFieldMarker,

    /// Signifies a value overflow based on protocol restrictions, containing details about
    /// fixed/variable size, maximum size allowed, and the offending value details.
    ValueExceedsMaxSize(bool, usize, usize, usize, CVec, usize),

    /// Triggered when a sequence type (`Seq0255`, `Seq064K`) exceeds its maximum allowable size.
    SeqExceedsMaxSize,

    /// Raised when no valid decodable field is provided during decoding.
    NoDecodableFieldPassed,

    /// Error for protocol-specific invalid values.
    ValueIsNotAValidProtocol(u8),

    /// Raised when an unsupported or unknown message type is encountered.
    UnknownMessageType(u8),

    /// Indicates a protocol constraint violation where `Sv2Option` unexpectedly contains multiple
    /// elements.
    Sv2OptionHaveMoreThenOneElement(u8),
}

impl From<Error> for CError {
    fn from(e: Error) -> CError {
        match e {
            Error::OutOfBound => CError::OutOfBound,
            Error::NotABool(u) => CError::NotABool(u),
            Error::WriteError(u1, u2) => CError::WriteError(u1, u2),
            Error::U24TooBig(u) => CError::U24TooBig(u),
            Error::InvalidSignatureSize(u) => CError::InvalidSignatureSize(u),
            Error::InvalidU256(u) => CError::InvalidU256(u),
            Error::InvalidU24(u) => CError::InvalidU24(u),
            Error::InvalidB0255Size(u) => CError::InvalidB0255Size(u),
            Error::InvalidB064KSize(u) => CError::InvalidB064KSize(u),
            Error::InvalidB016MSize(u) => CError::InvalidB016MSize(u),
            Error::InvalidSeq0255Size(u) => CError::InvalidSeq0255Size(u),
            Error::NonPrimitiveTypeCannotBeEncoded => CError::NonPrimitiveTypeCannotBeEncoded,
            Error::PrimitiveConversionError => CError::PrimitiveConversionError,
            Error::DecodableConversionError => CError::DecodableConversionError,
            Error::UnInitializedDecoder => CError::UnInitializedDecoder,
            Error::IoError => CError::IoError,
            Error::ReadError(u1, u2) => CError::ReadError(u1, u2),
            Error::VoidFieldMarker => CError::VoidFieldMarker,
            Error::ValueExceedsMaxSize(isfixed, size, headersize, maxsize, bad_value, bad_len) => {
                let bv1: &[u8] = bad_value.as_ref();
                let bv: CVec = bv1.into();
                CError::ValueExceedsMaxSize(isfixed, size, headersize, maxsize, bv, bad_len)
            }
            Error::SeqExceedsMaxSize => CError::SeqExceedsMaxSize,
            Error::NoDecodableFieldPassed => CError::NoDecodableFieldPassed,
            Error::ValueIsNotAValidProtocol(u) => CError::ValueIsNotAValidProtocol(u),
            Error::UnknownMessageType(u) => CError::UnknownMessageType(u),
            Error::Sv2OptionHaveMoreThenOneElement(u) => CError::Sv2OptionHaveMoreThenOneElement(u),
        }
    }
}

impl Drop for CError {
    fn drop(&mut self) {
        match self {
            Self::OutOfBound => (),
            Self::NotABool(_) => (),
            Self::WriteError(_, _) => (),
            Self::U24TooBig(_) => (),
            Self::InvalidSignatureSize(_) => (),
            Self::InvalidU256(_) => (),
            Self::InvalidU24(_) => (),
            Self::InvalidB0255Size(_) => (),
            Self::InvalidB064KSize(_) => (),
            Self::InvalidB016MSize(_) => (),
            Self::InvalidSeq0255Size(_) => (),
            Self::NonPrimitiveTypeCannotBeEncoded => (),
            Self::PrimitiveConversionError => (),
            Self::DecodableConversionError => (),
            Self::UnInitializedDecoder => (),
            Self::IoError => (),
            Self::ReadError(_, _) => (),
            Self::VoidFieldMarker => (),
            Self::ValueExceedsMaxSize(_, _, _, _, cvec, _) => free_vec(cvec),
            Self::SeqExceedsMaxSize => (),
            Self::NoDecodableFieldPassed => (),
            Self::ValueIsNotAValidProtocol(_) => (),
            Self::UnknownMessageType(_) => (),
            Self::Sv2OptionHaveMoreThenOneElement(_) => (),
        };
    }
}


/// Vec<u8> is used as the Sv2 type Bytes
impl GetSize for Vec<u8> {
    fn get_size(&self) -> usize {
        self.len()
    }
}

// Only needed for implement encodable for Frame never called
impl<'a> From<Vec<u8>> for EncodableField<'a> {
    fn from(_v: Vec<u8>) -> Self {
        unreachable!()
    }
}

#[cfg(feature = "with_buffer_pool")]
impl<'a> From<buffer_sv2::Slice> for EncodableField<'a> {
    fn from(_v: buffer_sv2::Slice) -> Self {
        unreachable!()
    }
}

/// A struct to facilitate transferring a `Vec<u8>` across FFI boundaries.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CVec {
    data: *mut u8,
    len: usize,
    capacity: usize,
}

impl CVec {
    /// Returns a mutable slice of the contained data.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the data pointed to by `self.data`
    /// remains valid for the duration of the returned slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.data, self.len) }
    }

    /// Fills a buffer allocated in Rust from C.
    ///
    /// # Safety
    ///
    /// Constructs a `CVec` without taking ownership of the pointed buffer. If the owner drops the
    /// buffer, the `CVec` will point to invalid memory.
    #[allow(clippy::wrong_self_convention)]
    pub fn as_shared_buffer(v: &mut [u8]) -> Self {
        let (data, len) = (v.as_mut_ptr(), v.len());
        Self {
            data,
            len,
            capacity: len,
        }
    }
}

impl From<&[u8]> for CVec {
    fn from(v: &[u8]) -> Self {
        let mut buffer: Vec<u8> = vec![0; v.len()];
        buffer.copy_from_slice(v);

        // Get the length, first, then the pointer (doing it the other way around **currently**
        // doesn't cause UB, but it may be unsound due to unclear (to me, at least) guarantees of
        // the std lib)
        let len = buffer.len();
        let ptr = buffer.as_mut_ptr();
        core::mem::forget(buffer);

        CVec {
            data: ptr,
            len,
            capacity: len,
        }
    }
}

/// Creates a `CVec` from a buffer that was allocated in C.
///
/// # Safety
/// The caller must ensure that the buffer is valid and that
/// the data length does not exceed the allocated size.
#[no_mangle]
pub unsafe extern "C" fn cvec_from_buffer(data: *const u8, len: usize) -> CVec {
    let input = core::slice::from_raw_parts(data, len);

    let mut buffer: Vec<u8> = vec![0; len];
    buffer.copy_from_slice(input);

    // Get the length, first, then the pointer (doing it the other way around **currently** doesn't
    // cause UB, but it may be unsound due to unclear (to me, at least) guarantees of the std lib)
    let len = buffer.len();
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);

    CVec {
        data: ptr,
        len,
        capacity: len,
    }
}

/// A struct to manage a collection of `CVec` objects across FFI boundaries.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CVec2 {
    data: *mut CVec,
    len: usize,
    capacity: usize,
}

impl CVec2 {
    /// `as_mut_slice`: helps to get a mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [CVec] {
        unsafe { core::slice::from_raw_parts_mut(self.data, self.len) }
    }
}
impl From<CVec2> for Vec<CVec> {
    fn from(v: CVec2) -> Self {
        unsafe { Vec::from_raw_parts(v.data, v.len, v.capacity) }
    }
}

/// Frees the underlying memory of a `CVec`.
pub fn free_vec(buf: &mut CVec) {
    let _: Vec<u8> = unsafe { Vec::from_raw_parts(buf.data, buf.len, buf.capacity) };
}

/// Frees the underlying memory of a `CVec2` and all its elements.
pub fn free_vec_2(buf: &mut CVec2) {
    let vs: Vec<CVec> = unsafe { Vec::from_raw_parts(buf.data, buf.len, buf.capacity) };
    for mut s in vs {
        free_vec(&mut s)
    }
}

impl<'a, const A: bool, const B: usize, const C: usize, const D: usize>
From<datatypes::Inner<'a, A, B, C, D>> for CVec
{
    fn from(v: datatypes::Inner<'a, A, B, C, D>) -> Self {
        let (ptr, len, cap): (*mut u8, usize, usize) = match v {
            datatypes::Inner::Ref(inner) => {
                // Data is copied in a vector that then will be forgetted from the allocator,
                // cause the owner of the data is going to be dropped by rust
                let mut inner: Vec<u8> = inner.into();

                // Get the length, first, then the pointer (doing it the other way around
                // **currently** doesn't cause UB, but it may be unsound due to unclear (to me, at
                // least) guarantees of the std lib)
                let len = inner.len();
                let cap = inner.capacity();
                let ptr = inner.as_mut_ptr();
                core::mem::forget(inner);

                (ptr, len, cap)
            }
            datatypes::Inner::Owned(mut inner) => {
                // Get the length, first, then the pointer (doing it the other way around
                // **currently** doesn't cause UB, but it may be unsound due to unclear (to me, at
                // least) guarantees of the std lib)
                let len = inner.len();
                let cap = inner.capacity();
                let ptr = inner.as_mut_ptr();
                core::mem::forget(inner);

                (ptr, len, cap)
            }
        };
        Self {
            data: ptr,
            len,
            capacity: cap,
        }
    }
}

/// Initializes an empty `CVec2`.
///
/// # Safety
/// The caller is responsible for freeing the `CVec2` when it is no longer needed.
#[no_mangle]
pub unsafe extern "C" fn init_cvec2() -> CVec2 {
    let mut buffer = Vec::<CVec>::new();

    // Get the length, first, then the pointer (doing it the other way around **currently** doesn't
    // cause UB, but it may be unsound due to unclear (to me, at least) guarantees of the std lib)
    let len = buffer.len();
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);

    CVec2 {
        data: ptr,
        len,
        capacity: len,
    }
}

/// Adds a `CVec` to a `CVec2`.
///
/// # Safety
/// The caller must ensure no duplicate `CVec`s are added, as duplicates may
/// lead to double-free errors when the message is dropped.
#[no_mangle]
pub unsafe extern "C" fn cvec2_push(cvec2: &mut CVec2, cvec: CVec) {
    let mut buffer: Vec<CVec> = Vec::from_raw_parts(cvec2.data, cvec2.len, cvec2.capacity);
    buffer.push(cvec);

    let len = buffer.len();
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);

    cvec2.data = ptr;
    cvec2.len = len;
    cvec2.capacity = len;
}

impl<'a, T: Into<CVec>> From<Seq0255<'a, T>> for CVec2 {
    fn from(v: Seq0255<'a, T>) -> Self {
        let mut v: Vec<CVec> = v.0.into_iter().map(|x| x.into()).collect();
        // Get the length, first, then the pointer (doing it the other way around **currently**
        // doesn't cause UB, but it may be unsound due to unclear (to me, at least) guarantees of
        // the std lib)
        let len = v.len();
        let capacity = v.capacity();
        let data = v.as_mut_ptr();
        core::mem::forget(v);
        Self {
            data,
            len,
            capacity,
        }
    }
}
impl<'a, T: Into<CVec>> From<Seq064K<'a, T>> for CVec2 {
    fn from(v: Seq064K<'a, T>) -> Self {
        let mut v: Vec<CVec> = v.0.into_iter().map(|x| x.into()).collect();
        // Get the length, first, then the pointer (doing it the other way around **currently**
        // doesn't cause UB, but it may be unsound due to unclear (to me, at least) guarantees of
        // the std lib)
        let len = v.len();
        let capacity = v.capacity();
        let data = v.as_mut_ptr();
        core::mem::forget(v);
        Self {
            data,
            len,
            capacity,
        }
    }
}

/// Exported FFI functions for interoperability with C code for u24
#[no_mangle]
pub extern "C" fn _c_export_u24(_a: U24) {}
/// Exported FFI functions for interoperability with C code for CVec
#[no_mangle]
pub extern "C" fn _c_export_cvec(_a: CVec) {}
/// Exported FFI functions for interoperability with C code for CVec2
#[no_mangle]
pub extern "C" fn _c_export_cvec2(_a: CVec2) {}


/// Converts a value implementing the `Into<u64>` trait into a custom `U256` type.
pub fn u256_from_int<V: Into<u64>>(value: V) -> U256<'static> {
    // initialize u256 as a bytes vec of len 24
    let mut u256 = vec![0_u8; 24];
    let val: u64 = value.into();
    for v in &(val.to_le_bytes()) {
        // add 8 bytes to u256
        u256.push(*v)
    }
    // Always safe cause u256 is 24 + 8 (32) bytes
    let u256: U256 = u256.try_into().unwrap();
    u256
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;

    mod test_struct {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test {
            a: u32,
            b: u8,
            c: U24,
        }

        #[test]
        fn test_struct() {
            let expected = Test {
                a: 456,
                b: 9,
                c: 67_u32.try_into().unwrap(),
            };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_f32 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test {
            a: u8,
            b: U24,
            c: f32,
        }

        #[test]
        fn test_struct() {
            let expected = Test {
                c: 0.345,
                a: 9,
                b: 67_u32.try_into().unwrap(),
            };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b0255 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test<'decoder> {
            a: B0255<'decoder>,
        }

        #[test]
        fn test_b0255() {
            let mut b0255 = [6; 3];
            let b0255: B0255 = (&mut b0255[..]).try_into().unwrap();

            let expected = Test { a: b0255 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }

        #[test]
        fn test_b0255_max() {
            let mut b0255 = [6; 255];
            let b0255: B0255 = (&mut b0255[..]).try_into().unwrap();

            let expected = Test { a: b0255 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test<'decoder> {
            a: U256<'decoder>,
        }

        #[test]
        fn test_u256() {
            let mut u256 = [6_u8; 32];
            let u256: U256 = (&mut u256[..]).try_into().unwrap();

            let expected = Test { a: u256 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Signature<'decoder>,
        }

        #[test]
        fn test_signature() {
            let mut s = [6; 64];
            let s: Signature = (&mut s[..]).try_into().unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b016m {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            b: bool,
            a: B016M<'decoder>,
        }

        #[test]
        fn test_b016m() {
            let mut b = [0_u8; 70000];
            let b: B016M = (&mut b[..]).try_into().unwrap();
            //println!("{:?}", to_bytes(&b).unwrap().len());

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }

        #[test]
        fn test_b016m_max() {
            let mut b = vec![0_u8; 16777215];
            let b: B016M = (&mut b[..]).try_into().unwrap();
            //println!("{:?}", to_bytes(&b).unwrap().len());

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b064k {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            b: bool,
            a: B064K<'decoder>,
        }

        #[test]
        fn test_b064k() {
            let mut b = [1, 2, 9];
            let b: B064K = (&mut b[..])
                .try_into()
                .expect("vector smaller than 64K should not fail");

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_seq0255_u256() {
            let mut u256_1 = [6; 32];
            let mut u256_2 = [5; 32];
            let mut u256_3 = [0; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();
            let u256_2: U256 = (&mut u256_2[..]).try_into().unwrap();
            let u256_3: U256 = (&mut u256_3[..]).try_into().unwrap();

            let val = vec![u256_1, u256_2, u256_3];
            let s = Seq0255::new(val).unwrap();

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }

    mod test_0255_bool {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, bool>,
        }

        #[test]
        fn test_seq0255_bool() {
            let s: Seq0255<bool> = Seq0255::new(vec![true, false, true]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_u16 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, u16>,
        }

        #[test]
        fn test_seq0255_u16() {
            let s: Seq0255<u16> = Seq0255::new(vec![10, 43, 89]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq_0255_u24 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, U24>,
        }

        #[test]
        fn test_seq0255_u24() {
            let u24_1: U24 = 56_u32.try_into().unwrap();
            let u24_2: U24 = 59_u32.try_into().unwrap();
            let u24_3: U24 = 70999_u32.try_into().unwrap();

            let val = vec![u24_1, u24_2, u24_3];
            let s: Seq0255<U24> = Seq0255::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seqo255_u32 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, u32>,
        }

        #[test]
        fn test_seq0255_u32() {
            let s: Seq0255<u32> = Seq0255::new(vec![546, 99999, 87, 32]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, Signature<'decoder>>,
        }

        #[test]
        fn test_seq0255_signature() {
            let mut siganture_1 = [88_u8; 64];
            let mut siganture_2 = [99_u8; 64];
            let mut siganture_3 = [220_u8; 64];
            let siganture_1: Signature = (&mut siganture_1[..]).try_into().unwrap();
            let siganture_2: Signature = (&mut siganture_2[..]).try_into().unwrap();
            let siganture_3: Signature = (&mut siganture_3[..]).try_into().unwrap();

            let val = vec![siganture_1, siganture_2, siganture_3];
            let s: Seq0255<Signature> = Seq0255::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq_064_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_seq064k_u256() {
            let mut u256_1 = [6; 32];
            let mut u256_2 = [5; 32];
            let mut u256_3 = [0; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();
            let u256_2: U256 = (&mut u256_2[..]).try_into().unwrap();
            let u256_3: U256 = (&mut u256_3[..]).try_into().unwrap();

            let val = vec![u256_1, u256_2, u256_3];
            let s = Seq064K::new(val).unwrap();

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }

    mod test_064_bool {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, bool>,
        }

        #[test]
        fn test_seq064k_bool() {
            let s: Seq064K<bool> = Seq064K::new(vec![true, false, true]).unwrap();
            let s2: Seq064K<bool> = Seq064K::new(vec![true; 64000]).unwrap();

            let expected = Test { a: s };
            let expected2 = Test { a: s2 };

            let mut bytes = to_bytes(expected.clone()).unwrap();
            let mut bytes2 = to_bytes(expected2.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();
            let deserialized2: Test = from_bytes(&mut bytes2[..]).unwrap();

            assert_eq!(deserialized, expected);
            assert_eq!(deserialized2, expected2);
        }
    }

    mod test_se1o64k_u16 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, u16>,
        }

        #[test]
        fn test_seq064k_u16() {
            let s: Seq064K<u16> = Seq064K::new(vec![10, 43, 89]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq064k_u24 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, U24>,
        }

        #[test]
        fn test_seq064k_u24() {
            let u24_1: U24 = 56_u32.try_into().unwrap();
            let u24_2: U24 = 59_u32.try_into().unwrap();
            let u24_3: U24 = 70999_u32.try_into().unwrap();

            let val = vec![u24_1, u24_2, u24_3];
            let s: Seq064K<U24> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq064k_u32 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, u32>,
        }

        #[test]
        fn test_seq064k_u32() {
            let s: Seq064K<u32> = Seq064K::new(vec![546, 99999, 87, 32]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq064k_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, Signature<'decoder>>,
        }

        #[test]
        fn test_seq064k_signature() {
            let mut siganture_1 = [88_u8; 64];
            let mut siganture_2 = [99_u8; 64];
            let mut siganture_3 = [220_u8; 64];
            let siganture_1: Signature = (&mut siganture_1[..]).try_into().unwrap();
            let siganture_2: Signature = (&mut siganture_2[..]).try_into().unwrap();
            let siganture_3: Signature = (&mut siganture_3[..]).try_into().unwrap();

            let val = vec![siganture_1, siganture_2, siganture_3];
            let s: Seq064K<Signature> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq064k_b016m {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, B016M<'decoder>>,
        }

        #[test]
        fn test_seq064k_b016m() {
            let mut bytes_1 = [88_u8; 64];
            let mut bytes_2 = [99_u8; 64];
            let mut bytes_3 = [220_u8; 64];
            let bytes_1: B016M = (&mut bytes_1[..]).try_into().unwrap();
            let bytes_2: B016M = (&mut bytes_2[..]).try_into().unwrap();
            let bytes_3: B016M = (&mut bytes_3[..]).try_into().unwrap();

            let val = vec![bytes_1, bytes_2, bytes_3];
            let s: Seq064K<B016M> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq_0255_in_struct {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: u8,
            b: Seq0255<'decoder, u8>,
            c: u32,
        }

        #[test]
        fn test_seq_0255_in_struct() {
            let expected = Test {
                a: 89,
                b: Seq0255::new(vec![]).unwrap(),
                c: 32,
            };
            let len = expected.get_size();
            let mut buffer = Vec::new();
            buffer.resize(len, 0);
            to_writer(expected.clone(), &mut buffer).unwrap();
            let deserialized: Test = from_bytes(&mut buffer[..]).unwrap();
            assert_eq!(deserialized, expected);
        }
    }
    mod test_sv2_option_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Sv2Option<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_sv2_option_u256() {
            let mut u256_1 = [6; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();

            let val = Some(u256_1);
            let s = Sv2Option::new(val);

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }
    mod test_sv2_option_none {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Sv2Option<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_sv2_option_none() {
            let val = None;
            let s = Sv2Option::new(val);

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }
}
