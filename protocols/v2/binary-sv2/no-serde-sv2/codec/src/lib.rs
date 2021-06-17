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

#[allow(clippy::wrong_self_convention)]
pub fn to_bytes<T: Encodable + GetSize>(src: T) -> Result<Vec<u8>, Error> {
    let mut result = vec![0_u8; src.get_size()];
    src.to_bytes(&mut result)?;
    Ok(result)
}

#[allow(clippy::wrong_self_convention)]
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

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CVec {
    data: *mut u8,
    len: usize,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CVec2 {
    data: *mut CVec,
    len: usize,
}

#[no_mangle]
pub extern "C" fn free_vec(buf: &mut CVec) {
    let s = unsafe { std::slice::from_raw_parts_mut(buf.data, buf.len) };
    let s = s.as_mut_ptr();
    unsafe {
        alloc::boxed::Box::from_raw(s);
    }
}

#[no_mangle]
pub extern "C" fn free_vec_2(buf: &mut CVec2) {
    let s_ = unsafe { std::slice::from_raw_parts_mut(buf.data, buf.len) };
    let s = s_.as_mut_ptr();
    unsafe {
        alloc::boxed::Box::from_raw(s);
    };
    for s in s_ {
        free_vec(s)
    }
}

impl<'a, const A: bool, const B: usize, const C: usize, const D: usize>
    From<datatypes::Inner<'a, A, B, C, D>> for CVec
{
    fn from(v: datatypes::Inner<'a, A, B, C, D>) -> Self {
        let (ptr, len) = match v {
            datatypes::Inner::Ref(inner) => {
                (inner.as_mut_ptr(), inner.len())
                //core::mem::forget(inner);
            }
            datatypes::Inner::Owned(mut inner) => {
                (inner.as_mut_ptr(), inner.len())
                //core::mem::forget(inner);
            }
        };
        Self { data: ptr, len }
    }
}

impl<'a, T: Into<CVec>> From<Seq0255<'a, T>> for CVec2 {
    fn from(v: Seq0255<'a, T>) -> Self {
        let mut v: Vec<CVec> = v.0.into_iter().map(|x| x.into()).collect();
        let data = v.as_mut_ptr();
        let len = v.len();
        std::mem::forget(v);
        Self { data, len }
    }
}
impl<'a, T: Into<CVec>> From<Seq064K<'a, T>> for CVec2 {
    fn from(v: Seq064K<'a, T>) -> Self {
        let mut v: Vec<CVec> = v.0.into_iter().map(|x| x.into()).collect();
        let data = v.as_mut_ptr();
        let len = v.len();
        std::mem::forget(v);
        Self { data, len }
    }
}

impl From<&mut [u8]> for CVec {
    fn from(v: &mut [u8]) -> Self {
        let (data, len) = (v.as_mut_ptr(), v.len());
        //core::mem::forget(v);
        Self { data, len }
    }
}

#[no_mangle]
pub extern "C" fn _c_export_u24(_a: U24) {}
