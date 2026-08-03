use crate::{
    codec::{GetSize, SizeHint},
    datatypes::{
        B016MOwned, B0255Owned, B032Owned, B064KOwned, Mac, MacOwned, Signature, SignatureOwned,
        Sv2DataType, U256Owned, B016M, B0255, B032, B064K, U24, U256,
    },
    Error,
};
use alloc::vec::Vec;
use core::convert::TryFrom;

/// Custom deserialization of types from binary data.
///
/// Defines the process of reconstructing a type from a sequence of bytes. It handles both simple
/// and nested or complex data structures.
pub trait Decodable<'a>: Sized {
    /// Defines the expected structure of a type based on binary data.
    ///
    /// Returns a vector of [`FieldMarker`]s, each representing a component of the structure.
    /// Useful for guiding the decoding process.
    fn get_structure(data: &[u8]) -> Result<Vec<FieldMarker>, Error>;

    /// Constructs the type from a vector of decoded fields.
    ///
    /// After the data has been split into fields, this method combines those fields
    /// back into the original type, handling nested structures or composite fields.
    fn from_decoded_fields(data: Vec<DecodableField<'a>>) -> Result<Self, Error>;

    /// Decodes the type from raw bytes.
    ///
    /// Orchestrates the decoding process, calling `get_structure` to break down
    /// the raw data, decoding each field, and then using `from_decoded_fields` to reassemble
    /// the fields into the original type.
    fn from_bytes(data: &'a mut [u8]) -> Result<Self, Error> {
        let structure = Self::get_structure(data)?;
        let mut fields = Vec::new();
        let mut tail = data;

        for field in structure {
            let field_size = field.size_hint_(tail, 0)?;
            if field_size > tail.len() {
                return Err(Error::DecodableConversionError);
            }
            let (head, t) = tail.split_at_mut(field_size);
            tail = t;
            fields.push(field.decode(head)?);
        }
        Self::from_decoded_fields(fields)
    }
}

// Primitive data marker.
//
// Fundamental data types that can be passed to a decoder to define the structure of the type to be
// decoded in a standardized way.
#[derive(Debug, Clone, Copy)]
pub enum PrimitiveMarker {
    U8,
    U16,
    Bool,
    U24,
    U256,
    U256Owned,
    Mac,
    MacOwned,
    Signature,
    SignatureOwned,
    U32,
    F32,
    U64,
    B032,
    B032Owned,
    B0255,
    B0255Owned,
    B064K,
    B064KOwned,
    B016M,
    B016MOwned,
}

/// Recursive enum representing data structure fields.
///
/// A `FieldMarker` can either be a primitive or a nested structure. The marker helps the decoder
/// understand the layout and type of each field in the data, guiding the decoding process.
#[derive(Debug, Clone)]
pub enum FieldMarker {
    /// A primitive data type.
    Primitive(PrimitiveMarker),

    /// A structured type composed of multiple fields, allowing for nested data.
    Struct(Vec<FieldMarker>),
}

/// Trait for retrieving the [`FieldMarker`] associated with a type.
///
/// Provides a standardized way to retrieve a `FieldMarker` for a type, allowing the protocol to
/// identify the structure and layout of data fields during decoding.
pub trait GetMarker {
    /// Defines the structure of a type for decoding purposes, supporting both primitive and
    /// structured types. It helps getting a marker for a type.
    fn get_marker() -> FieldMarker;
}

// Represents a list of decode-able primitive data types.
//
#[derive(Debug)]
pub enum DecodablePrimitive<'a> {
    U8(u8),
    U16(u16),
    Bool(bool),
    U24(U24),
    U256(U256<'a>),
    U256Owned(U256Owned),
    Mac(Mac<'a>),
    MacOwned(MacOwned),
    Signature(Signature<'a>),
    SignatureOwned(SignatureOwned),
    U32(u32),
    F32(f32),
    U64(u64),
    B032(B032<'a>),
    B032Owned(B032Owned),
    B0255(B0255<'a>),
    B0255Owned(B0255Owned),
    B064K(B064K<'a>),
    B064KOwned(B064KOwned),
    B016M(B016M<'a>),
    B016MOwned(B016MOwned),
}

/// Recursive enum representing a Decode-able field.
///
/// May be primitive or a nested struct.
///
/// Once the raw data is decoded, it is either classified as a primitive (e.g., integer, Boolean)
/// or a struct, which may itself contain multiple decoded fields. This type encapsulates that
/// distinction.
#[derive(Debug)]
pub enum DecodableField<'a> {
    /// Primitive field.
    Primitive(DecodablePrimitive<'a>),

    /// Structured field, allowing for nested data structures.
    Struct(Vec<DecodableField<'a>>),
}

impl SizeHint for PrimitiveMarker {
    // PrimitiveMarker requires a concrete marker instance to determine the size.
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        Err(Error::UnInitializedDecoder)
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        match self {
            Self::U8 => u8::size_hint(data, offset),
            Self::U16 => u16::size_hint(data, offset),
            Self::Bool => bool::size_hint(data, offset),
            Self::U24 => U24::size_hint(data, offset),
            Self::U256 => U256::size_hint(data, offset),
            Self::U256Owned => U256Owned::size_hint(data, offset),
            Self::Mac => Mac::size_hint(data, offset),
            Self::MacOwned => MacOwned::size_hint(data, offset),
            Self::Signature => Signature::size_hint(data, offset),
            Self::SignatureOwned => SignatureOwned::size_hint(data, offset),
            Self::U32 => u32::size_hint(data, offset),
            Self::F32 => f32::size_hint(data, offset),
            Self::U64 => u64::size_hint(data, offset),
            Self::B032 => B032::size_hint(data, offset),
            Self::B032Owned => B032Owned::size_hint(data, offset),
            Self::B0255 => B0255::size_hint(data, offset),
            Self::B0255Owned => B0255Owned::size_hint(data, offset),
            Self::B064K => B064K::size_hint(data, offset),
            Self::B064KOwned => B064KOwned::size_hint(data, offset),
            Self::B016M => B016M::size_hint(data, offset),
            Self::B016MOwned => B016MOwned::size_hint(data, offset),
        }
    }
}

impl SizeHint for FieldMarker {
    // FieldMarker requires a concrete marker instance to determine the size.
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        Err(Error::UnInitializedDecoder)
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        match self {
            Self::Primitive(p) => p.size_hint_(data, offset),
            Self::Struct(ps) => {
                let mut size = 0;
                for p in ps {
                    size += p.size_hint_(data, offset + size)?;
                }
                Ok(size)
            }
        }
    }
}

impl SizeHint for Vec<FieldMarker> {
    // The structure must be initialized before its aggregate size can be calculated.
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        Err(Error::UnInitializedDecoder)
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        let mut size = 0;
        for field in self {
            let field_size = field.size_hint_(data, offset + size)?;
            size += field_size;
        }
        Ok(size)
    }
}

impl From<PrimitiveMarker> for FieldMarker {
    fn from(v: PrimitiveMarker) -> Self {
        FieldMarker::Primitive(v)
    }
}

impl TryFrom<Vec<FieldMarker>> for FieldMarker {
    type Error = crate::Error;

    fn try_from(mut v: Vec<FieldMarker>) -> Result<Self, crate::Error> {
        match v.len() {
            // It shouldn't be possible to call this function with a void Vec but for safety
            // reasons it is implemented with TryFrom and not From if needed should be possible
            // to use From and just panic
            0 => Err(crate::Error::VoidFieldMarker),
            // This is always safe: if v.len is 1 pop can not fail
            1 => Ok(v.pop().unwrap()),
            _ => Ok(FieldMarker::Struct(v)),
        }
    }
}

impl<'a> From<DecodableField<'a>> for Vec<DecodableField<'a>> {
    fn from(v: DecodableField<'a>) -> Self {
        match v {
            DecodableField::Primitive(p) => vec![DecodableField::Primitive(p)],
            DecodableField::Struct(ps) => ps,
        }
    }
}

impl PrimitiveMarker {
    // Decodes a primitive value from a byte slice at the given offset, returning the corresponding
    // `DecodablePrimitive`. The specific decoding logic depends on the type of the primitive (e.g.,
    // `u8`, `u16`, etc.).
    fn decode<'a>(
        &self,
        data: &'a mut [u8],
        offset: usize,
    ) -> Result<DecodablePrimitive<'a>, Error> {
        match self {
            Self::U8 => Ok(DecodablePrimitive::U8(u8::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::U16 => Ok(DecodablePrimitive::U16(u16::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::Bool => Ok(DecodablePrimitive::Bool(bool::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::U24 => Ok(DecodablePrimitive::U24(U24::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::U256 => Ok(DecodablePrimitive::U256(U256::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::U256Owned => Ok(DecodablePrimitive::U256Owned(U256Owned::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::Mac => Ok(DecodablePrimitive::Mac(Mac::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::MacOwned => Ok(DecodablePrimitive::MacOwned(MacOwned::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::Signature => Ok(DecodablePrimitive::Signature(Signature::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::SignatureOwned => Ok(DecodablePrimitive::SignatureOwned(
                SignatureOwned::from_bytes_(&mut data[offset..])?,
            )),
            Self::U32 => Ok(DecodablePrimitive::U32(u32::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::F32 => Ok(DecodablePrimitive::F32(f32::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::U64 => Ok(DecodablePrimitive::U64(u64::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B032 => Ok(DecodablePrimitive::B032(B032::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B032Owned => Ok(DecodablePrimitive::B032Owned(B032Owned::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B0255 => Ok(DecodablePrimitive::B0255(B0255::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B0255Owned => Ok(DecodablePrimitive::B0255Owned(B0255Owned::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B064K => Ok(DecodablePrimitive::B064K(B064K::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B064KOwned => Ok(DecodablePrimitive::B064KOwned(B064KOwned::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B016M => Ok(DecodablePrimitive::B016M(B016M::from_bytes_(
                &mut data[offset..],
            )?)),
            Self::B016MOwned => Ok(DecodablePrimitive::B016MOwned(B016MOwned::from_bytes_(
                &mut data[offset..],
            )?)),
        }
    }
}

impl GetSize for DecodablePrimitive<'_> {
    fn get_size(&self) -> usize {
        match self {
            DecodablePrimitive::U8(v) => v.get_size(),
            DecodablePrimitive::U16(v) => v.get_size(),
            DecodablePrimitive::Bool(v) => v.get_size(),
            DecodablePrimitive::U24(v) => v.get_size(),
            DecodablePrimitive::U256(v) => v.get_size(),
            DecodablePrimitive::U256Owned(v) => v.get_size(),
            DecodablePrimitive::Mac(v) => v.get_size(),
            DecodablePrimitive::MacOwned(v) => v.get_size(),
            DecodablePrimitive::Signature(v) => v.get_size(),
            DecodablePrimitive::SignatureOwned(v) => v.get_size(),
            DecodablePrimitive::U32(v) => v.get_size(),
            DecodablePrimitive::F32(v) => v.get_size(),
            DecodablePrimitive::U64(v) => v.get_size(),
            DecodablePrimitive::B032(v) => v.get_size(),
            DecodablePrimitive::B032Owned(v) => v.get_size(),
            DecodablePrimitive::B0255(v) => v.get_size(),
            DecodablePrimitive::B0255Owned(v) => v.get_size(),
            DecodablePrimitive::B064K(v) => v.get_size(),
            DecodablePrimitive::B064KOwned(v) => v.get_size(),
            DecodablePrimitive::B016M(v) => v.get_size(),
            DecodablePrimitive::B016MOwned(v) => v.get_size(),
        }
    }
}

impl FieldMarker {
    // Implements the decoding functionality for a `FieldMarker`.
    // Depending on whether the field is primitive or structured, this method decodes the
    // corresponding data. If the field is a structure, it recursively decodes each nested field
    // and returns the resulting `DecodableField`.
    pub(crate) fn decode<'a>(&self, data: &'a mut [u8]) -> Result<DecodableField<'a>, Error> {
        match self {
            Self::Primitive(p) => Ok(DecodableField::Primitive(p.decode(data, 0)?)),
            Self::Struct(ps) => {
                let mut decodeds = Vec::new();
                let mut tail = data;
                for p in ps {
                    let field_size = p.size_hint_(tail, 0)?;
                    let (head, t) = tail.split_at_mut(field_size);
                    tail = t;
                    decodeds.push(p.decode(head)?);
                }
                Ok(DecodableField::Struct(decodeds))
            }
        }
    }
}
