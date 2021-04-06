use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_sv2::GetLen;
use serde_sv2::{Seq0255, Str0255, U24, U256};
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
    message: Seq0255<'a, U256<'a>>,
    id: U24,
}

impl GetLen for Pong<'_> {
    fn get_len(&self) -> usize {
        self.message.get_len() + self.id.get_len()
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
            message: Seq0255::new(seq).unwrap(),
            id: id.try_into().unwrap(),
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id.into()
    }
}

// TODO impl serialize and deserialize for enum in serde sv2
#[derive(Debug, Serialize)]
pub enum Message<'a> {
    Ping(Ping),
    Pong(Pong<'a>),
}

//impl Message<'_> {
//    #[inline]
//    pub fn to_bytes(self, buffer: &mut BytesMut) {
//        match self {
//            Self::Ping(ping) => {
//                let encodable = Frame::from(ping);
//                encodable.to_bytes(buffer);
//            }
//            Self::Pong(pong) => {
//                let encodable = Frame::from(pong);
//                encodable.to_bytes(buffer);
//            }
//        }
//    }
//}
impl GetLen for Message<'_> {
    fn get_len(&self) -> usize {
        match self {
            Self::Ping(ping) => ping.get_len(),
            Self::Pong(pong) => pong.get_len(),
        }
    }
}
