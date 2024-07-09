use super::{
    super::{Signature, B016M, B064K, U24, U256},
    Seq, SeqMaxLen, SeqVisitor, TryFromBSlice,
};
use crate::{
    primitives::{FixedSize, GetSize},
    Error, ShortTxId,
};
use alloc::vec::Vec;
use serde::{ser, ser::SerializeTuple, Deserialize, Deserializer, Serialize};

/// See `binary_sv2::Seq064k`
#[derive(Debug, Clone)]
pub struct Seq064K<'s, T: Clone + Serialize + TryFromBSlice<'s>> {
    seq: Option<Seq<'s, T>>,
    data: Option<Vec<T>>,
}

impl<'s, T: Clone + FixedSize + Serialize + TryFromBSlice<'s> + core::cmp::PartialEq> PartialEq
    for Seq064K<'s, T>
{
    fn eq(&self, other: &Self) -> bool {
        match (&self.seq, &self.data, &other.seq, &other.data) {
            (Some(seq1), _, Some(seq2), _) => seq1 == seq2,
            (_, Some(data1), _, Some(data2)) => data1 == data2,
            _ => crate::ser::to_bytes(&self) == crate::ser::to_bytes(&other),
        }
    }
}
impl<'s> Eq for Seq064K<'s, B016M<'s>> {}
impl<'s> Eq for Seq064K<'s, B064K<'s>> {}
impl<'s> Eq for Seq064K<'s, ShortTxId<'s>> {}
impl<'s> Eq for Seq064K<'s, U256<'s>> {}
impl<'s> Eq for Seq064K<'s, u16> {}

impl<'s> PartialEq for Seq064K<'s, B016M<'s>> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.seq, &self.data, &other.seq, &other.data) {
            (Some(seq1), _, Some(seq2), _) => seq1 == seq2,
            (_, Some(data1), _, Some(data2)) => data1 == data2,
            _ => crate::ser::to_bytes(&self) == crate::ser::to_bytes(&other),
        }
    }
}
impl<'s> PartialEq for Seq064K<'s, B064K<'s>> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.seq, &self.data, &other.seq, &other.data) {
            (Some(seq1), _, Some(seq2), _) => seq1 == seq2,
            (_, Some(data1), _, Some(data2)) => data1 == data2,
            _ => crate::ser::to_bytes(&self) == crate::ser::to_bytes(&other),
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> Seq064K<'s, T> {
    #[inline]
    pub fn new(data: Vec<T>) -> Result<Self, Error> {
        if data.len() > 65536 {
            Err(Error::LenBiggerThan64K)
        } else {
            Ok(Seq064K {
                seq: None,
                data: Some(data),
            })
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> From<Seq<'s, T>> for Seq064K<'s, T> {
    #[inline]
    fn from(val: Seq<'s, T>) -> Self {
        Self {
            seq: Some(val),
            data: None,
        }
    }
}

impl<'s, T: Clone + Serialize + TryFromBSlice<'s>> From<Vec<T>> for Seq064K<'s, T> {
    #[inline]
    fn from(val: Vec<T>) -> Self {
        Self {
            seq: None,
            data: Some(val),
        }
    }
}

impl<'s, T: Clone + FixedSize + Serialize + TryFromBSlice<'s>> Serialize for Seq064K<'s, T> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match (&self.seq, &self.data) {
            (Some(seq), None) => {
                let len = seq.data.len() / seq.size as usize;
                let tuple = (len as u16, seq.data);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            (None, Some(data)) => {
                let tuple = (data.len() as u16, &data[..]);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            _ => panic!(),
        }
    }
}

impl<'s> Serialize for Seq064K<'s, B064K<'s>> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match (&self.seq, &self.data) {
            (Some(seq), None) => {
                // TODO if len > than u16::MAX should return an error
                let len = B016M::get_elements_number_in_array(seq.data);
                let tuple = (len as u16, seq.data);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            (None, Some(data)) => {
                // TODO if data.len > than u16::MAX should return an error
                let tuple = (data.len() as u16, &data[..]);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            _ => panic!(),
        }
    }
}

impl<'s> Serialize for Seq064K<'s, B016M<'s>> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match (&self.seq, &self.data) {
            (Some(seq), None) => {
                // TODO if len > than u16::MAX should return an error
                let len = B016M::get_elements_number_in_array(seq.data);
                let tuple = (len as u16, seq.data);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            (None, Some(data)) => {
                // TODO if data.len > than u16::MAX should return an error
                let tuple = (data.len() as u16, &data[..]);
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&tuple.0)?;
                seq.serialize_element(tuple.1)?;
                seq.end()
            }
            _ => panic!(),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, U256<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_U256",
                    SeqVisitor {
                        inner_type_size: 32,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}
impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, ShortTxId<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_ShortTxId",
                    SeqVisitor {
                        inner_type_size: 6,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, bool> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_Bool",
                    SeqVisitor {
                        inner_type_size: 1,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, u16> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_U16",
                    SeqVisitor {
                        inner_type_size: 2,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, U24> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_U24",
                    SeqVisitor {
                        inner_type_size: 3,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, u32> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_U32",
                    SeqVisitor {
                        inner_type_size: 4,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, u64> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_U64",
                    SeqVisitor {
                        inner_type_size: 8,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, Signature<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_Signature",
                    SeqVisitor {
                        inner_type_size: 64,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, B064K<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_B064K",
                    SeqVisitor {
                        inner_type_size: 2,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Seq064K<'a, B016M<'a>> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.is_human_readable() {
            false => deserializer
                .deserialize_newtype_struct(
                    "Seq_064K_B016M",
                    SeqVisitor {
                        inner_type_size: 3,
                        max_len: SeqMaxLen::_2B,
                        _a: core::marker::PhantomData,
                    },
                )
                .map(|x| x.into()),
            true => Seq064K::deserialize_json(deserializer),
        }
    }
}

impl<'a, T: Clone + FixedSize + Serialize + TryFromBSlice<'a>> GetSize for Seq064K<'a, T> {
    fn get_size(&self) -> usize {
        if self.data.is_some() {
            (self.data.as_ref().unwrap().len() * T::FIXED_SIZE) + 2
        } else {
            self.seq.as_ref().unwrap().data.len() + 2
        }
    }
}

impl<'a> GetSize for Seq064K<'a, B016M<'a>> {
    fn get_size(&self) -> usize {
        if self.data.is_some() {
            (self
                .data
                .as_ref()
                .unwrap()
                .iter()
                .fold(0, |acc, x| acc + x.get_size()))
                + 2
        } else {
            self.seq.as_ref().unwrap().data.len() + 2
        }
    }
}
impl<'a> GetSize for Seq064K<'a, B064K<'a>> {
    fn get_size(&self) -> usize {
        if self.data.is_some() {
            (self
                .data
                .as_ref()
                .unwrap()
                .iter()
                .fold(0, |acc, x| acc + x.get_size()))
                + 2
        } else {
            self.seq.as_ref().unwrap().data.len() + 2
        }
    }
}
impl<'s> Seq064K<'s, B064K<'s>> {
    pub fn into_static(self) -> Seq064K<'static, B064K<'static>> {
        if let Some(inner) = self.data {
            let inner = inner.clone();
            let data: Vec<B064K<'static>> = inner.into_iter().map(|i| i.into_static()).collect();
            Seq064K {
                seq: None,
                data: Some(data),
            }
        } else {
            panic!()
        }
    }
}
impl<'s> Seq064K<'s, B016M<'s>> {
    pub fn into_static(self) -> Seq064K<'static, B016M<'static>> {
        if let Some(inner) = self.data {
            let inner = inner.clone();
            let data: Vec<B016M<'static>> = inner.into_iter().map(|i| i.into_static()).collect();
            Seq064K {
                seq: None,
                data: Some(data),
            }
        } else {
            panic!()
        }
    }
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.data
            .clone()
            .unwrap()
            .iter()
            .map(|x| x.clone().to_vec())
            .collect()
    }
}
impl<'s> Seq064K<'s, u32> {
    pub fn into_static(self) -> Seq064K<'static, u32> {
        if let Some(inner) = self.data {
            Seq064K {
                seq: None,
                data: Some(inner),
            }
        } else {
            panic!()
        }
    }
}
impl<'s> Seq064K<'s, u16> {
    pub fn into_static(self) -> Seq064K<'static, u16> {
        if let Some(inner) = self.data {
            Seq064K {
                seq: None,
                data: Some(inner),
            }
        } else {
            panic!()
        }
    }
}
impl<'s> Seq064K<'s, ShortTxId<'s>> {
    pub fn into_static(self) -> Seq064K<'static, ShortTxId<'static>> {
        if let Some(inner) = self.data {
            let inner = inner.clone();
            let data: Vec<ShortTxId<'static>> =
                inner.into_iter().map(|i| i.into_static()).collect();
            Seq064K {
                seq: None,
                data: Some(data),
            }
        } else {
            panic!()
        }
    }
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.data
            .clone()
            .unwrap()
            .iter()
            .map(|x| x.to_vec())
            .collect()
    }
}
impl<'s> Seq064K<'s, U256<'s>> {
    pub fn into_static(self) -> Seq064K<'static, U256<'static>> {
        if let Some(inner) = self.data {
            let inner = inner.clone();
            let data: Vec<U256<'static>> = inner.into_iter().map(|i| i.into_static()).collect();
            Seq064K {
                seq: None,
                data: Some(data),
            }
        } else {
            panic!()
        }
    }
}

impl<'de, 's, T: Clone + Serialize + Deserialize<'de> + TryFromBSlice<'s>> Seq064K<'s, T> {
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
        Ok(Seq064K {
            seq: None,
            data: Some(vec),
        })
    }
}
