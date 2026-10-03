use crate::{
    codec::GetSize,
    datatypes::{
        B016MOwned, B0255Owned, B032Owned, B064KOwned, B08Owned, Mac, MacOwned, Signature,
        SignatureOwned, Sv2DataType, U256Owned, B016M, B0255, B032, B064K, B08, U24, U256,
    },
    Error,
};
use alloc::vec::Vec;

/// The `Encodable` trait defines the interface for encoding a type into bytes.
///
/// The trait provides a method for serializing an instance of a type into a byte
/// array. The trait is flexible, allowing various types, including primitives,
/// structures, and collections, to implement custom serialization logic.
///
/// `to_bytes` takes a mutable byte slice as a destination buffer. This method encodes
/// the object directly into the provided buffer, returning the number of bytes written or an
/// error if the encoding process fails.
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
}

impl<'a, T: Into<EncodableField<'a>>> Encodable for T {
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes(self, dst: &mut [u8]) -> Result<usize, Error> {
        let encoded_field = self.into();
        encoded_field.encode(dst, 0)
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
    /// U16 Primitive, representing a u16 type
    U16(u16),
    /// Bool Primitive, representing a bool type
    Bool(bool),
    /// U24 Primitive, representing a U24 type
    U24(U24),
    /// U256 Primitive, representing a U256 type
    U256(U256<'a>),
    U256Owned(U256Owned),
    /// Mac Primitive, representing a MAC type
    Mac(Mac<'a>),
    MacOwned(MacOwned),
    /// Signature Primitive, representing a Signature type
    Signature(Signature<'a>),
    SignatureOwned(SignatureOwned),
    /// U32 Primitive, representing a u32 type
    U32(u32),
    /// F32 Primitive, representing a f32 type
    F32(f32),
    /// U64 Primitive, representing a u64 type
    U64(u64),
    /// B08 Primitive, same B0_255 encoding as B0255 with the payload capped at 8 bytes
    B08(B08<'a>),
    B08Owned(B08Owned),
    /// B032 Primitive, representing a B032 type
    B032(B032<'a>),
    B032Owned(B032Owned),
    /// B0255 Primitive, representing a B0255 type
    B0255(B0255<'a>),
    B0255Owned(B0255Owned),
    /// B064K Primitive, representing a B064K type
    B064K(B064K<'a>),
    B064KOwned(B064KOwned),
    /// B016M Primitive, representing a B016M type
    B016M(B016M<'a>),
    B016MOwned(B016MOwned),
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
            Self::U16(v) => v.to_slice(dst),
            Self::Bool(v) => v.to_slice(dst),
            Self::U24(v) => v.to_slice(dst),
            Self::U256(v) => v.to_slice(dst),
            Self::U256Owned(v) => v.to_slice(dst),
            Self::Mac(v) => v.to_slice(dst),
            Self::MacOwned(v) => v.to_slice(dst),
            Self::Signature(v) => v.to_slice(dst),
            Self::SignatureOwned(v) => v.to_slice(dst),
            Self::U32(v) => v.to_slice(dst),
            Self::F32(v) => v.to_slice(dst),
            Self::U64(v) => v.to_slice(dst),
            Self::B08(v) => v.to_slice(dst),
            Self::B08Owned(v) => v.to_slice(dst),
            Self::B032(v) => v.to_slice(dst),
            Self::B032Owned(v) => v.to_slice(dst),
            Self::B0255(v) => v.to_slice(dst),
            Self::B0255Owned(v) => v.to_slice(dst),
            Self::B064K(v) => v.to_slice(dst),
            Self::B064KOwned(v) => v.to_slice(dst),
            Self::B016M(v) => v.to_slice(dst),
            Self::B016MOwned(v) => v.to_slice(dst),
        }
    }
}

// Provides the logic for calculating the size of the encodable field.
impl GetSize for EncodablePrimitive<'_> {
    fn get_size(&self) -> usize {
        match self {
            Self::U8(v) => v.get_size(),
            Self::U16(v) => v.get_size(),
            Self::Bool(v) => v.get_size(),
            Self::U24(v) => v.get_size(),
            Self::U256(v) => v.get_size(),
            Self::U256Owned(v) => v.get_size(),
            Self::Mac(v) => v.get_size(),
            Self::MacOwned(v) => v.get_size(),
            Self::Signature(v) => v.get_size(),
            Self::SignatureOwned(v) => v.get_size(),
            Self::U32(v) => v.get_size(),
            Self::F32(v) => v.get_size(),
            Self::U64(v) => v.get_size(),
            Self::B08(v) => v.get_size(),
            Self::B08Owned(v) => v.get_size(),
            Self::B032(v) => v.get_size(),
            Self::B032Owned(v) => v.get_size(),
            Self::B0255(v) => v.get_size(),
            Self::B0255Owned(v) => v.get_size(),
            Self::B064K(v) => v.get_size(),
            Self::B064KOwned(v) => v.get_size(),
            Self::B016M(v) => v.get_size(),
            Self::B016MOwned(v) => v.get_size(),
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

impl<'a> EncodableField<'a> {
    /// The `encode` method serializes a field into the destination buffer `dst`, starting
    /// at the provided `offset`. If the field is a structure, it encodes each contained
    /// field in order. If the buffer is too small or encoding fails, the method returns an
    /// error.
    pub fn encode(&self, dst: &mut [u8], offset: usize) -> Result<usize, Error> {
        if dst.len() < offset {
            return Err(Error::WriteError(offset, dst.len()));
        }
        let mut written = 0;
        for p in self.primitives() {
            let at = offset + written;
            if dst.len() < at {
                return Err(Error::WriteError(at, dst.len()));
            }
            written += p.encode(&mut dst[at..])?;
        }
        Ok(written)
    }

    fn primitives(&self) -> Primitives<'_, 'a> {
        // Descending into a top level struct keeps flat messages from allocating a stack.
        let current = match self {
            Self::Struct(ps) => ps.iter(),
            other => core::slice::from_ref(other).iter(),
        };
        Primitives {
            current,
            stack: Vec::new(),
        }
    }
}

// Yields the primitives of a field in encoding order without recursing, so structural depth
// cannot exhaust the stack.
struct Primitives<'s, 'a> {
    current: core::slice::Iter<'s, EncodableField<'a>>,
    stack: Vec<core::slice::Iter<'s, EncodableField<'a>>>,
}

impl<'s, 'a> Iterator for Primitives<'s, 'a> {
    type Item = &'s EncodablePrimitive<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.current.next() {
                Some(EncodableField::Primitive(p)) => return Some(p),
                Some(EncodableField::Struct(ps)) => self
                    .stack
                    .push(core::mem::replace(&mut self.current, ps.iter())),
                None => self.current = self.stack.pop()?,
            }
        }
    }
}

impl GetSize for EncodableField<'_> {
    fn get_size(&self) -> usize {
        self.primitives()
            .fold(0usize, |size, p| size.saturating_add(p.get_size()))
    }
}

/// Drops nested structs iteratively, so structural depth cannot exhaust the stack.
///
/// Note that this makes [`EncodableField`] impossible to destructure by move: consumers have
/// to match its variants by reference. It also means drop-check requires an [`EncodableField`]
/// to go out of scope before the data it borrows from, so a field cannot be declared before the
/// buffer it points into.
impl Drop for EncodableField<'_> {
    fn drop(&mut self) {
        if let Self::Struct(ps) = self {
            let mut pending = core::mem::take(ps);
            while let Some(mut field) = pending.pop() {
                if let Self::Struct(inner) = &mut field {
                    pending.append(inner);
                }
            }
        }
    }
}
