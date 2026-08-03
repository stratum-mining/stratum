// # Sequence and Optional Data Structures
//
// Provides specialized implementations of sequences and optional data types, primarily
// designed to handle serialized data with fixed size constraints. These structures are particularly
// suited for encoding and decoding variable-length and optional data fields within serialized
// formats.
//
// ## Provided Types
//
// ### `Seq0255`
// - Represents a sequence of up to 255 elements.
// - Includes utility methods such as:
//   - `iter_bytes()`: Provides byte references for each element without allocating.
//   - `new()`: Creates a `Seq0255` instance, enforcing the maximum length constraint.
// - Implements the `Decodable` trait for seamless deserialization, and `GetSize` to calculate the
//   encoded size, ensuring compatibility with various serialization formats.
//
// ### `Seq064K`
// - Represents a sequence of up to 65535 elements.
// - Similar to `Seq0255`, it provides:
//   - `iter_bytes()` to reference each element's bytes without allocating.
//   - `new()` enforces the maximum size limit, preventing excess memory usage.
// - Like `Seq0255`, `Seq064K` is `Decodable` and implements `GetSize`, making it versatile for
//   serialization scenarios.
//
// ### `Sv2Option`
// - Represents an optional data type, encoding a single or absent element.
// - Provides `to_option()` to convert to a standard `Option<Vec<u8>>`.
// - `new()` and `into_inner()` enable flexible conversions between `Option` and `Sv2Option`.
//
// ## Utility Macros
//
// - `impl_codec_for_sequence!`: Implements the `Decodable` trait for a sequence type, allowing for
//   a custom deserialization process that interprets field markers.
// - `impl_into_encodable_field_for_seq!`: Implements conversions to `EncodableField` for a
//   sequence, adapting the sequence for inclusion in serialized structures.

use super::inner::{Inner, InnerOwned};
use crate::{
    codec::{
        decodable::{Decodable, DecodableField, FieldMarker, GetMarker, PrimitiveMarker},
        encodable::{EncodableField, EncodablePrimitive},
        Fixed, GetSize,
    },
    datatypes::{Sv2DataType, *},
    Error,
};
use core::{marker::PhantomData, ops::Index, slice};

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq0255<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Iterates over element payload bytes without allocating.
    pub fn iter_bytes(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(|x| x.as_bytes())
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq0255Owned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Iterates over element payload bytes without allocating.
    pub fn iter_bytes(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(|x| x.as_bytes())
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq064K<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Iterates over element payload bytes without allocating.
    pub fn iter_bytes(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(|x| x.as_bytes())
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq064KOwned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Iterates over element payload bytes without allocating.
    pub fn iter_bytes(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(|x| x.as_bytes())
    }
}

/// [`Seq0255`] represents a sequence with a maximum length of 255 elements.
/// This structure uses a generic type `T` and a lifetime parameter `'a`.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq0255<'a, T>(Vec<T>, PhantomData<&'a T>);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq0255Owned<T>(Vec<T>);

impl<'a, T> Index<usize> for Seq0255<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<T> Index<usize> for Seq0255Owned<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<'a, T: 'a> Seq0255<'a, T> {
    const HEADERSIZE: usize = 1;

    // Determines the expected length of the sequence by examining the first byte of `data`.
    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            Ok(data[0] as usize)
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    /// Creates a new `Seq0255` instance with the given inner vector.
    pub fn new(inner: Vec<T>) -> Result<Self, Error> {
        if inner.len() <= 255 {
            Ok(Self(inner, PhantomData))
        } else {
            Err(Error::SeqExceedsMaxSize)
        }
    }

    /// Consumes the `Seq0255` and returns the inner vector of elements.
    pub fn into_inner(self) -> Vec<T> {
        self.0
    }

    /// Returns the sequence as a slice.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Iterates over the sequence by reference.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Length of Seq0255 which is a list of bytes up-to 255 len
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when the sequence contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> Seq0255Owned<T> {
    const HEADERSIZE: usize = 1;

    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            Ok(data[0] as usize)
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    pub fn new(inner: Vec<T>) -> Result<Self, Error> {
        if inner.len() <= 255 {
            Ok(Self(inner))
        } else {
            Err(Error::SeqExceedsMaxSize)
        }
    }

    pub fn into_inner(self) -> Vec<T> {
        self.0
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T: GetSize> GetSize for Seq0255<'_, T> {
    // Calculates the total size of the sequence in bytes.
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

impl<T: GetSize> GetSize for Seq0255Owned<T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

/// [`Seq064K`] represents a sequence with a maximum length of 65535 elements.
/// This structure uses a generic type `T` and a lifetime parameter `'a`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq064K<'a, T>(Vec<T>, PhantomData<&'a T>);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq064KOwned<T>(Vec<T>);

impl<'a, T> Index<usize> for Seq064K<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<T> Index<usize> for Seq064KOwned<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<'a, T: 'a> Seq064K<'a, T> {
    const HEADERSIZE: usize = 2;

    // Determines the expected length of the sequence by examining the first two bytes of `data`.
    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            Ok(u16::from_le_bytes([data[0], data[1]]) as usize)
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    /// Creates a new `Seq064K` instance with the given inner vector.
    pub fn new(inner: Vec<T>) -> Result<Self, Error> {
        if inner.len() <= 65535 {
            Ok(Self(inner, PhantomData))
        } else {
            Err(Error::SeqExceedsMaxSize)
        }
    }

    /// Consumes the `Seq064K` and returns the inner vector of elements.
    pub fn into_inner(self) -> Vec<T> {
        self.0
    }

    /// Returns the sequence as a slice.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Iterates over the sequence by reference.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Length of Seq0255 which is a list of bytes up-to 64k len
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when the sequence contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> Seq064KOwned<T> {
    const HEADERSIZE: usize = 2;

    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            Ok(u16::from_le_bytes([data[0], data[1]]) as usize)
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    pub fn new(inner: Vec<T>) -> Result<Self, Error> {
        if inner.len() <= 65535 {
            Ok(Self(inner))
        } else {
            Err(Error::SeqExceedsMaxSize)
        }
    }

    pub fn into_inner(self) -> Vec<T> {
        self.0
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T: GetSize> GetSize for Seq064K<'_, T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

impl<T: GetSize> GetSize for Seq064KOwned<T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

/// Macro to implement encoding and decoding traits for sequence types (`Seq0255`, `Seq064K`, and
/// `Sv2Option`).
macro_rules! impl_codec_for_sequence {
    ($a:ty) => {
        impl<'a, T: 'a + Sv2DataType<'a> + GetMarker + GetSize + Decodable<'a>> Decodable<'a>
            for $a
        {
            fn get_structure(
                data: &[u8],
            ) -> Result<Vec<crate::codec::decodable::FieldMarker>, Error> {
                let len = Self::expected_len(data)?;
                let available = data.len().saturating_sub(Self::HEADERSIZE);
                if len > available {
                    return Err(Error::ReadError(data.len(), len + Self::HEADERSIZE));
                }
                let mut inner = Vec::with_capacity(len + Self::HEADERSIZE);
                for _ in 0..Self::HEADERSIZE {
                    inner.push(FieldMarker::Primitive(PrimitiveMarker::U8));
                }
                let inner_type = T::get_marker();
                inner.resize(len + Self::HEADERSIZE, inner_type);
                Ok(inner)
            }

            fn from_decoded_fields(
                data: Vec<crate::codec::decodable::DecodableField<'a>>,
            ) -> Result<Self, Error> {
                let mut inner: Vec<T> = Vec::with_capacity(data.len());
                let mut i = 0;
                for element in data {
                    if i >= Self::HEADERSIZE {
                        match element {
                            DecodableField::Primitive(p) => {
                                let element =
                                    T::from_decoded_fields(vec![DecodableField::Primitive(p)]);
                                inner.push(element?)
                            }
                            DecodableField::Struct(fields) => {
                                let element = T::from_decoded_fields(fields);
                                inner.push(element?)
                            }
                        }
                    }
                    i += 1;
                }
                Ok(Self(inner, PhantomData))
            }

            fn from_bytes(data: &'a mut [u8]) -> Result<Self, Error> {
                let len = Self::expected_len(data)?;

                let mut inner = Vec::new();
                let mut tail = &mut data[Self::HEADERSIZE..];

                for _ in 0..len {
                    let element_size = T::size_hint(tail, 0)?;
                    if element_size > tail.len() {
                        return Err(Error::OutOfBound);
                    }
                    let (head, t) = tail.split_at_mut(element_size);
                    tail = t;
                    inner.push(T::from_bytes_(head)?);
                }
                Ok(Self(inner, PhantomData))
            }
        }
    };
}

macro_rules! impl_decodable_for_owned_sequence {
    ($a:ty) => {
        impl<'a, T: 'a + Sv2DataType<'a> + GetMarker + GetSize + Decodable<'a>> Decodable<'a>
            for $a
        {
            fn get_structure(data: &[u8]) -> Result<Vec<FieldMarker>, Error> {
                let len = Self::expected_len(data)?;
                let available = data.len().saturating_sub(Self::HEADERSIZE);
                if len > available {
                    return Err(Error::ReadError(data.len(), len + Self::HEADERSIZE));
                }
                let mut inner = Vec::with_capacity(len + Self::HEADERSIZE);
                for _ in 0..Self::HEADERSIZE {
                    inner.push(FieldMarker::Primitive(PrimitiveMarker::U8));
                }
                let inner_type = T::get_marker();
                inner.resize(len + Self::HEADERSIZE, inner_type);
                Ok(inner)
            }

            fn from_decoded_fields(data: Vec<DecodableField<'a>>) -> Result<Self, Error> {
                let mut inner: Vec<T> = Vec::with_capacity(data.len());
                let mut i = 0;
                for element in data {
                    if i >= Self::HEADERSIZE {
                        match element {
                            DecodableField::Primitive(p) => inner
                                .push(T::from_decoded_fields(vec![DecodableField::Primitive(p)])?),
                            DecodableField::Struct(fields) => {
                                inner.push(T::from_decoded_fields(fields)?)
                            }
                        }
                    }
                    i += 1;
                }
                Ok(Self(inner))
            }

            fn from_bytes(data: &'a mut [u8]) -> Result<Self, Error> {
                let len = Self::expected_len(data)?;

                let mut inner = Vec::new();
                let mut tail = &mut data[Self::HEADERSIZE..];

                for _ in 0..len {
                    let element_size = T::size_hint(tail, 0)?;
                    if element_size > tail.len() {
                        return Err(Error::OutOfBound);
                    }
                    let (head, t) = tail.split_at_mut(element_size);
                    tail = t;
                    inner.push(T::from_bytes_(head)?);
                }
                Ok(Self(inner))
            }
        }
    };
}

// Implementations for encoding/decoding
impl_codec_for_sequence!(Seq0255<'a, T>);
impl_codec_for_sequence!(Seq064K<'a, T>);
impl_codec_for_sequence!(Sv2Option<'a, T>);
impl_decodable_for_owned_sequence!(Seq0255Owned<T>);
impl_decodable_for_owned_sequence!(Seq064KOwned<T>);
impl_decodable_for_owned_sequence!(Sv2OptionOwned<T>);

/// The `impl_into_encodable_field_for_seq` macro provides implementations of the `From` trait
/// to convert `Seq0255`, `Seq064K`, and `Sv2Option` types into `EncodableField`, making these
/// sequence types compatible with encoding.
macro_rules! impl_into_encodable_field_for_borrowed_seq {
    ($a:ty) => {
        impl<'a> From<Seq064K<'a, $a>> for EncodableField<'a> {
            fn from(v: Seq064K<'a, $a>) -> Self {
                let inner_len = v.0.len() as u16;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity(inner_len as usize + 2);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(
                    inner_len.to_le_bytes()[0],
                )));
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(
                    inner_len.to_le_bytes()[1],
                )));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }

        impl<'a> From<Seq0255<'a, $a>> for EncodableField<'a> {
            fn from(v: Seq0255<$a>) -> Self {
                let inner_len = v.0.len() as u8;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity((inner_len as usize) + 1);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(inner_len)));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }

        impl<'a> From<Sv2Option<'a, $a>> for EncodableField<'a> {
            fn from(v: Sv2Option<$a>) -> Self {
                let inner_len = v.0.len() as u8;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity((inner_len as usize) + 1);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(inner_len)));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }
    };
}

macro_rules! impl_into_encodable_field_for_owned_seq {
    ($a:ty) => {
        impl<'a> From<Seq064KOwned<$a>> for EncodableField<'a> {
            fn from(v: Seq064KOwned<$a>) -> Self {
                let inner_len = v.0.len() as u16;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity(inner_len as usize + 2);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(
                    inner_len.to_le_bytes()[0],
                )));
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(
                    inner_len.to_le_bytes()[1],
                )));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }

        impl<'a> From<Seq0255Owned<$a>> for EncodableField<'a> {
            fn from(v: Seq0255Owned<$a>) -> Self {
                let inner_len = v.0.len() as u8;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity((inner_len as usize) + 1);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(inner_len)));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }

        impl<'a> From<Sv2OptionOwned<$a>> for EncodableField<'a> {
            fn from(v: Sv2OptionOwned<$a>) -> Self {
                let inner_len = v.0.len() as u8;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity((inner_len as usize) + 1);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::U8(inner_len)));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }
    };
}

impl_into_encodable_field_for_borrowed_seq!(bool);
impl_into_encodable_field_for_borrowed_seq!(u8);
impl_into_encodable_field_for_borrowed_seq!(u16);
impl_into_encodable_field_for_borrowed_seq!(U24);
impl_into_encodable_field_for_borrowed_seq!(u32);
impl_into_encodable_field_for_borrowed_seq!(u64);
impl_into_encodable_field_for_borrowed_seq!(U256<'a>);
impl_into_encodable_field_for_borrowed_seq!(Mac<'a>);
impl_into_encodable_field_for_borrowed_seq!(Signature<'a>);
impl_into_encodable_field_for_borrowed_seq!(B0255<'a>);
impl_into_encodable_field_for_borrowed_seq!(B064K<'a>);
impl_into_encodable_field_for_borrowed_seq!(B016M<'a>);

impl_into_encodable_field_for_owned_seq!(bool);
impl_into_encodable_field_for_owned_seq!(u8);
impl_into_encodable_field_for_owned_seq!(u16);
impl_into_encodable_field_for_owned_seq!(U24);
impl_into_encodable_field_for_owned_seq!(u32);
impl_into_encodable_field_for_owned_seq!(u64);
impl_into_encodable_field_for_owned_seq!(U256Owned);
impl_into_encodable_field_for_owned_seq!(MacOwned);
impl_into_encodable_field_for_owned_seq!(SignatureOwned);
impl_into_encodable_field_for_owned_seq!(B0255Owned);
impl_into_encodable_field_for_owned_seq!(B064KOwned);
impl_into_encodable_field_for_owned_seq!(B016MOwned);

impl<T> TryFrom<Vec<T>> for Seq0255<'_, T> {
    type Error = Error;
    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        Seq0255::new(value)
    }
}

impl<T> TryFrom<Vec<T>> for Seq0255Owned<T> {
    type Error = Error;
    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        Seq0255Owned::new(value)
    }
}

impl<T> TryFrom<Vec<T>> for Seq064K<'_, T> {
    type Error = Error;
    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        Seq064K::new(value)
    }
}

impl<T> TryFrom<Vec<T>> for Seq064KOwned<T> {
    type Error = Error;
    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        Seq064KOwned::new(value)
    }
}

impl<T: Fixed> Seq0255<'_, T> {
    /// converts into an owned sequence.
    pub fn into_owned(self) -> Seq0255Owned<T> {
        // Safe unwrap cause the initial value is a valid Seq0255
        Seq0255Owned::new(self.0).unwrap()
    }
}
impl<T: Fixed> Sv2Option<'_, T> {
    /// converts into an owned option.
    pub fn into_owned(self) -> Sv2OptionOwned<T> {
        Sv2OptionOwned::new(self.into_inner())
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq0255<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts into an owned sequence.
    pub fn into_owned(self) -> Seq0255Owned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let seq = self.0;
        let owned_seq = seq.into_iter().map(|x| x.into_owned()).collect();
        // Safe unwrap cause the initial value is a valid Seq0255
        Seq0255Owned::new(owned_seq).unwrap()
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2Option<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts into an owned option.
    pub fn into_owned(self) -> Sv2OptionOwned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let inner = self.into_inner();
        let owned_inner = inner.map(|x| x.into_owned());
        Sv2OptionOwned::new(owned_inner)
    }
}

impl<T: Fixed> Seq064K<'_, T> {
    /// converts into an owned sequence.
    pub fn into_owned(self) -> Seq064KOwned<T> {
        // Safe unwrap cause the initial value is a valid Seq064K
        Seq064KOwned::new(self.0).unwrap()
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq064K<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts into an owned sequence.
    pub fn into_owned(self) -> Seq064KOwned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let seq = self.0;
        let owned_seq = seq.into_iter().map(|x| x.into_owned()).collect();
        // Safe unwrap cause the initial value is a valid Seq064K
        Seq064KOwned::new(owned_seq).unwrap()
    }
}

/// The lifetime 'a is defined.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Sv2Option<'a, T>(Vec<T>, PhantomData<&'a T>);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Sv2OptionOwned<T>(Vec<T>);

// TODO add test for that
impl<'a, const SIZE: usize> Sv2Option<'a, super::inner::Inner<'a, true, SIZE, 0, 0>> {
    /// Gets the owned first element of the sequence, if present
    pub fn to_option(&self) -> Option<Vec<u8>> {
        match self.0.len() {
            0 => None,
            1 => Some(self.0[0].to_owned_bytes()),
            // is impossible to deserialize Sv2Options with len bigger than 1
            _ => unreachable!(),
        }
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2Option<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Gets the reference to the first element's payload bytes, if present.
    pub fn as_option_bytes(&self) -> Option<&[u8]> {
        match self.0.len() {
            0 => None,
            1 => Some(self.0[0].as_bytes()),
            // is impossible to deserialize Sv2Options with len bigger than 1
            _ => unreachable!(),
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2OptionOwned<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Gets the reference to the first element's payload bytes, if present.
    pub fn as_option_bytes(&self) -> Option<&[u8]> {
        match self.0.len() {
            0 => None,
            1 => Some(self.0[0].as_bytes()),
            // is impossible to deserialize Sv2Options with len bigger than 1
            _ => unreachable!(),
        }
    }
}

impl<'a, T: 'a> Sv2Option<'a, T> {
    const HEADERSIZE: usize = 1;

    /// Return the len of the inner vector
    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            match data[0] {
                0 => Ok(0),
                1 => Ok(1),
                _ => Err(Error::Sv2OptionHaveMoreThenOneElement(data[0])),
            }
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    /// Initializes a new option type
    pub fn new(inner: Option<T>) -> Self {
        match inner {
            Some(x) => Self(vec![x], PhantomData),
            None => Self(vec![], PhantomData),
        }
    }

    /// Gets the inner value of Sv2Option
    pub fn into_inner(mut self) -> Option<T> {
        let len = self.0.len();
        match len {
            0 => None,
            // safe unwrap we already checked the len
            1 => Some(self.0.pop().unwrap()),
            // is impossible to deserialize Sv2Options with len bigger than 1
            _ => unreachable!(),
        }
    }
}

impl<T> Sv2OptionOwned<T> {
    const HEADERSIZE: usize = 1;

    fn expected_len(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= Self::HEADERSIZE {
            match data[0] {
                0 => Ok(0),
                1 => Ok(1),
                _ => Err(Error::Sv2OptionHaveMoreThenOneElement(data[0])),
            }
        } else {
            Err(Error::ReadError(data.len(), Self::HEADERSIZE))
        }
    }

    pub fn new(inner: Option<T>) -> Self {
        match inner {
            Some(x) => Self(vec![x]),
            None => Self(vec![]),
        }
    }

    pub fn into_inner(mut self) -> Option<T> {
        let len = self.0.len();
        match len {
            0 => None,
            1 => Some(self.0.pop().unwrap()),
            _ => unreachable!(),
        }
    }
}

impl<T: GetSize> GetSize for Sv2Option<'_, T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

impl<T: GetSize> GetSize for Sv2OptionOwned<T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}

#[cfg(test)]
mod test {
    use crate::{Decodable, Seq064K};

    #[test]
    fn get_structure_does_not_overallocate_from_tiny_header() {
        let data: [u8; 2] = [0xff, 0xff];
        match <Seq064K<'static, u8> as Decodable<'static>>::get_structure(&data) {
            Err(_) => {}
            Ok(markers) => assert!(
                markers.len() <= data.len(),
                "get_structure built {} markers from a {}-byte buffer",
                markers.len(),
                data.len()
            ),
        }
    }
}
