use crate::primitives::GetLen;
use serde::{de::Visitor, ser, Deserialize, Deserializer, Serialize};

#[derive(Debug, PartialEq)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl Serialize for Bytes {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let inner = self.0.as_ref();

        serializer.serialize_bytes(inner)
    }
}

struct BytesVisitor;

impl Visitor<'_> for BytesVisitor {
    type Value = Bytes;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte array with no encoded length")
    }

    #[inline]
    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Bytes(value))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(BytesVisitor)
    }
}

impl GetLen for Bytes {
    fn get_len(&self) -> usize {
        self.0.len()
    }
}

impl AsMut<[u8]> for Bytes {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(v: Bytes) -> Self {
        v.0
    }
}
