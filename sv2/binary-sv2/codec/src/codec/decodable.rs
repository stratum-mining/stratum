use crate::{
    codec::{GetSize, SizeHint},
    datatypes::{Signature, Sv2DataType, U32AsRef, B016M, B0255, B032, B064K, U24, U256},
    Error,
};
use alloc::vec::Vec;
use core::convert::TryFrom;
#[cfg(not(feature = "no_std"))]
use std::io::{Cursor, Read};

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

    /// Converts a readable input to self representation.
    ///
    /// Reads data from an input which implements [`std::ioRead`] and constructs the original struct
    /// out of it.
    #[cfg(not(feature = "no_std"))]
    fn from_reader(reader: &mut impl Read) -> Result<Self, Error> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;

        let structure = Self::get_structure(&data[..])?;

        let mut fields = Vec::new();
        let mut reader = Cursor::new(data);

        for field in structure {
            fields.push(field.from_reader(&mut reader)?);
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
    Signature,
    U32,
    U32AsRef,
    F32,
    U64,
    B032,
    B0255,
    B064K,
    B016M,
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
    Signature(Signature<'a>),
    U32(u32),
    U32AsRef(U32AsRef<'a>),
    F32(f32),
    U64(u64),
    B032(B032<'a>),
    B0255(B0255<'a>),
    B064K(B064K<'a>),
    B016M(B016M<'a>),
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
    // PrimitiveMarker needs introspection to return a size hint. This method is not implementable.
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        match self {
            Self::U8 => u8::size_hint(data, offset),
            Self::U16 => u16::size_hint(data, offset),
            Self::Bool => bool::size_hint(data, offset),
            Self::U24 => U24::size_hint(data, offset),
            Self::U256 => U256::size_hint(data, offset),
            Self::Signature => Signature::size_hint(data, offset),
            Self::U32 => u32::size_hint(data, offset),
            Self::U32AsRef => U32AsRef::size_hint(data, offset),
            Self::F32 => f32::size_hint(data, offset),
            Self::U64 => u64::size_hint(data, offset),
            Self::B032 => B032::size_hint(data, offset),
            Self::B0255 => B0255::size_hint(data, offset),
            Self::B064K => B064K::size_hint(data, offset),
            Self::B016M => B016M::size_hint(data, offset),
        }
    }
}

impl SizeHint for FieldMarker {
    // FieldMarker need introspection to return a size hint. This method is not implementeable
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
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
    // FieldMarker need introspection to return a size hint. This method is not implementeable
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
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
    fn decode<'a>(&self, data: &'a mut [u8], offset: usize) -> DecodablePrimitive<'a> {
        match self {
            Self::U8 => DecodablePrimitive::U8(u8::from_bytes_unchecked(&mut data[offset..])),
            Self::U16 => DecodablePrimitive::U16(u16::from_bytes_unchecked(&mut data[offset..])),
            Self::Bool => DecodablePrimitive::Bool(bool::from_bytes_unchecked(&mut data[offset..])),
            Self::U24 => DecodablePrimitive::U24(U24::from_bytes_unchecked(&mut data[offset..])),
            Self::U256 => DecodablePrimitive::U256(U256::from_bytes_unchecked(&mut data[offset..])),
            Self::Signature => {
                DecodablePrimitive::Signature(Signature::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::U32 => DecodablePrimitive::U32(u32::from_bytes_unchecked(&mut data[offset..])),
            Self::U32AsRef => {
                DecodablePrimitive::U32AsRef(U32AsRef::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::F32 => DecodablePrimitive::F32(f32::from_bytes_unchecked(&mut data[offset..])),
            Self::U64 => DecodablePrimitive::U64(u64::from_bytes_unchecked(&mut data[offset..])),
            Self::B032 => DecodablePrimitive::B032(B032::from_bytes_unchecked(&mut data[offset..])),
            Self::B0255 => {
                DecodablePrimitive::B0255(B0255::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::B064K => {
                DecodablePrimitive::B064K(B064K::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::B016M => {
                DecodablePrimitive::B016M(B016M::from_bytes_unchecked(&mut data[offset..]))
            }
        }
    }

    // Decodes a primitive value from a reader stream, returning the corresponding
    // `DecodablePrimitive`. This is useful when reading data from a file or network socket,
    // where the data is not immediately available as a slice but must be read incrementally.
    #[allow(clippy::wrong_self_convention)]
    #[cfg(not(feature = "no_std"))]
    fn from_reader<'a>(&self, reader: &mut impl Read) -> Result<DecodablePrimitive<'a>, Error> {
        match self {
            Self::U8 => Ok(DecodablePrimitive::U8(u8::from_reader_(reader)?)),
            Self::U16 => Ok(DecodablePrimitive::U16(u16::from_reader_(reader)?)),
            Self::Bool => Ok(DecodablePrimitive::Bool(bool::from_reader_(reader)?)),
            Self::U24 => Ok(DecodablePrimitive::U24(U24::from_reader_(reader)?)),
            Self::U256 => Ok(DecodablePrimitive::U256(U256::from_reader_(reader)?)),
            Self::Signature => Ok(DecodablePrimitive::Signature(Signature::from_reader_(
                reader,
            )?)),
            Self::U32 => Ok(DecodablePrimitive::U32(u32::from_reader_(reader)?)),
            Self::U32AsRef => Ok(DecodablePrimitive::U32AsRef(U32AsRef::from_reader_(
                reader,
            )?)),
            Self::F32 => Ok(DecodablePrimitive::F32(f32::from_reader_(reader)?)),
            Self::U64 => Ok(DecodablePrimitive::U64(u64::from_reader_(reader)?)),
            Self::B032 => Ok(DecodablePrimitive::B032(B032::from_reader_(reader)?)),
            Self::B0255 => Ok(DecodablePrimitive::B0255(B0255::from_reader_(reader)?)),
            Self::B064K => Ok(DecodablePrimitive::B064K(B064K::from_reader_(reader)?)),
            Self::B016M => Ok(DecodablePrimitive::B016M(B016M::from_reader_(reader)?)),
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
            DecodablePrimitive::Signature(v) => v.get_size(),
            DecodablePrimitive::U32(v) => v.get_size(),
            DecodablePrimitive::U32AsRef(v) => v.get_size(),
            DecodablePrimitive::F32(v) => v.get_size(),
            DecodablePrimitive::U64(v) => v.get_size(),
            DecodablePrimitive::B032(v) => v.get_size(),
            DecodablePrimitive::B0255(v) => v.get_size(),
            DecodablePrimitive::B064K(v) => v.get_size(),
            DecodablePrimitive::B016M(v) => v.get_size(),
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
            Self::Primitive(p) => Ok(DecodableField::Primitive(p.decode(data, 0))),
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

    #[allow(clippy::wrong_self_convention)]
    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_reader<'a>(
        &self,
        reader: &mut impl Read,
    ) -> Result<DecodableField<'a>, Error> {
        match self {
            Self::Primitive(p) => Ok(DecodableField::Primitive(p.from_reader(reader)?)),
            Self::Struct(ps) => {
                let mut decodeds = Vec::new();
                for p in ps {
                    decodeds.push(p.from_reader(reader)?);
                }
                Ok(DecodableField::Struct(decodeds))
            }
        }
    }
}
