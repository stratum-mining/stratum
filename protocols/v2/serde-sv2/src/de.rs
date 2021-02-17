//! Serde deserializer for [stratum v2][Sv2] implemented following [serde tutorial][tutorial]
//!
//! Possible implementation of an Sv2 deserializer from and io stream are:
//!
//! 1) io stream -> read -> 'de buffer -> borrow -> 'de deserialized
//! 2) io stream -> read -> 'de buffer -> take ownership -> 'a deserialized
//! 3) io stream -> read -> take ownership -> 'a deserialized
//!
//! The chosen implementation is (2) becasue:
//!
//! * Implementing a pareser for a byte slice is easyer than implementing a parser for a reader and
//! number of copyes are the same.
//! * Let the deserialized struct live longer than the initial buffer give more freedom for the same
//! number of copyes.
//!
//! Deserializer impl from_reader where [Sv2 frames][Sv2] are read and copied in Deserializer.
//! Each Deserializer contains a single Sv2 frame. Parsing a deserializer it never copy so we can
//! define deserializer in Sv2 messaged rapresented by struct of borrowed data that had the same
//! lifetime of Deserializer without copying any data.
//!
//! [Sv2]: https://docs.google.com/document/d/1FadCWj-57dvhxsnFM_7X806qyvhR0u3i85607bGHxvg/edit
//! [tutorial]: https://serde.rs/data-format.html
//!
use std::convert::TryInto;

use serde::de::{
    self, DeserializeSeed, SeqAccess,
    Visitor,
};
use serde::Deserialize;

use crate::error::{Error, Result};

enum Sv2Seq {
    S64k,
    S255,
}

//enum Sv2T {
//    Bool,
//    U8,
//    U16,
//    U24,
//    U32,
//    U256,
//    String_,
//    Signature,
//    B0255,
//    B064K,
//    B016M,
//    B,
//    Pubkey,
//}

/// TODO implementare Deserializer from_reader
pub struct Deserializer<'de> {
    input: &'de mut [u8],
    cursor_: usize,
    cursor: *mut usize,
}
impl<'de> Deserializer<'de> {
    fn _new(input: &'de mut [u8]) -> Self {
        // just initialize the *cursor to an arbitrary address the correct value will be set after
        let cursor = 100 as *mut usize;
        Deserializer {
            input,
            cursor,
            cursor_: 0,
        }
    }
    pub fn from_bytes(input: &'de mut [u8]) -> Self {
        let mut self_ = Self::_new(input);
        self_.cursor = &mut self_.cursor_ as *mut usize;
        self_
    }
}

pub fn from_bytes<'a, T>(b: &'a mut [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_bytes(b);
    let t = T::deserialize(&mut deserializer)?;
    Ok(t)
}

impl<'de> Deserializer<'de> {
    fn get_slice(&'de self, len: usize) -> Result<&'de [u8]> {
        let cursor: usize;
        unsafe {
            cursor = *self.cursor;
            *self.cursor += len;
        };

        let l = len - 1;

        if self.input.len() < cursor + l {
            return Err(Error::ReadError);
        }
        let res = &self.input[cursor..=cursor + l];
        Ok(res)
    }

    fn parse_bool(&'de self) -> Result<bool> {
        let bool_ = self.get_slice(1)?;
        match bool_ {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(Error::InvalidBool),
        }
    }

    fn parse_u8(&'de self) -> Result<u8> {
        let u8_ = self.get_slice(1)?;
        Ok(u8_[0])
    }

    fn parse_u16(&'de self) -> Result<u16> {
        let u16_ = self.get_slice(2)?;
        Ok(u16::from_le_bytes([u16_[0], u16_[1]]))
    }

    fn parse_u24(&'de self) -> Result<u32> {
        let u24 = self.get_slice(3)?;
        Ok(u32::from_le_bytes([u24[0], u24[1], u24[2], 0]))
    }

    fn parse_u32(&'de self) -> Result<u32> {
        let u32_ = self.get_slice(4)?;
        Ok(u32::from_le_bytes([u32_[0], u32_[1], u32_[2], u32_[3]]))
    }

    fn parse_u256(&'de self) -> Result<&'de [u8; 32]> {
        let u256: &[u8; 32] = self.get_slice(32)?.try_into().unwrap();
        Ok(u256)
    }

    fn parse_string(&'de self) -> Result<&'de str> {
        let len = self.parse_u8()?;
        let str_ = self.get_slice(len as usize)?;
        Ok(std::str::from_utf8(&str_).map_err(|_| Error::InvalidUTF8)?)
    }

    fn parse_b016m(&'de self) -> Result<&'de [u8]> {
        let len = self.parse_u24()?;
        Ok(self.get_slice(len as usize)?)
    }

}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.parse_u8()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.parse_u16()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.parse_u32()?)
    }

    // Refer to the "Understanding deserializer lifetimes" page for information
    // about the three deserialization flavors of strings in Serde.
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_str(self.parse_string()?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    // The `Serializer` implementation on the previous page serialized byte
    // arrays as JSON arrays of bytes. Handle that representation here.
    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_struct<V>(
        mut self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Ok(visitor.visit_seq(Struct::new(&mut self, fields.len())?)?)
    }

    fn deserialize_newtype_struct<V>(mut self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match _name {
            "U24" => visitor.visit_u32(self.parse_u24()?),
            "U256" => visitor.visit_bytes(self.parse_u256()?),
            "B016M" => visitor.visit_bytes(self.parse_b016m()?),
            "Seq0255" => Ok(visitor.visit_seq(Seq::new(&mut self, Sv2Seq::S255)?)?),
            "Seq064K" => Ok(visitor.visit_seq(Seq::new(&mut self, Sv2Seq::S64k)?)?),
            _ => visitor.visit_newtype_struct(self),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.parse_bool()?)
    }

    ///// UNIMPLEMENTED /////

    fn deserialize_option<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }


    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // Float parsing is stupidly hard.
    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // The `Serializer` implementation on the previous page serialized chars as
    // single-character strings so handle that representation here.
    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Parse a string, check that it is one character, call `visit_char`.
        unimplemented!()
    }

    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }
}

struct Seq<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    len: usize,
}

impl<'a, 'de> Seq<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, type_: Sv2Seq) -> std::result::Result<Self, Error> {
        let len = match type_ {
            Sv2Seq::S255 => de.parse_u8()? as usize,
            Sv2Seq::S64k => de.parse_u16()? as usize,
        };
        Ok(Self { de, len })
    }
}

impl<'de, 'a> SeqAccess<'de> for Seq<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        // Check if there are no more elements.
        if self.len == 0 {
            return Ok(None);
        }

        self.len -= 1;

        // Deserialize an array element.
        seed.deserialize(&mut *self.de).map(Some)
    }
}

struct Struct<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    len: usize,
}

impl<'a, 'de> Struct<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, len: usize) -> std::result::Result<Self, Error> {
        Ok(Self { de, len })
    }
}

impl<'de, 'a> SeqAccess<'de> for Struct<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        // Check if there are no more elements.
        if self.len == 0 {
            return Ok(None);
        }

        self.len -= 1;

        // Deserialize an array element.
        seed.deserialize(&mut *self.de).map(Some)
    }
}

///// TEST /////

#[test]
fn test_struct() {
    use serde::Serialize;

    #[derive(Deserialize, Serialize, PartialEq, Debug)]
    struct Test {
        a: u32,
        b: u8,
        c: crate::sv2_primitives::U24,
    }

    let expected = Test {
        a: 456,
        b: 9,
        c: 67.into(),
    };

    let mut bytes = crate::ser::to_bytes(&expected).unwrap();
    let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

    assert_eq!(deserialized, expected);
}
