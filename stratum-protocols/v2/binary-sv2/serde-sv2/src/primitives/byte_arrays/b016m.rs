use crate::{error::Error, primitives::GetSize};
use alloc::vec::Vec;
use core::convert::{TryFrom, TryInto};
use serde::{de::Visitor, ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone)]
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
    pub fn len(&self) -> [u8; 3] {
        let l = match self {
            Self::Ref(v) => v.len().to_le_bytes(),
            Self::Owned(v) => v.len().to_le_bytes(),
        };
        [l[0], l[1], l[2]]
    }

    #[inline]
    pub fn as_ref(&'a self) -> &'a [u8] {
        match self {
            Self::Ref(v) => v,
            Self::Owned(v) => &v[..],
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct B016M<'b>(Inner<'b>);

impl<'b> TryFrom<&'b [u8]> for B016M<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=16777215 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}
impl<'b> TryFrom<&'b mut [u8]> for B016M<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b mut [u8]) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=16777215 => Ok(Self(Inner::Ref(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> TryFrom<Vec<u8>> for B016M<'b> {
    type Error = Error;

    fn try_from(v: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        match v.len() {
            0..=16777215 => Ok(Self(Inner::Owned(v))),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> Serialize for B016M<'b> {
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

struct B016MVisitor;

impl<'a> Visitor<'a> for B016MVisitor {
    type Value = B016M<'a>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a byte array shorter than 16M")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let mut inner = vec![];
        while let Ok(Some(x)) = seq.next_element() {
            inner.push(x)
        }
        if inner.len() > 16777216 {
            return Err(serde::de::Error::custom(
                "Impossible deserialize B016M, len is bigger 16777216",
            ));
        }
        let inner = Inner::Owned(inner);
        Ok(B016M(inner))
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B016M(Inner::Ref(value)))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() < 3 {
            return Err(serde::de::Error::custom("Impossible deserialize B016M"));
        };
        let len = u32::from_le_bytes([v[0], v[1], v[2], 0]);
        if len as usize == v.len() - 3 && len <= 16777216 {
            // Can not fail already checked len above
            let self_: B016M = v[3..].to_vec().try_into().unwrap();
            Ok(self_)
        } else {
            Err(serde::de::Error::custom("Impossible deserialize B016M"))
        }
    }

    fn visit_bytes<E>(self, _v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        panic!();
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for B016M<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer.deserialize_newtype_struct("B016M", B016MVisitor),
            true => deserializer.deserialize_byte_buf(B016MVisitor),
        }
    }
}

impl<'a> GetSize for B016M<'a> {
    fn get_size(&self) -> usize {
        match &self.0 {
            Inner::Ref(v) => v.len() + 3,
            Inner::Owned(v) => v.len() + 3,
        }
    }
}

impl<'a> B016M<'a> {
    pub fn get_elements_number_in_array(a: &[u8]) -> usize {
        let total_len = a.len();
        let mut next_element_index: usize = 0;
        let mut elements_number: usize = 0;
        while next_element_index < total_len {
            let len = &a[next_element_index..next_element_index + 3];
            let len = u32::from_le_bytes([len[0], len[1], len[2], 0]);
            next_element_index += len as usize + 3;
            elements_number += 1;
        }
        elements_number
    }
}
impl<'a> B016M<'a> {
    pub fn into_static(self) -> B016M<'static> {
        match self.0 {
            Inner::Ref(slice) => B016M(Inner::Owned(slice.to_vec())),
            Inner::Owned(inner) => B016M(Inner::Owned(inner)),
        }
    }
    pub fn to_vec(self) -> Vec<u8> {
        match self.0 {
            Inner::Ref(v) => v.to_vec(),
            Inner::Owned(_) => todo!(),
        }
    }
}
