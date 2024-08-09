use super::{
    super::{Signature, U24, U256},
    Seq, SeqMaxLen, SeqVisitor, TryFromBSlice,
};
use crate::{
    primitives::{FixedSize, GetSize},
    Error,
};
use alloc::vec::Vec;
use serde::{ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone)]
pub struct Seq0255<'s, T: Serialize + TryFromBSlice<'s> + Clone> {
    seq: Option<Seq<'s, T>>,
    data: Option<Vec<T>>,
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s> + core::cmp::Eq> Eq for Seq0255<'s, T> {}
impl<'s, T: Clone + Serialize + TryFromBSlice<'s> + core::cmp::PartialEq> PartialEq
    for Seq0255<'s, T>
{
    fn eq(&self, other: &Self) -> bool {
        match (&self.seq, &self.data, &other.seq, &other.data) {
            (Some(seq1), _, Some(seq2), _) => seq1 == seq2,
            (_, Some(data1), _, Some(data2)) => data1 == data2,
            _ => crate::ser::to_bytes(&self) == crate::ser::to_bytes(&other),
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> Seq0255<'s, T> {
    #[inline]
    pub fn new(data: Vec<T>) -> Result<Self, Error> {
        if data.len() > 255 {
            Err(Error::LenBiggerThan255)
        } else {
            Ok(Seq0255 {
                seq: None,
                data: Some(data),
            })
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> From<Seq<'s, T>> for Seq0255<'s, T> {
    #[inline]
    fn from(val: Seq<'s, T>) -> Self {
        Self {
            seq: Some(val),
            data: None,
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> Serialize for Seq0255<'s, T> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match (&self.seq, &self.data) {
            (Some(seq), None) => {
                let len = seq.data.len() / seq.size as usize;
                let tuple = (len as u8, seq.data);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            (None, Some(data)) => {
                let tuple = (data.len() as u8, &data[..]);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            _ => panic!(),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, U256<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_U256",
                    SeqVisitor {
                        inner_type_size: 32,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, bool> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_Bool",
                    SeqVisitor {
                        inner_type_size: 1,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, u16> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_U16",
                    SeqVisitor {
                        inner_type_size: 2,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, U24> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_U24",
                    SeqVisitor {
                        inner_type_size: 3,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, u32> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_U32",
                    SeqVisitor {
                        inner_type_size: 4,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq0255<'a, Signature<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_0255_Signature",
                    SeqVisitor {
                        inner_type_size: 64,
                        max_len: SeqMaxLen::_1B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq0255::deserialize_json(deserializer),
        }
    }
}

impl<'a, T: Clone + FixedSize + Serialize + TryFromBSlice<'a>> GetSize for Seq0255<'a, T> {
    fn get_size(&self) -> usize {
        if self.data.is_some() {
            (self.data.as_ref().unwrap().len() * T::FIXED_SIZE) + 1
        } else {
            self.seq.as_ref().unwrap().data.len() + 1
        }
    }
}
impl<'s> Seq0255<'s, U256<'s>> {
    pub fn into_static(self) -> Seq0255<'static, U256<'static>> {
        if let Some(inner) = self.data {
            let inner = inner.clone();
            let data = inner.into_iter().map(|i| i.into_static()).collect();
            Seq0255 {
                seq: None,
                data: Some(data),
            }
        } else {
            panic!()
        }
    }
    pub fn inner_as_ref(&self) -> &[&[u8]] {
        todo!()
    }
}
impl<'s> Seq0255<'s, u32> {
    pub fn into_static(self) -> Seq0255<'static, u32> {
        if let Some(inner) = self.data {
            Seq0255 {
                seq: None,
                data: Some(inner),
            }
        } else {
            panic!()
        }
    }
}

impl<'a> From<Vec<u32>> for Seq0255<'a, u32> {
    fn from(v: Vec<u32>) -> Self {
        Seq0255 {
            seq: None,
            data: Some(v),
        }
    }
}

impl<'a> From<Vec<U256<'a>>> for Seq0255<'a, U256<'a>> {
    fn from(v: Vec<U256<'a>>) -> Self {
        Seq0255 {
            seq: None,
            data: Some(v),
        }
    }
}

impl<'a> From<Seq0255<'a, u32>> for Vec<u32> {
    fn from(v: Seq0255<u32>) -> Self {
        if let Some(inner) = v.data {
            inner
        } else {
            let data = v.seq.unwrap().data;
            if data.len() % 4 != 0 {
                panic!("{:?}", data);
            };
            let mut res = vec![];
            for v in data.chunks_exact(4) {
                res.push(u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
            }
            res
        }
    }
}

impl<'de, 's, T: Clone + Serialize + Deserialize<'de> + TryFromBSlice<'s>> Seq0255<'s, T> {
    pub fn into_inner(self) -> Vec<T> {
        if let Some(inner) = self.data {
            inner
        } else {
            todo!("Serde parser is supposed to be used only for message generator")
        }
    }

    fn deserialize_json<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<T> = Deserialize::deserialize(deserializer)?;
        Ok(Seq0255 {
            seq: None,
            data: Some(vec),
        })
    }
}

impl<'a, T: Serialize + Clone + TryFromBSlice<'a>> Seq0255<'a, T> {
    pub fn to_vec(&self) -> Vec<T> {
        todo!()
    }
}
