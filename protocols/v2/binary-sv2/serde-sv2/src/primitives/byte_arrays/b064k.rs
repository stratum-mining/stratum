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
    pub fn len(&self) -> [u8; 2] {
        let l = match self {
            Self::Ref(v) => v.len().to_le_bytes(),
            Self::Owned(v) => v.len().to_le_bytes(),
        };
        [l[0], l[1]]
    }

    #[inline]
    pub fn as_ref(&'a self) -> &'a [u8] {
        match self {
            Self::Ref(v) => v,
            Self::Owned(v) => &v[..],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct B064K<'b>(Inner<'b>);

impl<'b> TryFrom<&'b [u8]> for B064K<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=65535 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> TryFrom<&'b mut [u8]> for B064K<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b mut [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=65535 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> TryFrom<Vec<u8>> for B064K<'b> {
    type Error = Error;

    fn try_from(v: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=65535 => Ok(Self(Inner::Owned(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> Serialize for B064K<'b> {
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

struct B064KVisitor;

impl<'a> Visitor<'a> for B064KVisitor {
    type Value = B064K<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a byte array shorter than 64K")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B064K(Inner::Ref(value)))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let mut inner = vec![];
        while let Ok(Some(x)) = seq.next_element() {
            inner.push(x)
        }
        if inner.len() > u16::MAX as usize {
            return Err(serde::de::Error::custom(
                "Impossible deserialize B064K, len is bigger u16::MAX",
            ));
        }
        let inner = Inner::Owned(inner);
        Ok(B064K(inner))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() < 2 {
            return Err(serde::de::Error::custom("Impossible deserialize B064K"));
        };
        let len = u16::from_le_bytes([v[0], v[1]]);
        if len as usize == v.len() - 2 {
            // Can not fail already checked len above
            let self_: B064K = v[2..].to_vec().try_into().unwrap();
            Ok(self_)
        } else {
            Err(serde::de::Error::custom("Impossible deserialize B064K"))
        }
    }

    fn visit_bytes<E>(self, _v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        panic!();
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for B064K<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer.deserialize_newtype_struct("B064K", B064KVisitor),
            true => deserializer.deserialize_byte_buf(B064KVisitor),
        }
    }
}

impl<'a> GetSize for B064K<'a> {
    fn get_size(&self) -> usize {
        match &self.0 {
            Inner::Ref(v) => v.len() + 2,
            Inner::Owned(v) => v.len() + 2,
        }
    }
}
impl<'a> B064K<'a> {
    pub fn into_static(self) -> B064K<'static> {
        match self.0 {
            Inner::Ref(slice) => B064K(Inner::Owned(slice.to_vec())),
            Inner::Owned(inner) => B064K(Inner::Owned(inner)),
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
impl<'a> AsRef<[u8]> for B064K<'a> {
    fn as_ref(&self) -> &[u8] {
        todo!()
    }
}
