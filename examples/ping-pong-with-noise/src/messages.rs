#[cfg(not(feature = "with_serde"))]
use binary_sv2::{binary_codec_sv2, decodable::DecodableField, decodable::FieldMarker};
use binary_sv2::{Deserialize, GetSize, Seq064K, Serialize, Str0255, U24, U256};
use rand::{distributions::Alphanumeric, Rng};
use std::convert::TryInto;

#[derive(Debug, Serialize, Deserialize)]
pub struct Ping<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    message: Str0255<'decoder>,
    id: U24,
}

#[cfg(feature = "with_serde")]
impl<'decoder> GetSize for Ping<'decoder> {
    fn get_size(&self) -> usize {
        self.message.get_size() + self.id.get_size()
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BiggerPing<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    message: Str0255<'decoder>,
    id: U24,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    additional_data: Str0255<'decoder>,
}

#[cfg(feature = "with_serde")]
impl<'decoder> GetSize for BiggerPing<'decoder> {
    fn get_size(&self) -> usize {
        self.message.get_size() + self.id.get_size() + self.additional_data.get_size()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pong<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    message: Seq064K<'decoder, U256<'decoder>>,
    id: U24,
}

#[cfg(feature = "with_serde")]
impl GetSize for Pong<'_> {
    fn get_size(&self) -> usize {
        self.message.get_size() + self.id.get_size()
    }
}

#[derive(Debug, Serialize)]
pub struct NoiseHandShake {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    payload: Vec<u8>,
}

#[cfg(feature = "with_serde")]
impl<'decoder> GetSize for NoiseHandShake<'decoder> {
    fn get_size(&self) -> usize {
        self.payload.get_size()
    }
}

impl<'decoder> Ping<'decoder> {
    pub fn new(id: u32) -> Self {
        let message: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        Self {
            message: message.into_bytes().try_into().unwrap(),
            id: id.try_into().unwrap(),
        }
    }
}

impl<'decoder> BiggerPing<'decoder> {
    pub fn new(id: u32) -> Self {
        let message: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        Self {
            message: message.clone().into_bytes().try_into().unwrap(),
            id: id.try_into().unwrap(),
            additional_data: message.into_bytes().try_into().unwrap(),
        }
    }
}

impl<'decoder> Pong<'decoder> {
    pub fn new(id: u32, seq: Vec<U256<'decoder>>) -> Self {
        Self {
            message: Seq064K::new(seq).unwrap(),
            id: id.try_into().unwrap(),
        }
    }

    pub fn get_id(&self) -> u32 {
        //self.id.0
        self.id.into()
    }
}

//#[derive(Debug, Serialize, Deserialize)]
pub enum Message<'decoder> {
    Ping(Ping<'decoder>),
    Pong(Pong<'decoder>),
    BiggerPing(BiggerPing<'decoder>),
}

#[cfg(feature = "with_serde")]
impl<'decoder> binary_sv2::Serialize for Message<'decoder> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: binary_sv2::serde::Serializer,
    {
        match self {
            Message::Ping(p) => p.serialize(serializer),
            Message::Pong(p) => p.serialize(serializer),
            Message::BiggerPing(p) => p.serialize(serializer),
        }
    }
}

#[cfg(feature = "with_serde")]
impl<'decoder> binary_sv2::Deserialize<'decoder> for Message<'decoder> {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: binary_sv2::serde::Deserializer<'decoder>,
    {
        todo!()
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> From<Message<'decoder>> for binary_sv2::encodable::EncodableField<'decoder> {
    fn from(m: Message<'decoder>) -> Self {
        match m {
            Message::Ping(p) => p.into(),
            Message::Pong(p) => p.into(),
            Message::BiggerPing(p) => p.into(),
        }
    }
}

#[cfg(not(feature = "with_serde"))]
impl<'decoder> Deserialize<'decoder> for Message<'decoder> {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField<'decoder>>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}

impl GetSize for Message<'_> {
    fn get_size(&self) -> usize {
        match self {
            Self::Ping(ping) => ping.get_size(),
            Self::Pong(pong) => pong.get_size(),
            Self::BiggerPing(bp) => bp.get_size(),
        }
    }
}
