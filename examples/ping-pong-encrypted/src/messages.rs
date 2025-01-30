use crate::error::Error;
use binary_sv2::{
    binary_codec_sv2,
    decodable::{DecodableField, FieldMarker},
    Deserialize, Serialize,
};

use rand::Rng;

pub const PING_MSG_TYPE: u8 = 0xfe;
pub const PONG_MSG_TYPE: u8 = 0xff;

// we derive binary_sv2::{Serialize, Deserialize}
// to allow for binary encoding
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Ping {
    nonce: u8,
}

impl Ping {
    pub fn new() -> Result<Self, Error> {
        let mut rng = rand::thread_rng();
        let random: u8 = rng.gen();
        Ok(Self { nonce: random })
    }

    pub fn get_nonce(&self) -> u8 {
        self.nonce
    }
}

// we derive binary_sv2::{Serialize, Deserialize}
// to allow for binary encoding
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pong {
    nonce: u8,
}

impl<'decoder> Pong {
    pub fn new(nonce: u8) -> Result<Self, Error> {
        Ok(Self { nonce })
    }

    pub fn get_nonce(&self) -> u8 {
        self.nonce
    }
}

// unifies message types for noise_connection_tokio::Connection
#[derive(Clone)]
pub enum Message {
    Ping(Ping),
    Pong(Pong),
}

impl binary_sv2::GetSize for Message {
    fn get_size(&self) -> usize {
        match self {
            Self::Ping(ping) => ping.get_size(),
            Self::Pong(pong) => pong.get_size(),
        }
    }
}

impl From<Message> for binary_sv2::encodable::EncodableField<'_> {
    fn from(m: Message) -> Self {
        match m {
            Message::Ping(p) => p.into(),
            Message::Pong(p) => p.into(),
        }
    }
}

impl Deserialize<'_> for Message {
    fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
        unimplemented!()
    }
    fn from_decoded_fields(
        _v: Vec<DecodableField>,
    ) -> std::result::Result<Self, binary_sv2::Error> {
        unimplemented!()
    }
}
