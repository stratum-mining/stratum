use crate::error::Error;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};
use std::convert::TryFrom;

#[derive(Debug, PartialEq)]
pub struct Signature<'u>(pub &'u [u8]);

impl<'u> TryFrom<&'u [u8]> for Signature<'u> {
    type Error = Error;

    #[inline]
    fn try_from(v: &'u [u8]) -> std::result::Result<Self, Error> {
        if v.len() == 64 {
            Ok(Self(v))
        } else {
            Err(Error::InvalidSignatureSize(v.len()))
        }
    }
}

impl<'u> From<Signature<'u>> for &'u [u8] {
    #[inline]
    fn from(v: Signature<'u>) -> Self {
        v.0
    }
}

impl<'u> Serialize for Signature<'u> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

struct SignatureVisitor;

impl<'a> Visitor<'a> for SignatureVisitor {
    type Value = Signature<'a>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a 64 bytes unsigned le int")
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, value: &'a [u8]) -> Result<Self::Value, E> {
        Ok(Signature(value))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Signature<'a> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("Signature", SignatureVisitor)
    }
}
