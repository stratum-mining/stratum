use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_sv2::GetLen;
use serde_sv2::{Bytes as Sv2Bytes, Seq064K, Str0255, U24, U256};
use std::convert::TryInto;

#[derive(Debug, Serialize, Deserialize)]
pub struct Ping {
    message: Str0255,
    id: U24,
}

impl GetLen for Ping {
    fn get_len(&self) -> usize {
        self.message.get_len() + self.id.get_len()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pong<'a> {
    #[serde(borrow)]
    message: Seq064K<'a, U256<'a>>,
    id: U24,
}

impl GetLen for Pong<'_> {
    fn get_len(&self) -> usize {
        self.message.get_len() + self.id.get_len()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoiseHandShake {
    payload: Sv2Bytes,
}

impl GetLen for NoiseHandShake {
    fn get_len(&self) -> usize {
        self.payload.get_len()
    }
}

impl Ping {
    pub fn new(id: u32) -> Self {
        let message: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        Self {
            message,
            id: id.try_into().unwrap(),
        }
    }
}

impl<'a> Pong<'a> {
    pub fn new(id: u32, seq: Vec<U256<'a>>) -> Self {
        Self {
            message: Seq064K::new(seq).unwrap(),
            id: id.try_into().unwrap(),
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message<'a> {
    Ping(Ping),
    #[serde(borrow)]
    Pong(Pong<'a>),
}

impl GetLen for Message<'_> {
    fn get_len(&self) -> usize {
        match self {
            Self::Ping(ping) => ping.get_len(),
            Self::Pong(pong) => pong.get_len(),
        }
    }
}
