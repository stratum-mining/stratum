use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};

#[derive(Debug, PartialEq)]
pub struct U24(pub u32);

impl From<u32> for U24 {
    #[inline]
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<U24> for u32 {
    #[inline]
    fn from(v: U24) -> Self {
        v.0
    }
}

impl Serialize for U24 {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0.to_le_bytes()[0..=2])
    }
}

struct U24Visitor;

impl<'de> Visitor<'de> for U24Visitor {
    type Value = U24;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an integer between 0 and 2^24 3 bytes le")
    }

    #[inline]
    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E> {
        Ok(value.into())
    }
}

impl<'de> Deserialize<'de> for U24 {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("U24", U24Visitor)
    }
}
