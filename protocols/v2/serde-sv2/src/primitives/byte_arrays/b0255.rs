use crate::error::Error;
use serde::{de::Visitor, ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};
use std::convert::TryFrom;

#[derive(Debug, PartialEq)]
pub struct B0255<'b>(&'b [u8]);

impl<'b> TryFrom<&'b [u8]> for B0255<'b> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'b [u8]) -> std::result::Result<Self, Self::Error> {
        match v.len() {
            0..=255 => Ok(Self(v)),
            _ => Err(Error::LenBiggerThan255),
        }
    }
}

impl<'b> Serialize for B0255<'b> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        // tuple is: (byte array len, byte array)
        let tuple = (&self.0.len().to_le_bytes()[0], &self.0[..]);

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

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte array shorter than 255")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(B0255(value))
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
