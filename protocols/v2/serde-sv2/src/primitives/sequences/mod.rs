use super::{Signature, U24, U256};
use crate::Error;
use serde::{de::Visitor, Serialize};
use std::convert::TryInto;

pub mod seq0255;
pub mod seq064k;

#[derive(Debug, PartialEq)]
enum SeqMaxLen {
    _1B,
    _2B,
}

#[derive(Debug, PartialEq)]
struct Seq<'s, T: Serialize + TryFromBSlice<'s>> {
    data: &'s [u8],
    cursor: usize,
    size: u8,
    max_len: SeqMaxLen,
    _a: std::marker::PhantomData<T>,
}

impl<'a, T: TryFromBSlice<'a> + Serialize> Iterator for Seq<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let start = self.cursor;
        self.cursor += self.size as usize;
        let end = self.cursor;
        if end >= self.data.len() {
            None
        } else {
            // The below should be always valid as there is no way to construct invalid sequences
            // TODO check it
            match T::try_from_slice(&self.data[start..end]) {
                Ok(x) => Some(x),
                Err(_) => None,
            }
        }
    }
}

struct SeqVisitor<T> {
    inner_type_size: u8,
    max_len: SeqMaxLen,
    _a: std::marker::PhantomData<T>,
}

impl<'a, T: Serialize + TryFromBSlice<'a>> Visitor<'a> for SeqVisitor<T> {
    type Value = Seq<'a, T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        let max_len = match self.max_len {
            SeqMaxLen::_1B => "255",
            SeqMaxLen::_2B => "64K",
        };
        formatter.write_str(
            format!(
                "an array shorter than {} elements, with elements of {} bytes",
                max_len, self.inner_type_size
            )
            .as_str(),
        )
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(Seq {
            data: value,
            cursor: 0,
            max_len: self.max_len,
            size: self.inner_type_size,
            _a: std::marker::PhantomData,
        })
    }
}

pub trait TryFromBSlice<'a> {
    type Error;

    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error>
    where
        Self: std::marker::Sized;
}

impl<'a> TryFromBSlice<'a> for bool {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        if val.len() != 1 {
            return Err(Error::InvalidBoolSize(val.len()));
        }
        match val[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidBool(val[0])),
        }
    }
}

impl<'a> TryFromBSlice<'a> for u16 {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        if val.len() != 2 {
            return Err(Error::InvalidU16Size(val.len()));
        }
        Ok(u16::from_le_bytes([val[0], val[1]]))
    }
}

impl<'a> TryFromBSlice<'a> for U24 {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        if val.len() != 3 {
            return Err(Error::InvalidU24Size(val.len()));
        }
        Ok(U24(u32::from_le_bytes([val[0], val[1], val[2], 0])))
    }
}

impl<'a> TryFromBSlice<'a> for u32 {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        if val.len() != 4 {
            return Err(Error::InvalidU32Size(val.len()));
        }
        Ok(u32::from_le_bytes([val[0], val[1], val[2], val[3]]))
    }
}

impl<'a> TryFromBSlice<'a> for U256<'a> {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        val.try_into()
    }
}

impl<'a> TryFromBSlice<'a> for Signature<'a> {
    type Error = Error;

    #[inline]
    fn try_from_slice(val: &'a [u8]) -> Result<Self, Error> {
        val.try_into()
    }
}
