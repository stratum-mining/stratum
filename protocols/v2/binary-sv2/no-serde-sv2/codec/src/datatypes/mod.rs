// Provides implementations for encoding and decoding data types in the SV2 protocol,
// supporting both fixed-size and dynamically-sized types. Defines the `Sv2DataType` trait,
// which standardizes serialization and deserialization across various types, including
// those with custom requirements like byte padding and dynamic sizing.
//
// ## Structure and Contents
//
// ### `Sv2DataType` Trait
// The `Sv2DataType` trait offers methods to:
// - **Deserialize**: Convert byte slices or reader sources into Rust types.
// - **Serialize**: Encode Rust types into byte slices or write them to I/O streams.
//
// Supports both **checked** and **unchecked** variants for serialization and deserialization.
// Checked functions validate data lengths, while unchecked versions assume size correctness for
// optimized performance.
//
// ### Modules
// - **`copy_data_types`**: Defines fixed-size types directly copied into or from byte slices, such
//   as `U24` (24-bit unsigned integer).
// - **`non_copy_data_types`**: Manages dynamically-sized types, like sequences, public keys, and
//   strings, requiring size handling logic for SV2 compatibility.
//
// ### Re-exports
// Re-exports common data types used in SV2 serialization, such as `PubKey`, `Signature`, `Seq0255`,
// and others, simplifying protocol data handling with concrete implementations of `Sv2DataType`.
//
// The `Sv2DataType` trait and its implementations enable seamless conversion between in-memory
// representations and serialized forms, ensuring efficient protocol communication and
// interoperability.

use crate::{
    codec::{GetSize, SizeHint},
    Error,
};
mod non_copy_data_types;

mod copy_data_types;
use crate::codec::decodable::FieldMarker;
pub use copy_data_types::U24;
pub use non_copy_data_types::{
    Inner, PubKey, Seq0255, Seq064K, Signature, Str0255, Sv2Option, U32AsRef, B016M, B0255, B032,
    B064K, U256,
};

use alloc::vec::Vec;
use core::convert::TryInto;
#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Read, Write};

/// `Sv2DataType` is a trait that defines methods for encoding and decoding Stratum V2 data.
/// It is used for serializing and deserializing both fixed-size and dynamically-sized types.
///
/// Key Responsibilities:
/// - Serialization: Converting data from in-memory representations to byte slices or streams.
/// - Deserialization: Converting byte slices or streams back into the in-memory representation of
///   the data.
///
/// This trait includes functions for both checked and unchecked conversions, providing flexibility
/// in situations where error handling can be safely ignored.
pub trait Sv2DataType<'a>: Sized + SizeHint + GetSize + TryInto<FieldMarker> {
    /// Creates an instance of the type from a mutable byte slice, checking for size constraints.
    ///
    /// This function verifies that the provided byte slice has the correct size according to the
    /// type's size hint.
    fn from_bytes_(data: &'a mut [u8]) -> Result<Self, Error> {
        Self::size_hint(data, 0)?;
        Ok(Self::from_bytes_unchecked(data))
    }

    /// Constructs an instance from a mutable byte slice without verifying size constraints.
    fn from_bytes_unchecked(data: &'a mut [u8]) -> Self;

    /// Constructs an instance from a vector, checking for the correct size.
    fn from_vec_(data: Vec<u8>) -> Result<Self, Error>;

    /// Constructs an instance from a vector without validating its size.
    fn from_vec_unchecked(data: Vec<u8>) -> Self;

    // Constructs an instance from a reader source, checking for size constraints.
    #[cfg(not(feature = "no_std"))]
    fn from_reader_(reader: &mut impl Read) -> Result<Self, Error>;

    /// Serializes the instance to a mutable slice, checking the destination size.
    fn to_slice(&'a self, dst: &mut [u8]) -> Result<usize, Error> {
        if dst.len() >= self.get_size() {
            self.to_slice_unchecked(dst);
            Ok(self.get_size())
        } else {
            Err(Error::WriteError(self.get_size(), dst.len()))
        }
    }

    /// Serializes the instance to a mutable slice without checking the destination size.
    fn to_slice_unchecked(&'a self, dst: &mut [u8]);

    // Serializes the instance to a writer destination, checking for I/O errors.
    #[cfg(not(feature = "no_std"))]
    fn to_writer_(&self, writer: &mut impl Write) -> Result<(), E>;
}
