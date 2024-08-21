use crate::{error::Error, primitives::FixedSize};
use alloc::{boxed::Box, vec::Vec};
use core::convert::TryFrom;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Eq)]
enum Inner<'a> {
    Ref(&'a [u8]),
    Owned(Box<[u8; 6]>),
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
pub struct ShortTxId<'u>(Inner<'u>);

impl<'u> TryFrom<&'u [u8]> for ShortTxId<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 6 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidShortTxId(v.len()))
        }
    }
}

impl<'u> TryFrom<&'u mut [u8]> for ShortTxId<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u mut [u8]) -> core::result::Result<Self, Error> {
        if v.len() == 6 {
            Ok(Self(Inner::Ref(v)))
        } else {
            Err(Error::InvalidShortTxId(v.len()))
        }
    }
}

impl<'u> From<[u8; 6]> for ShortTxId<'u> {
    fn from(v: [u8; 6]) -> Self {
        ShortTxId(Inner::Owned(Box::new(v)))
    }
}

impl<'u> From<&'u ShortTxId<'u>> for &'u [u8] {
    #[inline]
    fn from(v: &'u ShortTxId<'u>) -> Self {
        match &v.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => &v[..],
        }
    }
}

impl<'u> Serialize for ShortTxId<'u> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(self.into())
    }
}

struct ShortTxIdVisitor;

impl<'a> Visitor<'a> for ShortTxIdVisitor {
    type Value = ShortTxId<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a 6 bytes unsigned le int")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(ShortTxId(Inner::Ref(value)))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() >= 6 {
            let v = v[..6].to_vec();
            // Safe unwrap v is a 6 bytes slice can not panic
            Ok(ShortTxId(Inner::Owned(Box::new(v.try_into().unwrap()))))
        } else {
            Err(serde::de::Error::custom("Impossible deserialize ShortTxId"))
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
            if i == 6 {
                break;
            }
        }
        if i < 6 {
            Err(serde::de::Error::custom(
                "Impossible deserialize ShortTxId len < than 6",
            ))
        } else {
            self.visit_byte_buf(buffer)
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for ShortTxId<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer.deserialize_newtype_struct("ShortTxId", ShortTxIdVisitor),
            true => deserializer.deserialize_byte_buf(ShortTxIdVisitor),
        }
    }
}

impl<'a> FixedSize for ShortTxId<'a> {
    const FIXED_SIZE: usize = 6;
}
use core::convert::TryInto;
impl<'a> ShortTxId<'a> {
    pub fn into_static(self) -> ShortTxId<'static> {
        match self.0 {
            // Safe unwrap, is impossible to construct a ShortTxId::Ref that contain a slice with
            // len different then 6
            Inner::Ref(inner) => ShortTxId(Inner::Owned(Box::new(inner.try_into().unwrap()))),
            Inner::Owned(inner) => ShortTxId(Inner::Owned(inner)),
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

impl<'a> TryFrom<alloc::vec::Vec<u8>> for ShortTxId<'a> {
    type Error = ();

    fn try_from(value: alloc::vec::Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() == 6 {
            // Safe unwrap value has len equal to 6 below can not panic
            let inner: [u8; 6] = value.try_into().unwrap();
            Ok(Self(Inner::Owned(Box::new(inner))))
        } else {
            Err(())
        }
    }
}
impl<'a> AsRef<[u8]> for ShortTxId<'a> {
    fn as_ref(&self) -> &[u8] {
        match &self.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => v.as_ref(),
        }
    }
}
