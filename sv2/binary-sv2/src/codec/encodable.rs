use crate::{
    codec::GetSize,
    datatypes::{Signature, Sv2DataType, U32AsRef, B016M, B0255, B032, B064K, U24, U256},
    Error,
};
use alloc::vec::Vec;
#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Write};

/// The `Encodable` trait defines the interface for encoding a type into bytes.
///
/// The trait provides methods for serializing an instance of a type into a byte
/// array or writing it directly into an output writer. The trait is flexible,
/// allowing various types, including primitives, structures, and collections,
/// to implement custom serialization logic.
///
/// The trait offers two key methods for encoding:
///
/// - The first, `to_bytes`, takes a mutable byte slice as a destination buffer. This method encodes
///   the object directly into the provided buffer, returning the number of bytes written or an
///   error if the encoding process fails.
/// - The second, `to_writer`, (only available when not compiling for `no-std`) accepts a writer as
///   a destination for the encoded bytes, allowing the serialized data to be written to any
///   implementor of the `Write` trait.
///
/// Implementing types can define custom encoding logic, and this trait is
/// especially useful when dealing with different data structures that need
/// to be serialized for transmission.
pub trait Encodable {
    /// Encodes the object into the provided byte slice.
    ///
    /// The method uses the destination buffer `dst` to write the serialized
    /// bytes. It returns the number of bytes written on success or an `Error`
    /// if encoding fails.
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes(self, dst: &mut [u8]) -> Result<usize, Error>;

    /// Write the encoded object into the provided writer.
    ///
    /// Serializes the object and writes it directly
    /// to the `dst` writer. It is only available in environments
    /// where `std` is available. If the encoding fails, error is
    /// returned.
    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention)]
    fn to_writer(self, dst: &mut impl Write) -> Result<(), E>;
}

impl<'a, T: Into<EncodableField<'a>>> Encodable for T {
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes(self, dst: &mut [u8]) -> Result<usize, Error> {
        let encoded_field = self.into();
        encoded_field.encode(dst, 0)
    }

    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention, unconditional_recursion)]
    fn to_writer(self, dst: &mut impl Write) -> Result<(), E> {
        let encoded_field = self.into();
        encoded_field.to_writer(dst)
    }
}

/// The `EncodablePrimitive` enum defines primitive types  that can be encoded.
///
/// The enum represents various data types, such a integers, bool, and byte array
/// that can be encoded into a byte representation. Each variant holds a specific
/// type, and encoding logic is provided through the `encode` method.
#[derive(Debug)]
pub enum EncodablePrimitive<'a> {
    /// U8 Primitive, representing a byte
    U8(u8),
    /// Owned U8 Primitive, representing an owned byte
    OwnedU8(u8),
    /// U16 Primitive, representing a u16 type
    U16(u16),
    /// Bool Primitive, representing a bool type
    Bool(bool),
    /// U24 Primitive, representing a U24 type
    U24(U24),
    /// U256 Primitive, representing a U256 type
    U256(U256<'a>),
    /// Signature Primitive, representing a Signature type
    Signature(Signature<'a>),
    /// U32 Primitive, representing a u32 type
    U32(u32),
    /// U32AsRef Primitive, representing a U32AsRef type
    U32AsRef(U32AsRef<'a>),
    /// F32 Primitive, representing a f32 type
    F32(f32),
    /// U64 Primitive, representing a u64 type
    U64(u64),
    /// B032 Primitive, representing a B032 type
    B032(B032<'a>),
    /// B0255 Primitive, representing a B0255 type
    B0255(B0255<'a>),
    /// B064K Primitive, representing a B064K type
    B064K(B064K<'a>),
    /// B016M Primitive, representing a B016M type
    B016M(B016M<'a>),
}

impl EncodablePrimitive<'_> {
    // Provides the encoding logic for each primitive type.
    //
    // The `encode` method takes the `EncodablePrimitive` variant and serializes it
    // into the destination buffer `dst`. The method returns the number of bytes written
    // . If the buffer is too small or encoding fails, it returns an error.
    fn encode(&self, dst: &mut [u8]) -> Result<usize, Error> {
        match self {
            Self::U8(v) => v.to_slice(dst),
            Self::OwnedU8(v) => v.to_slice(dst),
            Self::U16(v) => v.to_slice(dst),
            Self::Bool(v) => v.to_slice(dst),
            Self::U24(v) => v.to_slice(dst),
            Self::U256(v) => v.to_slice(dst),
            Self::Signature(v) => v.to_slice(dst),
            Self::U32(v) => v.to_slice(dst),
            Self::U32AsRef(v) => v.to_slice(dst),
            Self::F32(v) => v.to_slice(dst),
            Self::U64(v) => v.to_slice(dst),
            Self::B032(v) => v.to_slice(dst),
            Self::B0255(v) => v.to_slice(dst),
            Self::B064K(v) => v.to_slice(dst),
            Self::B016M(v) => v.to_slice(dst),
        }
    }

    // Write the encoded object into the provided writer.
    //
    // Serializes the object and writes it directly to the
    // provided writer. It is only available in environments where `std`
    // is available.
    #[cfg(not(feature = "no_std"))]
    pub fn write(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            Self::U8(v) => v.to_writer_(writer),
            Self::OwnedU8(v) => v.to_writer_(writer),
            Self::U16(v) => v.to_writer_(writer),
            Self::Bool(v) => v.to_writer_(writer),
            Self::U24(v) => v.to_writer_(writer),
            Self::U256(v) => v.to_writer_(writer),
            Self::Signature(v) => v.to_writer_(writer),
            Self::U32(v) => v.to_writer_(writer),
            Self::U32AsRef(v) => v.to_writer_(writer),
            Self::F32(v) => v.to_writer_(writer),
            Self::U64(v) => v.to_writer_(writer),
            Self::B032(v) => v.to_writer_(writer),
            Self::B0255(v) => v.to_writer_(writer),
            Self::B064K(v) => v.to_writer_(writer),
            Self::B016M(v) => v.to_writer_(writer),
        }
    }
}

// Provides the logic for calculating the size of the encodable field.
impl GetSize for EncodablePrimitive<'_> {
    fn get_size(&self) -> usize {
        match self {
            Self::U8(v) => v.get_size(),
            Self::OwnedU8(v) => v.get_size(),
            Self::U16(v) => v.get_size(),
            Self::Bool(v) => v.get_size(),
            Self::U24(v) => v.get_size(),
            Self::U256(v) => v.get_size(),
            Self::Signature(v) => v.get_size(),
            Self::U32(v) => v.get_size(),
            Self::U32AsRef(v) => v.get_size(),
            Self::F32(v) => v.get_size(),
            Self::U64(v) => v.get_size(),
            Self::B032(v) => v.get_size(),
            Self::B0255(v) => v.get_size(),
            Self::B064K(v) => v.get_size(),
            Self::B016M(v) => v.get_size(),
        }
    }
}

/// The [`EncodableField`] enum defines encodable fields, which may be a primitive or struct.
///
/// Each [`EncodableField`] represents either a primitive value or a collection of values
/// (a struct). The encoding process for [`EncodableField`] supports nesting, allowing
/// for complex hierarchical data structures to be serialized.
#[derive(Debug)]
pub enum EncodableField<'a> {
    /// Represents a primitive value
    ///
    /// For the full supported list please see [`EncodablePrimitive`]
    Primitive(EncodablePrimitive<'a>),
    /// Represents a struct like field structure.
    ///
    /// Note that this is a recursive enum type.
    Struct(Vec<EncodableField<'a>>),
}

impl EncodableField<'_> {
    /// The `encode` method serializes a field into the destination buffer `dst`, starting
    /// at the provided `offset`. If the field is a structure, it recursively encodes
    /// each contained field. If the buffer is too small or encoding fails, the method
    /// returns an error.
    pub fn encode(&self, dst: &mut [u8], mut offset: usize) -> Result<usize, Error> {
        match (self, dst.len() >= offset) {
            (Self::Primitive(p), true) => p.encode(&mut dst[offset..]),
            (Self::Struct(ps), true) => {
                let mut result = 0;
                for p in ps {
                    let encoded_bytes = p.encode(dst, offset)?;
                    offset += encoded_bytes;
                    result += encoded_bytes;
                }
                Ok(result)
            }
            (_, false) => Err(Error::WriteError(offset, dst.len())),
        }
    }

    #[cfg(not(feature = "no_std"))]
    pub fn to_writer(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            Self::Primitive(p) => p.write(writer),
            Self::Struct(ps) => {
                for p in ps {
                    p.to_writer(writer)?;
                }
                Ok(())
            }
        }
    }
}

impl GetSize for EncodableField<'_> {
    fn get_size(&self) -> usize {
        match self {
            Self::Primitive(p) => p.get_size(),
            Self::Struct(ps) => {
                let mut size = 0;
                for p in ps {
                    size += p.get_size();
                }
                size
            }
        }
    }
}
