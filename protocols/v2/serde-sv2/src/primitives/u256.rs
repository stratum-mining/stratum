use crate::error::Error;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};
use std::convert::TryFrom;

#[derive(Debug, PartialEq)]
pub struct U256<'u>(&'u [u8]);

impl<'u> TryFrom<&'u [u8]> for U256<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u [u8]) -> std::result::Result<Self, Error> {
        if v.len() == 32 {
            Ok(Self(v))
        } else {
            Err(Error::InvalidU256(v.len()))
        }
    }
}

impl<'u> From<U256<'u>> for &'u [u8] {
    #[inline]
    fn from(v: U256<'u>) -> Self {
        v.0
    }
}

impl<'u> Serialize for U256<'u> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

struct U256Visitor;

impl<'a> Visitor<'a> for U256Visitor {
    type Value = U256<'a>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a 32 bytes unsigned le int")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(U256(value))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for U256<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("U256", U256Visitor)
    }
}
