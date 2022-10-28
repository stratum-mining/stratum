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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct B0255<'b>(Inner<'b>);

impl<'b> TryFrom<&'b [u8]> for B0255<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=255 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan255),
        }
    }
}

impl<'b> TryFrom<&'b mut [u8]> for B0255<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b mut [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=255 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan255),
        }
    }
}

impl<'b> TryFrom<Vec<u8>> for B0255<'b> {
    type Error = Error;

    fn try_from(v: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=255 => Ok(Self(Inner::Owned(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> Serialize for B0255<'b> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let len = self.0.len();
        let inner = self.0.as_ref();

        // tuple is: (byte array len, byte array)
        let tuple = (len, &inner);

        let tuple_len = 2;
        let mut seq = serializer.serialize_tuple(tuple_len)?;

        seq.serialize_element(&tuple.0)?;
        seq.serialize_element(tuple.1)?;
        seq.end()
    }
}

struct B0255Visitor;

impl<'a> Visitor<'a> for B0255Visitor {
    type Value = B0255<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a byte array shorter than 255")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B0255(Inner::Ref(value)))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for B0255<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("B0255", B0255Visitor)
    }
}

impl<'a> GetSize for B0255<'a> {
    fn get_size(&self) -> usize {
        match &self.0 {
            Inner::Ref(v) => v.len() + 1,
            Inner::Owned(v) => v.len() + 1,
        }
    }
}
impl<'a> B0255<'a> {
    pub fn into_static(self) -> B0255<'static> {
        match self.0 {
            Inner::Ref(slice) => B0255(Inner::Owned(slice.to_vec())),
            Inner::Owned(inner) => B0255(Inner::Owned(inner)),
        }
    }
    pub fn to_vec(&self) -> Vec<u8> {
        match &self.0 {
            Inner::Ref(slice) => slice.to_vec(),
            Inner::Owned(inner) => inner.to_vec(),
        }
    }
}

impl<'a> TryFrom<alloc::string::String> for B0255<'a> {
    type Error = crate::Error;

    fn try_from(value: alloc::string::String) -> Result<Self, Self::Error> {
        value.into_bytes().try_into()
    }
}
