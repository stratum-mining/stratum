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
// Checked functions validate data lengths before decoding or encoding.
//
// ### Modules
// - **`copy_data_types`**: Defines fixed-size types directly copied into or from byte slices, such
//   as `U24` (24-bit unsigned integer).
// - **`non_copy_data_types`**: Manages dynamically-sized types, like sequences, public keys, and
//   strings, requiring size handling logic for SV2 compatibility.
//
// ### Re-exports
// Re-exports common data types used in SV2 serialization, such as `Signature`, `Seq0255`, and
// others, simplifying protocol data handling with concrete implementations of `Sv2DataType`.
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
pub(crate) use non_copy_data_types::Inner;
pub use non_copy_data_types::{
    B016MOwned, B0255Owned, B032Owned, B064KOwned, Mac, MacOwned, PubKey, PubKeyOwned, Seq0255,
    Seq0255Owned, Seq064K, Seq064KOwned, Signature, SignatureOwned, Str0255, Str0255Owned,
    Sv2Option, Sv2OptionOwned, U256Owned, B016M, B0255, B032, B064K, U256,
};

use alloc::vec::Vec;
use core::convert::TryInto;

/// `Sv2DataType` is a trait that defines methods for encoding and decoding Stratum V2 data.
/// It is used for serializing and deserializing both fixed-size and dynamically-sized types.
///
/// Key Responsibilities:
/// - Serialization: Converting data from in-memory representations to byte slices.
/// - Deserialization: Converting byte slices back into the in-memory representation of the data.
///
pub trait Sv2DataType<'a>: Sized + SizeHint + GetSize + TryInto<FieldMarker> {
    /// Creates an instance of the type from a mutable byte slice, checking for size constraints.
    ///
    /// This function verifies that the provided byte slice has the correct size according to the
    /// type's size hint.
    fn from_bytes_(data: &'a mut [u8]) -> Result<Self, Error>;

    /// Serializes the instance to a mutable slice, checking the destination size.
    fn to_slice(&'a self, dst: &mut [u8]) -> Result<usize, Error>;
}
