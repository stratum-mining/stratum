//! Stratum v2 primitives: as defined [here][Sv2]
//!
//! [Sv2]: https://docs.google.com/document/d/1FadCWj-57dvhxsnFM_7X806qyvhR0u3i85607bGHxvg/edit
//!
use crate::error::Error;
use serde::{de::Visitor, ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};
use std::convert::{TryFrom, TryInto};

#[derive(Debug, PartialEq)]
pub struct U24(u32);

impl From<u32> for U24 {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<U24> for u32 {
    fn from(v: U24) -> Self {
        v.0
    }
}

impl Serialize for U24 {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0.to_le_bytes()[0..=3])
    }
}

struct U24Visitor;

impl<'de> Visitor<'de> for U24Visitor {
    type Value = U24;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an integer between 0 and 2^24 3 bytes le")
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E> {
        Ok(value.into())
    }
}

impl<'de> Deserialize<'de> for U24 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("U24", U24Visitor)
    }
}

#[derive(Debug, PartialEq)]
pub struct U256([u8; 32]);
pub type Pubkey = U256;

impl From<[u8; 32]> for U256 {
    fn from(v: [u8; 32]) -> Self {
        Self(v)
    }
}

impl From<U256> for [u8; 32] {
    fn from(v: U256) -> Self {
        v.0
    }
}

impl Serialize for U256 {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

struct U256Visitor;

impl<'de> Visitor<'de> for U256Visitor {
    type Value = U256;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a 32 bytes unsigned le int")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        let u256: [u8; 32] = value.try_into().unwrap();
        Ok(u256.into())
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("U256", U256Visitor)
    }
}

#[derive(Debug, PartialEq)]
pub struct Signature([u8; 64]);

impl From<[u8; 64]> for Signature {
    fn from(v: [u8; 64]) -> Self {
        Self(v)
    }
}

impl From<Signature> for [u8; 64] {
    fn from(v: Signature) -> Self {
        v.0
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

struct SignatureVisitor;

impl<'de> Visitor<'de> for SignatureVisitor {
    type Value = Signature;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a 64 bytes unsigned le int")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        let u256: [u8; 64] = value.try_into().unwrap();
        Ok(u256.into())
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("Signature", SignatureVisitor)
    }
}

#[derive(Debug, PartialEq)]
pub struct B016M(Vec<u8>);

impl TryFrom<Vec<u8>> for B016M {
    type Error = Error;

    fn try_from(v: Vec<u8>) -> std::result::Result<Self, Self::Error> {
        match v.len() {
            0..=16777215 => Ok(Self(v)),
            _ => Err(Error::LenBiggerThan16M),
        }
    }
}

impl From<B016M> for Vec<u8> {
    fn from(v: B016M) -> Self {
        v.0
    }
}

impl Serialize for B016M {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let tuple = (&self.0.len().to_le_bytes()[0..=2], &self.0[..]);
        let mut seq = serializer.serialize_tuple(2)?;
        seq.serialize_element(&tuple.0)?;
        seq.serialize_element(tuple.1)?;
        seq.end()
    }
}

struct B016MVisitor;

impl<'de> Visitor<'de> for B016MVisitor {
    type Value = B016M;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte array shorter than 16M")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        let b0: Vec<u8> = value.into();
        Ok(b0.try_into().unwrap())
    }
}

impl<'de> Deserialize<'de> for B016M {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct("B016M", B016MVisitor)
    }
}

#[derive(Debug, PartialEq)]
pub struct Seq0255<T: Serialize>(Vec<T>);
pub type B0255 = Seq0255<u8>;

impl<T: Serialize> TryFrom<Vec<T>> for Seq0255<T> {
    type Error = Error;

    fn try_from(v: Vec<T>) -> std::result::Result<Self, Self::Error> {
        match v.len() {
            0..=255 => Ok(Self(v)),
            _ => Err(Error::LenBiggerThan255),
        }
    }
}

impl<T: Serialize> From<Seq0255<T>> for Vec<T> {
    fn from(v: Seq0255<T>) -> Self {
        v.0
    }
}

impl<T: Serialize> Serialize for Seq0255<T> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let tuple = (self.0.len() as u8, &self.0[..]);
        let mut seq = serializer.serialize_tuple(2)?;
        seq.serialize_element(&tuple.0)?;
        seq.serialize_element(tuple.1)?;
        seq.end()
    }
}

struct Seq0255Visitor<T> {
    _a: std::marker::PhantomData<T>,
}

impl<'de, T: Serialize + Deserialize<'de>> Visitor<'de> for Seq0255Visitor<T> {
    type Value = Seq0255<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a general array shorter than 255")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut access: A,
    ) -> Result<Self::Value, A::Error> {
        let mut s: Vec<T> = vec![];
        let mut i = 0;
        while let Some(value) = access.next_element()? {
            // TODO
            // if i > 255 {
            //     return Err(Error::LenBiggerThan255)
            // }
            if i > 255 {
                panic!()
            }
            s.push(value);
            i += 1;
        }
        Ok(s.try_into().unwrap())
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Deserialize<'de> for Seq0255<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(
            "Seq0255",
            Seq0255Visitor {
                _a: std::marker::PhantomData,
            },
        )
    }
}

#[derive(Debug, PartialEq)]
pub struct Seq064K<T: Serialize>(Vec<T>);
pub type B064K = Seq064K<u8>;

impl<T: Serialize> TryFrom<Vec<T>> for Seq064K<T> {
    type Error = Error;

    fn try_from(v: Vec<T>) -> std::result::Result<Self, Self::Error> {
        match v.len() {
            0..=65535 => Ok(Self(v)),
            _ => Err(Error::LenBiggerThan64K),
        }
    }
}

impl<T: Serialize> From<Seq064K<T>> for Vec<T> {
    fn from(v: Seq064K<T>) -> Self {
        v.0
    }
}

impl<T: Serialize> Serialize for Seq064K<T> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let tuple = (&self.0.len().to_le_bytes()[0..=1], &self.0[..]);
        let mut seq = serializer.serialize_tuple(2)?;
        seq.serialize_element(&tuple.0)?;
        seq.serialize_element(tuple.1)?;
        seq.end()
    }
}

struct Seq064KVisitor<T> {
    _a: std::marker::PhantomData<T>,
}

impl<'de, T: Serialize + Deserialize<'de>> Visitor<'de> for Seq064KVisitor<T> {
    type Value = Seq064K<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a general array shorter than 64K")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut access: A,
    ) -> Result<Self::Value, A::Error> {
        let mut s: Vec<T> = vec![];
        let mut i = 0;
        while let Some(value) = access.next_element()? {
            // TODO
            // if i > 255 {
            //     return Err(Error::LenBiggerThan255)
            // }
            if i > (2_u32.pow(16)) - 1 {
                panic!()
            }
            s.push(value);
            i += 1;
        }
        Ok(s.try_into().unwrap())
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Deserialize<'de> for Seq064K<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(
            "Seq064K",
            Seq064KVisitor {
                _a: std::marker::PhantomData,
            },
        )
    }
}

pub type Bool = bool;
pub type U8 = u8;
pub type U16 = u16;
pub type U32 = u32;
pub type Bytes = [u8];
// TODO rust string are valid UTF-8 Sv2 string (STR0255) are raw bytes. So there are Sv2 string not
// representable as Str0255. I suggest to define Sv2 STR0255 as 1 byte len + a valid UTF-8 string.
pub type Str0255 = String;

///// TEST /////

#[test]
fn test_b0_64k() {
    use crate::ser::to_bytes;
    use std::convert::TryInto;

    let test: B064K = vec![1, 2, 9]
        .try_into()
        .expect("vector smaller than 64K should not fail");

    let expected = vec![3, 0, 1, 2, 9];
    assert_eq!(to_bytes(&test).unwrap(), expected);
}

#[test]
fn test_b0_64k_2() {
    use crate::ser::to_bytes;
    use std::convert::TryInto;

    let test: B064K = vec![10; 754]
        .try_into()
        .expect("vector smaller than 64K should not fail");

    let mut expected = vec![10; 756];
    expected[0] = 242;
    expected[1] = 2;
    assert_eq!(to_bytes(&test).unwrap(), expected);
}

#[test]
fn test_b0_64k_3() {
    use std::convert::TryInto;

    let test: Result<B064K, Error> = vec![10; 70000].try_into();

    match test {
        Ok(_) => assert!(false, "vector bigger than 64K should return an error"),
        Err(_) => assert!(true),
    }
}

#[test]
fn test_b0_16m() {
    use crate::ser::to_bytes;
    use std::convert::TryInto;

    let test: B016M = vec![1, 2, 9]
        .try_into()
        .expect("vector smaller than 64K should not fail");

    let expected = vec![3, 0, 0, 1, 2, 9];
    assert_eq!(to_bytes(&test).unwrap(), expected);
}
