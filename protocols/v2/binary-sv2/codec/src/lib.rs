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

#![cfg_attr(feature = "no_std", no_std)]

#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, ErrorKind};

mod codec;
mod datatypes;
pub use datatypes::{
    PubKey, Seq0255, Seq064K, Signature, Str0255, Sv2DataType, Sv2Option, U32AsRef, B016M, B0255,
    B032, B064K, U24, U256,
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

#[macro_use]
extern crate alloc;

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

    #[cfg(not(feature = "no_std"))]
    /// Represents I/O-related errors, compatible with `no_std` mode where specific error types may
    /// vary.
    IoError(E),

    #[cfg(feature = "no_std")]
    /// Represents I/O-related errors, compatible with `no_std` mode.
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
            _ => Error::IoError(v),
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

    #[cfg(not(feature = "no_std"))]
    /// Represents I/O-related errors, compatible with `no_std` mode where specific error types may
    /// vary.
    IoError(E),

    #[cfg(feature = "no_std")]
    /// Represents I/O-related errors, compatible with `no_std` mode.
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
            #[cfg(not(feature = "no_std"))]
            Error::IoError(e) => CError::IoError(e),
            #[cfg(feature = "no_std")]
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
            #[cfg(not(feature = "no_std"))]
            Self::IoError(_) => (),
            #[cfg(feature = "no_std")]
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
impl From<Vec<u8>> for EncodableField<'_> {
    fn from(_v: Vec<u8>) -> Self {
        unreachable!()
    }
}

#[cfg(feature = "with_buffer_pool")]
impl From<buffer_sv2::Slice> for EncodableField<'_> {
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
