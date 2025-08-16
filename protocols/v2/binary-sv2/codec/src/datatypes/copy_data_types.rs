// Provides implementations for encoding and decoding copy data types as required by the SV2
// protocol. Facilitates byte-level serialization and deserialization, particularly for types
// without dynamically-sized data.
//
// ## Traits and Implementations
//
// ### `Fixed`
// The `Fixed` trait is implemented for various data types to specify a fixed size for each,
// enabling consistent memory allocation during serialization. The `SIZE` constant for each type
// defines its byte size, with implementations provided for `bool`, unsigned integers (e.g., `u8`,
// `u16`, `u32`, `u64`), and custom types like `U24`.
//
// ### `Sv2DataType`
// The `Sv2DataType` trait is implemented for these data types, providing methods for encoding and
// decoding operations such as `from_bytes_unchecked`, `from_vec_`, `from_reader_` (if `std` is
// available), and `to_slice_unchecked`. The methods use little-endian byte order for consistency
// across platforms.
//
// ## Special Types
//
// ### `U24`
// A custom 24-bit unsigned integer, represented as a `U24` struct, handles 3-byte data often used
// in SV2 protocols for memory-efficient encoding. Provides conversion methods to and from `u32`,
// with `TryFrom<u32>` ensuring values stay within the 24-bit range (0 to 16,777,215).
//
// ## Macros
// The `impl_sv2_for_unsigned` macro streamlines the implementation of the `Sv2DataType` trait for
// unsigned integer types, ensuring little-endian byte ordering for serialization and handling both
// in-memory buffers and `std::io::Read`/`Write` interfaces when `std` is available.
use crate::{codec::Fixed, datatypes::Sv2DataType, Error};

use alloc::vec::Vec;
use core::convert::{TryFrom, TryInto};

#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Read, Write};

// Impl bool as a primitive

impl Fixed for bool {
    const SIZE: usize = 1;
}

impl<'a> Sv2DataType<'a> for bool {
    fn from_bytes_unchecked(data: &'a mut [u8]) -> Self {
        match data
            .first()
            .map(|x: &u8| x << 7)
            .map(|x: u8| x >> 7)
            // This is an unchecked function is fine to panic
            .expect("Try to decode a bool from a buffer of len 0")
        {
            0 => false,
            1 => true,
            // Below panic is impossible value is either 0 or 1
            _ => panic!(),
        }
    }

    fn from_vec_(mut data: Vec<u8>) -> Result<Self, Error> {
        Self::from_bytes_(&mut data)
    }

    fn from_vec_unchecked(mut data: Vec<u8>) -> Self {
        Self::from_bytes_unchecked(&mut data)
    }

    #[cfg(not(feature = "no_std"))]
    fn from_reader_(reader: &mut impl Read) -> Result<Self, Error> {
        let mut dst = [0_u8; Self::SIZE];
        reader.read_exact(&mut dst)?;
        Self::from_bytes_(&mut dst)
    }

    fn to_slice_unchecked(&'a self, dst: &mut [u8]) {
        match self {
            true => dst[0] = 1,
            false => dst[0] = 0,
        };
    }

    #[cfg(not(feature = "no_std"))]
    fn to_writer_(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            true => writer.write_all(&[1]),
            false => writer.write_all(&[0]),
        }
    }
}

// Impl unsigned as a primitives

impl Fixed for u8 {
    const SIZE: usize = 1;
}

impl Fixed for u16 {
    const SIZE: usize = 2;
}

impl Fixed for u32 {
    const SIZE: usize = 4;
}

impl Fixed for u64 {
    const SIZE: usize = 8;
}

/// Macro to implement the `Sv2DataType` trait for unsigned integer types.
///
/// Simplifies encoding and decoding for various unsigned integer types, making them
/// compatible with the SV2 protocol. Each implementation uses the little-endian byte order for
/// serialization and deserialization, ensuring consistency across platforms.
macro_rules! impl_sv2_for_unsigned {
    ($a:ty) => {
        impl<'a> Sv2DataType<'a> for $a {
            fn from_bytes_unchecked(data: &'a mut [u8]) -> Self {
                // unchecked function is fine to panic
                let a: &[u8; Self::SIZE] = data[0..Self::SIZE].try_into().expect(
                    "Try to decode a copy data type from a buffer that do not have enough bytes",
                );
                Self::from_le_bytes(*a)
            }

            fn from_vec_(mut data: Vec<u8>) -> Result<Self, Error> {
                Self::from_bytes_(&mut data)
            }

            fn from_vec_unchecked(mut data: Vec<u8>) -> Self {
                Self::from_bytes_unchecked(&mut data)
            }

            #[cfg(not(feature = "no_std"))]
            fn from_reader_(reader: &mut impl Read) -> Result<Self, Error> {
                let mut dst = [0_u8; Self::SIZE];
                reader.read_exact(&mut dst)?;
                Ok(Self::from_bytes_unchecked(&mut dst))
            }

            fn to_slice_unchecked(&'a self, dst: &mut [u8]) {
                let dst = &mut dst[0..Self::SIZE];
                let src = self.to_le_bytes();
                dst.copy_from_slice(&src);
            }

            #[cfg(not(feature = "no_std"))]
            fn to_writer_(&self, writer: &mut impl Write) -> Result<(), E> {
                let bytes = self.to_le_bytes();
                writer.write_all(&bytes)
            }
        }
    };
}
impl_sv2_for_unsigned!(u8);
impl_sv2_for_unsigned!(u16);
impl_sv2_for_unsigned!(u32);
impl_sv2_for_unsigned!(u64);

impl Fixed for f32 {
    const SIZE: usize = 4;
}

impl_sv2_for_unsigned!(f32);

/// Represents a 24-bit unsigned integer (`U24`), supporting SV2 serialization and deserialization.
/// Only first 3 bytes of a u32 is considered to get the SV2 value, and rest are ignored (in little
/// endian).

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct U24(pub(crate) u32);

impl Fixed for U24 {
    const SIZE: usize = 3;
}

impl U24 {
    fn from_le_bytes(b: [u8; Self::SIZE]) -> Self {
        let inner = u32::from_le_bytes([b[0], b[1], b[2], 0]);
        Self(inner)
    }

    fn to_le_bytes(self) -> [u8; Self::SIZE] {
        let b = self.0.to_le_bytes();
        [b[0], b[1], b[2]]
    }
}

impl_sv2_for_unsigned!(U24);

impl TryFrom<u32> for U24 {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value <= 16777215 {
            Ok(Self(value))
        } else {
            Err(Error::InvalidU24(value))
        }
    }
}

impl From<U24> for u32 {
    fn from(v: U24) -> Self {
        v.0
    }
}
