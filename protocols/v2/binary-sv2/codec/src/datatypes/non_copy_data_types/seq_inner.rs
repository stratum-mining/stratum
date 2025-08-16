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
//   - `to_vec()`: Converts each element into its byte vector representation.
//   - `inner_as_ref()`: Provides references to the inner data for each element.
//   - `new()`: Creates a `Seq0255` instance, enforcing the maximum length constraint.
// - Implements the `Decodable` trait for seamless deserialization, and `GetSize` to calculate the
//   encoded size, ensuring compatibility with various serialization formats.
//
// ### `Seq064K`
// - Represents a sequence of up to 65535 elements.
// - Similar to `Seq0255`, it provides:
//   - `to_vec()` and `inner_as_ref()` methods to convert or reference each element.
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
//
// ## Build Options
//
// - `prop_test`: Enables property-based testing compatibility by implementing `TryFrom` for `Vec`
//   conversions.
// - `no_std`: Allows the module to be used in `no_std` environments by disabling `std::io::Read`
//   dependencies.

use crate::{
    codec::{
        decodable::{Decodable, DecodableField, FieldMarker, GetMarker, PrimitiveMarker},
        encodable::{EncodableField, EncodablePrimitive},
        Fixed, GetSize,
    },
    datatypes::{Sv2DataType, *},
    Error,
};
use core::marker::PhantomData;

// TODO add test for that
impl<'a, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq0255<'a, super::inner::Inner<'a, false, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Converts the inner types to owned vector, and collects.
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.0.iter().map(|x| x.to_vec()).collect()
    }
    /// Converts the inner types to shared reference, and collects.
    pub fn inner_as_ref(&self) -> Vec<&[u8]> {
        self.0.iter().map(|x| x.inner_as_ref()).collect()
    }
}

// TODO add test for that
impl<'a, const SIZE: usize> Seq0255<'a, super::inner::Inner<'a, true, SIZE, 0, 0>> {
    /// Converts the inner types to owned vector, and collects.
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.0.iter().map(|x| x.to_vec()).collect()
    }

    /// Converts the inner types to shared reference, and collects.
    pub fn inner_as_ref(&self) -> Vec<&[u8]> {
        self.0.iter().map(|x| x.inner_as_ref()).collect()
    }
}
// TODO add test for that
impl<'a, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq064K<'a, super::inner::Inner<'a, false, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// Converts the inner types to owned vector, and collects.
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.0.iter().map(|x| x.to_vec()).collect()
    }

    /// Converts the inner types to shared reference, and collects.
    pub fn inner_as_ref(&self) -> Vec<&[u8]> {
        self.0.iter().map(|x| x.inner_as_ref()).collect()
    }
}

// TODO add test for that
impl<'a, const SIZE: usize> Seq064K<'a, super::inner::Inner<'a, true, SIZE, 0, 0>> {
    /// Converts the inner types to owned vector, and collects.
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.0.iter().map(|x| x.to_vec()).collect()
    }

    /// Converts the inner types to shared reference, and collects.
    pub fn inner_as_ref(&self) -> Vec<&[u8]> {
        self.0.iter().map(|x| x.inner_as_ref()).collect()
    }
}

#[cfg(not(feature = "no_std"))]
use std::io::Read;

/// [`Seq0255`] represents a sequence with a maximum length of 255 elements.
/// This structure uses a generic type `T` and a lifetime parameter `'a`.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq0255<'a, T>(pub Vec<T>, PhantomData<&'a T>);

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

/// [`Seq064K`] represents a sequence with a maximum length of 65535 elements.
/// This structure uses a generic type `T` and a lifetime parameter `'a`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Seq064K<'a, T>(pub(crate) Vec<T>, PhantomData<&'a T>);

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
                            // A struct always recursivly call decode until it reach a primitive
                            DecodableField::Struct(_) => unreachable!(),
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
                    inner.push(T::from_bytes_unchecked(head));
                }
                Ok(Self(inner, PhantomData))
            }

            #[cfg(not(feature = "no_std"))]
            fn from_reader(reader: &mut impl Read) -> Result<Self, Error> {
                let mut header = vec![0; Self::HEADERSIZE];
                reader.read_exact(&mut header)?;

                let len = Self::expected_len(&header)?;

                let mut inner = Vec::new();

                for _ in 0..len {
                    inner.push(T::from_reader_(reader)?);
                }
                Ok(Self(inner, PhantomData))
            }
        }
    };
}

// Implementations for encoding/decoding
impl_codec_for_sequence!(Seq0255<'a, T>);
impl_codec_for_sequence!(Seq064K<'a, T>);
impl_codec_for_sequence!(Sv2Option<'a, T>);

/// The `impl_into_encodable_field_for_seq` macro provides implementations of the `From` trait
/// to convert `Seq0255`, `Seq064K`, and `Sv2Option` types into `EncodableField`, making these
/// sequence types compatible with encoding.
macro_rules! impl_into_encodable_field_for_seq {
    ($a:ty) => {
        impl<'a> From<Seq064K<'a, $a>> for EncodableField<'a> {
            fn from(v: Seq064K<'a, $a>) -> Self {
                let inner_len = v.0.len() as u16;
                let mut as_encodable: Vec<EncodableField> =
                    Vec::with_capacity((inner_len + 2) as usize);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::OwnedU8(
                    inner_len.to_le_bytes()[0],
                )));
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::OwnedU8(
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
                    Vec::with_capacity((inner_len + 1) as usize);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::OwnedU8(
                    inner_len,
                )));
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
                    Vec::with_capacity((inner_len + 1) as usize);
                as_encodable.push(EncodableField::Primitive(EncodablePrimitive::OwnedU8(
                    inner_len,
                )));
                for element in v.0 {
                    as_encodable.push(element.into());
                }
                EncodableField::Struct(as_encodable)
            }
        }
    };
}

impl_into_encodable_field_for_seq!(bool);
impl_into_encodable_field_for_seq!(u8);
impl_into_encodable_field_for_seq!(u16);
impl_into_encodable_field_for_seq!(U24);
impl_into_encodable_field_for_seq!(u32);
impl_into_encodable_field_for_seq!(u64);
impl_into_encodable_field_for_seq!(U256<'a>);
impl_into_encodable_field_for_seq!(Signature<'a>);
impl_into_encodable_field_for_seq!(B0255<'a>);
impl_into_encodable_field_for_seq!(B064K<'a>);
impl_into_encodable_field_for_seq!(B016M<'a>);

#[cfg(feature = "prop_test")]
impl<'a, T> core::convert::TryFrom<Seq0255<'a, T>> for Vec<T> {
    type Error = &'static str;
    fn try_from(v: Seq0255<'a, T>) -> Result<Self, Self::Error> {
        if v.0.len() > 255 {
            Ok(v.0)
        } else {
            Err("Incorrect length, expected 225")
        }
    }
}

#[cfg(feature = "prop_test")]
impl<'a, T> core::convert::TryFrom<Seq064K<'a, T>> for Vec<T> {
    type Error = &'static str;
    fn try_from(v: Seq064K<'a, T>) -> Result<Self, Self::Error> {
        if v.0.len() > 64 {
            Ok(v.0)
        } else {
            Err("Incorrect length, expected 64")
        }
    }
}

impl<T> From<Vec<T>> for Seq0255<'_, T> {
    fn from(v: Vec<T>) -> Self {
        Seq0255(v, PhantomData)
    }
}

impl<T> From<Vec<T>> for Seq064K<'_, T> {
    fn from(v: Vec<T>) -> Self {
        Seq064K(v, PhantomData)
    }
}

impl<T: Fixed> Seq0255<'_, T> {
    /// converts the lifetime to static
    pub fn into_static(self) -> Seq0255<'static, T> {
        // Safe unwrap cause the initial value is a valid Seq0255
        Seq0255::new(self.0).unwrap()
    }
}
impl<T: Fixed> Sv2Option<'_, T> {
    /// converts the lifetime to static
    pub fn into_static(self) -> Sv2Option<'static, T> {
        Sv2Option::new(self.into_inner())
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq0255<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts the lifetime to static
    pub fn into_static(
        self,
    ) -> Seq0255<'static, Inner<'static, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let seq = self.0;
        let static_seq = seq.into_iter().map(|x| x.into_static()).collect();
        // Safe unwrap cause the initial value is a valid Seq0255
        Seq0255::new(static_seq).unwrap()
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2Option<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts the lifetime to static
    pub fn into_static(
        self,
    ) -> Sv2Option<'static, Inner<'static, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let inner = self.into_inner();
        let static_inner = inner.map(|x| x.into_static());
        Sv2Option::new(static_inner)
    }
}

impl<T: Fixed> Seq064K<'_, T> {
    /// converts the lifetime to static
    pub fn into_static(self) -> Seq064K<'static, T> {
        // Safe unwrap cause the initial value is a valid Seq064K
        Seq064K::new(self.0).unwrap()
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Seq064K<'a, Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
{
    /// converts the lifetime to static
    pub fn into_static(
        self,
    ) -> Seq064K<'static, Inner<'static, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>> {
        let seq = self.0;
        let static_seq = seq.into_iter().map(|x| x.into_static()).collect();
        // Safe unwrap cause the initial value is a valid Seq064K
        Seq064K::new(static_seq).unwrap()
    }
}

/// The lifetime 'a is defined.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Sv2Option<'a, T>(pub Vec<T>, PhantomData<&'a T>);

// TODO add test for that
impl<'a, const SIZE: usize> Sv2Option<'a, super::inner::Inner<'a, true, SIZE, 0, 0>> {
    /// Gets the owned first element of the sequence, if present
    pub fn to_option(&self) -> Option<Vec<u8>> {
        let v: Vec<Vec<u8>> = self.0.iter().map(|x| x.to_vec()).collect();
        match v.len() {
            0 => None,
            1 => Some(v[0].clone()),
            // is impossible to deserialize Sv2Options with len bigger than 1
            _ => unreachable!(),
        }
    }
    /// Gets the reference to first element of the sequence, if present
    pub fn inner_as_ref(&self) -> Option<&[u8]> {
        let v: Vec<&[u8]> = self.0.iter().map(|x| x.inner_as_ref()).collect();
        match v.len() {
            0 => None,
            1 => Some(v[0]),
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

impl<T: GetSize> GetSize for Sv2Option<'_, T> {
    fn get_size(&self) -> usize {
        let mut size = Self::HEADERSIZE;
        for with_size in &self.0 {
            size += with_size.get_size()
        }
        size
    }
}
