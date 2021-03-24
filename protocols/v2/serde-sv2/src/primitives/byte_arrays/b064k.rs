use crate::error::Error;
use serde::{de::Visitor, ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};
use std::convert::TryFrom;

#[derive(Debug, PartialEq)]
pub struct B064K<'b>(&'b [u8]);

impl<'b> TryFrom<&'b [u8]> for B064K<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> std::result::Result<Self, Self::Error> {
        match v.len() {
            0..=65535 => Ok(Self(v)),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl<'b> Serialize for B064K<'b> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        // tuple is: (byte array len, byte array)
        let tuple = (&self.0.len().to_le_bytes()[0..=1], &self.0[..]);

        let tuple_len = 2;
        let mut seq = serializer.serialize_tuple(tuple_len)?;

        seq.serialize_element(&tuple.0)?;
        seq.serialize_element(tuple.1)?;
        seq.end()
    }
}

struct B064KVisitor;

impl<'a> Visitor<'a> for B064KVisitor {
    type Value = B064K<'a>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte array shorter than 64K")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B064K(value))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for B064K<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("B064K", B064KVisitor)
    }
}
