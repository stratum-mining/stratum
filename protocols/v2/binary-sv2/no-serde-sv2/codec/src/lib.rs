//! ```txt
//! SERDE    <-> Sv2
//! bool     <-> BOOL
//! u8       <-> U8
//! u16      <-> U16
//! U24      <-> U24
//! u32      <-> u32
//! u64      <-> u64 // TODO not in the spec but used
//! U256     <-> U256
//! String   <-> STRO_255
//! Signature<-> SIGNATURE
//! B0255    <-> B0_255
//! B064K    <-> B0_64K
//! B016M    <-> B0_16M
//! [u8]     <-> BYTES
//! Pubkey   <-> PUBKEY
//! Seq0255  <-> SEQ0_255[T]
//! Seq064K  <-> SEQ0_64K[T]
//! ```
#![cfg_attr(feature = "no_std", no_std)]
use core::convert::TryInto;

#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, ErrorKind};

mod codec;
mod datatypes;
pub use datatypes::{
    Bytes, PubKey, Seq0255, Seq064K, Signature, Str0255, B016M, B0255, B064K, U24, U256,
};

pub use crate::codec::decodable::Decodable;
pub use crate::codec::encodable::{Encodable, EncodableField};
pub use crate::codec::GetSize;
pub use crate::codec::SizeHint;

pub fn to_bytes<'a, T: Encodable + GetSize>(src: T) -> Result<Vec<u8>, Error> {
    let mut result = vec![0_u8; src.get_size()];
    src.to_bytes(&mut result)?;
    Ok(result)
}

pub fn to_writer<T: Encodable>(src: T, dst: &mut [u8]) -> Result<(), Error> {
    src.to_bytes(dst)?;
    Ok(())
}

pub fn from_bytes<'a, T: Decodable<'a>>(data: &'a mut [u8]) -> Result<T, Error> {
    T::from_bytes(data)
}

pub mod decodable {
    pub use crate::codec::decodable::Decodable;
    pub use crate::codec::decodable::DecodableField;
    pub use crate::codec::decodable::FieldMarker;
    //pub use crate::codec::decodable::PrimitiveMarker;
}

pub mod encodable {
    pub use crate::codec::encodable::Encodable;
    pub use crate::codec::encodable::EncodableField;
}

#[macro_use]
extern crate alloc;

#[derive(Debug)]
pub enum Error {
    OutOfBound,
    NotABool(u8),
    /// -> (expected size, actual size)
    WriteError(usize, usize),
    U24TooBig(u32),
    InvalidSignatureSize(usize),
    InvalidU256(usize),
    InvalidU24(u32),
    InvalidB0255Size(usize),
    InvalidB064KSize(usize),
    InvalidB016MSize(usize),
    InvalidSeq0255Size(usize),
    PrimitiveConversionError,
    DecodableConversionError,
    UnInitializedDecoder,
    #[cfg(not(feature = "no_std"))]
    IoError(E),
    ReadError(usize, usize),
    Todo,
}

#[cfg(not(feature = "no_std"))]
impl From<E> for Error {
    fn from(v: E) -> Self {
        match v.kind() {
            ErrorKind::UnexpectedEof => Error::OutOfBound,
            _ => Error::IoError(v),
        }
    }
}

/// Vec<u8> is used as the Sv2 type Bytes
impl GetSize for Vec<u8> {
    fn get_size(&self) -> usize {
        self.len()
    }
}

impl<'a> From<Vec<u8>> for EncodableField<'a> {
    fn from(v: Vec<u8>) -> Self {
        let bytes: Bytes = v.try_into().unwrap();
        crate::encodable::EncodableField::Primitive(
            crate::codec::encodable::EncodablePrimitive::Bytes(bytes),
        )
    }
}
