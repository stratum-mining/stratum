use crate::{error::Error, primitives::GetSize};
use alloc::vec::Vec;
use core::convert::{TryFrom, TryInto};
use serde::{de::Visitor, ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Eq)]
enum Inner<'a> {
    Ref(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> PartialEq for Inner<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Inner::Ref(slice), Inner::Ref(slice1)) => slice == slice1,
            (Inner::Ref(slice), Inner::Owned(inner)) => slice == &inner.as_slice(),
            (Inner::Owned(inner), Inner::Ref(slice)) => slice == &inner.as_slice(),
            (Inner::Owned(inner), Inner::Owned(inner1)) => inner == inner1,
        }
    }
}

impl<'a> Inner<'a> {
    #[inline]
    pub fn len(&self) -> [u8; 1] {
        let l = match self {
            Self::Ref(v) => v.len().to_le_bytes(),
            Self::Owned(v) => v.len().to_le_bytes(),
        };
        [l[0]]
    }

    #[inline]
    pub fn as_ref(&'a self) -> &'a [u8] {
        match self {
            Self::Ref(v) => v,
            Self::Owned(v) => &v[..],
        }
    }
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub struct B032<'b>(Inner<'b>);

impl<'b> TryFrom<&'b [u8]> for B032<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=32 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan32),
        }
    }
}

impl<'b> TryFrom<&'b mut [u8]> for B032<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b mut [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=32 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan32),
        }
    }
}

impl<'b> TryFrom<Vec<u8>> for B032<'b> {
    type Error = Error;

    fn try_from(v: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=32 => Ok(Self(Inner::Owned(v))),
            _ => Err(Error::LenBiggerThan32),
        }
    }
}

impl<'b> Serialize for B032<'b> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let len = self.0.len();
        let inner = self.0.as_ref();

        if serializer.is_human_readable() {
            serializer.serialize_bytes(inner)
        } else {
            // tuple is: (byte array len, byte array)
            let tuple = (len, &inner);

            let tuple_len = 2;
            let mut seq = serializer.serialize_tuple(tuple_len)?;

            seq.serialize_element(&tuple.0)?;
            seq.serialize_element(tuple.1)?;
            seq.end()
        }
    }
}

impl<'b> AsRef<[u8]> for B032<'b> {
    fn as_ref(&self) -> &[u8] {
        match &self.0 {
            Inner::Ref(v) => v,
            Inner::Owned(v) => v.as_ref(),
        }
    }
}

struct B032Visitor;

impl<'a> Visitor<'a> for B032Visitor {
    type Value = B032<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a byte array shorter than 32")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B032(Inner::Ref(value)))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let mut inner = vec![];
        while let Ok(Some(x)) = seq.next_element() {
            inner.push(x)
        }
        if inner.len() > 32 {
            return Err(serde::de::Error::custom(
                "Impossible deserialize B064K, len is bigger 32",
            ));
        }
        let inner = Inner::Owned(inner);
        Ok(B032(inner))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.is_empty() {
            return Err(serde::de::Error::custom("Impossible deserialize B032"));
        };
        let len = v[0];
        if len as usize == v.len() - 1 && len <= 32 {
            // Can not fail already checked len above
            let self_: B032 = v[1..].to_vec().try_into().unwrap();
            Ok(self_)
        } else {
            Err(serde::de::Error::custom("Impossible deserialize B032"))
        }
    }

    fn visit_bytes<E>(self, _v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        panic!();
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for B032<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer.deserialize_newtype_struct("B032", B032Visitor),
            true => deserializer.deserialize_byte_buf(B032Visitor),
        }
    }
}

impl<'a> GetSize for B032<'a> {
    fn get_size(&self) -> usize {
        match &self.0 {
            Inner::Ref(v) => v.len() + 1,
            Inner::Owned(v) => v.len() + 1,
        }
    }
}
impl<'a> B032<'a> {
    pub fn into_static(self) -> B032<'static> {
        match self.0 {
            Inner::Ref(slice) => B032(Inner::Owned(slice.to_vec())),
            Inner::Owned(inner) => B032(Inner::Owned(inner)),
        }
    }
    pub fn inner_as_ref(&self) -> &[u8] {
        match &self.0 {
            Inner::Ref(slice) => slice,
            Inner::Owned(inner) => inner.as_slice(),
        }
    }
    pub fn to_vec(&self) -> Vec<u8> {
        match &self.0 {
            Inner::Ref(slice) => slice.to_vec(),
            Inner::Owned(inner) => inner.to_vec(),
        }
    }
}
