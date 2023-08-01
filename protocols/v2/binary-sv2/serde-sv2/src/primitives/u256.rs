use crate::{error::Error, primitives::FixedSize};
use alloc::{boxed::Box, vec::Vec};
use core::convert::TryFrom;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Eq)]
enum Inner<'a> {
    Ref(&'a [u8]),
    Owned(Box<[u8; 32]>),
}

impl<'a> PartialEq for Inner<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Inner::Ref(slice), Inner::Ref(slice1)) => slice == slice1,
            (Inner::Ref(slice), Inner::Owned(inner)) => slice == &(*inner).as_slice(),
            (Inner::Owned(inner), Inner::Ref(slice)) => slice == &(*inner).as_slice(),
            (Inner::Owned(inner), Inner::Owned(inner1)) => inner == inner1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct U256<'u>(Inner<'u>);

impl<'u> TryFrom<&'u [u8]> for U256<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 32 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidU256(v.len()))
        }
    }
}

impl<'u> TryFrom<&'u mut [u8]> for U256<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u mut [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 32 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidU256(v.len()))
        }
    }
}

impl<'u> From<[u8; 32]> for U256<'u> {
    fn from(v: [u8; 32]) -> Self {
        U256(Inner::Owned(Box::new(v)))
    }
}

impl<'u> From<&'u U256<'u>> for &'u [u8] {
    #[inline]
    fn from(v: &'u U256<'u>) -> Self {
        match &v.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => &v[..],
        }
    }
}

impl<'u> Serialize for U256<'u> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(self.into())
    }
}

struct U256Visitor;

impl<'a> Visitor<'a> for U256Visitor {
    type Value = U256<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a 32 bytes unsigned le int")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(U256(Inner::Ref(value)))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() >= 32 {
            let v = v[..32].to_vec();
            Ok(U256(Inner::Owned(Box::new(v.try_into().unwrap()))))
        } else {
            Err(serde::de::Error::custom("Impossible deserialize U256"))
        }
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let mut buffer: Vec<u8> = vec![];
        let mut i = 0;
        while let Some(value) = seq.next_element()? {
            i += 1;
            buffer.push(value);
            if i == 32 {
                break;
            }
        }
        if i < 32 {
            Err(serde::de::Error::custom(
                "Impossible deserialize U256 len < than 32",
            ))
        } else {
            self.visit_byte_buf(buffer)
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for U256<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer.deserialize_newtype_struct("U256", U256Visitor),
            true => deserializer.deserialize_byte_buf(U256Visitor),
        }
    }
}

impl<'a> FixedSize for U256<'a> {
    const FIXED_SIZE: usize = 32;
}
use core::convert::TryInto;
impl<'a> U256<'a> {
    pub fn into_static(self) -> U256<'static> {
        match self.0 {
            Inner::Ref(inner) => U256(Inner::Owned(Box::new(inner.try_into().unwrap()))),
            Inner::Owned(inner) => U256(Inner::Owned(inner)),
        }
    }
    pub fn inner_as_ref(&self) -> &[u8] {
        match &self.0 {
            Inner::Ref(slice) => slice,
            Inner::Owned(inner) => inner.as_ref(),
        }
    }
    pub fn to_vec(&self) -> alloc::vec::Vec<u8> {
        match &self.0 {
            Inner::Ref(slice) => slice.to_vec(),
            Inner::Owned(inner) => inner.to_vec(),
        }
    }
}

impl<'a> TryFrom<alloc::vec::Vec<u8>> for U256<'a> {
    type Error = ();

    fn try_from(value: alloc::vec::Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() == 32 {
            let inner: [u8; 32] = value.try_into().unwrap();
            Ok(Self(Inner::Owned(Box::new(inner))))
        } else {
            Err(())
        }
    }
}
impl<'a> AsRef<[u8]> for U256<'a> {
    fn as_ref(&self) -> &[u8] {
        match &self.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => v.as_ref(),
        }
    }
}
