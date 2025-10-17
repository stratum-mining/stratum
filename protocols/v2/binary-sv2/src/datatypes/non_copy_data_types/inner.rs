// Provides a flexible container for managing either owned or mutable references to byte arrays.
//
// # Overview
// Defines the `Inner` enum to manage both mutable references to byte slices and owned vectors
// (`Vec<u8>`). Accommodates both fixed-size and variable-size data using const generics, offering
// control over size and header length constraints.
//
// # `Inner` Enum
// The `Inner` enum has two variants for data management:
// - `Ref(&'a mut [u8])`: A mutable reference to a byte slice, allowing in-place data modification.
// - `Owned(Vec<u8>)`: An owned byte vector, providing full control over data and supporting move
//   semantics.
//
// ## Const Parameters
// Configured using const generics for the following constraints:
// - `ISFIXED`: Indicates whether the data has a fixed size.
// - `SIZE`: Specifies the size when `ISFIXED` is true.
// - `HEADERSIZE`: Defines the size of the header, useful for variable-size data with a prefix
//   length.
// - `MAXSIZE`: Limits the maximum allowable size of the data.
//
// # Usage
// `Inner` offers several methods for data manipulation, including:
// - `to_vec()`: Returns a `Vec<u8>`, cloning the slice or owned data.
// - `inner_as_ref()` and `inner_as_mut()`: Provide immutable or mutable access to the data.
// - `expected_length(data: &[u8])`: Computes the expected length, validating it against
//   constraints.
// - `get_header()`: Returns the data's header based on `HEADERSIZE`.
//
// # Implementations
// The `Inner` enum implements `PartialEq`, `Eq`, `GetSize`, `SizeHint`, and `Sv2DataType` traits,
// enabling buffer size calculations, reading, and writing to byte slices.
//
// # Error Handling
// Methods return `Error` types when data exceeds size limits or deviates from the configuration,
// ensuring compliance with defined constraints.
use super::IntoOwned;
use crate::{
    codec::{GetSize, SizeHint},
    datatypes::Sv2DataType,
    Error,
};

use alloc::vec::Vec;
use core::convert::{TryFrom, TryInto};
#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Read, Write};

// The `Inner` enum represents a flexible container for managing both reference to mutable
// slices and owned bytes arrays (`Vec<u8>`). This design allows the container to either own
// its data or simply reference existing mutable data. It uses const generics to differentiate
// between fixed-size and variable-size data, as well as to specify key size-related parameters.
//
// It has two variants:
// - `Ref(&'a mut [u8])`: A mutable reference to an external byte slice.
// - `Owned (Vec<u8>)`: A vector that owns its data, enabling dynamic ownership.
//
// The const parameters that govern the behavior of this enum are:
//  - `ISFIXED`: A boolean indicating whether the data has a fixed size.
//  - `SIZE`: The size of the data if `ISFIXED` is true.
//  - `HEADERSIZE`: The size of the header, which is used for types that require a prefix to
//    describe the content's length.
//  - `MAXSIZE`: The maximum allowable size for the data.

#[derive(Debug)]
pub enum Inner<
    'a,
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
> {
    Ref(&'a mut [u8]),
    Owned(Vec<u8>),
}

impl<const SIZE: usize> Inner<'_, true, SIZE, 0, 0> {
    // Converts the inner data to a vector, either by cloning the referenced slice or
    // returning a clone of the owned vector.
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            Inner::Ref(ref_) => ref_.to_vec(),
            Inner::Owned(v) => v.clone(),
        }
    }
    // Returns an immutable reference to the inner data, whether it's a reference or
    // an owned vector.
    pub fn inner_as_ref(&self) -> &[u8] {
        match self {
            Inner::Ref(ref_) => ref_,
            Inner::Owned(v) => v,
        }
    }
    // Provides a mutable reference to the inner data, allowing modification if the
    // data is being referenced.
    pub fn inner_as_mut(&mut self) -> &mut [u8] {
        match self {
            Inner::Ref(ref_) => ref_,
            Inner::Owned(v) => v,
        }
    }
}

impl<const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Inner<'_, false, SIZE, HEADERSIZE, MAXSIZE>
{
    // Similar to the fixed-size variant, this method converts the inner data into a vector.
    // The data is either cloned from the referenced slice or returned as a clone of the
    // owned vector.
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            Inner::Ref(ref_) => ref_[..].to_vec(),
            Inner::Owned(v) => v[..].to_vec(),
        }
    }
    // Returns an immutable reference to the inner data for variable-size types, either
    // referencing a slice or an owned vector.
    pub fn inner_as_ref(&self) -> &[u8] {
        match self {
            Inner::Ref(ref_) => &ref_[..],
            Inner::Owned(v) => &v[..],
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    PartialEq for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    // Provides equality comparison between two `Inner` instances by checking the equality
    // of their data, regardless of whether they are references or owned vectors.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Inner::Ref(b), Inner::Owned(a)) => *b == &a[..],
            (Inner::Owned(b), Inner::Ref(a)) => *a == &b[..],
            (Inner::Owned(b), Inner::Owned(a)) => b == a,
            (Inner::Ref(b), Inner::Ref(a)) => b == a,
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> Eq
    for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    // Calculates the expected length of the data based on the type's parameters (fixed-size
    // or variable-size). It checks if the length conforms to the specified constraints like
    // `SIZE`, `MAXSIZE`, and `HEADERSIZE`, returning the length or an error if the data
    // exceeds the limits.
    fn expected_length(data: &[u8]) -> Result<usize, Error> {
        let expected_length = match ISFIXED {
            true => Self::expected_length_fixed(),
            false => Self::expected_length_variable(data)?,
        };
        if ISFIXED || expected_length <= (MAXSIZE + HEADERSIZE) {
            Ok(expected_length)
        } else {
            Err(Error::ReadError(data.len(), MAXSIZE))
        }
    }

    // For fixed-size data, the expected length is always `SIZE`.
    fn expected_length_fixed() -> usize {
        SIZE
    }

    // For variable-size data, this method calculates the size based on the header.
    // The header describes the length of the data, and this method ensures the data
    // is correctly sized relative to the header information.
    fn expected_length_variable(data: &[u8]) -> Result<usize, Error> {
        if data.len() >= HEADERSIZE {
            let size = match HEADERSIZE {
                1 => Ok(data[0] as usize),
                2 => Ok(u16::from_le_bytes([data[0], data[1]]) as usize),
                3 => Ok(u32::from_le_bytes([data[0], data[1], data[2], 0]) as usize),
                // HEADERSIZE for Sv2 datatypes is at maximum 3 bytes
                // When HEADERSIZE is 0 datatypes ISFIXED only exception is Bytes datatypes but is
                // not used
                _ => unreachable!(),
            };
            size.map(|x| x + HEADERSIZE)
        } else {
            Err(Error::ReadError(data.len(), HEADERSIZE))
        }
    }

    // Similar to the above but operates on a reader instead of a byte slice, reading
    // the header from the input and calculating the expected length of the data to be read.
    #[cfg(not(feature = "no_std"))]
    fn expected_length_for_reader(mut reader: impl Read) -> Result<usize, Error> {
        if ISFIXED {
            Ok(SIZE)
        } else {
            let mut header = [0_u8; HEADERSIZE];
            reader.read_exact(&mut header)?;
            let expected_length = match HEADERSIZE {
                1 => header[0] as usize,
                2 => u16::from_le_bytes([header[0], header[1]]) as usize,
                3 => u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize,
                // HEADERSIZE for Sv2 datatypes is at maximum 3 bytes
                // When HEADERSIZE is 0 datatypes ISFIXED only exception is Bytes datatypes but is
                // not used
                _ => unreachable!(),
            };
            if expected_length <= (MAXSIZE + HEADERSIZE) {
                Ok(expected_length)
            } else {
                Err(Error::ReadError(expected_length, MAXSIZE))
            }
        }
    }

    /// Returns the length of the data, either from the reference or the owned vector,
    /// or the fixed size if `ISFIXED` is true.
    pub fn len(&self) -> usize {
        match (self, ISFIXED) {
            (Inner::Ref(data), false) => data.len(),
            (Inner::Owned(data), false) => data.len(),
            (_, true) => 1,
        }
    }

    // Retrieves the header as a byte vector. If `HEADERSIZE` is zero, an empty vector is
    // returned. Otherwise, the header is constructed from the length of the data.
    fn get_header(&self) -> Vec<u8> {
        if HEADERSIZE == 0 {
            Vec::new()
        } else {
            let len = self.len();
            len.to_le_bytes().into()
        }
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&'a mut [u8]> for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &'a mut [u8]) -> Result<Self, Self::Error> {
        if ISFIXED && value.len() == SIZE {
            Ok(Self::Ref(value))
        } else if ISFIXED {
            Err(Error::ValueExceedsMaxSize(
                ISFIXED,
                SIZE,
                HEADERSIZE,
                MAXSIZE,
                value.to_vec(),
                value.len(),
            ))
        } else if value.len() <= MAXSIZE {
            Ok(Self::Ref(value))
        } else {
            Err(Error::ValueExceedsMaxSize(
                ISFIXED,
                SIZE,
                HEADERSIZE,
                MAXSIZE,
                value.to_vec(),
                value.len(),
            ))
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<Vec<u8>> for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if ISFIXED && value.len() == SIZE {
            Ok(Self::Owned(value))
        } else if ISFIXED {
            Err(Error::ValueExceedsMaxSize(
                ISFIXED,
                SIZE,
                HEADERSIZE,
                MAXSIZE,
                value.to_vec(),
                value.len(),
            ))
        } else if value.len() <= MAXSIZE {
            Ok(Self::Owned(value))
        } else {
            Err(Error::ValueExceedsMaxSize(
                ISFIXED,
                SIZE,
                HEADERSIZE,
                MAXSIZE,
                value.to_vec(),
                value.len(),
            ))
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> GetSize
    for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn get_size(&self) -> usize {
        match self {
            Inner::Ref(data) => data.len() + HEADERSIZE,
            Inner::Owned(data) => data.len() + HEADERSIZE,
        }
    }
}

impl<const ISFIXED: bool, const HEADERSIZE: usize, const SIZE: usize, const MAXSIZE: usize> SizeHint
    for Inner<'_, ISFIXED, HEADERSIZE, SIZE, MAXSIZE>
{
    fn size_hint(data: &[u8], offset: usize) -> Result<usize, Error> {
        if offset >= data.len() {
            return Err(Error::ReadError(data.len(), offset));
        }
        Self::expected_length(&data[offset..])
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        if offset >= data.len() {
            return Err(Error::ReadError(data.len(), offset));
        }
        Self::expected_length(&data[offset..])
    }
}
use crate::codec::decodable::FieldMarker;

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2DataType<'a> for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
where
    Self: TryInto<FieldMarker>,
{
    fn from_bytes_unchecked(data: &'a mut [u8]) -> Self {
        if ISFIXED {
            Self::Ref(data)
        } else {
            Self::Ref(&mut data[HEADERSIZE..])
        }
    }

    fn from_vec_(data: Vec<u8>) -> Result<Self, Error> {
        Self::size_hint(&data, 0)?;
        Ok(Self::Owned(data))
    }

    fn from_vec_unchecked(data: Vec<u8>) -> Self {
        Self::Owned(data)
    }

    #[cfg(not(feature = "no_std"))]
    fn from_reader_(mut reader: &mut impl Read) -> Result<Self, Error> {
        let size = Self::expected_length_for_reader(&mut reader)?;

        let mut dst = vec![0; size];

        reader.read_exact(&mut dst)?;
        Ok(Self::from_vec_unchecked(dst))
    }

    fn to_slice_unchecked(&'a self, dst: &mut [u8]) {
        let size = self.get_size();
        let header = self.get_header();
        dst[0..HEADERSIZE].copy_from_slice(&header[..HEADERSIZE]);
        match self {
            Inner::Ref(data) => {
                let dst = &mut dst[0..size];
                dst[HEADERSIZE..].copy_from_slice(data);
            }
            Inner::Owned(data) => {
                let dst = &mut dst[0..size];
                dst[HEADERSIZE..].copy_from_slice(data);
            }
        }
    }

    #[cfg(not(feature = "no_std"))]
    fn to_writer_(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            Inner::Ref(data) => {
                writer.write_all(data)?;
            }
            Inner::Owned(data) => {
                writer.write_all(data)?;
            }
        };
        Ok(())
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    IntoOwned for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn into_owned(self) -> Self {
        match self {
            Inner::Ref(data) => {
                let v: Vec<u8> = data.into();
                Self::Owned(v)
            }
            Inner::Owned(_) => self,
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    pub fn into_static(self) -> Inner<'static, ISFIXED, SIZE, HEADERSIZE, MAXSIZE> {
        match self {
            Inner::Ref(data) => {
                let mut v = Vec::with_capacity(data.len());
                v.extend_from_slice(data);
                Inner::Owned(v)
            }
            Inner::Owned(data) => Inner::Owned(data),
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> Clone
    for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn clone(&self) -> Inner<'static, ISFIXED, SIZE, HEADERSIZE, MAXSIZE> {
        match self {
            Inner::Ref(data) => {
                let mut v = Vec::with_capacity(data.len());
                v.extend_from_slice(data);
                Inner::Owned(v)
            }
            Inner::Owned(data) => Inner::Owned(data.clone()),
        }
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    AsRef<[u8]> for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn as_ref(&self) -> &[u8] {
        match self {
            Inner::Ref(r) => &r[..],
            Inner::Owned(r) => &r[..],
        }
    }
}
